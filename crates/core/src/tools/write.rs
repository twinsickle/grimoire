//! # Module contract
//!
//! ```json
//! {"purpose":"Answer the write tools: each applies one operation to the store and answers with the node as it now exists, the findings the write created and what changed for the caller to persist.","criteria":[{"id":"write-responses","statement":"Every write answers with the node as it now exists; put_page, patch_page, put_requirement, file_ticket and update_ticket add as warnings the new findings the write created on that node (its dangling edges, and unspecified for a requirement with no scenario) and a count of the baselined ones, start_ticket adds rests_on and related, close_ticket adds the new findings on the nodes it touched, a count of the baselined ones and the touched nodes that never changed, delete_node returns the deleted reference and the new dangling edges left behind with the same count, adopt returns the new baseline's date, reason and count with the count it replaced, and configure returns the config with the count of findings now new."},{"id":"untouched","statement":"start_ticket records the fingerprint of every node as the ticket's start; close_ticket answers with the touched nodes whose fingerprint is unchanged, or null when the ticket has no start, and removes the start."}]}
//! ```

use serde::Deserialize;
use serde_json::Value;

use super::read::baseline_summary;
use super::{answer, parse, Outcome, RefIn, State};
use crate::answer::*;
use crate::lint::{self, Rule};
use crate::model::*;
use crate::ops::{self, Changed, ToolError};
use crate::query;

/// Run the write tool `name`. `today` is the ISO date used when a decision has none.
pub(super) fn call(state: &mut State, name: &str, input: Value, today: &str) -> Result<Outcome, ToolError> {
    match name {
        "put_page" => {
            let page: Page = parse(input)?;
            let (node, changed) = ops::put_page(state.store_mut(), page)?;
            let r = Reference::page(&node.slug);
            written(name, state, &r, changed)
        }
        "patch_page" => {
            #[derive(Deserialize)]
            #[serde(deny_unknown_fields)]
            struct In {
                r#ref: Reference,
                title: Option<String>,
                body: Option<String>,
                add_links: Option<Vec<Reference>>,
                remove_links: Option<Vec<Reference>>,
            }
            let i: In = parse(input)?;
            let patch = ops::PagePatch { title: i.title, body: i.body, add_links: i.add_links.unwrap_or_default(), remove_links: i.remove_links.unwrap_or_default() };
            let (_, changed) = ops::patch_page(state.store_mut(), &i.r#ref, patch)?;
            written(name, state, &i.r#ref, changed)
        }
        "put_requirement" => {
            #[derive(Deserialize)]
            #[serde(deny_unknown_fields)]
            struct In {
                slug: String,
                title: String,
                statement: String,
                scenarios: Vec<Scenario>,
                covered_by: Vec<Reference>,
                #[allow(dead_code)]
                decisions: Option<Vec<Decision>>,
            }
            let i: In = parse(input)?;
            let req = Requirement { slug: i.slug, title: i.title, statement: i.statement, scenarios: i.scenarios, covered_by: i.covered_by, decisions: vec![] };
            let (node, changed) = ops::put_requirement(state.store_mut(), req)?;
            let r = Reference::req(&node.slug);
            written(name, state, &r, changed)
        }
        "record_decision" => {
            #[derive(Deserialize)]
            struct In {
                r#ref: Reference,
                text: String,
                date: Option<String>,
            }
            let i: In = parse(input)?;
            let (node, changed) = ops::record_decision(state.store_mut(), &i.r#ref, i.text, i.date.unwrap_or_else(|| today.to_string()))?;
            let r = Reference::req(&node.slug);
            let node = query::view(state.graph(), &r).expect("just written");
            answer(name, NodeAnswer { node }, Some(changed))
        }
        "file_ticket" => {
            #[derive(Deserialize)]
            #[serde(deny_unknown_fields)]
            struct In {
                title: String,
                kind: TicketKind,
                priority: Priority,
                touches: Option<Vec<Reference>>,
                relies_on: Option<Vec<Reference>>,
                body: String,
            }
            if input.get("slug").is_some() {
                return Err(ToolError::invalid("file_ticket no longer takes a slug: the id is generated. Give a title.", vec![]));
            }
            let i: In = parse(input)?;
            let t = ops::NewTicket { title: i.title, kind: i.kind, priority: i.priority, touches: i.touches.unwrap_or_default(), relies_on: i.relies_on.unwrap_or_default(), body: i.body };
            let (node, changed) = ops::file_ticket(state.store_mut(), t)?;
            let r = Reference::ticket(&node.slug);
            written(name, state, &r, changed)
        }
        "update_ticket" => {
            #[derive(Deserialize)]
            #[serde(deny_unknown_fields)]
            struct In {
                r#ref: Reference,
                title: Option<String>,
                priority: Option<Priority>,
                touches: Option<Vec<Reference>>,
                relies_on: Option<Vec<Reference>>,
                body: Option<String>,
            }
            let i: In = parse(input)?;
            let patch = ops::TicketPatch { title: i.title, priority: i.priority, touches: i.touches, relies_on: i.relies_on, body: i.body };
            let (_, changed) = ops::update_ticket(state.store_mut(), &i.r#ref, patch)?;
            written(name, state, &i.r#ref, changed)
        }
        "start_ticket" => {
            let i: RefIn = parse(input)?;
            ops::start_ticket(state.store_mut(), &i.r#ref)?;
            // Every node, not only what touches lists now: nodes are often
            // added to touches after they changed.
            let fingerprints = crate::fingerprint::all(state.graph()).into_iter().map(|(r, fp)| (r.to_string(), fp)).collect();
            state.store_mut().starts.insert(i.r#ref.id.clone(), Start { ticket: i.r#ref.id.clone(), fingerprints });
            let changed = Changed::Started(i.r#ref.id.clone());
            let g = state.graph();
            let node = query::view(g, &i.r#ref).expect("just written");
            let rests_on = query::rests_on(g, &i.r#ref, None);
            let related = match g.get(&i.r#ref) {
                Some(crate::graph::Node::Ticket(t)) => query::related_tickets(g, t),
                _ => vec![],
            };
            answer(name, Started { node, rests_on, related }, Some(changed))
        }
        "close_ticket" => {
            #[derive(Deserialize)]
            struct DecisionIn {
                text: String,
                date: Option<String>,
            }
            #[derive(Deserialize)]
            struct In {
                r#ref: Reference,
                decision: DecisionIn,
            }
            let i: In = parse(input)?;
            let touched: Vec<Reference> = match state.store.tickets.get(&i.r#ref.id) {
                Some(t) if i.r#ref.kind == Kind::Ticket => t.touches.clone(),
                _ => return Err(ToolError::not_found(&i.r#ref)),
            };
            // Compared before the close, which changes the ticket's own record.
            let untouched = state.store.starts.get(&i.r#ref.id).cloned().map(|start| crate::fingerprint::untouched(&touched, &start.fingerprints, &crate::fingerprint::all(state.graph())));
            let decision = Decision { date: i.decision.date.unwrap_or_else(|| today.to_string()), text: i.decision.text };
            ops::close_ticket(state.store_mut(), &i.r#ref, decision)?;
            state.store_mut().starts.remove(&i.r#ref.id);
            let changed = Changed::Closed(i.r#ref.id.clone());
            let g = state.graph();
            let node = query::view(g, &i.r#ref).expect("just written");
            let (findings, baselined) = split_new(lint::lint_touching(g, &touched));
            answer(name, Closed { node, findings, baselined, untouched }, Some(changed))
        }
        "rename_node" => {
            #[derive(Deserialize)]
            #[serde(deny_unknown_fields)]
            struct In {
                r#ref: Reference,
                to: String,
            }
            let i: In = parse(input)?;
            // What the code names is read before the rename; the tool never edits it.
            let g = state.graph();
            let still_named_in: Vec<StillNamed> = g
                .inbound_with(&i.r#ref, crate::model::Relation::Checks)
                .filter_map(|e| match g.get(&e.from) {
                    Some(crate::graph::Node::Test(t)) => Some(StillNamed { test: e.from.clone(), file: t.file.clone(), line: t.line }),
                    _ => None,
                })
                .collect();
            let (new, changed) = ops::rename_node(state.store_mut(), &i.r#ref, &i.to)?;
            let node = query::view(state.graph(), &new).expect("just renamed");
            answer(name, Renamed { node, still_named_in }, Some(changed))
        }
        "reorder" => {
            #[derive(Deserialize)]
            #[serde(deny_unknown_fields)]
            struct In {
                priority: Priority,
                order: Vec<Reference>,
            }
            let i: In = parse(input)?;
            let (slugs, changed) = ops::reorder(state.store_mut(), i.priority, &i.order)?;
            let order = slugs.iter().map(|s| Reference::ticket(s)).collect();
            answer(name, Reordered { priority: i.priority, order }, Some(changed))
        }
        "configure" => {
            let change: ops::ConfigChange = parse(input)?;
            let supported = state.build.supported_languages.clone();
            let (config, changed) = ops::configure(state.store_mut(), change, &supported)?;
            let new = lint::new_count(&lint::lint(state.graph(), &Rule::ALL));
            answer(name, Configured { config, new }, Some(changed))
        }
        "adopt" => {
            #[derive(Deserialize)]
            #[serde(deny_unknown_fields)]
            struct In {
                #[serde(default)]
                reason: String,
            }
            let i: In = parse(input)?;
            let keys = lint::lint(state.graph(), &Rule::ALL).iter().map(lint::Finding::key).collect();
            let (baseline, replaced, changed) = ops::adopt(state.store_mut(), keys, i.reason, today.to_string())?;
            answer(name, Adopted { baseline: baseline_summary(&baseline), replaced }, Some(changed))
        }
        "delete_node" => {
            let i: RefIn = parse(input)?;
            // Edges that pointed at the node become dangling: collect them first.
            let pointing: Vec<Reference> = state.graph().inbound(&i.r#ref).map(|e| e.from.clone()).collect();
            let changed = ops::delete_node(state.store_mut(), &i.r#ref)?;
            let g = state.graph();
            let mut warnings = Vec::new();
            for from in pointing {
                warnings.extend(lint::dangling_from(g, &from).into_iter().filter(|f| f.edge.as_ref().is_some_and(|e| e.to == i.r#ref)));
            }
            let (warnings, baselined) = split_new(warnings);
            answer(name, Deleted { deleted: i.r#ref, warnings, baselined }, Some(changed))
        }
        other => Err(ToolError::invalid(format!("unknown tool `{other}`"), vec![])),
    }
}

fn written(name: &str, state: &mut State, r: &Reference, changed: Changed) -> Result<Outcome, ToolError> {
    let g = state.graph();
    let node = query::view(g, r).expect("just written");
    let (warnings, baselined) = split_new(lint::written(g, r));
    answer(name, Written { node, warnings, baselined }, Some(changed))
}

/// The new findings, which are the caller's to act on, and how many
/// baselined ones were left out: inherited work, listed by `lint`.
fn split_new(findings: Vec<lint::Finding>) -> (Vec<lint::Finding>, usize) {
    let total = findings.len();
    let new: Vec<lint::Finding> = findings.into_iter().filter(|f| !f.baselined).collect();
    let baselined = total - new.len();
    (new, baselined)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    use crate::extraction::Extraction;
    use crate::tools::call;
    use crate::tools::fixtures::{built, state};

    /// # Test contract
    ///
    /// ```json
    /// {"test": "write-then-read-sees-it", "checks": ["crates/core/src/ops#never-fails-on-lint", "crates/core/src/query/lookup#node-view", "crates/core/src/query/lookup#get", "crates/core/src/tools/state#graph-current"]}
    /// ```
    #[test]
    fn write_then_read_sees_it() {
        let mut s = state();
        let out = call(&mut s, "file_ticket", json!({ "title": "New", "kind": "change", "priority": "high", "touches": ["page:nope"], "body": "x" }), "2026-09-27").unwrap();
        assert_eq!(out.output["warnings"].as_array().unwrap().len(), 1);
        assert!(matches!(out.changed, Some(Changed::Ticket(ref s)) if s == "0001"));
        let got = call(&mut s, "get", json!({ "ref": "ticket:0001" }), "2026-09-27").unwrap();
        assert_eq!(got.output["node"]["status"], "open");
        // Edges carry titles so an id is never read alone.
        assert_eq!(got.output["node"]["out"][0]["ref"], "page:nope");
        assert!(got.output["node"]["out"][0]["title"].is_null(), "a dangling target has no title");
        let page = call(&mut s, "get", json!({ "ref": "page:git-readiness" }), "2026-09-27").unwrap();
        assert!(page.output["node"]["in"].as_array().unwrap().iter().any(|e| e["ref"] == "ticket:phase-62" && e["title"] == "PHASE-62"));
        let board = call(&mut s, "board", json!({}), "2026-09-27").unwrap();
        assert!(board.output["tickets"].as_array().unwrap().iter().any(|t| t["ref"] == "ticket:0001"));
        let e = call(&mut s, "file_ticket", json!({ "slug": "x", "title": "New", "kind": "change", "priority": "high", "body": "x" }), "2026-09-27").unwrap_err();
        assert!(e.to_string().contains("no longer takes a slug"));
    }

    /// # Test contract
    ///
    /// ```json
    /// {"test": "put-requirement-warns-when-unspecified", "checks": ["crates/core/src/ops#never-fails-on-lint"]}
    /// ```
    #[test]
    fn put_requirement_warns_when_unspecified() {
        let mut s = state();
        let req = |scenarios: Value| json!({ "slug": "new-rule", "title": "New", "statement": "It SHALL hold.", "scenarios": scenarios, "covered_by": [] });
        let out = call(&mut s, "put_requirement", req(json!([])), "2026-09-27").unwrap();
        let w = out.output["warnings"].as_array().unwrap();
        assert_eq!(w.len(), 1);
        assert_eq!(w[0]["rule"], "unspecified");
        assert_eq!(w[0]["ref"], "req:new-rule");
        let scenario = json!([{ "title": "T", "given": "g", "when": "w", "then": "t" }]);
        let out = call(&mut s, "put_requirement", req(scenario), "2026-09-27").unwrap();
        assert!(out.output["warnings"].as_array().unwrap().is_empty());
    }

    /// # Test contract
    ///
    /// ```json
    /// {"test": "adopt-baselines-what-is-present", "checks": ["crates/core/src/ops#adopt", "crates/core/src/tools/read#status", "crates/core/src/tools/read#lint-tool", "crates/core/src/model#baseline-record"]}
    /// ```
    #[test]
    fn adopt_baselines_what_is_present() {
        let mut s = state();
        let before = call(&mut s, "lint", json!({}), "2026-09-28").unwrap().output;
        let total = before["findings"].as_array().unwrap().len();
        assert_eq!(before["new"], total, "nothing baselined yet");
        assert!(call(&mut s, "status", json!({}), "2026-09-28").unwrap().output["baseline"].is_null());

        let e = call(&mut s, "adopt", json!({}), "2026-09-28").unwrap_err();
        assert!(matches!(e, ToolError::Invalid { .. }), "a reason is required");

        let out = call(&mut s, "adopt", json!({ "reason": "adopted at init" }), "2026-09-28").unwrap();
        assert_eq!(out.output, json!({ "baseline": { "date": "2026-09-28", "reason": "adopted at init", "count": total }, "replaced": null }));
        assert!(matches!(out.changed, Some(Changed::Baseline)));
        let after = call(&mut s, "lint", json!({}), "2026-09-28").unwrap().output;
        assert_eq!(after["new"], 0);
        assert!(after["findings"].as_array().unwrap().iter().all(|f| f["baselined"] == true));
        assert_eq!(call(&mut s, "status", json!({}), "2026-09-28").unwrap().output["baseline"]["count"], total);

        let again = call(&mut s, "adopt", json!({ "reason": "again" }), "2026-09-29").unwrap();
        assert_eq!(again.output["replaced"], total);
    }

    /// # Test contract
    ///
    /// ```json
    /// {"test": "a-fix-prunes-and-a-regression-is-new", "checks": ["crates/core/src/tools/read#lint-tool"]}
    /// ```
    #[test]
    fn a_fix_prunes_and_a_regression_is_new() {
        let mut s = state();
        call(&mut s, "adopt", json!({ "reason": "adopted" }), "2026-09-28").unwrap();
        assert!(s.prune_baseline().is_none(), "nothing fixed yet");
        // Fix the one unspecified requirement.
        let scenario = json!([{ "title": "T", "given": "g", "when": "w", "then": "t" }]);
        call(&mut s, "put_requirement", json!({ "slug": "stale", "title": "Stale", "statement": "It SHALL.", "scenarios": scenario, "covered_by": ["criterion:src/a/ready#gone"] }), "2026-09-28").unwrap();
        assert!(matches!(s.prune_baseline(), Some(Changed::Baseline)));
        // Break it again: now it is new.
        call(&mut s, "put_requirement", json!({ "slug": "stale", "title": "Stale", "statement": "It SHALL.", "scenarios": [], "covered_by": ["criterion:src/a/ready#gone"] }), "2026-09-28").unwrap();
        let lint = call(&mut s, "lint", json!({ "kinds": ["unspecified"] }), "2026-09-28").unwrap().output;
        assert_eq!(lint["new"], 1);
        assert_eq!(lint["findings"][0]["baselined"], false);
    }

    /// # Test contract
    ///
    /// ```json
    /// {"test": "configure-changes-only-what-it-is-given", "checks": ["crates/core/src/ops#configure"]}
    /// ```
    #[test]
    fn configure_changes_only_what_it_is_given() {
        let mut s = built(state(), &["rust", "typescript"]);
        let before = s.store().config.clone();
        let out = call(&mut s, "configure", json!({ "languages": ["rust", "typescript"] }), "2026-09-28").unwrap();
        assert!(matches!(out.changed, Some(Changed::Config)));
        assert_eq!(out.output["config"]["languages"], json!(["rust", "typescript"]));
        assert_eq!(s.store().config.roots, before.roots);
        assert_eq!(s.store().config.repository, before.repository);
    }

    /// # Test contract
    ///
    /// ```json
    /// {"test": "configure-refuses-what-it-may-not-change", "checks": ["crates/core/src/ops#configure", "crates/core/src/ops#validate-first"]}
    /// ```
    #[test]
    fn configure_refuses_what_it_may_not_change() {
        let mut s = built(state(), &["rust"]);
        let before = s.store().config.clone();
        let unsupported = call(&mut s, "configure", json!({ "languages": ["python"] }), "2026-09-28").unwrap_err();
        assert!(matches!(unsupported, ToolError::Invalid { .. }));
        assert!(unsupported.to_string().contains("config"), "{unsupported}");
        let repo = call(&mut s, "configure", json!({ "repository": "D:/elsewhere" }), "2026-09-28").unwrap_err();
        assert!(repo.to_string().contains("repository"), "{repo}");
        let prefix = call(&mut s, "configure", json!({ "scope": { "include": ["../outside"] } }), "2026-09-28").unwrap_err();
        assert!(serde_json::to_value(&prefix).unwrap()["details"][0]["field"] == "scope.include");
        assert_eq!(s.store().config, before, "nothing changed");
    }

    /// # Test contract
    ///
    /// ```json
    /// {"test": "widening-the-scope-makes-new-findings", "checks": ["crates/core/src/tools/read#status"]}
    /// ```
    #[test]
    fn widening_the_scope_makes_new_findings() {
        let mut s = built(state(), &["rust"]);
        call(&mut s, "configure", json!({ "scope": { "include": ["src/a"] } }), "2026-09-28").unwrap();
        call(&mut s, "adopt", json!({ "reason": "adopted with src/a in scope" }), "2026-09-28").unwrap();
        let out = call(&mut s, "configure", json!({ "scope": { "include": ["src/a", "src/b"] } }), "2026-09-28").unwrap();
        assert_eq!(out.output["new"], 2, "src/b/unrelated is now expected to carry a contract and a page");
        assert_eq!(call(&mut s, "status", json!({}), "2026-09-28").unwrap().output["scope"]["include"], json!(["src/a", "src/b"]));
    }

    /// # Test contract
    ///
    /// ```json
    /// {"test": "inherited-findings-are-counted", "checks": ["crates/core/src/tools/write#write-responses"]}
    /// ```
    #[test]
    fn inherited_findings_are_counted_not_handed_over() {
        let mut s = state();
        call(&mut s, "adopt", json!({ "reason": "adopted" }), "2026-09-28").unwrap();
        let out = call(&mut s, "close_ticket", json!({ "ref": "ticket:phase-62", "decision": { "text": "Done." } }), "2026-09-28").unwrap().output;
        assert_eq!(out["findings"], json!([]), "everything on the touched nodes is inherited");
        assert!(out["baselined"].as_u64().unwrap() > 0);
        let w = call(&mut s, "put_requirement", json!({ "slug": "stale", "title": "Stale", "statement": "It SHALL.", "scenarios": [], "covered_by": ["criterion:src/a/ready#gone"] }), "2026-09-28").unwrap().output;
        assert_eq!(w["warnings"], json!([]), "stale was already dangling and unspecified when adopted");
        assert_eq!(w["baselined"], 2);
        let fresh = call(&mut s, "put_requirement", json!({ "slug": "fresh", "title": "Fresh", "statement": "It SHALL.", "scenarios": [], "covered_by": [] }), "2026-09-28").unwrap().output;
        assert_eq!(fresh["warnings"][0]["check"], "no-scenario", "new work is handed over");
        assert_eq!(fresh["baselined"], 0);
    }

    /// # Test contract
    ///
    /// ```json
    /// {"test": "record-decision-defaults-to-today", "checks": ["crates/core/src/ops#record-decision"]}
    /// ```
    #[test]
    fn record_decision_defaults_to_today() {
        let mut s = state();
        let out = call(&mut s, "record_decision", json!({ "ref": "req:ready-only-with-commit", "text": "Why." }), "2026-09-28").unwrap().output;
        let last = out["node"]["decisions"].as_array().unwrap().last().unwrap().clone();
        assert_eq!(last, json!({ "date": "2026-09-28", "text": "Why." }));
    }

    /// # Test contract
    ///
    /// ```json
    /// {"test": "a-close-names-what-never-changed", "checks": ["crates/core/src/tools/write#untouched"]}
    /// ```
    #[test]
    fn a_close_names_what_never_changed() {
        let mut s = state();
        call(&mut s, "start_ticket", json!({ "ref": "ticket:phase-62" }), "2026-09-28").unwrap();
        assert!(s.store().starts.contains_key("phase-62"), "the start is recorded");
        // phase-62 touches req:ready-only-with-commit and module:src/a/ready. Change
        // the requirement, and add to touches a page changed before it was listed.
        call(&mut s, "patch_page", json!({ "ref": "page:orphan", "body": "Changed before being listed." }), "2026-09-28").unwrap();
        call(&mut s, "put_requirement", json!({ "slug": "ready-only-with-commit", "title": "Changed", "statement": "It SHALL.", "scenarios": [{ "title": "T", "given": "g", "when": "w", "then": "t" }], "covered_by": ["criterion:src/a/ready#guarantee"] }), "2026-09-28").unwrap();
        call(&mut s, "update_ticket", json!({ "ref": "ticket:phase-62", "touches": ["req:ready-only-with-commit", "module:src/a/ready", "page:orphan", "req:never-was"] }), "2026-09-28").unwrap();
        let out = call(&mut s, "close_ticket", json!({ "ref": "ticket:phase-62", "decision": { "text": "Done." } }), "2026-09-28").unwrap();
        assert_eq!(out.output["untouched"], json!(["module:src/a/ready"]), "the requirement and the page changed; a reference that never existed is the lint's");
        assert!(matches!(out.changed, Some(Changed::Closed(ref slug)) if slug == "phase-62"));
        assert!(!s.store().starts.contains_key("phase-62"), "the start is removed on close");
    }

    #[test]
    fn a_ticket_with_no_start_answers_null() {
        let mut s = state();
        let out = call(&mut s, "close_ticket", json!({ "ref": "ticket:later-1", "decision": { "text": "Done." } }), "2026-09-28").unwrap();
        assert!(out.output["untouched"].is_null(), "unknown, not none");
    }

    /// # Test contract
    ///
    /// ```json
    /// {"test": "reorder-sets-one-band", "checks": ["crates/core/src/ops#reorder"]}
    /// ```
    #[test]
    fn reorder_sets_one_band() {
        let mut s = state();
        // phase-62 is next and open; later-1 is later and active; old is closed.
        let out = call(&mut s, "reorder", json!({ "priority": "next", "order": ["ticket:phase-62"] }), "2026-09-28").unwrap();
        assert_eq!(out.output, json!({ "priority": "next", "order": ["ticket:phase-62"] }));
        assert!(matches!(out.changed, Some(Changed::Config)));
        assert_eq!(s.store().config.order.next, vec!["phase-62".to_string()]);
        for bad in [json!(["ticket:later-1"]), json!(["ticket:old"]), json!(["ticket:phase-62", "ticket:phase-62"]), json!(["page:overview"])] {
            let e = call(&mut s, "reorder", json!({ "priority": "next", "order": bad }), "2026-09-28").unwrap_err();
            assert!(matches!(e, ToolError::Invalid { .. }), "{bad} is refused");
        }
    }

    /// # Test contract
    ///
    /// ```json
    /// {"test": "a-rename-keeps-decisions-and-rewrites-references", "checks": ["crates/core/src/ops#rename"]}
    /// ```
    #[test]
    fn a_rename_keeps_decisions_and_rewrites_references() {
        let mut s = state();
        // ready-only-with-commit: one dated decision, linked from git-readiness,
        // touched by the open ticket phase-62; and a closed ticket names it too.
        call(&mut s, "file_ticket", json!({ "title": "Old", "kind": "change", "priority": "later", "touches": ["req:ready-only-with-commit"], "body": "b" }), "2026-09-28").unwrap();
        call(&mut s, "start_ticket", json!({ "ref": "ticket:0001" }), "2026-09-28").unwrap();
        call(&mut s, "close_ticket", json!({ "ref": "ticket:0001", "decision": { "text": "Done." } }), "2026-09-28").unwrap();
        call(&mut s, "start_ticket", json!({ "ref": "ticket:phase-62" }), "2026-09-28").unwrap();
        call(&mut s, "adopt", json!({ "reason": "adopted" }), "2026-09-28").unwrap();
        let before = s.store().requirements["ready-only-with-commit"].clone();
        let out = call(&mut s, "rename_node", json!({ "ref": "req:ready-only-with-commit", "to": "ready-with-a-commit" }), "2026-09-28").unwrap();
        assert_eq!(out.output["node"]["ref"], "req:ready-with-a-commit");
        let after = &s.store().requirements["ready-with-a-commit"];
        assert_eq!(after.decisions, before.decisions, "decisions keep their dates");
        assert!(!s.store().requirements.contains_key("ready-only-with-commit"));
        assert!(s.store().pages["git-readiness"].links.contains(&Reference::req("ready-with-a-commit")));
        assert!(s.store().tickets["phase-62"].touches.contains(&Reference::req("ready-with-a-commit")), "an open ticket follows");
        assert!(s.store().tickets["0001"].touches.contains(&Reference::req("ready-only-with-commit")), "a closed ticket is history");
        assert!(s.store().starts["phase-62"].fingerprints.contains_key("req:ready-with-a-commit"), "the start snapshot follows");
        let lint = call(&mut s, "lint", json!({}), "2026-09-28").unwrap().output;
        assert_eq!(lint["new"], 0, "inherited findings stay inherited");
        // The changes persist new record first and old record last.
        let Some(Changed::Many(cs)) = out.changed else { panic!("a rename is several changes") };
        assert!(matches!(cs.first(), Some(Changed::Requirement(x)) if x == "ready-with-a-commit"));
        assert!(matches!(cs.last(), Some(Changed::Deleted(r)) if r == &Reference::req("ready-only-with-commit")));
    }

    #[test]
    fn rename_refuses_what_it_may_not_do() {
        let mut s = state();
        let code = |r: Result<Outcome, ToolError>| serde_json::to_value(r.unwrap_err()).unwrap()["error"].clone();
        assert_eq!(code(call(&mut s, "rename_node", json!({ "ref": "ticket:phase-62", "to": "x" }), "2026-09-28")), "invalid");
        assert_eq!(code(call(&mut s, "rename_node", json!({ "ref": "module:src/a/ready", "to": "x" }), "2026-09-28")), "invalid");
        assert_eq!(code(call(&mut s, "rename_node", json!({ "ref": "page:nope", "to": "x" }), "2026-09-28")), "not_found");
        assert_eq!(code(call(&mut s, "rename_node", json!({ "ref": "page:orphan", "to": "overview" }), "2026-09-28")), "conflict");
        assert_eq!(code(call(&mut s, "rename_node", json!({ "ref": "page:orphan", "to": "Not A Slug" }), "2026-09-28")), "invalid");
        // A root page's rename moves the root.
        call(&mut s, "rename_node", json!({ "ref": "page:overview", "to": "home" }), "2026-09-28").unwrap();
        assert_eq!(s.store().config.roots, vec!["home".to_string()]);
    }

    #[test]
    fn a_rename_reports_the_tests_that_name_it() {
        let mut s = state();
        let mut files = crate::extraction::fixtures::extraction_files();
        files[0].tests.push(Test { file: "src/a/ready.rs".into(), name: "holds".into(), line: 50, checks: vec!["req:stale".into()] });
        s.set_extraction(Extraction::from_files(&files));
        let out = call(&mut s, "rename_node", json!({ "ref": "req:stale", "to": "fresh" }), "2026-09-28").unwrap();
        assert_eq!(out.output["still_named_in"], json!([{ "test": "test:src/a/ready.rs#holds", "file": "src/a/ready.rs", "line": 50 }]));
    }

    #[test]
    fn start_and_close_return_context() {
        let mut s = state();
        let out = call(&mut s, "start_ticket", json!({ "ref": "ticket:phase-62" }), "2026-09-27").unwrap();
        assert!(!out.output["rests_on"].as_array().unwrap().is_empty());
        assert_eq!(out.output["related"].as_array().unwrap().len(), 1);
        let out = call(&mut s, "close_ticket", json!({ "ref": "ticket:phase-62", "decision": { "text": "Done." } }), "2026-09-27").unwrap();
        assert_eq!(out.output["node"]["decision"]["date"], "2026-09-27");
        assert!(out.output["findings"].is_array());
    }

    #[test]
    fn delete_reports_new_dangling() {
        let mut s = state();
        let out = call(&mut s, "delete_node", json!({ "ref": "page:git-readiness" }), "2026-09-27").unwrap();
        let w = out.output["warnings"].as_array().unwrap();
        assert!(w.iter().any(|f| f["ref"] == "page:overview"));
        assert!(w.iter().any(|f| f["ref"] == "ticket:phase-62"));
    }
}
