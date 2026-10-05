//! # Module contract
//!
//! ```json
//! {"purpose":"Run the daemon that owns the store, keeps the graph current, and serves the web UI, the JSON API and MCP from one port.","criteria":[{"id":"no-repository-writes","statement":"The daemon never writes an index, cache or marker into the target repository."},{"id":"holds-the-lock","statement":"serve takes the store's lock, or on a project that names a server the lock of the checkout's copy, before it opens the store and holds it until it has stopped serving, so a second daemon on the store is refused before it writes anything."},{"id":"shared-serve","statement":"serve with shared opens the store with no repository, starts no watcher, serves no interface, puts its router behind the token policy and listens on the address given or else the loopback port, under the store's lock like any daemon; an address to listen on without shared is refused before anything is opened."},{"id":"local-policy-installed","statement":"serve puts its router behind the local policy unless it is shared, on a project that names a server as on any other, so the daemon a person runs admits callers on this machine to read and write and refuses a request another site makes."},{"id":"serves-a-server-project","statement":"serve given a server opens the checkout's copy of that server's intent with the checkout's code, watches the repository, serves the interface, the API and MCP on the loopback port as for any project, and follows the server; given with shared it is refused before anything is opened."},{"id":"site-listener","statement":"serve, shared or not, given a site address binds it beside the daemon's own and serves the site's router alone there for as long as the daemon serves, refusing to start when the address cannot be bound; given none, it opens no further address."},{"id":"library","statement":"serve given a library name serves its own grimoire on the bare routes, marked at /g/<name>, and keeps the machine's library served beside it through library::live, found once before the daemon answers a request and again every few seconds after; it removes every marker it wrote when it stops; given none, it serves its own store alone."}]}
//! ```

pub mod access;
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

pub use daemon::{load, load_store, Daemon, Loaded};

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
}

/// Serve the project's site on its own address beside the daemon, until the process stops.
async fn serve_site(daemon: Arc<Daemon>, addr: SocketAddr) -> anyhow::Result<SocketAddr> {
    let listener = tokio::net::TcpListener::bind(addr).await?;
    let addr = listener.local_addr()?;
    tracing::info!("site on http://{addr}: how the project is meant to be used, with no sign-in; who may reach it is the deployment's to decide");
    let app = site::router(daemon);
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
    let (daemon, extract_errors) = match server {
        Some((server, repo)) => {
            tracing::info!("this project's grimoire is the server's at {}; this checkout's copy is {}", server.address, opts.store_dir.display());
            // Opening asks the server, which blocks: kept off the runtime's own threads.
            let copy = opts.store_dir.clone();
            tokio::task::spawn_blocking(move || Daemon::open_remote(&copy, &repo, server)).await??
        }
        None => Daemon::open(&opts.store_dir)?,
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
        serve_site(daemon.clone(), at).await?;
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
        Some(name) => daemon.mark_serving_in(addr.port(), &format!("/g/{name}"))?,
        None => daemon.mark_serving(addr.port())?,
    }
    // The local policy: callers on this machine may read and write.
    let policy: Arc<dyn access::Policy> = Arc::new(access::Local);
    let mut api = http::api(daemon.clone(), policy.clone());
    // The machine's library, kept current while the daemon runs (req:machine-library).
    let live = match &opts.library {
        Some(name) => {
            let default = library::Grimoire { name: name.clone(), store_dir: opts.store_dir.clone(), repo: daemon.repo.clone(), server: None };
            match (store_io::stores_dir(), store_io::copies_dir()) {
                (Ok(stores), Ok(copies)) => {
                    let live = library::live::Live::new(default, daemon.clone(), addr.port(), policy.clone(), stores, copies);
                    let first = live.clone();
                    tokio::task::spawn_blocking(move || first.refresh()).await?;
                    api = api.merge(library::live::routes(live.clone(), policy.clone()));
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
        serve_site(daemon.clone(), at).await?;
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

async fn shutdown() {
    let _ = tokio::signal::ctrl_c().await;
    tracing::info!("shutting down");
}
