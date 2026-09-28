//! # Module contract
//!
//! ```json
//! {"purpose":"Find the daemon serving a store and send it a tool call, so a command run beside a daemon defers to it instead of writing the store itself.","criteria":[{"id":"defers-to-daemon","statement":"A daemon counts as serving a store only when the port named in the store's serve.json answers status for the same repository; a marker nobody answers, or one answered for another repository, is ignored."}]}
//! ```

use std::io::{Read, Write};
use std::net::{SocketAddr, TcpStream};
use std::path::Path;
use std::time::Duration;

use serde_json::Value;

use crate::store_io;

#[derive(Debug, thiserror::Error)]
pub enum ClientError {
    #[error("the daemon on port {0} could not be reached: {1}")]
    Io(u16, std::io::Error),
    #[error("the daemon on port {0} sent an answer that is not HTTP with a JSON body")]
    BadAnswer(u16),
}

/// The port of the daemon serving this store, if one is. The marker is only a
/// hint: the daemon has to answer `status` for `repository`, so a marker left
/// by a crash, or a port reused by something else, is ignored.
pub fn serving(store_dir: &Path, repository: &str) -> Option<u16> {
    let marker = store_io::read_serving(store_dir)?;
    let (ok, status) = call(marker.port, "status", &Value::Object(Default::default())).ok()?;
    (ok && status["repository"] == repository).then_some(marker.port)
}

/// Call one tool on the daemon. `Ok((true, output))` on success,
/// `Ok((false, error))` when the tool answered with its error object.
pub fn call(port: u16, tool: &str, input: &Value) -> Result<(bool, Value), ClientError> {
    let addr = SocketAddr::from(([127, 0, 0, 1], port));
    let io = |e| ClientError::Io(port, e);
    let mut stream = TcpStream::connect_timeout(&addr, Duration::from_millis(500)).map_err(io)?;
    // A write such as adopt lints the whole graph; give it time.
    stream.set_read_timeout(Some(Duration::from_secs(60))).map_err(io)?;
    let body = input.to_string();
    // HTTP/1.0: the daemon closes the connection after answering and never
    // sends a chunked body, so the answer is everything up to EOF.
    let request = format!("POST /api/{tool} HTTP/1.0\r\nHost: 127.0.0.1\r\nContent-Type: application/json\r\nContent-Length: {}\r\n\r\n{body}", body.len());
    stream.write_all(request.as_bytes()).map_err(io)?;
    let mut answer = Vec::new();
    stream.read_to_end(&mut answer).map_err(io)?;
    parse_answer(&answer).ok_or(ClientError::BadAnswer(port))
}

fn parse_answer(answer: &[u8]) -> Option<(bool, Value)> {
    let text = std::str::from_utf8(answer).ok()?;
    let (head, body) = text.split_once("\r\n\r\n")?;
    let code: u16 = head.split_whitespace().nth(1)?.parse().ok()?;
    let value = serde_json::from_str(body).ok()?;
    Some(((200..300).contains(&code), value))
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

    fn temp_store() -> std::path::PathBuf {
        let nanos = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos();
        let d = std::env::temp_dir().join(format!("grimoire-client-{}-{nanos}", std::process::id()));
        std::fs::create_dir_all(&d).unwrap();
        d
    }

    #[test]
    fn serving_when_the_daemon_answers_for_this_repository() {
        let dir = temp_store();
        let port = answering("200 OK", r#"{"repository":"C:/repo"}"#.to_string());
        store_io::write_serving(&dir, port).unwrap();
        assert_eq!(serving(&dir, "C:/repo"), Some(port));
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn another_repository_is_not_this_store() {
        let dir = temp_store();
        let port = answering("200 OK", r#"{"repository":"C:/other"}"#.to_string());
        store_io::write_serving(&dir, port).unwrap();
        assert_eq!(serving(&dir, "C:/repo"), None);
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn a_stale_marker_is_ignored() {
        let dir = temp_store();
        // Bind and drop, so nothing listens on the port the marker names.
        let port = TcpListener::bind("127.0.0.1:0").unwrap().local_addr().unwrap().port();
        store_io::write_serving(&dir, port).unwrap();
        assert_eq!(serving(&dir, "C:/repo"), None);
        assert_eq!(serving(&temp_store(), "C:/repo"), None, "no marker at all");
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn a_tool_error_comes_back_as_its_object() {
        let port = answering("404 Not Found", r#"{"error":"not_found","message":"req:nope does not exist"}"#.to_string());
        let (ok, v) = call(port, "get", &serde_json::json!({ "ref": "req:nope" })).unwrap();
        assert!(!ok);
        assert_eq!(v["error"], "not_found");
    }
}
