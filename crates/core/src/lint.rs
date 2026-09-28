//! # Module contract
//!
//! ```json
//! {"purpose":"Report dangling edges, uncovered nodes, malformed store files and requirements without a scenario over the joined graph, each marked as baselined or new.","criteria":[{"id":"stable-order","statement":"Findings are ordered by rule, then reference, then edge, so two runs over the same graph produce identical output."},{"id":"closed-exempt","statement":"A closed ticket's edges are never reported as dangling."},{"id":"grimoire-material-only","statement":"A module is reported as unlinked only when it is expected to carry a contract or carries one; test files and build scripts are not part of the grimoire."},{"id":"unspecified","statement":"A requirement with no scenario is reported as unspecified, a fourth rule ordered after malformed."},{"id":"unsupported-language","statement":"A configured language the running build has no adapter for is a malformed finding with the check unsupported-language on file:grimoire.json, naming the languages the build supports."},{"id":"untested","statement":"A criterion that no test checks is uncovered with the check untested."},{"id":"malformed-test","statement":"A test block that could not be read is malformed with the check malformed-test, on the file that holds it, with the line and reason."},{"id":"baselined","statement":"Every finding names the check that produced it, and is reported and marked baselined exactly when its rule, check, ref and edge match an entry in the store's baseline; the reason text is not compared."},{"id":"baseline-pruned","statement":"The baseline a store should hold after a rebuild is the stored one minus every entry no longer produced; nothing is added."},{"id":"unheld","statement":"A requirement the Constraints page links that names no criterion and that no test checks is uncovered with the check unheld."}]}
//! ```

use std::collections::HashSet;

use serde::{Deserialize, Serialize};

use crate::graph::{Edge, Graph, Node};
use crate::model::*;

// The lint rules and a finding's edge are stored in a baseline, so they are
// model types; re-exported here, where callers look for them.
pub use crate::model::{FindingEdge, Rule};

/// The story page whose requirements are the constraints: construction rules
/// held by tests that name them, not by a module's criteria.
pub const CONSTRAINTS_PAGE: &str = "constraints";

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Finding {
    pub rule: Rule,
    /// Which test within the rule produced it, stable across wording
    /// changes: one node can fail two checks of the same rule.
    pub check: String,
    /// The node the finding is about. For a malformed store file, a
    /// reference of the kind the file would have held, with the file's slug.
    pub r#ref: Reference,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub edge: Option<FindingEdge>,
    pub reason: String,
    /// Whether the store's baseline holds this finding: inherited, not new.
    pub baselined: bool,
}

impl Finding {
    fn node(rule: Rule, check: &str, r: &Reference, reason: impl Into<String>) -> Finding {
        Finding { rule, check: check.to_string(), r#ref: r.clone(), edge: None, reason: reason.into(), baselined: false }
    }
    fn dangling(e: &Edge) -> Finding {
        Finding {
            rule: Rule::Dangling,
            check: "dangling-edge".to_string(),
            r#ref: e.from.clone(),
            edge: Some(FindingEdge { from: e.from.clone(), to: e.to.clone(), relation: e.relation }),
            reason: format!("{} {} {}, which does not exist", e.from, e.relation, e.to),
            baselined: false,
        }
    }
    /// What matches this finding against a baseline entry.
    pub fn key(&self) -> FindingKey {
        FindingKey { rule: self.rule, check: self.check.clone(), r#ref: self.r#ref.clone(), edge: self.edge.clone() }
    }
    fn sort_key(&self) -> (Rule, String, String, String) {
        (self.rule, self.r#ref.to_string(), self.edge.as_ref().map(|e| format!("{}{}", e.relation, e.to)).unwrap_or_default(), self.check.clone())
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
            out.push(Finding::node(Rule::Malformed, "malformed-file", &r, format!("{}: {}", m.path, m.reason)));
        }
        for e in &g.test_errors {
            out.push(Finding::node(Rule::Malformed, "malformed-test", &Reference::file(&e.file), format!("line {}: {}", e.line, e.reason)));
        }
        for l in &g.unsupported_languages {
            let reason = format!("language `{l}` is not supported by this build, which supports {}", g.supported_languages.join(", "));
            out.push(Finding::node(Rule::Malformed, "unsupported-language", &Reference::file("grimoire.json"), reason));
        }
    }
    if rules.contains(&Rule::Unspecified) {
        for (r, node) in &g.nodes {
            if let Some(f) = unspecified(r, node) {
                out.push(f);
            }
        }
    }
    out.sort_by_key(Finding::sort_key);
    out.dedup();
    mark(g, &mut out);
    out
}

fn mark(g: &Graph, findings: &mut [Finding]) {
    for f in findings {
        f.baselined = g.baseline.contains(&f.key());
    }
}

/// How many findings are not in the baseline: what the check fails on.
pub fn new_count(findings: &[Finding]) -> usize {
    findings.iter().filter(|f| !f.baselined).count()
}

/// The baseline the store should hold now: the stored one without the
/// entries no longer produced, in their stored order. `None` when nothing
/// was fixed, so the caller writes only when it shrank.
pub fn pruned(g: &Graph, baseline: Option<&Baseline>) -> Option<Baseline> {
    let b = baseline?;
    let present: HashSet<FindingKey> = lint(g, &Rule::ALL).iter().map(Finding::key).collect();
    let kept: Vec<FindingKey> = b.findings.iter().filter(|k| present.contains(k)).cloned().collect();
    (kept.len() < b.findings.len()).then(|| Baseline { findings: kept, ..b.clone() })
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

/// The findings a write created on the node it wrote: its dangling edges and,
/// for a requirement, a missing scenario. What a write returns as `warnings`.
pub fn written(g: &Graph, r: &Reference) -> Vec<Finding> {
    let mut out = dangling_from(g, r);
    if let Some(f) = g.get(r).and_then(|node| unspecified(r, node)) {
        out.push(f);
    }
    out.sort_by_key(Finding::sort_key);
    mark(g, &mut out);
    out
}

fn unspecified(r: &Reference, node: &Node) -> Option<Finding> {
    match node {
        Node::Req(q) if q.scenarios.is_empty() => Some(Finding::node(Rule::Unspecified, "no-scenario", r, "no scenario checks this requirement's criteria")),
        _ => None,
    }
}

/// Dangling findings for the edges one node owns.
pub fn dangling_from(g: &Graph, r: &Reference) -> Vec<Finding> {
    let exempt = matches!(g.get(r), Some(Node::Ticket(t)) if t.status == Status::Closed);
    if exempt {
        return Vec::new();
    }
    let mut out: Vec<Finding> = g.out(r).filter(|e| !g.contains(&e.to)).map(Finding::dangling).collect();
    out.sort_by_key(Finding::sort_key);
    mark(g, &mut out);
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
                    out.push(Finding::node(Rule::Uncovered, "uncovered-criterion", r, "no requirement names this criterion"));
                }
                if g.inbound_with(r, Relation::Checks).next().is_none() {
                    out.push(Finding::node(Rule::Uncovered, "untested", r, "no test checks this criterion"));
                }
            }
            Node::Module(m) => {
                // Only modules that are part of the grimoire: those expected to
                // carry a contract or that carry one anyway. Test files, build
                // scripts and examples are code, not part of the grimoire.
                let manual_material = m.expects_contract || m.purpose.is_some();
                if manual_material && g.inbound_with(r, Relation::Links).next().is_none() {
                    out.push(Finding::node(Rule::Uncovered, "unlinked-module", r, "no page links to this module"));
                }
                if m.expects_contract && m.purpose.is_none() {
                    let why = m.contract_error.clone().unwrap_or_else(|| "no contract".into());
                    out.push(Finding::node(Rule::Uncovered, "no-contract", r, format!("module has no usable contract: {why}")));
                }
            }
            Node::Page(p) => {
                if !g.is_root(&p.slug) && g.inbound_with(r, Relation::Links).next().is_none() {
                    out.push(Finding::node(Rule::Uncovered, "unlinked-page", r, "no page links to this page and it is not a root"));
                }
            }
            Node::Req(q) => {
                if g.inbound_with(r, Relation::Links).next().is_none() {
                    out.push(Finding::node(Rule::Uncovered, "unlinked-requirement", r, "no page links to this requirement"));
                }
                let constraint = g.inbound_with(r, Relation::Links).any(|e| e.from == Reference::page(CONSTRAINTS_PAGE));
                if constraint && q.covered_by.is_empty() && g.inbound_with(r, Relation::Checks).next().is_none() {
                    out.push(Finding::node(Rule::Uncovered, "unheld", r, "a constraint no test holds"));
                }
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

    /// # Test contract
    ///
    /// ```json
    /// {"test": "dangling-reports-open-not-closed", "checks": ["crates/core/src/lint#closed-exempt"]}
    /// ```
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

    /// # Test contract
    ///
    /// ```json
    /// {"test": "findings-are-stable", "checks": ["crates/core/src/lint#stable-order", "crates/core/src/lint#grimoire-material-only"]}
    /// ```
    #[test]
    fn test_files_are_not_grimoire() {
        // A test file's module: no contract expected, none carried, linked by no page.
        let mut files = crate::extraction::fixtures::extraction_files();
        let mut t = crate::extraction::fixtures::file("tests/it.rs", "tests/it", &[], None);
        t.expects_contract = false;
        files.push(t);
        let g = Graph::build(&crate::graph::fixtures::store(), &crate::extraction::Extraction::from_files(&files), &[]);
        let r = refs(&lint(&g, &[Rule::Uncovered]));
        assert!(!r.iter().any(|x| x.ends_with("module:tests/it")), "{r:?}");
        assert!(r.iter().any(|x| x.ends_with("module:src/b/unrelated")), "a module expected to carry a contract still is");
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
        // reads-only is named by no requirement and checked by no test: two findings.
        assert_eq!(r, vec!["Dangling req:stale".to_string(), "Uncovered criterion:src/a/ready#reads-only".to_string(), "Uncovered criterion:src/a/ready#reads-only".to_string()]);
    }

    /// # Test contract
    ///
    /// ```json
    /// {"test": "unspecified-reports-requirements-without-scenarios", "checks": ["crates/core/src/lint#unspecified"]}
    /// ```
    #[test]
    fn unspecified_reports_requirements_without_scenarios() {
        let g = graph();
        let f = lint(&g, &[Rule::Unspecified]);
        assert_eq!(refs(&f), vec!["Unspecified req:stale".to_string()]);
        assert_eq!(f[0].reason, "no scenario checks this requirement's criteria");
    }

    /// # Test contract
    ///
    /// ```json
    /// {"test": "unspecified-sorts-last", "checks": ["crates/core/src/lint#unspecified"]}
    /// ```
    #[test]
    fn unspecified_sorts_last() {
        let g = graph();
        let f = lint(&g, &Rule::ALL);
        assert_eq!(f.last().map(|f| f.rule), Some(Rule::Unspecified));
    }

    #[test]
    fn written_adds_unspecified_for_a_requirement() {
        let g = graph();
        let w = refs(&written(&g, &Reference::req("stale")));
        assert_eq!(w, vec!["Dangling req:stale".to_string(), "Unspecified req:stale".to_string()]);
        assert!(written(&g, &Reference::req("ready-only-with-commit")).is_empty());
    }

    fn baselined(keys: &[FindingKey]) -> Graph {
        let mut s = crate::graph::fixtures::store();
        s.baseline = Some(Baseline { date: "2026-09-28".into(), reason: "adopted".into(), findings: keys.to_vec() });
        Graph::build(&s, &crate::extraction::fixtures::extraction(), &[])
    }

    /// # Test contract
    ///
    /// ```json
    /// {"test": "baselined-findings-are-marked-not-hidden", "checks": ["crates/core/src/lint#baselined"]}
    /// ```
    #[test]
    fn baselined_findings_are_marked_not_hidden() {
        let key = FindingKey { rule: Rule::Unspecified, check: "no-scenario".into(), r#ref: Reference::req("stale"), edge: None };
        let g = baselined(std::slice::from_ref(&key));
        let all = lint(&g, &Rule::ALL);
        let stale = all.iter().find(|f| f.key() == key).expect("still reported");
        assert!(stale.baselined);
        assert_eq!(new_count(&all), all.len() - 1);
    }

    /// # Test contract
    ///
    /// ```json
    /// {"test": "matching-ignores-the-reason-but-not-the-edge", "checks": ["crates/core/src/lint#baselined"]}
    /// ```
    #[test]
    fn matching_ignores_the_reason_but_not_the_edge() {
        let dangling = lint(&graph(), &[Rule::Dangling]).remove(0);
        let g = baselined(&[dangling.key()]);
        assert!(lint(&g, &[Rule::Dangling])[0].baselined, "same rule, ref and edge");
        let mut other_edge = dangling.key();
        other_edge.edge.as_mut().expect("dangling has an edge").to = Reference::criterion("src/a/ready", "elsewhere");
        let g = baselined(&[other_edge]);
        assert!(!lint(&g, &[Rule::Dangling])[0].baselined, "a different edge is a different finding");
    }

    /// # Test contract
    ///
    /// ```json
    /// {"test": "two-checks-on-one-node-are-two-findings", "checks": ["crates/core/src/lint#baselined"]}
    /// ```
    #[test]
    fn two_checks_on_one_node_are_two_findings() {
        // `unrelated` is both unlinked and without a contract.
        let all = lint(&graph(), &[Rule::Uncovered]);
        let unrelated: Vec<FindingKey> = all.iter().filter(|f| f.r#ref == Reference::module("src/b/unrelated")).map(Finding::key).collect();
        assert_eq!(unrelated.len(), 2);
        assert_ne!(unrelated[0], unrelated[1]);
        // Baselining one leaves the other new.
        let g = baselined(&unrelated[..1]);
        let marked: Vec<bool> = lint(&g, &[Rule::Uncovered]).iter().filter(|f| f.r#ref == Reference::module("src/b/unrelated")).map(|f| f.baselined).collect();
        assert_eq!(marked.iter().filter(|b| **b).count(), 1);
    }

    /// # Test contract
    ///
    /// ```json
    /// {"test": "pruning-drops-only-what-is-fixed", "checks": ["crates/core/src/lint#baseline-pruned"]}
    /// ```
    #[test]
    fn pruning_drops_only_what_is_fixed() {
        let present = FindingKey { rule: Rule::Unspecified, check: "no-scenario".into(), r#ref: Reference::req("stale"), edge: None };
        let fixed = FindingKey { rule: Rule::Unspecified, check: "no-scenario".into(), r#ref: Reference::req("long-gone"), edge: None };
        let g = baselined(&[present.clone(), fixed]);
        let b = Baseline { date: "2026-09-28".into(), reason: "adopted".into(), findings: g.baseline.iter().cloned().collect() };
        let pruned = pruned(&g, Some(&b)).expect("one was fixed");
        assert_eq!(pruned.findings, vec![present.clone()]);
        assert_eq!(pruned.reason, "adopted");
        let unchanged = Baseline { findings: vec![present], ..b };
        assert!(super::pruned(&g, Some(&unchanged)).is_none(), "nothing fixed, nothing to write");
        assert!(super::pruned(&g, None).is_none());
    }

    /// # Test contract
    ///
    /// ```json
    /// {"test": "scope-narrows-what-must-carry-a-contract", "checks": ["crates/core/src/graph#contract-scope"]}
    /// ```
    #[test]
    fn scope_narrows_what_must_carry_a_contract() {
        let mut s = crate::graph::fixtures::store();
        s.config.scope = Scope { include: vec!["src/a".into()], exclude: vec![] };
        let g = Graph::build(&s, &crate::extraction::fixtures::extraction(), &[]);
        let r = refs(&lint(&g, &[Rule::Uncovered]));
        assert!(!r.iter().any(|x| x.ends_with("module:src/b/unrelated")), "out of scope, no contract, not reported: {r:?}");
        assert!(r.contains(&"Uncovered module:src/a/layout".to_string()), "a contracted module is still grimoire: {r:?}");
    }

    /// # Test contract
    ///
    /// ```json
    /// {"test": "unsupported-languages-are-malformed", "checks": ["crates/core/src/lint#unsupported-language"]}
    /// ```
    #[test]
    fn unsupported_languages_are_malformed() {
        let mut g = graph();
        g.unsupported_languages = vec!["typescript".into()];
        g.supported_languages = vec!["rust".into()];
        let f = lint(&g, &[Rule::Malformed]);
        assert_eq!(f.len(), 1);
        assert_eq!(f[0].check, "unsupported-language");
        assert_eq!(f[0].r#ref, Reference::file("grimoire.json"));
        assert_eq!(f[0].reason, "language `typescript` is not supported by this build, which supports rust");
    }

    /// # Test contract
    ///
    /// ```json
    /// {"test": "untested-criteria-are-uncovered", "checks": ["crates/core/src/lint#untested"]}
    /// ```
    #[test]
    fn untested_criteria_are_uncovered() {
        let f = lint(&graph(), &[Rule::Uncovered]);
        let untested: Vec<String> = f.iter().filter(|f| f.check == "untested").map(|f| f.r#ref.to_string()).collect();
        assert!(!untested.contains(&"criterion:src/a/ready#guarantee".to_string()), "a test checks it");
        assert!(untested.contains(&"criterion:src/a/ready#reads-only".to_string()));
        assert!(untested.contains(&"criterion:src/a/layout#stable".to_string()));
    }

    /// # Test contract
    ///
    /// ```json
    /// {"test": "unreadable-test-blocks-are-malformed", "checks": ["crates/core/src/lint#malformed-test"]}
    /// ```
    #[test]
    fn unreadable_test_blocks_are_malformed() {
        let mut g = graph();
        g.test_errors = vec![TestError { file: "src/a/ready.rs".into(), line: 30, reason: "test name `Bad` is not a slug".into() }];
        let f = lint(&g, &[Rule::Malformed]);
        assert_eq!(f[0].check, "malformed-test");
        assert_eq!(f[0].r#ref, Reference::file("src/a/ready.rs"));
        assert_eq!(f[0].reason, "line 30: test name `Bad` is not a slug");
    }

    /// # Test contract
    ///
    /// ```json
    /// {"test": "a-constraint-no-test-holds", "checks": ["crates/core/src/lint#unheld"]}
    /// ```
    #[test]
    fn a_constraint_no_test_holds_is_unheld() {
        let mut s = crate::graph::fixtures::store();
        let constraint = crate::graph::fixtures::req("no-io", &[], &[]);
        let in_progress = crate::graph::fixtures::req("not-yet", &[], &[]);
        s.requirements.insert("no-io".into(), constraint);
        s.requirements.insert("not-yet".into(), in_progress);
        s.pages.insert(CONSTRAINTS_PAGE.into(), crate::graph::fixtures::page(CONSTRAINTS_PAGE, &["req:no-io"]));
        s.pages.get_mut("overview").unwrap().links.push(Reference::page(CONSTRAINTS_PAGE));
        s.pages.get_mut("git-readiness").unwrap().links.push(Reference::req("not-yet"));
        let unheld = |x: &crate::extraction::Extraction| {
            let g = Graph::build(&s, x, &[]);
            lint(&g, &[Rule::Uncovered]).into_iter().filter(|f| f.check == "unheld").map(|f| f.r#ref.to_string()).collect::<Vec<_>>()
        };
        assert_eq!(unheld(&crate::extraction::fixtures::extraction()), vec!["req:no-io"], "a requirement elsewhere with no criterion is work in progress, not a constraint");
        // A test naming the requirement holds it.
        let mut files = crate::extraction::fixtures::extraction_files();
        files[0].tests.push(Test { file: "src/a/ready.rs".into(), name: "does-no-io".into(), line: 40, checks: vec!["req:no-io".into()] });
        assert!(unheld(&crate::extraction::Extraction::from_files(&files)).is_empty());
    }

    #[test]
    fn malformed_refs() {
        assert_eq!(malformed_ref("pages/x.json"), Reference::page("x"));
        assert_eq!(malformed_ref("grimoire.json"), Reference::file("grimoire.json"));
    }
}
