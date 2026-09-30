//! # Module contract
//!
//! ```json
//! {"purpose":"Walk the graph from a node: down to what it rests on, with the decisions behind each, and up to what rests on it.","criteria":[{"id":"nearest-first","statement":"rests_on lists nodes nearest the start first and, within a distance, by newest decision first, each with the path it was reached by and the decisions it carries that are in force, one superseded by another left out."},{"id":"bounded-walk","statement":"rests_on expands only the starting node by its full rules; a ticket reached on the way contributes its decision and is not expanded, a page reached on the way contributes its requirements and modules but not its child pages, and a retired requirement contributes its decisions but nothing below it, since it no longer holds."},{"id":"rests-on-me","statement":"rests_on_me returns the transitive upward closure of a node through the derived edges, and separately the open and active tickets whose touches or relies_on reach it."}]}
//! ```

use std::collections::{HashSet, VecDeque};

use serde::Serialize;

use super::lookup::{ticket_summary, Summary};
use crate::graph::{Graph, Node};
use crate::model::*;

#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[derive(Debug, Clone, Serialize)]
pub struct RestsOn {
    pub r#ref: Reference,
    pub title: String,
    pub via: Vec<Reference>,
    pub decisions: Vec<Decision>,
}

/// The downward relations each kind is followed through. `is_start` is the
/// node the question was asked about; nodes reached along the way are
/// bounded: a ticket contributes its decision and is not expanded, and a
/// page contributes its requirements and modules but not its child pages.
/// Module imports stay transitive, since that is what blast radius means.
fn downward(g: &Graph, r: &Reference, is_start: bool) -> Vec<Reference> {
    match g.get(r) {
        Some(Node::Ticket(t)) if is_start => t.relies_on.iter().chain(t.touches.iter()).cloned().collect(),
        Some(Node::Ticket(_)) => Vec::new(),
        Some(Node::Page(p)) => p.links.iter().filter(|l| l.kind != Kind::Page).cloned().collect(),
        // A retired requirement no longer holds: nothing rests on it through its old criteria.
        Some(Node::Req(q)) if q.retired => Vec::new(),
        Some(Node::Req(q)) => q.covered_by.clone(),
        Some(Node::Criterion(c)) => vec![Reference::module(&c.module)],
        Some(Node::Module(m)) => m.imports.iter().map(|i| Reference::module(i)).collect(),
        _ => Vec::new(),
    }
}

/// The decisions a reached node contributes: a requirement's, newest first,
/// or a closed ticket's.
fn decisions_of(g: &Graph, r: &Reference) -> Vec<Decision> {
    match g.get(r) {
        // Only what is in force: a superseded decision no longer says why.
        Some(Node::Req(q)) => {
            // Newest first; decisions of one day by position, the later first.
            let mut d: Vec<Decision> = q.in_force().into_iter().rev().map(|(_, d)| d.clone()).collect();
            d.sort_by(|a, b| b.date.cmp(&a.date));
            d
        }
        Some(Node::Ticket(t)) => t.decision.iter().cloned().collect(),
        _ => Vec::new(),
    }
}

pub fn rests_on(g: &Graph, start: &Reference, depth: Option<usize>) -> Vec<RestsOn> {
    let mut out: Vec<(usize, RestsOn)> = Vec::new();
    let mut seen: HashSet<Reference> = HashSet::new();
    seen.insert(start.clone());
    let mut queue: VecDeque<(Reference, Vec<Reference>)> = VecDeque::new();
    queue.push_back((start.clone(), vec![]));
    while let Some((r, via)) = queue.pop_front() {
        let dist = via.len();
        if depth.is_some_and(|d| dist >= d) {
            continue;
        }
        for next in downward(g, &r, r == *start) {
            if !seen.insert(next.clone()) {
                continue;
            }
            let mut path = via.clone();
            path.push(r.clone());
            let decisions = decisions_of(g, &next);
            let title = g.get(&next).map(Node::title).unwrap_or_else(|| format!("(missing) {next}"));
            out.push((dist + 1, RestsOn { r#ref: next.clone(), title, via: path.clone(), decisions }));
            queue.push_back((next, path));
        }
    }
    out.sort_by(|a, b| {
        a.0.cmp(&b.0)
            .then_with(|| {
                let da = a.1.decisions.first().map(|d| d.date.as_str()).unwrap_or("");
                let db = b.1.decisions.first().map(|d| d.date.as_str()).unwrap_or("");
                db.cmp(da)
            })
            .then_with(|| a.1.r#ref.cmp(&b.1.r#ref))
    });
    out.into_iter().map(|(_, r)| r).collect()
}

// ---------------------------------------------------------------------------
// rests_on_me
// ---------------------------------------------------------------------------

#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[derive(Debug, Clone, Serialize)]
pub struct Affected {
    pub r#ref: Reference,
    pub title: String,
    pub relation: Relation,
}

#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[derive(Debug, Clone, Serialize)]
pub struct RestsOnMe {
    pub nodes: Vec<Affected>,
    pub tickets: Vec<Summary>,
}

/// Everything that would be affected if the node changed: the transitive
/// upward closure through the derived edges, excluding tickets, which are
/// listed separately.
pub fn rests_on_me(g: &Graph, start: &Reference) -> RestsOnMe {
    let mut nodes: Vec<Affected> = Vec::new();
    let mut seen: HashSet<Reference> = HashSet::new();
    seen.insert(start.clone());
    let mut queue: VecDeque<Reference> = VecDeque::new();
    queue.push_back(start.clone());
    while let Some(r) = queue.pop_front() {
        for e in g.inbound(&r) {
            if e.from.kind == Kind::Ticket {
                continue;
            }
            if !seen.insert(e.from.clone()) {
                continue;
            }
            let title = g.get(&e.from).map(Node::title).unwrap_or_default();
            nodes.push(Affected { r#ref: e.from.clone(), title, relation: e.relation });
            queue.push_back(e.from.clone());
        }
    }
    let mut tickets: Vec<&Ticket> = g
        .tickets()
        .filter(|t| t.status != Status::Closed)
        .filter(|t| t.touches.iter().chain(t.relies_on.iter()).any(|x| seen.contains(x)))
        .collect();
    tickets.sort_by(|a, b| a.slug.cmp(&b.slug));
    RestsOnMe { nodes, tickets: tickets.into_iter().map(ticket_summary).collect() }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::graph::fixtures::graph;

    /// # Test contract
    ///
    /// ```json
    /// {"test": "rests-on-ticket-reaches-modules-and-decisions", "checks": ["crates/core/src/query/walks#nearest-first"]}
    /// ```
    #[test]
    fn rests_on_ticket_reaches_modules_and_decisions() {
        let g = graph();
        let r = rests_on(&g, &Reference::ticket("phase-62"), None);
        let refs: Vec<String> = r.iter().map(|x| x.r#ref.to_string()).collect();
        // Nearest first; within a distance the node with the newest decision first.
        assert_eq!(refs[0], "req:ready-only-with-commit");
        assert_eq!(refs[1], "page:git-readiness");
        assert_eq!(refs[2], "module:src/a/ready");
        assert!(refs.contains(&"module:src/a/layout".to_string()));
        let req = r.iter().find(|x| x.r#ref == Reference::req("ready-only-with-commit")).unwrap();
        assert_eq!(req.decisions.len(), 1);
        assert_eq!(req.via, vec![Reference::ticket("phase-62")]);
        let layout = r.iter().find(|x| x.r#ref == Reference::module("src/a/layout")).unwrap();
        assert_eq!(layout.via.last().unwrap(), &Reference::module("src/a/ready"));
        assert_eq!(rests_on(&g, &Reference::ticket("phase-62"), Some(1)).len(), 3);
    }

    /// # Test contract
    ///
    /// ```json
    /// {"test": "rests-on-is-bounded-at-tickets-and-pages", "checks": ["crates/core/src/query/walks#bounded-walk"]}
    /// ```
    #[test]
    fn rests_on_is_bounded_at_tickets_and_pages() {
        let mut store = crate::graph::fixtures::store();
        // A ticket relying on the closed ticket `old` (which touches a missing page)
        // and on the root page, which links to the whole grimoire.
        let t = crate::graph::fixtures::ticket("new", TicketKind::Change, Priority::Next, Status::Open, &[], &["ticket:old", "page:overview"]);
        store.tickets.insert(t.slug.clone(), t);
        let g = Graph::build(&store, &crate::extraction::fixtures::extraction(), &[]);
        let r = rests_on(&g, &Reference::ticket("new"), None);
        let refs: Vec<String> = r.iter().map(|x| x.r#ref.to_string()).collect();
        // The closed ticket is present with its decision and was not expanded.
        let old = r.iter().find(|x| x.r#ref == Reference::ticket("old")).unwrap();
        assert_eq!(old.decisions.len(), 1);
        assert!(!refs.contains(&"page:renamed-away".to_string()));
        // The root page is present; its child page and everything below are not.
        assert!(refs.contains(&"page:overview".to_string()));
        assert!(!refs.contains(&"page:git-readiness".to_string()));
        assert!(!refs.contains(&"module:src/a/ready".to_string()));
        assert_eq!(r.len(), 2, "{refs:?}");
        // Asked about a page directly, its requirements and modules are followed, not its pages.
        let r = rests_on(&g, &Reference::page("git-readiness"), None);
        let refs: Vec<String> = r.iter().map(|x| x.r#ref.to_string()).collect();
        assert!(refs.contains(&"req:ready-only-with-commit".to_string()));
        assert!(refs.contains(&"module:src/a/layout".to_string()), "imports stay transitive");
    }

    /// # Test contract
    ///
    /// ```json
    /// {"test": "rests-on-me-climbs-to-pages-and-tickets", "checks": ["crates/core/src/query/walks#rests-on-me"]}
    /// ```
    #[test]
    fn rests_on_me_climbs_to_pages_and_tickets() {
        let g = graph();
        let r = rests_on_me(&g, &Reference::module("src/a/layout"));
        let refs: Vec<String> = r.nodes.iter().map(|x| x.r#ref.to_string()).collect();
        assert!(refs.contains(&"module:src/a/ready".to_string()));
        assert!(refs.contains(&"req:ready-only-with-commit".to_string()));
        assert!(refs.contains(&"page:git-readiness".to_string()));
        assert!(refs.contains(&"page:overview".to_string()));
        let tickets: Vec<String> = r.tickets.iter().map(|t| t.r#ref.id.clone()).collect();
        assert_eq!(tickets, vec!["gap-1", "phase-62"]);
    }

    /// # Test contract
    ///
    /// ```json
    /// {"test": "rests-on-carries-only-decisions-in-force", "checks": ["crates/core/src/query/walks#nearest-first"]}
    /// ```
    #[test]
    fn rests_on_carries_only_decisions_in_force() {
        let mut s = crate::graph::fixtures::store();
        let q = s.requirements.get_mut("ready-only-with-commit").expect("fixture");
        q.decisions.push(Decision { supersedes: vec![1], ..Decision::new("2026-09-29", "Instead.") });
        let g = Graph::build(&s, &crate::extraction::fixtures::extraction(), &[]);
        let r = rests_on(&g, &Reference::ticket("phase-62"), None);
        let req = r.iter().find(|x| x.r#ref == Reference::req("ready-only-with-commit")).expect("reached");
        let texts: Vec<&str> = req.decisions.iter().map(|d| d.text.as_str()).collect();
        assert_eq!(texts, vec!["Instead."], "the superseded decision is left out");
        // Two decisions in force from one day: the later position first.
        let mut same = crate::graph::fixtures::store();
        let q = same.requirements.get_mut("ready-only-with-commit").expect("fixture");
        q.decisions = vec![Decision::new("2026-09-29", "Earlier."), Decision::new("2026-09-29", "Later.")];
        let g2 = Graph::build(&same, &crate::extraction::fixtures::extraction(), &[]);
        let r2 = rests_on(&g2, &Reference::ticket("phase-62"), None);
        let req2 = r2.iter().find(|x| x.r#ref == Reference::req("ready-only-with-commit")).expect("reached");
        assert_eq!(req2.decisions.iter().map(|d| d.text.as_str()).collect::<Vec<_>>(), vec!["Later.", "Earlier."]);
        // Retired, it contributes its decisions and nothing below it.
        s.requirements.get_mut("ready-only-with-commit").expect("fixture").retired = true;
        let g = Graph::build(&s, &crate::extraction::fixtures::extraction(), &[]);
        let r = rests_on(&g, &Reference::req("ready-only-with-commit"), None);
        assert!(r.is_empty(), "a retired requirement has nothing below it: {:?}", r.iter().map(|x| &x.r#ref).collect::<Vec<_>>());
        let from_ticket = rests_on(&g, &Reference::ticket("phase-62"), None);
        assert!(from_ticket.iter().all(|x| x.via.last() != Some(&Reference::req("ready-only-with-commit"))), "nothing reached through it");
    }
}
