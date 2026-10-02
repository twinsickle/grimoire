//! # Module contract
//!
//! ```json
//! {"purpose":"Hold a store's write lock, so it has one writer at a time, and name who holds it when it is taken.","criteria":[{"id":"exclusive","statement":"lock takes an exclusive operating-system lock on the lock file of an existing store outside its repository, and lock_new on a directory about to hold a store, held for as long as the value returned lives; a directory that is not a store, or a store inside its repository, is refused before any lock file is made; when another process holds the lock, it fails naming the holder, the daemon serve.json names given as likely, since a crashed daemon leaves one, or else another grimoire process, and changes nothing."}]}
//! ```

use std::path::Path;

use grimoire_core::model::StoreConfig;

use super::{migrate, read_config, read_serving, refuse_inside, StoreIoError};

/// The empty file a writer locks.
pub const LOCK_FILE: &str = "lock";

/// The store's write lock, held for as long as this value lives. Dropping
/// it releases it, and so does the process ending, however it ends.
#[derive(Debug)]
pub struct StoreLock {
    _file: std::fs::File,
}

/// Take the write lock of an existing store, or fail saying who holds it.
/// Nothing is created in a directory that is not a store, nor in a store
/// that lives inside its repository, since both are refused anyway: the
/// directory must hold grimoire.json naming a repository outside it.
pub fn lock(store_dir: &Path) -> Result<StoreLock, StoreIoError> {
    let config = read_config(store_dir)?;
    refuse_inside(store_dir, Path::new(&config.repository))?;
    let held = take(store_dir)?;
    // The first writer of a version 1 store rewrites it as version 2, under the lock it now holds.
    if config.version == StoreConfig::TRIMMED_ON_READ {
        let rewritten = migrate(store_dir)?;
        tracing::info!("store {} is now version {}: {rewritten} record(s) rewritten with their strings trimmed", store_dir.display(), StoreConfig::CURRENT_VERSION);
    }
    Ok(held)
}

/// Take the write lock of a store about to be written whole, for init and
/// scaffold: the directory must exist, and need not hold a store yet. The
/// caller has refused a location inside the repository already.
pub fn lock_new(store_dir: &Path) -> Result<StoreLock, StoreIoError> {
    if !store_dir.is_dir() {
        return Err(StoreIoError::NotAStore(store_dir.to_path_buf()));
    }
    take(store_dir)
}

fn take(store_dir: &Path) -> Result<StoreLock, StoreIoError> {
    let path = store_dir.join(LOCK_FILE);
    let file = std::fs::OpenOptions::new().read(true).write(true).create(true).truncate(false).open(&path).map_err(|e| StoreIoError::Io(path.clone(), e))?;
    match file.try_lock() {
        Ok(()) => Ok(StoreLock { _file: file }),
        Err(std::fs::TryLockError::WouldBlock) => Err(StoreIoError::Locked(store_dir.to_path_buf(), holder(store_dir))),
        Err(std::fs::TryLockError::Error(e)) => Err(StoreIoError::Io(path, e)),
    }
}

/// Who holds the lock, as far as the store says: a locked file cannot be
/// read on every platform, so a daemon is named from its serve.json, which
/// a daemon that crashed leaves behind, so the name is given as likely.
fn holder(store_dir: &Path) -> String {
    match read_serving(store_dir) {
        Some(s) => format!("the daemon serving it on port {} (process {}), or, if that daemon is gone, another grimoire process", s.port, s.pid),
        None => "another grimoire process, a command writing it or a daemon starting".to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::store_io::tests::temp;
    use crate::store_io::{write_all, write_serving};
    use grimoire_core::model::Scope;
    use grimoire_core::Store;

    /// # Test contract
    ///
    /// ```json
    /// {"test": "one-lock-holder-at-a-time", "checks": ["crates/server/src/store_io/lock#exclusive"]}
    /// ```
    #[test]
    fn one_lock_holder_at_a_time() {
        let tmp = temp();
        let dir = tmp.path();
        let (store_dir, repo) = (dir.join("store"), dir.join("repo"));
        std::fs::create_dir_all(&repo).unwrap();
        // Not a store yet: lock refuses and makes nothing, lock_new takes it for a store being written.
        std::fs::create_dir_all(&store_dir).unwrap();
        assert!(matches!(lock(&store_dir), Err(StoreIoError::NotAStore(_))));
        assert!(!store_dir.join(LOCK_FILE).exists(), "no lock file in what is not a store");
        drop(lock_new(&store_dir).unwrap());
        let config = StoreConfig { version: StoreConfig::CURRENT_VERSION, repository: repo.to_string_lossy().into(), roots: vec!["overview".into()], languages: vec![], scope: Scope::default(), order: Default::default() };
        write_all(&store_dir, &Store::new(config), false).unwrap();
        let held = lock(&store_dir).unwrap();
        // A second holder, here another handle, is refused and told who holds it.
        match lock(&store_dir) {
            Err(StoreIoError::Locked(_, who)) => assert_eq!(who, "another grimoire process, a command writing it or a daemon starting"),
            other => panic!("expected Locked, got {other:?}"),
        }
        assert!(matches!(lock_new(&store_dir), Err(StoreIoError::Locked(..))), "one lock, whichever way it is taken");
        // A serve.json names the daemon, but only as likely, since a crashed one leaves it behind.
        write_serving(&store_dir, 4299).unwrap();
        let refused = lock(&store_dir).unwrap_err().to_string();
        assert!(refused.contains("the daemon serving it on port 4299") && refused.contains(&format!("process {}", std::process::id())), "{refused}");
        assert!(refused.contains("if that daemon is gone, another grimoire process"), "{refused}");
        drop(held);
        assert!(lock(&store_dir).is_ok(), "released when the holder goes");
        assert!(matches!(lock(&dir.join("missing")), Err(StoreIoError::NotAStore(_))), "no store, no lock file made");
        // A store that names a repository holding it is refused before a lock file is made.
        let inside = repo.join("store");
        let config = StoreConfig { version: StoreConfig::CURRENT_VERSION, repository: repo.to_string_lossy().into(), roots: vec!["overview".into()], languages: vec![], scope: Scope::default(), order: Default::default() };
        write_all(&inside, &Store::new(config), false).unwrap();
        assert!(matches!(lock(&inside), Err(StoreIoError::InsideRepository(..))));
        assert!(!inside.join(LOCK_FILE).exists());
        assert!(!dir.join("missing").exists());
    }
}
