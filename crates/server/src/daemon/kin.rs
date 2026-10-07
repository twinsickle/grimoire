//! # Module contract
//!
//! ```json
//! {"purpose":"Serve one store of its own for its several checkouts side by side, each daemon a view of it with that checkout's code and current ticket, their writes taken one at a time and each handed to the others.","criteria":[{"id":"checkouts","statement":"open_for opens a store of its own as one of its checkouts sees it, its repository and its current ticket that checkout's, extracting that checkout's code; sibling opens another checkout's view of the same store beside a daemon already serving it, taking no lock of its own; the daemons serving one store take its writes one at a time, and each write, a pruned baseline included, is handed at once to every other, which keeps its own repository and its current ticket, the latter cleared and persisted when the ticket it named is no longer being worked; after another checkout's write, the main checkout's daemon prunes the baseline and hands it on."}]}
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

    /// Hand the store as it now is to every other checkout's daemon of this store: each keeps
    /// its own repository and current ticket, clearing the latter when its ticket is no longer
    /// worked. Called with the kin's write lock held, and this daemon's state not.
    fn share(&self, store: &grimoire_core::Store, malformed: &[grimoire_core::Malformed]) {
        for member in self.kin_but_me() {
            let mut state = member.state.lock().expect("state lock");
            let mut theirs = store.clone();
            theirs.config.repository = state.store().config.repository.clone();
            let current = state.store().current.clone();
            theirs.current = current.clone().filter(|t| theirs.tickets.get(t).is_some_and(|k| matches!(k.status, grimoire_core::model::Status::Active | grimoire_core::model::Status::Landing)));
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
    use serde_json::json;

    /// # Test contract
    ///
    /// ```json
    /// {"test": "every-checkout-reads-one-intent-with-its-own-ticket", "checks": ["crates/server/src/daemon/kin#checkouts", "crates/server/src/daemon#baseline-pruned"]}
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
        let filed = other.call("file_ticket", json!({ "title": "In the worktree", "kind": "change", "priority": "later", "body": "b" })).unwrap();
        let ticket = filed["node"]["ref"].as_str().unwrap().to_string();
        assert!(main.call("get", json!({ "ref": ticket })).is_ok(), "the main checkout reads the worktree's ticket");
        other.call("start_ticket", json!({ "ref": ticket })).unwrap();
        other.call("put_page", page("from-wt")).unwrap();
        let slug = ticket.trim_start_matches("ticket:").to_string();
        assert!(main.call("get", json!({ "ref": "page:from-wt", "ticket": slug })).is_ok(), "in the worktree's ticket's proposal, which the main checkout reads too");
        assert!(main.call("get", json!({ "ref": "page:from-main", "ticket": "work" })).is_ok(), "the main checkout's still lands in its own");
        // Kept apart on disk: the main checkout's in current.json, the worktree's with it in checkouts.json.
        assert_eq!(store_io::load(&h.store).unwrap().0.current.as_deref(), Some("work"));
        assert_eq!(store_io::load_for(&h.store, &wt_at).unwrap().0.current.as_deref(), Some(slug.as_str()));
        // Only the main checkout prunes: the worktree fixing a finding leaves the baseline as it is.
        let stored = || store_io::load(&h.store).unwrap().0.baseline.map(|b| b.findings.len()).unwrap_or(0);
        let before = stored();
        std::fs::write(wt.join("src").join("b.rs"), CONTRACTED.replace("Say a.", "Say b.")).unwrap();
        other.reextract(&["src/b.rs".to_string()]);
        assert_eq!(stored(), before, "a branch's fix has not landed");
        std::fs::write(h.repo.join("src").join("b.rs"), CONTRACTED.replace("Say a.", "Say b.")).unwrap();
        main.reextract(&["src/b.rs".to_string()]);
        assert!(stored() < before, "the main checkout's prunes it");
        let held = |d: &Daemon| d.state.lock().unwrap().store().baseline.as_ref().map(|b| b.findings.len()).unwrap_or(0);
        assert_eq!(held(&other), stored(), "and the smaller baseline is handed to the worktree's view");
        // A ticket closed in one clears it as the other's current ticket.
        main.call("start_ticket", json!({ "ref": ticket })).unwrap();
        main.call("close_ticket", json!({ "ref": ticket, "decision": { "text": "Done." } })).unwrap();
        assert_eq!(other.state.lock().unwrap().store().current, None);
        assert_eq!(store_io::load_for(&h.store, &wt_at).unwrap().0.current, None);
    }
}
