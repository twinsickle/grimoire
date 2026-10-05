//! # Module contract
//!
//! ```json
//! {"purpose":"Keep a library served while the daemon runs, a machine's or a team's server's: open and mark each grimoire it holds, find them again every few seconds, serve a checkout a request names, and let one go when it goes or while a command deletes its store; routing each request to its checkout is the module beneath.","criteria":[{"id":"current","statement":"refresh opens every grimoire library::find returns, or on a team's server library::find_shared, that is not served or held back, under its lock and marked at /g/<name> where the daemon listens, on a machine watched and following its server when it is a copy of one, on a team's server holding intent only and admitting callers by its own tokens, and lets go of every served grimoire, but the default, that find no longer returns by the same name at the same store with the same checkout; one that could not be taken or opened is tried again on the next refresh; after any change every served grimoire is told the whole library, and once stopped, refresh takes nothing."},{"id":"checkout-registered","statement":"A request whose checkout names a directory whose nearest project file names this grimoire, in a checkout not served yet, is answered by that checkout once it is served: one of a store of its own is added to the store's checkouts through the store's writer and served as a view beside the others, sharing their lock; one of a team's grimoire this daemon serves, the same server's, has its copy of the server's intent made and is served, and when that cannot be done the request is refused saying why and what to run there; none is taken for a caller the grimoire's trust step would refuse; one at a time with refresh and with any other such request, so a checkout is opened once, and never while its store is held back; a directory whose project file names another store, a server or no grimoire of that name is refused as before, and nothing is added."},{"id":"let-go","statement":"Letting a grimoire go takes it off the routes first, then waits for the requests already being answered by it to finish, up to a bound, before removing its marker and releasing its lock; a checkout of a store of its own whose other checkouts are still served leaves the marker, and the lock is released once the last of them lets it go."},{"id":"release","statement":"release lets go of the served grimoire whose store is the one given, every checkout of it, and holds that store back from refresh until restore is called for it or the store is gone; restore lets refresh take it again; release answers false for a store the default grimoire is a checkout of, which it never lets go, and for a store it does not serve."}]}
//! ```

use std::collections::{BTreeMap, BTreeSet};
use std::net::SocketAddr;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use grimoire_core::answer::LibraryEntry;

use super::{find, Grimoire};
use crate::access::{self, Policy};
use crate::daemon::Daemon;
use crate::store_io::{self, StoreLock};
use crate::{remote, watch};

mod routing;

pub use routing::{bare, routes};

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
    /// The store's lock, shared by every checkout of a store of its own served beside this one.
    lock: Option<Arc<StoreLock>>,
    watcher: Option<Watcher>,
}

impl Shelved {
    /// Let it go, off the routes already: stop watching, wait for the requests
    /// it is answering, which hold its daemon, then unmark and unlock.
    fn let_go(self) {
        let Shelved { daemon, lock, watcher, .. } = self;
        drop(watcher);
        let deadline = Instant::now() + DRAIN;
        // Ours is the one strong reference left once no request holds the daemon.
        while Arc::strong_count(&daemon) > 1 && Instant::now() < deadline {
            std::thread::sleep(Duration::from_millis(20));
        }
        // The store's other checkouts still serve it: its marker stays, as its lock does while they hold it.
        if !daemon.serves_beside_others() {
            daemon.unmark_serving();
        }
        drop(lock);
    }
}

/// Whether two checkouts are one: equal as given, two empty paths, as a team's server's are,
/// among them, or the same folder however spelt.
fn same_checkout(a: &Path, b: &Path) -> bool {
    a == b || same(a, b)
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
        let mut held = Held::default();
        let shelved = Shelved { store_dir: default.store_dir.clone(), repo: default.repo.clone(), canon: canonical(&default.repo), team: default.server.is_some(), daemon, lock: None, watcher: None };
        held.served.insert((default.name.clone(), default.repo.clone()), shelved);
        let addr = SocketAddr::from(([127, 0, 0, 1], port));
        let live = Arc::new(Live {
            default: Some(default),
            source: Source::Machine { stores, copies },
            addr,
            policy: Some(policy),
            held: Mutex::new(held),
            refreshing: Mutex::new(()),
            stopped: AtomicBool::new(false),
        });
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

    /// What a grimoire is served from, as the log names it: a checkout on a machine,
    /// whose code it is joined with; its store on a team's server, which holds no checkouts.
    fn served_from<'a>(&self, g: &'a Grimoire) -> &'a Path {
        if self.on_a_machine() {
            &g.repo
        } else {
            &g.store_dir
        }
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
                .filter(|(key, s)| !self.is_default(key) && !found.grimoires.iter().any(|g| g.name == key.0 && same(&g.store_dir, &s.store_dir) && same_checkout(&g.repo, &s.repo)))
                .map(|(key, _)| key.clone())
                .collect();
            let gone: Vec<(String, Shelved)> = keys.into_iter().filter_map(|k| held.served.remove(&k).map(|s| (k.0, s))).collect();
            let wanted: Vec<Grimoire> = found
                .grimoires
                .into_iter()
                .filter(|g| !held.served.values().any(|s| same(&s.store_dir, &g.store_dir) && same_checkout(&s.repo, &g.repo)) && !held.held_back.iter().any(|r| same(r, &g.store_dir)))
                .collect();
            (gone, wanted)
        };
        let mut changed = !gone.is_empty();
        for (name, s) in gone {
            let store_dir = s.store_dir.clone();
            s.let_go();
            if self.on_a_machine() {
                tracing::info!("library: let go of `{name}`, which this machine's checkouts no longer name");
            } else {
                tracing::info!("library: let go of `{name}`: {} is no longer a store of the library by that name", store_dir.display());
            }
        }
        // Opened without the library held, since opening reads a checkout's code.
        for g in wanted {
            let Some(shelved) = self.open(&g) else { continue };
            let mut held = self.held.lock().expect("library lock");
            if self.stopped.load(Ordering::SeqCst)
                || held.served.values().any(|s| same(&s.store_dir, &g.store_dir) && same_checkout(&s.repo, &g.repo))
                || held.held_back.iter().any(|r| same(r, &g.store_dir))
            {
                drop(held);
                shelved.let_go();
                continue;
            }
            tracing::info!("library: serving `{}` from {} at /g/{}", g.name, self.served_from(&g).display(), g.name);
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
        // Another checkout of a store of its own already served: a view of it beside that one.
        if g.server.is_none() {
            let first = self.held.lock().expect("library lock").served.values().find(|s| !s.team && same(&s.store_dir, &g.store_dir)).map(|s| (s.daemon.clone(), s.lock.clone()));
            if let Some((first, lock)) = first {
                return self.open_beside(g, &first, lock);
            }
        }
        match self.open_alone(g) {
            Ok(shelved) => Some(shelved),
            Err(why) => {
                self.say(&g.name, &format!("not served: {why}"));
                None
            }
        }
    }

    /// Take a grimoire no other checkout of which is served: its lock, its daemon, a watcher on
    /// its checkout, its marker, and its server followed when it is a copy of one; or why not.
    fn open_alone(&self, g: &Grimoire) -> Result<Shelved, String> {
        let lock = match &g.server {
            Some(_) => remote::lock_copy(&g.store_dir),
            None => store_io::lock(&g.store_dir),
        };
        let lock = lock.map_err(|e| format!("another process holds it: {e}"))?;
        let opened = match &g.server {
            Some(address) => store_io::credentials_file()
                .map_err(|e| e.to_string())
                .and_then(|credentials| Daemon::open_remote(&g.store_dir, &g.repo, remote::Server::signed_in(address, &credentials)).map_err(|e| e.to_string())),
            None => Daemon::open(&g.store_dir).map_err(|e| e.to_string()),
        };
        let (daemon, errors) = opened.map_err(|e| format!("it could not be opened: {e}"))?;
        for e in &errors {
            self.say(&g.name, &e.to_string());
        }
        let daemon = Arc::new(daemon);
        // A grimoire nobody watches would answer from a graph that goes stale: not served at all.
        let watcher = watch::start(daemon.clone()).map_err(|e| format!("its checkout could not be watched: {e}"))?;
        // Marked before it is routed: a grimoire commands cannot find would leave them refused by its lock.
        daemon.mark_serving_in(self.addr, &format!("/g/{}", g.name)).map_err(|e| format!("its store could not be marked as served: {e}"))?;
        if g.server.is_some() {
            remote::follow(&daemon);
        }
        Ok(Shelved { store_dir: g.store_dir.clone(), repo: g.repo.clone(), canon: canonical(&g.repo), team: g.server.is_some(), daemon, lock: Some(Arc::new(lock)), watcher: Some(watcher) })
    }

    /// Take another checkout of a store this library serves already: its view of the store beside
    /// the one served, sharing that one's lock and taking its writes in turn, watched and marked.
    fn open_beside(&self, g: &Grimoire, first: &Arc<Daemon>, lock: Option<Arc<StoreLock>>) -> Option<Shelved> {
        let repository = match store_io::repository_string(&g.repo) {
            Ok(r) => r,
            Err(e) => {
                self.say(&g.name, &format!("the checkout {} is not served: {e}", g.repo.display()));
                return None;
            }
        };
        let daemon = match first.sibling(&repository) {
            Ok((daemon, errors)) => {
                for e in &errors {
                    self.say(&g.name, &e.to_string());
                }
                daemon
            }
            Err(e) => {
                self.say(&g.name, &format!("the checkout {repository} is not served: it could not be opened: {e}"));
                return None;
            }
        };
        let watcher = match watch::start(daemon.clone()) {
            Ok(w) => w,
            Err(e) => {
                self.say(&g.name, &format!("the checkout {repository} is not served: it could not be watched: {e}"));
                return None;
            }
        };
        Some(Shelved { store_dir: g.store_dir.clone(), repo: g.repo.clone(), canon: canonical(&g.repo), team: false, daemon, lock, watcher: Some(watcher) })
    }

    /// Serve the checkout `dir` is in, by its nearest project file, when that file names the
    /// grimoire `name` and the checkout is not served yet: a checkout of a store of its own is
    /// added to the store's checkouts and served beside the others; one of a team's grimoire has
    /// its copy of the server's intent made, as a command run there would, and is served. Whether
    /// it is served now, or why it could not be.
    fn take_checkout(&self, name: &str, dir: &str) -> Took {
        // One at a time with refresh and with each other, so a checkout is never opened twice.
        let _one = self.refreshing.lock().expect("refresh lock");
        if self.stopped.load(Ordering::SeqCst) {
            return Took::NotOurs;
        }
        let Ok((repo, home, named)) = store_io::find_named(Path::new(dir)) else { return Took::NotOurs };
        if named != name {
            return Took::NotOurs;
        }
        let g = match home {
            store_io::Home::Store(store_dir) => {
                let first = self.held.lock().expect("library lock").served.iter().find(|((n, _), s)| n == name && !s.team && same(&s.store_dir, &store_dir)).map(|(_, s)| s.daemon.clone());
                let Some(first) = first else { return Took::NotOurs };
                match first.register_checkout(&repo) {
                    Ok(true) => tracing::info!("library: `{name}`: added the checkout {} to its store", repo.display()),
                    Ok(false) => {}
                    Err(e) => return Took::Failed(format!("this checkout, {}, of the grimoire `{name}` could not be added to its store: {e}", repo.display())),
                }
                Grimoire { name: name.to_string(), store_dir, repo: repo.clone(), server: None }
            }
            // A worktree of a team's grimoire this daemon serves: its own copy of the same server's intent.
            store_io::Home::Server { address, copy } => {
                let ours = self.held.lock().expect("library lock").served.iter().any(|((n, _), s)| n == name && s.daemon.server.as_ref().is_some_and(|srv| srv.address == address));
                if !ours {
                    return Took::NotOurs;
                }
                Grimoire { name: name.to_string(), store_dir: copy, repo: repo.clone(), server: Some(address) }
            }
        };
        if self.held.lock().expect("library lock").served.values().any(|s| same(&s.store_dir, &g.store_dir) && same_checkout(&s.repo, &repo)) {
            return Took::Served;
        }
        let shelved = match g.server {
            Some(_) => match self.open_alone(&g) {
                Ok(shelved) => shelved,
                Err(why) => {
                    return Took::Failed(format!(
                        "this checkout, {}, of the grimoire `{name}` is not served: its copy of the server's intent could not be made: {why}. Run `grimoire check` in it, after `grimoire login` if this machine is not signed in",
                        repo.display()
                    ))
                }
            },
            None => match self.open(&g) {
                Some(shelved) => shelved,
                None => return Took::Failed(format!("this checkout, {}, of the grimoire `{name}` could not be served; the daemon's log says why", repo.display())),
            },
        };
        let mut held = self.held.lock().expect("library lock");
        if self.stopped.load(Ordering::SeqCst) || held.held_back.iter().any(|r| same(r, &g.store_dir)) {
            drop(held);
            shelved.let_go();
            return Took::NotOurs;
        }
        tracing::info!("library: serving `{name}` from {} at /g/{name}", repo.display());
        held.served.insert((name.to_string(), repo), shelved);
        drop(held);
        self.tell();
        Took::Served
    }

    /// The policy the grimoire `name`'s trust step asks, or none when the library holds no grimoire by that name.
    fn policy_of(&self, name: &str) -> Option<Arc<dyn Policy>> {
        let store_dir = self.held.lock().expect("library lock").served.iter().find(|((n, _), _)| n == name).map(|(_, s)| s.store_dir.clone());
        store_dir.map(|dir| self.policy_for(&dir))
    }

    /// The daemon of the checkout served under `key`, if it is served still.
    fn daemon_at(&self, key: &Key) -> Option<Arc<Daemon>> {
        self.held.lock().expect("library lock").served.get(key).map(|s| s.daemon.clone())
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
        Some(Shelved { store_dir: g.store_dir.clone(), repo: g.repo.clone(), canon: canonical(&g.repo), team: false, daemon, lock: Some(Arc::new(lock)), watcher: None })
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
            // Every checkout of the store goes, or none: the default's store is never let go.
            let keys: Vec<Key> = held.served.iter().filter(|(_, s)| same(&s.store_dir, store_dir)).map(|(key, _)| key.clone()).collect();
            if keys.is_empty() || keys.iter().any(|k| self.is_default(k)) {
                return false;
            }
            let taken: Vec<(String, Shelved)> = keys.into_iter().filter_map(|k| held.served.remove(&k).map(|s| (k.0, s))).collect();
            if let Some((_, s)) = taken.first() {
                held.held_back.insert(s.store_dir.clone());
            }
            taken
        };
        let Some(name) = taken.first().map(|(n, _)| n.clone()) else { return false };
        for (_, s) in taken {
            s.let_go();
        }
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
}

/// What became of a checkout a call named that was not served.
enum Took {
    /// Served now, or already.
    Served,
    /// No checkout of this grimoire this daemon could serve: the call is refused as naming none.
    NotOurs,
    /// A checkout of this grimoire that could not be served, and why.
    Failed(String),
}

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

#[cfg(test)]
mod tests {
    use super::*;
    use grimoire_core::model::{Roots, Scope, StoreConfig};

    /// A checkout at `root/<name>` with a store at `root/stores/<name>` that its project file names.
    pub(super) fn project(root: &Path, name: &str) -> Grimoire {
        let repo = root.join(name);
        std::fs::create_dir_all(repo.join("src")).unwrap();
        std::fs::write(repo.join("src").join("lib.rs"), "pub fn a() {}\n").unwrap();
        let store_dir = root.join("stores").join(name);
        let config = StoreConfig {
            site: vec![],
            version: StoreConfig::CURRENT_VERSION,
            repository: store_io::repository_string(&repo).unwrap(),
            roots: Roots::new("overview", "rites"),
            languages: vec!["rust".into()],
            scope: Scope::default(),
            order: Default::default(),
        };
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
        // A request under way holds the daemon answering it.
        let under_way = live.daemon_of("ledger").unwrap();
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

    /// # Test contract
    ///
    /// ```json
    /// {"test": "a-teams-server-keeps-serving-what-it-serves", "checks": ["crates/server/src/library/live#current"]}
    /// ```
    #[test]
    fn a_teams_server_keeps_serving_what_it_serves() {
        // A team's library: its grimoires have no checkout, so nothing about one changes between refreshes.
        let tmp = tempfile::tempdir().unwrap();
        let shop = project(tmp.path(), "shop");
        let live = Live::new_shared(shop.store_dir.parent().unwrap().to_path_buf(), SocketAddr::from(([127, 0, 0, 1], 4299)));
        live.refresh();
        let first = live.daemon_of("shop").expect("served");
        for _ in 0..3 {
            live.refresh();
        }
        assert!(Arc::ptr_eq(&first, &live.daemon_of("shop").unwrap()), "the same daemon serves it throughout: never let go and taken again");
        live.stop();
    }

    #[test]
    fn the_log_names_what_a_grimoire_is_served_from() {
        // A team's server holds no checkouts, so it names the store; a machine names the checkout.
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path();
        let (shop, ledger) = (project(root, "shop"), project(root, "ledger"));
        let shared = Live::new_shared(root.join("stores"), SocketAddr::from(([127, 0, 0, 1], 4299)));
        let on_server = Grimoire { repo: PathBuf::new(), ..ledger.clone() };
        assert_eq!(shared.served_from(&on_server), ledger.store_dir.as_path());
        let _shop_lock = store_io::lock(&shop.store_dir).unwrap();
        let daemon = Arc::new(Daemon::open(&shop.store_dir).unwrap().0);
        let machine = Live::new(shop.clone(), daemon, 4299, Arc::new(access::Local), root.join("stores"), root.join("copies"));
        assert_eq!(machine.served_from(&ledger), ledger.repo.as_path());
    }
}
