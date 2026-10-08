//! # Module contract
//!
//! ```json
//! {"purpose":"Own the live state of one store, a project's own or a machine's copy of a server's, and of the one repository beside it unless it is shared: load them, apply tool calls, persist writes, re-extract changed files and tell listeners what changed.","criteria":[{"id":"reconfigure","statement":"On a daemon that holds code, a configure that changes the languages rebuilds the extractor for them and re-extracts the whole repository before answering."},{"id":"shared","statement":"open_shared opens a store with no repository beside it: it reads and extracts no code whatever path the store records, marks its state as holding none, so lint says without_code, status says shared and the baseline is never pruned, a configure that changes the languages extracts nothing, and what the store arrived with of one checkout's own, its current ticket and its starts, is dropped, in the store too."},{"id":"baseline-pruned","statement":"On a daemon that holds code, after every write, every re-extraction and on opening, the baseline is pruned of findings no longer produced and the smaller baseline persisted; a shared daemon never prunes it, nor does one whose intent is a server's, nor one serving another checkout of a store of its own than its main one, whose code is a branch's."},{"id":"prune-from-main-line","statement":"prune_to on a shared daemon, given the digest of the intent the findings were judged against, refuses as a conflict when its intent no longer gives that digest, comparing under the lock every write takes; otherwise it drops from the baseline every entry that neither the given findings nor its own lint, which sees what intent alone shows, a store file that did not load among it, produces, adding none, persists the smaller baseline and announces it, and answers how many entries went and how many are left; a daemon that holds code refuses it as a conflict, since it prunes its own baseline."},{"id":"main-line-recipes","statement":"take_main_line_recipes on a shared daemon validates the recipes the main line sent and replaces the store's record of them whole, persists it and announces it, unless it holds them already, and answers how many it holds; an invalid record is refused as invalid and changes nothing, and a daemon that holds code refuses it as a conflict, since it reads its own; site and site_recipe on a shared daemon read the recipes from that record, none until there is one, and on a daemon that holds code from its code, whatever record its store holds."},{"id":"serving-marker","statement":"While serving, the store holds serve.json naming this daemon's process and the port it is really listening on, with the address when a command on this machine does not reach it at 127.0.0.1, and for a grimoire served in a library the path it is served at, /g/<name>; it is removed on a clean shutdown."},{"id":"persist-after-write","statement":"A write to a store of the daemon's own is persisted to the store directory before its result is returned, and a failed persist reloads the store from disk so memory never drifts from it."},{"id":"one-file-reextract","statement":"A changed source file is re-extracted alone and the extraction re-joined; unchanged files are not re-read; a file the extractor no longer reads, as one in a folder that became a checkout of its own, is removed from the extraction, and a nested checkout's marker appearing or going reads the whole checkout again."},{"id":"catches-up","statement":"Before close_ticket, land_ticket or judge_recipe, on a daemon that holds code and before a write is sent to a team's server, catch_up makes the extraction what reading the whole checkout would make it, without waiting for the watcher: it walks what that reading would read, its ignore rules and nested checkouts heeded, keeps each held source file whose text's digest matches, extracts each that differs or appeared and every recipe file whatever its digest, and removes every other held file, one no longer readable as text among them; then re-joins the graph and announces what changed; with nothing changed it extracts nothing and announces nothing; a checkout that cannot be walked refuses the close, landing or judgement as store_unavailable, judging nothing."},{"id":"recipes-follow-files","statement":"When the paths handed to reextract include a path a held recipe's files name, or a folder holding one or held by one, that recipe's file is read again with them, so a named file appearing, changing or going is seen in what the recipe found without a save to the recipe; recipe_paths gives every path the held recipes name, for the watcher."},{"id":"events","statement":"Every store write and every graph rebuild is announced to listeners with what changed."},{"id":"refused-reads-nothing","statement":"A daemon whose intent is a server's, once that server has refused its token, answers status and refuses every other tool as store_unavailable, saying to sign in again, until the server takes its token; it asks the server again on each call meanwhile, status among them, so a person who has signed in again is answered with no restart."},{"id":"who-calls","statement":"A start or a close a daemon runs itself is run as its caller, with the time now as RFC 3339 text in UTC: on a shared daemon by the name call_for is given, the token's, and on any other by the person signed in to the machine, else the machine's name, as a command run with no daemon runs one through as_this_person; on a project that names a server the server says who called."},{"id":"as-the-caller","statement":"On a project that names a server, call_as sends a write to the server with the token its caller gave, in place of the daemon's own; with none, and on any other daemon, it is call."}]}
//! ```

mod kin;
mod loading;

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, RwLock, Weak};

use grimoire_core::model::CheckedOut;
use grimoire_core::{lint, tools};
use grimoire_core::{Extraction, FileExtraction, State, ToolError};
use grimoire_extract::Extractor;
use serde::Serialize;
use serde_json::Value;
use tokio::sync::broadcast;

use crate::store_io::{self, StoreIoError};
use kin::Kin;
pub use loading::{build, load, load_for, load_store, Loaded, RepoSource};

#[derive(Debug, Clone, Serialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Event {
    /// The store changed through a tool.
    Store { tool: String, changed: Vec<String> },
    /// The repository changed and the graph was rebuilt.
    Graph { changed: Vec<String> },
}

/// The base a daemon last read, by commit and languages, and when git was last asked for it.
pub(crate) type BaseSeen = (Option<(String, Vec<String>)>, Option<std::time::Instant>);

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
    /// The daemons serving the same store for its other checkouts, and the one lock their writes take.
    pub(crate) kin: Arc<Kin>,
    /// The daemons a library serves for the other checkouts of the same grimoire, kin or not:
    /// a team's checkouts each keep a copy of their own, so only the library knows them all.
    pub(crate) fellows: Mutex<Vec<Weak<Daemon>>>,
    /// This grimoire's name in its library and the library, as the library last told it: kept apart from the
    /// state, which a copy holds across a fetch while it syncs, so telling it never waits (req:machine-library).
    pub(crate) library: Mutex<Option<(String, Vec<grimoire_core::answer::LibraryEntry>)>>,
    /// Serving the store's main checkout, the only one that prunes its baseline.
    pub main: bool,
    /// The commit and languages whose code the state holds as this checkout's base, and when that was last
    /// asked, so a `lint` asks git again only now and then and re-reads the code only when either moved.
    pub(crate) base: Mutex<BaseSeen>,
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

/// Who a start or a close `name` is run as on a store of one's own, by a daemon or a command: the person
/// signed in to the machine, else the machine, now; none for any other tool (req:ticket-checked-out).
pub fn as_this_person(name: &str) -> Option<CheckedOut> {
    caller_of(name, None)
}

/// Who a start or a close `name` is run as: `who` when given, else the person signed in to the machine, now.
fn caller_of(name: &str, who: Option<String>) -> Option<CheckedOut> {
    matches!(name, "start_ticket" | "close_ticket").then(|| CheckedOut { by: who.unwrap_or_else(this_person), since: store_io::now() })
}

/// Who a daemon on a store of one's own runs as: the person signed in to the machine, else the machine, by name.
fn this_person() -> String {
    ["USERNAME", "USER", "COMPUTERNAME", "HOSTNAME"]
        .iter()
        .find_map(|k| std::env::var(k).ok().map(|v| v.trim().to_string()).filter(|v| !v.is_empty() && !v.contains('\n')))
        .unwrap_or_else(|| "this machine".to_string())
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
        Changed::Judged(recipe) => vec![format!("recipe:{recipe}")],
        Changed::MainLineRecipes => vec!["recipes".to_string()],
        Changed::CheckedOut => vec!["checked_out".to_string()],
        Changed::Many(cs) => cs.iter().flat_map(describe).collect(),
    }
}

impl Daemon {
    pub fn open(store_dir: &Path) -> Result<(Daemon, Vec<grimoire_extract::ExtractError>), OpenError> {
        Daemon::open_for(store_dir, None)
    }

    /// Open a store as one of its checkouts, `checkout`, sees it, or as its main one.
    pub fn open_for(store_dir: &Path, checkout: Option<&str>) -> Result<(Daemon, Vec<grimoire_extract::ExtractError>), OpenError> {
        let loaded = load_for(store_dir, checkout)?;
        let main = store_io::checkouts::main_of(store_dir).map_or(true, |main| main == loaded.state.store().config.repository);
        let daemon = Daemon::from_loaded(store_dir, loaded.repo, loaded.extractor, loaded.files, loaded.state, Arc::default(), main);
        daemon.prune(&mut daemon.state.lock().expect("state lock"));
        Ok((daemon, loaded.extract_errors))
    }

    fn from_loaded(store_dir: &Path, repo: PathBuf, extractor: Extractor, files: BTreeMap<String, FileExtraction>, state: State, kin: Arc<Kin>, main: bool) -> Daemon {
        let (events, _) = broadcast::channel(256);
        Daemon {
            store_dir: store_dir.to_path_buf(),
            repo,
            shared: false,
            server: None,
            extractor: RwLock::new(extractor),
            files: Mutex::new(files),
            state: Mutex::new(state),
            events,
            kin,
            fellows: Mutex::default(),
            library: Mutex::default(),
            main,
            base: Mutex::new((None, None)),
        }
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
            kin: Arc::default(),
            fellows: Mutex::default(),
            library: Mutex::default(),
            main: true,
            base: Mutex::new((None, None)),
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

    /// The same, for a grimoire served at `path` in the daemon's library on `port`.
    pub fn mark_serving_in(&self, addr: std::net::SocketAddr, path: &str) -> Result<(), StoreIoError> {
        store_io::write_serving_in(&self.store_dir, addr, Some(path))
    }

    pub fn unmark_serving(&self) {
        store_io::remove_serving(&self.store_dir);
    }

    /// Drop fixed findings from the baseline and persist it when it shrank.
    /// Prune the baseline of what this checkout's code no longer produces: whether it changed. Only the
    /// store's main checkout prunes; another checkout's code is a branch whose fixes have not landed.
    fn prune(&self, state: &mut State) -> bool {
        if !self.main {
            return false;
        }
        let Some(changed) = state.prune_baseline() else { return false };
        match store_io::persist(&self.store_dir, state.store(), &changed) {
            Ok(()) => {
                let _ = self.events.send(Event::Store { tool: "prune".to_string(), changed: vec!["baseline".to_string()] });
                true
            }
            Err(e) => {
                tracing::error!("the pruned baseline could not be written: {e}");
                false
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

    /// Run one tool, persisting any write.
    pub fn call(&self, name: &str, input: Value) -> Result<Value, ToolError> {
        self.call_as(name, input, None)
    }

    /// Run one tool for a caller that gave a token of its own: on a project that names a
    /// server, its write is sent to the server signed in as that caller, not as the daemon.
    /// Give the state the code this checkout's branch was cut from, when that commit moved: none on the
    /// main line, outside git, and on a daemon that holds no code.
    fn refresh_base(&self) {
        // Asked of git at most every few seconds: a lint follows every event, and a branch switched is seen soon enough.
        const ASK_EVERY: std::time::Duration = std::time::Duration::from_secs(3);
        let mut held = self.base.lock().expect("base lock");
        if held.1.is_some_and(|at| at.elapsed() < ASK_EVERY) {
            return;
        }
        held.1 = Some(std::time::Instant::now());
        let commit = if self.shared { None } else { crate::base::base_commit(&self.repo) };
        // The base's code depends on the languages read as much as on the commit.
        let languages = self.state.lock().expect("state lock").store().config.languages.clone();
        let wanted = commit.map(|c| (c, languages));
        if held.0 == wanted {
            return;
        }
        let code = wanted.as_ref().and_then(|(c, languages)| match crate::base::base_code(&self.repo, c, languages) {
            Ok(code) => Some(code),
            Err(e) => {
                tracing::warn!("the base {c} could not be read, so findings are judged as before: {e}");
                None
            }
        });
        // A base that could not be read is not remembered as read: the next lint tries it again.
        let read = code.is_some();
        self.state.lock().expect("state lock").set_base(code);
        held.0 = if read { wanted } else { None };
    }

    pub fn call_as(&self, name: &str, input: Value, token: Option<String>) -> Result<Value, ToolError> {
        self.call_for(name, input, token, None)
    }

    /// Run one tool for a caller the trust step named `who`: on a shared daemon a start lists them by that name as
    /// having its ticket checked out (req:ticket-checked-out); `call_as` otherwise.
    pub fn call_for(&self, name: &str, input: Value, token: Option<String>, who: Option<String>) -> Result<Value, ToolError> {
        // A close or a landing judges the code as it is on disk, not as the watcher last reported it,
        // before a write sent to a team's server too, whose answer is judged over this checkout's code.
        // So does a recipe's judgement, bound to the recipe and its files as they are on disk (req:tool-judge-recipe).
        if matches!(name, "close_ticket" | "land_ticket" | "judge_recipe") {
            self.catch_up()?;
        }
        // A close names only this ticket's own work, its code told from the base's (req:tool-untouched).
        if matches!(name, "close_ticket" | "land_ticket") {
            self.refresh_base();
        }
        // On a project that names a server, a write goes to the server.
        if let Some(server) = self.server.as_ref().filter(|_| tools::is_write(name)) {
            return match token {
                Some(token) => self.call_through(&server.as_token(token), name, input),
                None => self.call_through(server, name, input),
            };
        }
        // status names the library as it was last told and each checkout's current ticket as it is now.
        if name == "status" {
            self.tell_library();
        }
        // A lint tells this checkout's own work from what landed since its base: the base read first,
        // outside the state's lock, and again only when it moved (req:check-own-work).
        if name == "lint" {
            self.refresh_base();
        }
        let today = store_io::today();
        // A token the server refused may have been replaced since, by the person signing in again: asked again first.
        if self.state.lock().expect("state lock").server().is_some_and(|s| s.refused) {
            let _ = self.sync_now();
        }
        // The daemons serving one store for its several checkouts take its writes one at a time.
        let _one = tools::is_write(name).then(|| self.kin.write.lock().expect("kin write lock"));
        let mut state = self.state.lock().expect("state lock");
        // A token the server refuses is not a server that is away: its copy is not read on in silence.
        if let Some(refused) = state.server().filter(|s| s.refused && name != "status") {
            return Err(ToolError::unavailable(format!("the server at {} refused this machine's token, so its intent is not read here: sign in with `grimoire login`", refused.address)));
        }
        let languages = state.store().config.languages.clone();
        // Who is calling, and when: the token's name on a shared daemon, the person or machine on a store of one's own.
        let caller = caller_of(name, who.filter(|_| self.shared));
        let outcome = tools::call_by(&mut state, name, input, &today, caller)?;
        if let Some(changed) = &outcome.changed {
            if let Err(e) = store_io::persist(&self.store_dir, state.store(), changed) {
                tracing::error!("persist failed, reloading store from disk: {e}");
                let checkout = state.store().config.repository.clone();
                match store_io::load_for(&self.store_dir, &checkout) {
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
                let _ = self.reextract_all(&now);
                // configure answered with the count of new findings under the
                // old extraction; answer with the one the new languages give.
                let mut state = self.state.lock().expect("state lock");
                self.prune(&mut state);
                let mut output = outcome.output;
                output["new"] = Value::from(lint::new_count(&lint::lint(state.graph(), &lint::Rule::ALL)));
                self.hand_on(state);
                return Ok(output);
            }
            self.prune(&mut state);
            self.hand_on(state);
        }
        Ok(outcome.output)
    }

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
    /// the watcher, before a close or a landing judges the code (req:daemon-catches-up-before-judging).
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
    use super::*;
    use grimoire_core::model::{Scope, StoreConfig};
    use grimoire_core::Store;
    use serde_json::json;

    /// A repository with one contracted module and one without a contract,
    /// and an empty store pointed at it.
    pub(super) struct Harness {
        pub(super) repo: PathBuf,
        pub(super) store: PathBuf,
        /// Held, not read: dropping it removes the folder.
        _tmp: tempfile::TempDir,
    }

    impl Harness {
        pub(super) fn new() -> Harness {
            let tmp = tempfile::Builder::new().prefix("grimoire-daemon-").tempdir().unwrap();
            let dir = tmp.path().to_path_buf();
            let repo = dir.join("repo");
            std::fs::create_dir_all(repo.join("src")).unwrap();
            std::fs::write(repo.join("Cargo.toml"), "[package]\nname = \"h\"\nversion = \"0.1.0\"\n").unwrap();
            std::fs::write(repo.join("src").join("lib.rs"), "pub mod a;\npub mod b;\n").unwrap();
            std::fs::write(repo.join("src").join("a.rs"), CONTRACTED).unwrap();
            std::fs::write(repo.join("src").join("b.rs"), "pub fn b() {}\n").unwrap();
            let store = dir.join("store");
            let config = StoreConfig {
                large_file: None,
                plain_file: None,
                site: vec![],
                version: StoreConfig::CURRENT_VERSION,
                repository: repo.to_string_lossy().replace('\\', "/"),
                roots: vec!["overview".into()].into(),
                languages: vec!["rust".into()],
                scope: Scope::default(),
                order: Default::default(),
                recipe_kinds: Default::default(),
            };
            // An active, current ticket, so a write that changes intent has a proposal to land in.
            let mut s = Store::new(config);
            s.tickets.insert(
                "work".into(),
                grimoire_core::model::Ticket {
                    slug: "work".into(),
                    title: "Work".into(),
                    kind: grimoire_core::model::TicketKind::Change,
                    priority: grimoire_core::model::Priority::Later,
                    status: grimoire_core::model::Status::Active,
                    touches: vec![],
                    relies_on: vec![],
                    body: "b".into(),
                    decision: None,
                },
            );
            s.current = Some("work".into());
            store_io::write_all(&store, &s, false).unwrap();
            Harness { repo, store, _tmp: tmp }
        }

        pub(super) fn open(&self) -> Daemon {
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

    pub(super) const CONTRACTED: &str = "//! # Module contract\n//!\n//! ```json\n//! {\"purpose\":\"Say a.\",\"criteria\":[{\"id\":\"says\",\"statement\":\"It says a.\"}]}\n//! ```\npub fn a() {}\n";

    fn page(slug: &str) -> Value {
        json!({ "slug": slug, "title": "T", "body": "b", "links": [] })
    }

    /// # Test contract
    ///
    /// ```json
    /// {"test": "a-start-is-run-as-its-caller", "checks": ["crates/server/src/daemon#who-calls", "crates/server/src/remote#intent-digest"]}
    /// ```
    #[test]
    fn a_start_is_run_as_its_caller() {
        let h = Harness::new();
        let d = h.open();
        let filed = d.call("file_ticket", json!({ "title": "Why", "kind": "question", "priority": "later", "body": "b" })).unwrap();
        let r = filed["node"]["ref"].as_str().unwrap().to_string();
        // On a store of one's own, by the person signed in to the machine, as RFC 3339 text in UTC.
        let out = d.call_for("start_ticket", json!({ "ref": r }), None, Some("ignored".into())).unwrap();
        let since = out["checked_out"][0]["since"].as_str().unwrap().to_string();
        assert_eq!(out["checked_out"][0]["by"], json!(this_person()));
        assert!(time::OffsetDateTime::parse(&since, &time::format_description::well_known::Rfc3339).is_ok() && since.ends_with('Z'), "{since}");
        assert!(h.store.join(store_io::CHECKED_OUT_FILE).is_file());
        drop(d);
        // On a shared daemon, by the name the trust step gave its caller; and who has a ticket is not intent.
        let shared = Daemon::open_shared(&h.store).unwrap();
        let digest = crate::remote::intent_digest(&shared.exported());
        let out = shared.call_for("start_ticket", json!({ "ref": r }), None, Some("ana".into())).unwrap();
        assert_eq!(out["checked_out"][1]["by"], "ana");
        assert_eq!(out["others"][0]["by"], json!(this_person()), "and she is told who else has it");
        assert_eq!(crate::remote::intent_digest(&shared.exported()), digest, "who has a ticket moves no digest");
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
    /// {"test": "a-close-catches-up-with-the-files-on-disk", "checks": ["crates/server/src/daemon#catches-up", "req:daemon-catches-up-before-judging"]}
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
        assert_eq!(closed["node"]["status"], "closed", "{closed}");
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
    /// {"test": "catching-up-reads-what-the-whole-checkout-would", "checks": ["crates/server/src/daemon#catches-up", "req:daemon-catches-up-before-judging"]}
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
    /// {"test": "a-folder-that-becomes-a-checkout-leaves-the-graph", "checks": ["crates/server/src/daemon#one-file-reextract", "req:extract-nested-checkouts-skipped"]}
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
    /// {"test": "a-recipe-follows-the-files-it-names", "checks": ["crates/server/src/daemon#recipes-follow-files", "crates/server/src/daemon#catches-up", "req:recipe-read", "req:daemon-catches-up-before-judging"]}
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
        let ticket = d.call("file_ticket", json!({ "title": "T", "kind": "change", "priority": "later", "body": "b" })).unwrap()["node"]["ref"].clone();
        d.call("start_ticket", json!({ "ref": ticket })).unwrap();
        let judged = d.call("judge_recipe", json!({ "ref": "recipe:button", "outcome": "held" })).unwrap();
        assert_ne!(judged["judgement"]["fingerprint"], before, "not the digest the watcher last reported");
        assert_eq!(judged["judgement"]["fingerprint"], digest(&d));
        assert_eq!(judged["due"], false);
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
        assert_eq!(
            entries,
            vec!["baseline.json", "current.json", "grimoire.json", "pages", "proposals", "requirements", "serve.json", "tickets"],
            "records, the baseline, the current ticket, its proposal and the marker; no index or cache"
        );
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
        seen.push(grimoire_core::model::FindingKey {
            rule: grimoire_core::lint::Rule::Unspecified,
            check: "no-scenario".into(),
            r#ref: grimoire_core::model::Reference::req("never-held"),
            edge: None,
        });
        // Judged against intent that has since moved: refused, and nothing goes.
        let read = crate::remote::intent_digest(&shared.exported());
        shared.call("file_ticket", json!({ "title": "Meanwhile", "kind": "gap", "priority": "later", "body": "b" })).unwrap();
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
    /// {"test": "a-shared-daemon-keeps-the-main-lines-recipes", "checks": ["crates/server/src/daemon#main-line-recipes", "req:recipe-main-line-snapshot"]}
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
