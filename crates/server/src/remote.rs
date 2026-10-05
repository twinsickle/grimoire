//! # Module contract
//!
//! ```json
//! {"purpose":"Join a machine with a team's server: call the server with the person's token, keep the machine's copy of the server's intent, send a write there and answer it from the machine's own graph, and follow the server so a teammate's write arrives.","criteria":[{"id":"token-for-the-server","statement":"A call to a server carries GRIMOIRE_TOKEN when it is set and not blank, else the token kept for that server's address in the person's credentials file, as a bearer token, and no token when there is neither; a server signed in from the credentials file reads it on every call, so a token kept by a later login is used without a restart; as_token gives the same server called with another token, for a write a caller sends through a daemon with a token of its own, and how the server answers that token is never taken for how it answers the machine's; keep_token stores a token under the address, in a file only its owner reads where the platform has such a thing, and forget_token removes it, saying whether there was one."},{"id":"sync","statement":"sync fetches the server's intent in one read, joins it with what is the machine's own, creates the copy when there is none and otherwise writes only what differs, and rewrites from the server's intent, or removes, any file of the copy that did not load, and notes in the copy which server it is of and when it was fetched; it answers the store the copy now holds and what changed."},{"id":"offline","statement":"When the server does not answer, a machine that holds a copy opens on it and goes on reading it, status naming the server as unreachable and when the copy was fetched, and every write is refused as store_unavailable naming the server, with nothing changed; a machine that holds no copy does not open, and one whose token the server refuses does not open either; a server that refuses the token a call carries is told apart from one that refuses the call, by asking it with the same token for its status, whichever call finds it out, a write among them; a daemon already serving then says so in status, as refused and not as unreachable, stays refused while the server cannot be reached to be asked again, and takes up again by itself once the server takes its token."},{"id":"writes-through","statement":"A write on a machine is sent to the server as the same tool call; refused there, the server's error is the answer and the copy is unchanged; taken, the copy is synced before the answer is given, and the answer is the tool's over the machine's own graph when the write left the machine holding exactly what the server holds, and the server's otherwise, the machine's copy having been behind; when the server took the write and the copy could not then be synced, the write is still answered, with the server's answer, the machine's state is read again from its copy so that memory never holds what the copy does not, and the copy follows at the next sync, but that a start the server took is kept in the copy as the machine ran it, its current ticket and its start being nowhere else; adopt is sent with the findings the machine sees."},{"id":"current-is-the-checkouts","statement":"A write that changes intent and names no ticket is sent naming the checkout's current ticket; start_ticket on a ticket the server already has active or landing, and land_ticket naming none, are answered on the machine and never sent, and when the copy cannot be written the machine's state is read again from the copy; the current ticket and the starts are kept in the copy by the machine, survive a sync while the server has their ticket active or landing, and are never sent to the server."},{"id":"intent-digest","statement":"intent_digest names a state of a server's intent: the same for the intent the server holds and for that intent as a machine fetched it, and different once any of it changes."},{"id":"follows-the-server","statement":"A daemon on a project that names a server follows the server's event stream: it syncs when the stream opens and on every event, announces on its own stream what a sync changed, and when the stream cannot be opened marks the server unreachable and tries again; whenever the server stops or starts answering, the daemon announces that too, as a change to `server`."}]}
//! ```

use std::collections::BTreeMap;
use std::io::{BufRead, BufReader, Read};
use std::path::Path;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use grimoire_core::answer::ServerCopy;
use grimoire_core::export::{self, Exported};
use grimoire_core::ops::Changed;
use grimoire_core::store::FieldError;
use grimoire_core::{lint, tools, State, Store, ToolError};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use tokio::sync::broadcast;

use crate::client::TOKEN_ENV;
use crate::daemon::{self, Daemon, Event, OpenError};
use crate::store_io::{self, StoreIoError, StoreLock};

/// The note in a machine's copy of which server it is a copy of and when it was fetched.
pub const COPY_FILE: &str = "copy.json";

/// The largest answer read from a server: a whole store's intent fits well inside it.
const ANSWER_LIMIT: u64 = 512 * 1024 * 1024;
/// How long one event stream is followed before it is opened again, so a
/// connection that died without saying so is not followed for ever.
const STREAM: Duration = Duration::from_secs(300);
/// How long to wait before asking a server that did not answer again.
const RETRY: Duration = Duration::from_secs(2);

#[derive(Debug, thiserror::Error)]
pub enum RemoteError {
    #[error("the server at {0} cannot be reached: {1}")]
    Unreachable(String, String),
    #[error("the server at {0} refused this machine: {1}. Sign in with `grimoire login`, or set {TOKEN_ENV}")]
    Refused(String, String),
    #[error("the server at {0} sent an answer this build does not read: {1}")]
    BadAnswer(String, String),
    #[error(transparent)]
    Store(#[from] StoreIoError),
}

/// A team's server, and the token this caller signs in with.
#[derive(Debug)]
pub struct Server {
    pub address: String,
    token: Token,
    agent: ureq::Agent,
    /// Called with a token a caller lent for one write, not the machine's own: how the
    /// server answers it says nothing of whether the machine itself is signed in.
    lent: bool,
}

/// Where a caller's token comes from.
#[derive(Debug)]
enum Token {
    /// Given once: the environment's, or none.
    Given(Option<String>),
    /// Kept by `login` in this credentials file, read on every call, so a
    /// person who signs in again is signed in without a restart.
    Kept(std::path::PathBuf),
}

/// The token a call carries: the environment's when it is set and not blank,
/// which is how a pipeline signs in, else the one `login` kept for `address`.
pub fn token_for(env: Option<String>, credentials: &Path, address: &str) -> Option<String> {
    env.map(|t| t.trim().to_string()).filter(|t| !t.is_empty()).or_else(|| kept_tokens(credentials).remove(address))
}

fn kept_tokens(credentials: &Path) -> BTreeMap<String, String> {
    std::fs::read_to_string(credentials).ok().and_then(|text| serde_json::from_str(text.trim_start_matches('\u{feff}')).ok()).unwrap_or_default()
}

fn write_tokens(credentials: &Path, tokens: &BTreeMap<String, String>) -> std::io::Result<()> {
    use std::io::Write;
    if let Some(dir) = credentials.parent() {
        std::fs::create_dir_all(dir)?;
    }
    let mut options = std::fs::OpenOptions::new();
    options.write(true).create(true).truncate(true);
    // A token is a secret: where the platform has file modes, only its owner reads it.
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    let mut file = options.open(credentials)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        file.set_permissions(std::fs::Permissions::from_mode(0o600))?;
    }
    let mut text = serde_json::to_string_pretty(tokens).unwrap_or_default();
    text.push('\n');
    file.write_all(text.as_bytes())
}

/// Keep `token` for the server at `address`, in the person's credentials file.
pub fn keep_token(credentials: &Path, address: &str, token: &str) -> std::io::Result<()> {
    let mut tokens = kept_tokens(credentials);
    tokens.insert(address.to_string(), token.trim().to_string());
    write_tokens(credentials, &tokens)
}

/// Forget the token kept for `address`. False when there was none.
pub fn forget_token(credentials: &Path, address: &str) -> std::io::Result<bool> {
    let mut tokens = kept_tokens(credentials);
    if tokens.remove(address).is_none() {
        return Ok(false);
    }
    write_tokens(credentials, &tokens)?;
    Ok(true)
}

impl Server {
    /// The server at `address`, called with `token`.
    pub fn new(address: &str, token: Option<String>) -> Server {
        let config = ureq::Agent::config_builder()
            // A tool's error comes back as its error object under a 4xx or 5xx status: read, not thrown.
            .http_status_as_error(false)
            .timeout_connect(Some(Duration::from_secs(5)))
            // A write such as adopt lints the whole graph; give it time.
            .timeout_global(Some(Duration::from_secs(60)))
            // The certificates this machine trusts, a team's own authority among them.
            .tls_config(ureq::tls::TlsConfig::builder().root_certs(ureq::tls::RootCerts::PlatformVerifier).build())
            .build();
        Server { address: address.trim_end_matches('/').to_string(), token: Token::Given(token), agent: config.into(), lent: false }
    }

    /// The server at `address`, called with whatever token `login` has kept for it in `credentials`.
    pub fn kept(address: &str, credentials: &Path) -> Server {
        Server { token: Token::Kept(credentials.to_path_buf()), ..Server::new(address, None) }
    }

    /// The server at `address`, signed in as this person: with the environment's token, else the one `login` kept.
    pub fn signed_in(address: &str, credentials: &Path) -> Server {
        match token_for(std::env::var(TOKEN_ENV).ok(), Path::new(""), address) {
            Some(token) => Server::new(address, Some(token)),
            None => Server::kept(address, credentials),
        }
    }

    /// The same server, called with `token` in place of this caller's own: for a write a
    /// command sends through the checkout's daemon signed in as itself, as a pipeline's is.
    pub fn as_token(&self, token: String) -> Server {
        Server { address: self.address.clone(), token: Token::Given(Some(token)), agent: self.agent.clone(), lent: true }
    }

    fn token(&self) -> Option<String> {
        match &self.token {
            Token::Given(token) => token.clone(),
            Token::Kept(credentials) => token_for(None, credentials, &self.address),
        }
    }

    pub fn has_token(&self) -> bool {
        self.token().is_some()
    }

    fn unreachable(&self, e: impl std::fmt::Display) -> RemoteError {
        RemoteError::Unreachable(self.address.clone(), e.to_string())
    }

    fn bad(&self, why: impl Into<String>) -> RemoteError {
        RemoteError::BadAnswer(self.address.clone(), why.into())
    }

    /// The status and the JSON body of what the server answered.
    fn read(&self, mut response: ureq::http::Response<ureq::Body>) -> Result<(u16, Value), RemoteError> {
        let status = response.status().as_u16();
        let text = response.body_mut().with_config().limit(ANSWER_LIMIT).read_to_string().map_err(|e| self.unreachable(e))?;
        let value = serde_json::from_str(&text).map_err(|_| self.bad(format!("status {status}, and a body that is not JSON")))?;
        Ok((status, value))
    }

    /// Call one tool on the server. The inner error is the tool's error object,
    /// which is also what a server answers a caller it does not let in.
    pub fn call(&self, tool: &str, input: &Value) -> Result<Result<Value, Value>, RemoteError> {
        let (status, value) = self.post(tool, input)?;
        if (200..300).contains(&status) {
            return Ok(Ok(value));
        }
        if value.get("error").is_none() {
            return Err(self.bad(format!("status {status}, and no error object")));
        }
        // A 403 is the token refused, or the call refused to a token that may only read. Which, the
        // server says when asked with the same token for its status, which any token it takes may read.
        if status == 403 && (tool == "status" || self.post("status", &Value::Object(Default::default()))?.0 == 403) {
            return Err(RemoteError::Refused(self.address.clone(), value["message"].as_str().unwrap_or("no reason given").to_string()));
        }
        Ok(Err(value))
    }

    /// One tool call: the status and the JSON body.
    fn post(&self, tool: &str, input: &Value) -> Result<(u16, Value), RemoteError> {
        let mut request = self.agent.post(format!("{}/api/{tool}", self.address)).header("Content-Type", "application/json");
        if let Some(token) = self.token() {
            request = request.header("Authorization", format!("Bearer {token}"));
        }
        self.read(request.send(input.to_string()).map_err(|e| self.unreachable(e))?)
    }

    /// The server's whole intent, in one read.
    pub fn export(&self) -> Result<Exported, RemoteError> {
        let mut request = self.agent.get(format!("{}/api/export", self.address));
        if let Some(token) = self.token() {
            request = request.header("Authorization", format!("Bearer {token}"));
        }
        let (status, value) = self.read(request.call().map_err(|e| self.unreachable(e))?)?;
        match status {
            200 => serde_json::from_value(value).map_err(|e| self.bad(format!("its intent does not read here ({e}); is this build older than the server?"))),
            403 => Err(RemoteError::Refused(self.address.clone(), value["message"].as_str().unwrap_or("no reason given").to_string())),
            _ => Err(self.bad(format!("status {status} when asked for its intent: {}; is the server older than this build?", value["message"].as_str().unwrap_or("no reason given")))),
        }
    }

    /// The server's event stream, read until it ends or has been followed for `STREAM`.
    fn events(&self) -> Result<impl Read, RemoteError> {
        let mut request = self.agent.get(format!("{}/api/events", self.address)).header("Accept", "text/event-stream");
        if let Some(token) = self.token() {
            request = request.header("Authorization", format!("Bearer {token}"));
        }
        let response = request.config().timeout_global(None).timeout_recv_body(Some(STREAM)).build().call().map_err(|e| self.unreachable(e))?;
        match response.status().as_u16() {
            200 => Ok(response.into_body().into_reader()),
            403 => Err(RemoteError::Refused(self.address.clone(), "its event stream is closed to this token".to_string())),
            status => Err(self.bad(format!("status {status} for its event stream"))),
        }
    }
}

/// What names one state of a server's intent: a digest of the intent as the
/// server hands it over. The main line's prune carries the one it judged
/// against, and the server takes it only while its own intent still gives it.
pub fn intent_digest(intent: &Exported) -> String {
    use sha2::{Digest, Sha256};
    let bytes = serde_json::to_vec(intent).unwrap_or_default();
    Sha256::digest(&bytes).iter().map(|b| format!("{b:02x}")).collect()
}

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
enum Answered {
    Yes,
    /// It did not: the machine is offline, or the server is down.
    No,
    /// It did, and refused this machine's token.
    Refused,
}

impl Answered {
    /// What an error says of the server; none when it says nothing of it.
    fn of(e: &RemoteError) -> Option<Answered> {
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
fn mark(state: &mut State, server: &Server, fetched: Option<String>, answered: Answered) {
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
fn reread(copy_dir: &Path, state: &mut State) {
    match store_io::load(copy_dir) {
        Ok((store, malformed)) => state.set_store(store, malformed),
        Err(e) => tracing::error!("this checkout's copy could not be read again: {e}"),
    }
}

/// A tool's error object, as a server sent it, as the error it is.
fn tool_error(v: &Value) -> ToolError {
    let message = v["message"].as_str().unwrap_or("the server gave no reason").to_string();
    let details = || v["details"].as_array().into_iter().flatten().map(|d| FieldError { field: d["field"].as_str().unwrap_or_default().to_string(), reason: d["reason"].as_str().unwrap_or_default().to_string() }).collect();
    match v["error"].as_str() {
        Some("not_found") => ToolError::NotFound { message },
        Some("invalid") => ToolError::Invalid { message, details: details() },
        Some("conflict") => ToolError::Conflict { message, details: details() },
        _ => ToolError::StoreUnavailable { message },
    }
}

/// The ticket a reference names, when it names one.
fn ticket_of(input: &Value) -> Option<&str> {
    input.get("ref").and_then(Value::as_str).map(str::trim).and_then(|r| r.strip_prefix("ticket:"))
}

/// Run a write tool on a machine whose intent is a server's: sent to the
/// server, which stays the store's one writer, and answered from the
/// machine's own graph, which has the code the server does not. Answers the
/// tool's output and what changed in the copy.
pub fn write_through(server: &Server, copy_dir: &Path, state: &mut State, name: &str, mut input: Value, today: &str) -> Result<(Value, Vec<Changed>), ToolError> {
    let unavailable = |e: RemoteError| match e {
        // Refused is not away: it says how to sign in, and waiting changes nothing.
        RemoteError::Refused(..) => ToolError::unavailable(e.to_string()),
        e => ToolError::unavailable(format!("{e}; a write waits until the server answers")),
    };
    // Whether a ticket is already being worked is the server's to say: asked first, so a start is not sent twice.
    // And a token the server refused is asked about again: the person may have signed in since, and until
    // the server takes it nothing is read or written here.
    // A close and a landing are compared with this checkout's start over its code: brought up to date first, so
    // what others changed since the start is in the comparison.
    // A landing that names no ticket lands nothing on a team and is answered here, server or not.
    let names_one = input.get("ref").is_some_and(|r| !r.is_null());
    if matches!(name, "start_ticket" | "close_ticket") || (name == "land_ticket" && names_one) || state.server().is_some_and(|s| s.refused) {
        sync_state(server, copy_dir, state).map_err(unavailable)?;
    }
    // What is each checkout's own is answered here and never sent: taking up a ticket
    // already active, and a landing that names no ticket, which lands nothing on a team.
    let live = |slug: &str| state.store().tickets.get(slug).is_some_and(grimoire_core::model::Ticket::is_worked);
    let own = match name {
        "start_ticket" => ticket_of(&input).is_some_and(live),
        "land_ticket" => input.get("ref").is_none_or(Value::is_null),
        _ => false,
    };
    if own {
        let out = tools::call(state, name, input, today)?;
        let changed: Vec<Changed> = out.changed.into_iter().collect();
        if !changed.is_empty() {
            if let Err(e) = store_io::persist(copy_dir, state.store(), &Changed::Many(changed.clone())) {
                // Memory never says what the copy does not: read it again as the copy is.
                reread(copy_dir, state);
                return Err(ToolError::unavailable(format!("this checkout's copy could not be written: {e}")));
            }
        }
        return Ok((out.output, changed));
    }
    // What the machine adds: its current ticket, which the server does not keep, and for adopt the findings only code shows.
    if let Some(fields) = input.as_object_mut() {
        if tools::proposes(name) && fields.get("ticket").is_none_or(Value::is_null) && !fields.get("ref").and_then(Value::as_str).is_some_and(|r| r.trim().starts_with("ticket:")) {
            if let Some(current) = state.current() {
                fields.insert("ticket".to_string(), Value::String(current));
            }
        }
        if name == "adopt" && fields.get("findings").is_none_or(Value::is_null) {
            let keys: Vec<_> = lint::lint(state.graph(), &lint::Rule::ALL).iter().map(lint::Finding::key).collect();
            fields.insert("findings".to_string(), serde_json::to_value(keys).unwrap_or_default());
        }
    }
    let taken = match server.call(name, &input) {
        Ok(Ok(taken)) => taken,
        Ok(Err(refused)) => return Err(tool_error(&refused)),
        Err(e) => {
            if let Some(answered) = Answered::of(&e) {
                mark(state, server, None, answered);
            }
            return Err(unavailable(e));
        }
    };
    // The same tool over the machine's own graph: the write as the server made it, with what only code can add.
    let here = tools::call(state, name, input, today);
    match sync_state(server, copy_dir, state) {
        // The machine now holds exactly what the server does. When its own run of the write had left it
        // holding that already, its answer is the server's write with what only code can add; when not,
        // its copy was behind, and the server's answer is the true one.
        Ok(followed) => match here {
            Ok(out) if !followed.behind => Ok((out.output, followed.changed)),
            // Behind: the server's answer is the true one, but the comparison with the start is only this
            // checkout's to make, over its code, so its own is kept in it.
            Ok(out) if matches!(name, "close_ticket" | "land_ticket") => {
                let mut taken = taken;
                for key in ["untouched", "unnamed"] {
                    if let (Some(fields), Some(mine)) = (taken.as_object_mut(), out.output.get(key)) {
                        fields.insert(key.to_string(), mine.clone());
                    }
                }
                Ok((taken, followed.changed))
            }
            _ => Ok((taken, followed.changed)),
        },
        // The server took the write and the copy could not follow. The write was made, so it is answered,
        // with the server's answer, never refused, which would invite it a second time. The machine's own
        // run of it is dropped: memory holds what the copy does, and the next sync brings the write.
        Err(e) => {
            tracing::warn!("{name} was taken by the server, but this checkout's copy could not be synced after it: {e}");
            // But a start: what it leaves here, this checkout's current ticket and its start, is held nowhere
            // else and no later sync could bring it back. The machine's run of it is kept in the copy, the
            // ticket as the server answered it, started.
            let kept = match &here {
                Ok(out) if name == "start_ticket" => out.changed.clone().filter(|changed| store_io::persist(copy_dir, state.store(), changed).is_ok()),
                _ => None,
            };
            match kept {
                Some(changed) => Ok((taken, vec![changed])),
                None => {
                    reread(copy_dir, state);
                    Ok((taken, Vec::new()))
                }
            }
        }
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
        };
        Ok((daemon, loaded.extract_errors))
    }

    /// A write on a daemon whose intent is a server's.
    pub(crate) fn call_through(&self, server: &Server, name: &str, input: Value) -> Result<Value, ToolError> {
        let today = store_io::today();
        let mut state = self.state.lock().expect("state lock");
        let languages = state.store().config.languages.clone();
        let was = answering(&state);
        let result = write_through(server, &self.store_dir, &mut state, name, input, &today);
        let is = answering(&state);
        let now = state.store().config.languages.clone();
        drop(state);
        self.announce_server(was, is);
        let (mut output, changed) = result?;
        self.announce(name, &changed);
        if now != languages {
            self.reextract_all(&now);
            // configure answered with the count of new findings under the old extraction.
            if name == "configure" {
                let mut state = self.state.lock().expect("state lock");
                output["new"] = Value::from(lint::new_count(&lint::lint(state.graph(), &lint::Rule::ALL)));
            }
        }
        Ok(output)
    }

    /// Fetch the server's intent now. True when the copy changed.
    pub fn sync_now(&self) -> Result<bool, RemoteError> {
        let Some(server) = self.server.clone() else { return Ok(false) };
        let mut state = self.state.lock().expect("state lock");
        let languages = state.store().config.languages.clone();
        let was = answering(&state);
        let result = sync_state(&server, &self.store_dir, &mut state);
        let is = answering(&state);
        let now = state.store().config.languages.clone();
        drop(state);
        self.announce_server(was, is);
        let changed = result?.changed;
        self.announce("sync", &changed);
        if now != languages {
            self.reextract_all(&now);
        }
        Ok(!changed.is_empty())
    }

    fn announce(&self, tool: &str, changed: &[Changed]) {
        if !changed.is_empty() {
            let _ = self.events.send(Event::Store { tool: tool.to_string(), changed: changed.iter().flat_map(daemon::describe).collect() });
        }
    }

    /// Tell listeners the server stopped or started answering, or refused or took this machine's token, so an interface asks status again.
    fn announce_server(&self, was: (bool, bool), is: (bool, bool)) {
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
fn answering(state: &State) -> (bool, bool) {
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

#[cfg(test)]
mod tests {
    use super::*;

    /// # Test contract
    ///
    /// ```json
    /// {"test": "a-call-carries-the-persons-token", "checks": ["crates/server/src/remote#token-for-the-server"]}
    /// ```
    #[test]
    fn a_call_carries_the_persons_token() {
        let tmp = tempfile::Builder::new().prefix("grimoire-credentials-").tempdir().unwrap();
        let file = tmp.path().join("config").join("credentials.json");
        let (team, other) = ("https://grimoire.example", "http://127.0.0.1:4242");
        assert_eq!(token_for(None, &file, team), None, "no file, no token");
        keep_token(&file, team, " grm_team \n").unwrap();
        keep_token(&file, other, "grm_other").unwrap();
        assert_eq!(token_for(None, &file, team).as_deref(), Some("grm_team"), "kept trimmed, under its address");
        assert_eq!(token_for(None, &file, other).as_deref(), Some("grm_other"));
        assert_eq!(token_for(None, &file, "https://elsewhere.example"), None);
        // The environment's token is used in its place, as a pipeline signs in; a blank one is not a token.
        assert_eq!(token_for(Some("grm_pipeline".into()), &file, team).as_deref(), Some("grm_pipeline"));
        assert_eq!(token_for(Some("  ".into()), &file, team).as_deref(), Some("grm_team"));
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            assert_eq!(std::fs::metadata(&file).unwrap().permissions().mode() & 0o777, 0o600, "only its owner reads it");
        }
        assert!(forget_token(&file, team).unwrap());
        assert!(!forget_token(&file, team).unwrap(), "there was none left to forget");
        assert_eq!((token_for(None, &file, team), token_for(None, &file, other).as_deref()), (None, Some("grm_other")));
        // An address is the same with or without its trailing slash.
        assert_eq!(Server::new("https://grimoire.example/", None).address, team);
        assert!(!Server::new(team, None).has_token() && Server::new(team, Some("t".into())).has_token());
        // Signed in from the file, a server reads it on every call: a later login is used with no restart.
        let kept = Server::kept(other, &file);
        assert_eq!(kept.token().as_deref(), Some("grm_other"));
        keep_token(&file, other, "grm_again").unwrap();
        assert_eq!(kept.token().as_deref(), Some("grm_again"));
        forget_token(&file, other).unwrap();
        assert!(!kept.has_token());
    }
}
