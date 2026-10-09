//! # Module contract
//!
//! ```json
//! {"purpose":"Take Grimoire's integration back out of a repository: plan what happens to every path of it before changing any, then carry the plan out.","criteria":[{"id":"plans-before-changing","statement":"uninstall plans every path of the integration before changing any."},{"id":"skill-files-removed","statement":"uninstall removes both harnesses' skill files and their folders only when left empty."},{"id":"section-only-when-one-pair","statement":"uninstall removes Grimoire's section from CLAUDE.md and AGENTS.md only when its marker lines form exactly one pair."},{"id":"section-cut-out-byte-for-byte","statement":"uninstall cuts Grimoire's section out byte for byte, so line endings, a byte-order mark and the person's text survive."},{"id":"guidance-file-removed-when-empty","statement":"uninstall removes CLAUDE.md or AGENTS.md only when nothing else is left in it."},{"id":"mcp-json-entry-removed","statement":"uninstall removes the grimoire entry from .mcp.json with the other servers and keys kept."},{"id":"codex-server-only-as-init-wrote-it","statement":"uninstall removes the grimoire server from .codex/config.toml only when it is as init wrote it, last in the file, with every other byte kept, otherwise leaving it with a note."},{"id":"hooks-only-when-unedited","statement":"uninstall removes the pre-commit and post-commit hooks, each only when it is init's script unedited, as this release wrote it."},{"id":"hooks-removed-where-git-runs-them","statement":"uninstall removes init's hooks from the folder git runs hooks from for the checkout, by the same rule: a folder within a repository never touches that repository's hooks."},{"id":"earlier-release-post-commit-hook","statement":"uninstall removes the post-commit hook also when it is init's script unedited as the release before bottling was named wrote it."},{"id":"one-result-per-file","statement":"Every file of the integration gets one uninstall result, removed, changed, left with why, or absent."},{"id":"emptied-folders-reported","statement":"uninstall reports each folder init made when it is removed empty."},{"id":"failure-stops-and-reports-done","statement":"A failure part way through uninstall stops at once and returns what was already done."}]}
//! ```

use std::path::{Path, PathBuf};

use serde_json::Value;

use super::{former_hook_script, guidance::without_guidance, hooks, hooks_dir, io, without_codex_server, write, InitError, CODEX_REL, HARNESSES, MCP_REL, SKILL_FILES};

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

/// The folders init may have made, deepest first: each harness's skill
/// folder up to the harness's own, then Codex's config folder. Removed
/// only when left empty.
fn integration_dirs() -> Vec<PathBuf> {
    let mut dirs = Vec::new();
    for harness in &HARNESSES {
        let mut dir = Some(Path::new(harness.skill_dir));
        dirs.push(Path::new(harness.skill_dir).join("references"));
        while let Some(d) = dir.filter(|d| !d.as_os_str().is_empty()) {
            dirs.push(d.to_path_buf());
            dir = d.parent();
        }
    }
    dirs.extend(Path::new(CODEX_REL).parent().map(Path::to_path_buf));
    dirs
}

/// Work out what uninstall will do to every path of the integration in
/// `repo`, reading only. `.grimoire.toml` is the caller's, as it is at install.
pub fn plan_uninstall(repo: &Path) -> Plan {
    let mut steps: Vec<(Undone, Option<Op>)> = Vec::new();
    let left = |path: PathBuf, note: String| (Undone::Left { path, note }, None);

    // The skill's files, in every harness's place: always Grimoire's, whatever they hold.
    for harness in &HARNESSES {
        for (rel, _) in SKILL_FILES {
            let path = repo.join(format!("{}/{rel}", harness.skill_dir));
            steps.push(if path.is_file() { (Undone::Removed(path), Some(Op::Remove)) } else { (Undone::Absent(path), None) });
        }
    }

    // The guidance section: taken out byte for byte, the file only when nothing else is left.
    for harness in &HARNESSES {
        let path = repo.join(harness.guidance);
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

    // .codex/config.toml: our server out, the rest byte for byte; the file only when nothing else is in it.
    let codex = repo.join(CODEX_REL);
    steps.push(if !codex.is_file() {
        (Undone::Absent(codex), None)
    } else {
        match std::fs::read_to_string(&codex).map_err(|e| format!("could not be read ({e}); take out [mcp_servers.grimoire] by hand")).and_then(|t| without_codex_server(&t)) {
            Err(why) => left(codex, why),
            Ok(None) => left(codex, "has no grimoire server".into()),
            Ok(Some(rest)) if rest.trim_start_matches('\u{feff}').trim().is_empty() => (Undone::Removed(codex), Some(Op::Remove)),
            Ok(Some(rest)) => (Undone::Changed(codex), Some(Op::Write(rest))),
        }
    });

    // The hooks: each only when it is init's own script, unedited, as this release or an earlier one wrote it.
    // They are in the folder git runs hooks from; outside git there are none to find.
    let dir = hooks_dir(repo).unwrap_or_else(|| repo.join(".git").join("hooks"));
    for (rel, script, command) in hooks() {
        let hook = dir.join(rel);
        steps.push(if !hook.is_file() {
            (Undone::Absent(hook), None)
        } else {
            let existing = std::fs::read_to_string(&hook).unwrap_or_default();
            let existing_lf = existing.replace("\r\n", "\n");
            if existing_lf == script || former_hook_script(rel).is_some_and(|former| existing_lf == former) {
                (Undone::Removed(hook), Some(Op::Remove))
            } else if existing.contains(command) {
                left(hook, format!("not init's script as installed (a hook of your own, or init's edited); take out the `{command}` line"))
            } else {
                left(hook, "a hook of your own; Grimoire has nothing in it".into())
            }
        });
    }
    Plan { steps }
}

impl Plan {
    /// What the plan will do, one line per path, before anything is done.
    #[cfg(test)]
    pub fn report(&self) -> Vec<Undone> {
        self.steps.iter().map(|(u, _)| u.clone()).collect()
    }

    /// Carry the plan out, then remove the folders init made that are left empty.
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
        for rel in integration_dirs() {
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
    use crate::init::install;
    use crate::init::tests::{hook_at, temp, walk};
    use crate::init::{hook_script, BOTTLING_HOOK_FILE, HOOK_FILE};
    use serde_json::json;

    fn uninstall(repo: &Path) -> Vec<Undone> {
        plan_uninstall(repo).apply(repo).unwrap()
    }

    /// # Test contract
    ///
    /// ```json
    /// {"test": "uninstall-puts-the-repository-back", "checks": ["crates/cli/src/init/uninstall#skill-files-removed", "crates/cli/src/init/uninstall#section-cut-out-byte-for-byte", "crates/cli/src/init/uninstall#guidance-file-removed-when-empty", "crates/cli/src/init/uninstall#mcp-json-entry-removed", "crates/cli/src/init/uninstall#codex-server-only-as-init-wrote-it", "crates/cli/src/init/uninstall#hooks-only-when-unedited", "crates/cli/src/init/uninstall#one-result-per-file", "crates/cli/src/init/uninstall#emptied-folders-reported", "crates/cli/src/init#listed-writes"]}
    /// ```
    #[test]
    fn uninstall_puts_the_repository_back() {
        let tmp = temp();
        let repo = tmp.path();
        std::fs::create_dir_all(repo.join(".git").join("hooks")).unwrap();
        // Compact, in its own key order, with a key of its own: kept as data, not as layout.
        std::fs::write(repo.join(".mcp.json"), r#"{"mcpServers":{"other":{"type":"stdio","command":"x"}},"z":1}"#).unwrap();
        std::fs::write(repo.join("CLAUDE.md"), "# My project\n\nUse tabs.\n").unwrap();
        std::fs::create_dir_all(repo.join(".claude").join("skills").join("mine")).unwrap();
        std::fs::write(repo.join(".claude/skills/mine/SKILL.md"), "mine").unwrap();
        let codex_own = "# ours
model = \"o4\"
";
        std::fs::create_dir_all(repo.join(".codex")).unwrap();
        std::fs::write(repo.join(CODEX_REL), codex_own).unwrap();
        install(repo, 4242, "shop").unwrap();
        let undone = uninstall(repo);
        assert_eq!(std::fs::read_to_string(repo.join("CLAUDE.md")).unwrap(), "# My project\n\nUse tabs.\n", "a person's text is back exactly");
        assert!(!repo.join("AGENTS.md").exists(), "a file init created holds nothing else, so it goes");
        let mcp: Value = serde_json::from_str(&std::fs::read_to_string(repo.join(".mcp.json")).unwrap()).unwrap();
        assert_eq!(mcp, json!({ "mcpServers": { "other": { "type": "stdio", "command": "x" } }, "z": 1 }), "other servers and keys are kept");
        assert!(!hook_at(repo, HOOK_FILE).exists(), "init's own hook goes");
        assert!(!repo.join(".claude/skills/grimoire").exists());
        assert!(repo.join(".claude/skills/mine/SKILL.md").is_file(), "a person's skill and its folders stay");
        assert_eq!(std::fs::read_to_string(repo.join(CODEX_REL)).unwrap(), codex_own, "a person's Codex config is back exactly");
        assert!(!repo.join(".agents").exists(), "Codex's skill folders init made are gone");
        // One line for every path of the integration, and the emptied skill folders.
        let mut expected: Vec<Undone> = HARNESSES.iter().flat_map(|h| SKILL_FILES.map(|(rel, _)| Undone::Removed(repo.join(format!("{}/{rel}", h.skill_dir))))).collect();
        expected.extend([
            Undone::Changed(repo.join("CLAUDE.md")),
            Undone::Removed(repo.join("AGENTS.md")),
            Undone::Changed(repo.join(".mcp.json")),
            Undone::Changed(repo.join(CODEX_REL)),
            Undone::Removed(hook_at(repo, HOOK_FILE)),
            Undone::Removed(hook_at(repo, BOTTLING_HOOK_FILE)),
            Undone::Removed(repo.join(".claude/skills/grimoire/references")),
            Undone::Removed(repo.join(".claude/skills/grimoire")),
            Undone::Removed(repo.join(".agents/skills/grimoire/references")),
            Undone::Removed(repo.join(".agents/skills/grimoire")),
            Undone::Removed(repo.join(".agents/skills")),
            Undone::Removed(repo.join(".agents")),
        ]);
        assert_eq!(undone, expected);
        // A second run: every path accounted for, nothing done.
        let again = uninstall(repo);
        assert_eq!(again.len(), 12, "{again:?}");
        assert!(again.iter().all(|u| matches!(u, Undone::Left { .. } | Undone::Absent(_))), "{again:?}");
    }

    #[test]
    fn uninstall_leaves_a_hook_that_is_not_inits_as_installed() {
        for own in ["#!/bin/sh\nnpm test\ngrimoire check\n".to_string(), hook_script().replace("exec grimoire check", "npm test\nexec grimoire check")] {
            let tmp = temp();
            let repo = tmp.path();
            std::fs::create_dir_all(repo.join(".git").join("hooks")).unwrap();
            std::fs::write(hook_at(repo, HOOK_FILE), &own).unwrap();
            let undone = uninstall(repo);
            assert_eq!(std::fs::read_to_string(hook_at(repo, HOOK_FILE)).unwrap(), own, "an edited hook stays, even with init's marker in it");
            assert!(undone.iter().any(|u| matches!(u, Undone::Left { path, note } if path.ends_with("pre-commit") && note.contains("grimoire check"))), "{undone:?}");
        }
        // init's script, even with Windows line endings, is init's.
        let tmp = temp();
        let repo = tmp.path();
        std::fs::create_dir_all(repo.join(".git").join("hooks")).unwrap();
        std::fs::write(hook_at(repo, HOOK_FILE), hook_script().replace('\n', "\r\n")).unwrap();
        uninstall(repo);
        assert!(!hook_at(repo, HOOK_FILE).exists());
    }

    /// # Test contract
    ///
    /// ```json
    /// {"test": "an-edited-hook-is-kept-and-an-unedited-one-removed", "checks": ["crates/cli/src/init/uninstall#hooks-only-when-unedited"]}
    /// ```
    #[test]
    fn an_edited_hook_is_kept_and_an_unedited_one_removed() {
        let tmp = temp();
        let repo = tmp.path();
        std::fs::create_dir_all(repo.join(".git").join("hooks")).unwrap();
        install(repo, 4242, "shop").unwrap();
        // The check before a commit edited by a line; the bottling after it left as init wrote it.
        let edited = hook_script().replace("exec grimoire check", "echo before\nexec grimoire check");
        std::fs::write(hook_at(repo, HOOK_FILE), &edited).unwrap();
        let undone = uninstall(repo);
        assert_eq!(std::fs::read_to_string(hook_at(repo, HOOK_FILE)).unwrap(), edited, "the edited hook is kept, byte for byte");
        assert!(undone.iter().any(|u| matches!(u, Undone::Left { path, .. } if path.ends_with("pre-commit"))), "{undone:?}");
        assert!(undone.contains(&Undone::Removed(hook_at(repo, BOTTLING_HOOK_FILE))) && !hook_at(repo, BOTTLING_HOOK_FILE).exists(), "the unedited one goes: {undone:?}");
        // The other way round: the bottling hook edited, the check left as init wrote it.
        std::fs::write(hook_at(repo, HOOK_FILE), hook_script()).unwrap();
        let edited = crate::init::bottling_hook_script().replace("exec grimoire bottle", "exec grimoire bottle --quiet");
        std::fs::write(hook_at(repo, BOTTLING_HOOK_FILE), &edited).unwrap();
        let undone = uninstall(repo);
        assert_eq!(std::fs::read_to_string(hook_at(repo, BOTTLING_HOOK_FILE)).unwrap(), edited, "kept, byte for byte");
        assert!(undone.iter().any(|u| matches!(u, Undone::Left { path, .. } if path.ends_with("post-commit"))), "{undone:?}");
        assert!(undone.contains(&Undone::Removed(hook_at(repo, HOOK_FILE))) && !hook_at(repo, HOOK_FILE).exists());
        // A hook of the person's own, with nothing of init's in it, is kept too.
        std::fs::write(hook_at(repo, HOOK_FILE), "#!/bin/sh\necho mine\n").unwrap();
        uninstall(repo);
        assert_eq!(std::fs::read_to_string(hook_at(repo, HOOK_FILE)).unwrap(), "#!/bin/sh\necho mine\n");
    }

    /// # Test contract
    ///
    /// ```json
    /// {"test": "the-section-comes-out-only-when-its-markers-form-one-pair", "checks": ["crates/cli/src/init/uninstall#section-only-when-one-pair"]}
    /// ```
    #[test]
    fn the_section_comes_out_only_when_its_markers_form_one_pair() {
        use crate::init::guidance::{MARK_END, MARK_START};
        let section = format!("{MARK_START}\nours\n{MARK_END}\n");
        let cases = [
            ("two pairs", format!("# Mine\n\n{section}\n{section}")),
            ("a start with no end", format!("# Mine\n\n{MARK_START}\nours\n")),
            ("an end with no start", format!("# Mine\n\nours\n{MARK_END}\n")),
            ("the end before the start", format!("# Mine\n\n{MARK_END}\nours\n{MARK_START}\n")),
            ("two starts, one end", format!("# Mine\n\n{MARK_START}\n{MARK_START}\nours\n{MARK_END}\n")),
        ];
        for (what, text) in cases {
            for file in ["CLAUDE.md", "AGENTS.md"] {
                let tmp = temp();
                let repo = tmp.path();
                std::fs::write(repo.join(file), &text).unwrap();
                let undone = uninstall(repo);
                assert_eq!(std::fs::read_to_string(repo.join(file)).unwrap(), text, "{what} in {file}: left as it was");
                assert!(undone.iter().any(|u| matches!(u, Undone::Left { path, note } if path.ends_with(file) && note.contains("exactly one pair"))), "{what} in {file}: {undone:?}");
            }
        }
        // Exactly one pair comes out.
        for file in ["CLAUDE.md", "AGENTS.md"] {
            let tmp = temp();
            let repo = tmp.path();
            std::fs::write(repo.join(file), format!("# Mine\n\n{section}")).unwrap();
            let undone = uninstall(repo);
            assert_eq!(std::fs::read_to_string(repo.join(file)).unwrap(), "# Mine\n", "{file}");
            assert!(undone.contains(&Undone::Changed(repo.join(file))), "{undone:?}");
        }
    }

    /// # Test contract
    ///
    /// ```json
    /// {"test": "an-earlier-releases-post-commit-hook-is-inits-own", "checks": ["crates/cli/src/init/uninstall#hooks-only-when-unedited", "crates/cli/src/init/uninstall#earlier-release-post-commit-hook", "crates/cli/src/init#earlier-releases-hook-is-rewritten"]}
    /// ```
    #[test]
    fn an_earlier_releases_post_commit_hook_is_inits_own() {
        let former = crate::init::former_hook_script(BOTTLING_HOOK_FILE).expect("the post-commit hook had a former script");
        assert!(former.contains("exec grimoire land"));
        // Installed before landing was renamed bottling: init rewrites it to run `grimoire bottle`.
        let tmp = temp();
        let repo = tmp.path();
        std::fs::create_dir_all(repo.join(".git").join("hooks")).unwrap();
        std::fs::write(hook_at(repo, BOTTLING_HOOK_FILE), &former).unwrap();
        install(repo, 4242, "shop").unwrap();
        assert_eq!(std::fs::read_to_string(hook_at(repo, BOTTLING_HOOK_FILE)).unwrap(), crate::init::bottling_hook_script());
        // And uninstall takes it out as it was installed, even never rewritten.
        std::fs::write(hook_at(repo, BOTTLING_HOOK_FILE), former.replace('\n', "\r\n")).unwrap();
        uninstall(repo);
        assert!(!hook_at(repo, BOTTLING_HOOK_FILE).exists());
    }

    #[test]
    fn uninstall_empties_what_init_made_from_nothing() {
        let tmp = temp();
        let repo = tmp.path();
        std::fs::create_dir_all(repo.join(".git").join("hooks")).unwrap();
        install(repo, 4242, "shop").unwrap();
        uninstall(repo);
        assert!(!repo.join(".mcp.json").exists());
        assert!(!repo.join("CLAUDE.md").exists());
        assert!(!repo.join(".claude").exists());
        assert!(!repo.join(".agents").exists());
        assert!(!repo.join(".codex").exists());
    }

    /// # Test contract
    ///
    /// ```json
    /// {"test": "nothing-changes-until-the-plan-is-applied", "checks": ["crates/cli/src/init/uninstall#plans-before-changing"]}
    /// ```
    #[test]
    fn nothing_changes_until_the_plan_is_applied() {
        let tmp = temp();
        let repo = tmp.path();
        std::fs::create_dir_all(repo.join(".git").join("hooks")).unwrap();
        install(repo, 4242, "shop").unwrap();
        let before: Vec<_> = walk(repo);
        let plan = plan_uninstall(repo);
        assert_eq!(walk(repo), before, "planning only reads");
        assert!(plan.report().iter().any(|u| matches!(u, Undone::Removed(_))));
    }

    /// # Test contract
    ///
    /// ```json
    /// {"test": "a-failure-part-way-says-what-was-done", "checks": ["crates/cli/src/init/uninstall#failure-stops-and-reports-done"]}
    /// ```
    #[test]
    fn a_failure_part_way_says_what_was_done() {
        let tmp = temp();
        let repo = tmp.path();
        install(repo, 4242, "shop").unwrap();
        let plan = plan_uninstall(repo);
        // A folder where the plan expects to remove init's guidance file: that step fails.
        std::fs::remove_file(repo.join("AGENTS.md")).unwrap();
        std::fs::create_dir(repo.join("AGENTS.md")).unwrap();
        let stopped = plan.apply(repo).unwrap_err();
        assert!(matches!(&stopped.error, InitError::Io(p, _) if p.ends_with("AGENTS.md")), "{:?}", stopped.error);
        // What came before it in the plan was done, and is reported; nothing after it was touched.
        assert!(stopped.done.iter().all(|u| !matches!(u, Undone::Removed(p) | Undone::Changed(p) if p.ends_with("AGENTS.md") || p.ends_with(".mcp.json"))), "{:?}", stopped.done);
        assert!(stopped.done.contains(&Undone::Removed(repo.join(".claude/skills/grimoire/SKILL.md"))), "{:?}", stopped.done);
        assert!(!repo.join(".claude/skills/grimoire/SKILL.md").exists());
        assert!(repo.join(".mcp.json").is_file(), "the steps after the failure were not run");
    }
}
