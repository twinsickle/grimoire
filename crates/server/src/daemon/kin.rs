//! # Module contract
//!
//! ```json
//! {"purpose":"Serve one store of its own for its several checkouts side by side, each daemon a view of it with that checkout's code and current ticket, their writes taken one at a time and each handed to the others.","criteria":[{"id":"kin-open-for","statement":"open_for opens a store of its own as one of its checkouts sees it, its repository and its current ticket that checkout's, extracting that checkout's code."},{"id":"kin-sibling","statement":"sibling opens another checkout's view of the same store beside a daemon already serving it, taking no lock of its own."},{"id":"kin-writes-handed-on","statement":"The daemons serving one store take its writes one at a time, and each write, a pruned baseline included, is handed at once to every other, which keeps its own repository and its current ticket."},{"id":"kin-current-cleared","statement":"A daemon handed a write clears and persists its current ticket when the ticket it named is no longer being worked."},{"id":"kin-main-prunes","statement":"After another checkout's write, the main checkout's daemon prunes the baseline and hands it on."},{"id":"checkout-tickets","statement":"set_library keeps the library a daemon is told of apart from its state, taking no lock the daemon's work holds; before answering status, a daemon tells its state that library and the current ticket of every other checkout of its grimoire, those sharing its store and those its library told it of with set_fellows, a team's checkouts each holding a copy of their own, as that checkout's daemon holds it then, never waiting on a daemon busy with a write or a sync, whose ticket stays as last read, so status names each checkout's as it is and answers at once."}]}
//! ```

use std::path::Path;
use std::sync::{Arc, Mutex, Weak};

use grimoire_core::State;

use super::{load_for, Daemon, Event, OpenError};
use crate::store_io::{self, StoreIoError};

/// The daemons serving one store of its own, one per checkout: their writes are taken one at a
/// time, and each is handed to the rest, so every checkout reads one intent.
#[derive(Default)]
pub(crate) struct Kin {
    pub(super) write: Mutex<()>,
    members: Mutex<Vec<Weak<Daemon>>>,
}

impl Daemon {
    /// Open `checkout`'s view of the store this daemon serves, beside it: the same store, under the
    /// lock this daemon's holder took, that checkout's code and current ticket, its writes taken one
    /// at a time with this daemon's and handed to each other (req:store-checkouts).
    pub fn sibling(self: &Arc<Daemon>, checkout: &str) -> Result<(Arc<Daemon>, Vec<grimoire_extract::ExtractError>), OpenError> {
        // Read under the write lock, so no write of a sibling's falls between the read and the joining.
        let _one = self.kin.write.lock().expect("kin write lock");
        let loaded = load_for(&self.store_dir, Some(checkout))?;
        let main = store_io::checkouts::main_of(&self.store_dir).is_ok_and(|main| main == checkout);
        let sibling = Arc::new(Daemon::from_loaded(&self.store_dir, loaded.repo, loaded.extractor, loaded.files, loaded.state, self.kin.clone(), main));
        // Several checkouts share the store now: a ticket one works can be taken up in another.
        self.state.lock().expect("state lock").set_shares_checkouts(true);
        sibling.state.lock().expect("state lock").set_shares_checkouts(true);
        let mut members = self.kin.members.lock().expect("kin members lock");
        members.retain(|m| m.strong_count() > 0);
        if !members.iter().any(|m| m.as_ptr() == Arc::as_ptr(self)) {
            members.push(Arc::downgrade(self));
        }
        members.push(Arc::downgrade(&sibling));
        Ok((sibling, loaded.extract_errors))
    }

    /// Whether another checkout's daemon still serves this store beside this one.
    pub fn serves_beside_others(&self) -> bool {
        !self.kin_but_me().is_empty()
    }

    /// Add the checkout at `repo` to this store of its own's checkouts, as its writer, its writes
    /// taken in turn with its other checkouts': whether it was not one yet (req:store-checkouts).
    pub fn register_checkout(&self, repo: &Path) -> Result<bool, StoreIoError> {
        let _one = self.kin.write.lock().expect("kin write lock");
        let added = store_io::checkouts::register(&self.store_dir, repo)?;
        if added {
            // Several checkouts share the store now: a ticket one works can be taken up in another.
            self.state.lock().expect("state lock").set_shares_checkouts(true);
            // A write to the store, announced as every one is.
            for member in std::iter::once(self).chain(self.kin_but_me().iter().map(|m| m.as_ref())) {
                let _ = member.events.send(Event::Store { tool: "checkout".to_string(), changed: vec!["checkouts".to_string()] });
            }
        }
        Ok(added)
    }

    /// The other daemons serving this store, alive.
    fn kin_but_me(&self) -> Vec<Arc<Daemon>> {
        self.kin.members.lock().expect("kin members lock").iter().filter_map(Weak::upgrade).filter(|m| !std::ptr::eq(Arc::as_ptr(m), self)).collect()
    }

    /// Tell this daemon's state, for `status`, the library as set_library last gave it, and the current
    /// ticket of every other checkout of its grimoire, its kin and the fellows its library told it of, as
    /// each holds it now. Each other state is tried alone and never waited for: a fellow mid-sync holds its
    /// state across a fetch, and status answers always, so one that is busy keeps the ticket it was last read with.
    pub(super) fn tell_library(&self) {
        let library = self.library.lock().expect("library lock").clone();
        let mut others = self.kin_but_me();
        let fellows: Vec<Arc<Daemon>> = self.fellows.lock().expect("fellows lock").iter().filter_map(Weak::upgrade).collect();
        for f in fellows {
            if !std::ptr::eq(Arc::as_ptr(&f), self) && !others.iter().any(|o| Arc::ptr_eq(o, &f)) {
                others.push(f);
            }
        }
        let read = others
            .iter()
            .filter_map(|m| {
                let ticket = m.state.try_lock().ok()?.current_ticket();
                Some((store_io::repository_string(&m.repo).unwrap_or_else(|_| m.repo.display().to_string()), ticket))
            })
            .collect();
        let mut state = self.state.lock().expect("state lock");
        if let Some((name, library)) = library {
            state.set_library(name, library);
        }
        state.tell_checkout_tickets(read);
    }

    /// Say this grimoire is served in a library, as `name`, beside the grimoires of `library`. Kept apart
    /// from the state, which status is handed it from, so the library telling it never waits on this
    /// daemon's work: a copy holds its state across a fetch while it syncs.
    pub fn set_library(&self, name: &str, library: Vec<grimoire_core::answer::LibraryEntry>) {
        *self.library.lock().expect("library lock") = Some((name.to_string(), library));
    }

    /// Tell this daemon of the daemons its library serves for the other checkouts of its grimoire.
    pub fn set_fellows(&self, fellows: Vec<Weak<Daemon>>) {
        *self.fellows.lock().expect("fellows lock") = fellows;
    }

    /// Hand the store as it now is to every other checkout's daemon of this store: each keeps
    /// its own repository and current ticket, clearing the latter when its ticket is no longer
    /// worked. Called with the kin's write lock held, and this daemon's state not.
    fn share(&self, store: &grimoire_core::Store, malformed: &[grimoire_core::Malformed]) {
        for member in self.kin_but_me() {
            let mut state = member.state.lock().expect("state lock");
            let mut theirs = store.clone();
            theirs.config.repository = state.store().config.repository.clone();
            let current = state.store().current.clone();
            theirs.current = current.clone().filter(|t| theirs.tickets.get(t).is_some_and(|k| matches!(k.status, grimoire_core::model::Status::Brewing | grimoire_core::model::Status::Simmering)));
            let cleared = current.is_some() && theirs.current.is_none();
            let languages = (state.store().config.languages != store.config.languages).then(|| store.config.languages.clone());
            state.set_store(theirs, malformed.to_vec());
            if cleared {
                if let Err(e) = store_io::persist(&member.store_dir, state.store(), &grimoire_core::ops::Changed::Current) {
                    tracing::error!("{}'s current ticket could not be cleared: {e}", member.repo.display());
                }
            }
            drop(state);
            if let Some(languages) = languages {
                let _ = member.reextract_all(&languages);
            }
            let _ = member.events.send(Event::Store { tool: "checkout".to_string(), changed: vec!["*".to_string()] });
        }
    }

    /// After a write by another checkout, the main checkout's daemon prunes the baseline and
    /// hands a smaller one on; any other daemon does nothing. With the kin's write lock held.
    fn prune_for_kin(&self) {
        for member in self.kin_but_me().into_iter().filter(|m| m.main) {
            let mut state = member.state.lock().expect("state lock");
            if member.prune(&mut state) {
                let (store, malformed) = (state.store().clone(), state.malformed().to_vec());
                drop(state);
                member.share(&store, &malformed);
            }
        }
    }

    /// Hand the store, just written, to this store's other checkouts' daemons, then let the main
    /// checkout's prune the baseline for them all. With the kin's write lock held.
    pub(super) fn hand_on(&self, state: std::sync::MutexGuard<'_, State>) {
        if self.kin_but_me().is_empty() {
            return;
        }
        let (store, malformed) = (state.store().clone(), state.malformed().to_vec());
        drop(state);
        self.share(&store, &malformed);
        self.prune_for_kin();
    }
}

#[cfg(test)]
mod tests {
    use super::super::tests::{Harness, CONTRACTED};
    use super::*;
    use serde_json::{json, Value};

    /// # Test contract
    ///
    /// ```json
    /// {"test": "every-checkout-reads-one-intent-with-its-own-ticket", "checks": ["crates/server/src/daemon/kin#kin-open-for", "crates/server/src/daemon/kin#kin-sibling", "crates/server/src/daemon/kin#kin-writes-handed-on", "crates/server/src/daemon/kin#kin-current-cleared", "crates/server/src/daemon/kin#kin-main-prunes", "crates/server/src/daemon#baseline-pruned"]}
    /// ```
    #[test]
    fn every_checkout_reads_one_intent_with_its_own_ticket() {
        let h = Harness::new();
        // A second checkout of the project, its project file naming the store.
        let wt = h.repo.parent().unwrap().join("wt");
        std::fs::create_dir_all(wt.join("src")).unwrap();
        for f in ["Cargo.toml", "src/lib.rs", "src/a.rs", "src/b.rs"] {
            std::fs::copy(h.repo.join(f), wt.join(f)).unwrap();
        }
        std::fs::write(wt.join(store_io::PROJECT_FILE), format!("store = {}\n", toml::Value::String(h.store.to_string_lossy().to_string()))).unwrap();
        let main = Arc::new(h.open());
        // Added through the store's writer, announced as every write is.
        let mut told = main.subscribe();
        assert!(main.register_checkout(&wt).unwrap());
        assert!(matches!(told.try_recv(), Ok(Event::Store { .. })), "the checkout's joining is announced");
        main.call("adopt", json!({ "reason": "adopted" })).unwrap();
        let wt_at = store_io::repository_string(&wt).unwrap();
        let (other, _) = main.sibling(&wt_at).unwrap();
        assert!(main.main && !other.main);
        // Each its own current ticket: the main checkout's is `work`, the worktree's none yet.
        let page = |slug: &str| json!({ "slug": slug, "title": slug, "body": "b", "links": [] });
        assert!(main.call("put_page", page("from-main")).is_ok());
        assert_eq!(other.call("put_page", page("from-wt")).unwrap_err().code(), "invalid", "the worktree has started no ticket");
        // A write in one is read at once in the other.
        let filed = other.call("inscribe_ticket", json!({ "title": "In the worktree", "kind": "change", "priority": "later", "body": "b" })).unwrap();
        let ticket = filed["node"]["ref"].as_str().unwrap().to_string();
        assert!(main.call("get", json!({ "ref": ticket })).is_ok(), "the main checkout reads the worktree's ticket");
        other.call("brew_ticket", json!({ "ref": ticket })).unwrap();
        other.call("put_page", page("from-wt")).unwrap();
        let slug = ticket.trim_start_matches("ticket:").to_string();
        assert!(main.call("get", json!({ "ref": "page:from-wt", "ticket": slug })).is_ok(), "in the worktree's ticket's proposal, which the main checkout reads too");
        assert!(main.call("get", json!({ "ref": "page:from-main", "ticket": "work" })).is_ok(), "the main checkout's still goes into its own");
        // Kept apart on disk: the main checkout's in current.json, the worktree's with it in checkouts.json.
        assert_eq!(store_io::load(&h.store).unwrap().0.current.as_deref(), Some("work"));
        assert_eq!(store_io::load_for(&h.store, &wt_at).unwrap().0.current.as_deref(), Some(slug.as_str()));
        // Only the main checkout prunes: the worktree fixing a finding leaves the baseline as it is.
        let stored = || store_io::load(&h.store).unwrap().0.baseline.map(|b| b.findings.len()).unwrap_or(0);
        let before = stored();
        std::fs::write(wt.join("src").join("b.rs"), CONTRACTED.replace("Say a.", "Say b.")).unwrap();
        other.reextract(&["src/b.rs".to_string()]);
        assert_eq!(stored(), before, "a branch's fix has not reached the main line");
        std::fs::write(h.repo.join("src").join("b.rs"), CONTRACTED.replace("Say a.", "Say b.")).unwrap();
        main.reextract(&["src/b.rs".to_string()]);
        assert!(stored() < before, "the main checkout's prunes it");
        let held = |d: &Daemon| d.state.lock().unwrap().store().baseline.as_ref().map(|b| b.findings.len()).unwrap_or(0);
        assert_eq!(held(&other), stored(), "and the smaller baseline is handed to the worktree's view");
        // A ticket closed in one clears it as the other's current ticket.
        main.call("brew_ticket", json!({ "ref": ticket })).unwrap();
        main.call("close_ticket", json!({ "ref": ticket, "decision": { "text": "Done." } })).unwrap();
        assert_eq!(other.state.lock().unwrap().store().current, None);
        assert_eq!(store_io::load_for(&h.store, &wt_at).unwrap().0.current, None);
    }

    /// # Test contract
    ///
    /// ```json
    /// {"test": "status-names-each-checkouts-ticket", "checks": ["crates/server/src/daemon/kin#checkout-tickets"]}
    /// ```
    #[test]
    fn status_names_each_checkouts_ticket() {
        let h = Harness::new();
        let wt = h.repo.parent().unwrap().join("wt");
        std::fs::create_dir_all(wt.join("src")).unwrap();
        for f in ["Cargo.toml", "src/lib.rs", "src/a.rs", "src/b.rs"] {
            std::fs::copy(h.repo.join(f), wt.join(f)).unwrap();
        }
        std::fs::write(
            wt.join(store_io::PROJECT_FILE),
            format!(
                "store = {}
",
                toml::Value::String(h.store.to_string_lossy().to_string())
            ),
        )
        .unwrap();
        let main = Arc::new(h.open());
        assert!(main.register_checkout(&wt).unwrap());
        let (main_at, wt_at) = (store_io::repository_string(&h.repo).unwrap(), store_io::repository_string(&wt).unwrap());
        let (other, _) = main.sibling(&wt_at).unwrap();
        let entry = |repository: &str| grimoire_core::answer::LibraryEntry { name: "shop".into(), repository: repository.into(), server: None, current: None };
        main.set_library("shop", vec![entry(&main_at), entry(&wt_at)]);
        let tickets = |d: &Daemon| d.call("status", json!({})).unwrap()["library"].as_array().unwrap().iter().map(|e| e["current"]["id"].clone()).collect::<Vec<_>>();
        // The main checkout's `work` is current, the worktree's none yet.
        assert_eq!(tickets(&main), vec![json!("work"), Value::Null]);
        // The worktree starts one: the main checkout's status names it at once, with no listing of the library since.
        let filed = other.call("inscribe_ticket", json!({ "title": "In the worktree", "kind": "change", "priority": "later", "body": "b" })).unwrap();
        let ticket = filed["node"]["ref"].as_str().unwrap().to_string();
        other.call("brew_ticket", json!({ "ref": ticket })).unwrap();
        let listed = main.call("status", json!({})).unwrap()["library"][1]["current"].clone();
        assert_eq!(listed, json!({ "id": ticket.trim_start_matches("ticket:"), "title": "In the worktree" }));
        // A checkout that is no kin, as a team's each keep a copy of their own: named once the library tells of it.
        let apart = Harness::new();
        let fellow = Arc::new(apart.open());
        let fellow_at = store_io::repository_string(&apart.repo).unwrap();
        main.set_library("shop", vec![entry(&main_at), entry(&wt_at), entry(&fellow_at)]);
        assert_eq!(main.call("status", json!({})).unwrap()["library"][2]["current"], Value::Null, "not told of it, nothing is known");
        main.set_fellows(vec![Arc::downgrade(&fellow)]);
        assert_eq!(main.call("status", json!({})).unwrap()["library"][2]["current"]["id"], "work");
        // A fellow busy with its state, as one mid-sync is, is not waited for: its ticket stays as last read.
        let busy = fellow.state.lock().unwrap();
        assert_eq!(main.call("status", json!({})).unwrap()["library"][2]["current"]["id"], "work");
        drop(busy);
    }
}
