//! # Module contract
//!
//! ```json
//! {"purpose":"Write Grimoire's marked section into a guidance file, and take it out again, the person's text kept to the byte.","criteria":[{"id":"replace-or-append","statement":"with_guidance replaces what lies between the markers, or appends the section after one line ending of the file's own kind, written in the file's line endings, leaving every byte outside the markers as it was."},{"id":"cut-out","statement":"without_guidance cuts the section out only when its marker lines form exactly one pair, with the one line ending with_guidance put before it, so the person's text, line endings and a byte-order mark come back to the byte; markers that are not one pair are refused with the reason, and a marker mentioned inside a line is not a marker line."}]}
//! ```

use super::eol_of;

pub const MARK_START: &str = "<!-- grimoire -->";
pub const MARK_END: &str = "<!-- /grimoire -->";

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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::init::{section, HARNESSES};

    /// # Test contract
    ///
    /// ```json
    /// {"test": "guidance-replaces-only-its-own-section", "checks": ["crates/cli/src/init/guidance#replace-or-append", "crates/cli/src/init#guidance-section"]}
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
    /// {"test": "a-section-is-taken-out-byte-for-byte", "checks": ["crates/cli/src/init/guidance#cut-out"]}
    /// ```
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
}
