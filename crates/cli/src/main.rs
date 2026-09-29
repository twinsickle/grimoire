//! # Module contract
//!
//! ```json
//! {"purpose":"Parse the command line and dispatch to serve, check, adopt, scaffold, init, uninstall or tool.","criteria":[{"id":"check-without-daemon","statement":"The check command runs the lints without a daemon, for hooks and CI, exits non-zero only when there are findings outside the baseline, and prunes the baseline itself when no daemon serves the store."},{"id":"init-adopts","statement":"init ends by adopting with the reason adopted at init, so the hook it installs passes on the next commit, and prints how many findings it baselined."},{"id":"unsupported-first","statement":"check prints a finding that the build cannot extract a configured language before every other line, baselined or not, so the reason an old binary fails is the first thing read."},{"id":"writes-through-daemon","statement":"tool, adopt and init run a tool on the daemon serving the store when there is one, and against the store directly only when there is not."},{"id":"store-resolution","statement":"Every command finds its store from --store (a path or a name), else from the nearest .grimoire.toml above the working directory."},{"id":"repository-untouched","statement":"Only init writes into a repository, and only the integration: .grimoire.toml, the agent skill and its references, a marked section in CLAUDE.md and AGENTS.md, the MCP entry and a pre-commit hook; only uninstall takes it out again. Every other command reads it."},{"id":"uninstall-store","statement":"uninstall finds the project from the nearest .grimoire.toml above the working directory, or the --repo directory's own, refusing a malformed one; it removes .grimoire.toml last and keeps the store, printing where it is. With --delete-store it removes the store only when the store is not the repository, an ancestor of it or a root, holds nothing a store does not, names this repository, and is neither answered for by a daemon nor marked as served, checking all before removing anything and again just before the deletion."}]}
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
        Some(s) => Ok(store_io::resolve_store(s)?),
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
fn run_tool(store_dir: &Path, name: &str, input: Value) -> anyhow::Result<Result<Value, Value>> {
    let (store, _) = store_io::load(store_dir)?;
    if let Some(port) = client::serving(store_dir, &store.config.repository) {
        let (ok, v) = client::call(port, name, &input)?;
        return Ok(if ok { Ok(v) } else { Err(v) });
    }
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

/// Adopt and say what it did, for `adopt` and the end of `init`.
fn adopt(store_dir: &Path, reason: &str) -> anyhow::Result<()> {
    match run_tool(store_dir, "adopt", json!({ "reason": reason }))? {
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
    let s = c.to_string_lossy().to_string();
    // Windows canonical paths carry a \\?\ prefix that nothing else wants.
    Ok(s.strip_prefix(r"\\?\").unwrap_or(&s).replace('\\', "/"))
}

/// What a store directory may hold. Anything else means the path is not
/// only a store, and deleting it would take something else with it.
const STORE_ENTRIES: [&str; 7] = [store_io::CONFIG_FILE, store_io::BASELINE_FILE, store_io::SERVE_FILE, store_io::STARTS_DIR, "pages", "requirements", "tickets"];

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
    let (s, _) = store_io::load(&real)?;
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
    if delete_store {
        let Some(store) = &store else { bail!("{} names no store, so there is none to delete; nothing was removed", repo.display()) };
        if let Some(why) = refuse_store_deletion(store, &repo)? {
            bail!("not deleting the store: {why}. Nothing was removed");
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
    let project = repo.join(store_io::PROJECT_FILE);
    if project.is_file() {
        std::fs::remove_file(&project)?;
        println!("removed {}", project.display());
    } else {
        println!("absent  {}", project.display());
    }
    match store {
        Some(store) if delete_store => {
            // Checked again at the last moment: a daemon may have started since.
            if let Some(why) = refuse_store_deletion(&store, &repo)? {
                bail!("the integration is removed, but not the store: {why}");
            }
            std::fs::remove_dir_all(&store)?;
            println!("removed store {}", store.display());
        }
        Some(store) => println!("kept    store {} (--delete-store removes it)", store.display()),
        None => {}
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
            let mut loaded = grimoire_server::load(&store_dir)?;
            for e in &loaded.extract_errors {
                eprintln!("warning: {e}");
            }
            // A running daemon prunes the baseline on its own rebuilds and owns
            // the store; without one, this command does it.
            if client::serving(&store_dir, &loaded.state.store().config.repository).is_none() {
                if let Some(changed) = loaded.state.prune_baseline() {
                    store_io::persist(&store_dir, loaded.state.store(), &changed)?;
                }
            }
            let adopted = loaded.state.store().baseline.is_some();
            let rules: Vec<Rule> = if rules.is_empty() {
                Rule::ALL.to_vec()
            } else {
                rules.iter().map(|r| serde_json::from_value(serde_json::Value::String(r.clone())).with_context(|| format!("unknown rule `{r}`"))).collect::<anyhow::Result<_>>()?
            };
            let g = loaded.state.graph();
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
            let repo = repo.unwrap_or(cwd()?);
            let repo_str = canonical(&repo)?;
            let store_dir = store_io::resolve_store(&store)?;
            let extractor = Extractor::for_languages(&repo, &languages);
            let (files, errors) = extractor.extract_all()?;
            for e in &errors {
                eprintln!("warning: {e}");
            }
            let extraction = Extraction::from_files(&files);
            let skeleton = scaffold::scaffold(&extraction, &repo_str, &languages);
            store_io::write_all(&store_dir, &skeleton, force)?;
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
            let repo = repo.unwrap_or(cwd()?);
            let repo_str = canonical(&repo)?;
            let store_dir = store_io::resolve_store(&store)?;
            if !store_dir.join(store_io::CONFIG_FILE).exists() {
                let empty = grimoire_core::Store::new(grimoire_core::StoreConfig {
                    version: grimoire_core::StoreConfig::CURRENT_VERSION,
                    repository: repo_str.clone(),
                    roots: vec![scaffold::ROOT_SLUG.to_string()],
                    languages,
                    scope: grimoire_core::model::Scope::default(),
                    order: Default::default(),
                });
                store_io::write_all(&store_dir, &empty, false)?;
                println!("created empty store at {}", store_dir.display());
            } else {
                let (existing, _) = store_io::load(&store_dir)?;
                if existing.config.repository != repo_str {
                    bail!("store {} belongs to repository {}, not {}", store_dir.display(), existing.config.repository, repo_str);
                }
            }
            let f = store_io::write_project_file(&repo, &store)?;
            println!("wrote   {}", f.display());
            for w in init::install(&repo, port)? {
                match w {
                    init::Wrote::Written(p) => println!("wrote   {}", p.display()),
                    init::Wrote::Skipped { path, note } => println!("skipped {}: {note}", path.display()),
                }
            }
            adopt(&store_dir, "adopted at init")?;
            println!("done. Start the daemon with `grimoire serve --port {port}`; agents get the tools from .mcp.json and the sequences from the skill.");
            Ok(())
        }
        Command::Uninstall { repo, delete_store } => uninstall(repo, delete_store),
        Command::Adopt { store, reason } => adopt(&store_dir(store.as_deref())?, &reason),
        Command::Tool { store, name, input } => {
            let store_dir = store_dir(store.as_deref())?;
            let input: Value = match input {
                Some(s) => serde_json::from_str(&s).context("input is not JSON")?,
                None => json!({}),
            };
            match run_tool(&store_dir, &name, input)? {
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
