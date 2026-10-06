//! # Module contract
//!
//! ```json
//! {"purpose":"Tell a checkout's own work from other work: which findings its check fails on, and which it shows as advisories naming the work that may conflict with it.","criteria":[{"id":"own-work","statement":"own takes out of a checkout's findings each that is new, that the truth alone also produces against the base's code, and that is on no record the current ticket proposes (its pages, requirements and deletions, its own ticket and the tickets its proposal edits), and gives it back as an advisory from the main line since the base; every other finding stays, a baselined one inherited as before, and with no base every finding stays."},{"id":"other-tickets","statement":"others gives, for each ticket other than the current one that is active or landing, in slug order, an advisory naming the ticket for each finding the truth with that ticket's proposal produces against this checkout's code that the truth alone does not and that is not among the checkout's own findings already."}]}
//! ```

use std::collections::HashSet;

use serde::Serialize;

use crate::lint::Finding;
use crate::model::{FindingKey, Reference};
use crate::store::Store;

/// The work an advisory comes from: not this checkout's.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum OtherWork {
    /// Intent that landed on the main line since this checkout's base: merging the main line mends it.
    SinceBase,
    /// A ticket active or landing elsewhere: what its proposal would bring here when it lands.
    Ticket { ticket: String, title: String },
}

/// A finding other work causes, shown beside a check's findings and never failing it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
pub struct Advisory {
    pub from: OtherWork,
    pub finding: Finding,
}

/// The records `ticket`'s work is answerable for: the ticket itself, what its proposal holds or
/// deletes, and the tickets whose references it edits.
fn proposed(store: &Store, ticket: Option<&str>) -> HashSet<Reference> {
    let Some(ticket) = ticket else { return HashSet::new() };
    let mut out = HashSet::from([Reference::ticket(ticket)]);
    if let Some(p) = store.proposals.get(ticket) {
        out.extend(p.pages.keys().map(|s| Reference::page(s)));
        out.extend(p.requirements.keys().map(|s| Reference::req(s)));
        out.extend(p.deleted.keys().filter_map(|r| r.parse().ok()));
        out.extend(p.tickets.keys().map(|s| Reference::ticket(s)));
    }
    out
}

/// Split a checkout's `findings`, judged with `ticket`, into its own and the main line's, given
/// what the truth alone produces against the base's code: a new finding the base's code also
/// shows is not this checkout's doing, unless it is on a record its ticket proposes.
pub fn own(findings: Vec<Finding>, base: Option<&[Finding]>, store: &Store, ticket: Option<&str>) -> (Vec<Finding>, Vec<Advisory>) {
    let Some(base) = base else { return (findings, vec![]) };
    let at_base: HashSet<FindingKey> = base.iter().map(Finding::key).collect();
    let mine = proposed(store, ticket);
    let (mut kept, mut advised) = (Vec::new(), Vec::new());
    for f in findings {
        if !f.baselined && at_base.contains(&f.key()) && !mine.contains(&f.r#ref) {
            advised.push(Advisory { from: OtherWork::SinceBase, finding: f });
        } else {
            kept.push(f);
        }
    }
    (kept, advised)
}

/// What each other ticket active or landing would bring to this checkout's code when it lands:
/// the findings `judge` gives for the truth with that ticket's proposal, less those the truth
/// alone gives (`truth`) and those already among the checkout's own (`mine`).
pub fn others(store: &Store, ticket: Option<&str>, truth: &[Finding], mine: &[Finding], mut judge: impl FnMut(&str) -> Vec<Finding>) -> Vec<Advisory> {
    let seen: HashSet<FindingKey> = truth.iter().chain(mine).map(Finding::key).collect();
    let mut out = Vec::new();
    for t in store.tickets.values().filter(|t| t.status.is_live() && Some(t.slug.as_str()) != ticket) {
        for f in judge(&t.slug) {
            if !seen.contains(&f.key()) {
                out.push(Advisory { from: OtherWork::Ticket { ticket: t.slug.clone(), title: t.title.clone() }, finding: f });
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::graph::Graph;
    use crate::lint::{lint, Rule};
    use crate::model::*;

    fn checks(fs: &[Finding]) -> Vec<String> {
        fs.iter().map(|f| format!("{} {}", f.r#ref, f.edge.as_ref().map(|e| e.to.to_string()).unwrap_or_default())).collect()
    }

    /// # Test contract
    ///
    /// ```json
    /// {"test": "a-check-fails-only-on-its-own-work", "checks": ["crates/core/src/advisories#own-work"]}
    /// ```
    #[test]
    fn a_check_fails_only_on_its_own_work() {
        let ext = crate::extraction::fixtures::extraction();
        let mut s = crate::graph::fixtures::store();
        // Landed since the base: the truth names a criterion neither this code nor the base's declares.
        s.requirements.get_mut("ready-only-with-commit").unwrap().covered_by.push("criterion:src/a/ready#landed".parse().unwrap());
        // The branch's own proposal names one no code declares yet.
        s.tickets.get_mut("later-1").unwrap().status = Status::Active;
        let mut proposed_req = s.requirements["ready-only-with-commit"].clone();
        proposed_req.slug = "mine".into();
        proposed_req.covered_by = vec!["criterion:src/a/ready#not-yet".parse().unwrap()];
        let mut p = Proposal::new("later-1");
        p.requirements.insert("mine".into(), Proposed { record: proposed_req, base: None });
        s.proposals.insert("later-1".into(), p);
        let findings = lint(&Graph::with_proposal(&s, "later-1", &ext, &[]), &[Rule::Dangling]);
        // The base's code is this checkout's code here: the truth alone against it.
        let base = lint(&Graph::build(&s, &ext, &[]), &[Rule::Dangling]);
        let (kept, advised) = own(findings.clone(), Some(&base), &s, Some("later-1"));
        assert_eq!(checks(&kept), vec!["req:mine criterion:src/a/ready#not-yet"], "the branch's own proposal still fails");
        assert_eq!(
            checks(&advised.iter().map(|a| a.finding.clone()).collect::<Vec<_>>()),
            vec!["req:ready-only-with-commit criterion:src/a/ready#landed", "req:stale criterion:src/a/ready#gone"],
            "landed since the base, or already so at the base"
        );
        assert!(advised.iter().all(|a| a.from == OtherWork::SinceBase));
        // A finding the base's code does not show is the branch's own: here, its code lost a criterion.
        let (kept, advised) = own(findings.clone(), Some(&[]), &s, Some("later-1"));
        assert_eq!(kept.len(), findings.len());
        assert!(advised.is_empty());
        // With no base, nothing moves.
        assert_eq!(own(findings.clone(), None, &s, Some("later-1")).0, findings);
    }

    /// # Test contract
    ///
    /// ```json
    /// {"test": "other-tickets-say-what-they-would-bring", "checks": ["crates/core/src/advisories#other-tickets"]}
    /// ```
    #[test]
    fn other_tickets_say_what_they_would_bring() {
        let ext = crate::extraction::fixtures::extraction();
        let mut s = crate::graph::fixtures::store();
        for slug in ["later-1", "phase-62"] {
            s.tickets.get_mut(slug).unwrap().status = Status::Active;
        }
        // phase-62's proposal repoints a requirement at a criterion this checkout's code lacks.
        let mut repointed = s.requirements["ready-only-with-commit"].clone();
        repointed.covered_by = vec!["criterion:src/a/ready#elsewhere".parse().unwrap()];
        let mut p = Proposal::new("phase-62");
        p.requirements.insert("ready-only-with-commit".into(), Proposed { record: repointed, base: None });
        s.proposals.insert("phase-62".into(), p);
        let rules = [Rule::Dangling];
        let truth = lint(&Graph::build(&s, &ext, &[]), &rules);
        let judge = |t: &str| lint(&Graph::with_proposal(&s, t, &ext, &[]), &rules);
        let advised = others(&s, Some("later-1"), &truth, &[], judge);
        assert_eq!(checks(&advised.iter().map(|a| a.finding.clone()).collect::<Vec<_>>()), vec!["req:ready-only-with-commit criterion:src/a/ready#elsewhere"]);
        assert_eq!(advised[0].from, OtherWork::Ticket { ticket: "phase-62".into(), title: s.tickets["phase-62"].title.clone() });
        // The current ticket is not other work, and what the checkout already reports is not repeated.
        assert!(others(&s, Some("phase-62"), &truth, &[], |t| lint(&Graph::with_proposal(&s, t, &ext, &[]), &rules)).is_empty());
        let mine: Vec<Finding> = advised.iter().map(|a| a.finding.clone()).collect();
        assert!(others(&s, Some("later-1"), &truth, &mine, |t| lint(&Graph::with_proposal(&s, t, &ext, &[]), &rules)).is_empty());
    }
}
