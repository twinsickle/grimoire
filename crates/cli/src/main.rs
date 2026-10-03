//! # Module contract
//!
//! ```json
//! {"purpose":"Parse the command line and dispatch to serve, check, adopt, scaffold, init, uninstall or tool.","criteria":[{"id":"check-without-daemon","statement":"The check command runs the lints without a daemon, for hooks and CI, exits non-zero only when there are findings outside the baseline, and prunes the baseline itself when no daemon serves the store."},{"id":"init-adopts","statement":"init ends by adopting, with the reason adopted at init, when the store has no baseline yet, one it created or one scaffold made, so the hook it installs passes on the next commit, and prints how many findings it baselined; on a store that already has a baseline, a re-run or a moved checkout, it keeps that baseline as it is and says so."},{"id":"unsupported-first","statement":"check prints a finding that the build cannot extract a configured language before every other line, baselined or not, so the reason an old binary fails is the first thing read."},{"id":"writes-through-daemon","statement":"tool, adopt and init run a tool on the daemon serving the store when there is one, and against the store directly only when there is not."},{"id":"store-resolution","statement":"Every command finds its store from --store, else from the nearest .grimoire.toml above the working directory; a --store is resolved as the project file is, a relative path against the project's repository (the working directory outside any project) and a bare name under the stores directory."},{"id":"repository-untouched","statement":"Only init writes into a repository, and only the integration: .grimoire.toml, the agent skill and its references for each harness, a marked section in CLAUDE.md and AGENTS.md, the MCP entry in .mcp.json and .codex/config.toml, and a pre-commit hook; only uninstall takes it out again. Every other command reads it."},{"id":"uninstall-store","statement":"uninstall finds the project from the nearest .grimoire.toml above the working directory, or the --repo directory's own, refusing a malformed one; it keeps the store unless asked, printing where it is, and removes .grimoire.toml last of all, after the store when it deletes one, so a failure part way leaves it for a rerun. With --delete-store it removes the store only when the store is not the repository, an ancestor of it or a root, holds nothing a store does not, its record folders included, names this repository, and is neither answered for by a daemon nor marked as served, checking all before removing anything and again just before the deletion, refusing a store inside its repository, and holding the store's lock from the first check: under it the records go first and grimoire.json last, so a store stopped part way still loads for a rerun, then the lock file, and the folder once the lock is released. What a deletion stopped after grimoire.json went leaves, no folder or one holding only the lock file and empty record folders, a rerun removes and finishes."},{"id":"takes-the-lock","statement":"A command that writes the store with no daemon serving it takes the store's lock first and holds it to the end of its run: tool with a write tool and adopt for the call, init from creating or opening the store to adopting, scaffold while it writes the store, and uninstall --delete-store from its first check to the store's removal; refused, it writes nothing. Reads take no lock, and check, a read, takes the lock before it loads and prunes the baseline only when it got it."},{"id":"init-repoints","statement":"init on a store that records another repository re-points it to this one, under the store's lock, saying it did, only when no folder is left at the recorded repository and this checkout's .grimoire.toml names this store; when the recorded repository still exists, or the project file names another store, it refuses with the reason and changes nothing. The same folder spelt differently, by case or through a link, is this checkout, not a move; the repository is recorded as its canonical path, a share as //server/share."}]}
//! ```

mod init;

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
    /// AGENTS.md, the MCP entry and a pre-commit hook running `check`.
    Init {
        #[arg(long)]
        repo: Option<PathBuf>,
        /// Store path or name. Created empty if it does not exist.
        #[arg(long)]
        store: String,
        #[arg(long, value_delimiter = ',', default_value = "rust")]
        languages: Vec<String>,
        /// The port the daemon will serve on; written into .mcp.json.
        #[arg(long, default_value_t = 4242)]
        port: u16,
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

fn cwd() -> anyhow::Result<PathBuf> {
    Ok(std::env::current_dir()?)
}

fn store_dir(arg: Option<&str>) -> anyhow::Result<PathBuf> {
    match arg {
        // As init and the project file resolve it: a relative path against the
        // project's repository, or the working directory outside any project.
        Some(s) => {
            let here = cwd()?;
            let base = store_io::find_project(&here).map_or(here, |(repo, _)| repo);
            Ok(store_io::store_for_repo(s, &base)?)
        }
        None => {
            let (_, store) = store_io::find_project(&cwd()?)?;
            Ok(store)
        }
    }
}

/// Run one tool against the store. When a daemon serves it the call goes to
/// the daemon, which stays the store's only writer; otherwise it runs here
/// and any write is persisted, with the baseline pruned as the daemon would.
/// The inner error is the tool's error object.
fn run_tool(store_dir: &Path, name: &str, input: Value, held: Option<&store_io::StoreLock>) -> anyhow::Result<Result<Value, Value>> {
    let (store, _) = store_io::load(store_dir)?;
    if let Some(port) = client::serving(store_dir, &store.config.repository) {
        let (ok, v) = client::call(port, name, &input)?;
        return Ok(if ok { Ok(v) } else { Err(v) });
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

/// Adopt and say what it did, for `adopt` and the end of `init`.
fn adopt(store_dir: &Path, reason: &str, held: Option<&store_io::StoreLock>) -> anyhow::Result<()> {
    match run_tool(store_dir, "adopt", json!({ "reason": reason }), held)? {
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

fn canonical(p: &Path) -> anyhow::Result<String> {
    let c = p.canonicalize().with_context(|| format!("{} does not exist", p.display()))?;
    Ok(slashed(&c.to_string_lossy()))
}

/// A canonical path as the store records it: `/`-separated, without the
/// `\\?\` prefix Windows canonical paths carry, and a share, which comes back
/// as `\\?\UNC\server\share`, as `//server/share`.
fn slashed(canonical: &str) -> String {
    let s = match canonical.strip_prefix(r"\\?\UNC\") {
        Some(share) => format!(r"\\{share}"),
        None => canonical.strip_prefix(r"\\?\").unwrap_or(canonical).to_string(),
    };
    s.replace('\\', "/")
}

/// The folders of a store that hold one file per record.
const RECORD_DIRS: [&str; 4] = ["pages", "requirements", "tickets", store_io::STARTS_DIR];

/// What a store directory may hold. Anything else means the path is not
/// only a store, and deleting it would take something else with it.
const STORE_ENTRIES: [&str; 8] = [store_io::CONFIG_FILE, store_io::BASELINE_FILE, store_io::SERVE_FILE, store_io::STARTS_DIR, store_io::LOCK_FILE, "pages", "requirements", "tickets"];

/// Why `store` may not be deleted on behalf of `repo`, if it may not: it is
/// the repository, holds it, is a root, holds anything a store does not,
/// names another repository, or a daemon is serving it.
fn refuse_store_deletion(store: &Path, repo: &Path) -> anyhow::Result<Option<String>> {
    let real = store.canonicalize().with_context(|| format!("store {} does not exist", store.display()))?;
    let here = repo.canonicalize().with_context(|| format!("{} does not exist", repo.display()))?;
    if real.parent().is_none() {
        return Ok(Some(format!("{} is a filesystem root", real.display())));
    }
    if here.starts_with(&real) {
        return Ok(Some(format!("{} is the repository or holds it", real.display())));
    }
    for e in std::fs::read_dir(&real)? {
        let name = e?.file_name().to_string_lossy().to_string();
        if !STORE_ENTRIES.contains(&name.as_str()) {
            return Ok(Some(format!("{} holds `{name}`, which is not part of a store", real.display())));
        }
    }
    // The record folders hold records, and at most a temporary file a write left; nothing else.
    for dir in RECORD_DIRS {
        let Ok(entries) = std::fs::read_dir(real.join(dir)) else { continue };
        for e in entries {
            let e = e?;
            let name = e.file_name().to_string_lossy().to_string();
            if !e.path().is_file() || !(name.ends_with(".json") || (name.starts_with('.') && name.contains(".tmp-"))) {
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
    if let Some(port) = grimoire_server::client::serving(&real, &s.config.repository) {
        return Ok(Some(format!("a daemon is serving store {} on port {port}; stop it first", real.display())));
    }
    if real.join(store_io::SERVE_FILE).exists() {
        return Ok(Some(format!("store {} is marked as served ({}); stop its daemon, or remove the marker if none runs", real.display(), store_io::SERVE_FILE)));
    }
    Ok(None)
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
        if name != store_io::LOCK_FILE && !empty_records {
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
    let keep_for_last = [store_io::CONFIG_FILE, store_io::LOCK_FILE];
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
    // The store's lock, taken once it is known to be only a store and held to
    // the end, so no daemon can start on it while it is being deleted.
    let mut lock = None;
    // What a deletion that stopped part way left, which a rerun finishes without the checks a store needs.
    let mut leftover = false;
    if delete_store {
        let Some(store) = &store else { bail!("{} names no store, so there is none to delete; nothing was removed", repo.display()) };
        if deletion_leftover(store)? {
            leftover = true;
        } else {
            if let Some(why) = refuse_store_deletion(store, &repo)? {
                bail!("not deleting the store: {why}. Nothing was removed");
            }
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
            if let Some(why) = refuse_store_deletion(store, &repo)? {
                bail!("the integration is removed, but not the store: {why}. .grimoire.toml is kept, so running uninstall again finishes the rest");
            }
            delete_store_dir(store, lock.take()).map_err(|e| anyhow::anyhow!("the integration is removed, but the store was not fully deleted: {e}. .grimoire.toml is kept, so running uninstall again finishes the rest"))?;
            println!("removed store {}", store.display());
        }
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
        Command::Serve { store, port, ui_dir } => {
            let store_dir = store_dir(store.as_deref())?;
            let rt = tokio::runtime::Runtime::new()?;
            rt.block_on(grimoire_server::serve(grimoire_server::ServeOptions { store_dir, port, ui_dir }))
        }
        Command::Check { store, rules, json } => {
            let store_dir = store_dir(store.as_deref())?;
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
            drop(lock);
            let adopted = loaded.state.store().baseline.is_some();
            let rules: Vec<Rule> = if rules.is_empty() {
                Rule::ALL.to_vec()
            } else {
                rules.iter().map(|r| serde_json::from_value(serde_json::Value::String(r.clone())).with_context(|| format!("unknown rule `{r}`"))).collect::<anyhow::Result<_>>()?
            };
            // On a branch the current ticket is the branch; its proposal is the intent the code is judged against.
            let current = loaded.state.current();
            let g = loaded.state.graph_for(current.as_deref())?;
            let findings = lint::lint(g, &rules);
            let new = lint::new_count(&findings);
            if json {
                println!("{}", serde_json::to_string_pretty(&json!({ "findings": findings, "counts": grimoire_core::lint::counts(&findings), "new": new }))?);
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
            if new == 0 {
                Ok(())
            } else {
                std::process::exit(1)
            }
        }
        Command::Scaffold { repo, store, languages, force } => {
            let languages: Vec<String> = languages.iter().map(|l| l.trim().to_string()).collect();
            let repo = repo.unwrap_or(cwd()?);
            let repo_str = canonical(&repo)?;
            // Resolved as the project file would resolve it, a relative path against the repository.
            let store_dir = store_io::store_for_repo(&store, &repo)?;
            store_io::refuse_inside(&store_dir, &repo)?;
            let extractor = Extractor::for_languages(&repo, &languages);
            let (files, errors) = extractor.extract_all()?;
            for e in &errors {
                eprintln!("warning: {e}");
            }
            let extraction = Extraction::from_files(&files);
            let skeleton = scaffold::scaffold(&extraction, &repo_str, &languages);
            write_whole_store(&store_dir, &skeleton, force)?;
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
            Ok(())
        }
        Command::Init { repo, store, languages, port } => {
            let languages: Vec<String> = languages.iter().map(|l| l.trim().to_string()).collect();
            let repo = repo.unwrap_or(cwd()?);
            let repo_str = canonical(&repo)?;
            // Everything init will write, worked out first: a file it cannot merge into stops it before the store or .grimoire.toml.
            let integration = init::plan_install(&repo, port)?;
            // Resolved as the project file it writes will resolve it, a relative path against the repository.
            let store_dir = store_io::store_for_repo(&store, &repo)?;
            store_io::refuse_inside(&store_dir, &repo)?;
            // The store's lock for the whole run, from creating the store to adopting, unless
            // a daemon serves it, which then takes init's one write, the adopt, itself.
            let created = !store_dir.join(store_io::CONFIG_FILE).exists();
            let held = if created {
                let empty = grimoire_core::Store::new(grimoire_core::StoreConfig {
                    version: grimoire_core::StoreConfig::CURRENT_VERSION,
                    repository: repo_str.clone(),
                    roots: vec![scaffold::ROOT_SLUG.to_string()],
                    languages,
                    scope: grimoire_core::model::Scope::default(),
                    order: Default::default(),
                });
                let lock = write_whole_store(&store_dir, &empty, false)?;
                println!("created empty store at {}", store_dir.display());
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
            let f = store_io::write_project_file(&repo, &store)?;
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
                adopt(&store_dir, "adopted at init", held.as_ref())?;
            } else {
                println!("kept    the store's baseline; to accept what is there now, run `grimoire adopt --reason <why>`");
            }
            println!("done. Start the daemon with `grimoire serve --port {port}`; agents get the tools from .mcp.json (Claude Code) or .codex/config.toml (Codex), and the sequences from the skill.");
            Ok(())
        }
        Command::Uninstall { repo, delete_store } => uninstall(repo, delete_store),
        Command::Adopt { store, reason } => adopt(&store_dir(store.as_deref())?, &reason, None),
        Command::Tool { store, name, input } => {
            let store_dir = store_dir(store.as_deref())?;
            let input: Value = match input {
                Some(s) => serde_json::from_str(&s).context("input is not JSON")?,
                None => json!({}),
            };
            match run_tool(&store_dir, &name, input, None)? {
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
    use super::slashed;

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
