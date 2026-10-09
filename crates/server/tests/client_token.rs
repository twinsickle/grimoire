//! What a command sends a daemon to be let in: GRIMOIRE_TOKEN as a bearer token.
// An integration test is not cfg(test), so clippy's tests exemption misses it.
#![allow(clippy::unwrap_used)]

use std::io::{Read, Write};
use std::net::TcpListener;
use std::time::Duration;

use grimoire_server::client::{call, TOKEN_ENV};

/// # Test contract
///
/// ```json
/// {"test": "a-call-carries-the-token-in-the-environment", "checks": ["crates/server/src/client#call-carries-token"]}
/// ```
#[test]
fn a_call_carries_the_token_in_the_environment() {
    // A daemon stand-in that keeps the head of every request.
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let port = listener.local_addr().unwrap().port();
    let (sent, seen) = std::sync::mpsc::channel::<String>();
    std::thread::spawn(move || {
        for mut s in listener.incoming().flatten() {
            let mut buf = [0u8; 4096];
            let n = s.read(&mut buf).unwrap_or(0);
            let _ = sent.send(String::from_utf8_lossy(&buf[..n]).split("\r\n\r\n").next().unwrap_or_default().to_string());
            let body = "{}";
            let _ = write!(s, "HTTP/1.1 200 OK\r\ncontent-type: application/json\r\ncontent-length: {}\r\n\r\n{body}", body.len());
        }
    });
    let head = || seen.recv_timeout(Duration::from_secs(5)).unwrap();
    // This binary holds only this test, so changing the environment disturbs no other.
    std::env::set_var(TOKEN_ENV, " grm_secret \n");
    call(port, "status", &serde_json::json!({})).unwrap();
    let with = head();
    assert!(with.lines().any(|l| l == "Authorization: Bearer grm_secret"), "{with}");
    // Not set, or blank: no credential is sent.
    for unset in [None, Some("  ")] {
        match unset {
            Some(blank) => std::env::set_var(TOKEN_ENV, blank),
            None => std::env::remove_var(TOKEN_ENV),
        }
        call(port, "status", &serde_json::json!({})).unwrap();
        let without = head();
        assert!(!without.to_ascii_lowercase().contains("authorization"), "{without}");
    }
}
