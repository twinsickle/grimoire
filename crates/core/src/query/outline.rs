//! # Module contract
//!
//! ```json
//! {"purpose":"Answer outline: the grimoire as a tree from its roots, each node with its coverage, findings, tests and adoption rollup, and the modules no page links.","criteria":[{"id":"outline-tree","statement":"outline returns the grimoire as a tree from the root pages, the grimoire's and then the rites' (or a given page), to a depth."},{"id":"outline-counts","statement":"Each outline node carries its coverage, its count of lint findings on or below it and how many of those are baselined, its count of omens on or below it, a file's omen counted on its module, the checks of the findings on the node itself, and how many tests check it."},{"id":"outline-rollup","statement":"Each outline node carries its rollup over the whole subtree, each node counted once: the grimoire's modules and how many carry a contract, their criteria and how many are tested, the requirements, how many are held, how many are described and how many are decided, having a decision in force that is not an adoption, one with no decision being neither."},{"id":"outline-unplaced","statement":"outline lists apart the modules of the grimoire no page links."},{"id":"outline-retired","statement":"A retired requirement an outline reaches has no children and counts toward no rollup."},{"id":"outline-rites","statement":"Each outline node is marked rites when it is under the rites and not under the grimoire."},{"id":"pages-only","statement":"Asked for pages only, the trees hold pages alone, each page with the same findings, baselined count, omens, checks, coverage and rollup it has in the full outline, and the unplaced modules are unchanged."}]}
//! ```

use std::collections::{BTreeSet, HashMap, HashSet};

use serde::Serialize;
use serde_json::Value;

use crate::graph::{Graph, Node};
use crate::lint::{lint, omens, Rule};
use crate::model::*;

#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[derive(Debug, Clone, Serialize)]
pub struct Tree {
    pub r#ref: Reference,
    pub title: String,
    /// For a page, the fraction of criteria under it some requirement names;
    /// for a requirement, its criterion count; null for anything else.
    #[cfg_attr(feature = "ts", ts(type = "number | null"))]
    pub coverage: Value,
    pub findings: usize,
    /// How many of `findings` are in the baseline: inherited, not new.
    pub baselined: usize,
    // The omens on or below the node, a file's counted on its module: signs, never failures.
    pub omens: usize,
    /// The checks of the findings on this node itself, sorted and distinct,
    /// so a view can say what is wrong and not only how much.
    pub checks: Vec<String>,
    /// How many tests check this node: a criterion's evidence, or what holds
    /// a constraint that names no criterion.
    pub tests: usize,
    pub rollup: Rollup,
    // Under the rites and not under the grimoire: held by the work, so its rollup, which measures
    // the code, says nothing of it.
    pub rites: bool,
    pub children: Vec<Tree>,
}

/// How far adoption has come under a node, over its whole subtree with each
/// node counted once: the modules that are part of the grimoire and how many
/// carry a contract, those modules' criteria and how many a test checks, and
/// the requirements and how many name a criterion or are held by a test.
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize)]
pub struct Rollup {
    pub modules: usize,
    pub contracted: usize,
    pub criteria: usize,
    pub tested: usize,
    pub requirements: usize,
    pub held: usize,
    pub described: usize,
    pub decided: usize,
}

/// The outline's answer: the trees from the roots, and the modules of the
/// grimoire that no page links, which no tree reaches.
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[derive(Debug, Clone, Serialize)]
pub struct Outline {
    pub roots: Vec<Tree>,
    pub unplaced: Vec<Tree>,
    // True on a daemon that holds no code: the trees are the intent alone, and their counts say nothing of the code.
    pub without_code: bool,
}

/// The grimoire as trees from the roots (or `root`) to `depth`; with
/// `pages_only` the trees hold pages alone, their counts unchanged.
pub fn outline(g: &Graph, depth: usize, root: Option<&Reference>, pages_only: bool) -> Outline {
    let findings = lint(g, &Rule::ALL);
    let mut per_node: HashMap<&Reference, (usize, usize)> = HashMap::new();
    let mut checks: HashMap<&Reference, BTreeSet<&str>> = HashMap::new();
    for f in &findings {
        let e = per_node.entry(&f.r#ref).or_default();
        e.0 += 1;
        e.1 += usize::from(f.baselined);
        checks.entry(&f.r#ref).or_default().insert(f.check.as_str());
    }
    // An omen on a file is counted on its module, the node the trees hold.
    let mut omens_on: HashMap<Reference, (usize, usize)> = HashMap::new();
    for o in omens(g) {
        let on = match g.get(&o.r#ref) {
            Some(Node::File(f)) => Reference::module(&f.module),
            _ => o.r#ref.clone(),
        };
        omens_on.entry(on).or_default().0 += 1;
    }
    let roots: Vec<Reference> = match root {
        Some(r) => vec![r.clone()],
        None => g.roots.iter().map(|s| Reference::page(s)).collect(),
    };
    let on = Findings { count: per_node, checks, omens: omens_on.iter().map(|(r, n)| (r, *n)).collect() };
    let unplaced = g
        .nodes
        .iter()
        .filter(|(r, n)| matches!(n, Node::Module(m) if in_grimoire(m)) && g.inbound_with(r, Relation::Links).next().is_none())
        .map(|(r, _)| tree(g, r, 1, false, &on, &mut HashSet::new()))
        .collect();
    Outline { roots: roots.iter().filter(|r| g.contains(r)).map(|r| tree(g, r, depth, pages_only, &on, &mut HashSet::new())).collect(), unplaced, without_code: g.without_code }
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
    checks: HashMap<&'a Reference, BTreeSet<&'a str>>,
    // Omens per node, in the same shape as count so they sum the same way.
    omens: HashMap<&'a Reference, (usize, usize)>,
}

/// The tree at `r` to `depth`; with `pages_only` its children are its pages
/// alone, though every count still covers the whole subtree.
fn tree(g: &Graph, r: &Reference, depth: usize, pages_only: bool, on: &Findings, path: &mut HashSet<Reference>) -> Tree {
    let node = g.get(r);
    let title = node.map(Node::title).unwrap_or_else(|| crate::query::walks::absent_title(g, r));
    let mut children = Vec::new();
    if depth > 0 && path.insert(r.clone()) {
        for c in children_of(g, r).into_iter().filter(|c| !pages_only || c.kind == Kind::Page) {
            children.push(tree(g, &c, depth - 1, pages_only, on, path));
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
    let (omens, _) = findings_below(g, r, &on.omens, &mut HashSet::new());
    let checks = on.checks.get(r).map(|c| c.iter().map(|s| s.to_string()).collect()).unwrap_or_default();
    let tests = g.inbound_with(r, Relation::Checks).count();
    let rollup = rollup(g, r);
    Tree { r#ref: r.clone(), title, coverage, findings, baselined, omens, checks, tests, rollup, rites: g.is_rite(r), children }
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
            // A retired requirement no longer holds and counts toward nothing.
            Some(Node::Req(q)) if q.retired => {}
            Some(Node::Req(q)) => {
                out.requirements += 1;
                out.held += usize::from(!q.covered_by.is_empty() || g.inbound_with(&x, Relation::Checks).next().is_some());
                out.described += usize::from(q.described());
                out.decided += usize::from(!q.in_force().is_empty() && !q.described());
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
        Some(Node::Req(q)) if q.retired => Vec::new(),
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::graph::fixtures::graph;

    /// # Test contract
    ///
    /// ```json
    /// {"test": "outline-names-the-checks-on-each-node", "checks": ["crates/core/src/query/outline#outline-counts"]}
    /// ```
    #[test]
    fn outline_names_the_checks_on_each_node() {
        let g = graph();
        let t = outline(&g, 4, Some(&Reference::module("src/a/ready")), false).roots;
        let checks = |id: &str| t[0].children.iter().find(|c| c.r#ref == Reference::criterion("src/a/ready", id)).map(|c| c.checks.clone());
        assert_eq!(checks("reads-only"), Some(vec!["uncovered-criterion".to_string(), "untested".to_string()]));
        assert_eq!(checks("guarantee"), Some(vec![]), "named by a requirement and checked by a test");
        let tests = |id: &str| t[0].children.iter().find(|c| c.r#ref == Reference::criterion("src/a/ready", id)).map(|c| c.tests);
        assert_eq!((tests("guarantee"), tests("reads-only")), (Some(1), Some(0)), "outline counts the tests that check each node");
    }

    /// # Test contract
    ///
    /// ```json
    /// {"test": "outline-rolls-up-adoption", "checks": ["crates/core/src/query/outline#outline-rollup", "crates/core/src/query/outline#outline-unplaced"]}
    /// ```
    #[test]
    fn outline_rolls_up_adoption() {
        let mut s = crate::graph::fixtures::store();
        // Baseline the untested finding on reads-only: one of the page's findings is inherited.
        let plain = crate::graph::fixtures::graph();
        let inherited = lint(&plain, &Rule::ALL).into_iter().find(|f| f.check == "untested" && f.r#ref == Reference::criterion("src/a/ready", "reads-only")).expect("fixture has it");
        s.baseline = Some(Baseline { date: "2026-09-28".into(), reason: "Adopted.".into(), findings: vec![inherited.key()] });
        let g = Graph::build(&s, &crate::extraction::fixtures::extraction(), &[]);
        let expected = Rollup { modules: 1, contracted: 1, criteria: 2, tested: 1, requirements: 2, held: 2, described: 0, decided: 1 };
        for depth in [0, 1, 4] {
            let o = outline(&g, depth, None, false);
            assert_eq!(o.roots[0].rollup, expected, "the rollup is the whole subtree's at depth {depth}");
        }
        let o = outline(&g, 1, None, false);
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
    /// {"test": "a-page-linked-twice-counts-once", "checks": ["crates/core/src/query/outline#outline-rollup"]}
    /// ```
    #[test]
    fn a_page_linked_twice_counts_once() {
        let mut s = crate::graph::fixtures::store();
        s.pages.insert("again".into(), crate::graph::fixtures::page("again", &["page:git-readiness", "module:src/a/ready"]));
        s.pages.get_mut("overview").expect("fixture").links.push("page:again".parse().expect("reference"));
        let g = Graph::build(&s, &crate::extraction::fixtures::extraction(), &[]);
        let o = outline(&g, 2, None, false);
        assert_eq!(o.roots[0].rollup, Rollup { modules: 1, contracted: 1, criteria: 2, tested: 1, requirements: 2, held: 2, described: 0, decided: 1 });
    }

    /// # Test contract
    ///
    /// ```json
    /// {"test": "outline-from-roots", "checks": ["crates/core/src/query/outline#outline-tree", "crates/core/src/query/outline#outline-rites", "crates/core/src/query/outline#outline-counts"]}
    /// ```
    #[test]
    fn outline_from_roots() {
        let g = graph();
        let t = outline(&g, 3, None, false).roots;
        // The grimoire's root and then the rites', the rites' marked as held by the work.
        assert_eq!(t.iter().map(|x| x.r#ref.clone()).collect::<Vec<_>>(), vec![Reference::page("overview"), Reference::page("rites")]);
        assert!(!t[0].rites && t[1].rites);
        let gr = &t[0].children[0];
        assert_eq!(gr.r#ref, Reference::page("git-readiness"));
        // one of two criteria under module ready is covered.
        assert_eq!(gr.coverage, Value::from(0.5));
        assert!(gr.findings >= 1);
        let module = gr.children.iter().find(|c| c.r#ref.kind == Kind::Module).unwrap();
        assert_eq!(module.children.len(), 2);
        // A shallow outline still counts the findings deep below it.
        let shallow = outline(&g, 0, None, false).roots;
        assert_eq!(shallow[0].findings, t[0].findings);
        assert!(shallow[0].children.is_empty());
    }

    /// # Test contract
    ///
    /// ```json
    /// {"test": "omens-are-counted-apart-on-the-files-module", "checks": ["crates/core/src/query/outline#outline-counts", "crates/core/src/query/outline#pages-only"]}
    /// ```
    #[test]
    fn omens_are_counted_apart_on_the_files_module() {
        // Every fixture file has 40 lines: at a limit of 40, src/a/ready.rs, which git-readiness's module holds, is an omen.
        let mut s = crate::graph::fixtures::store();
        s.config.large_file = Some(40);
        let g = Graph::build(&s, &crate::extraction::fixtures::extraction(), &[]);
        let before = outline(&Graph::build(&crate::graph::fixtures::store(), &crate::extraction::fixtures::extraction(), &[]), 64, None, false);
        let o = outline(&g, 64, None, false);
        let overview = &o.roots[0];
        let page = &overview.children[0];
        let module = page.children.iter().find(|c| c.r#ref == Reference::module("src/a/ready")).expect("linked");
        assert_eq!((overview.omens, page.omens, module.omens), (1, 1, 1));
        assert_eq!((overview.findings, overview.baselined), (before.roots[0].findings, before.roots[0].baselined), "an omen is no finding");
        assert!(module.children.iter().all(|c| c.omens == 0), "on the module, not its criteria");
        assert_eq!(outline(&g, 64, None, true).roots[0].omens, 1, "the same with pages only");
        assert!(o.unplaced.iter().all(|u| u.omens == 1), "an unplaced module's file is counted on it too");
    }

    /// # Test contract
    ///
    /// ```json
    /// {"test": "pages-only-keeps-the-counts", "checks": ["crates/core/src/query/outline#pages-only"]}
    /// ```
    #[test]
    fn pages_only_keeps_the_counts() {
        // A page linked twice (from overview and from again), a cycle back to
        // overview, and a baselined finding, so inherited counts are not all zero.
        let mut s = crate::graph::fixtures::store();
        s.pages.insert("again".into(), crate::graph::fixtures::page("again", &["page:git-readiness", "page:overview", "module:src/a/ready"]));
        s.pages.get_mut("overview").expect("fixture").links.push("page:again".parse().expect("reference"));
        let plain = Graph::build(&s, &crate::extraction::fixtures::extraction(), &[]);
        let inherited = lint(&plain, &Rule::ALL).into_iter().find(|f| f.check == "untested" && f.r#ref == Reference::criterion("src/a/ready", "reads-only")).expect("fixture has it");
        s.baseline = Some(Baseline { date: "2026-10-01".into(), reason: "Adopted.".into(), findings: vec![inherited.key()] });
        let g = Graph::build(&s, &crate::extraction::fixtures::extraction(), &[]);
        // The full outline cut down to its pages, keeping the nesting.
        fn pruned(t: &Tree) -> serde_json::Value {
            let children: Vec<_> = t.children.iter().filter(|c| c.r#ref.kind == Kind::Page).map(pruned).collect();
            serde_json::json!([t.r#ref, t.title, t.coverage, t.findings, t.baselined, t.checks, t.tests, t.rollup, children])
        }
        fn kept(t: &Tree) -> serde_json::Value {
            assert_eq!(t.r#ref.kind, Kind::Page, "nothing but pages");
            let children: Vec<_> = t.children.iter().map(kept).collect();
            serde_json::json!([t.r#ref, t.title, t.coverage, t.findings, t.baselined, t.checks, t.tests, t.rollup, children])
        }
        let roots = std::iter::once(None).chain(s.pages.keys().map(|p| Some(Reference::page(p))));
        let mut baselined = 0;
        for root in roots {
            for depth in [0, 1, 2, 64] {
                let full = outline(&g, depth, root.as_ref(), false);
                let pages = outline(&g, depth, root.as_ref(), true);
                let expected: Vec<_> = full.roots.iter().map(pruned).collect();
                assert_eq!(pages.roots.iter().map(kept).collect::<Vec<_>>(), expected, "the full outline's pages, nested as there, from {root:?} at {depth}");
                assert_eq!(serde_json::to_value(&pages.unplaced).expect("json"), serde_json::to_value(&full.unplaced).expect("json"), "unplaced unchanged");
                baselined += pages.roots.iter().map(|t| t.baselined).sum::<usize>();
            }
        }
        assert!(baselined > 0, "the inherited counts compared are not all zero");
        let whole = outline(&g, 64, None, false);
        let deep = outline(&g, 64, None, true);
        fn has_other(t: &Tree) -> bool {
            t.r#ref.kind != Kind::Page || t.children.iter().any(has_other)
        }
        assert!(whole.roots.iter().any(has_other), "the fixture has more than pages beneath them");
        assert!(deep.roots[0].children.iter().any(|c| !c.children.is_empty()), "and pages nested more than one deep");
    }
}
