//! # Module contract
//!
//! ```json
//! {"purpose":"Install Grimoire's integration into a repository for Claude Code and Codex alike, and remove it again: the project file naming the store, the agent skill with its references in each harness's place, the always-loaded guidance section, the MCP entry pointing at the daemon in each harness's configuration, a pre-commit hook running the check and a post-commit hook running the bottling. The guidance section, the Codex configuration, the uninstall plan and the init command's own work each have a module beneath.","criteria":[{"id":"skill-sequences","statement":"The embedded skill holds a sequence to brew, specify, finish and inscribe a ticket, to resolve a gap or question with the person, and to adopt existing code and its tests, each under its own heading."},{"id":"skill-judges-recipes","statement":"The embedded skill's brew sequence reads the recipes brew_ticket names, and its finish sequence judges, before close_ticket, each recipe due over what the ticket touched, and any other the check reports due, only those, with judge_recipe, held when it still says truly how to use what it represents and updated once changed to say so, after the last change to the recipe and its files."},{"id":"listed-writes","statement":"Every path init writes or would have written, with what a written path still needs of the person, and every path uninstall removes, changes or leaves, is returned, so each command can print exactly what it did."},{"id":"mcp-json-keeps-other-servers","statement":"An existing .mcp.json keeps its other servers."},{"id":"codex-config-keeps-every-byte","statement":"An existing .codex/config.toml keeps every byte but the grimoire server's url value, or init's table appended after it, including comments, line endings and a byte-order mark."},{"id":"existing-hook-left-alone","statement":"An existing pre-commit or post-commit hook is left alone with a note."},{"id":"unmergeable-file-refused-first","statement":"A file init cannot merge into is refused before anything is written."},{"id":"only-own-files-overwritten","statement":"Only Grimoire's own files are overwritten."},{"id":"bottling-hook","statement":"In a git repository init writes a post-commit hook that runs grimoire bottle, so a commit bottles the ticket simmering on it, beside the pre-commit hook that runs the check; each exits 0 with a warning when grimoire is not on the path, and outside a git repository neither is written and the output says so; the post-commit hook an earlier release wrote, running grimoire land, is init's own and rewritten."},{"id":"every-harness","statement":"init writes the same embedded skill and references to .claude/skills/grimoire/ and .agents/skills/grimoire/, and the grimoire server at the URL of the project's grimoire in the daemon's library, http://127.0.0.1:<port>/g/<name>/mcp, to both .mcp.json and .codex/config.toml, always, with no flag or detection."},{"id":"codex-trust","statement":"init writes no agent configuration outside the repository, and returns .codex/config.toml with the note that Codex reads it only once the project is trusted in the person's own Codex configuration."},{"id":"skill-is-the-source","statement":"The installed skill and its reference files are the ones in this repository's .claude/skills, embedded at build time, so there is one copy to maintain."},{"id":"section-between-markers","statement":"A marked section between <!-- grimoire --> and <!-- /grimoire --> is written into CLAUDE.md and AGENTS.md, so an agent that never triggers the skill still learns the project is managed."},{"id":"replaces-previous-section","statement":"The marked section replaces a previous section."},{"id":"outside-markers-untouched","statement":"The marked section leaves everything outside the markers as it was."},{"id":"sends-to-the-rites","statement":"The section says how the work is done is in the rites, the grimoire's second root, to be read before working, and that a rule of work belongs there and never in the file."},{"id":"rest-of-file-is-the-projects","statement":"The section says the rest of the file is the project's own."},{"id":"names-own-harness-skill","statement":"Each file's section names its own harness's copy of the skill, CLAUDE.md the one under .claude/skills and AGENTS.md the one under .agents/skills."},{"id":"check-claim-is-only-unconnected","statement":"Of the pre-commit check the section says only that it finds what is left unconnected, never that it checks the grimoire and the code agree, since the check judges no meaning."},{"id":"skill-checkout","statement":"The embedded skill tells an agent to pass `checkout`, its working directory, with every call, and says that a new tool or a changed field may reach a client only in a new session, a reconnect bringing changed instructions but not always changed tools, and to call over HTTP until then."},{"id":"project-agnostic","statement":"The embedded skill, references and guidance section name no project and list no grimoire's pages: a test fails the build if any of them mentions Grimoire's own crates, web package, npm scripts or dev loop, or names the story pages as a list, since they are installed verbatim into every repository and each root page's links are its reading order."},{"id":"branch-holds-one-ticket","statement":"The embedded skill says a branch holds one ticket until that ticket is bottled."},{"id":"bottled-with-bottle-once-merged","statement":"The embedded skill says a ticket is bottled with grimoire bottle, on a team once the branch has merged to the main line."},{"id":"helpers-split-into-branches","statement":"The embedded skill says work too big for one checkout is split into branches of the ticket's branch, each taking the same ticket up and merging back into it, which alone is bottled."},{"id":"found-work-is-its-own-ticket","statement":"The embedded skill says work found along the way is inscribed as a ticket of its own."},{"id":"brew-answers-waiting-never-refuses","statement":"The embedded skill says brew_ticket answers in waiting each ticket the checkout still carries, never refusing."},{"id":"brew-sequence-bottle-first-or-move","statement":"The embedded skill's brew sequence says to bottle the waiting ticket first or move the work to a new branch."}]}
//! ```

use std::path::{Path, PathBuf};

use serde_json::{json, Value};

mod codex;
pub(crate) mod command;
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

pub const HARNESSES: [Harness; 2] = [Harness { skill_dir: ".claude/skills/grimoire", guidance: "CLAUDE.md" }, Harness { skill_dir: ".agents/skills/grimoire", guidance: "AGENTS.md" }];

pub const MCP_REL: &str = ".mcp.json";
pub const CODEX_REL: &str = ".codex/config.toml";
/// What `.codex/config.toml` still needs of the person.
pub const CODEX_TRUST: &str = "Codex reads this only once the project is trusted in your own Codex configuration (~/.codex/config.toml); init leaves that to you";
pub const HOOK_REL: &str = ".git/hooks/pre-commit";
pub const BOTTLING_HOOK_REL: &str = ".git/hooks/post-commit";

/// The guidance section for one harness, naming its copy of the skill.
pub fn section(harness: &Harness) -> String {
    GUIDANCE_MD.trim_end().replace("{skill}", &format!("{}/SKILL.md", harness.skill_dir))
}

/// What happened to one path.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Wrote {
    Written(PathBuf),
    /// Written, and still needing something of the person, which `note` says.
    Noted {
        path: PathBuf,
        note: String,
    },
    /// Already present and not Grimoire's to overwrite; `note` says what to add by hand.
    Skipped {
        path: PathBuf,
        note: String,
    },
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

/// The post-commit hook: tells Grimoire a commit was made, so the ticket simmering on it is bottled.
pub fn bottling_hook_script() -> String {
    "#!/bin/sh\n# Installed by `grimoire init`. Bottles the ticket simmering on this commit.\nif ! command -v grimoire >/dev/null 2>&1; then\n  echo \"post-commit: grimoire is not on PATH; no ticket was bottled\" >&2\n  exit 0\nfi\nexec grimoire bottle\n".to_string()
}

/// The script an earlier release installed at `rel`, before ticket 0168 renamed landing to bottling:
/// still init's own, so uninstall takes it out unedited and init rewrites it.
pub fn former_hook_script(rel: &str) -> Option<String> {
    (rel == BOTTLING_HOOK_REL).then(|| "#!/bin/sh\n# Installed by `grimoire init`. Lands the ticket waiting on this commit.\nif ! command -v grimoire >/dev/null 2>&1; then\n  echo \"post-commit: grimoire is not on PATH; no ticket was landed\" >&2\n  exit 0\nfi\nexec grimoire land\n".to_string())
}

/// init's hooks: where each goes, its script, and the command it runs.
pub fn hooks() -> [(&'static str, String, &'static str); 2] {
    [(HOOK_REL, hook_script(), "grimoire check"), (BOTTLING_HOOK_REL, bottling_hook_script(), "grimoire bottle")]
}

/// Write the integration into `repo`: `plan_install`, then apply it.
#[cfg(test)]
pub fn install(repo: &Path, port: u16, name: &str) -> Result<Vec<Wrote>, InitError> {
    plan_install(repo, port, name)?.apply()
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
pub fn plan_install(repo: &Path, port: u16, name: &str) -> Result<Install, InitError> {
    let url = mcp_url(port, name);
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
    let text = with_mcp_server(&read(&mcp)?, &url).map_err(|e| InitError::BadJson(mcp.clone(), e))?;
    texts.push((mcp, text, None));

    // .codex/config.toml: our server spliced in, every other byte as it was.
    let codex = repo.join(CODEX_REL);
    let text = with_codex_server(&read(&codex)?, &url).map_err(|e| InitError::BadToml(codex.clone(), e))?;
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

        // The hooks, the check before a commit and the bottling after it: only
        // when the repository is a git checkout, and each only where there is none.
        let git = repo.join(".git");
        for (rel, script, command) in hooks() {
            let hook = repo.join(rel);
            let kind = rel.rsplit('/').next().unwrap_or(rel);
            if !git.is_dir() {
                out.push(Wrote::Skipped { path: hook, note: format!("not a git repository; run `{command}` from your own hook or CI") });
            } else if hook.exists() {
                let existing = std::fs::read_to_string(&hook).unwrap_or_default();
                if existing.contains(HOOK_MARK) {
                    // Ours: rewrite, so a changed script reaches the repository.
                    write(&hook, &script)?;
                    out.push(Wrote::Written(hook));
                } else if existing.contains(command) {
                    out.push(Wrote::Skipped { path: hook, note: format!("the existing {kind} hook already runs `{command}`") });
                } else {
                    out.push(Wrote::Skipped { path: hook, note: format!("a {kind} hook already exists; add `{command}` to it") });
                }
            } else {
                write(&hook, &script)?;
                #[cfg(unix)]
                {
                    use std::os::unix::fs::PermissionsExt;
                    let _ = std::fs::set_permissions(&hook, std::fs::Permissions::from_mode(0o755));
                }
                out.push(Wrote::Written(hook));
            }
        }

        Ok(out)
    }
}

/// Where an agent reaches the project's grimoire: its path in the library of
/// the daemon on this machine's `port` (req:machine-library).
pub fn mcp_url(port: u16, name: &str) -> String {
    format!("http://127.0.0.1:{port}/g/{name}/mcp")
}

/// `existing` .mcp.json with the grimoire server set and everything else kept as data.
fn with_mcp_server(existing: &str, url: &str) -> Result<String, String> {
    let mut doc: Value = if existing.trim().is_empty() { json!({}) } else { serde_json::from_str(existing).map_err(|e| e.to_string())? };
    let Some(root) = doc.as_object_mut() else {
        return Err("top level is not an object".into());
    };
    let Some(servers) = root.entry("mcpServers").or_insert_with(|| json!({})).as_object_mut() else {
        return Err("mcpServers is not an object".into());
    };
    servers.insert("grimoire".into(), json!({ "type": "http", "url": url }));
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
    use super::guidance::{MARK_END, MARK_START};
    use super::*;

    /// A fresh folder, unique however many tests run at once, removed when dropped.
    pub(super) fn temp() -> tempfile::TempDir {
        tempfile::Builder::new().prefix("grimoire-init-").tempdir().unwrap()
    }

    /// # Test contract
    ///
    /// ```json
    /// {"test": "installs-and-merges", "checks": ["crates/cli/src/init#listed-writes", "crates/cli/src/init#mcp-json-keeps-other-servers", "crates/cli/src/init#bottling-hook"]}
    /// ```
    #[test]
    fn installs_and_merges() {
        let tmp = temp();
        let repo = tmp.path();
        std::fs::create_dir_all(repo.join(".git").join("hooks")).unwrap();
        std::fs::write(repo.join(".mcp.json"), r#"{"mcpServers":{"other":{"type":"stdio","command":"x"}}}"#).unwrap();

        std::fs::write(repo.join("CLAUDE.md"), "# My project\n\nUse tabs.\n").unwrap();

        let wrote = install(repo, 4343, "shop").unwrap();
        // Three skill files per harness, two guidance files, .mcp.json, .codex/config.toml, the two hooks.
        assert_eq!(wrote.len(), 12);
        // The check before a commit and the bottling after it, each quiet when grimoire is not on the path.
        for (rel, _, command) in hooks() {
            let hook = std::fs::read_to_string(repo.join(rel)).unwrap();
            assert!(hook.contains(HOOK_MARK) && hook.ends_with(&format!("exec {command}\n")) && hook.contains("exit 0"), "{rel}: {hook}");
        }
        assert!(bottling_hook_script().contains("exec grimoire bottle"));
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
        assert_eq!(mcp["mcpServers"]["grimoire"]["url"], "http://127.0.0.1:4343/g/shop/mcp");

        let hook = std::fs::read_to_string(repo.join(HOOK_REL)).unwrap();
        assert!(hook.contains("exec grimoire check"));

        // Second run: idempotent, hook recognised as ours, one section not two.
        let again = install(repo, 4343, "shop").unwrap();
        assert!(again.iter().all(|w| matches!(w, Wrote::Written(_) | Wrote::Noted { .. })));
        let claude = std::fs::read_to_string(repo.join("CLAUDE.md")).unwrap();
        assert_eq!(claude.matches(MARK_START).count(), 1);
        assert!(claude.starts_with("# My project\n\nUse tabs.\n"));
    }

    /// # Test contract
    ///
    /// ```json
    /// {"test": "each-guidance-file-names-its-own-skill", "checks": ["crates/cli/src/init#names-own-harness-skill"]}
    /// ```
    #[test]
    fn each_guidance_file_names_its_own_skill() {
        let tmp = temp();
        let repo = tmp.path();
        install(repo, 4242, "shop").unwrap();
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
        let wrote = install(repo, 4343, "shop").unwrap();
        for harness in &HARNESSES {
            for (rel, text) in SKILL_FILES {
                let on_disk = std::fs::read_to_string(repo.join(format!("{}/{rel}", harness.skill_dir))).unwrap();
                assert_eq!(on_disk, text, "{}/{rel} is the embedded text", harness.skill_dir);
            }
        }
        let mcp: Value = serde_json::from_str(&std::fs::read_to_string(repo.join(MCP_REL)).unwrap()).unwrap();
        assert_eq!(mcp["mcpServers"]["grimoire"]["url"], "http://127.0.0.1:4343/g/shop/mcp");
        let codex: toml_edit::DocumentMut = std::fs::read_to_string(repo.join(CODEX_REL)).unwrap().parse().unwrap();
        assert_eq!(codex["mcp_servers"]["grimoire"]["url"].as_str(), Some("http://127.0.0.1:4343/g/shop/mcp"));
        // Nothing outside the repository: every path written is in it, and Codex's trust is left to the person.
        let paths: Vec<&PathBuf> = wrote
            .iter()
            .map(|w| match w {
                Wrote::Written(p) | Wrote::Noted { path: p, .. } | Wrote::Skipped { path: p, .. } => p,
            })
            .collect();
        assert!(paths.iter().all(|p| p.starts_with(repo)), "{paths:?}");
        assert!(wrote.iter().any(|w| matches!(w, Wrote::Noted { path, note } if path.ends_with(CODEX_REL) && note.contains("trusted"))));
    }

    /// # Test contract
    ///
    /// ```json
    /// {"test": "a-refused-file-leaves-the-repository-as-it-was", "checks": ["crates/cli/src/init#unmergeable-file-refused-first", "crates/cli/src/init#listed-writes"]}
    /// ```
    #[test]
    fn a_refused_file_leaves_the_repository_as_it_was() {
        let tmp = temp();
        let repo = tmp.path();
        std::fs::create_dir_all(repo.join(".codex")).unwrap();
        std::fs::write(repo.join(CODEX_REL), "mcp_servers = { docs = { url = \"x\" } }\n").unwrap();
        let before = walk(repo);
        assert!(matches!(install(repo, 4242, "shop"), Err(InitError::BadToml(..))));
        assert_eq!(walk(repo), before, "nothing is written before every file's text is known");
    }

    /// # Test contract
    ///
    /// ```json
    /// {"test": "only-grimoires-own-files-are-overwritten", "checks": ["crates/cli/src/init#only-own-files-overwritten"]}
    /// ```
    #[test]
    fn only_grimoires_own_files_are_overwritten() {
        let tmp = temp();
        let repo = tmp.path();
        std::fs::create_dir_all(repo.join(".git").join("hooks")).unwrap();
        // A person's files, some beside init's and one in a folder init writes into.
        let theirs = [
            (".git/hooks/pre-commit", "#!/bin/sh\necho mine\n"),
            (".claude/skills/mine/SKILL.md", "my skill\n"),
            (".claude/skills/grimoire/notes.md", "my notes\n"),
            (".agents/skills/grimoire/notes.md", "my notes\n"),
            (".claude/settings.json", "{\"mine\":true}\n"),
            ("src/main.rs", "fn main() {}\n"),
            ("README.md", "# Mine\n"),
        ];
        for (rel, text) in theirs {
            let path = repo.join(rel);
            std::fs::create_dir_all(path.parent().unwrap()).unwrap();
            std::fs::write(path, text).unwrap();
        }
        // A post-commit hook an earlier release wrote is init's own.
        std::fs::write(repo.join(BOTTLING_HOOK_REL), former_hook_script(BOTTLING_HOOK_REL).unwrap()).unwrap();
        let wrote = install(repo, 4343, "shop").unwrap();
        for (rel, text) in theirs {
            assert_eq!(std::fs::read_to_string(repo.join(rel)).unwrap(), text, "{rel} is the person's");
        }
        assert!(wrote.iter().any(|w| matches!(w, Wrote::Skipped { path, .. } if path.ends_with(HOOK_REL))), "the person's hook is left with a note");
        assert_eq!(std::fs::read_to_string(repo.join(BOTTLING_HOOK_REL)).unwrap(), bottling_hook_script(), "its own is rewritten");
        for harness in &HARNESSES {
            for (rel, text) in SKILL_FILES {
                assert_eq!(std::fs::read_to_string(repo.join(format!("{}/{rel}", harness.skill_dir))).unwrap(), text);
            }
        }
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
        let repo_only = ["bacon", "web/", "npm ", "npx ", "crates/", "Grimoire's own repository", "Tailwind", "Playwright", "cargo install"];
        for (name, text) in [("SKILL.md", SKILL_MD), ("contracts.md", CONTRACTS_MD), ("changes.md", CHANGES_MD), ("guidance-section.md", GUIDANCE_MD)] {
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
    /// {"test": "the-guidance-says-only-what-the-check-does", "checks": ["crates/cli/src/init#sends-to-the-rites", "crates/cli/src/init#rest-of-file-is-the-projects", "crates/cli/src/init#check-claim-is-only-unconnected"]}
    /// ```
    #[test]
    fn the_guidance_says_only_what_the_check_does() {
        assert!(GUIDANCE_MD.contains("the pre-commit hook finds what is left unconnected"), "the section says what the check does");
        // How the work is done is the rites': the section sends every session there, and says the rest of the file is the project's own.
        for harness in &HARNESSES {
            let text = section(harness).split_whitespace().collect::<Vec<_>>().join(" ");
            for phrase in ["in the rites, the grimoire's second root", "read them before working", "never in this file", "the rest of the file is the project's own"] {
                assert!(text.contains(phrase), "{}: {phrase:?} in {text}", harness.guidance);
            }
        }
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
        for (name, text) in [("SKILL.md", SKILL_MD), ("contracts.md", CONTRACTS_MD), ("changes.md", CHANGES_MD), ("guidance-section.md", GUIDANCE_MD)] {
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
        for s in ["brew a ticket", "write the spec", "finish a ticket", "inscribe a ticket", "resolve a gap or question", "adopt existing code"] {
            assert!(SKILL_MD.contains(&format!("## Sequence: {s}")), "the skill has no `{s}` sequence");
        }
        let adopt = &SKILL_MD[SKILL_MD.find("## Sequence: adopt existing code").unwrap_or(0)..];
        assert!(adopt.contains("record_decision") && adopt.contains("Never run\n`scaffold`"), "adopt records provenance and never scaffolds");
        assert!(adopt.contains("### Tests") && adopt.contains("`untested`") && adopt.contains("req:<slug>"), "adopt covers tests and constraints");
        let resolve = &SKILL_MD[SKILL_MD.find("## Sequence: resolve a gap or question").unwrap_or(0)..];
        assert!(resolve.contains("do not resolve") && resolve.contains("relies_on"), "resolve leaves the decision to the person and files the change after it");
        // An adoption is recorded with the flag that marks its requirement described, not decided.
        let adopt = SKILL_MD.split("## Sequence: adopt existing code").nth(1).expect("an adopt sequence");
        assert!(adopt.contains("`record_decision` with `adopted: true`"), "the adopt sequence sets the flag");
    }

    /// # Test contract
    ///
    /// ```json
    /// {"test": "the-skill-judges-the-recipes-due-before-closing", "checks": ["crates/cli/src/init#skill-judges-recipes", "req:recipe-in-tickets"]}
    /// ```
    #[test]
    fn the_skill_judges_the_recipes_due_before_closing() {
        let start = SKILL_MD.split("## Sequence: brew a ticket").nth(1).and_then(|s| s.split("## Sequence:").next()).expect("a brew sequence");
        assert!(start.contains("`recipes`") && start.contains("`due`"), "the brew reads the recipes resting on what the ticket touches");
        let finish = SKILL_MD.split("## Sequence: finish a ticket").nth(1).and_then(|s| s.split("## Sequence:").next()).expect("a finish sequence");
        let (judge, close) = (finish.find("Judge the recipes").expect("a step judging the recipes"), finish.find("`close_ticket {").expect("a close step"));
        assert!(judge < close, "recipes are judged before the close");
        let step = &finish[judge..close];
        assert!(step.contains("judge_recipe") && step.contains("\"held\"") && step.contains("\"updated\"") && step.contains("due-recipe"), "{step}");
        assert!(step.contains("after the last change"), "a later edit makes it due again");
        assert!(step.contains("named as `due`"), "a recipe judged already is not judged again");
    }

    /// # Test contract
    ///
    /// ```json
    /// {"test": "the-skill-says-how-to-name-a-checkout", "checks": ["crates/cli/src/init#skill-checkout"]}
    /// ```
    #[test]
    fn the_skill_says_how_to_name_a_checkout() {
        let flat = SKILL_MD.split_whitespace().collect::<Vec<_>>().join(" ");
        assert!(flat.contains("Pass `checkout`, your working directory as an absolute path, with every call"), "the skill says to name the checkout");
        assert!(
            flat.contains("Changed instructions reach your MCP client when it reconnects")
                && flat.contains("a new tool or a changed field may reach it only in a new session")
                && flat.contains("call over HTTP"),
            "the skill says a reconnect may not bring changed tools"
        );
    }

    /// # Test contract
    ///
    /// ```json
    /// {"test": "the-skill-says-a-branch-holds-one-ticket", "checks": ["crates/cli/src/init#branch-holds-one-ticket", "crates/cli/src/init#bottled-with-bottle-once-merged", "crates/cli/src/init#helpers-split-into-branches", "crates/cli/src/init#found-work-is-its-own-ticket", "crates/cli/src/init#brew-answers-waiting-never-refuses", "crates/cli/src/init#brew-sequence-bottle-first-or-move"]}
    /// ```
    #[test]
    fn the_skill_says_a_branch_holds_one_ticket() {
        let flat = SKILL_MD.split_whitespace().collect::<Vec<_>>().join(" ");
        assert!(
            flat.contains("**A branch holds one ticket** until that ticket is bottled")
                && flat.contains("bottled with `grimoire bottle`, which on a team is once the branch has merged to the main line"),
            "the rule"
        );
        assert!(flat.contains("split into branches of the ticket's branch") && flat.contains("merges back into the ticket's branch, which alone is bottled"), "helpers");
        assert!(flat.contains("Work found along the way is inscribed as a ticket of its own"), "found work");
        assert!(flat.contains("Nothing refuses a second ticket: `brew_ticket` answers in `waiting`"), "a warning, never a refusal");
        let start = &flat[flat.find("## Sequence: brew a ticket").expect("a brew sequence")..];
        assert!(start.contains("If it answers `waiting`") && start.contains("bottle that one first or move this work to a new branch"), "the brew sequence says what to do");
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
    /// {"test": "leaves-foreign-hook-alone", "checks": ["crates/cli/src/init#existing-hook-left-alone"]}
    /// ```
    #[test]
    fn leaves_foreign_hook_alone() {
        let tmp = temp();
        let repo = tmp.path();
        std::fs::create_dir_all(repo.join(".git").join("hooks")).unwrap();
        std::fs::write(repo.join(HOOK_REL), "#!/bin/sh\nnpm test\n").unwrap();
        let wrote = install(repo, 4242, "shop").unwrap();
        assert!(wrote.iter().any(|w| matches!(w, Wrote::Skipped { .. })));
        assert_eq!(std::fs::read_to_string(repo.join(HOOK_REL)).unwrap(), "#!/bin/sh\nnpm test\n");
    }

    #[test]
    fn no_git_is_a_note_not_an_error() {
        let tmp = temp();
        let repo = tmp.path();
        let wrote = install(repo, 4242, "shop").unwrap();
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
