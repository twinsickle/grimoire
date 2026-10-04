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
        ".git/hooks/post-commit",
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
/// {"test": "the-cli-answers-as-the-api-does", "checks": ["req:tools-same-answer"]}
/// ```
#[test]
fn the_cli_answers_as_the_api_does() {
    let (p, _) = project();
    // The fixture's contracted module, read from the code on every call.
    let get = r#"{"ref":"module:src/a"}"#;
    let answer = |o: Output| -> serde_json::Value { serde_json::from_slice(&o.stdout).unwrap() };
    // The CLI on its own, the JSON API, and the CLI handing the call to the daemon: one answer.
    let alone = answer(run(&p.repo, &["tool", "get", get]));
    let (_daemon, port) = serve(&p);
    let (ok, over_http) = grimoire_server::client::call(port, "get", &serde_json::from_str(get).unwrap()).unwrap();
    assert!(ok, "{over_http}");
    let through = answer(run(&p.repo, &["tool", "get", get]));
    assert_eq!(alone["node"]["ref"], "module:src/a", "{alone}");
    assert_eq!(over_http, alone, "the JSON API answers as the CLI does on its own");
    assert_eq!(through, alone, "and so does the CLI beside a daemon");
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
    // A store in the middle of a ticket holds a proposal and a current ticket, both a store's own.
    assert!(run(&p.repo, &["init", "--store", &p.store.to_string_lossy()]).status.success());
    assert!(run(&p.repo, &["tool", "file_ticket", A_TICKET]).status.success());
    assert!(run(&p.repo, &["tool", "start_ticket", r#"{"ref":"ticket:0001"}"#]).status.success());
    assert!(run(&p.repo, &["tool", "put_page", r#"{"slug":"goal","title":"Goal","body":"b","links":[]}"#]).status.success());
    assert!(p.store.join("proposals").join("0001.json").is_file() && p.store.join("current.json").is_file());
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

    // A store's name with the wrong type, or a file in a record folder no write makes: refused too.
    std::fs::remove_file(q.store.join("notes.txt")).unwrap();
    type Make = fn(&Path);
    let odd: [(&str, Make); 3] = [
        ("current.json", |p| std::fs::create_dir_all(p.join("mine")).unwrap()),
        ("proposals", |p| std::fs::write(p, "mine").unwrap()),
        ("pages/.personal.tmp-copy", |p| std::fs::write(p, "mine").unwrap()),
    ];
    for (name, make) in odd {
        let path = q.store.join(name);
        make(&path);
        let out = run(&q.repo, &["uninstall", "--delete-store"]);
        assert!(!out.status.success(), "{name}: {}", text(&out));
        assert!(text(&out).contains("which is not part of a store"), "{name}: {}", text(&out));
        assert!(path.exists() && q.store.join("grimoire.json").is_file(), "{name}: nothing removed");
        if path.is_dir() { std::fs::remove_dir_all(&path) } else { std::fs::remove_file(&path) }.unwrap();
    }
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

/// # Test contract
///
/// ```json
/// {"test": "a-commit-lands-the-ticket-waiting-on-it", "checks": ["crates/cli/src#land", "crates/cli/src/init#landing-hook"]}
/// ```
#[test]
fn a_commit_lands_the_ticket_waiting_on_it() {
    // A real git repository, so the hooks init installs are the ones that run,
    // with this build first on the path, since the hooks call `grimoire` by name.
    let p = uninitialised();
    let _ = std::fs::remove_dir_all(p.repo.join(".git"));
    let bin_dir = Path::new(BIN).parent().unwrap().to_path_buf();
    let path = std::env::join_paths(std::iter::once(bin_dir).chain(std::env::split_paths(&std::env::var_os("PATH").unwrap_or_default()))).unwrap();
    let git = |args: &[&str]| Command::new("git").args(args).current_dir(&p.repo).env("PATH", &path).output().expect("this test needs git");
    for args in [&["init", "-q"][..], &["config", "user.email", "t@example.com"], &["config", "user.name", "T"], &["config", "commit.gpgsign", "false"]] {
        assert!(git(args).status.success(), "git {args:?}");
    }
    let init = run(&p.repo, &["init", "--store", &p.store.to_string_lossy()]);
    assert!(init.status.success(), "{}", text(&init));
    let tool = |name: &str, input: &str| -> serde_json::Value {
        let o = run(&p.repo, &["tool", name, input]);
        assert!(o.status.success(), "{name}: {}", text(&o));
        serde_json::from_slice(&o.stdout).unwrap()
    };
    let status = |ticket: &str| tool("get", &format!(r#"{{"ref":"{ticket}"}}"#))["node"]["status"].as_str().unwrap().to_string();
    let commit = |message: &str| {
        assert!(git(&["add", "-A"]).status.success());
        git(&["commit", "-q", "-m", message])
    };
    let ticket = tool("file_ticket", A_TICKET)["node"]["ref"].as_str().unwrap().to_string();
    tool("start_ticket", &format!(r#"{{"ref":"{ticket}"}}"#));
    // A commit made while the ticket is still being worked lands nothing, and is not refused for it.
    let c = commit("work");
    assert!(c.status.success(), "{}", text(&c));
    assert!(text(&c).contains("no ticket was waiting to land"), "{}", text(&c));
    assert_eq!(status(&ticket), "active");
    // The decision is written and the ticket waits; a commit the check refuses lands nothing.
    tool("close_ticket", &format!(r#"{{"ref":"{ticket}","decision":{{"text":"Done."}},"when":"landed"}}"#));
    assert_eq!(status(&ticket), "landing");
    std::fs::write(p.repo.join("src").join("b.rs"), "pub fn b() {}\n").unwrap();
    let refused = commit("a module with no contract");
    assert!(!refused.status.success(), "the check refuses a new finding: {}", text(&refused));
    assert_eq!(status(&ticket), "landing");
    // The next commit that passes the check lands it: the ticket is closed with the decision written before.
    std::fs::remove_file(p.repo.join("src").join("b.rs")).unwrap();
    std::fs::write(p.repo.join("notes.txt"), "landed\n").unwrap();
    let c = commit("land");
    assert!(c.status.success(), "{}", text(&c));
    assert!(text(&c).contains(&format!("landed {ticket}")), "{}", text(&c));
    let closed = tool("get", &format!(r#"{{"ref":"{ticket}"}}"#));
    assert_eq!((closed["node"]["status"].as_str(), closed["node"]["decision"]["text"].as_str()), (Some("closed"), Some("Done.")));
    // Named, a ticket that is not landing is refused, and only then does the command exit non-zero.
    let again = run(&p.repo, &["land", &ticket]);
    assert!(!again.status.success() && text(&again).contains("landing refused"), "{}", text(&again));
    assert!(run(&p.repo, &["land"]).status.success(), "nothing waiting is not a failure");
    // A store that cannot be reached is said, with a non-zero exit, and changes nothing.
    let gone = p.dir.join("no-such-store");
    let unreachable = run(&p.repo, &["land", "--store", &gone.to_string_lossy()]);
    assert!(!unreachable.status.success() && text(&unreachable).contains("is not a store"), "{}", text(&unreachable));
    // `git merge` makes its merge commit without invoking the pre-commit or post-commit hook, so a ticket landing then waits for the command.
    let second = tool("file_ticket", A_TICKET)["node"]["ref"].as_str().unwrap().to_string();
    tool("start_ticket", &format!(r#"{{"ref":"{second}"}}"#));
    assert!(git(&["switch", "-q", "-c", "side"]).status.success());
    std::fs::write(p.repo.join("side.txt"), "side\n").unwrap();
    assert!(commit("on a branch").status.success());
    assert!(git(&["switch", "-q", "-"]).status.success());
    std::fs::write(p.repo.join("main.txt"), "main\n").unwrap();
    assert!(commit("on the main line").status.success());
    tool("close_ticket", &format!(r#"{{"ref":"{second}","decision":{{"text":"Merged."}},"when":"landed"}}"#));
    let merge = git(&["merge", "--no-ff", "-q", "-m", "merge", "side"]);
    assert!(merge.status.success(), "{}", text(&merge));
    assert_eq!(status(&second), "landing", "a merge runs no post-commit hook: {}", text(&merge));
    let by_hand = run(&p.repo, &["land"]);
    assert!(by_hand.status.success() && text(&by_hand).contains(&format!("landed {second}")), "{}", text(&by_hand));
}

/// # Test contract
///
/// ```json
/// {"test": "a-store-is-served-for-a-team-behind-tokens", "checks": ["crates/server/src#shared-serve", "crates/cli/src#token"]}
/// ```
#[test]
fn a_store_is_served_for_a_team_behind_tokens() {
    // A project started as today, then its store served as a team's from another working directory, as on a host.
    let (p, _) = project();
    let store = p.store.to_string_lossy().to_string();
    let host = p.dir.join("host");
    std::fs::create_dir_all(&host).unwrap();
    // Tokens are made beside the store; the token is printed once and the file holds no token.
    let add = run(&host, &["token", "add", "ana", "--store", &store]);
    assert!(add.status.success(), "{}", text(&add));
    let ana = String::from_utf8_lossy(&add.stdout).trim().to_string();
    assert!(ana.starts_with("grm_"), "{ana}");
    let check = String::from_utf8_lossy(&run(&host, &["token", "add", "pipeline-check", "--read-only", "--store", &store]).stdout).trim().to_string();
    assert!(!std::fs::read_to_string(p.store.join("tokens.json")).unwrap().contains(&ana));
    let listed = text(&run(&host, &["token", "list", "--store", &store]));
    assert!(listed.contains("ana\tread and write") && listed.contains("pipeline-check\tread"), "{listed}");
    // A daemon that trusts local callers is not opened to the network.
    let open = run(&host, &["serve", "--store", &store, "--listen", "0.0.0.0:0"]);
    assert!(!open.status.success() && text(&open).contains("--listen needs --shared"), "{}", text(&open));
    // Served shared: the repository the store records plays no part.
    std::fs::remove_dir_all(p.repo.join("src")).unwrap();
    // The port is left to the system, and the marker records the one it got.
    let _daemon = Serving(Command::new(BIN).args(["serve", "--shared", "--store", &store, "--listen", "127.0.0.1:0"]).current_dir(&host).spawn().unwrap());
    let deadline = Instant::now() + Duration::from_secs(30);
    while !p.store.join("serve.json").is_file() {
        assert!(Instant::now() < deadline, "the shared daemon never marked the store");
        std::thread::sleep(Duration::from_millis(100));
    }
    let marker: serde_json::Value = serde_json::from_str(&std::fs::read_to_string(p.store.join("serve.json")).unwrap()).unwrap();
    let port = marker["port"].as_u64().unwrap() as u16;
    assert_ne!(port, 0, "the port it is really listening on: {marker}");
    let bearer = |t: &str| format!("Host: 127.0.0.1\r\nAuthorization: Bearer {t}\r\n");
    assert_eq!(status_of(port, "POST", "/api/status", "Host: 127.0.0.1\r\n", "{}"), 403, "no token, though local");
    assert_eq!(status_of(port, "POST", "/api/status", &bearer(&ana), "{}"), 200);
    assert_eq!(status_of(port, "POST", "/api/file_ticket", &bearer(&ana), A_TICKET), 200);
    assert_eq!(status_of(port, "POST", "/api/file_ticket", &bearer(&check), A_TICKET), 403, "read-only");
    assert_eq!(status_of(port, "POST", "/api/board", &bearer(&check), "{}"), 200);
    // A command beside the daemon sends its token and is answered by it; without one it is not let through.
    let via = Command::new(BIN).args(["tool", "--store", &store, "file_ticket", A_TICKET]).current_dir(&host).env("GRIMOIRE_TOKEN", &ana).output().unwrap();
    assert!(via.status.success(), "{}", text(&via));
    let without = Command::new(BIN).args(["tool", "--store", &store, "file_ticket", A_TICKET]).current_dir(&host).env_remove("GRIMOIRE_TOKEN").output().unwrap();
    assert!(!without.status.success() && text(&without).contains("refused this command") && text(&without).contains("GRIMOIRE_TOKEN"), "{}", text(&without));
    // A read without a token is refused the same way: it does not go round the daemon to a repository that is not there.
    let read = Command::new(BIN).args(["tool", "--store", &store, "board"]).current_dir(&host).env_remove("GRIMOIRE_TOKEN").output().unwrap();
    assert!(!read.status.success() && text(&read).contains("refused this command"), "{}", text(&read));
    // It holds the store's lock like any daemon: a second one is refused.
    let second = run(&host, &["serve", "--shared", "--store", &store, "--port", "0"]);
    assert!(!second.status.success() && text(&second).contains("in use"), "{}", text(&second));
    // Revoked while it serves: refused at once, and revoking a name that is gone fails.
    assert!(run(&host, &["token", "revoke", "ana", "--store", &store]).status.success());
    assert_eq!(status_of(port, "POST", "/api/status", &bearer(&ana), "{}"), 403);
    assert!(!run(&host, &["token", "revoke", "ana", "--store", &store]).status.success());
}

/// # Test contract
///
/// ```json
/// {"test": "a-store-with-tokens-is-deleted-under-their-lock", "checks": ["crates/cli/src#uninstall-store", "crates/server/src/store_io/tokens#digests-only"]}
/// ```
#[test]
fn a_store_with_tokens_is_deleted_under_their_lock() {
    let p = bare_repo();
    let store = p.store.to_string_lossy().to_string();
    assert!(run(&p.repo, &["init", "--store", &store]).status.success());
    assert!(run(&p.repo, &["token", "add", "ana"]).status.success());
    assert!(p.store.join("tokens.json").is_file() && p.store.join("tokens.lock").is_file());
    // While a token command holds the tokens' lock, the deletion waits for it and removes nothing.
    let held = grimoire_server::store_io::tokens::lock(&p.store).unwrap();
    let mut uninstall = Command::new(BIN).args(["uninstall", "--delete-store"]).current_dir(&p.repo).stdout(std::process::Stdio::piped()).stderr(std::process::Stdio::piped()).spawn().unwrap();
    std::thread::sleep(Duration::from_millis(700));
    assert!(uninstall.try_wait().unwrap().is_none(), "the deletion waits for the tokens' lock");
    assert!(p.store.join("tokens.json").is_file() && p.store.join("grimoire.json").is_file());
    drop(held);
    let out = uninstall.wait_with_output().unwrap();
    assert!(out.status.success(), "{}", text(&out));
    assert!(!p.store.exists(), "the store is gone, tokens and their lock with it");
    // A token command that comes after finds no store, and makes no file where it was.
    let late = run(&p.dir, &["token", "add", "late", "--store", &store]);
    assert!(!late.status.success() && !p.store.exists(), "{}", text(&late));
}

/// # Test contract
///
/// ```json
/// {"test": "the-marker-names-the-port-really-listened-on", "checks": ["crates/server/src/daemon#serving-marker"]}
/// ```
#[test]
fn the_marker_names_the_port_really_listened_on() {
    // A daemon on a person's machine, its port left to the system.
    let (p, _) = project();
    let _daemon = Serving(Command::new(BIN).args(["serve", "--port", "0"]).current_dir(&p.repo).spawn().unwrap());
    let deadline = Instant::now() + Duration::from_secs(30);
    while !p.store.join("serve.json").is_file() {
        assert!(Instant::now() < deadline, "the daemon never marked the store");
        std::thread::sleep(Duration::from_millis(100));
    }
    let marker: serde_json::Value = serde_json::from_str(&std::fs::read_to_string(p.store.join("serve.json")).unwrap()).unwrap();
    let port = marker["port"].as_u64().unwrap() as u16;
    assert_ne!(port, 0, "{marker}");
    assert_eq!(status_of(port, "GET", "/api/status", "Host: 127.0.0.1\r\n", ""), 200);
    // So a command beside it finds it and is answered by it.
    let filed = run(&p.repo, &["tool", "file_ticket", A_TICKET]);
    assert!(filed.status.success(), "{}", text(&filed));
}

/// One command in a checkout of a team's project: Grimoire's own files under
/// `home`, and signed in only as `token` says, whatever this machine has set.
fn team(cwd: &Path, home: &Path, token: Option<&str>, args: &[&str]) -> Output {
    let mut c = Command::new(BIN);
    c.args(args).current_dir(cwd).env("GRIMOIRE_HOME", home).env_remove("GRIMOIRE_TOKEN").stdin(std::process::Stdio::null());
    if let Some(t) = token {
        c.env("GRIMOIRE_TOKEN", t);
    }
    c.output().unwrap()
}

/// Wait for a daemon to mark `store` and return the port it listens on.
fn marked_port(store: &Path) -> u16 {
    let deadline = Instant::now() + Duration::from_secs(30);
    while !store.join("serve.json").is_file() {
        assert!(Instant::now() < deadline, "the daemon never marked {}", store.display());
        std::thread::sleep(Duration::from_millis(100));
    }
    let marker: serde_json::Value = serde_json::from_str(&std::fs::read_to_string(store.join("serve.json")).unwrap()).unwrap();
    marker["port"].as_u64().unwrap() as u16
}

/// # Test contract
///
/// ```json
/// {"test": "a-checkout-works-against-a-teams-server", "checks": ["crates/cli/src#init-server", "crates/cli/src#serve-a-server-project", "crates/cli/src/team#login", "crates/cli/src/team#opens-the-copy", "crates/cli/src/team#through-the-server", "crates/server/src#serves-a-server-project", "crates/server/src/http#passes-the-token"]}
/// ```
#[test]
fn a_checkout_works_against_a_teams_server() {
    // The team's server: a store started on one machine, then served shared from a host, with a person's token and a pipeline's.
    let (origin, _) = project();
    let store = origin.store.to_string_lossy().to_string();
    let host = origin.dir.join("host");
    std::fs::create_dir_all(&host).unwrap();
    let made = |name: &str| String::from_utf8_lossy(&run(&host, &["token", "add", name, "--store", &store]).stdout).trim().to_string();
    let (ana, pipeline, again) = (made("ana"), made("pipeline"), made("ana-again"));
    let mut server = Some(Serving(Command::new(BIN).args(["serve", "--shared", "--store", &store, "--listen", "127.0.0.1:0"]).current_dir(&host).spawn().unwrap()));
    let port = marked_port(&origin.store);
    let address = format!("http://127.0.0.1:{port}");
    // A teammate's checkout of the same code, with Grimoire's own files kept apart from this machine's.
    let p = uninitialised();
    let home = p.dir.join("home");
    // init for a team: an address that is not one, or a store as well, stops it before anything is written.
    assert!(!team(&p.repo, &home, None, &["init", "--server", "grimoire.example"]).status.success());
    assert!(!team(&p.repo, &home, None, &["init", "--server", &address, "--store", "mine"]).status.success());
    assert!(!p.repo.join(".grimoire.toml").exists() && !p.repo.join(".claude").exists());
    let init = team(&p.repo, &home, None, &["init", "--server", &format!("{address}/")]);
    assert!(init.status.success() && text(&init).contains("grimoire login"), "{}", text(&init));
    assert!(std::fs::read_to_string(p.repo.join(".grimoire.toml")).unwrap().contains(&format!("server = \"{address}\"")));
    assert!(p.repo.join(".claude").join("skills").join("grimoire").join("SKILL.md").is_file() && !home.exists(), "the integration, and no store");
    // Not signed in: the server refuses, and the command says how to sign in.
    let refused = team(&p.repo, &home, None, &["check"]);
    assert!(!refused.status.success() && text(&refused).contains("grimoire login"), "{}", text(&refused));
    // A token the server does not hold is not kept; the person's own is, once, outside the repository.
    let wrong = team(&p.repo, &home, None, &["login", "--token", "grm_not-held"]);
    assert!(!wrong.status.success() && text(&wrong).contains("refused the token") && !home.join("credentials.json").exists(), "{}", text(&wrong));
    let login = {
        use std::io::Write;
        let mut child = Command::new(BIN).arg("login").current_dir(&p.repo).env("GRIMOIRE_HOME", &home).env_remove("GRIMOIRE_TOKEN").stdin(std::process::Stdio::piped()).stdout(std::process::Stdio::piped()).stderr(std::process::Stdio::piped()).spawn().unwrap();
        writeln!(child.stdin.take().unwrap(), "{ana}").unwrap();
        child.wait_with_output().unwrap()
    };
    assert!(login.status.success() && text(&login).contains("signed in"), "{}", text(&login));
    let kept = std::fs::read_to_string(home.join("credentials.json")).unwrap();
    assert!(kept.contains(&address) && kept.contains(&ana) && !files(&p.repo).iter().any(|f| std::fs::read_to_string(f).is_ok_and(|t| t.contains(&ana))), "kept under the server's address, and nowhere in the repository");
    // The check runs with no daemon here: the server's intent, this checkout's code.
    let check = team(&p.repo, &home, None, &["check"]);
    assert!(check.status.success() && text(&check).contains("finding(s)"), "{}", text(&check));
    let copy = std::fs::read_dir(home.join("copies")).unwrap().flatten().next().unwrap().path();
    assert!(copy.join("grimoire.json").is_file() && copy.join("copy.json").is_file(), "the copy, laid out as a store, outside the repository");
    // A write goes to the server; the ticket it starts is this checkout's current one.
    let filed = team(&p.repo, &home, None, &["tool", "file_ticket", A_TICKET]);
    assert!(filed.status.success(), "{}", text(&filed));
    let ticket = serde_json::from_slice::<serde_json::Value>(&filed.stdout).unwrap()["node"]["ref"].as_str().unwrap().to_string();
    let slug = ticket.trim_start_matches("ticket:").to_string();
    assert!(team(&p.repo, &home, None, &["tool", "start_ticket", &format!(r#"{{"ref":"{ticket}"}}"#)]).status.success());
    let put = team(&p.repo, &home, None, &["tool", "put_page", r#"{"slug":"aim","title":"Aim","body":"b","links":[]}"#]);
    assert!(put.status.success(), "{}", text(&put));
    let bearer = format!("Host: 127.0.0.1\r\nAuthorization: Bearer {ana}\r\n");
    assert_eq!(status_of(port, "POST", "/api/get", &bearer, &format!(r#"{{"ref":"page:aim","ticket":"{slug}"}}"#)), 200, "in the ticket's proposal on the server");
    assert_eq!(status_of(port, "POST", "/api/get", &bearer, r#"{"ref":"page:aim"}"#), 404, "and not in the truth");
    // A commit on a team lands nothing: the hook's unnamed landing says where a ticket lands, and passes.
    let decided = team(&p.repo, &home, None, &["tool", "close_ticket", &format!(r#"{{"ref":"{ticket}","decision":{{"text":"Done."}},"when":"landed"}}"#)]);
    assert!(decided.status.success(), "{}", text(&decided));
    let hook = team(&p.repo, &home, None, &["land"]);
    assert!(hook.status.success() && text(&hook).contains("nothing landed") && text(&hook).contains("main line") && text(&hook).contains(&ticket), "{}", text(&hook));
    // The main line lands it, by name, signed in as a pipeline is: by the environment, with nothing kept.
    let ci = p.dir.join("pipeline-home");
    let landed = team(&p.repo, &ci, Some(&pipeline), &["land", &slug]);
    assert!(landed.status.success() && text(&landed).contains(&format!("landed {ticket}")), "{}", text(&landed));
    assert!(!ci.join("credentials.json").exists());
    assert_eq!(status_of(port, "POST", "/api/get", &bearer, r#"{"ref":"page:aim"}"#), 200, "applied to the server's truth");
    // A daemon in the checkout serves its copy with its code, and a command beside it goes through it.
    {
        let _daemon = Serving(Command::new(BIN).args(["serve", "--port", "0"]).current_dir(&p.repo).env("GRIMOIRE_HOME", &home).env_remove("GRIMOIRE_TOKEN").spawn().unwrap());
        let local = marked_port(&copy);
        assert_eq!(status_of(local, "GET", "/api/status", "Host: 127.0.0.1\r\n", ""), 200);
        let status = team(&p.repo, &home, None, &["tool", "status"]);
        let status: serde_json::Value = serde_json::from_slice(&status.stdout).unwrap();
        assert_eq!((status["server"]["address"].as_str(), status["server"]["reachable"].as_bool(), status["shared"].as_bool()), (Some(address.as_str()), Some(true), Some(false)), "{status}");
        assert!(status["nodes"]["module"].as_u64().unwrap() > 0, "the checkout's code is in its graph");
        let through = team(&p.repo, &home, None, &["tool", "file_ticket", A_TICKET]);
        assert!(through.status.success(), "{}", text(&through));
        // The person's token is revoked while the daemon serves. The next write finds out, and from then
        // nothing is read on in silence, by the daemon or by a command beside it, until they sign in again.
        assert!(run(&host, &["token", "revoke", "ana", "--store", &store]).status.success());
        let refused = team(&p.repo, &home, None, &["tool", "file_ticket", A_TICKET]);
        assert!(!refused.status.success() && text(&refused).contains("store_unavailable") && text(&refused).contains("grimoire login"), "{}", text(&refused));
        let check = team(&p.repo, &home, None, &["check"]);
        assert!(!check.status.success() && text(&check).contains("refused this machine's token"), "{}", text(&check));
        // Signed in again: the daemon takes up by itself, with no restart.
        assert!(team(&p.repo, &home, None, &["login", "--token", &again]).status.success());
        let check = team(&p.repo, &home, None, &["check"]);
        assert!(text(&check).contains("finding(s)") && !text(&check).contains("refused"), "{}", text(&check));
        assert!(team(&p.repo, &home, None, &["tool", "file_ticket", A_TICKET]).status.success());
        // A command run with a token of its own goes to the server as itself, though the daemon it goes through could write.
        let reader = String::from_utf8_lossy(&run(&host, &["token", "add", "reader", "--read-only", "--store", &store]).stdout).trim().to_string();
        let as_reader = team(&p.repo, &home, Some(&reader), &["tool", "file_ticket", A_TICKET]);
        assert!(!as_reader.status.success() && text(&as_reader).contains("may only read"), "{}", text(&as_reader));
    }
    // What a team's project has no use for here says so, and changes nothing.
    let shared = team(&p.repo, &home, None, &["serve", "--shared"]);
    assert!(!shared.status.success() && text(&shared).contains("--store"), "{}", text(&shared));
    let tokens = team(&p.repo, &home, None, &["token", "list"]);
    assert!(!tokens.status.success() && text(&tokens).contains("on the host"), "{}", text(&tokens));
    // The server goes away: the check still runs over the copy and says so; a write is refused.
    server.take();
    let offline = team(&p.repo, &home, None, &["check"]);
    // It fails only as it would with the server there: on the page the landing left unlinked, a new finding.
    assert!(text(&offline).contains("cannot be reached") && text(&offline).contains("as of") && text(&offline).contains("1 new") && text(&offline).contains("page:aim"), "{}", text(&offline));
    let write = team(&p.repo, &home, None, &["tool", "file_ticket", A_TICKET]);
    assert!(!write.status.success() && text(&write).contains("store_unavailable"), "{}", text(&write));
    // With the checkout's daemon running and holding the copy, a command asks it, and says the same.
    let _ = std::fs::remove_file(copy.join("serve.json"));
    {
        let _daemon = Serving(Command::new(BIN).args(["serve", "--port", "0"]).current_dir(&p.repo).env("GRIMOIRE_HOME", &home).env_remove("GRIMOIRE_TOKEN").spawn().unwrap());
        marked_port(&copy);
        let beside = team(&p.repo, &home, None, &["check"]);
        assert!(text(&beside).contains("cannot be reached") && text(&beside).contains("as of") && text(&beside).contains("finding(s)"), "{}", text(&beside));
    }
    // uninstall takes the integration out and keeps the copy; the store is the server's, so there is none to delete.
    let delete = team(&p.repo, &home, None, &["uninstall", "--delete-store"]);
    assert!(!delete.status.success() && text(&delete).contains("Nothing was removed") && p.repo.join(".grimoire.toml").is_file(), "{}", text(&delete));
    let uninstall = team(&p.repo, &home, None, &["uninstall"]);
    assert!(uninstall.status.success() && text(&uninstall).contains("this checkout's copy") && !p.repo.join(".grimoire.toml").exists() && copy.is_dir(), "{}", text(&uninstall));
    // Signing out forgets the token, and says so when there is none.
    let out = team(&p.repo, &home, None, &["logout", "--server", &address]);
    assert!(out.status.success() && text(&out).contains("signed out") && !std::fs::read_to_string(home.join("credentials.json")).unwrap().contains(&again), "{}", text(&out));
    assert!(text(&team(&p.repo, &home, None, &["logout", "--server", &address])).contains("no token was kept"));
}

/// A team's server on a store started as today, with a write token and a
/// read-only one, and a checkout of the same code that names it.
struct TeamFixture {
    origin: Project,
    checkout: Project,
    writer: String,
    reader: String,
    port: u16,
    _server: Serving,
}

fn team_fixture(languages: Option<&[&str]>) -> TeamFixture {
    let (origin, _) = project();
    if let Some(languages) = languages {
        let path = origin.store.join("grimoire.json");
        let mut config: serde_json::Value = serde_json::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap();
        config["languages"] = serde_json::json!(languages);
        std::fs::write(&path, config.to_string()).unwrap();
    }
    let store = origin.store.to_string_lossy().to_string();
    let host = origin.dir.join("host");
    std::fs::create_dir_all(&host).unwrap();
    let writer = String::from_utf8_lossy(&run(&host, &["token", "add", "pipeline", "--store", &store]).stdout).trim().to_string();
    let reader = String::from_utf8_lossy(&run(&host, &["token", "add", "reader", "--read-only", "--store", &store]).stdout).trim().to_string();
    let server = Serving(Command::new(BIN).args(["serve", "--shared", "--store", &store, "--listen", "127.0.0.1:0"]).current_dir(&host).spawn().unwrap());
    let port = marked_port(&origin.store);
    let checkout = uninitialised();
    let home = checkout.dir.join("home");
    assert!(team(&checkout.repo, &home, None, &["init", "--server", &format!("http://127.0.0.1:{port}")]).status.success());
    TeamFixture { origin, checkout, writer, reader, port, _server: server }
}

/// # Test contract
///
/// ```json
/// {"test": "the-main-line-prunes-a-teams-baseline", "checks": ["crates/cli/src#check-main-line", "crates/cli/src#land", "crates/cli/src/team#prunes-from-the-main-line", "crates/cli/src/team#read-now", "crates/server/src/http#prune", "crates/server/src/remote#intent-digest"]}
/// ```
#[test]
fn the_main_line_prunes_a_teams_baseline() {
    let t = team_fixture(None);
    let repo = &t.checkout.repo;
    let (home, ci) = (t.checkout.dir.join("home"), t.checkout.dir.join("pipeline-home"));
    let adopted = baseline_count(&t.origin.store);
    // A branch fixes the inherited findings on src/a: its check passes, and the team's baseline is left alone.
    std::fs::remove_file(repo.join("src").join("a.rs")).unwrap();
    assert!(team(repo, &home, Some(&t.writer), &["check"]).status.success());
    assert_eq!(baseline_count(&t.origin.store), adopted, "a branch prunes nothing");
    // The main line, with a token that may only read: refused, and nothing changes.
    let refused = team(repo, &ci, Some(&t.reader), &["check", "--main-line"]);
    assert!(!refused.status.success() && text(&refused).contains("refused the prune") && text(&refused).contains("may only read"), "{}", text(&refused));
    assert_eq!(baseline_count(&t.origin.store), adopted);
    assert!(!team(repo, &ci, Some(&t.writer), &["check", "--main-line", "--ticket", "x"]).status.success(), "the main line is judged against the truth");
    // With a write token: the check as usual, then the server's baseline loses what the main line no longer produces.
    let pruned = team(repo, &ci, Some(&t.writer), &["check", "--main-line"]);
    assert!(pruned.status.success() && text(&pruned).contains("pruned 2 baselined finding(s)"), "{}", text(&pruned));
    assert_eq!(baseline_count(&t.origin.store), adopted - 2);
    // So breaking it again is new, and fails.
    std::fs::write(repo.join("src").join("a.rs"), "pub fn a() {}\n").unwrap();
    let again = team(repo, &ci, Some(&t.writer), &["check", "--main-line", "--json"]);
    let answer: serde_json::Value = serde_json::from_slice(&again.stdout).unwrap();
    assert!(!again.status.success() && answer["new"].as_u64() == Some(2) && text(&again).contains("pruned 0"), "standard output is the findings alone: {}", text(&again));
    assert_eq!(baseline_count(&t.origin.store), adopted - 2, "what the main line saw and the baseline lacks is never added");
    // A landing on a team prunes against the truth with the ticket applied: src's missing contract is fixed on the main line.
    let tool = |name: &str, input: &str| assert!(team(repo, &home, Some(&t.writer), &["tool", name, input]).status.success(), "{name}");
    let filed = team(repo, &home, Some(&t.writer), &["tool", "file_ticket", A_TICKET]);
    let ticket = serde_json::from_slice::<serde_json::Value>(&filed.stdout).unwrap()["node"]["ref"].as_str().unwrap().to_string();
    tool("start_ticket", &format!(r#"{{"ref":"{ticket}"}}"#));
    tool("close_ticket", &format!(r#"{{"ref":"{ticket}","decision":{{"text":"Done."}},"when":"landed"}}"#));
    std::fs::write(repo.join("src").join("lib.rs"), CONTRACTED).unwrap();
    let landed = team(repo, &ci, Some(&t.writer), &["land", ticket.trim_start_matches("ticket:")]);
    assert!(landed.status.success() && text(&landed).contains(&format!("landed {ticket}")) && text(&landed).contains("pruned 1 baselined finding(s)"), "{}", text(&landed));
    assert_eq!(baseline_count(&t.origin.store), adopted - 3);
    // With a daemon serving the pipeline's checkout, the main line still reads the server, never that daemon's copy.
    let copy = {
        let _daemon = Serving(Command::new(BIN).args(["serve", "--port", "0"]).current_dir(repo).env("GRIMOIRE_HOME", &ci).env("GRIMOIRE_TOKEN", &t.writer).spawn().unwrap());
        let deadline = Instant::now() + Duration::from_secs(30);
        let copy = loop {
            if let Some(c) = std::fs::read_dir(ci.join("copies")).ok().and_then(|mut d| d.next()).and_then(Result::ok) {
                break c.path();
            }
            assert!(Instant::now() < deadline, "the daemon never made its copy");
            std::thread::sleep(Duration::from_millis(100));
        };
        marked_port(&copy);
        let beside = team(repo, &ci, Some(&t.writer), &["check", "--main-line"]);
        assert!(text(&beside).contains("pruned 0 baselined finding(s)"), "{}", text(&beside));
        copy
    };
    // A copy that says what the server does not: the main line neither reads it nor reports it.
    std::fs::write(copy.join("pages").join("planted.json"), r#"{"slug":"planted","title":"Planted","body":"b","links":["req:nowhere"]}"#).unwrap();
    let planted = team(repo, &ci, Some(&t.writer), &["check", "--main-line"]);
    assert!(!text(&planted).contains("page:planted") && text(&planted).contains("pruned 0"), "{}", text(&planted));
    // The route itself: it never adds, it needs a writer, a JSON body naming the intent it read, and nothing else.
    let bearer = |token: &str| format!("Host: 127.0.0.1\r\nAuthorization: Bearer {token}\r\n");
    let digest = |port: u16| {
        let (_, body) = http(port, &format!("GET /api/export HTTP/1.0\r\n{}\r\n", bearer(&t.reader)));
        grimoire_server::remote::intent_digest(&serde_json::from_str(&body).unwrap())
    };
    let stranger = |intent: &str| format!(r#"{{"present":[{{"rule":"unspecified","check":"no-scenario","ref":"req:never-held"}}],"intent":"{intent}"}}"#);
    assert_eq!(status_of(t.port, "POST", "/api/prune", &bearer(&t.reader), &stranger(&digest(t.port))), 403);
    assert_eq!(status_of(t.port, "POST", "/api/prune", &bearer(&t.writer), r#"{"present":[],"intent":"x","more":1}"#), 400);
    assert_eq!(status_of(t.port, "POST", "/api/prune", &bearer(&t.writer), &stranger("an intent that is not this one")), 409, "judged against intent that moved");
    assert_eq!(baseline_count(&t.origin.store), adopted - 3);
    assert_eq!(status_of(t.port, "POST", "/api/prune", &bearer(&t.writer), &stranger(&digest(t.port))), 200);
    // Told only of a finding it never held, every entry goes but what the server's own intent still shows: its
    // two roots, which init named and no page yet carries; none is added.
    let left: serde_json::Value = serde_json::from_str(&std::fs::read_to_string(t.origin.store.join("baseline.json")).unwrap()).unwrap();
    let checks: Vec<&str> = left["findings"].as_array().unwrap().iter().map(|f| f["check"].as_str().unwrap()).collect();
    assert_eq!(checks, ["missing-grimoire-root", "missing-rites-root"]);
    let (status, _) = http(t.port, &format!("POST /api/prune HTTP/1.0\r\n{}Content-Type: text/plain\r\nContent-Length: 2\r\n\r\n{{}}", bearer(&t.writer)));
    assert_eq!(status, 415);
    // An agent is never offered it.
    let (_, tools) = http(t.port, &format!("GET /api/tools HTTP/1.0\r\n{}\r\n", bearer(&t.reader)));
    assert!(!tools.contains("prune"), "{tools}");
    // A daemon that holds code prunes its own baseline, and refuses to be told.
    let (own, _) = project();
    let (_daemon, local) = serve(&own);
    assert_eq!(status_of(local, "POST", "/api/prune", "Host: 127.0.0.1\r\n", &stranger("x")), 409);
    drop(_daemon);
    // On a store of the project's own the flag is the check: it prunes as the check always does, and sends nothing.
    let mine = run(&own.repo, &["check", "--main-line"]);
    assert!(mine.status.success() && !text(&mine).contains("pruned"), "{}", text(&mine));
    // The server goes away: the main line reads nothing stale, prunes nothing, and fails.
    drop(t._server);
    let away = team(repo, &ci, Some(&t.writer), &["check", "--main-line"]);
    assert!(!away.status.success() && text(&away).contains("cannot be reached"), "{}", text(&away));
}

/// One raw HTTP exchange with a daemon: the status and the body.
fn http(port: u16, request: &str) -> (u16, String) {
    use std::io::{Read, Write};
    let mut s = std::net::TcpStream::connect(("127.0.0.1", port)).unwrap();
    s.set_read_timeout(Some(Duration::from_secs(10))).unwrap();
    s.write_all(request.as_bytes()).unwrap();
    let mut answer = String::new();
    s.read_to_string(&mut answer).unwrap();
    let status = answer.split_whitespace().nth(1).unwrap().parse().unwrap();
    (status, answer.split_once("\r\n\r\n").map(|(_, b)| b.to_string()).unwrap_or_default())
}

/// # Test contract
///
/// ```json
/// {"test": "a-build-that-cannot-read-a-language-does-not-prune", "checks": ["crates/cli/src/team#prunes-from-the-main-line", "crates/cli/src#land"]}
/// ```
#[test]
fn a_build_that_cannot_read_a_language_does_not_prune() {
    let t = team_fixture(Some(&["rust", "cobol"]));
    let adopted = baseline_count(&t.origin.store);
    let ci = t.checkout.dir.join("pipeline-home");
    // Every finding fixed, as far as this build can tell.
    std::fs::remove_dir_all(t.checkout.repo.join("src")).unwrap();
    let check = team(&t.checkout.repo, &ci, Some(&t.writer), &["check", "--main-line"]);
    assert!(!check.status.success() && text(&check).contains("the baseline was not pruned") && text(&check).contains("cobol"), "{}", text(&check));
    assert_eq!(baseline_count(&t.origin.store), adopted, "nothing was sent");
    // A landing is taken, then its prune is refused the same way: the command says the ticket landed, and fails.
    let home = t.checkout.dir.join("home");
    let tool = |name: &str, input: &str| team(&t.checkout.repo, &home, Some(&t.writer), &["tool", name, input]);
    let ticket = serde_json::from_slice::<serde_json::Value>(&tool("file_ticket", A_TICKET).stdout).unwrap()["node"]["ref"].as_str().unwrap().to_string();
    assert!(tool("start_ticket", &format!(r#"{{"ref":"{ticket}"}}"#)).status.success());
    assert!(tool("close_ticket", &format!(r#"{{"ref":"{ticket}","decision":{{"text":"Done."}},"when":"landed"}}"#)).status.success());
    let landed = team(&t.checkout.repo, &ci, Some(&t.writer), &["land", ticket.trim_start_matches("ticket:")]);
    assert!(!landed.status.success() && text(&landed).contains(&format!("landed {ticket}")) && text(&landed).contains("the ticket landed, but the baseline was not pruned"), "{}", text(&landed));
    let status = serde_json::from_slice::<serde_json::Value>(&tool("get", &format!(r#"{{"ref":"{ticket}"}}"#)).stdout).unwrap();
    assert_eq!(status["node"]["status"], "closed");
    assert_eq!(baseline_count(&t.origin.store), adopted);
}

/// # Test contract
///
/// ```json
/// {"test": "a-pipeline-checks-a-tickets-code-against-its-proposal", "checks": ["crates/cli/src#check-names-a-ticket", "crates/cli/src/team#opens-the-copy"]}
/// ```
#[test]
fn a_pipeline_checks_a_tickets_code_against_its_proposal() {
    // A team's server, a person's checkout that proposes intent under a ticket, and a pipeline's fresh checkout of the same code.
    let (origin, _) = project();
    let store = origin.store.to_string_lossy().to_string();
    let host = origin.dir.join("host");
    std::fs::create_dir_all(&host).unwrap();
    let ana = String::from_utf8_lossy(&run(&host, &["token", "add", "ana", "--store", &store]).stdout).trim().to_string();
    let pipeline = String::from_utf8_lossy(&run(&host, &["token", "add", "pipeline", "--read-only", "--store", &store]).stdout).trim().to_string();
    let _server = Serving(Command::new(BIN).args(["serve", "--shared", "--store", &store, "--listen", "127.0.0.1:0"]).current_dir(&host).spawn().unwrap());
    let address = format!("http://127.0.0.1:{}", marked_port(&origin.store));
    let p = uninitialised();
    let home = p.dir.join("home");
    assert!(team(&p.repo, &home, None, &["init", "--server", &address]).status.success());
    let tool = |name: &str, input: &str| -> serde_json::Value {
        let o = team(&p.repo, &home, Some(&ana), &["tool", name, input]);
        assert!(o.status.success(), "{name}: {}", text(&o));
        serde_json::from_slice(&o.stdout).unwrap()
    };
    let ticket = tool("file_ticket", A_TICKET)["node"]["ref"].as_str().unwrap().to_string();
    let slug = ticket.trim_start_matches("ticket:").to_string();
    tool("start_ticket", &format!(r#"{{"ref":"{ticket}"}}"#));
    // The proposal names a criterion this code does not have yet.
    tool("put_requirement", r#"{"slug":"proposed","title":"Proposed","statement":"It SHALL offer b.","scenarios":[{"title":"t","given":"g","when":"w","then":"t"}],"covered_by":["criterion:src/b#exports-b"]}"#);
    let open = tool("file_ticket", A_TICKET)["node"]["ref"].as_str().unwrap().to_string();
    // The pipeline: signed in by the environment with a read-only token, nothing kept, no current ticket.
    let ci = p.dir.join("pipeline-home");
    let check = |args: &[&str]| team(&p.repo, &ci, Some(&pipeline), &[&["check"][..], args].concat());
    let truth = check(&[]);
    assert!(truth.status.success() && !text(&truth).contains("req:proposed"), "no ticket named is judged against the truth: {}", text(&truth));
    for named in [slug.as_str(), ticket.as_str()] {
        let judged = check(&["--ticket", named]);
        assert!(!judged.status.success() && text(&judged).contains("req:proposed") && text(&judged).contains("criterion:src/b#exports-b"), "judged against the proposal: {}", text(&judged));
    }
    assert!(!ci.join("credentials.json").exists());
    // A ticket with no proposal to read, and one that does not exist, are said and fail.
    let not_live = check(&["--ticket", &open]);
    assert!(!not_live.status.success() && text(&not_live).contains("neither active nor landing"), "{}", text(&not_live));
    let missing = check(&["--ticket", "no-such-ticket"]);
    assert!(!missing.status.success() && text(&missing).contains("cannot check against ticket:no-such-ticket"), "{}", text(&missing));
    // The person's checkout has the ticket as its current one: with none named, its proposal is the intent; a ticket named overrides it.
    let mine = team(&p.repo, &home, Some(&ana), &["check"]);
    assert!(!mine.status.success() && text(&mine).contains("req:proposed"), "the current ticket's proposal: {}", text(&mine));
    let overridden = team(&p.repo, &home, Some(&ana), &["check", "--ticket", &open]);
    assert!(!overridden.status.success() && text(&overridden).contains("neither active nor landing"), "{}", text(&overridden));
    // A landing ticket keeps its proposal, so the pipeline still judges against it, the same in JSON.
    tool("close_ticket", &format!(r#"{{"ref":"{ticket}","decision":{{"text":"Done."}},"when":"landed"}}"#));
    let landing = check(&["--ticket", &slug, "--json"]);
    let found: serde_json::Value = serde_json::from_slice(&landing.stdout).unwrap();
    assert!(!landing.status.success() && found["new"].as_u64().unwrap() > 0 && found["findings"].as_array().unwrap().iter().any(|f| f["ref"] == "req:proposed"), "{}", text(&landing));
    // Another process holds the runner's copy, so it cannot be fetched now: a named ticket refuses to read it, though the server is there.
    let ci_copy = std::fs::read_dir(ci.join("copies")).unwrap().flatten().next().unwrap().path();
    {
        let _held = grimoire_server::remote::lock_copy(&ci_copy).unwrap();
        let held = check(&["--ticket", &slug]);
        assert!(!held.status.success() && text(&held).contains("another process holds") && !text(&held).contains("req:proposed"), "{}", text(&held));
    }
    // A daemon serving the runner's checkout keeps the copy current: while it says the server answers, a named check reads it.
    let ci_daemon = Serving(Command::new(BIN).args(["serve", "--port", "0"]).current_dir(&p.repo).env("GRIMOIRE_HOME", &ci).env("GRIMOIRE_TOKEN", &pipeline).spawn().unwrap());
    marked_port(&ci_copy);
    let beside = check(&["--ticket", &slug]);
    assert!(!beside.status.success() && text(&beside).contains("req:proposed"), "{}", text(&beside));
    // The server goes away. Once the daemon knows it, a named check beside it fails rather than read the copy.
    drop(_server);
    let deadline = Instant::now() + Duration::from_secs(30);
    let stale = loop {
        let o = check(&["--ticket", &slug]);
        if !text(&o).contains("req:proposed") || Instant::now() > deadline {
            break o;
        }
        std::thread::sleep(Duration::from_millis(200));
    };
    assert!(!stale.status.success() && text(&stale).contains("never a copy fetched before") && !text(&stale).contains("req:proposed"), "{}", text(&stale));
    drop(ci_daemon);
    let _ = std::fs::remove_file(ci_copy.join("serve.json"));
    // With no daemon, the runner's own fetch fails, and a named ticket refuses the copy it holds.
    let away = check(&["--ticket", &slug]);
    assert!(!away.status.success() && text(&away).contains("cannot be reached") && text(&away).contains("never a copy fetched before") && !text(&away).contains("req:proposed"), "{}", text(&away));
    let plain = check(&[]);
    assert!(plain.status.success() && text(&plain).contains("as of"), "with no ticket named the copy is read, as for a person offline: {}", text(&plain));
}
