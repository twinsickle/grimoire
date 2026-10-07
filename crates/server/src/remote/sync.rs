//! # Module contract
//!
//! ```json
//! {"purpose":"Keep a machine's copy of a team server's intent: fetch the intent and make the copy hold it, joined with what is the machine's own, say in the state how the server answered, and open a daemon on the copy, reading it when the server is away.","criteria":[{"id":"sync","statement":"sync fetches the server's intent in one read, joins it with what is the machine's own, creates the copy when there is none and otherwise writes only what differs, and rewrites from the server's intent, or removes, any file of the copy that did not load, and notes in the copy which server it is of and when it was fetched; it answers the store the copy now holds and what changed."},{"id":"offline","statement":"When the server does not answer, a machine that holds a copy opens on it and goes on reading it, status naming the server as unreachable and when the copy was fetched, and every write is refused as store_unavailable naming the server, with nothing changed; a machine that holds no copy does not open, and one whose token the server refuses does not open either; a server that refuses the token a call carries is told apart from one that refuses the call, by asking it with the same token for its status, whichever call finds it out, a write among them; a daemon already serving then says so in status, as refused and not as unreachable, stays refused while the server cannot be reached to be asked again, and takes up again by itself once the server takes its token."}]}
//! ```

use std::path::Path;
use std::sync::{Arc, Mutex};

use grimoire_core::answer::ServerCopy;
use grimoire_core::export;
use grimoire_core::ops::Changed;
use grimoire_core::{State, Store};
use serde::{Deserialize, Serialize};
use tokio::sync::broadcast;

use super::{RemoteError, Server};
use crate::daemon::{self, Daemon, OpenError};
use crate::store_io::{self, StoreIoError, StoreLock};

/// The note in a machine's copy of which server it is a copy of and when it was fetched.
pub const COPY_FILE: &str = "copy.json";

/// Which server a copy is of, and when its intent was last fetched (UTC, RFC 3339).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CopyNote {
    pub server: String,
    pub fetched: String,
}

/// The note a sync left in a copy, when there is one.
pub fn copy_note(copy_dir: &Path) -> Option<CopyNote> {
    serde_json::from_str(&std::fs::read_to_string(copy_dir.join(COPY_FILE)).ok()?).ok()
}

fn now() -> String {
    let now = time::OffsetDateTime::now_utc().replace_nanosecond(0).unwrap_or(time::OffsetDateTime::UNIX_EPOCH);
    now.format(&time::format_description::well_known::Rfc3339).unwrap_or_default()
}

/// Take the write lock of a machine's copy, making its folder when this is the first sync.
pub fn lock_copy(copy_dir: &Path) -> Result<StoreLock, StoreIoError> {
    if copy_dir.join(store_io::CONFIG_FILE).is_file() {
        return store_io::lock(copy_dir);
    }
    std::fs::create_dir_all(copy_dir).map_err(|e| StoreIoError::Io(copy_dir.to_path_buf(), e))?;
    store_io::lock_new(copy_dir)
}

/// What a sync left: the store the copy now holds, what changed in it, and when it was fetched.
#[derive(Debug)]
pub struct Synced {
    pub store: Store,
    pub changed: Vec<Changed>,
    pub fetched: String,
}

/// Fetch the server's intent and make the copy at `copy_dir` hold it, joined
/// with what is the machine's own: `own`'s current ticket and starts, or the
/// copy's when `own` is none. The caller holds the copy's lock.
pub fn sync(server: &Server, copy_dir: &Path, repository: &str, own: Option<&Store>) -> Result<Synced, RemoteError> {
    let exported = server.export()?;
    let held = match copy_dir.join(store_io::CONFIG_FILE).is_file() {
        true => Some(store_io::load(copy_dir)?),
        false => None,
    };
    let none = Store::default();
    let new = exported.joined(own.or(held.as_ref().map(|(store, _)| store)).unwrap_or(&none), repository).map_err(|why| server.bad(why))?;
    let changed = match &held {
        None => {
            store_io::write_all(copy_dir, &new, false)?;
            vec![Changed::Config]
        }
        Some((old, malformed)) => {
            let mut changed = export::difference(old, &new);
            // A file that did not load is in neither store, so no difference names it: a write stopped
            // part way, or a hand's edit. The copy is the server's intent, so it is made that again.
            changed.extend(mend(malformed, &new));
            if !changed.is_empty() {
                store_io::persist(copy_dir, &new, &Changed::Many(changed.clone()))?;
            }
            changed
        }
    };
    let fetched = now();
    let note = CopyNote { server: server.address.clone(), fetched: fetched.clone() };
    let path = copy_dir.join(COPY_FILE);
    std::fs::write(&path, serde_json::to_string_pretty(&note).unwrap_or_default()).map_err(|e| StoreIoError::Io(path, e))?;
    Ok(Synced { store: new, changed, fetched })
}

/// How a server answered the last time it was asked.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum Answered {
    Yes,
    /// It did not: the machine is offline, or the server is down.
    No,
    /// It did, and refused this machine's token.
    Refused,
}

impl Answered {
    /// What an error says of the server; none when it says nothing of it.
    pub(super) fn of(e: &RemoteError) -> Option<Answered> {
        match e {
            RemoteError::Unreachable(..) => Some(Answered::No),
            RemoteError::Refused(..) => Some(Answered::Refused),
            _ => None,
        }
    }
}

/// What makes each file of a copy that did not load what the server has: the
/// record written again from `new`, or the file removed when `new` has none.
fn mend(malformed: &[grimoire_core::Malformed], new: &Store) -> Vec<Changed> {
    use grimoire_core::model::Reference;
    let mut out = Vec::new();
    for m in malformed {
        let (dir, slug) = match m.path.rsplit_once('/') {
            Some((dir, file)) => (dir, file.strip_suffix(".json").unwrap_or(file)),
            None => ("", m.path.as_str()),
        };
        out.push(match (dir, slug) {
            ("", file) if file == store_io::CURRENT_FILE => Changed::Current,
            ("", file) if file == store_io::BASELINE_FILE => Changed::Baseline,
            ("pages", s) if new.pages.contains_key(s) => Changed::Page(s.to_string()),
            ("pages", s) => Changed::Deleted(Reference::page(s)),
            ("requirements", s) if new.requirements.contains_key(s) => Changed::Requirement(s.to_string()),
            ("requirements", s) => Changed::Deleted(Reference::req(s)),
            ("tickets", s) if new.tickets.contains_key(s) => Changed::Ticket(s.to_string()),
            ("tickets", s) => Changed::Deleted(Reference::ticket(s)),
            (dir, s) if dir == store_io::PROPOSALS_DIR => Changed::Proposed(s.to_string()),
            (dir, s) if dir == store_io::JUDGEMENTS_DIR => Changed::Judged(s.to_string()),
            (dir, s) if dir == store_io::STARTS_DIR && new.starts.contains_key(s) && new.tickets.contains_key(s) => Changed::Started(s.to_string()),
            (dir, s) if dir == store_io::STARTS_DIR && new.tickets.contains_key(s) => Changed::Closed(s.to_string()),
            // A start with no ticket: its file goes as a deleted ticket's does.
            (dir, s) if dir == store_io::STARTS_DIR => Changed::Deleted(Reference::ticket(s)),
            _ => continue,
        });
    }
    out
}

/// Say on `state` which server it is a copy of and how that server answered just now.
pub(super) fn mark(state: &mut State, server: &Server, fetched: Option<String>, answered: Answered) {
    // What a lent token is answered says nothing of the machine's own.
    if server.lent {
        return;
    }
    let fetched = fetched.or_else(|| state.server().and_then(|s| s.fetched.clone()));
    // Refused stays refused until the server takes a token: one that then cannot be reached has not taken it.
    let refused = answered == Answered::Refused || (answered == Answered::No && state.server().is_some_and(|s| s.refused));
    state.set_server(Some(ServerCopy { address: server.address.clone(), fetched, reachable: answered == Answered::Yes, refused }));
}

/// What syncing a state did: what changed in the copy, and whether the
/// state held something other than what the server does, so that anything
/// worked out from it beforehand was worked out from a copy that was behind.
#[derive(Debug)]
pub struct Followed {
    pub changed: Vec<Changed>,
    pub behind: bool,
}

/// Sync the copy and the state that reads it. What is the machine's own is
/// taken from the state, so a start or a close just made here is kept.
pub fn sync_state(server: &Server, copy_dir: &Path, state: &mut State) -> Result<Followed, RemoteError> {
    let repository = state.store().config.repository.clone();
    match sync(server, copy_dir, &repository, Some(state.store())) {
        Ok(synced) => {
            let behind = synced.store != *state.store();
            if behind {
                state.set_store(synced.store, Vec::new());
            }
            mark(state, server, Some(synced.fetched), Answered::Yes);
            Ok(Followed { changed: synced.changed, behind })
        }
        Err(e) => {
            if let Some(answered) = Answered::of(&e) {
                mark(state, server, None, answered);
            }
            Err(e)
        }
    }
}

/// Read the state again from the copy on disk, after something left the two apart.
pub(super) fn reread(copy_dir: &Path, state: &mut State) {
    match store_io::load(copy_dir) {
        Ok((store, malformed)) => state.set_store(store, malformed),
        Err(e) => tracing::error!("this checkout's copy could not be read again: {e}"),
    }
}

impl Daemon {
    /// Open a checkout whose project names a server: sync its copy of the
    /// server's intent, or read the copy it holds when the server does not
    /// answer, and join it with the checkout's code as any store is joined.
    /// The caller holds the copy's lock.
    pub fn open_remote(copy_dir: &Path, repo: &Path, server: Server) -> Result<(Daemon, Vec<grimoire_extract::ExtractError>), OpenError> {
        let repository = store_io::repository_string(repo)?;
        store_io::refuse_inside(copy_dir, repo)?;
        let reachable = match sync(&server, copy_dir, &repository, None) {
            Ok(_) => true,
            // Offline with a copy: it is read as it is. With none there is nothing to read.
            Err(e @ RemoteError::Unreachable(..)) if copy_dir.join(store_io::CONFIG_FILE).is_file() => {
                tracing::warn!("{e}; reading the copy this machine holds, and writing nothing until the server answers");
                false
            }
            Err(e) => return Err(e.into()),
        };
        let mut loaded = daemon::load(copy_dir)?;
        mark(&mut loaded.state, &server, copy_note(copy_dir).map(|n| n.fetched), if reachable { Answered::Yes } else { Answered::No });
        let (events, _) = broadcast::channel(256);
        let daemon = Daemon {
            store_dir: copy_dir.to_path_buf(),
            repo: loaded.repo,
            shared: false,
            server: Some(Arc::new(server)),
            extractor: std::sync::RwLock::new(loaded.extractor),
            files: Mutex::new(loaded.files),
            state: Mutex::new(loaded.state),
            events,
            kin: Arc::default(),
            main: true,
            base: Mutex::new((None, None)),
        };
        Ok((daemon, loaded.extract_errors))
    }
}
