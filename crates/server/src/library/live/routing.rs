//! # Module contract
//!
//! ```json
//! {"purpose":"Route a request to the library's grimoires: each one's API and MCP at /g/<name>/, and the daemon's own on the bare routes, handed unread to that grimoire's routes, which answer each call from the checkout it names; and the release a command asks for; and serve a checkout a call names that is not served yet.","criteria":[{"id":"handed-to-the-grimoires-routes","statement":"A request to /g/<name>/api/... or /g/<name>/mcp, or to the bare routes for the daemon's own grimoire, is handed to that grimoire's routes as the request arrives, behind its trust step."},{"id":"path-the-routes-expect","statement":"A request is handed on with the path the grimoire's own routes expect and none of the library's own path parameters."},{"id":"body-passed-on-unread","statement":"The library neither reads nor parses a request's body, passing it on unread."},{"id":"refused-caller-costs-a-discard","statement":"A caller the trust step refuses costs the library no more than its body's discard."},{"id":"handler-chooses-the-checkout","statement":"The handler that parses the call chooses its checkout."},{"id":"unheld-name-not-found","statement":"A request naming a grimoire the library does not hold is answered not_found."},{"id":"nearest-project-file","statement":"The checkout a call is answered by is the one the `checkout` its handler parsed names: the served checkout whose project file is nearest above that directory, and naming this grimoire at the store or copy that checkout is served from."},{"id":"nested-checkout-is-its-own","statement":"A checkout kept inside another is its own, and the outer one never answers for it."},{"id":"resolved-as-the-system-does","statement":"The checkout a call names is found from the directory as the system resolves it."},{"id":"missing-directory-is-its-nearest-existing","statement":"A call naming a directory that does not exist is answered by the checkout of its nearest existing folder."},{"id":"project-file-looked-up-before-library-held","statement":"The project file is looked up before the library is held."},{"id":"one-checkout-needs-no-naming","statement":"A call naming no checkout is answered by the grimoire's one checkout."},{"id":"default-answers-when-none-named","statement":"A call naming no checkout, for status, the event stream, the export and the prune, is answered by the default's checkout when it is a checkout of the grimoire."},{"id":"first-answers-when-default-is-not-ours","statement":"A call naming no checkout, for status, the event stream, the export and the prune, is answered by the grimoire's first checkout when the default's is not one of the grimoire."},{"id":"none-named-for-several-refused","statement":"A call naming no checkout to a grimoire served for several, but those answered without one, is refused."},{"id":"directory-no-checkout-holds-refused","statement":"A call naming a directory no served checkout of the grimoire is or holds is refused."},{"id":"refused-after-the-trust-step","statement":"Such a refusal is made once the trust step has admitted the caller."},{"id":"refused-over-the-api-as-invalid","statement":"Such a refusal is, over the API, a tool's own invalid error, its details naming `checkout`."},{"id":"refused-over-mcp-as-tool-error","statement":"Such a refusal is, over MCP, a tool-level error result."},{"id":"refusal-lists-checkouts-and-says-to-name-yours","statement":"The refusal's message lists the grimoire's checkouts and says to name the caller's working directory as `checkout`."},{"id":"refusal-explains-a-kept-tool-list","statement":"The refusal of a call naming no checkout also says that tools offering no such field are a list the client kept from before it, which a new session fetches again and a reconnect may not, and that the HTTP API at /g/<name>/api/<tool> takes `checkout` until then."},{"id":"registered-store","statement":"A request whose checkout names a directory whose nearest project file names this grimoire, in a checkout not served yet, is answered by that checkout once it is served: one of a store of its own is added to the store's checkouts through the store's writer and served as a view beside the others, sharing their lock."},{"id":"registered-team","statement":"A request whose checkout names a directory, in a checkout not served yet, of a team's grimoire this daemon serves, the same server's, has that checkout's copy of the server's intent made and is served; when that cannot be done the request is refused saying why and what to run there."},{"id":"registered-trust","statement":"No such checkout is taken for a caller the grimoire's trust step would refuse."},{"id":"registered-once","statement":"A checkout is taken one at a time with refresh and with any other such request, so it is opened once, and never while its store is held back."},{"id":"registered-name-taken","statement":"A request whose checkout names a directory whose project file names a grimoire of that name kept at another store or copied from another server is refused saying the name is served already and to give that grimoire another name in its project file, as library::name_taken words it."},{"id":"registered-no-such-grimoire","statement":"A request whose checkout names a directory whose project file names no grimoire of that name is refused as before, and nothing is added."}]}
//! ```

use std::path::{Path, PathBuf};
use std::sync::atomic::Ordering;
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

use super::{canonical, same, same_checkout, tell, Grimoire, Key, Live, Shelved};
use crate::access;
use crate::daemon::Daemon;
use crate::http;
use crate::store_io;

impl Live {
    /// Serve the checkout `dir` is in, by its nearest project file, when that file names the
    /// grimoire `name` and the checkout is not served yet: a checkout of a store of its own is
    /// added to the store's checkouts and served beside the others; one of a team's grimoire has
    /// its copy of the server's intent made, as a command run there would, and is served. Whether
    /// it is served now, or why it could not be.
    fn take_checkout(&self, name: &str, dir: &str) -> Took {
        // One at a time with refresh and with each other, so a checkout is never opened twice.
        let _one = self.refreshing.lock().expect("refresh lock");
        if self.stopped.load(Ordering::SeqCst) {
            return Took::NotOurs;
        }
        let Ok((repo, home, named)) = store_io::find_named(Path::new(dir)) else { return Took::NotOurs };
        if named != name {
            return Took::NotOurs;
        }
        if self.deletions().is_some_and(|d| store_io::deletions::is_queued(&d, home.dir())) {
            return Took::NotOurs;
        }
        let g = match home {
            store_io::Home::Store(store_dir) => {
                let first = self.held.lock().expect("library lock").served.iter().find(|((n, _), s)| n == name && s.server.is_none() && same(&s.store_dir, &store_dir)).map(|(_, s)| s.daemon.clone());
                let Some(first) = first else { return self.name_taken(name, &repo) };
                match first.register_checkout(&repo) {
                    Ok(true) => tracing::info!("library: `{name}`: added the checkout {} to its store", repo.display()),
                    Ok(false) => {}
                    Err(e) => return Took::Failed(format!("this checkout, {}, of the grimoire `{name}` could not be added to its store: {e}", repo.display())),
                }
                Grimoire { name: name.to_string(), store_dir, repo: repo.clone(), server: None }
            }
            // A worktree of a team's grimoire this daemon serves: its own copy of the same server's intent.
            store_io::Home::Server { address, copy } => {
                let ours = self.held.lock().expect("library lock").served.iter().any(|((n, _), s)| n == name && s.daemon.server.as_ref().is_some_and(|srv| srv.address == address));
                if !ours {
                    return self.name_taken(name, &repo);
                }
                Grimoire { name: name.to_string(), store_dir: copy, repo: repo.clone(), server: Some(address) }
            }
        };
        if self.held.lock().expect("library lock").served.values().any(|s| same(&s.store_dir, &g.store_dir) && same_checkout(&s.repo, &repo)) {
            return Took::Served;
        }
        let shelved = match g.server {
            Some(_) => match self.open_alone(&g) {
                Ok(shelved) => shelved,
                Err(why) => {
                    return Took::Failed(format!(
                        "this checkout, {}, of the grimoire `{name}` is not served: its copy of the server's intent could not be made: {why}. Run `grimoire check` in it, after `grimoire login` if this machine is not signed in",
                        repo.display()
                    ))
                }
            },
            None => match self.open(&g) {
                Some(shelved) => shelved,
                None => return Took::Failed(format!("this checkout, {}, of the grimoire `{name}` could not be served; the daemon's log says why", repo.display())),
            },
        };
        let mut held = self.held.lock().expect("library lock");
        if self.stopped.load(Ordering::SeqCst) || held.held_back.iter().any(|r| same(r, &g.store_dir)) || self.deletions().is_some_and(|d| store_io::deletions::is_queued(&d, &g.store_dir)) {
            drop(held);
            shelved.let_go();
            return Took::NotOurs;
        }
        tracing::info!("library: serving `{name}` from {} at /g/{name}", repo.display());
        held.served.insert((name.to_string(), repo), shelved);
        // Told before the lock is let go, so the request that asked finds a daemon that knows its library.
        tell(&held);
        Took::Served
    }

    /// A checkout of another grimoire named `name`, a store or a server this daemon does not serve
    /// it from, refused with the remedy library::find logs for it; nothing when `name` is not served.
    fn name_taken(&self, name: &str, repo: &Path) -> Took {
        let served = self.held.lock().expect("library lock").served.iter().find(|((n, _), _)| n == name).map(|(_, s)| s.repo.display().to_string());
        match served {
            Some(served) => Took::Failed(crate::library::name_taken(name, &served, &repo.display().to_string())),
            None => Took::NotOurs,
        }
    }

    /// The checkout of grimoire `name` that answers a call naming `checkout`, as its key, or why
    /// none does (req:machine-checkouts). `lenient`: a call answered without one, by the default's
    /// checkout when it is one of the grimoire's, else the first.
    fn choose(&self, name: &str, checkout: Option<&str>, lenient: bool) -> Result<Key, Refusal> {
        let checkout = checkout.map(str::trim).filter(|c| !c.is_empty());
        // The checkout a directory is in: its nearest project file, found from the directory as the
        // system resolves it, so a link into a checkout is that checkout's; looked up before the
        // library is held, so a slow path holds up no other call.
        let nearest = checkout
            .and_then(|dir| {
                #[cfg(test)]
                tests::while_looking_up();
                store_io::find_named(&canonical(Path::new(dir))).ok()
            })
            .map(|(root, home, named)| (canonical(&root), home, named));
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

/// Why no checkout answers a call.
enum Refusal {
    NotFound(String),
    Invalid(ToolError),
}

/// A call refused for its checkout: an invalid error naming the `checkout` field, as a tool's own is.
fn refused(message: String) -> Refusal {
    Refusal::Invalid(ToolError::invalid(message.clone(), vec![FieldError { field: "checkout".into(), reason: message }]))
}

/// A grimoire of the library as its routes' answerer: each call is answered by the checkout the
/// `checkout` its handler parsed names, and a checkout not served yet is served first. Its routes
/// run behind the grimoire's trust step, so only an admitted caller gets one served.
struct InLibrary {
    live: Arc<Live>,
    name: String,
}

impl http::Answerer for InLibrary {
    fn daemon(&self, checkout: Option<&str>, lenient: bool) -> Result<Arc<Daemon>, ToolError> {
        let chosen = match self.live.choose(&self.name, checkout, lenient) {
            // A directory in a checkout not served yet: served, then chosen (req:machine-checkouts).
            Err(Refusal::Invalid(e)) => match checkout.map(|dir| self.live.take_checkout(&self.name, dir)) {
                Some(Took::Served) => self.live.choose(&self.name, checkout, lenient),
                Some(Took::Failed(why)) => Err(refused(why)),
                _ => Err(Refusal::Invalid(e)),
            },
            other => other,
        };
        let key = match chosen {
            Ok(key) => key,
            Err(Refusal::Invalid(e)) => return Err(e),
            Err(Refusal::NotFound(message)) => return Err(ToolError::NotFound { message }),
        };
        self.live
            .daemon_at(&key)
            .ok_or_else(|| ToolError::unavailable(format!("the checkout {} of the grimoire `{}` was let go while the call was answered; send it again", key.1.display(), self.name)))
    }
}

/// Hand `req` to the routes of grimoire `name`, at `path`, its query kept and its body passed on
/// unread: the grimoire's trust step admits the caller on the headers, and the handler that parses
/// the call chooses the checkout from what it parsed (req:daemon-trust-model). The request is made
/// afresh from its method, headers and body, since the library's own route left its path
/// parameters on it, which the grimoire's routes would read as theirs.
async fn forward(live: &Arc<Live>, name: &str, path: &str, req: Request<Body>) -> Response {
    let Some(policy) = live.policy_of(name) else {
        return (StatusCode::NOT_FOUND, Json(serde_json::json!({ "error": "not_found", "message": format!("this daemon's library holds no grimoire `{name}`") }))).into_response();
    };
    let routes = http::api_over(Arc::new(InLibrary { live: live.clone(), name: name.to_string() }), policy);
    let target = match req.uri().query() {
        Some(q) => format!("{path}?{q}"),
        None => path.to_string(),
    };
    let (parts, body) = req.into_parts();
    let mut fresh = Request::new(body);
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

/// What became of a checkout a call named that was not served.
enum Took {
    /// Served now, or already.
    Served,
    /// No checkout of this grimoire this daemon could serve: the call is refused as naming none.
    NotOurs,
    /// A checkout of this grimoire that could not be served, and why.
    Failed(String),
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::daemon::Daemon;
    use crate::http;
    use crate::library::live::tests::project;
    use crate::library::Grimoire;

    thread_local! {
        /// What a test does at the moment a call's project file is being looked up.
        static WHILE_LOOKING_UP: std::cell::RefCell<Option<Box<dyn Fn()>>> = const { std::cell::RefCell::new(None) };
    }

    pub(super) fn while_looking_up() {
        WHILE_LOOKING_UP.with(|hook| {
            if let Some(hook) = &*hook.borrow() {
                hook();
            }
        });
    }

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
    /// {"test": "a-call-is-answered-by-the-checkout-it-names", "checks": ["crates/server/src/library/live/routing#nearest-project-file", "crates/server/src/library/live/routing#nested-checkout-is-its-own", "crates/server/src/library/live/routing#resolved-as-the-system-does", "crates/server/src/library/live/routing#missing-directory-is-its-nearest-existing", "crates/server/src/library/live/routing#one-checkout-needs-no-naming", "crates/server/src/library/live/routing#default-answers-when-none-named", "crates/server/src/library/live/routing#none-named-for-several-refused", "crates/server/src/library/live/routing#directory-no-checkout-holds-refused", "crates/server/src/library/live/routing#refusal-lists-checkouts-and-says-to-name-yours", "crates/server/src/library/live/routing#refusal-explains-a-kept-tool-list", "crates/server/src/library/live/routing#registered-no-such-grimoire", "crates/server/src/library/live/routing#registered-name-taken"]}
    /// ```
    #[tokio::test]
    async fn a_call_is_answered_by_the_checkout_it_names() {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path();
        let (live, main, worktree, _locks) = shop_and_worktree(root, Arc::new(access::Local));
        let repo = |g: &Grimoire| store_io::repository_string(&g.repo).unwrap();
        let named = |dir: &Path| dir.to_string_lossy().to_string();
        // Each checkout answers for itself; a subfolder is its checkout's, the deepest that holds it.
        let at = |key: Result<Key, Refusal>| http::api(live.daemon_at(&key.ok().unwrap()).unwrap(), Arc::new(access::Local));
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
        // A store of its own serves one checkout: another is told so, and that several make a team.
        let ledger = project(root, "ledger");
        let _ledger_lock = store_io::lock(&ledger.store_dir).unwrap();
        let solo = Arc::new(Daemon::open(&ledger.store_dir).unwrap().0);
        {
            let mut held = live.held.lock().unwrap();
            let shelved = Shelved { store_dir: ledger.store_dir.clone(), repo: ledger.repo.clone(), canon: canonical(&ledger.repo), server: None, daemon: solo.clone(), lock: None, watcher: None };
            held.served.insert(("ledger".into(), ledger.repo.clone()), shelved);
        }
        assert!(live.choose("ledger", None, false).is_ok(), "one checkout needs no naming");
        // A directory in another project's checkout is no checkout of this store: refused, and nothing taken.
        let message = why(live.choose("ledger", Some(&named(&main.repo)), false));
        assert!(message.contains("is no checkout of the grimoire `ledger`"), "{message}");
        assert!(matches!(live.take_checkout("ledger", &named(&main.repo)), Took::NotOurs), "its project file names another store");
        // Another project that names its own store `ledger` too is refused, told to give it another name.
        let other = project(root, "other");
        std::fs::write(
            other.repo.join(store_io::PROJECT_FILE),
            format!(
                "name = \"ledger\"
store = {}
",
                toml::Value::String(other.store_dir.to_string_lossy().to_string())
            ),
        )
        .unwrap();
        match live.take_checkout("ledger", &named(&other.repo)) {
            Took::Failed(why) => assert!(why.contains("`ledger` is served already") && why.contains("give it another `name`") && why.contains("/g/ledger"), "{why}"),
            _ => panic!("refused, saying why"),
        }
        // So is one whose project file names a copy of another server's grimoire `ledger`.
        std::fs::write(
            other.repo.join(store_io::PROJECT_FILE),
            "name = \"ledger\"
server = \"https://elsewhere.example/g/ledger\"
",
        )
        .unwrap();
        match live.take_checkout("ledger", &named(&other.repo)) {
            Took::Failed(why) => assert!(why.contains("`ledger` is served already") && why.contains("give it another `name`"), "{why}"),
            _ => panic!("refused, saying why"),
        }
        assert!(matches!(live.choose("audit", None, true), Err(Refusal::NotFound(_))));
    }

    /// `shop`, the daemon's own, and a worktree of it kept inside it, served as two checkouts of one
    /// team's grimoire, with the locks that keep them.
    fn shop_and_worktree(root: &Path, policy: Arc<dyn access::Policy>) -> (Arc<Live>, Grimoire, Grimoire, (store_io::StoreLock, store_io::StoreLock)) {
        let main = project(root, "shop");
        let worktree = project(&main.repo, "wt");
        // A worktree of `shop`: its project file names the grimoire `shop`, at the store it is served from.
        std::fs::write(worktree.repo.join(store_io::PROJECT_FILE), format!("name = \"shop\"\nstore = {}\n", toml::Value::String(worktree.store_dir.to_string_lossy().to_string()))).unwrap();
        let locks = (store_io::lock(&main.store_dir).unwrap(), store_io::lock(&worktree.store_dir).unwrap());
        let daemon = Arc::new(Daemon::open(&main.store_dir).unwrap().0);
        let live = Live::new(Grimoire { server: Some("http://team".into()), ..main.clone() }, daemon, 4299, policy, root.join("none"), root.join("none"));
        let other = Arc::new(Daemon::open(&worktree.store_dir).unwrap().0);
        {
            let mut held = live.held.lock().unwrap();
            let shelved = Shelved {
                store_dir: worktree.store_dir.clone(),
                repo: worktree.repo.clone(),
                canon: canonical(&worktree.repo),
                server: Some("http://team".into()),
                daemon: other.clone(),
                lock: None,
                watcher: None,
            };
            held.served.insert(("shop".into(), worktree.repo.clone()), shelved);
        }
        (live, main, worktree, locks)
    }

    /// What `req` is answered with: its status, whether the connection is closed, and its JSON.
    async fn answer(routes: &Router, req: Request<Body>) -> (StatusCode, bool, serde_json::Value) {
        let res = routes.clone().oneshot(req).await.unwrap();
        let (status, closed) = (res.status(), res.headers().get("connection").is_some_and(|v| v == "close"));
        let bytes = axum::body::to_bytes(res.into_body(), 1 << 20).await.unwrap();
        (status, closed, serde_json::from_slice(&bytes).unwrap_or_default())
    }

    /// What happened to a request, in order: each time the trust step asked the policy, and the first
    /// time anything read the body.
    type Log = Arc<std::sync::Mutex<Vec<&'static str>>>;

    /// The local policy, writing each time it is asked into the log.
    struct Logged(Log);

    impl access::Policy for Logged {
        fn admit(&self, headers: &axum::http::HeaderMap) -> Result<access::Caller, String> {
            self.0.lock().unwrap().push("policy");
            access::Local.admit(headers)
        }
    }

    /// A body that writes into the log when it is first read.
    fn logged(log: &Log, bytes: impl Into<Vec<u8>>) -> Body {
        let (log, bytes) = (log.clone(), bytes.into());
        Body::from_stream(futures::stream::once(async move {
            log.lock().unwrap().push("body");
            Ok::<_, std::io::Error>(bytes)
        }))
    }

    fn post(uri: &str, host: &str, body: impl Into<Body>) -> Request<Body> {
        Request::builder().method("POST").uri(uri).header("host", host).header("content-type", "application/json").header("accept", "application/json, text/event-stream").body(body.into()).unwrap()
    }

    /// # Test contract
    ///
    /// ```json
    /// {"test": "a-library-hands-a-call-on-unread", "checks": ["crates/server/src/library/live/routing#handed-to-the-grimoires-routes", "crates/server/src/library/live/routing#path-the-routes-expect", "crates/server/src/library/live/routing#body-passed-on-unread", "crates/server/src/library/live/routing#refused-caller-costs-a-discard", "crates/server/src/library/live/routing#handler-chooses-the-checkout", "crates/server/src/library/live/routing#unheld-name-not-found", "crates/server/src/library/live/routing#nearest-project-file", "crates/server/src/http#parsed-once-after-the-trust-step", "crates/server/src/http#checkout-from-the-post-input", "crates/server/src/http#checkout-from-the-get-query", "crates/server/src/http#refusal-answered-as-a-tools-own-error", "crates/server/src/mcp#checkout-from-arguments"]}
    /// ```
    #[tokio::test]
    async fn a_library_hands_a_call_on_unread_and_its_handler_chooses_the_checkout() {
        let tmp = tempfile::tempdir().unwrap();
        let log: Log = Arc::default();
        let (live, _main, worktree, _locks) = shop_and_worktree(tmp.path(), Arc::new(Logged(log.clone())));
        let app = routes(live.clone()).merge(bare(live.clone(), "shop".into()));
        // What a request did, taken from the log and cleared for the next.
        let did = || std::mem::take(&mut *log.lock().unwrap());
        let wt = worktree.repo.to_string_lossy().to_string();
        let repo = store_io::repository_string(&worktree.repo).unwrap();
        // The JSON API: the checkout from the input its handler parsed, at /g/<name>/ and on the bare routes.
        // The trust step asks the policy once, on the headers, before anything reads the body, which is
        // then read once, by the handler that parses it.
        for uri in ["/g/shop/api/status", "/api/status"] {
            let (status, _, v) = answer(&app, post(uri, "127.0.0.1", logged(&log, serde_json::json!({ "checkout": wt }).to_string()))).await;
            assert_eq!((status, v["repository"].as_str()), (StatusCode::OK, Some(repo.as_str())), "{uri}: {v}");
            assert_eq!(did(), ["policy", "body"], "{uri}");
        }
        // A GET names it in its query.
        let get =
            Request::builder().uri(format!("/g/shop/api/status?checkout={}", wt.replace('\\', "%5C").replace(':', "%3A").replace(' ', "%20"))).header("host", "127.0.0.1").body(Body::empty()).unwrap();
        let (status, _, v) = answer(&app, get).await;
        assert_eq!((status, v["repository"].as_str()), (StatusCode::OK, Some(repo.as_str())), "{v}");
        assert_eq!(did(), ["policy"]);
        // MCP: the checkout from the arguments rmcp parsed.
        let call = serde_json::json!({ "jsonrpc": "2.0", "id": 1, "method": "tools/call", "params": { "name": "status", "arguments": { "checkout": wt } } });
        let (status, _, v) = answer(&app, post("/g/shop/mcp", "127.0.0.1", logged(&log, call.to_string()))).await;
        assert_eq!((status, v["result"]["structuredContent"]["repository"].as_str()), (StatusCode::OK, Some(repo.as_str())), "{v}");
        assert_eq!(did(), ["policy", "body"], "MCP: the body read once, by rmcp, after the step");
        // A checkout refused, once the caller is admitted, is a tool's error over each.
        let (status, _, v) = answer(&app, post("/g/shop/api/lint", "127.0.0.1", "{}")).await;
        assert_eq!((status, v["error"].as_str(), v["details"][0]["field"].as_str()), (StatusCode::BAD_REQUEST, Some("invalid"), Some("checkout")), "{v}");
        let call = serde_json::json!({ "jsonrpc": "2.0", "id": 2, "method": "tools/call", "params": { "name": "lint", "arguments": {} } });
        let (_, _, v) = answer(&app, post("/g/shop/mcp", "127.0.0.1", call.to_string())).await;
        assert_eq!((v["result"]["isError"].as_bool(), v["result"]["structuredContent"]["details"][0]["field"].as_str()), (Some(true), Some("checkout")), "{v}");
        // A caller the trust step refuses is answered 403 on its headers: a body past what the library once
        // read (64 MiB) is not read to find a checkout, nor met with a 413, and the connection is closed.
        did();
        let huge = vec![b' '; 65 * 1024 * 1024];
        let (status, closed, v) = answer(&app, post("/g/shop/api/lint", "evil.example", logged(&log, huge))).await;
        assert_eq!((status, closed, v["error"].as_str()), (StatusCode::FORBIDDEN, true, Some("invalid")), "{v}");
        assert_eq!(did(), ["policy", "body"], "refused on the headers, the body read only by the step's discard");
        // A refused GET that carries a body has it discarded too, so its 403 is not lost to a reset.
        let get = Request::builder().uri("/g/shop/api/status").header("host", "evil.example").body(logged(&log, "{}")).unwrap();
        assert_eq!(answer(&app, get).await.0, StatusCode::FORBIDDEN);
        assert_eq!(did(), ["policy", "body"]);
        // A name the library does not hold.
        assert_eq!(answer(&app, post("/g/audit/api/status", "127.0.0.1", "{}")).await.0, StatusCode::NOT_FOUND);
    }

    /// # Test contract
    ///
    /// ```json
    /// {"test": "a-checkout-is-taken-once-and-never-while-its-store-is-held-back", "checks": ["crates/server/src/library/live/routing#registered-once"]}
    /// ```
    #[test]
    fn a_checkout_is_taken_once_and_never_while_its_store_is_held_back() {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path();
        let (shop, ledger) = (project(root, "shop"), project(root, "ledger"));
        let _shop_lock = store_io::lock(&shop.store_dir).unwrap();
        let daemon = Arc::new(Daemon::open(&shop.store_dir).unwrap().0);
        let live = Live::new(shop.clone(), daemon, 4299, Arc::new(access::Local), root.join("stores"), root.join("copies"));
        live.refresh();
        assert_eq!(live.names(), ["ledger", "shop"]);
        // A second checkout of the store, its project file naming the grimoire at that store, not served yet.
        let checkout = |name: &str| {
            let repo = root.join(name);
            std::fs::create_dir_all(repo.join("src")).unwrap();
            std::fs::write(
                repo.join("src").join("lib.rs"),
                "pub fn a() {}
",
            )
            .unwrap();
            std::fs::write(
                repo.join(store_io::PROJECT_FILE),
                format!(
                    "name = \"ledger\"
store = {}
",
                    toml::Value::String(ledger.store_dir.to_string_lossy().to_string())
                ),
            )
            .unwrap();
            repo
        };
        let second = checkout("ledger-two");
        let served = |repo: &Path| live.held.lock().unwrap().served.keys().filter(|(n, r)| n == "ledger" && same_checkout(r, repo)).count();
        // Requests for it arrive together, with a refresh: it is opened once, and each is answered by it.
        let dir = second.to_string_lossy().to_string();
        let took: Vec<bool> = std::thread::scope(|scope| {
            let handles: Vec<_> = (0..4)
                .map(|_| scope.spawn(|| matches!(live.take_checkout("ledger", &dir), Took::Served)))
                .chain(std::iter::once(scope.spawn(|| {
                    live.refresh();
                    true
                })))
                .collect();
            handles.into_iter().map(|h| h.join().unwrap()).collect()
        });
        assert!(took.iter().all(|t| *t), "{took:?}");
        assert_eq!(served(&second), 1, "opened once");
        // Its store held back, as while a command deletes it: no checkout of it is taken.
        let third = checkout("ledger-three");
        assert!(live.release(&ledger.store_dir));
        assert!(matches!(live.take_checkout("ledger", &third.to_string_lossy()), Took::NotOurs));
        assert_eq!(served(&third), 0, "not taken while its store is held back");
        assert_eq!(served(&second), 0, "and the others of it let go");
    }

    /// A second checkout of `ledger`, its project file naming the grimoire at its store, not served yet.
    fn second_checkout(root: &Path, name: &str, ledger: &Grimoire) -> PathBuf {
        let repo = root.join(name);
        std::fs::create_dir_all(repo.join("src")).unwrap();
        std::fs::write(
            repo.join("src").join("lib.rs"),
            "pub fn a() {}
",
        )
        .unwrap();
        std::fs::write(
            repo.join(store_io::PROJECT_FILE),
            format!(
                "name = \"ledger\"
store = {}
",
                toml::Value::String(ledger.store_dir.to_string_lossy().to_string())
            ),
        )
        .unwrap();
        repo
    }

    /// # Test contract
    ///
    /// ```json
    /// {"test": "a-checkout-of-a-queued-store-is-not-taken", "checks": ["crates/server/src/library/live#queued-checkout-not-taken"]}
    /// ```
    #[test]
    fn a_checkout_of_a_queued_store_is_not_taken() {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path();
        let (shop, ledger) = (project(root, "shop"), project(root, "ledger"));
        let _shop_lock = store_io::lock(&shop.store_dir).unwrap();
        let daemon = Arc::new(Daemon::open(&shop.store_dir).unwrap().0);
        let live = Live::new(shop.clone(), daemon, 4299, Arc::new(access::Local), root.join("stores"), root.join("copies"));
        live.refresh();
        let second = second_checkout(root, "ledger-two", &ledger);
        let checkouts_before = std::fs::read_to_string(ledger.store_dir.join("checkouts.json")).ok();
        // Its store queued for deletion: the checkout is not taken, nor added to the store.
        let queue = root.join("deletions");
        store_io::deletions::queue(&queue, &ledger.store_dir, &store_io::repository_string(&ledger.repo).unwrap()).unwrap();
        assert!(matches!(live.take_checkout("ledger", &second.to_string_lossy()), Took::NotOurs));
        let served = |repo: &Path| live.held.lock().unwrap().served.keys().filter(|(n, r)| n == "ledger" && same_checkout(r, repo)).count();
        assert_eq!(served(&second), 0, "not served");
        assert_eq!(std::fs::read_to_string(ledger.store_dir.join("checkouts.json")).ok(), checkouts_before, "and not added to the store's checkouts");
        // The deletion cancelled, the same checkout is taken.
        assert!(store_io::deletions::cancel(&queue, &ledger.store_dir).unwrap());
        assert!(matches!(live.take_checkout("ledger", &second.to_string_lossy()), Took::Served));
        assert_eq!(served(&second), 1);
        live.stop();
    }

    /// # Test contract
    ///
    /// ```json
    /// {"test": "a-checkout-served-for-a-request-is-told-the-library-first", "checks": ["crates/server/src/library/live#newly-served-told-first"]}
    /// ```
    #[test]
    fn a_checkout_served_for_a_request_is_told_the_library_first() {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path();
        let (shop, ledger) = (project(root, "shop"), project(root, "ledger"));
        let _shop_lock = store_io::lock(&shop.store_dir).unwrap();
        let daemon = Arc::new(Daemon::open(&shop.store_dir).unwrap().0);
        let live = Live::new(shop.clone(), daemon, 4299, Arc::new(access::Local), root.join("stores"), root.join("copies"));
        live.refresh();
        let second = second_checkout(root, "ledger-two", &ledger);
        // A request reaches the checkout's daemon the moment the routes find it: it is asked its status then.
        let seen = Arc::new(std::sync::Mutex::new(None));
        let (watching, found, target) = (live.clone(), seen.clone(), second.clone());
        let poller = std::thread::spawn(move || {
            let deadline = std::time::Instant::now() + std::time::Duration::from_secs(30);
            while std::time::Instant::now() < deadline {
                let daemon = watching.held.lock().unwrap().served.iter().find(|((n, r), _)| n == "ledger" && same_checkout(r, &target)).map(|(_, s)| s.daemon.clone());
                if let Some(daemon) = daemon {
                    let status = daemon.call("status", serde_json::json!({})).unwrap();
                    let names: Vec<String> = status["library"].as_array().map(|l| l.iter().map(|e| e["name"].as_str().unwrap().to_string()).collect()).unwrap_or_default();
                    *found.lock().unwrap() = Some(names);
                    return;
                }
            }
        });
        assert!(matches!(live.take_checkout("ledger", &second.to_string_lossy()), Took::Served));
        poller.join().unwrap();
        assert_eq!(*seen.lock().unwrap(), Some(vec!["ledger".to_string(), "ledger".to_string(), "shop".to_string()]), "the first request it answered named the whole library");
        live.stop();
    }

    /// `shop`, the daemon's own, and the two checkouts of `ledger` the library finds and is asked to serve.
    fn shop_and_two_ledgers(root: &Path) -> (Arc<Live>, Grimoire, PathBuf, store_io::StoreLock) {
        let (shop, ledger) = (project(root, "shop"), project(root, "ledger"));
        let shop_lock = store_io::lock(&shop.store_dir).unwrap();
        let daemon = Arc::new(Daemon::open(&shop.store_dir).unwrap().0);
        let live = Live::new(shop, daemon, 4299, Arc::new(access::Local), root.join("stores"), root.join("copies"));
        live.refresh();
        let second = second_checkout(root, "ledger-two", &ledger);
        assert!(matches!(live.take_checkout("ledger", &second.to_string_lossy()), Took::Served));
        (live, ledger, second, shop_lock)
    }

    /// # Test contract
    ///
    /// ```json
    /// {"test": "no-checkout-named-first-answers-when-default-not-ours", "checks": ["crates/server/src/library/live/routing#first-answers-when-default-is-not-ours"]}
    /// ```
    #[tokio::test]
    async fn no_checkout_named_first_answers_when_default_not_ours() {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path();
        let (live, ledger, second, _shop_lock) = shop_and_two_ledgers(root);
        let first = store_io::repository_string(&ledger.repo).unwrap();
        let other = store_io::repository_string(&second).unwrap();
        assert_ne!(first, other);
        // The daemon was started for `shop`, so no checkout of `ledger` is its default: the first of them answers.
        let keyed = live.choose("ledger", None, true).ok().unwrap();
        assert_eq!(keyed.1, ledger.repo);
        // The routes that answer without a checkout name one this way: the status, and the stream, export and prune.
        let routes = routes(live.clone());
        let (status, _, v) = answer(&routes, post("/g/ledger/api/status", "127.0.0.1", "{}")).await;
        assert_eq!((status, v["repository"].as_str()), (StatusCode::OK, Some(first.as_str())), "{v}");
        let answerer = InLibrary { live: live.clone(), name: "ledger".into() };
        assert_eq!(http::Answerer::daemon(&answerer, None, true).unwrap().repo, ledger.repo, "as the stream, the export and the prune ask");
        // A call that is not answered without one is still refused for naming none, the two checkouts being served.
        assert!(matches!(live.choose("ledger", None, false), Err(Refusal::Invalid(_))));
        let (status, _, v) = answer(&routes, post("/g/ledger/api/lint", "127.0.0.1", "{}")).await;
        assert_eq!((status, v["details"][0]["field"].as_str()), (StatusCode::BAD_REQUEST, Some("checkout")), "{v}");
        live.stop();
    }

    /// # Test contract
    ///
    /// ```json
    /// {"test": "the-project-file-is-looked-up-with-the-library-free", "checks": ["crates/server/src/library/live/routing#project-file-looked-up-before-library-held"]}
    /// ```
    #[test]
    fn the_project_file_is_looked_up_with_the_library_free() {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path();
        let (live, ledger, second, _shop_lock) = shop_and_two_ledgers(root);
        // At the moment the lookup is made, the library's lock is asked for: free means no other call waits on a slow path.
        let free = Arc::new(std::sync::Mutex::new(Vec::new()));
        let (watching, seen) = (live.clone(), free.clone());
        WHILE_LOOKING_UP.with(|hook| *hook.borrow_mut() = Some(Box::new(move || seen.lock().unwrap().push(watching.held.try_lock().is_ok()))));
        let chosen = live.choose("ledger", Some(&second.to_string_lossy()), false);
        WHILE_LOOKING_UP.with(|hook| *hook.borrow_mut() = None);
        assert_eq!(chosen.ok().unwrap().1, second, "and the lookup answered");
        assert_eq!(*free.lock().unwrap(), [true], "looked up once, with the library not held");
        // A call naming no checkout looks nothing up.
        WHILE_LOOKING_UP.with(|hook| *hook.borrow_mut() = Some(Box::new(|| panic!("a call naming no checkout looked a project file up"))));
        assert_eq!(live.choose("ledger", None, true).ok().unwrap().1, ledger.repo);
        WHILE_LOOKING_UP.with(|hook| *hook.borrow_mut() = None);
        live.stop();
    }
}
