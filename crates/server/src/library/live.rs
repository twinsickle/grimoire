//! # Module contract
//!
//! ```json
//! {"purpose":"Keep a library served while the daemon runs, a machine's or a team's server's: find its grimoires again every few seconds, serve each one found and let one go when it goes or while a command deletes its store, and tell each the whole library; opening a grimoire and letting it go is the module opening beneath, and routing each request to its checkout, serving one a request names, is the module routing.","criteria":[{"id":"opens-found-grimoires","statement":"refresh opens every grimoire library::find returns, or on a team's server library::find_shared, that is not served or held back."},{"id":"opened-under-its-lock","statement":"refresh opens each grimoire under its lock."},{"id":"marked-where-the-daemon-listens","statement":"refresh marks each grimoire it opens as served at /g/<name> where the daemon listens."},{"id":"machine-grimoire-watched","statement":"On a machine, refresh opens each grimoire watched."},{"id":"copy-follows-its-server","statement":"On a machine, refresh opens a grimoire that is a copy of a server's intent following that server."},{"id":"team-server-holds-intent-only","statement":"On a team's server, refresh opens each grimoire holding intent only."},{"id":"team-server-own-tokens","statement":"On a team's server, refresh opens each grimoire admitting callers by its own tokens."},{"id":"vanished-grimoire-let-go","statement":"refresh lets go of every served grimoire, but the default, that library::find no longer returns by the same name at the same store with the same checkout."},{"id":"failed-open-retried","statement":"A grimoire that could not be taken or opened is tried again on the next refresh."},{"id":"library-told-after-change","statement":"After any change every served grimoire is told the whole library."},{"id":"newly-served-told-first","statement":"A grimoire newly served, by refresh or by a request naming its checkout, is told the whole library before any request reaches it."},{"id":"server-of-each-grimoire","statement":"Each grimoire is told the server it comes from as library::server_of names it, or none for a store of its own."},{"id":"stopped-takes-nothing","statement":"Once stopped, refresh takes nothing."},{"id":"waits-on-no-state","statement":"Telling the served grimoires the library and their fellows takes no served daemon's state, so while one served daemon holds its state, as a copy does across a fetch while it syncs, refresh still serves a grimoire that appeared and a request to any other grimoire is still answered."},{"id":"release","statement":"release lets go of the served grimoire whose store is the one given, every checkout of it, and holds that store back from refresh until restore is called for it or the store is gone; restore lets refresh take it again; release answers false for a store the default grimoire is a checkout of, which it never lets go, and for a store it does not serve."},{"id":"queued-store-not-taken","statement":"On a machine, refresh serves no grimoire whose store is queued for deletion in the deletions folder beside its stores."},{"id":"queued-served-store-let-go","statement":"On a machine, refresh lets go of a grimoire it serves whose store is queued for deletion in the deletions folder beside its stores."},{"id":"queue-asked-again-before-serving","statement":"On a machine, refresh asks the queue again just before it serves a grimoire it opened."},{"id":"queued-checkout-not-taken","statement":"On a machine, a checkout whose store is queued is not taken."},{"id":"queued-deletions-carried-out","statement":"On a machine, after each refresh it carries out the queued deletions."},{"id":"kept-deletion-said-once","statement":"On a machine, after each refresh it says once each queued deletion it kept and why."},{"id":"team-server-has-no-queue","statement":"A team's server has no deletions folder."}]}
//! ```

use std::collections::{BTreeMap, BTreeSet};
use std::net::SocketAddr;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use grimoire_core::answer::LibraryEntry;

use super::{find, Grimoire};
use crate::access::Policy;
use crate::daemon::Daemon;
use crate::store_io;

mod opening;
mod routing;

use opening::Shelved;

pub use routing::{bare, routes};

/// How often the library is found again while the daemon runs.
pub const REFRESH: Duration = Duration::from_secs(2);

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
        let shelved = Shelved { store_dir: default.store_dir.clone(), repo: default.repo.clone(), canon: canonical(&default.repo), server: default.server.clone(), daemon, lock: None, watcher: None };
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
        // A store whose deletion is queued is never served again: one served is let go below (req:store-deletion-queued).
        let mut found = found;
        if let Some(dir) = self.deletions() {
            found.grimoires.retain(|g| !store_io::deletions::is_queued(&dir, &g.store_dir));
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
        let gone_any = !gone.is_empty();
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
            #[cfg(test)]
            tests::after_open();
            if self.stopped.load(Ordering::SeqCst)
                || held.served.values().any(|s| same(&s.store_dir, &g.store_dir) && same_checkout(&s.repo, &g.repo))
                || held.held_back.iter().any(|r| same(r, &g.store_dir))
                || self.deletions().is_some_and(|d| store_io::deletions::is_queued(&d, &g.store_dir))
            {
                drop(held);
                shelved.let_go();
                continue;
            }
            tracing::info!("library: serving `{}` from {} at /g/{}", g.name, self.served_from(&g).display(), g.name);
            held.served.insert((g.name.clone(), g.repo.clone()), shelved);
            // Told before the lock is let go, so no request reaches a daemon that knows no library yet.
            tell(&held);
        }
        if gone_any {
            self.tell();
        }
        self.carry_out_deletions();
    }

    /// The machine's folder of queued store deletions, beside its stores; a team's server has none.
    fn deletions(&self) -> Option<PathBuf> {
        match &self.source {
            Source::Machine { stores, .. } => stores.parent().map(|data| data.join("deletions")),
            Source::Shared { .. } => None,
        }
    }

    /// Carry out the queued deletions whose stores nothing holds, each outcome said once.
    fn carry_out_deletions(&self) {
        let Some(dir) = self.deletions() else { return };
        for outcome in store_io::deletions::carry_out(&dir) {
            match outcome {
                store_io::deletions::Outcome::Deleted(store) => tracing::info!("library: removed store {}, whose deletion was queued", store.display()),
                store_io::deletions::Outcome::Named(store) => self.say(&store.display().to_string(), "its deletion stays queued while its repository still names it"),
                store_io::deletions::Outcome::Kept(store, why) => self.say(&store.display().to_string(), &format!("its queued deletion is not carried out: {why}")),
                store_io::deletions::Outcome::Held(_) => {}
            }
        }
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

    /// Tell every served grimoire the whole library, for `status`.
    fn tell(&self) {
        tell(&self.held.lock().expect("library lock"));
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

/// Tell every grimoire `held` serves the whole library, for `status`. Called with the library's lock held, so the
/// last told is the latest and a daemon is told before any route reaches it, and waiting on no daemon's state:
/// set_library and set_fellows take locks of their own, since a copy holds its state across a fetch while it
/// syncs, and every route takes the library's lock (req:machine-library).
fn tell(held: &Held) {
    let all: Vec<LibraryEntry> = held
        .served
        .iter()
        .map(|((name, _), s)| LibraryEntry {
            name: name.clone(),
            repository: store_io::repository_string(&s.daemon.repo).unwrap_or_else(|_| s.daemon.repo.display().to_string()),
            server: s.server.as_deref().map(super::server_of),
            // Each checkout's current ticket is added by its daemon as status answers, as it is then.
            current: None,
        })
        .collect();
    for ((name, _), s) in &held.served {
        // Its grimoire's other checkouts, whose current tickets status names as it answers: told before the
        // listing that names them, so no status lists a checkout whose daemon it was not told of.
        let fellows = held.served.iter().filter(|((n, _), o)| n == name && !Arc::ptr_eq(&o.daemon, &s.daemon)).map(|(_, o)| Arc::downgrade(&o.daemon)).collect();
        s.daemon.set_fellows(fellows);
        s.daemon.set_library(name, all.clone());
    }
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
    thread_local! {
        /// What a test does at the moment a grimoire has been opened, before refresh asks again whether it may be served.
        static AFTER_OPEN: std::cell::RefCell<Option<Box<dyn Fn()>>> = const { std::cell::RefCell::new(None) };
    }

    pub(super) fn after_open() {
        AFTER_OPEN.with(|hook| {
            if let Some(hook) = &*hook.borrow() {
                hook();
            }
        });
    }

    use super::*;
    use crate::access;
    use grimoire_core::model::{Roots, Scope, StoreConfig};
    use std::time::Instant;

    /// A checkout at `root/<name>` with a store at `root/stores/<name>` that its project file names.
    pub(super) fn project(root: &Path, name: &str) -> Grimoire {
        let repo = root.join(name);
        std::fs::create_dir_all(repo.join("src")).unwrap();
        std::fs::write(repo.join("src").join("lib.rs"), "pub fn a() {}\n").unwrap();
        let store_dir = root.join("stores").join(name);
        let config = StoreConfig {
            large_file: None,
            plain_file: None,
            site: vec![],
            version: StoreConfig::CURRENT_VERSION,
            repository: store_io::repository_string(&repo).unwrap(),
            roots: Roots::new("overview", "rites"),
            languages: vec!["rust".into()],
            scope: Scope::default(),
            order: Default::default(),
            recipe_kinds: Default::default(),
        };
        store_io::write_all(&store_dir, &grimoire_core::Store::new(config), false).unwrap();
        std::fs::write(repo.join(store_io::PROJECT_FILE), format!("name = \"{name}\"\nstore = {}\n", toml::Value::String(store_dir.to_string_lossy().to_string()))).unwrap();
        Grimoire { name: name.into(), store_dir, repo, server: None }
    }

    /// # Test contract
    ///
    /// ```json
    /// {"test": "letting-go-waits-for-the-requests-under-way", "checks": ["crates/server/src/library/live/opening#let-go", "crates/server/src/library/live#release", "crates/server/src/library/live#marked-where-the-daemon-listens"]}
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
    /// {"test": "a-queued-store-is-let-go-and-deleted-on-refresh", "checks": ["crates/server/src/library/live#queued-store-not-taken", "crates/server/src/library/live#queued-served-store-let-go", "crates/server/src/library/live#queued-deletions-carried-out", "req:store-deletion-queued"]}
    /// ```
    #[test]
    fn a_queued_store_is_let_go_and_deleted_on_refresh() {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path();
        let (shop, ledger) = (project(root, "shop"), project(root, "ledger"));
        let _shop_lock = store_io::lock(&shop.store_dir).unwrap();
        let daemon = Arc::new(Daemon::open(&shop.store_dir).unwrap().0);
        let live = Live::new(shop.clone(), daemon, 4299, Arc::new(access::Local), root.join("stores"), root.join("copies"));
        live.refresh();
        assert_eq!(live.names(), ["ledger", "shop"]);
        // Its deletion queued while the library holds it, as uninstall does when the lock is still held.
        let deletions = root.join("deletions");
        store_io::deletions::queue(&deletions, &ledger.store_dir, &store_io::repository_string(&ledger.repo).unwrap()).unwrap();
        // Let go at once, though its checkout still names it, as it does until uninstall removes the project file;
        // kept and left queued while it does.
        live.refresh();
        assert_eq!(live.names(), ["shop"], "a queued store is let go");
        assert!(ledger.store_dir.exists() && store_io::deletions::is_queued(&deletions, &ledger.store_dir));
        live.refresh();
        assert_eq!(live.names(), ["shop"], "and not taken again while it is queued");
        // Its checkout no longer names it: deleted on the next refresh, its record dropped.
        std::fs::remove_file(ledger.repo.join(store_io::PROJECT_FILE)).unwrap();
        live.refresh();
        assert!(!ledger.store_dir.exists(), "deleted");
        assert!(store_io::deletions::queued(&deletions).0.is_empty(), "its record dropped");
        assert_eq!(live.names(), ["shop"]);
        live.stop();
    }

    /// # Test contract
    ///
    /// ```json
    /// {"test": "a-store-pointed-at-another-checkout-is-served-with-it", "checks": ["crates/server/src/library/live#opens-found-grimoires", "crates/server/src/library/live#vanished-grimoire-let-go"]}
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
    /// {"test": "a-teams-server-keeps-serving-what-it-serves", "checks": ["crates/server/src/library/live#opens-found-grimoires"]}
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

    /// # Test contract
    ///
    /// ```json
    /// {"test": "a-busy-grimoire-stalls-no-other", "checks": ["crates/server/src/library/live#waits-on-no-state", "req:machine-library"]}
    /// ```
    #[test]
    fn a_busy_grimoire_stalls_no_other() {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path();
        let shop = project(root, "shop");
        project(root, "ledger");
        let _shop_lock = store_io::lock(&shop.store_dir).unwrap();
        let daemon = Arc::new(Daemon::open(&shop.store_dir).unwrap().0);
        let live = Live::new(shop.clone(), daemon, 4299, Arc::new(access::Local), root.join("stores"), root.join("copies"));
        live.refresh();
        assert_eq!(live.names(), ["ledger", "shop"]);
        // `ledger`'s daemon holds its state, as a copy does across a fetch while it syncs.
        let busy = live.daemon_of("ledger").unwrap();
        let held = busy.state.lock().unwrap();
        // A grimoire appears: refresh serves it and tells the library, waiting on nobody's state.
        project(root, "audit");
        let (done, finished) = std::sync::mpsc::channel();
        let refreshing = live.clone();
        std::thread::spawn(move || {
            refreshing.refresh();
            done.send(()).unwrap();
        });
        finished.recv_timeout(Duration::from_secs(20)).expect("refresh finished while a served daemon held its state");
        assert_eq!(live.names(), ["audit", "ledger", "shop"]);
        // A request to another grimoire is answered meanwhile, with the library as now told.
        let (answer, answered) = std::sync::mpsc::channel();
        let asking = live.clone();
        std::thread::spawn(move || {
            let shop = asking.daemon_of("shop").unwrap();
            answer.send(shop.call("status", serde_json::json!({})).unwrap()).unwrap();
        });
        let status = answered.recv_timeout(Duration::from_secs(20)).expect("a request to another grimoire was answered");
        let names: Vec<&str> = status["library"].as_array().unwrap().iter().map(|e| e["name"].as_str().unwrap()).collect();
        assert_eq!(names, ["audit", "ledger", "shop"]);
        // Once free, the busy one names the grimoire it was told of while it was busy.
        drop(held);
        let names = busy.call("status", serde_json::json!({})).unwrap()["library"].as_array().unwrap().iter().map(|e| e["name"].as_str().unwrap().to_string()).collect::<Vec<_>>();
        assert_eq!(names, ["audit", "ledger", "shop"]);
        drop(busy);
        live.stop();
    }

    /// # Test contract
    ///
    /// ```json
    /// {"test": "a-stopped-library-takes-nothing", "checks": ["crates/server/src/library/live#stopped-takes-nothing"]}
    /// ```
    #[test]
    fn a_stopped_library_takes_nothing() {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path();
        let (shop, _ledger) = (project(root, "shop"), project(root, "ledger"));
        let _shop_lock = store_io::lock(&shop.store_dir).unwrap();
        let daemon = Arc::new(Daemon::open(&shop.store_dir).unwrap().0);
        let live = Live::new(shop.clone(), daemon, 4299, Arc::new(access::Local), root.join("stores"), root.join("copies"));
        live.refresh();
        assert_eq!(live.names(), ["ledger", "shop"]);
        live.stop();
        // A grimoire that appears after the stop is not taken, nor marked.
        let audit = project(root, "audit");
        live.refresh();
        assert_eq!(live.names(), ["ledger", "shop"]);
        assert!(store_io::read_serving(&audit.store_dir).is_none() && !audit.store_dir.join(store_io::LOCK_FILE).exists(), "not opened, locked or marked");
        // Nor does one that went get let go: stopped, refresh does nothing at all.
        std::fs::remove_file(root.join("ledger").join(store_io::PROJECT_FILE)).unwrap();
        live.refresh();
        assert_eq!(live.names(), ["ledger", "shop"]);
    }

    /// # Test contract
    ///
    /// ```json
    /// {"test": "a-grimoire-that-could-not-be-taken-is-tried-again", "checks": ["crates/server/src/library/live#failed-open-retried"]}
    /// ```
    #[test]
    fn a_grimoire_that_could_not_be_taken_is_tried_again() {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path();
        let (shop, ledger) = (project(root, "shop"), project(root, "ledger"));
        let _shop_lock = store_io::lock(&shop.store_dir).unwrap();
        let daemon = Arc::new(Daemon::open(&shop.store_dir).unwrap().0);
        let live = Live::new(shop.clone(), daemon, 4299, Arc::new(access::Local), root.join("stores"), root.join("copies"));
        // Another process holds its store: it cannot be taken, and the others are served.
        let held = store_io::lock(&ledger.store_dir).unwrap();
        live.refresh();
        assert_eq!(live.names(), ["shop"]);
        live.refresh();
        assert_eq!(live.names(), ["shop"], "still held, still not taken");
        // Let go, the next refresh takes it, with nothing else done.
        drop(held);
        live.refresh();
        assert_eq!(live.names(), ["ledger", "shop"]);
        live.stop();
    }

    /// # Test contract
    ///
    /// ```json
    /// {"test": "a-teams-server-holds-intent-only-and-has-no-queue", "checks": ["crates/server/src/library/live#team-server-holds-intent-only", "crates/server/src/library/live#team-server-has-no-queue"]}
    /// ```
    #[test]
    fn a_teams_server_holds_intent_only_and_has_no_queue() {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path();
        let shop = project(root, "shop");
        let live = Live::new_shared(root.join("stores"), SocketAddr::from(([127, 0, 0, 1], 4299)));
        // A deletion queued for the store where a machine keeps its queue: a team's server does not look there.
        let queue = root.join("deletions");
        store_io::deletions::queue(&queue, &shop.store_dir, &store_io::repository_string(&shop.repo).unwrap()).unwrap();
        assert!(live.deletions().is_none(), "there is no deletions folder");
        live.refresh();
        assert_eq!(live.names(), ["shop"], "served whatever a queue says");
        assert!(shop.store_dir.join(store_io::CONFIG_FILE).is_file() && store_io::deletions::is_queued(&queue, &shop.store_dir), "and nothing deleted or dropped");
        // Intent only: no code held, nothing watched.
        let served = live.held.lock().unwrap();
        assert!(served.served.values().all(|s| s.watcher.is_none()), "nothing is watched");
        drop(served);
        let daemon = live.daemon_of("shop").unwrap();
        assert_eq!(daemon.call("lint", serde_json::json!({})).unwrap()["without_code"], true, "judged without code");
        assert_eq!(daemon.call("status", serde_json::json!({})).unwrap()["shared"], true);
        live.stop();
    }

    /// Every event any test logs, as its message. Installed for the whole process, since a scoped subscriber
    /// is not seen by a callsite another test thread registered first.
    static EVENTS: Mutex<Vec<String>> = Mutex::new(Vec::new());

    struct Capture;

    fn capture_events() {
        static ONCE: std::sync::Once = std::sync::Once::new();
        ONCE.call_once(|| {
            let _ = tracing::subscriber::set_global_default(Capture);
        });
    }

    impl tracing::Subscriber for Capture {
        fn enabled(&self, _: &tracing::Metadata<'_>) -> bool {
            true
        }
        fn new_span(&self, _: &tracing::span::Attributes<'_>) -> tracing::span::Id {
            tracing::span::Id::from_u64(1)
        }
        fn record(&self, _: &tracing::span::Id, _: &tracing::span::Record<'_>) {}
        fn record_follows_from(&self, _: &tracing::span::Id, _: &tracing::span::Id) {}
        fn event(&self, event: &tracing::Event<'_>) {
            struct Message(String);
            impl tracing::field::Visit for Message {
                fn record_debug(&mut self, field: &tracing::field::Field, value: &dyn std::fmt::Debug) {
                    if field.name() == "message" {
                        self.0 = format!("{value:?}");
                    }
                }
            }
            let mut message = Message(String::new());
            event.record(&mut message);
            EVENTS.lock().unwrap().push(message.0);
        }
        fn enter(&self, _: &tracing::span::Id) {}
        fn exit(&self, _: &tracing::span::Id) {}
    }

    /// # Test contract
    ///
    /// ```json
    /// {"test": "a-kept-deletion-is-said-once-with-why", "checks": ["crates/server/src/library/live#kept-deletion-said-once"]}
    /// ```
    #[test]
    fn a_kept_deletion_is_said_once_with_why() {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path();
        let shop = project(root, "shop");
        let _shop_lock = store_io::lock(&shop.store_dir).unwrap();
        let daemon = Arc::new(Daemon::open(&shop.store_dir).unwrap().0);
        let live = Live::new(shop.clone(), daemon, 4299, Arc::new(access::Local), root.join("stores"), root.join("copies"));
        // A store outside the library that holds a file a store does not: its deletion is kept.
        let foreign = project(&root.join("elsewhere"), "foreign");
        std::fs::remove_file(foreign.repo.join(store_io::PROJECT_FILE)).unwrap();
        std::fs::write(foreign.store_dir.join("notes.txt"), "mine").unwrap();
        store_io::deletions::queue(&root.join("deletions"), &foreign.store_dir, &store_io::repository_string(&foreign.repo).unwrap()).unwrap();
        capture_events();
        for _ in 0..3 {
            live.refresh();
        }
        let ours = foreign.store_dir.display().to_string();
        let said: Vec<String> = EVENTS.lock().unwrap().iter().filter(|e| e.contains("its queued deletion is not carried out") && e.contains(&ours)).cloned().collect();
        assert_eq!(said.len(), 1, "said once, however many refreshes: {said:?}");
        assert!(said[0].contains("notes.txt") && said[0].contains("foreign"), "with why: {said:?}");
        assert!(foreign.store_dir.join(store_io::CONFIG_FILE).is_file() && store_io::deletions::is_queued(&root.join("deletions"), &foreign.store_dir), "kept, and still queued");
        live.stop();
    }

    /// # Test contract
    ///
    /// ```json
    /// {"test": "a-new-grimoire-is-told-the-library-before-any-request", "checks": ["crates/server/src/library/live#newly-served-told-first"]}
    /// ```
    #[test]
    fn a_new_grimoire_is_told_the_library_before_any_request() {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path();
        let (shop, _ledger) = (project(root, "shop"), project(root, "ledger"));
        let _shop_lock = store_io::lock(&shop.store_dir).unwrap();
        let daemon = Arc::new(Daemon::open(&shop.store_dir).unwrap().0);
        let live = Live::new(shop.clone(), daemon, 4299, Arc::new(access::Local), root.join("stores"), root.join("copies"));
        live.refresh();
        project(root, "audit");
        // A request reaches a grimoire the moment the routes find it: as soon as it does, it is asked its status.
        let seen = Arc::new(Mutex::new(None));
        let (watching, found) = (live.clone(), seen.clone());
        let poller = std::thread::spawn(move || {
            let deadline = Instant::now() + Duration::from_secs(30);
            while Instant::now() < deadline {
                if let Some(daemon) = watching.daemon_of("audit") {
                    let status = daemon.call("status", serde_json::json!({})).unwrap();
                    let names: Vec<String> = status["library"].as_array().map(|l| l.iter().map(|e| e["name"].as_str().unwrap().to_string()).collect()).unwrap_or_default();
                    *found.lock().unwrap() = Some(names);
                    return;
                }
            }
        });
        live.refresh();
        poller.join().unwrap();
        assert_eq!(*seen.lock().unwrap(), Some(vec!["audit".to_string(), "ledger".to_string(), "shop".to_string()]), "the first request it answered named the whole library");
        live.stop();
    }

    /// # Test contract
    ///
    /// ```json
    /// {"test": "a-grimoire-queued-while-it-is-opened-is-not-served", "checks": ["crates/server/src/library/live#queue-asked-again-before-serving"]}
    /// ```
    #[test]
    fn a_grimoire_queued_while_it_is_opened_is_not_served() {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path();
        let (shop, ledger) = (project(root, "shop"), project(root, "ledger"));
        let _shop_lock = store_io::lock(&shop.store_dir).unwrap();
        let daemon = Arc::new(Daemon::open(&shop.store_dir).unwrap().0);
        let live = Live::new(shop.clone(), daemon, 4299, Arc::new(access::Local), root.join("stores"), root.join("copies"));
        let (store, repository, queue) = (ledger.store_dir.clone(), store_io::repository_string(&ledger.repo).unwrap(), root.join("deletions"));
        // Queued the moment the grimoire is opened: after refresh asked the queue, before it serves the grimoire.
        AFTER_OPEN.with(|hook| *hook.borrow_mut() = Some(Box::new(move || drop(store_io::deletions::queue(&queue, &store, &repository).unwrap()))));
        live.refresh();
        AFTER_OPEN.with(|hook| *hook.borrow_mut() = None);
        assert_eq!(live.names(), ["shop"], "asked again just before serving, it was not served");
        assert!(store_io::read_serving(&ledger.store_dir).is_none(), "and let go: no marker");
        drop(store_io::lock(&ledger.store_dir).expect("and its lock is free"));
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
