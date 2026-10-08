//! # Module contract
//!
//! ```json
//! {"purpose":"Take what the main line sends a shared daemon, which holds no code of its own to read: the findings to prune its baseline to, and the recipes its site shows.","criteria":[{"id":"prune-from-main-line","statement":"prune_to on a shared daemon, given the digest of the intent the findings were judged against, refuses as a conflict when its intent no longer gives that digest, comparing under the lock every write takes; otherwise it drops from the baseline every entry that neither the given findings nor its own lint, which sees what intent alone shows, a store file that did not load among it, produces, adding none, persists the smaller baseline and announces it, and answers how many entries went and how many are left; a daemon that holds code refuses it as a conflict, since it prunes its own baseline."},{"id":"main-line-recipes","statement":"take_main_line_recipes on a shared daemon validates the recipes the main line sent and replaces the store's record of them whole, persists it and announces it, unless it holds them already, and answers how many it holds; an invalid record is refused as invalid and changes nothing, and a daemon that holds code refuses it as a conflict, since it reads its own; site and site_recipe on a shared daemon read the recipes from that record, none until there is one, and on a daemon that holds code from its code, whatever record its store holds."}]}
//! ```

use grimoire_core::{lint, State, ToolError};
use serde_json::Value;

use super::{describe, Daemon, Event};
use crate::store_io;

impl Daemon {
    /// Prune the baseline to the findings the main line saw, for a shared
    /// daemon, which holds no code to see them itself. Answers how many
    /// entries went and how many are left.
    pub fn prune_to(&self, present: Vec<grimoire_core::model::FindingKey>, intent: &str) -> Result<Value, ToolError> {
        if !self.shared {
            return Err(ToolError::conflict("only a shared daemon takes a prune from the main line: this one holds code and prunes its own baseline"));
        }
        let mut present: std::collections::HashSet<_> = present.into_iter().collect();
        let mut state = self.state.lock().expect("state lock");
        // What this daemon sees itself is present whatever the main line sent: a store file that does not
        // load is in no export, so the main line cannot see it, and it is not thereby fixed.
        present.extend(lint::lint(state.graph(), &lint::Rule::ALL).iter().map(lint::Finding::key));
        // Judged against intent that has since moved, a write or an adopt among them: refused, under the
        // same lock every write takes, so nothing is bottled between this comparison and the prune.
        if crate::remote::intent_digest(&grimoire_core::export::exported(state.store())) != intent {
            return Err(ToolError::conflict("the intent moved since the main line read it: read it again and send the prune again"));
        }
        let went = match state.prune_baseline_to(&present) {
            None => 0,
            Some((changed, went)) => {
                if let Err(e) = store_io::persist(&self.store_dir, state.store(), &changed) {
                    if let Ok((store, malformed)) = store_io::load(&self.store_dir) {
                        state.set_store(store, malformed);
                    }
                    return Err(ToolError::unavailable(format!("the pruned baseline could not be written: {e}")));
                }
                let _ = self.events.send(Event::Store { tool: "prune".to_string(), changed: describe(&changed) });
                went
            }
        };
        let left = state.store().baseline.as_ref().map_or(0, |b| b.findings.len());
        Ok(serde_json::json!({ "pruned": went, "left": left }))
    }

    /// Keep the recipes the main line sent, for a shared daemon's site, which holds no code to read them
    /// (req:recipe-main-line-snapshot): replaced whole, persisted and announced. Answers how many it holds.
    pub fn take_main_line_recipes(&self, recipes: grimoire_core::model::MainLineRecipes) -> Result<Value, ToolError> {
        if !self.shared {
            return Err(ToolError::conflict("only a shared daemon takes the main line's recipes: this one holds code and reads its own"));
        }
        let errors = grimoire_core::store::validate_main_line_recipes(&recipes);
        if !errors.is_empty() {
            return Err(ToolError::invalid("the main line's recipes do not validate", errors));
        }
        let held = recipes.recipes.len();
        let mut state = self.state.lock().expect("state lock");
        if let Some(changed) = state.set_main_line_recipes(recipes) {
            if let Err(e) = store_io::persist(&self.store_dir, state.store(), &changed) {
                if let Ok((store, malformed)) = store_io::load(&self.store_dir) {
                    state.set_store(store, malformed);
                }
                return Err(ToolError::unavailable(format!("the main line's recipes could not be written: {e}")));
            }
            let _ = self.events.send(Event::Store { tool: "recipes".to_string(), changed: describe(&changed) });
        }
        Ok(serde_json::json!({ "recipes": held }))
    }

    /// The project's site as the truth now has it: the pages its setting names and what is beneath them, the one `show` names in full, or the first.
    pub fn site(&self, show: Option<&str>) -> grimoire_core::query::Site {
        let mut state = self.state.lock().expect("state lock");
        let (config, sent) = self.site_reads(&state);
        grimoire_core::query::site(state.graph(), &config, show, sent.as_ref())
    }

    /// The site, its list of pages and its catalog, with the recipe `id` names in full, read under one lock.
    pub fn site_recipe(&self, id: &str) -> (grimoire_core::query::Site, Option<grimoire_core::query::SiteRecipe>) {
        let mut state = self.state.lock().expect("state lock");
        let (config, sent) = self.site_reads(&state);
        // An empty slug names no page, so only the list, the catalog and the name are gathered.
        (grimoire_core::query::site(state.graph(), &config, Some(""), sent.as_ref()), grimoire_core::query::recipe(state.graph(), id, sent.as_ref()))
    }

    /// What the site reads beside the graph: the config, and on a shared daemon, which holds no code, the
    /// recipes the main line last sent, an empty record until it has sent any; one holding code reads its own.
    fn site_reads(&self, state: &State) -> (grimoire_core::model::StoreConfig, Option<grimoire_core::model::MainLineRecipes>) {
        let store = state.store();
        let sent = self.shared.then(|| store.main_line_recipes.clone().unwrap_or_default());
        (store.config.clone(), sent)
    }
}

#[cfg(test)]
mod tests {
    use super::super::tests::Harness;
    use super::*;
    use serde_json::json;

    /// # Test contract
    ///
    /// ```json
    /// {"test": "a-shared-daemon-prunes-to-what-the-main-line-saw", "checks": ["crates/server/src/daemon/main_line#prune-from-main-line", "crates/server/src/remote#intent-digest", "crates/core/src/tools/state#prune-to"]}
    /// ```
    #[test]
    fn a_shared_daemon_prunes_to_what_the_main_line_saw() {
        let h = Harness::new();
        let local = Daemon::open(&h.store).unwrap().0;
        local.call("adopt", json!({ "reason": "inherited" })).unwrap();
        // A daemon that holds code prunes its own baseline, and refuses to be told.
        assert_eq!(local.prune_to(Vec::new(), "").unwrap_err().code(), "conflict");
        drop(local);
        let adopted = store_io::load(&h.store).unwrap().0.baseline.unwrap();
        assert!(adopted.findings.len() >= 2, "{adopted:?}");
        let shared = Daemon::open_shared(&h.store).unwrap();
        let mut events = shared.subscribe();
        // The main line still sees all but the first, and one the baseline never held.
        let mut seen: Vec<_> = adopted.findings[1..].to_vec();
        seen.push(grimoire_core::model::FindingKey {
            rule: grimoire_core::lint::Rule::Unspecified,
            check: "no-scenario".into(),
            r#ref: grimoire_core::model::Reference::req("never-held"),
            edge: None,
        });
        // Judged against intent that has since moved: refused, and nothing goes.
        let read = crate::remote::intent_digest(&shared.exported());
        shared.call("inscribe_ticket", json!({ "title": "Meanwhile", "kind": "gap", "priority": "later", "body": "b" })).unwrap();
        let _ = events.try_recv();
        assert_eq!(shared.prune_to(seen.clone(), &read).unwrap_err().code(), "conflict");
        assert_eq!(store_io::load(&h.store).unwrap().0.baseline.unwrap(), adopted);
        // The main line's recipes are not intent: sending them moves no digest, so a prune read before them is taken.
        let now = crate::remote::intent_digest(&shared.exported());
        let recipe = grimoire_core::model::SentRecipe {
            id: "button".into(),
            kind: "component".into(),
            title: "Button".into(),
            text: "Use it.".into(),
            ingredients: vec![],
            uses: vec![],
            digest: "0123456789abcdef".into(),
        };
        shared.take_main_line_recipes(grimoire_core::model::MainLineRecipes { recipes: vec![recipe] }).unwrap();
        let _ = events.try_recv();
        assert_eq!(crate::remote::intent_digest(&shared.exported()), now);
        let mut older = serde_json::to_value(shared.exported()).unwrap();
        older.as_object_mut().unwrap().remove("main_line_recipes");
        assert_eq!(crate::remote::intent_digest(&serde_json::from_value(older).unwrap()), now, "as a machine older than the record reads it");
        // Judged against the intent as it is: taken.
        let answer = shared.prune_to(seen.clone(), &now).unwrap();
        assert_eq!(answer, json!({ "pruned": 1, "left": adopted.findings.len() - 1 }));
        let stored = store_io::load(&h.store).unwrap().0.baseline.unwrap();
        assert_eq!(stored.findings, adopted.findings[1..].to_vec(), "persisted, and nothing added");
        assert!(matches!(events.try_recv(), Ok(Event::Store { tool, changed }) if tool == "prune" && changed == vec!["baseline".to_string()]));
        // Told the same again, nothing goes and nothing is announced; the prune itself moved the intent.
        let after = crate::remote::intent_digest(&shared.exported());
        assert_ne!(after, now);
        assert_eq!(shared.prune_to(seen, &after).unwrap()["pruned"], 0);
        assert!(events.try_recv().is_err());
        drop(shared);
        // A store file that does not load is in no export: the main line cannot see it, and its finding is kept.
        std::fs::write(h.store.join("pages").join("broken.json"), "{ not a page").unwrap();
        let shared = Daemon::open_shared(&h.store).unwrap();
        let broken = lint::lint(shared.state.lock().unwrap().graph(), &lint::Rule::ALL).into_iter().find(|f| f.check == "malformed-file").expect("the broken file is a finding").key();
        let mut store = store_io::load(&h.store).unwrap().0;
        let mut baseline = store.baseline.clone().unwrap();
        baseline.findings.push(broken.clone());
        store.baseline = Some(baseline);
        store_io::persist(&h.store, &store, &grimoire_core::ops::Changed::Baseline).unwrap();
        let shared = {
            drop(shared);
            Daemon::open_shared(&h.store).unwrap()
        };
        let intent = crate::remote::intent_digest(&shared.exported());
        shared.prune_to(Vec::new(), &intent).unwrap();
        let own: std::collections::HashSet<_> = lint::lint(shared.state.lock().unwrap().graph(), &lint::Rule::ALL).iter().map(lint::Finding::key).collect();
        let left = store_io::load(&h.store).unwrap().0.baseline.unwrap().findings;
        assert!(left.contains(&broken) && left.iter().all(|k| own.contains(k)), "told of nothing, all goes but what it sees itself: {left:?}");
    }

    /// # Test contract
    ///
    /// ```json
    /// {"test": "a-shared-daemon-keeps-the-main-lines-recipes", "checks": ["crates/server/src/daemon/main_line#main-line-recipes", "req:recipe-main-line-snapshot"]}
    /// ```
    #[test]
    fn a_shared_daemon_keeps_the_main_lines_recipes() {
        use grimoire_core::model::{MainLineRecipes, SentRecipe};
        let h = Harness::new();
        std::fs::write(h.repo.join("icon.recipe.md"), "# Icon\n\n## Recipe\n\n```json\n{\"id\":\"icon\",\"kind\":\"component\",\"files\":[\"src/a.rs\"]}\n```\n").unwrap();
        let sent = |ids: &[&str]| MainLineRecipes {
            recipes: ids
                .iter()
                .map(|id| SentRecipe {
                    id: id.to_string(),
                    kind: "component".into(),
                    title: id.to_uppercase(),
                    text: "Use it.".into(),
                    ingredients: vec![],
                    uses: vec![],
                    digest: "0123456789abcdef".into(),
                })
                .collect(),
        };
        // A daemon that holds code reads its own, and refuses to be told.
        let local = Daemon::open(&h.store).unwrap().0;
        assert_eq!(local.take_main_line_recipes(sent(&["button"])).unwrap_err().code(), "conflict");
        drop(local);
        let shared = Daemon::open_shared(&h.store).unwrap();
        let mut events = shared.subscribe();
        assert!(shared.site(None).recipes.is_empty(), "nothing sent yet, so no catalog");
        // Taken whole, persisted and announced; the site reads it, due against the judgements held.
        assert_eq!(shared.take_main_line_recipes(sent(&["button", "quest"])).unwrap(), json!({ "recipes": 2 }));
        assert!(matches!(events.try_recv(), Ok(Event::Store { tool, changed }) if tool == "recipes" && changed == vec!["recipes".to_string()]));
        assert_eq!(store_io::load(&h.store).unwrap().0.main_line_recipes, Some(sent(&["button", "quest"])));
        let listed: Vec<(String, bool)> = shared.site(None).recipes.into_iter().map(|r| (r.id, r.due)).collect();
        assert_eq!(listed, [("button".to_string(), true), ("quest".to_string(), true)]);
        assert_eq!(shared.site_recipe("quest").1.map(|r| r.title), Some("QUEST".to_string()));
        // Replaced whole; the same again changes and announces nothing.
        shared.take_main_line_recipes(sent(&["button"])).unwrap();
        let _ = events.try_recv();
        assert_eq!(shared.site_recipe("quest").1, None, "gone from the main line, gone from the site");
        shared.take_main_line_recipes(sent(&["button"])).unwrap();
        assert!(events.try_recv().is_err());
        // One that does not validate is refused, and nothing changes.
        let mut bad = sent(&["icon"]);
        bad.recipes[0].digest = "nope".into();
        assert_eq!(shared.take_main_line_recipes(bad).unwrap_err().code(), "invalid");
        assert_eq!(store_io::load(&h.store).unwrap().0.main_line_recipes, Some(sent(&["button"])));
        drop(shared);
        // A daemon that holds code reads its code, whatever record its store holds.
        let local = Daemon::open(&h.store).unwrap().0;
        assert_eq!(local.site(None).recipes.into_iter().map(|r| r.id).collect::<Vec<_>>(), ["icon"]);
        assert!(local.site_recipe("button").1.is_none());
    }
}
