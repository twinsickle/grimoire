//! # Module contract
//!
//! ```json
//! {"purpose":"Run the daemon that owns the store, keeps the graph current, and serves the web UI, the JSON API and MCP from one port.","criteria":[{"id":"no-repository-writes","statement":"The daemon never writes an index, cache or marker into the target repository."},{"id":"holds-the-lock","statement":"serve takes the store's lock before it opens the store and holds it until it has stopped serving, so a second daemon on the store is refused before it writes anything."},{"id":"shared-serve","statement":"serve with shared opens the store with no repository, starts no watcher, serves no interface, puts its router behind the token policy and listens on the address given or else the loopback port, under the store's lock like any daemon; an address to listen on without shared is refused before anything is opened."},{"id":"local-policy-installed","statement":"serve puts its router behind the local policy unless it is shared, so the daemon a person runs admits callers on this machine to read and write and refuses a request another site makes."}]}
//! ```

pub mod access;
pub mod client;
pub mod daemon;
pub mod http;
pub mod mcp;
pub mod store_io;
pub mod watch;

use std::net::SocketAddr;
use std::path::PathBuf;
use std::sync::Arc;

pub use daemon::{load, Daemon, Loaded};

pub struct ServeOptions {
    pub store_dir: PathBuf,
    pub port: u16,
    pub ui_dir: Option<PathBuf>,
    /// Serve the store for a team: no repository, no interface, callers admitted by token.
    pub shared: bool,
    /// Where a shared daemon listens; the loopback port when not given.
    pub listen: Option<SocketAddr>,
}

/// Open the store, start watching the repository and serve until the
/// process is interrupted.
pub async fn serve(opts: ServeOptions) -> anyhow::Result<()> {
    // The store's one writer from before it is opened, since opening prunes
    // the baseline, until serving has stopped; released when this returns.
    // A daemon that trusts every caller on this machine is never reachable from another.
    if opts.listen.is_some() && !opts.shared {
        anyhow::bail!("--listen needs --shared: a daemon that admits every local caller listens only on this machine");
    }
    let _lock = store_io::lock(&opts.store_dir)?;
    if opts.shared {
        return serve_shared(opts).await;
    }
    let (daemon, extract_errors) = Daemon::open(&opts.store_dir)?;
    for e in &extract_errors {
        tracing::warn!("{e}");
    }
    let daemon = Arc::new(daemon);
    let status = daemon.call("status", serde_json::json!({})).unwrap_or_default();
    tracing::info!("store {} for repository {}", opts.store_dir.display(), daemon.repo.display());
    tracing::info!("graph: {}", serde_json::to_string(&status["nodes"]).unwrap_or_default());

    let _watcher = watch::start(daemon.clone())?;
    let ui = opts.ui_dir.clone().map_or(http::Ui::Embedded, http::Ui::Dir);
    // The local policy: callers on this machine may read and write.
    let app = http::router(daemon.clone(), ui, Arc::new(access::Local));
    let addr = SocketAddr::from(([127, 0, 0, 1], opts.port));
    let listener = tokio::net::TcpListener::bind(addr).await?;
    // The port the listener got, which is not the one asked for when it was left to the system.
    let addr = listener.local_addr()?;
    daemon.mark_serving(addr.port())?;
    tracing::info!("listening on http://{addr}  (api: /api/<tool>, mcp: /mcp, events: /api/events)");
    let served = axum::serve(listener, app).with_graceful_shutdown(shutdown()).await;
    daemon.unmark_serving();
    served?;
    Ok(())
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
