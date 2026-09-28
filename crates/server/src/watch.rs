//! # Module contract
//!
//! ```json
//! {"purpose":"Watch the target repository and hand changed source paths to the daemon, debounced and filtered.","criteria":[{"id":"ignored-skipped","statement":"Paths that .gitignore ignores, and anything under .git, never reach the daemon."},{"id":"debounced","statement":"File events are debounced over a fixed window of 300 milliseconds, so a burst of events for one file becomes one re-extraction."}]}
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
            let mut rels: Vec<String> = Vec::new();
            for ev in events {
                for p in &ev.paths {
                    if ignorer.ignores(p) {
                        continue;
                    }
                    if let Some(rel) = daemon.relative(p) {
                        if daemon.extractor.read().expect("extractor lock").handles(&rel) && !rels.contains(&rel) {
                            rels.push(rel);
                        }
                    }
                }
            }
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

#[cfg(test)]
mod tests {
    use super::*;

    /// # Test contract
    ///
    /// ```json
    /// {"test": "ignored-paths-never-reach-the-daemon", "checks": ["crates/server/src/watch#ignored-skipped"]}
    /// ```
    /// # Test contract
    ///
    /// ```json
    /// {"test": "the-debounce-window", "checks": ["crates/server/src/watch#debounced"]}
    /// ```
    #[test]
    fn events_are_debounced_over_300_milliseconds() {
        assert_eq!(DEBOUNCE, Duration::from_millis(300));
    }

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
