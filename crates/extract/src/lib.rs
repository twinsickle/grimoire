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
    FileExtraction {
        path: rel_path.to_string(),
        language: adapter.language().to_string(),
        module,
        decls: parsed.decls,
        imports: parsed.imports,
        expects_contract: adapter.expects_contract(rel_path),
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

/// A repository root with its adapters.
pub struct Extractor {
    root: PathBuf,
    adapters: Vec<Box<dyn Adapter>>,
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
    /// naming a language this build lacks still opens.
    pub fn for_languages(root: impl Into<PathBuf>, languages: &[String]) -> Self {
        let root: PathBuf = root.into();
        let mut adapters: Vec<Box<dyn Adapter>> = Vec::new();
        for l in languages {
            match l.as_str() {
                "rust" => adapters.push(Box::new(rust::RustAdapter::new(&root))),
                "typescript" => adapters.push(Box::new(typescript::TypeScriptAdapter::new(&root))),
                _ => {}
            }
        }
        Extractor::new(root, adapters)
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
