//! # Module contract
//!
//! ```json
//! {"purpose":"Join per-file extraction results into modules with their criteria and resolved imports, without reading anything.","criteria":[{"id":"per-file-input","statement":"The input is one record per source file, so a caller can replace one file's record and re-join without re-extracting the rest."},{"id":"imports-resolve-to-known","statement":"An import resolves to the first candidate that names an existing module other than the importing one; anything else is dropped and counted."}]}
//! ```

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use crate::model::*;

/// What the extractor produces for one source file.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FileExtraction {
    /// Repository-relative, `/`-separated.
    pub path: String,
    pub language: String,
    /// Canonical module id from the language adapter.
    pub module: String,
    pub decls: Vec<Decl>,
    /// Each import as an ordered list of candidate module ids, most
    /// specific first. Resolution happens at join time against the set of
    /// modules that exist.
    pub imports: Vec<Vec<String>>,
    pub expects_contract: bool,
    /// Tests declared by `# Test contract` blocks anywhere in the file.
    #[serde(default)]
    pub tests: Vec<Test>,
    /// Test blocks that could not be read.
    #[serde(default)]
    pub test_errors: Vec<TestError>,
    /// `Some(Ok)` when a contract was found and parsed, `Some(Err)` when
    /// found and malformed, `None` when the file has none.
    pub contract: Option<Result<Contract, String>>,
    /// Line of the `# Module contract` heading, when present.
    pub contract_line: Option<u32>,
}

/// The joined view: every module and file the repository declares.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Extraction {
    pub modules: BTreeMap<String, Module>,
    pub files: BTreeMap<String, File>,
    pub unresolved_imports: usize,
    pub tests: Vec<Test>,
    pub test_errors: Vec<TestError>,
}

impl Extraction {
    pub fn from_files<'a>(files: impl IntoIterator<Item = &'a FileExtraction>) -> Extraction {
        let mut files: Vec<&FileExtraction> = files.into_iter().collect();
        files.sort_by(|a, b| a.path.cmp(&b.path));

        let mut modules: BTreeMap<String, Module> = BTreeMap::new();
        let mut raw_imports: BTreeMap<String, Vec<Vec<String>>> = BTreeMap::new();
        let mut out_files: BTreeMap<String, File> = BTreeMap::new();

        for f in &files {
            let m = modules.entry(f.module.clone()).or_insert_with(|| Module {
                id: f.module.clone(),
                files: Vec::new(),
                purpose: None,
                criteria: Vec::new(),
                imports: Vec::new(),
                expects_contract: false,
                contract_error: None,
                contract_at: None,
            });
            m.files.push(f.path.clone());
            m.expects_contract |= f.expects_contract;
            match &f.contract {
                Some(Ok(c)) if m.purpose.is_none() => {
                    m.purpose = Some(c.purpose.clone());
                    m.criteria = c
                        .criteria
                        .iter()
                        .map(|cc| Criterion { module: f.module.clone(), id: cc.id.clone(), statement: cc.statement.clone() })
                        .collect();
                    m.contract_at = Some((f.path.clone(), f.contract_line.unwrap_or(1)));
                    m.contract_error = None;
                }
                Some(Ok(_)) => {
                    // A second contract in the same module: keep the first, note the second.
                    m.contract_error.get_or_insert_with(|| format!("{} also carries a contract; only the first by path is used", f.path));
                }
                Some(Err(e)) if m.purpose.is_none() => {
                    m.contract_error = Some(format!("{}: {e}", f.path));
                    m.contract_at = Some((f.path.clone(), f.contract_line.unwrap_or(1)));
                }
                _ => {}
            }
            raw_imports.entry(f.module.clone()).or_default().extend(f.imports.iter().cloned());
            out_files.insert(
                f.path.clone(),
                File { path: f.path.clone(), module: f.module.clone(), language: f.language.clone(), decls: f.decls.clone() },
            );
        }

        for m in modules.values_mut() {
            if m.expects_contract && m.purpose.is_none() && m.contract_error.is_none() {
                m.contract_error = Some("no `# Module contract` block found".to_string());
            }
        }

        let mut unresolved = 0usize;
        let ids: Vec<String> = modules.keys().cloned().collect();
        for (module, imports) in raw_imports {
            let mut resolved: Vec<String> = Vec::new();
            for candidates in imports {
                match candidates.iter().find(|c| *c != &module && ids.binary_search(c).is_ok()) {
                    Some(hit) => {
                        if !resolved.contains(hit) {
                            resolved.push(hit.clone());
                        }
                    }
                    None => unresolved += 1,
                }
            }
            resolved.sort();
            modules.get_mut(&module).expect("module exists").imports = resolved;
        }

        let tests = files.iter().flat_map(|f| f.tests.iter().cloned()).collect();
        let test_errors = files.iter().flat_map(|f| f.test_errors.iter().cloned()).collect();
        Extraction { modules, files: out_files, unresolved_imports: unresolved, tests, test_errors }
    }

    pub fn criterion(&self, module: &str, id: &str) -> Option<&Criterion> {
        self.modules.get(module)?.criteria.iter().find(|c| c.id == id)
    }
}

#[cfg(test)]
pub(crate) mod fixtures {
    use super::*;

    pub fn contract(purpose: &str, criteria: &[(&str, &str)]) -> Option<Result<Contract, String>> {
        Some(Ok(Contract {
            purpose: purpose.into(),
            criteria: criteria.iter().map(|(id, s)| ContractCriterion { id: id.to_string(), statement: s.to_string() }).collect(),
        }))
    }

    pub fn file(path: &str, module: &str, imports: &[&[&str]], contract: Option<Result<Contract, String>>) -> FileExtraction {
        FileExtraction {
            path: path.into(),
            language: "rust".into(),
            module: module.into(),
            decls: vec![Decl { path: "run".into(), kind: "function".into(), line: 10 }],
            imports: imports.iter().map(|c| c.iter().map(|s| s.to_string()).collect()).collect(),
            expects_contract: true,
            tests: Vec::new(),
            test_errors: Vec::new(),
            contract,
            contract_line: Some(1),
        }
    }

    /// Three modules: `a/ready` imports `a/layout`; `b/unrelated` has no contract.
    /// One test, in `a/ready`, checks `guarantee`; every other criterion is untested.
    pub fn extraction() -> Extraction {
        let mut ready = file("src/a/ready.rs", "src/a/ready", &[&["src/a/layout/thing", "src/a/layout"], &["std/fs"]], contract("Decide readiness.", &[("guarantee", "Ready only with a commit."), ("reads-only", "Never writes.")]));
        ready.tests = vec![Test { file: "src/a/ready.rs".into(), name: "ready-needs-a-commit".into(), line: 20, checks: vec!["src/a/ready#guarantee".into()] }];
        let files = vec![
            ready,
            file("src/a/layout.rs", "src/a/layout", &[], contract("Lay out the workspace.", &[("stable", "Paths do not move.")])),
            file("src/b/unrelated.rs", "src/b/unrelated", &[], None),
        ];
        Extraction::from_files(&files)
    }
}

#[cfg(test)]
mod tests {
    use super::fixtures::*;

    #[test]
    fn joins_modules_and_resolves_imports() {
        let x = extraction();
        assert_eq!(x.modules.len(), 3);
        let ready = &x.modules["src/a/ready"];
        assert_eq!(ready.imports, vec!["src/a/layout".to_string()]);
        assert_eq!(ready.criteria.len(), 2);
        assert_eq!(x.unresolved_imports, 1);
        let unrelated = &x.modules["src/b/unrelated"];
        assert!(unrelated.purpose.is_none());
        assert!(unrelated.contract_error.is_some());
    }
}
