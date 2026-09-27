//! # Module contract
//!
//! ```json
//! {"purpose":"Install Grimoire's integration into a repository: the project file naming the store, the agent skill, the MCP entry pointing at the daemon, and a pre-commit hook running the check.","criteria":[{"id":"listed-writes","statement":"Every path init writes or would have written is returned, so the command can print exactly what it did."},{"id":"merge-not-clobber","statement":"An existing .mcp.json keeps its other servers and an existing pre-commit hook is left alone with a note; only Grimoire's own files are overwritten."},{"id":"skill-is-the-source","statement":"The installed skill is the one in this repository's .claude/skills, embedded at build time, so there is one copy to maintain."}]}
//! ```

use std::path::{Path, PathBuf};

use serde_json::{json, Value};

/// The Claude Code skill, embedded from this repository so `init` installs
/// exactly what Grimoire's own development uses.
pub const SKILL_MD: &str = include_str!("../../../.claude/skills/grimoire/SKILL.md");

pub const SKILL_REL: &str = ".claude/skills/grimoire/SKILL.md";
pub const MCP_REL: &str = ".mcp.json";
pub const HOOK_REL: &str = ".git/hooks/pre-commit";

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

/// The hook script: fails the commit when the manual is inconsistent.
pub fn hook_script() -> String {
    "#!/bin/sh\n# Installed by `grimoire init`. Fails the commit when the manual and the code disagree.\nif ! command -v grimoire >/dev/null 2>&1; then\n  echo \"pre-commit: grimoire is not on PATH; the manual was not checked\" >&2\n  exit 0\nfi\nexec grimoire check\n".to_string()
}

/// Write the integration into `repo`. `.grimoire.toml` is written by the
/// caller (store_io) since it is the store's concern; this writes the rest.
pub fn install(repo: &Path, port: u16) -> Result<Vec<Wrote>, InitError> {
    let mut out = Vec::new();

    // The skill: always Grimoire's, always overwritten.
    let skill = repo.join(SKILL_REL);
    write(&skill, SKILL_MD)?;
    out.push(Wrote::Written(skill));

    // .mcp.json: merge our server in, keep everything else.
    let mcp = repo.join(MCP_REL);
    let mut doc: Value = if mcp.is_file() {
        let text = std::fs::read_to_string(&mcp).map_err(|e| InitError::Io(mcp.clone(), e))?;
        serde_json::from_str(&text).map_err(|e| InitError::BadJson(mcp.clone(), e.to_string()))?
    } else {
        json!({})
    };
    if !doc.is_object() {
        return Err(InitError::BadJson(mcp, "top level is not an object".into()));
    }
    let servers = doc.as_object_mut().unwrap().entry("mcpServers").or_insert_with(|| json!({}));
    if !servers.is_object() {
        return Err(InitError::BadJson(mcp, "mcpServers is not an object".into()));
    }
    servers.as_object_mut().unwrap().insert("grimoire".into(), json!({ "type": "http", "url": format!("http://127.0.0.1:{port}/mcp") }));
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
            if existing.contains("grimoire check") {
                out.push(Wrote::Written(hook));
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

        let wrote = install(&repo, 4343).unwrap();
        assert_eq!(wrote.len(), 3);
        assert!(wrote.iter().all(|w| matches!(w, Wrote::Written(_))));

        let skill = std::fs::read_to_string(repo.join(SKILL_REL)).unwrap();
        assert!(skill.starts_with("---\nname: grimoire"));

        let mcp: Value = serde_json::from_str(&std::fs::read_to_string(repo.join(MCP_REL)).unwrap()).unwrap();
        assert_eq!(mcp["mcpServers"]["other"]["command"], "x");
        assert_eq!(mcp["mcpServers"]["grimoire"]["url"], "http://127.0.0.1:4343/mcp");

        let hook = std::fs::read_to_string(repo.join(HOOK_REL)).unwrap();
        assert!(hook.contains("exec grimoire check"));

        // Second run: idempotent, hook recognised as ours.
        let again = install(&repo, 4343).unwrap();
        assert!(again.iter().all(|w| matches!(w, Wrote::Written(_))));
        std::fs::remove_dir_all(&repo).unwrap();
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
