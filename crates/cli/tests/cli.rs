//! The `grimoire` binary as a person or a hook runs it, against a temporary
//! repository and store.
// An integration test is not cfg(test), so clippy's tests exemption misses it.
#![allow(clippy::unwrap_used)]

use std::net::TcpListener;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Output};
use std::time::{Duration, Instant};

const BIN: &str = env!("CARGO_BIN_EXE_grimoire");

struct Project {
    dir: PathBuf,
    repo: PathBuf,
    store: PathBuf,
}

impl Drop for Project {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.dir);
    }
}

const CONTRACTED: &str = "//! # Module contract\n//!\n//! ```json\n//! {\"purpose\":\"Say a.\",\"criteria\":[]}\n//! ```\npub fn a() {}\n";

/// A repository with two modules and no contracts, initialised with a store
/// beside it.
fn project() -> (Project, Output) {
    let nanos = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_nanos()).unwrap_or(0);
    let dir = std::env::temp_dir().join(format!("grimoire-cli-{}-{nanos}", std::process::id()));
    let repo = dir.join("repo");
    std::fs::create_dir_all(repo.join("src")).unwrap();
    std::fs::write(repo.join("Cargo.toml"), "[package]\nname = \"c\"\nversion = \"0.1.0\"\n").unwrap();
    std::fs::write(repo.join("src").join("lib.rs"), "pub mod a;\n").unwrap();
    std::fs::write(repo.join("src").join("a.rs"), "pub fn a() {}\n").unwrap();
    let store = dir.join("store");
    let p = Project { dir, repo, store };
    let init = run(&p.repo, &["init", "--store", &p.store.to_string_lossy()]);
    assert!(init.status.success(), "{}", text(&init));
    (p, init)
}

fn run(cwd: &Path, args: &[&str]) -> Output {
    Command::new(BIN).args(args).current_dir(cwd).output().unwrap()
}

fn text(o: &Output) -> String {
    format!("{}{}", String::from_utf8_lossy(&o.stdout), String::from_utf8_lossy(&o.stderr))
}

fn baseline_count(store: &Path) -> usize {
    let b: serde_json::Value = serde_json::from_str(&std::fs::read_to_string(store.join("baseline.json")).unwrap()).unwrap();
    b["findings"].as_array().unwrap().len()
}

/// Every path under a directory, to see nothing was written there.
fn files(dir: &Path) -> Vec<String> {
    let mut out = Vec::new();
    let mut stack = vec![dir.to_path_buf()];
    while let Some(d) = stack.pop() {
        for e in std::fs::read_dir(&d).unwrap().flatten() {
            if e.path().is_dir() {
                stack.push(e.path());
            } else {
                out.push(e.path().to_string_lossy().to_string());
            }
        }
    }
    out.sort();
    out
}

/// # Test contract
///
/// ```json
/// {"test": "init-adopts-what-it-finds", "checks": ["crates/cli/src#init-adopts"]}
/// ```
#[test]
fn init_adopts_what_it_finds() {
    let (p, init) = project();
    assert!(text(&init).contains("finding(s): adopted at init"), "{}", text(&init));
    assert!(baseline_count(&p.store) > 0);
    let check = run(&p.repo, &["check"]);
    assert!(check.status.success(), "the hook init installed passes on the next commit: {}", text(&check));
}

/// # Test contract
///
/// ```json
/// {"test": "check-fails-only-on-new-findings", "checks": ["crates/cli/src#check-without-daemon"]}
/// ```
#[test]
fn check_fails_only_on_new_findings() {
    let (p, _) = project();
    let before = baseline_count(&p.store);
    // A fix prunes the baseline, with no daemon running.
    std::fs::write(p.repo.join("src").join("a.rs"), CONTRACTED).unwrap();
    assert!(run(&p.repo, &["check"]).status.success());
    assert!(baseline_count(&p.store) < before, "check pruned the fixed finding");
    // Breaking it again is new, and fails.
    std::fs::write(p.repo.join("src").join("a.rs"), "pub fn a() {}\n").unwrap();
    let check = run(&p.repo, &["check"]);
    assert!(!check.status.success(), "{}", text(&check));
    assert!(text(&check).contains("module:src/a"), "the new finding is listed: {}", text(&check));
}

/// # Test contract
///
/// ```json
/// {"test": "an-unsupported-language-is-the-first-line", "checks": ["crates/cli/src#unsupported-first"]}
/// ```
#[test]
fn an_unsupported_language_is_the_first_line() {
    let (p, _) = project();
    let path = p.store.join("grimoire.json");
    let mut config: serde_json::Value = serde_json::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap();
    config["languages"] = serde_json::json!(["rust", "cobol"]);
    std::fs::write(&path, config.to_string()).unwrap();
    let check = run(&p.repo, &["check"]);
    let first = String::from_utf8_lossy(&check.stdout).lines().next().unwrap_or_default().to_string();
    assert!(first.contains("`cobol` is not supported"), "first line: {first}");
}

/// # Test contract
///
/// ```json
/// {"test": "the-store-is-found-from-anywhere", "checks": ["crates/cli/src#store-resolution"]}
/// ```
#[test]
fn the_store_is_found_from_anywhere() {
    let (p, _) = project();
    assert!(run(&p.repo.join("src"), &["check"]).status.success(), "from a subdirectory, by the nearest .grimoire.toml");
    let elsewhere = p.dir.join("elsewhere");
    std::fs::create_dir_all(&elsewhere).unwrap();
    assert!(run(&elsewhere, &["check", "--store", &p.store.to_string_lossy()]).status.success(), "--store names it");
    let lost = run(&elsewhere, &["check"]);
    assert!(!lost.status.success());
    assert!(text(&lost).contains("no .grimoire.toml found"), "{}", text(&lost));
}

/// # Test contract
///
/// ```json
/// {"test": "only-init-writes-the-repository", "checks": ["crates/cli/src#repository-untouched"]}
/// ```
#[test]
fn only_init_writes_the_repository() {
    let (p, _) = project();
    let after_init = files(&p.repo);
    assert!(after_init.iter().any(|f| f.ends_with(".grimoire.toml")), "init wrote the integration");
    run(&p.repo, &["check"]);
    run(&p.repo, &["tool", "put_page", r#"{"slug":"goal","title":"Goal","body":"b","links":[]}"#]);
    run(&p.repo, &["adopt", "--reason", "again"]);
    run(&p.repo, &["tool", "status"]);
    assert_eq!(files(&p.repo), after_init, "no other command writes into the repository");
}

struct Serving(Child);

impl Drop for Serving {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}

/// # Test contract
///
/// ```json
/// {"test": "writes-go-through-a-serving-daemon", "checks": ["crates/cli/src#writes-through-daemon"]}
/// ```
#[test]
fn writes_go_through_a_serving_daemon() {
    let (p, _) = project();
    let port = TcpListener::bind("127.0.0.1:0").unwrap().local_addr().unwrap().port();
    let _daemon = Serving(Command::new(BIN).args(["serve", "--port", &port.to_string()]).current_dir(&p.repo).spawn().unwrap());
    let deadline = Instant::now() + Duration::from_secs(30);
    while !p.store.join("serve.json").is_file() {
        assert!(Instant::now() < deadline, "the daemon never marked the store");
        std::thread::sleep(Duration::from_millis(100));
    }
    let filed = run(&p.repo, &["tool", "file_ticket", r#"{"title":"Through the daemon","kind":"change","priority":"later","body":"b"}"#]);
    assert!(filed.status.success(), "{}", text(&filed));
    // The daemon does not watch the store's files, so it knows the ticket only
    // if the write went through it.
    let (ok, board) = grimoire_server::client::call(port, "board", &serde_json::json!({ "priority": ["later"] })).unwrap();
    assert!(ok);
    assert!(board["tickets"].as_array().unwrap().iter().any(|t| t["title"] == "Through the daemon"), "{board}");
}

/// # Test contract
///
/// ```json
/// {"test": "check-and-lint-agree", "checks": ["req:lint-same-check"]}
/// ```
#[test]
fn check_and_the_lint_tool_report_the_same_findings() {
    let (p, _) = project();
    // Something new and something inherited, so both kinds are compared.
    std::fs::write(p.repo.join("src").join("b.rs"), "pub fn b() {}\n").unwrap();
    std::fs::write(p.repo.join("src").join("lib.rs"), "pub mod a;\npub mod b;\n").unwrap();
    let check = run(&p.repo, &["check", "--json"]);
    let lint = run(&p.repo, &["tool", "lint"]);
    let check: serde_json::Value = serde_json::from_slice(&check.stdout).unwrap();
    let lint: serde_json::Value = serde_json::from_slice(&lint.stdout).unwrap();
    assert!(check["new"].as_u64().unwrap() > 0 && check["findings"].as_array().unwrap().len() > check["new"].as_u64().unwrap() as usize);
    assert_eq!(check, lint, "a hook and an agent see the same findings and the same verdict");
}

/// A repository before init: its files, and where its store will go.
fn bare_repo() -> Project {
    let nanos = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_nanos()).unwrap_or(0);
    let dir = std::env::temp_dir().join(format!("grimoire-cli-u-{}-{nanos}", std::process::id()));
    let repo = dir.join("repo");
    std::fs::create_dir_all(repo.join("src")).unwrap();
    std::fs::create_dir_all(repo.join(".git").join("hooks")).unwrap();
    std::fs::write(repo.join("Cargo.toml"), "[package]\nname = \"c\"\nversion = \"0.1.0\"\n").unwrap();
    std::fs::write(repo.join("src").join("lib.rs"), CONTRACTED).unwrap();
    std::fs::write(repo.join("CLAUDE.md"), "# Mine\n").unwrap();
    let store = dir.join("store");
    Project { dir, repo, store }
}

/// # Test contract
///
/// ```json
/// {"test": "init-then-uninstall-leaves-the-repository-as-it-was", "checks": ["crates/cli/src#uninstall-store"]}
/// ```
#[test]
fn init_then_uninstall_leaves_the_repository_as_it_was() {
    let p = bare_repo();
    let before = files(&p.repo);
    let claude = std::fs::read_to_string(p.repo.join("CLAUDE.md")).unwrap();
    assert!(run(&p.repo, &["init", "--store", &p.store.to_string_lossy()]).status.success());
    assert_ne!(files(&p.repo), before);
    let out = run(&p.repo, &["uninstall"]);
    assert!(out.status.success(), "{}", text(&out));
    assert_eq!(files(&p.repo), before, "every file init wrote is gone, and no other");
    assert_eq!(std::fs::read_to_string(p.repo.join("CLAUDE.md")).unwrap(), claude);
    assert!(p.store.join("grimoire.json").is_file(), "the store is kept by default");
    assert!(text(&out).contains("kept    store"), "{}", text(&out));
    // And when asked, the store goes too.
    assert!(run(&p.repo, &["init", "--store", &p.store.to_string_lossy()]).status.success());
    let out = run(&p.repo, &["uninstall", "--delete-store"]);
    assert!(out.status.success(), "{}", text(&out));
    assert!(!p.store.exists(), "{}", text(&out));
    assert_eq!(files(&p.repo), before);
}

/// # Test contract
///
/// ```json
/// {"test": "uninstall-refuses-a-store-it-does-not-own-or-that-is-serving", "checks": ["crates/cli/src#uninstall-store"]}
/// ```
#[test]
fn uninstall_refuses_to_delete_a_store_it_does_not_own_or_that_is_serving() {
    // A store that names another repository: refused, and nothing removed.
    let (a, _) = project();
    let b = bare_repo();
    std::fs::copy(a.repo.join(".grimoire.toml"), b.repo.join(".grimoire.toml")).unwrap();
    let before = files(&b.repo);
    let out = run(&b.repo, &["uninstall", "--delete-store"]);
    assert!(!out.status.success());
    assert!(text(&out).contains("belongs to repository"), "{}", text(&out));
    assert_eq!(files(&b.repo), before, "a refusal changes nothing");
    assert!(a.store.join("grimoire.json").is_file());

    // A store a daemon is serving: refused, and nothing removed.
    let port = TcpListener::bind("127.0.0.1:0").unwrap().local_addr().unwrap().port();
    let _daemon = Serving(Command::new(BIN).args(["serve", "--port", &port.to_string()]).current_dir(&a.repo).spawn().unwrap());
    let deadline = Instant::now() + Duration::from_secs(30);
    while !a.store.join("serve.json").is_file() {
        assert!(Instant::now() < deadline, "the daemon never marked the store");
        std::thread::sleep(Duration::from_millis(100));
    }
    let before = files(&a.repo);
    let out = run(&a.repo, &["uninstall", "--delete-store"]);
    assert!(!out.status.success());
    assert!(text(&out).contains("stop it first"), "{}", text(&out));
    assert_eq!(files(&a.repo), before);
    assert!(a.store.join("grimoire.json").is_file());
}

/// # Test contract
///
/// ```json
/// {"test": "uninstall-never-deletes-more-than-a-store", "checks": ["crates/cli/src#uninstall-store"]}
/// ```
#[test]
fn uninstall_never_deletes_more_than_a_store() {
    // A store made in the repository's own parent: deleting it would take the repository.
    let p = bare_repo();
    let init = run(&p.repo, &["init", "--store", ".."]);
    assert!(init.status.success(), "{}", text(&init));
    let before = files(&p.dir);
    let out = run(&p.repo, &["uninstall", "--delete-store"]);
    assert!(!out.status.success());
    assert!(text(&out).contains("is the repository or holds it"), "{}", text(&out));
    assert_eq!(files(&p.dir), before, "nothing removed, the repository and its parent intact");

    // A store folder holding something a store does not: refused.
    let q = bare_repo();
    assert!(run(&q.repo, &["init", "--store", &q.store.to_string_lossy()]).status.success());
    std::fs::write(q.store.join("notes.txt"), "mine").unwrap();
    let out = run(&q.repo, &["uninstall", "--delete-store"]);
    assert!(!out.status.success());
    assert!(text(&out).contains("which is not part of a store"), "{}", text(&out));
    assert!(q.store.join("notes.txt").is_file() && q.repo.join(".grimoire.toml").is_file());
}

#[test]
fn uninstall_finds_the_project_from_below_and_refuses_a_broken_project_file() {
    let p = bare_repo();
    let before = files(&p.repo);
    assert!(run(&p.repo, &["init", "--store", &p.store.to_string_lossy()]).status.success());
    // Run from a subdirectory: the project above is uninstalled.
    let out = run(&p.repo.join("src"), &["uninstall"]);
    assert!(out.status.success(), "{}", text(&out));
    assert_eq!(files(&p.repo), before);
    // A project file that does not parse: an error, and nothing removed.
    assert!(run(&p.repo, &["init", "--store", &p.store.to_string_lossy()]).status.success());
    std::fs::write(p.repo.join(".grimoire.toml"), "store = [not toml").unwrap();
    let installed = files(&p.repo);
    let out = run(&p.repo, &["uninstall"]);
    assert!(!out.status.success(), "{}", text(&out));
    assert_eq!(files(&p.repo), installed);
}
