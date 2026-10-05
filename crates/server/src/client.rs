//! # Module contract
//!
//! ```json
//! {"purpose":"Find the daemon serving a store and send it a tool call, so a command run beside a daemon defers to it instead of writing the store itself; and ask a daemon's library to let a store go for deletion, and to take it again after.","criteria":[{"id":"defers-to-daemon","statement":"A daemon counts as serving a store only when the port named in the store's serve.json answers status for the same repository; a marker nobody answers, or one answered for another repository, is ignored; a daemon that answers by refusing the question at its trust step is reported as serving and refusing, with its reason, never taken for no daemon; the daemon is asked at the address its marker records, and at the path of the store's grimoire in its library when the marker records one, both to probe and to call, the loopback of the same family when it listens on every address; and a call carries GRIMOIRE_TOKEN as a bearer token when it is set."},{"id":"names-its-checkout","statement":"A call to the daemon serving a store, the probe's status included, names as `checkout` the repository the store records, unless its input already names one, so a daemon serving several checkouts of the grimoire answers from that one; a call to a daemon's bare routes by port or address names none."},{"id":"release","statement":"release asks the daemon at an address to let a store go from its library and answers whether it did; release_done tells it the command is done with the store, so its library may take it again."}]}
//! ```

use std::io::{Read, Write};
use std::net::{SocketAddr, TcpStream};
use std::path::Path;
use std::time::Duration;

use serde_json::Value;

use crate::store_io;

/// The environment variable holding the token a command sends to a daemon.
pub const TOKEN_ENV: &str = "GRIMOIRE_TOKEN";

#[derive(Debug, thiserror::Error)]
pub enum ClientError {
    #[error("the daemon on port {0} could not be reached: {1}")]
    Io(u16, std::io::Error),
    #[error("the daemon on port {0} sent an answer that is not HTTP with a JSON body")]
    BadAnswer(u16),
}

/// Where a store's grimoire answers: the daemon's address, and the path of
/// the grimoire in its library, empty for the bare routes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct At {
    pub addr: SocketAddr,
    pub path: String,
    /// The checkout every call names, so a daemon serving several checkouts of the grimoire answers from this one.
    pub checkout: Option<String>,
}

impl std::fmt::Display for At {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}{}", self.addr, self.path)
    }
}

/// What asking the daemon a store's marker names found.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Probe {
    /// No marker, one nobody answers, or one answered for another repository: no daemon serves the store.
    None,
    /// The daemon serving this store, and where it answers.
    Serving(At),
    /// A daemon answers there and refused the question at its trust step, with
    /// its reason: a shared daemon asked with no token, or with one it does
    /// not hold. It serves the store; this command is not let in.
    Refused(At, String),
}

/// Ask the daemon the store's marker names. The marker is only a hint: the
/// daemon has to answer `status` for `repository`, so a marker left by a
/// crash, or a port reused by something else, is ignored.
pub fn probe(store_dir: &Path, repository: &str) -> Probe {
    let Some(marker) = store_io::read_serving(store_dir) else { return Probe::None };
    let at = At { addr: marker.addr(), path: marker.path.clone().unwrap_or_default(), checkout: Some(repository.to_string()) };
    match exchange(&at, "status", &Value::Object(Default::default())) {
        Ok((code, status)) if (200..300).contains(&code) && status["repository"] == repository => Probe::Serving(at),
        // The trust step's own refusal, as a daemon gives it; anything else on that port is not ours.
        Ok((403, refusal)) if refusal["error"] == "invalid" => Probe::Refused(at, refusal["message"].as_str().unwrap_or("refused").to_string()),
        _ => Probe::None,
    }
}

/// Where the daemon serving this store answers for it, if one does and it lets this command in.
pub fn serving(store_dir: &Path, repository: &str) -> Option<At> {
    match probe(store_dir, repository) {
        Probe::Serving(at) => Some(at),
        _ => None,
    }
}

/// Call one tool on the daemon on this machine's `port`. `Ok((true, output))`
/// on success, `Ok((false, error))` when the tool answered with its error object.
pub fn call(port: u16, tool: &str, input: &Value) -> Result<(bool, Value), ClientError> {
    call_at(SocketAddr::from(([127, 0, 0, 1], port)), tool, input)
}

/// The same, for a daemon at `addr`, on its bare routes.
pub fn call_at(addr: SocketAddr, tool: &str, input: &Value) -> Result<(bool, Value), ClientError> {
    call_to(&At { addr, path: String::new(), checkout: None }, tool, input)
}

/// The same, for the grimoire at `at`.
pub fn call_to(at: &At, tool: &str, input: &Value) -> Result<(bool, Value), ClientError> {
    let (code, value) = exchange(at, tool, input)?;
    Ok(((200..300).contains(&code), value))
}

/// Ask the daemon at `addr` to let go of `store_dir` from its machine's library,
/// so it can be deleted: true when it did, false when it serves it as its own
/// grimoire or not at all.
pub fn release(addr: SocketAddr, store_dir: &Path) -> Result<bool, ClientError> {
    let input = serde_json::json!({ "store": store_dir.to_string_lossy() });
    let (code, answer) = post(addr, "/library/release", &input)?;
    Ok((200..300).contains(&code) && answer["released"] == true)
}

/// Tell the daemon at `addr` that the command which asked for `store_dir`'s
/// release is done with it, deleted or not: its library may take it again.
pub fn release_done(addr: SocketAddr, store_dir: &Path) -> Result<(), ClientError> {
    let input = serde_json::json!({ "store": store_dir.to_string_lossy(), "done": true });
    post(addr, "/library/release", &input).map(|_| ())
}

/// One call: the status code and the JSON body.
fn exchange(at: &At, tool: &str, input: &Value) -> Result<(u16, Value), ClientError> {
    match (&at.checkout, input) {
        (Some(checkout), Value::Object(fields)) if !fields.contains_key("checkout") => {
            let mut named = fields.clone();
            named.insert("checkout".to_string(), Value::String(checkout.clone()));
            post(at.addr, &format!("{}/api/{tool}", at.path), &Value::Object(named))
        }
        _ => post(at.addr, &format!("{}/api/{tool}", at.path), input),
    }
}

/// One POST of `input` to `path` on the daemon at `addr`: the status code and the JSON body.
fn post(addr: SocketAddr, path: &str, input: &Value) -> Result<(u16, Value), ClientError> {
    let port = addr.port();
    let io = |e| ClientError::Io(port, e);
    let mut stream = TcpStream::connect_timeout(&addr, Duration::from_millis(500)).map_err(io)?;
    // A write such as adopt lints the whole graph; give it time.
    stream.set_read_timeout(Some(Duration::from_secs(60))).map_err(io)?;
    let body = input.to_string();
    // HTTP/1.0: the daemon closes the connection after answering and never
    // sends a chunked body, so the answer is everything up to EOF.
    // A shared daemon admits a caller by token; a local one ignores the header.
    let token = std::env::var(TOKEN_ENV).ok().filter(|t| !t.trim().is_empty()).map(|t| format!("Authorization: Bearer {}\r\n", t.trim())).unwrap_or_default();
    let request = format!("POST {path} HTTP/1.0\r\nHost: {}\r\n{token}Content-Type: application/json\r\nContent-Length: {}\r\n\r\n{body}", addr, body.len());
    stream.write_all(request.as_bytes()).map_err(io)?;
    let mut answer = Vec::new();
    stream.read_to_end(&mut answer).map_err(io)?;
    parse_answer(&answer).ok_or(ClientError::BadAnswer(port))
}

fn parse_answer(answer: &[u8]) -> Option<(u16, Value)> {
    let text = std::str::from_utf8(answer).ok()?;
    let (head, body) = text.split_once("\r\n\r\n")?;
    let code: u16 = head.split_whitespace().nth(1)?.parse().ok()?;
    let value = serde_json::from_str(body).ok()?;
    Some((code, value))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::net::TcpListener;

    /// A one-shot daemon stand-in that answers every request with `body`.
    fn answering(status_line: &'static str, body: String) -> u16 {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let port = listener.local_addr().unwrap().port();
        std::thread::spawn(move || {
            for mut s in listener.incoming().flatten().take(2) {
                let mut buf = [0u8; 4096];
                let _ = s.read(&mut buf);
                let _ = write!(s, "HTTP/1.1 {status_line}\r\ncontent-type: application/json\r\ncontent-length: {}\r\n\r\n{body}", body.len());
            }
        });
        port
    }

    /// A fresh folder for a store, removed when dropped.
    fn temp_store() -> tempfile::TempDir {
        tempfile::Builder::new().prefix("grimoire-client-").tempdir().unwrap()
    }

    /// # Test contract
    ///
    /// ```json
    /// {"test": "a-grimoire-in-a-library-is-asked-at-its-path", "checks": ["crates/server/src/client#defers-to-daemon"]}
    /// ```
    #[test]
    fn a_grimoire_in_a_library_is_asked_at_its_path() {
        // A daemon that answers for the repository only at the grimoire's own path, as a library does.
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let port = listener.local_addr().unwrap().port();
        std::thread::spawn(move || {
            for mut s in listener.incoming().flatten().take(2) {
                let mut buf = [0u8; 4096];
                let n = s.read(&mut buf).unwrap_or(0);
                let request = String::from_utf8_lossy(&buf[..n]).to_string();
                let body = if request.starts_with("POST /g/ledger/api/") { r#"{"repository":"C:/ledger"}"# } else { r#"{"repository":"C:/shop"}"# };
                let _ = write!(s, "HTTP/1.1 200 OK\r\ncontent-type: application/json\r\ncontent-length: {}\r\n\r\n{body}", body.len());
            }
        });
        let tmp = temp_store();
        let dir = tmp.path();
        store_io::write_serving_in(dir, SocketAddr::from(([127, 0, 0, 1], port)), Some("/g/ledger")).unwrap();
        let at = serving(dir, "C:/ledger").expect("found at its path");
        assert_eq!((at.addr.port(), at.path.as_str()), (port, "/g/ledger"));
        let (ok, answer) = call_to(&at, "status", &Value::Object(Default::default())).unwrap();
        assert!(ok && answer["repository"] == "C:/ledger", "a call goes to the same path: {answer}");
    }

    /// # Test contract
    ///
    /// ```json
    /// {"test": "a-call-names-its-checkout", "checks": ["crates/server/src/client#names-its-checkout"]}
    /// ```
    #[test]
    fn a_call_names_its_checkout() {
        // A daemon that answers for the repository and keeps every request's body.
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let port = listener.local_addr().unwrap().port();
        let (sent, seen) = std::sync::mpsc::channel::<String>();
        std::thread::spawn(move || {
            for mut s in listener.incoming().flatten().take(4) {
                let mut buf = [0u8; 4096];
                let n = s.read(&mut buf).unwrap_or(0);
                let request = String::from_utf8_lossy(&buf[..n]).to_string();
                let _ = sent.send(request.split_once("\r\n\r\n").map(|(_, b)| b.to_string()).unwrap_or_default());
                let body = r#"{"repository":"C:/ledger-two"}"#;
                let _ = write!(s, "HTTP/1.1 200 OK\r\ncontent-type: application/json\r\ncontent-length: {}\r\n\r\n{body}", body.len());
            }
        });
        let tmp = temp_store();
        store_io::write_serving_in(tmp.path(), SocketAddr::from(([127, 0, 0, 1], port)), Some("/g/ledger")).unwrap();
        let body = || serde_json::from_str::<Value>(&seen.recv_timeout(Duration::from_secs(5)).unwrap()).unwrap();
        // The probe and every call through it name the repository the store records.
        let at = serving(tmp.path(), "C:/ledger-two").expect("served");
        assert_eq!(body()["checkout"], "C:/ledger-two", "the probe names it");
        call_to(&at, "lint", &serde_json::json!({})).unwrap();
        assert_eq!(body()["checkout"], "C:/ledger-two", "a call names it");
        // One the input names already is kept; a call by address alone names none.
        call_to(&at, "lint", &serde_json::json!({ "checkout": "C:/elsewhere" })).unwrap();
        assert_eq!(body()["checkout"], "C:/elsewhere");
        call_at(SocketAddr::from(([127, 0, 0, 1], port)), "status", &serde_json::json!({})).unwrap();
        assert!(body().get("checkout").is_none(), "a call to the bare routes names none");
    }

    /// # Test contract
    ///
    /// ```json
    /// {"test": "serving-when-the-daemon-answers-for-this-repository", "checks": ["crates/server/src/client#defers-to-daemon"]}
    /// ```
    #[test]
    fn serving_when_the_daemon_answers_for_this_repository() {
        let tmp = temp_store();
        let dir = tmp.path();
        let port = answering("200 OK", r#"{"repository":"C:/repo"}"#.to_string());
        store_io::write_serving(dir, port).unwrap();
        assert_eq!(serving(dir, "C:/repo").map(|at| (at.addr.port(), at.path)), Some((port, String::new())));
    }

    /// # Test contract
    ///
    /// ```json
    /// {"test": "another-repository-is-not-this-store", "checks": ["crates/server/src/client#defers-to-daemon"]}
    /// ```
    #[test]
    fn another_repository_is_not_this_store() {
        let tmp = temp_store();
        let dir = tmp.path();
        let port = answering("200 OK", r#"{"repository":"C:/other"}"#.to_string());
        store_io::write_serving(dir, port).unwrap();
        assert_eq!(serving(dir, "C:/repo"), None);
    }

    /// # Test contract
    ///
    /// ```json
    /// {"test": "a-stale-marker-is-ignored", "checks": ["crates/server/src/client#defers-to-daemon"]}
    /// ```
    #[test]
    fn a_stale_marker_is_ignored() {
        let tmp = temp_store();
        let dir = tmp.path();
        // Bind and drop, so nothing listens on the port the marker names.
        let port = TcpListener::bind("127.0.0.1:0").unwrap().local_addr().unwrap().port();
        store_io::write_serving(dir, port).unwrap();
        assert_eq!(serving(dir, "C:/repo"), None);
        assert_eq!(serving(temp_store().path(), "C:/repo"), None, "no marker at all");
    }

    #[test]
    fn a_tool_error_comes_back_as_its_object() {
        let port = answering("404 Not Found", r#"{"error":"not_found","message":"req:nope does not exist"}"#.to_string());
        let (ok, v) = call(port, "get", &serde_json::json!({ "ref": "req:nope" })).unwrap();
        assert!(!ok);
        assert_eq!(v["error"], "not_found");
    }

    /// # Test contract
    ///
    /// ```json
    /// {"test": "a-daemon-that-refuses-is-not-no-daemon", "checks": ["crates/server/src/client#defers-to-daemon"]}
    /// ```
    #[test]
    fn a_daemon_that_refuses_is_not_no_daemon() {
        let tmp = temp_store();
        let dir = tmp.path();
        // A shared daemon asked with no token answers 403 with its error object.
        let port = answering("403 Forbidden", r#"{"error":"invalid","message":"this is a shared daemon and the request carries no token"}"#.to_string());
        store_io::write_serving(dir, port).unwrap();
        match probe(dir, "C:/repo") {
            Probe::Refused(at, why) => assert!(at.addr.port() == port && why.contains("no token"), "{why}"),
            other => panic!("{other:?}"),
        }
        assert_eq!(serving(dir, "C:/repo"), None, "and it does not count as a daemon this command may use");
        // Something else on that port saying 403 is not a daemon of ours.
        let port = answering("403 Forbidden", r#"{"detail":"nope"}"#.to_string());
        store_io::write_serving(dir, port).unwrap();
        assert_eq!(probe(dir, "C:/repo"), Probe::None);
        // The marker says where the daemon listens; the loopback and every-address are reached on this machine.
        for (bound, reached) in
            [("127.0.0.1:4300", "127.0.0.1:4300"), ("0.0.0.0:4300", "127.0.0.1:4300"), ("192.0.2.7:4300", "192.0.2.7:4300"), ("[::]:4300", "[::1]:4300"), ("[::1]:4300", "[::1]:4300")]
        {
            store_io::write_serving_at(dir, bound.parse().unwrap()).unwrap();
            assert_eq!(store_io::read_serving(dir).unwrap().addr(), reached.parse().unwrap(), "{bound}");
        }
    }
}
