//! # Module contract
//!
//! ```json
//! {"purpose":"The TypeScript and JavaScript language adapter, one reading under two language names: module identity from the file path, which files carry contracts, the leading block comment, and tree-sitter extraction of declarations and of imports resolved through the packages the repository holds.","criteria":[{"id":"files","statement":"The typescript language handles .ts, .tsx, .mts and .cts files but declaration files, reading .tsx with the TSX grammar and the rest with TypeScript's; the javascript language handles .js, .jsx, .mjs and .cjs files, reading each with the JavaScript grammar, JSX included; neither handles the other's."},{"id":"comments","statement":"Every comment in the file, line comments merged into runs and block comments, is handed over with its markers stripped and its first line."},{"id":"identity","statement":"A module's id is its repository-relative path without its extension, any of .ts, .tsx, .mts, .cts, .js, .jsx, .mjs or .cjs, with a terminal /index folded away."},{"id":"coverage","statement":"Every file a language handles under a package's src/ is expected to carry a contract except test and spec files, declaration files, and files under a __tests__ directory."},{"id":"declarations","statement":"Function and generator declarations and function signatures are declarations of kind function, an overloaded function once at its first signature, as is a const, let or var whose value is an arrow function or function expression; classes, interfaces, type aliases and enums are of kind type and a class's methods, abstract ones included and an overloaded one once at its first signature, of kind method under its path; any other const is of kind const and let or var of kind variable, one per named declarator; a declaration under TypeScript's declare, exported or not, is read as one without it; each has the line it starts on, and nothing inside a block, a loop, a switch, a function's body or a class's body but its methods is reported."},{"id":"imports-as-candidates","statement":"A relative import specifier resolves against the importing file's own directory, an index file's included, to one candidate module id; a bare specifier whose package part is a package.json name in the repository, outside any nested checkout and not ignored by the repository's own .gitignore files, read as the extractor's walk reads them, resolves through that package; any other yields none."},{"id":"package-exports","statement":"A bare specifier into a package of the repository yields, in order: the target its exports gives for the subpath, by exact key or else the matching * pattern with the longest prefix, a conditions object taking the first of import, module, require, node, default and then any other but types whose value matches, nested likewise, a condition naming nothing going on to the next and a null one excluding the subpath, an array by its first entry that names a file; or, with no exports and no subpath, its module and then its main; then for a subpath the subpath under the package's src/ and under the package; then the package's src/ and the package itself; each as a module id, none repeated."},{"id":"require-and-dynamic-import","statement":"Import and export-from declarations, TypeScript's import-equals require, and anywhere in the file a require(...) call or a dynamic import(...) whose one argument is a plain string literal are imports; any other call is none."},{"id":"leading-block-comment","statement":"The leading documentation is the first /** */ or /* */ block or run of // lines at the top of the file, after blank lines and a #! line, with the comment markers stripped."}]}
//! ```

use std::path::Path;

use grimoire_core::Decl;
use serde_json::Value;
use tree_sitter::{Node, Parser};

use crate::{Adapter, Parsed};

/// Which of the two languages an adapter reads: they share everything but
/// their files and their grammar (req:javascript-files).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Dialect {
    TypeScript,
    JavaScript,
}

/// A package of the repository: its `package.json` name, its
/// repository-relative directory, and how it is entered.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Package {
    pub name: String,
    pub dir: String,
    /// `exports` as subpath keys (`.`, `./button`, `./icons/*`) with the
    /// target each gives, `None` where a subpath is excluded.
    pub exports: Vec<(String, Option<String>)>,
    /// `module` then `main`, the entries of a package with no `exports`.
    pub entries: Vec<String>,
}

/// The conditions an export target is chosen by, in order of preference;
/// `types` names declarations, never code, and is never chosen.
const CONDITIONS: [&str; 5] = ["import", "module", "require", "node", "default"];

impl Package {
    pub fn named(name: &str, dir: &str) -> Self {
        Package { name: name.to_string(), dir: dir.to_string(), ..Package::default() }
    }

    /// A package from its `package.json` document, or `None` when it has no name.
    pub fn from_manifest(dir: &str, doc: &Value) -> Option<Self> {
        let name = doc.get("name")?.as_str()?;
        let exports = match doc.get("exports") {
            None => Vec::new(),
            Some(Value::Object(m)) if m.keys().next().is_some_and(|k| k.starts_with('.')) => m.iter().map(|(k, v)| (k.clone(), export_target(v))).collect(),
            Some(other) => vec![(".".to_string(), export_target(other))],
        };
        let entries = ["module", "main"].iter().filter_map(|k| doc.get(*k).and_then(Value::as_str)).map(str::to_string).collect();
        Some(Package { name: name.to_string(), dir: dir.to_string(), exports, entries })
    }

    /// The target `exports` gives a subpath (`""` for the package itself):
    /// `None` when no key matches, `Some(None)` when the key excludes it.
    fn exported(&self, rest: &str) -> Option<Option<String>> {
        let key = if rest.is_empty() { ".".to_string() } else { format!("./{rest}") };
        if let Some((_, t)) = self.exports.iter().find(|(k, _)| *k == key) {
            return Some(t.clone());
        }
        self.exports
            .iter()
            .filter_map(|(k, t)| {
                let (pre, post) = k.split_once('*')?;
                let mid = key.strip_prefix(pre)?.strip_suffix(post)?;
                Some((pre.len(), t.as_ref().map(|t| t.replace('*', mid))))
            })
            .max_by_key(|(len, _)| *len)
            .map(|(_, t)| t)
    }
}

/// The file an export's value names: a string, the first of an array that
/// names one, or a conditions object by [`CONDITIONS`] and then any other
/// condition but `types`; `null` names none.
fn export_target(v: &Value) -> Option<String> {
    resolve_target(v).flatten()
}

/// As Node resolves a target: `Some(Some(file))` names a file, `Some(None)` excludes the
/// subpath (a `null`, or an array naming nothing), and `None` matches nothing, so a conditions
/// object goes on to its next condition; only a `null` stops it.
fn resolve_target(v: &Value) -> Option<Option<String>> {
    match v {
        Value::String(s) => Some(Some(s.clone())),
        Value::Null => Some(None),
        Value::Array(a) => Some(a.iter().find_map(export_target)),
        Value::Object(m) => CONDITIONS.iter().filter_map(|c| m.get(*c)).chain(m.iter().filter(|(k, _)| *k != "types" && !CONDITIONS.contains(&k.as_str())).map(|(_, v)| v)).find_map(resolve_target),
        _ => None,
    }
}

pub struct TypeScriptAdapter {
    dialect: Dialect,
    packages: Vec<Package>,
}

impl TypeScriptAdapter {
    /// The TypeScript adapter over the repository at `root`.
    pub fn new(root: &Path) -> Self {
        Self::of(Dialect::TypeScript, root)
    }

    /// Scan for `package.json` files (never inside `node_modules`, which
    /// the repository's own `.gitignore` excludes, nor inside a nested checkout) and remember each package's name, directory
    /// and entries.
    pub fn of(dialect: Dialect, root: &Path) -> Self {
        let mut packages = Vec::new();
        // As the extractor walks: by the repository's own .gitignore files alone, and a folder below the root that is a checkout
        // of its own is not this repository's.
        let walker = crate::own_ignores(root).hidden(true).filter_entry(|e| e.depth() == 0 || !(e.file_type().is_some_and(|t| t.is_dir()) && crate::is_checkout(e.path()))).build();
        for entry in walker.flatten() {
            if entry.file_name() != "package.json" {
                continue;
            }
            let Ok(text) = std::fs::read_to_string(entry.path()) else { continue };
            let Ok(doc) = serde_json::from_str::<Value>(&text) else { continue };
            let dir = entry.path().parent().and_then(|p| crate::relative(root, p)).unwrap_or_default();
            packages.extend(Package::from_manifest(&dir, &doc));
        }
        Self::with_packages(dialect, packages)
    }

    pub fn with_packages(dialect: Dialect, mut packages: Vec<Package>) -> Self {
        packages.sort_by_key(|p| std::cmp::Reverse(p.dir.len()));
        TypeScriptAdapter { dialect, packages }
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
            return vec![module_id_of(&joined)];
        }
        // Bare specifier: a package of the repository, else external.
        let (pkg, rest) = split_package(spec);
        match self.packages.iter().find(|p| p.name == pkg) {
            Some(p) => package_candidates(p, rest),
            None => Vec::new(),
        }
    }
}

/// The candidates for a subpath (`""` for the package itself) of a package
/// of the repository (req:typescript-package-exports).
fn package_candidates(p: &Package, rest: &str) -> Vec<String> {
    let at = |path: &str| module_id_of(&normalize(&join(&p.dir, path)));
    let mut out = Vec::new();
    match p.exported(rest) {
        Some(Some(target)) => out.push(at(&target)),
        Some(None) => {}
        None if rest.is_empty() && p.exports.is_empty() => out.extend(p.entries.iter().map(|e| at(e))),
        None => {}
    }
    let src = join(&p.dir, "src");
    if !rest.is_empty() {
        out.push(module_id_of(&join(&src, rest)));
        out.push(module_id_of(&join(&p.dir, rest)));
    }
    out.push(src);
    out.push(p.dir.clone());
    let mut seen = std::collections::HashSet::new();
    out.retain(|c| !c.is_empty() && seen.insert(c.clone()));
    out
}

fn join(dir: &str, rest: &str) -> String {
    if dir.is_empty() {
        rest.to_string()
    } else {
        format!("{dir}/{rest}")
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

const TYPESCRIPT_EXTENSIONS: [&str; 4] = [".tsx", ".ts", ".mts", ".cts"];
const JAVASCRIPT_EXTENSIONS: [&str; 4] = [".jsx", ".js", ".mjs", ".cjs"];

fn strip_extension(path: &str) -> &str {
    TYPESCRIPT_EXTENSIONS.iter().chain(JAVASCRIPT_EXTENSIONS.iter()).find_map(|e| path.strip_suffix(e)).unwrap_or(path)
}

/// The module a path names, whichever of the two languages it is in
/// (req:typescript-identity).
fn module_id_of(rel_path: &str) -> String {
    let no_ext = strip_extension(rel_path);
    match no_ext.strip_suffix("/index") {
        Some(dir) => dir.to_string(),
        None if no_ext == "index" => String::new(),
        None => no_ext.to_string(),
    }
}

fn is_declaration_file(rel: &str) -> bool {
    [".d.ts", ".d.mts", ".d.cts"].iter().any(|e| rel.ends_with(e))
}

fn is_test_file(rel: &str) -> bool {
    let name = rel.rsplit('/').next().unwrap_or(rel);
    name.contains(".test.") || name.contains(".spec.") || is_declaration_file(rel) || rel.split('/').any(|s| s == "__tests__")
}

impl Adapter for TypeScriptAdapter {
    fn language(&self) -> &'static str {
        match self.dialect {
            Dialect::TypeScript => "typescript",
            Dialect::JavaScript => "javascript",
        }
    }

    fn tier(&self) -> u8 {
        2
    }

    fn handles(&self, rel_path: &str) -> bool {
        match self.dialect {
            Dialect::TypeScript => TYPESCRIPT_EXTENSIONS.iter().any(|e| rel_path.ends_with(e)) && !is_declaration_file(rel_path),
            Dialect::JavaScript => JAVASCRIPT_EXTENSIONS.iter().any(|e| rel_path.ends_with(e)),
        }
    }

    fn module_id(&self, rel_path: &str) -> String {
        module_id_of(rel_path)
    }

    fn expects_contract(&self, rel_path: &str) -> bool {
        let Some(p) = self.package_of(rel_path) else { return false };
        rel_path.starts_with(&join(&p.dir, "src/")) && !is_test_file(rel_path)
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
        // Without the file's path, the module's own: an index file's imports
        // resolve from its directory only through parse_file.
        let ext = match self.dialect {
            Dialect::TypeScript => "ts",
            Dialect::JavaScript => "js",
        };
        self.parse_file(source, &format!("{module_id}.{ext}"))
    }

    fn parse_file(&self, source: &str, rel_path: &str) -> Parsed {
        let mut parser = Parser::new();
        let lang: tree_sitter::Language = match self.dialect {
            Dialect::JavaScript => tree_sitter_javascript::LANGUAGE.into(),
            Dialect::TypeScript if rel_path.ends_with(".tsx") => tree_sitter_typescript::LANGUAGE_TSX.into(),
            Dialect::TypeScript => tree_sitter_typescript::LANGUAGE_TYPESCRIPT.into(),
        };
        parser.set_language(&lang).expect("typescript and javascript grammars load");
        let Some(tree) = parser.parse(source, None) else { return Parsed::default() };
        let mut w = Walker { src: source.as_bytes(), out: Parsed::default(), specs: Vec::new() };
        w.visit(tree.root_node(), &mut Vec::new());
        w.calls(tree.root_node());
        w.out.comments = crate::slash_comments(tree.root_node(), source.as_bytes(), |k| k == "comment");
        let mut parsed = w.out;
        for spec in w.specs {
            let cands = self.candidates(&spec, rel_path);
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

    /// Every `require("…")`, dynamic `import("…")` and TypeScript
    /// `import x = require("…")` in the file, bodies included: a lazy load is
    /// as much a dependency as a static import (req:javascript-require).
    fn calls(&mut self, root: Node) {
        let mut stack = vec![root];
        while let Some(n) = stack.pop() {
            match n.kind() {
                "call_expression" => {
                    let callee = n.child_by_field_name("function");
                    let loads = callee.is_some_and(|f| f.kind() == "import" || (f.kind() == "identifier" && self.text(f) == "require"));
                    if loads {
                        if let Some(args) = n.child_by_field_name("arguments") {
                            let mut cur = args.walk();
                            let named: Vec<Node> = args.named_children(&mut cur).collect();
                            if let [arg] = named.as_slice() {
                                if arg.kind() == "string" {
                                    self.specs.push(self.string_content(*arg));
                                }
                            }
                        }
                    }
                }
                "import_require_clause" => {
                    if let Some(s) = n.child_by_field_name("source") {
                        self.specs.push(self.string_content(s));
                    }
                }
                _ => {}
            }
            // Reversed onto the stack, so loads come out in the order the file writes them.
            let mut cursor = n.walk();
            let children: Vec<Node> = n.children(&mut cursor).collect();
            stack.extend(children.into_iter().rev());
        }
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
                "function_declaration"
                | "generator_function_declaration"
                | "function_signature"
                | "class_declaration"
                | "abstract_class_declaration"
                | "interface_declaration"
                | "type_alias_declaration"
                | "enum_declaration"
                | "lexical_declaration"
                | "variable_declaration" => {
                    self.visit_decl(c, prefix);
                }
                // TypeScript's `declare` wraps a declaration; nothing else at the top level is entered,
                // so a block, a loop's initializer or a switch case declares nothing here.
                "ambient_declaration" => self.visit(c, prefix),
                _ => {}
            }
        }
    }

    fn visit_decl(&mut self, c: Node, prefix: &mut Vec<String>) {
        match c.kind() {
            // A signature is a declared or overloaded function; an overloaded one is one entry, at its first.
            "function_declaration" | "generator_function_declaration" | "function_signature" => {
                if let Some(name) = self.name(c) {
                    if !self.out.decls.iter().any(|d| d.path == name && d.kind == "function") {
                        self.decl(prefix, &name, "function", c);
                    }
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
                            // An abstract method and an overload are signatures; an overloaded method is one entry, at its first.
                            if matches!(m.kind(), "method_definition" | "abstract_method_signature" | "method_signature") {
                                if let Some(mn) = self.name(m) {
                                    let path = format!("{}::{mn}", prefix.join("::"));
                                    if !self.out.decls.iter().any(|d| d.path == path) {
                                        self.decl(prefix, &mn, "method", m);
                                    }
                                }
                            }
                        }
                        prefix.pop();
                    }
                }
            }
            // `export declare const x: T;` exports an ambient declaration.
            "ambient_declaration" => self.visit(c, prefix),
            "interface_declaration" | "type_alias_declaration" | "enum_declaration" => {
                if let Some(name) = self.name(c) {
                    self.decl(prefix, &name, "type", c);
                }
            }
            "lexical_declaration" | "variable_declaration" => {
                let is_const = c.child_by_field_name("kind").map(|k| self.text(k) == "const").unwrap_or(false);
                let mut cur = c.walk();
                let decls: Vec<Node> = c.children(&mut cur).filter(|d| d.kind() == "variable_declarator").collect();
                for d in decls {
                    let Some(name_node) = d.child_by_field_name("name") else { continue };
                    if name_node.kind() != "identifier" {
                        continue; // destructuring: not a named declaration
                    }
                    let name = self.text(name_node);
                    let is_fn = d.child_by_field_name("value").is_some_and(|v| matches!(v.kind(), "arrow_function" | "function_expression" | "generator_function"));
                    let kind = if is_fn {
                        "function"
                    } else if is_const {
                        "const"
                    } else {
                        "variable"
                    };
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
        TypeScriptAdapter::with_packages(Dialect::TypeScript, vec![Package::named("@grimoire/web", "web")])
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
        let script = "#!/usr/bin/env node
/** # Module contract */
const a = 1;
";
        assert_eq!(a.leading_doc(script).map(|(_, line)| line), Some(2), "a shebang comes before it");
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
var legacy = 1;
function* ids() { yield 1; }
if (counter) { function hidden() {} }
export class Store {
  get(ref: string) { return ref; }
  static open() {}
}
export interface Props { a: number }
export type Ref = string;
enum Kind { A, B }
function inner() { const nested = () => 1; }
"#;
        let p = a.parse_file(src, "web/src/views/Map.tsx");
        let decls: Vec<(String, String)> = p.decls.iter().map(|d| (d.path.clone(), d.kind.clone())).collect();
        assert!(decls.contains(&("MapView".into(), "function".into())));
        assert!(decls.contains(&("helper".into(), "function".into())));
        assert!(decls.contains(&("LIMIT".into(), "const".into())));
        assert!(decls.contains(&("counter".into(), "variable".into())));
        assert!(decls.contains(&("legacy".into(), "variable".into())));
        assert!(decls.contains(&("ids".into(), "function".into())));
        assert!(!decls.iter().any(|(p, _)| p == "hidden"), "a block's contents are not declarations");
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
        // Every comment, markers stripped, with its first line; line comments on consecutive lines are one run.
        let p = adapter().parse_file("/** a\n * b */\nconst x = 1; // c\n// d\n// e\nfunction f() { /* g */ }\n", "web/src/a.ts");
        assert_eq!(p.comments, vec![("a\nb\n".to_string(), 1), ("c\nd\ne\n".to_string(), 3), ("g\n".to_string(), 6)]);
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
    /// {"test": "package-specifiers", "checks": ["crates/extract/src/typescript#imports-as-candidates", "crates/extract/src/typescript#package-exports"]}
    /// ```
    #[test]
    fn package_specifiers() {
        let a = adapter();
        assert_eq!(a.candidates("@grimoire/web/views/Map", "x/y.ts"), ["web/src/views/Map", "web/views/Map", "web/src", "web"]);
        assert!(a.candidates("react-dom/client", "x/y.ts").is_empty());
        assert_eq!(split_package("@scope/name/a/b"), ("@scope/name", "a/b"));
        assert_eq!(split_package("name"), ("name", ""));
    }

    fn js() -> TypeScriptAdapter {
        TypeScriptAdapter::with_packages(Dialect::JavaScript, vec![Package::named("shop", "app")])
    }

    fn decls(p: &Parsed) -> Vec<(String, String, u32)> {
        p.decls.iter().map(|d| (d.path.clone(), d.kind.clone(), d.line)).collect()
    }

    /// # Test contract
    ///
    /// ```json
    /// {"test": "each-language-reads-its-own-files", "checks": ["crates/extract/src/typescript#files", "req:javascript-files"]}
    /// ```
    #[test]
    fn each_language_reads_its_own_files() {
        let (ts, js) = (adapter(), js());
        assert_eq!((ts.language(), js.language()), ("typescript", "javascript"));
        for f in ["a.ts", "a.tsx", "a.mts", "a.cts"] {
            assert!(ts.handles(f) && !js.handles(f), "{f}");
        }
        for f in ["a.js", "a.jsx", "a.mjs", "a.cjs"] {
            assert!(js.handles(f) && !ts.handles(f), "{f}");
        }
        assert!(!ts.handles("a.d.ts") && !ts.handles("a.d.mts") && !js.handles("a.json") && !js.handles("a.ts"));
        // JSX in a .jsx and in a .js file, with the JavaScript grammar.
        let jsx = "import { h } from \"./h.js\";\nexport function Card({ title }) {\n  return <div className=\"card\"><h2>{title}</h2></div>;\n}\nconst Empty = () => <></>;\n";
        for path in ["app/src/Card.jsx", "app/src/Card.js"] {
            let p = js.parse_file(jsx, path);
            assert_eq!(decls(&p), vec![("Card".into(), "function".into(), 2), ("Empty".into(), "function".into(), 5)], "{path}");
            assert_eq!(p.imports, vec![vec!["app/src/h".to_string()]]);
        }
        // A .ts file is read with TypeScript's grammar, where `<T>x` is a type assertion, not JSX.
        let p = ts.parse_file("const n = <number>value;\nexport function after() {}\n", "web/src/a.ts");
        assert_eq!(decls(&p), vec![("n".into(), "const".into(), 1), ("after".into(), "function".into(), 2)]);
        let p = ts.parse_file("export const View = () => <div>{1}</div>;\nexport function after() {}\n", "web/src/a.tsx");
        assert_eq!(decls(&p), vec![("View".into(), "function".into(), 1), ("after".into(), "function".into(), 2)]);
        let x = crate::extract_source(&js, "app/src/Card.jsx", jsx);
        assert_eq!((x.language.as_str(), x.module.as_str()), ("javascript", "app/src/Card"));
    }

    /// # Test contract
    ///
    /// ```json
    /// {"test": "javascript-identity-and-coverage", "checks": ["crates/extract/src/typescript#identity", "crates/extract/src/typescript#coverage", "req:typescript-identity", "req:typescript-coverage"]}
    /// ```
    #[test]
    fn javascript_identity_and_coverage() {
        let a = js();
        assert_eq!(a.module_id("lib/util.mjs"), "lib/util");
        assert_eq!(a.module_id("src/widgets/index.jsx"), "src/widgets");
        assert_eq!(a.module_id("app/src/cart.cjs"), "app/src/cart");
        assert_eq!(a.module_id("index.js"), "");
        assert!(a.expects_contract("app/src/cart.js"));
        assert!(!a.expects_contract("app/src/cart.spec.js"));
        assert!(!a.expects_contract("app/src/__tests__/cart.js"));
        assert!(!a.expects_contract("app/eslint.config.js"));
        assert!(!a.expects_contract("scripts/dev.mjs"), "outside every package");
    }

    /// # Test contract
    ///
    /// ```json
    /// {"test": "an-index-file-imports-from-its-own-directory", "checks": ["crates/extract/src/typescript#imports-as-candidates", "req:typescript-imports-as-candidates"]}
    /// ```
    #[test]
    fn an_index_file_imports_from_its_own_directory() {
        let a = adapter();
        let src = "import { Map } from \"./Map\";\nimport { api } from \"../api.js\";\n";
        let p = a.parse_file(src, "web/src/views/index.ts");
        assert_eq!(p.imports, vec![vec!["web/src/views/Map".to_string()], vec!["web/src/api".to_string()]]);
        let x = crate::extract_source(&a, "web/src/views/index.ts", src);
        assert_eq!(x.module, "web/src/views");
        assert_eq!(x.imports, p.imports, "extraction reads the file, not only its module");
        // An explicit .js extension names the module of the .ts source beside it.
        assert_eq!(a.parse_file("import { a } from \"./a.js\";\n", "src/b.ts").imports, vec![vec!["src/a".to_string()]]);
    }

    /// # Test contract
    ///
    /// ```json
    /// {"test": "require-and-dynamic-import", "checks": ["crates/extract/src/typescript#require-and-dynamic-import", "req:javascript-require"]}
    /// ```
    #[test]
    fn require_and_dynamic_import_are_imports() {
        let a = js();
        let src = r#"const { total } = require("./total");
const lazy = require(`./tpl`);
const named = require("./locales/" + lang);
async function open() {
  const m = await import("./heavy.js");
  return require("../shared/log.cjs");
}
other("./not-an-import");
module.exports = { open };
"#;
        let p = a.parse_file(src, "app/src/cart.cjs");
        let mut imports: Vec<String> = p.imports.iter().map(|c| c.join(",")).collect();
        imports.sort();
        assert_eq!(imports, vec!["app/shared/log", "app/src/heavy", "app/src/total"], "a template and a computed specifier yield none, nor does any other call");
        assert_eq!(decls(&p).iter().map(|d| d.0.as_str()).collect::<Vec<_>>(), vec!["lazy", "named", "open"]);
        // TypeScript's import-equals and a dynamic import in a .ts file.
        let ts = adapter();
        let p = ts.parse_file("import fs = require(\"./fs\");\nexport async function f() { return import(\"./g\"); }\n", "web/src/a.ts");
        assert_eq!(p.imports, vec![vec!["web/src/fs".to_string()], vec!["web/src/g".to_string()]]);
    }

    /// # Test contract
    ///
    /// ```json
    /// {"test": "package-exports", "checks": ["crates/extract/src/typescript#package-exports", "req:typescript-package-exports"]}
    /// ```
    #[test]
    fn a_package_is_entered_as_its_manifest_says() {
        let ui = Package::from_manifest(
            "packages/ui",
            &serde_json::json!({
                "name": "@acme/ui",
                "main": "./dist/cjs/index.js",
                "exports": {
                    ".": { "types": "./types/index.d.ts", "require": "./dist/cjs/index.js", "import": "./lib/index.mjs" },
                    "./button": { "types": "./types/button.d.ts", "import": "./lib/button.mjs" },
                    "./icons/*": "./lib/icons/*.js",
                    "./icons/internal/*": null,
                    "./theme": { "node": { "import": "./lib/theme.node.mjs" }, "default": "./lib/theme.js" },
                    "./package.json": "./package.json",
                    "./node-only": { "import": null, "default": "./lib/fallback.js" },
                    "./typed": { "import": { "types": "./types/typed.d.ts" }, "default": "./lib/typed.js" },
                    "./listed": [{ "worker": null }, "./lib/listed.js", "./lib/other.js"]
                }
            }),
        )
        .unwrap();
        let legacy = Package::from_manifest("libs/legacy", &serde_json::json!({ "name": "legacy", "main": "lib/index.js" })).unwrap();
        let sugar = Package::from_manifest("libs/sugar", &serde_json::json!({ "name": "sugar", "exports": "./sugar.mjs" })).unwrap();
        let dual = Package::from_manifest("libs/dual", &serde_json::json!({ "name": "dual", "exports": { "require": "./cjs/dual.cjs", "import": "./esm/dual.mjs" } })).unwrap();
        let bundled = Package::from_manifest("libs/bundled", &serde_json::json!({ "name": "bundled", "main": "dist/index.js", "module": "src/entry.js" })).unwrap();
        assert!(Package::from_manifest("x", &serde_json::json!({ "private": true })).is_none(), "no name, no package");
        let a = TypeScriptAdapter::with_packages(Dialect::JavaScript, vec![ui, legacy, sugar, dual, bundled]);
        let first = |spec: &str| a.candidates(spec, "app/src/x.js").first().cloned();
        assert_eq!(first("@acme/ui").as_deref(), Some("packages/ui/lib"), "import is preferred, types never chosen");
        assert_eq!(first("@acme/ui/button").as_deref(), Some("packages/ui/lib/button"));
        assert_eq!(first("@acme/ui/icons/star").as_deref(), Some("packages/ui/lib/icons/star"));
        assert_eq!(first("@acme/ui/theme").as_deref(), Some("packages/ui/lib/theme.node"), "nested conditions");
        assert_eq!(first("@acme/ui/node-only").as_deref(), Some("packages/ui/src/node-only"), "the chosen condition's null excludes; default is not tried");
        // The longer pattern excludes the subpath; the conventional places follow.
        assert_eq!(a.candidates("@acme/ui/icons/internal/x", "app/src/x.js"), ["packages/ui/src/icons/internal/x", "packages/ui/icons/internal/x", "packages/ui/src", "packages/ui"]);
        // With no exports, main enters the package itself, never a subpath.
        assert_eq!(a.candidates("legacy", "app/src/x.js"), ["libs/legacy/lib", "libs/legacy/src", "libs/legacy"]);
        assert_eq!(a.candidates("legacy/lib/util", "app/src/x.js"), ["libs/legacy/src/lib/util", "libs/legacy/lib/util", "libs/legacy/src", "libs/legacy"]);
        assert_eq!(first("sugar").as_deref(), Some("libs/sugar/sugar"), "a string export is the package's");
        assert!(a.candidates("lodash", "app/src/x.js").is_empty());
        // A condition naming nothing goes on to the next; only a null stops.
        assert_eq!(first("@acme/ui/typed").as_deref(), Some("packages/ui/lib/typed"));
        // An array by its first entry that names a file.
        assert_eq!(first("@acme/ui/listed").as_deref(), Some("packages/ui/lib/listed"));
        // A top-level conditions object is the package's own entry.
        assert_eq!(first("dual").as_deref(), Some("libs/dual/esm/dual"));
        // module before main.
        assert_eq!(a.candidates("bundled", "app/src/x.js"), ["libs/bundled/src/entry", "libs/bundled/dist", "libs/bundled/src", "libs/bundled"]);
    }

    /// # Test contract
    ///
    /// ```json
    /// {"test": "declarations-with-their-lines", "checks": ["crates/extract/src/typescript#declarations", "req:typescript-declarations"]}
    /// ```
    #[test]
    fn declarations_with_their_lines() {
        let src = "export function MapView() {}
function* ids() { yield 1; }
export const helper = (x: number) => x, LIMIT = 3;
let counter = 0;
var legacy = 1, { a } = obj;
export abstract class Shape {
  abstract area(): number;
  scale(n: number): void;
  scale(n: string): void;
  scale(n: any) { const inner = 1; }
}
interface Props { a: number }
type Ref = string;
enum Kind { A }
declare const injected: string;
if (counter) { function hidden() {} }
for (let i = 0; i < 1; i++) {}
switch (counter) { case 1: const inCase = 2; }
function outer() { const nested = () => 1; }
export declare const BRAND: unique symbol;
export declare function shout(s: string): void;
function pick(a: string): string;
function pick(a: number): number;
function pick(a: any) { return a; }
";
        let got = decls(&adapter().parse_file(src, "web/src/a.ts"));
        let want: Vec<(String, String, u32)> = [
            ("MapView", "function", 1),
            ("ids", "function", 2),
            ("helper", "function", 3),
            ("LIMIT", "const", 3),
            ("counter", "variable", 4),
            ("legacy", "variable", 5),
            ("Shape", "type", 6),
            ("Shape::area", "method", 7),
            ("Shape::scale", "method", 8),
            ("Props", "type", 12),
            ("Ref", "type", 13),
            ("Kind", "type", 14),
            ("injected", "const", 15),
            ("outer", "function", 19),
            ("BRAND", "const", 20),
            ("shout", "function", 21),
            ("pick", "function", 22),
        ]
        .iter()
        .map(|(p, k, l)| (p.to_string(), k.to_string(), *l))
        .collect();
        assert_eq!(got, want, "a block, a loop, a switch case and a body declare nothing; an overloaded method is one entry");
        // The same reading in JavaScript.
        let js_src = "export default function main() {}\nvar count = 0;\nclass Store { get(k) { return k; } static open() {} }\n";
        let got = decls(&js().parse_file(js_src, "app/src/store.js"));
        assert_eq!(
            got,
            vec![
                ("main".into(), "function".into(), 1),
                ("count".into(), "variable".into(), 2),
                ("Store".into(), "type".into(), 3),
                ("Store::get".into(), "method".into(), 3),
                ("Store::open".into(), "method".into(), 3)
            ]
        );
    }

    /// # Test contract
    ///
    /// ```json
    /// {"test": "a-nested-checkout-adds-no-package", "checks": ["crates/extract/src/typescript#imports-as-candidates"]}
    /// ```
    #[test]
    fn a_nested_checkout_adds_no_package() {
        let dir = tempfile::Builder::new().prefix("grimoire-ts-").tempdir().unwrap();
        let file = |rel: &str, text: &str| {
            let p = dir.path().join(rel);
            std::fs::create_dir_all(p.parent().unwrap()).unwrap();
            std::fs::write(p, text).unwrap();
        };
        file("package.json", r#"{"name": "app"}"#);
        file("vendor/sub/package.json", r#"{"name": "dep"}"#);
        std::fs::create_dir_all(dir.path().join("vendor/sub/.git")).unwrap();
        file("libs/other/package.json", r#"{"name": "other"}"#);
        for a in [TypeScriptAdapter::new(dir.path()), TypeScriptAdapter::of(Dialect::JavaScript, dir.path())] {
            assert!(a.candidates("dep", "src/a.ts").is_empty(), "dep lies only inside a checkout of its own");
            assert_eq!(a.candidates("other", "src/a.ts"), ["libs/other/src", "libs/other"]);
        }
    }

    /// # Test contract
    ///
    /// ```json
    /// {"test": "a-gitignore-above-the-repository-hides-no-package", "checks": ["crates/extract/src/typescript#imports-as-candidates", "req:typescript-imports-as-candidates"]}
    /// ```
    #[test]
    fn a_gitignore_above_the_repository_hides_no_package() {
        let dir = tempfile::Builder::new().prefix("grimoire-ts-").tempdir().unwrap();
        let file = |rel: &str, text: &str| {
            let p = dir.path().join(rel);
            std::fs::create_dir_all(p.parent().unwrap()).unwrap();
            std::fs::write(p, text).unwrap();
        };
        // A folder above the repository whose .gitignore would hide its packages, with a line the ignore library cannot
        // parse; the repository has no .git, and its own .gitignore still keeps node_modules out.
        file(".gitignore", "libs/\n{\n");
        file("repo/package.json", r#"{"name": "app"}"#);
        file("repo/.gitignore", "node_modules/\n");
        file("repo/libs/dep/package.json", r#"{"name": "dep"}"#);
        file("repo/node_modules/ext/package.json", r#"{"name": "ext"}"#);
        let root = dir.path().join("repo");
        for a in [TypeScriptAdapter::new(&root), TypeScriptAdapter::of(Dialect::JavaScript, &root)] {
            assert_eq!(a.candidates("dep", "src/a.ts"), ["libs/dep/src", "libs/dep"], "the .gitignore above is not read");
            assert!(a.candidates("ext", "src/a.ts").is_empty(), "the repository's own .gitignore is");
        }
    }
}
