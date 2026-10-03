//! # Module contract
//!
//! ```json
//! {"purpose":"The TypeScript language adapter: module identity from the file path, which files carry contracts, the leading block comment, and tree-sitter extraction of declarations and import specifiers.","criteria":[{"id":"comments","statement":"Every comment in the file, line comments merged into runs and block comments, is handed over with its markers stripped and its first line."},{"id":"identity","statement":"A module's id is its repository-relative path without extension, with a terminal /index folded away."},{"id":"coverage","statement":"Every .ts or .tsx file under a package's src/ is expected to carry a contract except test and spec files, declaration files, and files under a __tests__ directory."},{"id":"imports-as-candidates","statement":"A relative import specifier resolves against the importing file's directory to one candidate module id; a bare specifier resolves against workspace package names or yields none."},{"id":"leading-block-comment","statement":"The leading documentation is the first /** */ or /* */ block or run of // lines at the top of the file, with the comment markers stripped."}]}
//! ```

use std::path::Path;

use grimoire_core::Decl;
use tree_sitter::{Node, Parser};

use crate::{Adapter, Parsed};

/// A workspace package: `package.json` name and repository-relative directory.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Package {
    pub name: String,
    pub dir: String,
}

pub struct TypeScriptAdapter {
    packages: Vec<Package>,
}

impl TypeScriptAdapter {
    /// Scan for `package.json` files (never inside `node_modules`, which
    /// `.gitignore` excludes) and remember their names and directories.
    pub fn new(root: &Path) -> Self {
        let mut packages = Vec::new();
        let walker = ignore::WalkBuilder::new(root).hidden(true).git_ignore(true).require_git(false).build();
        for entry in walker.flatten() {
            if entry.file_name() != "package.json" {
                continue;
            }
            let Ok(text) = std::fs::read_to_string(entry.path()) else { continue };
            let Ok(doc) = serde_json::from_str::<serde_json::Value>(&text) else { continue };
            let Some(name) = doc.get("name").and_then(|n| n.as_str()) else { continue };
            let dir = entry.path().parent().and_then(|p| crate::relative(root, p)).unwrap_or_default();
            packages.push(Package { name: name.to_string(), dir });
        }
        Self::with_packages(packages)
    }

    pub fn with_packages(mut packages: Vec<Package>) -> Self {
        packages.sort_by_key(|p| std::cmp::Reverse(p.dir.len()));
        TypeScriptAdapter { packages }
    }

    fn package_of(&self, rel: &str) -> Option<&Package> {
        self.packages.iter().find(|p| p.dir.is_empty() || rel == p.dir || rel.starts_with(&format!("{}/", p.dir)))
    }

    /// Candidate module ids for one import specifier from a file.
    fn candidates(&self, spec: &str, from_file: &str) -> Vec<String> {
        let spec = spec.trim();
        if spec.starts_with("./") || spec.starts_with("../") || spec == "." || spec == ".." {
            let dir = from_file.rsplit_once('/').map(|(d, _)| d).unwrap_or("");
            let joined = normalize(&format!("{dir}/{spec}"));
            return vec![self.module_id(&joined)];
        }
        // Bare specifier: a workspace package, else external.
        let (pkg, rest) = split_package(spec);
        if let Some(p) = self.packages.iter().find(|p| p.name == pkg) {
            let src = if p.dir.is_empty() { "src".to_string() } else { format!("{}/src", p.dir) };
            let mut out = Vec::new();
            if !rest.is_empty() {
                out.push(self.module_id(&format!("{src}/{rest}")));
            }
            out.push(self.module_id(&format!("{src}/index.ts")));
            return out;
        }
        Vec::new()
    }
}

/// `@scope/name/sub/path` → (`@scope/name`, `sub/path`); `name/sub` → (`name`, `sub`).
fn split_package(spec: &str) -> (&str, &str) {
    let mut parts = spec.splitn(if spec.starts_with('@') { 3 } else { 2 }, '/');
    let first = parts.next().unwrap_or("");
    if spec.starts_with('@') {
        let second = parts.next().unwrap_or("");
        let pkg_len = first.len() + 1 + second.len();
        let pkg = &spec[..pkg_len.min(spec.len())];
        let rest = spec.get(pkg_len + 1..).unwrap_or("");
        (pkg, rest)
    } else {
        (first, parts.next().unwrap_or(""))
    }
}

/// Collapse `.` and `..` segments of a `/`-joined path.
fn normalize(path: &str) -> String {
    let mut out: Vec<&str> = Vec::new();
    for seg in path.split('/') {
        match seg {
            "" | "." => {}
            ".." => {
                out.pop();
            }
            s => out.push(s),
        }
    }
    out.join("/")
}

const EXTENSIONS: [&str; 6] = [".tsx", ".ts", ".mts", ".cts", ".jsx", ".js"];

fn strip_extension(path: &str) -> &str {
    for e in EXTENSIONS {
        if let Some(s) = path.strip_suffix(e) {
            return s;
        }
    }
    path
}

fn is_test_file(rel: &str) -> bool {
    let name = rel.rsplit('/').next().unwrap_or(rel);
    name.contains(".test.") || name.contains(".spec.") || name.ends_with(".d.ts") || rel.split('/').any(|s| s == "__tests__")
}

impl Adapter for TypeScriptAdapter {
    fn language(&self) -> &'static str {
        "typescript"
    }

    fn handles(&self, rel_path: &str) -> bool {
        (rel_path.ends_with(".ts") || rel_path.ends_with(".tsx") || rel_path.ends_with(".mts") || rel_path.ends_with(".cts")) && !rel_path.ends_with(".d.ts")
    }

    fn module_id(&self, rel_path: &str) -> String {
        let no_ext = strip_extension(rel_path);
        match no_ext.strip_suffix("/index") {
            Some(dir) => dir.to_string(),
            None if no_ext == "index" => String::new(),
            None => no_ext.to_string(),
        }
    }

    fn expects_contract(&self, rel_path: &str) -> bool {
        let Some(p) = self.package_of(rel_path) else { return false };
        let src = if p.dir.is_empty() { "src/".to_string() } else { format!("{}/src/", p.dir) };
        rel_path.starts_with(&src) && !is_test_file(rel_path)
    }

    fn leading_doc(&self, source: &str) -> Option<(String, u32)> {
        let mut lines = source.lines().enumerate().peekable();
        // Skip blank lines and a shebang.
        while let Some((_, l)) = lines.peek() {
            let t = l.trim();
            if t.is_empty() || t.starts_with("#!") {
                lines.next();
            } else {
                break;
            }
        }
        let (first_idx, first) = lines.peek().copied()?;
        let first_line = first_idx as u32 + 1;
        let t = first.trim_start();
        let mut text = String::new();
        if t.starts_with("/*") {
            // Block comment: collect until `*/`, stripping markers.
            let mut closed = false;
            for (i, l) in lines {
                let mut s = l.trim();
                if i == first_idx {
                    s = s.trim_start_matches("/**").trim_start_matches("/*").trim_start();
                }
                if let Some(before) = s.find("*/") {
                    s = s[..before].trim_end();
                    closed = true;
                }
                let s = s.strip_prefix('*').map(|r| r.strip_prefix(' ').unwrap_or(r)).unwrap_or(s);
                // One line out per line in, the opening `/**` included, so a
                // heading's line counts from the block's first line; only the
                // empty closing line is dropped.
                let skip = s.is_empty() && closed;
                if !skip {
                    text.push_str(s);
                    text.push('\n');
                }
                if closed {
                    break;
                }
            }
            Some((text, first_line))
        } else if t.starts_with("//") {
            for (_, l) in lines {
                let s = l.trim_start();
                let Some(rest) = s.strip_prefix("//") else { break };
                text.push_str(rest.strip_prefix(' ').unwrap_or(rest));
                text.push('\n');
            }
            Some((text, first_line))
        } else {
            None
        }
    }

    fn parse(&self, source: &str, module_id: &str) -> Parsed {
        let mut parser = Parser::new();
        let lang = if module_id.ends_with(".tsx") || source.contains("</") || source.contains("/>") { tree_sitter_typescript::LANGUAGE_TSX } else { tree_sitter_typescript::LANGUAGE_TYPESCRIPT };
        parser.set_language(&lang.into()).expect("typescript grammar loads");
        let Some(tree) = parser.parse(source, None) else { return Parsed::default() };
        let mut w = Walker { src: source.as_bytes(), out: Parsed::default(), specs: Vec::new() };
        w.visit(tree.root_node(), &mut Vec::new());
        w.out.comments = crate::slash_comments(tree.root_node(), source.as_bytes(), |k| k == "comment");
        // Imports are resolved from the file, which the caller identified by
        // module id; a module id plus the extension list is enough to find the
        // directory, since identity keeps the directory.
        let from_file = format!("{module_id}/index.ts");
        let from_file = if module_id.is_empty() { "index.ts".to_string() } else { from_file };
        let _ = from_file;
        let mut parsed = w.out;
        for spec in w.specs {
            let cands = self.candidates(&spec, &format!("{module_id}.ts"));
            if !cands.is_empty() {
                parsed.imports.push(cands);
            }
        }
        parsed
    }
}

struct Walker<'a> {
    src: &'a [u8],
    out: Parsed,
    specs: Vec<String>,
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
        self.out.decls.push(Decl { path, kind: kind.to_string(), line: n.start_position().row as u32 + 1, file: String::new() });
    }

    fn string_content(&self, n: Node) -> String {
        let t = self.text(n);
        t.trim_matches(|c| c == '"' || c == '\'' || c == '`').to_string()
    }

    fn visit(&mut self, n: Node, prefix: &mut Vec<String>) {
        let mut cursor = n.walk();
        let children: Vec<Node> = n.children(&mut cursor).collect();
        for c in children {
            match c.kind() {
                "import_statement" => {
                    if let Some(s) = c.child_by_field_name("source") {
                        self.specs.push(self.string_content(s));
                    }
                }
                "export_statement" => {
                    if let Some(s) = c.child_by_field_name("source") {
                        self.specs.push(self.string_content(s));
                    }
                    if let Some(d) = c.child_by_field_name("declaration") {
                        self.visit_decl(d, prefix);
                    }
                }
                "function_declaration" | "class_declaration" | "abstract_class_declaration" | "interface_declaration" | "type_alias_declaration" | "enum_declaration" | "lexical_declaration" => {
                    self.visit_decl(c, prefix);
                }
                _ => {
                    if c.child_count() > 0 && !matches!(c.kind(), "statement_block" | "arrow_function" | "function_expression") {
                        self.visit(c, prefix);
                    }
                }
            }
        }
    }

    fn visit_decl(&mut self, c: Node, prefix: &mut Vec<String>) {
        match c.kind() {
            "function_declaration" => {
                if let Some(name) = self.name(c) {
                    self.decl(prefix, &name, "function", c);
                }
            }
            "class_declaration" | "abstract_class_declaration" => {
                if let Some(name) = self.name(c) {
                    self.decl(prefix, &name, "type", c);
                    if let Some(body) = c.child_by_field_name("body") {
                        prefix.push(name);
                        let mut cur = body.walk();
                        let members: Vec<Node> = body.children(&mut cur).collect();
                        for m in members {
                            if m.kind() == "method_definition" {
                                if let Some(mn) = self.name(m) {
                                    self.decl(prefix, &mn, "method", m);
                                }
                            }
                        }
                        prefix.pop();
                    }
                }
            }
            "interface_declaration" | "type_alias_declaration" | "enum_declaration" => {
                if let Some(name) = self.name(c) {
                    self.decl(prefix, &name, "type", c);
                }
            }
            "lexical_declaration" => {
                let is_const = c.child_by_field_name("kind").map(|k| self.text(k) == "const").unwrap_or(false);
                let mut cur = c.walk();
                let decls: Vec<Node> = c.children(&mut cur).filter(|d| d.kind() == "variable_declarator").collect();
                for d in decls {
                    let Some(name_node) = d.child_by_field_name("name") else { continue };
                    if name_node.kind() != "identifier" {
                        continue; // destructuring: not a named declaration
                    }
                    let name = self.text(name_node);
                    let is_fn = d.child_by_field_name("value").is_some_and(|v| matches!(v.kind(), "arrow_function" | "function_expression"));
                    let kind = if is_fn { "function" } else if is_const { "const" } else { "variable" };
                    self.decl(prefix, &name, kind, d);
                }
            }
            _ => {}
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn adapter() -> TypeScriptAdapter {
        TypeScriptAdapter::with_packages(vec![Package { name: "@grimoire/web".into(), dir: "web".into() }])
    }

    /// # Test contract
    ///
    /// ```json
    /// {"test": "identity-and-coverage", "checks": ["crates/extract/src/typescript#identity", "crates/extract/src/typescript#coverage"]}
    /// ```
    #[test]
    fn identity_and_coverage() {
        let a = adapter();
        assert_eq!(a.module_id("web/src/views/Map.tsx"), "web/src/views/Map");
        assert_eq!(a.module_id("web/src/views/index.ts"), "web/src/views");
        assert!(a.expects_contract("web/src/api.ts"));
        assert!(!a.expects_contract("web/src/api.test.ts"));
        assert!(!a.expects_contract("web/src/types.d.ts"));
        assert!(!a.expects_contract("web/vite.config.ts"));
        assert!(!a.expects_contract("scripts/x.ts"));
        assert!(!a.handles("web/src/types.d.ts"));
    }

    /// # Test contract
    ///
    /// ```json
    /// {"test": "leading-block-and-line-comments", "checks": ["crates/extract/src/typescript#leading-block-comment"]}
    /// ```
    #[test]
    fn leading_block_and_line_comments() {
        let a = adapter();
        let src = "/**\n * # Module contract\n *\n * ```json\n * {}\n * ```\n */\nimport x from 'y';\n";
        let (text, line) = a.leading_doc(src).unwrap();
        assert_eq!(line, 1);
        assert_eq!(text, "\n# Module contract\n\n```json\n{}\n```\n", "one line per source line, the opening `/**` included");
        // The heading is on line 2, and that is the line the contract reports.
        let contract = "/**\n * # Module contract\n *\n * ```json\n * {\"purpose\":\"P\",\"criteria\":[{\"id\":\"a\",\"statement\":\"S\"}]}\n * ```\n */\nexport const a = 1;\n";
        assert_eq!(crate::extract_source(&a, "web/src/a.ts", contract).contract_line, Some(2));
        let src = "\n// # Module contract\n// text\nconst a = 1;\n";
        let (text, line) = a.leading_doc(src).unwrap();
        assert_eq!(line, 2);
        assert_eq!(text, "# Module contract\ntext\n");
        assert!(a.leading_doc("const a = 1;").is_none());
    }

    /// # Test contract
    ///
    /// ```json
    /// {"test": "parses-decls-and-imports", "checks": ["crates/extract/src/typescript#imports-as-candidates"]}
    /// ```
    #[test]
    fn parses_decls_and_imports() {
        let a = adapter();
        let src = r#"
import { useState } from "react";
import { api, useQuery } from "../api";
import type { Tree } from "./types";
export { KindTag } from "./components";
import "./styles.css";

export function MapView() { return null; }
export const helper = (x: number) => x;
const LIMIT = 3;
let counter = 0;
export class Store {
  get(ref: string) { return ref; }
  static open() {}
}
export interface Props { a: number }
export type Ref = string;
enum Kind { A, B }
function inner() { const nested = () => 1; }
"#;
        let p = a.parse(src, "web/src/views/Map");
        let decls: Vec<(String, String)> = p.decls.iter().map(|d| (d.path.clone(), d.kind.clone())).collect();
        assert!(decls.contains(&("MapView".into(), "function".into())));
        assert!(decls.contains(&("helper".into(), "function".into())));
        assert!(decls.contains(&("LIMIT".into(), "const".into())));
        assert!(decls.contains(&("counter".into(), "variable".into())));
        assert!(decls.contains(&("Store".into(), "type".into())));
        assert!(decls.contains(&("Store::get".into(), "method".into())));
        assert!(decls.contains(&("Store::open".into(), "method".into())));
        assert!(decls.contains(&("Props".into(), "type".into())));
        assert!(decls.contains(&("Ref".into(), "type".into())));
        assert!(decls.contains(&("Kind".into(), "type".into())));
        assert!(decls.contains(&("inner".into(), "function".into())));
        assert!(!decls.iter().any(|(p, _)| p == "nested"), "{decls:?}");

        assert!(p.imports.contains(&vec!["web/src/api".to_string()]), "{:?}", p.imports);
        assert!(p.imports.contains(&vec!["web/src/views/types".to_string()]));
        assert!(p.imports.contains(&vec!["web/src/views/components".to_string()]));
        assert!(p.imports.contains(&vec!["web/src/views/styles.css".to_string()]));
        assert!(!p.imports.iter().any(|c| c.iter().any(|s| s.contains("react"))));
    }

    /// # Test contract
    ///
    /// ```json
    /// {"test": "test-block-in-a-spec", "checks": ["crates/extract/src/typescript#comments"]}
    /// ```
    #[test]
    fn a_test_block_in_a_spec_whatever_its_titles() {
        let src = "/**\n * Screenshots every view.\n *\n * # Test contract\n *\n * ```json\n * {\"test\": \"views-match-baselines\", \"checks\": [\"web/src/App#visual-baselines\"]}\n * ```\n */\nfor (const v of views) { test(v.name, async () => {}); }\n// a trailing note\n";
        let x = crate::extract_source(&adapter(), "web/visual/views.spec.ts", src);
        assert_eq!(x.tests.len(), 1, "{:?}", x.test_errors);
        assert_eq!(x.tests[0].name, "views-match-baselines");
        assert_eq!(x.tests[0].line, 4);
        assert!(!x.expects_contract, "a spec carries no module contract");
    }

    #[test]
    fn a_name_used_twice_is_malformed() {
        let block = |n: &str| format!("// # Test contract\n// ```json\n// {{\"test\": \"{n}\", \"checks\": [\"m#c\"]}}\n// ```\n");
        let src = format!("{}test('a', () => {{}});\n\n{}test('b', () => {{}});\n", block("same"), block("same"));
        let x = crate::extract_source(&adapter(), "web/src/x.test.ts", &src);
        assert_eq!(x.tests.len(), 1);
        assert_eq!(x.test_errors.len(), 1);
        assert!(x.test_errors[0].reason.contains("used twice"));
        assert_eq!(x.test_errors[0].line, 7);
    }

    /// # Test contract
    ///
    /// ```json
    /// {"test": "package-specifiers", "checks": ["crates/extract/src/typescript#imports-as-candidates"]}
    /// ```
    #[test]
    fn package_specifiers() {
        let a = adapter();
        assert_eq!(a.candidates("@grimoire/web/views/Map", "x/y.ts"), vec!["web/src/views/Map".to_string(), "web/src".to_string()]);
        assert!(a.candidates("react-dom/client", "x/y.ts").is_empty());
        assert_eq!(split_package("@scope/name/a/b"), ("@scope/name", "a/b"));
        assert_eq!(split_package("name"), ("name", ""));
    }
}
