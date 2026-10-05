//! # Module contract
//!
//! ```json
//! {"purpose":"Find the grimoires a library serves: on a machine, those its checkouts name among the stores and copies of a server's intent kept on it; on a team's server, the stores under its library directory.","criteria":[{"id":"named-by-checkouts","statement":"find returns, in name order, each store under the stores directory and each copy under the copies directory whose recorded repository holds a project file naming that very folder, with the file's grimoire name and, for a copy, its server's address; one whose repository is gone, holds no project file, or names something else is skipped with why; every checkout of a copy of a team's grimoire is returned, so several share one name when they copy the same server's grimoire; one whose name the default checkout's grimoire or an earlier one already holds is skipped, the reason naming both checkouts, unless both are copies of that same server's grimoire; and the default checkout's own grimoire is never returned."},{"id":"shared-stores","statement":"find_shared returns, in name order, every folder directly under the library directory that holds a store and whose name is a slug, named by that folder; a folder holding no store, or named by no slug, is skipped with why."}]}
//! ```

pub mod live;

use std::path::{Path, PathBuf};

use crate::store_io::{self, Home};

/// One grimoire of the library: its name, its store, the checkout whose code
/// it is joined with and, for a copy of a server's intent, the server.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Grimoire {
    pub name: String,
    pub store_dir: PathBuf,
    pub repo: PathBuf,
    pub server: Option<String>,
}

/// What `find` found: the grimoires to serve, and why each other folder is not served.
#[derive(Debug, Default)]
pub struct Found {
    pub grimoires: Vec<Grimoire>,
    pub skipped: Vec<String>,
}

/// The grimoires the checkouts on this machine name, beside `default`, the
/// one the daemon was started for, which is served already.
pub fn find(stores: &Path, copies: &Path, default: &Grimoire) -> Found {
    let mut found = Found::default();
    let mut candidates = Vec::new();
    for dir in [stores, copies] {
        let Ok(entries) = std::fs::read_dir(dir) else { continue };
        let mut entries: Vec<PathBuf> = entries.flatten().map(|e| e.path()).filter(|p| p.join(store_io::CONFIG_FILE).is_file()).collect();
        entries.sort();
        candidates.extend(entries);
    }
    let same = |a: &Path, b: &Path| a.canonicalize().ok().zip(b.canonicalize().ok()).is_some_and(|(a, b)| a == b);
    for store_dir in candidates {
        if same(&store_dir, &default.store_dir) {
            continue;
        }
        let shown = store_dir.display().to_string();
        let Some(repository) = std::fs::read_to_string(store_dir.join(store_io::CONFIG_FILE)).ok().and_then(|t| serde_json::from_str::<serde_json::Value>(&t).ok()).and_then(|c| c["repository"].as_str().map(str::to_string)) else {
            found.skipped.push(format!("{shown}: its grimoire.json names no repository"));
            continue;
        };
        let repo = PathBuf::from(&repository);
        let named = match store_io::project_at(&repo) {
            Ok(Some(named)) => named,
            Ok(None) if repo.is_dir() => {
                found.skipped.push(format!("{shown}: its checkout {repository} holds no {}", store_io::PROJECT_FILE));
                continue;
            }
            Ok(None) => {
                found.skipped.push(format!("{shown}: its checkout {repository} is gone"));
                continue;
            }
            Err(e) => {
                found.skipped.push(format!("{shown}: {e}"));
                continue;
            }
        };
        let (home, name) = named;
        let server = match &home {
            Home::Store(named) if same(named, &store_dir) => None,
            Home::Server { address, copy } if same(copy, &store_dir) => Some(address.clone()),
            _ => {
                found.skipped.push(format!("{shown}: the project file in {repository} names another grimoire"));
                continue;
            }
        };
        // Every checkout of one team's grimoire is served, each a copy of its own; any other
        // grimoire of a name already taken is another project's, and skipped (question 0173).
        let one_team = |g: &Grimoire| g.server.is_some() && g.server == server;
        let taken = std::iter::once(default).chain(found.grimoires.iter()).find(|g| g.name == name && !one_team(g)).map(|g| g.repo.display().to_string());
        if let Some(other) = taken {
            found.skipped.push(format!("{shown}: the grimoire `{name}` is served already, with the checkout {other}; this checkout, {repository}, is not served beside it"));
            continue;
        }
        found.grimoires.push(Grimoire { name, store_dir, repo, server });
    }
    found.grimoires.sort_by(|a, b| a.name.cmp(&b.name).then_with(|| a.repo.cmp(&b.repo)));
    found
}

/// The grimoires of a team's server: every store directly under `dir`, named by
/// its folder, which must be a slug; a team's server holds no checkouts to name them.
pub fn find_shared(dir: &Path) -> Found {
    let mut found = Found::default();
    let Ok(entries) = std::fs::read_dir(dir) else {
        found.skipped.push(format!("{}: the library directory cannot be read", dir.display()));
        return found;
    };
    let mut entries: Vec<PathBuf> = entries.flatten().map(|e| e.path()).filter(|p| p.is_dir()).collect();
    entries.sort();
    for store_dir in entries {
        let folder = store_dir.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default();
        if !store_dir.join(store_io::CONFIG_FILE).is_file() {
            found.skipped.push(format!("{}: it holds no store", store_dir.display()));
        } else if !grimoire_core::store::is_slug(&folder) {
            found.skipped.push(format!("{}: `{folder}` is not a slug, so it names no grimoire; rename the folder", store_dir.display()));
        } else {
            found.grimoires.push(Grimoire { name: folder, store_dir, repo: PathBuf::new(), server: None });
        }
    }
    found
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A checkout at `repo` whose project file names `named` as `name`, and the store it records.
    fn checkout(root: &Path, repo: &str, store: &Path, name: &str, named: &str) -> PathBuf {
        let repo = root.join(repo);
        std::fs::create_dir_all(&repo).unwrap();
        std::fs::create_dir_all(store).unwrap();
        let repository = store_io::repository_string(&repo).unwrap();
        std::fs::write(store.join(store_io::CONFIG_FILE), serde_json::json!({ "repository": repository }).to_string()).unwrap();
        std::fs::write(repo.join(store_io::PROJECT_FILE), format!("name = \"{name}\"\nstore = {}\n", toml::Value::String(named.to_string()))).unwrap();
        repo
    }

    /// # Test contract
    ///
    /// ```json
    /// {"test": "a-teams-library-is-its-store-folders", "checks": ["crates/server/src/library#shared-stores"]}
    /// ```
    #[test]
    fn a_teams_library_is_its_store_folders() {
        let tmp = tempfile::tempdir().unwrap();
        let dir = tmp.path();
        for folder in ["shop", "ledger", "Not A Slug", "empty"] {
            std::fs::create_dir_all(dir.join(folder)).unwrap();
            if folder != "empty" {
                std::fs::write(dir.join(folder).join(store_io::CONFIG_FILE), "{}").unwrap();
            }
        }
        std::fs::write(dir.join("notes.txt"), "a file, not a store").unwrap();
        let found = find_shared(dir);
        let names: Vec<&str> = found.grimoires.iter().map(|g| g.name.as_str()).collect();
        assert_eq!(names, ["ledger", "shop"]);
        let why = found.skipped.join("\n");
        assert!(why.contains("is not a slug") && why.contains("holds no store"), "{why}");
    }

    /// # Test contract
    ///
    /// ```json
    /// {"test": "the-library-is-what-checkouts-name", "checks": ["crates/server/src/library#named-by-checkouts"]}
    /// ```
    #[test]
    fn the_library_is_what_checkouts_name() {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path();
        let (stores, copies) = (root.join("stores"), root.join("copies"));
        let at = |name: &str| stores.join(name);
        // The default checkout, served already, and two others that name their stores.
        let shop = checkout(root, "shop", &at("shop"), "shop", &at("shop").to_string_lossy());
        let ledger = checkout(root, "ledger", &at("ledger"), "ledger", &at("ledger").to_string_lossy());
        checkout(root, "audit", &at("audit"), "audit", &at("audit").to_string_lossy());
        // A store whose checkout names another; one whose checkout is gone; a second checkout of `ledger`.
        checkout(root, "moved", &at("old"), "moved", &at("elsewhere").to_string_lossy());
        let gone = checkout(root, "gone", &at("gone"), "gone", &at("gone").to_string_lossy());
        std::fs::remove_dir_all(&gone).unwrap();
        checkout(root, "ledger-two", &at("ledger-2"), "ledger", &at("ledger-2").to_string_lossy());
        let default = Grimoire { name: "shop".into(), store_dir: at("shop"), repo: shop, server: None };
        let found = find(&stores, &copies, &default);
        let names: Vec<&str> = found.grimoires.iter().map(|g| g.name.as_str()).collect();
        assert_eq!(names, ["audit", "ledger"], "the default is served already; {:?}", found.skipped);
        assert_eq!(found.grimoires[1].repo.canonicalize().unwrap(), ledger.canonicalize().unwrap());
        let why = found.skipped.join("\n");
        assert!(why.contains("names another grimoire") && why.contains("is gone"), "{why}");
        assert!(why.contains("`ledger` is served already") && why.contains("ledger-two"), "the second checkout is named with the first: {why}");
        // A second checkout of the default's grimoire is not served beside it either.
        checkout(root, "shop-two", &at("shop-2"), "shop", &at("shop-2").to_string_lossy());
        assert!(find(&stores, &copies, &default).skipped.iter().any(|s| s.contains("`shop` is served already")));
    }
}
