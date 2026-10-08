//! # Module contract
//!
//! ```json
//! {"purpose":"Keep a daemon's reading of its checkout's code current: one changed file at a time as the watcher reports it, the whole checkout before anything is judged over it, and every recipe whose files changed.","criteria":[{"id":"one-file-reextract","statement":"A changed source file is re-extracted alone and the extraction re-joined; unchanged files are not re-read; a file the extractor no longer reads, as one in a folder that became a checkout of its own, is removed from the extraction, and a nested checkout's marker appearing or going reads the whole checkout again."},{"id":"catches-up","statement":"Before close_ticket, bottle_ticket or judge_recipe, on a daemon that holds code and before a write is sent to a team's server, catch_up makes the extraction what reading the whole checkout would make it, without waiting for the watcher: it walks what that reading would read, its ignore rules and nested checkouts heeded, keeps each held source file whose text's digest matches, extracts each that differs or appeared and every recipe file whatever its digest, and removes every other held file, one no longer readable as text among them; then re-joins the graph and announces what changed; with nothing changed it extracts nothing and announces nothing; a checkout that cannot be walked refuses the close, bottling or judgement as store_unavailable, judging nothing."},{"id":"recipes-follow-files","statement":"When the paths handed to reextract include a path a held recipe's files name, or a folder holding one or held by one, that recipe's file is read again with them, so a named file appearing, changing or going is seen in what the recipe found without a save to the recipe; recipe_paths gives every path the held recipes name, for the watcher."}]}
//! ```

use std::path::Path;

use grimoire_core::{Extraction, ToolError};
use grimoire_extract::Extractor;

use super::{Daemon, Event};

impl Daemon {
    /// Replace the extractor with one for `languages` and extract the whole
    /// repository again, as on opening.
    /// Whether it did: a repository that could not be read leaves the extraction as it was.
    pub(crate) fn reextract_all(&self, languages: &[String]) -> bool {
        let extractor = Extractor::for_languages(&self.repo, languages);
        let extracted = match extractor.extract_all() {
            Ok((files, errors)) => {
                for e in errors {
                    tracing::warn!("{e}");
                }
                files
            }
            Err(e) => {
                tracing::error!("re-extracting for languages {languages:?} failed: {e}");
                return false;
            }
        };
        let mut files = self.files.lock().expect("files lock");
        *files = extracted.into_iter().map(|f| (f.path.clone(), f)).collect();
        self.state.lock().expect("state lock").set_extraction(Extraction::from_files(files.values()));
        *self.extractor.write().expect("extractor lock") = extractor;
        tracing::info!("languages are now {languages:?}; re-extracted {} file(s)", files.len());
        let _ = self.events.send(Event::Graph { changed: vec!["*".to_string()] });
        true
    }

    /// Bring the extraction up to the checkout's files as they are on disk now, without waiting for
    /// the watcher, before a close or a bottling judges the code (req:daemon-catches-up-before-judging).
    /// The walk is what reading the whole checkout would read, its ignore rules and the checkouts
    /// nested in it heeded, so the extraction becomes exactly that: a file whose text's digest matches
    /// the one held is kept, one that differs or appeared is extracted, and every other held file goes,
    /// one that can no longer be read as text among them, as a whole reading would leave it out.
    /// A checkout that cannot be walked is an error: nothing is judged over code not known to be current.
    pub(crate) fn catch_up(&self) -> Result<(), ToolError> {
        if self.shared {
            return Ok(());
        }
        // The files before the extractor, in the order every other reading takes them.
        let _one = self.kin.write.lock().expect("kin write lock");
        let mut files = self.files.lock().expect("files lock");
        let extractor = self.extractor.read().expect("extractor lock");
        let walked = extractor.walk().map_err(|e| ToolError::unavailable(format!("the checkout could not be read to judge its code as it is now, so nothing was judged: {e}")))?;
        let mut changed = Vec::new();
        for rel in &walked {
            let full = self.repo.join(rel.replace('/', std::path::MAIN_SEPARATOR_STR));
            // Read once: the text compared is the text extracted when it differs.
            let text = std::fs::read(&full).map_err(|e| e.to_string()).and_then(|bytes| String::from_utf8(bytes).map_err(|_| "it is not UTF-8".to_string()));
            let recipe = grimoire_extract::recipe::is_recipe_path(rel);
            match (text, extractor.handles(rel)) {
                (Ok(text), true) => {
                    // A recipe is read whatever its digest: what it finds is other files (req:recipe-read).
                    if !recipe && files.get(rel).is_some_and(|f| f.digest == grimoire_core::fingerprint::of_text(&text)) {
                        continue;
                    }
                    let Some(read) = extractor.extract_text(rel, &text) else { continue };
                    if files.get(rel) == Some(&read) {
                        continue;
                    }
                    files.insert(rel.clone(), read);
                    changed.push(rel.clone());
                }
                // Left out, as a whole reading leaves out a file it cannot read.
                (Err(why), _) => {
                    tracing::warn!("{rel} could not be read: {why}");
                    if files.remove(rel).is_some() {
                        changed.push(rel.clone());
                    }
                }
                (Ok(_), false) => {
                    if files.remove(rel).is_some() {
                        changed.push(rel.clone());
                    }
                }
            }
        }
        let gone: Vec<String> = files.keys().filter(|rel| walked.binary_search(rel).is_err()).cloned().collect();
        for rel in gone {
            files.remove(&rel);
            changed.push(rel);
        }
        if changed.is_empty() {
            return Ok(());
        }
        let extraction = Extraction::from_files(files.values());
        drop(files);
        drop(extractor);
        let mut state = self.state.lock().expect("state lock");
        state.set_extraction(extraction);
        if self.prune(&mut state) {
            self.hand_on(state);
        } else {
            drop(state);
        }
        changed.sort();
        tracing::info!("caught up before judging: {} file(s) changed on disk since last read: {}", changed.len(), changed.join(", "));
        let _ = self.events.send(Event::Graph { changed });
        Ok(())
    }

    /// Every path the held recipes' `files` name: what the watcher hands on though no adapter reads it.
    pub fn recipe_paths(&self) -> Vec<String> {
        let files = self.files.lock().expect("files lock");
        let mut out: Vec<String> = files.values().filter_map(|f| f.recipe.as_ref()?.as_ref().ok()).flat_map(|r| r.files.iter().cloned()).collect();
        out.sort();
        out.dedup();
        out
    }

    /// The held recipe files whose `files` name one of `paths`, a folder holding one or one held by
    /// it, not among `paths` already: read again with them (req:recipe-read).
    fn recipes_naming(&self, paths: &[String]) -> Vec<String> {
        let related = |a: &str, b: &str| a == b || a.strip_prefix(b).is_some_and(|r| r.starts_with('/')) || b.strip_prefix(a).is_some_and(|r| r.starts_with('/'));
        let files = self.files.lock().expect("files lock");
        files
            .values()
            .filter(|f| !paths.contains(&f.path))
            .filter(|f| f.recipe.as_ref().and_then(|r| r.as_ref().ok()).is_some_and(|r| r.files.iter().any(|n| paths.iter().any(|p| related(p, n)))))
            .map(|f| f.path.clone())
            .collect()
    }

    /// Re-extract the given repository-relative paths (deleted ones are
    /// removed) and re-join the graph, with every recipe naming one of them.
    pub fn reextract(&self, paths: &[String]) {
        let mut paths = paths.to_vec();
        paths.extend(self.recipes_naming(&paths));
        let paths = &paths[..];
        let mut changed = Vec::new();
        // A pruned baseline is a write to the store, taken one at a time with its other checkouts'.
        let _one = self.kin.write.lock().expect("kin write lock");
        // A nested checkout's marker appeared or went: where this checkout's code ends moved, so all of it is read again.
        if paths.iter().any(|p| self.extractor.read().expect("extractor lock").is_boundary(p)) {
            let languages = self.state.lock().expect("state lock").store().config.languages.clone();
            let _ = self.reextract_all(&languages);
            let mut state = self.state.lock().expect("state lock");
            if self.prune(&mut state) {
                self.hand_on(state);
            }
            return;
        }
        {
            let mut files = self.files.lock().expect("files lock");
            let extractor = self.extractor.read().expect("extractor lock");
            for rel in paths {
                // Read only when reading the whole checkout would read it (req:extract-nested-checkouts-skipped);
                // one held from before that is no longer read goes.
                if !extractor.reads(rel) {
                    if files.remove(rel).is_some() {
                        changed.push(rel.clone());
                    }
                    continue;
                }
                let full = self.repo.join(rel.replace('/', std::path::MAIN_SEPARATOR_STR));
                if full.is_file() {
                    match extractor.extract_path(rel) {
                        Ok(f) => {
                            if files.get(rel) != Some(&f) {
                                files.insert(rel.clone(), f);
                                changed.push(rel.clone());
                            }
                        }
                        Err(e) => tracing::warn!("{e}"),
                    }
                } else if files.remove(rel).is_some() {
                    changed.push(rel.clone());
                }
            }
            if changed.is_empty() {
                return;
            }
            let extraction = Extraction::from_files(files.values());
            let mut state = self.state.lock().expect("state lock");
            state.set_extraction(extraction);
            if self.prune(&mut state) {
                self.hand_on(state);
            }
        }
        tracing::info!("re-extracted {} file(s): {}", changed.len(), changed.join(", "));
        let _ = self.events.send(Event::Graph { changed });
    }

    /// Repository-relative form of an absolute path inside the repository.
    pub fn relative(&self, path: &Path) -> Option<String> {
        grimoire_extract::relative(&self.repo, path)
    }
}

#[cfg(test)]
mod tests {
    use super::super::tests::{Harness, CONTRACTED};
    use super::*;
    use serde_json::json;

    /// # Test contract
    ///
    /// ```json
    /// {"test": "one-changed-file-is-reextracted-alone", "checks": ["crates/server/src/daemon/extraction#one-file-reextract"]}
    /// ```
    #[test]
    fn one_changed_file_is_reextracted_alone() {
        let h = Harness::new();
        let d = h.open();
        // Change two files on disk, but tell the daemon about one.
        std::fs::write(h.repo.join("src").join("a.rs"), CONTRACTED.replace("It says a.", "It says a, changed.")).unwrap();
        std::fs::write(h.repo.join("src").join("b.rs"), "pub fn b_changed() {}\n").unwrap();
        d.reextract(&["src/a.rs".to_string()]);
        let a = d.call("get", json!({ "ref": "criterion:src/a#says" })).unwrap();
        assert_eq!(a["node"]["statement"], "It says a, changed.");
        let b = d.call("get", json!({ "ref": "file:src/b.rs" })).unwrap();
        assert!(b["node"]["decls"].as_array().unwrap().iter().any(|x| x["path"] == "b"), "the unchanged file was not read again");
    }

    /// # Test contract
    ///
    /// ```json
    /// {"test": "a-close-catches-up-with-the-files-on-disk", "checks": ["crates/server/src/daemon/extraction#catches-up", "req:daemon-catches-up-before-judging"]}
    /// ```
    #[test]
    fn a_close_catches_up_with_the_files_on_disk() {
        let h = Harness::new();
        let d = h.open();
        let mut rx = d.subscribe();
        // Nothing changed: a close reads no file again and rebuilds nothing.
        d.catch_up().unwrap();
        assert!(rx.try_recv().is_err(), "nothing to catch up with");
        // A merge rewrites one file, removes one and adds one, and no watcher has said so.
        std::fs::write(h.repo.join("src").join("a.rs"), CONTRACTED.replace("It says a.", "It says a, merged.")).unwrap();
        std::fs::remove_file(h.repo.join("src").join("b.rs")).unwrap();
        std::fs::write(h.repo.join("src").join("c.rs"), "pub fn c() {}\n").unwrap();
        // The close judges them: the criterion as merged, the file gone, the new one there.
        let closed = d.call("close_ticket", json!({ "ref": "ticket:work", "decision": { "text": "Done." } })).unwrap();
        assert_eq!(closed["node"]["status"], "bottled", "{closed}");
        match rx.try_recv().unwrap() {
            Event::Graph { mut changed } => {
                changed.sort();
                assert_eq!(changed, ["src/a.rs", "src/b.rs", "src/c.rs"]);
            }
            other => panic!("expected the catch-up's rebuild first, got {other:?}"),
        }
        assert_eq!(d.call("get", json!({ "ref": "criterion:src/a#says" })).unwrap()["node"]["statement"], "It says a, merged.");
        assert_eq!(d.call("get", json!({ "ref": "file:src/b.rs" })).unwrap_err().code(), "not_found");
        assert!(d.call("get", json!({ "ref": "file:src/c.rs" })).is_ok());
    }

    /// # Test contract
    ///
    /// ```json
    /// {"test": "catching-up-reads-what-the-whole-checkout-would", "checks": ["crates/server/src/daemon/extraction#catches-up", "req:daemon-catches-up-before-judging"]}
    /// ```
    #[test]
    fn catching_up_reads_what_the_whole_checkout_would() {
        let h = Harness::new();
        std::fs::create_dir_all(h.repo.join("pkg").join("src")).unwrap();
        std::fs::write(h.repo.join("pkg").join("src").join("lib.rs"), "pub fn p() {}\n").unwrap();
        let d = h.open();
        assert!(d.call("get", json!({ "ref": "file:pkg/src/lib.rs" })).is_ok());
        // A file newly ignored, and a folder that became a checkout of its own: a whole reading leaves both out.
        std::fs::write(h.repo.join(".gitignore"), "src/b.rs\n").unwrap();
        std::fs::write(h.repo.join("pkg").join(".grimoire.toml"), "store = \"elsewhere\"\n").unwrap();
        d.catch_up().unwrap();
        assert!(d.call("get", json!({ "ref": "file:src/b.rs" })).is_err(), "an ignored file is not read back in");
        assert!(d.call("get", json!({ "ref": "file:pkg/src/lib.rs" })).is_err(), "a nested checkout's code is not this checkout's");
        let fresh = Daemon::open(&h.store).unwrap().0;
        assert_eq!(d.files.lock().unwrap().keys().collect::<Vec<_>>(), fresh.files.lock().unwrap().keys().collect::<Vec<_>>(), "as a whole reading has it");
    }

    /// # Test contract
    ///
    /// ```json
    /// {"test": "a-folder-that-becomes-a-checkout-leaves-the-graph", "checks": ["crates/server/src/daemon/extraction#one-file-reextract", "req:extract-nested-checkouts-skipped"]}
    /// ```
    #[test]
    fn a_folder_that_becomes_a_checkout_leaves_the_graph() {
        let h = Harness::new();
        let d = h.open();
        let held = |d: &Daemon| d.files.lock().unwrap().contains_key("pkg/src/lib.rs");
        // A folder of this checkout's code, read as it changes.
        std::fs::create_dir_all(h.repo.join("pkg").join("src")).unwrap();
        std::fs::write(h.repo.join("pkg").join("src").join("lib.rs"), "pub fn p() {}\n").unwrap();
        d.reextract(&["pkg/src/lib.rs".to_string()]);
        assert!(held(&d));
        // It becomes a checkout of its own: its marker arriving takes its code out, and a later save is not read.
        std::fs::write(h.repo.join("pkg").join(".grimoire.toml"), "store = \"other\"\n").unwrap();
        d.reextract(&["pkg/.grimoire.toml".to_string()]);
        assert!(!held(&d), "the nested checkout's code is its own");
        std::fs::write(h.repo.join("pkg").join("src").join("lib.rs"), "pub fn p2() {}\n").unwrap();
        d.reextract(&["pkg/src/lib.rs".to_string()]);
        assert!(!held(&d), "a save there is not read");
        // The marker going brings it back, read whole again.
        std::fs::remove_file(h.repo.join("pkg").join(".grimoire.toml")).unwrap();
        d.reextract(&["pkg/.grimoire.toml".to_string()]);
        assert!(held(&d), "the folder is this checkout's again");
        // A file held from before that the extractor no longer reads goes when it is named.
        let held_from_before = d.files.lock().unwrap()["src/a.rs"].clone();
        d.files.lock().unwrap().insert(".hidden/x.rs".to_string(), held_from_before);
        d.reextract(&[".hidden/x.rs".to_string()]);
        assert!(!d.files.lock().unwrap().contains_key(".hidden/x.rs"));
    }

    /// # Test contract
    ///
    /// ```json
    /// {"test": "a-recipe-follows-the-files-it-names", "checks": ["crates/server/src/daemon/extraction#recipes-follow-files", "crates/server/src/daemon/extraction#catches-up", "req:recipe-read", "req:daemon-catches-up-before-judging"]}
    /// ```
    #[test]
    fn a_recipe_follows_the_files_it_names() {
        let h = Harness::new();
        std::fs::create_dir_all(h.repo.join("ui")).unwrap();
        std::fs::write(h.repo.join("ui").join("button.recipe.md"), "# Button\n\n## Recipe\n\n```json\n{\"id\":\"button\",\"kind\":\"component\",\"files\":[\"ui/button\",\"src/a.rs\"]}\n```\n")
            .unwrap();
        let d = h.open();
        let missing = |d: &Daemon| d.call("get", json!({ "ref": "recipe:button" })).unwrap()["node"]["missing"].clone();
        assert_eq!(missing(&d), json!(["ui/button"]), "read on opening, a store naming only rust");
        assert_eq!(d.recipe_paths(), ["src/a.rs", "ui/button"]);
        // A file appears in the folder it names: the watcher hands on that file alone, and the recipe is read again.
        std::fs::create_dir_all(h.repo.join("ui").join("button")).unwrap();
        std::fs::write(h.repo.join("ui").join("button").join("dialog.xml"), "<dialog/>\n").unwrap();
        d.reextract(&["ui/button/dialog.xml".to_string()]);
        assert_eq!(missing(&d), json!([]));
        // A source file it names goes: the file and the recipe are read again.
        std::fs::remove_file(h.repo.join("src").join("a.rs")).unwrap();
        d.reextract(&["src/a.rs".to_string()]);
        assert_eq!(missing(&d), json!(["src/a.rs"]));
        // Restored with no watcher to say so: a close's catch-up reads the recipe again whatever its digest.
        std::fs::write(h.repo.join("src").join("a.rs"), CONTRACTED).unwrap();
        d.catch_up().unwrap();
        assert_eq!(missing(&d), json!([]));
        // A judgement binds the recipe and its files as they are on disk: changed with no watcher to say so,
        // judge_recipe catches up first, and the digest it records is the one the change gives.
        let digest = |d: &Daemon| d.call("get", json!({ "ref": "recipe:button" })).unwrap()["node"]["digest"].clone();
        let before = digest(&d);
        std::fs::write(
            h.repo.join("ui").join("button").join("dialog.xml"),
            "<dialog open/>
",
        )
        .unwrap();
        let ticket = d.call("inscribe_ticket", json!({ "title": "T", "kind": "change", "priority": "later", "body": "b" })).unwrap()["node"]["ref"].clone();
        d.call("brew_ticket", json!({ "ref": ticket })).unwrap();
        let judged = d.call("judge_recipe", json!({ "ref": "recipe:button", "outcome": "held" })).unwrap();
        assert_ne!(judged["judged"][0]["judgement"]["fingerprint"], before, "not the digest the watcher last reported");
        assert_eq!(judged["judged"][0]["judgement"]["fingerprint"], digest(&d));
        assert_eq!(judged["judged"][0]["due"], false);
    }
}
