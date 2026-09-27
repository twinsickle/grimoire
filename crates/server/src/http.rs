//! # Module contract
//!
//! ```json
//! {"purpose":"Serve the JSON API, the event stream, MCP and the web UI from one port.","criteria":[{"id":"api-mirrors-tools","statement":"POST /api/<tool> runs the tool of that name with the JSON body as input and returns its output, or the error object with a status matching its code."},{"id":"events","statement":"GET /api/events streams every daemon event as server-sent events."},{"id":"ui-optional","statement":"When no UI directory is configured the API and MCP still serve; the UI is a static fallback, not a dependency."}]}
//! ```

use std::convert::Infallible;
use std::path::PathBuf;
use std::sync::Arc;

use axum::body::Bytes;
use axum::extract::{Path, Query, State};
use axum::http::StatusCode;
use axum::response::sse::{Event as SseEvent, KeepAlive, Sse};
use axum::response::{IntoResponse, Response};
use axum::routing::get;
use axum::{Json, Router};
use futures::stream::Stream;
use grimoire_core::ToolError;
use serde_json::{json, Value};
use tokio_stream::wrappers::BroadcastStream;
use tokio_stream::StreamExt;
use tower_http::cors::CorsLayer;
use tower_http::services::{ServeDir, ServeFile};

use crate::daemon::Daemon;

pub fn router(daemon: Arc<Daemon>, ui_dir: Option<PathBuf>) -> Router {
    let mut app = Router::new()
        .route("/api/tools", get(list_tools))
        .route("/api/events", get(events))
        .route("/api/{tool}", get(call_get).post(call_post))
        .with_state(daemon.clone())
        .nest_service("/mcp", crate::mcp::service(daemon))
        .layer(CorsLayer::permissive());
    if let Some(dir) = ui_dir {
        let index = dir.join("index.html");
        app = app.fallback_service(ServeDir::new(&dir).fallback(ServeFile::new(index)));
    } else {
        app = app.fallback(no_ui);
    }
    app
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

async fn call_post(State(daemon): State<Arc<Daemon>>, Path(tool): Path<String>, body: Bytes) -> Response {
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
