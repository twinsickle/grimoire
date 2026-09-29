//! # Module contract
//!
//! ```json
//! {"purpose":"Watch the target repository and hand changed source paths to the daemon, debounced and filtered.","criteria":[{"id":"ignored-skipped","statement":"Paths that .gitignore ignores, and anything under .git, never reach the daemon."},{"id":"debounced","statement":"File events are grouped by the notify debouncer over a fixed window of 300 milliseconds, and each group re-extracts every repository path it names that the extractor handles and the repository does not ignore, once, however many events name it."}]}
//! ```

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

use ignore::gitignore::{Gitignore, GitignoreBuilder};
use notify::RecursiveMode;
use notify_debouncer_full::{new_debouncer, DebounceEventResult, Debouncer, RecommendedCache};

use crate::daemon::Daemon;

/// Whether the repository ignores a path: `.git`, then `.gitignore` and
/// `.git/info/exclude` at the root.
pub struct Ignorer {
    root: PathBuf,
    gitignore: Gitignore,
}

impl Ignorer {
    pub fn new(root: &Path) -> Ignorer {
        let mut b = GitignoreBuilder::new(root);
        b.add(root.join(".gitignore"));
        b.add(root.join(".git").join("info").join("exclude"));
        let gitignore = b.build().unwrap_or_else(|_| Gitignore::empty());
        Ignorer { root: root.to_path_buf(), gitignore }
    }

    pub fn ignores(&self, path: &Path) -> bool {
        let Ok(rel) = path.strip_prefix(&self.root) else { return true };
        if rel.components().next().is_some_and(|c| c.as_os_str() == ".git") {
            return true;
        }
        let is_dir = path.is_dir();
        self.gitignore.matched_path_or_any_parents(path, is_dir).is_ignore()
    }
}

/// How long the watcher waits for a burst of events on one file to settle
/// before handing it on: an editor's save-and-rewrite or a formatter's pass
/// is one change.
pub const DEBOUNCE: Duration = Duration::from_millis(300);

/// Start watching. The returned debouncer must be kept alive.
pub fn start(daemon: Arc<Daemon>) -> notify::Result<Debouncer<notify::RecommendedWatcher, RecommendedCache>> {
    let root = daemon.repo.clone();
    let ignorer = Ignorer::new(&root);
    let mut debouncer = new_debouncer(DEBOUNCE, None, move |result: DebounceEventResult| match result {
        Ok(events) => {
            let rels = {
                let extractor = daemon.extractor.read().expect("extractor lock");
                to_reextract(events.iter().flat_map(|e| e.paths.iter().map(PathBuf::as_path)), |p| ignorer.ignores(p), |p| daemon.relative(p), |r| extractor.handles(r))
            };
            if !rels.is_empty() {
                daemon.reextract(&rels);
            }
        }
        Err(errors) => {
            for e in errors {
                tracing::warn!("watch error: {e}");
            }
        }
    })?;
    debouncer.watch(&root, RecursiveMode::Recursive)?;
    Ok(debouncer)
}

/// The paths one debounced batch re-extracts: each repository-relative path
/// the extractor handles and the repository does not ignore, once, in the
/// order first named. The debouncer groups a burst into one batch; this makes
/// the batch one re-extraction per file however many events name it.
fn to_reextract<'a>(paths: impl IntoIterator<Item = &'a Path>, ignored: impl Fn(&Path) -> bool, relative: impl Fn(&Path) -> Option<String>, handles: impl Fn(&str) -> bool) -> Vec<String> {
    let mut rels: Vec<String> = Vec::new();
    for p in paths {
        if ignored(p) {
            continue;
        }
        if let Some(rel) = relative(p) {
            if handles(&rel) && !rels.contains(&rel) {
                rels.push(rel);
            }
        }
    }
    rels
}

#[cfg(test)]
mod tests {
    use super::*;

    /// # Test contract
    ///
    /// ```json
    /// {"test": "a-burst-is-one-reextraction", "checks": ["crates/server/src/watch#debounced"]}
    /// ```
    #[test]
    fn a_burst_for_one_file_is_one_reextraction() {
        assert_eq!(DEBOUNCE, Duration::from_millis(300), "the window the debouncer groups a burst over");
        let root = Path::new("/repo");
        // A save-and-rewrite and a formatter's pass: one file named many times, among an ignored path, a file no adapter handles and one outside the repository.
        let mut batch: Vec<PathBuf> = std::iter::repeat_n(root.join("src/lib.rs"), 10).collect();
        batch.extend([root.join("target/x.rs"), root.join("README.md"), root.join("src/main.rs"), PathBuf::from("/elsewhere/a.rs"), root.join("src/lib.rs")]);
        let rels = to_reextract(
            batch.iter().map(PathBuf::as_path),
            |p| p.starts_with(root.join("target")),
            |p| p.strip_prefix(root).ok().map(|r| r.to_string_lossy().replace('\\', "/")),
            |r| r.ends_with(".rs"),
        );
        assert_eq!(rels, vec!["src/lib.rs".to_string(), "src/main.rs".to_string()]);
    }

    /// # Test contract
    ///
    /// ```json
    /// {"test": "ignored-paths-never-reach-the-daemon", "checks": ["crates/server/src/watch#ignored-skipped"]}
    /// ```
    #[test]
    fn ignored_paths_never_reach_the_daemon() {
        let nanos = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_nanos()).unwrap_or(0);
        let dir = std::env::temp_dir().join(format!("grimoire-watch-{}-{nanos}", std::process::id()));
        std::fs::create_dir_all(dir.join("src")).unwrap();
        std::fs::write(dir.join(".gitignore"), "target/\n").unwrap();
        let i = Ignorer::new(&dir);
        assert!(i.ignores(&dir.join(".git").join("index")), "anything under .git");
        assert!(i.ignores(&dir.join("target").join("debug").join("x.rs")), "what .gitignore ignores");
        assert!(!i.ignores(&dir.join("src").join("lib.rs")));
        std::fs::remove_dir_all(&dir).unwrap();
    }
}
