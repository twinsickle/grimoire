//! # Module contract
//!
//! ```json
//! {"purpose":"Report dangling edges, uncovered nodes, malformed store files and requirements without a scenario over the joined graph, each marked as baselined or new.","criteria":[{"id":"dangling-edge","statement":"An edge from a page, a requirement, a ticket that is not closed or a test to a node the graph does not hold is reported as dangling with the check dangling-edge, on the node that owns the edge, naming the edge and the missing node."},{"id":"uncovered-criterion","statement":"A criterion that no requirement in force names in covered_by is uncovered with the check uncovered-criterion, whether or not a test checks it; a retired requirement names nothing."},{"id":"no-contract","statement":"A module expected to carry a contract that carries no usable one, none at all or one that could not be read, is uncovered with the check no-contract, giving the reason; a module outside the contract scope is not."},{"id":"unlinked-page","statement":"A page that no page links and that is not a root is uncovered with the check unlinked-page."},{"id":"unlinked-requirement","statement":"A requirement that no page links is uncovered with the check unlinked-requirement, unless it is retired."},{"id":"stable-order","statement":"Findings are ordered by rule, then reference, then edge, so two runs over the same graph produce identical output."},{"id":"closed-exempt","statement":"A closed ticket's edges are never reported as dangling."},{"id":"grimoire-material-only","statement":"A module is reported as unlinked only when it is expected to carry a contract or carries one; test files and build scripts are not part of the grimoire."},{"id":"unspecified","statement":"A requirement with no scenario is reported as unspecified, a fourth rule ordered after malformed."},{"id":"unsupported-language","statement":"A configured language the running build has no adapter for is a malformed finding with the check unsupported-language on file:grimoire.json, naming the languages the build supports."},{"id":"untested","statement":"A criterion that no test checks is uncovered with the check untested."},{"id":"malformed-test","statement":"A test block that could not be read is malformed with the check malformed-test, on the file that holds it, with the line and reason."},{"id":"baselined","statement":"Every finding names the check that produced it, and is reported and marked baselined exactly when its rule, check, ref and edge match an entry in the store's baseline; the reason text is not compared."},{"id":"baseline-pruned","statement":"The baseline a store should hold after a rebuild is the stored one minus every entry no longer produced; nothing is added."},{"id":"pruned-to","statement":"pruned_to gives the stored baseline without the entries a given set of present findings lacks, in their stored order, and nothing when none go or there is no baseline; a present finding the baseline does not hold is never added."},{"id":"unheld","statement":"A requirement that names no criterion and that no test checks by name is uncovered with the check unheld, wherever its page sits, unless it is a rite, held by the work; a retired requirement is not, and one naming a criterion the graph lacks is dangling instead."},{"id":"ticket-cycle","statement":"A ticket that is not closed whose relies_on leads back to itself through other tickets that are not closed is malformed with the check ticket-cycle, naming the loop."},{"id":"without-code","statement":"On a graph built with no code, an edge to a module, criterion, file, declaration or test is not reported as dangling and no requirement is reported as unheld, since neither can be known there; every finding that intent alone shows is reported as usual."},{"id":"landing-moved","statement":"A landing ticket whose proposal rests on a truth that has since moved is malformed with the check landing-moved, on the ticket, naming the records that moved; an active ticket is not reported."},{"id":"retired-exempt","statement":"A retired requirement is never reported: not as unlinked, unheld or unspecified, and its covered_by makes no edge to dangle; a test that still names it in checks is reported as dangling with the check checks-retired, since what it holds no longer holds."},{"id":"rites","statement":"A rite, a requirement under the rites and not under the grimoire, is never reported as unheld, and one that names a criterion in covered_by is malformed with the check rite-names-code, since a rule the code carries belongs under the grimoire."},{"id":"missing-root","statement":"A root the config does not name, or names as a page the graph does not hold, is malformed on file:grimoire.json with the check missing-grimoire-root or missing-rites-root, so the two are told apart, saying which is missing and how to name it."}]}
//! ```

use std::collections::{BTreeMap, BTreeSet, HashSet};

use serde::{Deserialize, Serialize};

use crate::graph::{Edge, Graph, Node};
use crate::model::*;

// The lint rules and a finding's edge are stored in a baseline, so they are
// model types; re-exported here, where callers look for them.
pub use crate::model::{FindingEdge, Rule};

/// The story page whose requirements are the constraints: construction rules
/// held by tests that name them, not by a module's criteria.
pub const CONSTRAINTS_PAGE: &str = "constraints";

#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
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
    #[cfg_attr(feature = "ts", ts(optional))]
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
        for (r, members) in ticket_cycles(g) {
            out.push(cycle_finding(&r, &members));
        }
        for (ticket, moved) in &g.landing_moved {
            let reason = format!("waiting to land, but the truth moved under {}; read each again and propose it again under the ticket", moved.join(", "));
            out.push(Finding::node(Rule::Malformed, "landing-moved", &Reference::ticket(ticket), reason));
        }
        for e in &g.test_errors {
            out.push(Finding::node(Rule::Malformed, "malformed-test", &Reference::file(&e.file), format!("line {}: {}", e.line, e.reason)));
        }
        for l in &g.unsupported_languages {
            let reason = format!("language `{l}` is not supported by this build, which supports {}", g.supported_languages.join(", "));
            out.push(Finding::node(Rule::Malformed, "unsupported-language", &Reference::file("grimoire.json"), reason));
        }
        // A rite names no criterion: a rule the code carries belongs under the grimoire.
        for (r, n) in &g.nodes {
            if let Node::Req(q) = n {
                if !q.retired && g.is_rite(r) && !q.covered_by.is_empty() {
                    out.push(Finding::node(Rule::Malformed, "rite-names-code", r, "a requirement under the rites names a criterion: a rule the code carries belongs under the grimoire"));
                }
            }
        }
        // Every grimoire has two roots; one the config does not name, or names a page that is not there, is said.
        for (which, named, what) in [("grimoire", &g.named_roots.grimoire, "what the project is and must do"), ("rites", &g.named_roots.rites, "how its people work on it")] {
            let reason = match named {
                None => format!("no {which} root is named: put the page of {what} and name it with `configure {{\"roots\": {{\"{which}\": <slug>}}}}`"),
                Some(slug) if !g.contains(&Reference::page(slug)) => format!("the {which} root `{slug}` is not a page: put it, or name another with `configure`"),
                Some(_) => continue,
            };
            // A check for each root, so the baseline tells one missing root from the other.
            out.push(Finding::node(Rule::Malformed, &format!("missing-{which}-root"), &Reference::file("grimoire.json"), reason));
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

/// Count findings per rule, every rule present, for `lint`'s `counts`.
pub fn counts(findings: &[Finding]) -> BTreeMap<String, usize> {
    let mut m: BTreeMap<String, usize> = Rule::ALL.iter().map(|r| (r.name().to_string(), 0)).collect();
    for f in findings {
        *m.entry(f.rule.name().to_string()).or_insert(0) += 1;
    }
    m
}

/// The baseline the store should hold now: the stored one without the
/// entries no longer produced, in their stored order. `None` when nothing
/// was fixed, so the caller writes only when it shrank.
pub fn pruned(g: &Graph, baseline: Option<&Baseline>) -> Option<Baseline> {
    let present: HashSet<FindingKey> = lint(g, &Rule::ALL).iter().map(Finding::key).collect();
    pruned_to(baseline, &present)
}

/// The same for findings a caller saw, as the main line tells a store that
/// holds no code: the entries `present` lacks go, and nothing is added,
/// whatever `present` holds.
pub fn pruned_to(baseline: Option<&Baseline>, present: &HashSet<FindingKey>) -> Option<Baseline> {
    let b = baseline?;
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

/// For each ticket that is not closed in a loop of `relies_on` through other
/// tickets that are not closed, the loop's members in order. Shared by the lint and the
/// board, which breaks a loop by title.
pub fn ticket_cycles(g: &Graph) -> BTreeMap<Reference, Vec<Reference>> {
    let live: BTreeMap<Reference, &Ticket> = g.tickets().filter(|t| t.status != Status::Closed).map(|t| (Reference::ticket(&t.slug), t)).collect();
    let next = |r: &Reference| -> Vec<Reference> { live.get(r).map(|t| t.relies_on.iter().filter(|x| live.contains_key(*x)).cloned().collect()).unwrap_or_default() };
    let mut out = BTreeMap::new();
    for start in live.keys() {
        // Everything reachable from start; start is in a loop when it reaches itself.
        let mut seen: BTreeSet<Reference> = BTreeSet::new();
        let mut stack = next(start);
        while let Some(r) = stack.pop() {
            if seen.insert(r.clone()) {
                stack.extend(next(&r));
            }
        }
        if seen.contains(start) {
            let members: Vec<Reference> = seen.into_iter().filter(|r| {
                // A member of start's loop also reaches start.
                let mut s: BTreeSet<Reference> = BTreeSet::new();
                let mut st = next(r);
                while let Some(x) = st.pop() {
                    if s.insert(x.clone()) {
                        st.extend(next(&x));
                    }
                }
                s.contains(start)
            }).collect();
            out.insert(start.clone(), members);
        }
    }
    out
}

fn cycle_finding(r: &Reference, members: &[Reference]) -> Finding {
    let others: Vec<String> = members.iter().filter(|m| *m != r).map(|m| m.to_string()).collect();
    let reason = if others.is_empty() { "relies on itself".to_string() } else { format!("relies on itself through {}", others.join(", ")) };
    Finding::node(Rule::Malformed, "ticket-cycle", r, reason)
}

/// The findings a write created on the node it wrote: its dangling edges and,
/// for a requirement, a missing scenario. What a write returns as `warnings`.
pub fn written(g: &Graph, r: &Reference) -> Vec<Finding> {
    let mut out = dangling_from(g, r);
    if let Some(f) = g.get(r).and_then(|node| unspecified(r, node)) {
        out.push(f);
    }
    if let Some(members) = ticket_cycles(g).get(r) {
        out.push(cycle_finding(r, members));
    }
    out.sort_by_key(Finding::sort_key);
    mark(g, &mut out);
    out
}

fn unspecified(r: &Reference, node: &Node) -> Option<Finding> {
    match node {
        Node::Req(q) if q.scenarios.is_empty() && !q.retired => Some(Finding::node(Rule::Unspecified, "no-scenario", r, "no scenario checks this requirement's criteria")),
        _ => None,
    }
}

/// Dangling findings for the edges one node owns.
pub fn dangling_from(g: &Graph, r: &Reference) -> Vec<Finding> {
    let exempt = matches!(g.get(r), Some(Node::Ticket(t)) if t.status == Status::Closed);
    if exempt {
        return Vec::new();
    }
    let mut out: Vec<Finding> = g.out(r).filter(|e| !g.contains(&e.to) && !unknowable(g, &e.to)).map(Finding::dangling).collect();
    out.sort_by_key(Finding::sort_key);
    mark(g, &mut out);
    out
}

/// Whether `to` cannot be judged on this graph: a node that comes from the
/// code, on a graph built with no code. It may well exist; nothing here can say.
fn unknowable(g: &Graph, to: &Reference) -> bool {
    g.without_code && to.kind.is_derived()
}

fn dangling(g: &Graph, out: &mut Vec<Finding>) {
    for e in g.edges.iter().filter(|e| e.relation == Relation::Checks) {
        if let Some(Node::Req(q)) = g.get(&e.to) {
            if q.retired {
                out.push(Finding { check: "checks-retired".to_string(), reason: format!("{} checks {}, which is retired: it no longer holds", e.from, e.to), ..Finding::dangling(e) });
            }
        }
    }
    for e in &g.edges {
        if g.contains(&e.to) || unknowable(g, &e.to) {
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
            // A retired requirement no longer holds; nothing about it is work.
            Node::Req(q) if q.retired => {}
            Node::Req(q) => {
                if g.inbound_with(r, Relation::Links).next().is_none() {
                    out.push(Finding::node(Rule::Uncovered, "unlinked-requirement", r, "no page links to this requirement"));
                }
                // Intent connected to no code: no criterion named, no test naming it.
                // With no code held, whether a test names it cannot be known.
                // A rite is held by the work, which each ticket's closing decision shows, never by the code.
                if !g.is_rite(r) && !g.without_code && q.covered_by.is_empty() && g.inbound_with(r, Relation::Checks).next().is_none() {
                    out.push(Finding::node(Rule::Uncovered, "unheld", r, "no criterion names this requirement's code and no test holds it"));
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
    /// {"test": "dangling-reports-open-not-closed", "checks": ["crates/core/src/lint#dangling-edge", "crates/core/src/lint#closed-exempt"]}
    /// ```
    #[test]
    fn dangling_reports_open_not_closed() {
        let g = graph();
        let f = lint(&g, &[Rule::Dangling]);
        let r = refs(&f);
        assert_eq!(r, vec!["Dangling req:stale".to_string()]);
        // On the node that owns the edge, naming the edge and the missing node; the closed ticket's edge to a missing node is not there.
        let e = f[0].edge.as_ref().expect("a dangling finding carries its edge");
        assert_eq!((f[0].check.as_str(), e.from.to_string().as_str(), e.to.to_string().as_str()), ("dangling-edge", "req:stale", "criterion:src/a/ready#gone"));
        assert!(f[0].reason.contains("does not exist"), "{}", f[0].reason);
    }

    /// # Test contract
    ///
    /// ```json
    /// {"test": "uncovered-reports-each-rule", "checks": ["crates/core/src/lint#uncovered-criterion", "crates/core/src/lint#no-contract", "crates/core/src/lint#unlinked-page", "crates/core/src/lint#unlinked-requirement"]}
    /// ```
    #[test]
    fn uncovered_reports_each_rule() {
        // A requirement no page links, beside the fixture's.
        let mut s = crate::graph::fixtures::store();
        let mut loose = s.requirements["stale"].clone();
        loose.slug = "loose".into();
        s.requirements.insert("loose".into(), loose);
        let g = Graph::build(&s, &crate::extraction::fixtures::extraction(), &[]);
        let f = lint(&g, &[Rule::Uncovered]);
        let check = |check: &str, r: Reference| f.iter().find(|x| x.check == check && x.r#ref == r).map(|x| x.reason.clone());
        assert!(check("uncovered-criterion", Reference::criterion("src/a/ready", "reads-only")).is_some(), "named by no requirement");
        assert!(check("uncovered-criterion", Reference::criterion("src/a/ready", "guarantee")).is_none(), "named by one");
        let why = check("no-contract", Reference::module("src/b/unrelated"));
        assert_eq!(why.as_deref(), Some("module has no usable contract: no `# Module contract` block found"), "expected to carry one and carries none");
        assert!(check("no-contract", Reference::module("src/a/ready")).is_none(), "carries one");
        assert!(check("unlinked-page", Reference::page("orphan")).is_some());
        assert!(check("unlinked-page", Reference::page("overview")).is_none(), "a root");
        assert!(check("unlinked-requirement", Reference::req("loose")).is_some());
        assert!(check("unlinked-requirement", Reference::req("stale")).is_none(), "linked by a page");
        // unrelated is reported twice: unlinked and contractless.
        assert_eq!(refs(&f).iter().filter(|s| s.ends_with("module:src/b/unrelated")).count(), 2);
    }

    /// # Test contract
    ///
    /// ```json
    /// {"test": "test-files-are-not-grimoire", "checks": ["crates/core/src/lint#grimoire-material-only"]}
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

    /// # Test contract
    ///
    /// ```json
    /// {"test": "findings-are-stable", "checks": ["crates/core/src/lint#stable-order"]}
    /// ```
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
    /// {"test": "pruning-to-what-was-seen-never-adds", "checks": ["crates/core/src/lint#pruned-to"]}
    /// ```
    #[test]
    fn pruning_to_what_was_seen_never_adds() {
        let key = |id: &str| FindingKey { rule: Rule::Unspecified, check: "no-scenario".into(), r#ref: Reference::req(id), edge: None };
        let b = Baseline { date: "2026-10-03".into(), reason: "inherited".into(), findings: vec![key("c"), key("a"), key("b")] };
        // What the main line saw: two of the three, and one the baseline never held.
        let seen: HashSet<FindingKey> = [key("b"), key("c"), key("new")].into_iter().collect();
        let pruned = pruned_to(Some(&b), &seen).expect("one went");
        assert_eq!(pruned.findings, vec![key("c"), key("b")], "the stored order, and nothing added");
        assert_eq!((pruned.date.as_str(), pruned.reason.as_str()), ("2026-10-03", "inherited"));
        let all: HashSet<FindingKey> = b.findings.iter().cloned().collect();
        assert!(pruned_to(Some(&b), &all).is_none(), "none went, nothing to write");
        assert!(pruned_to(None, &seen).is_none());
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
    /// {"test": "a-rite-is-held-by-the-work", "checks": ["crates/core/src/lint#rites", "crates/core/src/lint#missing-root", "crates/core/src/graph#rites"]}
    /// ```
    #[test]
    fn a_rite_is_held_by_the_work() {
        use crate::graph::fixtures::{page, req, store};
        let mut s = store();
        // Under the rites: a page with a rule of work, a rule that names code, and one the grimoire links too.
        s.pages.get_mut("rites").unwrap().links.push(Reference::page("review"));
        s.pages.insert("review".into(), page("review", &["req:reviewed", "req:misplaced", "req:shared"]));
        s.pages.get_mut("git-readiness").unwrap().links.push(Reference::req("shared"));
        for r in [req("reviewed", &[], &[]), req("misplaced", &["criterion:src/a/ready#guarantee"], &[]), req("shared", &[], &[])] {
            s.requirements.insert(r.slug.clone(), r);
        }
        let x = crate::extraction::fixtures::extraction();
        let found = |s: &crate::store::Store| {
            let g = Graph::build(s, &x, &[]);
            lint(&g, &Rule::ALL).into_iter().map(|f| format!("{} {}", f.check, f.r#ref)).collect::<Vec<_>>()
        };
        let g = Graph::build(&s, &x, &[]);
        assert!(g.is_rite(&Reference::page("review")) && g.is_rite(&Reference::req("reviewed")) && g.is_rite(&Reference::page("rites")));
        assert!(!g.is_rite(&Reference::req("shared")) && !g.is_rite(&Reference::page("git-readiness")), "under both, it is the code's");
        let all = found(&s);
        assert!(!all.contains(&"unheld req:reviewed".to_string()), "a rite is held by the work: {all:?}");
        assert!(all.contains(&"unheld req:shared".to_string()), "a rule the grimoire links too must be held: {all:?}");
        assert!(all.contains(&"rite-names-code req:misplaced".to_string()), "{all:?}");
        // It is malformed, asked for as malformed and not as uncovered.
        let kinds = |rules: &[Rule]| lint(&g, rules).into_iter().any(|f| f.check == "rite-names-code");
        assert!(kinds(&[Rule::Malformed]) && !kinds(&[Rule::Uncovered]));
        assert!(!all.iter().any(|f| f.starts_with("missing-")), "both roots are there: {all:?}");
        // A root not named, or naming no page, is said, each apart.
        s.config.roots.rites = None;
        assert!(found(&s).contains(&"missing-rites-root file:grimoire.json".to_string()));
        s.config.roots = Roots::new("nowhere", "rites");
        let all = found(&s);
        assert!(all.contains(&"missing-grimoire-root file:grimoire.json".to_string()) && !all.iter().any(|f| f.starts_with("missing-rites")), "{all:?}");
    }

    /// # Test contract
    ///
    /// ```json
    /// {"test": "a-requirement-nothing-holds", "checks": ["crates/core/src/lint#unheld"]}
    /// ```
    #[test]
    fn a_requirement_nothing_holds_is_unheld() {
        let mut s = crate::graph::fixtures::store();
        // A construction rule on the Constraints page, a requirement on an area page, one whose criterion the code lacks yet, and a retired one.
        s.requirements.insert("no-io".into(), crate::graph::fixtures::req("no-io", &[], &[]));
        s.requirements.insert("loose".into(), crate::graph::fixtures::req("loose", &[], &[]));
        s.requirements.insert("not-yet".into(), crate::graph::fixtures::req("not-yet", &["criterion:src/a/ready#coming"], &[]));
        let mut gone = crate::graph::fixtures::req("gone", &[], &[]);
        gone.retired = true;
        s.requirements.insert("gone".into(), gone);
        s.pages.insert(CONSTRAINTS_PAGE.into(), crate::graph::fixtures::page(CONSTRAINTS_PAGE, &["req:no-io"]));
        s.pages.get_mut("overview").unwrap().links.push(Reference::page(CONSTRAINTS_PAGE));
        s.pages.get_mut("git-readiness").unwrap().links.extend([Reference::req("loose"), Reference::req("not-yet"), Reference::req("gone")]);
        let by_check = |x: &crate::extraction::Extraction, check: &str| {
            let g = Graph::build(&s, x, &[]);
            lint(&g, &Rule::ALL).into_iter().filter(|f| f.check == check).map(|f| f.r#ref.to_string()).collect::<Vec<_>>()
        };
        let x = crate::extraction::fixtures::extraction();
        assert_eq!(by_check(&x, "unheld"), vec!["req:loose", "req:no-io"], "wherever the page sits; the one naming a criterion and the retired one are not unheld");
        assert_eq!(by_check(&x, "dangling-edge"), vec!["req:not-yet", "req:stale"], "spec before code dangles instead");
        // A test naming a requirement holds it.
        let mut files = crate::extraction::fixtures::extraction_files();
        files[0].tests.push(Test { file: "src/a/ready.rs".into(), name: "does-no-io".into(), line: 40, checks: vec!["req:no-io".into(), "req:loose".into()] });
        assert!(by_check(&crate::extraction::Extraction::from_files(&files), "unheld").is_empty());
    }

    /// # Test contract
    ///
    /// ```json
    /// {"test": "a-loop-of-relies-on-is-malformed", "checks": ["crates/core/src/lint#ticket-cycle"]}
    /// ```
    #[test]
    fn a_loop_of_relies_on_is_malformed() {
        use crate::graph::fixtures::ticket;
        let loop_of = |b_status: Status| {
            let mut s = crate::graph::fixtures::store();
            s.tickets.insert("a".into(), ticket("a", TicketKind::Change, Priority::High, Status::Open, &["ticket:b"], &["ticket:b"]));
            s.tickets.insert("b".into(), ticket("b", TicketKind::Change, Priority::High, b_status, &["ticket:a"], &["ticket:a"]));
            // touches alone never makes a loop.
            s.tickets.insert("c".into(), ticket("c", TicketKind::Change, Priority::High, Status::Open, &["ticket:d"], &[]));
            s.tickets.insert("d".into(), ticket("d", TicketKind::Change, Priority::High, Status::Open, &["ticket:c"], &[]));
            Graph::build(&s, &crate::extraction::fixtures::extraction(), &[])
        };
        let g = loop_of(Status::Open);
        let f: Vec<Finding> = lint(&g, &[Rule::Malformed]).into_iter().filter(|f| f.check == "ticket-cycle").collect();
        assert_eq!(f.iter().map(|f| f.r#ref.id.as_str()).collect::<Vec<_>>(), vec!["a", "b"]);
        assert_eq!(f[0].reason, "relies on itself through ticket:b");
        assert!(written(&g, &Reference::ticket("b")).iter().any(|f| f.check == "ticket-cycle"), "the write that closed the loop hears of it");
        let closed = loop_of(Status::Closed);
        assert!(lint(&closed, &[Rule::Malformed]).iter().all(|f| f.check != "ticket-cycle"), "a closed ticket's decision is settled");
    }

    #[test]
    fn malformed_refs() {
        assert_eq!(malformed_ref("pages/x.json"), Reference::page("x"));
        assert_eq!(malformed_ref("grimoire.json"), Reference::file("grimoire.json"));
    }

    /// # Test contract
    ///
    /// ```json
    /// {"test": "a-retired-requirement-covers-nothing-and-is-not-reported", "checks": ["crates/core/src/lint#retired-exempt", "crates/core/src/graph#retired-covers-nothing", "crates/core/src/query/outline#outline"]}
    /// ```
    #[test]
    fn a_retired_requirement_covers_nothing_and_is_not_reported() {
        let mut s = crate::graph::fixtures::store();
        // ready-only-with-commit names src/a/ready#guarantee; stale is unspecified and dangling.
        for slug in ["ready-only-with-commit", "stale"] {
            let q = s.requirements.get_mut(slug).expect("fixture");
            q.retired = true;
        }
        s.pages.get_mut("git-readiness").expect("fixture").links.retain(|l| l != &Reference::req("ready-only-with-commit"));
        let g = Graph::build(&s, &crate::extraction::fixtures::extraction(), &[]);
        let guarantee = Reference::criterion("src/a/ready", "guarantee");
        assert!(g.inbound_with(&guarantee, Relation::CoveredBy).next().is_none(), "no covered_by edge from a retired requirement");
        let f = lint(&g, &Rule::ALL);
        assert!(f.iter().any(|x| x.r#ref == guarantee && x.check == "uncovered-criterion"), "its criterion is reported uncovered");
        for slug in ["ready-only-with-commit", "stale"] {
            let r = Reference::req(slug);
            assert!(!f.iter().any(|x| x.r#ref == r || x.edge.as_ref().is_some_and(|e| e.from == r)), "{slug} is never reported: {f:?}");
        }
        // Were a page to link one anyway, the outline counts it toward nothing.
        s.pages.get_mut("git-readiness").expect("fixture").links.push(Reference::req("ready-only-with-commit"));
        let g = Graph::build(&s, &crate::extraction::fixtures::extraction(), &[]);
        let o = crate::query::outline(&g, 4, Some(&Reference::page("git-readiness")), false);
        let node = o.roots[0].children.iter().find(|c| c.r#ref == Reference::req("ready-only-with-commit")).expect("linked");
        assert!(node.children.is_empty() && node.rollup.requirements == 0, "no children and no count: {:?}", node.rollup);
    }

    /// # Test contract
    ///
    /// ```json
    /// {"test": "a-landing-ticket-the-truth-moved-under-is-reported", "checks": ["crates/core/src/lint#landing-moved"]}
    /// ```
    #[test]
    fn a_landing_ticket_the_truth_moved_under_is_reported() {
        use crate::model::{Decision, Page, Proposal, Proposed};
        let extraction = crate::extraction::fixtures::extraction();
        let mut s = crate::graph::fixtures::store();
        let page = s.pages["orphan"].clone();
        // later-1 proposes a change to the orphan page, written against the truth as it then was.
        let proposed = Proposed { record: Page { body: "proposed".into(), ..page.clone() }, base: Some(crate::fingerprint::of_record(&page)) };
        s.proposals.insert("later-1".into(), Proposal { ticket: "later-1".into(), pages: [("orphan".to_string(), proposed)].into(), requirements: Default::default(), deleted: Default::default() });
        let found = |s: &crate::store::Store| -> Vec<Finding> { lint(&Graph::build(s, &extraction, &[]), &[Rule::Malformed]).into_iter().filter(|f| f.check == "landing-moved").collect() };
        // The truth moves; while the ticket is active nothing is reported, since its close will say so.
        s.pages.get_mut("orphan").unwrap().body = "moved".into();
        assert!(found(&s).is_empty(), "an active ticket is told at its close");
        // Landing, it is reported: on the ticket, naming the record.
        let t = s.tickets.get_mut("later-1").unwrap();
        t.status = Status::Landing;
        t.decision = Some(Decision::new("2026-10-03", "Done."));
        let f = found(&s);
        assert_eq!(f.len(), 1);
        assert_eq!(f[0].r#ref, Reference::ticket("later-1"));
        assert!(f[0].reason.contains("page:orphan"), "{}", f[0].reason);
        // Seen through the ticket's own proposal too, which is how the check before its commit sees it.
        let own = Graph::with_proposal(&s, "later-1", &extraction, &[]);
        assert!(lint(&own, &[Rule::Malformed]).iter().any(|f| f.check == "landing-moved"));
        // With the truth as the proposal was written against, there is nothing to report.
        s.pages.get_mut("orphan").unwrap().body = page.body.clone();
        assert!(found(&s).is_empty());
    }

    /// # Test contract
    ///
    /// ```json
    /// {"test": "with-no-code-only-what-intent-shows-is-reported", "checks": ["crates/core/src/lint#without-code"]}
    /// ```
    #[test]
    fn with_no_code_only_what_intent_shows_is_reported() {
        use crate::extraction::Extraction;
        let files: Vec<crate::extraction::FileExtraction> = Vec::new();
        // The fixture's store, with a constraint a test would hold, over no extraction at all, as a daemon holding no code builds it.
        let mut store = crate::graph::fixtures::store();
        store.requirements.insert("no-io".into(), crate::graph::fixtures::req("no-io", &[], &[]));
        let mut g = Graph::build(&store, &Extraction::from_files(&files), &[]);
        let all = |g: &Graph| -> Vec<String> { lint(g, &Rule::ALL).iter().map(|f| format!("{} {}", f.check, f.edge.as_ref().map_or(f.r#ref.to_string(), |e| e.to.to_string()))).collect() };
        // Judged as if the code were simply absent, every edge into it dangles and constraints look unheld.
        let before = all(&g);
        assert!(before.iter().any(|f| f.starts_with("dangling-edge criterion:") || f.starts_with("dangling-edge module:")), "{before:?}");
        assert!(before.iter().any(|f| f.starts_with("unheld ")), "{before:?}");
        // Told it holds no code, none of that is said; what intent alone shows still is.
        g.without_code = true;
        let after = all(&g);
        assert!(!after.iter().any(|f| f.starts_with("dangling-edge criterion:") || f.starts_with("dangling-edge module:") || f.starts_with("unheld ")), "{after:?}");
        let intent_only: Vec<&String> = before.iter().filter(|f| !(f.starts_with("dangling-edge criterion:") || f.starts_with("dangling-edge module:") || f.starts_with("dangling-edge test:") || f.starts_with("dangling-edge file:") || f.starts_with("dangling-edge decl:") || f.starts_with("unheld "))).collect();
        assert_eq!(after.iter().collect::<Vec<_>>(), intent_only, "nothing else is left out");
        assert!(dangling_from(&g, &Reference::req("ready-only-with-commit")).is_empty(), "a write's warnings leave it out too");
    }
}
