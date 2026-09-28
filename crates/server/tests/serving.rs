//! The daemon as a client sees it: the router served on a free port, spoken
//! to over plain HTTP/1.0 for the JSON API, the event stream and MCP.
// An integration test is not cfg(test), so clippy's tests exemption misses it.
#![allow(clippy::unwrap_used)]

use std::io::{Read, Write};
use std::net::TcpStream;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use grimoire_core::model::{Scope, StoreConfig};
use grimoire_core::Store;
use grimoire_server::daemon::Daemon;
use grimoire_server::{http, store_io};
use serde_json::{json, Value};

struct Served {
    dir: PathBuf,
    port: u16,
    daemon: Arc<Daemon>,
    /// Held, not read: dropping it stops the server.
    _runtime: tokio::runtime::Runtime,
}

impl Drop for Served {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.dir);
    }
}

/// A store over a one-file repository, served with no UI directory.
fn serve() -> Served {
    let nanos = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_nanos()).unwrap_or(0);
    let dir = std::env::temp_dir().join(format!("grimoire-serving-{}-{nanos}", std::process::id()));
    let repo = dir.join("repo");
    std::fs::create_dir_all(repo.join("src")).unwrap();
    std::fs::write(repo.join("Cargo.toml"), "[package]\nname = \"s\"\nversion = \"0.1.0\"\n").unwrap();
    std::fs::write(repo.join("src").join("lib.rs"), "pub fn s() {}\n").unwrap();
    let store = dir.join("store");
    let config = StoreConfig { version: StoreConfig::CURRENT_VERSION, repository: repo.to_string_lossy().replace('\\', "/"), roots: vec!["overview".into()], languages: vec!["rust".into()], scope: Scope::default() };
    store_io::write_all(&store, &Store::new(config), false).unwrap();
    let daemon = Arc::new(Daemon::open(&store).unwrap().0);
    let runtime = tokio::runtime::Runtime::new().unwrap();
    let listener = runtime.block_on(tokio::net::TcpListener::bind("127.0.0.1:0")).unwrap();
    let port = listener.local_addr().unwrap().port();
    let app = http::router(daemon.clone(), None);
    runtime.spawn(async move { axum::serve(listener, app).await });
    Served { dir, port, daemon, _runtime: runtime }
}

/// One HTTP/1.0 exchange: the status code and the body.
fn request(port: u16, method: &str, path: &str, body: Option<&Value>) -> (u16, String) {
    let mut s = TcpStream::connect(("127.0.0.1", port)).unwrap();
    s.set_read_timeout(Some(Duration::from_secs(10))).unwrap();
    let body = body.map(|b| b.to_string()).unwrap_or_default();
    write!(s, "{method} {path} HTTP/1.0\r\nHost: 127.0.0.1\r\nContent-Type: application/json\r\nAccept: application/json, text/event-stream\r\nContent-Length: {}\r\n\r\n{body}", body.len()).unwrap();
    let mut answer = String::new();
    s.read_to_string(&mut answer).unwrap();
    let (head, body) = answer.split_once("\r\n\r\n").unwrap();
    (head.split_whitespace().nth(1).unwrap().parse().unwrap(), body.to_string())
}

fn json(body: &str) -> Value {
    serde_json::from_str(body).unwrap()
}

fn mcp(port: u16, id: u32, method: &str, params: Value) -> Value {
    let (code, body) = request(port, "POST", "/mcp", Some(&json!({ "jsonrpc": "2.0", "id": id, "method": method, "params": params })));
    assert_eq!(code, 200, "{body}");
    json(&body)
}

/// # Test contract
///
/// ```json
/// {"test": "the-api-mirrors-the-tools", "checks": ["crates/server/src/http#api-mirrors-tools"]}
/// ```
#[test]
fn the_api_mirrors_the_tools() {
    let s = serve();
    let (code, body) = request(s.port, "POST", "/api/put_page", Some(&json!({ "slug": "goal", "title": "Goal", "body": "b", "links": [] })));
    assert_eq!(code, 200);
    assert_eq!(json(&body)["node"]["ref"], "page:goal");
    let (code, body) = request(s.port, "GET", "/api/get?ref=page:goal", None);
    assert_eq!((code, json(&body)["node"]["title"].clone()), (200, json!("Goal")), "GET works for reads");
    let (code, body) = request(s.port, "POST", "/api/get", Some(&json!({ "ref": "page:nope" })));
    assert_eq!((code, json(&body)["error"].clone()), (404, json!("not_found")), "the status matches the error code");
    let (code, _) = request(s.port, "POST", "/api/put_page", Some(&json!({ "slug": "Bad Slug", "title": "", "body": "b", "links": [] })));
    assert_eq!(code, 400);
}

/// # Test contract
///
/// ```json
/// {"test": "no-ui-still-serves", "checks": ["crates/server/src/http#ui-optional"]}
/// ```
#[test]
fn no_ui_still_serves() {
    let s = serve();
    let (code, body) = request(s.port, "GET", "/", None);
    assert_eq!(code, 200);
    assert!(body.contains("No UI directory is configured"), "{body}");
    assert_eq!(request(s.port, "POST", "/api/lint", Some(&json!({}))).0, 200);
    assert!(mcp(s.port, 1, "tools/list", json!({}))["result"]["tools"].is_array());
}

/// # Test contract
///
/// ```json
/// {"test": "events-stream-to-a-listener", "checks": ["crates/server/src/http#events"]}
/// ```
#[test]
fn events_stream_to_a_listener() {
    let s = serve();
    let mut stream = TcpStream::connect(("127.0.0.1", s.port)).unwrap();
    stream.set_read_timeout(Some(Duration::from_millis(200))).unwrap();
    write!(stream, "GET /api/events HTTP/1.0\r\nHost: 127.0.0.1\r\n\r\n").unwrap();
    // Let the stream subscribe before the write it should announce.
    std::thread::sleep(Duration::from_millis(300));
    request(s.port, "POST", "/api/put_page", Some(&json!({ "slug": "goal", "title": "Goal", "body": "b", "links": [] })));
    let mut seen = String::new();
    let mut buf = [0u8; 4096];
    for _ in 0..50 {
        if let Ok(n) = stream.read(&mut buf) {
            seen.push_str(&String::from_utf8_lossy(&buf[..n]));
        }
        if seen.contains("page:goal") {
            break;
        }
    }
    assert!(seen.contains("text/event-stream"), "{seen}");
    assert!(seen.contains("data:") && seen.contains("page:goal"), "the write arrives as a server-sent event: {seen}");
}

/// # Test contract
///
/// ```json
/// {"test": "mcp-lists-the-core-tools", "checks": ["crates/server/src/mcp#same-tools"]}
/// ```
#[test]
fn mcp_lists_the_core_tools() {
    let s = serve();
    let listed = mcp(s.port, 1, "tools/list", json!({}))["result"]["tools"].clone();
    let listed: Vec<(String, String, Value)> = listed.as_array().unwrap().iter().map(|t| (t["name"].as_str().unwrap().to_string(), t["description"].as_str().unwrap().to_string(), t["inputSchema"].clone())).collect();
    let core: Vec<(String, String, Value)> = grimoire_core::tools::definitions().into_iter().map(|d| (d.name.to_string(), d.description.to_string(), d.input_schema)).collect();
    assert_eq!(listed, core);
}

/// # Test contract
///
/// ```json
/// {"test": "mcp-errors-carry-the-code", "checks": ["crates/server/src/mcp#errors-visible"]}
/// ```
#[test]
fn mcp_errors_carry_the_code() {
    let s = serve();
    let r = mcp(s.port, 1, "tools/call", json!({ "name": "get", "arguments": { "ref": "req:nope" } }));
    assert!(r.get("error").is_none(), "a tool error is a result, not a protocol error: {r}");
    assert_eq!(r["result"]["isError"], true);
    assert_eq!(r["result"]["structuredContent"], json!({ "error": "not_found", "message": "req:nope does not exist" }));
}

/// # Test contract
///
/// ```json
/// {"test": "mcp-keeps-no-session", "checks": ["crates/server/src/mcp#stateless"]}
/// ```
#[test]
fn mcp_keeps_no_session() {
    let first = serve();
    // No initialize, no session header: a bare call is answered.
    assert!(mcp(first.port, 1, "tools/call", json!({ "name": "status", "arguments": {} }))["result"]["isError"] != true);
    // A second daemon (as after a restart) answers the same client the same way.
    let second = serve();
    assert!(mcp(second.port, 2, "tools/call", json!({ "name": "status", "arguments": {} }))["result"]["isError"] != true);
}

/// # Test contract
///
/// ```json
/// {"test": "mcp-instructions-carry-the-rules", "checks": ["crates/server/src/mcp#instructions"]}
/// ```
#[test]
fn mcp_instructions_carry_the_rules() {
    let s = serve();
    let r = mcp(s.port, 1, "initialize", json!({ "protocolVersion": "2025-06-18", "capabilities": {}, "clientInfo": { "name": "t", "version": "0" } }));
    let text = r["result"]["instructions"].as_str().unwrap().to_string();
    for rule in ["Orient first", "Work is a ticket", "same piece of work", "`# Module contract`", "`find` before writing any reference", "Never write the store's files", "never on your own decision"] {
        assert!(text.contains(rule), "the instructions say {rule:?}: {text}");
    }
}

/// # Test contract
///
/// ```json
/// {"test": "one-answer-whichever-transport", "checks": ["req:tools-one-dispatcher"]}
/// ```
#[test]
fn one_answer_whichever_transport() {
    let s = serve();
    for (tool, input) in [("status", json!({})), ("get", json!({ "ref": "module:src" })), ("lint", json!({})), ("get", json!({ "ref": "page:nope" }))] {
        let in_process = match s.daemon.call(tool, input.clone()) {
            Ok(v) => v,
            Err(e) => serde_json::to_value(&e).unwrap(),
        };
        let (_, body) = request(s.port, "POST", &format!("/api/{tool}"), Some(&input));
        let over_mcp = mcp(s.port, 1, "tools/call", json!({ "name": tool, "arguments": input }))["result"]["structuredContent"].clone();
        assert_eq!(json(&body), in_process, "{tool} over HTTP");
        assert_eq!(over_mcp, in_process, "{tool} over MCP");
    }
}

