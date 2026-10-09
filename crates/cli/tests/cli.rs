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

/// The binary, with a data directory of the test's own unless the test sets
/// one: a daemon serves the machine's library, and no test may reach the real
/// one, or another test's. Each test runs on a thread of its own.
fn grimoire() -> Command {
    let mut c = Command::new(BIN);
    c.env("GRIMOIRE_HOME", test_home());
    c
}

/// The data directory this test's commands use.
fn test_home() -> PathBuf {
    thread_local! {
        static HOME: tempfile::TempDir = tempfile::Builder::new().prefix("grimoire-cli-home-").tempdir().unwrap();
    }
    HOME.with(|home| home.path().to_path_buf())
}

struct Project {
    dir: PathBuf,
    repo: PathBuf,
    store: PathBuf,
    /// Held, not read: dropping it removes the folder.
    _tmp: tempfile::TempDir,
}

const CONTRACTED: &str =
    "//! # Module contract\n//!\n//! ```json\n//! {\"purpose\":\"Offer a.\",\"criteria\":[{\"id\":\"exports-a\",\"statement\":\"The module exports a function named a.\"}]}\n//! ```\npub fn a() {}\n";

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
    grimoire().args(args).current_dir(cwd).output().unwrap()
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
/// {"test": "init-adopts-what-it-finds", "checks": ["crates/cli/src/init/command#init-adopts"]}
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
/// {"test": "check-fails-only-on-new-findings", "checks": ["crates/cli/src/check#check-runs-without-daemon", "crates/cli/src/check#check-exits-on-new", "crates/cli/src/check#check-prunes"]}
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
/// {"test": "an-unsupported-language-is-the-first-line", "checks": ["crates/cli/src/check#unsupported-first"]}
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
/// {"test": "an-omen-is-printed-and-fails-nothing", "checks": ["crates/cli/src/check#check-omens"]}
/// ```
#[test]
fn an_omen_is_printed_and_fails_nothing() {
    let (p, _) = project();
    // A three-line limit: src/a.rs, of three lines and two more of tests, is an omen.
    std::fs::write(p.repo.join("src").join("a.rs"), "pub fn a() {}\n\npub fn b() {}\n#[cfg(test)]\nmod tests {}\n").unwrap();
    assert!(run(&p.repo, &["check"]).status.success(), "no omen yet at the default limit");
    let path = p.store.join("grimoire.json");
    let mut config: serde_json::Value = serde_json::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap();
    config["large_file"] = serde_json::json!(3);
    std::fs::write(&path, config.to_string()).unwrap();
    let check = run(&p.repo, &["check"]);
    assert!(check.status.success(), "an omen fails nothing: {}", text(&check));
    let out = String::from_utf8_lossy(&check.stdout).to_string();
    let omen = out.lines().position(|l| l.starts_with("omen") && l.contains("file:src/a.rs") && l.contains("3 lines, 5 with its tests")).expect(&out);
    let summary = out.lines().position(|l| l.contains("finding(s):")).expect(&out);
    assert!(summary < omen, "after the findings: {out}");
    assert!(out.contains("1 omen(s); they do not fail the check"), "{out}");
    let json = run(&p.repo, &["check", "--json"]);
    let answer: serde_json::Value = serde_json::from_slice(&json.stdout).unwrap();
    assert_eq!(answer["omens"][0]["ref"], "file:src/a.rs");
    assert!(!answer["findings"].as_array().unwrap().iter().any(|f| f["ref"] == "file:src/a.rs"), "not a finding");
}

/// # Test contract
///
/// ```json
/// {"test": "the-store-is-found-from-anywhere", "checks": ["crates/cli/src#store-found"]}
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
    let reported: BTreeSet<String> = String::from_utf8_lossy(&init.stdout).lines().filter_map(|l| l.strip_prefix("wrote   ")).map(|l| relative(&p.repo, l.split(": ").next().unwrap_or(l))).collect();
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
        &["tool", "inscribe_ticket", A_TICKET],
        &["tool", "brew_ticket", r#"{"ref":"ticket:0001"}"#],
        &["tool", "put_page", r#"{"slug":"goal","title":"Goal","body":"b","links":[]}"#],
        &["tool", "inscribe_ticket", A_TICKET],
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
    let _daemon = Serving(grimoire().args(["serve", "--port", &port.to_string()]).current_dir(&p.repo).spawn().unwrap());
    let deadline = Instant::now() + Duration::from_secs(30);
    while !p.store.join("serve.json").is_file() {
        assert!(Instant::now() < deadline, "the daemon never marked the store");
        std::thread::sleep(Duration::from_millis(100));
    }
    let filed = run(&p.repo, &["tool", "inscribe_ticket", r#"{"title":"Through the daemon","kind":"change","priority":"later","body":"b"}"#]);
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

/// Run git in `dir` as a test's author, and fail the test when it fails.
fn git(dir: &Path, args: &[&str]) {
    let out =
        Command::new("git").arg("-C").arg(dir).args(["-c", "user.name=t", "-c", "user.email=t@t", "-c", "commit.gpgsign=false", "-c", "core.hooksPath=/nonexistent"]).args(args).output().unwrap();
    assert!(out.status.success(), "git {args:?}: {}", String::from_utf8_lossy(&out.stderr));
}

/// # Test contract
///
/// ```json
/// {"test": "a-branch-is-judged-by-its-own-work", "checks": ["crates/cli/src/check#check-own-work-base", "crates/cli/src/check#check-prints-advisories", "crates/cli/src/check#check-json-advisories", "crates/cli/src/check#check-ticket-or-main-line-takes-no-base"]}
/// ```
#[test]
fn a_branch_is_judged_by_its_own_work() {
    let (p, _) = project();
    git(&p.repo, &["init", "-q", "-b", "main"]);
    git(&p.repo, &["add", "-A"]);
    git(&p.repo, &["commit", "-q", "-m", "start"]);
    // A branch is cut; then the main line brings a requirement covered by a criterion only its code declares.
    git(&p.repo, &["branch", "feature"]);
    let filed: serde_json::Value = serde_json::from_slice(&run(&p.repo, &["tool", "inscribe_ticket", r#"{"title":"b is bottled","kind":"change","priority":"next","body":"b"}"#]).stdout).unwrap();
    let ticket = filed["node"]["ref"].as_str().unwrap().trim_start_matches("ticket:").to_string();
    assert!(run(&p.repo, &["tool", "brew_ticket", &format!(r#"{{"ref":"ticket:{ticket}"}}"#)]).status.success());
    let req = r#"{"slug":"b-exports","title":"b exports b","statement":"The module b SHALL export b.","scenarios":[{"title":"b","given":"b","when":"b","then":"b"}],"covered_by":["criterion:src/b#exports-b"]}"#;
    assert!(run(&p.repo, &["tool", "put_requirement", req]).status.success());
    let close = run(&p.repo, &["tool", "close_ticket", &format!(r#"{{"ref":"ticket:{ticket}","decision":{{"text":"b is bottled"}}}}"#)]);
    assert!(close.status.success(), "{}", text(&close));
    std::fs::write(p.repo.join("src").join("b.rs"), CONTRACTED.replace("exports-a", "exports-b")).unwrap();
    std::fs::write(p.repo.join("src").join("lib.rs"), "pub mod a;\npub mod b;\n").unwrap();
    git(&p.repo, &["add", "-A"]);
    git(&p.repo, &["commit", "-q", "-m", "b is bottled"]);
    // On the branch, cut before it: the requirement's missing criterion reached the main line since its base, and is shown, not failed.
    git(&p.repo, &["checkout", "-q", "feature"]);
    let check = run(&p.repo, &["check"]);
    assert!(check.status.success(), "{}", text(&check));
    assert!(text(&check).contains("reached the main line since this branch's base") && text(&check).contains("req:b-exports"), "{}", text(&check));
    let json: serde_json::Value = serde_json::from_slice(&run(&p.repo, &["check", "--json"]).stdout).unwrap();
    assert_eq!((json["new"].as_u64(), json["advisories"][0]["from"]["kind"].as_str()), (Some(0), Some("since_base")), "{json}");
    let lint: serde_json::Value = serde_json::from_slice(&run(&p.repo, &["tool", "lint"]).stdout).unwrap();
    assert_eq!(json, lint, "the check and the lint tool answer alike, advisories included");
    // A pipeline's gate on the main line takes no base.
    assert!(!run(&p.repo, &["check", "--main-line"]).status.success());
    // The branch's own work still fails: a criterion it adds that no requirement names.
    std::fs::write(p.repo.join("src").join("c.rs"), CONTRACTED.replace("exports-a", "exports-c")).unwrap();
    std::fs::write(p.repo.join("src").join("lib.rs"), "pub mod a;\npub mod c;\n").unwrap();
    let check = run(&p.repo, &["check"]);
    assert!(!check.status.success() && text(&check).contains("criterion:src/c#exports-c"), "{}", text(&check));
    // On the main line there is no base: the same truth judged as before.
    std::fs::remove_file(p.repo.join("src").join("c.rs")).unwrap();
    git(&p.repo, &["checkout", "-q", "--", "."]);
    git(&p.repo, &["checkout", "-q", "main"]);
    let check = run(&p.repo, &["check", "--json"]);
    let json: serde_json::Value = serde_json::from_slice(&check.stdout).unwrap();
    assert!(json["advisories"].as_array().unwrap().iter().all(|a| a["from"]["kind"] != "since_base"), "{json}");
}

/// # Test contract
///
/// ```json
/// {"test": "the-check-names-a-ticket-still-simmering-here", "checks": ["crates/cli/src/check#check-names-what-would-mend"]}
/// ```
#[test]
fn the_check_names_a_ticket_still_simmering_here() {
    let (p, _) = project();
    let start = |title: &str| {
        let filed: serde_json::Value =
            serde_json::from_slice(&run(&p.repo, &["tool", "inscribe_ticket", &format!(r#"{{"title":"{title}","kind":"change","priority":"next","body":"b"}}"#)]).stdout).unwrap();
        let ticket = filed["node"]["ref"].as_str().unwrap().trim_start_matches("ticket:").to_string();
        let started: serde_json::Value = serde_json::from_slice(&run(&p.repo, &["tool", "brew_ticket", &format!(r#"{{"ref":"ticket:{ticket}"}}"#)]).stdout).unwrap();
        (ticket, started)
    };
    // The first ticket adds module b with a requirement naming its criterion, and is bottled to wait for its bottling.
    let (first, _) = start("b is bottled");
    let req = r#"{"slug":"b-exports","title":"b exports b","statement":"The module b SHALL export b.","scenarios":[{"title":"b","given":"b","when":"b","then":"b"}],"covered_by":["criterion:src/b#exports-b"]}"#;
    assert!(run(&p.repo, &["tool", "put_requirement", req]).status.success());
    std::fs::write(p.repo.join("src").join("b.rs"), CONTRACTED.replace("exports-a", "exports-b")).unwrap();
    std::fs::write(p.repo.join("src").join("lib.rs"), "pub mod a;\npub mod b;\n").unwrap();
    let close = run(&p.repo, &["tool", "close_ticket", &format!(r#"{{"ref":"ticket:{first}","decision":{{"text":"b is bottled"}},"when":"bottled"}}"#)]);
    assert!(close.status.success(), "{}", text(&close));
    // A second ticket started in the same checkout is told the first still waits here.
    let (_, started) = start("more");
    assert_eq!(started["waiting"][0]["ticket"], first.as_str(), "{started}");
    // Its check judges b's criterion against its own proposal, which lacks the first's requirement: it fails, naming the first beside it.
    let check = run(&p.repo, &["check"]);
    assert!(!check.status.success(), "the finding still fails: {}", text(&check));
    let out = text(&check);
    let at = out.find("criterion:src/b#exports-b").unwrap_or_else(|| panic!("{out}"));
    assert!(out[at..].contains(&format!("ticket:{first} (b is bottled) would mend this")) && out.contains("still simmering from this checkout"), "{out}");
    let json: serde_json::Value = serde_json::from_slice(&run(&p.repo, &["check", "--json"]).stdout).unwrap();
    assert!(json["mended"].as_array().unwrap().iter().any(|m| m["ticket"] == first.as_str() && m["finding"]["ref"] == "criterion:src/b#exports-b"), "{json}");
}

/// # Test contract
///
/// ```json
/// {"test": "a-library-serves-a-checkouts-copy-of-a-servers-intent", "checks": ["crates/server/src#serves-the-machines-library", "crates/server/src/library#named-finds"]}
/// ```
#[test]
fn a_library_serves_a_checkouts_copy_of_a_servers_intent() {
    // A teammate's checkout of a team's project, signed in, its copy of the server's intent made by a first check.
    let t = team_fixture(None);
    let home = t.checkout.dir.join("home");
    assert!(team(&t.checkout.repo, &home, None, &["login", "--token", &t.writer]).status.success());
    let check = team(&t.checkout.repo, &home, None, &["check"]);
    assert!(check.status.success(), "{}", text(&check));
    let named = std::fs::read_to_string(t.checkout.repo.join(".grimoire.toml")).unwrap();
    let grimoire_name = named.lines().find_map(|l| l.strip_prefix("name = ")).unwrap().trim_matches('"').to_string();
    // On the same machine, a project with a store of its own, where the daemon is started.
    let local = bare_repo();
    assert!(team(&local.repo, &home, None, &["init", "--store", "local", "--name", "local"]).status.success());
    let port = TcpListener::bind("127.0.0.1:0").unwrap().local_addr().unwrap().port();
    let _daemon = Serving(grimoire().args(["serve", "--port", &port.to_string()]).current_dir(&local.repo).env("GRIMOIRE_HOME", &home).env_remove("GRIMOIRE_TOKEN").spawn().unwrap());
    let copies = home.join("copies");
    let copy = std::fs::read_dir(&copies).unwrap().flatten().map(|e| e.path()).find(|p| p.join("grimoire.json").is_file()).expect("the checkout's copy");
    marked_port(&copy);
    // The copy is served at its grimoire's path, answering for the teammate's checkout, signed in with their token.
    let (code, status) = post(port, &format!("/g/{grimoire_name}/api/status"), "{}", "");
    assert_eq!(code, 200, "{status}");
    let status: serde_json::Value = serde_json::from_str(&status).unwrap();
    assert_eq!(status["repository"], grimoire_server::store_io::repository_string(&t.checkout.repo).unwrap());
    assert_eq!(status["server"]["reachable"], true, "{status}");
    // A write from the checkout goes through the library to the server.
    let filed = team(&t.checkout.repo, &home, None, &["tool", "inscribe_ticket", r#"{"title":"From the library","kind":"change","priority":"later","body":"b"}"#]);
    assert!(filed.status.success(), "{}", text(&filed));
    let (_, board) =
        http(t.port, &format!("POST /api/board HTTP/1.0\r\nHost: 127.0.0.1:{}\r\nAuthorization: Bearer {}\r\nContent-Type: application/json\r\nContent-Length: 2\r\n\r\n{{}}", t.port, t.reader));
    assert!(board.contains("From the library"), "the server holds it: {board}");
}

/// # Test contract
///
/// ```json
/// {"test": "a-served-copy-watches-its-checkout-and-follows-its-server", "checks": ["crates/server/src/library/live#machine-grimoire-watched", "crates/server/src/library/live#copy-follows-its-server"]}
/// ```
#[test]
fn a_served_copy_watches_its_checkout_and_follows_its_server() {
    let t = team_fixture(None);
    let home = t.checkout.dir.join("home");
    assert!(team(&t.checkout.repo, &home, None, &["login", "--token", &t.writer]).status.success());
    assert!(team(&t.checkout.repo, &home, None, &["check"]).status.success());
    let named = std::fs::read_to_string(t.checkout.repo.join(".grimoire.toml")).unwrap();
    let name = named.lines().find_map(|l| l.strip_prefix("name = ")).unwrap().trim_matches('"').to_string();
    let local = bare_repo();
    assert!(team(&local.repo, &home, None, &["init", "--store", "local", "--name", "local"]).status.success());
    let port = TcpListener::bind("127.0.0.1:0").unwrap().local_addr().unwrap().port();
    let _daemon = Serving(grimoire().args(["serve", "--port", &port.to_string()]).current_dir(&local.repo).env("GRIMOIRE_HOME", &home).env_remove("GRIMOIRE_TOKEN").spawn().unwrap());
    let ask = |path: &str, body: &str| post(port, &format!("/g/{name}{path}"), body, "");
    eventually("the copy is served", || ask("/api/status", "{}").0 == 200);
    // Watched: code written in the checkout reaches the served grimoire with no request to read it again.
    let seeking = r#"{"query":"only_after_the_edit","kinds":["decl"]}"#;
    let found = || serde_json::from_str::<serde_json::Value>(&ask("/api/find", seeking).1).ok().and_then(|a| a["nodes"].as_array().map(Vec::len));
    assert_eq!(found(), Some(0), "not there before the edit");
    std::fs::write(t.checkout.repo.join("src").join("lib.rs"), "pub mod a;\npub fn only_after_the_edit() {}\n").unwrap();
    eventually("the edit reaches the served grimoire", || found() == Some(1));
    // Following: a teammate's write straight to the server reaches the served copy with no request to sync.
    let ticket = r#"{"title":"From a teammate","kind":"change","priority":"later","body":"b"}"#;
    let (code, answer) = post(t.port, "/api/inscribe_ticket", ticket, &format!("Authorization: Bearer {}\r\n", t.writer));
    assert_eq!(code, 200, "{answer}");
    eventually("the teammate's ticket reaches the served copy", || ask("/api/board", "{}").1.contains("From a teammate"));
}

/// # Test contract
///
/// ```json
/// {"test": "init-sets-a-project-up-in-one-command", "checks": ["crates/cli/src/init/command#init-scaffolds", "req:init-scaffolds"]}
/// ```
#[test]
fn init_sets_a_project_up_in_one_command() {
    // A repository with contracted modules, one in a group of its own, and no store yet.
    let p = bare_repo();
    std::fs::create_dir_all(p.repo.join("src").join("area")).unwrap();
    std::fs::write(
        p.repo.join("src").join("area").join("b.rs"),
        CONTRACTED.replace("Offer a.", "Offer b.").replace("exports-a", "exports-b").replace("named a.", "named b.").replace("fn a()", "fn b()"),
    )
    .unwrap();
    let init = run(&p.repo, &["init", "--store", &p.store.to_string_lossy()]);
    assert!(init.status.success() && text(&init).contains("wrote") && text(&init).contains("requirement(s)"), "{}", text(&init));
    let outline: serde_json::Value = serde_json::from_slice(&run(&p.repo, &["tool", "outline", r#"{"depth":1,"pages_only":true}"#]).stdout).unwrap();
    let root = &outline["roots"][0];
    assert_eq!(root["ref"], "page:overview", "{outline}");
    let story: Vec<&str> = root["children"].as_array().unwrap().iter().map(|c| c["ref"].as_str().unwrap()).collect();
    assert_eq!(story, ["page:goal", "page:guarantees", "page:glossary", "page:architecture", "page:constraints"]);
    let req: serde_json::Value = serde_json::from_slice(&run(&p.repo, &["tool", "find", r#"{"query":"SHALL hold its criterion Exports","kinds":["req"]}"#]).stdout).unwrap();
    assert_eq!(req["nodes"].as_array().unwrap().len(), 2, "a requirement for each module's criterion: {req}");
    // The module's group has a page of its own, under architecture.
    let architecture: serde_json::Value = serde_json::from_slice(&run(&p.repo, &["tool", "get", r#"{"ref":"page:architecture"}"#]).stdout).unwrap();
    assert!(architecture["node"]["links"].as_array().unwrap().iter().any(|l| l == "page:src-area"), "{architecture}");
    // What the skeleton leaves to do, criteria no test checks and requirements with no scenario, is the baseline.
    assert!(baseline_count(&p.store) > 0, "the skeleton's findings are adopted");
    // Run again, the store is left as it is: a page a person removed stays removed.
    let pages = || std::fs::read_dir(p.store.join("pages")).unwrap().count();
    let before = pages();
    std::fs::remove_file(p.store.join("pages").join("glossary.json")).unwrap();
    assert!(run(&p.repo, &["init", "--store", &p.store.to_string_lossy()]).status.success());
    assert_eq!(pages(), before - 1, "init scaffolds only a store it creates");
    // Asked for an empty store, it names the roots and writes no page.
    let q = bare_repo();
    assert!(run(&q.repo, &["init", "--store", &q.store.to_string_lossy(), "--empty"]).status.success());
    assert_eq!(std::fs::read_dir(q.store.join("pages")).map(|d| d.count()).unwrap_or(0), 0);
    let config: serde_json::Value = serde_json::from_str(&std::fs::read_to_string(q.store.join("grimoire.json")).unwrap()).unwrap();
    assert_eq!((config["roots"]["grimoire"].clone(), config["roots"]["rites"].clone()), (serde_json::json!("overview"), serde_json::json!("rites")), "both roots named: {config}");
}

/// # Test contract
///
/// ```json
/// {"test": "a-teams-server-serves-its-library", "checks": ["crates/server/src#serves-every-store-in-the-library", "crates/server/src#no-grimoire-on-the-bare-routes", "crates/server/src/library/live#opens-found-grimoires", "crates/server/src/library/live#opened-under-its-lock", "crates/server/src/library/live#marked-where-the-daemon-listens", "crates/server/src/library/live#team-server-own-tokens", "crates/server/src/library/live#vanished-grimoire-let-go", "crates/server/src/library/live#server-of-each-grimoire", "crates/server/src/library/live/routing#handed-to-the-grimoires-routes", "crates/server/src/library/live/routing#path-the-routes-expect", "crates/server/src/library/live/routing#unheld-name-not-found", "crates/cli/src/init/command#init-server-writes", "crates/cli/src/init/command#init-server-name", "req:server-library", "req:project-names-a-server", "req:project-name"]}
/// ```
#[test]
fn a_teams_server_serves_its_library() {
    // A team's library: two stores in one directory, each with a token of its own.
    let tmp = tempfile::Builder::new().prefix("grimoire-library-").tempdir().unwrap();
    let library = tmp.path().join("library");
    let make = |name: &str| -> String {
        let p = uninitialised();
        let store = library.join(name);
        assert!(run(&p.repo, &["init", "--store", &store.to_string_lossy()]).status.success());
        String::from_utf8_lossy(&run(&p.repo, &["token", "add", "ana", "--store", &store.to_string_lossy()]).stdout).trim().to_string()
    };
    let (shop, ledger) = (make("shop"), make("ledger"));
    let _server = Serving(grimoire().args(["serve", "--shared", "--library", &library.to_string_lossy(), "--listen", "127.0.0.1:0"]).spawn().unwrap());
    let port = marked_port(&library.join("shop"));
    marked_port(&library.join("ledger"));
    let ask = |path: &str, token: &str| {
        http(port, &format!("POST {path} HTTP/1.0\r\nHost: 127.0.0.1:{port}\r\nAuthorization: Bearer {token}\r\nContent-Type: application/json\r\nContent-Length: 2\r\n\r\n{{}}"))
    };
    // Each grimoire admits its own token and refuses the other's; the bare routes are no grimoire's.
    assert_eq!(ask("/g/shop/api/status", &shop).0, 200);
    assert_eq!(ask("/g/ledger/api/status", &ledger).0, 200);
    assert_eq!(ask("/g/shop/api/status", &ledger).0, 403, "a token is its grimoire's alone");
    let (code, bare) = ask("/api/status", &shop);
    assert!(code == 404 && bare.contains("/g/<name>"), "no grimoire answers on the bare routes, and the answer says where to go: {bare}");
    // Every kind of route is the grimoire's own: a token for ledger reaches none of shop's.
    let crossed = |request: String| http(port, &request).0;
    for get in ["/g/shop/api/export", "/g/shop/api/events", "/g/shop/api/tools"] {
        assert_eq!(crossed(format!("GET {get} HTTP/1.0\r\nHost: 127.0.0.1:{port}\r\nAuthorization: Bearer {ledger}\r\n\r\n")), 403, "{get}");
    }
    assert_eq!(ask("/g/shop/api/board", &ledger).0, 403);
    assert_eq!(ask("/g/shop/api/prune", &ledger).0, 403);
    let init = r#"{"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2025-03-26","capabilities":{},"clientInfo":{"name":"t","version":"0"}}}"#;
    let mcp = |token: &str| {
        crossed(format!("POST /g/shop/mcp HTTP/1.0\r\nHost: 127.0.0.1:{port}\r\nAuthorization: Bearer {token}\r\nAccept: application/json, text/event-stream\r\nContent-Type: application/json\r\nContent-Length: {}\r\n\r\n{init}", init.len()))
    };
    assert_eq!((mcp(&ledger), mcp(&shop)), (403, 200), "MCP is shop's alone");
    // A team's server lets no command take a store back: there is no release route on it.
    let (_, released) = ask("/library/release", &shop);
    assert!(!released.contains("released"), "{released}");
    // Tokens are read as each request comes: one added while serving is let in, one revoked is not.
    let shop_store = library.join("shop").to_string_lossy().to_string();
    let host = tmp.path().to_path_buf();
    let bo = String::from_utf8_lossy(&run(&host, &["token", "add", "bo", "--store", &shop_store]).stdout).trim().to_string();
    assert_eq!(ask("/g/shop/api/status", &bo).0, 200);
    assert!(run(&host, &["token", "revoke", "bo", "--store", &shop_store]).status.success());
    assert_eq!(ask("/g/shop/api/status", &bo).0, 403);
    let (_, status) = ask("/g/ledger/api/status", &ledger);
    let status: serde_json::Value = serde_json::from_str(&status).unwrap();
    let names: Vec<&str> = status["library"].as_array().unwrap().iter().map(|g| g["name"].as_str().unwrap()).collect();
    assert_eq!((status["grimoire"].as_str(), names), (Some("ledger"), vec!["ledger", "shop"]));
    // Each grimoire is opened under its lock, held while it is served, and marked at /g/<name> where the daemon listens.
    for name in ["shop", "ledger"] {
        let marker: grimoire_server::store_io::Serving = serde_json::from_str(&std::fs::read_to_string(library.join(name).join("serve.json")).unwrap()).unwrap();
        assert_eq!((marker.path.as_deref(), marker.addr()), (Some(format!("/g/{name}").as_str()), format!("127.0.0.1:{port}").parse().unwrap()), "{name}'s marker");
        assert!(matches!(grimoire_server::store_io::lock_new(&library.join(name)), Err(grimoire_server::store_io::StoreIoError::Locked(..))), "{name}'s lock is held while it is served");
    }
    // A team's server holds its grimoires as stores of their own: none is told a server it comes from.
    let served: Vec<_> = status["library"].as_array().unwrap().iter().map(|g| (g["name"].as_str().unwrap(), g["server"].is_null())).collect();
    assert_eq!(served, [("ledger", true), ("shop", true)], "{status}");
    // A store added while it runs is served, with no restart.
    let audit = make("audit");
    eventually("audit is served", || ask("/g/audit/api/status", &audit).0 == 200);
    // A store that stops being one while served is let go: its marker goes and its lock is free.
    std::fs::remove_file(library.join("audit").join("grimoire.json")).unwrap();
    eventually("audit is let go", || ask("/g/audit/api/status", &audit).0 == 404);
    assert!(!library.join("audit").join("serve.json").exists(), "its marker is gone");
    drop(grimoire_server::store_io::lock_new(&library.join("audit")).expect("its lock is free"));
    // A checkout of shop: its grimoire is named by the address, and every command reaches shop alone.
    let p = uninitialised();
    let home = p.dir.join("home");
    let address = format!("http://127.0.0.1:{port}/g/shop");
    assert!(team(&p.repo, &home, None, &["init", "--server", &address]).status.success());
    let project = std::fs::read_to_string(p.repo.join(".grimoire.toml")).unwrap();
    assert!(project.contains("name = \"shop\"") && project.contains(&format!("server = \"{address}\"")), "{project}");
    assert!(team(&p.repo, &home, None, &["login", "--token", &shop]).status.success());
    let check = team(&p.repo, &home, None, &["check"]);
    assert!(check.status.success(), "{}", text(&check));
    let filed = team(&p.repo, &home, None, &["tool", "inscribe_ticket", r#"{"title":"For shop","kind":"change","priority":"later","body":"b"}"#]);
    assert!(filed.status.success(), "{}", text(&filed));
    let board = |path: &str, token: &str| ask(path, token).1;
    assert!(board("/g/shop/api/board", &shop).contains("For shop"));
    assert!(!board("/g/ledger/api/board", &ledger).contains("For shop"), "the write went to shop's grimoire alone");
    // Bottling and the main line's prune go to shop's grimoire through its address too.
    let ticket = serde_json::from_slice::<serde_json::Value>(&filed.stdout).unwrap()["node"]["ref"].as_str().unwrap().to_string();
    for (tool, input) in [("brew_ticket", format!(r#"{{"ref":"{ticket}"}}"#)), ("close_ticket", format!(r#"{{"ref":"{ticket}","decision":{{"text":"Done."}},"when":"bottled"}}"#))] {
        assert!(team(&p.repo, &home, None, &["tool", tool, &input]).status.success(), "{tool}");
    }
    let bottled = team(&p.repo, &home, Some(&shop), &["bottle", ticket.trim_start_matches("ticket:")]);
    assert!(bottled.status.success() && text(&bottled).contains(&format!("bottled {ticket}")), "{}", text(&bottled));
    let main_line = team(&p.repo, &home, Some(&shop), &["check", "--main-line"]);
    assert!(text(&main_line).contains("pruned"), "{}", text(&main_line));
    // The checkout's own daemon serves its copy of shop in the machine's library, following the server.
    let daemon_port = TcpListener::bind("127.0.0.1:0").unwrap().local_addr().unwrap().port();
    let _daemon = Serving(grimoire().args(["serve", "--port", &daemon_port.to_string()]).current_dir(&p.repo).env("GRIMOIRE_HOME", &home).env_remove("GRIMOIRE_TOKEN").spawn().unwrap());
    eventually("the copy of shop is served on this machine", || post(daemon_port, "/g/shop/api/status", "{}", "").0 == 200);
    let (_, status) = post(daemon_port, "/g/shop/api/status", "{}", "");
    let status: serde_json::Value = serde_json::from_str(&status).unwrap();
    assert_eq!((status["server"]["address"].as_str(), status["server"]["reachable"].as_bool()), (Some(address.as_str()), Some(true)), "{status}");
    // The library names where the copy comes from: its server, without the grimoire's /g/<name>.
    let server = format!("http://127.0.0.1:{port}");
    assert!(status["library"].as_array().unwrap().iter().any(|g| g["name"] == "shop" && g["server"].as_str() == Some(server.as_str())), "{status}");
    // A project file pointed at another grimoire of the library takes that grimoire's name.
    let q = uninitialised();
    assert!(team(&q.repo, &home, None, &["init", "--server", &format!("http://127.0.0.1:{port}"), "--name", "old-name"]).status.success());
    assert!(team(&q.repo, &home, None, &["init", "--server", &format!("http://127.0.0.1:{port}/g/ledger")]).status.success());
    assert!(std::fs::read_to_string(q.repo.join(".grimoire.toml")).unwrap().contains("name = \"ledger\""), "the address names the grimoire, not the name the file held before");
}

/// # Test contract
///
/// ```json
/// {"test": "a-teams-library-serves-the-shared-page-for-everything-else", "checks": ["crates/server/src#shared-page-for-everything-else"]}
/// ```
#[test]
fn a_teams_library_serves_the_shared_page_for_everything_else() {
    let tmp = tempfile::Builder::new().prefix("grimoire-library-page-").tempdir().unwrap();
    let library = tmp.path().join("library");
    let p = uninitialised();
    assert!(run(&p.repo, &["init", "--store", &library.join("shop").to_string_lossy()]).status.success());
    let _server = Serving(grimoire().args(["serve", "--shared", "--library", &library.to_string_lossy(), "--listen", "127.0.0.1:0"]).spawn().unwrap());
    let port = marked_port(&library.join("shop"));
    for path in ["/", "/index.html", "/some/other/page", "/g", "/assets/app.js"] {
        let (code, body) = http(port, &format!("GET {path} HTTP/1.0\r\nHost: 127.0.0.1:{port}\r\n\r\n"));
        assert_eq!(code, 200, "{path}: {body}");
        assert!(body.contains("This is a shared Grimoire server"), "{path}: {body}");
    }
    // The API is not that page.
    let (_, api) = http(port, &format!("POST /api/status HTTP/1.0\r\nHost: 127.0.0.1:{port}\r\nContent-Length: 2\r\n\r\n{{}}"));
    assert!(!api.contains("This is a shared Grimoire server"), "{api}");
}

/// # Test contract
///
/// ```json
/// {"test": "a-library-serves-the-site-it-names", "checks": ["crates/server/src#site-of-a-named-grimoire", "crates/server/src#site-without-grimoire-refused", "crates/server/src#grimoire-without-site-refused", "crates/server/src/site#library-site", "crates/cli/src#serve-site-of", "req:server-library"]}
/// ```
#[test]
fn a_library_serves_the_site_it_names() {
    // A team's library holding `shop`, whose site shows its goal.
    let tmp = tempfile::Builder::new().prefix("grimoire-library-site-").tempdir().unwrap();
    let library = tmp.path().join("library");
    // Made beside the library and moved in whole, so a server already running takes it complete.
    let make = |name: &str| {
        let p = uninitialised();
        let store = tmp.path().join(name);
        assert!(run(&p.repo, &["init", "--store", &store.to_string_lossy()]).status.success());
        assert!(run(&p.repo, &["tool", "configure", r#"{"site":["goal"]}"#]).status.success());
        std::fs::create_dir_all(&library).unwrap();
        std::fs::rename(&store, library.join(name)).unwrap();
    };
    make("shop");
    let free = || TcpListener::bind("127.0.0.1:0").unwrap().local_addr().unwrap().port();
    let lib = library.to_string_lossy().to_string();
    // A site beside a library names its grimoire, or the server does not start.
    let refused = grimoire().args(["serve", "--shared", "--library", &lib, "--listen", "127.0.0.1:0", "--site", "127.0.0.1:0"]).output().unwrap();
    assert!(!refused.status.success() && text(&refused).contains("--site-of"), "{}", text(&refused));
    // A grimoire named without a site to serve it on, or without a library, is refused too.
    for args in [
        vec!["serve", "--shared", "--library", &lib, "--listen", "127.0.0.1:0", "--site-of", "shop"],
        vec!["serve", "--shared", "--listen", "127.0.0.1:0", "--site", "127.0.0.1:0", "--site-of", "shop"],
    ] {
        let refused = grimoire().args(&args).output().unwrap();
        assert!(!refused.status.success() && text(&refused).contains("--site-of"), "{args:?}: {}", text(&refused));
    }
    let get = |port: u16, path: &str| {
        http(
            port,
            &format!(
                "GET {path} HTTP/1.0
Host: 127.0.0.1:{port}

"
            ),
        )
    };
    // Named, it is that grimoire's site, with no token asked.
    let site = free();
    let shop = Serving(grimoire().args(["serve", "--shared", "--library", &lib, "--listen", "127.0.0.1:0", "--site", &format!("127.0.0.1:{site}"), "--site-of", "shop"]).spawn().unwrap());
    marked_port(&library.join("shop"));
    eventually("shop's site answers", || std::net::TcpStream::connect(("127.0.0.1", site)).is_ok());
    let (code, page) = get(site, "/");
    assert!(code == 200 && page.contains("/goal\""), "{page}");
    assert_eq!(get(site, "/api/status").0, 404, "nothing but the site on its address");
    drop(shop);
    // Named before its grimoire exists, the site says so, and follows it once its store arrives.
    let site = free();
    let _audit = Serving(grimoire().args(["serve", "--shared", "--library", &lib, "--listen", "127.0.0.1:0", "--site", &format!("127.0.0.1:{site}"), "--site-of", "audit"]).spawn().unwrap());
    eventually("audit's site answers", || std::net::TcpStream::connect(("127.0.0.1", site)).is_ok());
    let (code, page) = get(site, "/");
    assert!(code == 404 && page.contains("no grimoire <code>audit</code>"), "{page}");
    make("audit");
    eventually("audit's site is served", || get(site, "/").0 == 200);
    // Let go when it stops being a store, and followed again when it is one again, with no restart.
    let (config, away) = (library.join("audit").join("grimoire.json"), tmp.path().join("audit-grimoire.json"));
    std::fs::rename(&config, &away).unwrap();
    eventually("audit's site is let go", || get(site, "/").0 == 404);
    std::fs::rename(&away, &config).unwrap();
    eventually("audit's site is served again", || get(site, "/").0 == 200);
}

/// # Test contract
///
/// ```json
/// {"test": "two-checkouts-of-a-teams-grimoire-are-served-side-by-side", "checks": ["crates/server/src/library#named-every-checkout", "crates/server/src/library/live#opens-found-grimoires", "crates/server/src/library/live/routing#handed-to-the-grimoires-routes", "crates/server/src/library/live/routing#refused-caller-costs-a-discard", "crates/server/src/library/live/routing#handler-chooses-the-checkout", "crates/server/src/library/live/routing#nearest-project-file", "crates/server/src/library/live/routing#default-answers-when-none-named", "crates/server/src/library/live/routing#none-named-for-several-refused", "crates/server/src/library/live/routing#directory-no-checkout-holds-refused", "crates/server/src/library/live/routing#refused-after-the-trust-step", "crates/server/src/library/live/routing#refused-over-the-api-as-invalid", "crates/server/src/library/live/routing#refused-over-mcp-as-tool-error", "crates/server/src/library/live/routing#refusal-lists-checkouts-and-says-to-name-yours", "crates/server/src#bare-routes-by-named-checkout", "crates/server/src/client#names-its-checkout", "req:machine-library", "req:machine-checkouts"]}
/// ```
#[test]
fn two_checkouts_of_a_teams_grimoire_are_served_side_by_side() {
    // A team's project, and on one machine two checkouts of it, the second as a worktree with code of its own.
    let t = team_fixture(None);
    let home = t.checkout.dir.join("home");
    let main = &t.checkout.repo;
    let wt = uninitialised();
    let address = format!("http://127.0.0.1:{}", t.port);
    assert!(team(&wt.repo, &home, None, &["init", "--server", &address]).status.success());
    std::fs::write(wt.repo.join("src").join("lib.rs"), "pub mod a;\npub fn only_in_the_worktree() {}\n").unwrap();
    assert!(team(main, &home, None, &["login", "--token", &t.writer]).status.success());
    for repo in [main, &wt.repo] {
        let check = team(repo, &home, None, &["check"]);
        assert!(check.status.success(), "{}", text(&check));
    }
    // One daemon, started in the main checkout, serves both under the grimoire's one name.
    let port = TcpListener::bind("127.0.0.1:0").unwrap().local_addr().unwrap().port();
    let _daemon = Serving(grimoire().args(["serve", "--port", &port.to_string()]).current_dir(main).env("GRIMOIRE_HOME", &home).env_remove("GRIMOIRE_TOKEN").spawn().unwrap());
    let repository = |r: &Path| grimoire_server::store_io::repository_string(r).unwrap();
    let (main_at, wt_at) = (repository(main), repository(&wt.repo));
    let ask = |path: &str, body: serde_json::Value| {
        let (code, answer) = post(port, path, &body.to_string(), "");
        (code, serde_json::from_str::<serde_json::Value>(&answer).unwrap_or_default())
    };
    let served = || ask("/g/repo/api/status", serde_json::json!({})).1["library"].as_array().map(|l| l.iter().filter(|g| g["name"] == "repo").count()).unwrap_or(0);
    eventually("both checkouts are served", || served() == 2);
    // Each answers from its own code; a call naming neither is refused, naming both checkouts.
    let found = |checkout: Option<&str>| {
        let mut input = serde_json::json!({ "query": "only_in_the_worktree", "kinds": ["decl"] });
        if let Some(c) = checkout {
            input["checkout"] = serde_json::json!(c);
        }
        ask("/g/repo/api/find", input)
    };
    let (code, answer) = found(Some(&wt_at));
    assert_eq!((code, answer["nodes"].as_array().map(Vec::len)), (200, Some(1)), "the worktree's code: {answer}");
    let (code, answer) = found(Some(&main_at));
    assert_eq!((code, answer["nodes"].as_array().map(Vec::len)), (200, Some(0)), "the main checkout's: {answer}");
    let (code, refused) = found(None);
    let why = refused["message"].as_str().unwrap_or_default();
    assert!(code == 400 && refused["error"] == "invalid" && why.contains("2 checkouts") && why.contains("`checkout`"), "{refused}");
    assert_eq!(refused["details"][0]["field"], "checkout", "an invalid error naming the field, as a tool's is: {refused}");
    // A caller the trust step refuses meets that refusal first, and learns nothing of the checkouts.
    let (code, foreign) = post(
        port,
        "/g/repo/api/find",
        r#"{"query":"x"}"#,
        "Origin: https://elsewhere.example
",
    );
    assert!(code == 403 && !foreign.contains(&wt_at) && !foreign.contains("checkouts"), "{foreign}");
    // The bare routes are the daemon's own grimoire, chosen the same way.
    assert_eq!(ask("/api/find", serde_json::json!({ "query": "only_in_the_worktree", "kinds": ["decl"], "checkout": wt_at })).1["nodes"].as_array().map(Vec::len), Some(1));
    assert_eq!(ask("/api/find", serde_json::json!({ "query": "x" })).0, 400);
    // status is answered without one, by the daemon's own checkout.
    let (code, status) = ask("/g/repo/api/status", serde_json::json!({}));
    assert_eq!((code, status["repository"].as_str()), (200, Some(main_at.as_str())));
    // Over MCP the refusal is a tool error the agent reads; named, the call is answered.
    let mcp = |arguments: serde_json::Value| {
        let call = serde_json::json!({ "jsonrpc": "2.0", "id": 9, "method": "tools/call", "params": { "name": "find", "arguments": arguments } });
        let (_, answer) = post(port, "/g/repo/mcp", &call.to_string(), "Accept: application/json, text/event-stream\r\n");
        let json = answer.lines().find_map(|l| l.strip_prefix("data: ")).unwrap_or(&answer).to_string();
        serde_json::from_str::<serde_json::Value>(&json).unwrap()
    };
    let refused = mcp(serde_json::json!({ "query": "x" }));
    assert!(refused["result"]["isError"] == true && refused["id"] == 9 && refused["result"]["content"][0]["text"].as_str().unwrap().contains("checkouts"), "{refused}");
    let named = mcp(serde_json::json!({ "query": "only_in_the_worktree", "kinds": ["decl"], "checkout": wt.repo.join("src").to_string_lossy() }));
    assert!(named["result"]["isError"] != true && named["result"]["content"][0]["text"].as_str().unwrap().contains("only_in_the_worktree"), "a subfolder of the worktree is the worktree's: {named}");
    // A command run in the worktree names it, and each checkout keeps its own current ticket.
    let filed = team(&wt.repo, &home, None, &["tool", "inscribe_ticket", r#"{"title":"In the worktree","kind":"change","priority":"later","body":"b"}"#]);
    assert!(filed.status.success(), "{}", text(&filed));
    let ticket = serde_json::from_slice::<serde_json::Value>(&filed.stdout).unwrap()["node"]["ref"].as_str().unwrap().to_string();
    assert!(team(&wt.repo, &home, None, &["tool", "brew_ticket", &format!(r#"{{"ref":"{ticket}"}}"#)]).status.success());
    let page = |checkout: &str| ask("/g/repo/api/put_page", serde_json::json!({ "slug": "notes", "title": "Notes", "body": "b", "links": [], "checkout": checkout }));
    assert_eq!(page(&wt_at).0, 200, "the worktree's write goes into its own current ticket");
    let (code, refused) = page(&main_at);
    assert!(code == 400 && refused["message"].as_str().unwrap_or_default().contains("no ticket is current"), "the main checkout has started none: {refused}");
    // A directory no checkout holds is refused.
    assert_eq!(ask("/g/repo/api/lint", serde_json::json!({ "checkout": t.origin.dir.to_string_lossy() })).0, 400);
}

/// A worktree of `p`'s repository: a folder beside it holding the same code and project file.
fn worktree_of(p: &Project, name: &str) -> PathBuf {
    let wt = p.dir.join(name);
    std::fs::create_dir_all(wt.join("src")).unwrap();
    for f in ["Cargo.toml", "src/lib.rs", store_io_project_file()] {
        if p.repo.join(f).is_file() {
            std::fs::copy(p.repo.join(f), wt.join(f)).unwrap();
        }
    }
    wt
}

fn store_io_project_file() -> &'static str {
    grimoire_server::store_io::PROJECT_FILE
}

/// # Test contract
///
/// ```json
/// {"test": "a-teams-worktree-inside-its-checkout-is-its-own", "checks": ["crates/server/src/library/live/routing#nearest-project-file", "crates/server/src/library/live/routing#nested-checkout-is-its-own", "crates/server/src/library/live/routing#registered-team", "crates/server/src/library/live/routing#registered-trust", "req:machine-checkouts"]}
/// ```
#[test]
fn a_teams_worktree_inside_its_checkout_is_its_own() {
    // A team's checkout, signed in and served, and a worktree kept inside it, as Claude Code keeps one,
    // with its own committed project file and code of its own, that has run nothing.
    let t = team_fixture(None);
    let home = t.checkout.dir.join("home");
    let main = &t.checkout.repo;
    assert!(team(main, &home, None, &["login", "--token", &t.writer]).status.success());
    assert!(team(main, &home, None, &["check"]).status.success());
    // No ignore file: the main checkout's code ends where a worktree, a checkout of its own, begins.
    let nested = |name: &str| {
        let wt = main.join("worktrees").join(name);
        std::fs::create_dir_all(wt.join("src")).unwrap();
        for f in ["Cargo.toml", "src/a.rs", ".grimoire.toml"] {
            std::fs::copy(main.join(f), wt.join(f)).unwrap();
        }
        std::fs::write(wt.join("src").join("lib.rs"), "pub mod a;\npub fn only_in_the_worktree() {}\n").unwrap();
        wt
    };
    let wt = nested("wt");
    let port = TcpListener::bind("127.0.0.1:0").unwrap().local_addr().unwrap().port();
    let _daemon = Serving(grimoire().args(["serve", "--port", &port.to_string()]).current_dir(main).env("GRIMOIRE_HOME", &home).env_remove("GRIMOIRE_TOKEN").spawn().unwrap());
    let ask = |tool: &str, input: serde_json::Value| {
        let (code, answer) = post(port, &format!("/g/repo/api/{tool}"), &input.to_string(), "");
        (code, serde_json::from_str::<serde_json::Value>(&answer).unwrap_or_default())
    };
    eventually("the main checkout is served", || ask("status", serde_json::json!({})).0 == 200);
    // Its first call, naming a directory in it, is answered by the worktree itself: its copy made, its code read.
    let (code, found) = ask("find", serde_json::json!({ "query": "only_in_the_worktree", "kinds": ["decl"], "checkout": wt.join("src").to_string_lossy() }));
    assert_eq!((code, found["nodes"].as_array().map(Vec::len)), (200, Some(1)), "the worktree's code, not the main checkout's: {found}");
    let (_, status) = ask("status", serde_json::json!({ "checkout": wt.to_string_lossy() }));
    assert_eq!(status["repository"], grimoire_server::store_io::repository_string(&wt).unwrap(), "{status}");
    // A caller the trust step refuses takes no checkout, the server up: a page on another site gets the 403, and nothing is served.
    let wt3 = nested("wt3");
    let (code, _) = post(port, "/g/repo/api/find", &serde_json::json!({ "query": "x", "checkout": wt3.to_string_lossy() }).to_string(), "Origin: https://elsewhere.example\r\n");
    assert_eq!(code, 403);
    let (_, status) = ask("status", serde_json::json!({}));
    assert!(!status["library"].to_string().contains("wt3"), "{status}");
    // With the server away a worktree's copy cannot be made: refused saying so, never answered by the main checkout.
    drop(t._server);
    let wt2 = nested("wt2");
    let (code, refused) = ask("find", serde_json::json!({ "query": "x", "checkout": wt2.to_string_lossy() }));
    let why = refused["message"].as_str().unwrap_or_default();
    assert!(code == 400 && why.contains("could not be made") && why.contains("cannot be reached") && why.contains("grimoire check"), "the reason, and what to run: {refused}");
}

/// # Test contract
///
/// ```json
/// {"test": "a-store-of-its-own-serves-every-checkout", "checks": ["crates/server/src/library/live/routing#registered-store", "crates/server/src/library/live/routing#registered-no-such-grimoire", "crates/server/src/library/live/routing#none-named-for-several-refused", "crates/server/src/library/live/routing#directory-no-checkout-holds-refused", "crates/server/src/library/live/routing#refusal-lists-checkouts-and-says-to-name-yours", "crates/server/src/library#named-every-checkout", "crates/server/src#serves-its-own-grimoire", "crates/cli/src/init/command#repoint-adds-checkout", "crates/cli/src/check#check-adds-checkout", "req:store-checkouts", "req:machine-checkouts"]}
/// ```
#[test]
fn a_store_of_its_own_serves_every_checkout() {
    // A project with a store of its own, served by its daemon, and a worktree of it that has run nothing.
    let p = bare_repo();
    assert!(run(&p.repo, &["init", "--store", "solo-one", "--name", "solo"]).status.success());
    let wt = worktree_of(&p, "wt");
    std::fs::write(wt.join("src").join("lib.rs"), format!("{CONTRACTED}pub fn only_in_the_worktree() {{}}\n")).unwrap();
    let store = test_home().join("stores").join("solo-one");
    let port = TcpListener::bind("127.0.0.1:0").unwrap().local_addr().unwrap().port();
    let _daemon = Serving(grimoire().args(["serve", "--port", &port.to_string()]).current_dir(&p.repo).spawn().unwrap());
    marked_port(&store);
    let at = |r: &Path| grimoire_server::store_io::repository_string(r).unwrap();
    let ask = |tool: &str, input: serde_json::Value| {
        let (code, answer) = post(port, &format!("/g/solo/api/{tool}"), &input.to_string(), "");
        (code, serde_json::from_str::<serde_json::Value>(&answer).unwrap_or_default())
    };
    let find = |checkout: &Path| {
        ask("find", serde_json::json!({ "query": "only_in_the_worktree", "kinds": ["decl"], "checkout": checkout.join("src").to_string_lossy() })).1["nodes"].as_array().map(Vec::len)
    };
    // Its first call, naming a directory in it, adds it to the store and is answered from its code.
    assert_eq!(find(&wt), Some(1), "the worktree's own code");
    assert_eq!(find(&p.repo), Some(0), "the main checkout's");
    let listed: serde_json::Value = serde_json::from_str(&std::fs::read_to_string(store.join("checkouts.json")).unwrap()).unwrap();
    assert!(listed["checkouts"].get(at(&wt)).is_some(), "{listed}");
    // Each its own current ticket: a command run in the worktree names it.
    let filed = run(&wt, &["tool", "inscribe_ticket", r#"{"title":"In the worktree","kind":"change","priority":"later","body":"b"}"#]);
    assert!(filed.status.success(), "{}", text(&filed));
    let ticket = serde_json::from_slice::<serde_json::Value>(&filed.stdout).unwrap()["node"]["ref"].as_str().unwrap().to_string();
    assert!(run(&wt, &["tool", "brew_ticket", &format!(r#"{{"ref":"{ticket}"}}"#)]).status.success());
    let page = |checkout: &Path| ask("put_page", serde_json::json!({ "slug": "notes", "title": "Notes", "body": "b", "links": [], "checkout": checkout.to_string_lossy() }));
    assert_eq!(page(&wt).0, 200, "the worktree's write goes into its own ticket");
    let (code, refused) = page(&p.repo);
    assert!(code == 400 && refused["message"].as_str().unwrap_or_default().contains("no ticket is current"), "the main checkout started none: {refused}");
    // Two checkouts served now: a call naming neither is refused, listing both.
    let (code, refused) = ask("lint", serde_json::json!({}));
    assert!(code == 400 && refused["message"].as_str().unwrap_or_default().contains("2 checkouts"), "{refused}");
    // init in a third checkout adds it, through the daemon.
    let wt2 = worktree_of(&p, "wt2");
    let init = run(&wt2, &["init", "--store", "solo-one", "--name", "solo"]);
    assert!(init.status.success() && text(&init).contains("serves this checkout"), "{}", text(&init));
    let listed: serde_json::Value = serde_json::from_str(&std::fs::read_to_string(store.join("checkouts.json")).unwrap()).unwrap();
    assert!(listed["checkouts"].get(at(&wt2)).is_some(), "{listed}");
    // A check in a fourth, while the daemon holds the store, has the daemon add it; and a command
    // given --store in a checkout reads that checkout's view of the store.
    let wt3 = worktree_of(&p, "wt3");
    let check = run(&wt3, &["check"]);
    assert!(text(&check).contains("finding(s)"), "{}", text(&check));
    let listed: serde_json::Value = serde_json::from_str(&std::fs::read_to_string(store.join("checkouts.json")).unwrap()).unwrap();
    assert!(listed["checkouts"].get(at(&wt3)).is_some(), "{listed}");
    let status = run(&wt, &["tool", "status", "--store", "solo-one"]);
    assert_eq!(serde_json::from_slice::<serde_json::Value>(&status.stdout).unwrap()["repository"], at(&wt), "{}", text(&status));
    // A folder of another project is no checkout of this store, and is not added.
    let other = bare_repo();
    let (code, refused) = ask("lint", serde_json::json!({ "checkout": other.repo.to_string_lossy() }));
    assert!(code == 400 && refused["message"].as_str().unwrap_or_default().contains("is no checkout of the grimoire `solo`"), "{refused}");
}

/// # Test contract
///
/// ```json
/// {"test": "a-worktrees-check-prunes-nothing", "checks": ["crates/cli/src/check#check-no-prune-in-worktree", "crates/cli/src/check#check-adds-checkout", "req:store-checkouts"]}
/// ```
#[test]
fn a_worktrees_check_prunes_nothing() {
    // A store of its own whose findings are adopted, and a worktree of it, with no daemon.
    let p = bare_repo();
    std::fs::write(p.repo.join("src").join("lib.rs"), "pub mod a;\n").unwrap();
    std::fs::write(p.repo.join("src").join("a.rs"), "pub fn a() {}\n").unwrap();
    assert!(run(&p.repo, &["init", "--store", "solo-two", "--name", "solo-two"]).status.success());
    let store = test_home().join("stores").join("solo-two");
    let adopted = baseline_count(&store);
    assert!(adopted > 0);
    let wt = worktree_of(&p, "wt");
    std::fs::copy(p.repo.join("src").join("a.rs"), wt.join("src").join("a.rs")).unwrap();
    // The worktree fixes a finding (its contract brings new ones of its own): its check prunes nothing, and adds it to the store.
    std::fs::write(wt.join("src").join("a.rs"), CONTRACTED).unwrap();
    let check = run(&wt, &["check"]);
    assert!(text(&check).contains("baselined"), "{}", text(&check));
    assert_eq!(baseline_count(&store), adopted, "a branch's fix has not reached the main line");
    assert!(std::fs::read_to_string(store.join("checkouts.json")).unwrap().contains("wt"));
    // The fix reached the main checkout: its check prunes.
    std::fs::write(p.repo.join("src").join("a.rs"), CONTRACTED).unwrap();
    run(&p.repo, &["check"]);
    assert!(baseline_count(&store) < adopted);
}

/// Wait until `ok` holds, up to thirty seconds.
fn eventually(what: &str, mut ok: impl FnMut() -> bool) {
    let deadline = Instant::now() + Duration::from_secs(30);
    while !ok() {
        assert!(Instant::now() < deadline, "{what}, within thirty seconds");
        std::thread::sleep(Duration::from_millis(200));
    }
}

/// # Test contract
///
/// ```json
/// {"test": "the-library-follows-what-the-machine-holds", "checks": ["crates/server/src/library/live#opens-found-grimoires", "crates/server/src/library/live#opened-under-its-lock", "crates/server/src/library/live#vanished-grimoire-let-go", "crates/server/src/library/live#library-told-after-change", "crates/server/src/library/live/routing#unheld-name-not-found", "crates/server/src/library/live#release", "crates/server/src/client#release", "crates/cli/src/uninstall#library-asked-to-let-go", "crates/cli/src/uninstall#library-told-when-done", "crates/cli/src/uninstall#default-grimoire-refused", "crates/cli/src/uninstall#store-deleted-when-asked", "crates/cli/src#lock-delete-holds", "req:machine-library", "req:cli-uninstall"]}
/// ```
#[test]
fn the_library_follows_what_the_machine_holds() {
    // A daemon already serving the machine's library, started in `shop`, and a store another daemon holds.
    let (shop, audit) = (bare_repo(), bare_repo());
    assert!(run(&shop.repo, &["init", "--store", "shop", "--name", "shop"]).status.success());
    assert!(run(&audit.repo, &["init", "--store", "audit", "--name", "audit"]).status.success());
    let audit_store = test_home().join("stores").join("audit");
    let other = Serving(grimoire().args(["serve", "--store", "audit", "--port", "0"]).current_dir(&audit.repo).spawn().unwrap());
    marked_port(&audit_store);
    let port = TcpListener::bind("127.0.0.1:0").unwrap().local_addr().unwrap().port();
    let _daemon = Serving(grimoire().args(["serve", "--port", &port.to_string()]).current_dir(&shop.repo).spawn().unwrap());
    marked_port(&test_home().join("stores").join("shop"));
    let library = |at: &str| -> Vec<String> {
        let (_, status) = post(port, &format!("{at}/api/status"), "{}", "");
        serde_json::from_str::<serde_json::Value>(&status)
            .ok()
            .and_then(|s| s["library"].as_array().map(|l| l.iter().filter_map(|g| g["name"].as_str().map(str::to_string)).collect()))
            .unwrap_or_default()
    };
    let answers = |name: &str| post(port, &format!("/g/{name}/api/status"), "{}", "").0 == 200;
    assert_eq!(library(""), ["shop"]);
    // A project made while it runs is served with nothing restarted, init says so, and every grimoire's status lists it.
    let ledger = bare_repo();
    let init = run(&ledger.repo, &["init", "--store", "ledger", "--name", "ledger", "--port", &port.to_string()]);
    assert!(init.status.success() && text(&init).contains("serves it within seconds"), "{}", text(&init));
    eventually("ledger is served", || answers("ledger"));
    assert_eq!((library(""), library("/g/ledger")), (vec!["ledger".to_string(), "shop".to_string()], vec!["ledger".to_string(), "shop".to_string()]));
    // A store another process held is taken once it is let go.
    drop(other);
    eventually("audit is served once its own daemon stops", || answers("audit"));
    // A project file that renames its grimoire: let go under the old name, served under the new.
    let project = ledger.repo.join(".grimoire.toml");
    std::fs::write(&project, std::fs::read_to_string(&project).unwrap().replace("name = \"ledger\"", "name = \"books\"")).unwrap();
    eventually("the renamed grimoire is served by its new name", || answers("books") && !answers("ledger"));
    // A store released for deletion is held back until the command is done with it, then taken again.
    let release = |done: bool| post(port, "/library/release", &serde_json::json!({ "store": audit_store.to_string_lossy(), "done": done }).to_string(), "");
    let (code, answer) = release(false);
    assert!(code == 200 && answer.contains("\"released\":true"), "{answer}");
    assert!(!answers("audit"));
    std::thread::sleep(Duration::from_secs(5));
    assert!(!answers("audit"), "held back across refreshes while the command holds it");
    release(true);
    eventually("audit is served again once the command is done", || answers("audit"));
    // Deleting a store the library serves: the library lets it go, and it is deleted;
    // made again at once in the same place, it is served again.
    let books_store = test_home().join("stores").join("ledger");
    let out = run(&ledger.repo, &["uninstall", "--delete-store"]);
    assert!(out.status.success() && text(&out).contains("released store"), "{}", text(&out));
    assert!(!books_store.exists());
    assert_eq!(post(port, "/g/books/api/status", "{}", "").0, 404, "a name the library no longer holds is not found");
    assert!(run(&ledger.repo, &["init", "--store", "ledger", "--name", "books"]).status.success());
    eventually("a store made again where one was deleted is served", || answers("books"));
    // The daemon's own grimoire is not let go: its deletion is refused, and nothing removed.
    let out = run(&shop.repo, &["uninstall", "--delete-store"]);
    assert!(!out.status.success() && text(&out).contains("stop it first"), "{}", text(&out));
    assert!(test_home().join("stores").join("shop").join("grimoire.json").is_file() && shop.repo.join(".grimoire.toml").is_file());
}

/// One request to the daemon on `port`: the status code and the body.
fn post(port: u16, path: &str, body: &str, extra_headers: &str) -> (u16, String) {
    use std::io::{Read, Write};
    let mut s = std::net::TcpStream::connect(("127.0.0.1", port)).unwrap();
    s.set_read_timeout(Some(Duration::from_secs(10))).unwrap();
    write!(s, "POST {path} HTTP/1.0\r\nHost: 127.0.0.1:{port}\r\n{extra_headers}Content-Type: application/json\r\nContent-Length: {}\r\n\r\n{body}", body.len()).unwrap();
    let mut answer = String::new();
    s.read_to_string(&mut answer).unwrap();
    let code = answer.split_whitespace().nth(1).unwrap().parse().unwrap();
    (code, answer.split_once("\r\n\r\n").map(|(_, b)| b.to_string()).unwrap_or_default())
}

/// # Test contract
///
/// ```json
/// {"test": "one-daemon-serves-the-machines-library", "checks": ["crates/server/src#serves-its-own-grimoire", "crates/server/src#marked-at-its-path", "crates/server/src#serves-the-machines-library", "req:project-name", "req:tool-status"]}
/// ```
#[test]
fn one_daemon_serves_the_machines_library() {
    // Two projects on one machine, each with a store of its own named by its project file.
    let (shop, ledger) = (bare_repo(), bare_repo());
    assert!(run(&shop.repo, &["init", "--store", "shop", "--name", "shop"]).status.success());
    let init = run(&ledger.repo, &["init", "--store", "ledger-store", "--name", "ledger"]);
    assert!(init.status.success(), "{}", text(&init));
    let project = std::fs::read_to_string(ledger.repo.join(".grimoire.toml")).unwrap();
    assert!(project.contains("name = \"ledger\""), "{project}");
    let mcp: serde_json::Value = serde_json::from_str(&std::fs::read_to_string(ledger.repo.join(".mcp.json")).unwrap()).unwrap();
    assert_eq!(mcp["mcpServers"]["grimoire"]["url"], "http://127.0.0.1:4242/g/ledger/mcp");
    // A third, whose store another daemon already serves on its own.
    let audit = bare_repo();
    assert!(run(&audit.repo, &["init", "--store", "audit", "--name", "audit"]).status.success());
    let audit_store = test_home().join("stores").join("audit");
    let _other = Serving(grimoire().args(["serve", "--store", "audit", "--port", "0"]).current_dir(&audit.repo).spawn().unwrap());
    let held_by = marked_port(&audit_store);
    // One daemon, started in the first.
    let port = TcpListener::bind("127.0.0.1:0").unwrap().local_addr().unwrap().port();
    let _daemon = Serving(grimoire().args(["serve", "--port", &port.to_string()]).current_dir(&shop.repo).spawn().unwrap());
    let ledger_store = test_home().join("stores").join("ledger-store");
    marked_port(&ledger_store);
    // Each marker names the grimoire's path, the default's too; the held one is left to its own daemon.
    let path_of = |store: &Path| serde_json::from_str::<serde_json::Value>(&std::fs::read_to_string(store.join("serve.json")).unwrap()).unwrap()["path"].clone();
    assert_eq!(path_of(&test_home().join("stores").join("shop")), "/g/shop");
    assert_eq!(path_of(&ledger_store), "/g/ledger");
    assert_eq!((marked_port(&audit_store), path_of(&audit_store)), (held_by, serde_json::Value::Null), "the other daemon still holds `audit`");
    let (_, body) = post(port, "/g/audit/api/status", "{}", "");
    assert!(!body.contains("\"repository\""), "the library does not answer for a grimoire another process holds: {body}");
    let repository = |status: &str| serde_json::from_str::<serde_json::Value>(status).unwrap()["repository"].as_str().unwrap_or_default().to_string();
    let named = |repo: &Path| grimoire_server::store_io::repository_string(repo).unwrap();
    // The bare routes and /g/shop are the first's; /g/ledger the second's.
    for (path, repo) in [("/api/status", &shop.repo), ("/g/shop/api/status", &shop.repo), ("/g/ledger/api/status", &ledger.repo)] {
        let (code, body) = post(port, path, "{}", "");
        assert_eq!(code, 200, "{path}: {body}");
        assert_eq!(repository(&body), named(repo), "{path}");
    }
    // Each grimoire's status names it and lists the library, the held one not among them.
    for (path, named) in [("/g/ledger/api/status", "ledger"), ("/g/shop/api/status", "shop"), ("/api/status", "shop")] {
        let (_, status) = post(port, path, "{}", "");
        let status: serde_json::Value = serde_json::from_str(&status).unwrap();
        assert_eq!(status["grimoire"], named, "{path}");
        let library: Vec<&str> = status["library"].as_array().unwrap().iter().map(|g| g["name"].as_str().unwrap()).collect();
        assert_eq!(library, ["ledger", "shop"], "{path}");
    }
    // A command in the second sends its write to the daemon, at the second's path.
    let filed = run(&ledger.repo, &["tool", "inscribe_ticket", r#"{"title":"Through the library","kind":"change","priority":"later","body":"b"}"#]);
    assert!(filed.status.success(), "{}", text(&filed));
    let (_, board) = post(port, "/g/ledger/api/board", r#"{"priority":["later"]}"#, "");
    assert!(board.contains("Through the library"), "the daemon holds the write, so it went through it: {board}");
    let (_, theirs) = post(port, "/api/board", r#"{"priority":["later"]}"#, "");
    assert!(!theirs.contains("Through the library"), "and not into the first's grimoire");
    // MCP answers at the grimoire's own path.
    let init = r#"{"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2025-03-26","capabilities":{},"clientInfo":{"name":"t","version":"0"}}}"#;
    let (code, _) = post(port, "/g/ledger/mcp", init, "Accept: application/json, text/event-stream\r\n");
    assert_eq!(code, 200, "MCP is served at /g/ledger/mcp");
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
/// {"test": "init-then-uninstall-leaves-the-repository-as-it-was", "checks": ["crates/cli/src/uninstall#store-kept", "crates/cli/src/uninstall#store-deleted-when-asked", "req:cli-uninstall"]}
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
    assert!(run(&p.repo, &["tool", "inscribe_ticket", A_TICKET]).status.success());
    assert!(run(&p.repo, &["tool", "brew_ticket", r#"{"ref":"ticket:0001"}"#]).status.success());
    assert!(run(&p.repo, &["tool", "put_page", r#"{"slug":"goal","title":"Goal","body":"b","links":[]}"#]).status.success());
    assert!(p.store.join("proposals").join("0001.json").is_file() && p.store.join("current.json").is_file());
    let out = run(&p.repo, &["uninstall", "--delete-store"]);
    assert!(out.status.success(), "{}", text(&out));
    assert!(!p.store.exists(), "{}", text(&out));
    assert_eq!(files(&p.repo), before);
    // A store in which a recipe was judged, holding the main line's recipes as a team's server does: both a store's own.
    assert!(run(&p.repo, &["init", "--store", &p.store.to_string_lossy()]).status.success());
    std::fs::create_dir_all(p.store.join("judgements")).unwrap();
    std::fs::write(
        p.store.join("judgements").join("button.json"),
        r#"{"recipe":"button","judgements":[{"recipe":"button","fingerprint":"00000000000000aa","date":"2026-10-08","ticket":"0001","outcome":"held"}]}"#,
    )
    .unwrap();
    std::fs::write(p.store.join("recipes.json"), r#"{"recipes":[{"id":"button","kind":"component","title":"A button","text":"How to use a button.","ingredients":[{"id":"label","says":"The text shown."}],"uses":[],"digest":"00000000000000aa"}]}"#).unwrap();
    let out = run(&p.repo, &["uninstall", "--delete-store"]);
    assert!(out.status.success(), "{}", text(&out));
    assert!(!p.store.exists(), "{}", text(&out));
    assert_eq!(files(&p.repo), before);
}

/// # Test contract
///
/// ```json
/// {"test": "init-takes-up-a-store-whose-deletion-is-queued", "checks": ["crates/cli/src/uninstall#queues-when-lock-held", "crates/cli/src/uninstall#says-queued-and-when", "crates/cli/src/uninstall#init-cancels-queued", "crates/server/src#queued-not-served", "req:store-deletion-queued"]}
/// ```
#[test]
fn init_takes_up_a_store_whose_deletion_is_queued() {
    let (p, _) = project();
    let held = grimoire_server::store_io::lock(&p.store).unwrap();
    let out = run(&p.repo, &["uninstall", "--delete-store"]);
    assert!(text(&out).contains("queued  deletion of store"), "{}", text(&out));
    // Queued, it is not served, and the marker serve wrote before asking the queue is gone. Its repository
    // names it again, as when a person puts the project file back, so nothing carries the deletion out first.
    drop(held);
    std::fs::write(p.repo.join(".grimoire.toml"), format!("store = '{}'\n", p.store.display())).unwrap();
    let out = run(&p.dir, &["serve", "--store", &p.store.to_string_lossy(), "--port", "0"]);
    assert!(!out.status.success() && text(&out).contains("queued for deletion"), "{}", text(&out));
    assert!(!p.store.join("serve.json").exists(), "{}", text(&out));
    // Taken up again before anything carried the deletion out: cancelled, and the store kept with its records.
    let out = run(&p.repo, &["init", "--store", &p.store.to_string_lossy()]);
    assert!(out.status.success(), "{}", text(&out));
    assert!(text(&out).contains("cancelled the queued deletion"), "{}", text(&out));
    let out = run(&p.repo, &["check"]);
    assert!(!text(&out).contains("whose deletion was queued"), "{}", text(&out));
    assert!(p.store.join("grimoire.json").is_file());
}

/// # Test contract
///
/// ```json
/// {"test": "a-held-store-is-queued-for-deletion-and-deleted-later", "checks": ["crates/cli/src/uninstall#queues-when-lock-held", "crates/cli/src/uninstall#integration-and-project-file-removed", "crates/cli/src/uninstall#says-queued-and-when", "crates/cli/src/uninstall#commands-carry-out-queued", "crates/cli/src/uninstall#says-what-became-of-each", "crates/cli/src/uninstall#store-lock-held", "crates/cli/src#lock-delete-queues", "req:store-deletion-queued"]}
/// ```
#[test]
fn a_held_store_is_queued_for_deletion_and_deleted_later() {
    let (p, _) = project();
    // Another process holds the store's lock, as a library slow to let go would.
    let held = grimoire_server::store_io::lock(&p.store).unwrap();
    let out = run(&p.repo, &["uninstall", "--delete-store"]);
    assert!(out.status.success(), "{}", text(&out));
    assert!(text(&out).contains("queued  deletion of store"), "{}", text(&out));
    assert!(!p.repo.join(".grimoire.toml").exists(), "the project file goes as usual");
    assert!(p.store.join("grimoire.json").is_file(), "the store waits while it is held");
    // Still held: the next command leaves it queued and says nothing of it.
    let out = run(&p.dir, &["check"]);
    assert!(!text(&out).contains("whose deletion was queued"), "{}", text(&out));
    assert!(p.store.exists());
    // Let go: the next command carries the deletion out, whatever it is.
    drop(held);
    let out = run(&p.dir, &["check"]);
    assert!(text(&out).contains("whose deletion was queued"), "{}", text(&out));
    assert!(!p.store.exists(), "{}", text(&out));
    let out = run(&p.dir, &["check"]);
    assert!(!text(&out).contains("whose deletion was queued"), "said once: {}", text(&out));
}

/// # Test contract
///
/// ```json
/// {"test": "a-refused-codex-config-stops-init-before-anything-is-written", "checks": ["crates/cli/src/init#unmergeable-file-refused-first"]}
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
/// {"test": "uninstall-refuses-a-store-it-does-not-own-or-that-is-serving", "checks": ["crates/cli/src/uninstall#names-this-repository", "crates/cli/src/uninstall#not-served", "crates/cli/src/uninstall#checked-before-removing"]}
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
    let _daemon = Serving(grimoire().args(["serve", "--port", &port.to_string()]).current_dir(&a.repo).spawn().unwrap());
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
/// {"test": "uninstall-deletes-past-a-dead-daemons-marker", "checks": ["crates/cli/src/uninstall#not-served", "req:cli-uninstall"]}
/// ```
#[test]
fn uninstall_deletes_past_a_dead_daemons_marker() {
    // A marker naming a daemon that is starting, holding the store's lock but not answering yet: refused.
    let (a, _) = project();
    let port = TcpListener::bind("127.0.0.1:0").unwrap().local_addr().unwrap().port();
    std::fs::write(a.store.join("serve.json"), format!(r#"{{"port":{port},"pid":1}}"#)).unwrap();
    let starting = grimoire_server::store_io::lock(&a.store).unwrap();
    let before = files(&a.repo);
    let out = run(&a.repo, &["uninstall", "--delete-store"]);
    assert!(!out.status.success());
    assert!(text(&out).contains("is marked as served"), "{}", text(&out));
    assert_eq!(files(&a.repo), before, "nothing removed");
    assert!(a.store.join("grimoire.json").is_file());
    // The daemon killed: its marker left behind, nothing answering, the lock free. The store goes, marker and all.
    drop(starting);
    let out = run(&a.repo, &["uninstall", "--delete-store"]);
    assert!(out.status.success(), "{}", text(&out));
    assert!(!a.store.exists(), "{}", text(&out));
    assert!(!a.repo.join(".grimoire.toml").exists());
}

/// # Test contract
///
/// ```json
/// {"test": "uninstall-never-deletes-more-than-a-store", "checks": ["crates/cli/src/uninstall#not-the-repository-or-a-root", "crates/cli/src/uninstall#only-a-store", "crates/cli/src/uninstall#checked-before-removing"]}
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

/// One request a stand-in daemon answered: what it was, and what was so at that moment.
struct Asked {
    what: &'static str,
    /// The machine's deletion queue was held by another process.
    queue_held: bool,
    /// The repository's `.grimoire.toml` was still there.
    project_file: bool,
}

/// A stand-in for a daemon serving a store in its machine's library, marking the store as one does. It answers
/// `status` for the store's repository and logs what it is asked. Asked to let the store go it says it has, and
/// then answers no more, as a library does, unless `keeps_serving`.
struct FakeLibrary {
    asked: std::sync::Arc<std::sync::Mutex<Vec<Asked>>>,
    stop: std::sync::Arc<std::sync::atomic::AtomicBool>,
}

impl Drop for FakeLibrary {
    fn drop(&mut self) {
        self.stop.store(true, std::sync::atomic::Ordering::SeqCst);
    }
}

impl FakeLibrary {
    /// What it was asked, in order, `status` left out.
    fn calls(&self) -> Vec<&'static str> {
        self.asked.lock().unwrap().iter().map(|a| a.what).filter(|w| *w != "status").collect()
    }
}

fn fake_library(p: &Project, keeps_serving: bool) -> FakeLibrary {
    use std::io::{Read, Write};
    use std::sync::atomic::Ordering::SeqCst;
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let port = listener.local_addr().unwrap().port();
    listener.set_nonblocking(true).unwrap();
    let config: serde_json::Value = serde_json::from_str(&std::fs::read_to_string(p.store.join("grimoire.json")).unwrap()).unwrap();
    let repository = config["repository"].as_str().unwrap().to_string();
    std::fs::write(p.store.join("serve.json"), format!(r#"{{"port":{port},"pid":1,"path":"/g/fake"}}"#)).unwrap();
    let (marker, project, queue) = (p.store.join("serve.json"), p.repo.join(".grimoire.toml"), test_home().join("deletions").join(".lock"));
    let asked = std::sync::Arc::new(std::sync::Mutex::new(Vec::new()));
    let stop = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
    let (log, halt) = (asked.clone(), stop.clone());
    std::thread::spawn(move || {
        let released = std::sync::atomic::AtomicBool::new(false);
        while !halt.load(SeqCst) {
            let Ok((mut s, _)) = listener.accept() else {
                std::thread::sleep(Duration::from_millis(10));
                continue;
            };
            s.set_nonblocking(false).unwrap();
            s.set_read_timeout(Some(Duration::from_secs(5))).unwrap();
            let mut request = Vec::new();
            let mut chunk = [0u8; 1024];
            let body = loop {
                let n = s.read(&mut chunk).unwrap_or(0);
                request.extend_from_slice(&chunk[..n]);
                let text = String::from_utf8_lossy(&request).to_string();
                if let Some((head, rest)) = text.split_once("\r\n\r\n") {
                    let length = head.lines().find_map(|l| l.to_ascii_lowercase().strip_prefix("content-length:").map(|v| v.trim().parse::<usize>().unwrap_or(0))).unwrap_or(0);
                    if rest.len() >= length || n == 0 {
                        break rest.to_string();
                    }
                } else if n == 0 {
                    break String::new();
                }
            };
            let path = String::from_utf8_lossy(&request).lines().next().unwrap_or_default().split_whitespace().nth(1).unwrap_or_default().to_string();
            let held = std::fs::OpenOptions::new().read(true).write(true).open(&queue).is_ok_and(|f| matches!(f.try_lock(), Err(std::fs::TryLockError::WouldBlock)));
            let note = |what| log.lock().unwrap().push(Asked { what, queue_held: held, project_file: project.is_file() });
            let (code, answer) = if path.ends_with("/library/release") {
                if body.contains("\"done\":true") {
                    note("done");
                } else {
                    note("release");
                    released.store(true, SeqCst);
                    if !keeps_serving {
                        let _ = std::fs::remove_file(&marker);
                    }
                }
                (200, r#"{"released":true}"#.to_string())
            } else if path.ends_with("/api/status") {
                note("status");
                if released.load(SeqCst) && !keeps_serving {
                    (404, "{}".to_string())
                } else {
                    (200, serde_json::json!({ "repository": repository }).to_string())
                }
            } else {
                (404, "{}".to_string())
            };
            let _ = write!(s, "HTTP/1.0 {code} X\r\nContent-Type: application/json\r\nContent-Length: {}\r\n\r\n{answer}", answer.len());
        }
    });
    FakeLibrary { asked, stop }
}

/// # Test contract
///
/// ```json
/// {"test": "a-library-is-told-when-uninstall-is-done-with-its-store", "checks": ["crates/cli/src/uninstall#library-told-when-done"]}
/// ```
#[test]
fn a_library_is_told_when_uninstall_is_done_with_its_store() {
    // Ending with the store deleted: asked to let it go, and told it is done, in that order and once each.
    let (p, _) = project();
    let library = fake_library(&p, false);
    let out = run(&p.repo, &["uninstall", "--delete-store"]);
    assert!(out.status.success() && !p.store.exists(), "{}", text(&out));
    assert_eq!(library.calls(), ["release", "done"], "{}", text(&out));
    // Ending in a refusal after the store was released: told all the same, and the store still there.
    let (q, _) = project();
    let library = fake_library(&q, true);
    let out = run(&q.repo, &["uninstall", "--delete-store"]);
    assert!(!out.status.success() && q.store.join("grimoire.json").is_file(), "{}", text(&out));
    assert_eq!(library.calls(), ["release", "done"], "{}", text(&out));
    // Ending with the deletion queued: told too.
    let (r, _) = project();
    let library = fake_library(&r, true);
    let held = grimoire_server::store_io::lock(&r.store).unwrap();
    let out = run(&r.repo, &["uninstall", "--delete-store"]);
    assert!(out.status.success() && text(&out).contains("queued  deletion of store"), "{}", text(&out));
    assert_eq!(library.calls(), ["release", "done"], "{}", text(&out));
    drop(held);
}

/// # Test contract
///
/// ```json
/// {"test": "uninstall-checks-again-just-before-the-deletion", "checks": ["crates/cli/src/uninstall#checked-again-before-deletion"]}
/// ```
#[test]
fn uninstall_checks_again_just_before_the_deletion() {
    // A daemon that let the store go, as far as the first check could tell, and serves it still at the end:
    // the first check lets a library's store through, the last does not.
    let (p, _) = project();
    let _library = fake_library(&p, true);
    let out = run(&p.repo, &["uninstall", "--delete-store"]);
    assert!(!out.status.success(), "{}", text(&out));
    assert!(text(&out).contains("the integration is removed, but not the store") && text(&out).contains("a daemon is serving store"), "{}", text(&out));
    assert!(p.store.join("grimoire.json").is_file(), "the store is not deleted");
    assert!(p.repo.join(".grimoire.toml").is_file(), "and the project file stays for a rerun");
}

/// # Test contract
///
/// ```json
/// {"test": "uninstall-holds-the-queue-while-asking-again", "checks": ["crates/cli/src/uninstall#queue-held-while-asking-served"]}
/// ```
#[test]
fn uninstall_holds_the_queue_while_asking_again() {
    let (p, _) = project();
    let library = fake_library(&p, true);
    // Held by another process, so the deletion is queued.
    let held = grimoire_server::store_io::lock(&p.store).unwrap();
    let out = run(&p.repo, &["uninstall", "--delete-store"]);
    assert!(out.status.success() && text(&out).contains("queued  deletion of store"), "{}", text(&out));
    drop(held);
    let asked = library.asked.lock().unwrap();
    let release = asked.iter().position(|a| a.what == "release").expect("the library was asked to let it go");
    assert!(asked[..release].iter().all(|a| !a.queue_held), "the queue is not held for the first questions");
    assert!(asked[release..].iter().any(|a| a.what == "status" && a.queue_held), "asked again, the queue held: the last status was asked with it free");
    assert!(asked.iter().filter(|a| a.what != "done").all(|a| a.project_file), "nothing was removed before it asked");
}

/// Hold `file` open so that it cannot be deleted while the returned guard lives.
#[cfg(windows)]
fn hold_open(file: &Path) -> std::fs::File {
    use std::os::windows::fs::OpenOptionsExt;
    std::fs::OpenOptions::new().read(true).share_mode(1).open(file).unwrap()
}

/// Hold `file` so that it cannot be deleted while the returned guard lives: its folder made read-only.
#[cfg(unix)]
fn hold_open(file: &Path) -> impl Drop {
    use std::os::unix::fs::PermissionsExt;
    struct Restore(PathBuf);
    impl Drop for Restore {
        fn drop(&mut self) {
            let _ = std::fs::set_permissions(&self.0, std::fs::Permissions::from_mode(0o755));
        }
    }
    let dir = file.parent().unwrap().to_path_buf();
    std::fs::set_permissions(&dir, std::fs::Permissions::from_mode(0o555)).unwrap();
    Restore(dir)
}

/// # Test contract
///
/// ```json
/// {"test": "a-stopped-deletion-leaves-grimoire-json-and-project", "checks": ["crates/cli/src/uninstall#project-file-last", "crates/cli/src/uninstall#records-first-grimoire-json-last"]}
/// ```
#[test]
fn a_stopped_deletion_leaves_grimoire_json_and_project() {
    let p = bare_repo();
    assert!(run(&p.repo, &["init", "--store", &p.store.to_string_lossy()]).status.success());
    // Something holds a file of the store, so its removal stops part way.
    let held = hold_open(&p.store.join("baseline.json"));
    let out = run(&p.repo, &["uninstall", "--delete-store"]);
    assert!(!out.status.success() && text(&out).contains("the store was not fully deleted"), "{}", text(&out));
    assert!(p.store.join("grimoire.json").is_file(), "grimoire.json goes last");
    assert!(p.repo.join(".grimoire.toml").is_file(), "and the project file after the store");
    let status = run(&p.repo, &["tool", "status"]);
    assert!(status.status.success(), "the store still loads for a rerun: {}", text(&status));
    // Let go, a rerun finishes.
    drop(held);
    let out = run(&p.repo, &["uninstall", "--delete-store"]);
    assert!(out.status.success(), "{}", text(&out));
    assert!(!p.store.exists() && !p.repo.join(".grimoire.toml").exists(), "{}", text(&out));
}

/// # Test contract
///
/// ```json
/// {"test": "init-takes-up-one-store-and-carries-out-the-others-deletions", "checks": ["crates/cli/src/uninstall#init-then-carries-out-others"]}
/// ```
#[test]
fn init_takes_up_one_store_and_carries_out_the_others_deletions() {
    // Two stores whose deletions are queued, each held when uninstall ran.
    let ((a, _), (b, _)) = (project(), project());
    let (held_a, held_b) = (grimoire_server::store_io::lock(&a.store).unwrap(), grimoire_server::store_io::lock(&b.store).unwrap());
    for p in [&a, &b] {
        let out = run(&p.repo, &["uninstall", "--delete-store"]);
        assert!(text(&out).contains("queued  deletion of store"), "{}", text(&out));
    }
    drop((held_a, held_b));
    assert!(a.store.exists() && b.store.exists(), "both wait");
    // Taking the first up again: its deletion cancelled, and the second's carried out.
    let out = run(&a.repo, &["init", "--store", &a.store.to_string_lossy()]);
    assert!(out.status.success() && text(&out).contains("cancelled the queued deletion"), "{}", text(&out));
    assert!(a.store.join("grimoire.json").is_file(), "the store taken up is kept");
    assert!(!b.store.exists(), "the other deletion is carried out: {}", text(&out));
    assert!(text(&out).contains("whose deletion was queued"), "{}", text(&out));
}

/// # Test contract
///
/// ```json
/// {"test": "uninstall-finds-the-project-above-or-at-repo", "checks": ["crates/cli/src/uninstall#project-found"]}
/// ```
#[test]
fn uninstall_finds_the_project_above_or_at_repo() {
    let p = bare_repo();
    let before = files(&p.repo);
    // From the nearest .grimoire.toml above the working directory.
    assert!(run(&p.repo, &["init", "--store", &p.store.to_string_lossy()]).status.success());
    let out = run(&p.repo.join("src"), &["uninstall"]);
    assert!(out.status.success(), "{}", text(&out));
    assert_eq!(files(&p.repo), before);
    // From --repo's own directory, whatever the working directory.
    assert!(run(&p.repo, &["init", "--store", &p.store.to_string_lossy()]).status.success());
    let out = run(&p.dir, &["uninstall", "--repo", &p.repo.to_string_lossy()]);
    assert!(out.status.success(), "{}", text(&out));
    assert_eq!(files(&p.repo), before);
    // A --repo directory has none of its own: the one above it is not looked for.
    assert!(run(&p.repo, &["init", "--store", &p.store.to_string_lossy()]).status.success());
    let installed = files(&p.repo);
    let out = run(&p.repo, &["uninstall", "--repo", &p.repo.join("src").to_string_lossy()]);
    assert!(p.repo.join(".grimoire.toml").is_file() && files(&p.repo) == installed, "{}", text(&out));
}

/// # Test contract
///
/// ```json
/// {"test": "a-project-that-names-a-server-has-no-store-to-delete", "checks": ["crates/cli/src/uninstall#server-store-not-deleted"]}
/// ```
#[test]
fn a_project_that_names_a_server_has_no_store_to_delete() {
    let t = team_fixture(None);
    let (repo, home) = (&t.checkout.repo, t.checkout.dir.join("home"));
    // The checkout's copy made, as any command does.
    assert!(team(repo, &home, Some(&t.writer), &["check"]).status.success());
    let (repo_before, home_before, server_before) = (contents(repo), contents(&home), contents(&t.origin.store));
    let out = team(repo, &home, Some(&t.writer), &["uninstall", "--delete-store"]);
    assert!(!out.status.success() && text(&out).contains("names a server") && text(&out).contains("Nothing was removed"), "{}", text(&out));
    assert_eq!((contents(repo), contents(&home)), (repo_before, home_before), "nothing removed here");
    assert_eq!(contents(&t.origin.store).len(), server_before.len(), "nor at the server's store");
}

/// # Test contract
///
/// ```json
/// {"test": "uninstall-refuses-a-malformed-project-file", "checks": ["crates/cli/src/uninstall#malformed-project-refused"]}
/// ```
#[test]
fn uninstall_refuses_a_malformed_project_file() {
    let p = bare_repo();
    assert!(run(&p.repo, &["init", "--store", &p.store.to_string_lossy()]).status.success());
    std::fs::write(p.repo.join(".grimoire.toml"), "store = [not toml").unwrap();
    let (repo_before, store_before) = (contents(&p.repo), contents(&p.store));
    for args in [vec!["uninstall"], vec!["uninstall", "--delete-store"]] {
        let out = run(&p.repo, &args);
        assert!(!out.status.success() && text(&out).contains(".grimoire.toml"), "{args:?}: {}", text(&out));
        assert_eq!((contents(&p.repo), contents(&p.store)), (repo_before.clone(), store_before.clone()), "{args:?}: nothing removed");
    }
}

/// A daemon serving `p`'s store, once it has marked it.
fn serve(p: &Project) -> (Serving, u16) {
    let port = TcpListener::bind("127.0.0.1:0").unwrap().local_addr().unwrap().port();
    let daemon = Serving(grimoire().args(["serve", "--port", &port.to_string()]).current_dir(&p.repo).spawn().unwrap());
    let deadline = Instant::now() + Duration::from_secs(30);
    while !p.store.join("serve.json").is_file() {
        assert!(Instant::now() < deadline, "the daemon never marked the store");
        std::thread::sleep(Duration::from_millis(100));
    }
    (daemon, port)
}

/// Every file under a directory with its bytes, to see nothing changed.
fn contents(dir: &Path) -> Vec<(String, Vec<u8>)> {
    files(dir)
        .into_iter()
        .map(|f| {
            let bytes = std::fs::read(&f).unwrap_or_default();
            (f, bytes)
        })
        .collect()
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
    let refused = run(&p.repo, &["tool", "inscribe_ticket", A_TICKET]);
    assert!(text(&refused).contains("if that daemon is gone, another grimoire process"), "{}", text(&refused));
    drop(held);
    let filed = run(&p.repo, &["tool", "inscribe_ticket", A_TICKET]);
    assert!(filed.status.success(), "{}", text(&filed));
}

/// # Test contract
///
/// ```json
/// {"test": "a-cli-writer-beside-a-holder-is-refused", "checks": ["crates/cli/src#lock-writers-hold", "crates/cli/src#lock-refused-writes-nothing", "crates/cli/src#lock-reads-take-none", "crates/cli/src#lock-check-prunes-when-got", "req:store-lock"]}
/// ```
#[test]
fn a_cli_writer_beside_a_holder_is_refused() {
    let (p, _) = project();
    // This test process holds the lock, as a daemon still starting or another command would.
    let held = grimoire_server::store_io::lock(&p.store).unwrap();
    let before = contents(&p.store);
    for args in [vec!["tool", "inscribe_ticket", A_TICKET], vec!["adopt", "--reason", "again"]] {
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
    let filed = run(&p.repo, &["tool", "inscribe_ticket", A_TICKET]);
    assert!(filed.status.success(), "{}", text(&filed));
}

/// # Test contract
///
/// ```json
/// {"test": "a-store-inside-the-repository-is-refused", "checks": ["crates/server/src/store_io/location#outside-repository", "crates/cli/src/uninstall#store-inside-repository-refused", "req:store-location"]}
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
    for args in [vec!["check"], vec!["tool", "board", "{}"], vec!["tool", "inscribe_ticket", A_TICKET], vec!["serve", "--port", "0"], vec!["uninstall", "--delete-store"]] {
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
/// {"test": "uninstall-finishes-a-deletion-stopped-part-way", "checks": ["crates/cli/src/uninstall#only-a-store", "crates/cli/src/uninstall#stopped-deletion-finished"]}
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
/// {"test": "an-explicit-store-resolves-as-the-project-file-does", "checks": ["crates/server/src/store_io/location#resolution-stores-dir", "crates/server/src/store_io/location#resolution-one-way", "crates/cli/src#store-flag-resolved"]}
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
    // A caller on this machine may read and write; a change of intent goes into a started ticket.
    assert_eq!(status_of(port, "GET", "/api/status", "Host: 127.0.0.1\r\n", ""), 200);
    assert_eq!(status_of(port, "POST", "/api/inscribe_ticket", "Host: 127.0.0.1\r\n", A_TICKET), 200);
    assert_eq!(status_of(port, "POST", "/api/brew_ticket", "Host: 127.0.0.1\r\n", r#"{"ref":"ticket:0001"}"#), 200);
    assert_eq!(status_of(port, "POST", "/api/put_page", "Host: 127.0.0.1\r\n", page), 200);
    // A page on another site, or a name rebound to this machine, is refused.
    assert_eq!(status_of(port, "POST", "/api/put_page", "Host: 127.0.0.1\r\nOrigin: https://example.com\r\n", page), 403);
    assert_eq!(status_of(port, "GET", "/api/status", "Host: evil.example\r\n", ""), 403);
}

/// # Test contract
///
/// ```json
/// {"test": "init-re-points-a-moved-checkout-and-refuses-a-copy", "checks": ["crates/cli/src/init/command#repoint-moved", "crates/cli/src/init/command#repoint-other-store-refused", "req:store-config-record"]}
/// ```
#[test]
fn init_re_points_a_moved_checkout_and_refuses_a_copy() {
    let (p, _) = project();
    let store = p.store.to_string_lossy().to_string();
    // A page written under a ticket and bottled in the truth by its close.
    assert!(run(&p.repo, &["tool", "inscribe_ticket", A_TICKET]).status.success());
    assert!(run(&p.repo, &["tool", "brew_ticket", r#"{"ref":"ticket:0001"}"#]).status.success());
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

/// A link at `link` to the folder `target`: a junction on Windows, which needs no privilege, a symlink elsewhere.
/// False when none can be made.
fn make_dir_link(target: &Path, link: &Path) -> bool {
    #[cfg(windows)]
    {
        Command::new("cmd").args(["/C", "mklink", "/J"]).arg(link).arg(target).output().is_ok_and(|o| o.status.success())
    }
    #[cfg(unix)]
    {
        std::os::unix::fs::symlink(target, link).is_ok()
    }
}

/// # Test contract
///
/// ```json
/// {"test": "init-takes-the-same-folder-spelt-differently-for-this-checkout", "checks": ["crates/cli/src/init/command#repoint-same-folder"]}
/// ```
#[test]
fn init_takes_the_same_folder_spelt_differently_for_this_checkout() {
    let (p, _) = project();
    let store = p.store.to_string_lossy().to_string();
    let recorded = std::fs::read(p.store.join("grimoire.json")).unwrap();
    // The checkout reached through a link: not a copy (the folder still exists) and not a move (nothing is re-pointed).
    let link = p.dir.join("link");
    if make_dir_link(&p.repo, &link) {
        let out = run(&link, &["init", "--store", &store]);
        assert!(out.status.success() && !text(&out).contains("re-pointed") && !text(&out).contains("still exists") && !text(&out).contains("beside"), "{}", text(&out));
        assert_eq!(std::fs::read(p.store.join("grimoire.json")).unwrap(), recorded, "the store still names this checkout, as it did");
    }
    // And by case, where the file system does not tell them apart.
    if cfg!(windows) {
        let upper = PathBuf::from(p.repo.to_string_lossy().to_uppercase());
        let out = run(&upper, &["init", "--store", &store]);
        assert!(out.status.success() && !text(&out).contains("re-pointed") && !text(&out).contains("still exists") && !text(&out).contains("beside"), "{}", text(&out));
        assert_eq!(std::fs::read(p.store.join("grimoire.json")).unwrap(), recorded);
    }
    assert!(!files(&p.dir).iter().any(|f| f.contains("checkouts.json")), "no second checkout was added");
}

/// # Test contract
///
/// ```json
/// {"test": "init-adopts-only-a-store-it-made", "checks": ["crates/cli/src/init/command#init-adopts"]}
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
/// {"test": "init-trims-what-it-is-given", "checks": ["crates/server/src/store_io#write-all-validated", "req:store-field-rules"]}
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
/// {"test": "init-adopts-a-scaffolded-store", "checks": ["crates/cli/src/init/command#init-adopts"]}
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
/// {"test": "a-commit-bottles-the-ticket-simmering-on-it", "checks": ["crates/cli/src/check#bottles-the-given-or-current-simmering-ticket", "crates/cli/src/check#land-still-runs-it-and-no-help-lists-it", "crates/cli/src/check#prints-what-was-bottled", "crates/cli/src/check#says-no-ticket-was-simmering", "crates/cli/src/check#exits-non-zero-when-refused", "crates/cli/src/check#exits-non-zero-when-the-store-is-unreachable", "crates/cli/src/check#exits-zero-when-no-ticket-was-simmering", "crates/cli/src/init#post-commit-hook-runs-bottle"]}
/// ```
#[test]
fn a_commit_bottles_the_ticket_simmering_on_it() {
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
    let ticket = tool("inscribe_ticket", A_TICKET)["node"]["ref"].as_str().unwrap().to_string();
    tool("brew_ticket", &format!(r#"{{"ref":"{ticket}"}}"#));
    // A commit made while the ticket is still being worked bottles nothing, and is not refused for it.
    let c = commit("work");
    assert!(c.status.success(), "{}", text(&c));
    assert!(text(&c).contains("no ticket was simmering"), "{}", text(&c));
    assert_eq!(status(&ticket), "brewing");
    // The decision is written and the ticket waits; a commit the check refuses bottles nothing.
    tool("close_ticket", &format!(r#"{{"ref":"{ticket}","decision":{{"text":"Done."}},"when":"bottled"}}"#));
    assert_eq!(status(&ticket), "simmering");
    std::fs::write(p.repo.join("src").join("b.rs"), "pub fn b() {}\n").unwrap();
    let refused = commit("a module with no contract");
    assert!(!refused.status.success(), "the check refuses a new finding: {}", text(&refused));
    assert_eq!(status(&ticket), "simmering");
    // The next commit that passes the check bottles it: the ticket is bottled with the decision written before.
    std::fs::remove_file(p.repo.join("src").join("b.rs")).unwrap();
    std::fs::write(p.repo.join("notes.txt"), "bottled\n").unwrap();
    let c = commit("bottle");
    assert!(c.status.success(), "{}", text(&c));
    assert!(text(&c).contains(&format!("bottled {ticket}")), "{}", text(&c));
    let closed = tool("get", &format!(r#"{{"ref":"{ticket}"}}"#));
    assert_eq!((closed["node"]["status"].as_str(), closed["node"]["decision"]["text"].as_str()), (Some("bottled"), Some("Done.")));
    // Named, a ticket that is not simmering is refused, and only then does the command exit non-zero.
    let again = run(&p.repo, &["bottle", &ticket]);
    assert!(!again.status.success() && text(&again).contains("bottling refused"), "{}", text(&again));
    assert!(run(&p.repo, &["bottle"]).status.success(), "nothing waiting is not a failure");
    // `land`, the command's name before bottling, still runs it, and no help lists it.
    let former = run(&p.repo, &["land"]);
    assert!(former.status.success() && text(&former).contains("no ticket was simmering"), "{}", text(&former));
    let help = text(&run(&p.repo, &["--help"]));
    assert!(help.contains("bottle") && !help.contains("land"), "{help}");
    // A store that cannot be reached is said, with a non-zero exit, and changes nothing.
    let gone = p.dir.join("no-such-store");
    let unreachable = run(&p.repo, &["bottle", "--store", &gone.to_string_lossy()]);
    assert!(!unreachable.status.success() && text(&unreachable).contains("is not a store"), "{}", text(&unreachable));
    // `git merge` makes its merge commit without invoking the pre-commit or post-commit hook, so a ticket bottling then waits for the command.
    let second = tool("inscribe_ticket", A_TICKET)["node"]["ref"].as_str().unwrap().to_string();
    tool("brew_ticket", &format!(r#"{{"ref":"{second}"}}"#));
    assert!(git(&["switch", "-q", "-c", "side"]).status.success());
    std::fs::write(p.repo.join("side.txt"), "side\n").unwrap();
    assert!(commit("on a branch").status.success());
    assert!(git(&["switch", "-q", "-"]).status.success());
    std::fs::write(p.repo.join("main.txt"), "main\n").unwrap();
    assert!(commit("on the main line").status.success());
    tool("close_ticket", &format!(r#"{{"ref":"{second}","decision":{{"text":"Merged."}},"when":"bottled"}}"#));
    let merge = git(&["merge", "--no-ff", "-q", "-m", "merge", "side"]);
    assert!(merge.status.success(), "{}", text(&merge));
    assert_eq!(status(&second), "simmering", "a merge runs no post-commit hook: {}", text(&merge));
    let by_hand = run(&p.repo, &["bottle"]);
    assert!(by_hand.status.success() && text(&by_hand).contains(&format!("bottled {second}")), "{}", text(&by_hand));
}

/// # Test contract
///
/// ```json
/// {"test": "a-bottling-that-cannot-load-or-write-its-store-exits-non-zero", "checks": ["crates/cli/src/check#exits-non-zero-when-the-store-is-unreachable", "req:cli-bottle"]}
/// ```
#[test]
fn a_bottling_that_cannot_load_or_write_its_store_exits_non_zero() {
    let p = uninitialised();
    let init = run(&p.repo, &["init", "--store", &p.store.to_string_lossy()]);
    assert!(init.status.success(), "{}", text(&init));
    let tool = |name: &str, input: &str| -> serde_json::Value {
        let o = run(&p.repo, &["tool", name, input]);
        assert!(o.status.success(), "{name}: {}", text(&o));
        serde_json::from_slice(&o.stdout).unwrap()
    };
    let ticket = tool("inscribe_ticket", A_TICKET)["node"]["ref"].as_str().unwrap().to_string();
    tool("brew_ticket", &format!(r#"{{"ref":"{ticket}"}}"#));
    tool("close_ticket", &format!(r#"{{"ref":"{ticket}","decision":{{"text":"Done."}},"when":"bottled"}}"#));
    let status = || tool("get", &format!(r#"{{"ref":"{ticket}"}}"#))["node"]["status"].as_str().unwrap().to_string();
    assert_eq!(status(), "simmering");
    let before = files(&p.store);
    // A store whose manifest is malformed is not loaded: the command says why, exits non-zero and bottles nothing.
    let manifest = p.store.join("grimoire.json");
    let good = std::fs::read(&manifest).unwrap();
    std::fs::write(&manifest, b"{ not json").unwrap();
    let malformed = run(&p.repo, &["bottle", "--store", &p.store.to_string_lossy()]);
    assert!(!malformed.status.success(), "{}", text(&malformed));
    assert!(text(&malformed).contains("grimoire.json is invalid") && !text(&malformed).contains("bottled "), "{}", text(&malformed));
    assert_eq!(std::fs::read(&manifest).unwrap(), b"{ not json", "nothing was written");
    std::fs::write(&manifest, &good).unwrap();
    assert_eq!(files(&p.store), before, "nothing was bottled");
    assert_eq!(status(), "simmering");
    // A store whose files cannot be written, so the bottling cannot be kept. The store writes a record through a
    // temporary file renamed over it: on Windows a read-only record refuses the rename; elsewhere the rename needs
    // only the folder, so the tickets folder is made read-only, which root alone can still write into.
    let record = p.store.join("tickets").join(ticket.trim_start_matches("ticket:")).with_extension("json");
    let name = record.file_name().unwrap().to_string_lossy().to_string();
    let kept = std::fs::read(&record).unwrap();
    #[cfg(windows)]
    let locked = record.clone();
    #[cfg(not(windows))]
    let locked = record.parent().unwrap().to_path_buf();
    let mut permissions = std::fs::metadata(&locked).unwrap().permissions();
    permissions.set_readonly(true);
    std::fs::set_permissions(&locked, permissions.clone()).unwrap();
    let probe = locked.parent().unwrap().join("tickets").join(".probe");
    if !cfg!(windows) && std::fs::write(&probe, "").is_ok() {
        let _ = std::fs::remove_file(&probe);
        #[allow(clippy::permissions_set_readonly_false)]
        permissions.set_readonly(false);
        std::fs::set_permissions(&locked, permissions).unwrap();
        eprintln!("skipped the unwritable store: this user writes into a read-only folder, as root does");
        return;
    }
    let unwritable = run(&p.repo, &["bottle", &ticket, "--store", &p.store.to_string_lossy()]);
    #[allow(clippy::permissions_set_readonly_false)]
    permissions.set_readonly(false);
    std::fs::set_permissions(&locked, permissions).unwrap();
    assert!(!unwritable.status.success(), "{}", text(&unwritable));
    assert!(text(&unwritable).contains(&name) && !text(&unwritable).contains(&format!("bottled {ticket}")), "{}", text(&unwritable));
    assert_eq!(std::fs::read(&record).unwrap(), kept, "the record is as it was");
    assert_eq!(status(), "simmering", "the ticket was not bottled");
}

/// # Test contract
///
/// ```json
/// {"test": "a-store-is-served-for-a-team-behind-tokens", "checks": ["crates/server/src#shared-serve", "crates/cli/src#token-add", "crates/cli/src#token-revoke", "crates/cli/src#token-list", "crates/cli/src#token-own-lock", "crates/cli/src#token-sent", "crates/cli/src#token-refused-says-so"]}
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
    let _daemon = Serving(grimoire().args(["serve", "--shared", "--store", &store, "--listen", "127.0.0.1:0"]).current_dir(&host).spawn().unwrap());
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
    assert_eq!(status_of(port, "POST", "/api/inscribe_ticket", &bearer(&ana), A_TICKET), 200);
    assert_eq!(status_of(port, "POST", "/api/inscribe_ticket", &bearer(&check), A_TICKET), 403, "read-only");
    assert_eq!(status_of(port, "POST", "/api/board", &bearer(&check), "{}"), 200);
    // A command beside the daemon sends its token and is answered by it; without one it is not let through.
    let via = grimoire().args(["tool", "--store", &store, "inscribe_ticket", A_TICKET]).current_dir(&host).env("GRIMOIRE_TOKEN", &ana).output().unwrap();
    assert!(via.status.success(), "{}", text(&via));
    let without = grimoire().args(["tool", "--store", &store, "inscribe_ticket", A_TICKET]).current_dir(&host).env_remove("GRIMOIRE_TOKEN").output().unwrap();
    assert!(!without.status.success() && text(&without).contains("refused this command") && text(&without).contains("GRIMOIRE_TOKEN"), "{}", text(&without));
    // A read without a token is refused the same way: it does not go round the daemon to a repository that is not there.
    let read = grimoire().args(["tool", "--store", &store, "board"]).current_dir(&host).env_remove("GRIMOIRE_TOKEN").output().unwrap();
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
/// {"test": "a-store-with-tokens-is-deleted-under-their-lock", "checks": ["crates/cli/src/uninstall#tokens-lock-held", "crates/cli/src/uninstall#lock-files-and-folder-last", "crates/server/src/store_io/tokens#tokens-locked-atomic", "crates/server/src/store_io/tokens#tokens-not-a-store"]}
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
    let mut uninstall = grimoire().args(["uninstall", "--delete-store"]).current_dir(&p.repo).stdout(std::process::Stdio::piped()).stderr(std::process::Stdio::piped()).spawn().unwrap();
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
    let _daemon = Serving(grimoire().args(["serve", "--port", "0"]).current_dir(&p.repo).spawn().unwrap());
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
    let filed = run(&p.repo, &["tool", "inscribe_ticket", A_TICKET]);
    assert!(filed.status.success(), "{}", text(&filed));
}

/// One command in a checkout of a team's project: Grimoire's own files under
/// `home`, and signed in only as `token` says, whatever this machine has set.
fn team(cwd: &Path, home: &Path, token: Option<&str>, args: &[&str]) -> Output {
    let mut c = grimoire();
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
/// {"test": "a-checkout-works-against-a-teams-server", "checks": ["crates/cli/src/init/command#init-server-writes", "crates/cli/src/init/command#init-server-exclusive", "crates/cli/src/init/command#init-server-bad-address", "crates/cli/src#serve-a-server-project", "crates/cli/src/team#token-from-flag-else-stdin", "crates/cli/src/team#address-from-project", "crates/cli/src/team#asked-of-the-server-before-kept", "crates/cli/src/team#kept-under-address", "crates/cli/src/team#refused-token-keeps-nothing", "crates/cli/src/team#logout-removes-token", "crates/cli/src/team#logout-says-when-none", "crates/cli/src/team#copy-synced-first", "crates/cli/src/team#copy-read-when-away", "crates/cli/src/team#copy-none-or-refused", "crates/cli/src/team#copy-daemon-asked", "crates/cli/src/team#runs-on-the-daemon-when-there-is-one", "crates/cli/src/team#read-tool-answers-over-the-copy", "crates/cli/src/team#write-tool-goes-through-the-server", "crates/cli/src/team#write-refused-when-server-away", "crates/cli/src/uninstall#server-copy-kept", "crates/server/src#serves-a-server-project", "crates/server/src/http#passes-the-token"]}
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
    let mut server = Some(Serving(grimoire().args(["serve", "--shared", "--store", &store, "--listen", "127.0.0.1:0"]).current_dir(&host).spawn().unwrap()));
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
        let mut child = grimoire()
            .arg("login")
            .current_dir(&p.repo)
            .env("GRIMOIRE_HOME", &home)
            .env_remove("GRIMOIRE_TOKEN")
            .stdin(std::process::Stdio::piped())
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped())
            .spawn()
            .unwrap();
        writeln!(child.stdin.take().unwrap(), "{ana}").unwrap();
        child.wait_with_output().unwrap()
    };
    assert!(login.status.success() && text(&login).contains("signed in"), "{}", text(&login));
    let kept = std::fs::read_to_string(home.join("credentials.json")).unwrap();
    assert!(
        kept.contains(&address) && kept.contains(&ana) && !files(&p.repo).iter().any(|f| std::fs::read_to_string(f).is_ok_and(|t| t.contains(&ana))),
        "kept under the server's address, and nowhere in the repository"
    );
    // The check runs with no daemon here: the server's intent, this checkout's code.
    let check = team(&p.repo, &home, None, &["check"]);
    assert!(check.status.success() && text(&check).contains("finding(s)"), "{}", text(&check));
    let copy = std::fs::read_dir(home.join("copies")).unwrap().flatten().next().unwrap().path();
    assert!(copy.join("grimoire.json").is_file() && copy.join("copy.json").is_file(), "the copy, laid out as a store, outside the repository");
    // A write goes to the server; the ticket it starts is this checkout's current one.
    let filed = team(&p.repo, &home, None, &["tool", "inscribe_ticket", A_TICKET]);
    assert!(filed.status.success(), "{}", text(&filed));
    let ticket = serde_json::from_slice::<serde_json::Value>(&filed.stdout).unwrap()["node"]["ref"].as_str().unwrap().to_string();
    let slug = ticket.trim_start_matches("ticket:").to_string();
    assert!(team(&p.repo, &home, None, &["tool", "brew_ticket", &format!(r#"{{"ref":"{ticket}"}}"#)]).status.success());
    let put = team(&p.repo, &home, None, &["tool", "put_page", r#"{"slug":"aim","title":"Aim","body":"b","links":[]}"#]);
    assert!(put.status.success(), "{}", text(&put));
    let bearer = format!("Host: 127.0.0.1\r\nAuthorization: Bearer {ana}\r\n");
    assert_eq!(status_of(port, "POST", "/api/get", &bearer, &format!(r#"{{"ref":"page:aim","ticket":"{slug}"}}"#)), 200, "in the ticket's proposal on the server");
    assert_eq!(status_of(port, "POST", "/api/get", &bearer, r#"{"ref":"page:aim"}"#), 404, "and not in the truth");
    // A commit on a team bottles nothing: the hook's unnamed bottling says where a ticket is bottled, and passes.
    let decided = team(&p.repo, &home, None, &["tool", "close_ticket", &format!(r#"{{"ref":"{ticket}","decision":{{"text":"Done."}},"when":"bottled"}}"#)]);
    assert!(decided.status.success(), "{}", text(&decided));
    let hook = team(&p.repo, &home, None, &["bottle"]);
    assert!(hook.status.success() && text(&hook).contains("nothing bottled") && text(&hook).contains("main line") && text(&hook).contains(&ticket), "{}", text(&hook));
    // The main line bottles it, by name, signed in as a pipeline is: by the environment, with nothing kept.
    let ci = p.dir.join("pipeline-home");
    let bottled = team(&p.repo, &ci, Some(&pipeline), &["bottle", &slug]);
    assert!(bottled.status.success() && text(&bottled).contains(&format!("bottled {ticket}")), "{}", text(&bottled));
    assert!(!ci.join("credentials.json").exists());
    assert_eq!(status_of(port, "POST", "/api/get", &bearer, r#"{"ref":"page:aim"}"#), 200, "applied to the server's truth");
    // A daemon in the checkout serves its copy with its code, and a command beside it goes through it.
    {
        let _daemon = Serving(grimoire().args(["serve", "--port", "0"]).current_dir(&p.repo).env("GRIMOIRE_HOME", &home).env_remove("GRIMOIRE_TOKEN").spawn().unwrap());
        let local = marked_port(&copy);
        assert_eq!(status_of(local, "GET", "/api/status", "Host: 127.0.0.1\r\n", ""), 200);
        let status = team(&p.repo, &home, None, &["tool", "status"]);
        let status: serde_json::Value = serde_json::from_slice(&status.stdout).unwrap();
        assert_eq!((status["server"]["address"].as_str(), status["server"]["reachable"].as_bool(), status["shared"].as_bool()), (Some(address.as_str()), Some(true), Some(false)), "{status}");
        assert!(status["nodes"]["module"].as_u64().unwrap() > 0, "the checkout's code is in its graph");
        let through = team(&p.repo, &home, None, &["tool", "inscribe_ticket", A_TICKET]);
        assert!(through.status.success(), "{}", text(&through));
        // The person's token is revoked while the daemon serves. The next write finds out, and from then
        // nothing is read on in silence, by the daemon or by a command beside it, until they sign in again.
        assert!(run(&host, &["token", "revoke", "ana", "--store", &store]).status.success());
        let refused = team(&p.repo, &home, None, &["tool", "inscribe_ticket", A_TICKET]);
        assert!(!refused.status.success() && text(&refused).contains("store_unavailable") && text(&refused).contains("grimoire login"), "{}", text(&refused));
        let check = team(&p.repo, &home, None, &["check"]);
        assert!(!check.status.success() && text(&check).contains("refused this machine's token"), "{}", text(&check));
        // Signed in again: the daemon takes up by itself, with no restart.
        assert!(team(&p.repo, &home, None, &["login", "--token", &again]).status.success());
        let check = team(&p.repo, &home, None, &["check"]);
        assert!(text(&check).contains("finding(s)") && !text(&check).contains("refused"), "{}", text(&check));
        assert!(team(&p.repo, &home, None, &["tool", "inscribe_ticket", A_TICKET]).status.success());
        // A command run with a token of its own goes to the server as itself, though the daemon it goes through could write.
        let reader = String::from_utf8_lossy(&run(&host, &["token", "add", "reader", "--read-only", "--store", &store]).stdout).trim().to_string();
        let as_reader = team(&p.repo, &home, Some(&reader), &["tool", "inscribe_ticket", A_TICKET]);
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
    // It fails only as it would with the server there: on the page the bottling left unlinked, a new finding.
    assert!(text(&offline).contains("cannot be reached") && text(&offline).contains("as of") && text(&offline).contains("1 new") && text(&offline).contains("page:aim"), "{}", text(&offline));
    let write = team(&p.repo, &home, None, &["tool", "inscribe_ticket", A_TICKET]);
    assert!(!write.status.success() && text(&write).contains("store_unavailable"), "{}", text(&write));
    // With the checkout's daemon running and holding the copy, a command asks it, and says the same.
    let _ = std::fs::remove_file(copy.join("serve.json"));
    {
        let _daemon = Serving(grimoire().args(["serve", "--port", "0"]).current_dir(&p.repo).env("GRIMOIRE_HOME", &home).env_remove("GRIMOIRE_TOKEN").spawn().unwrap());
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

/// # Test contract
///
/// ```json
/// {"test": "login-signs-in-to-the-address-given-in-place-of-the-projects", "checks": ["crates/cli/src/team#address-from-server-flag"]}
/// ```
#[test]
fn login_signs_in_to_the_address_given_in_place_of_the_projects() {
    let t = team_fixture(None);
    let address = format!("http://127.0.0.1:{}", t.port);
    // A checkout whose project file names another server, one that is not there.
    let other = uninitialised();
    let home = other.dir.join("home");
    std::fs::write(other.repo.join(".grimoire.toml"), "server = \"http://127.0.0.1:9\"\n").unwrap();
    let out = team(&other.repo, &home, None, &["login", "--server", &address, "--token", &t.writer]);
    assert!(out.status.success() && text(&out).contains(&format!("signed in to {address}")), "{}", text(&out));
    let kept = std::fs::read_to_string(home.join("credentials.json")).unwrap();
    assert!(kept.contains(&address) && kept.contains(&t.writer), "{kept}");
    assert!(!kept.contains("127.0.0.1:9\""), "nothing kept for the address the project names: {kept}");
}

/// # Test contract
///
/// ```json
/// {"test": "login-keeps-nothing-when-the-token-is-empty-or-the-server-silent", "checks": ["crates/cli/src/team#empty-token-keeps-nothing", "crates/cli/src/team#unanswering-server-keeps-nothing"]}
/// ```
#[test]
fn login_keeps_nothing_when_the_token_is_empty_or_the_server_silent() {
    let t = team_fixture(None);
    let (repo, home) = (&t.checkout.repo, t.checkout.dir.join("home"));
    // An empty token, given or read: fails, and nothing is kept.
    let given = team(repo, &home, None, &["login", "--token", "  "]);
    assert!(!given.status.success() && text(&given).contains("no token was given") && text(&given).contains("nothing was kept"), "{}", text(&given));
    let read = team(repo, &home, None, &["login"]);
    assert!(!read.status.success() && text(&read).contains("no token was given"), "{}", text(&read));
    assert!(!home.join("credentials.json").exists());
    // A server that does not answer: fails, and nothing is kept.
    let gone = TcpListener::bind("127.0.0.1:0").unwrap().local_addr().unwrap().port();
    let silent = team(repo, &home, None, &["login", "--server", &format!("http://127.0.0.1:{gone}"), "--token", &t.writer]);
    assert!(!silent.status.success() && text(&silent).contains("Nothing was kept"), "{}", text(&silent));
    assert!(!home.join("credentials.json").exists());
}

/// # Test contract
///
/// ```json
/// {"test": "a-write-is-refused-while-another-process-holds-the-copy", "checks": ["crates/cli/src/team#write-refused-while-lock-held"]}
/// ```
#[test]
fn a_write_is_refused_while_another_process_holds_the_copy() {
    let t = team_fixture(None);
    let (repo, home) = (&t.checkout.repo, t.checkout.dir.join("home"));
    assert!(team(repo, &home, Some(&t.writer), &["check"]).status.success(), "the first command makes the copy");
    let copy = std::fs::read_dir(home.join("copies")).unwrap().flatten().next().unwrap().path();
    let before = contents(&t.origin.store.join("tickets"));
    let held = grimoire_server::store_io::lock(&copy).unwrap();
    let refused = team(repo, &home, Some(&t.writer), &["tool", "inscribe_ticket", A_TICKET]);
    assert!(!refused.status.success() && text(&refused).contains("another process holds this checkout's copy"), "{}", text(&refused));
    assert_eq!(contents(&t.origin.store.join("tickets")), before, "nothing reached the server");
    drop(held);
    let filed = team(repo, &home, Some(&t.writer), &["tool", "inscribe_ticket", A_TICKET]);
    assert!(filed.status.success(), "{}", text(&filed));
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
    // An empty store, roots named and no page, so the server's intent shows exactly those two findings.
    let origin = uninitialised();
    let init = run(&origin.repo, &["init", "--store", &origin.store.to_string_lossy(), "--empty"]);
    assert!(init.status.success(), "{}", text(&init));
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
    let server = Serving(grimoire().args(["serve", "--shared", "--store", &store, "--listen", "127.0.0.1:0"]).current_dir(&host).spawn().unwrap());
    let port = marked_port(&origin.store);
    let checkout = uninitialised();
    let home = checkout.dir.join("home");
    assert!(team(&checkout.repo, &home, None, &["init", "--server", &format!("http://127.0.0.1:{port}")]).status.success());
    TeamFixture { origin, checkout, writer, reader, port, _server: server }
}

/// # Test contract
///
/// ```json
/// {"test": "the-main-line-prunes-a-teams-baseline", "checks": ["crates/cli/src/check#check-main-line-judges-the-truth", "crates/cli/src/check#check-main-line-refused-beside-ticket", "crates/cli/src/check#check-main-line-prunes-the-server", "crates/cli/src/check#check-main-line-prints-findings", "crates/cli/src/check#check-main-line-says-what-it-did", "crates/cli/src/check#check-main-line-exit", "crates/cli/src/check#check-main-line-own-store", "crates/cli/src/check#bottles-the-given-or-current-simmering-ticket", "crates/cli/src/check#prints-what-was-bottled", "crates/cli/src/check#prunes-a-teams-baseline-after-bottling", "crates/cli/src/check#says-how-many-entries-went", "crates/cli/src/team#prune-sends-findings-with-digest", "crates/cli/src/team#prune-answers-what-it-judged", "crates/cli/src/team#prune-refused-says-why", "crates/cli/src/team#prune-fails-outright-only-when-intent-unreadable", "crates/cli/src/team#read-now", "crates/server/src/http#prune-route-prunes", "crates/server/src/http#prune-conflict-when-intent-moved", "crates/server/src/http#prune-refused-to-reader", "crates/server/src/http#prune-refused-non-json", "crates/server/src/http#prune-invalid-body", "crates/server/src/http#prune-conflict-on-code-daemon", "crates/server/src/http#prune-route-not-tool", "crates/server/src/remote#intent-digest-names-intent"]}
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
    let beside = team(repo, &ci, Some(&t.writer), &["check", "--main-line", "--ticket", "x"]);
    assert!(!beside.status.success() && text(&beside).contains("cannot be used with"), "refused beside --ticket: {}", text(&beside));
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
    // A bottling on a team prunes against the truth with the ticket applied: src's missing contract is fixed on the main line.
    let tool = |name: &str, input: &str| assert!(team(repo, &home, Some(&t.writer), &["tool", name, input]).status.success(), "{name}");
    // The main line judges the truth whatever the current ticket: a ticket current here, proposing a dangling edge, plays no part.
    let other = team(repo, &home, Some(&t.writer), &["tool", "inscribe_ticket", A_TICKET]);
    let other = serde_json::from_slice::<serde_json::Value>(&other.stdout).unwrap()["node"]["ref"].as_str().unwrap().to_string();
    tool("brew_ticket", &format!(r#"{{"ref":"{other}"}}"#));
    tool(
        "put_requirement",
        &format!(
            r#"{{"slug":"main-only","title":"Main only","statement":"It SHALL hold.","scenarios":[{{"title":"s","given":"g","when":"w","then":"t"}}],"covered_by":["criterion:src#nowhere"],"ticket":"{other}"}}"#
        ),
    );
    let judged = team(repo, &home, Some(&t.writer), &["check", "--main-line", "--json"]);
    let answer: serde_json::Value = serde_json::from_slice(&judged.stdout).unwrap();
    let theirs = answer["findings"].as_array().unwrap().iter().any(|f| f["ref"] == "req:main-only");
    assert!(answer["new"] == serde_json::json!(2) && !theirs, "judged against the truth alone, the proposal only an advisory: {}", text(&judged));
    tool("withdraw_ticket", &format!(r#"{{"ref":"{other}","decision":{{"text":"Filed only to show the main line ignores it."}}}}"#));
    let filed = team(repo, &home, Some(&t.writer), &["tool", "inscribe_ticket", A_TICKET]);
    let ticket = serde_json::from_slice::<serde_json::Value>(&filed.stdout).unwrap()["node"]["ref"].as_str().unwrap().to_string();
    tool("brew_ticket", &format!(r#"{{"ref":"{ticket}"}}"#));
    tool("close_ticket", &format!(r#"{{"ref":"{ticket}","decision":{{"text":"Done."}},"when":"bottled"}}"#));
    std::fs::write(repo.join("src").join("lib.rs"), CONTRACTED).unwrap();
    let bottled = team(repo, &ci, Some(&t.writer), &["bottle", ticket.trim_start_matches("ticket:")]);
    assert!(bottled.status.success() && text(&bottled).contains(&format!("bottled {ticket}")) && text(&bottled).contains("pruned 1 baselined finding(s)"), "{}", text(&bottled));
    assert_eq!(baseline_count(&t.origin.store), adopted - 3);
    // With a daemon serving the pipeline's checkout, the main line still reads the server, never that daemon's copy.
    let copy = {
        let _daemon = Serving(grimoire().args(["serve", "--port", "0"]).current_dir(repo).env("GRIMOIRE_HOME", &ci).env("GRIMOIRE_TOKEN", &t.writer).spawn().unwrap());
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
    // two roots, which the empty store names and no page yet carries; none is added.
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

/// # Test contract
///
/// ```json
/// {"test": "a-command-on-a-team-prunes-no-baseline", "checks": ["crates/cli/src/check#check-no-prune-on-server", "crates/cli/src/team#copy-never-prunes"]}
/// ```
#[test]
fn a_command_on_a_team_prunes_no_baseline() {
    let t = team_fixture(None);
    let repo = &t.checkout.repo;
    let home = t.checkout.dir.join("home");
    let adopted = baseline_count(&t.origin.store);
    assert!(adopted > 0);
    // The checkout's first command made its copy of the server's intent, baseline and all.
    assert!(team(repo, &home, Some(&t.writer), &["check"]).status.success());
    let copy = std::fs::read_dir(home.join("copies")).unwrap().next().unwrap().unwrap().path();
    assert_eq!(baseline_count(&copy), adopted);
    // This checkout fixes the findings of src/a that it was inherited with: its check, and a tool run beside it, prune nothing,
    // neither the server's baseline nor the copy's.
    std::fs::remove_file(repo.join("src").join("a.rs")).unwrap();
    let check = team(repo, &home, Some(&t.writer), &["check"]);
    assert!(check.status.success() && !text(&check).contains("pruned"), "{}", text(&check));
    let tool = team(repo, &home, Some(&t.writer), &["tool", "inscribe_ticket", A_TICKET]);
    assert!(tool.status.success(), "{}", text(&tool));
    let lint = team(repo, &home, Some(&t.writer), &["tool", "lint", "{}"]);
    assert!(lint.status.success(), "{}", text(&lint));
    assert_eq!(baseline_count(&t.origin.store), adopted, "the server's baseline");
    assert_eq!(baseline_count(&copy), adopted, "and the copy's");
}

/// # Test contract
///
/// ```json
/// {"test": "a-server-project-keeps-its-store-in-a-copy-and-runs-no-token", "checks": ["crates/cli/src#store-found-server-copy", "crates/cli/src#token-finds-store"]}
/// ```
#[test]
fn a_server_project_keeps_its_store_in_a_copy_and_runs_no_token() {
    let t = team_fixture(None);
    let repo = &t.checkout.repo;
    let home = t.checkout.dir.join("home");
    let origin = t.origin.store.to_string_lossy().to_string();
    // A command on the project that names a server: its store is this checkout's copy of the server's intent, in the machine's copies.
    let checked = team(repo, &home, Some(&t.writer), &["check"]);
    assert!(checked.status.success(), "{}", text(&checked));
    let copies: Vec<PathBuf> = std::fs::read_dir(home.join("copies")).unwrap().flatten().map(|e| e.path()).collect();
    assert_eq!(copies.len(), 1, "{copies:?}");
    assert!(copies[0].join("grimoire.json").is_file() && copies[0].join("baseline.json").is_file(), "the server's intent, read into the copy");
    assert!(!repo.join("store").exists() && !repo.join("grimoire.json").exists(), "nothing of it in the repository");
    // The same store is found from below the project file: the copy is what uninstall says it keeps.
    let kept = team(&repo.join("src"), &home, None, &["uninstall"]);
    assert!(kept.status.success() && text(&kept).contains("this checkout's copy") && text(&kept).contains(&copies[0].to_string_lossy().to_string()), "{}", text(&kept));
    assert!(team(repo, &home, None, &["init", "--server", &format!("http://127.0.0.1:{}", t.port)]).status.success());
    // The tokens are the host's, kept beside the server's store: each token command refuses, and nothing is made or removed there.
    let tokens_before = std::fs::read(t.origin.store.join("tokens.json")).unwrap();
    for args in [vec!["token", "add", "mine"], vec!["token", "revoke", "pipeline"], vec!["token", "list"]] {
        let out = team(repo, &home, Some(&t.writer), &args);
        assert!(!out.status.success() && text(&out).contains("this project's grimoire is the server's at") && text(&out).contains("--store"), "{args:?}: {}", text(&out));
    }
    assert_eq!(std::fs::read(t.origin.store.join("tokens.json")).unwrap(), tokens_before);
    // Naming the host's store with --store is allowed, from the same checkout.
    let listed = team(repo, &home, None, &["token", "list", "--store", &origin]);
    assert!(listed.status.success() && text(&listed).contains("pipeline"), "{}", text(&listed));
    // On a project of a store of its own, each finds it as every command does, from below the project file.
    let (own, _) = project();
    let below = own.repo.join("src");
    let made = run(&below, &["token", "add", "ana"]);
    assert!(made.status.success(), "{}", text(&made));
    assert!(text(&run(&below, &["token", "list"])).contains("ana"));
    assert!(own.store.join("tokens.json").is_file(), "kept beside the project's store");
    assert!(run(&below, &["token", "revoke", "ana"]).status.success());
    assert!(!text(&run(&below, &["token", "list"])).contains("ana"));
}

/// # Test contract
///
/// ```json
/// {"test": "the-main-line-sends-its-recipes-to-a-teams-site", "checks": ["crates/cli/src/team#sends-recipes", "crates/cli/src/check#check-main-line-sends-recipes", "crates/cli/src/check#check-main-line-says-what-it-did", "crates/cli/src/check#check-main-line-says-recipes-not-taken", "crates/cli/src/check#check-main-line-exit", "crates/cli/src/check#warns-why-recipes-were-not-taken", "crates/server/src/http#recipes", "req:recipe-main-line-snapshot"]}
/// ```
#[test]
fn the_main_line_sends_its_recipes_to_a_teams_site() {
    let t = team_fixture(None);
    let repo = &t.checkout.repo;
    let (home, ci) = (t.checkout.dir.join("home"), t.checkout.dir.join("pipeline-home"));
    let recipe = |id: &str, title: &str| format!("# {title}\n\nHow to use {id}.\n\n## Recipe\n\n```json\n{{\"id\":\"{id}\",\"kind\":\"component\",\"files\":[\"src/a.rs\"]}}\n```\n");
    std::fs::create_dir_all(repo.join("ui")).unwrap();
    std::fs::write(repo.join("ui").join("button.recipe.md"), recipe("button", "Button")).unwrap();
    std::fs::write(repo.join("ui").join("icon.recipe.md"), recipe("icon", "Icon")).unwrap();
    let held = || -> serde_json::Value { std::fs::read_to_string(t.origin.store.join("recipes.json")).map(|s| serde_json::from_str(&s).unwrap()).unwrap_or_default() };
    // A branch sends nothing.
    team(repo, &home, Some(&t.writer), &["check"]);
    assert!(!t.origin.store.join("recipes.json").exists(), "a branch's check sends no recipe");
    // The main line, with a token that may only read: said, and the check is not failed by it.
    let refused = team(repo, &ci, Some(&t.reader), &["check", "--main-line"]);
    assert!(text(&refused).contains("did not take the main line's recipes") && text(&refused).contains("may only read"), "{}", text(&refused));
    assert!(!t.origin.store.join("recipes.json").exists());
    // With a write token: sent, without their files, and said on standard error under --json.
    let sent = team(repo, &ci, Some(&t.writer), &["check", "--main-line", "--json"]);
    let findings: serde_json::Value = serde_json::from_slice(&sent.stdout).unwrap();
    assert!(String::from_utf8_lossy(&sent.stderr).contains("it holds 2"), "{}", text(&sent));
    assert_eq!(sent.status.success(), findings["new"] == 0, "it exits as the check would on its findings: {}", text(&sent));
    let record = held();
    let ids: Vec<&str> = record["recipes"].as_array().unwrap().iter().map(|r| r["id"].as_str().unwrap()).collect();
    assert_eq!(ids, ["button", "icon"]);
    assert_eq!(record["recipes"][0]["title"], "Button");
    assert!(!record.to_string().contains("src/a.rs") && !record.to_string().contains("button.recipe.md"), "{record}");
    // A send the server does not take, here a body past what the route reads, is said and fails nothing: the ticket is bottled, the prune is taken.
    std::fs::remove_file(repo.join("ui").join("icon.recipe.md")).unwrap();
    std::fs::write(repo.join("ui").join("big.recipe.md"), format!("# Big\n\n{}\n\n## Recipe\n\n```json\n{{\"id\":\"big\",\"kind\":\"component\"}}\n```\n", "word ".repeat(1 << 20))).unwrap();
    let tool = |name: &str, input: &str| team(repo, &home, Some(&t.writer), &["tool", name, input]);
    let ticket = serde_json::from_slice::<serde_json::Value>(&tool("inscribe_ticket", A_TICKET).stdout).unwrap()["node"]["ref"].as_str().unwrap().to_string();
    assert!(tool("brew_ticket", &format!(r#"{{"ref":"{ticket}"}}"#)).status.success());
    assert!(tool("close_ticket", &format!(r#"{{"ref":"{ticket}","decision":{{"text":"Done."}},"when":"bottled"}}"#)).status.success());
    let bottled = team(repo, &ci, Some(&t.writer), &["bottle", ticket.trim_start_matches("ticket:")]);
    assert!(bottled.status.success() && text(&bottled).contains("pruned") && text(&bottled).contains("recipes were not sent"), "{}", text(&bottled));
    assert_eq!(held()["recipes"].as_array().unwrap().len(), 2, "the record the server last took stays");
    // The next main-line check sends them again, replaced whole.
    std::fs::remove_file(repo.join("ui").join("big.recipe.md")).unwrap();
    let again = team(repo, &ci, Some(&t.writer), &["check", "--main-line"]);
    assert!(text(&again).contains("it holds 1"), "{}", text(&again));
    assert_eq!(held()["recipes"].as_array().unwrap().len(), 1);
    // The export carries it to a machine's copy.
    let bearer = |token: &str| format!("Host: 127.0.0.1\r\nAuthorization: Bearer {token}\r\n");
    let (_, export) = http(t.port, &format!("GET /api/export HTTP/1.0\r\n{}\r\n", bearer(&t.reader)));
    assert!(export.contains("\"main_line_recipes\"") && export.contains("How to use button."), "{export}");
    // The route itself: a writer, a JSON body that is the record and validates, and not offered to an agent.
    let one = r#"{"recipes":[{"id":"quest","kind":"content","title":"Quest","text":"Go.","ingredients":[],"uses":[],"digest":"0123456789abcdef"}]}"#;
    assert_eq!(status_of(t.port, "POST", "/api/recipes", &bearer(&t.reader), one), 403);
    assert_eq!(status_of(t.port, "POST", "/api/recipes", &bearer(&t.writer), r#"{"recipes":[],"more":1}"#), 400);
    assert_eq!(status_of(t.port, "POST", "/api/recipes", &bearer(&t.writer), &one.replace("0123456789abcdef", "nope")), 400, "a record that does not validate");
    let (status, _) = http(t.port, &format!("POST /api/recipes HTTP/1.0\r\n{}Content-Type: text/plain\r\nContent-Length: 2\r\n\r\n{{}}", bearer(&t.writer)));
    assert_eq!(status, 415);
    assert_eq!(held()["recipes"][0]["id"], "button", "nothing refused changed it");
    // Past the 2 MiB a body is otherwise held to, and within 4 MiB, the record is taken.
    let long = one.replace("Go.", &"word ".repeat(700_000));
    assert!(long.len() > 3 << 20);
    assert_eq!(status_of(t.port, "POST", "/api/recipes", &bearer(&t.writer), &long), 200);
    assert_eq!(held()["recipes"][0]["id"], "quest");
    team(repo, &ci, Some(&t.writer), &["check", "--main-line"]);
    assert_eq!(held()["recipes"][0]["id"], "button", "the main line's again");
    let (_, tools) = http(t.port, &format!("GET /api/tools HTTP/1.0\r\n{}\r\n", bearer(&t.reader)));
    assert!(!tools.contains("\"recipes\""), "{tools}");
    let (own, _) = project();
    let (_daemon, local) = serve(&own);
    assert_eq!(status_of(local, "POST", "/api/recipes", "Host: 127.0.0.1\r\n", one), 409, "a daemon that holds code reads its own");
    drop(_daemon);
    // The team's site, served by the server that holds no code, shows the catalog the main line sent, and none of its files.
    let store = t.origin.store.to_string_lossy().to_string();
    drop(t._server);
    let site = std::net::TcpListener::bind("127.0.0.1:0").unwrap().local_addr().unwrap().port();
    let host = t.origin.dir.join("host");
    let _server = Serving(grimoire().args(["serve", "--shared", "--store", &store, "--listen", "127.0.0.1:0", "--site", &format!("127.0.0.1:{site}")]).current_dir(&host).spawn().unwrap());
    eventually("the site answers", || std::net::TcpStream::connect(("127.0.0.1", site)).is_ok());
    let get = |path: &str| http(site, &format!("GET {path} HTTP/1.0\r\nHost: 127.0.0.1:{site}\r\n\r\n"));
    let (code, catalog) = get("/recipes/");
    assert!(code == 200 && catalog.contains("/recipes/button") && !catalog.contains("/recipes/icon"), "{catalog}");
    let (code, button) = get("/recipes/button");
    assert!(code == 200 && button.contains("How to use button.") && !button.contains("src/a.rs"), "{button}");
}

/// # Test contract
///
/// ```json
/// {"test": "main-line-check-passes-though-recipes-not-taken", "checks": ["crates/cli/src/check#check-main-line-exit"]}
/// ```
#[test]
fn the_main_line_check_is_not_failed_by_recipes_the_server_did_not_take() {
    use std::io::Write;
    let t = team_fixture(None);
    let (_, intent) = http(t.port, &format!("GET /api/export HTTP/1.0\r\nHost: 127.0.0.1\r\nAuthorization: Bearer {}\r\n\r\n", t.reader));
    // A server with the same intent that takes the prune and knows no route for recipes, as an older one.
    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let address = format!("http://{}", listener.local_addr().unwrap());
    std::thread::spawn(move || {
        for mut stream in listener.incoming().flatten() {
            let head = read_request(&mut stream);
            let (status, body) = if head.starts_with("GET /api/export") {
                (200, intent.clone())
            } else if head.starts_with("POST /api/prune") {
                (200, r#"{"pruned":0}"#.to_string())
            } else {
                (404, r#"{"error":"not_found","message":"no such route"}"#.to_string())
            };
            let _ = stream.write_all(format!("HTTP/1.1 {status} X\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}", body.len()).as_bytes());
        }
    });
    let other = uninitialised();
    let home = other.dir.join("home");
    assert!(team(&other.repo, &home, None, &["init", "--server", &address]).status.success());
    let check = team(&other.repo, &home, Some(&t.writer), &["check", "--main-line"]);
    assert!(text(&check).contains("did not take the main line's recipes"), "the recipes were not taken, and it says so: {}", text(&check));
    assert!(text(&check).contains("pruned 0") && check.status.success(), "and the check is not failed by it: {}", text(&check));
}

/// One request read whole from `stream`: its head and body as text.
fn read_request(stream: &mut std::net::TcpStream) -> String {
    use std::io::Read;
    let mut got = Vec::new();
    let mut chunk = [0u8; 4096];
    loop {
        let n = stream.read(&mut chunk).unwrap_or(0);
        got.extend_from_slice(&chunk[..n]);
        let text = String::from_utf8_lossy(&got).to_string();
        let Some((head, body)) = text.split_once("\r\n\r\n") else {
            if n == 0 {
                return text;
            }
            continue;
        };
        let wanted: usize = head.lines().find_map(|l| l.to_ascii_lowercase().strip_prefix("content-length:").and_then(|v| v.trim().parse().ok())).unwrap_or(0);
        if body.len() >= wanted || n == 0 {
            return text;
        }
    }
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
/// {"test": "a-build-that-cannot-read-a-language-does-not-prune", "checks": ["crates/cli/src/team#prune-unreadable-language-sends-nothing", "crates/cli/src/check#exits-non-zero-when-the-prune-fails"]}
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
    // A bottling is taken, then its prune is refused the same way: the command says the ticket was bottled, and fails.
    let home = t.checkout.dir.join("home");
    let tool = |name: &str, input: &str| team(&t.checkout.repo, &home, Some(&t.writer), &["tool", name, input]);
    let ticket = serde_json::from_slice::<serde_json::Value>(&tool("inscribe_ticket", A_TICKET).stdout).unwrap()["node"]["ref"].as_str().unwrap().to_string();
    assert!(tool("brew_ticket", &format!(r#"{{"ref":"{ticket}"}}"#)).status.success());
    assert!(tool("close_ticket", &format!(r#"{{"ref":"{ticket}","decision":{{"text":"Done."}},"when":"bottled"}}"#)).status.success());
    let bottled = team(&t.checkout.repo, &ci, Some(&t.writer), &["bottle", ticket.trim_start_matches("ticket:")]);
    assert!(
        !bottled.status.success() && text(&bottled).contains(&format!("bottled {ticket}")) && text(&bottled).contains("the ticket was bottled, but the baseline was not pruned"),
        "{}",
        text(&bottled)
    );
    let status = serde_json::from_slice::<serde_json::Value>(&tool("get", &format!(r#"{{"ref":"{ticket}"}}"#)).stdout).unwrap();
    assert_eq!(status["node"]["status"], "bottled");
    assert_eq!(baseline_count(&t.origin.store), adopted);
}

/// # Test contract
///
/// ```json
/// {"test": "a-pipeline-checks-a-tickets-code-against-its-proposal", "checks": ["crates/cli/src/check#check-ticket-judges-its-proposal", "crates/cli/src/check#check-ticket-refused-when-not-live", "crates/cli/src/check#check-ticket-refused-when-intent-unreadable", "crates/cli/src/check#check-defaults-to-current-ticket", "crates/cli/src/team#copy-read-when-away", "crates/cli/src/team#copy-named-check-fails"]}
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
    let _server = Serving(grimoire().args(["serve", "--shared", "--store", &store, "--listen", "127.0.0.1:0"]).current_dir(&host).spawn().unwrap());
    let address = format!("http://127.0.0.1:{}", marked_port(&origin.store));
    let p = uninitialised();
    let home = p.dir.join("home");
    assert!(team(&p.repo, &home, None, &["init", "--server", &address]).status.success());
    let tool = |name: &str, input: &str| -> serde_json::Value {
        let o = team(&p.repo, &home, Some(&ana), &["tool", name, input]);
        assert!(o.status.success(), "{name}: {}", text(&o));
        serde_json::from_slice(&o.stdout).unwrap()
    };
    let ticket = tool("inscribe_ticket", A_TICKET)["node"]["ref"].as_str().unwrap().to_string();
    let slug = ticket.trim_start_matches("ticket:").to_string();
    tool("brew_ticket", &format!(r#"{{"ref":"{ticket}"}}"#));
    // The proposal names a criterion this code does not have yet.
    tool(
        "put_requirement",
        r#"{"slug":"proposed","title":"Proposed","statement":"It SHALL offer b.","scenarios":[{"title":"t","given":"g","when":"w","then":"t"}],"covered_by":["criterion:src/b#exports-b"]}"#,
    );
    let open = tool("inscribe_ticket", A_TICKET)["node"]["ref"].as_str().unwrap().to_string();
    // The pipeline: signed in by the environment with a read-only token, nothing kept, no current ticket.
    let ci = p.dir.join("pipeline-home");
    let check = |args: &[&str]| team(&p.repo, &ci, Some(&pipeline), &[&["check"][..], args].concat());
    let truth = check(&[]);
    // The proposal's requirement is no finding of the truth's; the ticket being worked shows it only as other work.
    let findings = text(&truth).split("may conflict").next().unwrap_or_default().to_string();
    assert!(truth.status.success() && !findings.contains("req:proposed"), "no ticket named is judged against the truth: {}", text(&truth));
    for named in [slug.as_str(), ticket.as_str()] {
        let judged = check(&["--ticket", named]);
        assert!(!judged.status.success() && text(&judged).contains("req:proposed") && text(&judged).contains("criterion:src/b#exports-b"), "judged against the proposal: {}", text(&judged));
    }
    assert!(!ci.join("credentials.json").exists());
    // A ticket with no proposal to read, and one that does not exist, are said and fail.
    let not_live = check(&["--ticket", &open]);
    assert!(!not_live.status.success() && text(&not_live).contains("neither brewing nor simmering"), "{}", text(&not_live));
    let missing = check(&["--ticket", "no-such-ticket"]);
    assert!(!missing.status.success() && text(&missing).contains("cannot check against ticket:no-such-ticket"), "{}", text(&missing));
    // The person's checkout has the ticket as its current one: with none named, its proposal is the intent; a ticket named overrides it.
    let mine = team(&p.repo, &home, Some(&ana), &["check"]);
    assert!(!mine.status.success() && text(&mine).contains("req:proposed"), "the current ticket's proposal: {}", text(&mine));
    let overridden = team(&p.repo, &home, Some(&ana), &["check", "--ticket", &open]);
    assert!(!overridden.status.success() && text(&overridden).contains("neither brewing nor simmering"), "{}", text(&overridden));
    // A simmering ticket keeps its proposal, so the pipeline still judges against it, the same in JSON.
    tool("close_ticket", &format!(r#"{{"ref":"{ticket}","decision":{{"text":"Done."}},"when":"bottled"}}"#));
    let bottling = check(&["--ticket", &slug, "--json"]);
    let found: serde_json::Value = serde_json::from_slice(&bottling.stdout).unwrap();
    assert!(!bottling.status.success() && found["new"].as_u64().unwrap() > 0 && found["findings"].as_array().unwrap().iter().any(|f| f["ref"] == "req:proposed"), "{}", text(&bottling));
    // Another process holds the runner's copy, so it cannot be fetched now: a named ticket refuses to read it, though the server is there.
    let ci_copy = std::fs::read_dir(ci.join("copies")).unwrap().flatten().next().unwrap().path();
    {
        let _held = grimoire_server::remote::lock_copy(&ci_copy).unwrap();
        let held = check(&["--ticket", &slug]);
        assert!(!held.status.success() && text(&held).contains("another process holds") && !text(&held).contains("req:proposed"), "{}", text(&held));
    }
    // A daemon serving the runner's checkout keeps the copy current: while it says the server answers, a named check reads it.
    let ci_daemon = Serving(grimoire().args(["serve", "--port", "0"]).current_dir(&p.repo).env("GRIMOIRE_HOME", &ci).env("GRIMOIRE_TOKEN", &pipeline).spawn().unwrap());
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

/// # Test contract
///
/// ```json
/// {"test": "bottle-names-what-still-simmers-and-what-moved", "checks": ["crates/cli/src/check#names-the-tickets-still-simmering", "crates/cli/src/check#prints-the-records-that-moved"]}
/// ```
#[test]
fn bottle_names_what_still_simmers_and_what_moved() {
    let (p, _) = project();
    let tool = |name: &str, input: &str| -> serde_json::Value {
        let o = run(&p.repo, &["tool", name, input]);
        assert!(o.status.success(), "{name}: {}", text(&o));
        serde_json::from_slice(&o.stdout).unwrap()
    };
    // A change that proposes a page and waits to be bottled.
    let waits = |title: &str, page: &str, body: &str| -> String {
        let filed = tool("inscribe_ticket", &format!(r#"{{"title":"{title}","kind":"change","priority":"later","body":"b"}}"#));
        let ticket = filed["node"]["ref"].as_str().unwrap().to_string();
        tool("brew_ticket", &format!(r#"{{"ref":"{ticket}"}}"#));
        tool("put_page", &format!(r#"{{"slug":"{page}","title":"{page}","body":"{body}","links":[]}}"#));
        tool("close_ticket", &format!(r#"{{"ref":"{ticket}","decision":{{"text":"Done."}},"when":"bottled"}}"#));
        ticket
    };
    // Two simmering: bottling the first names the second, and only the second.
    let (first, second) = (waits("first one", "one", "1"), waits("second one", "two", "2"));
    let bottled = run(&p.repo, &["bottle", first.trim_start_matches("ticket:")]);
    assert!(bottled.status.success() && text(&bottled).contains(&format!("bottled {first}")), "{}", text(&bottled));
    let out = String::from_utf8_lossy(&bottled.stdout).to_string();
    assert!(out.contains(&format!("still simmering: {second} (second one);")) && !out.contains(&format!("still simmering: {first}")), "{out}");
    // A page one ticket waits to change, moved under it by another ticket bottled first.
    let keeps = waits("keeps a page", "shared", "v1");
    assert!(run(&p.repo, &["bottle", keeps.trim_start_matches("ticket:")]).status.success());
    let late = waits("late change", "shared", "v2");
    let filed = tool("inscribe_ticket", r#"{"title":"early change","kind":"change","priority":"later","body":"b"}"#);
    let early = filed["node"]["ref"].as_str().unwrap().to_string();
    tool("brew_ticket", &format!(r#"{{"ref":"{early}"}}"#));
    tool("put_page", r#"{"slug":"shared","title":"shared","body":"v3","links":[]}"#);
    tool("close_ticket", &format!(r#"{{"ref":"{early}","decision":{{"text":"Done."}}}}"#));
    let refused = run(&p.repo, &["bottle", late.trim_start_matches("ticket:")]);
    let err = String::from_utf8_lossy(&refused.stderr).to_string();
    assert!(!refused.status.success() && err.contains("bottling refused") && err.contains("  page:shared: "), "{}", text(&refused));
    assert_eq!(tool("get", &format!(r#"{{"ref":"{late}"}}"#))["node"]["status"], "simmering", "nothing bottled");
}

/// # Test contract
///
/// ```json
/// {"test": "a-teams-bottling-sends-the-recipes-after-the-prune", "checks": ["crates/cli/src/check#team-without-a-ticket-says-the-main-line-bottles", "crates/cli/src/check#sends-the-recipes-after-the-prune", "crates/cli/src/check#says-how-many-recipes-the-server-holds"]}
/// ```
#[test]
fn a_teams_bottling_sends_the_recipes_after_the_prune() {
    let t = team_fixture(None);
    let repo = &t.checkout.repo;
    let (home, ci) = (t.checkout.dir.join("home"), t.checkout.dir.join("pipeline-home"));
    let recipe = |id: &str| format!("# {id}\n\nHow to use {id}.\n\n## Recipe\n\n```json\n{{\"id\":\"{id}\",\"kind\":\"component\",\"files\":[\"src/a.rs\"]}}\n```\n");
    std::fs::create_dir_all(repo.join("ui")).unwrap();
    std::fs::write(repo.join("ui").join("button.recipe.md"), recipe("button")).unwrap();
    std::fs::write(repo.join("ui").join("icon.recipe.md"), recipe("icon")).unwrap();
    let tool = |name: &str, input: &str| team(repo, &home, Some(&t.writer), &["tool", name, input]);
    let ticket = serde_json::from_slice::<serde_json::Value>(&tool("inscribe_ticket", A_TICKET).stdout).unwrap()["node"]["ref"].as_str().unwrap().to_string();
    assert!(tool("brew_ticket", &format!(r#"{{"ref":"{ticket}"}}"#)).status.success());
    assert!(tool("close_ticket", &format!(r#"{{"ref":"{ticket}","decision":{{"text":"Done."}},"when":"bottled"}}"#)).status.success());
    let held = || -> Vec<String> {
        let record: serde_json::Value = std::fs::read_to_string(t.origin.store.join("recipes.json")).map(|s| serde_json::from_str(&s).unwrap()).unwrap_or_default();
        record["recipes"].as_array().into_iter().flatten().map(|r| r["id"].as_str().unwrap().to_string()).collect()
    };
    // With no ticket given, nothing is bottled and the output says a ticket is bottled on the main line; no recipe is sent.
    let unnamed = team(repo, &ci, Some(&t.writer), &["bottle"]);
    assert!(unnamed.status.success(), "{}", text(&unnamed));
    assert!(
        String::from_utf8_lossy(&unnamed.stdout).contains("nothing bottled: this project names a server: a ticket is bottled on the main line, by name, once its change has merged"),
        "{}",
        text(&unnamed)
    );
    assert!(held().is_empty() && !text(&unnamed).contains("sent the server"), "{}", text(&unnamed));
    // Named, the ticket is bottled, the baseline pruned, and then the recipes sent: the server holds them, and the output says how many.
    let bottled = team(repo, &ci, Some(&t.writer), &["bottle", ticket.trim_start_matches("ticket:")]);
    assert!(bottled.status.success(), "{}", text(&bottled));
    let out = String::from_utf8_lossy(&bottled.stdout).to_string();
    let (pruned, sent) = (out.find("pruned "), out.find("sent the server the main line's recipes: it holds 2"));
    assert!(pruned.is_some() && sent.is_some() && pruned < sent, "{out}");
    assert_eq!(held(), ["button", "icon"]);
}

/// # Test contract
///
/// ```json
/// {"test": "outside-git-init-writes-no-hook-and-says-so", "checks": ["crates/cli/src/init#outside-git-writes-no-hook-and-says-so", "crates/cli/src/init#hooks-where-git-runs-them", "req:init-hooks-where-git-runs-them"]}
/// ```
#[test]
fn outside_git_init_writes_no_hook_and_says_so() {
    let p = uninitialised();
    assert!(!p.repo.join(".git").exists());
    let init = run(&p.repo, &["init", "--store", &p.store.to_string_lossy()]);
    assert!(init.status.success(), "{}", text(&init));
    let out = String::from_utf8_lossy(&init.stdout).to_string();
    for hook in ["pre-commit", "post-commit"] {
        assert!(!p.repo.join(".git").join("hooks").join(hook).exists(), "{hook} written");
        assert!(out.lines().any(|l| l.starts_with("skipped ") && l.contains(hook) && l.contains("not a git repository")), "{hook} not said: {out}");
    }
    assert!(!p.repo.join(".git").exists(), "no repository made for the hooks");
}

/// # Test contract
///
/// ```json
/// {"test": "the-hooks-exit-zero-when-grimoire-is-not-on-the-path", "checks": ["crates/cli/src/init#hooks-exit-zero-when-grimoire-is-missing"]}
/// ```
#[test]
fn the_hooks_exit_zero_when_grimoire_is_not_on_the_path() {
    let p = uninitialised();
    std::fs::create_dir_all(p.repo.join(".git").join("hooks")).unwrap();
    let init = run(&p.repo, &["init", "--store", &p.store.to_string_lossy()]);
    assert!(init.status.success(), "{}", text(&init));
    // The shell, found on this machine's path before the hooks are given a path without grimoire.
    let sh = std::env::split_paths(&std::env::var_os("PATH").unwrap_or_default()).flat_map(|d| [d.join("sh"), d.join("sh.exe")]).find(|c| c.is_file()).expect("this test needs a sh");
    let empty = p.dir.join("empty-path");
    std::fs::create_dir_all(&empty).unwrap();
    for (hook, warning) in [("pre-commit", "pre-commit: grimoire is not on PATH"), ("post-commit", "post-commit: grimoire is not on PATH")] {
        let script = p.repo.join(".git").join("hooks").join(hook);
        assert!(script.is_file(), "init wrote {hook}");
        let ran = Command::new(&sh).arg(&script).current_dir(&p.repo).env("PATH", &empty).output().unwrap();
        assert!(ran.status.success(), "{hook} exits 0: {}", text(&ran));
        assert!(String::from_utf8_lossy(&ran.stderr).contains(warning), "{hook} warns: {}", text(&ran));
    }
}

/// # Test contract
///
/// ```json
/// {"test": "a-daemon-started-in-a-worktree-adds-it-to-the-store", "checks": ["crates/server/src#adds-its-checkout"]}
/// ```
#[test]
fn a_daemon_started_in_a_worktree_adds_it_to_the_store() {
    let p = bare_repo();
    assert!(run(&p.repo, &["init", "--store", "solo-three", "--name", "solo-three"]).status.success());
    let store = test_home().join("stores").join("solo-three");
    let at = |r: &Path| grimoire_server::store_io::repository_string(r).unwrap();
    let listed = || -> serde_json::Value { serde_json::from_str(&std::fs::read_to_string(store.join("checkouts.json")).unwrap_or_else(|_| "{}".into())).unwrap() };
    let wt = worktree_of(&p, "wt");
    assert!(listed()["checkouts"].get(at(&wt)).is_none(), "the worktree is not a checkout of the store yet");
    // Started in the store's main checkout, it adds nothing.
    {
        let _main = Serving(grimoire().args(["serve", "--port", "0"]).current_dir(&p.repo).spawn().unwrap());
        marked_port(&store);
        assert!(listed()["checkouts"].get(at(&p.repo)).is_none() && listed()["checkouts"].get(at(&wt)).is_none(), "{}", listed());
    }
    // A killed daemon leaves its marker, which is not the next one's.
    let _ = std::fs::remove_file(store.join("serve.json"));
    // Started in the worktree, it adds it before it serves, and a second start finds it there already.
    for _ in 0..2 {
        let _daemon = Serving(grimoire().args(["serve", "--port", "0"]).current_dir(&wt).spawn().unwrap());
        let marker = store.join("serve.json");
        eventually("the daemon marks the store", || marker.is_file());
        assert!(listed()["checkouts"].get(at(&wt)).is_some(), "{}", listed());
        assert_eq!(listed()["checkouts"].as_object().map(|o| o.len()), Some(1), "added once: {}", listed());
        drop(_daemon);
        let _ = std::fs::remove_file(&marker);
    }
}

/// Run git in `dir` the way a person's commit does: hooks as the repository configures them, with the
/// binary on the path so a hook finds `grimoire`.
fn git_as_person(dir: &Path, args: &[&str]) -> Output {
    let path = std::env::join_paths(Path::new(BIN).parent().map(Path::to_path_buf).into_iter().chain(std::env::split_paths(&std::env::var_os("PATH").unwrap_or_default()))).unwrap();
    Command::new("git")
        .arg("-C")
        .arg(dir)
        .args(["-c", "user.name=t", "-c", "user.email=t@t", "-c", "commit.gpgsign=false"])
        .args(args)
        .env("PATH", path)
        .env("GRIMOIRE_HOME", test_home())
        .output()
        .unwrap()
}

/// A git repository of the uninitialised project, with its code committed.
fn committed() -> Project {
    let p = uninitialised();
    git(&p.repo, &["init", "-q", "-b", "main"]);
    git(&p.repo, &["add", "-A"]);
    git(&p.repo, &["commit", "-q", "-m", "start"]);
    p
}

/// # Test contract
///
/// ```json
/// {"test": "hooks-go-where-core-hooks-path-says", "checks": ["crates/cli/src/init#hooks-where-git-runs-them", "crates/cli/src/init#existing-hook-left-alone", "req:init-hooks-where-git-runs-them"]}
/// ```
#[test]
fn hooks_go_where_core_hooks_path_says() {
    let p = committed();
    git(&p.repo, &["config", "core.hooksPath", ".husky"]);
    let init = run(&p.repo, &["init", "--store", &p.store.to_string_lossy()]);
    assert!(init.status.success() && !text(&init).contains("not a git repository"), "{}", text(&init));
    assert!(p.repo.join(".husky/pre-commit").is_file() && p.repo.join(".husky/post-commit").is_file(), "the hooks are where core.hooksPath names");
    assert!(!p.repo.join(".git/hooks/pre-commit").exists() && !p.repo.join(".git/hooks/post-commit").exists(), "none in .git/hooks");
    // Git runs them: the check passes on the adopted state, and a commit that breaks a claim is refused.
    let add = git_as_person(&p.repo, &["add", "-A"]);
    assert!(add.status.success(), "{}", text(&add));
    let first = git_as_person(&p.repo, &["commit", "-q", "-m", "init"]);
    assert!(first.status.success(), "{}", text(&first));
    assert!(text(&first).contains("no ticket was simmering"), "the post-commit hook ran: {}", text(&first));
    std::fs::write(p.repo.join("src").join("b.rs"), "pub fn b() {}\n").unwrap();
    std::fs::write(p.repo.join("src").join("lib.rs"), "pub mod a;\npub mod b;\n").unwrap();
    git(&p.repo, &["add", "-A"]);
    let refused = git_as_person(&p.repo, &["commit", "-q", "-m", "breaks a claim"]);
    assert!(!refused.status.success() && text(&refused).contains("module:src/b"), "the pre-commit check ran and refused: {}", text(&refused));

    // A pre-commit hook of the project's own there is left as it was, with a note; uninstall removes only init's.
    let own = uninitialised();
    std::fs::create_dir_all(own.repo.join(".husky")).unwrap();
    std::fs::write(own.repo.join(".husky/pre-commit"), "#!/bin/sh\nnpx lint-staged\n").unwrap();
    git(&own.repo, &["init", "-q", "-b", "main"]);
    git(&own.repo, &["config", "core.hooksPath", ".husky"]);
    let init = run(&own.repo, &["init", "--store", &own.store.to_string_lossy()]);
    let out = text(&init);
    assert!(init.status.success() && out.lines().any(|l| l.starts_with("skipped ") && l.contains("pre-commit")), "{out}");
    assert_eq!(std::fs::read_to_string(own.repo.join(".husky/pre-commit")).unwrap(), "#!/bin/sh\nnpx lint-staged\n");
    assert!(own.repo.join(".husky/post-commit").is_file());
    let un = run(&own.repo, &["uninstall"]);
    assert!(un.status.success(), "{}", text(&un));
    assert!(!own.repo.join(".husky/post-commit").exists(), "init's hook is taken out of .husky");
    assert_eq!(std::fs::read_to_string(own.repo.join(".husky/pre-commit")).unwrap(), "#!/bin/sh\nnpx lint-staged\n", "the project's own stays");
}

/// # Test contract
///
/// ```json
/// {"test": "uninstall-takes-the-hooks-out-of-hooks-path", "checks": ["crates/cli/src/init/uninstall#hooks-removed-where-git-runs-them", "req:init-hooks-where-git-runs-them"]}
/// ```
#[test]
fn uninstall_takes_the_hooks_out_of_hooks_path() {
    let p = committed();
    git(&p.repo, &["config", "core.hooksPath", ".husky"]);
    assert!(run(&p.repo, &["init", "--store", &p.store.to_string_lossy()]).status.success());
    assert!(p.repo.join(".husky/pre-commit").is_file());
    let un = run(&p.repo, &["uninstall"]);
    assert!(un.status.success(), "{}", text(&un));
    assert!(!p.repo.join(".husky/pre-commit").exists() && !p.repo.join(".husky/post-commit").exists(), "{}", text(&un));
    // A plain repository, where git answers `.git/hooks` with a `/` the verbatim path would take for part of a name.
    let q = committed();
    assert!(run(&q.repo, &["init", "--store", &q.store.to_string_lossy()]).status.success());
    assert!(q.repo.join(".git/hooks/pre-commit").is_file() && q.repo.join(".git/hooks/post-commit").is_file());
    let canonical = q.repo.canonicalize().unwrap();
    let un = run(&q.dir, &["uninstall", "--repo", &canonical.to_string_lossy()]);
    assert!(un.status.success(), "{}", text(&un));
    assert!(!q.repo.join(".git/hooks/pre-commit").exists() && !q.repo.join(".git/hooks/post-commit").exists(), "{}", text(&un));
}

/// # Test contract
///
/// ```json
/// {"test": "a-worktree-gets-the-shared-hooks", "checks": ["crates/cli/src/init#hooks-where-git-runs-them", "req:init-hooks-where-git-runs-them"]}
/// ```
#[test]
fn a_worktree_gets_the_shared_hooks() {
    let p = committed();
    let wt = p.dir.join("wt");
    git(&p.repo, &["worktree", "add", "-q", &wt.to_string_lossy(), "-b", "wt"]);
    assert!(wt.join(".git").is_file(), "a worktree's .git is a file");
    let init = run(&wt, &["init", "--store", &p.store.to_string_lossy()]);
    assert!(init.status.success() && !text(&init).contains("not a git repository"), "{}", text(&init));
    assert!(p.repo.join(".git/hooks/pre-commit").is_file() && p.repo.join(".git/hooks/post-commit").is_file(), "the hooks are in the shared hooks folder");
}

/// # Test contract
///
/// ```json
/// {"test": "a-submodule-gets-its-own-hooks", "checks": ["crates/cli/src/init#hooks-where-git-runs-them", "req:init-hooks-where-git-runs-them"]}
/// ```
#[test]
fn a_submodule_gets_its_own_hooks() {
    let p = committed();
    let outer = uninitialised();
    git(&outer.repo, &["init", "-q", "-b", "main"]);
    git(&outer.repo, &["add", "-A"]);
    git(&outer.repo, &["commit", "-q", "-m", "start"]);
    git(&outer.repo, &["-c", "protocol.file.allow=always", "submodule", "add", "-q", &p.repo.to_string_lossy().replace(std::path::MAIN_SEPARATOR, "/"), "sub"]);
    let sub = outer.repo.join("sub");
    assert!(sub.join(".git").is_file(), "a submodule's .git is a file");
    let init = run(&sub, &["init", "--store", &outer.store.to_string_lossy()]);
    assert!(init.status.success() && !text(&init).contains("not a git repository"), "{}", text(&init));
    assert!(outer.repo.join(".git/modules/sub/hooks/pre-commit").is_file() && outer.repo.join(".git/modules/sub/hooks/post-commit").is_file(), "the submodule's own hooks folder");
    assert!(!outer.repo.join(".git/hooks/pre-commit").exists(), "not the parent's");
}

/// # Test contract
///
/// ```json
/// {"test": "uninstall-with-a-canonicalised-repo-finds-the-hooks", "checks": ["crates/cli/src/init/uninstall#hooks-removed-where-git-runs-them", "req:init-hooks-where-git-runs-them"]}
/// ```
#[test]
fn uninstall_with_a_canonicalised_repo_finds_the_hooks() {
    let p = committed();
    git(&p.repo, &["config", "core.hooksPath", ".husky"]);
    assert!(run(&p.repo, &["init", "--store", &p.store.to_string_lossy()]).status.success());
    assert!(p.repo.join(".husky/pre-commit").is_file() && p.repo.join(".husky/post-commit").is_file());
    // On Windows the canonical path is the verbatim \\?\C:\..., which git cannot change into.
    let canonical = p.repo.canonicalize().unwrap();
    let un = run(&p.dir, &["uninstall", "--repo", &canonical.to_string_lossy()]);
    assert!(un.status.success(), "{}", text(&un));
    assert!(!p.repo.join(".husky/pre-commit").exists() && !p.repo.join(".husky/post-commit").exists(), "{}", text(&un));
}

/// # Test contract
///
/// ```json
/// {"test": "a-folder-within-a-repository-gets-no-hooks", "checks": ["crates/cli/src/init#hooks-where-git-runs-them", "crates/cli/src/init/uninstall#hooks-removed-where-git-runs-them", "req:init-hooks-where-git-runs-them"]}
/// ```
#[test]
fn a_folder_within_a_repository_gets_no_hooks() {
    let top = committed();
    let pkg = top.repo.join("pkg");
    std::fs::create_dir_all(pkg.join("src")).unwrap();
    std::fs::write(
        pkg.join("Cargo.toml"),
        "[package]
name = \"p\"
version = \"0.1.0\"
",
    )
    .unwrap();
    std::fs::write(
        pkg.join("src").join("lib.rs"),
        "pub fn p() {}
",
    )
    .unwrap();
    let pkg_store = top.dir.join("pkg-store");
    let init = run(&pkg, &["init", "--store", &pkg_store.to_string_lossy()]);
    let out = text(&init);
    assert!(init.status.success(), "{out}");
    for hook in ["pre-commit", "post-commit"] {
        assert!(out.lines().any(|l| l.starts_with("skipped ") && l.contains(hook) && l.contains("is inside the git repository at") && l.contains("grimoire check")), "{hook} not said: {out}");
        assert!(!top.repo.join(".git/hooks").join(hook).exists(), "{hook} written in the enclosing repository");
        assert!(!pkg.join(".git").exists());
    }
    // The enclosing repository's own hooks are not touched by uninstalling the folder within it.
    assert!(run(&top.repo, &["init", "--store", &top.store.to_string_lossy()]).status.success());
    assert!(top.repo.join(".git/hooks/pre-commit").is_file() && top.repo.join(".git/hooks/post-commit").is_file());
    let un = run(&top.dir, &["uninstall", "--repo", &pkg.to_string_lossy()]);
    assert!(un.status.success(), "{}", text(&un));
    assert!(top.repo.join(".git/hooks/pre-commit").is_file() && top.repo.join(".git/hooks/post-commit").is_file(), "{}", text(&un));
}
