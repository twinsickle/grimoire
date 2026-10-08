//! # Module contract
//!
//! ```json
//! {"purpose":"Find the code a checkout's branch was cut from, so a check can tell its own work from what reached the main line since: the main line, the merge base of HEAD with it, and that commit's code read from git and extracted.","criteria":[{"id":"main-line","statement":"main_line names a repository's main line: the branch the remote origin's HEAD names, else a local main, else a local master; base_commit answers the merge base of HEAD with it, and none outside git, which it tells from the folders above the checkout without running git, with no main line, or when HEAD is the main line's own branch."},{"id":"base-code","statement":"base_code writes a commit's tree into a folder kept for it outside the repository, through an index of its own, so the checkout's index, HEAD and files are untouched, reuses a folder already written for that commit, removes another commit's folder only once it is an hour old, so a check still reading it is not cut short, and extracts it in the languages given from the checkout's place in the tree, refusing the whole base when any of it could not be read rather than judging against part of it; give hands a state the code of its repository's base, or none when it has none."}]}
//! ```

use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};
use std::path::{Path, PathBuf};
use std::process::Command;

use grimoire_core::extraction::Extraction;
use grimoire_extract::Extractor;

/// What `git -C repo <args>` prints, trimmed, or none when it fails or git is not there.
fn git(repo: &Path, args: &[&str]) -> Option<String> {
    let out = Command::new("git").arg("-C").arg(repo).args(args).output().ok()?;
    out.status.success().then(|| String::from_utf8_lossy(&out.stdout).trim().to_string())
}

/// The repository's main line: the branch the remote origin's HEAD names, else a local `main`, else a
/// local `master`; none outside git or when it has none of them.
pub fn main_line(repo: &Path) -> Option<String> {
    if let Some(remote) = git(repo, &["symbolic-ref", "--quiet", "--short", "refs/remotes/origin/HEAD"]).filter(|r| !r.is_empty()) {
        return Some(remote);
    }
    ["main", "master"].into_iter().find(|b| git(repo, &["rev-parse", "--verify", "--quiet", &format!("refs/heads/{b}")]).is_some()).map(str::to_string)
}

/// The commit this checkout's branch was cut from: the merge base of HEAD with the main line, or
/// none on the main line itself, outside git, or with no main line.
pub fn base_commit(repo: &Path) -> Option<String> {
    // Outside git there is nothing to ask it: told from the folders above, with no process started.
    if !repo.ancestors().any(|d| d.join(".git").exists()) {
        return None;
    }
    let line = main_line(repo)?;
    let line_branch = line.strip_prefix("origin/").unwrap_or(&line);
    if git(repo, &["symbolic-ref", "--quiet", "--short", "HEAD"]).as_deref() == Some(line_branch) {
        return None;
    }
    git(repo, &["merge-base", "HEAD", &line]).filter(|c| !c.is_empty())
}

/// Where the base's code is kept for `repo`: outside it, one folder per repository.
fn kept_for(repo: &Path) -> PathBuf {
    let mut h = DefaultHasher::new();
    repo.canonicalize().unwrap_or_else(|_| repo.to_path_buf()).hash(&mut h);
    std::env::temp_dir().join("grimoire-bases").join(format!("{:016x}", h.finish()))
}

/// Write `commit`'s tree into `dir` through an index of its own, leaving the checkout's index alone.
fn write_tree(repo: &Path, commit: &str, dir: &Path, index: &Path) -> Result<(), String> {
    for args in [vec!["read-tree", commit], vec!["checkout-index", "--all", "--force"]] {
        let out = Command::new("git").arg("-C").arg(repo).arg("--work-tree").arg(dir).args(&args).env("GIT_INDEX_FILE", index).output().map_err(|e| format!("git could not be run: {e}"))?;
        if !out.status.success() {
            return Err(format!("git {} failed: {}", args[0], String::from_utf8_lossy(&out.stderr).trim()));
        }
    }
    Ok(())
}

/// The code of `commit` as this checkout would extract it, in `languages`: the commit's tree written
/// once into a folder kept outside the repository, and extracted from the checkout's place in it.
pub fn base_code(repo: &Path, commit: &str, languages: &[String]) -> Result<Extraction, String> {
    let kept = kept_for(repo);
    let dir = kept.join(commit);
    if !dir.join(".written").exists() {
        let pid = std::process::id();
        let (tmp, index) = (kept.join(format!("{commit}.{pid}")), kept.join(format!("{commit}.{pid}.index")));
        std::fs::create_dir_all(&tmp).map_err(|e| format!("{} could not be made: {e}", tmp.display()))?;
        let written = write_tree(repo, commit, &tmp, &index);
        let _ = std::fs::remove_file(&index);
        if let Err(e) = written {
            let _ = std::fs::remove_dir_all(&tmp);
            return Err(e);
        }
        let _ = std::fs::write(tmp.join(".written"), commit);
        // Another check may have written it meanwhile: either copy will do.
        if std::fs::rename(&tmp, &dir).is_err() {
            let _ = std::fs::remove_dir_all(&tmp);
        }
        // Older bases go, but only once no check can still be reading them.
        let old = |e: &std::fs::DirEntry| e.metadata().and_then(|m| m.modified()).ok().and_then(|t| t.elapsed().ok()).is_some_and(|age| age > std::time::Duration::from_secs(3600));
        if let Ok(entries) = std::fs::read_dir(&kept) {
            for e in entries.flatten().filter(|e| e.path() != dir && old(e)) {
                let _ = std::fs::remove_dir_all(e.path());
            }
        }
    }
    // A checkout may be a folder inside its repository: extracted from the same place in the base.
    let prefix = git(repo, &["rev-parse", "--show-prefix"]).unwrap_or_default();
    let root = dir.join(prefix.trim_end_matches('/'));
    let (files, errors) = Extractor::for_languages(&root, languages).extract_all().map_err(|e| format!("the base could not be read: {e}"))?;
    // Judged against part of a base, a branch's own break could pass as the main line's: none, then.
    if let Some(e) = errors.first() {
        return Err(format!("the base could not be read whole: {e}"));
    }
    Ok(Extraction::from_files(files.iter()))
}

/// Give `state` the code of `repo`'s base, or none when it has none; why the base could not be
/// read, when it could not, findings then failing as before.
pub fn give(state: &mut grimoire_core::tools::State, repo: &Path) -> Result<(), String> {
    let Some(commit) = base_commit(repo) else {
        state.set_base(None);
        return Ok(());
    };
    let languages = state.store().config.languages.clone();
    let code = base_code(repo, &commit, &languages).map_err(|e| format!("the base {commit} could not be read, so every new finding fails: {e}"))?;
    state.set_base(Some(code));
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run(dir: &Path, args: &[&str]) {
        let ok = Command::new("git").arg("-C").arg(dir).args(["-c", "user.name=t", "-c", "user.email=t@t", "-c", "commit.gpgsign=false"]).args(args).output().unwrap();
        assert!(ok.status.success(), "git {args:?}: {}", String::from_utf8_lossy(&ok.stderr));
    }

    fn contract(criterion: &str) -> String {
        format!("//! # Module contract\n//!\n//! ```json\n//! {{\"purpose\":\"p\",\"criteria\":[{{\"id\":\"{criterion}\",\"statement\":\"s\"}}]}}\n//! ```\n")
    }

    /// # Test contract
    ///
    /// ```json
    /// {"test": "a-branch-finds-the-code-it-was-cut-from", "checks": ["crates/server/src/base#main-line", "crates/server/src/base#base-code"]}
    /// ```
    #[test]
    fn a_branch_finds_the_code_it_was_cut_from() {
        let tmp = tempfile::tempdir().unwrap();
        let repo = tmp.path().join("shop");
        std::fs::create_dir_all(repo.join("src")).unwrap();
        // Outside git there is no base.
        assert_eq!(base_commit(&repo), None);
        run(&repo, &["init", "-q", "-b", "main"]);
        std::fs::write(repo.join("src/lib.rs"), contract("at-base")).unwrap();
        run(&repo, &["add", "-A"]);
        run(&repo, &["commit", "-q", "-m", "base"]);
        let base = git(&repo, &["rev-parse", "HEAD"]).unwrap();
        assert_eq!(main_line(&repo).as_deref(), Some("main"));
        assert_eq!(base_commit(&repo), None, "on the main line there is no base");
        // A branch, and the main line moving on after it was cut.
        run(&repo, &["checkout", "-q", "-b", "feature"]);
        std::fs::write(repo.join("src/lib.rs"), contract("on-the-branch")).unwrap();
        run(&repo, &["commit", "-q", "-am", "branch"]);
        run(&repo, &["checkout", "-q", "main"]);
        std::fs::write(repo.join("src/lib.rs"), contract("bottled-since")).unwrap();
        run(&repo, &["commit", "-q", "-am", "main moves on"]);
        run(&repo, &["checkout", "-q", "feature"]);
        assert_eq!(base_commit(&repo).as_deref(), Some(base.as_str()), "the merge base, not the main line's tip");
        // The base's code, extracted as the checkout would be; the checkout itself untouched.
        std::fs::write(repo.join("src/lib.rs"), contract("uncommitted")).unwrap();
        let code = base_code(&repo, &base, &["rust".to_string()]).unwrap();
        let criteria: Vec<String> = code.modules.values().flat_map(|m| m.criteria.iter().map(|c| c.id.clone())).collect();
        assert_eq!(criteria, vec!["at-base"]);
        assert_eq!(git(&repo, &["status", "--porcelain"]).unwrap(), "M src/lib.rs", "only the change in hand, index untouched");
        assert!(kept_for(&repo).join(&base).join(".written").exists(), "kept for the next check");
        assert!(base_code(&repo, &base, &["rust".to_string()]).is_ok(), "and reused");
    }
}
