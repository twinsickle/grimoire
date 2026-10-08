//! # Module contract
//!
//! ```json
//! {"purpose":"Walk the graph from a node: down to what it rests on, with the decisions behind each, and up to what rests on it.","criteria":[{"id":"nearest-first","statement":"rests_on lists nodes nearest the start first and, within a distance, by newest decision in force first, each with the path it was reached by; a requirement with its scenario_count and its decision_count, how many of its decisions are in force, and no decisions, a bottled ticket with its decision, and no other node with any."},{"id":"bounded-walk","statement":"rests_on expands only the starting node by its full rules; a ticket reached on the way contributes its decision once it is bottled and is not expanded, a page reached on the way contributes its requirements and modules but not its child pages, and a retired requirement contributes its counts but nothing below it, since it no longer holds; a recipe leads to the recipes it uses and the modules it represents, an ingredient to its recipe, and a decision to its requirement."},{"id":"says-direct","statement":"Each node rests_on lists at distance one from the start carries says, what it says as text: a page its body, a requirement its statement then each scenario, a module its purpose then each criterion by id, a recipe its text then each ingredient by id, a decision its date and text, a gap or question that is simmering its answer with its date; any other node at distance one, a module on a graph that holds no code, which has no contract to say, and every node further away, carries none."},{"id":"intent-of","statement":"intent_of lists each given node in order as rests_on lists a node, with says, a module followed by the requirements that name any of its criteria, in reference order, with their says and the module as their path; each node appears once, a requirement already given or already reached through an earlier module left out; a node no longer in the graph carries its reference with no decisions, counts or says."},{"id":"recipes-resting","statement":"recipes_over gives the recipes a change to a node can break: a recipe itself, an ingredient's recipe, the recipes a requirement or page names, and for a module, or the module of a file or criterion, the recipes representing it or a module importing it, however far up through imports alone, never through what it imports or through the requirements, tests and pages above it; then every recipe using one of those, however far up. recipes_resting gives, for the given nodes, every recipe recipes_over one of them gives, each once, in reference order, with its title, whether it is due and the first given node it was reached from; an ingredient is not a recipe, and a node the graph does not hold reaches none."},{"id":"assumptions","statement":"assumptions parts a rests_on walk into the nodes kept, every node at distance one, every requirement and every node carrying a decision in force, in the walk's order, and the references of the rest, in the walk's order; constraints gives each requirement the constraints page links, in its order, with says, but those given."},{"id":"rests-on-me","statement":"rests_on_me answers as binds the requirements naming the node's own criteria, a module's, the module's of a file or declaration, or a criterion itself, each once in reference order with says and the criterion it names as its path; as nodes the rest of the transitive upward closure of the node through the derived edges but tests, criteria, files and declarations, which it counts by kind as counted, a requirement's decisions left out since they are read through it, and a recipe or its ingredient only when the recipe is one recipes_over the node gives, nothing being reached through one left out; and separately the tickets not bottled whose touches or relies_on reach it."}]}
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
    // A bottled ticket's decision; a requirement's are counted, and read with get (req:query-nearest-first).
    pub decisions: Vec<Decision>,
    // A requirement's: how many scenarios it has, and how many of its decisions are in force.
    #[serde(skip_serializing_if = "Option::is_none")]
    #[cfg_attr(feature = "ts", ts(optional))]
    pub scenario_count: Option<usize>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[cfg_attr(feature = "ts", ts(optional))]
    pub decision_count: Option<usize>,
    // What the node says, for a node the start names directly; none further away.
    #[serde(skip_serializing_if = "Option::is_none")]
    #[cfg_attr(feature = "ts", ts(optional))]
    pub says: Option<String>,
}

impl RestsOn {
    /// A node as a walk lists it: what binds, and of why only a bottled ticket's decision, which is all that remains of it.
    fn of(g: &Graph, r: &Reference, via: Vec<Reference>, says: Option<String>) -> RestsOn {
        let title = g.get(r).map(Node::title).unwrap_or_else(|| absent_title(g, r));
        let (scenario_count, decision_count) = match g.get(r) {
            Some(Node::Req(q)) => (Some(q.scenarios.len()), Some(q.in_force().len())),
            _ => (None, None),
        };
        let decisions = match g.get(r) {
            // A simmering ticket carries a decision already written; nothing is decided until it is bottled.
            Some(Node::Ticket(t)) if t.status == Status::Bottled => t.decision.iter().cloned().collect(),
            _ => Vec::new(),
        };
        RestsOn { r#ref: r.clone(), title, via, decisions, scenario_count, decision_count, says }
    }
}

/// What a node says, as text: a page's body; a requirement's statement and
/// its scenarios; a module's purpose and its criteria; and a gap or question
/// answered and waiting for its change, its answer, which is what that change
/// applies; a decision, its date and text. Anything else says nothing beyond its title and decisions.
fn says_of(g: &Graph, r: &Reference) -> Option<String> {
    match g.get(r)? {
        Node::Page(p) => Some(p.body.clone()),
        Node::Req(q) => {
            let mut out = q.statement.clone();
            for c in &q.scenarios {
                out.push_str(&format!("\n\nScenario: {}. Given {}; when {}; then {}.", c.title, c.given, c.when, c.then));
            }
            Some(out)
        }
        Node::Module(m) => {
            let mut out = m.purpose.clone()?;
            for c in &m.criteria {
                out.push_str(&format!("\n- {}: {}", c.id, c.statement));
            }
            Some(out)
        }
        Node::Recipe(x) => {
            let mut out = x.text.clone();
            for i in &x.ingredients {
                out.push_str(&format!("\n- {}: {}", i.id, i.says));
            }
            Some(out)
        }
        Node::Decision(d) => Some(format!("Decided {}: {}", d.date, d.text)),
        Node::Ticket(t) if t.kind != TicketKind::Change && t.status == Status::Simmering => {
            t.decision.as_ref().map(|d| format!("Answered {}, waiting for the change that applies it: {}", d.date, d.text))
        }
        _ => None,
    }
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
        // What it uses, then what it represents: its implementation (req:recipe-in-tickets).
        Some(Node::Recipe(x)) => x.uses.iter().map(|u| Reference::recipe(u)).chain(g.out_with(r, Relation::Represents).map(|e| e.to.clone())).collect(),
        Some(Node::Ingredient(i)) => vec![Reference::recipe(&i.recipe)],
        Some(Node::Decision(d)) => vec![Reference::req(&d.requirement)],
        _ => Vec::new(),
    }
}

/// What stands in for the title of a node the graph does not hold: missing,
/// or, for a node from the code on a graph built with none, not held here.
pub fn absent_title(g: &Graph, r: &Reference) -> String {
    if g.without_code && r.kind.is_derived() {
        format!("(no code held) {r}")
    } else {
        format!("(missing) {r}")
    }
}

/// The date a walk orders a node by within its distance: its newest decision in force, a
/// requirement's or a bottled ticket's, though the walk no longer answers a requirement's.
fn newest_decision(g: &Graph, r: &Reference) -> String {
    match g.get(r) {
        Some(Node::Req(q)) => q.in_force().iter().map(|d| d.date.clone()).max().unwrap_or_default(),
        Some(Node::Ticket(t)) if t.status == Status::Bottled => t.decision.as_ref().map(|d| d.date.clone()).unwrap_or_default(),
        _ => String::new(),
    }
}

/// What rests on each node a ticket's work reached without naming it, as
/// rests_on lists a node, with `says`, and a module followed by the
/// requirements that name its criteria, reached through it. A node removed
/// since carries its reference alone. Each node is listed once: a requirement
/// on two of the modules, or among the nodes given, appears only where it is
/// first met.
pub fn intent_of(g: &Graph, nodes: &[Reference]) -> Vec<RestsOn> {
    let mut out = Vec::new();
    let mut listed: HashSet<Reference> = nodes.iter().cloned().collect();
    for r in nodes {
        out.push(RestsOn::of(g, r, vec![], says_of(g, r)));
        if let Some(Node::Module(m)) = g.get(r) {
            let mut reqs: Vec<Reference> =
                m.criteria.iter().flat_map(|c| g.inbound_with(&Reference::criterion(&m.id, &c.id), Relation::CoveredBy).map(|e| e.from.clone()).collect::<Vec<_>>()).collect();
            reqs.sort();
            reqs.dedup();
            for q in reqs.into_iter().filter(|q| listed.insert(q.clone())) {
                out.push(RestsOn::of(g, &q, vec![r.clone()], says_of(g, &q)));
            }
        }
    }
    out
}

pub fn rests_on(g: &Graph, start: &Reference, depth: Option<usize>) -> Vec<RestsOn> {
    let mut out: Vec<(usize, String, RestsOn)> = Vec::new();
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
            // What the start names directly comes with what it says; nothing further away does.
            let says = if dist == 0 { says_of(g, &next) } else { None };
            out.push((dist + 1, newest_decision(g, &next), RestsOn::of(g, &next, path.clone(), says)));
            queue.push_back((next, path));
        }
    }
    out.sort_by(|a, b| a.0.cmp(&b.0).then_with(|| b.1.cmp(&a.1)).then_with(|| a.2.r#ref.cmp(&b.2.r#ref)));
    out.into_iter().map(|(_, _, r)| r).collect()
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
    // What binds whoever edits the node: the requirements on its own criteria, with what they say (req:tool-rests-on-me).
    pub binds: Vec<RestsOn>,
    pub nodes: Vec<Affected>,
    // Tests, criteria, files and declarations reached, by kind: climbed through, not listed.
    pub counted: std::collections::BTreeMap<String, usize>,
    pub tickets: Vec<Summary>,
}

/// The node's own criteria: a module's, the module's of a file or declaration, or a criterion itself.
fn own_criteria(g: &Graph, start: &Reference) -> Vec<Reference> {
    let module_of_file = |f: &Reference| -> Vec<Reference> { g.inbound_with(f, Relation::Contains).filter(|e| e.from.kind == Kind::Module).map(|e| e.from.clone()).collect() };
    let modules = match g.get(start) {
        Some(Node::Criterion(_)) => return vec![start.clone()],
        Some(Node::Module(_)) => vec![start.clone()],
        Some(Node::File(_)) => module_of_file(start),
        Some(Node::Decl(d)) => module_of_file(&Reference::file(&d.file)),
        _ => vec![],
    };
    modules
        .iter()
        .filter_map(|m| match g.get(m) {
            Some(Node::Module(m)) => Some(m.criteria.iter().map(|c| Reference::criterion(&m.id, &c.id)).collect::<Vec<_>>()),
            _ => None,
        })
        .flatten()
        .collect()
}

/// Everything that would be affected if the node changed: first the
/// requirements on its own criteria, with what they say; then the rest of the
/// transitive upward closure through the derived edges, tests, criteria,
/// files and declarations counted rather than listed, and tickets apart.
pub fn rests_on_me(g: &Graph, start: &Reference) -> RestsOnMe {
    let mut binds: Vec<RestsOn> = Vec::new();
    let mut bound: HashSet<Reference> = HashSet::new();
    let mut by_criterion: Vec<(Reference, Reference)> =
        own_criteria(g, start).into_iter().flat_map(|c| g.inbound_with(&c, Relation::CoveredBy).map(|e| (e.from.clone(), c.clone())).collect::<Vec<_>>()).collect();
    by_criterion.sort();
    for (q, c) in by_criterion {
        if bound.insert(q.clone()) {
            binds.push(RestsOn::of(g, &q, vec![c], says_of(g, &q)));
        }
    }
    let mut counted: std::collections::BTreeMap<String, usize> = std::collections::BTreeMap::new();
    let mut nodes: Vec<Affected> = Vec::new();
    let recipes = recipes_over(g, start);
    let mut seen: HashSet<Reference> = HashSet::new();
    seen.insert(start.clone());
    let mut queue: VecDeque<Reference> = VecDeque::new();
    queue.push_back(start.clone());
    while let Some(r) = queue.pop_front() {
        for e in g.inbound(&r) {
            // A ticket is listed apart; a decision is read through its requirement and rests on nothing of its own (req:decision-references).
            if e.from.kind == Kind::Ticket || matches!(g.get(&e.from), Some(Node::Decision(_))) {
                continue;
            }
            // A recipe rests on what its implementation depends on, not on whatever else climbs to it (req:recipe-in-tickets).
            let recipe = match g.get(&e.from) {
                Some(Node::Recipe(_)) => Some(e.from.clone()),
                Some(Node::Ingredient(i)) => Some(Reference::recipe(&i.recipe)),
                _ => None,
            };
            if recipe.is_some_and(|x| !recipes.contains(&x)) {
                continue;
            }
            if !seen.insert(e.from.clone()) {
                continue;
            }
            queue.push_back(e.from.clone());
            // Climbed through, and counted: the tests run anyway, and a criterion, file or declaration is read through its module.
            if matches!(e.from.kind, Kind::Test | Kind::Criterion | Kind::File | Kind::Decl) {
                *counted.entry(e.from.kind.as_str().to_string()).or_default() += 1;
                continue;
            }
            if bound.contains(&e.from) {
                continue;
            }
            let title = g.get(&e.from).map(Node::title).unwrap_or_default();
            nodes.push(Affected { r#ref: e.from.clone(), title, relation: e.relation });
        }
    }
    let mut tickets: Vec<&Ticket> = g.tickets().filter(|t| t.status != Status::Bottled).filter(|t| t.touches.iter().chain(t.relies_on.iter()).any(|x| seen.contains(x))).collect();
    tickets.sort_by(|a, b| a.slug.cmp(&b.slug));
    RestsOnMe { binds, nodes, counted, tickets: tickets.into_iter().map(ticket_summary).collect() }
}

/// A recipe resting on what a ticket touches, and whether a judgement holds of it as it is.
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct RecipeState {
    pub r#ref: Reference,
    pub title: String,
    pub due: bool,
    // The node it rests on that it was first reached from, or itself when it was given.
    pub via: Reference,
}

/// The recipes a change to `start` can break (req:recipe-in-tickets): a recipe itself, an ingredient's
/// recipe, the recipes a requirement or page names, and for a module, or the module of a file or
/// criterion, the recipes representing it or a module that imports it, however far up; then every
/// recipe using one of those, however far up. Not those reached through what it imports, nor
/// through the requirements, tests and pages above it, which would name every recipe at once.
pub fn recipes_over(g: &Graph, start: &Reference) -> std::collections::BTreeSet<Reference> {
    let module_of = |r: &Reference| -> Vec<Reference> {
        match g.get(r) {
            Some(Node::Module(_)) => vec![r.clone()],
            Some(Node::Criterion(c)) => vec![Reference::module(&c.module)],
            Some(Node::File(_)) => g.inbound_with(r, Relation::Contains).filter(|e| e.from.kind == Kind::Module).map(|e| e.from.clone()).collect(),
            _ => vec![],
        }
    };
    let mut found: std::collections::BTreeSet<Reference> = std::collections::BTreeSet::new();
    match g.get(start) {
        Some(Node::Recipe(_)) => {
            found.insert(start.clone());
        }
        Some(Node::Ingredient(i)) => {
            found.insert(Reference::recipe(&i.recipe));
        }
        Some(Node::Req(_) | Node::Page(_)) => {
            for e in g.out(start) {
                match g.get(&e.to) {
                    Some(Node::Recipe(_)) => {
                        found.insert(e.to.clone());
                    }
                    Some(Node::Ingredient(i)) => {
                        found.insert(Reference::recipe(&i.recipe));
                    }
                    _ => {}
                }
            }
        }
        _ => {
            // Upward through imports alone: the modules whose behaviour depends on this one.
            let mut seen: HashSet<Reference> = HashSet::new();
            let mut queue: VecDeque<Reference> = module_of(start).into_iter().collect();
            while let Some(m) = queue.pop_front() {
                if !seen.insert(m.clone()) {
                    continue;
                }
                found.extend(g.inbound_with(&m, Relation::Represents).map(|e| e.from.clone()));
                queue.extend(g.inbound_with(&m, Relation::Imports).map(|e| e.from.clone()));
            }
        }
    }
    // A recipe using one of them is built on it.
    let mut queue: VecDeque<Reference> = found.iter().cloned().collect();
    while let Some(x) = queue.pop_front() {
        for e in g.inbound_with(&x, Relation::Uses) {
            if found.insert(e.from.clone()) {
                queue.push_back(e.from.clone());
            }
        }
    }
    found.retain(|x| matches!(g.get(x), Some(Node::Recipe(_))));
    found
}

/// Every recipe `recipes_over` one of `nodes` gives, once each, in reference order, with the first node that reached it.
pub fn recipes_resting(g: &Graph, nodes: &[Reference]) -> Vec<RecipeState> {
    let mut out: std::collections::BTreeMap<Reference, RecipeState> = std::collections::BTreeMap::new();
    for n in nodes.iter().filter(|n| g.contains(n)) {
        for r in recipes_over(g, n) {
            if let Some(Node::Recipe(x)) = g.get(&r) {
                out.entry(r.clone()).or_insert_with(|| RecipeState { r#ref: r.clone(), title: x.title.clone(), due: g.is_due(x), via: n.clone() });
            }
        }
    }
    out.into_values().collect()
}

/// What holds whichever part is changed: each requirement the constraints page links, in its order,
/// saying what it says, but those in `given`, already said where they were named.
pub fn constraints(g: &Graph, given: &[Reference]) -> Vec<RestsOn> {
    let Some(Node::Page(p)) = g.get(&Reference::page(crate::lint::CONSTRAINTS_PAGE)) else { return Vec::new() };
    p.links.iter().filter(|l| l.kind == Kind::Req && !given.contains(l) && g.contains(l)).map(|q| RestsOn::of(g, q, vec![], says_of(g, q))).collect()
}

/// What brew_ticket answers of its walk: the map of what binds the work, every node the start names
/// directly, every requirement by its counts and every node carrying a decision in force; and the
/// rest, by reference alone, in the walk's order.
pub fn assumptions(walk: Vec<RestsOn>) -> (Vec<RestsOn>, Vec<Reference>) {
    let (kept, rest): (Vec<RestsOn>, Vec<RestsOn>) = walk.into_iter().partition(|x| x.via.len() <= 1 || x.r#ref.kind == Kind::Req || !x.decisions.is_empty());
    (kept, rest.into_iter().map(|x| x.r#ref).collect())
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
        // A requirement by its counts: what binds, and how much was decided, never the decisions (req:query-nearest-first).
        let req = r.iter().find(|x| x.r#ref == Reference::req("ready-only-with-commit")).unwrap();
        assert!(req.decisions.is_empty());
        assert_eq!(req.decision_count, Some(1));
        assert!(req.scenario_count.is_some_and(|n| n > 0), "{req:?}");
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
        // A ticket relying on the bottled ticket `old` (which touches a missing page)
        // and on the root page, which links to the whole grimoire.
        let t = crate::graph::fixtures::ticket("new", TicketKind::Change, Priority::Next, Status::Inscribed, &[], &["ticket:old", "page:overview"]);
        store.tickets.insert(t.slug.clone(), t);
        let g = Graph::build(&store, &crate::extraction::fixtures::extraction(), &[]);
        let r = rests_on(&g, &Reference::ticket("new"), None);
        let refs: Vec<String> = r.iter().map(|x| x.r#ref.to_string()).collect();
        // The bottled ticket is present with its decision and was not expanded.
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
        assert!(!refs.iter().any(|x| x.contains("#d")), "a requirement's decisions are read through it: {refs:?}");
        let tickets: Vec<String> = r.tickets.iter().map(|t| t.r#ref.id.clone()).collect();
        assert_eq!(tickets, vec!["gap-1", "phase-62"]);
    }

    /// # Test contract
    ///
    /// ```json
    /// {"test": "what-binds-a-module-comes-first", "checks": ["crates/core/src/query/walks#rests-on-me", "req:tool-rests-on-me"]}
    /// ```
    #[test]
    fn what_binds_a_module_comes_first() {
        let g = graph();
        // The module's own criterion is named by a requirement: it binds whoever edits it, saying what it says.
        let r = rests_on_me(&g, &Reference::module("src/a/ready"));
        let bound: Vec<String> = r.binds.iter().map(|x| x.r#ref.to_string()).collect();
        assert_eq!(bound, ["req:ready-only-with-commit"]);
        let req = &r.binds[0];
        assert_eq!(req.via, [Reference::criterion("src/a/ready", "guarantee")], "the criterion it names");
        assert!(req.says.as_deref().is_some_and(|s| s.starts_with("The system SHALL ready-only-with-commit.") && s.contains("Scenario: It holds.")), "{req:?}");
        assert!(req.decisions.is_empty() && req.decision_count == Some(1));
        // The rest by name; tests and criteria counted, not listed, and what binds not listed twice.
        let names: Vec<String> = r.nodes.iter().map(|x| x.r#ref.to_string()).collect();
        assert!(names.contains(&"page:git-readiness".to_string()) && names.contains(&"page:overview".to_string()), "{names:?}");
        assert!(!names.iter().any(|n| n.starts_with("test:") || n.starts_with("criterion:") || n == "req:ready-only-with-commit"), "{names:?}");
        assert_eq!(r.counted.get("criterion"), Some(&2), "{:?}", r.counted);
        assert_eq!(r.counted.get("test"), Some(&1), "{:?}", r.counted);
        // A module imported by it: nothing of its own binds, and the importer's requirement comes by name.
        let below = rests_on_me(&g, &Reference::module("src/a/layout"));
        assert!(below.binds.is_empty(), "{:?}", below.binds);
        assert!(below.nodes.iter().any(|x| x.r#ref == Reference::req("ready-only-with-commit")));
        // A file is its module's, a criterion itself.
        assert_eq!(rests_on_me(&g, &Reference::file("src/a/ready.rs")).binds.len(), 1);
        assert_eq!(rests_on_me(&g, &Reference::criterion("src/a/ready", "guarantee")).binds.len(), 1);
        assert!(rests_on_me(&g, &Reference::criterion("src/a/ready", "reads-only")).binds.is_empty());
    }

    /// # Test contract
    ///
    /// ```json
    /// {"test": "the-constraints-say-what-they-say", "checks": ["crates/core/src/query/walks#assumptions"]}
    /// ```
    #[test]
    fn the_constraints_say_what_they_say() {
        let mut store = crate::graph::fixtures::store();
        assert!(constraints(&Graph::build(&store, &crate::extraction::fixtures::extraction(), &[]), &[]).is_empty(), "no constraints page, none");
        store.pages.insert(crate::lint::CONSTRAINTS_PAGE.into(), crate::graph::fixtures::page(crate::lint::CONSTRAINTS_PAGE, &["req:stale", "req:ready-only-with-commit", "req:missing"]));
        let g = Graph::build(&store, &crate::extraction::fixtures::extraction(), &[]);
        let all: Vec<String> = constraints(&g, &[]).iter().map(|x| x.r#ref.id.clone()).collect();
        assert_eq!(all, ["stale", "ready-only-with-commit"], "in the page's order, a missing one left out");
        let c = constraints(&g, &[Reference::req("stale")]);
        assert_eq!(c.len(), 1, "one already named is said where it was named");
        assert!(c[0].says.as_deref().is_some_and(|s| s.contains("Scenario:")), "{c:?}");
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
        assert_eq!(req.decision_count, Some(1), "the superseded decision is not counted");
        assert!(req.decisions.is_empty(), "read with get, not answered by the walk");
        // Ordered by its newest decision in force, though the walk no longer answers it: the newer first within a distance.
        let mut newer = crate::graph::fixtures::store();
        newer.pages.get_mut("git-readiness").expect("fixture").links.push(Reference::req("other-rule"));
        let mut other = newer.requirements["ready-only-with-commit"].clone();
        other.slug = "other-rule".into();
        other.decisions = vec![Decision::new("2026-10-30", "Later.")];
        newer.requirements.insert(other.slug.clone(), other);
        let g2 = Graph::build(&newer, &crate::extraction::fixtures::extraction(), &[]);
        let order: Vec<String> = rests_on(&g2, &Reference::page("git-readiness"), None).into_iter().filter(|x| x.r#ref.kind == Kind::Req).map(|x| x.r#ref.id).collect();
        assert_eq!(order[..2], ["other-rule", "ready-only-with-commit"], "{order:?}");
        // Retired, it contributes its counts and nothing below it.
        s.requirements.get_mut("ready-only-with-commit").expect("fixture").retired = true;
        let g = Graph::build(&s, &crate::extraction::fixtures::extraction(), &[]);
        let r = rests_on(&g, &Reference::req("ready-only-with-commit"), None);
        assert!(r.is_empty(), "a retired requirement has nothing below it: {:?}", r.iter().map(|x| &x.r#ref).collect::<Vec<_>>());
        let from_ticket = rests_on(&g, &Reference::ticket("phase-62"), None);
        assert!(from_ticket.iter().all(|x| x.via.last() != Some(&Reference::req("ready-only-with-commit"))), "nothing reached through it");
    }

    /// # Test contract
    ///
    /// ```json
    /// {"test": "a-simmering-tickets-decision-is-not-yet-decided", "checks": ["crates/core/src/query/walks#bounded-walk"]}
    /// ```
    #[test]
    fn a_simmering_tickets_decision_is_not_yet_decided() {
        let mut store = crate::graph::fixtures::store();
        store.tickets.insert("waits".into(), crate::graph::fixtures::ticket("waits", TicketKind::Change, Priority::Next, Status::Simmering, &[], &[]));
        let t = crate::graph::fixtures::ticket("new", TicketKind::Change, Priority::Next, Status::Inscribed, &[], &["ticket:waits", "ticket:old"]);
        store.tickets.insert(t.slug.clone(), t);
        let g = Graph::build(&store, &crate::extraction::fixtures::extraction(), &[]);
        let r = rests_on(&g, &Reference::ticket("new"), None);
        let decisions = |slug: &str| r.iter().find(|x| x.r#ref == Reference::ticket(slug)).unwrap().decisions.len();
        assert_eq!((decisions("waits"), decisions("old")), (0, 1), "a decision written is not one in force until the change is bottled");
    }

    /// # Test contract
    ///
    /// ```json
    /// {"test": "a-recipe-rests-on-what-it-uses", "checks": ["crates/core/src/query/walks#says-direct", "crates/core/src/query/walks#bounded-walk"]}
    /// ```
    #[test]
    fn a_recipe_rests_on_what_it_uses() {
        use crate::extraction::fixtures::{extraction_files, recipe_file};
        let mut files = extraction_files();
        files.push(recipe_file("ui/button.recipe.md", "button", "component", &["icon"], &["src/a/ready.rs"], &[]));
        files.push(recipe_file("ui/icon.recipe.md", "icon", "component", &[], &[], &[]));
        let g = Graph::build(&crate::graph::fixtures::store(), &crate::extraction::Extraction::from_files(&files), &[]);
        let r = rests_on(&g, &Reference::recipe("button"), None);
        // Its implementation too: the module it represents, and what that module imports.
        assert!(r.iter().any(|x| x.r#ref == Reference::module("src/a/ready") && x.via == [Reference::recipe("button")]), "{r:?}");
        assert!(r.iter().any(|x| x.r#ref == Reference::module("src/a/layout")));
        let icon = r.iter().find(|x| x.r#ref == Reference::recipe("icon")).expect("a recipe rests on what it uses");
        assert_eq!(icon.says.as_deref(), Some("How to use icon.\n- label: The text shown."), "its text, then each ingredient");
        let from_ingredient = rests_on(&g, &Reference::ingredient("button", "label"), None);
        assert_eq!(from_ingredient.first().map(|x| x.r#ref.clone()), Some(Reference::recipe("button")), "an ingredient rests on its recipe");
        assert!(from_ingredient.iter().any(|x| x.r#ref == Reference::recipe("icon")));
    }

    /// # Test contract
    ///
    /// ```json
    /// {"test": "what-the-brew-names-says-what-it-says", "checks": ["crates/core/src/query/walks#says-direct", "crates/core/src/query/walks#bounded-walk"]}
    /// ```
    #[test]
    fn what_the_start_names_says_what_it_says() {
        let mut store = crate::graph::fixtures::store();
        // A gap answered and waiting for the change relying on it, and one still inscribed.
        let mut answered = crate::graph::fixtures::ticket("answered", TicketKind::Gap, Priority::Next, Status::Simmering, &[], &[]);
        answered.decision = Some(Decision::new("2026-10-04", "Fix it so."));
        store.tickets.insert(answered.slug.clone(), answered);
        store.tickets.insert("unanswered".into(), crate::graph::fixtures::ticket("unanswered", TicketKind::Gap, Priority::Next, Status::Inscribed, &[], &[]));
        let t = crate::graph::fixtures::ticket(
            "work",
            TicketKind::Change,
            Priority::Next,
            Status::Inscribed,
            &["req:ready-only-with-commit", "page:git-readiness", "module:src/a/ready"],
            &["ticket:answered", "ticket:unanswered", "ticket:old"],
        );
        store.tickets.insert(t.slug.clone(), t);
        // A ticket resting on one decision: it says its text, and leads to its requirement.
        let on_decision = crate::graph::fixtures::ticket("on-decision", TicketKind::Change, Priority::Next, Status::Inscribed, &[], &["req:ready-only-with-commit#d1"]);
        store.tickets.insert(on_decision.slug.clone(), on_decision);
        let g = Graph::build(&store, &crate::extraction::fixtures::extraction(), &[]);
        let r = rests_on(&g, &Reference::ticket("work"), None);
        let says = |r_: &Reference| r.iter().find(|x| &x.r#ref == r_).unwrap_or_else(|| panic!("{r_} reached")).says.clone();
        let Some(Node::Page(page)) = g.get(&Reference::page("git-readiness")) else { panic!("fixture page") };
        assert_eq!(says(&Reference::page("git-readiness")), Some(page.body.clone()));
        let Some(Node::Req(req)) = g.get(&Reference::req("ready-only-with-commit")) else { panic!("fixture req") };
        let req_says = says(&Reference::req("ready-only-with-commit")).expect("a requirement says its statement");
        assert!(req_says.starts_with(&req.statement), "{req_says}");
        for c in &req.scenarios {
            assert!(req_says.contains(&format!("Scenario: {}. Given {}; when {}; then {}.", c.title, c.given, c.when, c.then)), "{req_says}");
        }
        let Some(Node::Module(module)) = g.get(&Reference::module("src/a/ready")) else { panic!("fixture module") };
        let module_says = says(&Reference::module("src/a/ready")).expect("a module says its contract");
        assert!(module_says.starts_with(module.purpose.as_deref().unwrap()), "{module_says}");
        for c in &module.criteria {
            assert!(module_says.contains(&format!("- {}: {}", c.id, c.statement)), "{module_says}");
        }
        // The answer a change applies, though not yet in force; nothing for an inscribed gap or a bottled ticket, whose decision is listed already.
        assert_eq!(says(&Reference::ticket("answered")), Some("Answered 2026-10-04, waiting for the change that applies it: Fix it so.".to_string()));
        assert_eq!(r.iter().find(|x| x.r#ref == Reference::ticket("answered")).unwrap().decisions.len(), 0);
        assert_eq!((says(&Reference::ticket("unanswered")), says(&Reference::ticket("old"))), (None, None));
        let d = rests_on(&g, &Reference::ticket("on-decision"), None);
        let d1 = d.iter().find(|x| x.r#ref == Reference::decision("ready-only-with-commit", 1)).expect("the decision it relies on");
        assert_eq!(d1.says.as_deref(), Some("Decided 2026-09-12: Identity is required rather than invented."));
        assert!(d.iter().any(|x| x.r#ref == Reference::req("ready-only-with-commit") && x.says.is_none()), "its requirement, one step further");
        // Further away, nothing: the module imported by one named directly.
        assert_eq!(says(&Reference::module("src/a/layout")), None);
        // Named directly but saying nothing of its own: a criterion, whose statement is its title, and a node that does not exist.
        let mut named = store.clone();
        let t = crate::graph::fixtures::ticket("odd", TicketKind::Change, Priority::Next, Status::Inscribed, &["criterion:src/a/ready#guarantee", "page:nowhere"], &[]);
        named.tickets.insert(t.slug.clone(), t);
        let g2 = Graph::build(&named, &crate::extraction::fixtures::extraction(), &[]);
        let r2 = rests_on(&g2, &Reference::ticket("odd"), None);
        assert!(r2.iter().any(|x| x.r#ref == Reference::page("nowhere")) && r2.iter().any(|x| x.r#ref.kind == Kind::Criterion));
        assert!(r2.iter().all(|x| x.says.is_none()), "{:?}", r2.iter().map(|x| (&x.r#ref, &x.says)).collect::<Vec<_>>());
        // From a page, its own direct links say what they say too.
        let from_page = rests_on(&g, &Reference::page("git-readiness"), None);
        assert!(from_page.iter().find(|x| x.r#ref == Reference::req("ready-only-with-commit")).and_then(|x| x.says.clone()).is_some());
    }

    /// # Test contract
    ///
    /// ```json
    /// {"test": "a-simmering-ticket-still-reaches-what-it-touches", "checks": ["crates/core/src/query/walks#rests-on-me"]}
    /// ```
    #[test]
    fn a_simmering_ticket_still_reaches_what_it_touches() {
        let mut store = crate::graph::fixtures::store();
        store.tickets.insert("waits".into(), crate::graph::fixtures::ticket("waits", TicketKind::Change, Priority::Next, Status::Simmering, &["page:orphan"], &[]));
        store.tickets.insert("done".into(), crate::graph::fixtures::ticket("done", TicketKind::Change, Priority::Next, Status::Bottled, &["page:orphan"], &[]));
        let g = Graph::build(&store, &crate::extraction::fixtures::extraction(), &[]);
        let reaching: Vec<String> = rests_on_me(&g, &Reference::page("orphan")).tickets.iter().map(|t| t.r#ref.to_string()).collect();
        assert!(reaching.contains(&"ticket:waits".to_string()), "{reaching:?}");
        assert!(!reaching.contains(&"ticket:done".to_string()), "a bottled ticket no longer reaches it: {reaching:?}");
    }

    /// # Test contract
    ///
    /// ```json
    /// {"test": "the-recipes-resting-on-what-is-touched", "checks": ["crates/core/src/query/walks#recipes-resting", "crates/core/src/query/walks#rests-on-me", "req:recipe-in-tickets"]}
    /// ```
    #[test]
    fn the_recipes_resting_on_what_is_touched() {
        use crate::extraction::fixtures::{extraction_files, recipe_file};
        let mut files = extraction_files();
        // `layout` represents the module src/a/ready imports; `ready` the module itself; `other` neither.
        files.push(recipe_file("ui/layout.recipe.md", "layout", "component", &[], &["src/a/layout.rs"], &[]));
        files.push(recipe_file("ui/ready.recipe.md", "ready", "component", &[], &["src/a/ready.rs"], &[]));
        files.push(recipe_file("ui/other.recipe.md", "other", "component", &[], &["src/b/unrelated.rs"], &[]));
        let mut store = crate::graph::fixtures::store();
        store.judgements.insert("ready".into(), vec![]);
        let g = Graph::build(&store, &crate::extraction::Extraction::from_files(&files), &[]);
        // Upward from the imported module: the module importing it, and both recipes, through represents.
        let up = rests_on_me(&g, &Reference::module("src/a/layout"));
        assert!(up.nodes.iter().any(|a| a.r#ref == Reference::recipe("layout") && a.relation == Relation::Represents));
        assert!(up.nodes.iter().any(|a| a.r#ref == Reference::recipe("ready")), "through the module that imports it");
        let at = |nodes: &[Reference]| -> Vec<(String, bool, String)> { recipes_resting(&g, nodes).into_iter().map(|x| (x.r#ref.to_string(), x.due, x.via.to_string())).collect() };
        assert_eq!(
            at(&[Reference::module("src/a/layout"), Reference::module("src/a/ready")]),
            [("recipe:layout".to_string(), true, "module:src/a/layout".to_string()), ("recipe:ready".to_string(), true, "module:src/a/layout".to_string())],
            "each once, by reference, with the first node that reached it"
        );
        // A recipe given is itself; an ingredient is none, and a node the graph lacks reaches nothing.
        assert_eq!(at(&[Reference::recipe("other")]), [("recipe:other".to_string(), true, "recipe:other".to_string())]);
        assert_eq!(at(&[Reference::ingredient("other", "label")]), [("recipe:other".to_string(), true, "recipe:other#label".to_string())], "an ingredient's recipe");
        assert!(at(&[Reference::module("gone"), Reference::page("orphan")]).is_empty());
        assert!(!recipes_resting(&g, &[Reference::recipe("other")]).iter().any(|x| x.r#ref.id.contains('#')));
    }

    /// # Test contract
    ///
    /// ```json
    /// {"test": "a-ticket-is-told-of-the-recipes-above-what-it-touches-not-below", "checks": ["crates/core/src/query/walks#recipes-resting", "crates/core/src/query/walks#rests-on-me", "req:recipe-in-tickets"]}
    /// ```
    #[test]
    fn a_ticket_is_told_of_the_recipes_above_what_it_touches_not_below() {
        use crate::extraction::fixtures::{extraction_files, recipe_file};
        let mut files = extraction_files();
        files.push(recipe_file("ui/layout.recipe.md", "layout", "component", &[], &["src/a/layout.rs"], &[]));
        files.push(recipe_file("ui/ready.recipe.md", "ready", "component", &[], &["src/a/ready.rs"], &[]));
        files.push(recipe_file("ui/other.recipe.md", "other", "component", &[], &["src/b/unrelated.rs"], &[]));
        files.push(recipe_file("ui/user.recipe.md", "user", "component", &["ready"], &[], &[]));
        // A test in the unrelated module checks layout's criterion: the upward closure climbs to that module that way, not by an import.
        let unrelated = files.iter_mut().find(|f| f.path == "src/b/unrelated.rs").expect("fixture");
        unrelated.tests = vec![Test { file: "src/b/unrelated.rs".into(), name: "layout-is-stable".into(), line: 3, checks: vec!["src/a/layout#stable".into()] }];
        let mut store = crate::graph::fixtures::store();
        // A requirement naming one recipe.
        store.requirements.get_mut("ready-only-with-commit").expect("fixture").covered_by.push(Reference::recipe("other"));
        let g = Graph::build(&store, &crate::extraction::Extraction::from_files(&files), &[]);
        let at = |n: Reference| -> Vec<String> { recipes_resting(&g, &[n]).into_iter().map(|x| x.r#ref.to_string()).collect() };
        // Touching the low-level module: its own recipe, the recipe of the module importing it, and the recipe using that one.
        assert_eq!(at(Reference::module("src/a/layout")), ["recipe:layout", "recipe:ready", "recipe:user"]);
        // Touching the high-level module: not the recipe of the module it imports.
        assert_eq!(at(Reference::module("src/a/ready")), ["recipe:ready", "recipe:user"]);
        // A file or a criterion is its module's; a requirement tells of the recipe it names, and nothing above it.
        assert_eq!(at(Reference::file("src/a/ready.rs")), ["recipe:ready", "recipe:user"]);
        assert_eq!(at(Reference::criterion("src/a/layout", "stable")), ["recipe:layout", "recipe:ready", "recipe:user"]);
        assert_eq!(at(Reference::req("ready-only-with-commit")), ["recipe:other"]);
        // rests_on_me still climbs to the unrelated module through the test, but lists no recipe it reaches that way.
        let up: Vec<String> = rests_on_me(&g, &Reference::module("src/a/layout")).nodes.into_iter().map(|a| a.r#ref.to_string()).collect();
        assert!(up.contains(&"module:src/b/unrelated".to_string()), "{up:?}");
        assert!(!up.iter().any(|r| r.starts_with("recipe:other")), "{up:?}");
        assert!(up.contains(&"recipe:user".to_string()) && up.contains(&"recipe:ready#label".to_string()), "{up:?}");
    }

    /// # Test contract
    ///
    /// ```json
    /// {"test": "the-assumptions-in-full-and-the-rest-by-reference", "checks": ["crates/core/src/query/walks#assumptions"]}
    /// ```
    #[test]
    fn the_assumptions_in_full_and_the_rest_by_reference() {
        let mut store = crate::graph::fixtures::store();
        let t = crate::graph::fixtures::ticket("work", TicketKind::Change, Priority::Next, Status::Inscribed, &["module:src/a/ready", "page:orphan"], &["ticket:old"]);
        store.tickets.insert(t.slug.clone(), t);
        let g = Graph::build(&store, &crate::extraction::fixtures::extraction(), &[]);
        let walk = rests_on(&g, &Reference::ticket("work"), None);
        let (kept, rest) = assumptions(walk.clone());
        let refs: Vec<String> = kept.iter().map(|x| x.r#ref.to_string()).collect();
        // Named directly, with what each says, and the bottled ticket with its decision.
        assert!(refs.contains(&"module:src/a/ready".to_string()) && refs.contains(&"page:orphan".to_string()) && refs.contains(&"ticket:old".to_string()), "{refs:?}");
        // The module it imports and the criteria further away, by reference alone.
        assert!(rest.contains(&Reference::module("src/a/layout")), "{rest:?}");
        assert!(rest.iter().all(|r| r.kind != Kind::Req));
        assert_eq!(kept.len() + rest.len(), walk.len(), "nothing lost");
        // In the walk's order.
        let order: Vec<Reference> = walk.iter().map(|x| x.r#ref.clone()).filter(|r| rest.contains(r)).collect();
        assert_eq!(order, rest);
    }
}
