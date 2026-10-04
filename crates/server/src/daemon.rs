//! # Module contract
//!
//! ```json
//! {"purpose":"Own the live state of one store, a project's own or a machine's copy of a server's, and of the one repository beside it unless it is shared: load them, apply tool calls, persist writes, re-extract changed files and tell listeners what changed.","criteria":[{"id":"reconfigure","statement":"On a daemon that holds code, a configure that changes the languages rebuilds the extractor for them and re-extracts the whole repository before answering."},{"id":"shared","statement":"open_shared opens a store with no repository beside it: it reads and extracts no code whatever path the store records, marks its state as holding none, so lint says without_code, status says shared and the baseline is never pruned, a configure that changes the languages extracts nothing, and what the store arrived with of one checkout's own, its current ticket and its starts, is dropped, in the store too."},{"id":"baseline-pruned","statement":"On a daemon that holds code, after every write, every re-extraction and on opening, the baseline is pruned of findings no longer produced and the smaller baseline persisted; a shared daemon never prunes it, nor does one whose intent is a server's."},{"id":"prune-from-main-line","statement":"prune_to on a shared daemon, given the digest of the intent the findings were judged against, refuses as a conflict when its intent no longer gives that digest, comparing under the lock every write takes; otherwise it drops from the baseline every entry that neither the given findings nor its own lint, which sees what intent alone shows, a store file that did not load among it, produces, adding none, persists the smaller baseline and announces it, and answers how many entries went and how many are left; a daemon that holds code refuses it as a conflict, since it prunes its own baseline."},{"id":"serving-marker","statement":"While serving, the store holds serve.json naming this daemon's process and the port it is really listening on, with the address when a command on this machine does not reach it at 127.0.0.1; it is removed on a clean shutdown."},{"id":"persist-after-write","statement":"A write to a store of the daemon's own is persisted to the store directory before its result is returned, and a failed persist reloads the store from disk so memory never drifts from it."},{"id":"one-file-reextract","statement":"A changed source file is re-extracted alone and the extraction re-joined; unchanged files are not re-read."},{"id":"events","statement":"Every store write and every graph rebuild is announced to listeners with what changed."},{"id":"source-reader","statement":"A daemon that holds code gives the source tool a reader over its repository that reads only a relative path inside it, refusing an absolute path, a drive or UNC prefix, a .. component, or a path that resolves outside the repository through a link; it reads no more than the limit it is given, reporting a larger file as too large before reading it, drops a byte-order mark, replaces invalid UTF-8, and never writes."},{"id":"refused-reads-nothing","statement":"A daemon whose intent is a server's, once that server has refused its token, answers status and refuses every other tool as store_unavailable, saying to sign in again, until the server takes its token; it asks the server again on each call meanwhile, status among them, so a person who has signed in again is answered with no restart."},{"id":"as-the-caller","statement":"On a project that names a server, call_as sends a write to the server with the token its caller gave, in place of the daemon's own; with none, and on any other daemon, it is call."}]}
//! ```

use std::collections::BTreeMap;
use std::path::{Component, Path, PathBuf};
use std::sync::{Arc, Mutex, RwLock};

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
    /// The repository the store describes; empty for a shared daemon, which has none beside it.
    pub repo: PathBuf,
    /// Serving a store for a team: no code is read, extracted or watched.
    pub shared: bool,
    /// The team's server this daemon's store is a copy of, on a project that names one.
    pub server: Option<Arc<crate::remote::Server>>,
    /// Rebuilt when `configure` changes the languages.
    pub extractor: RwLock<Extractor>,
    pub(crate) files: Mutex<BTreeMap<String, FileExtraction>>,
    pub(crate) state: Mutex<State>,
    pub(crate) events: broadcast::Sender<Event>,
}

#[derive(Debug, thiserror::Error)]
pub enum OpenError {
    #[error(transparent)]
    Store(#[from] StoreIoError),
    #[error("repository {0} does not exist")]
    NoRepository(PathBuf),
    #[error(transparent)]
    Extract(#[from] grimoire_extract::ExtractError),
    #[error(transparent)]
    Server(#[from] crate::remote::RemoteError),
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
    store_io::refuse_inside(store_dir, &repo)?;
    load_store(store, malformed)
}

/// The same for a store held only in memory, as a server's intent fetched
/// for the main line is: its repository extracted once, nothing written.
pub fn load_store(store: grimoire_core::Store, malformed: Vec<grimoire_core::Malformed>) -> Result<Loaded, OpenError> {
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
    state.set_source(Arc::new(RepoSource { root: repo.clone() }));
    Ok(Loaded { state, files, extractor, repo, extract_errors })
}

/// Reads the repository's files for the `source` tool, and nothing else: the
/// core does no I/O, so it is handed this. A path that is absolute, climbs
/// with `..`, or resolves outside the repository through a link is refused.
#[derive(Debug)]
pub struct RepoSource {
    pub root: PathBuf,
}

impl tools::SourceReader for RepoSource {
    fn read(&self, path: &str, limit: usize) -> Result<String, tools::SourceError> {
        use std::io::Read;
        use tools::SourceError::{TooLarge, Unreadable};
        let rel = Path::new(path);
        // Only plain names: no root, drive or UNC prefix, `.` or `..`.
        if rel.is_absolute() || !rel.components().all(|c| matches!(c, Component::Normal(_))) {
            return Err(Unreadable(format!("`{path}` is not a path inside the repository")));
        }
        let root = self.root.canonicalize().map_err(|e| Unreadable(e.to_string()))?;
        let full = root.join(rel).canonicalize().map_err(|e| Unreadable(e.to_string()))?;
        if !full.starts_with(&root) {
            return Err(Unreadable(format!("`{path}` resolves outside the repository")));
        }
        // Size first, then never more than the limit even if it grows meanwhile.
        let size = std::fs::metadata(&full).map_err(|e| Unreadable(e.to_string()))?.len();
        if size > limit as u64 {
            return Err(TooLarge(size));
        }
        let mut bytes = Vec::new();
        std::fs::File::open(&full).and_then(|f| f.take(limit as u64 + 1).read_to_end(&mut bytes)).map_err(|e| Unreadable(e.to_string()))?;
        if bytes.len() > limit {
            return Err(TooLarge(bytes.len() as u64));
        }
        let text = String::from_utf8_lossy(&bytes);
        Ok(text.strip_prefix('\u{feff}').unwrap_or(&text).to_string())
    }
}

/// What this binary is, for `status`: the workspace version and the
/// languages the extractor has adapters for. Told to core here because the
/// core crate does no I/O and this is the one path every command loads through.
pub fn build() -> Build {
    Build { version: env!("CARGO_PKG_VERSION").to_string(), supported_languages: SUPPORTED_LANGUAGES.iter().map(|l| l.to_string()).collect() }
}

/// What a write changed, as references, for the event it announces.
pub(crate) fn describe(changed: &grimoire_core::ops::Changed) -> Vec<String> {
    use grimoire_core::ops::Changed;
    match changed {
        Changed::Page(s) => vec![format!("page:{s}")],
        Changed::Requirement(s) => vec![format!("req:{s}")],
        Changed::Ticket(s) | Changed::Started(s) | Changed::Closed(s) | Changed::Proposed(s) => vec![format!("ticket:{s}")],
        Changed::Current => vec!["current".to_string()],
        Changed::Deleted(r) => vec![r.to_string()],
        Changed::Baseline => vec!["baseline".to_string()],
        Changed::Config => vec!["config".to_string()],
        Changed::Many(cs) => cs.iter().flat_map(describe).collect(),
    }
}

impl Daemon {
    pub fn open(store_dir: &Path) -> Result<(Daemon, Vec<grimoire_extract::ExtractError>), OpenError> {
        let loaded = load(store_dir)?;
        let (events, _) = broadcast::channel(256);
        let daemon = Daemon {
            store_dir: store_dir.to_path_buf(),
            repo: loaded.repo,
            shared: false,
            server: None,
            extractor: RwLock::new(loaded.extractor),
            files: Mutex::new(loaded.files),
            state: Mutex::new(loaded.state),
            events,
        };
        daemon.prune(&mut daemon.state.lock().expect("state lock"));
        Ok((daemon, loaded.extract_errors))
    }

    /// Open a store for a team, with no repository beside it: whatever path
    /// the store records is not used, nothing is extracted or watched, and
    /// the state says it holds no code, so nothing only code can show is said.
    pub fn open_shared(store_dir: &Path) -> Result<Daemon, OpenError> {
        let (store, malformed) = store_io::load(store_dir)?;
        let arrived_with_current = store.current.is_some();
        let arrived_with_starts: Vec<String> = store.starts.keys().cloned().collect();
        let none: Vec<FileExtraction> = Vec::new();
        let mut state = State::new(store, Extraction::from_files(&none), malformed);
        state.set_build(build());
        state.set_without_code(true);
        // The current ticket and the starts are each checkout's own: those the store arrived with were its machine's, and go.
        if arrived_with_current {
            store_io::persist(store_dir, state.store(), &grimoire_core::ops::Changed::Current)?;
        }
        for slug in arrived_with_starts {
            // The ticket's record is written as it is, and its start removed beside it.
            store_io::persist(store_dir, state.store(), &grimoire_core::ops::Changed::Closed(slug))?;
        }
        let (events, _) = broadcast::channel(256);
        Ok(Daemon {
            store_dir: store_dir.to_path_buf(),
            repo: PathBuf::new(),
            shared: true,
            server: None,
            // With no language it extracts nothing; a shared daemon never asks it to.
            extractor: RwLock::new(Extractor::for_languages(store_dir, &Vec::<String>::new())),
            files: Mutex::new(BTreeMap::new()),
            state: Mutex::new(state),
            events,
        })
    }

    /// Say in the store that this process serves it on `port`, so a command
    /// run beside it sends its writes here.
    pub fn mark_serving(&self, port: u16) -> Result<(), StoreIoError> {
        store_io::write_serving(&self.store_dir, port)
    }

    /// The same, for a daemon listening at `addr`, so a command finds it where it listens.
    pub fn mark_serving_at(&self, addr: std::net::SocketAddr) -> Result<(), StoreIoError> {
        store_io::write_serving_at(&self.store_dir, addr)
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
        // same lock every write takes, so nothing lands between this comparison and the prune.
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

    pub fn subscribe(&self) -> broadcast::Receiver<Event> {
        self.events.subscribe()
    }

    /// The store's whole intent, for a machine that keeps a copy of it.
    pub fn exported(&self) -> grimoire_core::export::Exported {
        grimoire_core::export::exported(self.state.lock().expect("state lock").store())
    }

    /// The project's site as the truth now has it: the pages its setting names and what is beneath them, the one `show` names in full, or the first.
    pub fn site(&self, show: Option<&str>) -> grimoire_core::query::Site {
        let mut state = self.state.lock().expect("state lock");
        let config = state.store().config.clone();
        grimoire_core::query::site(state.graph(), &config, show)
    }

    /// Run one tool, persisting any write.
    pub fn call(&self, name: &str, input: Value) -> Result<Value, ToolError> {
        self.call_as(name, input, None)
    }

    /// Run one tool for a caller that gave a token of its own: on a project that names a
    /// server, its write is sent to the server signed in as that caller, not as the daemon.
    pub fn call_as(&self, name: &str, input: Value, token: Option<String>) -> Result<Value, ToolError> {
        // On a project that names a server, a write goes to the server.
        if let Some(server) = self.server.as_ref().filter(|_| tools::is_write(name)) {
            return match token {
                Some(token) => self.call_through(&server.as_token(token), name, input),
                None => self.call_through(server, name, input),
            };
        }
        let today = store_io::today();
        // A token the server refused may have been replaced since, by the person signing in again: asked again first.
        if self.state.lock().expect("state lock").server().is_some_and(|s| s.refused) {
            let _ = self.sync_now();
        }
        let mut state = self.state.lock().expect("state lock");
        // A token the server refuses is not a server that is away: its copy is not read on in silence.
        if let Some(refused) = state.server().filter(|s| s.refused && name != "status") {
            return Err(ToolError::unavailable(format!("the server at {} refused this machine's token, so its intent is not read here: sign in with `grimoire login`", refused.address)));
        }
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
            let what = describe(changed);
            let _ = self.events.send(Event::Store { tool: name.to_string(), changed: what });
            // A shared daemon holds no code, so a change of languages gives it nothing to extract.
            if !self.shared && state.store().config.languages != languages {
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
    pub(crate) fn reextract_all(&self, languages: &[String]) {
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
        repo: PathBuf,
        store: PathBuf,
        /// Held, not read: dropping it removes the folder.
        _tmp: tempfile::TempDir,
    }

    impl Harness {
        fn new() -> Harness {
            let tmp = tempfile::Builder::new().prefix("grimoire-daemon-").tempdir().unwrap();
            let dir = tmp.path().to_path_buf();
            let repo = dir.join("repo");
            std::fs::create_dir_all(repo.join("src")).unwrap();
            std::fs::write(repo.join("Cargo.toml"), "[package]\nname = \"h\"\nversion = \"0.1.0\"\n").unwrap();
            std::fs::write(repo.join("src").join("lib.rs"), "pub mod a;\npub mod b;\n").unwrap();
            std::fs::write(repo.join("src").join("a.rs"), CONTRACTED).unwrap();
            std::fs::write(repo.join("src").join("b.rs"), "pub fn b() {}\n").unwrap();
            let store = dir.join("store");
            let config = StoreConfig { site: vec![], version: StoreConfig::CURRENT_VERSION, repository: repo.to_string_lossy().replace('\\', "/"), roots: vec!["overview".into()].into(), languages: vec!["rust".into()], scope: Scope::default(), order: Default::default() };
            // An active, current ticket, so a write that changes intent has a proposal to land in.
            let mut s = Store::new(config);
            s.tickets.insert("work".into(), grimoire_core::model::Ticket { slug: "work".into(), title: "Work".into(), kind: grimoire_core::model::TicketKind::Change, priority: grimoire_core::model::Priority::Later, status: grimoire_core::model::Status::Active, touches: vec![], relies_on: vec![], body: "b".into(), decision: None });
            s.current = Some("work".into());
            store_io::write_all(&store, &s, false).unwrap();
            Harness { repo, store, _tmp: tmp }
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
        assert!(h.store.join("proposals").join("work.json").is_file(), "the proposal it landed in is persisted before the answer came back");
        d.call("close_ticket", json!({ "ref": "ticket:work", "decision": { "text": "Done." } })).unwrap();
        assert!(h.store.join("pages").join("goal.json").is_file(), "applied to the truth and persisted before the close answered");
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
        // Make the proposals directory unwritable by putting a file where it should be.
        let _ = std::fs::remove_dir_all(h.store.join("proposals"));
        std::fs::write(h.store.join("proposals"), "not a directory").unwrap();
        let e = d.call("put_page", page("goal")).unwrap_err();
        assert_eq!(serde_json::to_value(&e).unwrap()["error"], "store_unavailable");
        let got = d.call("get", json!({ "ref": "page:goal", "ticket": "work" })).unwrap_err();
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
            // What changed is the ticket's proposal, not the truth's page.
            Event::Store { tool, changed } => assert_eq!((tool.as_str(), changed), ("put_page", vec!["ticket:work".to_string()])),
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
    /// {"test": "nothing-derived-on-disk", "checks": ["req:core-one-graph"]}
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
        assert_eq!(entries, vec!["baseline.json", "current.json", "grimoire.json", "pages", "proposals", "requirements", "serve.json", "tickets"], "records, the baseline, the current ticket, its proposal and the marker; no index or cache");
        for dir in ["pages", "requirements", "tickets", "proposals"] {
            for e in std::fs::read_dir(h.store.join(dir)).unwrap().flatten() {
                assert!(e.file_name().to_string_lossy().ends_with(".json"), "{dir} holds only records");
            }
        }
    }

    /// # Test contract
    ///
    /// ```json
    /// {"test": "serving-writes-nothing-into-the-repository", "checks": ["crates/server/src#no-repository-writes"]}
    /// ```
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

    /// # Test contract
    ///
    /// ```json
    /// {"test": "the-source-reader-stays-inside-the-repository", "checks": ["crates/server/src/daemon#source-reader"]}
    /// ```
    #[test]
    fn the_source_reader_stays_inside_the_repository() {
        use grimoire_core::tools::SourceReader;
        let tmp = tempfile::Builder::new().prefix("grimoire-source-").tempdir().unwrap();
        let dir = tmp.path().to_path_buf();
        let repo = dir.join("repo");
        std::fs::create_dir_all(repo.join("src")).unwrap();
        std::fs::write(repo.join("src").join("lib.rs"), "\u{feff}pub fn a() {}\n").unwrap();
        std::fs::write(dir.join("secret.txt"), "outside").unwrap();
        let reader = RepoSource { root: repo.clone() };
        let big = 1 << 20;
        assert_eq!(reader.read("src/lib.rs", big).unwrap(), "pub fn a() {}\n", "the text, without its byte-order mark");
        let outside = dir.join("secret.txt");
        for escape in ["../secret.txt", "src/../../secret.txt", outside.to_str().unwrap(), "./src/lib.rs", "C:/Windows/win.ini", "C:secret.txt", r"\\server\share\secret.txt", "/etc/passwd"] {
            assert!(reader.read(escape, big).is_err(), "{escape} is refused");
        }
        assert!(reader.read("src/missing.rs", big).is_err());
        // A link inside the repository to a file outside it is refused, where links can be made.
        #[cfg(unix)]
        let linked = std::os::unix::fs::symlink(&outside, repo.join("src").join("link.rs")).is_ok();
        #[cfg(windows)]
        let linked = std::os::windows::fs::symlink_file(&outside, repo.join("src").join("link.rs")).is_ok();
        if linked {
            assert!(reader.read("src/link.rs", big).is_err(), "a link out of the repository is refused");
            let _ = std::fs::remove_file(repo.join("src").join("link.rs"));
        }
        // Past the limit: too large, and not read.
        assert_eq!(reader.read("src/lib.rs", 4), Err(grimoire_core::tools::SourceError::TooLarge(17)));
        let before = std::fs::read_dir(repo.join("src")).unwrap().count();
        let _ = reader.read("src/lib.rs", big);
        assert_eq!(std::fs::read_dir(repo.join("src")).unwrap().count(), before, "reading writes nothing");
    }

    /// # Test contract
    ///
    /// ```json
    /// {"test": "a-shared-daemon-opens-a-store-with-no-repository", "checks": ["crates/server/src/daemon#shared"]}
    /// ```
    #[test]
    fn a_shared_daemon_opens_a_store_with_no_repository() {
        let h = Harness::new();
        // The store as it arrives on a host: it records a checkout that is not there, and carries the baseline its machine adopted.
        let local = Daemon::open(&h.store).unwrap().0;
        local.call("put_requirement", json!({ "slug": "held-by-code", "title": "Held", "statement": "It SHALL hold.", "scenarios": [{"title": "t", "given": "g", "when": "w", "then": "t"}], "covered_by": ["criterion:src/a#says"] })).unwrap();
        local.call("close_ticket", json!({ "ref": "ticket:work", "decision": { "text": "Done." } })).unwrap();
        local.call("adopt", json!({ "reason": "inherited" })).unwrap();
        drop(local);
        let (mut store, _) = store_io::load(&h.store).unwrap();
        let adopted = store.baseline.clone().unwrap();
        assert!(!adopted.findings.is_empty(), "something only its machine could see was adopted");
        store.config.repository = "C:/gone/elsewhere".into();
        store_io::persist(&h.store, &store, &grimoire_core::ops::Changed::Config).unwrap();
        assert!(matches!(Daemon::open(&h.store), Err(OpenError::NoRepository(_))), "a daemon that reads code needs the checkout");
        let shared = Daemon::open_shared(&h.store).unwrap();
        // It says what it is, and nothing only code can show.
        assert_eq!(shared.call("status", json!({})).unwrap()["shared"], true);
        let lint = shared.call("lint", json!({})).unwrap();
        assert_eq!(lint["without_code"], true);
        assert!(!lint["findings"].as_array().unwrap().iter().any(|f| f["edge"]["to"].as_str().is_some_and(|t| t.starts_with("criterion:"))), "{lint}");
        // A write is taken and persisted, and the baseline is as it arrived.
        shared.call("file_ticket", json!({ "title": "From the team", "kind": "gap", "priority": "later", "body": "b" })).unwrap();
        assert_eq!(store_io::load(&h.store).unwrap().0.baseline, Some(adopted));
        assert_eq!(shared.call("adopt", json!({ "reason": "again" })).unwrap_err().code(), "invalid");
        // A change of languages extracts nothing and reads no repository.
        shared.call("configure", json!({ "languages": ["rust", "typescript"] })).unwrap();
        assert_eq!(shared.call("status", json!({})).unwrap()["nodes"]["module"], 0);
    }

    /// # Test contract
    ///
    /// ```json
    /// {"test": "a-shared-daemon-prunes-to-what-the-main-line-saw", "checks": ["crates/server/src/daemon#prune-from-main-line", "crates/server/src/remote#intent-digest", "crates/core/src/tools/state#prune-to"]}
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
        seen.push(grimoire_core::model::FindingKey { rule: grimoire_core::lint::Rule::Unspecified, check: "no-scenario".into(), r#ref: grimoire_core::model::Reference::req("never-held"), edge: None });
        // Judged against intent that has since moved: refused, and nothing goes.
        let read = crate::remote::intent_digest(&shared.exported());
        shared.call("file_ticket", json!({ "title": "Meanwhile", "kind": "gap", "priority": "later", "body": "b" })).unwrap();
        let _ = events.try_recv();
        assert_eq!(shared.prune_to(seen.clone(), &read).unwrap_err().code(), "conflict");
        assert_eq!(store_io::load(&h.store).unwrap().0.baseline.unwrap(), adopted);
        // Judged against the intent as it is: taken.
        let now = crate::remote::intent_digest(&shared.exported());
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
        let shared = { drop(shared); Daemon::open_shared(&h.store).unwrap() };
        let intent = crate::remote::intent_digest(&shared.exported());
        shared.prune_to(Vec::new(), &intent).unwrap();
        let own: std::collections::HashSet<_> = lint::lint(shared.state.lock().unwrap().graph(), &lint::Rule::ALL).iter().map(lint::Finding::key).collect();
        let left = store_io::load(&h.store).unwrap().0.baseline.unwrap().findings;
        assert!(left.contains(&broken) && left.iter().all(|k| own.contains(k)), "told of nothing, all goes but what it sees itself: {left:?}");
    }
}
