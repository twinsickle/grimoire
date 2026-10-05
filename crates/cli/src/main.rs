//! # Module contract
//!
//! ```json
//! {"purpose":"Parse the command line and dispatch to serve, check, adopt, land, token, login, logout, scaffold, init, uninstall or tool.","criteria":[{"id":"check-without-daemon","statement":"The check command runs the lints without a daemon, for hooks and CI, exits non-zero only when there are findings outside the baseline, and prunes the baseline itself when no daemon serves the store, but never on a project that names a server."},{"id":"check-names-a-ticket","statement":"check with --ticket judges the code against the truth plus that ticket's proposal in place of the current ticket's, a slug or a ticket: reference, and fails with the reason when the ticket does not exist or is neither active nor landing, or, on a project that names a server, when the server's intent cannot be read as it is now; without it, against the current ticket's proposal, or the truth when there is no current ticket."},{"id":"check-main-line","statement":"check with --main-line judges the code against the truth whatever the current ticket, and is refused beside --ticket; on a project that names a server it prunes the server's baseline with prune_from_main_line, which reads the server's intent as it is now, never the checkout's copy, then prints the findings of the intent and code that prune judged, as check does, and says how many entries went, on standard error under --json; it exits non-zero when there are new findings or the prune failed; on a store of the project's own it does what check does."},{"id":"init-adopts","statement":"init ends by adopting, with the reason adopted at init, when the store has no baseline yet, one it created or one scaffold made, so the hook it installs passes on the next commit, and prints how many findings it baselined; on a store that already has a baseline, a re-run or a moved checkout, it keeps that baseline as it is and says so."},{"id":"unsupported-first","statement":"check prints a finding that the build cannot extract a configured language before every other line, baselined or not, so the reason an old binary fails is the first thing read."},{"id":"land","statement":"land runs land_ticket for the ticket it is given, or with none for the current ticket when that one is landing; it prints what landed or that no ticket was waiting, or on a project that names a server and with no ticket given that a ticket lands on the main line, names the tickets still landing; on a project that names a server, once a ticket it named has landed, it prunes the server's baseline from the checkout's code against the truth as it is now, with prune_from_main_line, and says how many entries went; it exits non-zero when a landing was refused, when a landing taken on a team could not then prune, saying the ticket landed, printing the records that moved, or when the store cannot be reached, never because no ticket was waiting."},{"id":"token","statement":"token add makes a token for the store, prints it once and keeps its digest, read-only when asked; token revoke removes one by name and fails when there is none; token list prints each name and what it may do; each finds its store as every command does, refusing a project that names a server unless --store names the store, and takes the tokens' own lock, never the store's, so it works while a daemon serves; a command run with GRIMOIRE_TOKEN set sends it to the daemon it calls, and one the daemon refuses says so and does not go round it."},{"id":"writes-through-daemon","statement":"tool, adopt, land and init run a tool on the daemon serving the store when there is one, and against the store directly only when there is not; on a project that names a server the store is the server's, and a write goes to it."},{"id":"store-resolution","statement":"Every command finds its store from --store, else from the nearest .grimoire.toml above the working directory, which names it or names a server, whose store here is this checkout's copy of the server's intent; a --store is resolved as the project file is, a relative path against the project's repository (the working directory outside any project) and a bare name under the stores directory."},{"id":"repository-untouched","statement":"Only init writes into a repository, and only the integration: .grimoire.toml, the agent skill and its references for each harness, a marked section in CLAUDE.md and AGENTS.md, the MCP entry in .mcp.json and .codex/config.toml, a pre-commit hook and a post-commit hook; only uninstall takes it out again. Every other command reads it."},{"id":"uninstall-store","statement":"uninstall finds the project from the nearest .grimoire.toml above the working directory, or the --repo directory's own, refusing a malformed one; it keeps the store unless asked, printing where it is, on a project that names a server keeps the checkout's copy of the server's intent and refuses --delete-store before removing anything, the store being the server's, and removes .grimoire.toml last of all, after the store when it deletes one, so a failure part way leaves it for a rerun. With --delete-store it removes the store only when the store is not the repository, an ancestor of it or a root, holds nothing a store does not, its record folders included, names this repository, and is neither answered for by a daemon nor marked as served, a daemon serving it in its machine's library being asked to let it go once every other check has passed and told when uninstall is done with it, however it ends, its own default being refused, checking all before removing anything and again just before the deletion, refusing a store inside its repository, and holding the store's lock from the first check, and the tokens' lock while it removes: under them the records and the tokens go first and grimoire.json last, so a store stopped part way still loads for a rerun, then the lock files, and the folder once the lock is released. What a deletion stopped after grimoire.json went leaves, no folder or one holding only the lock files and empty record folders, a rerun removes and finishes."},{"id":"takes-the-lock","statement":"A command that writes the store with no daemon serving it takes the store's lock first and holds it to the end of its run: tool with a write tool, adopt and land for the call, init from creating or opening the store to adopting, scaffold while it writes the store, and uninstall --delete-store from its first check, or from the moment a daemon's library has let the store go, to the store's removal; refused, it writes nothing. Reads take no lock, and check, a read, takes the lock before it loads and prunes the baseline only when it got it."},{"id":"init-repoints","statement":"init on a store that records another repository re-points it to this one, under the store's lock, saying it did, only when no folder is left at the recorded repository and this checkout's .grimoire.toml names this store; when the recorded repository still exists, or the project file names another store, it refuses with the reason and changes nothing. The same folder spelt differently, by case or through a link, is this checkout, not a move; the repository is recorded as its canonical path, a share as //server/share."},{"id":"init-server","statement":"init with --server writes .grimoire.toml naming the server, as its address is kept, and installs the integration, creating no store and adopting nothing; it takes --store or --server and never both, and refuses an address that is not one before writing anything. The grimoire it names is the one --name gives, else the one the address carries in /g/<name>, else the repository folder's."},{"id":"serve-site-of","statement":"serve takes --site-of only beside --library and --site, refusing it otherwise before anything is served, and hands the name to the team's library server as the grimoire whose site is served."},{"id":"serve-a-server-project","statement":"serve on a project that names a server, with no --store, serves the checkout's copy of the server's intent, signed in with the person's token; with --shared and no --store it is refused."},{"id":"init-scaffolds","statement":"init, creating a store, writes into it the skeleton scaffold would write for the repository in the languages given, or with --empty a store whose roots are named and which holds no page, and says what it made, before adopting; a store that already exists is left as it is, and init with --server makes no store."}]}
//! ```

mod init;
mod team;

use std::path::{Path, PathBuf};

use anyhow::{bail, Context};
use clap::{Parser, Subcommand};
use grimoire_core::lint::{self, Rule};
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
        /// reference, as a pipeline does before the change lands. Defaults to
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
    /// post-commit hook running `land`.
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
    /// Tell Grimoire a change landed: close the ticket waiting on it with the
    /// decision it carries. With no ticket, the current one when it is landing.
    Land {
        #[arg(long)]
        store: Option<String>,
        /// The ticket to land, a slug or a ticket: reference. Defaults to the current ticket.
        ticket: Option<String>,
    },
    /// The tokens a shared daemon admits: add one, revoke one, or list them.
    Token {
        #[command(subcommand)]
        command: TokenCommand,
    },
    /// Sign in to a team's server, once: the token is checked with the server
    /// and kept outside any repository. Read from standard input when not given.
    Login {
        /// The server's address. Defaults to the one the project names.
        #[arg(long)]
        server: Option<String>,
        /// The token, as `grimoire token add` printed it.
        #[arg(long)]
        token: Option<String>,
    },
    /// Forget the token kept for a team's server.
    Logout {
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
}

fn target(arg: Option<&str>) -> anyhow::Result<Target> {
    match arg {
        // As init and the project file resolve it: a relative path against the
        // project's repository, or the working directory outside any project.
        Some(s) => {
            let here = cwd()?;
            let base = store_io::find_project(&here).map_or(here, |(repo, _)| repo);
            Ok(Target { store_dir: store_io::store_for_repo(s, &base)?, team: None })
        }
        None => match store_io::find_home(&cwd()?)? {
            (_, store_io::Home::Store(store_dir)) => Ok(Target { store_dir, team: None }),
            (repo, store_io::Home::Server { address, copy }) => Ok(Target { store_dir: copy.clone(), team: Some(team::Team { address, repo, copy }) }),
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
    let (store, _) = store_io::load(store_dir)?;
    match client::probe(store_dir, &store.config.repository) {
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
    let mut loaded = grimoire_server::load(store_dir)?;
    match grimoire_core::tools::call(&mut loaded.state, name, input, &store_io::today()) {
        Ok(out) => {
            if let Some(changed) = &out.changed {
                store_io::persist(store_dir, loaded.state.store(), changed)?;
                if let Some(pruned) = loaded.state.prune_baseline() {
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

/// For `init` on a store that names another repository: re-point it here
/// when that repository is gone and this checkout's `.grimoire.toml` names
/// this store, which is a checkout that moved; otherwise refuse, since a
/// second checkout of a project that still exists is a copy, not a move.
/// `held` is the store's lock, which the re-pointing needs.
fn repoint_moved_store(store_dir: &Path, recorded: &str, repo: &Path, repo_str: &str, held: Option<&store_io::StoreLock>) -> anyhow::Result<()> {
    let refuse = |why: &str| anyhow::anyhow!("store {} belongs to repository {recorded}, not {repo_str}: {why}", store_dir.display());
    if Path::new(recorded).is_dir() {
        return Err(refuse("that repository still exists, so this checkout is a copy; give it a store of its own"));
    }
    let names_this_store = repo.join(store_io::PROJECT_FILE).is_file()
        && store_io::find_project(repo).is_ok_and(|(dir, named)| dir.canonicalize().ok() == repo.canonicalize().ok() && named.canonicalize().ok() == store_dir.canonicalize().ok());
    if !names_this_store {
        return Err(refuse("that repository is gone, but this checkout's .grimoire.toml does not name this store, so it is not the same project moved"));
    }
    if held.is_none() {
        return Err(refuse("a daemon is serving the store; stop it to re-point the store here"));
    }
    let (mut store, _) = store_io::load(store_dir)?;
    store.config.repository = repo_str.to_string();
    store_io::persist(store_dir, &store, &grimoire_core::ops::Changed::Config)?;
    println!("re-pointed store {} from {recorded}, which is gone, to {repo_str}", store_dir.display());
    Ok(())
}

/// Land a ticket and say what happened, for the post-commit hook and for a
/// pipeline. A commit with no ticket waiting on it is ordinary and exits 0;
/// a landing that was refused exits 1, and so does a store that cannot be
/// reached, which a pipeline must hear of and a post-commit hook cannot act on.
fn land(target: &Target, ticket: Option<&str>) -> anyhow::Result<()> {
    let input = match ticket {
        Some(t) => json!({ "ref": format!("ticket:{}", t.strip_prefix("ticket:").unwrap_or(t)) }),
        None => json!({}),
    };
    match run_tool(target, "land_ticket", input, None)? {
        Ok(out) => {
            if let Some(note) = out["note"].as_str() {
                // On a team a commit on a machine is not the main line: nothing lands here unnamed.
                println!("nothing landed: {note}");
            } else if out["landed"].as_bool() == Some(true) {
                println!("landed {}: {}", out["node"]["ref"].as_str().unwrap_or("the ticket"), out["node"]["title"].as_str().unwrap_or(""));
                let new = out["findings"].as_array().map_or(0, Vec::len);
                if new > 0 {
                    println!("{new} new finding(s) on what it touched; run `grimoire check`");
                }
                // What a close answers with is seen here now: the nodes the ticket named and never changed.
                let untouched: Vec<&str> = out["untouched"].as_array().into_iter().flatten().filter_map(Value::as_str).collect();
                if !untouched.is_empty() {
                    println!("touched but never changed: {}", untouched.join(", "));
                }
                // On a team a ticket lands by name on the main line, which then tells the server what is fixed,
                // against the truth with the ticket applied. A store of the project's own was pruned by the write.
                if let Some(team) = &target.team {
                    let pruned = team.server().and_then(|server| team::prune_from_main_line(team, &server)).and_then(|line| line.pruned);
                    match pruned {
                        Ok(went) => println!("pruned {went} baselined finding(s) the main line no longer produces"),
                        Err(e) => {
                            eprintln!("the ticket landed, but {e}");
                            std::process::exit(1)
                        }
                    }
                }
            } else {
                println!("no ticket was waiting to land");
            }
            let waiting: Vec<String> = out["waiting"].as_array().into_iter().flatten().map(|w| format!("{} ({})", w["ref"].as_str().unwrap_or("?"), w["title"].as_str().unwrap_or(""))).collect();
            if !waiting.is_empty() {
                println!("still waiting to land: {}; land one with `grimoire land <ticket>`", waiting.join(", "));
            }
            Ok(())
        }
        Err(e) => {
            eprintln!("landing refused: {}", e["message"].as_str().unwrap_or("unknown error"));
            for d in e["details"].as_array().into_iter().flatten() {
                eprintln!("  {}: {}", d["field"].as_str().unwrap_or("?"), d["reason"].as_str().unwrap_or(""));
            }
            std::process::exit(1)
        }
    }
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

/// The folders of a store that hold one file per record.
const RECORD_DIRS: [&str; 5] = ["pages", "requirements", "tickets", store_io::STARTS_DIR, store_io::PROPOSALS_DIR];

/// The files a store directory may hold beside its record folders. Anything
/// else, or one of these of the wrong type, means the path is not only a
/// store, and deleting it would take something else with it.
const STORE_FILES: [&str; 7] = [store_io::CONFIG_FILE, store_io::BASELINE_FILE, store_io::SERVE_FILE, store_io::CURRENT_FILE, store_io::tokens::TOKENS_FILE, store_io::tokens::TOKENS_LOCK, store_io::LOCK_FILE];

/// Whether a name in a record folder is one store_io writes: `<slug>.json`,
/// or the `.<slug>.json.tmp-<pid>` a write left behind.
fn is_record_file_name(name: &str) -> bool {
    let record = |n: &str| n.strip_suffix(".json").is_some_and(grimoire_core::store::is_slug);
    record(name)
        || name.strip_prefix('.').and_then(|n| n.rsplit_once(".tmp-")).is_some_and(|(r, pid)| record(r) && !pid.is_empty() && pid.bytes().all(|b| b.is_ascii_digit()))
}

/// Why `store` may not be deleted on behalf of `repo`, if it may not: it is
/// the repository, holds it, is a root, holds anything a store does not,
/// names another repository, or a daemon is serving it.
/// `in_library`: a store a daemon serves in its machine's library is not refused
/// here, since the library is asked to let it go next (`release_from_library`).
fn refuse_store_deletion(store: &Path, repo: &Path, in_library: bool) -> anyhow::Result<Option<String>> {
    let real = store.canonicalize().with_context(|| format!("store {} does not exist", store.display()))?;
    let here = repo.canonicalize().with_context(|| format!("{} does not exist", repo.display()))?;
    if real.parent().is_none() {
        return Ok(Some(format!("{} is a filesystem root", real.display())));
    }
    if here.starts_with(&real) {
        return Ok(Some(format!("{} is the repository or holds it", real.display())));
    }
    // Each entry by its own type, a link never followed: a folder where a file belongs, or the reverse, is not the store's.
    for e in std::fs::read_dir(&real)? {
        let e = e?;
        let (name, kind) = (e.file_name().to_string_lossy().to_string(), e.file_type()?);
        let ours = if kind.is_dir() { RECORD_DIRS.contains(&name.as_str()) } else { kind.is_file() && STORE_FILES.contains(&name.as_str()) };
        if !ours {
            return Ok(Some(format!("{} holds `{name}`, which is not part of a store", real.display())));
        }
    }
    // The record folders hold records, and at most a temporary file a write left; nothing else.
    for dir in RECORD_DIRS {
        let Ok(entries) = std::fs::read_dir(real.join(dir)) else { continue };
        for e in entries {
            let e = e?;
            let name = e.file_name().to_string_lossy().to_string();
            if !e.file_type()?.is_file() || !is_record_file_name(&name) {
                return Ok(Some(format!("{} holds `{dir}/{name}`, which is not part of a store", real.display())));
            }
        }
    }
    let (s, _) = store_io::load(&real)?;
    if let Err(e) = store_io::refuse_inside(&real, Path::new(&s.config.repository)) {
        return Ok(Some(e.to_string()));
    }
    let repo_str = canonical(repo)?;
    if s.config.repository != repo_str {
        return Ok(Some(format!("store {} belongs to repository {}, not {repo_str}", real.display(), s.config.repository)));
    }
    if let Some(at) = grimoire_server::client::serving(&real, &s.config.repository) {
        if in_library && !at.path.is_empty() {
            return Ok(None);
        }
        return Ok(Some(format!("a daemon is serving store {} at {at}; stop it first", real.display())));
    }
    if real.join(store_io::SERVE_FILE).exists() {
        return Ok(Some(format!("store {} is marked as served ({}); stop its daemon, or remove the marker if none runs", real.display(), store_io::SERVE_FILE)));
    }
    Ok(None)
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

/// Whether a daemon serving this machine's library answers on `port`: then a
/// grimoire just made is served by it with nothing started.
fn library_on(port: u16) -> bool {
    matches!(client::call(port, "status", &json!({})), Ok((true, status)) if status["library"].as_array().is_some_and(|l| !l.is_empty()))
}

/// Ask a daemon serving `store` in its machine's library to let it go, so it
/// can be deleted with no daemon stopped (req:cli-uninstall). A daemon serving
/// it as its own default keeps it, and the deletion is refused.
/// The guard it returns tells the daemon, when dropped on any path out, that
/// uninstall is done with the store, so a store not deleted is served again.
fn release_from_library(store: &Path) -> anyhow::Result<Option<Released>> {
    let (s, _) = store_io::load(store)?;
    let Some(at) = client::serving(store, &s.config.repository).filter(|at| !at.path.is_empty()) else { return Ok(None) };
    let released = Released(at.addr, store.to_path_buf());
    if !client::release(at.addr, store)? {
        bail!("not deleting the store: the daemon at {} serves it as the grimoire it was started for; stop it first. Nothing was removed", at.addr);
    }
    println!("released store {} from the library of the daemon at {}", store.display(), at.addr);
    Ok(Some(released))
}

/// A store released from a daemon's library for deletion, handed back when dropped.
struct Released(std::net::SocketAddr, PathBuf);

impl Drop for Released {
    fn drop(&mut self) {
        let _ = client::release_done(self.0, &self.1);
    }
}

/// Whether `store` is what `delete_store_dir` leaves when it stops after
/// removing grimoire.json: no folder at all, or one holding nothing but the
/// lock file and empty record folders. Such a folder has no records left to
/// protect and no grimoire.json to say whose it was, so a rerun removes it.
fn deletion_leftover(store: &Path) -> anyhow::Result<bool> {
    if !store.exists() {
        return Ok(true);
    }
    if !store.is_dir() || store.join(store_io::CONFIG_FILE).exists() {
        return Ok(false);
    }
    for e in std::fs::read_dir(store)? {
        let e = e?;
        let name = e.file_name().to_string_lossy().to_string();
        let empty_records = RECORD_DIRS.contains(&name.as_str()) && e.path().is_dir() && std::fs::read_dir(e.path())?.next().is_none();
        if name != store_io::LOCK_FILE && name != store_io::tokens::TOKENS_LOCK && !empty_records {
            return Ok(false);
        }
    }
    Ok(true)
}

/// Delete a store directory its caller holds the lock of. Under the lock:
/// the records and everything else first, grimoire.json last, so a failure
/// part way leaves a store a rerun can load and finish; then the lock file
/// itself, while still held, so no daemon can open it after. Released, the
/// folder last, since a process cannot remove a folder holding a file it has
/// open. A daemon starting after the records are gone finds no grimoire.json
/// and takes no lock.
fn delete_store_dir(store: &Path, lock: Option<store_io::StoreLock>) -> anyhow::Result<()> {
    // The tokens' lock too, after the store's: a token command waits, and then finds no store.
    // There is none to take when a deletion stopped after grimoire.json went, which leaves no
    // tokens to change; any other failure to take it stops the deletion before anything is removed.
    let tokens_lock = match store_io::tokens::lock(store) {
        Ok(held) => Some(held),
        Err(store_io::StoreIoError::NotAStore(_)) => None,
        Err(e) => return Err(e.into()),
    };
    let keep_for_last = [store_io::CONFIG_FILE, store_io::LOCK_FILE, store_io::tokens::TOKENS_LOCK];
    for e in std::fs::read_dir(store)? {
        let path = e?.path();
        if path.file_name().is_some_and(|n| keep_for_last.iter().any(|k| n == *k)) {
            continue;
        }
        if path.is_dir() {
            std::fs::remove_dir_all(&path)?;
        } else {
            std::fs::remove_file(&path)?;
        }
    }
    std::fs::remove_file(store.join(store_io::CONFIG_FILE))?;
    // Released before its file goes, since a process cannot remove a file it holds open and locked.
    drop(tokens_lock);
    let tokens_lock_file = store.join(store_io::tokens::TOKENS_LOCK);
    if tokens_lock_file.exists() {
        std::fs::remove_file(&tokens_lock_file)?;
    }
    let lock_file = store.join(store_io::LOCK_FILE);
    if lock_file.exists() {
        std::fs::remove_file(&lock_file)?;
    }
    drop(lock);
    std::fs::remove_dir(store)?;
    Ok(())
}

/// Remove what init wrote. Without `--repo`, the project is the nearest
/// `.grimoire.toml` above the working directory; with it, that directory's
/// own. Every check runs before anything is removed.
fn uninstall(repo: Option<PathBuf>, delete_store: bool) -> anyhow::Result<()> {
    let (repo, store) = match repo {
        None => {
            let (dir, store) = store_io::find_project(&cwd()?)?;
            (dir, Some(store))
        }
        Some(r) => {
            let r = r.canonicalize().with_context(|| format!("{} does not exist", r.display()))?;
            if r.join(store_io::PROJECT_FILE).is_file() {
                let (dir, store) = store_io::find_project(&r)?;
                (dir, Some(store))
            } else {
                (r, None)
            }
        }
    };
    // On a project that names a server the store is the server's: what is here is this checkout's copy of it.
    let on_server = team::names_a_server(&repo);
    if on_server && delete_store {
        bail!("this project names a server, so its store is not on this machine and there is none to delete; run uninstall without --delete-store. Nothing was removed");
    }
    // The store's lock, taken once it is known to be only a store and held to
    // the end, so no daemon can start on it while it is being deleted.
    let mut lock = None;
    // What a deletion that stopped part way left, which a rerun finishes without the checks a store needs.
    let mut leftover = false;
    // A store a daemon's library let go of, handed back however uninstall ends.
    let mut _released = None;
    if delete_store {
        let Some(store) = &store else { bail!("{} names no store, so there is none to delete; nothing was removed", repo.display()) };
        if deletion_leftover(store)? {
            leftover = true;
        } else {
            if let Some(why) = refuse_store_deletion(store, &repo, true)? {
                bail!("not deleting the store: {why}. Nothing was removed");
            }
            _released = release_from_library(store)?;
            lock = Some(store_io::lock(store).map_err(|e| anyhow::anyhow!("not deleting the store: {e}. Nothing was removed"))?);
        }
    }
    let plan = init::plan_uninstall(&repo);
    let print = |u: &init::Undone| match u {
        init::Undone::Removed(p) => println!("removed {}", p.display()),
        init::Undone::Changed(p) => println!("changed {} (Grimoire's part taken out)", p.display()),
        init::Undone::Left { path, note } => println!("left    {}: {note}", path.display()),
        init::Undone::Absent(p) => println!("absent  {}", p.display()),
    };
    match plan.apply(&repo) {
        Ok(done) => done.iter().for_each(print),
        Err(stopped) => {
            stopped.done.iter().for_each(print);
            bail!("stopped part way: {}. What is listed above was done; .grimoire.toml is kept, so running uninstall again finishes the rest", stopped.error);
        }
    }
    match &store {
        Some(store) if delete_store && leftover => {
            if store.exists() {
                std::fs::remove_dir_all(store).map_err(|e| anyhow::anyhow!("the integration is removed, but not what is left of the store at {}: {e}. .grimoire.toml is kept, so running uninstall again finishes the rest", store.display()))?;
            }
            println!("removed store {} (what a deletion stopped part way had left)", store.display());
        }
        Some(store) if delete_store => {
            // Checked again at the last moment, under the lock taken before the first check.
            if let Some(why) = refuse_store_deletion(store, &repo, false)? {
                bail!("the integration is removed, but not the store: {why}. .grimoire.toml is kept, so running uninstall again finishes the rest");
            }
            delete_store_dir(store, lock.take()).map_err(|e| anyhow::anyhow!("the integration is removed, but the store was not fully deleted: {e}. .grimoire.toml is kept, so running uninstall again finishes the rest"))?;
            println!("removed store {}", store.display());
        }
        Some(copy) if on_server => println!("kept    this checkout's copy of the server's intent at {}", copy.display()),
        Some(store) => println!("kept    store {} (--delete-store removes it)", store.display()),
        None => {}
    }
    // Last, so any failure above leaves the project file naming the store for a rerun.
    let project = repo.join(store_io::PROJECT_FILE);
    if project.is_file() {
        std::fs::remove_file(&project)?;
        println!("removed {}", project.display());
    } else {
        println!("absent  {}", project.display());
    }
    Ok(())
}

fn main() -> anyhow::Result<()> {
    let cli = Cli::parse();
    tracing_subscriber::fmt().with_env_filter(tracing_subscriber::EnvFilter::try_from_default_env().unwrap_or_else(|_| "info".into())).with_target(false).init();

    match cli.command {
        Command::Serve { store, port, ui_dir, shared, listen, site, library: shared_library, site_of } => {
            let rt = tokio::runtime::Runtime::new()?;
            // A team's library: the stores under a directory, with no checkout here (req:server-library).
            if let Some(dir) = shared_library {
                let options = grimoire_server::ServeOptions { store_dir: dir.clone(), port, ui_dir, shared, listen, server: None, site, library: None, shared_library: Some(dir), site_of };
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
            rt.block_on(grimoire_server::serve(grimoire_server::ServeOptions { store_dir: target.store_dir, port, ui_dir, shared, listen, server, site, library, shared_library: None, site_of: None }))
        }
        Command::Token { command } => token(command),
        Command::Login { server, token } => team::login(server.as_deref(), token),
        Command::Logout { server } => team::logout(server.as_deref()),
        Command::Check { store, rules, json, ticket, main_line } => {
            let target = target(store.as_deref())?;
            let store_dir = target.store_dir.clone();
            // On a team, what the main line's prune came to.
            let mut pruned = None;
            let mut loaded = match &target.team {
                // The main line reads the server's intent into memory, never the copy a daemon beside it may not have
                // synced, prunes from it, and reports on that same intent, the one the prune was judged against.
                Some(team) if main_line => {
                    let line = team::prune_from_main_line(team, &team.server()?)?;
                    for e in &line.loaded.extract_errors {
                        eprintln!("warning: {e}");
                    }
                    pruned = Some(line.pruned);
                    line.loaded
                }
                // The server's intent, synced into this checkout's copy when the server answers, with this checkout's code.
                // Nothing is pruned: a finding fixed on a branch is not fixed on the main line.
                Some(team) => {
                    // A named ticket is a pipeline's gate: only the server's intent as it is now will do.
                    let opened = team::open(team, &team.server()?, ticket.is_some())?;
                    for e in &opened.loaded.extract_errors {
                        eprintln!("warning: {e}");
                    }
                    opened.loaded
                }
                None => {
                    // The baseline is pruned here only by the store's writer: with the lock
                    // taken before loading, so what is pruned is what was read under it. A
                    // daemon serving holds the lock and prunes on its own rebuilds; another
                    // writer may be mid-change; either way the check is a read, and reads.
                    let lock = store_io::lock(&store_dir).ok();
                    let mut loaded = grimoire_server::load(&store_dir)?;
                    for e in &loaded.extract_errors {
                        eprintln!("warning: {e}");
                    }
                    if lock.is_some() {
                        if let Some(changed) = loaded.state.prune_baseline() {
                            store_io::persist(&store_dir, loaded.state.store(), &changed)?;
                        }
                    }
                    loaded
                }
            };
            let adopted = loaded.state.store().baseline.is_some();
            let rules: Vec<Rule> = if rules.is_empty() {
                Rule::ALL.to_vec()
            } else {
                rules.iter().map(|r| serde_json::from_value(serde_json::Value::String(r.clone())).with_context(|| format!("unknown rule `{r}`"))).collect::<anyhow::Result<_>>()?
            };
            // On a branch the current ticket is the branch; its proposal is the intent the code is judged against.
            // A pipeline has no current ticket and names the one it checks, as it names the one it lands.
            // The main line is judged against the truth, whatever ticket this checkout is working.
            let judged = match &ticket {
                Some(t) => Some(t.trim().strip_prefix("ticket:").unwrap_or(t.trim()).to_string()),
                None if main_line => None,
                None => loaded.state.current(),
            };
            let g = match loaded.state.graph_for(judged.as_deref()) {
                Ok(g) => g,
                Err(e) => bail!("cannot check against ticket:{}: {}", judged.as_deref().unwrap_or(""), e),
            };
            let findings = lint::lint(g, &rules);
            let new = lint::new_count(&findings);
            if json {
                println!("{}", serde_json::to_string_pretty(&json!({ "findings": findings, "counts": grimoire_core::lint::counts(&findings), "new": new, "without_code": false }))?);
            } else {
                // A build that cannot read part of the project explains the rest,
                // so it comes first. Then the new findings; inherited ones are
                // counted, not listed: the list is the work this commit is
                // answerable for.
                let unsupported = |f: &&lint::Finding| f.check == "unsupported-language";
                for f in findings.iter().filter(unsupported) {
                    println!("{:<11} {}  {}", f.rule.name(), f.r#ref, f.reason);
                }
                for f in findings.iter().filter(|f| !f.baselined && !unsupported(f)) {
                    println!("{:<11} {}  {}", f.rule.name(), f.r#ref, f.reason);
                }
                let counts = grimoire_core::lint::counts(&findings);
                let mut line = format!("{} finding(s): {}", findings.len(), counts.iter().map(|(k, v)| format!("{v} {k}")).collect::<Vec<_>>().join(", "));
                if adopted {
                    line.push_str(&format!("; {} baselined, {new} new", findings.len() - new));
                }
                println!("{line}");
            }
            // What the main line told a team's server; a store of the project's own was pruned above.
            let mut failed = new > 0;
            if let Some(pruned) = pruned {
                match pruned {
                    // Kept off standard output under --json, which is the findings alone.
                    Ok(went) if json => eprintln!("pruned {went} baselined finding(s) the main line no longer produces"),
                    Ok(went) => println!("pruned {went} baselined finding(s) the main line no longer produces"),
                    Err(e) => {
                        eprintln!("{e}");
                        failed = true;
                    }
                }
            }
            if failed {
                std::process::exit(1)
            }
            Ok(())
        }
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
        Command::Init { repo, store, server, languages, port, name, empty } => {
            let languages: Vec<String> = languages.iter().map(|l| l.trim().to_string()).collect();
            let repo = repo.unwrap_or(cwd()?);
            let repo_str = canonical(&repo)?;
            // The grimoire's name: the one given, else the one a team's server's address carries in
            // /g/<name>, else the one a project file here already holds, else the folder's (req:project-name).
            let from_server = server.as_deref().and_then(|s| store_io::server_address(s).ok()).as_deref().and_then(store_io::server_grimoire);
            let name = match name {
                Some(n) if grimoire_core::store::is_slug(&n) => n,
                Some(n) => bail!("the name `{n}` is not a slug: lowercase letters and digits joined by single dashes"),
                None if from_server.is_some() => from_server.unwrap_or_default(),
                None => match store_io::project_at(&repo) {
                    Ok(Some((_, n))) => n,
                    _ => store_io::default_name(&repo),
                },
            };
            // An address that is not one stops init before anything is written.
            let server = server.as_deref().map(store_io::server_address).transpose().map_err(anyhow::Error::msg)?;
            // Everything init will write, worked out first: a file it cannot merge into stops it before the store or .grimoire.toml.
            let integration = init::plan_install(&repo, port, &name)?;
            // A team's server holds the grimoire: the project file names it, and no store is made here.
            if let Some(address) = server {
                let f = store_io::write_project_file_for_server(&repo, &address, &name)?;
                println!("wrote   {}", f.display());
                for w in integration.apply()? {
                    match w {
                        init::Wrote::Written(p) => println!("wrote   {}", p.display()),
                        init::Wrote::Noted { path, note } => println!("wrote   {}: {note}", path.display()),
                        init::Wrote::Skipped { path, note } => println!("skipped {}: {note}", path.display()),
                    }
                }
                let daemon = match library_on(port) {
                    true => format!("the daemon serving this machine's library on {port} serves it at http://127.0.0.1:{port}/g/{name}/ once its first check has made this checkout's copy"),
                    false => format!("then start the daemon with `grimoire serve --port {port}`"),
                };
                println!("done. This project's grimoire is the server's at {address}: sign in once with `grimoire login`; {daemon}; agents get the tools from .mcp.json (Claude Code) or .codex/config.toml (Codex), and the sequences from the skill.");
                return Ok(());
            }
            let Some(store) = store else { bail!("init takes --store or --server") };
            // Resolved as the project file it writes will resolve it, a relative path against the repository.
            let store_dir = store_io::store_for_repo(&store, &repo)?;
            store_io::refuse_inside(&store_dir, &repo)?;
            // The store's lock for the whole run, from creating the store to adopting, unless
            // a daemon serves it, which then takes init's one write, the adopt, itself.
            let created = !store_dir.join(store_io::CONFIG_FILE).exists();
            let held = if created && empty {
                let empty = grimoire_core::Store::new(grimoire_core::StoreConfig { site: vec![],
                    version: grimoire_core::StoreConfig::CURRENT_VERSION,
                    repository: repo_str.clone(),
                    // Both roots named, with no page yet: the check says which pages to put.
                    roots: grimoire_core::model::Roots::new(scaffold::ROOT_SLUG, scaffold::RITES_SLUG),
                    languages: languages.clone(),
                    scope: grimoire_core::model::Scope::default(),
                    order: Default::default(),
                });
                let lock = write_whole_store(&store_dir, &empty, false)?;
                println!("created empty store at {}", store_dir.display());
                Some(lock)
            } else if created {
                // A store init makes starts as the skeleton scaffold would write, so it has its roots and story pages (req:init-scaffolds).
                let (skeleton, extraction) = skeleton(&repo, &repo_str, &languages)?;
                let lock = write_whole_store(&store_dir, &skeleton, false)?;
                report_skeleton(&skeleton, &extraction, &store_dir);
                Some(lock)
            } else {
                let (first, _) = store_io::load(&store_dir)?;
                let lock = match client::serving(&store_dir, &first.config.repository) {
                    Some(_) => None,
                    None => Some(store_io::lock(&store_dir)?),
                };
                // Whose store it is, read again under the lock, so it cannot be replaced between the check and the writes.
                let repository = if lock.is_some() { store_io::load(&store_dir)?.0.config.repository } else { first.config.repository };
                // The same folder spelt differently (case, a link) is this checkout, not a move.
                let same_checkout = Path::new(&repository).canonicalize().ok().zip(repo.canonicalize().ok()).is_some_and(|(a, b)| a == b);
                if repository != repo_str && !same_checkout {
                    repoint_moved_store(&store_dir, &repository, &repo, &repo_str, lock.as_ref())?;
                }
                lock
            };
            let f = store_io::write_project_file(&repo, &store, &name)?;
            println!("wrote   {}", f.display());
            for w in integration.apply()? {
                match w {
                    init::Wrote::Written(p) => println!("wrote   {}", p.display()),
                    init::Wrote::Noted { path, note } => println!("wrote   {}: {note}", path.display()),
                    init::Wrote::Skipped { path, note } => println!("skipped {}: {note}", path.display()),
                }
            }
            // A store with no baseline yet, one init made or scaffold did, starts from what is there;
            // a store that has one keeps it, so a re-run or a move never hides what is new since.
            if created || !store_dir.join(store_io::BASELINE_FILE).exists() {
                adopt(&Target { store_dir: store_dir.clone(), team: None }, "adopted at init", held.as_ref())?;
            } else {
                println!("kept    the store's baseline; to accept what is there now, run `grimoire adopt --reason <why>`");
            }
            let daemon = match library_on(port) {
                true => format!("The daemon serving this machine's library on {port} serves it within seconds at http://127.0.0.1:{port}/g/{name}/"),
                false => format!("Start the daemon with `grimoire serve --port {port}`"),
            };
            println!("done. {daemon}; agents get the tools from .mcp.json (Claude Code) or .codex/config.toml (Codex), and the sequences from the skill.");
            Ok(())
        }
        Command::Uninstall { repo, delete_store } => uninstall(repo, delete_store),
        Command::Adopt { store, reason } => adopt(&target(store.as_deref())?, &reason, None),
        Command::Land { store, ticket } => land(&target(store.as_deref())?, ticket.as_deref()),
        Command::Tool { store, name, input } => {
            let target = target(store.as_deref())?;
            let input: Value = match input {
                Some(s) => serde_json::from_str(&s).context("input is not JSON")?,
                None => json!({}),
            };
            match run_tool(&target, &name, input, None)? {
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

#[cfg(test)]
mod tests {
    use grimoire_server::store_io::slashed;

    /// # Test contract
    ///
    /// ```json
    /// {"test": "a-canonical-path-is-recorded-absolute", "checks": ["crates/cli/src#init-repoints", "crates/core/src/store#config-rules"]}
    /// ```
    #[test]
    fn a_canonical_path_is_recorded_absolute() {
        for (canonical, recorded) in [(r"\\?\C:\work\app", "C:/work/app"), (r"\\?\UNC\server\share\app", "//server/share/app"), ("/home/me/app", "/home/me/app")] {
            assert_eq!(slashed(canonical), recorded);
            assert!(grimoire_core::store::is_absolute_path(recorded), "{recorded} is what the config accepts");
        }
    }
}
