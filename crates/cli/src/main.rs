//! # Module contract
//!
//! ```json
//! {"purpose":"Parse the command line and dispatch to serve, check, adopt, bottle, token, login, logout, scaffold, init, uninstall or tool.","criteria":[{"id":"token-add","statement":"token add makes a token for the store, prints it once and keeps its digest, read-only when asked."},{"id":"token-revoke","statement":"token revoke removes a token by name and fails when there is none."},{"id":"token-list","statement":"token list prints each name and what it may do."},{"id":"token-finds-store","statement":"token add, revoke and list each find their store as every command does, refusing a project that names a server unless --store names the store."},{"id":"token-own-lock","statement":"token add, revoke and list take the tokens' own lock, never the store's, so they work while a daemon serves."},{"id":"token-sent","statement":"A command run with GRIMOIRE_TOKEN set sends it to the daemon it calls."},{"id":"token-refused-says-so","statement":"A command the daemon refuses says so and does not go round the daemon."},{"id":"writes-through-daemon","statement":"tool, adopt, bottle and init run a tool on the daemon serving the store when there is one, and against the store directly only when there is not; on a project that names a server the store is the server's, and a write goes to it."},{"id":"store-found","statement":"Every command finds its store from --store, else from the nearest .grimoire.toml above the working directory, which names it."},{"id":"store-found-server-copy","statement":"When the nearest .grimoire.toml names a server, a command's store is this checkout's copy of the server's intent."},{"id":"store-flag-resolved","statement":"A --store is resolved as the project file is, a relative path against the project's repository (the working directory outside any project) and a bare name under the stores directory."},{"id":"repository-untouched","statement":"Only init writes into a repository, and only the integration: .grimoire.toml, the agent skill and its references for each harness, a marked section in CLAUDE.md and AGENTS.md, the MCP entry in .mcp.json and .codex/config.toml, a pre-commit hook and a post-commit hook; only uninstall takes it out again. Every other command reads it."},{"id":"lock-writers-hold","statement":"A command that writes the store with no daemon serving it takes the store's lock first and holds it to the end of its run: tool with a write tool, adopt and bottle for the call, init from creating or opening the store to adopting, and scaffold while it writes the store."},{"id":"lock-delete-holds","statement":"uninstall --delete-store, with no daemon serving the store, checks it once, lets a daemon's library release it, then takes the store's lock under the deletion queue's lock and holds it to the store's removal, checking again under it before anything is removed."},{"id":"lock-refused-writes-nothing","statement":"A command that writes the store and is refused the lock writes nothing, but uninstall --delete-store, which queues the deletion instead."},{"id":"lock-delete-queues","statement":"uninstall --delete-store queues the deletion when another holds the lock."},{"id":"lock-reads-take-none","statement":"Reads take no lock."},{"id":"lock-check-prunes-when-got","statement":"check, a read, takes the lock before it loads and prunes the baseline only when it got it."},{"id":"serve-site-of","statement":"serve takes --site-of only beside --library and --site, refusing it otherwise before anything is served, and hands the name to the team's library server as the grimoire whose site is served."},{"id":"serve-a-server-project","statement":"serve on a project that names a server, with no --store, serves the checkout's copy of the server's intent, signed in with the person's token; with --shared and no --store it is refused."}]}
//! ```

mod check;
mod init;
mod team;
mod uninstall;

use std::path::{Path, PathBuf};

use anyhow::{bail, Context};
use clap::{Parser, Subcommand};
use grimoire_core::scaffold;
use grimoire_core::Extraction;
use grimoire_extract::Extractor;
use grimoire_server::{client, store_io};
use serde_json::{json, Value};

#[derive(Parser)]
#[command(name = "grimoire", version, about = "Gives a software project its grimoire and its tickets: one graph of intent, requirements, module contracts and code, served to people and agents.")]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Run the daemon: watch the repository, serve the API, MCP and the UI.
    Serve {
        /// Store path or name. Defaults to the store named by the nearest .grimoire.toml.
        #[arg(long)]
        store: Option<String>,
        #[arg(long, default_value_t = 4242)]
        port: u16,
        /// Directory holding the built web UI. Without it, only the API and MCP serve.
        #[arg(long)]
        ui_dir: Option<PathBuf>,
        /// Serve the store for a team: intent only, with no repository beside it,
        /// no web UI, and callers admitted by a token (see `grimoire token`).
        #[arg(long)]
        shared: bool,
        /// The address and port a shared daemon listens on, such as 0.0.0.0:4242.
        /// Plain HTTP: put a proxy in front for HTTPS. Without it, the loopback --port.
        #[arg(long)]
        listen: Option<std::net::SocketAddr>,
        /// Also serve the project's site, how it is meant to be used, on this address, such as
        /// 0.0.0.0:8080: the pages `configure` names in `site`, with no sign-in. Who may reach it
        /// is for the deployment to decide.
        #[arg(long)]
        site: Option<std::net::SocketAddr>,
        /// With --shared, serve every store under this directory as the team's library, each
        /// grimoire at /g/<folder>/ behind its own tokens, kept current while it runs.
        #[arg(long, requires = "shared", conflicts_with = "store")]
        library: Option<PathBuf>,
        /// With --library and --site, the grimoire whose site is served: one library, one site.
        #[arg(long, requires_all = ["library", "site"])]
        site_of: Option<String>,
    },
    /// Run the lints without a daemon. Exits 1 when there are findings.
    Check {
        #[arg(long)]
        store: Option<String>,
        /// Only these rules: dangling, uncovered, malformed, unspecified.
        #[arg(long, value_delimiter = ',')]
        rules: Vec<String>,
        /// Print findings as JSON, the same shape as the `lint` tool.
        #[arg(long)]
        json: bool,
        /// Judge the code against this ticket's proposal, a slug or a ticket:
        /// reference, as a pipeline does before the change is bottled. Defaults to
        /// the current ticket, or the truth when there is none.
        #[arg(long)]
        ticket: Option<String>,
        /// Run on the main line, as a pipeline does on every main-line build:
        /// judged against the truth, then on a team the server's baseline is
        /// pruned of what this code no longer produces. Needs a write token.
        #[arg(long, conflicts_with = "ticket")]
        main_line: bool,
    },
    /// Build a skeleton store from a repository's module contracts alone.
    Scaffold {
        /// The repository to read. Defaults to the working directory.
        #[arg(long)]
        repo: Option<PathBuf>,
        /// Store path or name to write. Required.
        #[arg(long)]
        store: String,
        /// Languages to extract, comma separated.
        #[arg(long, value_delimiter = ',', default_value = "rust")]
        languages: Vec<String>,
        /// Replace an existing store's pages, requirements and tickets.
        #[arg(long)]
        force: bool,
    },
    /// Point a repository at a store and install the integration: .grimoire.toml,
    /// the agent skill and its references, a marked section in CLAUDE.md and
    /// AGENTS.md, the MCP entry, a pre-commit hook running `check` and a
    /// post-commit hook running `bottle`.
    Init {
        #[arg(long)]
        repo: Option<PathBuf>,
        /// Store path or name. Made if it does not exist: the skeleton read off the module contracts, or empty with --empty.
        #[arg(long, required_unless_present = "server", conflicts_with = "server")]
        store: Option<String>,
        /// The address of the team's server that holds this project's grimoire,
        /// such as https://grimoire.example. No store is made on this machine.
        #[arg(long)]
        server: Option<String>,
        #[arg(long, value_delimiter = ',', default_value = "rust")]
        languages: Vec<String>,
        /// The port the daemon will serve on; written into .mcp.json.
        #[arg(long, default_value_t = 4242)]
        port: u16,
        /// The grimoire's name, the same on every machine, which the MCP entry's URL
        /// carries: /g/<name>/mcp. Defaults to the name the project file already
        /// holds, else the repository folder's, made a slug.
        #[arg(long)]
        name: Option<String>,
        /// Make a new store with its roots named and no page, rather than the skeleton
        /// read off the module contracts: for writing a grimoire from nothing.
        #[arg(long, conflicts_with = "server")]
        empty: bool,
    },
    /// Remove exactly what init wrote, and nothing of yours. The store is kept
    /// unless --delete-store is given.
    Uninstall {
        #[arg(long)]
        repo: Option<PathBuf>,
        /// Also delete the store, when it names this repository and no daemon serves it.
        #[arg(long)]
        delete_store: bool,
    },
    /// Record every present finding as the baseline, so the check fails only
    /// on new ones. Resets any earlier baseline; the reason is kept.
    Adopt {
        #[arg(long)]
        store: Option<String>,
        /// Why the present findings are accepted as inherited work.
        #[arg(long)]
        reason: String,
    },
    /// Tell Grimoire a change is on the main line: bottle the ticket simmering
    /// on it, with the decision it carries. With no ticket, the current one when
    /// it is simmering.
    // `land`, its name before ticket 0168, still runs it, so hooks and pipelines written for it go on working.
    #[command(alias = "land")]
    Bottle {
        #[arg(long)]
        store: Option<String>,
        /// The ticket to bottle, a slug or a ticket: reference. Defaults to the current ticket.
        ticket: Option<String>,
    },
    /// The tokens a shared daemon admits: add one, revoke one, or list them.
    Token {
        #[command(subcommand)]
        command: TokenCommand,
    },
    /// Sign in to a grimoire of a team's server, once per grimoire address: the
    /// token is checked with the server and kept outside any repository, under
    /// that address. Read from standard input when not given.
    Login {
        /// The grimoire's address on the server. Defaults to the one the project names.
        #[arg(long)]
        server: Option<String>,
        /// The token, as `grimoire token add` printed it.
        #[arg(long)]
        token: Option<String>,
    },
    /// Forget the token kept for a grimoire of a team's server.
    Logout {
        /// The grimoire's address on the server. Defaults to the one the project names.
        #[arg(long)]
        server: Option<String>,
    },
    /// Run one tool against the store and print its output: on the daemon
    /// serving the store when there is one, directly otherwise.
    Tool {
        #[arg(long)]
        store: Option<String>,
        name: String,
        /// JSON input, default {}.
        input: Option<String>,
    },
}

#[derive(Subcommand)]
enum TokenCommand {
    /// Make a token and print it, once. Only its digest is kept.
    Add {
        /// Who or what it is for: a slug, used once.
        name: String,
        /// May read and not write, as for a pipeline's check.
        #[arg(long)]
        read_only: bool,
        #[arg(long)]
        store: Option<String>,
    },
    /// Remove a token; it is refused from then on, with no restart.
    Revoke {
        name: String,
        #[arg(long)]
        store: Option<String>,
    },
    /// Print each token's name and what it may do.
    List {
        #[arg(long)]
        store: Option<String>,
    },
}

/// Manage the tokens file beside a store. It is not a store record: it is
/// changed under a lock of its own, never the store's, so tokens are added
/// and revoked while a daemon serves the store.
fn token(command: TokenCommand) -> anyhow::Result<()> {
    use store_io::tokens;
    let store_dir = |store: Option<&str>| -> anyhow::Result<PathBuf> {
        let target = target(store)?;
        if let Some(team) = &target.team {
            bail!("{}, and its tokens are kept beside its store on the host: run this there, or name that store with --store", team::says(team));
        }
        Ok(target.store_dir)
    };
    match command {
        TokenCommand::Add { name, read_only, store } => {
            let dir = store_dir(store.as_deref())?;
            let made = tokens::add(&dir, &name, !read_only)?;
            println!("{made}");
            eprintln!("token `{name}` may {}; this is the only time it is shown, and the store keeps its digest", if read_only { "read" } else { "read and write" });
            Ok(())
        }
        TokenCommand::Revoke { name, store } => {
            let dir = store_dir(store.as_deref())?;
            if !tokens::revoke(&dir, &name)? {
                bail!("no token named `{name}` in {}", dir.display());
            }
            println!("revoked `{name}`");
            Ok(())
        }
        TokenCommand::List { store } => {
            let dir = store_dir(store.as_deref())?;
            for t in tokens::list(&dir)? {
                println!("{}\t{}", t.name, if t.write { "read and write" } else { "read" });
            }
            Ok(())
        }
    }
}

fn cwd() -> anyhow::Result<PathBuf> {
    Ok(std::env::current_dir()?)
}

/// Where a command's grimoire is: the store it reads, and on a project that
/// names a server, the server, with the store then this checkout's copy.
struct Target {
    store_dir: PathBuf,
    team: Option<team::Team>,
    /// The checkout the command runs in, for a store of its own several checkouts may share.
    checkout: Option<PathBuf>,
}

/// The checkout of a store of its own a command runs in, when it is not the store's main one:
/// its repository as the store lists it, added to the store's checkouts first if it is not yet
/// one, taking the store's lock for that unless `held` is it (req:store-checkouts). None for the
/// main checkout, and for a team's grimoire, whose copy is this checkout's own already.
fn checkout_view(target: &Target, held: Option<&store_io::StoreLock>) -> anyhow::Result<Option<String>> {
    let Some(dir) = target.checkout.as_deref().filter(|_| target.team.is_none()) else { return Ok(None) };
    let repository = store_io::repository_string(dir)?;
    let main = store_io::checkouts::main_of(&target.store_dir)?;
    if repository == main {
        return Ok(None);
    }
    if !store_io::checkouts::others(&target.store_dir).contains(&repository) {
        let _lock = match held {
            Some(_) => None,
            None => match store_io::lock(&target.store_dir) {
                Ok(lock) => Some(lock),
                // A daemon serving the store holds its lock, and adds the checkout as its writer when asked as it.
                Err(locked) => match client::probe(&target.store_dir, &repository) {
                    client::Probe::Serving(_) => return Ok(Some(repository)),
                    _ => return Err(anyhow::anyhow!("this checkout, {repository}, is not yet one of store {}'s, and it could not be added: {locked}", target.store_dir.display())),
                },
            },
        };
        if store_io::checkouts::register(&target.store_dir, dir)? {
            eprintln!("added the checkout {repository} to store {}", target.store_dir.display());
        }
    }
    Ok(Some(repository))
}

fn target(arg: Option<&str>) -> anyhow::Result<Target> {
    match arg {
        // As init and the project file resolve it: a relative path against the
        // project's repository, or the working directory outside any project.
        Some(s) => {
            let here = cwd()?;
            let project = store_io::find_home(&here).ok();
            let base = project.as_ref().map_or(here, |(repo, _)| repo.clone());
            let store_dir = store_io::store_for_repo(s, &base)?;
            // Run in a checkout whose project file names that same store: that checkout's view of it.
            let same = |a: &Path, b: &Path| a.canonicalize().ok().zip(b.canonicalize().ok()).is_some_and(|(a, b)| a == b);
            let checkout = project.and_then(|(repo, home)| match home {
                store_io::Home::Store(named) if same(&named, &store_dir) => Some(repo),
                _ => None,
            });
            Ok(Target { store_dir, team: None, checkout })
        }
        None => match store_io::find_home(&cwd()?)? {
            (repo, store_io::Home::Store(store_dir)) => Ok(Target { store_dir, team: None, checkout: Some(repo) }),
            (repo, store_io::Home::Server { address, copy }) => Ok(Target { store_dir: copy.clone(), team: Some(team::Team { address, repo, copy }), checkout: None }),
        },
    }
}

/// Run one tool against the store. When a daemon serves it the call goes to
/// the daemon, which stays the store's only writer; otherwise it runs here
/// and any write is persisted, with the baseline pruned as the daemon would.
/// The inner error is the tool's error object.
fn run_tool(target: &Target, name: &str, input: Value, held: Option<&store_io::StoreLock>) -> anyhow::Result<Result<Value, Value>> {
    // On a project that names a server the store is the server's, and a write goes to it.
    if let Some(team) = &target.team {
        return team::run_tool(team, name, input);
    }
    let store_dir = target.store_dir.as_path();
    let view = checkout_view(target, held)?;
    let (store, _) = store_io::load(store_dir)?;
    // The daemon is asked as this checkout: it answers from this checkout's code and current ticket.
    match client::probe(store_dir, view.as_deref().unwrap_or(&store.config.repository)) {
        client::Probe::Serving(addr) => {
            let (ok, v) = client::call_to(&addr, name, &input)?;
            return Ok(if ok { Ok(v) } else { Err(v) });
        }
        // A daemon serves the store and will not let this command in: say so, and never go round it.
        client::Probe::Refused(addr, why) => bail!("a daemon serves this store at {addr} and refused this command: {why}. Set {} to a token it holds", client::TOKEN_ENV),
        client::Probe::None => {}
    }
    // No daemon answers, so this command is the writer, and holds the lock for
    // its run; a daemon still starting holds it already and refuses this one.
    // `held` is a lock the caller took for a longer run, as init does.
    let _lock = if grimoire_core::tools::is_write(name) && held.is_none() { Some(store_io::lock(store_dir)?) } else { None };
    let mut loaded = grimoire_server::load_for(store_dir, view.as_deref())?;
    // A lint is the check's answer, judged by this checkout's own work (req:check-own-work), and a close
    // names only this ticket's own work (req:tool-untouched): both tell it from the base's code.
    if matches!(name, "lint" | "close_ticket" | "bottle_ticket") {
        if let Err(e) = grimoire_server::base::give(&mut loaded.state, &loaded.repo) {
            eprintln!("warning: {e}");
        }
    }
    // Run as the person on this machine: a start lists them as having its ticket checked out (req:ticket-checked-out).
    match grimoire_core::tools::call_by(&mut loaded.state, name, input, &store_io::today(), grimoire_server::daemon::as_this_person(name)) {
        Ok(out) => {
            if let Some(changed) = &out.changed {
                store_io::persist(store_dir, loaded.state.store(), changed)?;
                // Only the store's main checkout prunes its baseline: another's code is a branch's.
                if let Some(pruned) = loaded.state.prune_baseline().filter(|_| view.is_none()) {
                    store_io::persist(store_dir, loaded.state.store(), &pruned)?;
                }
            }
            Ok(Ok(out.output))
        }
        Err(e) => Ok(Err(serde_json::to_value(&e)?)),
    }
}

/// Write a whole store, for `init` and `scaffold`, under its lock, which is
/// returned for the caller to hold to the end of its run: a daemon serving
/// the store refuses this rather than having it replaced underneath. The
/// directory is made first, since the lock lives in it.
fn write_whole_store(store_dir: &Path, store: &grimoire_core::Store, replace: bool) -> anyhow::Result<store_io::StoreLock> {
    std::fs::create_dir_all(store_dir).with_context(|| format!("creating {}", store_dir.display()))?;
    let lock = store_io::lock_new(store_dir)?;
    store_io::write_all(store_dir, store, replace)?;
    Ok(lock)
}

/// Adopt and say what it did, for `adopt` and the end of `init`.
fn adopt(target: &Target, reason: &str, held: Option<&store_io::StoreLock>) -> anyhow::Result<()> {
    match run_tool(target, "adopt", json!({ "reason": reason }), held)? {
        Ok(out) => {
            let count = out["baseline"]["count"].as_u64().unwrap_or(0);
            match out["replaced"].as_u64() {
                Some(was) => println!("baselined {count} finding(s), replacing a baseline of {was}: {reason}"),
                None => println!("baselined {count} finding(s): {reason}"),
            }
            Ok(())
        }
        Err(e) => bail!("adopt failed: {}", e["message"].as_str().unwrap_or("unknown error")),
    }
}

/// The repository as the store records it: its canonical path, slashed.
fn canonical(p: &Path) -> anyhow::Result<String> {
    let c = p.canonicalize().with_context(|| format!("{} does not exist", p.display()))?;
    Ok(store_io::slashed(&c.to_string_lossy()))
}

/// The skeleton of a store for the repository at `repo`, read off its module
/// contracts in `languages`, with the extraction it was read from.
fn skeleton(repo: &Path, repo_str: &str, languages: &[String]) -> anyhow::Result<(grimoire_core::Store, Extraction)> {
    let extractor = Extractor::for_languages(repo, languages);
    let (files, errors) = extractor.extract_all()?;
    for e in &errors {
        eprintln!("warning: {e}");
    }
    let extraction = Extraction::from_files(&files);
    Ok((scaffold::scaffold(&extraction, repo_str, languages), extraction))
}

/// Say what a skeleton holds, and which modules it found no usable contract in.
fn report_skeleton(skeleton: &grimoire_core::Store, extraction: &Extraction, store_dir: &Path) {
    println!(
        "wrote {} page(s) and {} requirement(s) for {} contracted module(s) to {}",
        skeleton.pages.len(),
        skeleton.requirements.len(),
        extraction.modules.values().filter(|m| m.purpose.is_some()).count(),
        store_dir.display()
    );
    let missing: Vec<&String> = extraction.modules.values().filter(|m| m.expects_contract && m.purpose.is_none()).map(|m| &m.id).collect();
    if !missing.is_empty() {
        println!("{} module(s) without a usable contract:", missing.len());
        for m in missing {
            println!("  {m}");
        }
    }
}

fn main() -> anyhow::Result<()> {
    let cli = Cli::parse();
    tracing_subscriber::fmt().with_env_filter(tracing_subscriber::EnvFilter::try_from_default_env().unwrap_or_else(|_| "info".into())).with_target(false).init();
    // Not under init, which may be taking up again a store whose deletion is queued: it cancels that first.
    if !matches!(cli.command, Command::Init { server: None, .. }) {
        uninstall::carry_out_deletions();
    }

    match cli.command {
        Command::Serve { store, port, ui_dir, shared, listen, site, library: shared_library, site_of } => {
            let rt = tokio::runtime::Runtime::new()?;
            // A team's library: the stores under a directory, with no checkout here (req:server-library).
            if let Some(dir) = shared_library {
                let options =
                    grimoire_server::ServeOptions { store_dir: dir.clone(), port, ui_dir, shared, listen, server: None, site, library: None, shared_library: Some(dir), site_of, checkout: None };
                return rt.block_on(grimoire_server::serve(options));
            }
            // Started in a checkout and given no store, the daemon serves the machine's library,
            // this checkout's grimoire first; given one, that store alone (req:machine-library).
            let library = match (&store, shared) {
                (None, false) => Some(store_io::find_named(&cwd()?)?.2),
                _ => None,
            };
            let target = target(store.as_deref())?;
            // On a project that names a server, the daemon serves this checkout's copy of the server's intent.
            let server = match target.team {
                Some(team) => Some((team.server()?, team.repo)),
                None => None,
            };
            // A store of its own is served as the checkout serve runs in sees it; a --store given names no checkout.
            let checkout = if shared || server.is_some() { None } else { target.checkout };
            rt.block_on(grimoire_server::serve(grimoire_server::ServeOptions {
                store_dir: target.store_dir,
                port,
                ui_dir,
                shared,
                listen,
                server,
                site,
                library,
                shared_library: None,
                site_of: None,
                checkout,
            }))
        }
        Command::Token { command } => token(command),
        Command::Login { server, token } => team::login(server.as_deref(), token),
        Command::Logout { server } => team::logout(server.as_deref()),
        Command::Check { store, rules, json, ticket, main_line } => check::check(target(store.as_deref())?, rules, json, ticket, main_line),
        Command::Scaffold { repo, store, languages, force } => {
            let languages: Vec<String> = languages.iter().map(|l| l.trim().to_string()).collect();
            let repo = repo.unwrap_or(cwd()?);
            let repo_str = canonical(&repo)?;
            // Resolved as the project file would resolve it, a relative path against the repository.
            let store_dir = store_io::store_for_repo(&store, &repo)?;
            store_io::refuse_inside(&store_dir, &repo)?;
            let (skeleton, extraction) = skeleton(&repo, &repo_str, &languages)?;
            write_whole_store(&store_dir, &skeleton, force)?;
            report_skeleton(&skeleton, &extraction, &store_dir);
            Ok(())
        }
        Command::Init { repo, store, server, languages, port, name, empty } => init::command::run(repo, store, server, languages, port, name, empty),
        Command::Uninstall { repo, delete_store } => uninstall::uninstall(repo, delete_store),
        Command::Adopt { store, reason } => adopt(&target(store.as_deref())?, &reason, None),
        Command::Bottle { store, ticket } => check::bottle(&target(store.as_deref())?, ticket.as_deref()),
        Command::Tool { store, name, input } => {
            let target = target(store.as_deref())?;
            let input: Value = match input {
                Some(s) => serde_json::from_str(&s).context("input is not JSON")?,
                None => json!({}),
            };
            // A former tool name is answered as the tool it now names (req:tools-former-names).
            match run_tool(&target, grimoire_core::tools::canonical(&name), input, None)? {
                Ok(out) => {
                    println!("{}", serde_json::to_string_pretty(&out)?);
                    Ok(())
                }
                Err(e) => {
                    println!("{}", serde_json::to_string_pretty(&e)?);
                    std::process::exit(1)
                }
            }
        }
    }
}
