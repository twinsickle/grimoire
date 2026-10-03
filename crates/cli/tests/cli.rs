//! The `grimoire` binary as a person or a hook runs it, against a temporary
//! repository and store.
// An integration test is not cfg(test), so clippy's tests exemption misses it.
#![allow(clippy::unwrap_used)]

use std::collections::{BTreeMap, BTreeSet};
use std::net::TcpListener;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Output};
use std::time::{Duration, Instant};

const BIN: &str = env!("CARGO_BIN_EXE_grimoire");

struct Project {
    dir: PathBuf,
    repo: PathBuf,
    store: PathBuf,
    /// Held, not read: dropping it removes the folder.
    _tmp: tempfile::TempDir,
}

const CONTRACTED: &str = "//! # Module contract\n//!\n//! ```json\n//! {\"purpose\":\"Offer a.\",\"criteria\":[{\"id\":\"exports-a\",\"statement\":\"The module exports a function named a.\"}]}\n//! ```\npub fn a() {}\n";

/// A repository with two modules and no contracts, initialised with a store
/// beside it.
fn project() -> (Project, Output) {
    let p = uninitialised();
    let init = run(&p.repo, &["init", "--store", &p.store.to_string_lossy()]);
    assert!(init.status.success(), "{}", text(&init));
    (p, init)
}

/// The same repository before init, and where its store will go.
fn uninitialised() -> Project {
    let tmp = tempfile::Builder::new().prefix("grimoire-cli-").tempdir().unwrap();
    let dir = tmp.path().to_path_buf();
    let repo = dir.join("repo");
    std::fs::create_dir_all(repo.join("src")).unwrap();
    std::fs::write(repo.join("Cargo.toml"), "[package]\nname = \"c\"\nversion = \"0.1.0\"\n").unwrap();
    std::fs::write(repo.join("src").join("lib.rs"), "pub mod a;\n").unwrap();
    std::fs::write(repo.join("src").join("a.rs"), "pub fn a() {}\n").unwrap();
    let store = dir.join("store");
    Project { dir, repo, store, _tmp: tmp }
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
    // A fix prunes the baseline, with no daemon running: the module without a
    // contract goes (a contract would bring a criterion, and findings of its own).
    std::fs::remove_file(p.repo.join("src").join("a.rs")).unwrap();
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
    let p = uninitialised();
    // A git directory, so init writes its hook too.
    std::fs::create_dir_all(p.repo.join(".git").join("hooks")).unwrap();
    let before = state(&p.repo);
    let init = run(&p.repo, &["init", "--store", &p.store.to_string_lossy()]);
    assert!(init.status.success(), "{}", text(&init));
    let after_init = state(&p.repo);
    // Every file's modified time set well into the past, so a later write shows however coarse the clock.
    let past = std::time::UNIX_EPOCH + std::time::Duration::from_secs(1_000_000_000);
    for (at, e) in &after_init {
        if matches!(e, Entry::File(..)) {
            std::fs::File::options().write(true).open(p.repo.join(at)).unwrap().set_modified(past).unwrap();
        }
    }
    let reported: BTreeSet<String> = String::from_utf8_lossy(&init.stdout)
        .lines()
        .filter_map(|l| l.strip_prefix("wrote   "))
        .map(|l| relative(&p.repo, l.split(": ").next().unwrap_or(l)))
        .collect();
    // The whole integration, as req:cli-repository-untouched names it.
    let integration: BTreeSet<String> = [
        ".grimoire.toml",
        ".claude/skills/grimoire/SKILL.md",
        ".claude/skills/grimoire/references/contracts.md",
        ".claude/skills/grimoire/references/changes.md",
        ".agents/skills/grimoire/SKILL.md",
        ".agents/skills/grimoire/references/contracts.md",
        ".agents/skills/grimoire/references/changes.md",
        "CLAUDE.md",
        "AGENTS.md",
        ".mcp.json",
        ".codex/config.toml",
        ".git/hooks/pre-commit",
    ]
    .into_iter()
    .map(String::from)
    .collect();
    assert_eq!(reported, integration, "init reports writing the whole integration");
    // init changes exactly the files it reports, and creates no folder but those holding them.
    let (files, folders): (BTreeSet<String>, BTreeSet<String>) = changed(&before, &after_init).into_iter().partition(|f| matches!(after_init.get(f), Some(Entry::File(..))));
    assert_eq!(files, reported, "init writes the integration it reports, and nothing else");
    for f in &folders {
        assert!(!before.contains_key(f) && reported.iter().any(|r| r.starts_with(&format!("{f}/"))), "init made {f}, which holds nothing it reports");
    }
    // Every other command leaves every folder, byte and modified time as it was.
    let scaffolded = p.dir.join("scaffolded");
    let mut now = state(&p.repo);
    for args in [
        &["check"][..],
        &["tool", "lint"],
        &["tool", "outline"],
        &["tool", "file_ticket", A_TICKET],
        &["tool", "start_ticket", r#"{"ref":"ticket:0001"}"#],
        &["tool", "put_page", r#"{"slug":"goal","title":"Goal","body":"b","links":[]}"#],
        &["tool", "file_ticket", A_TICKET],
        &["adopt", "--reason", "again"],
        &["tool", "status"],
        &["scaffold", "--store", &scaffolded.to_string_lossy()],
    ] {
        let out = run(&p.repo, args);
        assert!(out.status.success(), "{args:?}: {}", text(&out));
        let after = state(&p.repo);
        assert_eq!(changed(&now, &after), BTreeSet::new(), "{args:?} wrote into the repository");
        now = after;
    }
    assert!(scaffolded.join("grimoire.json").is_file(), "scaffold wrote its store, outside the repository");

    // A repository with its own pre-commit hook: init leaves it exactly as it was and says so.
    let own = uninitialised();
    let hook = own.repo.join(".git").join("hooks").join("pre-commit");
    std::fs::create_dir_all(hook.parent().unwrap()).unwrap();
    std::fs::write(&hook, "#!/bin/sh\ncargo test\n").unwrap();
    std::fs::File::options().write(true).open(&hook).unwrap().set_modified(past).unwrap();
    let before = state(&own.repo);
    let init = run(&own.repo, &["init", "--store", &own.store.to_string_lossy()]);
    assert!(init.status.success(), "{}", text(&init));
    let out = String::from_utf8_lossy(&init.stdout);
    assert!(out.lines().any(|l| l.starts_with("skipped ") && relative(&own.repo, l).contains(".git/hooks/pre-commit")), "{out}");
    assert!(!out.lines().any(|l| l.starts_with("wrote ") && l.contains("pre-commit")), "{out}");
    assert_eq!(state(&own.repo).get(".git/hooks/pre-commit"), before.get(".git/hooks/pre-commit"), "the person's hook is untouched, to the modified time");
}

/// One entry under a repository: a folder, or a file with its bytes,
/// modified time and read-only flag. A folder's own modified time is left
/// out: NTFS updates it lazily, so it moves after the fixture's writes
/// whatever runs next.
#[derive(Debug, PartialEq, Eq)]
enum Entry {
    Folder,
    File(Vec<u8>, std::time::SystemTime, bool),
}

/// Every folder and file under `dir`, by its path relative to `dir` with forward slashes.
fn state(dir: &Path) -> BTreeMap<String, Entry> {
    let mut out = BTreeMap::new();
    let mut stack = vec![dir.to_path_buf()];
    while let Some(d) = stack.pop() {
        for e in std::fs::read_dir(&d).unwrap().flatten() {
            let (path, meta) = (e.path(), e.metadata().unwrap());
            let at = relative(dir, &path.to_string_lossy());
            if meta.is_dir() {
                out.insert(at, Entry::Folder);
                stack.push(path);
            } else {
                out.insert(at, Entry::File(std::fs::read(&path).unwrap(), meta.modified().unwrap(), meta.permissions().readonly()));
            }
        }
    }
    out
}

/// The paths added, removed or changed between two states.
fn changed(before: &BTreeMap<String, Entry>, after: &BTreeMap<String, Entry>) -> BTreeSet<String> {
    before.keys().chain(after.keys()).filter(|k| before.get(*k) != after.get(*k)).cloned().collect()
}

/// A path under the repository as init prints it, relative and with forward slashes.
fn relative(repo: &Path, path: &str) -> String {
    let (repo, path) = (repo.to_string_lossy().replace('\\', "/"), path.replace('\\', "/"));
    path.strip_prefix(&format!("{repo}/")).unwrap_or(&path).to_string()
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
    let tmp = tempfile::Builder::new().prefix("grimoire-cli-u-").tempdir().unwrap();
    let dir = tmp.path().to_path_buf();
    let repo = dir.join("repo");
    std::fs::create_dir_all(repo.join("src")).unwrap();
    std::fs::create_dir_all(repo.join(".git").join("hooks")).unwrap();
    std::fs::write(repo.join("Cargo.toml"), "[package]\nname = \"c\"\nversion = \"0.1.0\"\n").unwrap();
    std::fs::write(repo.join("src").join("lib.rs"), CONTRACTED).unwrap();
    std::fs::write(repo.join("CLAUDE.md"), "# Mine\n").unwrap();
    let store = dir.join("store");
    Project { dir, repo, store, _tmp: tmp }
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
/// {"test": "a-refused-codex-config-stops-init-before-anything-is-written", "checks": ["crates/cli/src/init#merge-not-clobber"]}
/// ```
#[test]
fn a_refused_codex_config_stops_init_before_anything_is_written() {
    let p = bare_repo();
    std::fs::create_dir_all(p.repo.join(".codex")).unwrap();
    std::fs::write(p.repo.join(".codex").join("config.toml"), "mcp_servers = { docs = { url = \"x\" } }\n").unwrap();
    let before = files(&p.repo);
    let bytes = |f: &str| std::fs::read(p.repo.join(f)).unwrap();
    let (claude, codex) = (bytes("CLAUDE.md"), bytes(".codex/config.toml"));
    let out = run(&p.repo, &["init", "--store", &p.store.to_string_lossy()]);
    assert!(!out.status.success(), "{}", text(&out));
    assert!(text(&out).contains("config.toml"), "the refusal names the file: {}", text(&out));
    assert_eq!(files(&p.repo), before, "not even .grimoire.toml is written");
    assert_eq!((bytes("CLAUDE.md"), bytes(".codex/config.toml")), (claude, codex));
    assert!(!p.store.exists(), "nor is the store created");
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

/// A daemon serving `p`'s store, once it has marked it.
fn serve(p: &Project) -> (Serving, u16) {
    let port = TcpListener::bind("127.0.0.1:0").unwrap().local_addr().unwrap().port();
    let daemon = Serving(Command::new(BIN).args(["serve", "--port", &port.to_string()]).current_dir(&p.repo).spawn().unwrap());
    let deadline = Instant::now() + Duration::from_secs(30);
    while !p.store.join("serve.json").is_file() {
        assert!(Instant::now() < deadline, "the daemon never marked the store");
        std::thread::sleep(Duration::from_millis(100));
    }
    (daemon, port)
}

/// Every file under a directory with its bytes, to see nothing changed.
fn contents(dir: &Path) -> Vec<(String, Vec<u8>)> {
    files(dir).into_iter().map(|f| { let bytes = std::fs::read(&f).unwrap_or_default(); (f, bytes) }).collect()
}

const A_TICKET: &str = r#"{"title":"T","kind":"change","priority":"later","body":"b"}"#;

/// # Test contract
///
/// ```json
/// {"test": "a-second-daemon-is-refused", "checks": ["crates/server/src#holds-the-lock", "req:store-lock"]}
/// ```
#[test]
fn a_second_daemon_is_refused() {
    let (p, _) = project();
    let (daemon, port) = serve(&p);
    let other = TcpListener::bind("127.0.0.1:0").unwrap().local_addr().unwrap().port();
    let before = contents(&p.store);
    let second = run(&p.repo, &["serve", "--port", &other.to_string()]);
    assert!(!second.status.success(), "{}", text(&second));
    assert!(text(&second).contains(&format!("in use by the daemon serving it on port {port}")), "{}", text(&second));
    assert_eq!(contents(&p.store), before, "the second daemon wrote nothing");
    // Killed, not shut down: the lock goes with the process, and the next writer needs nothing cleared.
    drop(daemon);
    // The marker it leaves is only a likely name, said as such, when another process holds the lock.
    let held = grimoire_server::store_io::lock(&p.store).unwrap();
    let refused = run(&p.repo, &["tool", "file_ticket", A_TICKET]);
    assert!(text(&refused).contains("if that daemon is gone, another grimoire process"), "{}", text(&refused));
    drop(held);
    let filed = run(&p.repo, &["tool", "file_ticket", A_TICKET]);
    assert!(filed.status.success(), "{}", text(&filed));
}

/// # Test contract
///
/// ```json
/// {"test": "a-cli-writer-beside-a-holder-is-refused", "checks": ["crates/cli/src#takes-the-lock", "req:store-lock"]}
/// ```
#[test]
fn a_cli_writer_beside_a_holder_is_refused() {
    let (p, _) = project();
    // This test process holds the lock, as a daemon still starting or another command would.
    let held = grimoire_server::store_io::lock(&p.store).unwrap();
    let before = contents(&p.store);
    for args in [vec!["tool", "file_ticket", A_TICKET], vec!["adopt", "--reason", "again"]] {
        let out = run(&p.repo, &args);
        assert!(!out.status.success(), "{args:?}: {}", text(&out));
        assert!(text(&out).contains("in use by another grimoire process"), "{}", text(&out));
    }
    let scaffold = run(&p.repo, &["scaffold", "--store", &p.store.to_string_lossy(), "--force"]);
    assert!(!scaffold.status.success() && text(&scaffold).contains("in use"), "{}", text(&scaffold));
    // Reads take no lock: a read tool answers, and check reports, without the
    // baseline pruning it would write: a finding init adopted is fixed meanwhile.
    std::fs::remove_file(p.repo.join("src").join("a.rs")).unwrap();
    let read = run(&p.repo, &["tool", "board", "{}"]);
    assert!(read.status.success(), "{}", text(&read));
    let check = run(&p.repo, &["check"]);
    assert!(check.status.success(), "{}", text(&check));
    assert_eq!(contents(&p.store), before, "nothing was written while another held the lock");
    drop(held);
    let adopted = baseline_count(&p.store);
    assert!(run(&p.repo, &["check"]).status.success());
    assert!(baseline_count(&p.store) < adopted, "with the lock free, check prunes what was fixed");
    let filed = run(&p.repo, &["tool", "file_ticket", A_TICKET]);
    assert!(filed.status.success(), "{}", text(&filed));
}

/// # Test contract
///
/// ```json
/// {"test": "a-locked-store-is-not-deleted", "checks": ["crates/cli/src#uninstall-store", "crates/cli/src#takes-the-lock"]}
/// ```
#[test]
fn a_locked_store_is_not_deleted() {
    let p = bare_repo();
    assert!(run(&p.repo, &["init", "--store", &p.store.to_string_lossy()]).status.success());
    let held = grimoire_server::store_io::lock(&p.store).unwrap();
    let before = files(&p.repo);
    let out = run(&p.repo, &["uninstall", "--delete-store"]);
    assert!(!out.status.success());
    assert!(text(&out).contains("Nothing was removed") && text(&out).contains("in use"), "{}", text(&out));
    assert_eq!(files(&p.repo), before, "a refusal changes nothing");
    assert!(p.store.join("grimoire.json").is_file());
    drop(held);
    let out = run(&p.repo, &["uninstall", "--delete-store"]);
    assert!(out.status.success(), "{}", text(&out));
    assert!(!p.store.exists(), "the store goes, lock file and all: {}", text(&out));
}

/// # Test contract
///
/// ```json
/// {"test": "a-store-inside-the-repository-is-refused", "checks": ["crates/server/src/store_io/location#outside-repository", "req:store-location"]}
/// ```
#[test]
fn a_store_inside_the_repository_is_refused() {
    // init: refused before anything is written, the store included.
    let p = bare_repo();
    let before = contents(&p.repo);
    let out = run(&p.repo, &["init", "--store", "./local-store"]);
    assert!(!out.status.success());
    assert!(text(&out).contains("inside the repository"), "{}", text(&out));
    assert_eq!(contents(&p.repo), before, "nothing written: {}", text(&out));
    // scaffold too.
    let out = run(&p.repo, &["scaffold", "--store", "./local-store"]);
    assert!(!out.status.success() && text(&out).contains("inside the repository"), "{}", text(&out));
    assert_eq!(contents(&p.repo), before);
    // A relative --store is the repository's, as the project file will read it, wherever init runs from.
    let elsewhere = p.dir.join("elsewhere");
    std::fs::create_dir_all(&elsewhere).unwrap();
    let repo_arg = p.repo.to_string_lossy().to_string();
    let out = run(&elsewhere, &["init", "--repo", &repo_arg, "--store", "./local-store"]);
    assert!(!out.status.success() && text(&out).contains("inside the repository"), "{}", text(&out));
    assert!(!elsewhere.join("local-store").exists() && contents(&p.repo) == before, "nothing written anywhere");
    let out = run(&elsewhere, &["init", "--repo", &repo_arg, "--store", "../beside"]);
    assert!(out.status.success(), "{}", text(&out));
    assert!(p.dir.join("beside").join("grimoire.json").is_file(), "beside the repository, not beside the working directory");
    let check = run(&p.repo.join("src"), &["check"]);
    assert!(check.status.success(), "the project file finds the same store: {}", text(&check));
    // A store that is inside by a hand edit: every command that loads it refuses.
    let (q, _) = project();
    let inside = q.repo.join("local-store");
    std::fs::create_dir_all(&inside).unwrap();
    for e in std::fs::read_dir(&q.store).unwrap().flatten() {
        if e.file_name() == grimoire_server::store_io::LOCK_FILE {
            continue;
        }
        if e.path().is_file() {
            std::fs::copy(e.path(), inside.join(e.file_name())).unwrap();
        } else {
            std::fs::create_dir_all(inside.join(e.file_name())).unwrap();
        }
    }
    std::fs::write(q.repo.join(".grimoire.toml"), "store = \"./local-store\"\n").unwrap();
    for args in [vec!["check"], vec!["tool", "board", "{}"], vec!["tool", "file_ticket", A_TICKET], vec!["serve", "--port", "0"], vec!["uninstall", "--delete-store"]] {
        let out = run(&q.repo, &args);
        assert!(!out.status.success(), "{args:?}: {}", text(&out));
        assert!(text(&out).contains("inside the repository"), "{args:?}: {}", text(&out));
    }
    assert!(!inside.join(grimoire_server::store_io::LOCK_FILE).exists(), "refused before any lock file was made");
    assert!(inside.join("grimoire.json").is_file() && q.repo.join(".grimoire.toml").is_file(), "and nothing deleted");
}

/// # Test contract
///
/// ```json
/// {"test": "uninstall-finishes-a-deletion-stopped-part-way", "checks": ["crates/cli/src#uninstall-store"]}
/// ```
#[test]
fn uninstall_finishes_a_deletion_stopped_part_way() {
    // A person's file inside a record folder makes the folder more than a store.
    let p = bare_repo();
    assert!(run(&p.repo, &["init", "--store", &p.store.to_string_lossy()]).status.success());
    std::fs::create_dir_all(p.store.join("pages").join("archive")).unwrap();
    std::fs::write(p.store.join("pages").join("archive").join("photo.jpg"), "mine").unwrap();
    let out = run(&p.repo, &["uninstall", "--delete-store"]);
    assert!(!out.status.success() && text(&out).contains("pages/archive"), "{}", text(&out));
    assert!(p.store.join("pages").join("archive").join("photo.jpg").is_file());
    std::fs::remove_dir_all(p.store.join("pages").join("archive")).unwrap();

    // Stopped after grimoire.json went: the lock file and empty record folders are left, and .grimoire.toml.
    for e in std::fs::read_dir(&p.store).unwrap().flatten() {
        let name = e.file_name().to_string_lossy().to_string();
        if e.path().is_dir() {
            std::fs::remove_dir_all(e.path()).unwrap();
            std::fs::create_dir(e.path()).unwrap();
        } else if name != grimoire_server::store_io::LOCK_FILE {
            std::fs::remove_file(e.path()).unwrap();
        }
    }
    let out = run(&p.repo, &["uninstall", "--delete-store"]);
    assert!(out.status.success(), "{}", text(&out));
    assert!(!p.store.exists() && !p.repo.join(".grimoire.toml").exists(), "{}", text(&out));

    // Stopped after the folder went but before .grimoire.toml did.
    let q = bare_repo();
    assert!(run(&q.repo, &["init", "--store", &q.store.to_string_lossy()]).status.success());
    std::fs::remove_dir_all(&q.store).unwrap();
    let out = run(&q.repo, &["uninstall", "--delete-store"]);
    assert!(out.status.success(), "{}", text(&out));
    assert!(!q.repo.join(".grimoire.toml").exists());
}

/// # Test contract
///
/// ```json
/// {"test": "an-explicit-store-resolves-as-the-project-file-does", "checks": ["crates/server/src/store_io/location#resolution", "crates/cli/src#store-resolution"]}
/// ```
#[test]
fn an_explicit_store_resolves_as_the_project_file_does() {
    let (p, _) = project();
    // A relative --store from below the repository is the repository's, as .grimoire.toml would read it.
    let rel = format!("../{}", p.store.file_name().unwrap().to_string_lossy());
    let check = run(&p.repo.join("src"), &["check", "--store", &rel]);
    assert!(check.status.success(), "{}", text(&check));
    // A bare name is a name, even where a folder of that name happens to sit in the working directory.
    std::fs::create_dir_all(p.repo.join("src").join("store")).unwrap();
    let named = run(&p.repo.join("src"), &["check", "--store", "store"]);
    assert!(!named.status.success(), "{}", text(&named));
    assert!(text(&named).contains("stores"), "looked under the stores directory: {}", text(&named));
}

/// One raw HTTP/1.0 exchange with a daemon: the status code.
fn status_of(port: u16, method: &str, path: &str, extra_headers: &str, body: &str) -> u16 {
    use std::io::{Read, Write};
    let mut s = std::net::TcpStream::connect(("127.0.0.1", port)).unwrap();
    s.set_read_timeout(Some(Duration::from_secs(10))).unwrap();
    write!(s, "{method} {path} HTTP/1.0\r\n{extra_headers}Content-Type: application/json\r\nContent-Length: {}\r\n\r\n{body}", body.len()).unwrap();
    let mut answer = String::new();
    s.read_to_string(&mut answer).unwrap();
    answer.split_whitespace().nth(1).unwrap().parse().unwrap()
}

/// # Test contract
///
/// ```json
/// {"test": "the-daemon-runs-the-local-policy", "checks": ["crates/server/src#local-policy-installed"]}
/// ```
#[test]
fn the_daemon_runs_the_local_policy() {
    let (p, _) = project();
    let (_daemon, port) = serve(&p);
    let page = r#"{"slug":"goal","title":"Goal","body":"b","links":[]}"#;
    // A caller on this machine may read and write; a change of intent lands in a started ticket.
    assert_eq!(status_of(port, "GET", "/api/status", "Host: 127.0.0.1\r\n", ""), 200);
    assert_eq!(status_of(port, "POST", "/api/file_ticket", "Host: 127.0.0.1\r\n", A_TICKET), 200);
    assert_eq!(status_of(port, "POST", "/api/start_ticket", "Host: 127.0.0.1\r\n", r#"{"ref":"ticket:0001"}"#), 200);
    assert_eq!(status_of(port, "POST", "/api/put_page", "Host: 127.0.0.1\r\n", page), 200);
    // A page on another site, or a name rebound to this machine, is refused.
    assert_eq!(status_of(port, "POST", "/api/put_page", "Host: 127.0.0.1\r\nOrigin: https://example.com\r\n", page), 403);
    assert_eq!(status_of(port, "GET", "/api/status", "Host: evil.example\r\n", ""), 403);
}

/// # Test contract
///
/// ```json
/// {"test": "init-re-points-a-moved-checkout-and-refuses-a-copy", "checks": ["crates/cli/src#init-repoints", "req:store-config-record"]}
/// ```
#[test]
fn init_re_points_a_moved_checkout_and_refuses_a_copy() {
    let (p, _) = project();
    let store = p.store.to_string_lossy().to_string();
    // A page written under a ticket and landed in the truth by its close.
    assert!(run(&p.repo, &["tool", "file_ticket", A_TICKET]).status.success());
    assert!(run(&p.repo, &["tool", "start_ticket", r#"{"ref":"ticket:0001"}"#]).status.success());
    assert!(run(&p.repo, &["tool", "put_page", r#"{"slug":"goal","title":"Goal","body":"b","links":[]}"#]).status.success());
    assert!(run(&p.repo, &["tool", "close_ticket", r#"{"ref":"ticket:0001","decision":{"text":"Done."}}"#]).status.success());
    // A copy: the first checkout still exists, so the store stays its own.
    let copy = p.dir.join("copy");
    std::fs::create_dir_all(copy.join("src")).unwrap();
    std::fs::write(copy.join("Cargo.toml"), "[package]\nname = \"c\"\nversion = \"0.1.0\"\n").unwrap();
    let out = run(&copy, &["init", "--store", &store]);
    assert!(!out.status.success() && text(&out).contains("still exists"), "{}", text(&out));
    assert!(!copy.join(".grimoire.toml").exists(), "a refusal writes nothing");
    // A move: the checkout is renamed, .grimoire.toml and all, and the old path is gone.
    let moved = p.dir.join("moved");
    std::fs::rename(&p.repo, &moved).unwrap();
    let out = run(&moved, &["init", "--store", &store]);
    assert!(out.status.success(), "{}", text(&out));
    assert!(text(&out).contains("re-pointed store"), "{}", text(&out));
    let goal = run(&moved, &["tool", "get", r#"{"ref":"page:goal"}"#]);
    assert!(goal.status.success() && text(&goal).contains("\"Goal\""), "the history moved with it: {}", text(&goal));
    // Moving re-points without re-baselining: the baseline is the one init made the first time.
    let baseline: serde_json::Value = serde_json::from_str(&std::fs::read_to_string(p.store.join("baseline.json")).unwrap()).unwrap();
    assert_eq!(baseline["reason"], "adopted at init");
    // A file where the old checkout was is not the old checkout: a move from there is still a move.
    let elsewhere = p.dir.join("elsewhere");
    std::fs::rename(&moved, &elsewhere).unwrap();
    std::fs::write(&moved, "not a repository").unwrap();
    let out = run(&elsewhere, &["init", "--store", &store]);
    assert!(out.status.success() && text(&out).contains("re-pointed store"), "{}", text(&out));
    std::fs::remove_file(&moved).unwrap();
    std::fs::rename(&elsewhere, &moved).unwrap();
    assert!(run(&moved, &["init", "--store", &store]).status.success());
    // A checkout whose project file names some other store is not this project moved.
    let stranger = p.dir.join("stranger");
    std::fs::create_dir_all(&stranger).unwrap();
    std::fs::write(stranger.join(".grimoire.toml"), "store = \"../other-store\"\n").unwrap();
    std::fs::rename(&moved, p.dir.join("gone")).unwrap();
    let out = run(&stranger, &["init", "--store", &store]);
    assert!(!out.status.success() && text(&out).contains("does not name this store"), "{}", text(&out));
}

/// # Test contract
///
/// ```json
/// {"test": "init-adopts-only-a-store-it-made", "checks": ["crates/cli/src#init-adopts"]}
/// ```
#[test]
fn init_adopts_only_a_store_it_made() {
    let (p, first) = project();
    assert!(text(&first).contains("adopted at init"), "a new store is adopted: {}", text(&first));
    let baseline = std::fs::read(p.store.join("baseline.json")).unwrap();
    // Something new, then init again: the baseline stays as it was, so the new finding still counts.
    std::fs::write(p.repo.join("src").join("b.rs"), "pub fn b() {}\n").unwrap();
    std::fs::write(p.repo.join("src").join("lib.rs"), "pub mod a;\npub mod b;\n").unwrap();
    let again = run(&p.repo, &["init", "--store", &p.store.to_string_lossy(), "--languages", " rust "]);
    assert!(again.status.success(), "{}", text(&again));
    assert!(text(&again).contains("kept    the store's baseline"), "{}", text(&again));
    assert_eq!(std::fs::read(p.store.join("baseline.json")).unwrap(), baseline, "re-running init does not re-baseline");
    assert!(!run(&p.repo, &["check"]).status.success(), "the new finding is new, not hidden");
}

/// # Test contract
///
/// ```json
/// {"test": "init-trims-what-it-is-given", "checks": ["crates/server/src/store_io#whole-store-valid", "req:store-field-rules"]}
/// ```
#[test]
fn init_trims_what_it_is_given() {
    let p = bare_repo();
    let out = run(&p.repo, &["init", "--store", &p.store.to_string_lossy(), "--languages", " rust "]);
    assert!(out.status.success(), "{}", text(&out));
    let config: serde_json::Value = serde_json::from_str(&std::fs::read_to_string(p.store.join("grimoire.json")).unwrap()).unwrap();
    assert_eq!(config["languages"], serde_json::json!(["rust"]));
    assert_eq!(config["version"], 2, "a new store is written at the current version");
}

/// # Test contract
///
/// ```json
/// {"test": "init-adopts-a-scaffolded-store", "checks": ["crates/cli/src#init-adopts"]}
/// ```
#[test]
fn init_adopts_a_scaffolded_store() {
    // The README's setup: scaffold a store from the contracts, then init. The store has no baseline yet, so init adopts.
    let p = bare_repo();
    let store = p.store.to_string_lossy().to_string();
    let scaffold = run(&p.repo, &["scaffold", "--store", &store]);
    assert!(scaffold.status.success(), "{}", text(&scaffold));
    let init = run(&p.repo, &["init", "--store", &store]);
    assert!(init.status.success() && text(&init).contains("adopted at init"), "{}", text(&init));
    let check = run(&p.repo, &["check"]);
    assert!(check.status.success(), "the hook passes on the next commit: {}", text(&check));
}
