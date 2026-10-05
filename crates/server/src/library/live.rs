//! # Module contract
//!
//! ```json
//! {"purpose":"Keep a library served while the daemon runs, a machine's or a team's server's: open, route and mark each grimoire it holds, find them again every few seconds, and let one go when it goes or while a command deletes its store.","criteria":[{"id":"current","statement":"refresh opens every grimoire library::find returns, or on a team's server library::find_shared, that is not served or held back, under its lock and marked at /g/<name> where the daemon listens, on a machine watched and following its server when it is a copy of one, on a team's server holding intent only and admitting callers by its own tokens, and lets go of every served grimoire, but the default, that find no longer returns by the same name at the same store; one that could not be taken or opened is tried again on the next refresh; after any change every served grimoire is told the whole library, and once stopped, refresh takes nothing."},{"id":"routes","statement":"A request to /g/<name>/api/... or /g/<name>/mcp is answered by the routes of the grimoire the library holds by that name as the request arrives, the default's included, with the path the grimoire's own routes expect and none of the library's own path parameters; a name the library does not hold is answered not_found."},{"id":"let-go","statement":"Letting a grimoire go takes it off the routes first, then waits for the requests already being answered by it to finish, up to a bound, before removing its marker and releasing its lock."},{"id":"release","statement":"release lets go of the served grimoire whose store is the one given and holds that store back from refresh until restore is called for it or the store is gone; restore lets refresh take it again; release answers false for the default grimoire, which it never lets go, and for a store it does not serve."}]}
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

#[derive(Default)]
struct Held {
    served: BTreeMap<String, Shelved>,
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
        held.served.insert(default.name.clone(), Shelved { store_dir: default.store_dir.clone(), daemon, routes, lock: None, watcher: None });
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

    fn is_default(&self, name: &str) -> bool {
        self.default.as_ref().is_some_and(|d| d.name == name)
    }

    /// Whether this is a machine's library, which commands ask to let a store go.
    fn on_a_machine(&self) -> bool {
        matches!(self.source, Source::Machine { .. })
    }

    /// The names of the grimoires served now, in name order.
    pub fn names(&self) -> Vec<String> {
        self.held.lock().expect("library lock").served.keys().cloned().collect()
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
            let names: Vec<String> = held
                .served
                .iter()
                .filter(|(name, s)| !self.is_default(name) && !found.grimoires.iter().any(|g| g.name == **name && same(&g.store_dir, &s.store_dir)))
                .map(|(name, _)| name.clone())
                .collect();
            let gone: Vec<(String, Shelved)> = names.into_iter().filter_map(|n| held.served.remove(&n).map(|s| (n, s))).collect();
            let wanted: Vec<Grimoire> = found.grimoires.into_iter().filter(|g| !held.served.contains_key(&g.name) && !held.held_back.iter().any(|r| same(r, &g.store_dir))).collect();
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
            if self.stopped.load(Ordering::SeqCst) || held.served.contains_key(&g.name) || held.held_back.iter().any(|r| same(r, &g.store_dir)) {
                drop(held);
                shelved.let_go();
                continue;
            }
            tracing::info!("library: serving `{}` from {} at /g/{}", g.name, g.repo.display(), g.name);
            held.served.insert(g.name.clone(), shelved);
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
        Some(Shelved { store_dir: g.store_dir.clone(), daemon, routes, lock: Some(lock), watcher: Some(watcher) })
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
        Some(Shelved { store_dir: g.store_dir.clone(), daemon, routes, lock: Some(lock), watcher: None })
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
            .map(|(name, s)| LibraryEntry { name: name.clone(), repository: store_io::repository_string(&s.daemon.repo).unwrap_or_else(|_| s.daemon.repo.display().to_string()) })
            .collect();
        for (name, s) in &held.served {
            s.daemon.set_library(name, all.clone());
        }
    }

    /// Let go of the grimoire whose store is `store_dir`, so a command can delete
    /// it, and hold that store back from refresh until `restore` or until it is
    /// gone. False for the default and for a store not served.
    pub fn release(&self, store_dir: &Path) -> bool {
        let taken = {
            let mut held = self.held.lock().expect("library lock");
            let name = held.served.iter().find(|(name, s)| !self.is_default(name) && same(&s.store_dir, store_dir)).map(|(name, _)| name.clone());
            let taken = name.and_then(|n| held.served.remove(&n).map(|s| (n, s)));
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
        for (name, s) in &held.served {
            if !self.is_default(name) {
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

    fn routes_of(&self, name: &str) -> Option<Router> {
        self.held.lock().expect("library lock").served.get(name).map(|s| s.routes.clone())
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

async fn to_grimoire(State(live): State<Arc<Live>>, UrlPath((name, rest)): UrlPath<(String, String)>, req: Request) -> Response {
    let under = if req.uri().path().starts_with(&format!("/g/{name}/mcp/")) { "mcp" } else { "api" };
    forward(&live, &name, &format!("/{under}/{rest}"), req).await
}

async fn to_grimoire_mcp(State(live): State<Arc<Live>>, UrlPath(name): UrlPath<String>, req: Request) -> Response {
    forward(&live, &name, "/mcp", req).await
}

/// Hand `req` to the grimoire `name`'s routes at `path`, its query kept. The
/// request is made afresh from its method, headers and body: the library's own
/// route left its path parameters on it, which the grimoire's routes would read
/// as theirs, and the grimoire's trust step admits the caller again itself.
async fn forward(live: &Live, name: &str, path: &str, req: Request<Body>) -> Response {
    let Some(routes) = live.routes_of(name) else {
        return (StatusCode::NOT_FOUND, Json(serde_json::json!({ "error": "not_found", "message": format!("this daemon's library holds no grimoire `{name}`") }))).into_response();
    };
    let target = match req.uri().query() {
        Some(q) => format!("{path}?{q}"),
        None => path.to_string(),
    };
    let (parts, body) = req.into_parts();
    let mut fresh = Request::new(body);
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
        let under_way = live.routes_of("ledger").unwrap();
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
}
