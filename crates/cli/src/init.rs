//! # Module contract
//!
//! ```json
//! {"purpose":"Install Grimoire's integration into a repository for Claude Code and Codex alike, and remove it again: the project file naming the store, the agent skill with its references in each harness's place, the always-loaded guidance section, the MCP entry pointing at the daemon in each harness's configuration, and a pre-commit hook running the check. The guidance section, the Codex configuration and the uninstall plan each have a module beneath.","criteria":[{"id":"skill-sequences","statement":"The embedded skill holds a sequence to start, specify, finish and file a ticket, to resolve a gap or question with the person, and to adopt existing code and its tests, each under its own heading."},{"id":"listed-writes","statement":"Every path init writes or would have written, with what a written path still needs of the person, and every path uninstall removes, changes or leaves, is returned, so each command can print exactly what it did."},{"id":"merge-not-clobber","statement":"An existing .mcp.json keeps its other servers; an existing .codex/config.toml keeps every byte but the grimoire server's url value, or init's table appended after it, including comments, line endings and a byte-order mark; an existing pre-commit hook is left alone with a note; a file init cannot merge into is refused before anything is written; only Grimoire's own files are overwritten."},{"id":"every-harness","statement":"init writes the same embedded skill and references to .claude/skills/grimoire/ and .agents/skills/grimoire/, and the grimoire server at the daemon's URL to both .mcp.json and .codex/config.toml, always, with no flag or detection."},{"id":"codex-trust","statement":"init writes no agent configuration outside the repository, and returns .codex/config.toml with the note that Codex reads it only once the project is trusted in the person's own Codex configuration."},{"id":"skill-is-the-source","statement":"The installed skill and its reference files are the ones in this repository's .claude/skills, embedded at build time, so there is one copy to maintain."},{"id":"guidance-section","statement":"A marked section between <!-- grimoire --> and <!-- /grimoire --> is written into CLAUDE.md and AGENTS.md, replacing a previous section and leaving everything outside the markers as it was, so an agent that never triggers the skill still learns the project is managed; each file's section names its own harness's copy of the skill, CLAUDE.md the one under .claude/skills and AGENTS.md the one under .agents/skills; of the pre-commit check it says only that it finds what is left unconnected, never that it checks the grimoire and the code agree, since the check judges no meaning."},{"id":"project-agnostic","statement":"The embedded skill, references and guidance section name no project and list no grimoire's pages: a test fails the build if any of them mentions Grimoire's own crates, web package, npm scripts or dev loop, or names the story pages as a list, since they are installed verbatim into every repository and each root page's links are its reading order."}]}
//! ```

use std::path::{Path, PathBuf};

use serde_json::{json, Value};

mod codex;
mod guidance;
mod uninstall;

pub use codex::{with_codex_server, without_codex_server};
pub use guidance::with_guidance;
pub use uninstall::{plan_uninstall, Undone};

/// The skill and its references, embedded from this repository so `init`
/// installs exactly what Grimoire's own development uses.
pub const SKILL_MD: &str = include_str!("../../../.claude/skills/grimoire/SKILL.md");
pub const CONTRACTS_MD: &str = include_str!("../../../.claude/skills/grimoire/references/contracts.md");
pub const CHANGES_MD: &str = include_str!("../../../.claude/skills/grimoire/references/changes.md");
/// The section every session reads, for CLAUDE.md and AGENTS.md, with
/// `{skill}` standing for the path of that harness's copy of the skill.
pub const GUIDANCE_MD: &str = include_str!("../../../.claude/skills/grimoire/references/guidance-section.md");

/// The skill's files, relative to a harness's skill folder.
pub const SKILL_FILES: [(&str, &str); 3] = [("SKILL.md", SKILL_MD), ("references/contracts.md", CONTRACTS_MD), ("references/changes.md", CHANGES_MD)];

/// An agent harness init sets up: where it loads skills from, and the file
/// every one of its sessions reads.
pub struct Harness {
    /// The skill's folder; its parents up to the harness's own folder are
    /// removed by uninstall only when left empty.
    pub skill_dir: &'static str,
    pub guidance: &'static str,
}

pub const HARNESSES: [Harness; 2] = [
    Harness { skill_dir: ".claude/skills/grimoire", guidance: "CLAUDE.md" },
    Harness { skill_dir: ".agents/skills/grimoire", guidance: "AGENTS.md" },
];

pub const MCP_REL: &str = ".mcp.json";
pub const CODEX_REL: &str = ".codex/config.toml";
/// What `.codex/config.toml` still needs of the person.
pub const CODEX_TRUST: &str = "Codex reads this only once the project is trusted in your own Codex configuration (~/.codex/config.toml); init leaves that to you";
pub const HOOK_REL: &str = ".git/hooks/pre-commit";

/// The guidance section for one harness, naming its copy of the skill.
pub fn section(harness: &Harness) -> String {
    GUIDANCE_MD.trim_end().replace("{skill}", &format!("{}/SKILL.md", harness.skill_dir))
}

/// What happened to one path.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Wrote {
    Written(PathBuf),
    /// Written, and still needing something of the person, which `note` says.
    Noted { path: PathBuf, note: String },
    /// Already present and not Grimoire's to overwrite; `note` says what to add by hand.
    Skipped { path: PathBuf, note: String },
}

#[derive(Debug, thiserror::Error)]
pub enum InitError {
    #[error("{0}: {1}")]
    Io(PathBuf, std::io::Error),
    #[error("{0} is not valid JSON: {1}")]
    BadJson(PathBuf, String),
    #[error("{0} is not valid TOML: {1}")]
    BadToml(PathBuf, String),
}

fn write(path: &Path, text: &str) -> Result<(), InitError> {
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir).map_err(|e| InitError::Io(dir.to_path_buf(), e))?;
    }
    std::fs::write(path, text).map_err(|e| InitError::Io(path.to_path_buf(), e))
}

/// The marker line init's hook carries, which tells it from a person's.
const HOOK_MARK: &str = "Installed by `grimoire init`";

/// The hook script: fails the commit when the grimoire is inconsistent.
pub fn hook_script() -> String {
    "#!/bin/sh\n# Installed by `grimoire init`. Fails the commit when the grimoire and the code disagree.\nif ! command -v grimoire >/dev/null 2>&1; then\n  echo \"pre-commit: grimoire is not on PATH; the grimoire was not checked\" >&2\n  exit 0\nfi\nexec grimoire check\n".to_string()
}

/// Write the integration into `repo`: `plan_install`, then apply it.
#[cfg(test)]
pub fn install(repo: &Path, port: u16) -> Result<Vec<Wrote>, InitError> {
    plan_install(repo, port)?.apply()
}

/// An install worked out in full: every file of the integration with its
/// new text. Planning only reads, so a file init cannot merge into stops it
/// before the caller has written anything either.
pub struct Install {
    repo: PathBuf,
    texts: Vec<(PathBuf, String, Option<&'static str>)>,
}

/// Work out the integration for `repo`, reading only. `.grimoire.toml` is
/// written by the caller (store_io) since it is the store's concern, after
/// this has succeeded.
pub fn plan_install(repo: &Path, port: u16) -> Result<Install, InitError> {
    let read = |path: &Path| -> Result<String, InitError> {
        if path.is_file() {
            std::fs::read_to_string(path).map_err(io(path))
        } else {
            Ok(String::new())
        }
    };
    let mut texts: Vec<(PathBuf, String, Option<&'static str>)> = Vec::new();

    // The skill and its references, in every harness's place: always Grimoire's, always overwritten.
    for harness in &HARNESSES {
        for (rel, text) in SKILL_FILES {
            texts.push((repo.join(format!("{}/{rel}", harness.skill_dir)), text.to_string(), None));
        }
    }

    // The guidance section every session reads, naming that harness's skill: replace ours, keep theirs.
    for harness in &HARNESSES {
        let path = repo.join(harness.guidance);
        let text = with_guidance(&read(&path)?, &section(harness));
        texts.push((path, text, None));
    }

    // .mcp.json: merge our server in, keep everything else.
    let mcp = repo.join(MCP_REL);
    let text = with_mcp_server(&read(&mcp)?, port).map_err(|e| InitError::BadJson(mcp.clone(), e))?;
    texts.push((mcp, text, None));

    // .codex/config.toml: our server spliced in, every other byte as it was.
    let codex = repo.join(CODEX_REL);
    let text = with_codex_server(&read(&codex)?, port).map_err(|e| InitError::BadToml(codex.clone(), e))?;
    texts.push((codex, text, Some(CODEX_TRUST)));
    Ok(Install { repo: repo.to_path_buf(), texts })
}

impl Install {
    /// Write every planned file, then the hook.
    pub fn apply(self) -> Result<Vec<Wrote>, InitError> {
        let repo = self.repo.as_path();
        let mut out = Vec::new();
        for (path, text, note) in self.texts {
            write(&path, &text)?;
            out.push(match note {
                Some(note) => Wrote::Noted { path, note: note.into() },
                None => Wrote::Written(path),
            });
        }

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
}

/// `existing` .mcp.json with the grimoire server set and everything else kept as data.
fn with_mcp_server(existing: &str, port: u16) -> Result<String, String> {
    let mut doc: Value = if existing.trim().is_empty() { json!({}) } else { serde_json::from_str(existing).map_err(|e| e.to_string())? };
    let Some(root) = doc.as_object_mut() else {
        return Err("top level is not an object".into());
    };
    let Some(servers) = root.entry("mcpServers").or_insert_with(|| json!({})).as_object_mut() else {
        return Err("mcpServers is not an object".into());
    };
    servers.insert("grimoire".into(), json!({ "type": "http", "url": format!("http://127.0.0.1:{port}/mcp") }));
    let mut text = serde_json::to_string_pretty(&doc).map_err(|e| e.to_string())?;
    text.push('\n');
    Ok(text)
}

fn io(path: &Path) -> impl FnOnce(std::io::Error) -> InitError + '_ {
    move |e| InitError::Io(path.to_path_buf(), e)
}

/// The line ending a text uses, for the lines init adds to it.
fn eol_of(text: &str) -> &'static str {
    if text.contains("\r\n") {
        "\r\n"
    } else {
        "\n"
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use super::guidance::{MARK_END, MARK_START};

    /// A fresh folder, unique however many tests run at once, removed when dropped.
    pub(super) fn temp() -> tempfile::TempDir {
        tempfile::Builder::new().prefix("grimoire-init-").tempdir().unwrap()
    }

    /// # Test contract
    ///
    /// ```json
    /// {"test": "installs-and-merges", "checks": ["crates/cli/src/init#listed-writes", "crates/cli/src/init#merge-not-clobber"]}
    /// ```
    #[test]
    fn installs_and_merges() {
        let tmp = temp();
        let repo = tmp.path();
        std::fs::create_dir_all(repo.join(".git").join("hooks")).unwrap();
        std::fs::write(repo.join(".mcp.json"), r#"{"mcpServers":{"other":{"type":"stdio","command":"x"}}}"#).unwrap();

        std::fs::write(repo.join("CLAUDE.md"), "# My project\n\nUse tabs.\n").unwrap();

        let wrote = install(repo, 4343).unwrap();
        // Three skill files per harness, two guidance files, .mcp.json, .codex/config.toml, the hook.
        assert_eq!(wrote.len(), 11);
        let noted: Vec<_> = wrote.iter().filter(|w| !matches!(w, Wrote::Written(_))).collect();
        assert_eq!(noted, [&Wrote::Noted { path: repo.join(CODEX_REL), note: CODEX_TRUST.into() }]);

        let skill = std::fs::read_to_string(repo.join(".claude/skills/grimoire/SKILL.md")).unwrap();
        assert!(skill.starts_with("---\nname: grimoire"));
        assert!(std::fs::read_to_string(repo.join(".claude/skills/grimoire/references/contracts.md")).unwrap().contains("# Module contracts"));
        assert!(std::fs::read_to_string(repo.join(".claude/skills/grimoire/references/changes.md")).unwrap().contains("| Change in the code |"));

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
        let again = install(repo, 4343).unwrap();
        assert!(again.iter().all(|w| matches!(w, Wrote::Written(_) | Wrote::Noted { .. })));
        let claude = std::fs::read_to_string(repo.join("CLAUDE.md")).unwrap();
        assert_eq!(claude.matches(MARK_START).count(), 1);
        assert!(claude.starts_with("# My project\n\nUse tabs.\n"));
    }

    /// # Test contract
    ///
    /// ```json
    /// {"test": "each-guidance-file-names-its-own-skill", "checks": ["crates/cli/src/init#guidance-section"]}
    /// ```
    #[test]
    fn each_guidance_file_names_its_own_skill() {
        let tmp = temp();
        let repo = tmp.path();
        install(repo, 4242).unwrap();
        let claude = std::fs::read_to_string(repo.join("CLAUDE.md")).unwrap();
        let agents = std::fs::read_to_string(repo.join("AGENTS.md")).unwrap();
        assert!(claude.contains("`.claude/skills/grimoire/SKILL.md`") && !claude.contains(".agents/"), "{claude}");
        assert!(agents.contains("`.agents/skills/grimoire/SKILL.md`") && !agents.contains(".claude/"), "{agents}");
        assert!(!claude.contains("{skill}") && !agents.contains("{skill}"), "the placeholder is filled");
        assert!(GUIDANCE_MD.matches("{skill}").count() == 1, "the source names the skill once, by placeholder");
    }

    /// # Test contract
    ///
    /// ```json
    /// {"test": "every-harness-gets-the-skill-and-the-tools", "checks": ["crates/cli/src/init#every-harness", "crates/cli/src/init#codex-trust"]}
    /// ```
    #[test]
    fn every_harness_gets_the_skill_and_the_tools() {
        let tmp = temp();
        let repo = tmp.path();
        let wrote = install(repo, 4343).unwrap();
        for harness in &HARNESSES {
            for (rel, text) in SKILL_FILES {
                let on_disk = std::fs::read_to_string(repo.join(format!("{}/{rel}", harness.skill_dir))).unwrap();
                assert_eq!(on_disk, text, "{}/{rel} is the embedded text", harness.skill_dir);
            }
        }
        let mcp: Value = serde_json::from_str(&std::fs::read_to_string(repo.join(MCP_REL)).unwrap()).unwrap();
        assert_eq!(mcp["mcpServers"]["grimoire"]["url"], "http://127.0.0.1:4343/mcp");
        let codex: toml_edit::DocumentMut = std::fs::read_to_string(repo.join(CODEX_REL)).unwrap().parse().unwrap();
        assert_eq!(codex["mcp_servers"]["grimoire"]["url"].as_str(), Some("http://127.0.0.1:4343/mcp"));
        // Nothing outside the repository: every path written is in it, and Codex's trust is left to the person.
        let paths: Vec<&PathBuf> = wrote.iter().map(|w| match w {
            Wrote::Written(p) | Wrote::Noted { path: p, .. } | Wrote::Skipped { path: p, .. } => p,
        }).collect();
        assert!(paths.iter().all(|p| p.starts_with(repo)), "{paths:?}");
        assert!(wrote.iter().any(|w| matches!(w, Wrote::Noted { path, note } if path.ends_with(CODEX_REL) && note.contains("trusted"))));
    }

    /// # Test contract
    ///
    /// ```json
    /// {"test": "a-refused-file-leaves-the-repository-as-it-was", "checks": ["crates/cli/src/init#merge-not-clobber", "crates/cli/src/init#listed-writes"]}
    /// ```
    #[test]
    fn a_refused_file_leaves_the_repository_as_it_was() {
        let tmp = temp();
        let repo = tmp.path();
        std::fs::create_dir_all(repo.join(".codex")).unwrap();
        std::fs::write(repo.join(CODEX_REL), "mcp_servers = { docs = { url = \"x\" } }\n").unwrap();
        let before = walk(repo);
        assert!(matches!(install(repo, 4242), Err(InitError::BadToml(..))));
        assert_eq!(walk(repo), before, "nothing is written before every file's text is known");
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

    /// Criterion project-agnostic: a paragraph naming three or more of the
    /// story pages is a copy of the reading order, which each root's links own.
    /// # Test contract
    ///
    /// ```json
    /// {"test": "the-guidance-says-only-what-the-check-does", "checks": ["crates/cli/src/init#guidance-section"]}
    /// ```
    #[test]
    fn the_guidance_says_only_what_the_check_does() {
        assert!(GUIDANCE_MD.contains("the pre-commit hook finds what is left unconnected"), "the section says what the check does");
        let lower = GUIDANCE_MD.to_lowercase();
        for claim in ["agree", "catch", "holds the code", "hold the code", "judge"] {
            assert!(!lower.contains(claim), "the section claims the check judges meaning: {claim:?}");
        }
    }

    /// # Test contract
    ///
    /// ```json
    /// {"test": "installed-texts-list-no-pages", "checks": ["crates/cli/src/init#project-agnostic"]}
    /// ```
    #[test]
    fn installed_texts_list_no_pages() {
        for (name, text) in [
            ("SKILL.md", SKILL_MD),
            ("contracts.md", CONTRACTS_MD),
            ("changes.md", CHANGES_MD),
            ("guidance-section.md", GUIDANCE_MD),
        ] {
            for paragraph in text.split("\n\n") {
                let lower = paragraph.to_lowercase();
                let named: Vec<&str> = grimoire_core::scaffold::STORY.iter().map(|(slug, _, _)| *slug).filter(|slug| lower.contains(slug)).collect();
                assert!(named.len() < 3, "{name} lists the story pages {named:?}; say to read each root's links instead:\n{paragraph}");
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
        let tmp = temp();
        let repo = tmp.path();
        std::fs::create_dir_all(repo.join(".git").join("hooks")).unwrap();
        std::fs::write(repo.join(HOOK_REL), "#!/bin/sh\nnpm test\n").unwrap();
        let wrote = install(repo, 4242).unwrap();
        assert!(wrote.iter().any(|w| matches!(w, Wrote::Skipped { .. })));
        assert_eq!(std::fs::read_to_string(repo.join(HOOK_REL)).unwrap(), "#!/bin/sh\nnpm test\n");
    }

    #[test]
    fn no_git_is_a_note_not_an_error() {
        let tmp = temp();
        let repo = tmp.path();
        let wrote = install(repo, 4242).unwrap();
        assert!(wrote.iter().any(|w| matches!(w, Wrote::Skipped { note, .. } if note.contains("not a git repository"))));
    }

    pub(super) fn walk(dir: &Path) -> Vec<(PathBuf, Vec<u8>)> {
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
