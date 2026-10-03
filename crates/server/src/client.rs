//! # Module contract
//!
//! ```json
//! {"purpose":"Find the daemon serving a store and send it a tool call, so a command run beside a daemon defers to it instead of writing the store itself.","criteria":[{"id":"defers-to-daemon","statement":"A daemon counts as serving a store only when the port named in the store's serve.json answers status for the same repository; a marker nobody answers, or one answered for another repository, is ignored; a daemon that answers by refusing the question at its trust step is reported as serving and refusing, with its reason, never taken for no daemon; the daemon is asked at the address its marker records, the loopback of the same family when it listens on every address; and a call carries GRIMOIRE_TOKEN as a bearer token when it is set."}]}
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

/// What asking the daemon a store's marker names found.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Probe {
    /// No marker, one nobody answers, or one answered for another repository: no daemon serves the store.
    None,
    /// The daemon serving this store, and where it answers.
    Serving(SocketAddr),
    /// A daemon answers there and refused the question at its trust step, with
    /// its reason: a shared daemon asked with no token, or with one it does
    /// not hold. It serves the store; this command is not let in.
    Refused(SocketAddr, String),
}

/// Ask the daemon the store's marker names. The marker is only a hint: the
/// daemon has to answer `status` for `repository`, so a marker left by a
/// crash, or a port reused by something else, is ignored.
pub fn probe(store_dir: &Path, repository: &str) -> Probe {
    let Some(marker) = store_io::read_serving(store_dir) else { return Probe::None };
    let addr = marker.addr();
    match exchange(addr, "status", &Value::Object(Default::default())) {
        Ok((code, status)) if (200..300).contains(&code) && status["repository"] == repository => Probe::Serving(addr),
        // The trust step's own refusal, as a daemon gives it; anything else on that port is not ours.
        Ok((403, refusal)) if refusal["error"] == "invalid" => Probe::Refused(addr, refusal["message"].as_str().unwrap_or("refused").to_string()),
        _ => Probe::None,
    }
}

/// The port of the daemon serving this store, if one is and it lets this command in.
pub fn serving(store_dir: &Path, repository: &str) -> Option<u16> {
    match probe(store_dir, repository) {
        Probe::Serving(addr) => Some(addr.port()),
        _ => None,
    }
}

/// Call one tool on the daemon on this machine's `port`. `Ok((true, output))`
/// on success, `Ok((false, error))` when the tool answered with its error object.
pub fn call(port: u16, tool: &str, input: &Value) -> Result<(bool, Value), ClientError> {
    call_at(SocketAddr::from(([127, 0, 0, 1], port)), tool, input)
}

/// The same, for a daemon at `addr`.
pub fn call_at(addr: SocketAddr, tool: &str, input: &Value) -> Result<(bool, Value), ClientError> {
    let (code, value) = exchange(addr, tool, input)?;
    Ok(((200..300).contains(&code), value))
}

/// One call: the status code and the JSON body.
fn exchange(addr: SocketAddr, tool: &str, input: &Value) -> Result<(u16, Value), ClientError> {
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
    let request = format!("POST /api/{tool} HTTP/1.0\r\nHost: {}\r\n{token}Content-Type: application/json\r\nContent-Length: {}\r\n\r\n{body}", addr, body.len());
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
    /// {"test": "serving-when-the-daemon-answers-for-this-repository", "checks": ["crates/server/src/client#defers-to-daemon"]}
    /// ```
    #[test]
    fn serving_when_the_daemon_answers_for_this_repository() {
        let tmp = temp_store();
        let dir = tmp.path();
        let port = answering("200 OK", r#"{"repository":"C:/repo"}"#.to_string());
        store_io::write_serving(dir, port).unwrap();
        assert_eq!(serving(dir, "C:/repo"), Some(port));
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
            Probe::Refused(addr, why) => assert!(addr.port() == port && why.contains("no token"), "{why}"),
            other => panic!("{other:?}"),
        }
        assert_eq!(serving(dir, "C:/repo"), None, "and it does not count as a daemon this command may use");
        // Something else on that port saying 403 is not a daemon of ours.
        let port = answering("403 Forbidden", r#"{"detail":"nope"}"#.to_string());
        store_io::write_serving(dir, port).unwrap();
        assert_eq!(probe(dir, "C:/repo"), Probe::None);
        // The marker says where the daemon listens; the loopback and every-address are reached on this machine.
        for (bound, reached) in [("127.0.0.1:4300", "127.0.0.1:4300"), ("0.0.0.0:4300", "127.0.0.1:4300"), ("192.0.2.7:4300", "192.0.2.7:4300"), ("[::]:4300", "[::1]:4300"), ("[::1]:4300", "[::1]:4300")] {
            store_io::write_serving_at(dir, bound.parse().unwrap()).unwrap();
            assert_eq!(store_io::read_serving(dir).unwrap().addr(), reached.parse().unwrap(), "{bound}");
        }
    }
}
