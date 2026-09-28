//! # Module contract
//!
//! ```json
//! {"purpose":"Install Grimoire's integration into a repository: the project file naming the store, the agent skill with its references, the always-loaded guidance section, the MCP entry pointing at the daemon, and a pre-commit hook running the check.","criteria":[{"id":"listed-writes","statement":"Every path init writes or would have written is returned, so the command can print exactly what it did."},{"id":"merge-not-clobber","statement":"An existing .mcp.json keeps its other servers and an existing pre-commit hook is left alone with a note; only Grimoire's own files are overwritten."},{"id":"skill-is-the-source","statement":"The installed skill and its reference files are the ones in this repository's .claude/skills, embedded at build time, so there is one copy to maintain."},{"id":"guidance-section","statement":"A marked section between <!-- grimoire --> and <!-- /grimoire --> is written into CLAUDE.md and AGENTS.md, replacing a previous section and leaving everything outside the markers as it was, so an agent that never triggers the skill still learns the project is managed."},{"id":"project-agnostic","statement":"The embedded skill, references and guidance section name no project: a test fails the build if any of them mentions Grimoire's own crates, web package, npm scripts or dev loop, since they are installed verbatim into every repository."}]}
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
            if existing.contains("Installed by `grimoire init`") {
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

#[cfg(test)]
mod tests {
    use super::*;

    fn temp() -> PathBuf {
        let nanos = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos();
        let d = std::env::temp_dir().join(format!("grimoire-init-{}-{nanos}", std::process::id()));
        std::fs::create_dir_all(&d).unwrap();
        d
    }

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
}
