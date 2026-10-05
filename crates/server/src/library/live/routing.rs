//! # Module contract
//!
//! ```json
//! {"purpose":"Route a request to the library's grimoires: each one's API and MCP at /g/<name>/, and the daemon's own on the bare routes, handed to the checkout the request names or refused for its checkout at that grimoire's trust step; and the release a command asks for.","criteria":[{"id":"routes","statement":"A request to /g/<name>/api/... or /g/<name>/mcp is answered by the routes of a checkout the library holds of the grimoire by that name as the request arrives, the default's included, with the path the grimoire's own routes expect, its body passed on as it came but for a GET's, which carries nothing, and none of the library's own path parameters; a name the library does not hold is answered not_found."},{"id":"checkout-chosen","statement":"The checkout a request is answered by is the one its `checkout` names, read from the JSON body of a POST to /api/<tool>, from the arguments of an MCP tools/call, every tools/call of a batch naming the same one, or from the query of any other request: the served checkout whose project file is nearest above that directory, found from the directory as the system resolves it, a directory that does not exist as its nearest existing folder's, and naming this grimoire at the store or copy that checkout is served from, so a checkout kept inside another is its own and the outer one never answers for it, and the project file is looked up before the library is held; with none named, the grimoire's one checkout, or, for status, the tools list, the event stream, the export, the prune and every MCP message but a tools/call, the default's when it is a checkout of the grimoire and else the first."},{"id":"checkout-refused","statement":"A request naming no checkout to a grimoire served for several, but those answered without one, and a request naming a directory no served checkout of the grimoire is or holds, are refused, as are a batch's calls naming different checkouts: the request is handed to the grimoire's first checkout's routes marked CheckoutRefused, so its caller is admitted or refused at that trust step first, and the refusal is then answered as a tool's own invalid error is, its details naming `checkout`, over the API as that error and over MCP as a tool-level error result; the message lists the grimoire's checkouts and says to name the caller's working directory as `checkout`, and, for a call naming none, that tools offering no such field are a list the client kept from before it, which a new session fetches again and a reconnect may not, and that the HTTP API at /g/<name>/api/<tool> takes `checkout` until then."}]}
//! ```

use std::path::{Path, PathBuf};
use std::sync::Arc;

use axum::body::Body;
use axum::extract::{Path as UrlPath, Request, State};
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::routing::{any, post};
use axum::{middleware, Json, Router};
use grimoire_core::ops::ToolError;
use grimoire_core::store::FieldError;
use tower::ServiceExt;

use super::{canonical, same, Key, Live, Shelved, Took};
use crate::access;
use crate::store_io;

impl Live {
    /// The checkout of grimoire `name` that answers a call naming `checkout`, as its key, or why
    /// none does (req:machine-checkouts). `lenient`: a call answered without one, by the default's
    /// checkout when it is one of the grimoire's, else the first.
    fn choose(&self, name: &str, checkout: Option<&str>, lenient: bool) -> Result<Key, Refusal> {
        let checkout = checkout.map(str::trim).filter(|c| !c.is_empty());
        // The checkout a directory is in: its nearest project file, found from the directory as the
        // system resolves it, so a link into a checkout is that checkout's; looked up before the
        // library is held, so a slow path holds up no other call.
        let nearest = checkout.and_then(|dir| store_io::find_named(&canonical(Path::new(dir))).ok()).map(|(root, home, named)| (canonical(&root), home, named));
        let held = self.held.lock().expect("library lock");
        let ours: Vec<(&Key, &Shelved)> = held.served.iter().filter(|((n, _), _)| n == name).collect();
        let Some(&(first_key, _)) = ours.first() else {
            return Err(Refusal::NotFound(format!("this daemon's library holds no grimoire `{name}`")));
        };
        let list = || ours.iter().map(|(_, s)| s.repo.display().to_string()).collect::<Vec<_>>().join(", ");
        match checkout {
            Some(dir) => {
                // The checkout whose project file is nearest above the directory, and names this grimoire
                // at the store or copy that checkout is served from: a worktree kept inside its main
                // checkout is its own, and an outer checkout never answers for it.
                let names_it = |s: &Shelved, home: &store_io::Home| match home {
                    store_io::Home::Store(store) => same(store, &s.store_dir),
                    store_io::Home::Server { copy, .. } => same(copy, &s.store_dir),
                };
                let found = nearest.as_ref().filter(|(_, _, named)| named == name).and_then(|(root, home, _)| ours.iter().find(|(_, s)| s.canon == *root && names_it(s, home)));
                match found {
                    Some((key, _)) => Ok((*key).clone()),
                    None => Err(refused(format!("`{dir}` is no checkout of the grimoire `{name}` served here, whose checkouts are {}; name your working directory as `checkout`", list()))),
                }
            }
            None if ours.len() == 1 => Ok(first_key.clone()),
            None if lenient => Ok(ours.iter().find(|(key, _)| self.is_default(key)).map_or(first_key, |(key, _)| *key).clone()),
            // A client may keep the tools it was first given past a reconnect, so one that cannot name
            // its checkout is told how to get tools that can, or to go round them.
            None => Err(refused(format!(
                "the grimoire `{name}` is served for {} checkouts on this machine: {}; this call names none, so it cannot be told which one it comes from. Name yours with `checkout`, your working directory. If your tools offer no `checkout` field, your client is keeping a tool list from before it: start a new session, since a reconnect may not fetch the tools again, or until then call the HTTP API, POST /g/{name}/api/<tool>, naming `checkout` in the body",
                ours.len(),
                list()
            ))),
        }
    }

    /// The routes a request is handed to: the checkout's under `key`, or, refused, the grimoire's
    /// first checkout's, which admit the caller at their trust step before answering the refusal.
    pub(super) fn routes_for(&self, name: &str, key: Option<&Key>) -> Option<Router> {
        let held = self.held.lock().expect("library lock");
        match key {
            Some(key) => held.served.get(key),
            None => held.served.iter().find(|((n, _), _)| n == name).map(|(_, s)| s),
        }
        .map(|s| s.routes.clone())
    }
}

/// The routes of the library: each grimoire's API and MCP at `/g/<name>/`,
/// looked up as each request arrives, and the release a command asks for.
pub fn routes(live: Arc<Live>) -> Router {
    let grimoires =
        Router::new().route("/g/{name}/api/{*rest}", any(to_grimoire)).route("/g/{name}/mcp", any(to_grimoire_mcp)).route("/g/{name}/mcp/{*rest}", any(to_grimoire)).with_state(live.clone());
    // Only a machine's library is asked to let a store go: a team's server has no uninstall beside it.
    match live.policy.clone() {
        Some(policy) => grimoires.merge(Router::new().route("/library/release", post(release)).with_state(live).layer(middleware::from_fn_with_state(policy, access::admit))),
        None => grimoires,
    }
}

/// The bare routes, /api/... and /mcp, for the grimoire `name` the daemon was started for: answered
/// as /g/<name>/ is, so a call there is answered by the checkout it names too (req:machine-checkouts).
pub fn bare(live: Arc<Live>, name: String) -> Router {
    let api = {
        let (live, name) = (live.clone(), name.clone());
        move |UrlPath(rest): UrlPath<String>, req: Request| {
            let (live, name) = (live.clone(), name.clone());
            async move { forward(&live, &name, &format!("/api/{rest}"), req).await }
        }
    };
    let mcp_rest = {
        let (live, name) = (live.clone(), name.clone());
        move |UrlPath(rest): UrlPath<String>, req: Request| {
            let (live, name) = (live.clone(), name.clone());
            async move { forward(&live, &name, &format!("/mcp/{rest}"), req).await }
        }
    };
    let mcp = move |req: Request| {
        let (live, name) = (live.clone(), name.clone());
        async move { forward(&live, &name, "/mcp", req).await }
    };
    Router::new().route("/api/{*rest}", any(api)).route("/mcp", any(mcp)).route("/mcp/{*rest}", any(mcp_rest))
}

async fn to_grimoire(State(live): State<Arc<Live>>, UrlPath((name, rest)): UrlPath<(String, String)>, req: Request) -> Response {
    let under = if req.uri().path().starts_with(&format!("/g/{name}/mcp/")) { "mcp" } else { "api" };
    forward(&live, &name, &format!("/{under}/{rest}"), req).await
}

async fn to_grimoire_mcp(State(live): State<Arc<Live>>, UrlPath(name): UrlPath<String>, req: Request) -> Response {
    forward(&live, &name, "/mcp", req).await
}

/// Why no checkout answers a request.
enum Refusal {
    NotFound(String),
    Invalid(ToolError),
}

/// A call refused for its checkout: an invalid error naming the `checkout` field, as a tool's own is.
fn refused(message: String) -> Refusal {
    Refusal::Invalid(ToolError::invalid(message.clone(), vec![FieldError { field: "checkout".into(), reason: message }]))
}

/// The most a request's body may hold for the library to read its `checkout`.
const BODY_LIMIT: usize = 64 * 1024 * 1024;

/// What a request says about its checkout: for each call in it, the directory it names and
/// whether it may be answered without one. An MCP message that is no tools/call is one call
/// answered by any checkout; a batch is as many as its tools/calls.
fn asked(path: &str, query: Option<&str>, body: &[u8]) -> Vec<(Option<String>, bool)> {
    let from_query = query.and_then(|q| q.split('&').filter_map(|pair| pair.split_once('=')).find(|(k, _)| *k == "checkout").map(|(_, v)| percent_decode(v)));
    let json: Option<serde_json::Value> = serde_json::from_slice(body).ok();
    if path == "/mcp" || path.starts_with("/mcp/") {
        let messages: Vec<serde_json::Value> = match json {
            Some(serde_json::Value::Array(all)) => all,
            Some(one) => vec![one],
            None => vec![],
        };
        let calls: Vec<(Option<String>, bool)> = messages
            .iter()
            .filter(|m| m["method"] == "tools/call")
            .map(|m| (m["params"]["arguments"]["checkout"].as_str().map(str::to_string), m["params"]["name"].as_str().is_some_and(grimoire_core::tools::answers_any_checkout)))
            .collect();
        // Every other message is the same whichever checkout answers.
        return if calls.is_empty() { vec![(from_query, true)] } else { calls };
    }
    let tool = path.strip_prefix("/api/").unwrap_or("");
    // The tools list, the event stream, the export and the prune are routes, not tools: the same whichever checkout answers.
    let lenient = grimoire_core::tools::answers_any_checkout(tool) || matches!(tool, "tools" | "events" | "export" | "prune");
    let from_body = json.as_ref().and_then(|j| j["checkout"].as_str().map(str::to_string));
    vec![(from_body.or(from_query), lenient)]
}

/// A query value with its `+` and `%XX` escapes undone.
fn percent_decode(v: &str) -> String {
    let bytes = v.replace('+', " ").into_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        let hex = |b: u8| (b as char).to_digit(16);
        match (bytes[i], bytes.get(i + 1).copied().and_then(hex), bytes.get(i + 2).copied().and_then(hex)) {
            (b'%', Some(h), Some(l)) => {
                out.push((h * 16 + l) as u8);
                i += 3;
            }
            (b, _, _) => {
                out.push(b);
                i += 1;
            }
        }
    }
    String::from_utf8_lossy(&out).into_owned()
}

/// The checkout every call of a request names, or why it cannot be answered by one.
fn one_checkout(live: &Live, name: &str, calls: &[(Option<String>, bool)]) -> Result<Key, Refusal> {
    let mut chosen: Option<Key> = None;
    for (checkout, lenient) in calls {
        let key = live.choose(name, checkout.as_deref(), *lenient)?;
        match &chosen {
            Some(other) if *other != key => return Err(refused(format!("the calls of one request name different checkouts of the grimoire `{name}`; send each checkout's calls apart"))),
            _ => chosen = Some(key),
        }
    }
    chosen.ok_or_else(|| Refusal::NotFound(format!("this daemon's library holds no grimoire `{name}`")))
}

/// Hand `req` to the routes of the checkout of grimoire `name` it names, at `path`, its query
/// kept. The request is made afresh from its method, headers and body: the library's own
/// route left its path parameters on it, which the grimoire's routes would read as theirs,
/// and the grimoire's trust step admits the caller again itself. A request refused for its
/// checkout goes to the grimoire's first checkout marked CheckoutRefused, so it meets the
/// trust step first and its error is answered as any tool's is, over the API or MCP alike.
async fn forward(live: &Arc<Live>, name: &str, path: &str, req: Request<Body>) -> Response {
    let query = req.uri().query().map(str::to_string);
    let target = match &query {
        Some(q) => format!("{path}?{q}"),
        None => path.to_string(),
    };
    let (parts, body) = req.into_parts();
    // A GET, the event stream's among them, carries nothing in its body and names its checkout in
    // its query; any other request is read whole for it and passed on as it came.
    let bytes = if parts.method == axum::http::Method::GET {
        Default::default()
    } else {
        match axum::body::to_bytes(body, BODY_LIMIT).await {
            Ok(b) => b,
            Err(_) => return (StatusCode::PAYLOAD_TOO_LARGE, Json(serde_json::json!({ "error": "invalid", "message": "the request is too large" }))).into_response(),
        }
    };
    let calls = asked(path, query.as_deref(), &bytes);
    let mut chosen = one_checkout(live, name, &calls);
    // A directory in a checkout not served yet: served, then chosen (req:machine-checkouts), but only for a
    // caller this grimoire admits; any other meets the trust step, as a refusal does.
    if matches!(chosen, Err(Refusal::Invalid(_))) && live.admits(name, &parts.headers) {
        let dirs: Vec<String> = calls.iter().filter_map(|(c, _)| c.clone()).collect();
        if !dirs.is_empty() {
            let (owner, grimoire) = (live.clone(), name.to_string());
            let took = tokio::task::spawn_blocking(move || dirs.iter().map(|dir| owner.take_checkout(&grimoire, dir)).collect::<Vec<Took>>()).await.unwrap_or_default();
            if took.iter().any(|t| matches!(t, Took::Served)) {
                chosen = one_checkout(live, name, &calls);
            } else if let Some(Took::Failed(why)) = took.into_iter().find(|t| matches!(t, Took::Failed(_))) {
                chosen = Err(refused(why));
            }
        }
    }
    let (routes, refusal) = match chosen {
        Ok(key) => (live.routes_for(name, Some(&key)), None),
        Err(Refusal::Invalid(e)) => (live.routes_for(name, None), Some(e)),
        Err(Refusal::NotFound(message)) => return (StatusCode::NOT_FOUND, Json(serde_json::json!({ "error": "not_found", "message": message }))).into_response(),
    };
    let Some(routes) = routes else {
        return (StatusCode::NOT_FOUND, Json(serde_json::json!({ "error": "not_found", "message": format!("this daemon's library holds no grimoire `{name}`") }))).into_response();
    };
    let mut fresh = Request::new(Body::from(bytes));
    if let Some(e) = refusal {
        fresh.extensions_mut().insert(access::CheckoutRefused(e));
    }

    match target.parse() {
        Ok(uri) => *fresh.uri_mut() = uri,
        Err(_) => return StatusCode::BAD_REQUEST.into_response(),
    }
    *fresh.method_mut() = parts.method;
    *fresh.version_mut() = parts.version;
    *fresh.headers_mut() = parts.headers;
    match routes.oneshot(fresh).await {
        Ok(r) => r,
        Err(never) => match never {},
    }
}

#[derive(serde::Deserialize)]
struct ReleaseIn {
    store: String,
    /// The command that asked for the release is done with the store: refresh may take it again.
    #[serde(default)]
    done: bool,
}

async fn release(State(live): State<Arc<Live>>, Json(input): Json<ReleaseIn>) -> Response {
    let store = PathBuf::from(input.store);
    let answer = tokio::task::spawn_blocking(move || {
        if input.done {
            live.restore(&store);
            serde_json::json!({ "restored": true })
        } else {
            serde_json::json!({ "released": live.release(&store) })
        }
    })
    .await
    .unwrap_or_else(|_| serde_json::json!({ "released": false }));
    Json(answer).into_response()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::daemon::Daemon;
    use crate::http;
    use crate::library::live::tests::project;
    use crate::library::Grimoire;

    /// Answer `status` at `routes`, for the repository it names.
    async fn repository_of(routes: Router, checkout: Option<&str>) -> String {
        let body = match checkout {
            Some(c) => serde_json::json!({ "checkout": c }).to_string(),
            None => "{}".to_string(),
        };
        let req = Request::builder().method("POST").uri("/api/status").header("host", "127.0.0.1").header("content-type", "application/json").body(Body::from(body)).unwrap();
        let res = routes.oneshot(req).await.unwrap();
        let bytes = axum::body::to_bytes(res.into_body(), 1 << 20).await.unwrap();
        serde_json::from_slice::<serde_json::Value>(&bytes).unwrap()["repository"].as_str().unwrap_or_default().to_string()
    }

    /// # Test contract
    ///
    /// ```json
    /// {"test": "a-call-is-answered-by-the-checkout-it-names", "checks": ["crates/server/src/library/live/routing#checkout-chosen", "crates/server/src/library/live/routing#checkout-refused"]}
    /// ```
    #[tokio::test]
    async fn a_call_is_answered_by_the_checkout_it_names() {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path();
        // `shop`, the daemon's own, and a worktree of it kept inside it, served as two checkouts of one team's grimoire.
        let main = project(root, "shop");
        let worktree = project(&main.repo, "wt");
        // A worktree of `shop`: its project file names the grimoire `shop`, at the store it is served from.
        std::fs::write(worktree.repo.join(store_io::PROJECT_FILE), format!("name = \"shop\"\nstore = {}\n", toml::Value::String(worktree.store_dir.to_string_lossy().to_string()))).unwrap();
        let _locks = (store_io::lock(&main.store_dir).unwrap(), store_io::lock(&worktree.store_dir).unwrap());
        let daemon = Arc::new(Daemon::open(&main.store_dir).unwrap().0);
        let live = Live::new(Grimoire { server: Some("http://team".into()), ..main.clone() }, daemon, 4299, Arc::new(access::Local), root.join("none"), root.join("none"));
        let other = Arc::new(Daemon::open(&worktree.store_dir).unwrap().0);
        {
            let mut held = live.held.lock().unwrap();
            let shelved = Shelved {
                store_dir: worktree.store_dir.clone(),
                repo: worktree.repo.clone(),
                canon: canonical(&worktree.repo),
                team: true,
                daemon: other.clone(),
                routes: http::api(other, Arc::new(access::Local)),
                lock: None,
                watcher: None,
            };
            held.served.insert(("shop".into(), worktree.repo.clone()), shelved);
        }
        let repo = |g: &Grimoire| store_io::repository_string(&g.repo).unwrap();
        let named = |dir: &Path| dir.to_string_lossy().to_string();
        // Each checkout answers for itself; a subfolder is its checkout's, the deepest that holds it.
        let at = |key: Result<Key, Refusal>| live.routes_for("shop", Some(&key.ok().unwrap())).unwrap();
        assert_eq!(repository_of(at(live.choose("shop", Some(&named(&main.repo)), false)), None).await, repo(&main));
        assert_eq!(repository_of(at(live.choose("shop", Some(&named(&worktree.repo.join("src"))), false)), None).await, repo(&worktree), "the worktree inside the main checkout is the deeper");
        assert_eq!(repository_of(at(live.choose("shop", Some(&named(&main.repo.join("src"))), false)), None).await, repo(&main));
        // A directory that does not exist yet is its deepest existing folder's, however it is spelt.
        let unborn = named(&worktree.repo.join("src").join("not-yet").join("deeper"));
        assert_eq!(repository_of(at(live.choose("shop", Some(&unborn), false)), None).await, repo(&worktree));
        let spelt = named(&main.repo).replace('\\', "/").to_uppercase();
        if cfg!(windows) {
            assert_eq!(repository_of(at(live.choose("shop", Some(&spelt), false)), None).await, repo(&main), "another spelling of the same folder");
        }
        // Named by no call, two checkouts are refused, but for what is answered without one, by the default.
        let why = |r: Result<Key, Refusal>| match r {
            Err(Refusal::Invalid(e)) => {
                let v = serde_json::to_value(&e).unwrap();
                assert_eq!((v["error"].as_str(), v["details"][0]["field"].as_str()), (Some("invalid"), Some("checkout")), "an invalid error naming the field: {v}");
                v["message"].as_str().unwrap().to_string()
            }
            _ => panic!("refused"),
        };
        let message = why(live.choose("shop", None, false));
        assert!(
            message.contains("2 checkouts") && message.contains(&main.repo.display().to_string()) && message.contains(&worktree.repo.display().to_string()) && message.contains("`checkout`"),
            "{message}"
        );
        // A client whose tools cannot name one is told to start a new session, or to call over HTTP.
        assert!(
            message.contains("offer no `checkout` field") && message.contains("new session") && message.contains("a reconnect may not") && message.contains("POST /g/shop/api/<tool>"),
            "{message}"
        );
        assert_eq!(repository_of(at(live.choose("shop", None, true)), None).await, repo(&main), "the default's");
        // A directory no checkout holds is refused, naming them.
        let message = why(live.choose("shop", Some(&named(root)), false));
        assert!(message.contains("is no checkout of the grimoire `shop`") && message.contains(&worktree.repo.display().to_string()), "{message}");
        // The calls of one request name one checkout, or all are refused.
        assert!(one_checkout(&live, "shop", &[(Some(named(&main.repo)), false), (Some(named(&main.repo.join("src"))), false)]).is_ok());
        let message = why(one_checkout(&live, "shop", &[(Some(named(&main.repo)), false), (Some(named(&worktree.repo)), false)]));
        assert!(message.contains("different checkouts"), "{message}");
        // A store of its own serves one checkout: another is told so, and that several make a team.
        let ledger = project(root, "ledger");
        let _ledger_lock = store_io::lock(&ledger.store_dir).unwrap();
        let solo = Arc::new(Daemon::open(&ledger.store_dir).unwrap().0);
        {
            let mut held = live.held.lock().unwrap();
            let shelved = Shelved {
                store_dir: ledger.store_dir.clone(),
                repo: ledger.repo.clone(),
                canon: canonical(&ledger.repo),
                team: false,
                daemon: solo.clone(),
                routes: http::api(solo, Arc::new(access::Local)),
                lock: None,
                watcher: None,
            };
            held.served.insert(("ledger".into(), ledger.repo.clone()), shelved);
        }
        assert!(live.choose("ledger", None, false).is_ok(), "one checkout needs no naming");
        // A directory in another project's checkout is no checkout of this store: refused, and nothing taken.
        let message = why(live.choose("ledger", Some(&named(&main.repo)), false));
        assert!(message.contains("is no checkout of the grimoire `ledger`"), "{message}");
        assert!(matches!(live.take_checkout("ledger", &named(&main.repo)), Took::NotOurs), "its project file names another store");
        assert!(matches!(live.choose("audit", None, true), Err(Refusal::NotFound(_))));
    }

    /// # Test contract
    ///
    /// ```json
    /// {"test": "a-request-says-its-checkout", "checks": ["crates/server/src/library/live/routing#checkout-chosen"]}
    /// ```
    #[test]
    fn a_request_says_its_checkout() {
        assert_eq!(asked("/api/lint", None, br#"{"checkout":"C:/shop/wt"}"#), [(Some("C:/shop/wt".to_string()), false)]);
        assert_eq!(asked("/api/status", None, b"{}"), [(None, true)], "status is answered without one");
        assert_eq!(asked("/api/events", Some("checkout=C%3A%2Fshop+wt&x=1"), b""), [(Some("C:/shop wt".to_string()), true)], "from the query, unescaped");
        let call = br#"{"jsonrpc":"2.0","id":7,"method":"tools/call","params":{"name":"lint","arguments":{"checkout":"C:/shop"}}}"#;
        assert_eq!(asked("/mcp", None, call), [(Some("C:/shop".to_string()), false)]);
        assert_eq!(asked("/mcp", None, br#"{"jsonrpc":"2.0","id":1,"method":"tools/list"}"#), [(None, true)], "any other MCP message goes to any checkout");
        let batch = br#"[{"jsonrpc":"2.0","method":"notifications/initialized"},{"jsonrpc":"2.0","id":2,"method":"tools/call","params":{"name":"status","arguments":{}}},{"jsonrpc":"2.0","id":3,"method":"tools/call","params":{"name":"lint","arguments":{"checkout":"C:/b"}}}]"#;
        assert_eq!(asked("/mcp", None, batch), [(None, true), (Some("C:/b".to_string()), false)], "every call of a batch");
    }
}
