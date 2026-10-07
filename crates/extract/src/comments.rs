//! # Module contract
//!
//! ```json
//! {"purpose":"Read a language by its comments alone, at tier 0: the comment-syntax table of the languages Grimoire knows no more of, and the adapter that hands their comments to the language-neutral contract parser.","criteria":[{"id":"table","statement":"The comment-syntax table names html (.html, .htm; <!-- -->), htl (.html, .htl; <!--/* */--> tried before <!-- -->), xml (.xml; <!-- -->), css (.css; /* */) and scss (.scss; // and /* */), each with the extensions it handles, its line marker if it has one and its block markers in the order they are tried."},{"id":"identity","statement":"A file read by its comments is its own module, its id the repository-relative path with its extension kept, so files of two languages never join one module."},{"id":"comments","statement":"Every comment the table's markers delimit is handed over with its markers stripped, one text line per file line, and the file line it starts on, line comments on consecutive lines merged into one run; comments are found by their markers alone, and a line marker counts only at the start of a line or after whitespace."},{"id":"leading","statement":"The leading comment is the first comment in the file with nothing before it but whitespace and, for a markup language, the prolog: an XML declaration, processing instructions and a doctype, its internal subset included; a file whose first content is anything else has none."},{"id":"no-code","statement":"A file read by its comments yields no declarations and no imports."},{"id":"coverage","statement":"Every file a language read by its comments handles is expected to carry a contract, the project's contract scope narrowing which."}]}
//! ```

use crate::{Adapter, Parsed};

/// How one language writes its comments.
#[derive(Debug)]
pub struct CommentSyntax {
    pub language: &'static str,
    /// File name endings this language handles, the dot included.
    pub extensions: &'static [&'static str],
    /// The marker that comments out the rest of a line.
    pub line: Option<&'static str>,
    /// Opening and closing markers of a block comment, tried in order at each position.
    pub block: &'static [(&'static str, &'static str)],
    /// Whether an XML declaration or a doctype may come before the leading comment.
    pub markup: bool,
}

const MARKUP_COMMENT: (&str, &str) = ("<!--", "-->");
const C_BLOCK: (&str, &str) = ("/*", "*/");

/// The languages read at tier 0, by their comments alone (req:extract-comment-syntax).
pub const TABLE: &[CommentSyntax] = &[
    CommentSyntax { language: "html", extensions: &[".html", ".htm"], line: None, block: &[MARKUP_COMMENT], markup: true },
    // HTL is AEM's template language: its own comment is `<!--/* */-->`, never sent to the page.
    CommentSyntax { language: "htl", extensions: &[".html", ".htl"], line: None, block: &[("<!--/*", "*/-->"), MARKUP_COMMENT], markup: true },
    CommentSyntax { language: "xml", extensions: &[".xml"], line: None, block: &[MARKUP_COMMENT], markup: true },
    CommentSyntax { language: "css", extensions: &[".css"], line: None, block: &[C_BLOCK], markup: false },
    CommentSyntax { language: "scss", extensions: &[".scss"], line: Some("//"), block: &[C_BLOCK], markup: false },
];

/// The table's entry for a language name.
pub fn syntax_of(language: &str) -> Option<&'static CommentSyntax> {
    TABLE.iter().find(|s| s.language == language)
}

/// A language read by its comments alone.
pub struct CommentAdapter {
    syntax: &'static CommentSyntax,
}

impl CommentAdapter {
    pub fn new(syntax: &'static CommentSyntax) -> Self {
        CommentAdapter { syntax }
    }

    /// Every comment in `source`, markers stripped, with the 1-based line it
    /// starts on and the byte offset it starts at.
    fn scan(&self, source: &str) -> Vec<(String, u32, usize)> {
        let s = self.syntax;
        let bytes = source.as_bytes();
        let mut out: Vec<(String, u32, usize)> = Vec::new();
        // The line the last line comment sat on, so a run on consecutive lines merges.
        let mut last_line_comment: Option<u32> = None;
        let mut line: u32 = 1;
        // A byte-order mark is no content: what follows it starts the file.
        let start = if source.starts_with('\u{feff}') { '\u{feff}'.len_utf8() } else { 0 };
        let mut i = start;
        while i < bytes.len() {
            let rest = &source[i..];
            if let Some((open, close)) = s.block.iter().find(|(open, _)| rest.starts_with(open)) {
                let body_start = i + open.len();
                let (body, end) = match source[body_start..].find(close) {
                    Some(at) => (&source[body_start..body_start + at], body_start + at + close.len()),
                    None => (&source[body_start..], source.len()),
                };
                out.push((strip_block(body, open.ends_with("/*")), line, i));
                line += source[i..end].matches('\n').count() as u32;
                last_line_comment = None;
                i = end;
                continue;
            }
            if let Some(marker) = s.line.filter(|m| rest.starts_with(m) && (i == start || bytes[i - 1].is_ascii_whitespace())) {
                let end = rest.find('\n').map_or(source.len(), |at| i + at);
                let text = source[i + marker.len()..end].trim_end();
                let text = text.strip_prefix(' ').unwrap_or(text);
                match out.last_mut() {
                    Some(prev) if last_line_comment.is_some_and(|l| l + 1 == line) => {
                        prev.0.push_str(text);
                        prev.0.push('\n');
                    }
                    _ => out.push((format!("{text}\n"), line, i)),
                }
                last_line_comment = Some(line);
                i = end;
                continue;
            }
            if bytes[i] == b'\n' {
                line += 1;
            }
            // Step a whole character, so a slice never splits one.
            i += rest.chars().next().map_or(1, char::len_utf8);
        }
        out
    }
}

/// A block comment's text, one line per source line, each line trimmed; in a
/// `/* */` block, a leading `*` such as a `/** */` block's is taken off with
/// the space after it, and in a markup comment it is text.
fn strip_block(body: &str, stars: bool) -> String {
    let body = if stars { body.strip_prefix('*').unwrap_or(body) } else { body };
    let lines: Vec<&str> = body.split('\n').collect();
    // The closing line, when it holds nothing but the marker, gives no text line.
    let keep = if lines.len() > 1 && lines.last().is_some_and(|l| l.trim().is_empty()) { lines.len() - 1 } else { lines.len() };
    let mut out = String::new();
    for l in &lines[..keep] {
        let t = l.trim();
        let t = if stars { t.strip_prefix('*').map(|r| r.strip_prefix(' ').unwrap_or(r)).unwrap_or(t) } else { t };
        out.push_str(t);
        out.push('\n');
    }
    out
}

/// Where a markup file's prolog ends: past its XML declaration, processing
/// instructions and doctype, and the whitespace around them.
fn after_prolog(source: &str) -> usize {
    let mut i = 0;
    loop {
        let rest = &source[i..];
        let trimmed = rest.trim_start();
        let at = i + (rest.len() - trimmed.len());
        let end = if trimmed.starts_with("<?") {
            trimmed.find("?>").map(|e| e + 2)
        } else if trimmed.get(..9).is_some_and(|p| p.eq_ignore_ascii_case("<!doctype")) && trimmed[9..].starts_with(|c: char| c.is_ascii_whitespace() || c == '>') {
            doctype_end(trimmed)
        } else {
            return at;
        };
        match end {
            Some(end) => i = at + end,
            None => return at,
        }
    }
}

/// The byte just past a doctype's closing `>`, skipping a `>` inside quotes,
/// and anything inside an internal subset's `[ ]`, a comment there included.
fn doctype_end(text: &str) -> Option<usize> {
    let mut quote: Option<char> = None;
    let mut subset = false;
    let mut at = 0;
    while let Some(c) = text[at..].chars().next() {
        let next = at + c.len_utf8();
        match (quote, c) {
            (Some(q), c) if c == q => quote = None,
            (Some(_), _) => {}
            (None, '<') if subset && text[at..].starts_with("<!--") => {
                at += 4 + text[at + 4..].find("-->")? + 3;
                continue;
            }
            (None, '"' | '\'') => quote = Some(c),
            (None, '[') => subset = true,
            (None, ']') => subset = false,
            (None, '>') if !subset => return Some(next),
            _ => {}
        }
        at = next;
    }
    None
}

impl Adapter for CommentAdapter {
    fn language(&self) -> &'static str {
        self.syntax.language
    }

    fn tier(&self) -> u8 {
        0
    }

    fn handles(&self, rel_path: &str) -> bool {
        self.syntax.extensions.iter().any(|e| rel_path.len() > e.len() && rel_path.to_ascii_lowercase().ends_with(e))
    }

    fn module_id(&self, rel_path: &str) -> String {
        rel_path.to_string()
    }

    fn expects_contract(&self, _rel_path: &str) -> bool {
        true
    }

    fn leading_doc(&self, source: &str) -> Option<(String, u32)> {
        let source = source.strip_prefix('\u{feff}').unwrap_or(source);
        let start = if self.syntax.markup { after_prolog(source) } else { source.len() - source.trim_start().len() };
        // Scanned from the prolog's end, so a comment inside a doctype's subset is not taken for it.
        let (text, line, at) = self.scan(&source[start..]).into_iter().next()?;
        let before = source[..start].matches('\n').count() as u32;
        (at == 0).then_some((text, line + before))
    }

    fn parse(&self, source: &str, _module_id: &str) -> Parsed {
        Parsed { comments: self.scan(source).into_iter().map(|(text, line, _)| (text, line)).collect(), ..Parsed::default() }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::extract_source;

    fn adapter(language: &str) -> CommentAdapter {
        CommentAdapter::new(syntax_of(language).unwrap())
    }

    fn comments(language: &str, source: &str) -> Vec<(String, u32)> {
        adapter(language).parse(source, "m").comments
    }

    /// # Test contract
    ///
    /// ```json
    /// {"test": "the-table-names-the-first-languages", "checks": ["crates/extract/src/comments#table"]}
    /// ```
    #[test]
    fn the_table_names_the_first_languages() {
        let names: Vec<&str> = TABLE.iter().map(|s| s.language).collect();
        assert_eq!(names, ["html", "htl", "xml", "css", "scss"]);
        assert!(adapter("html").handles("a/b.html") && adapter("html").handles("a/b.htm"));
        assert!(adapter("htl").handles("apps/button/button.html") && adapter("htl").handles("x.htl"));
        assert!(adapter("xml").handles("pom.xml") && adapter("css").handles("a.css") && adapter("scss").handles("a.scss"));
        assert!(!adapter("css").handles("a.scss") && !adapter("html").handles("a.xml"));
        // HTL's own comment comes off whole, inner markers and all.
        assert_eq!(comments("htl", "<!--/* note */-->\n"), [("note\n".to_string(), 1)]);
        assert_eq!(comments("html", "<!--/* note */-->\n"), [("/* note */\n".to_string(), 1)], "HTML knows no HTL comment");
        // Only SCSS has a line marker.
        assert_eq!(comments("scss", "// one\n/* two */\n"), [("one\n".to_string(), 1), ("two\n".to_string(), 2)]);
        assert_eq!(comments("css", "// one\n/* two */\n"), [("two\n".to_string(), 2)]);
    }

    /// # Test contract
    ///
    /// ```json
    /// {"test": "a-file-is-its-own-module-by-its-whole-path", "checks": ["crates/extract/src/comments#identity"]}
    /// ```
    #[test]
    fn a_file_is_its_own_module_by_its_whole_path() {
        assert_eq!(adapter("html").module_id("components/button/button.html"), "components/button/button.html");
        assert_eq!(adapter("css").module_id("components/button/button.css"), "components/button/button.css");
    }

    /// # Test contract
    ///
    /// ```json
    /// {"test": "every-comment-is-handed-over", "checks": ["crates/extract/src/comments#comments"]}
    /// ```
    #[test]
    fn every_comment_is_handed_over() {
        let html = "<p>a</p>\n<!--\n  first\n  second\n-->\n<p>b</p> <!-- one line -->\n";
        assert_eq!(comments("html", html), [("\nfirst\nsecond\n".to_string(), 2), ("one line\n".to_string(), 6)]);
        // A run of line comments on consecutive lines is one, as in every language; a gap ends it; code before a marker does not hide it.
        let scss = "// a\n// b\n\n// c\n\n.x { color: red; } // d\n";
        assert_eq!(comments("scss", scss), [("a\nb\n".to_string(), 1), ("c\n".to_string(), 4), ("d\n".to_string(), 6)]);
        // An address's `//` follows no whitespace: no comment.
        assert!(comments("scss", ".x { background: url(http://example.com/a.png); }\n").is_empty());
        // A /** */ block loses its stars, one text line per file line.
        assert_eq!(comments("css", "/**\n * a\n * b\n */\n"), [("\na\nb\n".to_string(), 1)]);
        // A comment never closed runs to the end, and non-ASCII text is kept whole.
        assert_eq!(comments("css", "a { } /* é"), [("é\n".to_string(), 1)]);
        // A byte-order mark before a first line comment hides nothing.
        assert_eq!(comments("scss", "\u{feff}// first\n"), [("first\n".to_string(), 1)]);
    }

    /// # Test contract
    ///
    /// ```json
    /// {"test": "the-leading-comment-is-the-first-thing", "checks": ["crates/extract/src/comments#leading"]}
    /// ```
    #[test]
    fn the_leading_comment_is_the_first_thing() {
        assert_eq!(adapter("css").leading_doc("\n\n/* top */\na {}\n"), Some(("top\n".to_string(), 3)));
        assert_eq!(adapter("css").leading_doc("a {}\n/* not leading */\n"), None);
        let xml = "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n<!-- top -->\n<root/>\n";
        assert_eq!(adapter("xml").leading_doc(xml), Some(("top\n".to_string(), 2)));
        let html = "<!DOCTYPE html>\n<!-- top -->\n<html></html>\n";
        assert_eq!(adapter("html").leading_doc(html), Some(("top\n".to_string(), 2)));
        assert_eq!(adapter("html").leading_doc("<div></div>\n<!-- late -->\n"), None);
        // A style sheet has no prolog.
        assert_eq!(adapter("css").leading_doc("<?xml?>/* x */"), None);
        // A processing instruction and a doctype with an internal subset are prolog too.
        let xml = "<?xml version=\"1.0\"?>\n<?xml-stylesheet href=\"a.xsl\"?>\n<!DOCTYPE note [\n  <!ENTITY a \">\">\n]>\n<!-- top -->\n<note/>\n";
        assert_eq!(adapter("xml").leading_doc(xml), Some(("top\n".to_string(), 6)));
        // Content that is not ASCII is content, never a panic.
        assert_eq!(adapter("html").leading_doc("ééééé<!-- late -->"), None);
        assert_eq!(adapter("html").leading_doc("<!é"), None);
        // A comment inside the subset is the doctype's; the one after it leads.
        let xml = "<!DOCTYPE a [\n<!-- inner -->\n]>\n<!-- top -->\n<a/>\n";
        assert_eq!(adapter("xml").leading_doc(xml), Some(("top\n".to_string(), 4)));
        assert_eq!(adapter("html").leading_doc("<!doctypex>\n<!-- late -->\n"), None, "not a doctype");
        // A `]` or `>` in a comment inside the subset ends nothing.
        assert_eq!(adapter("xml").leading_doc("<!DOCTYPE a [<!-- ] > -->]><!-- top -->"), Some(("top\n".to_string(), 1)));
        // A markup comment's star is its text.
        assert_eq!(comments("html", "<!--\n * a\n-->\n"), [("\n* a\n".to_string(), 1)]);
    }

    /// # Test contract
    ///
    /// ```json
    /// {"test": "a-style-sheet-yields-its-contract-and-tests-and-no-code", "checks": ["crates/extract/src/comments#no-code", "crates/extract/src/comments#coverage", "crates/extract/src/comments#leading", "req:extract-comment-reading"]}
    /// ```
    #[test]
    fn a_style_sheet_yields_its_contract_and_tests_and_no_code() {
        let src = "/**\n * # Module contract\n *\n * ```json\n * {\"purpose\":\"Style a button.\",\"criteria\":[{\"id\":\"round\",\"statement\":\"A button is round.\"}]}\n * ```\n */\n@import \"base.css\";\n.button { border-radius: 50%; }\n\n/*\n # Test contract\n\n ```json\n {\"test\": \"round-button\", \"checks\": [\"theme/button.css#round\"]}\n ```\n*/\n";
        let x = extract_source(&adapter("css"), "theme/button.css", src);
        assert_eq!(x.module, "theme/button.css");
        assert_eq!(x.language, "css");
        let contract = x.contract.expect("a contract").expect("well formed");
        assert_eq!(contract.criteria[0].id, "round");
        assert_eq!(x.contract_line, Some(2));
        assert_eq!(x.tests.len(), 1, "{:?}", x.test_errors);
        assert_eq!((x.tests[0].name.as_str(), x.tests[0].line), ("round-button", 12));
        assert!(x.decls.is_empty() && x.imports.is_empty(), "no declarations and no imports, not even the @import");
        assert!(x.expects_contract);
        assert!(extract_source(&adapter("scss"), "anything/at/all.scss", "a {}\n").expects_contract);
    }
}
