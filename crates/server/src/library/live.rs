//! # Module contract
//!
//! ```json
//! {"purpose":"Keep a library served while the daemon runs, a machine's or a team's server's: open, route and mark each grimoire it holds, find them again every few seconds, and let one go when it goes or while a command deletes its store.","criteria":[{"id":"current","statement":"refresh opens every grimoire library::find returns, or on a team's server library::find_shared, that is not served or held back, under its lock and marked at /g/<name> where the daemon listens, on a machine watched and following its server when it is a copy of one, on a team's server holding intent only and admitting callers by its own tokens, and lets go of every served grimoire, but the default, that find no longer returns by the same name at the same store with the same checkout; one that could not be taken or opened is tried again on the next refresh; after any change every served grimoire is told the whole library, and once stopped, refresh takes nothing."},{"id":"routes","statement":"A request to /g/<name>/api/... or /g/<name>/mcp is answered by the routes of a checkout the library holds of the grimoire by that name as the request arrives, the default's included, with the path the grimoire's own routes expect, its body passed on as it came but for a GET's, which carries nothing, and none of the library's own path parameters; a name the library does not hold is answered not_found."},{"id":"checkout-chosen","statement":"The checkout a request is answered by is the one its `checkout` names, read from the JSON body of a POST to /api/<tool>, from the arguments of an MCP tools/call, every tools/call of a batch naming the same one, or from the query of any other request: the served checkout of the grimoire that is or holds that directory, the deepest when several do, compared as canonical paths, a directory that does not exist as its deepest existing folder's; with none named, the grimoire's one checkout, or, for status, the tools list, the event stream, the export, the prune and every MCP message but a tools/call, the default's when it is a checkout of the grimoire and else the first."},{"id":"checkout-refused","statement":"A request naming no checkout to a grimoire served for several, but those answered without one, and a request naming a directory no served checkout of the grimoire is or holds, are refused, as are a batch's calls naming different checkouts: the request is handed to the grimoire's first checkout's routes marked CheckoutRefused, so its caller is admitted or refused at that trust step first, and the refusal is then answered as a tool's own invalid error is, its details naming `checkout`, over the API as that error and over MCP as a tool-level error result; the message lists the grimoire's checkouts and says to name the caller's working directory as `checkout`, and for a store of its own, served with the one checkout it records, says it serves one checkout and that a project worked on in several checkouts at once is a team's."},{"id":"let-go","statement":"Letting a grimoire go takes it off the routes first, then waits for the requests already being answered by it to finish, up to a bound, before removing its marker and releasing its lock."},{"id":"release","statement":"release lets go of the served grimoire whose store is the one given and holds that store back from refresh until restore is called for it or the store is gone; restore lets refresh take it again; release answers false for the default grimoire, which it never lets go, and for a store it does not serve."}]}
//! ```

use std::collections::{BTreeMap, BTreeSet};
use std::net::SocketAddr;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use axum::body::Body;
use axum::extract::{Path as UrlPath, Request, State};
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::routing::{any, post};
use axum::{middleware, Json, Router};
use grimoire_core::answer::LibraryEntry;
use grimoire_core::ops::ToolError;
use grimoire_core::store::FieldError;
use tower::ServiceExt;

use super::{find, Grimoire};
use crate::access::{self, Policy};
use crate::daemon::Daemon;
use crate::store_io::{self, StoreLock};
use crate::{http, remote, watch};

/// How often the library is found again while the daemon runs.
pub const REFRESH: Duration = Duration::from_secs(2);

/// How long letting a grimoire go waits for the requests it is answering.
const DRAIN: Duration = Duration::from_secs(10);

type Watcher = notify_debouncer_full::Debouncer<notify::RecommendedWatcher, notify_debouncer_full::RecommendedCache>;

/// A grimoire the library serves, with what it holds while it is served:
/// none of it for the default, whose lock and watcher `serve` holds itself.
struct Shelved {
    store_dir: PathBuf,
    /// The checkout whose code it is joined with, and that checkout as a canonical path, for choosing by `checkout`.
    repo: PathBuf,
    canon: PathBuf,
    /// A copy of a team's grimoire, of which a machine may hold several checkouts; else a store of its own.
    team: bool,
    daemon: Arc<Daemon>,
    routes: Router,
    lock: Option<StoreLock>,
    watcher: Option<Watcher>,
}

impl Shelved {
    /// Let it go, off the routes already: stop watching, wait for the requests
    /// it is answering, which hold its daemon, then unmark and unlock.
    fn let_go(self) {
        let Shelved { daemon, routes, lock, watcher, .. } = self;
        drop(watcher);
        drop(routes);
        let deadline = Instant::now() + DRAIN;
        // Ours is the one strong reference left once no request holds the daemon.
        while Arc::strong_count(&daemon) > 1 && Instant::now() < deadline {
            std::thread::sleep(Duration::from_millis(20));
        }
        daemon.unmark_serving();
        drop(lock);
    }
}

/// What the library serves by: a grimoire's name and the checkout it is served with.
type Key = (String, PathBuf);

#[derive(Default)]
struct Held {
    served: BTreeMap<Key, Shelved>,
    /// Stores a command is deleting, held back from refresh until it says it is done.
    held_back: BTreeSet<PathBuf>,
    /// The reasons already logged, by grimoire, so each is said once.
    said: BTreeSet<String>,
}

/// The machine's library as the daemon serves it.
pub struct Live {
    /// The grimoire a machine's daemon was started for, served already; a team's server has none.
    default: Option<Grimoire>,
    source: Source,
    /// Where the daemon listens, which every marker names.
    addr: SocketAddr,
    /// A machine's one policy; a team's server admits each grimoire's callers by its own tokens.
    policy: Option<Arc<dyn Policy>>,
    held: Mutex<Held>,
    /// One refresh at a time, and the stop waits for one under way.
    refreshing: Mutex<()>,
    stopped: AtomicBool,
}

/// Where a library's grimoires come from.
enum Source {
    /// A machine's: the stores and copies its checkouts name.
    Machine { stores: PathBuf, copies: PathBuf },
    /// A team's server's: the stores under its library directory, holding no code.
    Shared { dir: PathBuf },
}

fn same(a: &Path, b: &Path) -> bool {
    a.canonicalize().ok().zip(b.canonicalize().ok()).is_some_and(|(a, b)| a == b)
}

impl Live {
    /// The library around `default`, already served by `daemon` on `port`;
    /// the others are taken by `refresh`.
    pub fn new(default: Grimoire, daemon: Arc<Daemon>, port: u16, policy: Arc<dyn Policy>, stores: PathBuf, copies: PathBuf) -> Arc<Live> {
        let routes = http::api(daemon.clone(), policy.clone());
        let mut held = Held::default();
        let shelved = Shelved { store_dir: default.store_dir.clone(), repo: default.repo.clone(), canon: canonical(&default.repo), team: default.server.is_some(), daemon, routes, lock: None, watcher: None };
        held.served.insert((default.name.clone(), default.repo.clone()), shelved);
        let addr = SocketAddr::from(([127, 0, 0, 1], port));
        let live = Arc::new(Live { default: Some(default), source: Source::Machine { stores, copies }, addr, policy: Some(policy), held: Mutex::new(held), refreshing: Mutex::new(()), stopped: AtomicBool::new(false) });
        // Told from the start, so status names a library of one before anything else is found.
        live.tell();
        live
    }

    /// A team's server's library: the stores under `dir`, served at `addr`, each
    /// admitting a caller by its own tokens; all of them are taken by `refresh`.
    pub fn new_shared(dir: PathBuf, addr: SocketAddr) -> Arc<Live> {
        Arc::new(Live { default: None, source: Source::Shared { dir }, addr, policy: None, held: Mutex::new(Held::default()), refreshing: Mutex::new(()), stopped: AtomicBool::new(false) })
    }

    fn is_default(&self, key: &Key) -> bool {
        self.default.as_ref().is_some_and(|d| d.name == key.0 && d.repo == key.1)
    }

    /// Whether this is a machine's library, which commands ask to let a store go.
    fn on_a_machine(&self) -> bool {
        matches!(self.source, Source::Machine { .. })
    }

    /// The names of the grimoires served now, in name order.
    pub fn names(&self) -> Vec<String> {
        let mut names: Vec<String> = self.held.lock().expect("library lock").served.keys().map(|(name, _)| name.clone()).collect();
        names.dedup();
        names
    }

    /// Say `what` about grimoire `name` once, however often it stays true.
    fn say(&self, name: &str, what: &str) {
        if self.held.lock().expect("library lock").said.insert(format!("{name}\n{what}")) {
            tracing::info!("library: `{name}`: {what}");
        }
    }

    /// Find the library again: let go of what went, take what came.
    pub fn refresh(&self) {
        let _one = self.refreshing.lock().expect("refresh lock");
        if self.stopped.load(Ordering::SeqCst) {
            return;
        }
        let found = match (&self.source, &self.default) {
            (Source::Machine { stores, copies }, Some(default)) => find(stores, copies, default),
            (Source::Shared { dir }, _) => super::find_shared(dir),
            (Source::Machine { .. }, None) => super::Found::default(),
        };
        for why in &found.skipped {
            self.say("not served", why);
        }
        // Gone, renamed, or no longer named by its checkout: off the routes now, let go below.
        let (gone, wanted) = {
            let mut held = self.held.lock().expect("library lock");
            held.held_back.retain(|dir| dir.exists());
            let keys: Vec<Key> = held
                .served
                .iter()
                .filter(|(key, s)| !self.is_default(key) && !found.grimoires.iter().any(|g| g.name == key.0 && same(&g.store_dir, &s.store_dir) && same(&g.repo, &s.repo)))
                .map(|(key, _)| key.clone())
                .collect();
            let gone: Vec<(String, Shelved)> = keys.into_iter().filter_map(|k| held.served.remove(&k).map(|s| (k.0, s))).collect();
            let wanted: Vec<Grimoire> = found.grimoires.into_iter().filter(|g| !held.served.values().any(|s| same(&s.store_dir, &g.store_dir)) && !held.held_back.iter().any(|r| same(r, &g.store_dir))).collect();
            (gone, wanted)
        };
        let mut changed = !gone.is_empty();
        for (name, s) in gone {
            s.let_go();
            tracing::info!("library: let go of `{name}`, which this machine's checkouts no longer name");
        }
        // Opened without the library held, since opening reads a checkout's code.
        for g in wanted {
            let Some(shelved) = self.open(&g) else { continue };
            let mut held = self.held.lock().expect("library lock");
            if self.stopped.load(Ordering::SeqCst) || held.served.values().any(|s| same(&s.store_dir, &g.store_dir)) || held.held_back.iter().any(|r| same(r, &g.store_dir)) {
                drop(held);
                shelved.let_go();
                continue;
            }
            tracing::info!("library: serving `{}` from {} at /g/{}", g.name, g.repo.display(), g.name);
            held.served.insert((g.name.clone(), g.repo.clone()), shelved);
            changed = true;
        }
        if changed {
            self.tell();
        }
    }

    /// Take one grimoire: its lock, its daemon, a watcher on its checkout and its
    /// marker, and its server followed when it is a copy of one. None when any of
    /// these fails, said once, to be tried again on the next refresh.
    fn open(&self, g: &Grimoire) -> Option<Shelved> {
        if !self.on_a_machine() {
            return self.open_shared(g);
        }
        let lock = match &g.server {
            Some(_) => remote::lock_copy(&g.store_dir),
            None => store_io::lock(&g.store_dir),
        };
        let lock = match lock {
            Ok(lock) => lock,
            Err(e) => {
                self.say(&g.name, &format!("not served: another process holds it: {e}"));
                return None;
            }
        };
        let opened = match &g.server {
            Some(address) => store_io::credentials_file().map_err(|e| e.to_string()).and_then(|credentials| Daemon::open_remote(&g.store_dir, &g.repo, remote::Server::signed_in(address, &credentials)).map_err(|e| e.to_string())),
            None => Daemon::open(&g.store_dir).map_err(|e| e.to_string()),
        };
        let daemon = match opened {
            Ok((daemon, errors)) => {
                for e in &errors {
                    self.say(&g.name, &e.to_string());
                }
                Arc::new(daemon)
            }
            Err(e) => {
                self.say(&g.name, &format!("not served: it could not be opened: {e}"));
                return None;
            }
        };
        // A grimoire nobody watches would answer from a graph that goes stale: not served at all.
        let watcher = match watch::start(daemon.clone()) {
            Ok(w) => w,
            Err(e) => {
                self.say(&g.name, &format!("not served: its checkout could not be watched: {e}"));
                return None;
            }
        };
        // Marked before it is routed: a grimoire commands cannot find would leave them refused by its lock.
        if let Err(e) = daemon.mark_serving_in(self.addr, &format!("/g/{}", g.name)) {
            self.say(&g.name, &format!("not served: its store could not be marked as served: {e}"));
            return None;
        }
        if g.server.is_some() {
            remote::follow(&daemon);
        }
        let routes = http::api(daemon.clone(), self.policy_for(&g.store_dir));
        Some(Shelved { store_dir: g.store_dir.clone(), repo: g.repo.clone(), canon: canonical(&g.repo), team: g.server.is_some(), daemon, routes, lock: Some(lock), watcher: Some(watcher) })
    }

    /// Take one grimoire of a team's server: its lock and its daemon, which holds
    /// intent only, and its marker; nothing is watched, since it holds no code.
    fn open_shared(&self, g: &Grimoire) -> Option<Shelved> {
        let lock = match store_io::lock(&g.store_dir) {
            Ok(lock) => lock,
            Err(e) => {
                self.say(&g.name, &format!("not served: another process holds it: {e}"));
                return None;
            }
        };
        let daemon = match Daemon::open_shared(&g.store_dir) {
            Ok(daemon) => Arc::new(daemon),
            Err(e) => {
                self.say(&g.name, &format!("not served: it could not be opened: {e}"));
                return None;
            }
        };
        if store_io::tokens::list(&g.store_dir).map(|t| t.is_empty()).unwrap_or(true) {
            self.say(&g.name, &format!("served, but it holds no tokens, so nobody can sign in: make one with `grimoire token add <name> --store {}`", g.store_dir.display()));
        }
        if let Err(e) = daemon.mark_serving_in(self.addr, &format!("/g/{}", g.name)) {
            self.say(&g.name, &format!("not served: its store could not be marked as served: {e}"));
            return None;
        }
        let routes = http::api(daemon.clone(), self.policy_for(&g.store_dir));
        Some(Shelved { store_dir: g.store_dir.clone(), repo: g.repo.clone(), canon: canonical(&g.repo), team: false, daemon, routes, lock: Some(lock), watcher: None })
    }

    /// Who may call a grimoire: the machine's one policy, or on a team's server its own tokens.
    fn policy_for(&self, store_dir: &Path) -> Arc<dyn Policy> {
        match &self.policy {
            Some(p) => p.clone(),
            None => Arc::new(access::Tokens { store_dir: store_dir.to_path_buf() }),
        }
    }

    /// Tell every served grimoire the whole library, for `status`.
    fn tell(&self) {
        let held = self.held.lock().expect("library lock");
        let all: Vec<LibraryEntry> = held
            .served
            .iter()
            .map(|((name, _), s)| LibraryEntry { name: name.clone(), repository: store_io::repository_string(&s.daemon.repo).unwrap_or_else(|_| s.daemon.repo.display().to_string()) })
            .collect();
        for ((name, _), s) in &held.served {
            s.daemon.set_library(name, all.clone());
        }
    }

    /// Let go of the grimoire whose store is `store_dir`, so a command can delete
    /// it, and hold that store back from refresh until `restore` or until it is
    /// gone. False for the default and for a store not served.
    pub fn release(&self, store_dir: &Path) -> bool {
        let taken = {
            let mut held = self.held.lock().expect("library lock");
            let key = held.served.iter().find(|(key, s)| !self.is_default(key) && same(&s.store_dir, store_dir)).map(|(key, _)| key.clone());
            let taken = key.and_then(|k| held.served.remove(&k).map(|s| (k.0, s)));
            if let Some((_, s)) = &taken {
                held.held_back.insert(s.store_dir.clone());
            }
            taken
        };
        let Some((name, s)) = taken else { return false };
        s.let_go();
        tracing::info!("library: let go of `{name}`, as a command deleting its store asked");
        self.tell();
        true
    }

    /// Let refresh take `store_dir` again: the command that asked for its release
    /// is done with it, whether it deleted it or stopped.
    pub fn restore(&self, store_dir: &Path) {
        let mut held = self.held.lock().expect("library lock");
        held.held_back.retain(|r| r.as_path() != store_dir && !same(r, store_dir));
    }

    /// Stop: no refresh takes anything after this, one under way is waited for,
    /// and every marker the library took is removed; the default's is `serve`'s.
    pub fn stop(&self) {
        self.stopped.store(true, Ordering::SeqCst);
        let _one = self.refreshing.lock().expect("refresh lock");
        let held = self.held.lock().expect("library lock");
        for (key, s) in &held.served {
            if !self.is_default(key) {
                s.daemon.unmark_serving();
            }
        }
    }

    /// Find the library again every `REFRESH` until the library is stopped or dropped.
    pub fn keep_current(self: &Arc<Live>) {
        let live = Arc::downgrade(self);
        let spawned = std::thread::Builder::new().name("grimoire-library".to_string()).spawn(move || loop {
            std::thread::sleep(REFRESH);
            let Some(live) = live.upgrade() else { return };
            if live.stopped.load(Ordering::SeqCst) {
                return;
            }
            live.refresh();
        });
        if let Err(e) = spawned {
            tracing::warn!("library: it will not be kept current, since its thread could not start: {e}");
        }
    }

    /// The daemon of the grimoire served by `name` now, if any.
    pub fn daemon_of(&self, name: &str) -> Option<Arc<Daemon>> {
        self.held.lock().expect("library lock").served.iter().find(|((n, _), _)| n == name).map(|(_, s)| s.daemon.clone())
    }

    /// The checkout of grimoire `name` that answers a call naming `checkout`, as its key, or why
    /// none does (req:machine-checkouts). `lenient`: a call answered without one, by the default's
    /// checkout when it is one of the grimoire's, else the first.
    fn choose(&self, name: &str, checkout: Option<&str>, lenient: bool) -> Result<Key, Refusal> {
        let held = self.held.lock().expect("library lock");
        let ours: Vec<(&Key, &Shelved)> = held.served.iter().filter(|((n, _), _)| n == name).collect();
        let Some(&(first_key, first)) = ours.first() else {
            return Err(Refusal::NotFound(format!("this daemon's library holds no grimoire `{name}`")));
        };
        let list = || ours.iter().map(|(_, s)| s.repo.display().to_string()).collect::<Vec<_>>().join(", ");
        match checkout.map(str::trim).filter(|c| !c.is_empty()) {
            Some(dir) => {
                let asked = canonical(Path::new(dir));
                // The deepest checkout that is or holds the directory: a worktree may be kept inside its main checkout.
                match ours.iter().filter(|(_, s)| asked.starts_with(&s.canon)).max_by_key(|(_, s)| s.canon.components().count()) {
                    Some((key, _)) => Ok((*key).clone()),
                    None if ours.len() == 1 && !first.team => Err(refused(format!(
                        "the grimoire `{name}` is a store of its own, which serves one checkout, {}; this call names `{dir}`. A project worked on in several checkouts at once is a team's: name a team's server in its .grimoire.toml",
                        first.repo.display()
                    ))),
                    None => Err(refused(format!("`{dir}` is no checkout of the grimoire `{name}` served here, whose checkouts are {}; name your working directory as `checkout`", list()))),
                }
            }
            None if ours.len() == 1 => Ok(first_key.clone()),
            None if lenient => Ok(ours.iter().find(|(key, _)| self.is_default(key)).map_or(first_key, |(key, _)| *key).clone()),
            None => Err(refused(format!(
                "the grimoire `{name}` is served for {} checkouts on this machine: {}; this call names none, so it cannot be told which one it comes from. Name yours with `checkout`, your working directory",
                ours.len(),
                list()
            ))),
        }
    }

    /// The routes a request is handed to: the checkout's under `key`, or, refused, the grimoire's
    /// first checkout's, which admit the caller at their trust step before answering the refusal.
    fn routes_for(&self, name: &str, key: Option<&Key>) -> Option<Router> {
        let held = self.held.lock().expect("library lock");
        match key {
            Some(key) => held.served.get(key),
            None => held.served.iter().find(|((n, _), _)| n == name).map(|(_, s)| s),
        }
        .map(|s| s.routes.clone())
    }
}

/// The routes of the library: each grimoire's API and MCP at `/g/<name>/`,
/// looked up as each request arrives, and the release a command asks for.
pub fn routes(live: Arc<Live>) -> Router {
    let grimoires = Router::new()
        .route("/g/{name}/api/{*rest}", any(to_grimoire))
        .route("/g/{name}/mcp", any(to_grimoire_mcp))
        .route("/g/{name}/mcp/{*rest}", any(to_grimoire))
        .with_state(live.clone());
    // Only a machine's library is asked to let a store go: a team's server has no uninstall beside it.
    match live.policy.clone() {
        Some(policy) => grimoires.merge(Router::new().route("/library/release", post(release)).with_state(live).layer(middleware::from_fn_with_state(policy, access::admit))),
        None => grimoires,
    }
}

/// The bare routes, /api/... and /mcp, for the grimoire `name` the daemon was started for: answered
/// as /g/<name>/ is, so a call there is answered by the checkout it names too (req:machine-checkouts).
pub fn bare(live: Arc<Live>, name: String) -> Router {
    let api = {
        let (live, name) = (live.clone(), name.clone());
        move |UrlPath(rest): UrlPath<String>, req: Request| {
            let (live, name) = (live.clone(), name.clone());
            async move { forward(&live, &name, &format!("/api/{rest}"), req).await }
        }
    };
    let mcp_rest = {
        let (live, name) = (live.clone(), name.clone());
        move |UrlPath(rest): UrlPath<String>, req: Request| {
            let (live, name) = (live.clone(), name.clone());
            async move { forward(&live, &name, &format!("/mcp/{rest}"), req).await }
        }
    };
    let mcp = move |req: Request| {
        let (live, name) = (live.clone(), name.clone());
        async move { forward(&live, &name, "/mcp", req).await }
    };
    Router::new().route("/api/{*rest}", any(api)).route("/mcp", any(mcp)).route("/mcp/{*rest}", any(mcp_rest))
}

async fn to_grimoire(State(live): State<Arc<Live>>, UrlPath((name, rest)): UrlPath<(String, String)>, req: Request) -> Response {
    let under = if req.uri().path().starts_with(&format!("/g/{name}/mcp/")) { "mcp" } else { "api" };
    forward(&live, &name, &format!("/{under}/{rest}"), req).await
}

async fn to_grimoire_mcp(State(live): State<Arc<Live>>, UrlPath(name): UrlPath<String>, req: Request) -> Response {
    forward(&live, &name, "/mcp", req).await
}

/// Why no checkout answers a request.
enum Refusal {
    NotFound(String),
    Invalid(ToolError),
}

/// A call refused for its checkout: an invalid error naming the `checkout` field, as a tool's own is.
fn refused(message: String) -> Refusal {
    Refusal::Invalid(ToolError::invalid(message.clone(), vec![FieldError { field: "checkout".into(), reason: message }]))
}

/// The most a request's body may hold for the library to read its `checkout`.
const BODY_LIMIT: usize = 64 * 1024 * 1024;

/// A path as the system names it, so two spellings of one folder compare equal. A path that does
/// not exist is its deepest existing folder as the system names it, with the rest as given.
fn canonical(p: &Path) -> PathBuf {
    let mut rest = Vec::new();
    let mut at = p;
    loop {
        if let Ok(found) = at.canonicalize() {
            return rest.iter().rev().fold(found, |path: PathBuf, part| path.join(part));
        }
        match (at.parent(), at.file_name()) {
            (Some(parent), Some(part)) => {
                rest.push(part.to_os_string());
                at = parent;
            }
            _ => return p.to_path_buf(),
        }
    }
}

/// What a request says about its checkout: for each call in it, the directory it names and
/// whether it may be answered without one. An MCP message that is no tools/call is one call
/// answered by any checkout; a batch is as many as its tools/calls.
fn asked(path: &str, query: Option<&str>, body: &[u8]) -> Vec<(Option<String>, bool)> {
    let from_query = query.and_then(|q| q.split('&').filter_map(|pair| pair.split_once('=')).find(|(k, _)| *k == "checkout").map(|(_, v)| percent_decode(v)));
    let json: Option<serde_json::Value> = serde_json::from_slice(body).ok();
    if path == "/mcp" || path.starts_with("/mcp/") {
        let messages: Vec<serde_json::Value> = match json {
            Some(serde_json::Value::Array(all)) => all,
            Some(one) => vec![one],
            None => vec![],
        };
        let calls: Vec<(Option<String>, bool)> = messages
            .iter()
            .filter(|m| m["method"] == "tools/call")
            .map(|m| (m["params"]["arguments"]["checkout"].as_str().map(str::to_string), m["params"]["name"].as_str().is_some_and(grimoire_core::tools::answers_any_checkout)))
            .collect();
        // Every other message is the same whichever checkout answers.
        return if calls.is_empty() { vec![(from_query, true)] } else { calls };
    }
    let tool = path.strip_prefix("/api/").unwrap_or("");
    // The tools list, the event stream, the export and the prune are routes, not tools: the same whichever checkout answers.
    let lenient = grimoire_core::tools::answers_any_checkout(tool) || matches!(tool, "tools" | "events" | "export" | "prune");
    let from_body = json.as_ref().and_then(|j| j["checkout"].as_str().map(str::to_string));
    vec![(from_body.or(from_query), lenient)]
}

/// A query value with its `+` and `%XX` escapes undone.
fn percent_decode(v: &str) -> String {
    let bytes = v.replace('+', " ").into_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        let hex = |b: u8| (b as char).to_digit(16);
        match (bytes[i], bytes.get(i + 1).copied().and_then(hex), bytes.get(i + 2).copied().and_then(hex)) {
            (b'%', Some(h), Some(l)) => {
                out.push((h * 16 + l) as u8);
                i += 3;
            }
            (b, _, _) => {
                out.push(b);
                i += 1;
            }
        }
    }
    String::from_utf8_lossy(&out).into_owned()
}

/// The checkout every call of a request names, or why it cannot be answered by one.
fn one_checkout(live: &Live, name: &str, calls: &[(Option<String>, bool)]) -> Result<Key, Refusal> {
    let mut chosen: Option<Key> = None;
    for (checkout, lenient) in calls {
        let key = live.choose(name, checkout.as_deref(), *lenient)?;
        match &chosen {
            Some(other) if *other != key => return Err(refused(format!("the calls of one request name different checkouts of the grimoire `{name}`; send each checkout's calls apart"))),
            _ => chosen = Some(key),
        }
    }
    chosen.ok_or_else(|| Refusal::NotFound(format!("this daemon's library holds no grimoire `{name}`")))
}

/// Hand `req` to the routes of the checkout of grimoire `name` it names, at `path`, its query
/// kept. The request is made afresh from its method, headers and body: the library's own
/// route left its path parameters on it, which the grimoire's routes would read as theirs,
/// and the grimoire's trust step admits the caller again itself. A request refused for its
/// checkout goes to the grimoire's first checkout marked CheckoutRefused, so it meets the
/// trust step first and its error is answered as any tool's is, over the API or MCP alike.
pub(crate) async fn forward(live: &Live, name: &str, path: &str, req: Request<Body>) -> Response {
    let query = req.uri().query().map(str::to_string);
    let target = match &query {
        Some(q) => format!("{path}?{q}"),
        None => path.to_string(),
    };
    let (parts, body) = req.into_parts();
    // A GET, the event stream's among them, carries nothing in its body and names its checkout in
    // its query; any other request is read whole for it and passed on as it came.
    let bytes = if parts.method == axum::http::Method::GET {
        Default::default()
    } else {
        match axum::body::to_bytes(body, BODY_LIMIT).await {
            Ok(b) => b,
            Err(_) => return (StatusCode::PAYLOAD_TOO_LARGE, Json(serde_json::json!({ "error": "invalid", "message": "the request is too large" }))).into_response(),
        }
    };
    let (routes, refusal) = match one_checkout(live, name, &asked(path, query.as_deref(), &bytes)) {
        Ok(key) => (live.routes_for(name, Some(&key)), None),
        Err(Refusal::Invalid(e)) => (live.routes_for(name, None), Some(e)),
        Err(Refusal::NotFound(message)) => return (StatusCode::NOT_FOUND, Json(serde_json::json!({ "error": "not_found", "message": message }))).into_response(),
    };
    let Some(routes) = routes else {
        return (StatusCode::NOT_FOUND, Json(serde_json::json!({ "error": "not_found", "message": format!("this daemon's library holds no grimoire `{name}`") }))).into_response();
    };
    let mut fresh = Request::new(Body::from(bytes));
    if let Some(e) = refusal {
        fresh.extensions_mut().insert(access::CheckoutRefused(e));
    }

    match target.parse() {
        Ok(uri) => *fresh.uri_mut() = uri,
        Err(_) => return StatusCode::BAD_REQUEST.into_response(),
    }
    *fresh.method_mut() = parts.method;
    *fresh.version_mut() = parts.version;
    *fresh.headers_mut() = parts.headers;
    match routes.oneshot(fresh).await {
        Ok(r) => r,
        Err(never) => match never {},
    }
}

#[derive(serde::Deserialize)]
struct ReleaseIn {
    store: String,
    /// The command that asked for the release is done with the store: refresh may take it again.
    #[serde(default)]
    done: bool,
}

async fn release(State(live): State<Arc<Live>>, Json(input): Json<ReleaseIn>) -> Response {
    let store = PathBuf::from(input.store);
    let answer = tokio::task::spawn_blocking(move || {
        if input.done {
            live.restore(&store);
            serde_json::json!({ "restored": true })
        } else {
            serde_json::json!({ "released": live.release(&store) })
        }
    })
    .await
    .unwrap_or_else(|_| serde_json::json!({ "released": false }));
    Json(answer).into_response()
}

#[cfg(test)]
mod tests {
    use super::*;
    use grimoire_core::model::{Roots, Scope, StoreConfig};

    /// A checkout at `root/<name>` with a store at `root/stores/<name>` that its project file names.
    fn project(root: &Path, name: &str) -> Grimoire {
        let repo = root.join(name);
        std::fs::create_dir_all(repo.join("src")).unwrap();
        std::fs::write(repo.join("src").join("lib.rs"), "pub fn a() {}\n").unwrap();
        let store_dir = root.join("stores").join(name);
        let config = StoreConfig { site: vec![], version: StoreConfig::CURRENT_VERSION, repository: store_io::repository_string(&repo).unwrap(), roots: Roots::new("overview", "rites"), languages: vec!["rust".into()], scope: Scope::default(), order: Default::default() };
        store_io::write_all(&store_dir, &grimoire_core::Store::new(config), false).unwrap();
        std::fs::write(repo.join(store_io::PROJECT_FILE), format!("name = \"{name}\"\nstore = {}\n", toml::Value::String(store_dir.to_string_lossy().to_string()))).unwrap();
        Grimoire { name: name.into(), store_dir, repo, server: None }
    }

    /// # Test contract
    ///
    /// ```json
    /// {"test": "letting-go-waits-for-the-requests-under-way", "checks": ["crates/server/src/library/live#let-go", "crates/server/src/library/live#release"]}
    /// ```
    #[test]
    fn letting_go_waits_for_the_requests_under_way() {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path();
        let (shop, ledger) = (project(root, "shop"), project(root, "ledger"));
        let _shop_lock = store_io::lock(&shop.store_dir).unwrap();
        let daemon = Arc::new(Daemon::open(&shop.store_dir).unwrap().0);
        let live = Live::new(shop.clone(), daemon, 4299, Arc::new(access::Local), root.join("stores"), root.join("copies"));
        live.refresh();
        assert_eq!(live.names(), ["ledger", "shop"]);
        assert!(store_io::read_serving(&ledger.store_dir).is_some_and(|m| m.path.as_deref() == Some("/g/ledger")));
        // A request under way holds the grimoire's routes, and with them its daemon.
        let under_way = live.routes_for("ledger", None).unwrap();
        let answering = std::thread::spawn(move || {
            std::thread::sleep(Duration::from_millis(400));
            drop(under_way);
        });
        let asked = Instant::now();
        assert!(live.release(&ledger.store_dir), "a grimoire the library serves is let go");
        assert!(asked.elapsed() >= Duration::from_millis(300), "letting go waited for the request under way");
        answering.join().unwrap();
        assert!(store_io::read_serving(&ledger.store_dir).is_none(), "its marker is gone");
        drop(store_io::lock(&ledger.store_dir).expect("and its lock is free for the command that asked"));
        // Held back while the command holds it, taken again once it is done.
        live.refresh();
        assert_eq!(live.names(), ["shop"]);
        live.restore(&ledger.store_dir);
        live.refresh();
        assert_eq!(live.names(), ["ledger", "shop"]);
        // The default is never let go; once stopped, nothing more is taken and the markers go.
        assert!(!live.release(&shop.store_dir));
        live.stop();
        assert!(store_io::read_serving(&ledger.store_dir).is_none());
    }

    /// Answer `status` at `routes`, for the repository it names.
    async fn repository_of(routes: Router, checkout: Option<&str>) -> String {
        let body = match checkout {
            Some(c) => serde_json::json!({ "checkout": c }).to_string(),
            None => "{}".to_string(),
        };
        let req = Request::builder().method("POST").uri("/api/status").header("host", "127.0.0.1").header("content-type", "application/json").body(Body::from(body)).unwrap();
        let res = routes.oneshot(req).await.unwrap();
        let bytes = axum::body::to_bytes(res.into_body(), 1 << 20).await.unwrap();
        serde_json::from_slice::<serde_json::Value>(&bytes).unwrap()["repository"].as_str().unwrap_or_default().to_string()
    }

    /// # Test contract
    ///
    /// ```json
    /// {"test": "a-call-is-answered-by-the-checkout-it-names", "checks": ["crates/server/src/library/live#checkout-chosen", "crates/server/src/library/live#checkout-refused"]}
    /// ```
    #[tokio::test]
    async fn a_call_is_answered_by_the_checkout_it_names() {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path();
        // `shop`, the daemon's own, and a worktree of it kept inside it, served as two checkouts of one team's grimoire.
        let main = project(root, "shop");
        let worktree = project(&main.repo, "wt");
        let _locks = (store_io::lock(&main.store_dir).unwrap(), store_io::lock(&worktree.store_dir).unwrap());
        let daemon = Arc::new(Daemon::open(&main.store_dir).unwrap().0);
        let live = Live::new(Grimoire { server: Some("http://team".into()), ..main.clone() }, daemon, 4299, Arc::new(access::Local), root.join("none"), root.join("none"));
        let other = Arc::new(Daemon::open(&worktree.store_dir).unwrap().0);
        {
            let mut held = live.held.lock().unwrap();
            let shelved = Shelved { store_dir: worktree.store_dir.clone(), repo: worktree.repo.clone(), canon: canonical(&worktree.repo), team: true, daemon: other.clone(), routes: http::api(other, Arc::new(access::Local)), lock: None, watcher: None };
            held.served.insert(("shop".into(), worktree.repo.clone()), shelved);
        }
        let repo = |g: &Grimoire| store_io::repository_string(&g.repo).unwrap();
        let named = |dir: &Path| dir.to_string_lossy().to_string();
        // Each checkout answers for itself; a subfolder is its checkout's, the deepest that holds it.
        let at = |key: Result<Key, Refusal>| live.routes_for("shop", Some(&key.ok().unwrap())).unwrap();
        assert_eq!(repository_of(at(live.choose("shop", Some(&named(&main.repo)), false)), None).await, repo(&main));
        assert_eq!(repository_of(at(live.choose("shop", Some(&named(&worktree.repo.join("src"))), false)), None).await, repo(&worktree), "the worktree inside the main checkout is the deeper");
        assert_eq!(repository_of(at(live.choose("shop", Some(&named(&main.repo.join("src"))), false)), None).await, repo(&main));
        // A directory that does not exist yet is its deepest existing folder's, however it is spelt.
        let unborn = named(&worktree.repo.join("src").join("not-yet").join("deeper"));
        assert_eq!(repository_of(at(live.choose("shop", Some(&unborn), false)), None).await, repo(&worktree));
        let spelt = named(&main.repo).replace('\\', "/").to_uppercase();
        if cfg!(windows) {
            assert_eq!(repository_of(at(live.choose("shop", Some(&spelt), false)), None).await, repo(&main), "another spelling of the same folder");
        }
        // Named by no call, two checkouts are refused, but for what is answered without one, by the default.
        let why = |r: Result<Key, Refusal>| match r {
            Err(Refusal::Invalid(e)) => {
                let v = serde_json::to_value(&e).unwrap();
                assert_eq!((v["error"].as_str(), v["details"][0]["field"].as_str()), (Some("invalid"), Some("checkout")), "an invalid error naming the field: {v}");
                v["message"].as_str().unwrap().to_string()
            }
            _ => panic!("refused"),
        };
        let message = why(live.choose("shop", None, false));
        assert!(message.contains("2 checkouts") && message.contains(&main.repo.display().to_string()) && message.contains(&worktree.repo.display().to_string()) && message.contains("`checkout`"), "{message}");
        assert_eq!(repository_of(at(live.choose("shop", None, true)), None).await, repo(&main), "the default's");
        // A directory no checkout holds is refused, naming them.
        let message = why(live.choose("shop", Some(&named(root)), false));
        assert!(message.contains("is no checkout of the grimoire `shop`") && message.contains(&worktree.repo.display().to_string()), "{message}");
        // The calls of one request name one checkout, or all are refused.
        assert!(one_checkout(&live, "shop", &[(Some(named(&main.repo)), false), (Some(named(&main.repo.join("src"))), false)]).is_ok());
        let message = why(one_checkout(&live, "shop", &[(Some(named(&main.repo)), false), (Some(named(&worktree.repo)), false)]));
        assert!(message.contains("different checkouts"), "{message}");
        // A store of its own serves one checkout: another is told so, and that several make a team.
        let ledger = project(root, "ledger");
        let _ledger_lock = store_io::lock(&ledger.store_dir).unwrap();
        let solo = Arc::new(Daemon::open(&ledger.store_dir).unwrap().0);
        {
            let mut held = live.held.lock().unwrap();
            let shelved = Shelved { store_dir: ledger.store_dir.clone(), repo: ledger.repo.clone(), canon: canonical(&ledger.repo), team: false, daemon: solo.clone(), routes: http::api(solo, Arc::new(access::Local)), lock: None, watcher: None };
            held.served.insert(("ledger".into(), ledger.repo.clone()), shelved);
        }
        assert!(live.choose("ledger", None, false).is_ok(), "one checkout needs no naming");
        let message = why(live.choose("ledger", Some(&named(&main.repo)), false));
        assert!(message.contains("a store of its own, which serves one checkout") && message.contains("team"), "{message}");
        assert!(matches!(live.choose("audit", None, true), Err(Refusal::NotFound(_))));
    }

    /// # Test contract
    ///
    /// ```json
    /// {"test": "a-request-says-its-checkout", "checks": ["crates/server/src/library/live#checkout-chosen"]}
    /// ```
    #[test]
    fn a_request_says_its_checkout() {
        assert_eq!(asked("/api/lint", None, br#"{"checkout":"C:/shop/wt"}"#), [(Some("C:/shop/wt".to_string()), false)]);
        assert_eq!(asked("/api/status", None, b"{}"), [(None, true)], "status is answered without one");
        assert_eq!(asked("/api/events", Some("checkout=C%3A%2Fshop+wt&x=1"), b""), [(Some("C:/shop wt".to_string()), true)], "from the query, unescaped");
        let call = br#"{"jsonrpc":"2.0","id":7,"method":"tools/call","params":{"name":"lint","arguments":{"checkout":"C:/shop"}}}"#;
        assert_eq!(asked("/mcp", None, call), [(Some("C:/shop".to_string()), false)]);
        assert_eq!(asked("/mcp", None, br#"{"jsonrpc":"2.0","id":1,"method":"tools/list"}"#), [(None, true)], "any other MCP message goes to any checkout");
        let batch = br#"[{"jsonrpc":"2.0","method":"notifications/initialized"},{"jsonrpc":"2.0","id":2,"method":"tools/call","params":{"name":"status","arguments":{}}},{"jsonrpc":"2.0","id":3,"method":"tools/call","params":{"name":"lint","arguments":{"checkout":"C:/b"}}}]"#;
        assert_eq!(asked("/mcp", None, batch), [(None, true), (Some("C:/b".to_string()), false)], "every call of a batch");
    }

    /// # Test contract
    ///
    /// ```json
    /// {"test": "a-store-pointed-at-another-checkout-is-served-with-it", "checks": ["crates/server/src/library/live#current"]}
    /// ```
    #[test]
    fn a_store_pointed_at_another_checkout_is_served_with_it() {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path();
        let (shop, ledger) = (project(root, "shop"), project(root, "ledger"));
        let _shop_lock = store_io::lock(&shop.store_dir).unwrap();
        let daemon = Arc::new(Daemon::open(&shop.store_dir).unwrap().0);
        let live = Live::new(shop.clone(), daemon, 4299, Arc::new(access::Local), root.join("stores"), root.join("copies"));
        live.refresh();
        let served_with = || live.held.lock().unwrap().served.keys().filter(|(n, _)| n == "ledger").map(|(_, r)| r.clone()).collect::<Vec<_>>();
        assert_eq!(served_with(), std::slice::from_ref(&ledger.repo));
        // The store now records another checkout, whose project file names it: served with that one, the first let go.
        let moved = root.join("ledger-moved");
        std::fs::create_dir_all(moved.join("src")).unwrap();
        std::fs::write(moved.join("src").join("lib.rs"), "pub fn a() {}\n").unwrap();
        std::fs::copy(ledger.repo.join(store_io::PROJECT_FILE), moved.join(store_io::PROJECT_FILE)).unwrap();
        let config = ledger.store_dir.join(store_io::CONFIG_FILE);
        let mut c: serde_json::Value = serde_json::from_str(&std::fs::read_to_string(&config).unwrap()).unwrap();
        c["repository"] = serde_json::json!(store_io::repository_string(&moved).unwrap());
        std::fs::write(&config, c.to_string()).unwrap();
        live.refresh();
        assert_eq!(served_with().iter().map(|r| canonical(r)).collect::<Vec<_>>(), [canonical(&moved)]);
        live.stop();
    }
}
