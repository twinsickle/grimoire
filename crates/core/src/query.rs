//! # Module contract
//!
//! ```json
//! {"purpose":"Answer the read tools over the graph: find, get, outline, rests_on, rests_on_me, coverage, board, related_tickets and module_source.","criteria":[{"id":"documented-shapes","statement":"Every answer serialises to the shape its requirement in the manual gives for that tool, identically over every transport."},{"id":"node-view","statement":"Every node any tool returns has the same shape: its stored or derived fields, plus ref, plus in and out, the derived edge lists as {ref, relation, title}, title being the other end's one-line name or null when it is dangling."},{"id":"find","statement":"find matches a query case-insensitively against slug, title, statement, purpose, body and declaration path, optionally restricted to kinds, and returns up to limit summaries ordered by match quality."},{"id":"get","statement":"get returns one node in full with its edges, or not_found."},{"id":"outline","statement":"outline returns the manual as a tree from the root pages (or a given page) to a depth, each node with its coverage and its count of lint findings on or below it."},{"id":"nearest-first","statement":"rests_on lists nodes nearest the start first and, within a distance, by newest decision first, each with the path it was reached by and the decisions it carries."},{"id":"bounded-walk","statement":"rests_on expands only the starting node by its full rules; a ticket reached on the way contributes its decision and is not expanded, and a page reached on the way contributes its requirements and modules but not its child pages."},{"id":"rests-on-me","statement":"rests_on_me returns the transitive upward closure of a node through the derived edges, and separately the open and active tickets whose touches or relies_on reach it."},{"id":"coverage","statement":"coverage returns, for a criterion, the requirements naming it; for a module, each criterion with the requirements naming it, empty where uncovered; and for a requirement, each criterion it names in covered_by order with its statement and its module's ref and purpose, null where the criterion or module does not exist."},{"id":"board","statement":"board returns open and active change tickets by default, filterable by status, kind, priority and a touched node, ordered next, high, later, then active before open, then title."},{"id":"related-tickets","statement":"related_tickets returns the open gaps and questions whose touches or relies_on share a reference with the given ticket's touches."},{"id":"module-source","statement":"module_source returns the file and line of a module, criterion, file or declaration, with the contract heading's line for criteria."}]}
//! ```

use std::collections::{BTreeMap, HashMap, HashSet, VecDeque};

use serde::Serialize;
use serde_json::Value;

use crate::graph::{Graph, Node, SourceLocation};
use crate::lint::{lint, Rule};
use crate::model::*;

/// A node as every tool returns it: its fields, plus `ref`, `in` and `out`.
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
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct EdgeView {
    pub r#ref: Reference,
    pub relation: Relation,
    pub title: Option<String>,
}

pub fn view(g: &Graph, r: &Reference) -> Option<NodeView> {
    let node = g.get(r)?.clone();
    let edge = |other: &Reference, relation: Relation| EdgeView { r#ref: other.clone(), relation, title: g.get(other).map(Node::title) };
    Some(NodeView {
        r#ref: r.clone(),
        node,
        r#in: g.inbound(r).map(|e| edge(&e.from, e.relation)).collect(),
        out: g.out(r).map(|e| edge(&e.to, e.relation)).collect(),
    })
}

/// The short form used in lists.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Summary {
    pub r#ref: Reference,
    pub title: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub kind: Option<TicketKind>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub priority: Option<Priority>,
    #[serde(skip_serializing_if = "Option::is_none")]
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

// ---------------------------------------------------------------------------
// find
// ---------------------------------------------------------------------------

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

// ---------------------------------------------------------------------------
// outline
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize)]
pub struct Tree {
    pub r#ref: Reference,
    pub title: String,
    pub coverage: Value,
    pub findings: usize,
    pub children: Vec<Tree>,
}

pub fn outline(g: &Graph, depth: usize, root: Option<&Reference>) -> Vec<Tree> {
    let findings = lint(g, &Rule::ALL);
    let mut per_node: HashMap<&Reference, usize> = HashMap::new();
    for f in &findings {
        *per_node.entry(&f.r#ref).or_default() += 1;
    }
    let roots: Vec<Reference> = match root {
        Some(r) => vec![r.clone()],
        None => g.roots.iter().map(|s| Reference::page(s)).collect(),
    };
    roots.iter().filter(|r| g.contains(r)).map(|r| tree(g, r, depth, &per_node, &mut HashSet::new())).collect()
}

fn tree(g: &Graph, r: &Reference, depth: usize, per_node: &HashMap<&Reference, usize>, path: &mut HashSet<Reference>) -> Tree {
    let node = g.get(r);
    let title = node.map(Node::title).unwrap_or_else(|| format!("(missing) {r}"));
    let mut children = Vec::new();
    if depth > 0 && path.insert(r.clone()) {
        for c in children_of(g, r) {
            children.push(tree(g, &c, depth - 1, per_node, path));
        }
        path.remove(r);
    }
    let coverage = match node {
        Some(Node::Page(_)) => {
            let (covered, total) = page_coverage(g, r, &mut HashSet::new());
            if total == 0 {
                Value::Null
            } else {
                Value::from(covered as f64 / total as f64)
            }
        }
        Some(Node::Req(q)) => Value::from(q.covered_by.len()),
        _ => Value::Null,
    };
    // Findings on or below the node, over the whole subtree regardless of the
    // requested depth, so a shallow outline still shows where the problems are.
    let findings = findings_below(g, r, per_node, &mut HashSet::new());
    Tree { r#ref: r.clone(), title, coverage, findings, children }
}

fn children_of(g: &Graph, r: &Reference) -> Vec<Reference> {
    match g.get(r) {
        Some(Node::Page(p)) => p.links.clone(),
        Some(Node::Req(q)) => q.covered_by.clone(),
        Some(Node::Module(m)) => m.criteria.iter().map(|c| Reference::criterion(&c.module, &c.id)).collect(),
        _ => Vec::new(),
    }
}

fn findings_below(g: &Graph, r: &Reference, per_node: &HashMap<&Reference, usize>, seen: &mut HashSet<Reference>) -> usize {
    if !seen.insert(r.clone()) {
        return 0;
    }
    let own = per_node.get(r).copied().unwrap_or(0);
    own + children_of(g, r).iter().map(|c| findings_below(g, c, per_node, seen)).sum::<usize>()
}

/// Criteria under a page (through its pages and modules) and how many some
/// requirement names.
fn page_coverage(g: &Graph, r: &Reference, seen: &mut HashSet<Reference>) -> (usize, usize) {
    if !seen.insert(r.clone()) {
        return (0, 0);
    }
    let mut covered = 0;
    let mut total = 0;
    match g.get(r) {
        Some(Node::Page(p)) => {
            for l in &p.links {
                let (c, t) = page_coverage(g, l, seen);
                covered += c;
                total += t;
            }
        }
        Some(Node::Module(m)) => {
            for c in &m.criteria {
                total += 1;
                let cref = Reference::criterion(&c.module, &c.id);
                if g.inbound_with(&cref, Relation::CoveredBy).next().is_some() {
                    covered += 1;
                }
            }
        }
        _ => {}
    }
    (covered, total)
}

// ---------------------------------------------------------------------------
// rests_on
// ---------------------------------------------------------------------------

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
        Some(Node::Req(q)) => {
            let mut d = q.decisions.clone();
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

#[derive(Debug, Clone, Serialize)]
pub struct Affected {
    pub r#ref: Reference,
    pub title: String,
    pub relation: Relation,
}

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

// ---------------------------------------------------------------------------
// coverage
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize)]
pub struct CriterionCoverage {
    pub r#ref: Reference,
    pub statement: String,
    pub requirements: Vec<Summary>,
}

pub fn requirements_covering(g: &Graph, criterion: &Reference) -> Vec<Summary> {
    let mut v: Vec<Summary> = g.inbound_with(criterion, Relation::CoveredBy).filter_map(|e| g.get(&e.from).map(|n| summary(&e.from, n))).collect();
    v.sort_by(|a, b| a.r#ref.cmp(&b.r#ref));
    v
}

pub fn module_coverage(g: &Graph, module: &Module) -> Vec<CriterionCoverage> {
    module
        .criteria
        .iter()
        .map(|c| {
            let r = Reference::criterion(&c.module, &c.id);
            CriterionCoverage { requirements: requirements_covering(g, &r), r#ref: r, statement: c.statement.clone() }
        })
        .collect()
}

/// A criterion as a requirement's coverage lists it: what it says and the
/// module it lives in. `statement` is `None` when the criterion is dangling.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct CoveringCriterion {
    pub r#ref: Reference,
    pub statement: Option<String>,
    pub module: ModuleSummary,
}

/// A module beside a criterion: its reference and purpose, `None` when the
/// module does not exist or has no contract.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ModuleSummary {
    pub r#ref: Reference,
    pub purpose: Option<String>,
}

/// The criteria a requirement names, in `covered_by` order, each with its
/// module. Dangling entries are kept so a dangling requirement never looks covered.
pub fn requirement_coverage(g: &Graph, req: &Requirement) -> Vec<CoveringCriterion> {
    req.covered_by
        .iter()
        .map(|r| {
            let statement = match g.get(r) {
                Some(Node::Criterion(c)) => Some(c.statement.clone()),
                _ => None,
            };
            let module_id = r.split_hash().map_or(r.id.as_str(), |(m, _)| m);
            let module = Reference::module(module_id);
            let purpose = match g.get(&module) {
                Some(Node::Module(m)) => m.purpose.clone(),
                _ => None,
            };
            CoveringCriterion { r#ref: r.clone(), statement, module: ModuleSummary { r#ref: module, purpose } }
        })
        .collect()
}

// ---------------------------------------------------------------------------
// board, related_tickets
// ---------------------------------------------------------------------------

pub struct BoardFilter<'a> {
    pub status: Option<&'a [Status]>,
    pub kinds: Option<&'a [TicketKind]>,
    pub priority: Option<&'a [Priority]>,
    pub touching: Option<&'a Reference>,
}

pub fn board(g: &Graph, f: &BoardFilter) -> Vec<Summary> {
    let default_status = [Status::Open, Status::Active];
    let default_kinds = [TicketKind::Change];
    let status = f.status.unwrap_or(&default_status);
    let kinds = f.kinds.unwrap_or(&default_kinds);
    let mut tickets: Vec<&Ticket> = g
        .tickets()
        .filter(|t| status.contains(&t.status) && kinds.contains(&t.kind))
        .filter(|t| f.priority.is_none_or(|p| p.contains(&t.priority)))
        .filter(|t| f.touching.is_none_or(|x| t.touches.contains(x) || t.relies_on.contains(x)))
        .collect();
    tickets.sort_by(|a, b| a.priority.cmp(&b.priority).then_with(|| b.status.cmp(&a.status)).then_with(|| a.title.cmp(&b.title)));
    tickets.into_iter().map(ticket_summary).collect()
}

pub fn related_tickets(g: &Graph, ticket: &Ticket) -> Vec<Summary> {
    let touches: HashSet<&Reference> = ticket.touches.iter().collect();
    let mut v: Vec<&Ticket> = g
        .tickets()
        .filter(|t| t.slug != ticket.slug && t.status != Status::Closed && matches!(t.kind, TicketKind::Gap | TicketKind::Question))
        .filter(|t| t.touches.iter().chain(t.relies_on.iter()).any(|x| touches.contains(x)))
        .collect();
    v.sort_by(|a, b| a.priority.cmp(&b.priority).then_with(|| a.title.cmp(&b.title)));
    v.into_iter().map(ticket_summary).collect()
}

pub fn module_source(g: &Graph, r: &Reference) -> Option<SourceLocation> {
    g.source_of(r)
}

/// Count findings per rule, for `lint`'s `counts`.
pub fn counts(findings: &[crate::lint::Finding]) -> BTreeMap<String, usize> {
    let mut m = BTreeMap::new();
    for r in Rule::ALL {
        m.insert(serde_json::to_value(r).unwrap().as_str().unwrap().to_string(), 0usize);
    }
    for f in findings {
        *m.get_mut(serde_json::to_value(f.rule).unwrap().as_str().unwrap()).unwrap() += 1;
    }
    m
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::graph::fixtures::graph;

    #[test]
    fn find_ranks_exact_slug_first() {
        let g = graph();
        let hits = find(&g, "git-readiness", None, 10);
        assert_eq!(hits[0].r#ref, Reference::page("git-readiness"));
        let only_reqs = find(&g, "ready", Some(&[Kind::Req]), 10);
        assert!(only_reqs.iter().all(|s| s.r#ref.kind == Kind::Req));
        assert!(!only_reqs.is_empty());
    }

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

    #[test]
    fn outline_from_roots() {
        let g = graph();
        let t = outline(&g, 3, None);
        assert_eq!(t.len(), 1);
        assert_eq!(t[0].r#ref, Reference::page("overview"));
        let gr = &t[0].children[0];
        assert_eq!(gr.r#ref, Reference::page("git-readiness"));
        // one of two criteria under module ready is covered.
        assert_eq!(gr.coverage, Value::from(0.5));
        assert!(gr.findings >= 1);
        let module = gr.children.iter().find(|c| c.r#ref.kind == Kind::Module).unwrap();
        assert_eq!(module.children.len(), 2);
        // A shallow outline still counts the findings deep below it.
        let shallow = outline(&g, 0, None);
        assert_eq!(shallow[0].findings, t[0].findings);
        assert!(shallow[0].children.is_empty());
    }

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

    #[test]
    fn rests_on_is_bounded_at_tickets_and_pages() {
        let mut store = crate::graph::fixtures::store();
        // A ticket relying on the closed ticket `old` (which touches a missing page)
        // and on the root page, which links to the whole manual.
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

    #[test]
    fn requirement_coverage_in_order_with_modules() {
        let mut store = crate::graph::fixtures::store();
        let r = crate::graph::fixtures::req(
            "spans",
            &["criterion:src/a/layout#stable", "criterion:src/a/ready#gone", "criterion:src/a/ready#guarantee", "criterion:src/z/missing#x"],
            &[],
        );
        store.requirements.insert(r.slug.clone(), r.clone());
        let empty = crate::graph::fixtures::req("names-nothing", &[], &[]);
        store.requirements.insert(empty.slug.clone(), empty.clone());
        let g = Graph::build(&store, &crate::extraction::fixtures::extraction(), &[]);

        let cov = requirement_coverage(&g, &r);
        let refs: Vec<String> = cov.iter().map(|c| c.r#ref.to_string()).collect();
        assert_eq!(refs, vec!["criterion:src/a/layout#stable", "criterion:src/a/ready#gone", "criterion:src/a/ready#guarantee", "criterion:src/z/missing#x"], "covered_by order");
        assert_eq!(cov[0].statement.as_deref(), Some("Paths do not move."));
        assert_eq!(cov[0].module, ModuleSummary { r#ref: Reference::module("src/a/layout"), purpose: Some("Lay out the workspace.".into()) });
        // A dangling criterion stays, with no statement, beside its existing module.
        assert_eq!(cov[1].statement, None);
        assert_eq!(cov[1].module.purpose.as_deref(), Some("Decide readiness."));
        // A criterion in a module that does not exist: both null.
        assert_eq!(cov[3].statement, None);
        assert_eq!(cov[3].module, ModuleSummary { r#ref: Reference::module("src/z/missing"), purpose: None });
        assert!(requirement_coverage(&g, &empty).is_empty());
    }

    #[test]
    fn board_orders_and_filters() {
        let g = graph();
        let b = board(&g, &BoardFilter { status: None, kinds: None, priority: None, touching: None });
        let slugs: Vec<&str> = b.iter().map(|s| s.r#ref.id.as_str()).collect();
        assert_eq!(slugs, vec!["phase-62", "later-1"]);
        let touching = Reference::module("src/a/ready");
        let b = board(&g, &BoardFilter { status: None, kinds: Some(&[TicketKind::Gap]), priority: None, touching: Some(&touching) });
        assert_eq!(b.len(), 1);
        assert_eq!(b[0].r#ref.id, "gap-1");
    }

    #[test]
    fn related_finds_gaps_on_shared_nodes() {
        let g = graph();
        let t = match g.get(&Reference::ticket("phase-62")).unwrap() {
            Node::Ticket(t) => t.clone(),
            _ => unreachable!(),
        };
        let r = related_tickets(&g, &t);
        assert_eq!(r.len(), 1);
        assert_eq!(r[0].r#ref.id, "gap-1");
    }
}
