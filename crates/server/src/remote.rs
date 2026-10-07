//! # Module contract
//!
//! ```json
//! {"purpose":"Join a machine with a team's server: call the server with the person's token, and send a write there and answer it from the machine's own graph; keeping the machine's copy of the server's intent and following the server each have a module beneath.","criteria":[{"id":"checkout-stays","statement":"A call sent to the server never carries `checkout`: the caller's checkout chose a checkout on this machine and names nothing on the server, so a server older than the field is not refused it."},{"id":"token-for-the-server","statement":"A call to a server carries GRIMOIRE_TOKEN when it is set and not blank, else the token kept for that server's address in the person's credentials file, as a bearer token, and no token when there is neither; a server signed in from the credentials file reads it on every call, so a token kept by a later login is used without a restart; as_token gives the same server called with another token, for a write a caller sends through a daemon with a token of its own, and how the server answers that token is never taken for how it answers the machine's; keep_token stores a token under the address, in a file only its owner reads where the platform has such a thing, and forget_token removes it, saying whether there was one."},{"id":"writes-through","statement":"A write on a machine is sent to the server as the same tool call; refused there, the server's error is the answer and the copy is unchanged; taken, the copy is synced before the answer is given, a close or a landing naming its ticket being sent only once the daemon has synced the copy and extracted any languages that brings, so it is judged as the server's intent now reads the code, and the answer is the tool's over the machine's own graph when the write left the machine holding exactly what the server holds, and the server's otherwise, the machine's copy having been behind; when the server took the write and the copy could not then be synced, the write is still answered, with the server's answer, the machine's state is read again from its copy so that memory never holds what the copy does not, and the copy follows at the next sync, but that a start the server took is kept in the copy as the machine ran it, its current ticket and its start being nowhere else; adopt is sent with the findings the machine sees, and judge_recipe naming no fingerprint with the digest the machine's code gives the recipe, refused on the machine as not_found, sending nothing, when its code holds no such recipe."},{"id":"current-is-the-checkouts","statement":"A write that changes intent, or an update_ticket giving touches or relies_on of a ticket other than the current one, that names no ticket is sent naming the checkout's current ticket, so such an edit waits in that change's proposal on the server, and an update_ticket of anything else is sent as it was given; start_ticket on a ticket the server already has active or landing, and land_ticket naming none, are answered on the machine and never sent, and when the copy cannot be written the machine's state is read again from the copy; the current ticket and the starts are kept in the copy by the machine, survive a sync while the server has their ticket active or landing, and are never sent to the server."},{"id":"intent-digest","statement":"intent_digest names a state of a server's intent: the same for the intent the server holds and for that intent as a machine fetched it, and different once any of it changes."}]}
//! ```

use std::collections::BTreeMap;
use std::io::Read;
use std::path::Path;
use std::time::Duration;

use grimoire_core::export::Exported;
use grimoire_core::graph::Node;
use grimoire_core::model::{Kind, Reference};
use grimoire_core::ops::Changed;
use grimoire_core::store::FieldError;
use grimoire_core::{lint, tools, State, ToolError};
use serde_json::Value;

use crate::client::TOKEN_ENV;
use crate::daemon::Daemon;
use crate::store_io::{self, StoreIoError};

mod follow;
mod sync;

use follow::answering;
pub use follow::follow;
pub use sync::{copy_note, lock_copy, sync, sync_state, CopyNote, Followed, Synced, COPY_FILE};
use sync::{mark, reread, Answered};

/// The largest answer read from a server: a whole store's intent fits well inside it.
const ANSWER_LIMIT: u64 = 512 * 1024 * 1024;
/// How long one event stream is followed before it is opened again, so a
/// connection that died without saying so is not followed for ever.
const STREAM: Duration = Duration::from_secs(300);

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
        // The caller's checkout chose this machine's checkout; it names nothing on the server, which may predate it.
        let sent = match input {
            Value::Object(fields) if fields.contains_key("checkout") => Value::Object(fields.iter().filter(|(k, _)| *k != "checkout").map(|(k, v)| (k.clone(), v.clone())).collect()),
            _ => input.clone(),
        };
        self.read(request.send(sent.to_string()).map_err(|e| self.unreachable(e))?)
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

/// A tool's error object, as a server sent it, as the error it is.
fn tool_error(v: &Value) -> ToolError {
    let message = v["message"].as_str().unwrap_or("the server gave no reason").to_string();
    let details = || {
        v["details"]
            .as_array()
            .into_iter()
            .flatten()
            .map(|d| FieldError { field: d["field"].as_str().unwrap_or_default().to_string(), reason: d["reason"].as_str().unwrap_or_default().to_string() })
            .collect()
    };
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
pub fn write_through(server: &Server, copy_dir: &Path, state: &mut State, name: &str, input: Value, today: &str) -> Result<(Value, Vec<Changed>), ToolError> {
    write_through_synced(server, copy_dir, state, name, input, today, false)
}

/// Whether a write is compared with this checkout's start over its code, and so is sent only once the copy
/// is brought up to date: a close, and a landing that names its ticket.
pub(crate) fn synced_first(name: &str, input: &Value) -> bool {
    name == "close_ticket" || (name == "land_ticket" && input.get("ref").is_some_and(|r| !r.is_null()))
}

/// Why a write could not reach the server.
pub(crate) fn unavailable(e: RemoteError) -> ToolError {
    match e {
        // Refused is not away: it says how to sign in, and waiting changes nothing.
        RemoteError::Refused(..) => ToolError::unavailable(e.to_string()),
        e => ToolError::unavailable(format!("{e}; a write waits until the server answers")),
    }
}

/// `write_through`, told whether its caller has just synced the copy, as a daemon does before a close or a
/// landing so that languages the sync brings are extracted before the write is judged (req:remote-writes-through-the-server).
pub(crate) fn write_through_synced(server: &Server, copy_dir: &Path, state: &mut State, name: &str, mut input: Value, today: &str, synced: bool) -> Result<(Value, Vec<Changed>), ToolError> {
    // Whether a ticket is already being worked is the server's to say: asked first, so a start is not sent twice.
    // And a token the server refused is asked about again: the person may have signed in since, and until
    // the server takes it nothing is read or written here.
    // A close and a landing are compared with this checkout's start over its code: brought up to date first, so
    // what others changed since the start is in the comparison.
    // A landing that names no ticket lands nothing on a team and is answered here, server or not.
    // Synced by the caller just now, with the token the write carries: not again, which would bring what it has not extracted.
    if !synced && (name == "start_ticket" || synced_first(name, &input) || state.server().is_some_and(|s| s.refused)) {
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
        // update_ticket names it only for references it gives of another ticket, which wait in the current change's proposal,
        // so a title, priority or body sent to a server older than the field is not refused it; delete_node of a ticket takes none.
        let current = state.current();
        let names_ticket = match name {
            "delete_node" => !fields.get("ref").and_then(Value::as_str).is_some_and(|r| r.trim().starts_with("ticket:")),
            "update_ticket" => {
                let other = fields.get("ref").and_then(Value::as_str).map(|r| r.trim().trim_start_matches("ticket:").to_string()) != current;
                other && (fields.contains_key("touches") || fields.contains_key("relies_on"))
            }
            _ => tools::takes_ticket(name),
        };
        if names_ticket && fields.get("ticket").is_none_or(Value::is_null) {
            if let Some(current) = current {
                fields.insert("ticket".to_string(), Value::String(current));
            }
        }
        if name == "adopt" && fields.get("findings").is_none_or(Value::is_null) {
            let keys: Vec<_> = lint::lint(state.graph(), &lint::Rule::ALL).iter().map(lint::Finding::key).collect();
            fields.insert("findings".to_string(), serde_json::to_value(keys).unwrap_or_default());
        }
        // A judgement is bound to the recipe and its files by their digest, which only code gives (req:remote-writes-through-the-server).
        if name == "judge_recipe" && fields.get("fingerprint").is_none_or(Value::is_null) {
            let recipe = fields.get("ref").and_then(Value::as_str).and_then(|r| r.trim().parse::<Reference>().ok()).filter(|r| r.kind == Kind::Recipe && !r.id.contains('#'));
            if let Some(recipe) = recipe {
                match state.graph().get(&recipe) {
                    Some(Node::Recipe(x)) => {
                        fields.insert("fingerprint".to_string(), Value::String(x.digest.clone()));
                    }
                    _ => return Err(ToolError::not_found(&recipe)),
                }
            }
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
            // Behind: the server's answer is the true one, but the comparison with the start, and which recipes
            // are due, are only this checkout's to say, over its code, so its own are kept in it.
            Ok(out) if matches!(name, "close_ticket" | "land_ticket" | "judge_recipe") => {
                let mut taken = taken;
                let mine: &[&str] = if name == "judge_recipe" { &["due"] } else { &["untouched", "unnamed", "due"] };
                for key in mine.iter().copied() {
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
    /// A write on a daemon whose intent is a server's.
    pub(crate) fn call_through(&self, server: &Server, name: &str, input: Value) -> Result<Value, ToolError> {
        let today = store_io::today();
        // A close or a landing is judged over this checkout's code as the server's intent reads it: the copy is
        // synced here, outside the state's lock, so that languages the sync brings are extracted before the
        // write is judged, and the write then sends with no second sync (req:remote-writes-through-the-server).
        // Synced with the server the write goes to, so with the caller's token when it gave one.
        let synced = synced_first(name, &input);
        if synced {
            self.sync_with(server).map_err(unavailable)?;
        }
        // The follower may have synced languages in since, and not yet extracted them: the write waits for that.
        // Asked under the files' lock, which a re-extraction holds while it installs the extraction and its extractor,
        // so the two are never seen half changed.
        let mut tries = 0;
        let mut state = loop {
            let files = self.files.lock().expect("files lock");
            let state = self.state.lock().expect("state lock");
            if !synced || self.extracted_for(&state.store().config.languages) {
                drop(files);
                break state;
            }
            let wanted = state.store().config.languages.clone();
            drop(state);
            drop(files);
            // A checkout that cannot be read, or languages that keep moving, refuse the write rather than judge stale code.
            tries += 1;
            if tries > 3 || !self.reextract_all(&wanted) {
                return Err(ToolError::unavailable(format!("this checkout's code could not be read for the languages {wanted:?} the server now names, so the {name} was not sent")));
            }
        };
        let languages = state.store().config.languages.clone();
        let was = answering(&state);
        let result = write_through_synced(server, &self.store_dir, &mut state, name, input, &today, synced);
        let is = answering(&state);
        let now = state.store().config.languages.clone();
        drop(state);
        self.announce_server(was, is);
        let (mut output, changed) = result?;
        self.announce(name, &changed);
        if now != languages {
            let _ = self.reextract_all(&now);
            // configure answered with the count of new findings under the old extraction.
            if name == "configure" {
                let mut state = self.state.lock().expect("state lock");
                output["new"] = Value::from(lint::new_count(&lint::lint(state.graph(), &lint::Rule::ALL)));
            }
        }
        Ok(output)
    }

    /// Whether the extraction was read for `languages`: the extractor is the one they would build.
    fn extracted_for(&self, languages: &[String]) -> bool {
        let built = grimoire_extract::Extractor::for_languages(&self.repo, languages);
        self.extractor.read().expect("extractor lock").languages() == built.languages()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// # Test contract
    ///
    /// ```json
    /// {"test": "the-checkout-stays-on-the-machine", "checks": ["crates/server/src/remote#checkout-stays"]}
    /// ```
    #[test]
    fn the_checkout_stays_on_the_machine() {
        use std::io::{Read, Write};
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let port = listener.local_addr().unwrap().port();
        let seen = std::thread::spawn(move || {
            let (mut s, _) = listener.accept().unwrap();
            s.set_read_timeout(Some(Duration::from_secs(5))).unwrap();
            let mut got = Vec::new();
            let mut buf = [0u8; 4096];
            // Headers, then a body as long as Content-Length says, or chunks up to the last, empty one.
            while let Ok(n) = s.read(&mut buf) {
                if n == 0 {
                    break;
                }
                got.extend_from_slice(&buf[..n]);
                let text = String::from_utf8_lossy(&got).to_string();
                if let Some((head, body)) = text.split_once("\r\n\r\n") {
                    let len = head.lines().find_map(|l| l.to_lowercase().strip_prefix("content-length:").map(|v| v.trim().parse::<usize>().unwrap_or(0)));
                    if len.is_some_and(|len| body.len() >= len) || body.ends_with("0\r\n\r\n") {
                        break;
                    }
                }
            }
            let _ = s.write_all(b"HTTP/1.1 200 OK\r\ncontent-type: application/json\r\ncontent-length: 2\r\n\r\n{}");
            // The JSON object the body holds, whether sent sized or in chunks.
            let text = String::from_utf8_lossy(&got).split_once("\r\n\r\n").map(|(_, b)| b.to_string()).unwrap_or_default();
            text.find('{').zip(text.rfind('}')).map(|(a, b)| text[a..=b].to_string()).unwrap_or_default()
        });
        let server = Server::new(&format!("http://127.0.0.1:{port}"), Some("t".into()));
        server.call("land_ticket", &serde_json::json!({ "ref": "ticket:0001", "checkout": "C:/a/worktree" })).unwrap().unwrap();
        let body: Value = serde_json::from_str(&seen.join().unwrap()).unwrap();
        assert_eq!(body, serde_json::json!({ "ref": "ticket:0001" }), "the rest of the input goes as it came");
    }

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
