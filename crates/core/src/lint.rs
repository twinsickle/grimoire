//! # Module contract
//!
//! ```json
//! {"purpose":"Report dangling edges, uncovered nodes and malformed store files over the joined graph.","criteria":[{"id":"stable-order","statement":"Findings are ordered by rule, then reference, then edge, so two runs over the same graph produce identical output."},{"id":"closed-exempt","statement":"A closed ticket's edges are never reported as dangling."},{"id":"same-check","statement":"The check run by the CLI and the lint tool served by the daemon are this one function."},{"id":"grimoire-material-only","statement":"A module is reported as unlinked only when it is expected to carry a contract or carries one; test files and build scripts are not part of the grimoire."}]}
//! ```

use std::collections::HashSet;

use serde::{Deserialize, Serialize};

use crate::graph::{Edge, Graph, Node};
use crate::model::*;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Rule {
    Dangling,
    Uncovered,
    Malformed,
}

impl Rule {
    pub const ALL: [Rule; 3] = [Rule::Dangling, Rule::Uncovered, Rule::Malformed];
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FindingEdge {
    pub from: Reference,
    pub to: Reference,
    pub relation: Relation,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Finding {
    pub rule: Rule,
    /// The node the finding is about. For a malformed store file, a
    /// reference of the kind the file would have held, with the file's slug.
    pub r#ref: Reference,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub edge: Option<FindingEdge>,
    pub reason: String,
}

impl Finding {
    fn node(rule: Rule, r: &Reference, reason: impl Into<String>) -> Finding {
        Finding { rule, r#ref: r.clone(), edge: None, reason: reason.into() }
    }
    fn dangling(e: &Edge) -> Finding {
        Finding {
            rule: Rule::Dangling,
            r#ref: e.from.clone(),
            edge: Some(FindingEdge { from: e.from.clone(), to: e.to.clone(), relation: e.relation }),
            reason: format!("{} {} {}, which does not exist", e.from, e.relation, e.to),
        }
    }
    fn sort_key(&self) -> (Rule, String, String) {
        (self.rule, self.r#ref.to_string(), self.edge.as_ref().map(|e| format!("{}{}", e.relation, e.to)).unwrap_or_default())
    }
}

/// Run the selected rules over the whole graph.
pub fn lint(g: &Graph, rules: &[Rule]) -> Vec<Finding> {
    let mut out = Vec::new();
    if rules.contains(&Rule::Dangling) {
        dangling(g, &mut out);
    }
    if rules.contains(&Rule::Uncovered) {
        uncovered(g, &mut out);
    }
    if rules.contains(&Rule::Malformed) {
        for m in &g.malformed {
            let r = malformed_ref(&m.path);
            out.push(Finding::node(Rule::Malformed, &r, format!("{}: {}", m.path, m.reason)));
        }
    }
    out.sort_by_key(Finding::sort_key);
    out.dedup();
    out
}

/// Findings that touch any of `refs`: the finding's own node, or the target
/// of its edge. What `close_ticket` returns.
pub fn lint_touching(g: &Graph, refs: &[Reference]) -> Vec<Finding> {
    let set: HashSet<&Reference> = refs.iter().collect();
    let modules: HashSet<&str> = refs.iter().filter(|r| r.kind == Kind::Module).map(|r| r.id.as_str()).collect();
    // A criterion counts as touched when its module is.
    let touched = |r: &Reference| set.contains(r) || (r.kind == Kind::Criterion && r.split_hash().is_some_and(|(m, _)| modules.contains(m)));
    lint(g, &Rule::ALL).into_iter().filter(|f| touched(&f.r#ref) || f.edge.as_ref().is_some_and(|e| touched(&e.to))).collect()
}

/// Dangling findings for the edges one node owns. What a write returns as
/// `warnings`.
pub fn dangling_from(g: &Graph, r: &Reference) -> Vec<Finding> {
    let exempt = matches!(g.get(r), Some(Node::Ticket(t)) if t.status == Status::Closed);
    if exempt {
        return Vec::new();
    }
    let mut out: Vec<Finding> = g.out(r).filter(|e| !g.contains(&e.to)).map(Finding::dangling).collect();
    out.sort_by_key(Finding::sort_key);
    out
}

fn dangling(g: &Graph, out: &mut Vec<Finding>) {
    for e in &g.edges {
        if g.contains(&e.to) {
            continue;
        }
        if let Some(Node::Ticket(t)) = g.get(&e.from) {
            if t.status == Status::Closed {
                continue;
            }
        }
        out.push(Finding::dangling(e));
    }
}

fn uncovered(g: &Graph, out: &mut Vec<Finding>) {
    for (r, node) in &g.nodes {
        match node {
            Node::Criterion(_) => {
                if g.inbound_with(r, Relation::CoveredBy).next().is_none() {
                    out.push(Finding::node(Rule::Uncovered, r, "no requirement names this criterion"));
                }
            }
            Node::Module(m) => {
                // Only modules that are part of the grimoire: those expected to
                // carry a contract or that carry one anyway. Test files, build
                // scripts and examples are code, not part of the grimoire.
                let manual_material = m.expects_contract || m.purpose.is_some();
                if manual_material && g.inbound_with(r, Relation::Links).next().is_none() {
                    out.push(Finding::node(Rule::Uncovered, r, "no page links to this module"));
                }
                if m.expects_contract && m.purpose.is_none() {
                    let why = m.contract_error.clone().unwrap_or_else(|| "no contract".into());
                    out.push(Finding::node(Rule::Uncovered, r, format!("module has no usable contract: {why}")));
                }
            }
            Node::Page(p) => {
                if !g.is_root(&p.slug) && g.inbound_with(r, Relation::Links).next().is_none() {
                    out.push(Finding::node(Rule::Uncovered, r, "no page links to this page and it is not a root"));
                }
            }
            Node::Req(_) if g.inbound_with(r, Relation::Links).next().is_none() => {
                out.push(Finding::node(Rule::Uncovered, r, "no page links to this requirement"));
            }
            _ => {}
        }
    }
}

/// `pages/foo.json` becomes `page:foo`; anything unrecognised becomes a
/// `file:` reference to the store path so it still sorts stably.
fn malformed_ref(path: &str) -> Reference {
    let stem = path.rsplit('/').next().unwrap_or(path).trim_end_matches(".json");
    if let Some(dir) = path.split('/').next() {
        let kind = match dir {
            "pages" => Some(Kind::Page),
            "requirements" => Some(Kind::Req),
            "tickets" => Some(Kind::Ticket),
            _ => None,
        };
        if let Some(k) = kind {
            if path.contains('/') {
                return Reference::new(k, stem);
            }
        }
    }
    Reference::file(path)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::graph::fixtures::graph;

    fn refs(f: &[Finding]) -> Vec<String> {
        f.iter().map(|f| format!("{:?} {}", f.rule, f.r#ref)).collect()
    }

    #[test]
    fn dangling_reports_open_not_closed() {
        let g = graph();
        let f = lint(&g, &[Rule::Dangling]);
        let r = refs(&f);
        assert_eq!(r, vec!["Dangling req:stale".to_string()]);
    }

    #[test]
    fn uncovered_reports_each_rule() {
        let g = graph();
        let f = lint(&g, &[Rule::Uncovered]);
        let r = refs(&f);
        assert!(r.contains(&"Uncovered criterion:src/a/ready#reads-only".to_string()));
        assert!(r.contains(&"Uncovered criterion:src/a/layout#stable".to_string()));
        assert!(r.contains(&"Uncovered module:src/a/layout".to_string()));
        assert!(r.contains(&"Uncovered module:src/b/unrelated".to_string()));
        assert!(r.contains(&"Uncovered page:orphan".to_string()));
        assert!(!r.contains(&"Uncovered page:overview".to_string()));
        assert!(!r.contains(&"Uncovered criterion:src/a/ready#guarantee".to_string()));
        // unrelated is reported twice: unlinked and contractless.
        assert_eq!(r.iter().filter(|s| s.ends_with("module:src/b/unrelated")).count(), 2);
    }

    #[test]
    fn stable_order() {
        let g = graph();
        let a = lint(&g, &Rule::ALL);
        let b = lint(&g, &Rule::ALL);
        assert_eq!(a, b);
        let mut sorted = a.clone();
        sorted.sort_by_key(Finding::sort_key);
        assert_eq!(a, sorted);
    }

    #[test]
    fn touching_filters() {
        let g = graph();
        let f = lint_touching(&g, &[Reference::module("src/a/ready")]);
        let r = refs(&f);
        assert_eq!(r, vec!["Dangling req:stale".to_string(), "Uncovered criterion:src/a/ready#reads-only".to_string()]);
    }

    #[test]
    fn malformed_refs() {
        assert_eq!(malformed_ref("pages/x.json"), Reference::page("x"));
        assert_eq!(malformed_ref("grimoire.json"), Reference::file("grimoire.json"));
    }
}
