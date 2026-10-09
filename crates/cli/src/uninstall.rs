//! # Module contract
//!
//! ```json
//! {"purpose":"Take Grimoire back out of a repository on the uninstall command, and delete its store when asked, now or, while something holds it, queued for later.","criteria":[{"id":"project-found","statement":"uninstall finds the project from the nearest .grimoire.toml above the working directory, or the --repo directory's own."},{"id":"malformed-project-refused","statement":"uninstall refuses a malformed .grimoire.toml."},{"id":"store-kept","statement":"uninstall keeps the store unless asked, printing where it is."},{"id":"server-copy-kept","statement":"On a project that names a server, uninstall keeps the checkout's copy of the server's intent."},{"id":"server-store-not-deleted","statement":"On a project that names a server, uninstall refuses --delete-store before removing anything, the store being the server's."},{"id":"project-file-last","statement":"uninstall removes .grimoire.toml last of all, after the store when it deletes one, so a failure part way leaves it for a rerun."},{"id":"store-deleted-when-asked","statement":"With --delete-store, uninstall removes the store when none of the refusals that follow applies."},{"id":"not-the-repository-or-a-root","statement":"With --delete-store, uninstall removes the store only when the store is not the repository, an ancestor of it or a root."},{"id":"only-a-store","statement":"With --delete-store, uninstall removes the store only when it holds nothing a store does not, its record folders included."},{"id":"names-this-repository","statement":"With --delete-store, uninstall removes the store only when the store names this repository."},{"id":"not-served","statement":"With --delete-store, uninstall removes the store only when it is neither answered for by a daemon nor marked as served by one still holding the store's lock; a marker left by a daemon that no longer runs, the lock free, is stale and goes with the store."},{"id":"default-grimoire-refused","statement":"With --delete-store, uninstall refuses a store a daemon serves as its own default."},{"id":"library-asked-to-let-go","statement":"With --delete-store, a daemon serving the store in its machine's library is asked to let it go once every other check has passed."},{"id":"library-told-when-done","statement":"With --delete-store, a daemon serving the store in its machine's library is told when uninstall is done with it, however it ends."},{"id":"checked-before-removing","statement":"With --delete-store, uninstall checks everything before removing anything."},{"id":"checked-again-before-deletion","statement":"With --delete-store, uninstall checks everything again just before the deletion."},{"id":"store-inside-repository-refused","statement":"With --delete-store, uninstall refuses a store inside its repository."},{"id":"store-lock-held","statement":"With --delete-store, uninstall holds the store's lock from the first check."},{"id":"tokens-lock-held","statement":"With --delete-store, uninstall holds the tokens' lock while it removes."},{"id":"records-first-grimoire-json-last","statement":"Under the locks, uninstall removes the records and the tokens first and grimoire.json last, so a store stopped part way still loads for a rerun."},{"id":"lock-files-and-folder-last","statement":"After the records and grimoire.json, uninstall removes the lock files, and the folder once the lock is released."},{"id":"stopped-deletion-finished","statement":"What a deletion stopped after grimoire.json went leaves, no folder or one holding only the lock files and empty record folders, a rerun removes and finishes."},{"id":"queues-when-lock-held","statement":"uninstall --delete-store, every check passed but the store's lock held by another, queues the store's deletion in the machine's deletions folder in place of refusing."},{"id":"queue-held-while-asking-served","statement":"uninstall --delete-store holds the queue while it asks again whether a daemon marks the store as served and queues, before removing anything."},{"id":"integration-and-project-file-removed","statement":"uninstall --delete-store, once it has queued the deletion, removes the integration and the project file."},{"id":"says-queued-and-when","statement":"uninstall --delete-store, once it has queued the deletion, says the deletion is queued and when it happens."},{"id":"commands-carry-out-queued","statement":"Every command but init with a store, when it starts, carries out the queued deletions."},{"id":"says-what-became-of-each","statement":"A command that carries out the queued deletions says what became of each but those still held."},{"id":"init-cancels-queued","statement":"init with a store cancels a queued deletion of that store before writing anything, saying so."},{"id":"init-then-carries-out-others","statement":"init with a store, once it has cancelled a queued deletion of that store, carries out the others."}]}
//! ```

use std::path::{Path, PathBuf};

use anyhow::{bail, Context};
use grimoire_server::client;
use grimoire_server::store_io::{self, deletions};

use crate::{canonical, cwd, init, team};

/// Remove what init wrote. Without `--repo`, the project is the nearest
/// `.grimoire.toml` above the working directory; with it, that directory's
/// own. Every check runs before anything is removed.
pub(crate) fn uninstall(repo: Option<PathBuf>, delete_store: bool) -> anyhow::Result<()> {
    let (repo, store) = match repo {
        None => {
            let (dir, store) = store_io::find_project(&cwd()?)?;
            (dir, Some(store))
        }
        Some(r) => {
            let r = r.canonicalize().with_context(|| format!("{} does not exist", r.display()))?;
            if r.join(store_io::PROJECT_FILE).is_file() {
                let (dir, store) = store_io::find_project(&r)?;
                (dir, Some(store))
            } else {
                (r, None)
            }
        }
    };
    // On a project that names a server the store is the server's: what is here is this checkout's copy of it.
    let on_server = team::names_a_server(&repo);
    if on_server && delete_store {
        bail!("this project names a server, so its store is not on this machine and there is none to delete; run uninstall without --delete-store. Nothing was removed");
    }
    // The store's lock, taken once it is known to be only a store and held to
    // the end, so no daemon can start on it while it is being deleted.
    let mut lock = None;
    // What a deletion that stopped part way left, which a rerun finishes without the checks a store needs.
    let mut leftover = false;
    // A store a daemon's library let go of, handed back however uninstall ends.
    let mut _released = None;
    // Where the store's deletion was queued, and why the store's lock could not be taken, when it was.
    let mut queued_at: Option<(PathBuf, String)> = None;
    if delete_store {
        let Some(store) = &store else { bail!("{} names no store, so there is none to delete; nothing was removed", repo.display()) };
        if deletions::leftover(store)? {
            leftover = true;
        } else {
            if let Some(why) = refuse_store_deletion(store, &repo, true, false)? {
                bail!("not deleting the store: {why}. Nothing was removed");
            }
            _released = release_from_library(store)?;
            // Held still, every check passed: the deletion is queued and happens once nothing holds it (req:store-deletion-queued).
            // Under the queue's lock, a daemon's marker asked again there: a serve starting marks the store before it asks the
            // queue under the same lock, so one of the two always sees the other.
            let queue = deletions::hold(&deletions::deletions_dir()?)?;
            lock = match store_io::lock(store) {
                Ok(held) => Some(held),
                Err(e @ store_io::StoreIoError::Locked(..)) => {
                    if let Some(why) = refuse_store_deletion(store, &repo, true, false)? {
                        bail!("not deleting the store: {why}. Nothing was removed");
                    }
                    queued_at = Some((queue.queue(store, &canonical(&repo)?)?, e.to_string()));
                    None
                }
                Err(e) => bail!("not deleting the store: {e}. Nothing was removed"),
            };
        }
    }
    let plan = init::plan_uninstall(&repo);
    let print = |u: &init::Undone| match u {
        init::Undone::Removed(p) => println!("removed {}", p.display()),
        init::Undone::Changed(p) => println!("changed {} (Grimoire's part taken out)", p.display()),
        init::Undone::Left { path, note } => println!("left    {}: {note}", path.display()),
        init::Undone::Absent(p) => println!("absent  {}", p.display()),
    };
    match plan.apply(&repo) {
        Ok(done) => done.iter().for_each(print),
        Err(stopped) => {
            stopped.done.iter().for_each(print);
            bail!("stopped part way: {}. What is listed above was done; .grimoire.toml is kept, so running uninstall again finishes the rest", stopped.error);
        }
    }
    match &store {
        Some(store) if delete_store && leftover => {
            if store.exists() {
                std::fs::remove_dir_all(store).map_err(|e| {
                    anyhow::anyhow!("the integration is removed, but not what is left of the store at {}: {e}. .grimoire.toml is kept, so running uninstall again finishes the rest", store.display())
                })?;
            }
            println!("removed store {} (what a deletion stopped part way had left)", store.display());
        }
        Some(store) if delete_store && queued_at.is_some() => {
            let (record, why) = queued_at.as_ref().expect("queued");
            println!("queued  deletion of store {}: {why}. It is deleted once nothing holds it, by the daemon's library or the next grimoire command ({})", store.display(), record.display());
        }
        Some(store) if delete_store => {
            // Checked again at the last moment, under the lock taken after the first check and the library's release.
            if let Some(why) = refuse_store_deletion(store, &repo, false, true)? {
                bail!("the integration is removed, but not the store: {why}. .grimoire.toml is kept, so running uninstall again finishes the rest");
            }
            deletions::delete(store, lock.take())
                .map_err(|e| anyhow::anyhow!("the integration is removed, but the store was not fully deleted: {e}. .grimoire.toml is kept, so running uninstall again finishes the rest"))?;
            println!("removed store {}", store.display());
        }
        Some(copy) if on_server => println!("kept    this checkout's copy of the server's intent at {}", copy.display()),
        Some(store) => println!("kept    store {} (--delete-store removes it)", store.display()),
        None => {}
    }
    // Last, so any failure above leaves the project file naming the store for a rerun.
    let project = repo.join(store_io::PROJECT_FILE);
    if project.is_file() {
        std::fs::remove_file(&project)?;
        println!("removed {}", project.display());
    } else {
        println!("absent  {}", project.display());
    }
    Ok(())
}

/// Why `store` may not be deleted on behalf of `repo`, if it may not: it is
/// the repository, holds it, is a root, holds anything a store does not,
/// names another repository, or a daemon is serving it.
/// `in_library`: a store a daemon serves in its machine's library is not refused
/// here, since the library is asked to let it go next (`release_from_library`).
/// `held`: this command holds the store's lock, so no daemon can.
fn refuse_store_deletion(store: &Path, repo: &Path, in_library: bool, held: bool) -> anyhow::Result<Option<String>> {
    let real = store.canonicalize().with_context(|| format!("store {} does not exist", store.display()))?;
    let here = repo.canonicalize().with_context(|| format!("{} does not exist", repo.display()))?;
    if real.parent().is_none() {
        return Ok(Some(format!("{} is a filesystem root", real.display())));
    }
    if here.starts_with(&real) {
        return Ok(Some(format!("{} is the repository or holds it", real.display())));
    }
    if let Some(why) = deletions::not_only_a_store(&real)? {
        return Ok(Some(why));
    }
    let (s, _) = store_io::load(&real)?;
    if let Err(e) = store_io::refuse_inside(&real, Path::new(&s.config.repository)) {
        return Ok(Some(e.to_string()));
    }
    let repo_str = canonical(repo)?;
    if s.config.repository != repo_str {
        return Ok(Some(format!("store {} belongs to repository {}, not {repo_str}", real.display(), s.config.repository)));
    }
    if let Some(at) = grimoire_server::client::serving(&real, &s.config.repository) {
        if in_library && !at.path.is_empty() {
            return Ok(None);
        }
        return Ok(Some(format!("a daemon is serving store {} at {at}; stop it first", real.display())));
    }
    // A marker is a daemon's only while it holds the lock, from before it opens the store until it stops:
    // one left by a daemon that was killed, the lock free, is stale and goes with the store (req:cli-uninstall).
    if real.join(store_io::SERVE_FILE).exists() && !held && store_io::lock(&real).is_err() {
        return Ok(Some(format!("store {} is marked as served ({}); stop its daemon, or remove the marker if none runs", real.display(), store_io::SERVE_FILE)));
    }
    Ok(None)
}

/// Ask a daemon serving `store` in its machine's library to let it go, so it
/// can be deleted with no daemon stopped (req:cli-uninstall). A daemon serving
/// it as its own default keeps it, and the deletion is refused.
/// The guard it returns tells the daemon, when dropped on any path out, that
/// uninstall is done with the store, so a store not deleted is served again.
fn release_from_library(store: &Path) -> anyhow::Result<Option<Released>> {
    let (s, _) = store_io::load(store)?;
    let Some(at) = client::serving(store, &s.config.repository).filter(|at| !at.path.is_empty()) else { return Ok(None) };
    let released = Released(at.addr, store.to_path_buf());
    if !client::release(at.addr, store)? {
        bail!("not deleting the store: the daemon at {} serves it as the grimoire it was started for; stop it first. Nothing was removed", at.addr);
    }
    println!("released store {} from the library of the daemon at {}", store.display(), at.addr);
    Ok(Some(released))
}

/// A store released from a daemon's library for deletion, handed back when dropped.
struct Released(std::net::SocketAddr, PathBuf);

impl Drop for Released {
    fn drop(&mut self) {
        let _ = client::release_done(self.0, &self.1);
    }
}

/// Carry out the store deletions queued on this machine whose stores nothing
/// holds, saying what became of each but those still held (req:store-deletion-queued).
pub(crate) fn carry_out_deletions() {
    let Ok(dir) = deletions::deletions_dir() else { return };
    for outcome in deletions::carry_out(&dir) {
        match outcome {
            deletions::Outcome::Deleted(store) => eprintln!("removed store {}, whose deletion was queued", store.display()),
            deletions::Outcome::Named(store) => {
                eprintln!("kept store {} for now: its repository still names it; its deletion stays queued until the repository no longer does, or init cancels it", store.display())
            }
            deletions::Outcome::Kept(store, why) => eprintln!("warning: the queued deletion of store {} is not carried out: {why}", store.display()),
            deletions::Outcome::Held(_) => {}
        }
    }
}
