//! # Module contract
//!
//! ```json
//! {"purpose":"Define the records the extraction derives from the repository on every read and never stores: module contracts as written in source, modules, criteria, files, declarations, tests and recipes.","criteria":[{"id":"derived-module","statement":"A module is an id, its files, a purpose, none when its contract is missing or malformed, its criteria and its resolved imports."},{"id":"derived-criterion","statement":"A criterion is its module, id and statement."},{"id":"derived-file","statement":"A file is its path, module, language and declarations."},{"id":"derived-declaration","statement":"A declaration is its path within the file, kind, line and file."},{"id":"derived-contract-grammar","statement":"A module contract as written in source is exactly a purpose and criteria, each an id and a statement, a block holding any other field refused."}]}
//! ```

use serde::{Deserialize, Serialize};

/// A module contract as it appears in source.
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Contract {
    pub purpose: String,
    pub criteria: Vec<ContractCriterion>,
}

#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ContractCriterion {
    pub id: String,
    pub statement: String,
}

#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Criterion {
    pub module: String,
    pub id: String,
    pub statement: String,
}

#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Module {
    pub id: String,
    pub files: Vec<String>,
    pub purpose: Option<String>,
    pub criteria: Vec<Criterion>,
    pub imports: Vec<String>,
    /// Whether the adapter expects this module to carry a contract.
    pub expects_contract: bool,
    /// Why the contract is missing or malformed, when `purpose` is `None`
    /// and one was expected.
    pub contract_error: Option<String>,
    /// The file and line of the `# Module contract` heading.
    pub contract_at: Option<(String, u32)>,
}

#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct File {
    pub path: String,
    pub module: String,
    pub language: String,
    pub decls: Vec<Decl>,
    /// The fingerprint of the file's text as read.
    #[serde(default)]
    pub digest: String,
    // Whether its adapter expects this file itself to carry a contract: a test file folded into a module does not.
    #[serde(default)]
    pub expects_contract: bool,
    // How many lines the file has, and how many of them its adapter tells are test code.
    #[serde(default)]
    pub lines: u32,
    #[serde(default)]
    pub test_lines: u32,
}

/// A test, from a `# Test contract` block: its name, where the block is, and
/// the criteria it checks as `module id#criterion id`.
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Test {
    pub file: String,
    pub name: String,
    pub line: u32,
    pub checks: Vec<String>,
}

/// A `# Test contract` block that could not be read, and why.
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TestError {
    pub file: String,
    pub line: u32,
    pub reason: String,
}

/// A recipe, read from a `.recipe.md` file: something the project offers the people who build with it.
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Recipe {
    pub id: String,
    pub kind: String,
    // The prose's own first heading, or the id.
    pub title: String,
    // The prose for people, Markdown, without the title's heading and the recipe block.
    pub text: String,
    pub ingredients: Vec<Ingredient>,
    // The implementation it represents: repository paths, each a file or a folder.
    pub files: Vec<String>,
    // The ids of the recipes it uses.
    pub uses: Vec<String>,
    // The `.recipe.md` file it is written in, and the line of its `Recipe` heading.
    pub file: String,
    pub line: u32,
    // The paths of `files` not found in the checkout when it was read.
    pub missing: Vec<String>,
    // The fingerprint of the recipe file and of every file its `files` name, as read: what a judgement is matched against.
    pub digest: String,
}

// One part of a recipe: an id and what it says.
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Ingredient {
    pub recipe: String,
    pub id: String,
    pub says: String,
}

// A `.recipe.md` file that could not be read as a recipe, and why.
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RecipeError {
    pub file: String,
    pub line: u32,
    pub reason: String,
}

#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Decl {
    pub path: String,
    pub kind: String,
    pub line: u32,
    /// The file it is declared in, set where the file is known, so no one
    /// recovers it from the reference.
    #[serde(default)]
    pub file: String,
}

#[cfg(test)]
mod tests {
    use super::*;

    /// # Test contract
    ///
    /// ```json
    /// {"test": "derived-records-carry-what-the-store-page-names", "checks": ["crates/core/src/model/derived#derived-module", "crates/core/src/model/derived#derived-criterion", "crates/core/src/model/derived#derived-file", "crates/core/src/model/derived#derived-declaration", "crates/core/src/model/derived#derived-contract-grammar"]}
    /// ```
    #[test]
    fn derived_records_carry_what_the_store_page_names() {
        // A contract block is a purpose and criteria of id and statement, and nothing else.
        let block = r#"{"purpose":"P","criteria":[{"id":"a","statement":"S"}]}"#;
        let c: Contract = serde_json::from_str(block).unwrap();
        assert_eq!((c.purpose.as_str(), c.criteria[0].id.as_str(), c.criteria[0].statement.as_str()), ("P", "a", "S"));
        assert!(serde_json::from_str::<Contract>(r#"{"purpose":"P","criteria":[{"id":"a","statement":"S"}],"owner":"x"}"#).is_err(), "a field beyond the grammar is refused");
        assert!(serde_json::from_str::<Contract>(r#"{"purpose":"P","criteria":[{"id":"a","statement":"S","why":"x"}]}"#).is_err(), "and so is one on a criterion");
        // A module, a criterion, a file and a declaration carry the fields the Store page names.
        let criterion = Criterion { module: "src/a".into(), id: "a".into(), statement: "S".into() };
        let module = Module {
            id: "src/a".into(),
            files: vec!["src/a.rs".into()],
            purpose: None,
            criteria: vec![criterion],
            imports: vec!["src/b".into()],
            expects_contract: true,
            contract_error: Some("invalid contract JSON".into()),
            contract_at: None,
        };
        let keys = |v: serde_json::Value| v.as_object().unwrap().keys().cloned().collect::<std::collections::BTreeSet<_>>().into_iter().collect::<Vec<_>>();
        let m = serde_json::to_value(&module).unwrap();
        for field in ["id", "files", "purpose", "criteria", "imports"] {
            assert!(keys(m.clone()).contains(&field.to_string()), "a module carries its {field}");
        }
        assert!(m["purpose"].is_null(), "no purpose when its contract is malformed");
        assert_eq!(keys(m["criteria"][0].clone()), ["id", "module", "statement"]);
        let decl = Decl { path: "Repo::open".into(), kind: "method".into(), line: 3, file: "src/a.rs".into() };
        let file = File { path: "src/a.rs".into(), module: "src/a".into(), language: "rust".into(), decls: vec![decl], digest: String::new(), expects_contract: true, lines: 10, test_lines: 0 };
        let f = serde_json::to_value(&file).unwrap();
        for field in ["path", "module", "language", "decls"] {
            assert!(keys(f.clone()).contains(&field.to_string()), "a file carries its {field}");
        }
        assert_eq!(keys(f["decls"][0].clone()), ["file", "kind", "line", "path"]);
    }
}
