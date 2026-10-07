//! # Module contract
//!
//! ```json
//! {"purpose":"Carry out the init command: name the grimoire, make or open its store under the store's lock, re-pointing it or adding this checkout, or name a team's server in its place, then write the project file, install the integration and adopt what the store holds.","criteria":[{"id":"init-adopts","statement":"init ends by adopting, with the reason adopted at init, when the store has no baseline yet, one it created or one scaffold made, so the hook it installs passes on the next commit, and prints how many findings it baselined; on a store that already has a baseline, a re-run or a moved checkout, it keeps that baseline as it is and says so."},{"id":"init-repoints","statement":"init on a store that records another repository re-points it to this one, under the store's lock, saying it did, only when no folder is left at the recorded repository and this checkout's .grimoire.toml names this store; when the recorded repository still exists and this checkout's .grimoire.toml names this store, it adds this checkout to the store's checkouts instead, itself under the lock or through the daemon serving the store, saying it did; when the project file names another store, it refuses with the reason and changes nothing. The same folder spelt differently, by case or through a link, is this checkout, not a move; the repository is recorded as its canonical path, a share as //server/share."},{"id":"init-server","statement":"init with --server writes .grimoire.toml naming the server, as its address is kept, and installs the integration, creating no store and adopting nothing; it takes --store or --server and never both, and refuses an address that is not one before writing anything. The grimoire it names is the one --name gives, else the one the address carries in /g/<name>, else the repository folder's."},{"id":"init-scaffolds","statement":"init, creating a store, writes into it the skeleton scaffold would write for the repository in the languages given, or with --empty a store whose roots are named and which holds no page, and says what it made, before adopting; a store that already exists is left as it is, and init with --server makes no store."}]}
//! ```

use std::path::{Path, PathBuf};

use anyhow::bail;
use grimoire_core::scaffold;
use grimoire_server::client;
use grimoire_server::store_io::{self, deletions};
use serde_json::json;

use super::{plan_install, Wrote};
use crate::{adopt, canonical, checkout_view, cwd, report_skeleton, run_tool, skeleton, uninstall, write_whole_store, Target};

/// Run `init`: a project naming a team's server, or one with a store of its own on this machine,
/// made as the skeleton or empty, or taken up again, and adopted when it has no baseline yet.
pub fn run(repo: Option<PathBuf>, store: Option<String>, server: Option<String>, languages: Vec<String>, port: u16, name: Option<String>, empty: bool) -> anyhow::Result<()> {
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
    let integration = plan_install(&repo, port, &name)?;
    // A team's server holds the grimoire: the project file names it, and no store is made here.
    if let Some(address) = server {
        let f = store_io::write_project_file_for_server(&repo, &address, &name)?;
        println!("wrote   {}", f.display());
        for w in integration.apply()? {
            match w {
                Wrote::Written(p) => println!("wrote   {}", p.display()),
                Wrote::Noted { path, note } => println!("wrote   {}: {note}", path.display()),
                Wrote::Skipped { path, note } => println!("skipped {}: {note}", path.display()),
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
    // Taken up again: a deletion queued for this store is cancelled before anything is written (req:store-deletion-queued).
    if deletions::cancel(&deletions::deletions_dir()?, &store_dir)? {
        println!("cancelled the queued deletion of store {}", store_dir.display());
    }
    uninstall::carry_out_deletions();
    // The store's lock for the whole run, from creating the store to adopting, unless
    // a daemon serves it, which then takes init's one write, the adopt, itself.
    let created = !store_dir.join(store_io::CONFIG_FILE).exists();
    let held = if created && empty {
        let empty = grimoire_core::Store::new(grimoire_core::StoreConfig {
            large_file: None,
            plain_file: None,
            site: vec![],
            version: grimoire_core::StoreConfig::CURRENT_VERSION,
            repository: repo_str.clone(),
            // Both roots named, with no page yet: the check says which pages to put.
            roots: grimoire_core::model::Roots::new(scaffold::ROOT_SLUG, scaffold::RITES_SLUG),
            languages: languages.clone(),
            scope: grimoire_core::model::Scope::default(),
            order: Default::default(),
            recipe_kinds: Default::default(),
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
            Wrote::Written(p) => println!("wrote   {}", p.display()),
            Wrote::Noted { path, note } => println!("wrote   {}: {note}", path.display()),
            Wrote::Skipped { path, note } => println!("skipped {}: {note}", path.display()),
        }
    }
    // A store with no baseline yet, one init made or scaffold did, starts from what is there;
    // a store that has one keeps it, so a re-run or a move never hides what is new since.
    if created || !store_dir.join(store_io::BASELINE_FILE).exists() {
        adopt(&Target { store_dir: store_dir.clone(), team: None, checkout: None }, "adopted at init", held.as_ref())?;
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

/// For `init` on a store that names another repository: re-point it here
/// when that repository is gone and this checkout's `.grimoire.toml` names
/// this store, which is a checkout that moved; otherwise refuse, since a
/// second checkout of a project that still exists is a copy, not a move.
/// `held` is the store's lock, which the re-pointing needs.
fn repoint_moved_store(store_dir: &Path, recorded: &str, repo: &Path, repo_str: &str, held: Option<&store_io::StoreLock>) -> anyhow::Result<()> {
    let refuse = |why: &str| anyhow::anyhow!("store {} belongs to repository {recorded}, not {repo_str}: {why}", store_dir.display());
    let names_this_store = repo.join(store_io::PROJECT_FILE).is_file()
        && store_io::find_project(repo).is_ok_and(|(dir, named)| dir.canonicalize().ok() == repo.canonicalize().ok() && named.canonicalize().ok() == store_dir.canonicalize().ok());
    // That repository still there, this is another checkout of the project: added to the store's checkouts (req:store-checkouts).
    if Path::new(recorded).is_dir() {
        if !names_this_store {
            return Err(refuse("that repository still exists, and this checkout's .grimoire.toml does not name this store, so it is no checkout of the same project"));
        }
        let target = Target { store_dir: store_dir.to_path_buf(), team: None, checkout: Some(repo.to_path_buf()) };
        match held {
            Some(_) => {
                checkout_view(&target, held)?;
            }
            // A daemon serving the store adds it, as its writer, when asked as this checkout.
            None => {
                run_tool(&target, "status", json!({}), None)?.map_err(|e| anyhow::anyhow!("{}", e["message"].as_str().unwrap_or("the daemon refused")))?;
            }
        }
        println!("store {} serves this checkout, {repo_str}, beside {recorded}", store_dir.display());
        return Ok(());
    }
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

/// Whether a daemon serving this machine's library answers on `port`: then a
/// grimoire just made is served by it with nothing started.
fn library_on(port: u16) -> bool {
    matches!(client::call(port, "status", &json!({})), Ok((true, status)) if status["library"].as_array().is_some_and(|l| !l.is_empty()))
}

#[cfg(test)]
mod tests {
    use grimoire_server::store_io::slashed;

    /// # Test contract
    ///
    /// ```json
    /// {"test": "a-canonical-path-is-recorded-absolute", "checks": ["crates/cli/src/init/command#init-repoints", "crates/core/src/store#config-rules"]}
    /// ```
    #[test]
    fn a_canonical_path_is_recorded_absolute() {
        for (canonical, recorded) in [(r"\\?\C:\work\app", "C:/work/app"), (r"\\?\UNC\server\share\app", "//server/share/app"), ("/home/me/app", "/home/me/app")] {
            assert_eq!(slashed(canonical), recorded);
            assert!(grimoire_core::store::is_absolute_path(recorded), "{recorded} is what the config accepts");
        }
    }
}
