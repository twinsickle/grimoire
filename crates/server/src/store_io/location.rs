//! # Module contract
//!
//! ```json
//! {"purpose":"Say where a store lives: resolve a store argument or a project file's entry the one way every command uses, find and write a repository's .grimoire.toml, and refuse a store inside the repository it describes.","criteria":[{"id":"resolution","statement":"A store lives under <data>/grimoire/stores/<name> unless given as a path; a store argument or a repository's .grimoire.toml is resolved one way, an absolute path as it is, a bare name of one plain component under the stores directory, and anything else, . and .. included, as a path relative to the repository."},{"id":"outside-repository","statement":"refuse_inside refuses a store that resolves inside the repository it describes, through . and .. or links, whether or not the store exists yet, and accepts one beside or above it."}]}
//! ```

use std::path::{Path, PathBuf};

use super::StoreIoError;

pub const PROJECT_FILE: &str = ".grimoire.toml";

/// The platform data directory for stores: `<data>/grimoire/stores`.
pub fn stores_dir() -> Result<PathBuf, StoreIoError> {
    let dirs = directories::ProjectDirs::from("", "", "grimoire").ok_or(StoreIoError::NoDataDir)?;
    Ok(dirs.data_dir().join("stores"))
}

/// Walk up from `start` to the nearest `.grimoire.toml` and return the
/// directory holding it and the store it names, resolved.
pub fn find_project(start: &Path) -> Result<(PathBuf, PathBuf), StoreIoError> {
    let mut dir = Some(start.to_path_buf());
    while let Some(d) = dir {
        let f = d.join(PROJECT_FILE);
        if f.is_file() {
            let text = std::fs::read_to_string(&f).map_err(|e| StoreIoError::Io(f.clone(), e))?;
            let doc: toml::Table = text.parse().map_err(|e: toml::de::Error| StoreIoError::BadProjectFile(f.clone(), e.to_string()))?;
            let store = doc.get("store").and_then(|s| s.as_str()).ok_or_else(|| StoreIoError::BadProjectFile(f.clone(), "missing `store`".into()))?;
            let resolved = store_for_repo(store, &d)?;
            return Ok((d, resolved));
        }
        dir = d.parent().map(Path::to_path_buf);
    }
    Err(StoreIoError::NoProjectFile(start.to_path_buf()))
}

/// Write `.grimoire.toml` into a repository.
pub fn write_project_file(repo: &Path, store: &str) -> Result<PathBuf, StoreIoError> {
    let f = repo.join(PROJECT_FILE);
    let text = format!("# Written once by `grimoire init`. Names the store that holds this project's grimoire.\nstore = {}\n", toml::Value::String(store.to_string()));
    std::fs::write(&f, text).map_err(|e| StoreIoError::Io(f.clone(), e))?;
    Ok(f)
}

/// The store a project file or `init` names, resolved the one way both
/// use: an absolute path as it is, a path with a separator relative to the
/// repository, and a bare name under the stores directory. `.` and `..`
/// are paths, never names: a name is one plain component, joined to the
/// stores directory (where a link a person made there still leads where
/// they pointed it).
pub fn store_for_repo(arg: &str, repo: &Path) -> Result<PathBuf, StoreIoError> {
    let p = Path::new(arg);
    let mut parts = p.components();
    let a_name = matches!((parts.next(), parts.next()), (Some(std::path::Component::Normal(_)), None)) && !arg.contains(['/', '\\']);
    if p.is_absolute() {
        Ok(p.to_path_buf())
    } else if a_name {
        Ok(stores_dir()?.join(arg))
    } else {
        Ok(repo.join(arg))
    }
}

/// `path` made absolute with `.` and `..` resolved, through links as far as
/// it exists, so a path not created yet still resolves to where it would be.
fn resolved(path: &Path) -> std::io::Result<PathBuf> {
    let abs = std::path::absolute(path)?;
    let mut existing = abs.as_path();
    let mut rest = Vec::new();
    let base = loop {
        match existing.canonicalize() {
            Ok(c) => break c,
            Err(_) => match (existing.parent(), existing.file_name()) {
                (Some(parent), Some(name)) => {
                    rest.push(name.to_os_string());
                    existing = parent;
                }
                _ => break existing.to_path_buf(),
            },
        }
    };
    let mut out = base;
    for name in rest.into_iter().rev() {
        out.push(name);
    }
    // `absolute` keeps `..`; what did not exist cannot be a link, so it folds lexically.
    let mut folded = PathBuf::new();
    for c in out.components() {
        match c {
            std::path::Component::ParentDir => {
                folded.pop();
            }
            std::path::Component::CurDir => {}
            other => folded.push(other),
        }
    }
    Ok(folded)
}

/// Refuse a store that resolves inside `repo`, the repository it describes,
/// whether or not the store exists yet.
pub fn refuse_inside(store_dir: &Path, repo: &Path) -> Result<(), StoreIoError> {
    let store = resolved(store_dir).map_err(|e| StoreIoError::Io(store_dir.to_path_buf(), e))?;
    let repo_at = resolved(repo).map_err(|e| StoreIoError::Io(repo.to_path_buf(), e))?;
    if store.starts_with(&repo_at) {
        return Err(StoreIoError::InsideRepository(store_dir.to_path_buf(), repo.to_path_buf()));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::store_io::tests::temp;

    /// # Test contract
    ///
    /// ```json
    /// {"test": "a-store-inside-its-repository-is-refused", "checks": ["crates/server/src/store_io/location#outside-repository", "crates/server/src/store_io/location#resolution"]}
    /// ```
    #[test]
    fn a_store_inside_its_repository_is_refused() {
        let dir = temp();
        let repo = dir.join("repo");
        std::fs::create_dir_all(repo.join("src")).unwrap();
        let inside = |store: &Path| matches!(refuse_inside(store, &repo), Err(StoreIoError::InsideRepository(..)));
        assert!(inside(&repo.join("local-store")), "not created yet");
        assert!(inside(&repo.join("src")), "an existing folder in it");
        assert!(inside(&repo), "the repository itself");
        assert!(inside(&repo.join("src").join("..").join("store")), "through ..");
        assert!(inside(&dir.join("elsewhere").join("..").join("repo").join("store")), "back in through ..");
        assert!(refuse_inside(&dir.join("store"), &repo).is_ok(), "beside it");
        assert!(refuse_inside(&repo.join("..").join("store"), &repo).is_ok(), "beside it, through ..");
        assert!(refuse_inside(&dir.join("repo-store"), &repo).is_ok(), "a name that only starts like it");
        // Through a link: a folder outside the repository that leads into it is inside.
        let link = dir.join("link");
        if make_dir_link(&repo, &link) {
            assert!(inside(&link.join("store")), "through a link into the repository");
            assert!(refuse_inside(&dir.join("store"), &link).is_ok(), "a store beside the link's target is beside the repository");
        }
        // How a store argument resolves for a repository: a name only when it is one plain component.
        let stores = stores_dir().unwrap();
        assert_eq!(store_for_repo("mine", &repo).unwrap(), stores.join("mine"));
        for path in ["..", ".", "./mine", "../mine", "a/b"] {
            assert_eq!(store_for_repo(path, &repo).unwrap(), repo.join(path), "{path} is a path, never a name");
        }
        assert_eq!(store_for_repo(&dir.to_string_lossy(), &repo).unwrap(), dir);
        std::fs::remove_dir_all(&dir).unwrap();
    }

    /// # Test contract
    ///
    /// ```json
    /// {"test": "project-file-round-trip", "checks": ["crates/server/src/store_io/location#resolution"]}
    /// ```
    #[test]
    fn project_file_round_trip() {
        let dir = temp();
        let nested = dir.join("a").join("b");
        std::fs::create_dir_all(&nested).unwrap();
        write_project_file(&dir, "my-store").unwrap();
        let (root, store) = find_project(&nested).unwrap();
        assert_eq!(root, dir);
        assert!(store.ends_with("my-store"));
        write_project_file(&dir, "./local-store").unwrap();
        let (_, store) = find_project(&nested).unwrap();
        assert_eq!(store, dir.join("./local-store"));
        std::fs::remove_dir_all(&dir).unwrap();
    }

    /// A link at `link` to the folder `target`: a junction on Windows, which
    /// needs no privilege, a symlink elsewhere. False when none can be made.
    fn make_dir_link(target: &Path, link: &Path) -> bool {
        #[cfg(windows)]
        {
            std::process::Command::new("cmd").args(["/C", "mklink", "/J"]).arg(link).arg(target).output().is_ok_and(|o| o.status.success())
        }
        #[cfg(unix)]
        {
            std::os::unix::fs::symlink(target, link).is_ok()
        }
    }
}
