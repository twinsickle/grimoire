//! # Module contract
//!
//! ```json
//! {"purpose":"Open a grimoire for the library to serve, and let it go again: under its store's lock, alone, beside another checkout of a store of its own, or on a team's server holding intent only, watched and marked as served.","criteria":[{"id":"let-go","statement":"Letting a grimoire go takes it off the routes first, then waits for the requests already being answered by it to finish, up to a bound, before removing its marker and releasing its lock; a checkout of a store of its own whose other checkouts are still served leaves the marker, and the lock is released once the last of them lets it go."}]}
//! ```

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{Duration, Instant};

use super::{canonical, same, Grimoire, Live};
use crate::access::{self, Policy};
use crate::daemon::Daemon;
use crate::store_io::{self, StoreLock};
use crate::{remote, watch};

/// How long letting a grimoire go waits for the requests it is answering.
const DRAIN: Duration = Duration::from_secs(10);

type Watcher = notify_debouncer_full::Debouncer<notify::RecommendedWatcher, notify_debouncer_full::RecommendedCache>;

/// A grimoire the library serves, with what it holds while it is served:
/// none of it for the default, whose lock and watcher `serve` holds itself.
pub(super) struct Shelved {
    pub(super) store_dir: PathBuf,
    /// The checkout whose code it is joined with, and that checkout as a canonical path, for choosing by `checkout`.
    pub(super) repo: PathBuf,
    pub(super) canon: PathBuf,
    /// The address of the team's server it is a copy of, of which a machine may hold several checkouts; none for a store of its own.
    pub(super) server: Option<String>,
    pub(super) daemon: Arc<Daemon>,
    /// The store's lock, shared by every checkout of a store of its own served beside this one.
    pub(super) lock: Option<Arc<StoreLock>>,
    pub(super) watcher: Option<Watcher>,
}

impl Shelved {
    /// Let it go, off the routes already: stop watching, wait for the requests
    /// it is answering, which hold its daemon, then unmark and unlock.
    pub(super) fn let_go(self) {
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

impl Live {
    /// Take one grimoire: its lock, its daemon, a watcher on its checkout and its
    /// marker, and its server followed when it is a copy of one. None when any of
    /// these fails, said once, to be tried again on the next refresh.
    pub(super) fn open(&self, g: &Grimoire) -> Option<Shelved> {
        if !self.on_a_machine() {
            return self.open_shared(g);
        }
        // Another checkout of a store of its own already served: a view of it beside that one.
        if g.server.is_none() {
            let first = self.held.lock().expect("library lock").served.values().find(|s| s.server.is_none() && same(&s.store_dir, &g.store_dir)).map(|s| (s.daemon.clone(), s.lock.clone()));
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
    pub(super) fn open_alone(&self, g: &Grimoire) -> Result<Shelved, String> {
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
        Ok(Shelved { store_dir: g.store_dir.clone(), repo: g.repo.clone(), canon: canonical(&g.repo), server: g.server.clone(), daemon, lock: Some(Arc::new(lock)), watcher: Some(watcher) })
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
        Some(Shelved { store_dir: g.store_dir.clone(), repo: g.repo.clone(), canon: canonical(&g.repo), server: None, daemon, lock, watcher: Some(watcher) })
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
        Some(Shelved { store_dir: g.store_dir.clone(), repo: g.repo.clone(), canon: canonical(&g.repo), server: None, daemon, lock: Some(Arc::new(lock)), watcher: None })
    }

    /// Who may call a grimoire: the machine's one policy, or on a team's server its own tokens.
    pub(super) fn policy_for(&self, store_dir: &Path) -> Arc<dyn Policy> {
        match &self.policy {
            Some(p) => p.clone(),
            None => Arc::new(access::Tokens { store_dir: store_dir.to_path_buf() }),
        }
    }
}
