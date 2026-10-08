//! # Module contract
//!
//! ```json
//! {"purpose":"Show a node the way every read tool returns it, in full or as a summary, and find nodes by text.","criteria":[{"id":"node-view","statement":"Every node a tool returns in full, as get and brew_ticket do, has the same shape: its stored or derived fields, plus ref, plus in and out, the derived edge lists as {ref, relation, title}, title being the other end's one-line name or null when it is dangling; in leaves out the edges from bottled tickets, which bottled_tickets counts per relation, left out when there are none."},{"id":"withdrawn-marked","statement":"A ticket's summary says withdrawn, true, when its decision is marked withdrawn, and leaves it out otherwise, so the board and every list of tickets tell a withdrawn change from one that bottled."},{"id":"find","statement":"find matches a query case-insensitively against slug, title, statement, purpose, body, declaration path and test name, optionally restricted to kinds, and returns up to limit summaries ordered by match quality."},{"id":"get","statement":"get returns one node in full with its edges, or not_found."}]}
//! ```

use std::collections::BTreeMap;

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
    // The node's history: the edges from bottled tickets, counted per relation rather than listed (req:tools-node-view).
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    #[cfg_attr(feature = "ts", ts(as = "Option<BTreeMap<String, usize>>", optional))]
    pub bottled_tickets: BTreeMap<String, usize>,
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
    let bottled = |from: &Reference| matches!(g.get(from), Some(Node::Ticket(t)) if t.status == Status::Bottled);
    let mut bottled_tickets: BTreeMap<String, usize> = BTreeMap::new();
    let mut r#in = Vec::new();
    for e in g.inbound(r) {
        if bottled(&e.from) {
            *bottled_tickets.entry(e.relation.as_str().to_string()).or_default() += 1;
        } else {
            r#in.push(edge(&e.from, e.relation));
        }
    }
    Some(NodeView { r#ref: r.clone(), node, r#in, out: g.out(r).map(|e| edge(&e.to, e.relation)).collect(), bottled_tickets })
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
    // A bottled change withdrawn, nothing of it applied (req:tool-withdraw-ticket).
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    #[cfg_attr(feature = "ts", ts(as = "Option<bool>", optional))]
    pub withdrawn: bool,
}

pub fn summary(r: &Reference, n: &Node) -> Summary {
    let (kind, priority, status, withdrawn) = match n {
        Node::Ticket(t) => (Some(t.kind), Some(t.priority), Some(t.status), t.decision.as_ref().is_some_and(|d| d.withdrawn)),
        _ => (None, None, None, false),
    };
    Summary { r#ref: r.clone(), title: n.title(), kind, priority, status, withdrawn }
}

/// A node's summary, as a write answers it, or none when the graph does not hold it.
pub fn summary_at(g: &Graph, r: &Reference) -> Option<Summary> {
    g.get(r).map(|n| summary(r, n))
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
    /// {"test": "a-withdrawn-change-is-marked-in-its-summary", "checks": ["crates/core/src/query/lookup#withdrawn-marked"]}
    /// ```
    #[test]
    fn a_withdrawn_change_is_marked_in_its_summary() {
        use crate::graph::fixtures::ticket;
        let mut t = ticket("w", TicketKind::Change, Priority::Later, Status::Bottled, &[], &[]);
        assert!(!ticket_summary(&t).withdrawn);
        assert!(serde_json::to_value(ticket_summary(&t)).unwrap().get("withdrawn").is_none(), "left out when not withdrawn");
        t.decision.as_mut().unwrap().withdrawn = true;
        assert_eq!(serde_json::to_value(ticket_summary(&t)).unwrap()["withdrawn"], true);
    }

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
        assert!(json.get("bottled_tickets").is_none(), "left out when there are none");
    }

    /// # Test contract
    ///
    /// ```json
    /// {"test": "bottled-tickets-are-counted-not-listed", "checks": ["crates/core/src/query/lookup#node-view", "req:tools-node-view"]}
    /// ```
    #[test]
    fn bottled_tickets_are_counted_not_listed() {
        use crate::graph::fixtures::ticket;
        let mut s = crate::graph::fixtures::store();
        for (slug, status, touches, relies_on) in [
            ("b1", Status::Bottled, true, false),
            ("b2", Status::Bottled, true, false),
            ("b3", Status::Bottled, true, false),
            ("b4", Status::Bottled, false, true),
            ("live", Status::Brewing, true, false),
        ] {
            let on: &[&str] = &["page:orphan"];
            s.tickets.insert(slug.into(), ticket(slug, TicketKind::Change, Priority::Later, status, if touches { on } else { &[] }, if relies_on { on } else { &[] }));
        }
        let g = Graph::build(&s, &crate::extraction::fixtures::extraction(), &[]);
        let v = view(&g, &Reference::page("orphan")).unwrap();
        let listed: Vec<String> = v.r#in.iter().map(|e| e.r#ref.id.clone()).collect();
        assert!(listed.contains(&"live".to_string()) && listed.contains(&"later-1".to_string()), "{listed:?}");
        assert!(!listed.iter().any(|t| t.starts_with('b')), "{listed:?}");
        assert_eq!(serde_json::to_value(&v).unwrap()["bottled_tickets"], serde_json::json!({ "touches": 3, "relies_on": 1 }));
    }
}
