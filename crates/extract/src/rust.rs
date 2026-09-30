//! # Module contract
//!
//! ```json
//! {"purpose":"The Rust language adapter: canonical module identity, which files carry contracts, the leading //! comment, and tree-sitter extraction of declarations and use paths.","criteria":[{"id":"comments","statement":"Every comment in the file, line comments merged into runs and block comments, is handed over with its markers stripped and its first line, including those inside a tests module."},{"id":"identity","statement":"A module's id is its workspace-relative path without .rs, with a terminal /mod folded away, tests.rs mapped to its directory, X_tests.rs mapped to X, and a crate root mapped to its src directory."},{"id":"coverage","statement":"Every .rs file under a workspace member's src/ is expected to carry a contract except test files and files under a tests directory."},{"id":"imports-as-candidates","statement":"A use path becomes an ordered list of candidate module ids resolved from crate, self, super and workspace crate names, and from a bare first name only when the enclosing scope, the file or an inline module in it, declares a module of that name, which is then relative to that scope's module; std and any other name yield none, being an external crate."},{"id":"test-bodies-skipped","statement":"Declarations and imports inside a module named tests are not reported."}]}
//! ```

use std::path::Path;

use grimoire_core::Decl;
use tree_sitter::{Node, Parser};

use crate::{Adapter, Parsed};

/// A workspace member: package name and repository-relative crate directory
/// (`""` for a package at the root).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Crate {
    pub name: String,
    pub dir: String,
}

impl Crate {
    /// Repository-relative `src` directory, the crate root's module id.
    pub fn src(&self) -> String {
        if self.dir.is_empty() {
            "src".to_string()
        } else {
            format!("{}/src", self.dir)
        }
    }
}

pub struct RustAdapter {
    crates: Vec<Crate>,
}

impl RustAdapter {
    /// Scan the repository for `Cargo.toml` files with a `[package]` and
    /// remember their names and directories. Reads only manifests.
    pub fn new(root: &Path) -> Self {
        let mut crates = Vec::new();
        let walker = ignore::WalkBuilder::new(root).hidden(true).git_ignore(true).require_git(false).build();
        for entry in walker.flatten() {
            if entry.file_name() != "Cargo.toml" {
                continue;
            }
            let Ok(text) = std::fs::read_to_string(entry.path()) else { continue };
            let Ok(doc) = text.parse::<toml::Table>() else { continue };
            let Some(name) = doc.get("package").and_then(|p| p.get("name")).and_then(|n| n.as_str()) else { continue };
            let dir = entry.path().parent().and_then(|p| crate::relative(root, p)).unwrap_or_default();
            crates.push(Crate { name: name.to_string(), dir });
        }
        Self::with_crates(crates)
    }

    /// Longest directory first, so `crate_of` finds the most specific member.
    pub fn with_crates(mut crates: Vec<Crate>) -> Self {
        crates.sort_by_key(|c| std::cmp::Reverse(c.dir.len()));
        RustAdapter { crates }
    }

    pub fn crates(&self) -> &[Crate] {
        &self.crates
    }

    /// The crate a repository-relative path belongs to: the one with the
    /// longest directory prefix.
    fn crate_of(&self, rel: &str) -> Option<&Crate> {
        self.crates.iter().find(|c| c.dir.is_empty() || rel == c.dir || rel.starts_with(&format!("{}/", c.dir)))
    }

    fn crate_by_name(&self, name: &str) -> Option<&Crate> {
        let wanted = name.replace('-', "_");
        self.crates.iter().find(|c| c.name.replace('-', "_") == wanted)
    }

    /// Candidate module ids for one `use` path, from the importing module.
    /// `declared` is the modules the file declares at its top level: a bare
    /// first name is local only when it is one of them (Rust 2018 paths), and
    /// is otherwise an external crate, which yields nothing.
    fn candidates(&self, segs: &[String], from_module: &str, declared: &[String]) -> Vec<String> {
        let Some(first) = segs.first() else { return Vec::new() };
        let crate_src = self.crate_of(from_module).map(|c| c.src());
        let mut out = Vec::new();
        match first.as_str() {
            "std" | "core" | "alloc" | "proc_macro" => {}
            "crate" => {
                if let Some(base) = &crate_src {
                    push_candidates(&mut out, base, &segs[1..]);
                }
            }
            "self" => push_candidates(&mut out, from_module, &segs[1..]),
            "super" => {
                let mut base = from_module.to_string();
                let mut i = 0;
                while i < segs.len() && segs[i] == "super" {
                    if crate_src.as_deref() == Some(base.as_str()) {
                        break;
                    }
                    base = parent(&base);
                    i += 1;
                }
                push_candidates(&mut out, &base, &segs[i..]);
            }
            name => {
                if let Some(c) = self.crate_by_name(name) {
                    push_candidates(&mut out, &c.src(), &segs[1..]);
                } else if declared.iter().any(|d| d == name) {
                    push_candidates(&mut out, from_module, segs);
                }
            }
        }
        out.dedup();
        out
    }
}

fn parent(module: &str) -> String {
    module.rsplit_once('/').map(|(p, _)| p.to_string()).unwrap_or_default()
}

fn push_candidates(out: &mut Vec<String>, base: &str, rest: &[String]) {
    let rest: Vec<&str> = rest.iter().map(|s| s.as_str()).filter(|s| *s != "self").collect();
    for k in (0..=rest.len()).rev() {
        if k == 0 {
            out.push(base.to_string());
        } else {
            out.push(format!("{base}/{}", rest[..k].join("/")));
        }
    }
}

fn is_test_file(file_stem: &str) -> bool {
    file_stem == "tests" || file_stem.ends_with("_tests")
}

impl Adapter for RustAdapter {
    fn language(&self) -> &'static str {
        "rust"
    }

    fn handles(&self, rel_path: &str) -> bool {
        rel_path.ends_with(".rs")
    }

    fn module_id(&self, rel_path: &str) -> String {
        let no_ext = rel_path.strip_suffix(".rs").unwrap_or(rel_path);
        let (dir, file) = match no_ext.rsplit_once('/') {
            Some((d, f)) => (d.to_string(), f),
            None => (String::new(), no_ext),
        };
        let join = |d: &str, f: &str| if d.is_empty() { f.to_string() } else { format!("{d}/{f}") };
        if file == "mod" || file == "tests" {
            return if dir.is_empty() { file.to_string() } else { dir };
        }
        if let Some(stem) = file.strip_suffix("_tests") {
            return join(&dir, stem);
        }
        if (file == "lib" || file == "main") && (dir == "src" || dir.ends_with("/src")) {
            return dir;
        }
        join(&dir, file)
    }

    fn expects_contract(&self, rel_path: &str) -> bool {
        let Some(c) = self.crate_of(rel_path) else { return false };
        let src = format!("{}/", c.src());
        let Some(inside) = rel_path.strip_prefix(&src) else { return false };
        let parts: Vec<&str> = inside.split('/').collect();
        if parts[..parts.len() - 1].contains(&"tests") {
            return false;
        }
        let stem = parts.last().unwrap_or(&"").trim_end_matches(".rs");
        !is_test_file(stem)
    }

    fn leading_doc(&self, source: &str) -> Option<(String, u32)> {
        let mut text = String::new();
        let mut first: Option<u32> = None;
        for (i, line) in source.lines().enumerate() {
            let t = line.trim_start();
            if let Some(rest) = t.strip_prefix("//!") {
                first.get_or_insert(i as u32 + 1);
                text.push_str(rest.strip_prefix(' ').unwrap_or(rest));
                text.push('\n');
            } else if first.is_some() {
                break;
            } else if t.is_empty() || t.starts_with("//") || t.starts_with("#!") {
                continue;
            } else {
                break;
            }
        }
        first.map(|l| (text, l))
    }

    fn parse(&self, source: &str, module_id: &str) -> Parsed {
        let mut parser = Parser::new();
        parser.set_language(&tree_sitter_rust::LANGUAGE.into()).expect("rust grammar loads");
        let Some(tree) = parser.parse(source, None) else { return Parsed::default() };
        let src = source.as_bytes();
        let scopes = vec![(module_id.to_string(), declared_in(tree.root_node(), src))];
        let mut w = Walker { src, adapter: self, scopes, out: Parsed::default() };
        w.visit(tree.root_node(), &mut Vec::new());
        // Over the whole tree: the walker skips test bodies, and tests are
        // where test blocks live.
        w.out.comments = crate::slash_comments(tree.root_node(), source.as_bytes(), |k| k == "line_comment" || k == "block_comment");
        w.out
    }
}

/// The modules a scope (the file, or an inline module's body) declares,
/// found before any use in it: `mod graph;` may follow `use graph::Graph;`.
fn declared_in(scope: Node, src: &[u8]) -> Vec<String> {
    let mut cursor = scope.walk();
    scope
        .children(&mut cursor)
        .filter(|c| c.kind() == "mod_item")
        .filter_map(|c| c.child_by_field_name("name").and_then(|n| n.utf8_text(src).ok()).map(str::to_string))
        .collect()
}

struct Walker<'a> {
    src: &'a [u8],
    adapter: &'a RustAdapter,
    /// Innermost last: each scope's module id and the modules it declares.
    scopes: Vec<(String, Vec<String>)>,
    out: Parsed,
}

impl<'a> Walker<'a> {
    fn text(&self, n: Node) -> String {
        n.utf8_text(self.src).unwrap_or("").to_string()
    }

    fn name(&self, n: Node) -> Option<String> {
        n.child_by_field_name("name").map(|c| self.text(c))
    }

    fn decl(&mut self, prefix: &[String], name: &str, kind: &str, n: Node) {
        let path = if prefix.is_empty() { name.to_string() } else { format!("{}::{name}", prefix.join("::")) };
        self.out.decls.push(Decl { path, kind: kind.to_string(), line: n.start_position().row as u32 + 1 });
    }

    fn visit(&mut self, n: Node, prefix: &mut Vec<String>) {
        let mut cursor = n.walk();
        let children: Vec<Node> = n.children(&mut cursor).collect();
        for c in children {
            match c.kind() {
                "function_item" | "function_signature_item" => {
                    if let Some(name) = self.name(c) {
                        let kind = if prefix.last().is_some_and(|p| p.chars().next().is_some_and(char::is_uppercase)) { "method" } else { "function" };
                        self.decl(prefix, &name, kind, c);
                    }
                }
                "struct_item" | "enum_item" | "union_item" | "type_item" => {
                    if let Some(name) = self.name(c) {
                        self.decl(prefix, &name, "type", c);
                    }
                }
                "trait_item" => {
                    if let Some(name) = self.name(c) {
                        self.decl(prefix, &name, "trait", c);
                        prefix.push(name);
                        if let Some(body) = c.child_by_field_name("body") {
                            self.visit(body, prefix);
                        }
                        prefix.pop();
                    }
                }
                "const_item" | "static_item" => {
                    if let Some(name) = self.name(c) {
                        self.decl(prefix, &name, "const", c);
                    }
                }
                "macro_definition" => {
                    if let Some(name) = self.name(c) {
                        self.decl(prefix, &name, "macro", c);
                    }
                }
                "impl_item" => {
                    let ty = c.child_by_field_name("type").map(|t| type_name(&self.text(t))).unwrap_or_default();
                    if !ty.is_empty() {
                        prefix.push(ty);
                    }
                    if let Some(body) = c.child_by_field_name("body") {
                        self.visit(body, prefix);
                    }
                    if c.child_by_field_name("type").is_some() {
                        prefix.pop();
                    }
                }
                "mod_item" => {
                    let Some(name) = self.name(c) else { continue };
                    if name == "tests" {
                        continue;
                    }
                    if let Some(body) = c.child_by_field_name("body") {
                        let module = format!("{}/{name}", self.scopes.last().map_or("", |s| s.0.as_str()));
                        self.scopes.push((module, declared_in(body, self.src)));
                        prefix.push(name);
                        self.visit(body, prefix);
                        prefix.pop();
                        self.scopes.pop();
                    }
                }
                "use_declaration" => {
                    if let Some(arg) = c.child_by_field_name("argument") {
                        for path in self.flatten(arg) {
                            let (module, declared) = self.scopes.last().expect("the file's scope");
                            let cands = self.adapter.candidates(&path, module, declared);
                            if !cands.is_empty() {
                                self.out.imports.push(cands);
                            }
                        }
                    }
                }
                _ => {
                    // Descend into anything else that may hold items (blocks, bodies).
                    if c.child_count() > 0 {
                        self.visit(c, prefix);
                    }
                }
            }
        }
    }

    /// Expand a use tree into full paths.
    fn flatten(&self, n: Node) -> Vec<Vec<String>> {
        match n.kind() {
            "identifier" | "crate" | "super" | "self" | "metavariable" => vec![vec![self.text(n)]],
            "scoped_identifier" => {
                let prefixes = n.child_by_field_name("path").map(|p| self.flatten(p)).unwrap_or_else(|| vec![vec![]]);
                let name = n.child_by_field_name("name").map(|c| self.text(c)).unwrap_or_default();
                prefixes.into_iter().map(|mut p| {
                    p.push(name.clone());
                    p
                }).collect()
            }
            "scoped_use_list" => {
                let prefixes = n.child_by_field_name("path").map(|p| self.flatten(p)).unwrap_or_else(|| vec![vec![]]);
                let list = n.child_by_field_name("list").map(|l| self.flatten(l)).unwrap_or_default();
                let mut out = Vec::new();
                for p in &prefixes {
                    for item in &list {
                        let mut full = p.clone();
                        full.extend(item.iter().cloned());
                        out.push(full);
                    }
                }
                out
            }
            "use_list" => {
                let mut cursor = n.walk();
                let mut out = Vec::new();
                for c in n.named_children(&mut cursor) {
                    out.extend(self.flatten(c));
                }
                out
            }
            "use_as_clause" => n.child_by_field_name("path").map(|p| self.flatten(p)).unwrap_or_default(),
            "use_wildcard" => {
                let mut cursor = n.walk();
                let first = n.named_children(&mut cursor).next();
                match first {
                    Some(p) => self.flatten(p),
                    None => Vec::new(),
                }
            }
            _ => Vec::new(),
        }
    }
}

/// `Foo<T>` and `super::Foo` both name `Foo`; `&mut Foo` and `(A, B)` are
/// reduced the same way, imperfectly but harmlessly.
fn type_name(t: &str) -> String {
    let t = t.trim_start_matches(['&', '*', '(']).trim_start_matches("mut ").trim_start_matches("dyn ");
    let t = t.split('<').next().unwrap_or(t);
    t.rsplit("::").next().unwrap_or(t).trim().trim_end_matches([')', ',']).to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn adapter() -> RustAdapter {
        RustAdapter::with_crates(vec![
            Crate { name: "grimoire-core".into(), dir: "crates/core".into() },
            Crate { name: "grimoire-extract".into(), dir: "crates/extract".into() },
        ])
    }

    /// # Test contract
    ///
    /// ```json
    /// {"test": "identity-rules", "checks": ["crates/extract/src/rust#identity"]}
    /// ```
    #[test]
    fn identity_rules() {
        let a = adapter();
        assert_eq!(a.module_id("crates/core/src/lib.rs"), "crates/core/src");
        assert_eq!(a.module_id("crates/core/src/main.rs"), "crates/core/src");
        assert_eq!(a.module_id("crates/core/src/git/mod.rs"), "crates/core/src/git");
        assert_eq!(a.module_id("crates/core/src/git/ready.rs"), "crates/core/src/git/ready");
        assert_eq!(a.module_id("crates/core/src/git/tests.rs"), "crates/core/src/git");
        assert_eq!(a.module_id("crates/core/src/git/ready_tests.rs"), "crates/core/src/git/ready");
        assert_eq!(a.module_id("crates/core/build.rs"), "crates/core/build");
        assert_eq!(a.module_id("lib.rs"), "lib");
    }

    /// # Test contract
    ///
    /// ```json
    /// {"test": "coverage-rules", "checks": ["crates/extract/src/rust#coverage"]}
    /// ```
    #[test]
    fn coverage_rules() {
        let a = adapter();
        assert!(a.expects_contract("crates/core/src/lib.rs"));
        assert!(a.expects_contract("crates/core/src/git/ready.rs"));
        assert!(!a.expects_contract("crates/core/src/git/tests.rs"));
        assert!(!a.expects_contract("crates/core/src/git/ready_tests.rs"));
        assert!(!a.expects_contract("crates/core/tests/it.rs"));
        assert!(!a.expects_contract("crates/core/build.rs"));
        assert!(!a.expects_contract("scripts/x.rs"));
    }

    #[test]
    fn leading_doc_is_the_bang_block() {
        let a = adapter();
        let src = "// SPDX\n\n#![allow(x)]\n//! # Module contract\n//!\n//! text\n\nuse x;\n//! not part\n";
        let (text, line) = a.leading_doc(src).unwrap();
        assert_eq!(line, 4);
        assert_eq!(text, "# Module contract\n\ntext\n");
        assert!(a.leading_doc("fn main() {}").is_none());
    }

    /// # Test contract
    ///
    /// ```json
    /// {"test": "parses-decls-and-imports", "checks": ["crates/extract/src/rust#imports-as-candidates", "crates/extract/src/rust#test-bodies-skipped"]}
    /// ```
    #[test]
    fn parses_decls_and_imports() {
        let a = adapter();
        let src = r#"
use crate::git::layout;
use super::{ready, self};
use grimoire_core::model::Page;
use std::fs;
use serde::Serialize;
use log::info;
use loader::Load;

mod loader;
pub struct Repository;
impl Repository {
    pub fn from_top(p: &str) -> Self { Repository }
}
impl<'a> Iterator for Wrapper<'a> { fn next(&mut self) -> Option<()> { None } }
pub trait Ready { fn check(&self); }
pub enum State { A }
pub type Alias = u8;
pub const N: u8 = 1;
fn helper() {}
mod inner { pub fn deep() {} mod b; use b::X; use self::b::Y; }
#[cfg(test)]
mod tests { use crate::nope; fn t() {} }
"#;
        let p = a.parse(src, "crates/core/src/git/ready");
        let decls: Vec<(String, String)> = p.decls.iter().map(|d| (d.path.clone(), d.kind.clone())).collect();
        assert!(decls.contains(&("Repository".into(), "type".into())));
        assert!(decls.contains(&("Repository::from_top".into(), "method".into())));
        assert!(decls.contains(&("Wrapper::next".into(), "method".into())));
        assert!(decls.contains(&("Ready".into(), "trait".into())));
        assert!(decls.contains(&("Ready::check".into(), "method".into())));
        assert!(decls.contains(&("State".into(), "type".into())));
        assert!(decls.contains(&("Alias".into(), "type".into())));
        assert!(decls.contains(&("N".into(), "const".into())));
        assert!(decls.contains(&("helper".into(), "function".into())));
        assert!(decls.contains(&("inner::deep".into(), "function".into())));
        assert!(!decls.iter().any(|(p, _)| p.contains("t") && p.len() == 1));
        let line = p.decls.iter().find(|d| d.path == "Repository").unwrap().line;
        assert_eq!(line, 11);

        // imports
        assert!(p.imports.contains(&vec!["crates/core/src/git/layout".to_string(), "crates/core/src/git".to_string(), "crates/core/src".to_string()]));
        assert!(p.imports.contains(&vec!["crates/core/src/git/ready".to_string(), "crates/core/src/git".to_string()]));
        assert!(p.imports.contains(&vec!["crates/core/src/git".to_string()]));
        assert!(p.imports.iter().any(|c| c[0] == "crates/core/src/model/Page" && c[1] == "crates/core/src/model"));
        // A module the file declares is local, even when the use comes first.
        assert!(p.imports.contains(&vec!["crates/core/src/git/ready/loader/Load".to_string(), "crates/core/src/git/ready/loader".to_string(), "crates/core/src/git/ready".to_string()]));
        // Inside an inline module, its own declarations and module path.
        assert!(p.imports.contains(&vec!["crates/core/src/git/ready/inner/b/X".to_string(), "crates/core/src/git/ready/inner/b".to_string(), "crates/core/src/git/ready/inner".to_string()]), "{:?}", p.imports);
        assert!(p.imports.iter().any(|c| c[0] == "crates/core/src/git/ready/inner/b/Y"), "self is the inline module");
        assert!(!p.imports.iter().any(|c| c[0] == "crates/core/src/git/ready/b/X"), "not the file's scope");
        // std, and names the file does not declare, are external crates: nothing,
        // even where a local module of the same name exists.
        assert!(!p.imports.iter().any(|c| c.iter().any(|s| s.contains("std") || s.contains("serde") || s.contains("log"))), "{:?}", p.imports);
        assert!(!p.imports.iter().any(|c| c.iter().any(|s| s.contains("nope"))));
    }

    /// # Test contract
    ///
    /// ```json
    /// {"test": "test-blocks-inside-tests-modules", "checks": ["crates/extract/src/rust#comments", "crates/extract/src/rust#test-bodies-skipped"]}
    /// ```
    #[test]
    fn test_blocks_inside_a_tests_module_are_found() {
        let src = "//! # Module contract\n//!\n//! text\n\nfn run() {}\n\n#[cfg(test)]\nmod tests {\n    /// # Test contract\n    ///\n    /// ```json\n    /// {\"test\": \"runs\", \"checks\": [\"crates/x/src#runs\"]}\n    /// ```\n    #[test]\n    fn runs() {}\n}\n";
        let a = adapter();
        let x = crate::extract_source(&a, "crates/x/src/lib.rs", src);
        assert_eq!(x.tests.len(), 1, "{:?} {:?}", x.tests, x.test_errors);
        assert_eq!(x.tests[0].name, "runs");
        assert_eq!(x.tests[0].line, 9, "the heading's line");
        assert_eq!(x.tests[0].checks, vec!["crates/x/src#runs".to_string()]);
        assert!(!x.decls.iter().any(|d| d.path.contains("runs")), "test bodies are still not declarations");
    }

    /// # Test contract
    ///
    /// ```json
    /// {"test": "comments-merge-line-runs", "checks": ["crates/extract/src/rust#comments"]}
    /// ```
    #[test]
    fn comments_merge_line_runs_and_keep_blocks() {
        let src = "//! lead\n//! more\n\n/// doc\nfn a() {}\n/* block\n * two\n */\nfn b() { let s = \"http://not-a-comment\"; }\n";
        let c = adapter().parse(src, "m").comments;
        assert_eq!(c, vec![("lead\nmore\n".to_string(), 1), ("doc\n".to_string(), 4), ("block\ntwo\n".to_string(), 6)]);
    }

    #[test]
    fn type_names() {
        assert_eq!(type_name("Foo<T>"), "Foo");
        assert_eq!(type_name("super::Foo"), "Foo");
        assert_eq!(type_name("&mut Foo"), "Foo");
    }
}
