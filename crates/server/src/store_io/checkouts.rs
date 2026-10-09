//! # Module contract
//!
//! ```json
//! {"purpose":"Keep the other checkouts a store of its own serves beside the one it was made for: list them, add one whose project file names the store, and give each its own current ticket, in a file of their own, so a store with one checkout reads as it always did.","criteria":[{"id":"checkouts-listed","statement":"checkouts.json lists, by repository as its canonical path, every checkout of a store of its own but the main one grimoire.json names, each with its own current ticket; a store without it has the main checkout alone."},{"id":"checkouts-view","statement":"view makes a loaded store the view of one listed checkout, its repository that checkout and its current ticket that checkout's, one naming a ticket neither brewing nor simmering reported malformed and left none."},{"id":"checkouts-register","statement":"register adds a checkout whose project file names this very store, answering whether it was new, the main one and one already listed being no change."},{"id":"checkouts-register-refuses","statement":"register refuses a checkout whose project file names no store or another, one holding the store and one inside it."},{"id":"checkouts-set-current","statement":"set_current writes one checkout's current ticket, leaving the others'."},{"id":"checkouts-unloadable","statement":"A checkouts.json that does not load is said as malformed, and the other checkouts are then none."}]}
//! ```

use std::collections::BTreeMap;
use std::path::Path;

use grimoire_core::{Malformed, Store};
use serde::{Deserialize, Serialize};

use super::{location, StoreIoError};

pub const CHECKOUTS_FILE: &str = "checkouts.json";

/// `checkouts.json`: the checkouts of a store of its own beside its main one, by repository.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Checkouts {
    #[serde(default)]
    pub checkouts: BTreeMap<String, Entry>,
}

/// One checkout's own: the ticket a write from it goes into when it names none.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Entry {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub current: Option<String>,
}

/// The other checkouts, or why the file does not load; none when there is no file.
pub fn read(store_dir: &Path) -> Result<Checkouts, String> {
    let path = store_dir.join(CHECKOUTS_FILE);
    if !path.is_file() {
        return Ok(Checkouts::default());
    }
    super::read_json(&path)
}

/// The checkouts beside the main one, in order; none when the file is missing or does not load.
pub fn others(store_dir: &Path) -> Vec<String> {
    read(store_dir).map(|c| c.checkouts.into_keys().collect()).unwrap_or_default()
}

/// The checkout the store was made for, as grimoire.json names it.
pub fn main_of(store_dir: &Path) -> Result<String, StoreIoError> {
    Ok(super::read_config(store_dir)?.repository)
}

/// Make `store`, loaded as its main checkout's, the view of `checkout`: its repository and
/// its own current ticket. Refused when the store lists no such checkout.
pub fn view(store: &mut Store, store_dir: &Path, checkout: &str, malformed: &mut Vec<Malformed>) -> Result<(), StoreIoError> {
    if checkout == store.config.repository {
        return Ok(());
    }
    let listed = match read(store_dir) {
        Ok(c) => c,
        Err(reason) => return Err(StoreIoError::Invalid(CHECKOUTS_FILE.into(), reason)),
    };
    let Some(entry) = listed.checkouts.get(checkout) else {
        return Err(StoreIoError::Invalid(CHECKOUTS_FILE.into(), format!("the store serves no checkout {checkout}")));
    };
    store.config.repository = checkout.to_string();
    store.current = match &entry.current {
        Some(t) if super::worked(store, t) => Some(t.clone()),
        Some(t) => {
            malformed.push(Malformed { path: CHECKOUTS_FILE.to_string(), reason: format!("{checkout}'s current ticket `{t}` is neither brewing nor simmering") });
            None
        }
        None => None,
    };
    Ok(())
}

/// Add the checkout at `repo` to the store's checkouts: true when it was not one yet. Its
/// project file must name this very store, and it must not hold the store.
pub fn register(store_dir: &Path, repo: &Path) -> Result<bool, StoreIoError> {
    let repository = location::repository_string(repo)?;
    let named = match location::project_at(repo)? {
        Some((location::Home::Store(named), _)) => named,
        Some((location::Home::Server { .. }, _)) => return Err(StoreIoError::Invalid(repo.display().to_string(), "its project file names a team's server, not this store".into())),
        None => return Err(StoreIoError::Invalid(repo.display().to_string(), format!("it holds no {}", location::PROJECT_FILE))),
    };
    let same = named.canonicalize().ok().zip(store_dir.canonicalize().ok()).is_some_and(|(a, b)| a == b);
    if !same {
        return Err(StoreIoError::Invalid(repo.display().to_string(), format!("its project file names the store {}, not {}", named.display(), store_dir.display())));
    }
    location::refuse_inside(store_dir, repo)?;
    // Nor the other way round: a checkout kept inside the store.
    let inside = repo.canonicalize().ok().zip(store_dir.canonicalize().ok()).is_some_and(|(r, s)| r.starts_with(&s));
    if inside {
        return Err(StoreIoError::Invalid(repo.display().to_string(), format!("it lies inside the store {}", store_dir.display())));
    }
    if repository == main_of(store_dir)? {
        return Ok(false);
    }
    let mut listed = read(store_dir).map_err(|reason| StoreIoError::Invalid(CHECKOUTS_FILE.into(), reason))?;
    if listed.checkouts.contains_key(&repository) {
        return Ok(false);
    }
    listed.checkouts.insert(repository, Entry::default());
    super::write_atomic(&store_dir.join(CHECKOUTS_FILE), &super::to_pretty(&listed))?;
    Ok(true)
}

/// Write `checkout`'s current ticket, the other checkouts' left as they are.
pub fn set_current(store_dir: &Path, checkout: &str, current: Option<&str>) -> Result<(), StoreIoError> {
    let mut listed = read(store_dir).map_err(|reason| StoreIoError::Invalid(CHECKOUTS_FILE.into(), reason))?;
    let entry = listed.checkouts.entry(checkout.to_string()).or_default();
    entry.current = current.map(str::to_string);
    super::write_atomic(&store_dir.join(CHECKOUTS_FILE), &super::to_pretty(&listed))
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]
    use super::*;
    use grimoire_core::model::{Roots, Scope, StoreConfig};

    /// A main checkout and a second one, each a folder whose project file names the store.
    fn two_checkouts(root: &Path) -> (std::path::PathBuf, std::path::PathBuf, std::path::PathBuf) {
        let store_dir = root.join("store");
        let (main, wt) = (root.join("main"), root.join("main-wt"));
        for repo in [&main, &wt] {
            std::fs::create_dir_all(repo).unwrap();
            std::fs::write(repo.join(location::PROJECT_FILE), format!("store = {}\n", toml::Value::String(store_dir.to_string_lossy().to_string()))).unwrap();
        }
        let config = StoreConfig {
            large_file: None,
            long_statement: None,
            plain_file: None,
            site: vec![],
            version: StoreConfig::CURRENT_VERSION,
            repository: location::repository_string(&main).unwrap(),
            roots: Roots::new("overview", "rites"),
            languages: vec![],
            scope: Scope::default(),
            order: Default::default(),
            recipe_kinds: Default::default(),
        };
        super::super::write_all(&store_dir, &Store::new(config), false).unwrap();
        (store_dir, main, wt)
    }

    /// # Test contract
    ///
    /// ```json
    /// {"test": "a-store-keeps-its-other-checkouts", "checks": ["crates/server/src/store_io/checkouts#checkouts-listed", "crates/server/src/store_io/checkouts#checkouts-view", "crates/server/src/store_io/checkouts#checkouts-register", "crates/server/src/store_io/checkouts#checkouts-register-refuses", "crates/server/src/store_io/checkouts#checkouts-set-current", "crates/server/src/store_io/checkouts#checkouts-unloadable"]}
    /// ```
    #[test]
    fn a_store_keeps_its_other_checkouts() {
        let tmp = tempfile::tempdir().unwrap();
        let (store_dir, main, wt) = two_checkouts(tmp.path());
        assert!(others(&store_dir).is_empty(), "a store with no file has its main checkout alone");
        assert!(!register(&store_dir, &main).unwrap(), "the main checkout is no change");
        assert!(register(&store_dir, &wt).unwrap());
        assert!(!register(&store_dir, &wt).unwrap(), "added once");
        let wt_at = location::repository_string(&wt).unwrap();
        assert_eq!(others(&store_dir), std::slice::from_ref(&wt_at));
        // Each its own current ticket: the main's in current.json, the other's here, neither touching the other.
        let (mut store, _) = super::super::load(&store_dir).unwrap();
        store.tickets.insert(
            "0001".into(),
            grimoire_core::model::Ticket {
                slug: "0001".into(),
                title: "T".into(),
                kind: grimoire_core::model::TicketKind::Change,
                priority: grimoire_core::model::Priority::Later,
                status: grimoire_core::model::Status::Brewing,
                touches: vec![],
                relies_on: vec![],
                body: "b".into(),
                decision: None,
            },
        );
        super::super::persist(&store_dir, &store, &grimoire_core::ops::Changed::Ticket("0001".into())).unwrap();
        set_current(&store_dir, &wt_at, Some("0001")).unwrap();
        let (mut seen, mut malformed) = super::super::load(&store_dir).unwrap();
        assert_eq!(seen.current, None, "the main checkout's own is still none");
        view(&mut seen, &store_dir, &wt_at, &mut malformed).unwrap();
        assert_eq!((seen.config.repository.as_str(), seen.current.as_deref()), (wt_at.as_str(), Some("0001")));
        // A view's writes keep to its checkout: its current ticket here, the config still naming the main one.
        let mut wt_view = seen.clone();
        wt_view.current = None;
        super::super::persist(&store_dir, &wt_view, &grimoire_core::ops::Changed::Current).unwrap();
        assert_eq!(read(&store_dir).unwrap().checkouts[&wt_at].current, None);
        wt_view.current = Some("0001".into());
        super::super::persist(&store_dir, &wt_view, &grimoire_core::ops::Changed::Current).unwrap();
        assert!(!store_dir.join(super::super::CURRENT_FILE).exists(), "the main checkout's current.json is not written by another's view");
        super::super::persist(&store_dir, &wt_view, &grimoire_core::ops::Changed::Config).unwrap();
        assert_eq!(main_of(&store_dir).unwrap(), location::repository_string(&main).unwrap(), "the config still names the main checkout");
        // One naming a ticket no longer worked is said, and is none.
        set_current(&store_dir, &wt_at, Some("0002")).unwrap();
        let (mut seen, mut malformed) = super::super::load(&store_dir).unwrap();
        view(&mut seen, &store_dir, &wt_at, &mut malformed).unwrap();
        assert!(seen.current.is_none() && malformed.iter().any(|m| m.path == CHECKOUTS_FILE), "{malformed:?}");
        // A checkout not listed is no view of this store.
        let (mut seen, mut malformed) = super::super::load(&store_dir).unwrap();
        assert!(view(&mut seen, &store_dir, "C:/elsewhere", &mut malformed).is_err());
        // A folder whose project file names another store, or none, is never added.
        let other = tmp.path().join("other");
        std::fs::create_dir_all(&other).unwrap();
        assert!(register(&store_dir, &other).is_err(), "no project file");
        std::fs::write(other.join(location::PROJECT_FILE), "store = \"elsewhere\"\n").unwrap();
        assert!(register(&store_dir, &other).is_err(), "another store");
        assert_eq!(others(&store_dir).len(), 1);
        // Nor one inside the store, whatever its project file says.
        let within = store_dir.join("pages").join("nested");
        std::fs::create_dir_all(&within).unwrap();
        std::fs::write(within.join(location::PROJECT_FILE), format!("store = {}\n", toml::Value::String(store_dir.to_string_lossy().to_string()))).unwrap();
        assert!(register(&store_dir, &within).is_err(), "inside the store");
        std::fs::remove_dir_all(&within).unwrap();
        // A file that does not load is said as such, and serves no other checkout.
        std::fs::write(store_dir.join(CHECKOUTS_FILE), "{ not json").unwrap();
        assert!(read(&store_dir).is_err() && others(&store_dir).is_empty());
        let (_, malformed) = super::super::load(&store_dir).unwrap();
        assert!(malformed.iter().any(|m| m.path == CHECKOUTS_FILE), "loading the store says so: {malformed:?}");
        let (mut seen, mut malformed) = super::super::load(&store_dir).unwrap();
        assert!(view(&mut seen, &store_dir, &wt_at, &mut malformed).is_err(), "and serves no other checkout");
    }
}
