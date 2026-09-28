//! # Module contract
//!
//! ```json
//! {"purpose":"Read a repository's source and produce its files, declarations, imports and module contracts as values.","criteria":[{"id":"read-only","statement":"The extractor reads the paths it is given and writes nothing anywhere."},{"id":"per-file","statement":"Extraction is per file, so one changed file is re-extracted without touching the others."},{"id":"no-guessed-calls","statement":"The extractor reports declarations and imports it parsed and never infers a call edge by name."},{"id":"gitignore-respected","statement":"Walking a repository skips whatever its .gitignore files ignore, so build output is never parsed."}]}
//! ```

pub mod contract;
pub mod rust;
pub mod typescript;

use std::path::{Path, PathBuf};

use grimoire_core::{Decl, FileExtraction};

/// What a language adapter parses out of one file, before identity and
/// contract handling are added.
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct Parsed {
    pub decls: Vec<Decl>,
    /// Each import as candidate module ids, most specific first.
    pub imports: Vec<Vec<String>>,
    /// Every comment in the file, markers stripped, with the 1-based line it
    /// starts on; where test blocks are looked for.
    pub comments: Vec<(String, u32)>,
}

/// Every comment under `root`, in order, for a language whose comments are
/// `//` lines and `/* */` blocks. `is_comment` says which syntax nodes are
/// comments. Adjacent line comments merge into one, so a block written as a
/// run of `///` lines reads as one text.
pub(crate) fn slash_comments(root: tree_sitter::Node, src: &[u8], is_comment: fn(&str) -> bool) -> Vec<(String, u32)> {
    let mut nodes = Vec::new();
    let mut stack = vec![root];
    while let Some(n) = stack.pop() {
        if is_comment(n.kind()) {
            nodes.push(n);
            continue;
        }
        let mut cursor = n.walk();
        let children: Vec<_> = n.children(&mut cursor).collect();
        stack.extend(children.into_iter().rev());
    }
    let mut out: Vec<(String, u32)> = Vec::new();
    let mut last_line_comment_row: Option<usize> = None;
    for n in nodes {
        let raw = n.utf8_text(src).unwrap_or_default();
        let row = n.start_position().row;
        let text = strip_slash_comment(raw);
        let is_line = raw.starts_with("//");
        match out.last_mut() {
            Some(prev) if is_line && last_line_comment_row.is_some_and(|r| r + 1 == row) => prev.0.push_str(&text),
            _ => out.push((text, row as u32 + 1)),
        }
        last_line_comment_row = is_line.then_some(n.end_position().row.saturating_sub(usize::from(raw.ends_with('\n'))));
    }
    out
}

/// Comment text without `//`, `///`, `//!`, `/*`, `/**`, `*/` or a leading
/// ` * ` on each line, one line per source line.
fn strip_slash_comment(raw: &str) -> String {
    let mut out = String::new();
    if let Some(body) = raw.strip_prefix("/*") {
        let body = body.strip_suffix("*/").unwrap_or(body);
        let body = body.strip_prefix('*').unwrap_or(body);
        // One text line per source line, so a heading's line in the text plus
        // the comment's first line is its line in the file. Only the closing
        // line, when it holds nothing but `*/`, is dropped.
        let lines: Vec<&str> = body.lines().collect();
        let keep = if lines.len() > 1 && lines.last().is_some_and(|l| l.trim().is_empty()) { lines.len() - 1 } else { lines.len() };
        for line in &lines[..keep] {
            let t = line.trim_start();
            let t = t.strip_prefix('*').map(|r| r.strip_prefix(' ').unwrap_or(r)).unwrap_or(t);
            out.push_str(t.trim_end());
            out.push('\n');
        }
        return out;
    }
    for line in raw.lines() {
        let t = line.trim_start();
        let t = ["//!", "///", "//"].iter().find_map(|m| t.strip_prefix(m)).unwrap_or(t);
        out.push_str(t.strip_prefix(' ').unwrap_or(t).trim_end());
        out.push('\n');
    }
    out
}

/// The four things a language supplies, plus how to find its leading comment.
pub trait Adapter: Send + Sync {
    fn language(&self) -> &'static str;
    /// Whether this adapter handles a repository-relative path.
    fn handles(&self, rel_path: &str) -> bool;
    /// The canonical module id for a path.
    fn module_id(&self, rel_path: &str) -> String;
    /// Whether the file is expected to carry a contract.
    fn expects_contract(&self, rel_path: &str) -> bool;
    /// The file's leading documentation comment with markers stripped, and
    /// the 1-based line its first line sits on.
    fn leading_doc(&self, source: &str) -> Option<(String, u32)>;
    /// Declarations and imports.
    fn parse(&self, source: &str, module_id: &str) -> Parsed;
}

/// Extract one file whose contents are already read.
pub fn extract_source(adapter: &dyn Adapter, rel_path: &str, source: &str) -> FileExtraction {
    let module = adapter.module_id(rel_path);
    let parsed = adapter.parse(source, &module);
    let (contract, contract_line) = match adapter.leading_doc(source).and_then(|(text, line)| contract::find(&text, line)) {
        Some(found) => (Some(found.contract), Some(found.heading_line)),
        None => (None, None),
    };
    let mut tests: Vec<grimoire_core::model::Test> = Vec::new();
    let mut test_errors = Vec::new();
    for (text, line) in &parsed.comments {
        for found in contract::find_tests(text, *line) {
            let error = |reason: String| grimoire_core::model::TestError { file: rel_path.to_string(), line: found.line, reason };
            match found.test {
                Ok((name, _)) if tests.iter().any(|t| t.name == name) => test_errors.push(error(format!("test name `{name}` is used twice in this file"))),
                Ok((name, checks)) => tests.push(grimoire_core::model::Test { file: rel_path.to_string(), name, line: found.line, checks }),
                Err(reason) => test_errors.push(error(reason)),
            }
        }
    }
    FileExtraction {
        path: rel_path.to_string(),
        language: adapter.language().to_string(),
        module,
        decls: parsed.decls,
        imports: parsed.imports,
        expects_contract: adapter.expects_contract(rel_path),
        tests,
        test_errors,
        contract,
        contract_line,
    }
}

#[derive(Debug, thiserror::Error)]
pub enum ExtractError {
    #[error("{path}: {source}")]
    Read { path: String, source: std::io::Error },
    #[error("{0} is not a UTF-8 text file")]
    NotUtf8(String),
    #[error("no adapter handles {0}")]
    Unhandled(String),
    #[error("walking {0}: {1}")]
    Walk(PathBuf, ignore::Error),
}

/// The languages this build has an adapter for, in the order
/// [`Extractor::for_languages`] recognises them. What `status` reports as
/// `supported_languages`.
pub const SUPPORTED_LANGUAGES: &[&str] = &["rust", "typescript"];

/// A repository root with its adapters.
pub struct Extractor {
    root: PathBuf,
    adapters: Vec<Box<dyn Adapter>>,
}

/// The adapter for one language name, or `None` when this build has none.
/// The names here and [`SUPPORTED_LANGUAGES`] are the same list.
fn adapter_named(language: &str, root: &Path) -> Option<Box<dyn Adapter>> {
    match language {
        "rust" => Some(Box::new(rust::RustAdapter::new(root))),
        "typescript" => Some(Box::new(typescript::TypeScriptAdapter::new(root))),
        _ => None,
    }
}

/// Repository-relative, `/`-separated.
pub fn relative(root: &Path, path: &Path) -> Option<String> {
    let rel = path.strip_prefix(root).ok()?;
    let mut s = String::new();
    for c in rel.components() {
        if !s.is_empty() {
            s.push('/');
        }
        s.push_str(&c.as_os_str().to_string_lossy());
    }
    Some(s)
}

impl Extractor {
    pub fn new(root: impl Into<PathBuf>, adapters: Vec<Box<dyn Adapter>>) -> Self {
        Extractor { root: root.into(), adapters }
    }

    /// Adapters for the named languages. Unknown names are ignored so a store
    /// naming a language this build lacks still opens; `status` shows the
    /// mismatch through [`SUPPORTED_LANGUAGES`].
    pub fn for_languages(root: impl Into<PathBuf>, languages: &[String]) -> Self {
        let root: PathBuf = root.into();
        let adapters: Vec<Box<dyn Adapter>> = languages.iter().filter_map(|l| adapter_named(l, &root)).collect();
        Extractor::new(root, adapters)
    }

    /// The adapters this extractor was built with, by language name.
    pub fn languages(&self) -> Vec<&'static str> {
        self.adapters.iter().map(|a| a.language()).collect()
    }

    pub fn root(&self) -> &Path {
        &self.root
    }

    pub fn adapter_for(&self, rel_path: &str) -> Option<&dyn Adapter> {
        self.adapters.iter().find(|a| a.handles(rel_path)).map(|a| a.as_ref())
    }

    /// Whether a path is one this extractor would parse: handled by an
    /// adapter and not ignored.
    pub fn handles(&self, rel_path: &str) -> bool {
        self.adapter_for(rel_path).is_some()
    }

    /// Every handled file under the root, honouring `.gitignore`, sorted.
    pub fn walk(&self) -> Result<Vec<String>, ExtractError> {
        let mut out = Vec::new();
        let walker = ignore::WalkBuilder::new(&self.root).hidden(true).git_ignore(true).git_exclude(true).git_global(true).require_git(false).build();
        for entry in walker {
            let entry = entry.map_err(|e| ExtractError::Walk(self.root.clone(), e))?;
            if !entry.file_type().is_some_and(|t| t.is_file()) {
                continue;
            }
            if let Some(rel) = relative(&self.root, entry.path()) {
                if self.handles(&rel) {
                    out.push(rel);
                }
            }
        }
        out.sort();
        Ok(out)
    }

    /// Read and extract one repository-relative path.
    pub fn extract_path(&self, rel_path: &str) -> Result<FileExtraction, ExtractError> {
        let adapter = self.adapter_for(rel_path).ok_or_else(|| ExtractError::Unhandled(rel_path.to_string()))?;
        let full = self.root.join(rel_path.replace('/', std::path::MAIN_SEPARATOR_STR));
        let bytes = std::fs::read(&full).map_err(|e| ExtractError::Read { path: rel_path.to_string(), source: e })?;
        let source = String::from_utf8(bytes).map_err(|_| ExtractError::NotUtf8(rel_path.to_string()))?;
        Ok(extract_source(adapter, rel_path, &source))
    }

    /// Walk and extract everything. Unreadable files are skipped and
    /// returned as errors alongside the results.
    pub fn extract_all(&self) -> Result<(Vec<FileExtraction>, Vec<ExtractError>), ExtractError> {
        let mut files = Vec::new();
        let mut errors = Vec::new();
        for rel in self.walk()? {
            match self.extract_path(&rel) {
                Ok(f) => files.push(f),
                Err(e) => errors.push(e),
            }
        }
        Ok((files, errors))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn supported_languages_is_exactly_what_for_languages_accepts() {
        let root = Path::new(".");
        for l in SUPPORTED_LANGUAGES {
            let a = adapter_named(l, root).unwrap_or_else(|| panic!("{l} is listed as supported but has no adapter"));
            assert_eq!(a.language(), *l);
        }
        assert!(adapter_named("cobol", root).is_none());
        let x = Extractor::for_languages(root, &["rust".to_string(), "cobol".to_string(), "typescript".to_string()]);
        assert_eq!(x.languages(), vec!["rust", "typescript"]);
    }

    fn temp_repo(files: &[(&str, &[u8])]) -> PathBuf {
        let nanos = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_nanos()).unwrap_or(0);
        let dir = std::env::temp_dir().join(format!("grimoire-extract-{}-{nanos}", std::process::id()));
        for (rel, bytes) in files {
            let p = dir.join(rel);
            std::fs::create_dir_all(p.parent().unwrap()).unwrap();
            std::fs::write(p, bytes).unwrap();
        }
        dir
    }

    /// # Test contract
    ///
    /// ```json
    /// {"test": "the-walk-skips-what-gitignore-ignores", "checks": ["crates/extract/src#gitignore-respected"]}
    /// ```
    #[test]
    fn the_walk_skips_what_gitignore_ignores() {
        let dir = temp_repo(&[(".gitignore", b"target/\n"), ("src/lib.rs", b"pub fn a() {}\n"), ("target/debug/build/out.rs", b"pub fn generated() {}\n")]);
        let x = Extractor::for_languages(&dir, &["rust".to_string()]);
        assert_eq!(x.walk().unwrap(), vec!["src/lib.rs".to_string()], "build output is never parsed, with or without git");
        std::fs::remove_dir_all(&dir).unwrap();
    }

    /// # Test contract
    ///
    /// ```json
    /// {"test": "one-file-is-extracted-alone", "checks": ["crates/extract/src#per-file"]}
    /// ```
    #[test]
    fn one_file_is_extracted_alone() {
        let dir = temp_repo(&[("src/a.rs", b"pub fn a() {}\n"), ("src/b.rs", &[0xff, 0xfe, 0x00])]);
        let x = Extractor::for_languages(&dir, &["rust".to_string()]);
        let a = x.extract_path("src/a.rs").unwrap();
        assert_eq!(a.path, "src/a.rs");
        assert!(a.decls.iter().any(|d| d.path == "a"));
        // A broken neighbour does not stop the rest: it is an error beside the results.
        let (files, errors) = x.extract_all().unwrap();
        assert_eq!(files.len(), 1);
        assert_eq!(errors.len(), 1);
        std::fs::remove_dir_all(&dir).unwrap();
    }
}
