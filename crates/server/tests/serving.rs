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
    serve_app(|daemon| http::router(daemon, ui, policy))
}

/// The same store, served by the router `app` builds for its daemon.
fn serve_app(app: impl FnOnce(Arc<Daemon>) -> axum::Router) -> Served {
    let tmp = tempfile::Builder::new().prefix("grimoire-serving-").tempdir().unwrap();
    let dir = tmp.path().to_path_buf();
    let repo = dir.join("repo");
    std::fs::create_dir_all(repo.join("src")).unwrap();
    std::fs::write(repo.join("Cargo.toml"), "[package]\nname = \"s\"\nversion = \"0.1.0\"\n").unwrap();
    std::fs::write(repo.join("src").join("lib.rs"), "pub fn s() {}\n").unwrap();
    let store = dir.join("store");
    let config = StoreConfig {
        large_file: None,
        long_statement: None,
        plain_file: None,
        site: vec![],
        version: StoreConfig::CURRENT_VERSION,
        repository: repo.to_string_lossy().replace('\\', "/"),
        roots: vec!["overview".into()].into(),
        languages: vec!["rust".into()],
        scope: Scope::default(),
        order: Default::default(),
        recipe_kinds: Default::default(),
    };
    // A brewing, current ticket, so a write that changes intent has a proposal to go into; reads of it name the ticket.
    let mut s = Store::new(config);
    s.tickets.insert(
        "work".into(),
        grimoire_core::model::Ticket {
            slug: "work".into(),
            title: "Work".into(),
            kind: grimoire_core::model::TicketKind::Change,
            priority: grimoire_core::model::Priority::Later,
            status: grimoire_core::model::Status::Brewing,
            touches: vec![],
            relies_on: vec![],
            body: "b".into(),
            decision: None,
        },
    );
    s.current = Some("work".into());
    store_io::write_all(&store, &s, false).unwrap();
    let daemon = Arc::new(Daemon::open(&store).unwrap().0);
    let runtime = tokio::runtime::Runtime::new().unwrap();
    let listener = runtime.block_on(tokio::net::TcpListener::bind("127.0.0.1:0")).unwrap();
    let port = listener.local_addr().unwrap().port();
    let app = app(daemon.clone());
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
    write!(s, "{method} {path} HTTP/1.0\r\nHost: {host}\r\n{extra}Content-Type: application/json\r\nAccept: application/json, text/event-stream\r\nContent-Length: {}\r\n\r\n{body}", body.len())
        .unwrap();
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
/// {"test": "mcp-lists-the-core-tools", "checks": ["crates/server/src/mcp#same-tools", "crates/server/src/mcp#tools-fresh"]}
/// ```
#[test]
fn mcp_lists_the_core_tools() {
    let s = serve();
    let answer = mcp(s.port, 1, "tools/list", json!({}))["result"].clone();
    // Stale at once and this client's alone: a rebuilt daemon's tools reach a client that asks again.
    assert_eq!((answer["ttlMs"].clone(), answer["cacheScope"].clone()), (json!(0), json!("private")), "{answer}");
    let listed = answer["tools"].clone();
    let listed: Vec<(String, String, Value)> =
        listed.as_array().unwrap().iter().map(|t| (t["name"].as_str().unwrap().to_string(), t["description"].as_str().unwrap().to_string(), t["inputSchema"].clone())).collect();
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
/// {"test": "mcp-instructions-carry-the-rules", "checks": ["crates/server/src/mcp#instructions-carry-the-rules", "crates/server/src/mcp#instructions-say-to-pass-checkout", "crates/server/src/mcp#instructions-name-the-statuses", "crates/server/src/mcp#instructions-list-no-pages"]}
/// ```
#[test]
fn mcp_instructions_carry_the_rules() {
    let s = serve();
    let r = mcp(s.port, 1, "initialize", json!({ "protocolVersion": "2025-06-18", "capabilities": {}, "clientInfo": { "name": "t", "version": "0" } }));
    let text = r["result"]["instructions"].as_str().unwrap().to_string();
    for rule in [
        "Orient first",
        "Work is a ticket",
        "same piece of work",
        "`# Module contract`",
        "`find` before writing any reference",
        "Never write the store's files",
        "never on your own decision",
        "Pass `checkout`, your working directory, with every call",
        "`grimoire bottle` then bottles it",
        "A ticket is inscribed, brewing, simmering or bottled",
    ] {
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
/// {"test": "one-answer-whichever-transport", "checks": ["req:tools-same-answer"]}
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
/// {"test": "other-sites-are-refused-on-every-route", "checks": ["crates/server/src/http#foreign-origin-refused", "crates/server/src/http#non-json-post-refused", "crates/server/src/http#ui-files-to-any-navigation"]}
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
/// {"test": "no-route-family-sends-cors-headers", "checks": ["crates/server/src/http#no-cors-headers"]}
/// ```
#[test]
fn no_route_family_sends_cors_headers() {
    let tmp = tempfile::Builder::new().prefix("grimoire-ui-").tempdir().unwrap();
    let dir = tmp.path().to_path_buf();
    std::fs::write(dir.join("index.html"), "<div></div>").unwrap();
    let s = serve_with(http::Ui::Dir(dir));
    let own = format!("127.0.0.1:{}", s.port);
    // Asked as a page of the daemon's own origin would, and as a page elsewhere would, which a browser would let read a CORS answer.
    let same = format!("Origin: http://{own}\r\nSec-Fetch-Site: same-origin\r\n");
    let elsewhere = "Origin: https://example.com\r\n".to_string();
    for origin in [same, elsewhere] {
        let init = json!({ "jsonrpc": "2.0", "id": 1, "method": "tools/list", "params": {} });
        let status = json!({});
        for (family, method, path, body) in
            [("the API", "POST", "/api/status", Some(&status)), ("MCP", "POST", "/mcp", Some(&init)), ("the UI's files", "GET", "/", None), ("a preflight", "OPTIONS", "/api/status", None)]
        {
            let (_, head, _) = request_as(s.port, method, path, body, &own, &origin);
            assert!(!head.to_ascii_lowercase().contains("access-control-"), "{family} with {origin:?}: {head}");
        }
        // The event stream never ends: its head alone is read.
        let mut stream = TcpStream::connect(("127.0.0.1", s.port)).unwrap();
        stream.set_read_timeout(Some(Duration::from_secs(10))).unwrap();
        write!(stream, "GET /api/events HTTP/1.0\r\nHost: {own}\r\n{origin}\r\n").unwrap();
        let mut head = Vec::new();
        let mut byte = [0u8; 1];
        while !head.ends_with(b"\r\n\r\n") {
            stream.read_exact(&mut byte).unwrap();
            head.push(byte[0]);
        }
        let head = String::from_utf8_lossy(&head).to_ascii_lowercase();
        assert!(origin.contains("example.com") || head.contains(" 200 "), "the stream is served to its own origin: {head}");
        assert!(!head.contains("access-control-"), "the event stream with {origin:?}: {head}");
    }
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
    for route in ["/", "/cauldron", "/n/page%3Agoal"] {
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
/// {"test": "a-library-serves-each-grimoire-and-the-interface-beside-them", "checks": ["crates/server/src/http#library-routes"]}
/// ```
#[test]
fn a_library_serves_each_grimoire_and_the_interface_beside_them() {
    let tmp = tempfile::Builder::new().prefix("grimoire-ui-").tempdir().unwrap();
    let dir = tmp.path().to_path_buf();
    std::fs::create_dir_all(dir.join("assets")).unwrap();
    std::fs::write(dir.join("index.html"), "<div id=\"library\"></div>").unwrap();
    std::fs::write(dir.join("assets").join("app.js"), "run()").unwrap();
    // A library as serve builds one: a grimoire on the bare routes and at its own path, another at its own.
    let policy: Arc<dyn grimoire_server::access::Policy> = Arc::new(grimoire_server::access::Local);
    let s = serve_app(|d| {
        http::with_ui(http::api(d.clone(), policy.clone()).nest("/g/first", http::api(d.clone(), policy.clone())).nest("/g/second", http::api(d, policy.clone())), http::Ui::Dir(dir.clone()))
    });
    for path in ["/api/status", "/g/first/api/status", "/g/second/api/status"] {
        let (code, body) = request(s.port, "POST", path, Some(&json!({})));
        assert_eq!(code, 200, "{path}: {body}");
        assert!(json(&body)["repository"].is_string(), "{path} is a grimoire's API: {body}");
    }
    // A grimoire's client routes land on the interface, whose files are served from the root.
    for route in ["/g/second/", "/g/second/cauldron", "/g/second/n/page%3Agoal", "/cauldron"] {
        let (code, body) = request(s.port, "GET", route, None);
        assert_eq!(code, 200, "{route}");
        assert!(body.contains("library"), "{route} is the interface's index: {body}");
    }
    let (_, body) = request(s.port, "GET", "/assets/app.js", None);
    assert_eq!(body, "run()");
}

/// # Test contract
///
/// ```json
/// {"test": "the-embedded-ui-or-a-page-saying-there-is-none", "checks": ["crates/server/src/http#ui-optional", "crates/server/src/http#ui-embedded"]}
/// ```
#[test]
fn the_embedded_ui_or_a_page_saying_there_is_none() {
    let s = serve_with(http::Ui::Embedded);
    let (code, body) = request(s.port, "GET", "/cauldron", None);
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
    let config = StoreConfig {
        large_file: None,
        long_statement: None,
        plain_file: None,
        site: vec![],
        version: StoreConfig::CURRENT_VERSION,
        repository: "C:/a/checkout/on/another/machine".into(),
        roots: vec!["overview".into()].into(),
        languages: vec!["rust".into()],
        scope: Scope::default(),
        order: Default::default(),
        recipe_kinds: Default::default(),
    };
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
/// {"test": "a-shared-daemon-admits-by-token-and-serves-no-interface", "checks": ["crates/server/src/http#shared-page", "crates/server/src/access#token-admits", "crates/server/src/access#token-refuses-other", "crates/server/src/access#writes-need-write"]}
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
    let (code, _, body) = request_as(s.port, "POST", "/api/inscribe_ticket", Some(&ticket), "grimoire.team.example", &bearer(&writer));
    assert_eq!(code, 200, "{body}");
    let (code, _, body) = request_as(s.port, "POST", "/api/status", Some(&json!({})), "grimoire.team.example", &bearer(&reader));
    assert_eq!((code, json(&body)["shared"].clone()), (200, json!(true)), "{body}");
    // A read-only token reads and does not write.
    let (code, _, body) = request_as(s.port, "POST", "/api/lint", Some(&json!({})), "127.0.0.1", &bearer(&reader));
    assert_eq!((code, json(&body)["without_code"].clone()), (200, json!(true)), "{body}");
    let (code, _, body) = request_as(s.port, "POST", "/api/inscribe_ticket", Some(&ticket), "127.0.0.1", &bearer(&reader));
    assert_eq!(code, 403, "{body}");
    assert!(json(&body)["message"].as_str().unwrap().contains("may only read"), "{body}");
    assert_eq!(s.daemon.call("board", json!({ "kinds": ["gap"] })).unwrap()["unattached"].as_array().unwrap().len(), 1, "one ticket was filed, by the writer");
    // Revoked while serving: refused on its next call.
    assert!(store_io::tokens::revoke(&s.daemon.store_dir, "ana").unwrap());
    assert_eq!(request_as(s.port, "POST", "/api/board", Some(&json!({})), "127.0.0.1", &bearer(&writer)).0, 403);
    // No interface: any other path is a page saying what this is, to anyone.
    for path in ["/", "/cauldron", "/n/page%3Aoverview"] {
        let (code, _, body) = request_as(s.port, "GET", path, None, "grimoire.team.example", "");
        assert_eq!(code, 200, "{path}");
        assert!(body.contains("shared Grimoire server") && body.contains("no interface"), "{path}: {body}");
    }
}

/// # Test contract
///
/// ```json
/// {"test": "a-library-directory-without-shared-is-refused", "checks": ["crates/server/src#library-without-shared-refused"]}
/// ```
#[test]
fn a_library_directory_without_shared_is_refused() {
    let tmp = tempfile::Builder::new().prefix("grimoire-library-refused-").tempdir().unwrap();
    let library = tmp.path().join("library");
    std::fs::create_dir_all(&library).unwrap();
    let runtime = tokio::runtime::Runtime::new().unwrap();
    let refused = runtime.block_on(grimoire_server::serve(grimoire_server::ServeOptions {
        store_dir: library.clone(),
        port: 0,
        ui_dir: None,
        shared: false,
        listen: None,
        server: None,
        site: None,
        library: None,
        shared_library: Some(library.clone()),
        site_of: None,
        checkout: None,
    }));
    let why = refused.unwrap_err().to_string();
    assert!(why.contains("--library") && why.contains("needs --shared"), "{why}");
    assert_eq!(std::fs::read_dir(&library).unwrap().count(), 0, "nothing was opened or marked");
}

/// A request answered with its head alone: the status code and the head, for a stream that never ends.
fn head_of(port: u16, path: &str) -> (u16, String) {
    let mut s = TcpStream::connect(("127.0.0.1", port)).unwrap();
    s.set_read_timeout(Some(Duration::from_secs(10))).unwrap();
    write!(s, "GET {path} HTTP/1.0\r\nHost: 127.0.0.1\r\n\r\n").unwrap();
    let mut head = Vec::new();
    let mut byte = [0u8; 1];
    while !head.ends_with(b"\r\n\r\n") {
        s.read_exact(&mut byte).unwrap();
        head.push(byte[0]);
    }
    let head = String::from_utf8_lossy(&head).to_string();
    (head.split_whitespace().nth(1).unwrap().parse().unwrap(), head)
}

/// The checkout and leniency of every call an answerer was asked for.
type Asked = Arc<std::sync::Mutex<Vec<(Option<String>, bool)>>>;

/// An answerer that keeps the checkout and leniency of every call it is asked for, and answers with one daemon.
struct Recording {
    daemon: Arc<Daemon>,
    asked: Asked,
}

impl http::Answerer for Recording {
    fn daemon(&self, checkout: Option<&str>, lenient: bool) -> Result<Arc<Daemon>, grimoire_core::ToolError> {
        self.asked.lock().unwrap().push((checkout.map(str::to_string), lenient));
        Ok(self.daemon.clone())
    }
}

/// The routes of a daemon whose answerer is recorded.
fn serve_recorded() -> (Served, Asked) {
    let asked = Arc::new(std::sync::Mutex::new(Vec::new()));
    let kept = asked.clone();
    let s = serve_app(move |daemon| http::api_over(Arc::new(Recording { daemon, asked: kept }), Arc::new(grimoire_server::access::Local)));
    (s, asked)
}

/// # Test contract
///
/// ```json
/// {"test": "the-checkout-of-prune-recipes-stream-export-is-in-the-query", "checks": ["crates/server/src/http#checkout-from-the-prune-and-recipes-query", "crates/server/src/http#checkout-from-the-stream-and-export-query"]}
/// ```
#[test]
fn the_checkout_of_prune_recipes_stream_export_is_in_the_query() {
    let (s, asked) = serve_recorded();
    let last = || asked.lock().unwrap().last().cloned().unwrap();
    let count = || asked.lock().unwrap().len();
    // The prune and the recipes: the checkout in the query chooses the daemon, and the body names only what it carries.
    let prune = json!({ "present": [], "intent": "0000" });
    request(s.port, "POST", "/api/prune?checkout=C%3A%2Fwt", Some(&prune));
    assert_eq!(last(), (Some("C:/wt".to_string()), true));
    request(s.port, "POST", "/api/prune", Some(&prune));
    assert_eq!(last(), (None, true), "none named, none asked for");
    request(s.port, "POST", "/api/recipes?checkout=C%3A%2Fwt", Some(&json!({ "recipes": [] })));
    assert_eq!(last(), (Some("C:/wt".to_string()), true));
    // A checkout in either body is no part of it: refused, and no daemon is chosen for it.
    let before = count();
    let (code, body) = request(s.port, "POST", "/api/prune", Some(&json!({ "present": [], "intent": "0000", "checkout": "C:/wt" })));
    assert_eq!((code, json(&body)["error"].clone()), (400, json!("invalid")), "{body}");
    let (code, body) = request(s.port, "POST", "/api/recipes", Some(&json!({ "recipes": [], "checkout": "C:/wt" })));
    assert_eq!((code, json(&body)["error"].clone()), (400, json!("invalid")), "{body}");
    assert_eq!(count(), before);
    // The export and the event stream, which have no body at all.
    let (code, _) = request(s.port, "GET", "/api/export?checkout=C%3A%2Fwt", None);
    assert_eq!(code, 200);
    assert_eq!(last(), (Some("C:/wt".to_string()), true));
    let (code, head) = head_of(s.port, "/api/events?checkout=C%3A%2Fother");
    assert_eq!(code, 200, "{head}");
    assert_eq!(last(), (Some("C:/other".to_string()), true));
}

/// # Test contract
///
/// ```json
/// {"test": "the-tools-list-asks-no-daemon", "checks": ["crates/server/src/http#tools-list-needs-no-daemon"]}
/// ```
#[test]
fn the_tools_list_asks_no_daemon() {
    struct NoDaemon;
    impl http::Answerer for NoDaemon {
        fn daemon(&self, _: Option<&str>, _: bool) -> Result<Arc<Daemon>, grimoire_core::ToolError> {
            panic!("the tools list asked for a daemon")
        }
    }
    let runtime = tokio::runtime::Runtime::new().unwrap();
    let listener = runtime.block_on(tokio::net::TcpListener::bind("127.0.0.1:0")).unwrap();
    let port = listener.local_addr().unwrap().port();
    let app = http::api_over(Arc::new(NoDaemon), Arc::new(grimoire_server::access::Local));
    runtime.spawn(async move { axum::serve(listener, app).await });
    let (code, body) = request(port, "GET", "/api/tools", None);
    assert_eq!(code, 200, "{body}");
    let tools = json(&body)["tools"].as_array().unwrap().iter().map(|t| t["name"].as_str().unwrap().to_string()).collect::<Vec<_>>();
    assert!(tools.iter().any(|t| t == "status") && tools.iter().any(|t| t == "lint"), "{tools:?}");
}

/// # Test contract
///
/// ```json
/// {"test": "one-daemon-answers-whatever-checkout-is-named", "checks": ["crates/server/src/http#one-daemon-answers-whatever-is-named"]}
/// ```
#[test]
fn one_daemon_answers_whatever_checkout_is_named() {
    let s = serve();
    let (code, plain) = request(s.port, "POST", "/api/status", Some(&json!({})));
    assert_eq!(code, 200, "{plain}");
    // A checkout that is none of the daemon's, in the input of a call and in the query of the routes without a body.
    let (code, named) = request(s.port, "POST", "/api/status", Some(&json!({ "checkout": "C:/no/such/checkout" })));
    assert_eq!(code, 200, "{named}");
    assert_eq!(json(&named)["repository"], json(&plain)["repository"]);
    assert_eq!(request(s.port, "GET", "/api/export?checkout=C%3A%2Fno%2Fsuch%2Fcheckout", None).0, 200);
    assert_eq!(head_of(s.port, "/api/events?checkout=C%3A%2Fno%2Fsuch%2Fcheckout").0, 200);
    let (code, answer) = request(s.port, "GET", "/api/board?checkout=C%3A%2Fno%2Fsuch%2Fcheckout", None);
    assert_eq!(code, 200, "{answer}");
}

/// # Test contract
///
/// ```json
/// {"test": "a-page-on-another-site-is-refused-under-the-token-policy-too", "checks": ["crates/server/src/http#foreign-origin-refused"]}
/// ```
#[test]
fn a_page_on_another_site_is_refused_under_the_token_policy_too() {
    let (s, writer, _) = serve_shared();
    let own = format!("127.0.0.1:{}", s.port);
    let bearer = format!("Authorization: Bearer {writer}\r\n");
    let ticket = json!({ "title": "Forged", "kind": "gap", "priority": "later", "body": "b" });
    let init = json!({ "jsonrpc": "2.0", "id": 1, "method": "tools/list", "params": {} });
    // A valid token carried by a page on another site: refused on the API, the event stream and MCP, with the error object.
    for foreign in ["Origin: https://example.com\r\n", "Origin: http://127.0.0.1:1\r\n", "Sec-Fetch-Site: cross-site\r\n", "Sec-Fetch-Site: same-site\r\n"] {
        let extra = format!("{bearer}{foreign}");
        for (method, path, body) in [("POST", "/api/inscribe_ticket", Some(&ticket)), ("GET", "/api/events", None), ("POST", "/mcp", Some(&init))] {
            let (code, _, answer) = request_as(s.port, method, path, body, &own, &extra);
            assert_eq!(code, 403, "{method} {path} with {foreign:?}: {answer}");
            let refusal = json(&answer);
            assert!(refusal["error"] == "invalid" && refusal["message"].as_str().is_some_and(|m| !m.is_empty()), "{method} {path}: {answer}");
        }
    }
    assert_eq!(s.daemon.call("board", json!({ "kinds": ["gap"] })).unwrap()["unattached"].as_array().unwrap().len(), 0, "nothing was written");
    // The daemon's own origin, over http or behind a proxy over https, and a caller sending neither header, are served.
    for own_origin in [format!("Origin: http://{own}\r\n"), format!("Origin: https://{own}\r\nSec-Fetch-Site: same-origin\r\n"), String::new()] {
        let (code, _, answer) = request_as(s.port, "POST", "/api/status", Some(&json!({})), &own, &format!("{bearer}{own_origin}"));
        assert_eq!(code, 200, "{own_origin:?}: {answer}");
    }
}

/// # Test contract
///
/// ```json
/// {"test": "a-rebound-host-is-refused-on-every-route-under-the-local-policy", "checks": ["crates/server/src/http#foreign-host-refused"]}
/// ```
#[test]
fn a_rebound_host_is_refused_on_every_route_under_the_local_policy() {
    let s = serve();
    let init = json!({ "jsonrpc": "2.0", "id": 1, "method": "tools/list", "params": {} });
    let page = json!({ "slug": "rebound", "title": "Rebound", "body": "b", "links": [] });
    for host in [format!("evil.example:{}", s.port), format!("127.0.0.2:{}", s.port), "localhost.evil.example".to_string(), String::new()] {
        for (method, path, body) in [("POST", "/api/put_page", Some(&page)), ("GET", "/api/events", None), ("POST", "/mcp", Some(&init))] {
            let (code, _, answer) = request_as(s.port, method, path, body, &host, "");
            assert_eq!(code, 403, "{method} {path} as {host:?}: {answer}");
            let refusal = json(&answer);
            assert!(refusal["error"] == "invalid" && refusal["message"].as_str().is_some_and(|m| m.contains("loopback")), "{method} {path} as {host:?}: {answer}");
        }
    }
    assert_eq!(request(s.port, "POST", "/api/get", Some(&json!({ "ref": "page:rebound", "ticket": "work" }))).0, 404, "nothing was written");
    // Only the loopback names are answered.
    for host in [format!("localhost:{}", s.port), format!("LOCALHOST:{}", s.port), format!("127.0.0.1:{}", s.port), format!("[::1]:{}", s.port), "localhost".to_string()] {
        let (code, _, answer) = request_as(s.port, "POST", "/api/status", Some(&json!({})), &host, "");
        assert_eq!(code, 200, "{host}: {answer}");
    }
}

/// # Test contract
///
/// ```json
/// {"test": "every-listener-names-an-address-in-use", "checks": ["crates/server/src#address-taken-named", "req:serve-address-taken"]}
/// ```
#[test]
fn every_listener_names_an_address_in_use() {
    let tmp = tempfile::Builder::new().prefix("grimoire-address-").tempdir().unwrap();
    let repo = tmp.path().join("repo");
    std::fs::create_dir_all(repo.join("src")).unwrap();
    std::fs::write(repo.join("Cargo.toml"), "[package]\nname = \"s\"\nversion = \"0.1.0\"\n").unwrap();
    std::fs::write(repo.join("src").join("lib.rs"), "pub fn s() {}\n").unwrap();
    let config = |repository: String| StoreConfig {
        large_file: None,
        long_statement: None,
        plain_file: None,
        site: vec!["overview".into()],
        version: StoreConfig::CURRENT_VERSION,
        repository,
        roots: vec!["overview".into()].into(),
        languages: vec!["rust".into()],
        scope: Scope::default(),
        order: Default::default(),
        recipe_kinds: Default::default(),
    };
    let own = tmp.path().join("own");
    store_io::write_all(&own, &Store::new(config(repo.to_string_lossy().replace('\\', "/"))), false).unwrap();
    let team = tmp.path().join("library").join("team");
    store_io::write_all(&team, &Store::new(config("C:/a/checkout/elsewhere".into())), false).unwrap();
    let options = |store_dir: &std::path::Path| grimoire_server::ServeOptions {
        store_dir: store_dir.to_path_buf(),
        port: 0,
        ui_dir: None,
        shared: false,
        listen: None,
        server: None,
        site: None,
        library: None,
        shared_library: None,
        site_of: None,
        checkout: None,
    };
    // An address another program holds.
    let held = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let taken = held.local_addr().unwrap();
    let runtime = tokio::runtime::Runtime::new().unwrap();
    let refused = |opts: grimoire_server::ServeOptions| {
        let ended = runtime.block_on(async { tokio::time::timeout(Duration::from_secs(30), grimoire_server::serve(opts)).await });
        ended.expect("serve stops at once on an address it cannot take").unwrap_err().to_string()
    };
    let named = |flag: &str| format!("cannot listen on {taken}: another program is listening there; choose another with {flag}");
    // The daemon on its port.
    assert_eq!(refused(grimoire_server::ServeOptions { port: taken.port(), ..options(&own) }), named("--port"));
    // Its site, on an address of its own, the daemon's port free.
    assert_eq!(refused(grimoire_server::ServeOptions { site: Some(taken), ..options(&own) }), named("--site"));
    // A shared daemon, by --listen and by its port.
    assert_eq!(refused(grimoire_server::ServeOptions { shared: true, listen: Some(taken), ..options(&team) }), named("--listen"));
    assert_eq!(refused(grimoire_server::ServeOptions { shared: true, port: taken.port(), ..options(&team) }), named("--port"));
    // A team's library, by --listen.
    let library = tmp.path().join("library");
    assert_eq!(refused(grimoire_server::ServeOptions { shared: true, listen: Some(taken), shared_library: Some(library.clone()), ..options(&library) }), named("--listen"));
    assert_eq!(refused(grimoire_server::ServeOptions { shared: true, port: taken.port(), shared_library: Some(library.clone()), ..options(&library) }), named("--port"));
    // Any other refusal names the address and the system's reason: an address this machine does not hold,
    // when it does not (a host may own it, or let a program take an address it does not hold).
    let foreign: std::net::SocketAddr = "192.0.2.1:4242".parse().unwrap();
    if std::net::TcpListener::bind(foreign).is_err() {
        let why = refused(grimoire_server::ServeOptions { shared: true, listen: Some(foreign), ..options(&team) });
        assert!(why.starts_with("cannot listen on 192.0.2.1:4242: ") && !why.contains("choose another"), "{why}");
    } else {
        eprintln!("skipped the other refusal: this machine can listen on {foreign}");
    }
    drop(held);
}
