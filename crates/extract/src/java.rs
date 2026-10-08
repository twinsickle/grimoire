//! # Module contract
//!
//! ```json
//! {"purpose":"The Java language adapter: module identity from the file path with a package-info standing for its package, which files carry contracts, the leading Javadoc, and tree-sitter extraction of declarations, and of imports and same-package class uses resolved through the packages the repository declares.","criteria":[{"id":"identity","statement":"A module's id is its repository-relative path without .java, with a package-info.java mapped to its directory."},{"id":"coverage","statement":"Every .java file under a src/main/java directory is expected to carry a contract except package-info.java and module-info.java; no other file is."},{"id":"leading-javadoc","statement":"The leading documentation is the first /** */ comment at the file's top level, outside every type's body, that holds a # Module contract heading, else the first such comment, with the markers stripped and the line it starts on; a /* */ or // comment is never it."},{"id":"comments","statement":"Every comment in the file, line comments merged into runs and block comments, is handed over with its markers stripped and its first line, including those inside a type's or a method's body."},{"id":"declarations","statement":"Classes, interfaces, enums, records and annotation types are declarations of kind type, a nested one under its enclosing type's path; methods, constructors and annotation elements are of kind method, and fields and interface constants of kind field, one per declarator; each has the line it starts on, and nothing inside a method, constructor or initializer body is reported."},{"id":"imports-as-candidates","statement":"An import, single-type, on-demand or static, whose longest leading run of names is a package a repository file declares, outside any nested checkout and not ignored by the repository's own .gitignore files, read as the extractor's walk reads them, and whose next name is a type file of that package, yields candidates in each directory holding that file, the named member or nested type first, then each enclosing name down to the type, the importing file's own directory first among equals; an on-demand import of such a package itself yields one import per type file of it and one for its package-info; any other import yields none."},{"id":"same-package-references","statement":"Every type name a file uses, every capitalised name qualifying a method call, field access or method reference, and every annotation's name, outside its import and package lines, that is a type file of the file's own package yields once the candidates a single-type import of it from that package would, unless it is the file's own name or a single-type import brings that name in; any other name yields none."}]}
//! ```

use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;

use grimoire_core::Decl;
use tree_sitter::{Node, Parser, Tree};

use crate::{Adapter, Parsed};

/// Where one package's files sit: the directory, the class files in it by
/// module id, and whether it holds a `package-info.java`.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct PackageDir {
    pub dir: String,
    pub types: Vec<String>,
    pub has_info: bool,
}

pub struct JavaAdapter {
    /// Each package name the repository declares, with every directory that
    /// holds files declaring it (a package may span `src/main/java` and
    /// `src/test/java`, or several modules of a build).
    packages: BTreeMap<String, Vec<PackageDir>>,
}

fn parse_tree(source: &str) -> Option<Tree> {
    let mut parser = Parser::new();
    parser.set_language(&tree_sitter_java::LANGUAGE.into()).expect("java grammar loads");
    parser.parse(source, None)
}

/// The package a file declares, or `None` for the unnamed package.
fn package_name(root: Node, src: &[u8]) -> Option<String> {
    let mut cursor = root.walk();
    let decl = root.children(&mut cursor).find(|c| c.kind() == "package_declaration")?;
    let mut cursor = decl.walk();
    let name = decl.named_children(&mut cursor).find(|c| matches!(c.kind(), "identifier" | "scoped_identifier"))?;
    Some(name.utf8_text(src).ok()?.chars().filter(|c| !c.is_whitespace()).collect())
}

fn file_name(rel: &str) -> &str {
    rel.rsplit('/').next().unwrap_or(rel)
}

fn parent(rel: &str) -> &str {
    rel.rsplit_once('/').map(|(d, _)| d).unwrap_or("")
}

fn join(dir: &str, rest: &str) -> String {
    if dir.is_empty() {
        rest.to_string()
    } else {
        format!("{dir}/{rest}")
    }
}

impl JavaAdapter {
    /// Read the package declaration of every `.java` file under the root,
    /// honouring the repository's own `.gitignore` files alone and stopping at nested checkouts, and remember where each package's files sit.
    /// Reads only the files' text; parses nothing else.
    pub fn new(root: &Path) -> Self {
        let mut files = Vec::new();
        // As the extractor walks: by the repository's own .gitignore files alone, and a folder below the root that is a checkout
        // of its own is not this repository's.
        let walker = crate::own_ignores(root).hidden(true).filter_entry(|e| e.depth() == 0 || !(e.file_type().is_some_and(|t| t.is_dir()) && crate::is_checkout(e.path()))).build();
        for entry in walker.flatten() {
            if !entry.file_type().is_some_and(|t| t.is_file()) || !entry.file_name().to_string_lossy().ends_with(".java") {
                continue;
            }
            let Some(rel) = crate::relative(root, entry.path()) else { continue };
            let Ok(text) = std::fs::read_to_string(entry.path()) else { continue };
            let Some(tree) = parse_tree(&text) else { continue };
            if let Some(package) = package_name(tree.root_node(), text.as_bytes()) {
                files.push((package, rel));
            }
        }
        Self::with_files(files)
    }

    /// An adapter over known files: each a declared package and the
    /// repository-relative path of a file declaring it.
    pub fn with_files(files: Vec<(String, String)>) -> Self {
        let mut packages: BTreeMap<String, Vec<PackageDir>> = BTreeMap::new();
        for (package, rel) in files {
            let dir = parent(&rel).to_string();
            let dirs = packages.entry(package).or_default();
            let at = match dirs.iter().position(|d| d.dir == dir) {
                Some(i) => i,
                None => {
                    dirs.push(PackageDir { dir: dir.clone(), ..PackageDir::default() });
                    dirs.len() - 1
                }
            };
            let entry = &mut dirs[at];
            match file_name(&rel) {
                "package-info.java" => entry.has_info = true,
                "module-info.java" => {}
                _ => entry.types.push(module_id_of(&rel)),
            }
        }
        for dirs in packages.values_mut() {
            dirs.sort_by(|a, b| a.dir.cmp(&b.dir));
            for d in dirs.iter_mut() {
                d.types.sort();
                d.types.dedup();
            }
        }
        JavaAdapter { packages }
    }

    /// The directories holding a package, the importing file's own first.
    fn dirs_of(&self, package: &str, from_dir: &str) -> Vec<&PackageDir> {
        let mut dirs: Vec<&PackageDir> = self.packages.get(package).map(|d| d.iter().collect()).unwrap_or_default();
        dirs.sort_by_key(|d| d.dir != from_dir);
        dirs
    }

    /// The imports one import declaration stands for, each as its candidate
    /// module ids, from a file in `from_dir`. `names` is the dotted name
    /// without `static` or a trailing `.*`.
    fn imports(&self, names: &[&str], on_demand: bool, from_dir: &str) -> Vec<Vec<String>> {
        // The longest leading run that is a package this repository declares and that the
        // rest fits: an on-demand import of the package itself, or a name whose first part is
        // a type file of it. Anything else, `com.acme.vendor.Widget` from a library beside a
        // local `com.acme`, is not this repository's.
        let holds = |d: &PackageDir, name: &str| d.types.iter().any(|t| t.rsplit('/').next() == Some(name));
        let fits = |k: usize| {
            let Some(dirs) = self.packages.get(&names[..k].join(".")) else { return false };
            match names.get(k) {
                None => on_demand,
                Some(first) => dirs.iter().any(|d| holds(d, first)),
            }
        };
        let Some(k) = (1..=names.len()).rev().find(|k| fits(*k)) else { return Vec::new() };
        let package = names[..k].join(".");
        let rest = &names[k..];
        let dirs = self.dirs_of(&package, from_dir);
        if rest.is_empty() {
            let mut out = Vec::new();
            for d in &dirs {
                out.extend(d.types.iter().map(|t| vec![t.clone()]));
                if d.has_info {
                    out.push(vec![d.dir.clone()]);
                }
            }
            return out;
        }
        // The named type, a member or a nested type of it first; only the directories holding its file.
        let dirs: Vec<&&PackageDir> = dirs.iter().filter(|d| holds(d, rest[0])).collect();
        let mut cands = Vec::new();
        for j in (1..=rest.len()).rev() {
            for d in &dirs {
                cands.push(join(&d.dir, &rest[..j].join("/")));
            }
        }
        vec![cands]
    }

    /// The classes of the file's own package it uses without an import, each
    /// once, as the candidates a single-type import of it would give. Read
    /// off the parse: type names, capitalised qualifiers and annotation names.
    fn same_package(&self, root: Node, src: &[u8], module_id: &str, from_dir: &str, imported: &BTreeSet<String>) -> Vec<Vec<String>> {
        let Some(package) = package_name(root, src) else { return Vec::new() };
        let Some(dirs) = self.packages.get(&package) else { return Vec::new() };
        // A file's mention of its own class is no edge.
        let own = file_name(module_id);
        let names = referenced_names(root, src);
        let mut out = Vec::new();
        for name in &names {
            let is_type = dirs.iter().any(|d| d.types.iter().any(|t| file_name(t) == name));
            if !is_type || name == own || imported.contains(name) {
                continue;
            }
            let qualified: Vec<&str> = package.split('.').chain([name.as_str()]).collect();
            out.extend(self.imports(&qualified, false, from_dir));
        }
        out
    }
}

/// The names a file uses that may name a class, each once: every type
/// identifier, every capitalised identifier qualifying a call, a field access
/// or a method reference, and every annotation's name. Import and package
/// lines are skipped.
fn referenced_names(root: Node, src: &[u8]) -> BTreeSet<String> {
    let mut names = BTreeSet::new();
    // Iterative, so a deeply nested expression cannot exhaust the stack.
    let mut stack = vec![root];
    while let Some(n) = stack.pop() {
        let named = match n.kind() {
            "import_declaration" | "package_declaration" => continue,
            "type_identifier" => true,
            "identifier" => {
                n.parent().is_some_and(|p| {
                    let is = |o: Option<Node>| o.is_some_and(|o| o.id() == n.id());
                    match p.kind() {
                        "method_invocation" | "field_access" => is(p.child_by_field_name("object")),
                        "method_reference" => is(p.named_child(0)),
                        "marker_annotation" | "annotation" => is(p.child_by_field_name("name")),
                        _ => false,
                    }
                }) && n.utf8_text(src).is_ok_and(|t| t.starts_with(|c: char| c.is_ascii_uppercase()))
            }
            _ => false,
        };
        if named {
            names.extend(n.utf8_text(src).ok().map(str::to_string));
        }
        let mut cursor = n.walk();
        stack.extend(n.children(&mut cursor));
    }
    names
}

/// The module id of a repository-relative `.java` path.
fn module_id_of(rel: &str) -> String {
    if file_name(rel) == "package-info.java" {
        return parent(rel).to_string();
    }
    rel.strip_suffix(".java").unwrap_or(rel).to_string()
}

/// Whether the path lies under a `src/main/java` directory.
fn in_main_sources(rel: &str) -> bool {
    rel.starts_with("src/main/java/") || rel.contains("/src/main/java/")
}

impl Adapter for JavaAdapter {
    fn language(&self) -> &'static str {
        "java"
    }

    fn tier(&self) -> u8 {
        2
    }

    fn handles(&self, rel_path: &str) -> bool {
        rel_path.ends_with(".java")
    }

    fn module_id(&self, rel_path: &str) -> String {
        module_id_of(rel_path)
    }

    fn expects_contract(&self, rel_path: &str) -> bool {
        in_main_sources(rel_path) && !matches!(file_name(rel_path), "package-info.java" | "module-info.java")
    }

    fn leading_doc(&self, source: &str) -> Option<(String, u32)> {
        let tree = parse_tree(source)?;
        let root = tree.root_node();
        let mut cursor = root.walk();
        let docs: Vec<(String, u32)> = root
            .children(&mut cursor)
            .filter(|c| c.kind() == "block_comment")
            .filter_map(|c| {
                let raw = c.utf8_text(source.as_bytes()).ok()?;
                // `/**/` is an empty plain comment, not a Javadoc.
                (raw.starts_with("/**") && raw != "/**/").then(|| (crate::strip_slash_comment(raw), c.start_position().row as u32 + 1))
            })
            .collect();
        let with_heading = docs.iter().position(|(t, line)| crate::contract::find(t, *line).is_some());
        docs.into_iter().nth(with_heading.unwrap_or(0))
    }

    fn parse(&self, source: &str, module_id: &str) -> Parsed {
        let Some(tree) = parse_tree(source) else { return Parsed::default() };
        let src = source.as_bytes();
        let root = tree.root_node();
        let mut out = Parsed::default();
        // The importing file's directory: a package-info's module id is that directory, as
        // one of its package's; any other file's is its parent.
        let own = package_name(root, src).and_then(|p| self.packages.get(&p)).is_some_and(|dirs| dirs.iter().any(|d| d.dir == module_id && d.has_info));
        let from_dir = if own { module_id } else { parent(module_id) }.to_string();
        // The simple names single-type imports bring in: Java's import shadows a same-package class.
        let mut imported: BTreeSet<String> = BTreeSet::new();
        let mut cursor = root.walk();
        for c in root.children(&mut cursor) {
            if c.kind() == "import_declaration" {
                let mut cur = c.walk();
                let on_demand = c.children(&mut cur).any(|n| n.kind() == "asterisk");
                let mut cur = c.walk();
                let is_static = c.children(&mut cur).any(|n| n.kind() == "static");
                let mut cur = c.walk();
                let Some(name) = c.named_children(&mut cur).find(|n| matches!(n.kind(), "identifier" | "scoped_identifier")) else { continue };
                let text: String = name.utf8_text(src).unwrap_or_default().chars().filter(|ch| !ch.is_whitespace()).collect();
                let names: Vec<&str> = text.split('.').collect();
                if !on_demand && !is_static {
                    imported.extend(names.last().map(|n| n.to_string()));
                }
                out.imports.extend(self.imports(&names, on_demand, &from_dir));
            }
        }
        out.imports.extend(self.same_package(root, src, module_id, &from_dir, &imported));
        let mut w = Walker { src, out };
        w.members(root, &mut Vec::new());
        w.out.comments = crate::slash_comments(root, src, |k| k == "line_comment" || k == "block_comment");
        w.out
    }
}

/// The node kinds that declare a type.
const TYPES: [&str; 5] = ["class_declaration", "interface_declaration", "enum_declaration", "record_declaration", "annotation_type_declaration"];

struct Walker<'a> {
    src: &'a [u8],
    out: Parsed,
}

impl Walker<'_> {
    fn text(&self, n: Node) -> String {
        n.utf8_text(self.src).unwrap_or("").to_string()
    }

    fn decl(&mut self, prefix: &[String], name: &str, kind: &str, n: Node) {
        let path = if prefix.is_empty() { name.to_string() } else { format!("{}::{name}", prefix.join("::")) };
        self.out.decls.push(Decl { path, kind: kind.to_string(), line: n.start_position().row as u32 + 1, file: String::new() });
    }

    /// The declarations directly in a scope: the file, or a type's body.
    /// Bodies of methods, constructors and initializers are never entered.
    fn members(&mut self, scope: Node, prefix: &mut Vec<String>) {
        let mut cursor = scope.walk();
        let children: Vec<Node> = scope.children(&mut cursor).collect();
        for c in children {
            match c.kind() {
                k if TYPES.contains(&k) => {
                    let Some(name) = c.child_by_field_name("name").map(|n| self.text(n)) else { continue };
                    self.decl(prefix, &name, "type", c);
                    if let Some(body) = c.child_by_field_name("body") {
                        prefix.push(name);
                        self.members(body, prefix);
                        prefix.pop();
                    }
                }
                // An enum's members after its constants.
                "enum_body_declarations" => self.members(c, prefix),
                "method_declaration" | "constructor_declaration" | "compact_constructor_declaration" | "annotation_type_element_declaration" => {
                    if let Some(name) = c.child_by_field_name("name").map(|n| self.text(n)) {
                        self.decl(prefix, &name, "method", c);
                    }
                }
                "field_declaration" | "constant_declaration" => {
                    let mut cur = c.walk();
                    let declarators: Vec<Node> = c.children_by_field_name("declarator", &mut cur).collect();
                    for d in declarators {
                        if let Some(name) = d.child_by_field_name("name").map(|n| self.text(n)) {
                            self.decl(prefix, &name, "field", d);
                        }
                    }
                }
                _ => {}
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const MAIN: &str = "core/src/main/java/com/example/core";

    fn adapter() -> JavaAdapter {
        JavaAdapter::with_files(vec![
            ("com.example.core".into(), format!("{MAIN}/Greeter.java")),
            ("com.example.core".into(), format!("{MAIN}/package-info.java")),
            ("com.example.core.util".into(), format!("{MAIN}/util/Strings.java")),
            ("com.example.core.util".into(), format!("{MAIN}/util/Numbers.java")),
            ("com.example.core".into(), "core/src/test/java/com/example/core/GreeterTest.java".into()),
        ])
    }

    /// # Test contract
    ///
    /// ```json
    /// {"test": "identity-and-coverage", "checks": ["crates/extract/src/java#identity", "crates/extract/src/java#coverage"]}
    /// ```
    #[test]
    fn identity_and_coverage() {
        let a = adapter();
        assert_eq!(a.module_id(&format!("{MAIN}/Greeter.java")), format!("{MAIN}/Greeter"));
        assert_eq!(a.module_id(&format!("{MAIN}/package-info.java")), MAIN, "a package-info stands for its package");
        assert_eq!(a.module_id("src/main/java/package-info.java"), "src/main/java");
        assert!(a.expects_contract(&format!("{MAIN}/Greeter.java")));
        assert!(a.expects_contract("src/main/java/com/x/Root.java"), "a single-module build at the repository's root");
        assert!(!a.expects_contract(&format!("{MAIN}/package-info.java")));
        assert!(!a.expects_contract("core/src/main/java/module-info.java"));
        assert!(!a.expects_contract("core/src/test/java/com/example/core/GreeterTest.java"));
        assert!(!a.expects_contract("tools/Gen.java"), "outside src/main/java");
        assert!(!a.expects_contract("core/src/main/javascript/x.java"));
        assert!(a.handles("tools/Gen.java") && !a.handles("pom.xml"));
    }

    /// # Test contract
    ///
    /// ```json
    /// {"test": "the-class-javadoc-leads", "checks": ["crates/extract/src/java#leading-javadoc"]}
    /// ```
    #[test]
    fn the_class_javadoc_leads() {
        let a = adapter();
        let src = "/*\n * Licensed to the Apache Software Foundation.\n */\npackage com.example.core;\n\nimport java.util.List;\n\n/**\n * Greets.\n *\n * # Module contract\n *\n * ```json\n * {\"purpose\":\"P\",\"criteria\":[{\"id\":\"a\",\"statement\":\"S\"}]}\n * ```\n */\n@Component\npublic class Greeter {\n    /** # Module contract inside a body is not the file's. */\n    void f() {}\n}\n";
        let (text, line) = a.leading_doc(src).unwrap();
        assert_eq!(line, 8);
        assert!(text.starts_with("\nGreets.\n\n# Module contract\n"), "{text:?}");
        let x = crate::extract_source(&a, &format!("{MAIN}/Greeter.java"), src);
        assert_eq!(x.contract_line, Some(11), "{:?}", x.contract);
        assert!(x.contract.as_ref().is_some_and(|c| c.is_ok()));
        // A package-info's Javadoc sits before its package line.
        let info = "/**\n * # Module contract\n *\n * ```json\n * {\"purpose\":\"P\",\"criteria\":[{\"id\":\"a\",\"statement\":\"S\"}]}\n * ```\n */\n@Version(\"1.0\")\npackage com.example.core;\n";
        assert_eq!(crate::extract_source(&a, &format!("{MAIN}/package-info.java"), info).contract_line, Some(2));
        // A file Javadoc that holds no heading gives way to one that does.
        let two = "/** Copyright. */\npackage p;\n/**\n * # Module contract\n */\nclass A {}\n";
        assert_eq!(a.leading_doc(two).unwrap().1, 3);
        // With no Javadoc holding the heading, the first one leads.
        assert_eq!(
            a.leading_doc(
                "/* Licence. */
package p;
/** First. */
class A {
}
/** Second. */
class B {}
"
            )
            .unwrap(),
            (
                "First.
"
                .to_string(),
                3
            )
        );
        // A licence header alone is no leading documentation.
        assert!(a.leading_doc("/* Licence. */\npackage p;\n// note\nclass A {}\n").is_none());
    }

    /// # Test contract
    ///
    /// ```json
    /// {"test": "every-comment-reaches-the-test-blocks", "checks": ["crates/extract/src/java#comments"]}
    /// ```
    #[test]
    fn every_comment_reaches_the_test_blocks() {
        let src = "/* Licence. */\npackage com.example.core;\n\nimport org.junit.jupiter.api.Test;\n\nclass GreeterTest {\n    /**\n     * # Test contract\n     *\n     * ```json\n     * {\"test\": \"greets-by-name\", \"checks\": [\"core/src/main/java/com/example/core/Greeter#greets\"]}\n     * ```\n     */\n    @Test\n    void greetsByName() {\n        // one\n        // two\n    }\n}\n";
        let p = adapter().parse(src, "core/src/test/java/com/example/core/GreeterTest");
        let starts: Vec<u32> = p.comments.iter().map(|c| c.1).collect();
        assert_eq!(starts, vec![1, 7, 16], "{:?}", p.comments);
        assert_eq!(p.comments[2].0, "one\ntwo\n", "a run of line comments is one");
        let x = crate::extract_source(&adapter(), "core/src/test/java/com/example/core/GreeterTest.java", src);
        assert_eq!(x.tests.len(), 1, "{:?}", x.test_errors);
        assert_eq!((x.tests[0].name.as_str(), x.tests[0].line), ("greets-by-name", 8));
        assert!(!x.expects_contract);
    }

    /// # Test contract
    ///
    /// ```json
    /// {"test": "declarations-with-their-lines", "checks": ["crates/extract/src/java#declarations"]}
    /// ```
    #[test]
    fn declarations_with_their_lines() {
        let src = r#"package com.example.core;

public class Greeter {
    private int count, limit;
    public Greeter() {}
    public String greet(String name) {
        class Local {}
        Runnable r = new Runnable() { public void run() {} };
        return name;
    }
    enum Tone {
        LOUD, SOFT;
        int volume() { return 1; }
    }
    static { int ignored = 0; }
}
interface Named { String NAME = "n"; String name(); }
record Pair(int a, int b) { Pair { } }
@interface Marker { int value() default 0; }
"#;
        let p = adapter().parse(src, &format!("{MAIN}/Greeter"));
        let decls: Vec<(String, String, u32)> = p.decls.iter().map(|d| (d.path.clone(), d.kind.clone(), d.line)).collect();
        let expect = [
            ("Greeter", "type", 3),
            ("Greeter::count", "field", 4),
            ("Greeter::limit", "field", 4),
            ("Greeter::Greeter", "method", 5),
            ("Greeter::greet", "method", 6),
            ("Greeter::Tone", "type", 11),
            ("Greeter::Tone::volume", "method", 13),
            ("Named", "type", 17),
            ("Named::NAME", "field", 17),
            ("Named::name", "method", 17),
            ("Pair", "type", 18),
            ("Pair::Pair", "method", 18),
            ("Marker", "type", 19),
            ("Marker::value", "method", 19),
        ];
        let expect: Vec<(String, String, u32)> = expect.iter().map(|(p, k, l)| (p.to_string(), k.to_string(), *l)).collect();
        assert_eq!(decls, expect);
    }

    /// # Test contract
    ///
    /// ```json
    /// {"test": "a-nested-checkout-declares-nothing-here", "checks": ["crates/extract/src/java#imports-as-candidates"]}
    /// ```
    #[test]
    fn a_nested_checkout_declares_nothing_here() {
        let dir = tempfile::Builder::new().prefix("grimoire-java-").tempdir().unwrap();
        let file = |rel: &str, text: &str| {
            let p = dir.path().join(rel);
            std::fs::create_dir_all(p.parent().unwrap()).unwrap();
            std::fs::write(p, text).unwrap();
        };
        file(
            "src/main/java/p/A.java",
            "package p;
import q.B;
import p.C;
class A {}
",
        );
        file(
            "src/main/java/p/C.java",
            "package p;
class C {}
",
        );
        file(
            "vendor/sub/src/main/java/q/B.java",
            "package q;
class B {}
",
        );
        std::fs::create_dir_all(dir.path().join("vendor/sub/.git")).unwrap();
        let a = JavaAdapter::new(dir.path());
        let p = a.parse(
            "package p;
import q.B;
import p.C;
class A {}
",
            "src/main/java/p/A",
        );
        assert_eq!(p.imports, vec![vec!["src/main/java/p/C".to_string()]], "q is declared only inside a checkout of its own");
    }

    /// # Test contract
    ///
    /// ```json
    /// {"test": "imports-resolve-through-declared-packages", "checks": ["crates/extract/src/java#imports-as-candidates"]}
    /// ```
    #[test]
    fn imports_resolve_through_declared_packages() {
        let a = adapter();
        let src = "package com.example.core;\n\nimport java.util.List;\nimport org.osgi.service.component.annotations.Component;\nimport com.example.core.util.Strings;\nimport static com.example.core.util.Strings.shout;\nimport com.example.core.util.*;\nimport com.example.core.Greeter.Tone;\n\nclass GreeterTest {}\n";
        let p = a.parse(src, "core/src/test/java/com/example/core/GreeterTest");
        let util = format!("{MAIN}/util");
        assert_eq!(p.imports[0], vec![format!("{util}/Strings")], "a single-type import: the type");
        assert_eq!(p.imports[1], vec![format!("{util}/Strings/shout"), format!("{util}/Strings")], "a static import's member, then its type");
        assert_eq!(p.imports[2..4], [vec![format!("{util}/Numbers")], vec![format!("{util}/Strings")]], "an on-demand import stands for every type of the package");
        // A nested type, with the package spanning main and test: only where its outer type's file is.
        assert_eq!(p.imports[4], vec![format!("{MAIN}/Greeter/Tone"), format!("{MAIN}/Greeter")]);
        assert_eq!(p.imports.len(), 5, "java.util and org.osgi yield nothing: {:?}", p.imports);
        // A library's package below a local one, or a type the local package has no file of, is not this repository's.
        let vendor = a.parse(
            "package x;
import com.example.vendor.Widget;
import com.example.core.Missing;
import com.example.vendor.*;
import static com.example.core.util.Strings.*;
class A {}
",
            "src/main/java/x/A",
        );
        assert_eq!(vendor.imports, vec![vec![format!("{util}/Strings")]], "only the static on-demand import of a local type");
        // The importer's own directory first, when both hold the type.
        let both = JavaAdapter::with_files(vec![("p".into(), "a/src/main/java/p/T.java".into()), ("p".into(), "b/src/main/java/p/T.java".into())]);
        let from_b = both.parse(
            "package p;
import p.T.Inner;
class U {}
",
            "b/src/main/java/p/U",
        );
        assert_eq!(from_b.imports, vec![vec!["b/src/main/java/p/T/Inner".to_string(), "a/src/main/java/p/T/Inner".to_string(), "b/src/main/java/p/T".to_string(), "a/src/main/java/p/T".to_string()]]);
        // A file declaring no type is not a package-info: its directory is still its parent.
        let empty = both.parse(
            "package p;
import p.T;
",
            "b/src/main/java/p/Empty",
        );
        assert_eq!(empty.imports, vec![vec!["b/src/main/java/p/T".to_string(), "a/src/main/java/p/T".to_string()]]);
        // An on-demand import of a package with a package-info stands for it too.
        let on_demand = a.parse("package x;\nimport com.example.core.*;\nclass A {}\n", "src/main/java/x/A");
        assert!(on_demand.imports.contains(&vec![MAIN.to_string()]) && on_demand.imports.contains(&vec![format!("{MAIN}/Greeter")]), "{:?}", on_demand.imports);
    }

    /// # Test contract
    ///
    /// ```json
    /// {"test": "same-package-classes-are-used-without-imports", "checks": ["crates/extract/src/java#same-package-references"]}
    /// ```
    #[test]
    fn same_package_classes_are_used_without_imports() {
        let a = JavaAdapter::with_files(vec![
            ("p".into(), "src/main/java/p/A.java".into()),
            ("p".into(), "src/main/java/p/B.java".into()),
            ("p".into(), "src/main/java/p/C.java".into()),
            ("p".into(), "src/main/java/p/D.java".into()),
            ("p".into(), "src/main/java/p/E.java".into()),
            ("p".into(), "src/main/java/p/Marker.java".into()),
            ("p".into(), "src/main/java/p/T.java".into()),
            ("p".into(), "src/main/java/p/Shadowed.java".into()),
            ("p".into(), "src/main/java/p/Unused.java".into()),
            ("p".into(), "src/test/java/p/ATest.java".into()),
            ("q".into(), "src/main/java/q/Shadowed.java".into()),
        ]);
        let src = r#"package p;
import q.Shadowed;
import java.util.List;
@Marker
public class A<T> extends B {
    private List<C> cs;
    private Shadowed s;
    A self() { return new A<T>(); }
    <U> U id(U u) { return u; }
    String go(T t, String x) {
        D.run();
        Runnable r = E::work;
        b.go();
        return x + D.NAME;
    }
}
"#;
        let p = a.parse(src, "src/main/java/p/A");
        let pkg = |n: &str| vec![format!("src/main/java/p/{n}")];
        // T is a type parameter here, but a class of the package bears the name: the over-count the parse accepts.
        assert_eq!(p.imports, vec![vec!["src/main/java/q/Shadowed".to_string()], pkg("B"), pkg("C"), pkg("D"), pkg("E"), pkg("Marker"), pkg("T")], "{:?}", p.imports);
        // A test in the same package under another source root uses the class it tests.
        let t = a.parse("package p;\nclass ATest {\n    void t() { new A().self(); }\n}\n", "src/test/java/p/ATest");
        assert_eq!(t.imports, vec![pkg("A")]);
        // The unnamed package and a package of no repository file make no same-package edge.
        assert!(a.parse("class X { B b; }\n", "X").imports.is_empty());
        assert!(a.parse("package z;\nclass X { B b; }\n", "src/main/java/z/X").imports.is_empty());
    }

    /// # Test contract
    ///
    /// ```json
    /// {"test": "a-gitignore-above-the-repository-hides-no-package", "checks": ["crates/extract/src/java#imports-as-candidates", "req:java-imports-as-candidates"]}
    /// ```
    #[test]
    fn a_gitignore_above_the_repository_hides_no_package() {
        let dir = tempfile::Builder::new().prefix("grimoire-java-").tempdir().unwrap();
        let file = |rel: &str, text: &str| {
            let p = dir.path().join(rel);
            std::fs::create_dir_all(p.parent().unwrap()).unwrap();
            std::fs::write(p, text).unwrap();
        };
        // A folder above the repository whose .gitignore would hide a package, with a line the ignore library cannot
        // parse; the repository has no .git, and its own .gitignore still applies.
        file(".gitignore", "lib/\n{\n");
        file("repo/.gitignore", "out/\n");
        file("repo/src/main/java/p/A.java", "package p;\nimport q.B;\nimport r.C;\nclass A {}\n");
        file("repo/lib/src/main/java/q/B.java", "package q;\nclass B {}\n");
        file("repo/out/src/main/java/r/C.java", "package r;\nclass C {}\n");
        let a = JavaAdapter::new(&dir.path().join("repo"));
        let p = a.parse("package p;\nimport q.B;\nimport r.C;\nclass A {}\n", "src/main/java/p/A");
        assert_eq!(p.imports, vec![vec!["lib/src/main/java/q/B".to_string()]], "q is read past the .gitignore above; r is ignored by the repository's own");
    }
}
