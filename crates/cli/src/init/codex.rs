//! # Module contract
//!
//! ```json
//! {"purpose":"Splice Grimoire's MCP server into a project's .codex/config.toml, and take it out again, every other byte as it was.","criteria":[{"id":"splice","statement":"with_codex_server changes no byte but the grimoire server's url value, or appends init's table after one line ending of the file's own kind, so comments, other tables and keys, a byte-order mark and CRLF or mixed line endings survive; it refuses a shape it cannot set a url in, an mcp_servers or grimoire entry that is not a table or a grimoire server with no url string, and reads its result back before returning it."},{"id":"own-block-only","statement":"without_codex_server takes out only init's own two lines with a loopback url, last in the file and in either line ending, with the one line ending init put before them, leaving every other byte; any other grimoire server, one the person wrote or extended included, is refused with what to do by hand, and a file with none is reported as having none."}]}
//! ```

use toml_edit::Item;

use super::eol_of;

/// A TOML text with its byte-order mark set aside, so it parses and is put back.
fn split_bom(text: &str) -> (&str, &str) {
    text.strip_prefix('\u{feff}').map_or(("", text), |rest| ("\u{feff}", rest))
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

#[cfg(test)]
mod tests {
    use super::*;

    /// # Test contract
    ///
    /// ```json
    /// {"test": "a-codex-config-of-the-projects-own-is-kept", "checks": ["crates/cli/src/init/codex#splice", "crates/cli/src/init/codex#own-block-only", "crates/cli/src/init#merge-not-clobber"]}
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
}
