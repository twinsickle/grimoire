//! # Module contract
//!
//! ```json
//! {"purpose":"Delete a store, and make a deletion asked for happen eventually: tell whether a path is only a store, remove one under its lock, and queue a deletion something still holds the store against, carrying it out once nothing does.","criteria":[{"id":"refuses-a-filesystem-root","statement":"not_only_a_store says a filesystem root may not be deleted as a store."},{"id":"refuses-a-foreign-entry","statement":"not_only_a_store says why a path may not be deleted as a store when it holds an entry a store does not hold by its name and its own type."},{"id":"refuses-a-link","statement":"not_only_a_store says why a path may not be deleted as a store when it holds a link, which is never followed."},{"id":"refuses-a-foreign-record-name","statement":"not_only_a_store says why a path may not be deleted as a store when a record folder holds a name that no write makes."},{"id":"none-when-only-a-store","statement":"not_only_a_store answers none when the path is only a store."},{"id":"leftover-answers","statement":"leftover answers whether a path is what a deletion stopped after grimoire.json went leaves: no folder, or one holding only the lock files and empty record folders."},{"id":"deletes-under-lock","statement":"delete removes a store its caller holds the lock of, taking the tokens' lock too: everything but grimoire.json and the lock files first, then grimoire.json, so a deletion stopped part way leaves a store a rerun can load, then the tokens' lock file and the store's while still held, and the folder once the lock is released."},{"id":"queue-record-per-store","statement":"queue records a store's deletion under the deletions folder it is given, one JSON file per store holding the store's path as the system names it and the repository it names."},{"id":"record-found-by-path","statement":"A store's record is found by the store's path, and placed by a name made from it."},{"id":"name-clash-numbered-sibling","statement":"A record whose name another store's record holds takes a numbered sibling name."},{"id":"queue-replaces-own-record","statement":"queue replaces the store's earlier record, never another store's."},{"id":"record-written-whole","statement":"queue writes a record whole or not at all."},{"id":"queued-lists-records","statement":"queued lists the records in the deletions folder."},{"id":"queued-names-unreadable","statement":"queued names any record, or the folder, that cannot be read."},{"id":"is-queued-answers","statement":"is_queued answers whether a store is among the records."},{"id":"unreadable-queue-holds-every-store","statement":"is_queued counts a queue that cannot be read whole as holding the store."},{"id":"cancel-removes-record","statement":"cancel removes a store's record, answering whether there was one."},{"id":"cancel-fails-unreadable","statement":"cancel fails when the queue cannot be read whole or a record cannot be removed."},{"id":"hold-queue-lock","statement":"hold holds the queue's lock, a file in the folder, which queue, cancel and carry_out take, so none of them overlaps another."},{"id":"carried-out-in-turn","statement":"carry_out takes each queued deletion in turn, under the queue's lock, and answers what happened to it."},{"id":"gone-store-removed","statement":"carry_out removes a store gone, or what a stopped deletion leaves, and drops its record."},{"id":"named-by-project-kept","statement":"carry_out keeps a store its repository's project file still names and leaves it queued, since uninstall removes that file last and only cancel drops a deletion."},{"id":"more-than-a-store-kept","statement":"carry_out keeps a store that is not only a store, its record kept, with why."},{"id":"other-repository-kept","statement":"carry_out keeps a store whose grimoire.json names another repository, its record kept, with why."},{"id":"held-left-queued","statement":"carry_out leaves queued a store whose lock another holds."},{"id":"deleted-under-lock","statement":"carry_out deletes any other store under its lock and drops its record."},{"id":"checks-repeated-under-lock","statement":"carry_out repeats its checks once the store's lock is held."},{"id":"unreadable-record-kept","statement":"carry_out keeps a record that cannot be read, and says so."},{"id":"record-removal-failure-said","statement":"carry_out says when a store it deleted has a record that cannot be removed, and leaves the record for a later carry-out to drop."},{"id":"stopped-deletion-stays-queued","statement":"carry_out leaves queued a store whose deletion stopped part way, with why."}]}
//! ```

use std::path::{Path, PathBuf};

use super::{project_at, read_config, StoreIoError, CONFIG_FILE, JUDGEMENTS_DIR, LOCK_FILE, PROPOSALS_DIR, STARTS_DIR};

/// The folders of a store that hold one file per record: a judged recipe's judgements are one file named for the recipe.
pub const RECORD_DIRS: [&str; 6] = ["pages", "requirements", "tickets", STARTS_DIR, PROPOSALS_DIR, JUDGEMENTS_DIR];

/// The files a store directory may hold beside its record folders. Anything
/// else, or one of these of the wrong type, means the path is not only a
/// store, and deleting it would take something else with it.
pub const STORE_FILES: [&str; 10] = [
    CONFIG_FILE,
    super::RECIPES_FILE,
    super::BASELINE_FILE,
    super::SERVE_FILE,
    super::CURRENT_FILE,
    super::CHECKED_OUT_FILE,
    super::checkouts::CHECKOUTS_FILE,
    super::tokens::TOKENS_FILE,
    super::tokens::TOKENS_LOCK,
    LOCK_FILE,
];

/// Whether a name in a record folder is one store_io writes: `<slug>.json`,
/// or the `.<slug>.json.tmp-<pid>` a write left behind.
pub fn is_record_file_name(name: &str) -> bool {
    let record = |n: &str| n.strip_suffix(".json").is_some_and(grimoire_core::store::is_slug);
    record(name) || name.strip_prefix('.').and_then(|n| n.rsplit_once(".tmp-")).is_some_and(|(r, pid)| record(r) && !pid.is_empty() && pid.bytes().all(|b| b.is_ascii_digit()))
}

/// Why `store`, an existing folder, may not be deleted as a store, if it may not.
pub fn not_only_a_store(store: &Path) -> std::io::Result<Option<String>> {
    let real = store.canonicalize()?;
    if real.parent().is_none() {
        return Ok(Some(format!("{} is a filesystem root", real.display())));
    }
    // Each entry by its own type, a link never followed: a folder where a file belongs, or the reverse, is not the store's.
    for e in std::fs::read_dir(&real)? {
        let e = e?;
        let (name, kind) = (e.file_name().to_string_lossy().to_string(), e.file_type()?);
        let ours = if kind.is_dir() { RECORD_DIRS.contains(&name.as_str()) } else { kind.is_file() && STORE_FILES.contains(&name.as_str()) };
        if !ours {
            return Ok(Some(format!("{} holds `{name}`, which is not part of a store", real.display())));
        }
    }
    // The record folders hold records, and at most a temporary file a write left; nothing else.
    for dir in RECORD_DIRS {
        let Ok(entries) = std::fs::read_dir(real.join(dir)) else { continue };
        for e in entries {
            let e = e?;
            let name = e.file_name().to_string_lossy().to_string();
            if !e.file_type()?.is_file() || !is_record_file_name(&name) {
                return Ok(Some(format!("{} holds `{dir}/{name}`, which is not part of a store", real.display())));
            }
        }
    }
    Ok(None)
}

/// Whether `store` is what `delete` leaves when it stops after removing
/// grimoire.json: no folder at all, or one holding nothing but the lock
/// files and empty record folders. Such a folder has no records left to
/// protect and no grimoire.json to say whose it was, so a rerun removes it.
pub fn leftover(store: &Path) -> std::io::Result<bool> {
    if !store.exists() {
        return Ok(true);
    }
    if !store.is_dir() || store.join(CONFIG_FILE).exists() {
        return Ok(false);
    }
    for e in std::fs::read_dir(store)? {
        let e = e?;
        let name = e.file_name().to_string_lossy().to_string();
        let empty_records = RECORD_DIRS.contains(&name.as_str()) && e.path().is_dir() && std::fs::read_dir(e.path())?.next().is_none();
        if name != LOCK_FILE && name != super::tokens::TOKENS_LOCK && !empty_records {
            return Ok(false);
        }
    }
    Ok(true)
}

/// Delete a store directory its caller holds the lock of. Under the lock:
/// the records and everything else first, grimoire.json last, so a failure
/// part way leaves a store a rerun can load and finish; then the lock file
/// itself, while still held, so no daemon can open it after. Released, the
/// folder last, since a process cannot remove a folder holding a file it has
/// open. A daemon starting after the records are gone finds no grimoire.json
/// and takes no lock.
pub fn delete(store: &Path, lock: Option<super::StoreLock>) -> Result<(), StoreIoError> {
    let io = |e: std::io::Error| StoreIoError::Io(store.to_path_buf(), e);
    // The tokens' lock too, after the store's: a token command waits, and then finds no store.
    // There is none to take when a deletion stopped after grimoire.json went, which leaves no
    // tokens to change; any other failure to take it stops the deletion before anything is removed.
    let tokens_lock = match super::tokens::lock(store) {
        Ok(held) => Some(held),
        Err(StoreIoError::NotAStore(_)) => None,
        Err(e) => return Err(e),
    };
    let keep_for_last = [CONFIG_FILE, LOCK_FILE, super::tokens::TOKENS_LOCK];
    for e in std::fs::read_dir(store).map_err(io)? {
        let path = e.map_err(io)?.path();
        if path.file_name().is_some_and(|n| keep_for_last.iter().any(|k| n == *k)) {
            continue;
        }
        if path.is_dir() {
            std::fs::remove_dir_all(&path).map_err(io)?;
        } else {
            std::fs::remove_file(&path).map_err(io)?;
        }
    }
    std::fs::remove_file(store.join(CONFIG_FILE)).map_err(io)?;
    // Released before its file goes, since a process cannot remove a file it holds open and locked.
    drop(tokens_lock);
    for last in [super::tokens::TOKENS_LOCK, LOCK_FILE] {
        let file = store.join(last);
        if file.exists() {
            std::fs::remove_file(&file).map_err(io)?;
        }
    }
    drop(lock);
    std::fs::remove_dir(store).map_err(io)?;
    Ok(())
}

/// The machine's folder of queued deletions: `<data>/grimoire/deletions`.
pub fn deletions_dir() -> Result<PathBuf, StoreIoError> {
    Ok(super::location::data_dir()?.join("deletions"))
}

/// A deletion asked for and not yet carried out.
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct Queued {
    /// The store to delete.
    pub store: PathBuf,
    /// The repository its grimoire.json named when the deletion was asked for.
    pub repository: String,
}

/// The store as a record names it: as the system names it, so two spellings of one folder are one store.
fn spelt(store: &Path) -> PathBuf {
    store.canonicalize().unwrap_or_else(|_| store.to_path_buf())
}

/// A record's file name, from the store's path: stable across builds, unlike the standard hasher.
/// It only places the record: a record is found by the path it holds, and a name another store's
/// record has taken is followed by `-1`, `-2` and on.
fn record_name(store: &Path, n: usize) -> String {
    let mut h: u64 = 0xcbf2_9ce4_8422_2325;
    for b in spelt(store).to_string_lossy().bytes() {
        h = (h ^ b as u64).wrapping_mul(0x0100_0000_01b3);
    }
    if n == 0 {
        format!("{h:016x}.json")
    } else {
        format!("{h:016x}-{n}.json")
    }
}

/// The file whose lock every change to the queue holds, so a cancel, a queuing and a carry-out never overlap.
const QUEUE_LOCK: &str = ".lock";

/// The queue held: while this lives, no other process changes the queue or carries it out.
pub struct Held {
    dir: PathBuf,
    _file: std::fs::File,
}

/// Hold the queue in `dir`, waiting for whoever holds it.
pub fn hold(dir: &Path) -> Result<Held, StoreIoError> {
    let io = |e: std::io::Error| StoreIoError::Io(dir.join(QUEUE_LOCK), e);
    std::fs::create_dir_all(dir).map_err(io)?;
    let file = std::fs::OpenOptions::new().read(true).write(true).create(true).truncate(false).open(dir.join(QUEUE_LOCK)).map_err(io)?;
    #[cfg(test)]
    match file.try_lock() {
        Ok(()) => return Ok(Held { dir: dir.to_path_buf(), _file: file }),
        Err(std::fs::TryLockError::WouldBlock) => tell_blocked(dir),
        Err(std::fs::TryLockError::Error(e)) => return Err(io(e)),
    }
    file.lock().map_err(io)?;
    Ok(Held { dir: dir.to_path_buf(), _file: file })
}

/// Test hook: those waiting to hear that a taker of the queue in a folder found it held, and is about to wait.
#[cfg(test)]
static BLOCKED: std::sync::Mutex<Vec<(PathBuf, std::sync::mpsc::Sender<()>)>> = std::sync::Mutex::new(Vec::new());

#[cfg(test)]
fn tell_blocked(dir: &Path) {
    for (watched, tell) in BLOCKED.lock().unwrap().iter() {
        if watched == dir {
            let _ = tell.send(());
        }
    }
}

impl Held {
    /// Queue `store`'s deletion, replacing its earlier record, never another store's.
    pub fn queue(&self, store: &Path, repository: &str) -> Result<PathBuf, StoreIoError> {
        let store = spelt(store);
        let (found, _) = queued(&self.dir);
        let path = match found.into_iter().find(|(_, q)| q.store == store) {
            Some((path, _)) => path,
            None => (0..).map(|n| self.dir.join(record_name(&store, n))).find(|p| !p.exists()).expect("a free name"),
        };
        let text = serde_json::to_string_pretty(&Queued { store, repository: repository.to_string() }).expect("a queued deletion serialises");
        super::write_atomic(&path, &text)?;
        Ok(path)
    }

    /// Cancel `store`'s queued deletion, answering whether there was one; a queue that cannot be
    /// read whole, or a record that cannot be removed, is an error, so the store is not taken up
    /// while its deletion may still stand.
    pub fn cancel(&self, store: &Path) -> Result<bool, StoreIoError> {
        let (found, unread) = queued(&self.dir);
        if let Some(why) = unread.into_iter().next() {
            return Err(StoreIoError::Io(self.dir.clone(), std::io::Error::other(why)));
        }
        let store = spelt(store);
        let mut cancelled = false;
        for (path, _) in found.into_iter().filter(|(_, q)| q.store == store) {
            std::fs::remove_file(&path).map_err(|e| StoreIoError::Io(path, e))?;
            cancelled = true;
        }
        Ok(cancelled)
    }
}

/// The record at `path`, when there is one: an error when it is there and cannot be read.
fn read_record(path: &Path) -> Result<Option<Queued>, String> {
    match std::fs::read_to_string(path) {
        Ok(text) => serde_json::from_str(&text).map(Some).map_err(|e| format!("queued deletion {} could not be read: {e}", path.display())),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(e) => Err(format!("queued deletion {} could not be read: {e}", path.display())),
    }
}

/// Queue `store`'s deletion in `dir`, holding the queue; see `Held::queue`.
pub fn queue(dir: &Path, store: &Path, repository: &str) -> Result<PathBuf, StoreIoError> {
    hold(dir)?.queue(store, repository)
}

/// The deletions queued in `dir`, each with its record's path, and why any record, or the folder,
/// could not be read; no folder is no queue.
pub fn queued(dir: &Path) -> (Vec<(PathBuf, Queued)>, Vec<String>) {
    let (mut found, mut unread) = (vec![], vec![]);
    let entries = match std::fs::read_dir(dir) {
        Ok(entries) => entries,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return (found, unread),
        Err(e) => return (found, vec![format!("the queued deletions in {} could not be read: {e}", dir.display())]),
    };
    let mut paths = vec![];
    for e in entries {
        match e {
            Ok(e) if e.path().extension().is_some_and(|x| x == "json") => paths.push(e.path()),
            Ok(_) => {}
            Err(e) => unread.push(format!("the queued deletions in {} could not be read: {e}", dir.display())),
        }
    }
    paths.sort();
    for path in paths {
        match read_record(&path) {
            Ok(Some(q)) => found.push((path, q)),
            Ok(None) => {}
            Err(why) => unread.push(why),
        }
    }
    (found, unread)
}

/// Cancel `store`'s queued deletion in `dir`, holding the queue, so a carry-out under way finishes
/// first; see `Held::cancel`. A folder that cannot be looked at is an error, not an empty queue.
pub fn cancel(dir: &Path, store: &Path) -> Result<bool, StoreIoError> {
    match dir.try_exists() {
        Ok(false) => Ok(false),
        Ok(true) => hold(dir)?.cancel(store),
        Err(e) => Err(StoreIoError::Io(dir.to_path_buf(), e)),
    }
}

/// Whether `store` is queued for deletion in `dir`. A queue that cannot be read whole counts as
/// holding it, so a store is never served for want of reading its deletion.
pub fn is_queued(dir: &Path, store: &Path) -> bool {
    match dir.try_exists() {
        Ok(false) => false,
        Err(_) => true,
        Ok(true) => {
            let (found, unread) = queued(dir);
            let store = spelt(store);
            !unread.is_empty() || found.iter().any(|(_, q)| q.store == store)
        }
    }
}

/// What became of a queued deletion.
#[derive(Debug, Clone, PartialEq)]
pub enum Outcome {
    /// Deleted, or found gone; its record dropped.
    Deleted(PathBuf),
    /// Its repository still names it: kept, and left queued, since uninstall removes the project file last.
    Named(PathBuf),
    /// Something still holds its lock: left queued for next time.
    Held(PathBuf),
    /// Kept, its record kept, with why: it is more than a store, or not this repository's, or could not be removed.
    Kept(PathBuf, String),
}

/// Carry out every deletion queued in `dir` whose store nothing holds, under the queue's lock.
pub fn carry_out(dir: &Path) -> Vec<Outcome> {
    match dir.try_exists() {
        Ok(false) => return vec![],
        Ok(true) => {}
        Err(e) => return vec![Outcome::Kept(dir.to_path_buf(), format!("the queued deletions in {} could not be read: {e}", dir.display()))],
    }
    let _queue = match hold(dir) {
        Ok(held) => held,
        Err(e) => return vec![Outcome::Kept(dir.to_path_buf(), e.to_string())],
    };
    let (found, unread) = queued(dir);
    let mut out: Vec<Outcome> = unread.into_iter().map(|why| Outcome::Kept(dir.to_path_buf(), why)).collect();
    for (record, q) in found {
        let outcome = match carry_out_one(&q) {
            // Its record stays when it cannot be removed, said: the next carry-out finds the store gone and drops it.
            Outcome::Deleted(store) => match std::fs::remove_file(&record) {
                Ok(()) => Outcome::Deleted(store),
                Err(e) => Outcome::Kept(store, format!("it is deleted, but its record {} could not be removed: {e}", record.display())),
            },
            other => other,
        };
        out.push(outcome);
    }
    out
}

fn carry_out_one(q: &Queued) -> Outcome {
    let store = q.store.clone();
    let kept = |why: String| Outcome::Kept(store.clone(), why);
    // Gone, or what a stopped deletion left: nothing whose owner matters is there.
    match leftover(&store) {
        Ok(true) => {
            return match if store.exists() { std::fs::remove_dir_all(&store) } else { Ok(()) } {
                Ok(()) => Outcome::Deleted(store),
                Err(e) => kept(format!("what is left of it could not be removed: {e}")),
            };
        }
        Ok(false) => {}
        Err(e) => return kept(e.to_string()),
    }
    // Its repository still names it: uninstall has yet to remove the project file, or a person put one back.
    // Kept, and left queued: init cancels a deletion explicitly, and nothing here drops one.
    if let Ok(Some((home, _))) = project_at(Path::new(&q.repository)) {
        if home.dir().canonicalize().ok() == store.canonicalize().ok() {
            return Outcome::Named(store);
        }
    }
    let checked = || -> Result<Option<String>, String> {
        if let Some(why) = not_only_a_store(&store).map_err(|e| e.to_string())? {
            return Ok(Some(why));
        }
        let config = read_config(&store).map_err(|e| e.to_string())?;
        Ok((config.repository != q.repository).then(|| format!("it now names the repository {}, not {}", config.repository, q.repository)))
    };
    match checked() {
        Ok(None) => {}
        Ok(Some(why)) | Err(why) => return kept(why),
    }
    let lock = match super::lock(&store) {
        Ok(lock) => lock,
        Err(StoreIoError::Locked(..)) => return Outcome::Held(store),
        Err(e) => return kept(e.to_string()),
    };
    #[cfg(test)]
    tests::after_lock();
    // Again under the lock: nothing can have changed it since.
    match checked() {
        Ok(None) => {}
        Ok(Some(why)) | Err(why) => return kept(why),
    }
    match delete(&store, Some(lock)) {
        Ok(()) => Outcome::Deleted(store),
        Err(e) => kept(format!("its deletion stopped part way: {e}")),
    }
}

#[cfg(test)]
mod tests {
    thread_local! {
        /// What a test does at the moment a deletion holds the store's lock, before it looks the store over again.
        static AFTER_LOCK: std::cell::RefCell<Option<Box<dyn Fn()>>> = const { std::cell::RefCell::new(None) };
    }

    pub(super) fn after_lock() {
        AFTER_LOCK.with(|hook| {
            if let Some(hook) = &*hook.borrow() {
                hook();
            }
        });
    }

    use super::*;

    fn store_for(root: &Path, name: &str) -> (PathBuf, PathBuf, String) {
        let repo = root.join(format!("{name}-repo"));
        std::fs::create_dir_all(&repo).unwrap();
        let store = root.join(name);
        std::fs::create_dir_all(store.join("pages")).unwrap();
        let repository = super::super::repository_string(&repo).unwrap();
        std::fs::write(
            store.join(CONFIG_FILE),
            format!(r#"{{"version":2,"repository":{},"roots":{{"grimoire":"overview","rites":"rites"}},"languages":[]}}"#, serde_json::to_string(&repository).unwrap()),
        )
        .unwrap();
        (store, repo, repository)
    }

    /// # Test contract
    ///
    /// ```json
    /// {"test": "a-queued-deletion-waits-for-the-lock-then-happens", "checks": ["crates/server/src/store_io/deletions#queue-record-per-store", "crates/server/src/store_io/deletions#queue-replaces-own-record", "crates/server/src/store_io/deletions#queued-lists-records", "crates/server/src/store_io/deletions#is-queued-answers", "crates/server/src/store_io/deletions#carried-out-in-turn", "crates/server/src/store_io/deletions#held-left-queued", "crates/server/src/store_io/deletions#deleted-under-lock", "crates/server/src/store_io/deletions#deletes-under-lock"]}
    /// ```
    #[test]
    fn a_queued_deletion_waits_for_the_lock_then_happens() {
        let tmp = tempfile::tempdir().unwrap();
        let dir = tmp.path().join("deletions");
        let (store, _, repository) = store_for(tmp.path(), "held");
        queue(&dir, &store, &repository).unwrap();
        queue(&dir, &store, &repository).unwrap();
        assert_eq!(queued(&dir).0.len(), 1, "one record per store");
        assert!(is_queued(&dir, &store));
        // Held by another writer: left queued.
        let held = super::super::lock(&store).unwrap();
        assert_eq!(carry_out(&dir), vec![Outcome::Held(store.canonicalize().unwrap())]);
        assert!(store.exists() && is_queued(&dir, &store));
        drop(held);
        let canon = store.canonicalize().unwrap();
        assert_eq!(carry_out(&dir), vec![Outcome::Deleted(canon)]);
        assert!(!store.exists() && !is_queued(&dir, &store), "deleted, and its record dropped");
        assert!(carry_out(&dir).is_empty());
    }

    /// # Test contract
    ///
    /// ```json
    /// {"test": "a-store-with-judged-recipes-is-only-a-store", "checks": ["crates/server/src/store_io/deletions#refuses-a-foreign-record-name", "crates/server/src/store_io/deletions#none-when-only-a-store", "crates/server/src/store_io/deletions#deletes-under-lock", "crates/server/src/store_io/deletions#deleted-under-lock"]}
    /// ```
    #[test]
    fn a_store_with_judged_recipes_is_only_a_store() {
        let tmp = tempfile::tempdir().unwrap();
        // A recipe judged, and the main line's recipes a team's server keeps: both are the store's (req:store-layout).
        let judged = |store: &Path| {
            std::fs::create_dir_all(store.join(super::super::JUDGEMENTS_DIR)).unwrap();
            std::fs::write(store.join(super::super::JUDGEMENTS_DIR).join("button.json"), JUDGED).unwrap();
            std::fs::write(store.join(super::super::RECIPES_FILE), SENT).unwrap();
            assert!(super::super::load(store).unwrap().1.is_empty(), "both load as the store's records");
        };
        let (store, _, _) = store_for(tmp.path(), "judged");
        judged(&store);
        assert_eq!(not_only_a_store(&store).unwrap(), None);
        // A name no write makes, in the judgements as in any record folder, is still someone else's.
        std::fs::write(store.join(super::super::JUDGEMENTS_DIR).join("notes.txt"), "mine").unwrap();
        assert!(not_only_a_store(&store).unwrap().is_some_and(|why| why.contains("judgements/notes.txt")));
        std::fs::remove_file(store.join(super::super::JUDGEMENTS_DIR).join("notes.txt")).unwrap();
        // Deleted whole, under its lock.
        let held = super::super::lock(&store).unwrap();
        delete(&store, Some(held)).unwrap();
        assert!(!store.exists());
        // Queued, it is carried out the same way.
        let dir = tmp.path().join("deletions");
        let (queued_store, _, repository) = store_for(tmp.path(), "judged-queued");
        judged(&queued_store);
        queue(&dir, &queued_store, &repository).unwrap();
        assert!(matches!(&carry_out(&dir)[..], [Outcome::Deleted(_)]));
        assert!(!queued_store.exists());
    }

    const JUDGED: &str = r#"{"recipe":"button","judgements":[{"recipe":"button","fingerprint":"00000000000000aa","date":"2026-10-08","ticket":"0001","outcome":"held"}]}"#;
    const SENT: &str = r#"{"recipes":[{"id":"button","kind":"component","title":"A button","text":"How to use a button.","ingredients":[{"id":"label","says":"The text shown."}],"uses":[],"digest":"00000000000000aa"}]}"#;

    /// # Test contract
    ///
    /// ```json
    /// {"test": "a-queued-store-that-is-more-or-named-is-kept", "checks": ["crates/server/src/store_io/deletions#gone-store-removed", "crates/server/src/store_io/deletions#named-by-project-kept", "crates/server/src/store_io/deletions#more-than-a-store-kept", "crates/server/src/store_io/deletions#refuses-a-foreign-entry"]}
    /// ```
    #[test]
    fn a_queued_store_that_is_more_or_named_is_kept() {
        let tmp = tempfile::tempdir().unwrap();
        let dir = tmp.path().join("deletions");
        // A file of someone's inside: kept, and still queued.
        let (more, _, repository) = store_for(tmp.path(), "more");
        std::fs::write(more.join("notes.txt"), "mine").unwrap();
        queue(&dir, &more, &repository).unwrap();
        let out = carry_out(&dir);
        assert!(matches!(&out[..], [Outcome::Kept(_, why)] if why.contains("notes.txt")), "{out:?}");
        assert!(more.join("notes.txt").exists() && is_queued(&dir, &more));
        std::fs::remove_file(more.join("notes.txt")).unwrap();
        assert!(cancel(&dir, &more).unwrap());
        // Its repository still names it, as until uninstall removes the project file: kept, and left queued.
        let (again, repo, repository) = store_for(tmp.path(), "again");
        let project = repo.join(super::super::PROJECT_FILE);
        std::fs::write(&project, format!("store = {}\n", toml_string(&again))).unwrap();
        queue(&dir, &again, &repository).unwrap();
        assert_eq!(carry_out(&dir), vec![Outcome::Named(again.canonicalize().unwrap())]);
        assert!(again.join(CONFIG_FILE).exists() && is_queued(&dir, &again));
        // Cancelled, as init does when it takes the store up again: nothing more happens to it.
        assert!(cancel(&dir, &again).unwrap());
        assert!(!cancel(&dir, &again).unwrap(), "there is nothing left to cancel");
        std::fs::remove_file(&project).unwrap();
        assert!(carry_out(&dir).is_empty());
        assert!(again.join(CONFIG_FILE).exists());
        // A store gone already: its record dropped.
        let (gone, _, repository) = store_for(tmp.path(), "gone");
        queue(&dir, &gone, &repository).unwrap();
        std::fs::remove_dir_all(&gone).unwrap();
        assert!(matches!(&carry_out(&dir)[..], [Outcome::Deleted(_)]));
        assert!(!is_queued(&dir, &gone));
    }

    /// # Test contract
    ///
    /// ```json
    /// {"test": "a-record-that-cannot-be-read-counts-as-queued", "checks": ["crates/server/src/store_io/deletions#record-found-by-path", "crates/server/src/store_io/deletions#name-clash-numbered-sibling", "crates/server/src/store_io/deletions#is-queued-answers", "crates/server/src/store_io/deletions#unreadable-queue-holds-every-store", "crates/server/src/store_io/deletions#cancel-removes-record", "crates/server/src/store_io/deletions#cancel-fails-unreadable", "crates/server/src/store_io/deletions#unreadable-record-kept"]}
    /// ```
    #[test]
    fn a_record_that_cannot_be_read_counts_as_queued() {
        let tmp = tempfile::tempdir().unwrap();
        let dir = tmp.path().join("deletions");
        let (store, _, repository) = store_for(tmp.path(), "unread");
        let record = queue(&dir, &store, &repository).unwrap();
        std::fs::write(&record, "not json").unwrap();
        // Fails closed: never served for want of reading it, said, and nothing deleted or dropped.
        assert!(is_queued(&dir, &store));
        assert!(matches!(&carry_out(&dir)[..], [Outcome::Kept(_, why)] if why.contains("could not be read")));
        assert!(store.join(CONFIG_FILE).exists() && record.exists());
        assert!(cancel(&dir, &store).is_err(), "nor cancelled while the queue cannot be read whole");
        // A record placed where this store's would go, holding another store: not this store's, and never overwritten.
        let (other, _, _) = store_for(tmp.path(), "other");
        let theirs = serde_json::to_string(&Queued { store: spelt(&other), repository: repository.clone() }).unwrap();
        std::fs::write(&record, &theirs).unwrap();
        assert!(!is_queued(&dir, &store) && !cancel(&dir, &store).unwrap());
        let ours = queue(&dir, &store, &repository).unwrap();
        assert_ne!(ours, record, "placed beside it");
        assert_eq!(std::fs::read_to_string(&record).unwrap(), theirs);
        assert!(is_queued(&dir, &store) && is_queued(&dir, &other));
        assert!(cancel(&dir, &store).unwrap() && !is_queued(&dir, &store) && is_queued(&dir, &other));
    }

    /// # Test contract
    ///
    /// ```json
    /// {"test": "a-filesystem-root-is-never-deleted-as-a-store", "checks": ["crates/server/src/store_io/deletions#refuses-a-filesystem-root"]}
    /// ```
    #[test]
    fn a_filesystem_root_is_never_deleted_as_a_store() {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path().canonicalize().unwrap().ancestors().last().unwrap().to_path_buf();
        let why = not_only_a_store(&root).unwrap().expect("a root is refused");
        assert!(why.contains("filesystem root"), "{why}");
    }

    /// # Test contract
    ///
    /// ```json
    /// {"test": "a-link-in-a-store-is-never-followed-and-refuses-its-deletion", "checks": ["crates/server/src/store_io/deletions#refuses-a-link"]}
    /// ```
    #[test]
    fn a_link_in_a_store_is_never_followed_and_refuses_its_deletion() {
        let tmp = tempfile::tempdir().unwrap();
        let (store, _, _) = store_for(tmp.path(), "linked");
        let target = tmp.path().join("elsewhere");
        std::fs::create_dir_all(&target).unwrap();
        std::fs::write(target.join("mine.txt"), "mine").unwrap();
        let link = |at: PathBuf| {
            #[cfg(unix)]
            std::os::unix::fs::symlink(&target, at).unwrap();
            // A junction, which needs no privilege and is a link all the same.
            #[cfg(windows)]
            assert!(std::process::Command::new("cmd").args(["/C", "mklink", "/J"]).arg(&at).arg(&target).output().unwrap().status.success());
        };
        // A link among a record folder's entries, and one in the store in place of a record folder.
        link(store.join("pages").join("link"));
        let why = not_only_a_store(&store).unwrap().expect("a link is refused");
        assert!(why.contains("pages/link"), "{why}");
        std::fs::remove_dir(store.join("pages").join("link")).unwrap();
        link(store.join("tickets"));
        let why = not_only_a_store(&store).unwrap().expect("a link is refused");
        assert!(why.contains("`tickets`"), "{why}");
        assert!(target.join("mine.txt").exists(), "what a link names is left alone");
    }

    /// # Test contract
    ///
    /// ```json
    /// {"test": "only-what-a-stopped-deletion-leaves-is-left-over", "checks": ["crates/server/src/store_io/deletions#leftover-answers"]}
    /// ```
    #[test]
    fn only_what_a_stopped_deletion_leaves_is_left_over() {
        let tmp = tempfile::tempdir().unwrap();
        assert!(leftover(&tmp.path().join("no-folder")).unwrap(), "no folder");
        let (whole, _, _) = store_for(tmp.path(), "whole");
        assert!(!leftover(&whole).unwrap(), "a store still holding grimoire.json");
        std::fs::remove_file(whole.join(CONFIG_FILE)).unwrap();
        assert!(leftover(&whole).unwrap(), "an empty record folder alone");
        std::fs::write(whole.join(LOCK_FILE), "").unwrap();
        std::fs::write(whole.join(super::super::tokens::TOKENS_LOCK), "").unwrap();
        assert!(leftover(&whole).unwrap(), "the lock files and empty record folders");
        std::fs::write(whole.join("pages").join("a.json"), "{}").unwrap();
        assert!(!leftover(&whole).unwrap(), "a record folder holding a record");
        std::fs::remove_file(whole.join("pages").join("a.json")).unwrap();
        std::fs::write(whole.join("notes.txt"), "mine").unwrap();
        assert!(!leftover(&whole).unwrap(), "something else");
        let file = tmp.path().join("a-file");
        std::fs::write(&file, "x").unwrap();
        assert!(!leftover(&file).unwrap(), "not a folder");
    }

    /// # Test contract
    ///
    /// ```json
    /// {"test": "a-record-is-written-whole-or-not-at-all", "checks": ["crates/server/src/store_io/deletions#record-written-whole"]}
    /// ```
    #[test]
    fn a_record_is_written_whole_or_not_at_all() {
        let tmp = tempfile::tempdir().unwrap();
        let dir = tmp.path().join("deletions");
        let (store, _, repository) = store_for(tmp.path(), "whole");
        let blocker = |record: &Path| dir.join(format!(".{}.tmp-{}", record.file_name().unwrap().to_string_lossy(), std::process::id()));
        // A write that fails leaves no record, and no temporary file.
        let name = dir.join(record_name(&store, 0));
        std::fs::create_dir_all(blocker(&name)).unwrap();
        assert!(queue(&dir, &store, &repository).is_err());
        assert!(!name.exists() && queued(&dir).0.is_empty());
        std::fs::remove_dir(blocker(&name)).unwrap();
        // One that succeeds leaves a whole record and nothing beside it but the queue's lock.
        let record = queue(&dir, &store, &repository).unwrap();
        let held: Queued = serde_json::from_str(&std::fs::read_to_string(&record).unwrap()).unwrap();
        assert_eq!(held, Queued { store: spelt(&store), repository: repository.clone() });
        let mut names: Vec<String> = std::fs::read_dir(&dir).unwrap().map(|e| e.unwrap().file_name().to_string_lossy().to_string()).collect();
        names.sort();
        assert_eq!(names, vec![QUEUE_LOCK.to_string(), record.file_name().unwrap().to_string_lossy().to_string()]);
        // One that fails replacing it leaves the earlier record as it was.
        std::fs::create_dir_all(blocker(&record)).unwrap();
        assert!(queue(&dir, &store, "another").is_err());
        assert_eq!(serde_json::from_str::<Queued>(&std::fs::read_to_string(&record).unwrap()).unwrap(), held);
    }

    /// # Test contract
    ///
    /// ```json
    /// {"test": "queued-names-a-record-and-a-folder-it-cannot-read", "checks": ["crates/server/src/store_io/deletions#queued-names-unreadable"]}
    /// ```
    #[test]
    fn queued_names_a_record_and_a_folder_it_cannot_read() {
        let tmp = tempfile::tempdir().unwrap();
        let dir = tmp.path().join("deletions");
        let (store, _, repository) = store_for(tmp.path(), "unread");
        let record = queue(&dir, &store, &repository).unwrap();
        assert!(queued(&dir).1.is_empty(), "a readable queue names nothing");
        std::fs::write(&record, "not json").unwrap();
        let (found, unread) = queued(&dir);
        assert!(found.is_empty());
        assert!(matches!(&unread[..], [why] if why.contains("could not be read") && why.contains(&record.display().to_string())), "{unread:?}");
        // A folder that is not a folder cannot be read.
        let file = tmp.path().join("not-a-folder");
        std::fs::write(&file, "x").unwrap();
        let (found, unread) = queued(&file);
        assert!(found.is_empty());
        assert!(matches!(&unread[..], [why] if why.contains("could not be read") && why.contains(&file.display().to_string())), "{unread:?}");
    }

    /// # Test contract
    ///
    /// ```json
    /// {"test": "the-queue-lock-keeps-queue-cancel-and-carry-out-apart", "checks": ["crates/server/src/store_io/deletions#hold-queue-lock"]}
    /// ```
    #[test]
    fn the_queue_lock_keeps_queue_cancel_and_carry_out_apart() {
        let tmp = tempfile::tempdir().unwrap();
        let dir = tmp.path().join("deletions");
        let (store, _, repository) = store_for(tmp.path(), "locked");
        let held = hold(&dir).unwrap();
        // The lock is a file in the folder, and no one else can take it while it is held.
        let other = std::fs::OpenOptions::new().read(true).write(true).open(dir.join(QUEUE_LOCK)).unwrap();
        assert!(matches!(other.try_lock(), Err(std::fs::TryLockError::WouldBlock)));
        // Queue, cancel and carry_out each wait for it: each is heard finding the lock held, before it waits, and none returns while it is held.
        let done = std::sync::Arc::new(std::sync::atomic::AtomicUsize::new(0));
        let (blocked, told) = std::sync::mpsc::channel();
        BLOCKED.lock().unwrap().push((dir.clone(), blocked));
        let threads: Vec<_> = (0..3)
            .map(|i| {
                let (dir, store, repository, done) = (dir.clone(), store.clone(), repository.clone(), done.clone());
                std::thread::spawn(move || {
                    match i {
                        0 => drop(queue(&dir, &store, &repository).unwrap()),
                        1 => drop(cancel(&dir, &store).unwrap()),
                        _ => drop(carry_out(&dir)),
                    }
                    done.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
                })
            })
            .collect();
        for _ in 0..3 {
            told.recv_timeout(std::time::Duration::from_secs(30)).expect("each finds the lock held");
        }
        BLOCKED.lock().unwrap().retain(|(watched, _)| watched != &dir);
        assert!(matches!(other.try_lock(), Err(std::fs::TryLockError::WouldBlock)), "still held");
        assert_eq!(done.load(std::sync::atomic::Ordering::SeqCst), 0, "none overlaps the holder");
        drop(other);
        drop(held);
        for t in threads {
            t.join().unwrap();
        }
        assert_eq!(done.load(std::sync::atomic::Ordering::SeqCst), 3);
    }

    /// # Test contract
    ///
    /// ```json
    /// {"test": "a-store-naming-another-repository-is-kept", "checks": ["crates/server/src/store_io/deletions#other-repository-kept"]}
    /// ```
    #[test]
    fn a_store_naming_another_repository_is_kept() {
        let tmp = tempfile::tempdir().unwrap();
        let dir = tmp.path().join("deletions");
        let (store, _, _) = store_for(tmp.path(), "theirs");
        queue(&dir, &store, "somewhere-else").unwrap();
        let out = carry_out(&dir);
        assert!(matches!(&out[..], [Outcome::Kept(_, why)] if why.contains("somewhere-else") && why.contains("names the repository")), "{out:?}");
        assert!(store.join(CONFIG_FILE).exists(), "the store stays");
        assert!(is_queued(&dir, &store), "its record stays");
    }

    /// Hold `file` so that it cannot be removed, nor its folder's entries: open without sharing delete on Windows, its folder made read-only elsewhere.
    #[cfg(windows)]
    fn hold_in_place(file: &Path) -> std::fs::File {
        use std::os::windows::fs::OpenOptionsExt;
        std::fs::OpenOptions::new().read(true).share_mode(1).open(file).unwrap()
    }

    #[cfg(unix)]
    fn hold_in_place(file: &Path) -> impl Drop {
        use std::os::unix::fs::PermissionsExt;
        struct Restore(PathBuf);
        impl Drop for Restore {
            fn drop(&mut self) {
                let _ = std::fs::set_permissions(&self.0, std::fs::Permissions::from_mode(0o755));
            }
        }
        let dir = file.parent().unwrap().to_path_buf();
        std::fs::set_permissions(&dir, std::fs::Permissions::from_mode(0o555)).unwrap();
        Restore(dir)
    }

    /// # Test contract
    ///
    /// ```json
    /// {"test": "a-deleted-store-whose-record-stays-is-said-and-dropped-next-time", "checks": ["crates/server/src/store_io/deletions#record-removal-failure-said"]}
    /// ```
    #[test]
    fn a_deleted_store_whose_record_stays_is_said_and_dropped_next_time() {
        let tmp = tempfile::tempdir().unwrap();
        let dir = tmp.path().join("deletions");
        let (store, _, repository) = store_for(tmp.path(), "gone");
        let record = queue(&dir, &store, &repository).unwrap();
        let guard = hold_in_place(&record);
        let out = carry_out(&dir);
        assert!(matches!(&out[..], [Outcome::Kept(_, why)] if why.contains("is deleted, but its record") && why.contains("could not be removed")), "{out:?}");
        assert!(!store.exists(), "the store is deleted");
        assert!(record.exists(), "its record is left");
        drop(guard);
        assert!(matches!(&carry_out(&dir)[..], [Outcome::Deleted(_)]));
        assert!(!record.exists(), "a later carry-out drops it");
    }

    /// # Test contract
    ///
    /// ```json
    /// {"test": "a-deletion-that-stopped-part-way-stays-queued-with-why", "checks": ["crates/server/src/store_io/deletions#stopped-deletion-stays-queued"]}
    /// ```
    #[test]
    fn a_deletion_that_stopped_part_way_stays_queued_with_why() {
        let tmp = tempfile::tempdir().unwrap();
        let dir = tmp.path().join("deletions");
        let (store, _, repository) = store_for(tmp.path(), "stuck");
        let held_record = store.join("pages").join("a.json");
        std::fs::write(&held_record, "{}").unwrap();
        queue(&dir, &store, &repository).unwrap();
        let guard = hold_in_place(&held_record);
        let out = carry_out(&dir);
        assert!(matches!(&out[..], [Outcome::Kept(_, why)] if why.contains("its deletion stopped part way")), "{out:?}");
        assert!(is_queued(&dir, &store), "left queued");
        assert!(store.join(CONFIG_FILE).exists(), "grimoire.json goes last, so a rerun can load it");
        drop(guard);
    }

    /// # Test contract
    ///
    /// ```json
    /// {"test": "a-store-that-gained-something-while-the-lock-was-taken-is-kept", "checks": ["crates/server/src/store_io/deletions#checks-repeated-under-lock"]}
    /// ```
    #[test]
    fn a_store_that_gained_something_while_the_lock_was_taken_is_kept() {
        let tmp = tempfile::tempdir().unwrap();
        let dir = tmp.path().join("deletions");
        let (store, _, repository) = store_for(tmp.path(), "changing");
        queue(&dir, &store, &repository).unwrap();
        let foreign = store.join("tickets").join("notes.txt");
        std::fs::create_dir_all(foreign.parent().unwrap()).unwrap();
        // Added the moment the lock is held: after the first look at the store, before the second.
        let added = foreign.clone();
        AFTER_LOCK.with(|hook| *hook.borrow_mut() = Some(Box::new(move || std::fs::write(&added, "mine").unwrap())));
        let out = carry_out(&dir);
        AFTER_LOCK.with(|hook| *hook.borrow_mut() = None);
        assert!(matches!(&out[..], [Outcome::Kept(_, why)] if why.contains("notes.txt")), "{out:?}");
        assert!(store.join(CONFIG_FILE).exists() && foreign.exists(), "nothing was deleted");
        assert!(is_queued(&dir, &store), "and the record stays");
    }

    fn toml_string(p: &Path) -> String {
        toml::Value::String(p.to_string_lossy().to_string()).to_string()
    }
}
