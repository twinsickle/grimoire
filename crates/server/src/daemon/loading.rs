//! # Module contract
//!
//! ```json
//! {"purpose":"Read a store and the repository beside it into the state the tools operate on, its code extracted once, for a daemon to serve or a command to run over.","criteria":[{"id":"source-reader","statement":"A daemon that holds code gives the source tool a reader over its repository that reads only a relative path inside it, refusing an absolute path, a drive or UNC prefix, a .. component, or a path that resolves outside the repository through a link; it reads no more than the limit it is given, reporting a larger file as too large before reading it, drops a byte-order mark, replaces invalid UTF-8, and never writes."}]}
//! ```

use std::collections::BTreeMap;
use std::path::{Component, Path, PathBuf};
use std::sync::Arc;

use grimoire_core::tools;
use grimoire_core::{Build, Extraction, FileExtraction, State};
use grimoire_extract::{supported_languages, Extractor};

use super::OpenError;
use crate::store_io;

/// What loading produced, for the CLI as well as the daemon.
pub struct Loaded {
    pub state: State,
    pub files: BTreeMap<String, FileExtraction>,
    pub extractor: Extractor,
    pub repo: PathBuf,
    pub extract_errors: Vec<grimoire_extract::ExtractError>,
}

/// Load a store and extract its repository once.
pub fn load(store_dir: &Path) -> Result<Loaded, OpenError> {
    load_for(store_dir, None)
}

/// Load a store as `checkout`, one of its checkouts, sees it, or as its main one, and extract
/// that checkout's code once.
pub fn load_for(store_dir: &Path, checkout: Option<&str>) -> Result<Loaded, OpenError> {
    let (store, malformed) = match checkout {
        Some(c) => store_io::load_for(store_dir, c)?,
        None => store_io::load(store_dir)?,
    };
    let shared_by_checkouts = !store_io::checkouts::others(store_dir).is_empty();
    let repo = PathBuf::from(&store.config.repository);
    if !repo.is_dir() {
        return Err(OpenError::NoRepository(repo));
    }
    store_io::refuse_inside(store_dir, &repo)?;
    let mut loaded = load_store(store, malformed)?;
    loaded.state.set_shares_checkouts(shared_by_checkouts);
    Ok(loaded)
}

/// The same for a store held only in memory, as a server's intent fetched
/// for the main line is: its repository extracted once, nothing written.
pub fn load_store(store: grimoire_core::Store, malformed: Vec<grimoire_core::Malformed>) -> Result<Loaded, OpenError> {
    let repo = PathBuf::from(&store.config.repository);
    if !repo.is_dir() {
        return Err(OpenError::NoRepository(repo));
    }
    let extractor = Extractor::for_languages(&repo, &store.config.languages);
    let (extracted, extract_errors) = extractor.extract_all()?;
    let files: BTreeMap<String, FileExtraction> = extracted.into_iter().map(|f| (f.path.clone(), f)).collect();
    let extraction = Extraction::from_files(files.values());
    let mut state = State::new(store, extraction, malformed);
    state.set_build(build());
    state.set_source(Arc::new(RepoSource { root: repo.clone() }));
    Ok(Loaded { state, files, extractor, repo, extract_errors })
}

/// Reads the repository's files for the `source` tool, and nothing else: the
/// core does no I/O, so it is handed this. A path that is absolute, climbs
/// with `..`, or resolves outside the repository through a link is refused.
#[derive(Debug)]
pub struct RepoSource {
    pub root: PathBuf,
}

impl tools::SourceReader for RepoSource {
    fn read(&self, path: &str, limit: usize) -> Result<String, tools::SourceError> {
        use std::io::Read;
        use tools::SourceError::{TooLarge, Unreadable};
        let rel = Path::new(path);
        // Only plain names: no root, drive or UNC prefix, `.` or `..`.
        if rel.is_absolute() || !rel.components().all(|c| matches!(c, Component::Normal(_))) {
            return Err(Unreadable(format!("`{path}` is not a path inside the repository")));
        }
        let root = self.root.canonicalize().map_err(|e| Unreadable(e.to_string()))?;
        let full = root.join(rel).canonicalize().map_err(|e| Unreadable(e.to_string()))?;
        if !full.starts_with(&root) {
            return Err(Unreadable(format!("`{path}` resolves outside the repository")));
        }
        // Size first, then never more than the limit even if it grows meanwhile.
        let size = std::fs::metadata(&full).map_err(|e| Unreadable(e.to_string()))?.len();
        if size > limit as u64 {
            return Err(TooLarge(size));
        }
        let mut bytes = Vec::new();
        std::fs::File::open(&full).and_then(|f| f.take(limit as u64 + 1).read_to_end(&mut bytes)).map_err(|e| Unreadable(e.to_string()))?;
        if bytes.len() > limit {
            return Err(TooLarge(bytes.len() as u64));
        }
        let text = String::from_utf8_lossy(&bytes);
        Ok(text.strip_prefix('\u{feff}').unwrap_or(&text).to_string())
    }
}

/// What this binary is, for `status`: the workspace version and the
/// languages the extractor reads, with the tier it reads each at. Told to core here because the
/// core crate does no I/O and this is the one path every command loads through.
pub fn build() -> Build {
    let supported = supported_languages();
    Build {
        version: env!("CARGO_PKG_VERSION").to_string(),
        supported_languages: supported.iter().map(|(l, _)| l.to_string()).collect(),
        tiers: supported.iter().map(|(l, t)| (l.to_string(), *t)).collect(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// # Test contract
    ///
    /// ```json
    /// {"test": "the-source-reader-stays-inside-the-repository", "checks": ["crates/server/src/daemon/loading#source-reader"]}
    /// ```
    #[test]
    fn the_source_reader_stays_inside_the_repository() {
        use grimoire_core::tools::SourceReader;
        let tmp = tempfile::Builder::new().prefix("grimoire-source-").tempdir().unwrap();
        let dir = tmp.path().to_path_buf();
        let repo = dir.join("repo");
        std::fs::create_dir_all(repo.join("src")).unwrap();
        std::fs::write(repo.join("src").join("lib.rs"), "\u{feff}pub fn a() {}\n").unwrap();
        std::fs::write(dir.join("secret.txt"), "outside").unwrap();
        let reader = RepoSource { root: repo.clone() };
        let big = 1 << 20;
        assert_eq!(reader.read("src/lib.rs", big).unwrap(), "pub fn a() {}\n", "the text, without its byte-order mark");
        let outside = dir.join("secret.txt");
        for escape in ["../secret.txt", "src/../../secret.txt", outside.to_str().unwrap(), "./src/lib.rs", "C:/Windows/win.ini", "C:secret.txt", r"\\server\share\secret.txt", "/etc/passwd"] {
            assert!(reader.read(escape, big).is_err(), "{escape} is refused");
        }
        assert!(reader.read("src/missing.rs", big).is_err());
        // A link inside the repository to a file outside it is refused, where links can be made.
        #[cfg(unix)]
        let linked = std::os::unix::fs::symlink(&outside, repo.join("src").join("link.rs")).is_ok();
        #[cfg(windows)]
        let linked = std::os::windows::fs::symlink_file(&outside, repo.join("src").join("link.rs")).is_ok();
        if linked {
            assert!(reader.read("src/link.rs", big).is_err(), "a link out of the repository is refused");
            let _ = std::fs::remove_file(repo.join("src").join("link.rs"));
        }
        // Past the limit: too large, and not read.
        assert_eq!(reader.read("src/lib.rs", 4), Err(grimoire_core::tools::SourceError::TooLarge(17)));
        let before = std::fs::read_dir(repo.join("src")).unwrap().count();
        let _ = reader.read("src/lib.rs", big);
        assert_eq!(std::fs::read_dir(repo.join("src")).unwrap().count(), before, "reading writes nothing");
    }
}
