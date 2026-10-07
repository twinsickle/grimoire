//! # Module contract
//!
//! ```json
//! {"purpose":"Keep a daemon whose intent is a team server's current with that server: follow its event stream, sync on each event, and announce what changed and when the server stops or starts answering.","criteria":[{"id":"follows-the-server","statement":"A daemon on a project that names a server follows the server's event stream: it syncs when the stream opens and on every event, announces on its own stream what a sync changed, and when the stream cannot be opened marks the server unreachable and tries again; whenever the server stops or starts answering, the daemon announces that too, as a change to `server`."}]}
//! ```

use std::io::{BufRead, BufReader};
use std::sync::Arc;
use std::time::{Duration, Instant};

use grimoire_core::ops::Changed;
use grimoire_core::State;

use super::sync::{mark, sync_state, Answered};
use super::{RemoteError, Server};
use crate::daemon::{self, Daemon, Event};

/// How long to wait before asking a server that did not answer again.
const RETRY: Duration = Duration::from_secs(2);

impl Daemon {
    /// Fetch the server's intent now. True when the copy changed.
    pub fn sync_now(&self) -> Result<bool, RemoteError> {
        let Some(server) = self.server.clone() else { return Ok(false) };
        self.sync_with(&server)
    }

    /// Fetch the intent from `server`, the daemon's own or one called with a caller's token, and extract any
    /// languages it brings. True when the copy changed.
    pub(super) fn sync_with(&self, server: &Server) -> Result<bool, RemoteError> {
        let mut state = self.state.lock().expect("state lock");
        let languages = state.store().config.languages.clone();
        let was = answering(&state);
        let result = sync_state(server, &self.store_dir, &mut state);
        let is = answering(&state);
        let now = state.store().config.languages.clone();
        drop(state);
        self.announce_server(was, is);
        let changed = result?.changed;
        self.announce("sync", &changed);
        if now != languages {
            let _ = self.reextract_all(&now);
        }
        Ok(!changed.is_empty())
    }

    pub(super) fn announce(&self, tool: &str, changed: &[Changed]) {
        if !changed.is_empty() {
            let _ = self.events.send(Event::Store { tool: tool.to_string(), changed: changed.iter().flat_map(daemon::describe).collect() });
        }
    }

    /// Tell listeners the server stopped or started answering, or refused or took this machine's token, so an interface asks status again.
    pub(super) fn announce_server(&self, was: (bool, bool), is: (bool, bool)) {
        if was != is {
            let _ = self.events.send(Event::Store { tool: "sync".to_string(), changed: vec!["server".to_string()] });
        }
    }

    /// Say how the server failed to answer, so status shows it.
    fn mark_away(&self, answered: Answered) {
        if let Some(server) = &self.server {
            let mut state = self.state.lock().expect("state lock");
            let was = answering(&state);
            mark(&mut state, server, None, answered);
            let is = answering(&state);
            drop(state);
            self.announce_server(was, is);
        }
    }
}

/// Whether the server a state is a copy of answered the last time it was asked, and whether it refused this machine.
pub(super) fn answering(state: &State) -> (bool, bool) {
    state.server().map_or((false, false), |s| (s.reachable, s.refused))
}

/// Follow the server a daemon's intent is a copy of, for as long as the
/// daemon lives: sync when its event stream opens and on every event, and
/// when the stream cannot be opened, say so in status and try again.
pub fn follow(daemon: &Arc<Daemon>) {
    let Some(server) = daemon.server.clone() else { return };
    let daemon = Arc::downgrade(daemon);
    let spawned = std::thread::Builder::new().name("grimoire-follow".to_string()).spawn(move || loop {
        let opened = Instant::now();
        match server.events() {
            Ok(stream) => {
                // Whatever was written while nothing was followed is fetched now.
                let Some(d) = daemon.upgrade() else { return };
                if let Err(e) = d.sync_now() {
                    tracing::warn!("{e}");
                }
                drop(d);
                for line in BufReader::new(stream).lines() {
                    let Ok(line) = line else { break };
                    // Asked on every line, a keep-alive too, so a daemon let go of is followed no longer.
                    let Some(d) = daemon.upgrade() else { return };
                    if !line.starts_with("data:") {
                        continue;
                    }
                    if let Err(e) = d.sync_now() {
                        tracing::warn!("{e}");
                    }
                }
                // A stream that ends at once is not opened again in a tight loop.
                if opened.elapsed() < RETRY {
                    std::thread::sleep(RETRY);
                }
            }
            Err(e) => {
                let Some(d) = daemon.upgrade() else { return };
                // A refused token is said as that, and not read on as if the server were only away.
                let answered = Answered::of(&e).unwrap_or(Answered::No);
                if answering(&d.state.lock().expect("state lock")).0 {
                    match answered {
                        Answered::Refused => tracing::warn!("{e}"),
                        _ => tracing::warn!("{e}; reading the copy this machine holds until the server answers"),
                    }
                }
                d.mark_away(answered);
                drop(d);
                std::thread::sleep(RETRY);
            }
        }
        if daemon.strong_count() == 0 {
            return;
        }
    });
    if let Err(e) = spawned {
        tracing::error!("the server will not be followed, so a teammate's write shows only after a write here: {e}");
    }
}
