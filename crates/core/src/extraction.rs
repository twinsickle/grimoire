//! # Module contract
//!
//! ```json
//! {"purpose":"Join per-file extraction results into modules with their criteria and resolved imports, without reading anything.","criteria":[{"id":"per-file-input","statement":"The input is one record per source file, so a caller can replace one file's record and re-join without re-extracting the rest."},{"id":"imports-resolve-to-known","statement":"An import resolves to the first candidate that names an existing module other than the importing one; anything else is dropped and counted."},{"id":"recipes-joined","statement":"A recipe file's record joins as its recipe, or as the reason it could not be read, in the order of the files' paths, and never as a module or a file."}]}
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
    /// The fingerprint of the file's text as read.
    #[serde(default)]
    pub digest: String,
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
    /// How many lines the file has, and how many of them its adapter tells are test code.
    #[serde(default)]
    pub lines: u32,
    #[serde(default)]
    pub test_lines: u32,
    /// For a recipe file, which is no module's, the recipe it holds or why it could not be read;
    /// `None` for a source file.
    #[serde(default)]
    pub recipe: Option<Result<Recipe, RecipeError>>,
}

/// The joined view: every module and file the repository declares.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Extraction {
    pub modules: BTreeMap<String, Module>,
    pub files: BTreeMap<String, File>,
    pub unresolved_imports: usize,
    pub tests: Vec<Test>,
    pub test_errors: Vec<TestError>,
    /// Every recipe read, by its file's path, two of one id among them.
    pub recipes: Vec<Recipe>,
    /// Recipe files that could not be read as one.
    pub recipe_errors: Vec<RecipeError>,
}

impl Extraction {
    pub fn from_files<'a>(files: impl IntoIterator<Item = &'a FileExtraction>) -> Extraction {
        let mut files: Vec<&FileExtraction> = files.into_iter().collect();
        files.sort_by(|a, b| a.path.cmp(&b.path));

        let mut modules: BTreeMap<String, Module> = BTreeMap::new();
        let mut raw_imports: BTreeMap<String, Vec<Vec<String>>> = BTreeMap::new();
        let mut out_files: BTreeMap<String, File> = BTreeMap::new();
        let mut recipes = Vec::new();
        let mut recipe_errors = Vec::new();

        for f in &files {
            // A recipe file is no module's: it joins as its recipe.
            match &f.recipe {
                Some(Ok(r)) => {
                    recipes.push(r.clone());
                    continue;
                }
                Some(Err(e)) => {
                    recipe_errors.push(e.clone());
                    continue;
                }
                None => {}
            }
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
                    m.criteria = c.criteria.iter().map(|cc| Criterion { module: f.module.clone(), id: cc.id.clone(), statement: cc.statement.clone() }).collect();
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
                File {
                    path: f.path.clone(),
                    module: f.module.clone(),
                    language: f.language.clone(),
                    decls: f.decls.clone(),
                    digest: f.digest.clone(),
                    expects_contract: f.expects_contract,
                    lines: f.lines,
                    test_lines: f.test_lines,
                },
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
        Extraction { modules, files: out_files, unresolved_imports: unresolved, tests, test_errors, recipes, recipe_errors }
    }

    pub fn criterion(&self, module: &str, id: &str) -> Option<&Criterion> {
        self.modules.get(module)?.criteria.iter().find(|c| c.id == id)
    }
}

#[cfg(test)]
pub(crate) mod fixtures {
    use super::*;

    pub fn contract(purpose: &str, criteria: &[(&str, &str)]) -> Option<Result<Contract, String>> {
        Some(Ok(Contract { purpose: purpose.into(), criteria: criteria.iter().map(|(id, s)| ContractCriterion { id: id.to_string(), statement: s.to_string() }).collect() }))
    }

    pub fn file(path: &str, module: &str, imports: &[&[&str]], contract: Option<Result<Contract, String>>) -> FileExtraction {
        FileExtraction {
            path: path.into(),
            language: "rust".into(),
            module: module.into(),
            decls: vec![Decl { path: "run".into(), kind: "function".into(), line: 10, file: path.into() }],
            imports: imports.iter().map(|c| c.iter().map(|s| s.to_string()).collect()).collect(),
            expects_contract: true,
            digest: crate::fingerprint::of_text(path),
            tests: Vec::new(),
            test_errors: Vec::new(),
            contract,
            contract_line: Some(1),
            lines: 40,
            test_lines: 0,
            recipe: None,
        }
    }

    /// A recipe file's record: `button` of kind `component`, with one ingredient and the given uses and files.
    pub fn recipe_file(path: &str, id: &str, kind: &str, uses: &[&str], files: &[&str], missing: &[&str]) -> FileExtraction {
        let recipe = Recipe {
            id: id.into(),
            kind: kind.into(),
            title: id.to_uppercase(),
            text: format!("How to use {id}."),
            ingredients: vec![Ingredient { recipe: id.into(), id: "label".into(), says: "The text shown.".into() }],
            files: files.iter().map(|s| s.to_string()).collect(),
            uses: uses.iter().map(|s| s.to_string()).collect(),
            file: path.into(),
            line: 3,
            missing: missing.iter().map(|s| s.to_string()).collect(),
        };
        FileExtraction {
            path: path.into(),
            language: "recipe".into(),
            module: String::new(),
            decls: vec![],
            imports: vec![],
            expects_contract: false,
            digest: crate::fingerprint::of_text(path),
            tests: vec![],
            test_errors: vec![],
            contract: None,
            contract_line: None,
            lines: 0,
            test_lines: 0,
            recipe: Some(Ok(recipe)),
        }
    }

    /// Three modules: `a/ready` imports `a/layout`; `b/unrelated` has no contract.
    /// One test, in `a/ready`, checks `guarantee`; every other criterion is untested.
    pub fn extraction() -> Extraction {
        Extraction::from_files(&extraction_files())
    }

    /// The per-file records `extraction` joins.
    pub fn extraction_files() -> Vec<FileExtraction> {
        let mut ready = file(
            "src/a/ready.rs",
            "src/a/ready",
            &[&["src/a/layout/thing", "src/a/layout"], &["std/fs"]],
            contract("Decide readiness.", &[("guarantee", "Ready only with a commit."), ("reads-only", "Never writes.")]),
        );
        ready.tests = vec![Test { file: "src/a/ready.rs".into(), name: "ready-needs-a-commit".into(), line: 20, checks: vec!["src/a/ready#guarantee".into()] }];
        let files = vec![
            ready,
            file("src/a/layout.rs", "src/a/layout", &[], contract("Lay out the workspace.", &[("stable", "Paths do not move.")])),
            file("src/b/unrelated.rs", "src/b/unrelated", &[], None),
        ];
        files
    }
}

#[cfg(test)]
mod tests {
    use super::fixtures::*;
    use super::Extraction;

    /// # Test contract
    ///
    /// ```json
    /// {"test": "one-file-is-replaced-and-rejoined", "checks": ["crates/core/src/extraction#per-file-input"]}
    /// ```
    #[test]
    fn one_file_is_replaced_and_rejoined() {
        let mut files = extraction_files();
        let before = Extraction::from_files(&files);
        // Replace one file's record, as a watcher would after a save, and join again.
        let layout = files.iter_mut().find(|f| f.path == "src/a/layout.rs").expect("fixture has layout");
        *layout = file("src/a/layout.rs", "src/a/layout", &[], contract("Lay out the workspace.", &[("stable", "Paths do not move."), ("new", "A second promise.")]));
        let after = Extraction::from_files(&files);
        assert_eq!(after.modules["src/a/layout"].criteria.len(), 2);
        assert_eq!(after.modules["src/a/ready"], before.modules["src/a/ready"], "the other records are used as they were");
        assert_eq!(after.modules["src/b/unrelated"], before.modules["src/b/unrelated"]);
    }

    /// # Test contract
    ///
    /// ```json
    /// {"test": "joins-modules-and-resolves-imports", "checks": ["crates/core/src/extraction#imports-resolve-to-known"]}
    /// ```
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

    /// # Test contract
    ///
    /// ```json
    /// {"test": "a-recipe-file-joins-as-its-recipe", "checks": ["crates/core/src/extraction#recipes-joined"]}
    /// ```
    #[test]
    fn a_recipe_file_joins_as_its_recipe() {
        let mut files = extraction_files();
        files.push(recipe_file("z/button.recipe.md", "button", "component", &[], &[], &[]));
        files.push(recipe_file("a/button.recipe.md", "button", "component", &[], &[], &[]));
        let mut broken = recipe_file("docs/broken.recipe.md", "x", "component", &[], &[], &[]);
        broken.recipe = Some(Err(crate::model::RecipeError { file: "docs/broken.recipe.md".into(), line: 2, reason: "no block".into() }));
        files.push(broken);
        let x = Extraction::from_files(&files);
        assert_eq!(x.recipes.iter().map(|r| r.file.as_str()).collect::<Vec<_>>(), ["a/button.recipe.md", "z/button.recipe.md"], "in the order of the files' paths");
        assert_eq!(x.recipe_errors.len(), 1);
        assert_eq!(x.modules.len(), 3, "no module of a recipe file");
        assert!(!x.files.keys().any(|f| f.ends_with(".recipe.md")), "no file of one either");
    }
}
