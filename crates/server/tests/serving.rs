//! The daemon as a client sees it: the router served on a free port, spoken
//! to over plain HTTP/1.0 for the JSON API, the event stream and MCP.
// An integration test is not cfg(test), so clippy's tests exemption misses it.
#![allow(clippy::unwrap_used)]

use std::io::{Read, Write};
use std::net::TcpStream;
use std::sync::Arc;
use std::time::Duration;

use grimoire_core::model::{Scope, StoreConfig};
use grimoire_core::Store;
use grimoire_server::daemon::Daemon;
use grimoire_server::{http, store_io};
use serde_json::{json, Value};

struct Served {
    port: u16,
    daemon: Arc<Daemon>,
    /// Held, not read: dropping it stops the server.
    _runtime: tokio::runtime::Runtime,
    /// Held, not read, and last, so the server has stopped before its folder is removed.
    _tmp: tempfile::TempDir,
}

/// A store over a one-file repository, served with no UI.
fn serve() -> Served {
    serve_with(http::Ui::None)
}

/// The same store, served with the given UI.
fn serve_with(ui: http::Ui) -> Served {
    serve_as(ui, Arc::new(grimoire_server::access::Local))
}

/// The same store, served with the given UI behind the given policy.
fn serve_as(ui: http::Ui, policy: Arc<dyn grimoire_server::access::Policy>) -> Served {
    let tmp = tempfile::Builder::new().prefix("grimoire-serving-").tempdir().unwrap();
    let dir = tmp.path().to_path_buf();
    let repo = dir.join("repo");
    std::fs::create_dir_all(repo.join("src")).unwrap();
    std::fs::write(repo.join("Cargo.toml"), "[package]\nname = \"s\"\nversion = \"0.1.0\"\n").unwrap();
    std::fs::write(repo.join("src").join("lib.rs"), "pub fn s() {}\n").unwrap();
    let store = dir.join("store");
    let config = StoreConfig { site: vec![], version: StoreConfig::CURRENT_VERSION, repository: repo.to_string_lossy().replace('\\', "/"), roots: vec!["overview".into()], languages: vec!["rust".into()], scope: Scope::default(), order: Default::default() };
    // An active, current ticket, so a write that changes intent has a proposal to land in; reads of it name the ticket.
    let mut s = Store::new(config);
    s.tickets.insert("work".into(), grimoire_core::model::Ticket { slug: "work".into(), title: "Work".into(), kind: grimoire_core::model::TicketKind::Change, priority: grimoire_core::model::Priority::Later, status: grimoire_core::model::Status::Active, touches: vec![], relies_on: vec![], body: "b".into(), decision: None });
    s.current = Some("work".into());
    store_io::write_all(&store, &s, false).unwrap();
    let daemon = Arc::new(Daemon::open(&store).unwrap().0);
    let runtime = tokio::runtime::Runtime::new().unwrap();
    let listener = runtime.block_on(tokio::net::TcpListener::bind("127.0.0.1:0")).unwrap();
    let port = listener.local_addr().unwrap().port();
    let app = http::router(daemon.clone(), ui, policy);
    runtime.spawn(async move { axum::serve(listener, app).await });
    Served { port, daemon, _runtime: runtime, _tmp: tmp }
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

/// One HTTP/1.0 exchange with the given Host and extra header lines: the
/// status code, the response head and the body.
fn request_as(port: u16, method: &str, path: &str, body: Option<&Value>, host: &str, extra: &str) -> (u16, String, String) {
    let mut s = TcpStream::connect(("127.0.0.1", port)).unwrap();
    s.set_read_timeout(Some(Duration::from_secs(10))).unwrap();
    let body = body.map(|b| b.to_string()).unwrap_or_default();
    write!(s, "{method} {path} HTTP/1.0\r\nHost: {host}\r\n{extra}Content-Type: application/json\r\nAccept: application/json, text/event-stream\r\nContent-Length: {}\r\n\r\n{body}", body.len()).unwrap();
    let mut answer = String::new();
    s.read_to_string(&mut answer).unwrap();
    let (head, body) = answer.split_once("\r\n\r\n").unwrap();
    (head.split_whitespace().nth(1).unwrap().parse().unwrap(), head.to_string(), body.to_string())
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
    let (code, body) = request(s.port, "GET", "/api/get?ref=page:goal&ticket=work", None);
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
    assert!(body.contains("This binary holds no UI"), "{body}");
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
        if seen.contains("ticket:work") {
            break;
        }
    }
    assert!(seen.contains("text/event-stream"), "{seen}");
    assert!(seen.contains("data:") && seen.contains("ticket:work"), "the write arrives as a server-sent event: {seen}");
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
    // The reading order is each root page's links; a list of the story pages here goes stale.
    let lower = text.to_lowercase();
    let named: Vec<&str> = grimoire_core::scaffold::STORY.iter().map(|(slug, _, _)| *slug).filter(|slug| lower.contains(slug)).collect();
    assert!(named.len() < 3, "the instructions list the story pages {named:?}: {text}");
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


/// # Test contract
///
/// ```json
/// {"test": "other-sites-are-refused-on-every-route", "checks": ["crates/server/src/http#same-origin"]}
/// ```
#[test]
fn other_sites_are_refused_on_every_route() {
    let s = serve();
    let own = format!("127.0.0.1:{}", s.port);
    let page = json!({ "slug": "overview", "title": "Overview", "body": "The root.", "links": [] });
    // A page elsewhere posting a write: refused, and nothing written.
    let (code, _, body) = request_as(s.port, "POST", "/api/put_page", Some(&page), &own, "Origin: https://example.com\r\n");
    assert_eq!(code, 403, "{body}");
    assert_eq!(json(&body)["error"], "invalid");
    let (code, _) = request(s.port, "POST", "/api/get", Some(&json!({ "ref": "page:overview", "ticket": "work" })));
    assert_eq!(code, 404, "the forged write wrote nothing");
    // MCP and the event stream are guarded too; so is a rebound name.
    let init = json!({ "jsonrpc": "2.0", "id": 1, "method": "tools/list", "params": {} });
    assert_eq!(request_as(s.port, "POST", "/mcp", Some(&init), &own, "Sec-Fetch-Site: cross-site\r\n").0, 403);
    assert_eq!(request_as(s.port, "GET", "/api/events", None, &own, "Origin: https://example.com\r\n").0, 403);
    assert_eq!(request_as(s.port, "POST", "/api/status", None, &format!("evil.example:{}", s.port), "").0, 403);
    // A preflight gets no CORS headers, so a browser never sends the real request.
    let (_, head, _) = request_as(s.port, "OPTIONS", "/api/put_page", None, &own, "Origin: https://example.com\r\nAccess-Control-Request-Method: POST\r\n");
    assert!(!head.to_ascii_lowercase().contains("access-control-allow"), "{head}");
    // A form shaped as JSON, from a browser that sends no Origin or
    // Sec-Fetch-Site: refused for its content type, and nothing written.
    let form = r#"{"slug":"forged","title":"Forged","body":"=","links":[]}"#;
    for ct in ["text/plain", "application/x-www-form-urlencoded", "multipart/form-data; boundary=x"] {
        let mut s = TcpStream::connect(("127.0.0.1", s.port)).unwrap();
        s.set_read_timeout(Some(Duration::from_secs(10))).unwrap();
        write!(s, "POST /api/put_page HTTP/1.0\r\nHost: {own}\r\nContent-Type: {ct}\r\nContent-Length: {}\r\n\r\n{form}", form.len()).unwrap();
        let mut answer = String::new();
        s.read_to_string(&mut answer).unwrap();
        assert!(answer.starts_with("HTTP/1.0 415") || answer.starts_with("HTTP/1.1 415"), "{ct}: {answer}");
    }
    assert_eq!(request(s.port, "POST", "/api/get", Some(&json!({ "ref": "page:forged" }))).0, 404, "the form wrote nothing");
    // A navigation from elsewhere still opens the UI's own files.
    assert_eq!(request_as(s.port, "GET", "/", None, &own, "Sec-Fetch-Site: cross-site\r\nOrigin: https://example.com\r\n").0, 200);
    // The daemon's own origin, and an agent sending none, are served.
    let (code, head, _) = request_as(s.port, "POST", "/api/put_page", Some(&page), &own, &format!("Origin: http://{own}\r\nSec-Fetch-Site: same-origin\r\n"));
    assert_eq!(code, 200);
    assert!(!head.to_ascii_lowercase().contains("access-control-allow"), "no CORS headers on an answer either: {head}");
    assert_eq!(request(s.port, "POST", "/api/get", Some(&json!({ "ref": "page:overview", "ticket": "work" }))).0, 200);
}

/// # Test contract
///
/// ```json
/// {"test": "get-runs-reads-only", "checks": ["crates/server/src/http#reads-on-get"]}
/// ```
#[test]
fn get_runs_reads_only() {
    let s = serve();
    let page = json!({ "slug": "overview", "title": "Overview", "body": "The root.", "links": [] });
    let (code, body) = request(s.port, "POST", "/api/put_page", Some(&page));
    assert_eq!(code, 200, "{body}");
    assert_eq!(request(s.port, "GET", "/api/get?ref=page:overview&ticket=work", None).0, 200);
    let (code, body) = request(s.port, "GET", "/api/delete_node?ref=page:overview", None);
    assert_eq!(code, 405, "{body}");
    assert_eq!(json(&body)["error"], "invalid");
    assert_eq!(request(s.port, "GET", "/api/get?ref=page:overview&ticket=work", None).0, 200, "still there");
}

/// # Test contract
///
/// ```json
/// {"test": "a-ui-directory-overrides-and-routes-land-on-its-index", "checks": ["crates/server/src/http#ui-optional"]}
/// ```
#[test]
fn a_ui_directory_overrides_and_routes_land_on_its_index() {
    let tmp = tempfile::Builder::new().prefix("grimoire-ui-").tempdir().unwrap();
    let dir = tmp.path().to_path_buf();
    std::fs::create_dir_all(dir.join("assets")).unwrap();
    std::fs::write(dir.join("index.html"), "<div id=\"override\"></div>").unwrap();
    std::fs::write(dir.join("assets").join("app.js"), "run()").unwrap();
    let s = serve_with(http::Ui::Dir(dir.clone()));
    for route in ["/", "/board", "/n/page%3Agoal"] {
        let (code, body) = request(s.port, "GET", route, None);
        assert_eq!(code, 200, "{route}");
        assert!(body.contains("override"), "{route} is the given directory's index, not the embedded one: {body}");
    }
    let (_, head, body) = request_as(s.port, "GET", "/assets/app.js", None, "127.0.0.1", "");
    assert_eq!(body, "run()");
    assert!(head.to_ascii_lowercase().contains("javascript"), "{head}");
}

/// # Test contract
///
/// ```json
/// {"test": "the-embedded-ui-or-a-page-saying-there-is-none", "checks": ["crates/server/src/http#ui-optional", "crates/server/src/http#ui-embedded"]}
/// ```
#[test]
fn the_embedded_ui_or_a_page_saying_there_is_none() {
    let s = serve_with(http::Ui::Embedded);
    let (code, body) = request(s.port, "GET", "/board", None);
    assert_eq!(code, 200);
    if http::embedded_ui_present() {
        assert!(body.contains("<div id=\"root\">"), "a client-side route gets the embedded index: {body}");
        assert_eq!(request(s.port, "GET", "/assets/definitely-missing.js", None).0, 404);
    } else {
        assert!(body.contains("This binary holds no UI"), "{body}");
    }
    // HEAD answers like GET, without a body.
    let (code, _, body) = request_as(s.port, "HEAD", "/", None, "127.0.0.1", "");
    assert_eq!((code, body.as_str()), (200, ""));
    // Nothing outside the UI is reachable through it, encoded or not.
    for escape in ["/../Cargo.toml", "/..%2f..%2fCargo.toml", "/assets/..%2f..%2f..%2fCargo.toml", "/..\\..\\Cargo.toml"] {
        let (_, body) = request(s.port, "GET", escape, None);
        assert!(!body.contains("[package]") && !body.contains("[workspace]"), "{escape} reached a file outside the UI");
    }
    // The API and MCP serve whatever the UI.
    assert_eq!(request(s.port, "POST", "/api/status", Some(&json!({}))).0, 200);
}

/// A policy that admits nobody, as a server policy would a caller without a token.
struct Nobody;

impl grimoire_server::access::Policy for Nobody {
    fn admit(&self, _: &axum::http::HeaderMap) -> Result<grimoire_server::access::Caller, String> {
        Err("no one is admitted".into())
    }
}

/// A policy that admits every caller to read only.
struct Readers;

impl grimoire_server::access::Policy for Readers {
    fn admit(&self, _: &axum::http::HeaderMap) -> Result<grimoire_server::access::Caller, String> {
        Ok(grimoire_server::access::Caller { who: "reader".into(), access: grimoire_server::access::Access::Read })
    }
}

fn mcp_call(port: u16, tool: &str, arguments: Value) -> Value {
    mcp(port, 1, "tools/call", json!({ "name": tool, "arguments": arguments }))
}

/// # Test contract
///
/// ```json
/// {"test": "a-refused-caller-reaches-no-tool", "checks": ["crates/server/src/http#every-route-passes", "crates/server/src/access#one-step"]}
/// ```
#[test]
fn a_refused_caller_reaches_no_tool() {
    let s = serve_as(http::Ui::None, Arc::new(Nobody));
    let page = json!({ "slug": "goal", "title": "Goal", "body": "b", "links": [] });
    let mcp_write = json!({ "jsonrpc": "2.0", "id": 1, "method": "tools/call", "params": { "name": "put_page", "arguments": page } });
    let initialize = json!({ "jsonrpc": "2.0", "id": 1, "method": "initialize", "params": { "protocolVersion": "2026-07-28", "capabilities": {}, "clientInfo": { "name": "t", "version": "1" } } });
    let list = json!({ "jsonrpc": "2.0", "id": 2, "method": "tools/list", "params": {} });
    let prune = json!({ "present": [], "intent": "x" });
    for (method, path, body) in [
        ("GET", "/api/tools", None),
        ("GET", "/api/export", None),
        ("POST", "/api/prune", Some(&prune)),
        ("GET", "/api/events", None),
        ("GET", "/api/get?ref=page:goal&ticket=work", None),
        ("HEAD", "/api/get?ref=page:goal", None),
        ("OPTIONS", "/api/put_page", None),
        ("POST", "/api/put_page", Some(&page)),
        ("POST", "/mcp", Some(&mcp_write)),
        ("POST", "/mcp/", Some(&mcp_write)),
        ("POST", "/mcp", Some(&initialize)),
        ("POST", "/mcp", Some(&list)),
        ("GET", "/mcp", None),
        ("DELETE", "/mcp", None),
    ] {
        let (code, body) = request(s.port, method, path, body);
        assert_eq!(code, 403, "{method} {path}: {body}");
        if method != "HEAD" {
            assert_eq!(json(&body)["error"], "invalid", "{method} {path}: the error object");
        }
    }
    assert!(s.daemon.call("get", json!({ "ref": "page:goal", "ticket": "work" })).is_err(), "nothing was written");
    // The UI's files are not behind the step: a link to the UI from anywhere opens it.
    assert_eq!(request(s.port, "GET", "/", None).0, 200);
}

/// # Test contract
///
/// ```json
/// {"test": "a-reader-cannot-write", "checks": ["crates/server/src/access#writes-need-write", "crates/server/src/mcp#caller-checked", "req:daemon-trust-model"]}
/// ```
#[test]
fn a_reader_cannot_write() {
    let s = serve_as(http::Ui::None, Arc::new(Readers));
    s.daemon.call("put_page", json!({ "slug": "goal", "title": "Goal", "body": "b", "links": [] })).unwrap();
    // Reads answer, over the JSON API and MCP.
    let (code, body) = request(s.port, "GET", "/api/get?ref=page:goal&ticket=work", None);
    assert_eq!((code, json(&body)["node"]["title"].clone()), (200, json!("Goal")));
    let read = mcp_call(s.port, "get", json!({ "ref": "page:goal", "ticket": "work" }));
    assert_eq!(read["result"]["isError"], json!(false), "{read}");
    // Writes are refused before they run, over both.
    let (code, body) = request(s.port, "POST", "/api/put_page", Some(&json!({ "slug": "goal", "title": "Changed", "body": "b", "links": [] })));
    assert_eq!(code, 403, "{body}");
    assert!(json(&body)["message"].as_str().unwrap().contains("may only read"), "{body}");
    let write = mcp_call(s.port, "delete_node", json!({ "ref": "page:goal" }));
    assert_eq!(write["result"]["isError"], json!(true), "{write}");
    assert!(write["result"]["structuredContent"]["message"].as_str().unwrap().contains("may only read"), "{write}");
    let after = s.daemon.call("get", json!({ "ref": "page:goal", "ticket": "work" })).unwrap();
    assert_eq!(after["node"]["title"], "Goal", "neither write ran");
}

/// A store with no repository, served as a team's: no interface, callers admitted by token.
fn serve_shared() -> (Served, String, String) {
    let tmp = tempfile::Builder::new().prefix("grimoire-shared-").tempdir().unwrap();
    let store = tmp.path().join("store");
    let config = StoreConfig { site: vec![], version: StoreConfig::CURRENT_VERSION, repository: "C:/a/checkout/on/another/machine".into(), roots: vec!["overview".into()], languages: vec!["rust".into()], scope: Scope::default(), order: Default::default() };
    store_io::write_all(&store, &Store::new(config), false).unwrap();
    let writer = store_io::tokens::add(&store, "ana", true).unwrap();
    let reader = store_io::tokens::add(&store, "pipeline-check", false).unwrap();
    let daemon = Arc::new(Daemon::open_shared(&store).unwrap());
    let runtime = tokio::runtime::Runtime::new().unwrap();
    let listener = runtime.block_on(tokio::net::TcpListener::bind("127.0.0.1:0")).unwrap();
    let port = listener.local_addr().unwrap().port();
    let app = http::router(daemon.clone(), http::Ui::Shared, Arc::new(grimoire_server::access::Tokens { store_dir: store }));
    runtime.spawn(async move { axum::serve(listener, app).await });
    (Served { port, daemon, _runtime: runtime, _tmp: tmp }, writer, reader)
}

/// # Test contract
///
/// ```json
/// {"test": "a-shared-daemon-admits-by-token-and-serves-no-interface", "checks": ["crates/server/src/http#shared-page", "crates/server/src/access#token-policy", "crates/server/src/access#writes-need-write"]}
/// ```
#[test]
fn a_shared_daemon_admits_by_token_and_serves_no_interface() {
    let (s, writer, reader) = serve_shared();
    let bearer = |t: &str| format!("Authorization: Bearer {t}\r\n");
    let ticket = json!({ "title": "From the team", "kind": "gap", "priority": "later", "body": "b" });
    // No token, or one it does not hold: refused before any tool, from this machine as from any other.
    let (code, _, body) = request_as(s.port, "POST", "/api/board", Some(&json!({})), "127.0.0.1", "");
    assert_eq!(code, 403, "{body}");
    assert!(json(&body)["message"].as_str().unwrap().contains("no token"), "{body}");
    assert_eq!(request_as(s.port, "POST", "/api/board", Some(&json!({})), "127.0.0.1", &bearer("grm_not-held")).0, 403);
    assert_eq!(request_as(s.port, "GET", "/api/tools", None, "127.0.0.1", "").0, 403);
    assert_eq!(request_as(s.port, "POST", "/mcp", Some(&json!({ "jsonrpc": "2.0", "id": 1, "method": "tools/list", "params": {} })), "127.0.0.1", "").0, 403);
    // A token signs its caller in, whatever name the request was addressed to.
    let (code, _, body) = request_as(s.port, "POST", "/api/file_ticket", Some(&ticket), "grimoire.team.example", &bearer(&writer));
    assert_eq!(code, 200, "{body}");
    let (code, _, body) = request_as(s.port, "POST", "/api/status", Some(&json!({})), "grimoire.team.example", &bearer(&reader));
    assert_eq!((code, json(&body)["shared"].clone()), (200, json!(true)), "{body}");
    // A read-only token reads and does not write.
    let (code, _, body) = request_as(s.port, "POST", "/api/lint", Some(&json!({})), "127.0.0.1", &bearer(&reader));
    assert_eq!((code, json(&body)["without_code"].clone()), (200, json!(true)), "{body}");
    let (code, _, body) = request_as(s.port, "POST", "/api/file_ticket", Some(&ticket), "127.0.0.1", &bearer(&reader));
    assert_eq!(code, 403, "{body}");
    assert!(json(&body)["message"].as_str().unwrap().contains("may only read"), "{body}");
    assert_eq!(s.daemon.call("board", json!({ "kinds": ["gap"] })).unwrap()["unattached"].as_array().unwrap().len(), 1, "one ticket was filed, by the writer");
    // Revoked while serving: refused on its next call.
    assert!(store_io::tokens::revoke(&s.daemon.store_dir, "ana").unwrap());
    assert_eq!(request_as(s.port, "POST", "/api/board", Some(&json!({})), "127.0.0.1", &bearer(&writer)).0, 403);
    // No interface: any other path is a page saying what this is, to anyone.
    for path in ["/", "/board", "/n/page%3Aoverview"] {
        let (code, _, body) = request_as(s.port, "GET", path, None, "grimoire.team.example", "");
        assert_eq!(code, 200, "{path}");
        assert!(body.contains("shared Grimoire server") && body.contains("no interface"), "{path}: {body}");
    }
}
