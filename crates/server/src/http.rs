//! # Module contract
//!
//! ```json
//! {"purpose":"Serve the JSON API, the event stream, MCP and the web UI from one port.","criteria":[{"id":"api-mirrors-tools","statement":"POST /api/<tool> runs the tool of that name with the JSON body as input and returns its output, or the error object with a status matching its code."},{"id":"checkout-from-the-parse","statement":"Each handler reads and parses its request once, after the trust step, and asks the routes' answerer for the daemon that answers from the `checkout` it parsed: the input of POST /api/<tool>, the query of GET /api/<tool>, of the event stream and of the export, and the query of the prune and of the recipes, whose bodies name only their findings and recipes; the tools list needs no daemon; the answerer's error, a checkout refused, is answered as a tool's own error is, after may_call, and over one daemon the answerer is that daemon whatever is named."},{"id":"events","statement":"GET /api/events streams every daemon event as server-sent events."},{"id":"ui-optional","statement":"The UI is served from --ui-dir when given, otherwise from the copy embedded in the binary when there is one, otherwise as a plain page saying no UI is available; the API and MCP serve in every case."},{"id":"shared-page","statement":"A router built for a shared daemon answers every path that is not the API, the event stream or MCP with a plain page saying it is a shared server with no interface of its own, whatever UI the build holds."},{"id":"same-origin","statement":"The JSON API, the event stream and MCP answer 403 with the error object to a request whose Origin is not the daemon's own or whose Sec-Fetch-Site is cross-site or same-site, under either policy, and under the local policy to one whose Host is not localhost, 127.0.0.1 or [::1]; a POST to /api/<tool> whose body is not application/json is refused with 415; no route sends CORS headers, and the UI's own files are served to any navigation."},{"id":"reads-on-get","statement":"GET /api/<tool> runs read tools only; a write tool named over GET is refused as invalid with status 405."},{"id":"ui-embedded","statement":"From the embedded UI, a file is served with its content type; a missing file under /assets/ is a 404; and any other path, a client-side route, is answered with index.html."},{"id":"prune","statement":"POST /api/prune takes a JSON body naming the findings the main line saw and the digest of the intent it judged them against, and has the daemon prune its baseline to them, refused as a conflict when the intent has moved since, answering how many entries went and how many are left; it is refused with 403 to a caller who may only read, with 415 to a body that is not JSON, as invalid to one that names anything else, and as a conflict by a daemon that holds code; it is a route and not a tool, so it is not listed for an agent nor offered over MCP."},{"id":"recipes","statement":"POST /api/recipes takes a JSON body of the recipes the main line read and has the daemon keep them as its record of the main line's recipes, answering how many it holds; it is refused with 403 to a caller who may only read, with 415 to a body that is not JSON, as invalid to one that is not that record or does not validate, and as a conflict by a daemon that holds code; it takes a body of up to 4 MiB, and it is a route and not a tool, so it is not listed for an agent nor offered over MCP."},{"id":"every-route-passes","statement":"/api/tools, /api/events, /api/export, /api/prune, /api/recipes, /api/<tool> and /mcp all sit behind the trust step, which runs before any of their handlers, and /api/<tool> asks may_call before dispatch; only the UI's own files are served without it."},{"id":"export","statement":"GET /api/export answers the store's whole intent as one value, to any caller the trust step admits, a reader included; it is a route and not a tool, so it is not listed for an agent nor offered over MCP."},{"id":"passes-the-token","statement":"POST /api/<tool> hands the tool the bearer token its request carries, so a daemon whose intent is a server's sends that caller's write signed in as the caller; no other daemon makes any use of it past the trust step."},{"id":"library-routes","statement":"with_ui serves the interface for every path the routes beside it do not answer, a grimoire's client routes under /g/<name>/ included, while those routes answer each grimoire's API and MCP at its own path, with the built files served from the root."}]}
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

/// Which daemon answers a call, chosen from the `checkout` its handler parsed (req:machine-checkouts):
/// one daemon's routes always answer with it; a library's grimoire with the checkout the call names.
/// Asked off the async runtime, since choosing may look for a project file or serve a checkout.
pub trait Answerer: Send + Sync {
    /// The daemon answering a call naming `checkout`; `lenient`: one that may be answered without one.
    fn daemon(&self, checkout: Option<&str>, lenient: bool) -> Result<Arc<Daemon>, ToolError>;
}

/// One daemon, answering whatever a call names.
struct One(Arc<Daemon>);

impl Answerer for One {
    fn daemon(&self, _checkout: Option<&str>, _lenient: bool) -> Result<Arc<Daemon>, ToolError> {
        Ok(self.0.clone())
    }
}

/// The daemon a call naming `checkout` is answered by, asked off the async runtime.
async fn answering(answerer: &Arc<dyn Answerer>, checkout: Option<String>, lenient: bool) -> Result<Arc<Daemon>, ToolError> {
    let answerer = answerer.clone();
    tokio::task::spawn_blocking(move || answerer.daemon(checkout.as_deref(), lenient)).await.unwrap_or_else(|e| Err(ToolError::unavailable(e.to_string())))
}

/// Where the UI's files come from.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Ui {
    /// A directory given on the command line, which overrides the embedded copy.
    Dir(PathBuf),
    /// The copy embedded at build time, when the build had one.
    Embedded,
    /// None: the API and MCP only, and a page saying so.
    None,
    /// A shared daemon: no interface whatever the build holds, and a page saying what it is.
    Shared,
}

/// The built UI, `web/dist`. A release build embeds it; a debug build reads
/// it from disk, so the dev loop is unchanged. A build without it still
/// succeeds (its build script warns), and then holds no files.
#[derive(rust_embed::RustEmbed)]
#[folder = "../../web/dist/"]
#[allow_missing = true]
struct Built;

pub fn router(daemon: Arc<Daemon>, ui: Ui, policy: Arc<dyn Policy>) -> Router {
    with_ui(api(daemon, policy), ui)
}

/// One grimoire's routes, behind the trust step: the tools, the event stream,
/// the export, the prune, the main line's recipes and MCP. A library nests one under `/g/<name>` for
/// each of its grimoires (req:machine-library).
pub fn api(daemon: Arc<Daemon>, policy: Arc<dyn Policy>) -> Router {
    api_over(Arc::new(One(daemon)), policy)
}

/// A grimoire's routes over `answerer`, which says which daemon answers each call: the
/// trust step runs on the headers first, and each handler parses its request once.
pub fn api_over(answerer: Arc<dyn Answerer>, policy: Arc<dyn Policy>) -> Router {
    Router::new()
        .route("/api/tools", get(list_tools))
        .route("/api/events", get(events))
        .route("/api/export", get(export))
        .route("/api/prune", axum::routing::post(prune))
        // The main line's recipes may be long prose: up to what the trust step drains of a refused body.
        .route("/api/recipes", axum::routing::post(recipes).layer(axum::extract::DefaultBodyLimit::max(4 << 20)))
        .route("/api/{tool}", get(call_get).post(call_post))
        .with_state(answerer.clone())
        .nest_service("/mcp", crate::mcp::service(answerer))
        // Everything above passes the trust step first; the UI's files below are
        // served to any navigation, so a link to the UI from elsewhere opens it.
        .layer(middleware::from_fn_with_state(policy, access::admit))
}

/// `app` with the interface served beside it, for anything its routes do not answer.
pub fn with_ui(mut app: Router, ui: Ui) -> Router {
    match ui {
        Ui::Dir(dir) => {
            let index = dir.join("index.html");
            app = app.fallback_service(ServeDir::new(&dir).fallback(ServeFile::new(index)));
        }
        Ui::Embedded => app = app.fallback(embedded),
        Ui::None => app = app.fallback(no_ui),
        Ui::Shared => app = app.fallback(shared_page),
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

/// What a shared daemon answers in place of an interface.
async fn shared_page() -> Response {
    (
        StatusCode::OK,
        [("content-type", "text/html; charset=utf-8")],
        "<!doctype html><title>grimoire</title><body style=\"background:#111;color:#ddd;font-family:system-ui;padding:2rem\"><h1>grimoire</h1><p>This is a shared Grimoire server. It holds a team's intent and has no interface of its own: point a project at it and read through your own daemon. The API is under <code>/api/</code> and MCP at <code>/mcp</code>, for callers who sign in with a token.</p></body>",
    )
        .into_response()
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

async fn run(answerer: Arc<dyn Answerer>, caller: Option<Caller>, tool: String, input: Value, token: Option<String>) -> Response {
    if !grimoire_core::tools::TOOL_NAMES.contains(&tool.as_str()) {
        return error_response(ToolError::invalid(format!("unknown tool `{tool}`"), vec![]));
    }
    if let Err(e) = access::may_call(caller.as_ref(), &tool) {
        return (StatusCode::FORBIDDEN, Json(serde_json::to_value(&e).unwrap_or_default())).into_response();
    }
    // The checkout the parsed input names chooses the daemon (req:machine-checkouts); the tool never sees it.
    let checkout = input.get("checkout").and_then(Value::as_str).map(str::to_string);
    let lenient = grimoire_core::tools::answers_any_checkout(&tool);
    let result = tokio::task::spawn_blocking(move || answerer.daemon(checkout.as_deref(), lenient).and_then(|daemon| daemon.call_as(&tool, input, token))).await;
    match result {
        Ok(Ok(v)) => Json(v).into_response(),
        Ok(Err(e)) => error_response(e),
        Err(e) => error_response(ToolError::unavailable(e.to_string())),
    }
}

async fn call_post(State(answerer): State<Arc<dyn Answerer>>, caller: Option<Extension<Caller>>, Path(tool): Path<String>, headers: HeaderMap, body: Bytes) -> Response {
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
    // The token the request carries: a command run with GRIMOIRE_TOKEN sends it, and on a project
    // that names a server its write goes to the server signed in as that command, not as this daemon.
    let token = headers.get(axum::http::header::AUTHORIZATION).and_then(|v| v.to_str().ok()).and_then(|v| v.strip_prefix("Bearer ")).map(|t| t.trim().to_string()).filter(|t| !t.is_empty());
    run(answerer, caller.map(|Extension(c)| c), tool, input, token).await
}

/// GET form for convenience: each query value is parsed as JSON when it is,
/// otherwise taken as a string, so `?ref=page:overview&depth=3` works.
async fn call_get(State(answerer): State<Arc<dyn Answerer>>, caller: Option<Extension<Caller>>, Path(tool): Path<String>, Query(q): Query<Vec<(String, String)>>) -> Response {
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
    run(answerer, caller.map(|Extension(c)| c), tool, Value::Object(input), None).await
}

/// The store's whole intent in one read, for a machine that keeps a copy of
/// it. Not a tool: no agent has a use for a whole store in one answer.
async fn export(State(answerer): State<Arc<dyn Answerer>>, Query(q): Query<Named>) -> Response {
    let daemon = match answering(&answerer, q.checkout, true).await {
        Ok(d) => d,
        Err(e) => return error_response(e),
    };
    match tokio::task::spawn_blocking(move || daemon.exported()).await {
        Ok(exported) => Json(exported).into_response(),
        Err(e) => error_response(ToolError::unavailable(e.to_string())),
    }
}

/// Prune the baseline to the findings the main line saw, for the command a
/// pipeline runs there. Not a tool: an agent on a branch is never offered a
/// way to prune the team's baseline. It writes, so it takes a caller who may.
async fn prune(State(answerer): State<Arc<dyn Answerer>>, Query(q): Query<Named>, caller: Option<Extension<Caller>>, headers: HeaderMap, body: Bytes) -> Response {
    #[derive(serde::Deserialize)]
    #[serde(deny_unknown_fields)]
    struct In {
        present: Vec<grimoire_core::model::FindingKey>,
        /// The digest of the intent the findings were judged against.
        intent: String,
    }
    if let Some(refused) = refused_unless_writer(caller.as_ref().map(|Extension(c)| c), "a prune writes the baseline") {
        return refused;
    }
    if !is_json(&headers) {
        let e = ToolError::invalid("send the findings as JSON with Content-Type: application/json", vec![]);
        return (StatusCode::UNSUPPORTED_MEDIA_TYPE, Json(serde_json::to_value(e).unwrap_or_default())).into_response();
    }
    let input: In = match serde_json::from_slice(&body) {
        Ok(i) => i,
        Err(e) => return error_response(ToolError::invalid(format!("the body is not a list of present findings: {e}"), vec![])),
    };
    let daemon = match answering(&answerer, q.checkout, true).await {
        Ok(d) => d,
        Err(e) => return error_response(e),
    };
    match tokio::task::spawn_blocking(move || daemon.prune_to(input.present, &input.intent)).await {
        Ok(Ok(v)) => Json(v).into_response(),
        Ok(Err(e)) => error_response(e),
        Err(e) => error_response(ToolError::unavailable(e.to_string())),
    }
}

/// Keep the recipes the main line read, for the site of a daemon that holds no code
/// (req:recipe-main-line-snapshot). Not a tool, as the prune is not: it writes, so it takes a caller who may.
async fn recipes(State(answerer): State<Arc<dyn Answerer>>, Query(q): Query<Named>, caller: Option<Extension<Caller>>, headers: HeaderMap, body: Bytes) -> Response {
    if let Some(refused) = refused_unless_writer(caller.as_ref().map(|Extension(c)| c), "sending the main line's recipes writes the store") {
        return refused;
    }
    if !is_json(&headers) {
        let e = ToolError::invalid("send the recipes as JSON with Content-Type: application/json", vec![]);
        return (StatusCode::UNSUPPORTED_MEDIA_TYPE, Json(serde_json::to_value(e).unwrap_or_default())).into_response();
    }
    let input: grimoire_core::model::MainLineRecipes = match serde_json::from_slice(&body) {
        Ok(i) => i,
        Err(e) => return error_response(ToolError::invalid(format!("the body is not the main line's recipes: {e}"), vec![])),
    };
    let daemon = match answering(&answerer, q.checkout, true).await {
        Ok(d) => d,
        Err(e) => return error_response(e),
    };
    match tokio::task::spawn_blocking(move || daemon.take_main_line_recipes(input)).await {
        Ok(Ok(v)) => Json(v).into_response(),
        Ok(Err(e)) => error_response(e),
        Err(e) => error_response(ToolError::unavailable(e.to_string())),
    }
}

/// A 403 unless the trust step admitted `caller` to write: `what` says why it must.
fn refused_unless_writer(caller: Option<&Caller>, what: &str) -> Option<Response> {
    let e = match caller {
        Some(c) if c.access == access::Access::ReadWrite => return None,
        Some(c) => ToolError::invalid(format!("{what}, and caller `{}` may only read", c.who), vec![]),
        None => ToolError::invalid("the call was not admitted by the daemon's trust step", vec![]),
    };
    Some((StatusCode::FORBIDDEN, Json(serde_json::to_value(&e).unwrap_or_default())).into_response())
}

/// Whether a request's body is declared JSON.
fn is_json(headers: &HeaderMap) -> bool {
    headers.get(CONTENT_TYPE).and_then(|v| v.to_str().ok()).is_some_and(|ct| ct.split(';').next().is_some_and(|m| m.trim().eq_ignore_ascii_case("application/json")))
}

/// The checkout a request with no body names in its query.
#[derive(serde::Deserialize)]
struct Named {
    checkout: Option<String>,
}

async fn events(State(answerer): State<Arc<dyn Answerer>>, Query(q): Query<Named>) -> Response {
    match answering(&answerer, q.checkout, true).await {
        Ok(daemon) => stream_of(&daemon).into_response(),
        Err(e) => error_response(e),
    }
}

fn stream_of(daemon: &Daemon) -> Sse<impl Stream<Item = Result<SseEvent, Infallible>>> {
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
        for route in ["/", "/cauldron", "/n/page%3Agoal", "/n/file%3Acrates%2Fcore%2Fsrc%2Flib.rs", "/unplaced", "/g/ledger/", "/g/ledger/cauldron"] {
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
