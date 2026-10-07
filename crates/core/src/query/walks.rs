//! # Module contract
//!
//! ```json
//! {"purpose":"Walk the graph from a node: down to what it rests on, with the decisions behind each, and up to what rests on it.","criteria":[{"id":"nearest-first","statement":"rests_on lists nodes nearest the start first and, within a distance, by newest decision first, each with the path it was reached by and the decisions it carries that are in force, one superseded by another left out."},{"id":"bounded-walk","statement":"rests_on expands only the starting node by its full rules; a ticket reached on the way contributes its decision once it is closed and is not expanded, a page reached on the way contributes its requirements and modules but not its child pages, and a retired requirement contributes its decisions but nothing below it, since it no longer holds; a recipe leads to the recipes it uses and the modules it represents, and an ingredient to its recipe."},{"id":"says-direct","statement":"Each node rests_on lists at distance one from the start carries says, what it says as text: a page its body, a requirement its statement then each scenario, a module its purpose then each criterion by id, a recipe its text then each ingredient by id, a gap or question that is landing its answer with its date; any other node at distance one, a module on a graph that holds no code, which has no contract to say, and every node further away, carries none."},{"id":"intent-of","statement":"intent_of lists each given node in order with its decisions in force and says, a module followed by the requirements that name any of its criteria, in reference order, with their decisions and says and the module as their path; each node appears once, a requirement already given or already reached through an earlier module left out; a node no longer in the graph carries its reference with no decisions and no says."},{"id":"recipes-resting","statement":"recipes_resting gives, for the given nodes, every recipe that is one of them, whose ingredient is one of them, or that rests_on_me of one of them reaches, each once, in reference order, with its title, whether it is due and the first given node it was reached from; an ingredient is not a recipe, and a node the graph does not hold reaches none."},{"id":"rests-on-me","statement":"rests_on_me returns the transitive upward closure of a node through the derived edges, and separately the tickets not closed whose touches or relies_on reach it."}]}
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
    // What the node says, for a node the start names directly; none further away.
    #[serde(skip_serializing_if = "Option::is_none")]
    #[cfg_attr(feature = "ts", ts(optional))]
    pub says: Option<String>,
}

/// What a node says, as text: a page's body; a requirement's statement and
/// its scenarios; a module's purpose and its criteria; and a gap or question
/// answered and waiting for its change, its answer, which is what that change
/// applies. Anything else says nothing beyond its title and decisions.
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
        Node::Ticket(t) if t.kind != TicketKind::Change && t.status == Status::Landing => {
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
        // A landing ticket carries a decision already written; nothing is decided until it lands.
        Some(Node::Ticket(t)) if t.status == Status::Closed => t.decision.iter().cloned().collect(),
        _ => Vec::new(),
    }
}

/// What rests on each node a ticket's work reached without naming it, as
/// rests_on lists a node: each with its decisions in force and `says`, and a
/// module followed by the requirements that name its criteria, reached
/// through it. A node removed since carries its reference alone. Each node
/// is listed once: a requirement on two of the modules, or among the nodes
/// given, appears only where it is first met.
pub fn intent_of(g: &Graph, nodes: &[Reference]) -> Vec<RestsOn> {
    let mut out = Vec::new();
    let mut listed: HashSet<Reference> = nodes.iter().cloned().collect();
    for r in nodes {
        let title = g.get(r).map(Node::title).unwrap_or_else(|| absent_title(g, r));
        out.push(RestsOn { r#ref: r.clone(), title, via: vec![], decisions: decisions_of(g, r), says: says_of(g, r) });
        if let Some(Node::Module(m)) = g.get(r) {
            let mut reqs: Vec<Reference> =
                m.criteria.iter().flat_map(|c| g.inbound_with(&Reference::criterion(&m.id, &c.id), Relation::CoveredBy).map(|e| e.from.clone()).collect::<Vec<_>>()).collect();
            reqs.sort();
            reqs.dedup();
            for q in reqs.into_iter().filter(|q| listed.insert(q.clone())) {
                let title = g.get(&q).map(Node::title).unwrap_or_default();
                out.push(RestsOn { r#ref: q.clone(), title, via: vec![r.clone()], decisions: decisions_of(g, &q), says: says_of(g, &q) });
            }
        }
    }
    out
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
            let title = g.get(&next).map(Node::title).unwrap_or_else(|| absent_title(g, &next));
            // What the start names directly comes with what it says; nothing further away does.
            let says = if dist == 0 { says_of(g, &next) } else { None };
            out.push((dist + 1, RestsOn { r#ref: next.clone(), title, via: path.clone(), decisions, says }));
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
    let mut tickets: Vec<&Ticket> = g.tickets().filter(|t| t.status != Status::Closed).filter(|t| t.touches.iter().chain(t.relies_on.iter()).any(|x| seen.contains(x))).collect();
    tickets.sort_by(|a, b| a.slug.cmp(&b.slug));
    RestsOnMe { nodes, tickets: tickets.into_iter().map(ticket_summary).collect() }
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

/// Every recipe that is one of `nodes` or rests on one of them, once each, in reference order (req:recipe-in-tickets).
pub fn recipes_resting(g: &Graph, nodes: &[Reference]) -> Vec<RecipeState> {
    let mut out: std::collections::BTreeMap<Reference, RecipeState> = std::collections::BTreeMap::new();
    for n in nodes.iter().filter(|n| g.contains(n)) {
        let above = rests_on_me(g, n).nodes.into_iter().map(|a| a.r#ref);
        // An ingredient touched is its recipe's: the recipe is what is judged.
        let own = match g.get(n) {
            Some(Node::Ingredient(i)) => Some(Reference::recipe(&i.recipe)),
            _ => None,
        };
        for r in std::iter::once(n.clone()).chain(own).chain(above) {
            if let Some(Node::Recipe(x)) = g.get(&r) {
                out.entry(r.clone()).or_insert_with(|| RecipeState { r#ref: r.clone(), title: x.title.clone(), due: g.is_due(x), via: n.clone() });
            }
        }
    }
    out.into_values().collect()
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

    /// # Test contract
    ///
    /// ```json
    /// {"test": "a-landing-tickets-decision-is-not-yet-decided", "checks": ["crates/core/src/query/walks#bounded-walk"]}
    /// ```
    #[test]
    fn a_landing_tickets_decision_is_not_yet_decided() {
        let mut store = crate::graph::fixtures::store();
        store.tickets.insert("waits".into(), crate::graph::fixtures::ticket("waits", TicketKind::Change, Priority::Next, Status::Landing, &[], &[]));
        let t = crate::graph::fixtures::ticket("new", TicketKind::Change, Priority::Next, Status::Open, &[], &["ticket:waits", "ticket:old"]);
        store.tickets.insert(t.slug.clone(), t);
        let g = Graph::build(&store, &crate::extraction::fixtures::extraction(), &[]);
        let r = rests_on(&g, &Reference::ticket("new"), None);
        let decisions = |slug: &str| r.iter().find(|x| x.r#ref == Reference::ticket(slug)).unwrap().decisions.len();
        assert_eq!((decisions("waits"), decisions("old")), (0, 1), "a decision written is not one in force until the change lands");
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
    /// {"test": "what-the-start-names-says-what-it-says", "checks": ["crates/core/src/query/walks#says-direct"]}
    /// ```
    #[test]
    fn what_the_start_names_says_what_it_says() {
        let mut store = crate::graph::fixtures::store();
        // A gap answered and waiting for the change relying on it, and one still open.
        let mut answered = crate::graph::fixtures::ticket("answered", TicketKind::Gap, Priority::Next, Status::Landing, &[], &[]);
        answered.decision = Some(Decision::new("2026-10-04", "Fix it so."));
        store.tickets.insert(answered.slug.clone(), answered);
        store.tickets.insert("unanswered".into(), crate::graph::fixtures::ticket("unanswered", TicketKind::Gap, Priority::Next, Status::Open, &[], &[]));
        let t = crate::graph::fixtures::ticket(
            "work",
            TicketKind::Change,
            Priority::Next,
            Status::Open,
            &["req:ready-only-with-commit", "page:git-readiness", "module:src/a/ready"],
            &["ticket:answered", "ticket:unanswered", "ticket:old"],
        );
        store.tickets.insert(t.slug.clone(), t);
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
        // The answer a change applies, though not yet in force; nothing for an open gap or a closed ticket, whose decision is listed already.
        assert_eq!(says(&Reference::ticket("answered")), Some("Answered 2026-10-04, waiting for the change that applies it: Fix it so.".to_string()));
        assert_eq!(r.iter().find(|x| x.r#ref == Reference::ticket("answered")).unwrap().decisions.len(), 0);
        assert_eq!((says(&Reference::ticket("unanswered")), says(&Reference::ticket("old"))), (None, None));
        // Further away, nothing: the module imported by one named directly.
        assert_eq!(says(&Reference::module("src/a/layout")), None);
        // Named directly but saying nothing of its own: a criterion, whose statement is its title, and a node that does not exist.
        let mut named = store.clone();
        let t = crate::graph::fixtures::ticket("odd", TicketKind::Change, Priority::Next, Status::Open, &["criterion:src/a/ready#guarantee", "page:nowhere"], &[]);
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
    /// {"test": "a-landing-ticket-still-reaches-what-it-touches", "checks": ["crates/core/src/query/walks#rests-on-me"]}
    /// ```
    #[test]
    fn a_landing_ticket_still_reaches_what_it_touches() {
        let mut store = crate::graph::fixtures::store();
        store.tickets.insert("waits".into(), crate::graph::fixtures::ticket("waits", TicketKind::Change, Priority::Next, Status::Landing, &["page:orphan"], &[]));
        store.tickets.insert("done".into(), crate::graph::fixtures::ticket("done", TicketKind::Change, Priority::Next, Status::Closed, &["page:orphan"], &[]));
        let g = Graph::build(&store, &crate::extraction::fixtures::extraction(), &[]);
        let reaching: Vec<String> = rests_on_me(&g, &Reference::page("orphan")).tickets.iter().map(|t| t.r#ref.to_string()).collect();
        assert!(reaching.contains(&"ticket:waits".to_string()), "{reaching:?}");
        assert!(!reaching.contains(&"ticket:done".to_string()), "a closed ticket no longer reaches it: {reaching:?}");
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
}
