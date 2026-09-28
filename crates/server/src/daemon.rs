//! # Module contract
//!
//! ```json
//! {"purpose":"Own the live state of one store and one repository: load both, apply tool calls, persist writes, re-extract changed files and tell listeners what changed.","criteria":[{"id":"reconfigure","statement":"A configure that changes the languages rebuilds the extractor for them and re-extracts the whole repository before answering."},{"id":"baseline-pruned","statement":"After every write, every re-extraction and on opening, the baseline is pruned of findings no longer produced and the smaller baseline persisted."},{"id":"serving-marker","statement":"While serving, the store holds serve.json naming this daemon's port and process; it is removed on a clean shutdown."},{"id":"persist-after-write","statement":"A write is persisted to the store directory before its result is returned, and a failed persist reloads the store from disk so memory never drifts from it."},{"id":"one-file-reextract","statement":"A changed source file is re-extracted alone and the extraction re-joined; unchanged files are not re-read."},{"id":"events","statement":"Every store write and every graph rebuild is announced to listeners with what changed."}]}
//! ```

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::{Mutex, RwLock};

use grimoire_core::{lint, tools};
use grimoire_core::{Build, Extraction, FileExtraction, State, ToolError};
use grimoire_extract::{Extractor, SUPPORTED_LANGUAGES};
use serde::Serialize;
use serde_json::Value;
use tokio::sync::broadcast;

use crate::store_io::{self, StoreIoError};

#[derive(Debug, Clone, Serialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Event {
    /// The store changed through a tool.
    Store { tool: String, changed: Vec<String> },
    /// The repository changed and the graph was rebuilt.
    Graph { changed: Vec<String> },
}

pub struct Daemon {
    pub store_dir: PathBuf,
    pub repo: PathBuf,
    /// Rebuilt when `configure` changes the languages.
    pub extractor: RwLock<Extractor>,
    files: Mutex<BTreeMap<String, FileExtraction>>,
    state: Mutex<State>,
    events: broadcast::Sender<Event>,
}

#[derive(Debug, thiserror::Error)]
pub enum OpenError {
    #[error(transparent)]
    Store(#[from] StoreIoError),
    #[error("repository {0} does not exist")]
    NoRepository(PathBuf),
    #[error(transparent)]
    Extract(#[from] grimoire_extract::ExtractError),
}

/// What loading produced, for the CLI as well as the daemon.
pub struct Loaded {
    pub state: State,
    pub files: BTreeMap<String, FileExtraction>,
    pub extractor: Extractor,
    pub repo: PathBuf,
    pub extract_errors: Vec<grimoire_extract::ExtractError>,
}

/// Load a store and extract its repository once.
pub fn load(store_dir: &Path) -> Result<Loaded, OpenError> {
    let (store, malformed) = store_io::load(store_dir)?;
    let repo = PathBuf::from(&store.config.repository);
    if !repo.is_dir() {
        return Err(OpenError::NoRepository(repo));
    }
    let extractor = Extractor::for_languages(&repo, &store.config.languages);
    let (extracted, extract_errors) = extractor.extract_all()?;
    let files: BTreeMap<String, FileExtraction> = extracted.into_iter().map(|f| (f.path.clone(), f)).collect();
    let extraction = Extraction::from_files(files.values());
    let mut state = State::new(store, extraction, malformed);
    state.set_build(build());
    Ok(Loaded { state, files, extractor, repo, extract_errors })
}

/// What this binary is, for `status`: the workspace version and the
/// languages the extractor has adapters for. Told to core here because the
/// core crate does no I/O and this is the one path every command loads through.
pub fn build() -> Build {
    Build { version: env!("CARGO_PKG_VERSION").to_string(), supported_languages: SUPPORTED_LANGUAGES.iter().map(|l| l.to_string()).collect() }
}

impl Daemon {
    pub fn open(store_dir: &Path) -> Result<(Daemon, Vec<grimoire_extract::ExtractError>), OpenError> {
        let loaded = load(store_dir)?;
        let (events, _) = broadcast::channel(256);
        let daemon = Daemon {
            store_dir: store_dir.to_path_buf(),
            repo: loaded.repo,
            extractor: RwLock::new(loaded.extractor),
            files: Mutex::new(loaded.files),
            state: Mutex::new(loaded.state),
            events,
        };
        daemon.prune(&mut daemon.state.lock().expect("state lock"));
        Ok((daemon, loaded.extract_errors))
    }

    /// Say in the store that this process serves it on `port`, so a command
    /// run beside it sends its writes here.
    pub fn mark_serving(&self, port: u16) -> Result<(), StoreIoError> {
        store_io::write_serving(&self.store_dir, port)
    }

    pub fn unmark_serving(&self) {
        store_io::remove_serving(&self.store_dir);
    }

    /// Drop fixed findings from the baseline and persist it when it shrank.
    fn prune(&self, state: &mut State) {
        if let Some(changed) = state.prune_baseline() {
            match store_io::persist(&self.store_dir, state.store(), &changed) {
                Ok(()) => {
                    let _ = self.events.send(Event::Store { tool: "prune".to_string(), changed: vec!["baseline".to_string()] });
                }
                Err(e) => tracing::error!("the pruned baseline could not be written: {e}"),
            }
        }
    }

    pub fn subscribe(&self) -> broadcast::Receiver<Event> {
        self.events.subscribe()
    }

    /// Run one tool, persisting any write.
    pub fn call(&self, name: &str, input: Value) -> Result<Value, ToolError> {
        let today = store_io::today();
        let mut state = self.state.lock().expect("state lock");
        let languages = state.store().config.languages.clone();
        let outcome = tools::call(&mut state, name, input, &today)?;
        if let Some(changed) = &outcome.changed {
            if let Err(e) = store_io::persist(&self.store_dir, state.store(), changed) {
                tracing::error!("persist failed, reloading store from disk: {e}");
                match store_io::load(&self.store_dir) {
                    Ok((store, malformed)) => state.set_store(store, malformed),
                    Err(e2) => tracing::error!("reload failed too: {e2}"),
                }
                return Err(ToolError::unavailable(format!("the store could not be written: {e}")));
            }
            let what = match changed {
                grimoire_core::ops::Changed::Page(s) => format!("page:{s}"),
                grimoire_core::ops::Changed::Requirement(s) => format!("req:{s}"),
                grimoire_core::ops::Changed::Ticket(s) => format!("ticket:{s}"),
                grimoire_core::ops::Changed::Deleted(r) => r.to_string(),
                grimoire_core::ops::Changed::Baseline => "baseline".to_string(),
                grimoire_core::ops::Changed::Config => "config".to_string(),
                grimoire_core::ops::Changed::Started(s) | grimoire_core::ops::Changed::Closed(s) => format!("ticket:{s}"),
            };
            let _ = self.events.send(Event::Store { tool: name.to_string(), changed: vec![what] });
            if state.store().config.languages != languages {
                let now = state.store().config.languages.clone();
                drop(state);
                self.reextract_all(&now);
                // configure answered with the count of new findings under the
                // old extraction; answer with the one the new languages give.
                let mut state = self.state.lock().expect("state lock");
                self.prune(&mut state);
                let mut output = outcome.output;
                output["new"] = Value::from(lint::new_count(&lint::lint(state.graph(), &lint::Rule::ALL)));
                return Ok(output);
            }
            self.prune(&mut state);
        }
        Ok(outcome.output)
    }

    /// Replace the extractor with one for `languages` and extract the whole
    /// repository again, as on opening.
    fn reextract_all(&self, languages: &[String]) {
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
                return;
            }
        };
        let mut files = self.files.lock().expect("files lock");
        *files = extracted.into_iter().map(|f| (f.path.clone(), f)).collect();
        self.state.lock().expect("state lock").set_extraction(Extraction::from_files(files.values()));
        *self.extractor.write().expect("extractor lock") = extractor;
        tracing::info!("languages are now {languages:?}; re-extracted {} file(s)", files.len());
        let _ = self.events.send(Event::Graph { changed: vec!["*".to_string()] });
    }

    /// Re-extract the given repository-relative paths (deleted ones are
    /// removed) and re-join the graph.
    pub fn reextract(&self, paths: &[String]) {
        let mut changed = Vec::new();
        {
            let mut files = self.files.lock().expect("files lock");
            let extractor = self.extractor.read().expect("extractor lock");
            for rel in paths {
                if !extractor.handles(rel) {
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
            self.prune(&mut state);
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
    use super::*;
    use grimoire_core::model::{Scope, StoreConfig};
    use grimoire_core::Store;
    use serde_json::json;

    /// A repository with one contracted module and one without a contract,
    /// and an empty store pointed at it.
    struct Harness {
        dir: PathBuf,
        repo: PathBuf,
        store: PathBuf,
    }

    impl Harness {
        fn new() -> Harness {
            let nanos = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_nanos()).unwrap_or(0);
            let dir = std::env::temp_dir().join(format!("grimoire-daemon-{}-{nanos}", std::process::id()));
            let repo = dir.join("repo");
            std::fs::create_dir_all(repo.join("src")).unwrap();
            std::fs::write(repo.join("Cargo.toml"), "[package]\nname = \"h\"\nversion = \"0.1.0\"\n").unwrap();
            std::fs::write(repo.join("src").join("lib.rs"), "pub mod a;\npub mod b;\n").unwrap();
            std::fs::write(repo.join("src").join("a.rs"), CONTRACTED).unwrap();
            std::fs::write(repo.join("src").join("b.rs"), "pub fn b() {}\n").unwrap();
            let store = dir.join("store");
            let config = StoreConfig { version: StoreConfig::CURRENT_VERSION, repository: repo.to_string_lossy().replace('\\', "/"), roots: vec!["overview".into()], languages: vec!["rust".into()], scope: Scope::default() };
            store_io::write_all(&store, &Store::new(config), false).unwrap();
            Harness { dir, repo, store }
        }

        fn open(&self) -> Daemon {
            Daemon::open(&self.store).unwrap().0
        }

        /// Every path under the repository, to see that nothing was written there.
        fn repo_files(&self) -> Vec<String> {
            let mut out = Vec::new();
            let mut stack = vec![self.repo.clone()];
            while let Some(d) = stack.pop() {
                for e in std::fs::read_dir(&d).unwrap().flatten() {
                    if e.path().is_dir() {
                        stack.push(e.path());
                    } else {
                        out.push(e.path().to_string_lossy().to_string());
                    }
                }
            }
            out.sort();
            out
        }
    }

    impl Drop for Harness {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.dir);
        }
    }

    const CONTRACTED: &str = "//! # Module contract\n//!\n//! ```json\n//! {\"purpose\":\"Say a.\",\"criteria\":[{\"id\":\"says\",\"statement\":\"It says a.\"}]}\n//! ```\npub fn a() {}\n";

    fn page(slug: &str) -> Value {
        json!({ "slug": slug, "title": "T", "body": "b", "links": [] })
    }

    /// # Test contract
    ///
    /// ```json
    /// {"test": "a-write-is-on-disk-before-its-answer", "checks": ["crates/server/src/daemon#persist-after-write"]}
    /// ```
    #[test]
    fn a_write_is_on_disk_before_its_answer() {
        let h = Harness::new();
        let d = h.open();
        d.call("put_page", page("goal")).unwrap();
        assert!(h.store.join("pages").join("goal.json").is_file(), "persisted before the answer came back");
    }

    /// # Test contract
    ///
    /// ```json
    /// {"test": "a-failed-persist-leaves-memory-as-the-disk-is", "checks": ["crates/server/src/daemon#persist-after-write"]}
    /// ```
    #[test]
    fn a_failed_persist_leaves_memory_as_the_disk_is() {
        let h = Harness::new();
        let d = h.open();
        // Make the pages directory unwritable by putting a file where it should be.
        std::fs::remove_dir_all(h.store.join("pages")).unwrap();
        std::fs::write(h.store.join("pages"), "not a directory").unwrap();
        let e = d.call("put_page", page("goal")).unwrap_err();
        assert_eq!(serde_json::to_value(&e).unwrap()["error"], "store_unavailable");
        let got = d.call("get", json!({ "ref": "page:goal" })).unwrap_err();
        assert_eq!(serde_json::to_value(&got).unwrap()["error"], "not_found", "memory was reloaded from disk");
    }

    /// # Test contract
    ///
    /// ```json
    /// {"test": "writes-and-rebuilds-are-announced", "checks": ["crates/server/src/daemon#events"]}
    /// ```
    #[test]
    fn writes_and_rebuilds_are_announced() {
        let h = Harness::new();
        let d = h.open();
        let mut rx = d.subscribe();
        d.call("put_page", page("goal")).unwrap();
        match rx.try_recv().unwrap() {
            Event::Store { tool, changed } => assert_eq!((tool.as_str(), changed), ("put_page", vec!["page:goal".to_string()])),
            other => panic!("expected a store event, got {other:?}"),
        }
        std::fs::write(h.repo.join("src").join("b.rs"), "pub fn b2() {}\n").unwrap();
        d.reextract(&["src/b.rs".to_string()]);
        assert!(matches!(rx.try_recv().unwrap(), Event::Graph { changed } if changed == vec!["src/b.rs".to_string()]));
    }

    /// # Test contract
    ///
    /// ```json
    /// {"test": "one-changed-file-is-reextracted-alone", "checks": ["crates/server/src/daemon#one-file-reextract"]}
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
    /// {"test": "a-fix-shrinks-the-stored-baseline", "checks": ["crates/server/src/daemon#baseline-pruned"]}
    /// ```
    #[test]
    fn a_fix_shrinks_the_stored_baseline() {
        let h = Harness::new();
        let d = h.open();
        d.call("adopt", json!({ "reason": "adopted" })).unwrap();
        let stored = || store_io::load(&h.store).unwrap().0.baseline.map(|b| b.findings.len()).unwrap_or(0);
        let before = stored();
        std::fs::write(h.repo.join("src").join("b.rs"), CONTRACTED.replace("Say a.", "Say b.")).unwrap();
        d.reextract(&["src/b.rs".to_string()]);
        assert!(stored() < before, "the contract fixed b's no-contract finding, and the file says so");
        // Opening prunes too: fix another on disk, then open fresh.
        drop(d);
        std::fs::write(h.repo.join("src").join("lib.rs"), CONTRACTED.replace("Say a.", "Say the crate.")).unwrap();
        let after_b = stored();
        let _d = h.open();
        assert!(stored() < after_b);
    }

    /// # Test contract
    ///
    /// ```json
    /// {"test": "configure-with-new-languages-reextracts", "checks": ["crates/server/src/daemon#reconfigure"]}
    /// ```
    #[test]
    fn configure_with_new_languages_reextracts() {
        let h = Harness::new();
        std::fs::create_dir_all(h.repo.join("web").join("src")).unwrap();
        std::fs::write(h.repo.join("web").join("package.json"), "{\"name\":\"w\"}").unwrap();
        std::fs::write(h.repo.join("web").join("src").join("x.ts"), "export const x = 1;\n").unwrap();
        let d = h.open();
        assert!(d.call("get", json!({ "ref": "module:web/src/x" })).is_err());
        let out = d.call("configure", json!({ "languages": ["rust", "typescript"] })).unwrap();
        assert_eq!(out["config"]["languages"], json!(["rust", "typescript"]));
        assert!(d.call("get", json!({ "ref": "module:web/src/x" })).is_ok(), "re-extracted before answering");
    }

    /// # Test contract
    ///
    /// ```json
    /// {"test": "the-serving-marker-comes-and-goes", "checks": ["crates/server/src/daemon#serving-marker"]}
    /// ```
    #[test]
    fn the_serving_marker_comes_and_goes() {
        let h = Harness::new();
        let d = h.open();
        d.mark_serving(4299).unwrap();
        assert_eq!(store_io::read_serving(&h.store).map(|s| s.port), Some(4299));
        d.unmark_serving();
        assert!(store_io::read_serving(&h.store).is_none());
    }

    /// # Test contract
    ///
    /// ```json
    /// {"test": "serving-writes-nothing-into-the-repository", "checks": ["crates/server/src#no-repository-writes"]}
    /// ```
    /// # Test contract
    ///
    /// ```json
    /// {"test": "nothing-derived-on-disk", "checks": ["req:src-one-graph"]}
    /// ```
    #[test]
    fn nothing_derived_is_kept_on_disk() {
        let h = Harness::new();
        let d = h.open();
        d.call("put_page", page("goal")).unwrap();
        d.call("adopt", json!({ "reason": "adopted" })).unwrap();
        std::fs::write(h.repo.join("src").join("b.rs"), "pub fn b4() {}\n").unwrap();
        d.reextract(&["src/b.rs".to_string()]);
        d.mark_serving(4299).unwrap();
        let mut entries: Vec<String> = std::fs::read_dir(&h.store).unwrap().flatten().map(|e| e.file_name().to_string_lossy().to_string()).collect();
        entries.sort();
        assert_eq!(entries, vec!["baseline.json", "grimoire.json", "pages", "requirements", "serve.json", "tickets"], "records, the baseline and the marker; no index or cache");
        for dir in ["pages", "requirements", "tickets"] {
            for e in std::fs::read_dir(h.store.join(dir)).unwrap().flatten() {
                assert!(e.file_name().to_string_lossy().ends_with(".json"), "{dir} holds only records");
            }
        }
    }

    #[test]
    fn serving_writes_nothing_into_the_repository() {
        let h = Harness::new();
        let before = h.repo_files();
        let d = h.open();
        d.call("put_page", page("goal")).unwrap();
        d.call("adopt", json!({ "reason": "adopted" })).unwrap();
        std::fs::write(h.repo.join("src").join("b.rs"), "pub fn b3() {}\n").unwrap();
        d.reextract(&["src/b.rs".to_string()]);
        d.mark_serving(4299).unwrap();
        assert_eq!(h.repo_files(), before, "no index, cache or marker in the repository");
    }
}

