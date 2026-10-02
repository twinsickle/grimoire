//! # Module contract
//!
//! ```json
//! {"purpose":"Watch the target repository and hand changed source paths to the daemon, debounced and filtered.","criteria":[{"id":"ignored-skipped","statement":"Paths that a .gitignore in any directory from the path up to the repository root ignores, the nearest deciding and a ! pattern re-including, or that .git/info/exclude ignores, and anything under .git, never reach the daemon; a .gitignore that changes applies from the next event."},{"id":"debounced","statement":"File events are grouped by the notify debouncer over a fixed window of 300 milliseconds, and each group re-extracts every repository path it names that the extractor handles and the repository does not ignore, once, however many events name it."}]}
//! ```

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use ignore::gitignore::{Gitignore, GitignoreBuilder};
use notify::RecursiveMode;
use notify_debouncer_full::{new_debouncer, DebounceEventResult, Debouncer, RecommendedCache};

use crate::daemon::Daemon;

/// Whether the repository ignores a path, as git would: anything under
/// `.git`; then each directory's `.gitignore` from the path's up to the root,
/// the nearest that matches deciding, a `!` pattern re-including; then
/// `.git/info/exclude`. Each directory's matcher is read once and kept until
/// `forget` is told its `.gitignore` changed.
pub struct Ignorer {
    root: PathBuf,
    exclude: Gitignore,
    dirs: Mutex<HashMap<PathBuf, Arc<Gitignore>>>,
}

impl Ignorer {
    pub fn new(root: &Path) -> Ignorer {
        let mut b = GitignoreBuilder::new(root);
        b.add(root.join(".git").join("info").join("exclude"));
        let exclude = b.build().unwrap_or_else(|_| Gitignore::empty());
        Ignorer { root: root.to_path_buf(), exclude, dirs: Mutex::new(HashMap::new()) }
    }

    pub fn ignores(&self, path: &Path) -> bool {
        let Ok(rel) = path.strip_prefix(&self.root) else { return true };
        if rel.components().next().is_some_and(|c| c.as_os_str() == ".git") {
            return true;
        }
        // As git: a path under an ignored directory is ignored, whatever a
        // pattern nearer to it says, so the directories are decided first,
        // from the root down.
        let mut above: Vec<&Path> = path.ancestors().skip(1).take_while(|d| d.starts_with(&self.root) && *d != self.root).collect();
        above.reverse();
        if above.iter().any(|d| self.decide(d, true) == Some(true)) {
            return true;
        }
        self.decide(path, path.is_dir()) == Some(true)
    }

    /// Whether `path` itself is ignored (`Some(true)`), re-included
    /// (`Some(false)`) or matched by nothing: each `.gitignore` from the
    /// path's directory up to the root, the nearest that matches deciding,
    /// then `.git/info/exclude`.
    fn decide(&self, path: &Path, is_dir: bool) -> Option<bool> {
        let mut dir = path.parent();
        while let Some(d) = dir.filter(|d| d.starts_with(&self.root)) {
            let matcher = self.matcher(d);
            let m = matcher.matched(path, is_dir);
            if !m.is_none() {
                return Some(m.is_ignore());
            }
            dir = d.parent();
        }
        let m = self.exclude.matched(path, is_dir);
        (!m.is_none()).then(|| m.is_ignore())
    }

    /// Drop what was read of the `.gitignore` at `path`, when it changed.
    pub fn forget(&self, path: &Path) {
        if path.file_name().is_some_and(|n| n == ".gitignore") {
            if let Some(dir) = path.parent() {
                self.dirs.lock().expect("ignore cache").remove(dir);
            }
        }
    }

    /// The matcher of `dir`'s own `.gitignore`, empty when it has none.
    fn matcher(&self, dir: &Path) -> Arc<Gitignore> {
        let mut dirs = self.dirs.lock().expect("ignore cache");
        // Directories come and go (build output, temporary files); past a
        // bound the cache starts again rather than growing for good.
        if dirs.len() >= MATCHERS_KEPT && !dirs.contains_key(dir) {
            dirs.clear();
        }
        dirs.entry(dir.to_path_buf())
            .or_insert_with(|| {
                let mut b = GitignoreBuilder::new(dir);
                b.add(dir.join(".gitignore"));
                Arc::new(b.build().unwrap_or_else(|_| Gitignore::empty()))
            })
            .clone()
    }
}

/// How many directories' ignore matchers the watcher keeps before starting afresh.
const MATCHERS_KEPT: usize = 4096;

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
            for p in events.iter().flat_map(|e| e.paths.iter()) {
                ignorer.forget(p);
            }
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
        let tmp = tempfile::Builder::new().prefix("grimoire-watch-").tempdir().unwrap();
        let dir = tmp.path().to_path_buf();
        std::fs::create_dir_all(dir.join("src")).unwrap();
        std::fs::create_dir_all(dir.join("web")).unwrap();
        std::fs::create_dir_all(dir.join(".git").join("info")).unwrap();
        std::fs::write(dir.join(".gitignore"), "target/\n*.gen.rs\n").unwrap();
        std::fs::write(dir.join("web").join(".gitignore"), "dist/\n").unwrap();
        std::fs::write(dir.join("src").join(".gitignore"), "!keep.gen.rs\n").unwrap();
        std::fs::write(dir.join(".git").join("info").join("exclude"), "scratch.rs\n").unwrap();
        let i = Ignorer::new(&dir);
        assert!(i.ignores(&dir.join(".git").join("index")), "anything under .git");
        assert!(i.ignores(&dir.join("target").join("debug").join("x.rs")), "what the root .gitignore ignores");
        assert!(!i.ignores(&dir.join("src").join("lib.rs")));
        assert!(i.ignores(&dir.join("web").join("dist").join("x.js")), "a nested .gitignore applies below it");
        assert!(!i.ignores(&dir.join("web").join("src").join("a.ts")));
        assert!(!i.ignores(&dir.join("src").join("keep.gen.rs")), "the nearest file re-includes");
        assert!(i.ignores(&dir.join("src").join("other.gen.rs")), "the root's pattern still holds for the rest");
        assert!(i.ignores(&dir.join("scratch.rs")), ".git/info/exclude");
        // Nothing re-includes a file under an ignored directory, as in git.
        std::fs::create_dir_all(dir.join("target")).unwrap();
        std::fs::write(dir.join("target").join(".gitignore"), "!keep.rs\n").unwrap();
        assert!(i.ignores(&dir.join("target").join("keep.rs")), "the directory above is ignored");
        // A changed .gitignore applies once the watcher is told of it.
        std::fs::write(dir.join("web").join(".gitignore"), "").unwrap();
        assert!(i.ignores(&dir.join("web").join("dist").join("x.js")), "read once and kept");
        i.forget(&dir.join("web").join(".gitignore"));
        assert!(!i.ignores(&dir.join("web").join("dist").join("x.js")), "read again after the change");
    }
}
