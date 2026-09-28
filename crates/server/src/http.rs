//! # Module contract
//!
//! ```json
//! {"purpose":"Serve the JSON API, the event stream, MCP and the web UI from one port.","criteria":[{"id":"api-mirrors-tools","statement":"POST /api/<tool> runs the tool of that name with the JSON body as input and returns its output, or the error object with a status matching its code."},{"id":"events","statement":"GET /api/events streams every daemon event as server-sent events."},{"id":"ui-optional","statement":"When no UI directory is configured the API and MCP still serve; the UI is a static fallback, not a dependency."},{"id":"same-origin","statement":"The JSON API, the event stream and MCP answer 403 with the error object to a request whose Origin is not the daemon's own, whose Sec-Fetch-Site is cross-site or same-site, or whose Host is not localhost, 127.0.0.1 or [::1]; a POST to /api/<tool> whose body is not application/json is refused with 415; no route sends CORS headers, and the UI's own files are served to any navigation."},{"id":"reads-on-get","statement":"GET /api/<tool> runs read tools only; a write tool named over GET is refused as invalid with status 405."}]}
//! ```

use std::convert::Infallible;
use std::path::PathBuf;
use std::sync::Arc;

use axum::body::Bytes;
use axum::extract::{Path, Query, Request, State};
use axum::http::header::{CONTENT_TYPE, HOST, ORIGIN};
use axum::http::{HeaderMap, StatusCode};
use axum::middleware::{self, Next};
use axum::response::sse::{Event as SseEvent, KeepAlive, Sse};
use axum::response::{IntoResponse, Response};
use axum::routing::get;
use axum::{Json, Router};
use futures::stream::Stream;
use grimoire_core::ToolError;
use serde_json::{json, Value};
use tokio_stream::wrappers::BroadcastStream;
use tokio_stream::StreamExt;
use tower_http::services::{ServeDir, ServeFile};

use crate::daemon::Daemon;

pub fn router(daemon: Arc<Daemon>, ui_dir: Option<PathBuf>) -> Router {
    let mut app = Router::new()
        .route("/api/tools", get(list_tools))
        .route("/api/events", get(events))
        .route("/api/{tool}", get(call_get).post(call_post))
        .with_state(daemon.clone())
        .nest_service("/mcp", crate::mcp::service(daemon))
        // Everything above is refused to other sites; the UI's files below are
        // served to any navigation, so a link to the UI from elsewhere opens it.
        .layer(middleware::from_fn(same_origin));
    if let Some(dir) = ui_dir {
        let index = dir.join("index.html");
        app = app.fallback_service(ServeDir::new(&dir).fallback(ServeFile::new(index)));
    } else {
        app = app.fallback(no_ui);
    }
    app
}

/// Refuse a request a browser makes on behalf of another site. Agents, MCP
/// clients and the CLI send no Origin and no Sec-Fetch-Site, and a loopback Host.
async fn same_origin(req: Request, next: Next) -> Response {
    match refusal(req.headers()) {
        Some(why) => (StatusCode::FORBIDDEN, Json(serde_json::to_value(ToolError::invalid(why, vec![])).unwrap_or_default())).into_response(),
        None => next.run(req).await,
    }
}

/// Why a request is refused, if it is: a Host that is not a loopback name
/// (DNS rebinding), a fetch the browser marks as from another site, or an
/// Origin other than the daemon's own.
fn refusal(headers: &HeaderMap) -> Option<String> {
    let host = headers.get(HOST).and_then(|v| v.to_str().ok()).unwrap_or("");
    if !is_loopback(host) {
        return Some(format!("Host `{host}` is not a loopback name; the daemon answers only localhost, 127.0.0.1 and [::1]"));
    }
    if let Some(site) = headers.get("sec-fetch-site").and_then(|v| v.to_str().ok()) {
        if site == "cross-site" || site == "same-site" {
            return Some(format!("a request from another site ({site}) is refused"));
        }
    }
    if let Some(origin) = headers.get(ORIGIN) {
        let origin = origin.to_str().unwrap_or("");
        if origin != format!("http://{host}") {
            return Some(format!("a request from origin `{origin}` is refused; only the daemon's own origin may call it from a browser"));
        }
    }
    None
}

/// `localhost`, `127.0.0.1` or `[::1]`, with or without a port.
fn is_loopback(host: &str) -> bool {
    let name = if let Some(rest) = host.strip_prefix('[') {
        rest.split(']').next().map(|n| format!("[{n}]")).unwrap_or_default()
    } else {
        host.split(':').next().unwrap_or("").to_string()
    };
    matches!(name.to_ascii_lowercase().as_str(), "localhost" | "127.0.0.1" | "[::1]")
}

async fn no_ui() -> Response {
    (
        StatusCode::OK,
        [("content-type", "text/html; charset=utf-8")],
        "<!doctype html><title>grimoire</title><body style=\"background:#111;color:#ddd;font-family:system-ui;padding:2rem\"><h1>grimoire</h1><p>The daemon is running. The API is under <code>/api/&lt;tool&gt;</code> (<a href=\"/api/tools\" style=\"color:#9cf\">list</a>), MCP at <code>/mcp</code>, events at <code>/api/events</code>. No UI directory is configured; run the Vite dev server for the UI.</p>",
    )
        .into_response()
}

async fn list_tools() -> Json<Value> {
    Json(json!({ "tools": grimoire_core::tools::definitions() }))
}

fn error_response(e: ToolError) -> Response {
    let status = match &e {
        ToolError::NotFound { .. } => StatusCode::NOT_FOUND,
        ToolError::Invalid { .. } => StatusCode::BAD_REQUEST,
        ToolError::Conflict { .. } => StatusCode::CONFLICT,
        ToolError::StoreUnavailable { .. } => StatusCode::SERVICE_UNAVAILABLE,
    };
    (status, Json(serde_json::to_value(&e).unwrap_or_default())).into_response()
}

async fn run(daemon: Arc<Daemon>, tool: String, input: Value) -> Response {
    if !grimoire_core::tools::TOOL_NAMES.contains(&tool.as_str()) {
        return error_response(ToolError::invalid(format!("unknown tool `{tool}`"), vec![]));
    }
    let result = tokio::task::spawn_blocking(move || daemon.call(&tool, input)).await;
    match result {
        Ok(Ok(v)) => Json(v).into_response(),
        Ok(Err(e)) => error_response(e),
        Err(e) => error_response(ToolError::unavailable(e.to_string())),
    }
}

async fn call_post(State(daemon): State<Arc<Daemon>>, Path(tool): Path<String>, headers: HeaderMap, body: Bytes) -> Response {
    // A form can post a body spelled as JSON with a form content type, and a
    // browser sends a form without asking first; JSON cannot be sent that way.
    let json_body = headers.get(CONTENT_TYPE).and_then(|v| v.to_str().ok()).is_some_and(|ct| ct.split(';').next().is_some_and(|m| m.trim().eq_ignore_ascii_case("application/json")));
    if !body.is_empty() && !json_body {
        let e = ToolError::invalid("send the input as JSON with Content-Type: application/json", vec![]);
        return (StatusCode::UNSUPPORTED_MEDIA_TYPE, Json(serde_json::to_value(e).unwrap_or_default())).into_response();
    }
    let input = if body.is_empty() {
        json!({})
    } else {
        match serde_json::from_slice::<Value>(&body) {
            Ok(v) => v,
            Err(e) => return error_response(ToolError::invalid(format!("body is not JSON: {e}"), vec![])),
        }
    };
    run(daemon, tool, input).await
}

/// GET form for convenience: each query value is parsed as JSON when it is,
/// otherwise taken as a string, so `?ref=page:overview&depth=3` works.
async fn call_get(State(daemon): State<Arc<Daemon>>, Path(tool): Path<String>, Query(q): Query<Vec<(String, String)>>) -> Response {
    // A write always takes a POST with a JSON body: a GET is the easiest
    // request for another site to forge.
    if grimoire_core::tools::is_write(&tool) {
        let e = ToolError::invalid(format!("`{tool}` writes; send it as POST /api/{tool} with a JSON body"), vec![]);
        return (StatusCode::METHOD_NOT_ALLOWED, Json(serde_json::to_value(e).unwrap_or_default())).into_response();
    }
    let mut input = serde_json::Map::new();
    for (k, v) in q {
        let parsed = serde_json::from_str::<Value>(&v).ok().filter(|p| !p.is_string()).unwrap_or(Value::String(v));
        input.insert(k, parsed);
    }
    run(daemon, tool, Value::Object(input)).await
}

async fn events(State(daemon): State<Arc<Daemon>>) -> Sse<impl Stream<Item = Result<SseEvent, Infallible>>> {
    let rx = daemon.subscribe();
    let stream = BroadcastStream::new(rx).filter_map(|item| match item {
        Ok(ev) => Some(Ok(SseEvent::default().json_data(&ev).unwrap_or_else(|_| SseEvent::default().data("{}")))),
        Err(_) => None,
    });
    Sse::new(stream).keep_alive(KeepAlive::default())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn headers(pairs: &[(&'static str, &str)]) -> HeaderMap {
        let mut h = HeaderMap::new();
        for (k, v) in pairs {
            h.insert(*k, v.parse().expect("header value"));
        }
        h
    }

    /// # Test contract
    ///
    /// ```json
    /// {"test": "only-the-daemons-own-origin", "checks": ["crates/server/src/http#same-origin"]}
    /// ```
    #[test]
    fn only_the_daemons_own_origin() {
        // Agents: no Origin, no Sec-Fetch-Site, a loopback Host with or without a port.
        for host in ["127.0.0.1", "127.0.0.1:4242", "localhost:4242", "LOCALHOST", "[::1]:4242"] {
            assert_eq!(refusal(&headers(&[("host", host)])), None, "{host}");
        }
        // The daemon's own UI, and the dev server proxying to it.
        assert_eq!(refusal(&headers(&[("host", "127.0.0.1:4242"), ("origin", "http://127.0.0.1:4242"), ("sec-fetch-site", "same-origin")])), None);
        assert_eq!(refusal(&headers(&[("host", "localhost:4242"), ("sec-fetch-site", "none")])), None, "typed into the address bar");
        // Another site, a rebound name, a request marked cross-site, a missing Host.
        assert!(refusal(&headers(&[("host", "127.0.0.1:4242"), ("origin", "https://example.com")])).is_some());
        assert!(refusal(&headers(&[("host", "127.0.0.1:4242"), ("origin", "null")])).is_some());
        assert!(refusal(&headers(&[("host", "evil.example:4242")])).is_some(), "DNS rebinding");
        assert!(refusal(&headers(&[("host", "127.0.0.1.evil.example")])).is_some());
        assert!(refusal(&headers(&[("host", "127.0.0.1:4242"), ("sec-fetch-site", "cross-site")])).is_some(), "an image tag elsewhere");
        assert!(refusal(&headers(&[("host", "127.0.0.1:4242"), ("sec-fetch-site", "same-site")])).is_some(), "another port on localhost");
        assert!(refusal(&HeaderMap::new()).is_some());
    }
}
