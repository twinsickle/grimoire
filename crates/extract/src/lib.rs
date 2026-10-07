//! # Module contract
//!
//! ```json
//! {"purpose":"Read a repository's source and produce its files, declarations, imports and module contracts as values.","criteria":[{"id":"read-only","statement":"The extractor reads the paths it is given and writes nothing anywhere."},{"id":"per-file","statement":"Extraction is per file, so one changed file is re-extracted without touching the others."},{"id":"gitignore-respected","statement":"Walking a repository skips whatever its .gitignore files ignore, so build output is never parsed."},{"id":"nested-checkouts-skipped","statement":"Walking a repository stops at a folder below its root that is a checkout of its own, one holding a .git file or folder or a .grimoire.toml, and skips hidden files and folders; reads answers the same for one repository path, true only for a file an adapter handles that is in no hidden folder, is not hidden itself and lies in no such nested checkout, so a single re-extraction reads a file only when the walk would; is_boundary is true for a .git or a .grimoire.toml in a folder below the root, the marker of a nested checkout, and for nothing inside a .git folder."},{"id":"digest","statement":"Each file's text is fingerprinted as it is read, so a change to any byte changes the file's fingerprint and its module's."},{"id":"recipes-read","statement":"Every file whose name ends .recipe.md is read as a recipe whatever languages the extractor is built for, by the same walk and under the same ignore, hidden and nested-checkout rules as source, handles and reads answering for it as for source; extract_text gives any handled file's record from text already read."},{"id":"line-count","statement":"Each file's lines are counted as it is read, only a line feed ending a line, beside the lines of it its adapter tells are test code."},{"id":"tiers","statement":"Every supported language is read at one tier, the highest the build has: an adapter's own, 2 for one that resolves imports and 1 for one that reads declarations alone, else 0 for a language in the comment-syntax table; a language with an adapter is never read by the table, and the supported languages are exactly those with a tier."}]}
//! ```

pub mod comments;
pub mod contract;
pub mod java;
pub mod recipe;
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
    /// How many of the file's lines are test code the adapter can tell apart.
    pub test_lines: u32,
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
pub(crate) fn strip_slash_comment(raw: &str) -> String {
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
    /// The tier this adapter reads its language at (req:extract-tiers): 2 when
    /// it resolves imports to modules, 1 when it reads declarations alone, 0
    /// when it reads comments alone.
    fn tier(&self) -> u8;
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
    /// Declarations and imports of the file at a repository-relative path.
    /// The module id is enough for most languages; an adapter whose reading
    /// needs the file itself, its extension or the directory an index file
    /// sits in, reads it here.
    fn parse_file(&self, source: &str, rel_path: &str) -> Parsed {
        self.parse(source, &self.module_id(rel_path))
    }
}

/// Extract one file whose contents are already read.
pub fn extract_source(adapter: &dyn Adapter, rel_path: &str, source: &str) -> FileExtraction {
    let module = adapter.module_id(rel_path);
    let parsed = adapter.parse_file(source, rel_path);
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
        decls: parsed.decls.into_iter().map(|d| Decl { file: rel_path.to_string(), ..d }).collect(),
        imports: parsed.imports,
        expects_contract: adapter.expects_contract(rel_path),
        digest: grimoire_core::fingerprint::of_text(source),
        tests,
        test_errors,
        contract,
        contract_line,
        lines: u32::try_from(source.lines().count()).unwrap_or(u32::MAX),
        test_lines: parsed.test_lines,
        recipe: None,
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

/// The languages this build has an adapter of their own for, each with its
/// tier. A language here is never read by the comment-syntax table.
const ADAPTERS: &[(&str, u8)] = &[("rust", 2), ("typescript", 2), ("javascript", 2), ("java", 2)];

/// The languages this build reads, each with the tier it reads it at
/// (req:extract-tiers): those with an adapter of their own, then those read by
/// their comments alone, in the order [`Extractor::for_languages`] recognises
/// them. What `status` reports as `supported_languages` and `tiers`.
pub fn supported_languages() -> Vec<(&'static str, u8)> {
    let mut out: Vec<(&'static str, u8)> = ADAPTERS.to_vec();
    for s in comments::TABLE {
        if !out.iter().any(|(l, _)| *l == s.language) {
            out.push((s.language, 0));
        }
    }
    out
}

/// A repository root with its adapters.
pub struct Extractor {
    root: PathBuf,
    adapters: Vec<Box<dyn Adapter>>,
}

/// The adapter for one language name, or `None` when this build reads it at
/// no tier. An adapter of the language's own comes first; the comment-syntax
/// table is the fallback.
fn adapter_named(language: &str, root: &Path) -> Option<Box<dyn Adapter>> {
    match language {
        "rust" => Some(Box::new(rust::RustAdapter::new(root))),
        "typescript" => Some(Box::new(typescript::TypeScriptAdapter::new(root))),
        "javascript" => Some(Box::new(typescript::TypeScriptAdapter::of(typescript::Dialect::JavaScript, root))),
        "java" => Some(Box::new(java::JavaAdapter::new(root))),
        _ => comments::syntax_of(language).map(|s| Box::new(comments::CommentAdapter::new(s)) as Box<dyn Adapter>),
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
    /// mismatch through [`supported_languages`].
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

    /// Whether a path is one this extractor would parse: a recipe file, whatever the languages,
    /// or a file an adapter handles.
    pub fn handles(&self, rel_path: &str) -> bool {
        recipe::is_recipe_path(rel_path) || self.adapter_for(rel_path).is_some()
    }

    /// The record of a handled file whose text is already read: its recipe, or its source's
    /// extraction; `None` for a path this extractor does not handle.
    pub fn extract_text(&self, rel_path: &str, text: &str) -> Option<FileExtraction> {
        if recipe::is_recipe_path(rel_path) {
            return Some(recipe::extract(&self.root, rel_path, text));
        }
        self.adapter_for(rel_path).map(|a| extract_source(a, rel_path, text))
    }

    /// Whether this repository path marks a checkout nested below the root, a `.git` or a
    /// `.grimoire.toml` in a folder beneath it: one appearing or going moves where the
    /// repository's code ends, so the whole of it is read again.
    pub fn is_boundary(&self, rel_path: &str) -> bool {
        let parts: Vec<&str> = rel_path.split('/').filter(|p| !p.is_empty()).collect();
        parts.len() > 1 && matches!(parts.last(), Some(&".git") | Some(&".grimoire.toml")) && !parts[..parts.len() - 1].contains(&".git")
    }

    /// Whether reading the whole repository would read this repository path: a file an adapter
    /// handles, hidden nowhere along its path and in no checkout nested below the root
    /// (req:extract-nested-checkouts-skipped). Asked of a single changed file, so it is read
    /// only when the walk would read it.
    pub fn reads(&self, rel_path: &str) -> bool {
        if !self.handles(rel_path) {
            return false;
        }
        let parts: Vec<&str> = rel_path.split('/').filter(|p| !p.is_empty()).collect();
        if parts.iter().any(|p| p.starts_with('.')) {
            return false;
        }
        let mut dir = self.root.clone();
        for part in &parts[..parts.len().saturating_sub(1)] {
            dir.push(part);
            if is_checkout(&dir) {
                return false;
            }
        }
        true
    }

    /// Every handled file under the root, honouring `.gitignore`, sorted.
    pub fn walk(&self) -> Result<Vec<String>, ExtractError> {
        let mut out = Vec::new();
        // A folder below the root that is a checkout of its own is that checkout's code, not this one's.
        let walker = ignore::WalkBuilder::new(&self.root)
            .hidden(true)
            .git_ignore(true)
            .git_exclude(true)
            .git_global(true)
            .require_git(false)
            .filter_entry(|e| e.depth() == 0 || !(e.file_type().is_some_and(|t| t.is_dir()) && is_checkout(e.path())))
            .build();
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
        if !self.handles(rel_path) {
            return Err(ExtractError::Unhandled(rel_path.to_string()));
        }
        let full = self.root.join(rel_path.replace('/', std::path::MAIN_SEPARATOR_STR));
        let bytes = std::fs::read(&full).map_err(|e| ExtractError::Read { path: rel_path.to_string(), source: e })?;
        let source = String::from_utf8(bytes).map_err(|_| ExtractError::NotUtf8(rel_path.to_string()))?;
        self.extract_text(rel_path, &source).ok_or_else(|| ExtractError::Unhandled(rel_path.to_string()))
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

/// Whether a folder is a checkout of its own: a git repository, a worktree or a submodule, or a
/// folder a Grimoire project file names a grimoire for.
pub(crate) fn is_checkout(dir: &Path) -> bool {
    dir.join(".git").exists() || dir.join(".grimoire.toml").exists()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// # Test contract
    ///
    /// ```json
    /// {"test": "a-checkout-ends-where-another-begins", "checks": ["crates/extract/src#nested-checkouts-skipped", "req:extract-nested-checkouts-skipped"]}
    /// ```
    #[test]
    fn a_checkout_ends_where_another_begins() {
        let dir = tempfile::Builder::new().prefix("grimoire-extract-").tempdir().unwrap();
        let root = dir.path();
        let file = |rel: &str| {
            let p = root.join(rel);
            std::fs::create_dir_all(p.parent().unwrap()).unwrap();
            std::fs::write(p, "pub fn f() {}\n").unwrap();
        };
        std::fs::write(root.join("Cargo.toml"), "[package]\nname = \"r\"\nversion = \"0.1.0\"\n").unwrap();
        file("src/lib.rs");
        // A worktree kept inside, a submodule, a folder another grimoire names, and a hidden folder.
        file("worktrees/wt/src/lib.rs");
        std::fs::write(root.join("worktrees/wt/.git"), "gitdir: elsewhere\n").unwrap();
        file("vendor/sub/src/lib.rs");
        std::fs::create_dir_all(root.join("vendor/sub/.git")).unwrap();
        file("tools/other/src/lib.rs");
        std::fs::write(root.join("tools/other/.grimoire.toml"), "store = \"other\"\n").unwrap();
        file(".claude/worktrees/agent/src/lib.rs");
        let e = Extractor::for_languages(root, &["rust".to_string()]);
        assert_eq!(e.walk().unwrap(), ["src/lib.rs"], "the checkout's own code alone");
        // A single changed file is read only when the walk would read it.
        assert!(e.reads("src/lib.rs"));
        for rel in ["worktrees/wt/src/lib.rs", "vendor/sub/src/lib.rs", "tools/other/src/lib.rs", ".claude/worktrees/agent/src/lib.rs", "Cargo.toml"] {
            assert!(!e.reads(rel), "{rel}");
        }
        // A nested checkout's marker is a boundary; the root's own, and anything inside a .git folder, are not.
        assert!(e.is_boundary("worktrees/wt/.git") && e.is_boundary("tools/other/.grimoire.toml"));
        assert!(!e.is_boundary(".grimoire.toml") && !e.is_boundary("vendor/sub/.git/index") && !e.is_boundary("src/lib.rs"));
    }

    /// # Test contract
    ///
    /// ```json
    /// {"test": "a-private-declaration-in-a-hash-file-keeps-its-file", "checks": ["crates/core/src/model#kinds", "crates/core/src/query/source#module-source"]}
    /// ```
    #[test]
    fn a_private_declaration_in_a_hash_file_keeps_its_file() {
        use grimoire_core::graph::Node;
        use grimoire_core::{Extraction, Graph, Reference, Store};
        let a = typescript::TypeScriptAdapter::new(Path::new("."));
        let src = "/**
 * # Module contract
 *
 * ```json
 * {\"purpose\":\"Hold a hash.\",\"criteria\":[{\"id\":\"kept\",\"statement\":\"It holds.\"}]}
 * ```
 */
export class Box {
  #secret(): number {
    return 1;
  }
}
";
        let x = extract_source(&a, "src/a#b.ts", src);
        let secret = x.decls.iter().find(|d| d.path.contains("#secret")).unwrap_or_else(|| panic!("the private method is a declaration: {:?}", x.decls));
        assert_eq!(secret.file, "src/a#b.ts", "extraction sets the file");
        let g = Graph::build(&Store::default(), &Extraction::from_files(std::slice::from_ref(&x)), &[]);
        let decl = Reference::decl("src/a#b.ts", &secret.path);
        assert!(matches!(g.get(&decl), Some(Node::Decl(d)) if d.file == "src/a#b.ts"), "the graph's node carries it");
        let loc = g.source_of(&decl).expect("found");
        assert_eq!((loc.file.as_str(), loc.line), ("src/a#b.ts", secret.line));
        assert!(g.contains(&Reference::criterion(&x.module, "kept")), "the module {} keeps its criterion", x.module);
    }

    /// # Test contract
    ///
    /// ```json
    /// {"test": "each-language-is-read-at-its-tier", "checks": ["crates/extract/src#tiers", "req:extract-tiers"]}
    /// ```
    #[test]
    fn each_language_is_read_at_its_tier() {
        let root = Path::new(".");
        let supported = supported_languages();
        for (l, tier) in &supported {
            let a = adapter_named(l, root).unwrap_or_else(|| panic!("{l} is listed as supported but nothing reads it"));
            assert_eq!((a.language(), a.tier()), (*l, *tier));
        }
        let tier_of = |l: &str| supported.iter().find(|(s, _)| *s == l).map(|(_, t)| *t);
        assert_eq!((tier_of("rust"), tier_of("typescript"), tier_of("javascript"), tier_of("java")), (Some(2), Some(2), Some(2), Some(2)), "the adapters read declarations and imports");
        for l in ["html", "htl", "xml", "css", "scss"] {
            assert_eq!(tier_of(l), Some(0), "{l} is read by its comments");
        }
        assert_eq!(tier_of("cobol"), None);
        assert!(adapter_named("cobol", root).is_none());
        // Every language the table knows is supported, and none with an adapter of its own is read by the table.
        assert_eq!(supported.len(), ADAPTERS.len() + comments::TABLE.iter().filter(|s| !ADAPTERS.iter().any(|(l, _)| *l == s.language)).count());
        assert!(ADAPTERS.iter().all(|(l, t)| *t > 0 && adapter_named(l, root).is_some_and(|a| a.tier() == *t)));
        let x = Extractor::for_languages(root, &["rust".to_string(), "cobol".to_string(), "css".to_string(), "typescript".to_string()]);
        assert_eq!(x.languages(), vec!["rust", "css", "typescript"]);
    }

    /// A repository of the given files in a fresh folder, removed when dropped.
    fn temp_repo(files: &[(&str, &[u8])]) -> tempfile::TempDir {
        let dir = tempfile::Builder::new().prefix("grimoire-extract-").tempdir().unwrap();
        for (rel, bytes) in files {
            let p = dir.path().join(rel);
            std::fs::create_dir_all(p.parent().unwrap()).unwrap();
            std::fs::write(p, bytes).unwrap();
        }
        dir
    }

    /// # Test contract
    ///
    /// ```json
    /// {"test": "recipes-are-read-whatever-the-languages", "checks": ["crates/extract/src#recipes-read", "req:recipe-read"]}
    /// ```
    #[test]
    fn recipes_are_read_whatever_the_languages() {
        let recipe = b"# Quest\n\n## Recipe\n\n```json\n{\"id\":\"find-the-key\",\"kind\":\"quest\",\"files\":[\"levels/one.txt\"]}\n```\n";
        let tmp = temp_repo(&[
            (".gitignore", b"out/\n"),
            ("docs/quest.recipe.md", recipe),
            ("out/old.recipe.md", recipe),
            (".hidden/h.recipe.md", recipe),
            ("nested/.grimoire.toml", b"store = \"x\"\n"),
            ("nested/n.recipe.md", recipe),
            ("docs/README.md", b"# Recipe\n"),
            ("src/lib.rs", b"pub fn a() {}\n"),
        ]);
        // No language at all: the recipe is read; what .gitignore, a hidden folder and a nested checkout keep out stays out.
        let x = Extractor::for_languages(tmp.path(), &[]);
        assert_eq!(x.walk().unwrap(), ["docs/quest.recipe.md"]);
        assert!(x.reads("docs/quest.recipe.md") && !x.reads("nested/n.recipe.md") && !x.reads(".hidden/h.recipe.md") && !x.reads("docs/README.md"));
        let (files, errors) = x.extract_all().unwrap();
        assert!(errors.is_empty());
        let r = files[0].recipe.clone().expect("a recipe file").expect("in the format");
        assert_eq!((r.id.as_str(), r.title.as_str(), r.missing.clone()), ("find-the-key", "Quest", vec!["levels/one.txt".to_string()]));
        // With a language, the source beside it.
        let both = Extractor::for_languages(tmp.path(), &["rust".to_string()]);
        assert_eq!(both.walk().unwrap(), ["docs/quest.recipe.md", "src/lib.rs"]);
        assert_eq!(both.extract_text("docs/quest.recipe.md", std::str::from_utf8(recipe).unwrap()), Some(files[0].clone()));
        assert!(both.extract_text("docs/README.md", "# Recipe\n").is_none());
    }

    /// # Test contract
    ///
    /// ```json
    /// {"test": "the-walk-skips-what-gitignore-ignores", "checks": ["crates/extract/src#gitignore-respected"]}
    /// ```
    #[test]
    fn the_walk_skips_what_gitignore_ignores() {
        let tmp = temp_repo(&[(".gitignore", b"target/\n"), ("src/lib.rs", b"pub fn a() {}\n"), ("target/debug/build/out.rs", b"pub fn generated() {}\n")]);
        let dir = tmp.path();
        let x = Extractor::for_languages(dir, &["rust".to_string()]);
        assert_eq!(x.walk().unwrap(), vec!["src/lib.rs".to_string()], "build output is never parsed, with or without git");
    }

    /// # Test contract
    ///
    /// ```json
    /// {"test": "each-file-is-fingerprinted", "checks": ["crates/extract/src#digest"]}
    /// ```
    #[test]
    fn each_file_is_fingerprinted_as_read() {
        let tmp = temp_repo(&[("src/lib.rs", b"pub fn a() {}\n")]);
        let dir = tmp.path();
        let x = Extractor::for_languages(dir, &["rust".to_string()]);
        let first = x.extract_path("src/lib.rs").unwrap().digest;
        assert_eq!(first, grimoire_core::fingerprint::of_text("pub fn a() {}\n"));
        std::fs::write(dir.join("src").join("lib.rs"), "pub fn a() { }\n").unwrap();
        assert_ne!(x.extract_path("src/lib.rs").unwrap().digest, first, "one byte changes it");
    }

    /// # Test contract
    ///
    /// ```json
    /// {"test": "each-file-is-counted-with-its-tests-apart", "checks": ["crates/extract/src#line-count"]}
    /// ```
    #[test]
    fn each_file_is_counted_with_its_tests_apart() {
        let text = "pub fn a() {}\r\npub fn b() {}\n\n#[cfg(test)]\nmod tests {\n    #[test]\n    fn t() {}\n}\n";
        let tmp = temp_repo(&[("src/lib.rs", text.as_bytes())]);
        let x = Extractor::for_languages(tmp.path(), &["rust".to_string()]);
        let f = x.extract_path("src/lib.rs").unwrap();
        assert_eq!((f.lines, f.test_lines), (8, 5), "a CR before a line feed ends no extra line");
        std::fs::write(
            tmp.path().join("src").join("lib.rs"),
            "pub fn a() {}
",
        )
        .unwrap();
        assert_eq!(x.extract_path("src/lib.rs").unwrap().lines, 1, "counted again as read");
    }

    /// # Test contract
    ///
    /// ```json
    /// {"test": "one-file-is-extracted-alone", "checks": ["crates/extract/src#per-file"]}
    /// ```
    #[test]
    fn one_file_is_extracted_alone() {
        let tmp = temp_repo(&[("src/a.rs", b"pub fn a() {}\n"), ("src/b.rs", &[0xff, 0xfe, 0x00])]);
        let dir = tmp.path();
        let x = Extractor::for_languages(dir, &["rust".to_string()]);
        let a = x.extract_path("src/a.rs").unwrap();
        assert_eq!(a.path, "src/a.rs");
        assert!(a.decls.iter().any(|d| d.path == "a"));
        // A broken neighbour does not stop the rest: it is an error beside the results.
        let (files, errors) = x.extract_all().unwrap();
        assert_eq!(files.len(), 1);
        assert_eq!(errors.len(), 1);
    }

    /// # Test contract
    ///
    /// ```json
    /// {"test": "a-call-is-not-an-edge", "checks": ["req:extract-no-guessed-calls"]}
    /// ```
    #[test]
    fn a_call_between_functions_makes_no_edge() {
        let tmp = temp_repo(&[("src/lib.rs", b"pub mod a;\npub mod b;\n"), ("src/a.rs", b"pub fn run() { crate::b::run(); }\n"), ("src/b.rs", b"pub fn run() {}\n")]);
        let dir = tmp.path();
        let x = Extractor::for_languages(dir, &["rust".to_string()]);
        let (files, _) = x.extract_all().unwrap();
        let g = grimoire_core::graph::Graph::build(&grimoire_core::Store::default(), &grimoire_core::Extraction::from_files(&files), &[]);
        let decl_to_decl = g.edges.iter().filter(|e| e.from.kind == grimoire_core::model::Kind::Decl && e.to.kind == grimoire_core::model::Kind::Decl).count();
        assert_eq!(decl_to_decl, 0, "a.rs calls b's run, and no edge says so");
    }

    /// # Test contract
    ///
    /// ```json
    /// {"test": "extraction-writes-nothing", "checks": ["crates/extract/src#read-only"]}
    /// ```
    #[test]
    fn extraction_writes_nothing() {
        let tmp = temp_repo(&[("src/lib.rs", b"pub fn a() {}\n"), ("src/b.rs", b"pub fn b() {}\n")]);
        let dir = tmp.path();
        let snapshot = || {
            // Every path, and each file's modification time. A directory's own time is
            // left out: NTFS updates it lazily, after files already written.
            let mut v: Vec<(PathBuf, Option<std::time::SystemTime>)> = ignore::WalkBuilder::new(dir)
                .hidden(false)
                .build()
                .flatten()
                .map(|e| {
                    let file = e.file_type().is_some_and(|t| t.is_file());
                    (e.path().to_path_buf(), file.then(|| e.metadata().unwrap().modified().unwrap()))
                })
                .collect();
            v.sort();
            v
        };
        let before = snapshot();
        let x = Extractor::for_languages(dir, &["rust".to_string()]);
        x.extract_all().unwrap();
        x.extract_path("src/b.rs").unwrap();
        assert_eq!(snapshot(), before, "nothing created or modified");
    }
}
