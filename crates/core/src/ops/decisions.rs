//! # Module contract
//!
//! ```json
//! {"purpose":"Keep a requirement's decisions: append one, link the earlier decisions one supersedes, and retire a requirement that no longer holds, removing and rewriting nothing.","criteria":[{"id":"decisions-append-only","statement":"No write removes a decision or changes its text, date or id: put_requirement preserves the existing decisions, record_decision appends, and supersede only adds to a decision's supersedes list."},{"id":"record-decision","statement":"record_decision appends a dated decision to an existing requirement, defaulting the date to today, with the next id, the ids of the earlier decisions it supersedes and whether it is an adoption, the mark set only here; an id naming no landed decision the requirement holds, or a retired requirement, is invalid, and a missing requirement not_found."},{"id":"supersede","statement":"supersede adds to the landed decision of a requirement with a given id the ids of earlier decisions it supersedes, keeping those it had and every text, date and id; an id naming no landed decision below it, or a retired requirement, is invalid, and a missing requirement or decision, one not yet landed among them, not_found."},{"id":"retire","statement":"retire_node records on a requirement a last decision superseding every earlier landed one, marks it retired and removes it from every page's links, persisting the pages first and the requirement last so a failure part way leaves it live and a retry completes; a ticket, an already retired requirement or any other kind is invalid, a missing requirement not_found, and a retired requirement refuses put_requirement, record_decision and supersede."}]}
//! ```

use super::{invalid_if, Changed, ToolError};
use crate::model::*;
use crate::store::{validate_requirement, FieldError, Store};

/// The requirement `r` names, refused when it is not one or is retired.
fn live<'a>(store: &'a Store, r: &Reference) -> Result<&'a Requirement, ToolError> {
    if r.kind != Kind::Req {
        return Err(ToolError::invalid(format!("{r} is not a requirement"), vec![]));
    }
    let req = store.requirements.get(&r.id).ok_or_else(|| ToolError::not_found(r))?;
    if req.retired {
        return Err(ToolError::invalid(format!("{r} is retired: it no longer holds and takes no more decisions"), vec![]));
    }
    Ok(req)
}

/// Validate `req` and put it back.
fn store_it(store: &mut Store, req: Requirement) -> Result<(Requirement, Changed), ToolError> {
    invalid_if(validate_requirement(&req), "requirement")?;
    store.requirements.insert(req.slug.clone(), req.clone());
    Ok((req.clone(), Changed::Requirement(req.slug)))
}

/// The id a decision appended now takes: the next, unless one before it waits in a proposal, read
/// through it, when it waits too and takes its id as it lands.
fn next(req: &Requirement) -> Option<u32> {
    req.decisions.iter().all(|d| d.id.is_some()).then(|| req.next_id())
}

/// Refused unless each of `ids` names a landed decision of `req` below `below`.
fn earlier(req: &Requirement, r: &Reference, ids: &[u32], below: u32) -> Result<(), ToolError> {
    match ids.iter().find(|&&id| id >= below || req.decision(id).is_none()) {
        Some(id) => {
            Err(ToolError::invalid(format!("{r} has no decision d{id} earlier than this one"), vec![FieldError::new("supersedes", "names only earlier decisions the requirement holds, by id")]))
        }
        None => Ok(()),
    }
}

/// Append a decision with the next id. Under a ticket, the proposal keeps it apart with no id, and
/// it takes its id when the ticket lands (req:tool-record-decision).
pub fn record_decision(store: &mut Store, r: &Reference, text: String, date: String, supersedes: Vec<u32>, adopted: bool) -> Result<(Requirement, Changed), ToolError> {
    let mut req = live(store, r)?.clone();
    earlier(&req, r, &supersedes, req.next_id())?;
    req.decisions.push(Decision { id: next(&req), supersedes, adopted, ..Decision::new(date, text) });
    store_it(store, req)
}

/// Add to the landed decision `decision`, by id, the earlier decisions it
/// supersedes: how a supersession written in prose is linked.
pub fn supersede(store: &mut Store, r: &Reference, decision: u32, supersedes: &[u32]) -> Result<(Requirement, Changed), ToolError> {
    let mut req = live(store, r)?.clone();
    if req.decision(decision).is_none() {
        return Err(ToolError::NotFound { message: format!("{r} has no decision d{decision}: a decision not yet landed has no id") });
    }
    earlier(&req, r, supersedes, decision)?;
    let d = req.decisions.iter_mut().find(|d| d.id == Some(decision)).expect("found above");
    for &p in supersedes {
        if !d.supersedes.contains(&p) {
            d.supersedes.push(p);
        }
    }
    d.supersedes.sort_unstable();
    store_it(store, req)
}

/// Retire a requirement that no longer holds: a last decision saying why,
/// superseding every earlier one; marked retired; unlinked from every page.
pub fn retire_node(store: &mut Store, r: &Reference, text: String, date: String) -> Result<(Requirement, Changed), ToolError> {
    if r.kind == Kind::Ticket {
        return Err(ToolError::invalid(format!("{r} is a ticket: a ticket retires by closing"), vec![]));
    }
    let mut req = live(store, r)?.clone();
    // Every landed decision; under a ticket, the landing supersedes every one the truth holds then (req:tool-retire-node).
    let earlier = req.decisions.iter().filter_map(|d| d.id).collect();
    req.decisions.push(Decision { id: next(&req), supersedes: earlier, ..Decision::new(date, text) });
    req.retired = true;
    invalid_if(validate_requirement(&req), "requirement")?;
    // The pages first and the requirement last: if a write fails part way,
    // the requirement is still live, and retiring it again completes.
    let mut changed = Vec::new();
    for page in store.pages.values_mut().filter(|p| p.links.contains(r)) {
        page.links.retain(|l| l != r);
        changed.push(Changed::Page(page.slug.clone()));
    }
    changed.push(Changed::Requirement(req.slug.clone()));
    store.requirements.insert(req.slug.clone(), req.clone());
    Ok((req, Changed::Many(changed)))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::graph::fixtures::store;
    use crate::ops::put_requirement;

    /// # Test contract
    ///
    /// ```json
    /// {"test": "record-decision-appends-and-validates", "checks": ["crates/core/src/ops#validate-first", "crates/core/src/ops/decisions#decisions-append-only", "crates/core/src/ops/decisions#record-decision"]}
    /// ```
    #[test]
    fn record_decision_appends_and_validates() {
        let mut s = store();
        let r = Reference::req("ready-only-with-commit");
        let (out, _) = record_decision(&mut s, &r, "Because.".into(), "2026-09-27".into(), vec![], false).unwrap();
        assert_eq!(out.decisions.len(), 2);
        let e = record_decision(&mut s, &r, "x".into(), "not-a-date".into(), vec![], false).unwrap_err();
        assert_eq!(e.code(), "invalid");
        assert_eq!(s.requirements["ready-only-with-commit"].decisions.len(), 2);
        assert_eq!(record_decision(&mut s, &Reference::req("nope"), "x".into(), "2026-01-01".into(), vec![], false).unwrap_err().code(), "not_found");
        assert_eq!(out.decisions[1].id, Some(2), "the next id");
        // A new decision supersedes earlier ones by id, never itself, a later one or one the requirement does not hold.
        let (out, _) = record_decision(&mut s, &r, "Instead.".into(), "2026-09-28".into(), vec![1], false).unwrap();
        assert_eq!((out.decisions[2].id, out.decisions[2].supersedes.clone()), (Some(3), vec![1]));
        assert_eq!(out.in_force().iter().map(|d| d.id).collect::<Vec<_>>(), vec![Some(2), Some(3)]);
        assert_eq!(record_decision(&mut s, &r, "x".into(), "2026-09-28".into(), vec![4], false).unwrap_err().code(), "invalid");
        assert_eq!(s.requirements["ready-only-with-commit"].decisions.len(), 3, "nothing written");
        // Ids are not positions: with d2 gone, the next is still above every id held, and d2 names nothing.
        let mut gap = s.requirements["ready-only-with-commit"].clone();
        gap.decisions.remove(1);
        s.requirements.insert(gap.slug.clone(), gap);
        assert_eq!(record_decision(&mut s, &r, "x".into(), "2026-09-29".into(), vec![2], false).unwrap_err().code(), "invalid", "a decision that went missing is named by nothing");
        let (out, _) = record_decision(&mut s, &r, "After.".into(), "2026-09-29".into(), vec![3], false).unwrap();
        assert_eq!(out.decisions.iter().map(|d| d.id).collect::<Vec<_>>(), vec![Some(1), Some(3), Some(4)]);
        // A decision dated before the last still lands after it: the order is the order they landed.
        let (out, _) = record_decision(&mut s, &r, "Earlier date.".into(), "2026-01-01".into(), vec![], false).unwrap();
        assert_eq!(out.decisions.last().map(|d| d.id), Some(Some(5)));
    }

    /// # Test contract
    ///
    /// ```json
    /// {"test": "supersede-links-without-rewriting", "checks": ["crates/core/src/ops/decisions#supersede", "crates/core/src/ops/decisions#decisions-append-only"]}
    /// ```
    #[test]
    fn supersede_links_without_rewriting() {
        let mut s = store();
        let r = Reference::req("ready-only-with-commit");
        record_decision(&mut s, &r, "Then this.".into(), "2026-09-28".into(), vec![], false).unwrap();
        let before = s.requirements["ready-only-with-commit"].decisions.clone();
        let (out, _) = supersede(&mut s, &r, 2, &[1]).unwrap();
        assert_eq!(out.decisions[1].supersedes, vec![1]);
        let texts = |d: &[Decision]| d.iter().map(|x| (x.date.clone(), x.text.clone())).collect::<Vec<_>>();
        assert_eq!(texts(&out.decisions), texts(&before), "no text or date changes");
        assert_eq!(supersede(&mut s, &r, 2, &[1]).unwrap().0.decisions[1].supersedes, vec![1], "linking again changes nothing");
        assert_eq!(supersede(&mut s, &r, 1, &[2]).unwrap_err().code(), "invalid", "only earlier decisions");
        assert_eq!(supersede(&mut s, &r, 2, &[7]).unwrap_err().code(), "invalid", "only decisions it holds");
        assert_eq!(supersede(&mut s, &r, 9, &[1]).unwrap_err().code(), "not_found");
        assert!(out.decisions.iter().zip(&before).all(|(a, b)| a.id == b.id), "no id changes");
    }

    /// # Test contract
    ///
    /// ```json
    /// {"test": "retire-supersedes-everything-and-unlinks", "checks": ["crates/core/src/ops/decisions#retire", "crates/core/src/ops#put-page", "crates/core/src/ops#patch-is-partial"]}
    /// ```
    #[test]
    fn retire_supersedes_everything_and_unlinks() {
        let mut s = store();
        let r = Reference::req("ready-only-with-commit");
        assert!(s.pages["git-readiness"].links.contains(&r));
        let (out, changed) = retire_node(&mut s, &r, "No longer holds.".into(), "2026-09-29".into()).unwrap();
        assert!(out.retired);
        assert_eq!(out.decisions.len(), 2);
        assert_eq!(out.decisions[1].supersedes, vec![1]);
        assert_eq!(out.in_force().len(), 1, "only the retirement is in force");
        assert!(!s.pages["git-readiness"].links.contains(&r), "no page links it");
        let Changed::Many(order) = changed else { panic!("a retirement is several changes") };
        assert_eq!(order.first(), Some(&Changed::Page("git-readiness".into())), "pages first");
        assert_eq!(order.last(), Some(&Changed::Requirement("ready-only-with-commit".into())), "the requirement last, so a partial write can be retried");
        // A retired requirement takes nothing more.
        assert_eq!(retire_node(&mut s, &r, "Again.".into(), "2026-09-29".into()).unwrap_err().code(), "invalid");
        assert_eq!(record_decision(&mut s, &r, "x".into(), "2026-09-29".into(), vec![], false).unwrap_err().code(), "invalid");
        assert_eq!(supersede(&mut s, &r, 2, &[1]).unwrap_err().code(), "invalid");
        assert_eq!(put_requirement(&mut s, out.clone()).unwrap_err().code(), "invalid");
        // No page links it again.
        let add = crate::ops::PagePatch { add_links: vec![r.clone()], ..Default::default() };
        assert_eq!(crate::ops::patch_page(&mut s, &Reference::page("git-readiness"), add).unwrap_err().code(), "invalid");
        let mut page = s.pages["git-readiness"].clone();
        page.links.push(r.clone());
        assert_eq!(crate::ops::put_page(&mut s, page).unwrap_err().code(), "invalid");
        assert!(!s.pages["git-readiness"].links.contains(&r));
        assert_eq!(retire_node(&mut s, &Reference::ticket("phase-62"), "x".into(), "2026-09-29".into()).unwrap_err().code(), "invalid");
        assert_eq!(retire_node(&mut s, &Reference::req("nope"), "x".into(), "2026-09-29".into()).unwrap_err().code(), "not_found");
    }

    /// # Test contract
    ///
    /// ```json
    /// {"test": "an-adoption-is-described-until-decided", "checks": ["crates/core/src/model#requirement-record", "crates/core/src/ops/decisions#record-decision", "crates/core/src/query/outline#outline"]}
    /// ```
    #[test]
    fn an_adoption_is_described_until_decided() {
        let mut s = store();
        let r = Reference::req("ready-only-with-commit");
        let q = |s: &Store| s.requirements["ready-only-with-commit"].clone();
        let described = |s: &Store| {
            let g = crate::graph::Graph::build(s, &crate::extraction::fixtures::extraction(), &[]);
            crate::query::outline(&g, 4, None, false).roots[0].rollup.described
        };
        assert!(!q(&s).described(), "its decision is a plain one");
        assert_eq!(described(&s), 0);
        // Restated as an adoption, superseding the text-only one: described, the old decision kept.
        let (out, _) = record_decision(&mut s, &r, "Read off the code at adoption.".into(), "2026-10-04".into(), vec![1], true).unwrap();
        assert!(out.decisions[1].adopted && !out.decisions[0].adopted && out.decisions.len() == 2);
        assert!(q(&s).described());
        assert_eq!(described(&s), 1, "the outline counts it");
        // The mark is stored with the decision and read back with it.
        let json = serde_json::to_value(&q(&s).decisions).unwrap();
        assert_eq!((json[0].get("adopted"), json[1]["adopted"].clone()), (None, serde_json::json!(true)));
        // A real decision clears it, and the adoption keeps its mark.
        record_decision(&mut s, &r, "Chosen.".into(), "2026-10-05".into(), vec![], false).unwrap();
        assert!(!q(&s).described() && q(&s).decisions[1].adopted);
        assert_eq!(described(&s), 0);
        // A requirement with no decision is not described.
        let mut none = q(&s);
        none.decisions.clear();
        assert!(!none.described());
    }
}
