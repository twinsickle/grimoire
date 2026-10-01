//! # Module contract
//!
//! ```json
//! {"purpose":"Serve the JSON API, the event stream, MCP and the web UI from one port.","criteria":[{"id":"api-mirrors-tools","statement":"POST /api/<tool> runs the tool of that name with the JSON body as input and returns its output, or the error object with a status matching its code."},{"id":"events","statement":"GET /api/events streams every daemon event as server-sent events."},{"id":"ui-optional","statement":"The UI is served from --ui-dir when given, otherwise from the copy embedded in the binary when there is one, otherwise as a plain page saying no UI is available; the API and MCP serve in every case."},{"id":"same-origin","statement":"The JSON API, the event stream and MCP answer 403 with the error object to a request whose Origin is not the daemon's own, whose Sec-Fetch-Site is cross-site or same-site, or whose Host is not localhost, 127.0.0.1 or [::1]; a POST to /api/<tool> whose body is not application/json is refused with 415; no route sends CORS headers, and the UI's own files are served to any navigation."},{"id":"reads-on-get","statement":"GET /api/<tool> runs read tools only; a write tool named over GET is refused as invalid with status 405."},{"id":"ui-embedded","statement":"From the embedded UI, a file is served with its content type; a missing file under /assets/ is a 404; and any other path, a client-side route, is answered with index.html."},{"id":"every-route-passes","statement":"/api/tools, /api/events, /api/<tool> and /mcp all sit behind the trust step, which runs before any of their handlers, and /api/<tool> asks may_call before dispatch; only the UI's own files are served without it."}]}
//! ```

use std::convert::Infallible;
use std::path::PathBuf;
use std::sync::Arc;

use axum::body::Bytes;
use axum::extract::{Path, Query, State};
use axum::http::header::CONTENT_TYPE;
use axum::http::{HeaderMap, StatusCode, Uri};
use axum::middleware;
use axum::response::sse::{Event as SseEvent, KeepAlive, Sse};
use axum::response::{IntoResponse, Response};
use axum::routing::get;
use axum::{Extension, Json, Router};
use futures::stream::Stream;
use grimoire_core::ToolError;
use serde_json::{json, Value};
use tokio_stream::wrappers::BroadcastStream;
use tokio_stream::StreamExt;
use tower_http::services::{ServeDir, ServeFile};

use crate::access::{self, Caller, Policy};
use crate::daemon::Daemon;

/// Where the UI's files come from.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Ui {
    /// A directory given on the command line, which overrides the embedded copy.
    Dir(PathBuf),
    /// The copy embedded at build time, when the build had one.
    Embedded,
    /// None: the API and MCP only, and a page saying so.
    None,
}

/// The built UI, `web/dist`. A release build embeds it; a debug build reads
/// it from disk, so the dev loop is unchanged. A build without it still
/// succeeds (its build script warns), and then holds no files.
#[derive(rust_embed::RustEmbed)]
#[folder = "../../web/dist/"]
#[allow_missing = true]
struct Built;

pub fn router(daemon: Arc<Daemon>, ui: Ui, policy: Arc<dyn Policy>) -> Router {
    let mut app = Router::new()
        .route("/api/tools", get(list_tools))
        .route("/api/events", get(events))
        .route("/api/{tool}", get(call_get).post(call_post))
        .with_state(daemon.clone())
        .nest_service("/mcp", crate::mcp::service(daemon))
        // Everything above passes the trust step first; the UI's files below are
        // served to any navigation, so a link to the UI from elsewhere opens it.
        .layer(middleware::from_fn_with_state(policy, access::admit));
    match ui {
        Ui::Dir(dir) => {
            let index = dir.join("index.html");
            app = app.fallback_service(ServeDir::new(&dir).fallback(ServeFile::new(index)));
        }
        Ui::Embedded => app = app.fallback(embedded),
        Ui::None => app = app.fallback(no_ui),
    }
    app
}

/// Whether this binary holds a UI. Asked on every request, so a debug daemon
/// serves `web/dist` as soon as it is built, even when it was empty at start.
pub fn embedded_ui_present() -> bool {
    Built::get("index.html").is_some()
}

async fn embedded(uri: Uri) -> Response {
    if !embedded_ui_present() {
        return no_ui().await;
    }
    asset(uri.path(), |name| Built::get(name).map(|f| (f.data.into_owned(), f.metadata.mimetype().to_string())))
}

/// The answer for `path` from files looked up by name: the file with its
/// content type; a 404 for a missing file under `/assets/`, where the build
/// puts its files; and `index.html` for anything else, a client-side route,
/// whose last segment may well hold a dot (`/n/file:src/lib.rs`).
fn asset(path: &str, get: impl Fn(&str) -> Option<(Vec<u8>, String)>) -> Response {
    let name = match path.trim_start_matches('/') {
        "" => "index.html",
        n => n,
    };
    let serve = |(bytes, mime): (Vec<u8>, String)| ([(CONTENT_TYPE, mime)], bytes).into_response();
    if let Some(file) = get(name) {
        return serve(file);
    }
    if name.starts_with("assets/") {
        return StatusCode::NOT_FOUND.into_response();
    }
    get("index.html").map_or_else(|| StatusCode::NOT_FOUND.into_response(), serve)
}

async fn no_ui() -> Response {
    (
        StatusCode::OK,
        [("content-type", "text/html; charset=utf-8")],
        "<!doctype html><title>grimoire</title><body style=\"background:#111;color:#ddd;font-family:system-ui;padding:2rem\"><h1>grimoire</h1><p>The daemon is running. The API is under <code>/api/&lt;tool&gt;</code> (<a href=\"/api/tools\" style=\"color:#9cf\">list</a>), MCP at <code>/mcp</code>, events at <code>/api/events</code>. This binary holds no UI: it was built without <code>web/dist</code>. Give one with <code>--ui-dir</code>, run the Vite dev server, or build with <code>scripts/release</code>.</p>",
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

async fn run(daemon: Arc<Daemon>, caller: Option<Caller>, tool: String, input: Value) -> Response {
    if !grimoire_core::tools::TOOL_NAMES.contains(&tool.as_str()) {
        return error_response(ToolError::invalid(format!("unknown tool `{tool}`"), vec![]));
    }
    if let Err(e) = access::may_call(caller.as_ref(), &tool) {
        return (StatusCode::FORBIDDEN, Json(serde_json::to_value(&e).unwrap_or_default())).into_response();
    }
    let result = tokio::task::spawn_blocking(move || daemon.call(&tool, input)).await;
    match result {
        Ok(Ok(v)) => Json(v).into_response(),
        Ok(Err(e)) => error_response(e),
        Err(e) => error_response(ToolError::unavailable(e.to_string())),
    }
}

async fn call_post(State(daemon): State<Arc<Daemon>>, caller: Option<Extension<Caller>>, Path(tool): Path<String>, headers: HeaderMap, body: Bytes) -> Response {
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
    run(daemon, caller.map(|Extension(c)| c), tool, input).await
}

/// GET form for convenience: each query value is parsed as JSON when it is,
/// otherwise taken as a string, so `?ref=page:overview&depth=3` works.
async fn call_get(State(daemon): State<Arc<Daemon>>, caller: Option<Extension<Caller>>, Path(tool): Path<String>, Query(q): Query<Vec<(String, String)>>) -> Response {
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
    run(daemon, caller.map(|Extension(c)| c), tool, Value::Object(input)).await
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

    /// # Test contract
    ///
    /// ```json
    /// {"test": "the-embedded-ui-serves-files-routes-and-404s", "checks": ["crates/server/src/http#ui-embedded"]}
    /// ```
    #[tokio::test]
    async fn the_embedded_ui_serves_files_routes_and_404s() {
        let files = |name: &str| match name {
            "index.html" => Some((b"<div id=root></div>".to_vec(), "text/html".to_string())),
            "assets/app.js" => Some((b"run()".to_vec(), "text/javascript".to_string())),
            _ => None,
        };
        let body = |r: Response| async move { axum::body::to_bytes(r.into_body(), 1 << 20).await.map(|b| b.to_vec()).unwrap_or_default() };
        for route in ["/", "/board", "/n/page%3Agoal", "/n/file%3Acrates%2Fcore%2Fsrc%2Flib.rs", "/unplaced"] {
            let r = asset(route, files);
            assert_eq!(r.status(), StatusCode::OK, "{route}");
            assert_eq!(body(r).await, b"<div id=root></div>", "{route} is a client-side route");
        }
        let js = asset("/assets/app.js", files);
        assert_eq!(js.headers().get(CONTENT_TYPE).and_then(|v| v.to_str().ok()), Some("text/javascript"));
        assert_eq!(body(js).await, b"run()");
        assert_eq!(asset("/assets/missing.js", files).status(), StatusCode::NOT_FOUND, "a missing built file is not the index");
    }
}
