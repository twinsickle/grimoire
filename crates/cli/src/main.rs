//! # Module contract
//!
//! ```json
//! {"purpose":"Parse the command line and dispatch to serve, check, scaffold or init.","criteria":[{"id":"check-without-daemon","statement":"The check command runs the lints without a daemon, for hooks and CI, and exits non-zero when there are findings."},{"id":"store-resolution","statement":"Every command finds its store from --store (a path or a name), else from the nearest .grimoire.toml above the working directory."},{"id":"repository-untouched","statement":"Only init writes into a repository, and only .grimoire.toml."}]}
//! ```

use std::path::{Path, PathBuf};

use anyhow::{bail, Context};
use clap::{Parser, Subcommand};
use grimoire_core::lint::{self, Rule};
use grimoire_core::scaffold;
use grimoire_core::Extraction;
use grimoire_extract::Extractor;
use grimoire_server::store_io;

#[derive(Parser)]
#[command(name = "grimoire", version, about = "A manual and ticketing tool for software projects: one graph of intent, requirements, module contracts and code, served to people and agents.")]
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
        /// Only these rules: dangling, uncovered, malformed.
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
    /// Point a repository at a store: write .grimoire.toml, creating the store if needed.
    Init {
        #[arg(long)]
        repo: Option<PathBuf>,
        /// Store path or name.
        #[arg(long)]
        store: String,
        #[arg(long, value_delimiter = ',', default_value = "rust")]
        languages: Vec<String>,
    },
    /// Run one tool against the store without a daemon and print its output.
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

fn canonical(p: &Path) -> anyhow::Result<String> {
    let c = p.canonicalize().with_context(|| format!("{} does not exist", p.display()))?;
    let s = c.to_string_lossy().to_string();
    // Windows canonical paths carry a \\?\ prefix that nothing else wants.
    Ok(s.strip_prefix(r"\\?\").unwrap_or(&s).replace('\\', "/"))
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
            let rules: Vec<Rule> = if rules.is_empty() {
                Rule::ALL.to_vec()
            } else {
                rules.iter().map(|r| serde_json::from_value(serde_json::Value::String(r.clone())).with_context(|| format!("unknown rule `{r}`"))).collect::<anyhow::Result<_>>()?
            };
            let g = loaded.state.graph();
            let findings = lint::lint(g, &rules);
            if json {
                println!("{}", serde_json::to_string_pretty(&serde_json::json!({ "findings": findings, "counts": grimoire_core::query::counts(&findings) }))?);
            } else {
                for f in &findings {
                    println!("{:<9} {}  {}", format!("{:?}", f.rule).to_lowercase(), f.r#ref, f.reason);
                }
                let counts = grimoire_core::query::counts(&findings);
                println!("{} finding(s): {}", findings.len(), counts.iter().map(|(k, v)| format!("{v} {k}")).collect::<Vec<_>>().join(", "));
            }
            if findings.is_empty() {
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
        Command::Init { repo, store, languages } => {
            let repo = repo.unwrap_or(cwd()?);
            let repo_str = canonical(&repo)?;
            let store_dir = store_io::resolve_store(&store)?;
            if !store_dir.join(store_io::CONFIG_FILE).exists() {
                let empty = grimoire_core::Store::new(grimoire_core::StoreConfig {
                    version: grimoire_core::StoreConfig::CURRENT_VERSION,
                    repository: repo_str.clone(),
                    roots: vec![scaffold::ROOT_SLUG.to_string()],
                    languages,
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
            println!("wrote {}", f.display());
            Ok(())
        }
        Command::Tool { store, name, input } => {
            let store_dir = store_dir(store.as_deref())?;
            let mut loaded = grimoire_server::load(&store_dir)?;
            let input: serde_json::Value = match input {
                Some(s) => serde_json::from_str(&s).context("input is not JSON")?,
                None => serde_json::json!({}),
            };
            match grimoire_core::tools::call(&mut loaded.state, &name, input, &store_io::today()) {
                Ok(out) => {
                    if let Some(changed) = &out.changed {
                        store_io::persist(&store_dir, loaded.state.store(), changed)?;
                    }
                    println!("{}", serde_json::to_string_pretty(&out.output)?);
                    Ok(())
                }
                Err(e) => {
                    println!("{}", serde_json::to_string_pretty(&serde_json::to_value(&e)?)?);
                    std::process::exit(1)
                }
            }
        }
    }
}
