//! # Module contract
//!
//! ```json
//! {"purpose":"Run the daemon that owns the store, keeps the graph current, and serves the web UI, the JSON API and MCP from one port.","criteria":[{"id":"no-repository-writes","statement":"The daemon never writes an index, cache or marker into the target repository."},{"id":"holds-the-lock","statement":"serve takes the store's lock, or on a project that names a server the lock of the checkout's copy, before it opens the store and holds it until it has stopped serving, so a second daemon on the store is refused before it writes anything."},{"id":"shared-serve","statement":"serve with shared opens the store with no repository, starts no watcher, serves no interface, puts its router behind the token policy and listens on the address given or else the loopback port, under the store's lock like any daemon; an address to listen on without shared is refused before anything is opened."},{"id":"local-policy-installed","statement":"serve puts its router behind the local policy unless it is shared, on a project that names a server as on any other, so the daemon a person runs admits callers on this machine to read and write and refuses a request another site makes."},{"id":"serves-a-server-project","statement":"serve given a server opens the checkout's copy of that server's intent with the checkout's code, watches the repository, serves the interface, the API and MCP on the loopback port as for any project, and follows the server; given with shared it is refused before anything is opened."},{"id":"site-listener","statement":"serve, shared or not, given a site address binds it beside the daemon's own and serves the site's router alone there for as long as the daemon serves, refusing to start when the address cannot be bound; given none, it opens no further address."},{"id":"serves-its-own-grimoire","statement":"serve given a library name serves its own grimoire on the bare routes, as the checkout it was started in sees it."},{"id":"adds-its-checkout","statement":"serve given a library name adds the checkout it was started in to a store of its own's checkouts if it is not its main one or one already."},{"id":"marked-at-its-path","statement":"serve given a library name marks its own grimoire as served at /g/<name>."},{"id":"bare-routes-by-named-checkout","statement":"serve given a library name answers the bare routes through library::live as /g/<name>/ is, by the checkout a call names."},{"id":"serves-the-machines-library","statement":"serve given a library name keeps the machine's library served beside its own grimoire through library::live."},{"id":"library-found-before-first-request","statement":"serve given a library name finds the machine's library once before the daemon answers a request."},{"id":"library-found-again-every-few-seconds","statement":"serve given a library name finds the machine's library again every few seconds after."},{"id":"markers-removed-on-stop","statement":"serve removes every marker it wrote when it stops."},{"id":"serves-its-own-store-alone","statement":"serve given no library name serves its own store alone."},{"id":"serves-every-store-in-the-library","statement":"serve with shared and a library directory serves every store under it through library::live, each grimoire at /g/<name> where it listens, behind its own tokens."},{"id":"shared-page-for-everything-else","statement":"serve with shared and a library directory serves the shared page for everything else."},{"id":"no-grimoire-on-the-bare-routes","statement":"serve with shared and a library directory serves no grimoire on the bare routes."},{"id":"library-without-shared-refused","statement":"serve refuses a library directory without shared."},{"id":"site-of-a-named-grimoire","statement":"Given a site address and the name of a grimoire, serve serves that grimoire's site there through site::router_in."},{"id":"site-without-grimoire-refused","statement":"serve refuses a site that names no grimoire."},{"id":"grimoire-without-site-refused","statement":"serve refuses a grimoire named with no site."},{"id":"graceful-stop","statement":"serve stops on Ctrl+C and, where the system has one, on SIGTERM, as docker stop sends: it finishes the requests it is answering and removes every marker it wrote before it exits."},{"id":"queued-not-served","statement":"serve, once it has marked the store it serves, a store of its own, asks the deletions queue under its lock and refuses a store whose deletion is queued, its marker taken away; uninstall asks for the marker under the same lock before it queues, so a store is never both queued and served however the two start."}]}
//! ```

pub mod access;
pub mod base;
pub mod client;
pub mod daemon;
pub mod http;
pub mod library;
pub mod mcp;
pub mod remote;
pub mod site;
pub mod store_io;
pub mod watch;

use std::net::SocketAddr;
use std::path::PathBuf;
use std::sync::Arc;

pub use daemon::{load, load_for, load_store, Daemon, Loaded};

pub struct ServeOptions {
    pub store_dir: PathBuf,
    pub port: u16,
    pub ui_dir: Option<PathBuf>,
    /// Serve the store for a team: no repository, no interface, callers admitted by token.
    pub shared: bool,
    /// Where a shared daemon listens; the loopback port when not given.
    pub listen: Option<SocketAddr>,
    /// On a project that names a server: the server, and the checkout to join
    /// its intent with. `store_dir` is then the checkout's copy.
    pub server: Option<(remote::Server, PathBuf)>,
    /// Where the project's site is served, on an address of its own; no site when not given.
    pub site: Option<SocketAddr>,
    /// Serve the machine's library, this grimoire on the bare routes and as `/g/<name>`
    /// beside every other grimoire a checkout here names; none to serve this store alone.
    pub library: Option<String>,
    /// With `shared`, serve every store under this directory as the team's library, in place of `store_dir` alone.
    pub shared_library: Option<PathBuf>,
    /// With a team's library, the grimoire whose site `site` serves.
    pub site_of: Option<String>,
    /// The checkout serve was started in, for a store of its own several checkouts may share:
    /// served as that checkout sees the store, added to its checkouts first if need be.
    pub checkout: Option<PathBuf>,
}

/// Serve a site's router on its own address beside the daemon, until the process stops.
async fn serve_site(app: axum::Router, addr: SocketAddr) -> anyhow::Result<SocketAddr> {
    let listener = tokio::net::TcpListener::bind(addr).await?;
    let addr = listener.local_addr()?;
    tracing::info!("site on http://{addr}: how the project is meant to be used, with no sign-in; who may reach it is the deployment's to decide");
    tokio::spawn(async move {
        if let Err(e) = axum::serve(listener, app).with_graceful_shutdown(shutdown()).await {
            tracing::warn!("the site stopped: {e}");
        }
    });
    Ok(addr)
}

/// Open the store, start watching the repository and serve until the
/// process is interrupted.
pub async fn serve(mut opts: ServeOptions) -> anyhow::Result<()> {
    // The store's one writer from before it is opened, since opening prunes
    // the baseline, until serving has stopped; released when this returns.
    // A daemon that trusts every caller on this machine is never reachable from another.
    if opts.listen.is_some() && !opts.shared {
        anyhow::bail!("--listen needs --shared: a daemon that admits every local caller listens only on this machine");
    }
    if opts.shared && opts.server.is_some() {
        anyhow::bail!("--shared serves a store, and this project names a server: name the store to serve with --store");
    }
    if let Some(dir) = opts.shared_library.take() {
        if !opts.shared {
            anyhow::bail!("--library serves a team's stores, and needs --shared");
        }
        return serve_shared_library(dir, opts).await;
    }
    let server = opts.server.take();
    // A checkout's copy of a server's intent may not exist until the first sync makes it.
    let _lock = match &server {
        Some(_) => remote::lock_copy(&opts.store_dir)?,
        None => store_io::lock(&opts.store_dir)?,
    };
    if opts.shared {
        return serve_shared(opts).await;
    }
    let follows = server.is_some();
    // The default's server, so the library knows another checkout of the same team's grimoire for one.
    let default_server = server.as_ref().map(|(s, _)| s.address.clone());
    let (daemon, extract_errors) = match server {
        Some((server, repo)) => {
            tracing::info!("this project's grimoire is the server's at {}; this checkout's copy is {}", server.address, opts.store_dir.display());
            // Opening asks the server, which blocks: kept off the runtime's own threads.
            let copy = opts.store_dir.clone();
            tokio::task::spawn_blocking(move || Daemon::open_remote(&copy, &repo, server)).await??
        }
        None => {
            // Started in another checkout of a store of its own than its main one: that checkout's view (req:store-checkouts).
            let view = match &opts.checkout {
                Some(dir) => {
                    let repository = store_io::repository_string(dir)?;
                    if repository != store_io::checkouts::main_of(&opts.store_dir)? {
                        // This daemon holds the store's lock, so it adds the checkout itself.
                        store_io::checkouts::register(&opts.store_dir, dir)?;
                        Some(repository)
                    } else {
                        None
                    }
                }
                None => None,
            };
            Daemon::open_for(&opts.store_dir, view.as_deref())?
        }
    };
    for e in &extract_errors {
        tracing::warn!("{e}");
    }
    let daemon = Arc::new(daemon);
    let status = daemon.call("status", serde_json::json!({})).unwrap_or_default();
    tracing::info!("store {} for repository {}", opts.store_dir.display(), daemon.repo.display());
    tracing::info!("graph: {}", serde_json::to_string(&status["nodes"]).unwrap_or_default());

    let _watcher = watch::start(daemon.clone())?;
    if let Some(at) = opts.site {
        serve_site(site::router(daemon.clone()), at).await?;
    }
    let ui = opts.ui_dir.clone().map_or(http::Ui::Embedded, http::Ui::Dir);
    // Bound first, so every grimoire is marked with the port it is really served on before it gets a route.
    let addr = SocketAddr::from(([127, 0, 0, 1], opts.port));
    let listener = tokio::net::TcpListener::bind(addr).await?;
    // The port the listener got, which is not the one asked for when it was left to the system.
    let addr = listener.local_addr()?;
    // Every marker this daemon writes is taken away when it stops, on any path out.
    let marked = Unmark(vec![daemon.clone()]);
    match &opts.library {
        Some(name) => daemon.mark_serving_in(addr, &format!("/g/{name}"))?,
        None => daemon.mark_serving(addr.port())?,
    }
    // Marked first, then the queue asked under its lock, which uninstall holds while it asks for the marker and queues:
    // so a store queued for deletion is never served, however the two start (req:store-deletion-queued).
    if !follows {
        if let Ok(dir) = store_io::deletions::deletions_dir() {
            let queue = store_io::deletions::hold(&dir)?;
            if store_io::deletions::is_queued(&dir, &opts.store_dir) {
                drop(queue);
                anyhow::bail!("store {} is queued for deletion, so it is not served; run init with it again to keep it", opts.store_dir.display());
            }
        }
    }
    // The local policy: callers on this machine may read and write.
    let policy: Arc<dyn access::Policy> = Arc::new(access::Local);
    let mut api = http::api(daemon.clone(), policy.clone());
    // The machine's library, kept current while the daemon runs (req:machine-library).
    let live = match &opts.library {
        Some(name) => {
            let default = library::Grimoire { name: name.clone(), store_dir: opts.store_dir.clone(), repo: daemon.repo.clone(), server: default_server.clone() };
            match (store_io::stores_dir(), store_io::copies_dir()) {
                (Ok(stores), Ok(copies)) => {
                    let live = library::live::Live::new(default, daemon.clone(), addr.port(), policy.clone(), stores, copies);
                    let first = live.clone();
                    tokio::task::spawn_blocking(move || first.refresh()).await?;
                    // The bare routes are this checkout's grimoire's, chosen by checkout as /g/<name>/ is.
                    api = library::live::bare(live.clone(), name.clone()).merge(library::live::routes(live.clone()));
                    live.keep_current();
                    Some(live)
                }
                _ => {
                    tracing::warn!("library: no data directory on this machine, so only this checkout's grimoire is served");
                    None
                }
            }
        }
        None => None,
    };
    let _unmark_library = live.clone().map(UnmarkLibrary);
    let app = http::with_ui(api, ui);
    if follows {
        remote::follow(&daemon);
    }
    tracing::info!("listening on http://{addr}  (api: /api/<tool>, mcp: /mcp, events: /api/events)");
    if let (Some(name), Some(live)) = (&opts.library, &live) {
        tracing::info!("library: {name} on the bare routes; {} at /g/<name>, kept current every {}s", live.names().join(", "), library::live::REFRESH.as_secs());
    }
    axum::serve(listener, app).with_graceful_shutdown(shutdown()).await?;
    drop(marked);
    Ok(())
}

/// The library, stopped when `serve` lets go of it, on any path out: nothing
/// more is taken, and every marker it wrote is removed.
struct UnmarkLibrary(Arc<library::live::Live>);

impl Drop for UnmarkLibrary {
    fn drop(&mut self) {
        self.0.stop();
    }
}

/// The daemons whose stores this process marked as served, unmarked when it lets go of them.
struct Unmark(Vec<Arc<Daemon>>);

impl Drop for Unmark {
    fn drop(&mut self) {
        for d in &self.0 {
            d.unmark_serving();
        }
    }
}

/// Serve a store for a team, under the lock `serve` holds: no repository is
/// read or watched, no interface is served, and a caller is admitted by token.
async fn serve_shared(opts: ServeOptions) -> anyhow::Result<()> {
    let daemon = Arc::new(Daemon::open_shared(&opts.store_dir)?);
    let status = daemon.call("status", serde_json::json!({})).unwrap_or_default();
    tracing::info!("shared store {}: intent only, no code held", opts.store_dir.display());
    tracing::info!("graph: {}", serde_json::to_string(&status["nodes"]).unwrap_or_default());
    if store_io::tokens::list(&opts.store_dir)?.is_empty() {
        tracing::warn!("the store holds no tokens, so nobody can sign in: make one with `grimoire token add <name> --store {}`", opts.store_dir.display());
    }
    if let Some(at) = opts.site {
        serve_site(site::router(daemon.clone()), at).await?;
    }
    let policy = Arc::new(access::Tokens { store_dir: opts.store_dir.clone() });
    let app = http::router(daemon.clone(), http::Ui::Shared, policy);
    let addr = opts.listen.unwrap_or_else(|| SocketAddr::from(([127, 0, 0, 1], opts.port)));
    let listener = tokio::net::TcpListener::bind(addr).await?;
    // The address the listener got, which is not the one asked for when the port was left to the system.
    let addr = listener.local_addr()?;
    daemon.mark_serving_at(addr)?;
    tracing::info!("listening on http://{addr}  (api: /api/<tool>, mcp: /mcp, events: /api/events); put a proxy in front for HTTPS");
    let served = axum::serve(listener, app).with_graceful_shutdown(shutdown()).await;
    daemon.unmark_serving();
    served?;
    Ok(())
}

/// Serve a team's library: every store under `dir`, each at `/g/<name>/` behind
/// its own tokens, kept current while it runs (req:server-library). No grimoire
/// answers on the bare routes, and no interface is served.
async fn serve_shared_library(dir: PathBuf, opts: ServeOptions) -> anyhow::Result<()> {
    // A library holds many grimoires, so a site beside it names whose it is (req:server-library).
    let site = match (opts.site, opts.site_of.clone()) {
        (Some(at), Some(name)) => Some((at, name)),
        (Some(_), None) => anyhow::bail!("--site beside --library serves one grimoire's site: name it with --site-of <name>"),
        (None, Some(_)) => anyhow::bail!("--site-of names whose site --site serves: give --site too"),
        (None, None) => None,
    };
    if !dir.is_dir() {
        anyhow::bail!("the library directory {} does not exist", dir.display());
    }
    let addr = opts.listen.unwrap_or_else(|| SocketAddr::from(([127, 0, 0, 1], opts.port)));
    let listener = tokio::net::TcpListener::bind(addr).await?;
    // The address the listener got, which is not the one asked for when the port was left to the system.
    let addr = listener.local_addr()?;
    let live = library::live::Live::new_shared(dir.clone(), addr);
    let first = live.clone();
    tokio::task::spawn_blocking(move || first.refresh()).await?;
    live.keep_current();
    let _stop = UnmarkLibrary(live.clone());
    if let Some((at, name)) = site {
        if !live.names().contains(&name) {
            tracing::warn!("the library holds no grimoire `{name}` yet: its site answers that it is not served until one is");
        }
        serve_site(site::router_in(live.clone(), name), at).await?;
    }
    // The bare API is no grimoire's here: said plainly, so a client named at the server alone learns where to go.
    let bare = axum::Router::new().route("/api/{*rest}", axum::routing::any(no_grimoire_here)).route("/mcp", axum::routing::any(no_grimoire_here));
    let app = http::with_ui(library::live::routes(live.clone()).merge(bare), http::Ui::Shared);
    let names = live.names();
    tracing::info!(
        "team library {}: {} at /g/<name>, intent only, each behind its own tokens, kept current every {}s",
        dir.display(),
        if names.is_empty() { "no store yet".to_string() } else { names.join(", ") },
        library::live::REFRESH.as_secs()
    );
    tracing::info!("listening on http://{addr}; put a proxy in front for HTTPS");
    axum::serve(listener, app).with_graceful_shutdown(shutdown()).await?;
    Ok(())
}

/// A team's library answers its bare API for no grimoire.
async fn no_grimoire_here() -> axum::response::Response {
    use axum::response::IntoResponse;
    let message = "this server serves a team's library: name a grimoire in its address, http://<host>/g/<name>, as the project file's server";
    (axum::http::StatusCode::NOT_FOUND, axum::Json(serde_json::json!({ "error": "not_found", "message": message }))).into_response()
}

/// Wait for Ctrl+C, or where there is one for SIGTERM, which `docker stop` and a
/// service manager send: in a container the server is process 1, which the
/// system does not stop for a signal it does not handle, so without this it
/// would be killed at the end of the grace period, mid-write, its stores marked.
async fn shutdown() {
    #[cfg(unix)]
    {
        let mut term = match tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate()) {
            Ok(term) => term,
            Err(e) => {
                tracing::warn!("SIGTERM cannot be handled ({e}): only Ctrl+C stops the daemon gracefully");
                let _ = tokio::signal::ctrl_c().await;
                tracing::info!("shutting down");
                return;
            }
        };
        tokio::select! {
            _ = tokio::signal::ctrl_c() => {}
            _ = term.recv() => {}
        }
    }
    #[cfg(not(unix))]
    let _ = tokio::signal::ctrl_c().await;
    tracing::info!("shutting down");
}

#[cfg(test)]
mod tests {
    use super::*;
    use grimoire_core::model::{Roots, Scope, StoreConfig};
    use std::io::{Read, Write};
    use std::path::Path;
    use std::process::{Child, Command, Stdio};
    use std::time::{Duration, Instant};

    /// What the child half is told: the grimoire to serve, with `:library` after it to serve the machine's library.
    const CHILD: &str = "GRIMOIRE_TEST_SERVE";

    /// A checkout at `root/<name>` with a store at `root/stores/<name>` that its project file names, as the machine's library finds one.
    fn project(root: &Path, name: &str) -> PathBuf {
        let repo = root.join(name);
        std::fs::create_dir_all(repo.join("src")).unwrap();
        std::fs::write(repo.join("src").join("lib.rs"), "pub fn a() {}\n").unwrap();
        let store_dir = root.join("stores").join(name);
        let config = StoreConfig {
            large_file: None,
            plain_file: None,
            site: vec![],
            version: StoreConfig::CURRENT_VERSION,
            repository: store_io::repository_string(&repo).unwrap(),
            roots: Roots::new("overview", "rites"),
            languages: vec!["rust".into()],
            scope: Scope::default(),
            order: Default::default(),
            recipe_kinds: Default::default(),
        };
        store_io::write_all(&store_dir, &grimoire_core::Store::new(config), false).unwrap();
        std::fs::write(repo.join(store_io::PROJECT_FILE), format!("name = \"{name}\"\nstore = {}\n", toml::Value::String(store_dir.to_string_lossy().to_string()))).unwrap();
        store_dir
    }

    /// The child half of the tests below, which run in a process of their own: `serve` for the grimoire it is told, the
    /// machine's data directory being its own, until its standard input closes, which drops `serve` mid-run as a
    /// stop does. Nothing when it is not run as a child.
    #[test]
    fn serve_child() {
        let Ok(spec) = std::env::var(CHILD) else { return };
        let (name, library) = match spec.split_once(':') {
            Some((name, _)) => (name.to_string(), Some(name.to_string())),
            None => (spec.clone(), None),
        };
        let (stop, stopped) = tokio::sync::oneshot::channel::<()>();
        std::thread::spawn(move || {
            let mut rest = String::new();
            let _ = std::io::stdin().read_to_string(&mut rest);
            let _ = stop.send(());
        });
        let options = ServeOptions {
            store_dir: store_io::stores_dir().unwrap().join(&name),
            port: 0,
            ui_dir: None,
            shared: false,
            listen: None,
            server: None,
            site: None,
            library,
            shared_library: None,
            site_of: None,
            checkout: None,
        };
        let rt = tokio::runtime::Runtime::new().unwrap();
        rt.block_on(async {
            tokio::select! {
                served = serve(options) => served.unwrap(),
                _ = stopped => {}
            }
        });
    }

    /// A daemon serving `spec` in a process of its own over the machine's data directory `root`.
    fn start(root: &Path, spec: &str) -> Child {
        Command::new(std::env::current_exe().unwrap())
            .args(["--exact", "tests::serve_child", "--test-threads=1"])
            .env(CHILD, spec)
            .env(store_io::HOME_ENV, root)
            .stdin(Stdio::piped())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .unwrap()
    }

    /// The port a daemon marked `store_dir` with, once it has.
    fn marked(store_dir: &Path) -> u16 {
        let deadline = Instant::now() + Duration::from_secs(60);
        loop {
            if let Some(marker) = store_io::read_serving(store_dir) {
                return marker.port;
            }
            assert!(Instant::now() < deadline, "the daemon never marked {}", store_dir.display());
            std::thread::sleep(Duration::from_millis(100));
        }
    }

    /// One call of a tool on the daemon at `port`: the status code and the body.
    fn call(port: u16, path: &str) -> (u16, String) {
        let mut s = std::net::TcpStream::connect(("127.0.0.1", port)).unwrap();
        s.set_read_timeout(Some(Duration::from_secs(60))).unwrap();
        write!(s, "POST {path} HTTP/1.0\r\nHost: 127.0.0.1\r\nContent-Type: application/json\r\nContent-Length: 2\r\n\r\n{{}}").unwrap();
        let mut answer = String::new();
        s.read_to_string(&mut answer).unwrap();
        let code = answer.split_whitespace().nth(1).unwrap().parse().unwrap();
        (code, answer.split_once("\r\n\r\n").map(|(_, b)| b.to_string()).unwrap_or_default())
    }

    /// Stop the daemon the way a stop does and wait for it to be gone.
    fn stop(mut child: Child) {
        drop(child.stdin.take());
        let deadline = Instant::now() + Duration::from_secs(60);
        loop {
            if let Some(status) = child.try_wait().unwrap() {
                assert!(status.success(), "the daemon stopped cleanly: {status}");
                return;
            }
            if Instant::now() > deadline {
                let _ = child.kill();
                panic!("the daemon did not stop");
            }
            std::thread::sleep(Duration::from_millis(100));
        }
    }

    /// # Test contract
    ///
    /// ```json
    /// {"test": "a-library-is-found-before-the-first-request-and-again-after", "checks": ["crates/server/src#library-found-before-first-request", "crates/server/src#library-found-again-every-few-seconds", "crates/server/src#markers-removed-on-stop"]}
    /// ```
    #[test]
    fn a_library_is_found_before_the_first_request_and_again_after() {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path();
        let (shop, ledger) = (project(root, "shop"), project(root, "ledger"));
        let child = start(root, "shop:library");
        // The first request the daemon answers is answered by a library that holds the other grimoire already.
        let port = marked(&shop);
        let (code, status) = call(port, "/g/ledger/api/status");
        assert_eq!(code, 200, "{status}");
        let names: Vec<String> = serde_json::from_str::<serde_json::Value>(&status).unwrap()["library"].as_array().unwrap().iter().map(|g| g["name"].as_str().unwrap().to_string()).collect();
        assert_eq!(names, ["ledger", "shop"]);
        // A grimoire made after that is found again, with nothing restarted.
        let audit = project(root, "audit");
        // A few refreshes, not a minute: a library found again only after a long interval fails here.
        let deadline = Instant::now() + library::live::REFRESH * 3 + Duration::from_secs(10);
        while call(port, "/g/audit/api/status").0 != 200 {
            assert!(Instant::now() < deadline, "audit was never found again");
            std::thread::sleep(Duration::from_millis(200));
        }
        // Every marker the daemon wrote goes when it stops: its own grimoire's and those of the library.
        for store in [&shop, &ledger, &audit] {
            assert!(store_io::read_serving(store).is_some(), "{} is marked while served", store.display());
        }
        stop(child);
        for store in [&shop, &ledger, &audit] {
            assert!(store_io::read_serving(store).is_none(), "{} is no longer marked", store.display());
            assert!(!store.join("serve.json").exists());
        }
    }

    /// # Test contract
    ///
    /// ```json
    /// {"test": "a-daemon-given-no-library-name-serves-its-own-store-alone", "checks": ["crates/server/src#serves-its-own-store-alone"]}
    /// ```
    #[test]
    fn a_daemon_given_no_library_name_serves_its_own_store_alone() {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path();
        let (shop, ledger) = (project(root, "shop"), project(root, "ledger"));
        let child = start(root, "shop");
        let port = marked(&shop);
        let (code, status) = call(port, "/api/status");
        assert_eq!(code, 200, "{status}");
        let status: serde_json::Value = serde_json::from_str(&status).unwrap();
        assert!(status["library"].as_array().is_none_or(|l| l.is_empty()), "no library is listed: {status}");
        // The machine's other grimoire is not served at its address, and is not marked.
        for path in ["/g/ledger/api/status", "/g/shop/api/status"] {
            let (_, body) = call(port, path);
            assert!(!body.contains("\"repository\""), "{path} answers no status: {body}");
        }
        assert!(store_io::read_serving(&ledger).is_none());
        stop(child);
    }
}
