//! # Module contract
//!
//! ```json
//! {"purpose":"Watch the target repository and hand changed source paths to the daemon, debounced and filtered.","criteria":[{"id":"gitignored-paths-never-reach","statement":"Paths that a .gitignore in any directory from the path up to the repository root ignores never reach the daemon."},{"id":"nearest-gitignore-decides","statement":"Of the .gitignore files from a path up to the repository root, the nearest decides, and a ! pattern re-includes."},{"id":"under-git-never-reach","statement":"Anything under .git never reaches the daemon."},{"id":"no-other-ignore-keeps-one-out","statement":"No other ignore keeps a path from the daemon, .git/info/exclude and a .gitignore above the root among them, as the reading of the checkout reads none."},{"id":"changed-gitignore-applies-next-event","statement":"A .gitignore that changes applies from the next event."},{"id":"events-grouped-over-300ms","statement":"File events are grouped by the notify debouncer over a fixed window of 300 milliseconds."},{"id":"group-hands-handled-unignored-paths","statement":"Each group hands the daemon every repository path it names that the repository does not ignore and that is of a kind the extractor handles."},{"id":"recipe-files-handed","statement":"Each group hands the daemon every repository path it names that the repository does not ignore and that is a recipe file."},{"id":"nested-checkout-boundary-handed","statement":"Each group hands the daemon every repository path it names that the repository does not ignore and that marks a nested checkout's boundary."},{"id":"held-recipe-paths-handed","statement":"Each group hands the daemon every repository path it names that the repository does not ignore and that is a path a held recipe names, lies under one or is a folder holding one."},{"id":"daemon-reads-only-what-the-whole-reading-would","statement":"The daemon reads only what the whole checkout's reading would."},{"id":"one-reading-however-many-events","statement":"The daemon reads a path once, however many events name it."}]}
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
/// the nearest that matches deciding, a `!` pattern re-including. Nothing else:
/// not `.git/info/exclude`, a person's global excludes nor a `.gitignore` above
/// the root, which the reading of the checkout does not read either, so an edit
/// to any file it reads arrives. Each directory's matcher is read once and kept
/// until `forget` is told its `.gitignore` changed.
pub struct Ignorer {
    root: PathBuf,
    dirs: Mutex<HashMap<PathBuf, Arc<Gitignore>>>,
}

impl Ignorer {
    pub fn new(root: &Path) -> Ignorer {
        Ignorer { root: root.to_path_buf(), dirs: Mutex::new(HashMap::new()) }
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
    /// path's directory up to the root, the nearest that matches deciding.
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
        None
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
            // The paths the recipes name, read before the extractor as every other reading takes them.
            let named = daemon.recipe_paths();
            let rels = {
                let extractor = daemon.extractor.read().expect("extractor lock");
                to_reextract(
                    events.iter().flat_map(|e| e.paths.iter().map(PathBuf::as_path)),
                    |p| ignorer.ignores(p),
                    |p| daemon.relative(p),
                    |r| extractor.handles(r) || extractor.is_boundary(r) || named_by(&named, r),
                )
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

/// Whether a repository path is one a recipe names, lies under one, or is a folder holding one:
/// a file it represents changed, or a folder above it went or came.
fn named_by(named: &[String], rel: &str) -> bool {
    let under = |a: &str, b: &str| a.strip_prefix(b).is_some_and(|r| r.starts_with('/'));
    named.iter().any(|n| rel == n || under(rel, n) || under(n, rel))
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
    /// {"test": "the-watcher-hands-on-markers-and-files-of-a-kind-read", "checks": ["crates/server/src/watch#group-hands-handled-unignored-paths", "crates/server/src/watch#recipe-files-handed", "crates/server/src/watch#nested-checkout-boundary-handed", "crates/server/src/watch#held-recipe-paths-handed"]}
    /// ```
    #[test]
    fn the_watcher_hands_on_markers_and_files_of_a_kind_read() {
        // As start asks it: the extractor's kinds and a nested checkout's marker reach the daemon, which decides what is read.
        let root = Path::new("C:/repo");
        let e = grimoire_extract::Extractor::for_languages(root, &["rust".to_string()]);
        let paths = ["C:/repo/src/a.rs", "C:/repo/worktrees/wt/src/b.rs", "C:/repo/worktrees/wt/.git", "C:/repo/pkg/.grimoire.toml", "C:/repo/README.md", "C:/repo/.grimoire.toml"].map(Path::new);
        let rel = |p: &Path| p.strip_prefix(root).ok().map(|r| r.to_string_lossy().replace('\\', "/"));
        let handed = to_reextract(paths, |_| false, rel, |r| e.handles(r) || e.is_boundary(r));
        assert_eq!(handed, ["src/a.rs", "worktrees/wt/src/b.rs", "worktrees/wt/.git", "pkg/.grimoire.toml"]);
        // A recipe file whatever the languages, and a path a held recipe names or one under it.
        let named = ["ui/button".to_string(), "levels/one.txt".to_string()];
        let paths =
            ["C:/repo/docs/quest.recipe.md", "C:/repo/ui/button/dialog.xml", "C:/repo/levels/one.txt", "C:/repo/levels", "C:/repo/ui/buttons.xml", "C:/repo/lev", "C:/repo/notes.txt"].map(Path::new);
        let handed = to_reextract(paths, |_| false, rel, |r| e.handles(r) || e.is_boundary(r) || named_by(&named, r));
        assert_eq!(handed, ["docs/quest.recipe.md", "ui/button/dialog.xml", "levels/one.txt", "levels"], "a folder holding a named file too");
    }

    /// # Test contract
    ///
    /// ```json
    /// {"test": "a-burst-is-one-reextraction", "checks": ["crates/server/src/watch#events-grouped-over-300ms", "crates/server/src/watch#group-hands-handled-unignored-paths", "crates/server/src/watch#one-reading-however-many-events"]}
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
    /// {"test": "ignored-paths-never-reach-the-daemon", "checks": ["crates/server/src/watch#gitignored-paths-never-reach", "crates/server/src/watch#nearest-gitignore-decides", "crates/server/src/watch#under-git-never-reach", "crates/server/src/watch#no-other-ignore-keeps-one-out", "crates/server/src/watch#changed-gitignore-applies-next-event"]}
    /// ```
    #[test]
    fn ignored_paths_never_reach_the_daemon() {
        let tmp = tempfile::Builder::new().prefix("grimoire-watch-").tempdir().unwrap();
        // The repository sits in a folder whose .gitignore, as a home folder's may, would leave its source out.
        std::fs::write(
            tmp.path().join(".gitignore"),
            "src/
*.rs
",
        )
        .unwrap();
        let dir = tmp.path().join("repo");
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
        assert!(!i.ignores(&dir.join("src").join("lib.rs")), "a .gitignore above the repository keeps nothing out");
        assert!(i.ignores(&dir.join("web").join("dist").join("x.js")), "a nested .gitignore applies below it");
        assert!(!i.ignores(&dir.join("web").join("src").join("a.ts")));
        assert!(!i.ignores(&dir.join("src").join("keep.gen.rs")), "the nearest file re-includes");
        assert!(i.ignores(&dir.join("src").join("other.gen.rs")), "the root's pattern still holds for the rest");
        assert!(!i.ignores(&dir.join("scratch.rs")), ".git/info/exclude is a clone's own, read no more than the walk reads it");
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

    /// # Test contract
    ///
    /// ```json
    /// {"test": "the-daemon-reads-only-what-the-whole-checkout-would", "checks": ["crates/server/src/watch#daemon-reads-only-what-the-whole-reading-would"]}
    /// ```
    #[test]
    fn the_daemon_reads_only_what_the_whole_checkout_would() {
        use grimoire_core::model::{Scope, StoreConfig};
        let tmp = tempfile::Builder::new().prefix("grimoire-watch-reads-").tempdir().unwrap();
        let repo = tmp.path().join("repo");
        for dir in ["src", "pkg/src", ".hidden"] {
            std::fs::create_dir_all(repo.join(dir)).unwrap();
        }
        std::fs::write(
            repo.join("Cargo.toml"),
            "[package]
name = \"w\"
version = \"0.1.0\"
",
        )
        .unwrap();
        std::fs::write(
            repo.join("src").join("lib.rs"),
            "pub mod a;
",
        )
        .unwrap();
        std::fs::write(
            repo.join("src").join("a.rs"),
            "pub fn a() {}
",
        )
        .unwrap();
        // A folder that is a checkout of its own, and a path the repository ignores.
        std::fs::write(
            repo.join("pkg").join(".grimoire.toml"),
            "store = \"elsewhere\"
",
        )
        .unwrap();
        std::fs::write(
            repo.join(".gitignore"),
            "src/ignored.rs
",
        )
        .unwrap();
        let store = tmp.path().join("store");
        let config = StoreConfig {
            large_file: None,
            long_statement: None,
            plain_file: None,
            site: vec![],
            version: StoreConfig::CURRENT_VERSION,
            repository: repo.to_string_lossy().replace('\\', "/"),
            roots: vec!["overview".into()].into(),
            languages: vec!["rust".into()],
            scope: Scope::default(),
            order: Default::default(),
            recipe_kinds: Default::default(),
        };
        crate::store_io::write_all(&store, &grimoire_core::Store::new(config), false).unwrap();
        let _lock = crate::store_io::lock(&store).unwrap();
        let daemon = Arc::new(Daemon::open(&store).unwrap().0);
        let _watching = start(daemon.clone()).unwrap();
        // Files the whole reading would leave out are saved, then one it would read: once that one is in, the earlier events were handled.
        std::fs::write(
            repo.join("pkg").join("src").join("lib.rs"),
            "pub fn p() {}
",
        )
        .unwrap();
        std::fs::write(
            repo.join(".hidden").join("x.rs"),
            "pub fn x() {}
",
        )
        .unwrap();
        std::fs::write(
            repo.join("src").join("ignored.rs"),
            "pub fn i() {}
",
        )
        .unwrap();
        std::thread::sleep(DEBOUNCE * 3);
        std::fs::write(
            repo.join("src").join("c.rs"),
            "pub fn c() {}
",
        )
        .unwrap();
        let deadline = std::time::Instant::now() + Duration::from_secs(30);
        while !daemon.files.lock().unwrap().contains_key("src/c.rs") {
            assert!(std::time::Instant::now() < deadline, "the watcher never handed on src/c.rs");
            std::thread::sleep(Duration::from_millis(100));
        }
        std::thread::sleep(DEBOUNCE * 3);
        let held: Vec<String> = daemon.files.lock().unwrap().keys().cloned().collect();
        assert!(!held.iter().any(|f| f.starts_with("pkg/") || f.starts_with(".hidden/") || f == "src/ignored.rs"), "{held:?}");
        // The extraction is what a whole reading of the checkout makes.
        let fresh = Daemon::open(&store).unwrap().0;
        assert_eq!(held, fresh.files.lock().unwrap().keys().cloned().collect::<Vec<_>>());
    }
}
