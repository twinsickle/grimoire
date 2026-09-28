//! # Module contract
//!
//! ```json
//! {"purpose":"Answer the read tools over the graph: find, get, outline, rests_on, rests_on_me, coverage, board, related_tickets and module_source.","criteria":[{"id":"documented-shapes","statement":"Every answer serialises to the shape its requirement in the grimoire gives for that tool, identically over every transport."},{"id":"node-view","statement":"Every node any tool returns has the same shape: its stored or derived fields, plus ref, plus in and out, the derived edge lists as {ref, relation, title}, title being the other end's one-line name or null when it is dangling."},{"id":"find","statement":"find matches a query case-insensitively against slug, title, statement, purpose, body, declaration path and test name, optionally restricted to kinds, and returns up to limit summaries ordered by match quality."},{"id":"get","statement":"get returns one node in full with its edges, or not_found."},{"id":"outline","statement":"outline returns the grimoire as a tree from the root pages (or a given page) to a depth, each node with its coverage, its count of lint findings on or below it and how many of those are baselined, the checks of the findings on the node itself, how many tests check it, and its rollup over the whole subtree, each node counted once: the grimoire's modules and how many carry a contract, their criteria and how many are tested, the requirements and how many are held; and apart, the modules of the grimoire no page links."},{"id":"nearest-first","statement":"rests_on lists nodes nearest the start first and, within a distance, by newest decision first, each with the path it was reached by and the decisions it carries."},{"id":"bounded-walk","statement":"rests_on expands only the starting node by its full rules; a ticket reached on the way contributes its decision and is not expanded, and a page reached on the way contributes its requirements and modules but not its child pages."},{"id":"rests-on-me","statement":"rests_on_me returns the transitive upward closure of a node through the derived edges, and separately the open and active tickets whose touches or relies_on reach it."},{"id":"coverage","statement":"coverage returns, for a criterion, the requirements naming it and the tests checking it; for a module, each criterion with its requirements and tests, empty where there are none; and for a requirement, each criterion it names in covered_by order with its statement and its module's ref and purpose, null where the criterion or module does not exist."},{"id":"board","statement":"board returns open and active change tickets by default, filterable by status, kind, priority and a touched node, ordered next, high, later; within a band the tickets its order list names come first in that order, then the rest with each after any open ticket it relies on, then active before open, then title, a loop broken by that same rule."},{"id":"related-tickets","statement":"related_tickets returns the open gaps and questions whose touches or relies_on share a reference with the given ticket's touches."},{"id":"module-source","statement":"module_source returns the file and line of a module, criterion, file, declaration or test, with the contract heading's line for criteria and the test block's heading for a test."}]}
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
    /// How many of `findings` are in the baseline: inherited, not new.
    pub baselined: usize,
    /// The checks of the findings on this node itself, sorted and distinct,
    /// so a view can say what is wrong and not only how much.
    pub checks: Vec<String>,
    /// How many tests check this node: a criterion's evidence, or what holds
    /// a constraint that names no criterion.
    pub tests: usize,
    pub rollup: Rollup,
    pub children: Vec<Tree>,
}

/// How far adoption has come under a node, over its whole subtree with each
/// node counted once: the modules that are part of the grimoire and how many
/// carry a contract, those modules' criteria and how many a test checks, and
/// the requirements and how many name a criterion or are held by a test.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize)]
pub struct Rollup {
    pub modules: usize,
    pub contracted: usize,
    pub criteria: usize,
    pub tested: usize,
    pub requirements: usize,
    pub held: usize,
}

/// The outline's answer: the trees from the roots, and the modules of the
/// grimoire that no page links, which no tree reaches.
#[derive(Debug, Clone, Serialize)]
pub struct Outline {
    pub roots: Vec<Tree>,
    pub unplaced: Vec<Tree>,
}

pub fn outline(g: &Graph, depth: usize, root: Option<&Reference>) -> Outline {
    let findings = lint(g, &Rule::ALL);
    let mut per_node: HashMap<&Reference, (usize, usize)> = HashMap::new();
    let mut checks: HashMap<&Reference, std::collections::BTreeSet<&str>> = HashMap::new();
    for f in &findings {
        let e = per_node.entry(&f.r#ref).or_default();
        e.0 += 1;
        e.1 += usize::from(f.baselined);
        checks.entry(&f.r#ref).or_default().insert(f.check.as_str());
    }
    let roots: Vec<Reference> = match root {
        Some(r) => vec![r.clone()],
        None => g.roots.iter().map(|s| Reference::page(s)).collect(),
    };
    let on = Findings { count: per_node, checks };
    let unplaced = g
        .nodes
        .iter()
        .filter(|(r, n)| matches!(n, Node::Module(m) if in_grimoire(m)) && g.inbound_with(r, Relation::Links).next().is_none())
        .map(|(r, _)| tree(g, r, 1, &on, &mut HashSet::new()))
        .collect();
    Outline { roots: roots.iter().filter(|r| g.contains(r)).map(|r| tree(g, r, depth, &on, &mut HashSet::new())).collect(), unplaced }
}

/// A module is part of the grimoire when it is expected to carry a contract
/// or carries one anyway; test files and build scripts are not.
fn in_grimoire(m: &Module) -> bool {
    m.expects_contract || m.purpose.is_some()
}

/// Lint findings indexed by the node they are on: how many, and how many of
/// those are baselined.
struct Findings<'a> {
    count: HashMap<&'a Reference, (usize, usize)>,
    checks: HashMap<&'a Reference, std::collections::BTreeSet<&'a str>>,
}

fn tree(g: &Graph, r: &Reference, depth: usize, on: &Findings, path: &mut HashSet<Reference>) -> Tree {
    let node = g.get(r);
    let title = node.map(Node::title).unwrap_or_else(|| format!("(missing) {r}"));
    let mut children = Vec::new();
    if depth > 0 && path.insert(r.clone()) {
        for c in children_of(g, r) {
            children.push(tree(g, &c, depth - 1, on, path));
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
    let (findings, baselined) = findings_below(g, r, &on.count, &mut HashSet::new());
    let checks = on.checks.get(r).map(|c| c.iter().map(|s| s.to_string()).collect()).unwrap_or_default();
    let tests = g.inbound_with(r, Relation::Checks).count();
    let rollup = rollup(g, r);
    Tree { r#ref: r.clone(), title, coverage, findings, baselined, checks, tests, rollup, children }
}

/// The rollup of everything at or below `r`, each node once however many
/// paths reach it. Criteria count through their module only, so a page's
/// criteria are its own contracts' and not those its requirements borrow.
fn rollup(g: &Graph, r: &Reference) -> Rollup {
    let mut seen = HashSet::new();
    let mut stack = vec![r.clone()];
    let mut out = Rollup::default();
    while let Some(x) = stack.pop() {
        if !seen.insert(x.clone()) {
            continue;
        }
        match g.get(&x) {
            Some(Node::Module(m)) if in_grimoire(m) => {
                out.modules += 1;
                out.contracted += usize::from(m.purpose.is_some());
                for c in &m.criteria {
                    out.criteria += 1;
                    let cref = Reference::criterion(&c.module, &c.id);
                    out.tested += usize::from(g.inbound_with(&cref, Relation::Checks).next().is_some());
                }
            }
            Some(Node::Req(q)) => {
                out.requirements += 1;
                out.held += usize::from(!q.covered_by.is_empty() || g.inbound_with(&x, Relation::Checks).next().is_some());
            }
            _ => {}
        }
        stack.extend(children_of(g, &x));
    }
    out
}

fn children_of(g: &Graph, r: &Reference) -> Vec<Reference> {
    match g.get(r) {
        Some(Node::Page(p)) => p.links.clone(),
        Some(Node::Req(q)) => q.covered_by.clone(),
        Some(Node::Module(m)) => m.criteria.iter().map(|c| Reference::criterion(&c.module, &c.id)).collect(),
        _ => Vec::new(),
    }
}

/// Findings on or below `r`, and how many of them are baselined.
fn findings_below(g: &Graph, r: &Reference, per_node: &HashMap<&Reference, (usize, usize)>, seen: &mut HashSet<Reference>) -> (usize, usize) {
    if !seen.insert(r.clone()) {
        return (0, 0);
    }
    let (mut all, mut baselined) = per_node.get(r).copied().unwrap_or((0, 0));
    for c in children_of(g, r) {
        let (a, b) = findings_below(g, &c, per_node, seen);
        all += a;
        baselined += b;
    }
    (all, baselined)
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
    pub tests: Vec<Summary>,
}

pub fn requirements_covering(g: &Graph, criterion: &Reference) -> Vec<Summary> {
    let mut v: Vec<Summary> = g.inbound_with(criterion, Relation::CoveredBy).filter_map(|e| g.get(&e.from).map(|n| summary(&e.from, n))).collect();
    v.sort_by(|a, b| a.r#ref.cmp(&b.r#ref));
    v
}

pub fn tests_checking(g: &Graph, criterion: &Reference) -> Vec<Summary> {
    let mut v: Vec<Summary> = g.inbound_with(criterion, Relation::Checks).filter_map(|e| g.get(&e.from).map(|n| summary(&e.from, n))).collect();
    v.sort_by(|a, b| a.r#ref.cmp(&b.r#ref));
    v
}

pub fn module_coverage(g: &Graph, module: &Module) -> Vec<CriterionCoverage> {
    module
        .criteria
        .iter()
        .map(|c| {
            let r = Reference::criterion(&c.module, &c.id);
            CriterionCoverage { requirements: requirements_covering(g, &r), tests: tests_checking(g, &r), r#ref: r, statement: c.statement.clone() }
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
    let mut out = Vec::new();
    for p in [Priority::Next, Priority::High, Priority::Later] {
        let band: Vec<&Ticket> = tickets.iter().copied().filter(|t| t.priority == p).collect();
        out.extend(band_order(g, band, g.order.band(p)).into_iter().map(ticket_summary));
    }
    out
}

/// One band: the tickets its order list names, in that order; then the rest,
/// each after any open or active ticket it relies on, choosing among those
/// ready by active before open, then title. A loop of relies_on is broken by
/// the same rule. `band` arrives sorted by status and title.
fn band_order<'a>(g: &Graph, band: Vec<&'a Ticket>, list: &[String]) -> Vec<&'a Ticket> {
    let mut out: Vec<&Ticket> = list.iter().filter_map(|s| band.iter().copied().find(|t| &t.slug == s)).collect();
    let mut rest: Vec<&Ticket> = band.into_iter().filter(|t| !list.contains(&t.slug)).collect();
    let waiting_on = |t: &Ticket, pending: &[&Ticket]| {
        t.relies_on.iter().any(|r| r.kind == Kind::Ticket && r.id != t.slug && pending.iter().any(|p| p.slug == r.id) && matches!(g.get(r), Some(Node::Ticket(x)) if x.status != Status::Closed))
    };
    while !rest.is_empty() {
        // The first ready ticket in status-then-title order, or, when every one
        // waits on another (a loop), the first of them.
        let i = rest.iter().position(|t| !waiting_on(t, &rest)).unwrap_or(0);
        out.push(rest.remove(i));
    }
    out
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
    let mut m: BTreeMap<String, usize> = Rule::ALL.iter().map(|r| (r.name().to_string(), 0)).collect();
    for f in findings {
        *m.entry(f.rule.name().to_string()).or_insert(0) += 1;
    }
    m
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::graph::fixtures::graph;

    /// # Test contract
    ///
    /// ```json
    /// {"test": "find-ranks-exact-slug-first", "checks": ["crates/core/src/query#find"]}
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
    /// {"test": "view-has-both-directions", "checks": ["crates/core/src/query#node-view"]}
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

    /// # Test contract
    ///
    /// ```json
    /// {"test": "outline-names-the-checks-on-each-node", "checks": ["crates/core/src/query#outline"]}
    /// ```
    #[test]
    fn outline_names_the_checks_on_each_node() {
        let g = graph();
        let t = outline(&g, 4, Some(&Reference::module("src/a/ready"))).roots;
        let checks = |id: &str| t[0].children.iter().find(|c| c.r#ref == Reference::criterion("src/a/ready", id)).map(|c| c.checks.clone());
        assert_eq!(checks("reads-only"), Some(vec!["uncovered-criterion".to_string(), "untested".to_string()]));
        assert_eq!(checks("guarantee"), Some(vec![]), "named by a requirement and checked by a test");
        let tests = |id: &str| t[0].children.iter().find(|c| c.r#ref == Reference::criterion("src/a/ready", id)).map(|c| c.tests);
        assert_eq!((tests("guarantee"), tests("reads-only")), (Some(1), Some(0)), "outline counts the tests that check each node");
    }

    /// # Test contract
    ///
    /// ```json
    /// {"test": "outline-rolls-up-adoption", "checks": ["crates/core/src/query#outline"]}
    /// ```
    #[test]
    fn outline_rolls_up_adoption() {
        let mut s = crate::graph::fixtures::store();
        // Baseline the untested finding on reads-only: one of the page's findings is inherited.
        let plain = crate::graph::fixtures::graph();
        let inherited = lint(&plain, &Rule::ALL).into_iter().find(|f| f.check == "untested" && f.r#ref == Reference::criterion("src/a/ready", "reads-only")).expect("fixture has it");
        s.baseline = Some(Baseline { date: "2026-09-28".into(), reason: "Adopted.".into(), findings: vec![inherited.key()] });
        let g = Graph::build(&s, &crate::extraction::fixtures::extraction(), &[]);
        let expected = Rollup { modules: 1, contracted: 1, criteria: 2, tested: 1, requirements: 2, held: 2 };
        for depth in [0, 1, 4] {
            let o = outline(&g, depth, None);
            assert_eq!(o.roots[0].rollup, expected, "the rollup is the whole subtree's at depth {depth}");
        }
        let o = outline(&g, 1, None);
        let gr = &o.roots[0].children[0];
        assert_eq!(gr.rollup, expected);
        assert_eq!(gr.baselined, 1);
        assert!(gr.findings > gr.baselined, "new findings are counted apart from inherited ones");
        // layout carries a contract and unrelated is expected to; no page links either.
        let unplaced: Vec<String> = o.unplaced.iter().map(|t| t.r#ref.to_string()).collect();
        assert_eq!(unplaced, vec!["module:src/a/layout", "module:src/b/unrelated"]);
        assert_eq!(o.unplaced[0].children.len(), 1, "an unplaced module comes with its criteria");
        assert_eq!(o.unplaced[1].rollup, Rollup { modules: 1, ..Rollup::default() });
    }

    /// # Test contract
    ///
    /// ```json
    /// {"test": "a-page-linked-twice-counts-once", "checks": ["crates/core/src/query#outline"]}
    /// ```
    #[test]
    fn a_page_linked_twice_counts_once() {
        let mut s = crate::graph::fixtures::store();
        s.pages.insert("again".into(), crate::graph::fixtures::page("again", &["page:git-readiness", "module:src/a/ready"]));
        s.pages.get_mut("overview").expect("fixture").links.push("page:again".parse().expect("reference"));
        let g = Graph::build(&s, &crate::extraction::fixtures::extraction(), &[]);
        let o = outline(&g, 2, None);
        assert_eq!(o.roots[0].rollup, Rollup { modules: 1, contracted: 1, criteria: 2, tested: 1, requirements: 2, held: 2 });
    }

    /// # Test contract
    ///
    /// ```json
    /// {"test": "outline-from-roots", "checks": ["crates/core/src/query#outline"]}
    /// ```
    #[test]
    fn outline_from_roots() {
        let g = graph();
        let t = outline(&g, 3, None).roots;
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
        let shallow = outline(&g, 0, None).roots;
        assert_eq!(shallow[0].findings, t[0].findings);
        assert!(shallow[0].children.is_empty());
    }

    /// # Test contract
    ///
    /// ```json
    /// {"test": "rests-on-ticket-reaches-modules-and-decisions", "checks": ["crates/core/src/query#nearest-first"]}
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
    /// {"test": "rests-on-is-bounded-at-tickets-and-pages", "checks": ["crates/core/src/query#bounded-walk"]}
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
    /// {"test": "rests-on-me-climbs-to-pages-and-tickets", "checks": ["crates/core/src/query#rests-on-me"]}
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
    /// {"test": "requirement-coverage-in-order-with-modules", "checks": ["crates/core/src/query#coverage"]}
    /// ```
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

    /// # Test contract
    ///
    /// ```json
    /// {"test": "board-orders-and-filters", "checks": ["crates/core/src/query#board"]}
    /// ```
    fn band(order: &[&str], tickets: &[(&str, Status, &[&str])]) -> Vec<String> {
        let mut s = crate::graph::fixtures::store();
        s.tickets.clear();
        for (slug, status, relies) in tickets {
            let rel: Vec<&str> = relies.to_vec();
            let mut t = crate::graph::fixtures::ticket(slug, TicketKind::Change, Priority::High, *status, &[], &rel);
            t.title = slug.to_uppercase();
            s.tickets.insert(slug.to_string(), t);
        }
        s.config.order.high = order.iter().map(|o| o.to_string()).collect();
        let g = Graph::build(&s, &crate::extraction::fixtures::extraction(), &[]);
        board(&g, &BoardFilter { status: None, kinds: None, priority: None, touching: None }).into_iter().map(|t| t.r#ref.id).collect()
    }

    /// # Test contract
    ///
    /// ```json
    /// {"test": "a-band-is-ordered-by-list-then-dependency", "checks": ["crates/core/src/query#board"]}
    /// ```
    #[test]
    fn a_band_is_ordered_by_list_then_dependency() {
        use Status::*;
        // No list: a ticket after the open ticket it relies on, otherwise active, then title.
        assert_eq!(band(&[], &[("a", Open, &["ticket:c"]), ("b", Open, &[]), ("c", Open, &[])]), vec!["b", "c", "a"]);
        assert_eq!(band(&[], &[("a", Open, &[]), ("z", Active, &[])]), vec!["z", "a"], "active before open");
        // A list comes first, in its order; the rest follow.
        assert_eq!(band(&["b"], &[("a", Open, &["ticket:c"]), ("b", Open, &[]), ("c", Open, &[])]), vec!["b", "c", "a"]);
        assert_eq!(band(&["a", "c"], &[("a", Open, &["ticket:c"]), ("b", Open, &[]), ("c", Open, &[])]), vec!["a", "c", "b"], "a person's order wins over dependency");
        // A loop is broken by title, and a listed ticket that closed is ignored.
        assert_eq!(band(&["gone"], &[("a", Open, &["ticket:b"]), ("b", Open, &["ticket:a"])]), vec!["a", "b"]);
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

    /// # Test contract
    ///
    /// ```json
    /// {"test": "related-finds-gaps-on-shared-nodes", "checks": ["crates/core/src/query#related-tickets"]}
    /// ```
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
