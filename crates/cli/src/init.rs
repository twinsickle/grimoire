//! # Module contract
//!
//! ```json
//! {"purpose":"Install Grimoire's integration into a repository, and remove it again: the project file naming the store, the agent skill with its references, the always-loaded guidance section, the MCP entry pointing at the daemon, and a pre-commit hook running the check.","criteria":[{"id":"skill-sequences","statement":"The embedded skill holds a sequence to start, specify, finish and file a ticket, to resolve a gap or question with the person, and to adopt existing code and its tests, each under its own heading."},{"id":"listed-writes","statement":"Every path init writes or would have written, and every path uninstall removes, changes or leaves, is returned, so each command can print exactly what it did."},{"id":"merge-not-clobber","statement":"An existing .mcp.json keeps its other servers and an existing pre-commit hook is left alone with a note; only Grimoire's own files are overwritten."},{"id":"skill-is-the-source","statement":"The installed skill and its reference files are the ones in this repository's .claude/skills, embedded at build time, so there is one copy to maintain."},{"id":"guidance-section","statement":"A marked section between <!-- grimoire --> and <!-- /grimoire --> is written into CLAUDE.md and AGENTS.md, replacing a previous section and leaving everything outside the markers as it was, so an agent that never triggers the skill still learns the project is managed."},{"id":"project-agnostic","statement":"The embedded skill, references and guidance section name no project: a test fails the build if any of them mentions Grimoire's own crates, web package, npm scripts or dev loop, since they are installed verbatim into every repository."},{"id":"uninstall","statement":"uninstall plans every path of the integration before changing any, then removes the skill's files and its folders only when left empty; Grimoire's section from CLAUDE.md and AGENTS.md only when its marker lines form exactly one pair, cut out byte for byte so line endings, a byte-order mark and the person's text survive, the file removed only when nothing else is left; the grimoire entry from .mcp.json with the other servers and keys kept; and the pre-commit hook only when it is init's script unedited. Every path gets one result: removed, changed, left with why, or absent; a failure part way returns what was already done."}]}
//! ```

use std::path::{Path, PathBuf};

use serde_json::{json, Value};

/// The Claude Code skill and its references, embedded from this repository
/// so `init` installs exactly what Grimoire's own development uses.
pub const SKILL_MD: &str = include_str!("../../../.claude/skills/grimoire/SKILL.md");
pub const CONTRACTS_MD: &str = include_str!("../../../.claude/skills/grimoire/references/contracts.md");
pub const CHANGES_MD: &str = include_str!("../../../.claude/skills/grimoire/references/changes.md");
/// The section every session reads, for CLAUDE.md and AGENTS.md.
pub const GUIDANCE_MD: &str = include_str!("../../../.claude/skills/grimoire/references/guidance-section.md");

pub const SKILL_REL: &str = ".claude/skills/grimoire/SKILL.md";
pub const CONTRACTS_REL: &str = ".claude/skills/grimoire/references/contracts.md";
pub const CHANGES_REL: &str = ".claude/skills/grimoire/references/changes.md";
pub const MCP_REL: &str = ".mcp.json";
pub const HOOK_REL: &str = ".git/hooks/pre-commit";
pub const GUIDANCE_FILES: [&str; 2] = ["CLAUDE.md", "AGENTS.md"];
pub const MARK_START: &str = "<!-- grimoire -->";
pub const MARK_END: &str = "<!-- /grimoire -->";

/// What happened to one path.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Wrote {
    Written(PathBuf),
    /// Already present and not Grimoire's to overwrite; `note` says what to add by hand.
    Skipped { path: PathBuf, note: String },
}

/// What uninstall did, or will do, to one path of the integration.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Undone {
    /// Grimoire's, and gone.
    Removed(PathBuf),
    /// A person's file with Grimoire's part taken out of it.
    Changed(PathBuf),
    /// Left as it was; `note` says why, and what to take out by hand if anything.
    Left { path: PathBuf, note: String },
    /// Not there to begin with.
    Absent(PathBuf),
}

/// One step of an uninstall, worked out before anything is changed.
#[derive(Debug, Clone, PartialEq, Eq)]
enum Op {
    Remove,
    Write(String),
}

/// An uninstall planned in full: every path of the integration with what
/// will happen to it. Planning only reads, so a file that cannot be read
/// becomes a path left with its reason, and nothing has changed yet.
#[derive(Debug, Clone)]
pub struct Plan {
    steps: Vec<(Undone, Option<Op>)>,
}

/// A step that failed part way: what was already done, and where it stopped.
#[derive(Debug)]
pub struct Stopped {
    pub done: Vec<Undone>,
    pub error: InitError,
}

#[derive(Debug, thiserror::Error)]
pub enum InitError {
    #[error("{0}: {1}")]
    Io(PathBuf, std::io::Error),
    #[error("{0} is not valid JSON: {1}")]
    BadJson(PathBuf, String),
}

fn write(path: &Path, text: &str) -> Result<(), InitError> {
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir).map_err(|e| InitError::Io(dir.to_path_buf(), e))?;
    }
    std::fs::write(path, text).map_err(|e| InitError::Io(path.to_path_buf(), e))
}

/// `existing` with Grimoire's marked section replaced, or appended when
/// absent. Everything outside the markers is untouched.
pub fn with_guidance(existing: &str) -> String {
    let section = GUIDANCE_MD.trim_end();
    if let (Some(s), Some(e)) = (existing.find(MARK_START), existing.find(MARK_END)) {
        if e > s {
            let after = &existing[e + MARK_END.len()..];
            return format!("{}{}{}", &existing[..s], section, after);
        }
    }
    if existing.trim().is_empty() {
        format!("{section}\n")
    } else {
        format!("{}\n\n{section}\n", existing.trim_end())
    }
}

/// `existing` with Grimoire's marked section taken out, byte for byte: the
/// markers must be whole lines forming exactly one pair, and everything
/// outside them (line endings, a byte-order mark, the person's text) is kept,
/// less the one blank line `with_guidance` put before the section.
/// `Err` says why the section cannot be told apart safely; `Ok(None)` means
/// there is none.
pub fn without_guidance(existing: &str) -> Result<Option<String>, String> {
    // Every line with the byte range it spans, its terminator included.
    let mut lines = Vec::new();
    let mut at = 0;
    for line in existing.split_inclusive('\n') {
        lines.push((at, at + line.len(), line.trim_end_matches(['\n', '\r']).trim_start_matches('\u{feff}')));
        at += line.len();
    }
    let starts: Vec<_> = lines.iter().filter(|l| l.2 == MARK_START).collect();
    let ends: Vec<_> = lines.iter().filter(|l| l.2 == MARK_END).collect();
    match (starts.len(), ends.len()) {
        (0, 0) => return Ok(None),
        (1, 1) if starts[0].0 < ends[0].0 => {}
        _ => return Err(format!("its {MARK_START} and {MARK_END} lines do not form exactly one pair; take out Grimoire's section by hand")),
    }
    let (before, after) = (&existing[..starts[0].0], &existing[ends[0].1..]);
    let eol = if existing.contains("\r\n") { "\r\n" } else { "\n" };
    let blank = format!("{eol}{eol}");
    let rest = if after.trim().is_empty() {
        // The section was last: drop the blank line init put before it.
        before.strip_suffix(eol).filter(|_| before.ends_with(&blank)).unwrap_or(before).to_string()
    } else if before.ends_with(&blank) || before.is_empty() {
        // Between a person's text: keep one blank line between the two parts.
        format!("{before}{}", after.strip_prefix(eol).unwrap_or(after))
    } else {
        format!("{before}{after}")
    };
    Ok(Some(rest))
}

/// The marker line init's hook carries, which tells it from a person's.
const HOOK_MARK: &str = "Installed by `grimoire init`";

/// The hook script: fails the commit when the grimoire is inconsistent.
pub fn hook_script() -> String {
    "#!/bin/sh\n# Installed by `grimoire init`. Fails the commit when the grimoire and the code disagree.\nif ! command -v grimoire >/dev/null 2>&1; then\n  echo \"pre-commit: grimoire is not on PATH; the grimoire was not checked\" >&2\n  exit 0\nfi\nexec grimoire check\n".to_string()
}

/// Write the integration into `repo`. `.grimoire.toml` is written by the
/// caller (store_io) since it is the store's concern; this writes the rest.
pub fn install(repo: &Path, port: u16) -> Result<Vec<Wrote>, InitError> {
    let mut out = Vec::new();

    // The skill and its references: always Grimoire's, always overwritten.
    for (rel, text) in [(SKILL_REL, SKILL_MD), (CONTRACTS_REL, CONTRACTS_MD), (CHANGES_REL, CHANGES_MD)] {
        let path = repo.join(rel);
        write(&path, text)?;
        out.push(Wrote::Written(path));
    }

    // The guidance section every session reads: replace ours, keep theirs.
    for name in GUIDANCE_FILES {
        let path = repo.join(name);
        let existing = if path.is_file() { std::fs::read_to_string(&path).map_err(|e| InitError::Io(path.clone(), e))? } else { String::new() };
        write(&path, &with_guidance(&existing))?;
        out.push(Wrote::Written(path));
    }

    // .mcp.json: merge our server in, keep everything else.
    let mcp = repo.join(MCP_REL);
    let mut doc: Value = if mcp.is_file() {
        let text = std::fs::read_to_string(&mcp).map_err(|e| InitError::Io(mcp.clone(), e))?;
        serde_json::from_str(&text).map_err(|e| InitError::BadJson(mcp.clone(), e.to_string()))?
    } else {
        json!({})
    };
    let Some(root) = doc.as_object_mut() else {
        return Err(InitError::BadJson(mcp, "top level is not an object".into()));
    };
    let Some(servers) = root.entry("mcpServers").or_insert_with(|| json!({})).as_object_mut() else {
        return Err(InitError::BadJson(mcp, "mcpServers is not an object".into()));
    };
    servers.insert("grimoire".into(), json!({ "type": "http", "url": format!("http://127.0.0.1:{port}/mcp") }));
    let mut text = serde_json::to_string_pretty(&doc).expect("serialisable");
    text.push('\n');
    write(&mcp, &text)?;
    out.push(Wrote::Written(mcp));

    // Pre-commit hook: only when the repository is a git checkout and has none.
    let git = repo.join(".git");
    if git.is_dir() {
        let hook = repo.join(HOOK_REL);
        if hook.exists() {
            let existing = std::fs::read_to_string(&hook).unwrap_or_default();
            if existing.contains(HOOK_MARK) {
                // Ours: rewrite, so a changed script reaches the repository.
                write(&hook, &hook_script())?;
                out.push(Wrote::Written(hook));
            } else if existing.contains("grimoire check") {
                out.push(Wrote::Skipped { path: hook, note: "the existing pre-commit hook already runs `grimoire check`".into() });
            } else {
                out.push(Wrote::Skipped { path: hook, note: "a pre-commit hook already exists; add `grimoire check` to it".into() });
            }
        } else {
            write(&hook, &hook_script())?;
            #[cfg(unix)]
            {
                use std::os::unix::fs::PermissionsExt;
                let _ = std::fs::set_permissions(&hook, std::fs::Permissions::from_mode(0o755));
            }
            out.push(Wrote::Written(hook));
        }
    } else {
        out.push(Wrote::Skipped { path: repo.join(HOOK_REL), note: "not a git repository; run `grimoire check` from your own hook or CI".into() });
    }

    Ok(out)
}

fn io(path: &Path) -> impl FnOnce(std::io::Error) -> InitError + '_ {
    move |e| InitError::Io(path.to_path_buf(), e)
}

/// The skill's folders, deepest first: removed only when left empty.
const SKILL_DIRS: [&str; 4] = [".claude/skills/grimoire/references", ".claude/skills/grimoire", ".claude/skills", ".claude"];

/// Work out what uninstall will do to every path of the integration in
/// `repo`, reading only. `.grimoire.toml` is the caller's, as it is at install.
pub fn plan_uninstall(repo: &Path) -> Plan {
    let mut steps: Vec<(Undone, Option<Op>)> = Vec::new();
    let left = |path: PathBuf, note: String| (Undone::Left { path, note }, None);

    // The skill's files: always Grimoire's, whatever they hold.
    for rel in [SKILL_REL, CONTRACTS_REL, CHANGES_REL] {
        let path = repo.join(rel);
        steps.push(if path.is_file() { (Undone::Removed(path), Some(Op::Remove)) } else { (Undone::Absent(path), None) });
    }

    // The guidance section: taken out byte for byte, the file only when nothing else is left.
    for name in GUIDANCE_FILES {
        let path = repo.join(name);
        if !path.is_file() {
            steps.push((Undone::Absent(path), None));
            continue;
        }
        let step = match std::fs::read_to_string(&path) {
            Err(e) => left(path, format!("could not be read ({e}); take out Grimoire's section by hand")),
            Ok(existing) => match without_guidance(&existing) {
                Ok(None) => left(path, "holds no Grimoire section".into()),
                Ok(Some(rest)) if rest.trim_start_matches('\u{feff}').trim().is_empty() => (Undone::Removed(path), Some(Op::Remove)),
                Ok(Some(rest)) => (Undone::Changed(path), Some(Op::Write(rest))),
                Err(why) => left(path, why),
            },
        };
        steps.push(step);
    }

    // .mcp.json: our entry out, other servers and keys kept; the file only when nothing else is in it.
    let mcp = repo.join(MCP_REL);
    steps.push(if !mcp.is_file() {
        (Undone::Absent(mcp), None)
    } else {
        match std::fs::read_to_string(&mcp).map_err(|e| e.to_string()).and_then(|t| serde_json::from_str::<Value>(&t).map_err(|e| e.to_string())) {
            Err(e) => left(mcp, format!("could not be read as JSON ({e}); take out the grimoire server by hand")),
            Ok(mut doc) => {
                if doc.get_mut("mcpServers").and_then(Value::as_object_mut).and_then(|s| s.remove("grimoire")).is_none() {
                    left(mcp, "has no grimoire server".into())
                } else {
                    if let Some(root) = doc.as_object_mut() {
                        if root.get("mcpServers").and_then(Value::as_object).is_some_and(|s| s.is_empty()) {
                            root.remove("mcpServers");
                        }
                    }
                    if doc.as_object().is_some_and(|r| r.is_empty()) {
                        (Undone::Removed(mcp), Some(Op::Remove))
                    } else {
                        let mut text = serde_json::to_string_pretty(&doc).expect("serialisable");
                        text.push('\n');
                        (Undone::Changed(mcp), Some(Op::Write(text)))
                    }
                }
            }
        }
    });

    // The hook: only init's own script, unedited.
    let hook = repo.join(HOOK_REL);
    steps.push(if !hook.is_file() {
        (Undone::Absent(hook), None)
    } else {
        let existing = std::fs::read_to_string(&hook).unwrap_or_default();
        if existing.replace("\r\n", "\n") == hook_script() {
            (Undone::Removed(hook), Some(Op::Remove))
        } else if existing.contains("grimoire check") {
            left(hook, "not init's script as installed (a hook of your own, or init's edited); take out the `grimoire check` line".into())
        } else {
            left(hook, "a hook of your own; Grimoire has nothing in it".into())
        }
    });
    Plan { steps }
}

impl Plan {
    /// What the plan will do, one line per path, before anything is done.
    #[cfg(test)]
    pub fn report(&self) -> Vec<Undone> {
        self.steps.iter().map(|(u, _)| u.clone()).collect()
    }

    /// Carry the plan out, then remove the skill's folders that are left empty.
    /// A failure stops at once and says what was already done.
    pub fn apply(self, repo: &Path) -> Result<Vec<Undone>, Stopped> {
        let mut done = Vec::new();
        for (undone, op) in self.steps {
            let path = match &undone {
                Undone::Removed(p) | Undone::Changed(p) => p.clone(),
                Undone::Left { .. } | Undone::Absent(_) => {
                    done.push(undone);
                    continue;
                }
            };
            let result = match op {
                Some(Op::Remove) => std::fs::remove_file(&path).map_err(io(&path)),
                Some(Op::Write(text)) => write(&path, &text),
                None => Ok(()),
            };
            if let Err(error) = result {
                return Err(Stopped { done, error });
            }
            done.push(undone);
        }
        for rel in SKILL_DIRS {
            let dir = repo.join(rel);
            let empty = dir.is_dir() && std::fs::read_dir(&dir).map(|mut d| d.next().is_none()).unwrap_or(false);
            if empty {
                if let Err(e) = std::fs::remove_dir(&dir) {
                    return Err(Stopped { done, error: InitError::Io(dir, e) });
                }
                done.push(Undone::Removed(dir));
            }
        }
        Ok(done)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp() -> PathBuf {
        let nanos = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos();
        let d = std::env::temp_dir().join(format!("grimoire-init-{}-{nanos}", std::process::id()));
        std::fs::create_dir_all(&d).unwrap();
        d
    }

    /// # Test contract
    ///
    /// ```json
    /// {"test": "installs-and-merges", "checks": ["crates/cli/src/init#listed-writes", "crates/cli/src/init#merge-not-clobber"]}
    /// ```
    #[test]
    fn installs_and_merges() {
        let repo = temp();
        std::fs::create_dir_all(repo.join(".git").join("hooks")).unwrap();
        std::fs::write(repo.join(".mcp.json"), r#"{"mcpServers":{"other":{"type":"stdio","command":"x"}}}"#).unwrap();

        std::fs::write(repo.join("CLAUDE.md"), "# My project\n\nUse tabs.\n").unwrap();

        let wrote = install(&repo, 4343).unwrap();
        assert_eq!(wrote.len(), 7);
        assert!(wrote.iter().all(|w| matches!(w, Wrote::Written(_))));

        let skill = std::fs::read_to_string(repo.join(SKILL_REL)).unwrap();
        assert!(skill.starts_with("---\nname: grimoire"));
        assert!(std::fs::read_to_string(repo.join(CONTRACTS_REL)).unwrap().contains("# Module contracts"));
        assert!(std::fs::read_to_string(repo.join(CHANGES_REL)).unwrap().contains("| Change in the code |"));

        // Guidance appended to the existing CLAUDE.md, created for AGENTS.md.
        let claude = std::fs::read_to_string(repo.join("CLAUDE.md")).unwrap();
        assert!(claude.starts_with("# My project\n\nUse tabs.\n\n<!-- grimoire -->"));
        assert!(claude.trim_end().ends_with(MARK_END));
        let agents = std::fs::read_to_string(repo.join("AGENTS.md")).unwrap();
        assert!(agents.starts_with(MARK_START));
        assert_eq!(agents.matches(MARK_START).count(), 1);

        let mcp: Value = serde_json::from_str(&std::fs::read_to_string(repo.join(MCP_REL)).unwrap()).unwrap();
        assert_eq!(mcp["mcpServers"]["other"]["command"], "x");
        assert_eq!(mcp["mcpServers"]["grimoire"]["url"], "http://127.0.0.1:4343/mcp");

        let hook = std::fs::read_to_string(repo.join(HOOK_REL)).unwrap();
        assert!(hook.contains("exec grimoire check"));

        // Second run: idempotent, hook recognised as ours, one section not two.
        let again = install(&repo, 4343).unwrap();
        assert!(again.iter().all(|w| matches!(w, Wrote::Written(_))));
        let claude = std::fs::read_to_string(repo.join("CLAUDE.md")).unwrap();
        assert_eq!(claude.matches(MARK_START).count(), 1);
        assert!(claude.starts_with("# My project\n\nUse tabs.\n"));
        std::fs::remove_dir_all(&repo).unwrap();
    }

    /// # Test contract
    ///
    /// ```json
    /// {"test": "guidance-replaces-only-its-own-section", "checks": ["crates/cli/src/init#guidance-section"]}
    /// ```
    #[test]
    fn guidance_replaces_only_its_own_section() {
        let before = "intro\n\n<!-- grimoire -->\nold stuff\n<!-- /grimoire -->\n\nafter\n";
        let after = with_guidance(before);
        assert!(after.starts_with("intro\n\n<!-- grimoire -->"));
        assert!(after.ends_with("<!-- /grimoire -->\n\nafter\n"));
        assert!(!after.contains("old stuff"));
        assert!(after.contains("## Grimoire"));
        assert_eq!(with_guidance(""), format!("{}\n", GUIDANCE_MD.trim_end()));
    }

    /// Criterion project-agnostic: what init installs is the same for every
    /// repository, so nothing in it may be about this one.
    /// # Test contract
    ///
    /// ```json
    /// {"test": "installed-texts-name-no-project", "checks": ["crates/cli/src/init#project-agnostic"]}
    /// ```
    #[test]
    fn installed_texts_name_no_project() {
        let repo_only = [
            "bacon",
            "web/",
            "npm ",
            "npx ",
            "crates/",
            "Grimoire's own repository",
            "Tailwind",
            "Playwright",
            "cargo install",
        ];
        for (name, text) in [
            ("SKILL.md", SKILL_MD),
            ("contracts.md", CONTRACTS_MD),
            ("changes.md", CHANGES_MD),
            ("guidance-section.md", GUIDANCE_MD),
        ] {
            for token in repo_only {
                if let Some(at) = text.find(token) {
                    let line = text[..at].matches('\n').count() + 1;
                    panic!("{name}:{line} mentions {token:?}; the installed skill must not know about this repository");
                }
            }
        }
    }

    /// # Test contract
    ///
    /// ```json
    /// {"test": "skill-holds-every-sequence", "checks": ["crates/cli/src/init#skill-sequences"]}
    /// ```
    #[test]
    fn skill_holds_every_sequence() {
        for s in ["start a ticket", "write the spec", "finish a ticket", "file a ticket", "resolve a gap or question", "adopt existing code"] {
            assert!(SKILL_MD.contains(&format!("## Sequence: {s}")), "the skill has no `{s}` sequence");
        }
        let adopt = &SKILL_MD[SKILL_MD.find("## Sequence: adopt existing code").unwrap_or(0)..];
        assert!(adopt.contains("record_decision") && adopt.contains("Never run\n`scaffold`"), "adopt records provenance and never scaffolds");
        assert!(adopt.contains("### Tests") && adopt.contains("`untested`") && adopt.contains("req:<slug>"), "adopt covers tests and constraints");
        let resolve = &SKILL_MD[SKILL_MD.find("## Sequence: resolve a gap or question").unwrap_or(0)..];
        assert!(resolve.contains("do not resolve") && resolve.contains("relies_on"), "resolve leaves the decision to the person and files the change after it");
    }

    /// # Test contract
    ///
    /// ```json
    /// {"test": "the-skill-is-embedded-from-this-repository", "checks": ["crates/cli/src/init#skill-is-the-source"]}
    /// ```
    #[test]
    fn the_skill_is_embedded_from_this_repository() {
        let skill = concat!(env!("CARGO_MANIFEST_DIR"), "/../../.claude/skills/grimoire/");
        let on_disk = |rel: &str| std::fs::read_to_string(format!("{skill}{rel}")).expect("the skill is in the repository");
        assert_eq!(SKILL_MD, on_disk("SKILL.md"), "one copy to maintain");
        assert_eq!(CONTRACTS_MD, on_disk("references/contracts.md"));
        assert_eq!(CHANGES_MD, on_disk("references/changes.md"));
        assert_eq!(GUIDANCE_MD, on_disk("references/guidance-section.md"));
    }

    /// # Test contract
    ///
    /// ```json
    /// {"test": "leaves-foreign-hook-alone", "checks": ["crates/cli/src/init#merge-not-clobber"]}
    /// ```
    #[test]
    fn leaves_foreign_hook_alone() {
        let repo = temp();
        std::fs::create_dir_all(repo.join(".git").join("hooks")).unwrap();
        std::fs::write(repo.join(HOOK_REL), "#!/bin/sh\nnpm test\n").unwrap();
        let wrote = install(&repo, 4242).unwrap();
        assert!(wrote.iter().any(|w| matches!(w, Wrote::Skipped { .. })));
        assert_eq!(std::fs::read_to_string(repo.join(HOOK_REL)).unwrap(), "#!/bin/sh\nnpm test\n");
        std::fs::remove_dir_all(&repo).unwrap();
    }

    #[test]
    fn no_git_is_a_note_not_an_error() {
        let repo = temp();
        let wrote = install(&repo, 4242).unwrap();
        assert!(wrote.iter().any(|w| matches!(w, Wrote::Skipped { note, .. } if note.contains("not a git repository"))));
        std::fs::remove_dir_all(&repo).unwrap();
    }

    fn uninstall(repo: &Path) -> Vec<Undone> {
        plan_uninstall(repo).apply(repo).unwrap()
    }

    /// # Test contract
    ///
    /// ```json
    /// {"test": "uninstall-puts-the-repository-back", "checks": ["crates/cli/src/init#uninstall", "crates/cli/src/init#listed-writes"]}
    /// ```
    #[test]
    fn uninstall_puts_the_repository_back() {
        let repo = temp();
        std::fs::create_dir_all(repo.join(".git").join("hooks")).unwrap();
        // Compact, in its own key order, with a key of its own: kept as data, not as layout.
        std::fs::write(repo.join(".mcp.json"), r#"{"mcpServers":{"other":{"type":"stdio","command":"x"}},"z":1}"#).unwrap();
        std::fs::write(repo.join("CLAUDE.md"), "# My project\n\nUse tabs.\n").unwrap();
        std::fs::create_dir_all(repo.join(".claude").join("skills").join("mine")).unwrap();
        std::fs::write(repo.join(".claude/skills/mine/SKILL.md"), "mine").unwrap();
        install(&repo, 4242).unwrap();
        let undone = uninstall(&repo);
        assert_eq!(std::fs::read_to_string(repo.join("CLAUDE.md")).unwrap(), "# My project\n\nUse tabs.\n", "a person's text is back exactly");
        assert!(!repo.join("AGENTS.md").exists(), "a file init created holds nothing else, so it goes");
        let mcp: Value = serde_json::from_str(&std::fs::read_to_string(repo.join(".mcp.json")).unwrap()).unwrap();
        assert_eq!(mcp, json!({ "mcpServers": { "other": { "type": "stdio", "command": "x" } }, "z": 1 }), "other servers and keys are kept");
        assert!(!repo.join(HOOK_REL).exists(), "init's own hook goes");
        assert!(!repo.join(".claude/skills/grimoire").exists());
        assert!(repo.join(".claude/skills/mine/SKILL.md").is_file(), "a person's skill and its folders stay");
        // One line for every path of the integration, and the emptied skill folders.
        let expected = vec![
            Undone::Removed(repo.join(SKILL_REL)),
            Undone::Removed(repo.join(CONTRACTS_REL)),
            Undone::Removed(repo.join(CHANGES_REL)),
            Undone::Changed(repo.join("CLAUDE.md")),
            Undone::Removed(repo.join("AGENTS.md")),
            Undone::Changed(repo.join(".mcp.json")),
            Undone::Removed(repo.join(HOOK_REL)),
            Undone::Removed(repo.join(".claude/skills/grimoire/references")),
            Undone::Removed(repo.join(".claude/skills/grimoire")),
        ];
        assert_eq!(undone, expected);
        // A second run: every path accounted for, nothing done.
        let again = uninstall(&repo);
        assert_eq!(again.len(), 7, "{again:?}");
        assert!(again.iter().all(|u| matches!(u, Undone::Left { .. } | Undone::Absent(_))), "{again:?}");
    }

    #[test]
    fn uninstall_leaves_a_hook_that_is_not_inits_as_installed() {
        for own in ["#!/bin/sh\nnpm test\ngrimoire check\n".to_string(), hook_script().replace("exec grimoire check", "npm test\nexec grimoire check")] {
            let repo = temp();
            std::fs::create_dir_all(repo.join(".git").join("hooks")).unwrap();
            std::fs::write(repo.join(HOOK_REL), &own).unwrap();
            let undone = uninstall(&repo);
            assert_eq!(std::fs::read_to_string(repo.join(HOOK_REL)).unwrap(), own, "an edited hook stays, even with init's marker in it");
            assert!(undone.iter().any(|u| matches!(u, Undone::Left { path, note } if path.ends_with("pre-commit") && note.contains("grimoire check"))), "{undone:?}");
        }
        // init's script, even with Windows line endings, is init's.
        let repo = temp();
        std::fs::create_dir_all(repo.join(".git").join("hooks")).unwrap();
        std::fs::write(repo.join(HOOK_REL), hook_script().replace('\n', "\r\n")).unwrap();
        uninstall(&repo);
        assert!(!repo.join(HOOK_REL).exists());
    }

    #[test]
    fn uninstall_empties_what_init_made_from_nothing() {
        let repo = temp();
        std::fs::create_dir_all(repo.join(".git").join("hooks")).unwrap();
        install(&repo, 4242).unwrap();
        uninstall(&repo);
        assert!(!repo.join(".mcp.json").exists());
        assert!(!repo.join("CLAUDE.md").exists());
        assert!(!repo.join(".claude").exists());
    }

    #[test]
    fn a_section_is_taken_out_byte_for_byte() {
        let section = GUIDANCE_MD.trim_end();
        // Between a person's text, and last.
        assert_eq!(without_guidance(&format!("# Top\n\n{section}\n\n## Below\n")).unwrap().as_deref(), Some("# Top\n\n## Below\n"));
        assert_eq!(without_guidance(&with_guidance("Keep me.\n")).unwrap().as_deref(), Some("Keep me.\n"));
        assert_eq!(without_guidance(&with_guidance("")).unwrap().as_deref(), Some(""));
        // Windows line endings and a byte-order mark survive.
        let crlf = format!("\u{feff}Keep\r\n\r\n{}\r\n", section.replace('\n', "\r\n"));
        assert_eq!(without_guidance(&crlf).unwrap().as_deref(), Some("\u{feff}Keep\r\n"));
        // No section, and markers that are not one clean pair: left alone.
        assert_eq!(without_guidance("no section here").unwrap(), None);
        assert!(without_guidance(&format!("{section}\n\n{section}\n")).is_err(), "two sections");
        assert!(without_guidance(&format!("{MARK_START}\nonly a start\n")).is_err());
        assert!(without_guidance(&format!("```\n{MARK_START}\n```\n\n{section}\n")).is_err(), "a quoted marker makes it ambiguous");
        // A marker mentioned inside a sentence is not a marker line.
        let mention = format!("Grimoire's section starts at {MARK_START} and ends at {MARK_END}.\n");
        assert_eq!(without_guidance(&mention).unwrap(), None);
    }

    #[test]
    fn nothing_changes_until_the_plan_is_applied() {
        let repo = temp();
        std::fs::create_dir_all(repo.join(".git").join("hooks")).unwrap();
        install(&repo, 4242).unwrap();
        let before: Vec<_> = walk(&repo);
        let plan = plan_uninstall(&repo);
        assert_eq!(walk(&repo), before, "planning only reads");
        assert!(plan.report().iter().any(|u| matches!(u, Undone::Removed(_))));
    }

    fn walk(dir: &Path) -> Vec<(PathBuf, Vec<u8>)> {
        let mut out = Vec::new();
        let mut stack = vec![dir.to_path_buf()];
        while let Some(d) = stack.pop() {
            for e in std::fs::read_dir(&d).unwrap().flatten() {
                if e.path().is_dir() {
                    stack.push(e.path());
                } else {
                    out.push((e.path(), std::fs::read(e.path()).unwrap()));
                }
            }
        }
        out.sort();
        out
    }

}
