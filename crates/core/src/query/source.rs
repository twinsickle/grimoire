//! # Module contract
//!
//! ```json
//! {"purpose":"Place nodes in the source: where a node is written, and what the graph attaches to each line of a file.","criteria":[{"id":"module-source","statement":"module_source returns the file and line of a module, criterion, file, declaration or test, with the contract heading's line for criteria and the test block's heading for a test."},{"id":"source-annotations","statement":"The annotations of a file are the module contract at its heading line linking the module's criteria, each test block at its line linking what it checks, and each declaration at its line, ordered by line; each carries the node's reference and one-line title."}]}
//! ```

use serde::Serialize;

use crate::graph::{Graph, Node, SourceLocation};
use crate::model::*;

pub fn module_source(g: &Graph, r: &Reference) -> Option<SourceLocation> {
    g.source_of(r)
}

/// What a line of a file is, for the annotated source.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[serde(rename_all = "lowercase")]
pub enum AnnotationKind {
    Contract,
    Test,
    Decl,
}

/// A node another annotation links: its reference and one-line title.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
pub struct Linked {
    pub r#ref: Reference,
    pub title: Option<String>,
}

/// One line of a file that a node attaches to.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
pub struct Annotation {
    pub line: u32,
    pub r#ref: Reference,
    pub kind: AnnotationKind,
    pub title: String,
    /// For a contract, its criteria; for a test block, what it checks.
    pub links: Vec<Linked>,
}

/// Where the graph attaches nodes to the file at `path`: the module contract,
/// each test block and each declaration, ordered by line.
pub fn source_annotations(g: &Graph, path: &str) -> Vec<Annotation> {
    let linked = |r: &Reference| Linked { r#ref: r.clone(), title: g.get(r).map(Node::title) };
    let mut out = Vec::new();
    for (r, node) in &g.nodes {
        match node {
            Node::Module(m) => {
                if let Some((file, line)) = &m.contract_at {
                    if file == path {
                        let links = m.criteria.iter().map(|c| linked(&Reference::criterion(&c.module, &c.id))).collect();
                        out.push(Annotation { line: *line, r#ref: r.clone(), kind: AnnotationKind::Contract, title: node.title(), links });
                    }
                }
            }
            Node::Test(t) if t.file == path => {
                let links = g.out(r).filter(|e| e.relation == Relation::Checks).map(|e| linked(&e.to)).collect();
                out.push(Annotation { line: t.line, r#ref: r.clone(), kind: AnnotationKind::Test, title: t.name.clone(), links });
            }
            Node::File(f) if f.path == path => {
                for d in &f.decls {
                    out.push(Annotation { line: d.line, r#ref: Reference::decl(path, &d.path), kind: AnnotationKind::Decl, title: format!("{} {}", d.kind, d.path), links: vec![] });
                }
            }
            _ => {}
        }
    }
    out.sort_by(|a, b| a.line.cmp(&b.line).then_with(|| a.r#ref.cmp(&b.r#ref)));
    out
}
