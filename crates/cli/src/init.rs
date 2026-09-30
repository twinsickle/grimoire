//! # Module contract
//!
//! ```json
//! {"purpose":"Install Grimoire's integration into a repository for Claude Code and Codex alike, and remove it again: the project file naming the store, the agent skill with its references in each harness's place, the always-loaded guidance section, the MCP entry pointing at the daemon in each harness's configuration, and a pre-commit hook running the check.","criteria":[{"id":"skill-sequences","statement":"The embedded skill holds a sequence to start, specify, finish and file a ticket, to resolve a gap or question with the person, and to adopt existing code and its tests, each under its own heading."},{"id":"listed-writes","statement":"Every path init writes or would have written, with what a written path still needs of the person, and every path uninstall removes, changes or leaves, is returned, so each command can print exactly what it did."},{"id":"merge-not-clobber","statement":"An existing .mcp.json keeps its other servers; an existing .codex/config.toml keeps every byte but the grimoire server's url value, or init's table appended after it, including comments, line endings and a byte-order mark; an existing pre-commit hook is left alone with a note; a file init cannot merge into is refused before anything is written; only Grimoire's own files are overwritten."},{"id":"every-harness","statement":"init writes the same embedded skill and references to .claude/skills/grimoire/ and .agents/skills/grimoire/, and the grimoire server at the daemon's URL to both .mcp.json and .codex/config.toml, always, with no flag or detection."},{"id":"codex-trust","statement":"init writes no agent configuration outside the repository, and returns .codex/config.toml with the note that Codex reads it only once the project is trusted in the person's own Codex configuration."},{"id":"skill-is-the-source","statement":"The installed skill and its reference files are the ones in this repository's .claude/skills, embedded at build time, so there is one copy to maintain."},{"id":"guidance-section","statement":"A marked section between <!-- grimoire --> and <!-- /grimoire --> is written into CLAUDE.md and AGENTS.md, replacing a previous section and leaving everything outside the markers as it was, so an agent that never triggers the skill still learns the project is managed; each file's section names its own harness's copy of the skill, CLAUDE.md the one under .claude/skills and AGENTS.md the one under .agents/skills."},{"id":"project-agnostic","statement":"The embedded skill, references and guidance section name no project: a test fails the build if any of them mentions Grimoire's own crates, web package, npm scripts or dev loop, since they are installed verbatim into every repository."},{"id":"uninstall","statement":"uninstall plans every path of the integration before changing any, then removes both harnesses' skill files and their folders only when left empty; Grimoire's section from CLAUDE.md and AGENTS.md only when its marker lines form exactly one pair, cut out byte for byte so line endings, a byte-order mark and the person's text survive, the file removed only when nothing else is left; the grimoire entry from .mcp.json with the other servers and keys kept; the grimoire server from .codex/config.toml only when it is as init wrote it, last in the file, with every other byte kept, otherwise leaving it with a note; and the pre-commit hook only when it is init's script unedited. Every path gets one result: removed, changed, left with why, or absent; a failure part way returns what was already done."}]}
//! ```

use std::path::{Path, PathBuf};

use serde_json::{json, Value};
use toml_edit::Item;

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
pub const MARK_START: &str = "<!-- grimoire -->";
pub const MARK_END: &str = "<!-- /grimoire -->";

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
    #[error("{0} is not valid TOML: {1}")]
    BadToml(PathBuf, String),
}

fn write(path: &Path, text: &str) -> Result<(), InitError> {
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir).map_err(|e| InitError::Io(dir.to_path_buf(), e))?;
    }
    std::fs::write(path, text).map_err(|e| InitError::Io(path.to_path_buf(), e))
}

/// `existing` with Grimoire's marked `section` replaced, or appended when
/// absent. Everything outside the markers is untouched.
pub fn with_guidance(existing: &str, section: &str) -> String {
    let eol = eol_of(existing);
    let section = section.replace('\n', eol);
    if let (Some(s), Some(e)) = (existing.find(MARK_START), existing.find(MARK_END)) {
        if e > s {
            let after = &existing[e + MARK_END.len()..];
            return format!("{}{}{}", &existing[..s], section, after);
        }
    }
    // Appended after one line ending of the file's own kind, the person's text kept to the byte.
    if existing.is_empty() {
        format!("{section}{eol}")
    } else {
        format!("{existing}{eol}{section}{eol}")
    }
}

/// `existing` with Grimoire's marked section taken out, byte for byte: the
/// markers must be whole lines forming exactly one pair, and everything
/// outside them (line endings, a byte-order mark, the person's text) is kept,
/// less the one line ending `with_guidance` put before the section.
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
        // The section was last: drop the one line ending init put before it.
        before.strip_suffix(eol).unwrap_or(before).to_string()
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

/// A TOML text with its byte-order mark set aside, so it parses and is put back.
fn split_bom(text: &str) -> (&str, &str) {
    text.strip_prefix('\u{feff}').map_or(("", text), |rest| ("\u{feff}", rest))
}

/// The line ending a text uses, for the lines init adds to it.
fn eol_of(text: &str) -> &'static str {
    if text.contains("\r\n") {
        "\r\n"
    } else {
        "\n"
    }
}

/// The table init writes into .codex/config.toml, and its first line.
const CODEX_HEADER: &str = "[mcp_servers.grimoire]";

/// The grimoire server's url in a parsed .codex/config.toml, if it has one.
fn codex_url(body: &str) -> Option<String> {
    let doc = toml_edit::Document::parse(body).ok()?;
    doc.get("mcp_servers")?.get("grimoire")?.get("url")?.as_str().map(str::to_string)
}

/// `existing` with the grimoire server pointing at the daemon. The file is
/// spliced, never re-serialised, so every byte outside what init adds or
/// sets stays as it was: with no grimoire server, init's table is appended
/// after one line ending; with one, only its url's value is replaced, and
/// the table keeps its place, its other keys and its comments. A shape init
/// cannot set a url in is refused, never overwritten.
pub fn with_codex_server(existing: &str, port: u16) -> Result<String, String> {
    let (bom, body) = split_bom(existing);
    let doc = toml_edit::Document::parse(body).map_err(|e| e.to_string())?;
    let url = format!("http://127.0.0.1:{port}/mcp");
    let text = match doc.get("mcp_servers") {
        None => None,
        Some(Item::Table(servers)) => match servers.get("grimoire") {
            None => None,
            Some(Item::Table(ours)) => {
                let Some(span) = ours.get("url").and_then(Item::as_value).filter(|v| v.is_str()).and_then(toml_edit::Value::span) else {
                    return Err("its grimoire server has no url string for init to set; set it by hand".into());
                };
                Some(format!("{bom}{}\"{url}\"{}", &body[..span.start], &body[span.end..]))
            }
            Some(_) => return Err("mcp_servers.grimoire is not a table; set its url by hand".into()),
        },
        Some(_) => return Err("mcp_servers is not a table init can add to; add [mcp_servers.grimoire] by hand".into()),
    };
    let text = text.unwrap_or_else(|| {
        let eol = eol_of(body);
        let block = format!("{CODEX_HEADER}{eol}url = \"{url}\"{eol}");
        if body.is_empty() {
            format!("{bom}{block}")
        } else {
            format!("{bom}{body}{eol}{block}")
        }
    });
    // What was written must read back as what was meant.
    if codex_url(split_bom(&text).1).as_deref() != Some(url.as_str()) {
        return Err("init could not add its server to this file safely; add [mcp_servers.grimoire] by hand".into());
    }
    Ok(text)
}

/// `existing` without the grimoire server, every other byte as it was.
/// Only a server exactly as init writes it, last in the file, is taken
/// out, with the one line ending init put before it; anything else is
/// `Err` with what to do by hand. `Ok(None)` means there is none.
pub fn without_codex_server(existing: &str) -> Result<Option<String>, String> {
    let (bom, body) = split_bom(existing);
    let doc = toml_edit::Document::parse(body).map_err(|e| format!("could not be read as TOML ({e}); take out [mcp_servers.grimoire] by hand"))?;
    if !doc.get("mcp_servers").and_then(Item::as_table_like).is_some_and(|s| s.contains_key("grimoire")) {
        return Ok(None);
    }
    // init's block in the line ending it was written with, which later edits elsewhere may no longer match.
    let ours = ["\r\n", "\n"].into_iter().find_map(|eol| {
        let start = format!("{CODEX_HEADER}{eol}url = \"http://127.0.0.1:");
        let at = body.rfind(&start)?;
        let port = body[at + start.len()..].strip_suffix(eol)?.strip_suffix("/mcp\"")?;
        let whole_line = at == 0 || body[..at].ends_with('\n');
        (whole_line && !port.is_empty() && port.bytes().all(|b| b.is_ascii_digit())).then_some((at, eol))
    });
    let Some((at, eol)) = ours else {
        return Err("its grimoire server is not as init wrote it; take out [mcp_servers.grimoire] by hand".into());
    };
    let before = &body[..at];
    Ok(Some(format!("{bom}{}", before.strip_suffix(eol).unwrap_or(before))))
}

fn io(path: &Path) -> impl FnOnce(std::io::Error) -> InitError + '_ {
    move |e| InitError::Io(path.to_path_buf(), e)
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
        let again = install(&repo, 4343).unwrap();
        assert!(again.iter().all(|w| matches!(w, Wrote::Written(_) | Wrote::Noted { .. })));
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
        let claude = section(&HARNESSES[0]);
        let after = with_guidance(before, &claude);
        assert!(after.starts_with("intro\n\n<!-- grimoire -->"));
        assert!(after.ends_with("<!-- /grimoire -->\n\nafter\n"));
        assert!(!after.contains("old stuff"));
        assert!(after.contains("## Grimoire"));
        assert_eq!(with_guidance("", &claude), format!("{claude}\n"));
        // Appending keeps the person's text to the byte: trailing spaces, blank lines, no final
        // line ending, Windows line endings; and uninstall gives exactly it back.
        for own in ["Use tabs.  \n\n\n", "Use tabs.", "Use tabs.\r\n", "\u{feff}Use tabs.\r\n\r\n"] {
            let with = with_guidance(own, &claude);
            assert!(with.starts_with(own), "{with:?}");
            if own.contains("\r\n") {
                assert!(!with.replace("\r\n", "").contains('\n'), "the section takes the file's line endings: {with:?}");
            }
            assert_eq!(without_guidance(&with).unwrap().as_deref(), Some(own));
        }
    }

    /// # Test contract
    ///
    /// ```json
    /// {"test": "each-guidance-file-names-its-own-skill", "checks": ["crates/cli/src/init#guidance-section"]}
    /// ```
    #[test]
    fn each_guidance_file_names_its_own_skill() {
        let repo = temp();
        install(&repo, 4242).unwrap();
        let claude = std::fs::read_to_string(repo.join("CLAUDE.md")).unwrap();
        let agents = std::fs::read_to_string(repo.join("AGENTS.md")).unwrap();
        assert!(claude.contains("`.claude/skills/grimoire/SKILL.md`") && !claude.contains(".agents/"), "{claude}");
        assert!(agents.contains("`.agents/skills/grimoire/SKILL.md`") && !agents.contains(".claude/"), "{agents}");
        assert!(!claude.contains("{skill}") && !agents.contains("{skill}"), "the placeholder is filled");
        assert!(GUIDANCE_MD.matches("{skill}").count() == 1, "the source names the skill once, by placeholder");
        std::fs::remove_dir_all(&repo).unwrap();
    }

    /// # Test contract
    ///
    /// ```json
    /// {"test": "every-harness-gets-the-skill-and-the-tools", "checks": ["crates/cli/src/init#every-harness", "crates/cli/src/init#codex-trust"]}
    /// ```
    #[test]
    fn every_harness_gets_the_skill_and_the_tools() {
        let repo = temp();
        let wrote = install(&repo, 4343).unwrap();
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
        assert!(paths.iter().all(|p| p.starts_with(&repo)), "{paths:?}");
        assert!(wrote.iter().any(|w| matches!(w, Wrote::Noted { path, note } if path.ends_with(CODEX_REL) && note.contains("trusted"))));
        std::fs::remove_dir_all(&repo).unwrap();
    }

    /// # Test contract
    ///
    /// ```json
    /// {"test": "a-codex-config-of-the-projects-own-is-kept", "checks": ["crates/cli/src/init#merge-not-clobber", "crates/cli/src/init#uninstall"]}
    /// ```
    #[test]
    fn a_codex_config_of_the_projects_own_is_kept() {
        let own = "# the team's Codex settings\nmodel = \"o4\"\n\n[mcp_servers.docs]\ncommand = \"docs-mcp\" # local\n";
        let once = with_codex_server(own, 4242).unwrap();
        assert!(once.starts_with(own), "the person's text is untouched: {once}");
        assert_eq!(once, format!("{own}\n[mcp_servers.grimoire]\nurl = \"http://127.0.0.1:4242/mcp\"\n"));
        // Again, on another port: one server, set in place.
        let twice = with_codex_server(&once, 4343).unwrap();
        assert_eq!(twice, once.replace("4242", "4343"));
        // A grimoire server the person wrote keeps its place, its keys and its comments; only the url's value is set.
        let extended = format!("# my server\n[mcp_servers.grimoire]\nurl = \"old\" # keep me\nenabled = false\n\n{own}");
        let set = with_codex_server(&extended, 4242).unwrap();
        assert_eq!(set, extended.replace("\"old\"", "\"http://127.0.0.1:4242/mcp\""));
        // and uninstall leaves it, with a note, since it is not as init wrote it.
        assert!(without_codex_server(&set).unwrap_err().contains("by hand"));
        // Dotted keys are a table too.
        let dotted = "mcp_servers.grimoire.url = \"old\"\n";
        assert_eq!(with_codex_server(dotted, 4242).unwrap(), "mcp_servers.grimoire.url = \"http://127.0.0.1:4242/mcp\"\n");
        // Taken out byte for byte, with Windows line endings and a byte-order mark too.
        assert_eq!(without_codex_server(&once).unwrap().as_deref(), Some(own));
        let crlf = format!("\u{feff}{}", own.replace('\n', "\r\n"));
        assert_eq!(without_codex_server(&with_codex_server(&crlf, 4242).unwrap()).unwrap().as_deref(), Some(crlf.as_str()));
        assert_eq!(without_codex_server(&with_codex_server("", 4242).unwrap()).unwrap().as_deref(), Some(""));
        // Mixed line endings, a bare LF inside a string of a CRLF file, and no final line ending: every byte kept.
        for odd in ["a = 1\r\nb = 2\n", "s = \"\"\"\nx\r\ny\"\"\"\r\nb = 1\r\n", "model = \"o4\""] {
            let with = with_codex_server(odd, 4242).unwrap();
            assert!(with.starts_with(odd), "{with:?}");
            assert_eq!(without_codex_server(&with).unwrap().as_deref(), Some(odd), "{with:?}");
        }
        assert_eq!(without_codex_server(own).unwrap(), None);
        // A server of the person's own in init's shape, last in the file, is not init's: its url is not one init writes.
        let theirs = format!("{own}\n[mcp_servers.grimoire]\nurl = \"https://example.internal/mcp\"\n");
        assert!(without_codex_server(&theirs).unwrap_err().contains("by hand"));
        // init's block is still found after a line elsewhere is changed to another line ending.
        let edited = once.replacen("model = \"o4\"\n", "model = \"o4\"\r\n", 1);
        assert_eq!(without_codex_server(&edited).unwrap(), Some(own.replacen("model = \"o4\"\n", "model = \"o4\"\r\n", 1)));
        // Shapes init cannot set a url in are refused, not overwritten.
        for refused in ["mcp_servers = 1\n", "mcp_servers = { docs = { url = \"x\" } }\n", "[mcp_servers]\ngrimoire = \"x\"\n", "[mcp_servers.grimoire]\ncommand = \"x\"\n"] {
            assert!(with_codex_server(refused, 4242).is_err(), "{refused:?}");
        }
    }

    /// # Test contract
    ///
    /// ```json
    /// {"test": "a-refused-file-leaves-the-repository-as-it-was", "checks": ["crates/cli/src/init#merge-not-clobber", "crates/cli/src/init#listed-writes"]}
    /// ```
    #[test]
    fn a_refused_file_leaves_the_repository_as_it_was() {
        let repo = temp();
        std::fs::create_dir_all(repo.join(".codex")).unwrap();
        std::fs::write(repo.join(CODEX_REL), "mcp_servers = { docs = { url = \"x\" } }\n").unwrap();
        let before = walk(&repo);
        assert!(matches!(install(&repo, 4242), Err(InitError::BadToml(..))));
        assert_eq!(walk(&repo), before, "nothing is written before every file's text is known");
        std::fs::remove_dir_all(&repo).unwrap();
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
        let codex_own = "# ours
model = \"o4\"
";
        std::fs::create_dir_all(repo.join(".codex")).unwrap();
        std::fs::write(repo.join(CODEX_REL), codex_own).unwrap();
        install(&repo, 4242).unwrap();
        let undone = uninstall(&repo);
        assert_eq!(std::fs::read_to_string(repo.join("CLAUDE.md")).unwrap(), "# My project\n\nUse tabs.\n", "a person's text is back exactly");
        assert!(!repo.join("AGENTS.md").exists(), "a file init created holds nothing else, so it goes");
        let mcp: Value = serde_json::from_str(&std::fs::read_to_string(repo.join(".mcp.json")).unwrap()).unwrap();
        assert_eq!(mcp, json!({ "mcpServers": { "other": { "type": "stdio", "command": "x" } }, "z": 1 }), "other servers and keys are kept");
        assert!(!repo.join(HOOK_REL).exists(), "init's own hook goes");
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
            Undone::Removed(repo.join(HOOK_REL)),
            Undone::Removed(repo.join(".claude/skills/grimoire/references")),
            Undone::Removed(repo.join(".claude/skills/grimoire")),
            Undone::Removed(repo.join(".agents/skills/grimoire/references")),
            Undone::Removed(repo.join(".agents/skills/grimoire")),
            Undone::Removed(repo.join(".agents/skills")),
            Undone::Removed(repo.join(".agents")),
        ]);
        assert_eq!(undone, expected);
        // A second run: every path accounted for, nothing done.
        let again = uninstall(&repo);
        assert_eq!(again.len(), 11, "{again:?}");
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
        assert!(!repo.join(".agents").exists());
        assert!(!repo.join(".codex").exists());
    }

    #[test]
    fn a_section_is_taken_out_byte_for_byte() {
        let section = section(&HARNESSES[0]);
        let with_guidance = |existing: &str| with_guidance(existing, &section);
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
