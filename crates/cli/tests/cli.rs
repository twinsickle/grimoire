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
