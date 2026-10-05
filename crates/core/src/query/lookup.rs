//! # Module contract
//!
//! ```json
//! {"purpose":"Show a node the way every read tool returns it, in full or as a summary, and find nodes by text.","criteria":[{"id":"node-view","statement":"Every node any tool returns has the same shape: its stored or derived fields, plus ref, plus in and out, the derived edge lists as {ref, relation, title}, title being the other end's one-line name or null when it is dangling."},{"id":"find","statement":"find matches a query case-insensitively against slug, title, statement, purpose, body, declaration path and test name, optionally restricted to kinds, and returns up to limit summaries ordered by match quality."},{"id":"get","statement":"get returns one node in full with its edges, or not_found."}]}
//! ```

use serde::Serialize;

use crate::graph::{Graph, Node};
use crate::model::*;

/// A node as every tool returns it: its fields, plus `ref`, `in` and `out`.
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[derive(Debug, Clone, Serialize)]
pub struct NodeView {
    pub r#ref: Reference,
    #[serde(flatten)]
    pub node: Node,
    pub r#in: Vec<EdgeView>,
    pub out: Vec<EdgeView>,
}

/// One edge as a node view lists it: the other end, the relation, and the
/// other end's title when it exists, so a generated ticket id is never read
/// alone. `None` when the target is dangling.
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct EdgeView {
    pub r#ref: Reference,
    pub relation: Relation,
    pub title: Option<String>,
}

pub fn view(g: &Graph, r: &Reference) -> Option<NodeView> {
    let node = g.get(r)?.clone();
    let edge = |other: &Reference, relation: Relation| EdgeView { r#ref: other.clone(), relation, title: g.get(other).map(Node::title) };
    Some(NodeView { r#ref: r.clone(), node, r#in: g.inbound(r).map(|e| edge(&e.from, e.relation)).collect(), out: g.out(r).map(|e| edge(&e.to, e.relation)).collect() })
}

/// The short form used in lists.
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Summary {
    pub r#ref: Reference,
    pub title: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[cfg_attr(feature = "ts", ts(optional))]
    pub kind: Option<TicketKind>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[cfg_attr(feature = "ts", ts(optional))]
    pub priority: Option<Priority>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[cfg_attr(feature = "ts", ts(optional))]
    pub status: Option<Status>,
}

pub fn summary(r: &Reference, n: &Node) -> Summary {
    let (kind, priority, status) = match n {
        Node::Ticket(t) => (Some(t.kind), Some(t.priority), Some(t.status)),
        _ => (None, None, None),
    };
    Summary { r#ref: r.clone(), title: n.title(), kind, priority, status }
}

pub fn ticket_summary(t: &Ticket) -> Summary {
    summary(&Reference::ticket(&t.slug), &Node::Ticket(t.clone()))
}

pub fn find(g: &Graph, query: &str, kinds: Option<&[Kind]>, limit: usize) -> Vec<Summary> {
    let q = query.trim().to_lowercase();
    if q.is_empty() {
        return Vec::new();
    }
    let mut scored: Vec<(u32, &Reference, &Node)> = Vec::new();
    for (r, n) in &g.nodes {
        if let Some(kinds) = kinds {
            if !kinds.contains(&r.kind) {
                continue;
            }
        }
        let texts = n.searchable();
        let mut best = 0u32;
        for (i, t) in texts.iter().enumerate() {
            let t = t.to_lowercase();
            let s: u32 = if t == q {
                100
            } else if t.starts_with(&q) {
                60
            } else if t.contains(&q) {
                30
            } else {
                0
            };
            // Earlier fields (slug, title) weigh more than bodies.
            let s = s.saturating_sub(i as u32 * 5);
            best = best.max(s);
        }
        if r.id.to_lowercase().contains(&q) {
            best = best.max(25);
        }
        if best > 0 {
            scored.push((best, r, n));
        }
    }
    scored.sort_by(|a, b| b.0.cmp(&a.0).then_with(|| a.1.cmp(b.1)));
    scored.into_iter().take(limit).map(|(_, r, n)| summary(r, n)).collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::graph::fixtures::graph;

    /// # Test contract
    ///
    /// ```json
    /// {"test": "find-ranks-exact-slug-first", "checks": ["crates/core/src/query/lookup#find"]}
    /// ```
    #[test]
    fn find_ranks_exact_slug_first() {
        let g = graph();
        let hits = find(&g, "git-readiness", None, 10);
        assert_eq!(hits[0].r#ref, Reference::page("git-readiness"));
        let only_reqs = find(&g, "ready", Some(&[Kind::Req]), 10);
        assert!(only_reqs.iter().all(|s| s.r#ref.kind == Kind::Req));
        assert!(!only_reqs.is_empty());
    }

    /// # Test contract
    ///
    /// ```json
    /// {"test": "view-has-both-directions", "checks": ["crates/core/src/query/lookup#node-view"]}
    /// ```
    #[test]
    fn view_has_both_directions() {
        let g = graph();
        let v = view(&g, &Reference::module("src/a/ready")).unwrap();
        assert!(v.r#in.iter().any(|e| e.relation == Relation::Touches));
        assert!(v.out.iter().any(|e| e.relation == Relation::Imports));
        let json = serde_json::to_value(&v).unwrap();
        assert_eq!(json["ref"], "module:src/a/ready");
        assert!(json["purpose"].is_string());
    }
}
