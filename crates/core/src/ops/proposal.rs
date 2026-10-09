//! # Module contract
//!
//! ```json
//! {"purpose":"Keep a write that changes intent in its ticket's proposal, leaving the truth alone, and apply the proposal to the truth when the ticket closes or is bottled, refusing what moved under it.","criteria":[{"id":"keeps-write-in-proposal","statement":"propose runs a write that changes intent on the store as the named brewing or simmering ticket's proposal would leave it and keeps what it changed in that proposal, each record with the fingerprint of the truth's record it rests on, so the truth is untouched."},{"id":"deletion-kept-as-mark","statement":"propose keeps a deletion in the proposal as a mark."},{"id":"ticket-references-kept-as-lists","statement":"propose keeps a ticket whose references a write changed in the proposal as that ticket's touches and relies_on with the fingerprint of the truth's two lists."},{"id":"neither-brewing-nor-simmering-conflict","statement":"propose on a ticket that is neither brewing nor simmering, or on a gap or question that is simmering, is a conflict."},{"id":"whole-write-rebases","statement":"A whole-record write through propose rests on the truth as it is now, rebasing a record the proposal already held."},{"id":"partial-write-keeps-base","statement":"A partial write through propose (a patch, a retirement, a deletion, one of a ticket's two lists) keeps the base the proposal held for the record."},{"id":"partial-write-refused-when-moved","statement":"A partial write through propose (a patch, a retirement, a deletion, one of a ticket's two lists) is refused as a conflict naming the record when the truth moved under it since it was proposed."},{"id":"decisions-left-out-of-record","statement":"A requirement's decisions are never part of the record propose keeps, whose fingerprint leaves them out."},{"id":"appended-decision-kept-apart","statement":"A decision a write appends to a requirement is kept apart as one the proposal adds, with no id."},{"id":"supersession-kept-as-link","statement":"A supersession a write links on a bottled decision is kept as a link by id."},{"id":"decision-never-refused-as-moved","statement":"A decision a write appends, and a supersession it links on a bottled decision, rest on no record and are never refused as moved."},{"id":"moved-names-differing-records","statement":"moved names every proposed record, and every ticket whose references the proposal edits, whose truth fingerprint differs from the base it was written against, a requirement's matched with or without its decisions, or that was proposed as new and now exists."},{"id":"moved-leaves-out-equal-records","statement":"moved never names a record or edit equal to the truth's, a requirement's decisions aside."},{"id":"moved-leaves-out-closed-edit","statement":"moved never names an edit to a ticket since closed or gone."},{"id":"apply-writes-differing-records","statement":"apply_proposal writes the proposed records that differ from the truth's."},{"id":"apply-decisions-after-truths","statement":"apply_proposal writes each requirement with the truth's decisions as they are then, the links it adds added to them and the decisions it adds after them, each taking the next id."},{"id":"apply-retirement-supersedes-all","statement":"apply_proposal gives a retirement's decision supersession of every decision before it."},{"id":"apply-removes-deleted","statement":"apply_proposal removes the deleted records."},{"id":"apply-deleted-page-leaves-site","statement":"apply_proposal leaves a deleted page out of the pages the site shows."},{"id":"apply-edited-tickets-take-references","statement":"apply_proposal gives each edited ticket that is not bottled the references its edit holds."},{"id":"apply-in-one-pass","statement":"apply_proposal does all of this in one pass."},{"id":"apply-drops-proposal","statement":"apply_proposal drops the proposal."}]}
//! ```

use super::*;

/// How a proposed write relates to the record it changes. A whole-record
/// write (a put) replaces a record the caller has just read, so it rests on
/// the truth as it is now, whatever the proposal held before. A partial write
/// (a patch, a decision, a supersession, a retirement, a deletion) builds on
/// the proposal's copy, which must still rest on the truth: if the truth
/// moved under it, the write would carry the old truth forward unseen.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Proposing {
    Whole,
    Partial,
}

/// Run `op`, a write that changes intent, as `ticket`'s proposal: it runs on
/// the store as the proposal would leave it, and what it changed is kept in
/// the proposal, each record with the fingerprint of the truth's record it
/// rests on. The truth is untouched. The ticket must be brewing or simmering, and a
/// partial write on a record whose truth moved since it was proposed is a
/// conflict naming the record.
pub fn propose<T>(store: &mut Store, ticket: &str, how: Proposing, op: impl FnOnce(&mut Store) -> Result<(T, Changed), ToolError>) -> Result<(T, Changed), ToolError> {
    match store.tickets.get(ticket) {
        None => return Err(ToolError::not_found(&Reference::ticket(ticket))),
        Some(t) if t.kind != TicketKind::Change && t.status == Status::Simmering => {
            return Err(ToolError::conflict(format!("ticket:{ticket} is an answer waiting for the change that applies it: its spec belongs in that change")))
        }
        Some(t) if !t.status.is_live() => return Err(ToolError::conflict(format!("ticket:{ticket} is {}, neither brewing nor simmering: a change belongs to a ticket being worked", t.status.name()))),
        _ => {}
    }
    let mut work = store.applied(ticket);
    let before = work.requirements.clone();
    let (out, changed) = op(&mut work)?;
    let mut proposal = store.proposals.get(ticket).cloned().unwrap_or_else(|| Proposal::new(ticket));
    if how == Proposing::Partial {
        // A requirement whose write only added decisions or links rests on no copy of a record, so nothing of the truth's can be carried forward.
        let decisions_only = |r: &Reference| r.kind == Kind::Req && matches!((before.get(&r.id), work.requirements.get(&r.id)), (Some(a), Some(b)) if a.without_decisions() == b.without_decisions());
        let stale: Vec<FieldError> = changed_refs(&changed)
            .into_iter()
            .filter(|r| !decisions_only(r))
            .filter(|r| record_moved(store, &proposal, r))
            .map(|r| FieldError::new(&r.to_string(), "changed in the truth since it was proposed; get it as the truth now has it and put it again under the ticket"))
            .collect();
        if !stale.is_empty() {
            return Err(ToolError::conflict_with("the truth moved under this proposal's record since it was proposed; get it as the truth now has it and put it again under the ticket", stale));
        }
    }
    absorb(&mut proposal, &work, &before, store, &changed, how);
    store.proposals.insert(ticket.to_string(), proposal);
    Ok((out, Changed::Proposed(ticket.to_string())))
}

/// The pages and requirements a change names, each once.
fn changed_refs(changed: &Changed) -> Vec<Reference> {
    let mut out: Vec<Reference> = Vec::new();
    let mut push = |r: Reference| {
        if !out.contains(&r) {
            out.push(r);
        }
    };
    match changed {
        Changed::Page(slug) => push(Reference::page(slug)),
        Changed::Requirement(slug) => push(Reference::req(slug)),
        Changed::Ticket(slug) => push(Reference::ticket(slug)),
        Changed::Deleted(r) if matches!(r.kind, Kind::Page | Kind::Req) => push(r.clone()),
        Changed::Many(cs) => cs.iter().flat_map(changed_refs).for_each(push),
        _ => {}
    }
    out
}

/// The fingerprint a proposed requirement rests on: the truth's record with its decisions left
/// out, since a proposal never carries them and the truth gaining one moves nothing it holds.
pub fn requirement_base(q: &Requirement) -> String {
    crate::fingerprint::of_record(&q.without_decisions())
}

/// Whether `base`, which a proposal's requirement rests on, is the truth's `now`: matched with its
/// decisions left out, or with them, as a proposal written before decisions left the record holds it.
fn rests_on_requirement(base: Option<&str>, now: Option<&Requirement>) -> bool {
    match (base, now) {
        (None, None) => true,
        (Some(b), Some(q)) => b == requirement_base(q) || b == crate::fingerprint::of_record(q),
        _ => false,
    }
}

/// What a write did to one requirement's decisions, seen from the record before it and after: each
/// decision appended becomes one the proposal adds, with no id, and each id a bottled decision came
/// to supersede a link (req:ticket-proposal).
fn absorb_decisions(p: &mut Proposal, slug: &str, before: Option<&Requirement>, after: &Requirement) {
    let was: &[Decision] = before.map(|q| q.decisions.as_slice()).unwrap_or(&[]);
    let adds = p.decisions.entry(slug.to_string()).or_default();
    for (i, d) in after.decisions.iter().enumerate() {
        match was.get(i) {
            None => adds.added.push(Decision { id: None, ..d.clone() }),
            Some(w) => {
                if let Some(id) = d.id.filter(|_| w.id == d.id) {
                    for x in d.supersedes.iter().filter(|x| !w.supersedes.contains(x)) {
                        let linked = adds.linked.entry(id).or_default();
                        if !linked.contains(x) {
                            linked.push(*x);
                            linked.sort_unstable();
                        }
                    }
                }
            }
        }
    }
    if adds.is_empty() {
        p.decisions.remove(slug);
    }
}

/// Move what an op changed in the working store into the proposal. A whole
/// write rests on the truth as it is now; a partial write keeps the base the
/// proposal already held for the record, or its deletion mark's. A
/// requirement's decisions go apart from its record, which is kept only when
/// the write was whole, changed more than its decisions, or was already held.
fn absorb(p: &mut Proposal, work: &Store, before: &std::collections::BTreeMap<String, Requirement>, truth: &Store, changed: &Changed, how: Proposing) {
    match changed {
        Changed::Page(slug) => {
            if let Some(rec) = work.pages.get(slug) {
                let held = p.deleted.remove(&Reference::page(slug).to_string()).or_else(|| p.pages.get(slug).and_then(|q| q.base.clone()));
                let now = truth.pages.get(slug).map(crate::fingerprint::of_record);
                let base = if how == Proposing::Whole { now } else { held.or(now) };
                p.pages.insert(slug.clone(), Proposed { record: rec.clone(), base });
            }
        }
        Changed::Requirement(slug) => {
            if let Some(rec) = work.requirements.get(slug) {
                let prior = before.get(slug);
                absorb_decisions(p, slug, prior, rec);
                let more_than_decisions = prior.map(Requirement::without_decisions) != Some(rec.without_decisions());
                if how == Proposing::Whole || more_than_decisions || p.requirements.contains_key(slug) {
                    let held = p.deleted.remove(&Reference::req(slug).to_string()).or_else(|| p.requirements.get(slug).and_then(|q| q.base.clone()));
                    let now = truth.requirements.get(slug).map(requirement_base);
                    let base = if how == Proposing::Whole { now } else { held.or(now) };
                    p.requirements.insert(slug.clone(), Proposed { record: rec.without_decisions(), base });
                }
            }
        }
        Changed::Deleted(r) => {
            let (held, now) = match r.kind {
                Kind::Page => (p.pages.remove(&r.id).and_then(|q| q.base), truth.pages.get(&r.id).map(crate::fingerprint::of_record)),
                Kind::Req => {
                    p.decisions.remove(&r.id);
                    (p.requirements.remove(&r.id).and_then(|q| q.base), truth.requirements.get(&r.id).map(requirement_base))
                }
                _ => (None, None),
            };
            // A record the truth never had leaves no mark: there is nothing to delete at the close.
            if let Some(fp) = if how == Proposing::Whole { now } else { held.or(now) } {
                p.deleted.insert(r.to_string(), fp);
            }
        }
        // A ticket's references, as an edit to another ticket leaves them: its proposal state.
        Changed::Ticket(slug) => {
            if let Some(t) = work.tickets.get(slug) {
                let now = truth.tickets.get(slug).map(|t| crate::fingerprint::of_record(&TicketReferences::of(t)));
                let held = p.tickets.get(slug).and_then(|q| q.base.clone());
                let base = if how == Proposing::Whole { now } else { held.or(now) };
                p.tickets.insert(slug.clone(), Proposed { record: TicketReferences::of(t), base });
            }
        }
        Changed::Many(cs) => {
            for c in cs {
                absorb(p, work, before, truth, c, how);
            }
        }
        // The baseline, the config, starts: not intent, never proposed.
        _ => {}
    }
}

/// Whether the truth moved under one record a proposal holds: its base no
/// longer matches the truth's fingerprint, or it was proposed as new and the
/// truth now has it. A proposed record equal to the truth's is not a change
/// and never moved; a deletion of a record already gone has nothing to move.
/// A record the proposal does not hold has not moved.
fn record_moved(truth: &Store, p: &Proposal, r: &Reference) -> bool {
    if r.kind == Kind::Ticket {
        // An edit to a ticket since closed or gone is dropped at the close, so it cannot have moved.
        let (Some(q), Some(t)) = (p.tickets.get(&r.id), truth.tickets.get(&r.id).filter(|t| t.status != Status::Bottled)) else { return false };
        let now = TicketReferences::of(t);
        return now != q.record && q.base.as_deref() != Some(crate::fingerprint::of_record(&now).as_str());
    }
    if r.kind == Kind::Req {
        let now = truth.requirements.get(&r.id);
        if let Some(base) = p.deleted.get(&r.to_string()) {
            return now.is_some() && !rests_on_requirement(Some(base), now);
        }
        let Some(q) = p.requirements.get(&r.id) else { return false };
        let same = now.is_some_and(|t| t.without_decisions() == q.record.without_decisions());
        return !same && !rests_on_requirement(q.base.as_deref(), now);
    }
    let now = match r.kind {
        Kind::Page => truth.pages.get(&r.id).map(crate::fingerprint::of_record),
        _ => return false,
    };
    if let Some(base) = p.deleted.get(&r.to_string()) {
        return now.as_ref().is_some_and(|n| n != base);
    }
    let Some(q) = p.pages.get(&r.id) else { return false };
    truth.pages.get(&r.id) != Some(&q.record) && q.base != now
}

/// The proposed records whose truth has moved since they were written, pages
/// first, then requirements, then deletions, then edits to tickets.
pub fn moved(store: &Store, ticket: &str) -> Vec<FieldError> {
    let Some(p) = store.proposals.get(ticket) else { return vec![] };
    let refs = p
        .pages
        .keys()
        .map(|s| Reference::page(s))
        .chain(p.requirements.keys().map(|s| Reference::req(s)))
        .chain(p.deleted.keys().filter_map(|k| k.parse::<Reference>().ok()))
        .chain(p.tickets.keys().map(|s| Reference::ticket(s)));
    refs.filter(|r| record_moved(store, p, r)).map(|r| FieldError::new(&r.to_string(), "changed in the truth since it was proposed; read it again and propose again")).collect()
}

/// Every simmering ticket whose proposal rests on a truth that has since
/// moved, with the records that moved: its bottling would be refused.
pub fn simmering_moved(store: &Store) -> std::collections::BTreeMap<String, Vec<String>> {
    store
        .tickets
        .values()
        .filter(|t| t.status == Status::Simmering)
        .filter_map(|t| {
            let refs: Vec<String> = moved(store, &t.slug).into_iter().map(|e| e.field).collect();
            (!refs.is_empty()).then(|| (t.slug.clone(), refs))
        })
        .collect()
}

/// Apply `ticket`'s proposal to the truth, in one pass, and drop it. The
/// caller checks `moved` first. Returns what changed, the proposal last.
pub fn apply_proposal(store: &mut Store, ticket: &str) -> Changed {
    let mut changes = Vec::new();
    if let Some(p) = store.proposals.remove(ticket) {
        for (slug, q) in &p.pages {
            if store.pages.get(slug) != Some(&q.record) {
                store.pages.insert(slug.clone(), q.record.clone());
                changes.push(Changed::Page(slug.clone()));
            }
        }
        // Each with the truth's decisions as they are now, and after them those the ticket adds, taking their ids.
        let slugs: std::collections::BTreeSet<&String> = p.requirements.keys().chain(p.decisions.keys()).collect();
        for slug in slugs {
            if let Some(q) = crate::store::with_decisions(store.requirements.get(slug), &p, slug, crate::store::Bottling::Bottled) {
                if store.requirements.get(slug) != Some(&q) {
                    store.requirements.insert(slug.clone(), q);
                    changes.push(Changed::Requirement(slug.clone()));
                }
            }
        }
        for key in p.deleted.keys() {
            if let Ok(r) = key.parse::<Reference>() {
                let removed = match r.kind {
                    Kind::Page => store.pages.remove(&r.id).is_some(),
                    Kind::Req => store.requirements.remove(&r.id).is_some(),
                    _ => false,
                };
                if removed {
                    // A page that is gone leaves the site, which would otherwise show nothing in its place.
                    let shown = r.kind == Kind::Page && store.config.site.contains(&r.id);
                    store.config.site.retain(|p| !shown || *p != r.id);
                    changes.push(Changed::Deleted(r));
                    if shown {
                        changes.push(Changed::Config);
                    }
                }
            }
        }
        // Edits to other tickets' references reach them now; one since closed or gone takes none.
        for (slug, q) in p.tickets {
            if let Some(t) = store.tickets.get_mut(&slug).filter(|t| t.status != Status::Bottled) {
                if TicketReferences::of(t) != q.record {
                    t.touches = q.record.touches;
                    t.relies_on = q.record.relies_on;
                    changes.push(Changed::Ticket(slug));
                }
            }
        }
    }
    changes.push(Changed::Proposed(ticket.to_string()));
    Changed::Many(changes)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::graph::fixtures::store;

    /// # Test contract
    ///
    /// ```json
    /// {"test": "a-proposal-leaves-the-truth-alone", "checks": ["crates/core/src/ops/proposal#keeps-write-in-proposal", "crates/core/src/ops/proposal#deletion-kept-as-mark", "crates/core/src/ops/proposal#neither-brewing-nor-simmering-conflict", "crates/core/src/ops/proposal#whole-write-rebases", "crates/core/src/ops/proposal#partial-write-keeps-base", "crates/core/src/ops/proposal#partial-write-refused-when-moved", "crates/core/src/ops/tickets#current-ticket"]}
    /// ```
    #[test]
    fn a_proposal_leaves_the_truth_alone() {
        let mut s = store();
        let before = s.pages["orphan"].clone();
        let orphan = |body: &str| Page { body: body.into(), ..before.clone() };
        // later-1 is brewing in the fixture: the write runs, and the truth does not move.
        let (p, changed) = propose(&mut s, "later-1", Proposing::Whole, |w| put_page(w, orphan("proposed"))).unwrap();
        assert_eq!(p.body, "proposed");
        assert!(matches!(changed, Changed::Proposed(ref t) if t == "later-1"));
        assert_eq!(s.pages["orphan"], before, "the truth is untouched");
        let q = &s.proposals["later-1"].pages["orphan"];
        assert_eq!((q.record.body.as_str(), q.base.clone()), ("proposed", Some(crate::fingerprint::of_record(&before))));
        // The next write sees the proposal's own view and keeps the base; a new record carries no base; a deletion is a mark.
        let (p, _) =
            propose(&mut s, "later-1", Proposing::Partial, |w| patch_page(w, &Reference::page("orphan"), PagePatch { add_links: vec![Reference::page("overview")], ..Default::default() })).unwrap();
        assert_eq!((p.body.as_str(), p.links.len()), ("proposed", 1));
        assert_eq!(s.proposals["later-1"].pages["orphan"].base, Some(crate::fingerprint::of_record(&before)));
        propose(&mut s, "later-1", Proposing::Whole, |w| put_page(w, Page { slug: "aim".into(), title: "Aim".into(), body: "b".into(), links: vec![] })).unwrap();
        assert_eq!(s.proposals["later-1"].pages["aim"].base, None);
        propose(&mut s, "later-1", Proposing::Partial, |w| delete_node(w, &Reference::page("git-readiness")).map(|c| ((), c))).unwrap();
        assert!(s.proposals["later-1"].deleted.contains_key("page:git-readiness"));
        assert!(s.pages.contains_key("git-readiness") && !s.pages.contains_key("aim"));
        // The truth moves under the proposed page: a partial write is refused naming it, a whole write rests on the new truth.
        put_page(&mut s, orphan("moved")).unwrap();
        let e = propose(&mut s, "later-1", Proposing::Partial, |w| patch_page(w, &Reference::page("orphan"), PagePatch { title: Some("T".into()), ..Default::default() })).unwrap_err();
        let v = serde_json::to_value(&e).unwrap();
        assert_eq!((v["error"].clone(), v["details"][0]["field"].clone()), (serde_json::json!("conflict"), serde_json::json!("page:orphan")));
        assert_eq!(s.proposals["later-1"].pages["orphan"].record.title, before.title, "nothing absorbed");
        propose(&mut s, "later-1", Proposing::Whole, |w| put_page(w, orphan("proposed again"))).unwrap();
        assert_eq!(s.proposals["later-1"].pages["orphan"].base, Some(crate::fingerprint::of_record(&s.pages["orphan"])));
        assert!(moved(&s, "later-1").is_empty(), "rebased");
        // An inscribed ticket is a conflict, a missing one not found.
        assert_eq!(propose(&mut s, "phase-62", Proposing::Whole, |w| put_page(w, orphan("x"))).unwrap_err().code(), "conflict");
        assert_eq!(propose(&mut s, "nope", Proposing::Whole, |w| put_page(w, orphan("x"))).unwrap_err().code(), "not_found");
        // The current ticket: a start sets it, the target falls back to it, a close of it clears it.
        assert_eq!(target_ticket(&s, Some("ticket:phase-62")).unwrap(), "phase-62");
        assert_eq!(target_ticket(&s, Some("phase-62")).unwrap(), "phase-62");
        assert_eq!(target_ticket(&s, None).unwrap_err().code(), "invalid");
        brew_ticket(&mut s, &Reference::ticket("phase-62")).unwrap();
        assert_eq!(s.current.as_deref(), Some("phase-62"));
        assert_eq!(target_ticket(&s, None).unwrap(), "phase-62");
        close_ticket(&mut s, &Reference::ticket("later-1"), Decision { id: None, date: "2026-10-02".into(), text: "Done.".into(), supersedes: vec![], adopted: false, withdrawn: false }).unwrap();
        assert_eq!(s.current.as_deref(), Some("phase-62"), "closing another ticket leaves it");
        close_ticket(&mut s, &Reference::ticket("phase-62"), Decision { id: None, date: "2026-10-02".into(), text: "Done.".into(), supersedes: vec![], adopted: false, withdrawn: false }).unwrap();
        assert_eq!(s.current, None);
    }

    /// # Test contract
    ///
    /// ```json
    /// {"test": "a-close-applies-the-proposal-unless-the-truth-moved", "checks": ["crates/core/src/ops/proposal#moved-names-differing-records", "crates/core/src/ops/proposal#moved-leaves-out-equal-records", "crates/core/src/ops/proposal#apply-writes-differing-records", "crates/core/src/ops/proposal#apply-removes-deleted", "crates/core/src/ops/proposal#apply-deleted-page-leaves-site", "crates/core/src/ops/proposal#apply-in-one-pass", "crates/core/src/ops/proposal#apply-drops-proposal"]}
    /// ```
    #[test]
    fn a_close_applies_the_proposal_unless_the_truth_moved() {
        let mut s = store();
        s.config.site = vec!["git-readiness".into(), "orphan".into()];
        let orphan = s.pages["orphan"].clone();
        let aim = |body: &str| Page { slug: "aim".into(), title: "Aim".into(), body: body.into(), links: vec![] };
        propose(&mut s, "later-1", Proposing::Whole, |w| put_page(w, Page { body: "proposed".into(), ..orphan.clone() })).unwrap();
        propose(&mut s, "later-1", Proposing::Whole, |w| put_page(w, aim("b"))).unwrap();
        propose(&mut s, "later-1", Proposing::Partial, |w| delete_node(w, &Reference::page("git-readiness")).map(|c| ((), c))).unwrap();
        assert!(moved(&s, "later-1").is_empty());
        // The truth moves under it: the page changed, the new one appeared.
        put_page(&mut s, Page { body: "moved".into(), ..orphan.clone() }).unwrap();
        put_page(&mut s, aim("someone else's")).unwrap();
        let fields = |s: &Store| moved(s, "later-1").into_iter().map(|e| e.field).collect::<Vec<_>>();
        assert_eq!(fields(&s), vec!["page:aim", "page:orphan"]);
        // A truth equal to the proposed record is no move, whatever its base.
        put_page(&mut s, Page { body: "proposed".into(), ..orphan.clone() }).unwrap();
        s.pages.remove("aim");
        assert!(fields(&s).is_empty());
        // A deletion conflicts when the record changed, not when it is already gone.
        let readiness = s.pages["git-readiness"].clone();
        put_page(&mut s, Page { body: "moved".into(), ..readiness.clone() }).unwrap();
        assert_eq!(fields(&s), vec!["page:git-readiness"]);
        s.pages.remove("git-readiness");
        assert!(fields(&s).is_empty());
        put_page(&mut s, readiness).unwrap();
        // Applied in one pass: only what differs is written, the deleted go, the proposal is dropped.
        let Changed::Many(cs) = apply_proposal(&mut s, "later-1") else { panic!("one pass") };
        assert_eq!(s.pages["orphan"].body, "proposed");
        assert!(s.pages.contains_key("aim") && !s.pages.contains_key("git-readiness"));
        assert!(cs.contains(&Changed::Config) && s.config.site == vec!["orphan".to_string()], "a deleted page left the site: {:?}", s.config.site);
        assert!(!s.proposals.contains_key("later-1"));
        assert!(matches!(cs[0], Changed::Page(ref p) if p == "aim"), "orphan already equal to the truth is not written: {cs:?}");
        assert!(matches!(cs[1], Changed::Deleted(ref r) if r.id == "git-readiness"));
        assert_eq!(cs[2], Changed::Config, "the site setting, for the page that left it");
        assert!(matches!(cs[3], Changed::Proposed(ref t) if t == "later-1"));
        assert_eq!(cs.len(), 4);
    }

    /// # Test contract
    ///
    /// ```json
    /// {"test": "an-edit-to-a-ticket-is-proposed-moved-renamed-and-applied", "checks": ["crates/core/src/ops/proposal#ticket-references-kept-as-lists", "crates/core/src/ops/proposal#moved-names-differing-records", "crates/core/src/ops/proposal#moved-leaves-out-closed-edit", "crates/core/src/ops/proposal#apply-edited-tickets-take-references", "crates/core/src/ops/proposal#apply-in-one-pass", "crates/core/src/ops/rename#rename-rewrites-links-marks-edits", "crates/core/src/ops/rename#rename-edit-base-follows-ticket"]}
    /// ```
    #[test]
    fn an_edit_to_a_ticket_is_proposed_moved_renamed_and_applied() {
        let mut s = store();
        let r = |v: &str| v.parse::<Reference>().unwrap();
        let edit = |touches: Vec<Reference>| TicketPatch { touches: Some(touches), ..Default::default() };
        let truth_62 = s.tickets["phase-62"].clone();
        // Proposed as the ticket's two lists, with the truth's lists' fingerprint; the truth is untouched.
        propose(&mut s, "later-1", Proposing::Partial, |w| update_ticket(w, &Reference::ticket("phase-62"), edit(vec![r("page:orphan"), r("req:ready-only-with-commit")]))).unwrap();
        propose(&mut s, "later-1", Proposing::Partial, |w| update_ticket(w, &Reference::ticket("gap-1"), edit(vec![r("page:orphan")]))).unwrap();
        assert_eq!(s.tickets["phase-62"], truth_62);
        let held = &s.proposals["later-1"].tickets["phase-62"];
        assert_eq!((held.record.relies_on.clone(), held.base.clone()), (truth_62.relies_on.clone(), Some(crate::fingerprint::of_record(&TicketReferences::of(&truth_62)))));
        assert_eq!(s.applied("later-1").tickets["phase-62"].touches, vec![r("page:orphan"), r("req:ready-only-with-commit")]);
        // A rename rewrites the edit, and its base follows the ticket's lists the rename rewrote.
        rename_node(&mut s, &Reference::req("ready-only-with-commit"), "ready").unwrap();
        assert_eq!(s.proposals["later-1"].tickets["phase-62"].record.touches, vec![r("page:orphan"), r("req:ready")]);
        assert!(moved(&s, "later-1").is_empty(), "the rename moved the truth's lists and the edit's base with them");
        // A rename stopped part way, the truth's lists rewritten and the proposal not, is finished by running it again.
        let renamed = s.clone();
        let old_base = crate::fingerprint::of_record(&TicketReferences::of(&truth_62));
        s.proposals.get_mut("later-1").unwrap().tickets.get_mut("phase-62").unwrap().base = Some(old_base);
        let mut back = s.requirements["ready"].clone();
        back.slug = "ready-only-with-commit".into();
        s.requirements.insert(back.slug.clone(), back);
        rename_node(&mut s, &Reference::req("ready-only-with-commit"), "ready").unwrap();
        assert_eq!(s.proposals["later-1"], renamed.proposals["later-1"]);
        assert!(moved(&s, "later-1").is_empty());
        // The truth's lists moving under the edit is a move; the ticket closing drops the edit, which can no longer move.
        update_ticket(&mut s, &Reference::ticket("gap-1"), edit(vec![r("page:overview")])).unwrap();
        assert_eq!(moved(&s, "later-1").into_iter().map(|e| e.field).collect::<Vec<_>>(), vec!["ticket:gap-1"]);
        s.tickets.get_mut("gap-1").unwrap().status = Status::Bottled;
        s.tickets.get_mut("gap-1").unwrap().decision = Some(Decision::new("2026-10-05", "Closed."));
        assert!(moved(&s, "later-1").is_empty());
        let Changed::Many(cs) = apply_proposal(&mut s, "later-1") else { panic!("one pass") };
        assert_eq!(s.tickets["phase-62"].touches, vec![r("page:orphan"), r("req:ready")]);
        assert_eq!(s.tickets["gap-1"].touches, vec![r("page:overview")], "a bottled ticket takes no edit");
        assert_eq!(cs, vec![Changed::Ticket("phase-62".into()), Changed::Proposed("later-1".into())]);
    }

    /// # Test contract
    ///
    /// ```json
    /// {"test": "decisions-wait-apart-and-are-bottled-after-the-truths", "checks": ["crates/core/src/ops/proposal#decisions-left-out-of-record", "crates/core/src/ops/proposal#appended-decision-kept-apart", "crates/core/src/ops/proposal#supersession-kept-as-link", "crates/core/src/ops/proposal#moved-leaves-out-equal-records", "crates/core/src/ops/proposal#apply-decisions-after-truths", "crates/core/src/ops/proposal#apply-retirement-supersedes-all", "crates/core/src/ops/proposal#apply-in-one-pass", "crates/core/src/ops/decisions#decisions-append-only", "crates/core/src/ops/decisions#retire"]}
    /// ```
    #[test]
    fn decisions_wait_apart_and_are_bottled_after_the_truths() {
        let mut s = store();
        let x = Reference::req("ready-only-with-commit");
        let truth = s.requirements["ready-only-with-commit"].clone();
        // A decision under the ticket waits apart, with no id, and no copy of the record is proposed.
        propose(&mut s, "later-1", Proposing::Partial, |w| record_decision(w, &x, "Ours.".into(), "2026-10-05".into(), vec![1], false)).unwrap();
        assert_eq!(s.requirements["ready-only-with-commit"], truth, "the truth is untouched");
        let p = &s.proposals["later-1"];
        assert!(p.requirements.is_empty());
        assert_eq!((p.decisions["ready-only-with-commit"].added[0].id, p.decisions["ready-only-with-commit"].added[0].supersedes.clone()), (None, vec![1]));
        let seen = &s.applied("later-1").requirements["ready-only-with-commit"];
        assert_eq!(seen.decisions.iter().map(|d| (d.id, d.text.as_str())).collect::<Vec<_>>(), vec![(Some(1), "Identity is required rather than invented."), (None, "Ours.")]);
        // A new statement: the record is proposed without decisions, resting on the truth's with them left out.
        let restated = Requirement { statement: "It SHALL hold, restated.".into(), ..truth.clone() };
        propose(&mut s, "later-1", Proposing::Whole, |w| put_requirement(w, restated.clone())).unwrap();
        let held = &s.proposals["later-1"].requirements["ready-only-with-commit"];
        assert!(held.record.decisions.is_empty());
        assert_eq!(held.base, Some(requirement_base(&truth)));
        // The truth gains a decision: nothing the proposal holds moved, and a decision or a link under it is never refused.
        record_decision(&mut s, &x, "Theirs.".into(), "2026-10-06".into(), vec![], false).unwrap();
        assert!(moved(&s, "later-1").is_empty(), "a decision the truth gained moves no proposal");
        propose(&mut s, "later-1", Proposing::Partial, |w| supersede(w, &x, 2, &[1])).unwrap();
        assert_eq!(s.proposals["later-1"].decisions["ready-only-with-commit"].linked[&2], vec![1]);
        assert_eq!(propose(&mut s, "later-1", Proposing::Partial, |w| supersede(w, &x, 3, &[1])).unwrap_err().code(), "not_found", "its own decision has no id yet");
        // Re-put as the truth now has it: the decisions it adds are kept, and nothing of the truth's is dropped.
        let now = s.requirements["ready-only-with-commit"].clone();
        propose(&mut s, "later-1", Proposing::Whole, |w| put_requirement(w, Requirement { statement: "It SHALL hold, restated.".into(), ..now.clone() })).unwrap();
        assert_eq!(s.proposals["later-1"].decisions["ready-only-with-commit"].added.len(), 1);
        // Bottled: the truth's decisions, the link added, and the ticket's own after them with the next id, keeping its date.
        let Changed::Many(cs) = apply_proposal(&mut s, "later-1") else { panic!("one pass") };
        assert!(cs.contains(&Changed::Requirement("ready-only-with-commit".into())));
        let bottled = &s.requirements["ready-only-with-commit"];
        assert_eq!(bottled.statement, "It SHALL hold, restated.");
        let ids = bottled.decisions.iter().map(|d| (d.id, d.text.as_str(), d.date.as_str(), d.supersedes.clone())).collect::<Vec<_>>();
        assert_eq!(ids, vec![(Some(1), "Identity is required rather than invented.", "2026-09-12", vec![]), (Some(2), "Theirs.", "2026-10-06", vec![1]), (Some(3), "Ours.", "2026-10-05", vec![1])]);
        assert!(crate::store::validate_requirement(bottled).is_empty());
        // A retirement under a ticket supersedes, as it is bottled, every decision the truth holds then.
        s.tickets.get_mut("later-1").unwrap().status = Status::Brewing;
        propose(&mut s, "later-1", Proposing::Partial, |w| retire_node(w, &x, "Gone.".into(), "2026-10-07".into())).unwrap();
        record_decision(&mut s, &x, "Meanwhile.".into(), "2026-10-07".into(), vec![], false).unwrap();
        assert_eq!(moved(&s, "later-1").len(), 0);
        assert_eq!(s.applied("later-1").requirements["ready-only-with-commit"].in_force().len(), 1, "read through the ticket, the retirement already supersedes what the truth gained");
        apply_proposal(&mut s, "later-1");
        let retired = &s.requirements["ready-only-with-commit"];
        assert!(retired.retired);
        assert_eq!((retired.decisions[4].id, retired.decisions[4].supersedes.clone()), (Some(5), vec![1, 2, 3, 4]));
        assert_eq!(retired.in_force().len(), 1);
    }

    /// # Test contract
    ///
    /// ```json
    /// {"test": "a-decision-under-a-moved-record-is-never-refused", "checks": ["crates/core/src/ops/proposal#decision-never-refused-as-moved"]}
    /// ```
    #[test]
    fn a_decision_under_a_moved_record_is_never_refused() {
        let mut s = store();
        let x = Reference::req("ready-only-with-commit");
        let truth = s.requirements["ready-only-with-commit"].clone();
        propose(&mut s, "later-1", Proposing::Whole, |w| put_requirement(w, Requirement { statement: "It SHALL hold, restated.".into(), ..truth.clone() })).unwrap();
        // The truth gains a decision, then its statement changes: the record the proposal holds has moved.
        record_decision(&mut s, &x, "Theirs.".into(), "2026-10-06".into(), vec![], false).unwrap();
        let now = s.requirements["ready-only-with-commit"].clone();
        put_requirement(&mut s, Requirement { statement: "It SHALL hold, moved.".into(), ..now }).unwrap();
        assert_eq!(moved(&s, "later-1").iter().map(|f| f.field.as_str()).collect::<Vec<_>>(), vec![x.to_string().as_str()], "the record moved");
        // A decision appended and a link on a bottled decision rest on no record: neither is refused.
        propose(&mut s, "later-1", Proposing::Partial, |w| record_decision(w, &x, "Ours.".into(), "2026-10-07".into(), vec![], false)).unwrap();
        propose(&mut s, "later-1", Proposing::Partial, |w| supersede(w, &x, 2, &[1])).unwrap();
        let waiting = &s.proposals["later-1"].decisions["ready-only-with-commit"];
        assert_eq!((waiting.added.len(), waiting.linked[&2].clone()), (1, vec![1]));
    }

    /// # Test contract
    ///
    /// ```json
    /// {"test": "a-proposal-written-before-ids-is-read-as-kept-now", "checks": ["crates/core/src/store#settles-proposals", "crates/core/src/ops/proposal#moved-names-differing-records", "crates/core/src/ops/proposal#apply-decisions-after-truths"]}
    /// ```
    #[test]
    fn a_proposal_written_before_ids_is_read_as_kept_now() {
        let mut s = store();
        let truth = s.requirements["ready-only-with-commit"].clone();
        // As an older build wrote it: the record whole, the truth's decision and its own, resting on the truth's whole record.
        let mut old = truth.clone();
        old.statement = "Restated.".into();
        old.decisions.push(Decision { id: Some(2), ..Decision::new("2026-10-01", "Ours.") });
        // Its own second decision superseding its own first, by position, as an older build allowed: it names nothing once settled.
        old.decisions.push(Decision { id: Some(3), supersedes: vec![1, 2], ..Decision::new("2026-10-02", "Ours, instead.") });
        let mut p = Proposal::new("later-1");
        p.requirements.insert(old.slug.clone(), Proposed { record: old, base: Some(crate::fingerprint::of_record(&truth)) });
        s.proposals.insert("later-1".into(), p);
        s.settle_proposals();
        let p = &s.proposals["later-1"];
        assert!(p.requirements["ready-only-with-commit"].record.decisions.is_empty());
        let added = |p: &Proposal| p.decisions["ready-only-with-commit"].added.iter().map(|d| (d.id, d.text.clone(), d.supersedes.clone())).collect::<Vec<_>>();
        assert_eq!(added(p), vec![(None, "Ours.".to_string(), vec![]), (None, "Ours, instead.".to_string(), vec![1])], "it keeps what it named among the truth's");
        assert!(moved(&s, "later-1").is_empty(), "a base taken over the whole record is still the truth's");
        // Settling again changes nothing.
        let once = s.clone();
        s.settle_proposals();
        assert_eq!(s, once);
        apply_proposal(&mut s, "later-1");
        let bottled = &s.requirements["ready-only-with-commit"];
        assert_eq!((bottled.statement.as_str(), bottled.decisions.len(), bottled.decisions[2].id, bottled.decisions[2].supersedes.clone()), ("Restated.", 3, Some(3), vec![1]));
    }

    /// # Test contract
    ///
    /// ```json
    /// {"test": "a-whole-write-rests-on-the-truth-as-it-is-now", "checks": ["crates/core/src/ops/proposal#whole-write-rebases"]}
    /// ```
    #[test]
    fn a_whole_write_rests_on_the_truth_as_it_is_now() {
        let mut s = store();
        let before = s.pages["orphan"].clone();
        let orphan = |body: &str| Page { body: body.into(), ..before.clone() };
        // The proposal already holds the page, resting on the truth as it was.
        propose(&mut s, "later-1", Proposing::Whole, |w| put_page(w, orphan("first"))).unwrap();
        assert_eq!(s.proposals["later-1"].pages["orphan"].base, Some(crate::fingerprint::of_record(&before)));
        // The truth moves; the whole write replaces the held record and rests on the truth now, not on the base it held.
        put_page(&mut s, orphan("moved")).unwrap();
        assert_eq!(moved(&s, "later-1").len(), 1);
        propose(&mut s, "later-1", Proposing::Whole, |w| put_page(w, orphan("second"))).unwrap();
        let held = &s.proposals["later-1"].pages["orphan"];
        assert_eq!(held.record.body, "second");
        assert_eq!(held.base, Some(crate::fingerprint::of_record(&s.pages["orphan"])));
        assert_ne!(held.base, Some(crate::fingerprint::of_record(&before)));
        assert!(moved(&s, "later-1").is_empty());
    }

    /// # Test contract
    ///
    /// ```json
    /// {"test": "a-partial-write-on-a-moved-record-is-refused-naming-it", "checks": ["crates/core/src/ops/proposal#partial-write-refused-when-moved"]}
    /// ```
    #[test]
    fn a_partial_write_on_a_moved_record_is_refused_naming_it() {
        let mut s = store();
        let r = |v: &str| v.parse::<Reference>().unwrap();
        let refused = |e: ToolError| {
            let v = serde_json::to_value(&e).unwrap();
            (v["error"].as_str().unwrap().to_string(), v["details"][0]["field"].as_str().unwrap().to_string())
        };
        let fresh = Requirement { slug: "fresh".into(), ..s.requirements.values().next().unwrap().clone() };
        let fresh = Requirement { decisions: vec![], retired: false, ..fresh };
        put_requirement(&mut s, fresh.clone()).unwrap();
        let orphan = s.pages["orphan"].clone();
        let touches = |t: Vec<Reference>| TicketPatch { touches: Some(t), ..Default::default() };
        // The proposal holds a page, a requirement and a ticket's lists, each resting on the truth as it is.
        propose(&mut s, "later-1", Proposing::Whole, |w| put_page(w, Page { title: "Held".into(), ..orphan.clone() })).unwrap();
        propose(&mut s, "later-1", Proposing::Whole, |w| put_requirement(w, Requirement { title: "Held".into(), ..fresh.clone() }).map(|(_, c)| ((), c))).unwrap();
        propose(&mut s, "later-1", Proposing::Partial, |w| update_ticket(w, &Reference::ticket("gap-1"), touches(vec![r("page:orphan")])).map(|(_, c)| ((), c))).unwrap();
        // The truth moves under all three.
        put_page(&mut s, Page { body: "moved".into(), ..orphan.clone() }).unwrap();
        put_requirement(&mut s, Requirement { statement: "The system SHALL move.".into(), ..fresh.clone() }).unwrap();
        update_ticket(&mut s, &Reference::ticket("gap-1"), touches(vec![r("page:overview")])).unwrap();
        let held = s.proposals["later-1"].clone();
        // A patch, a deletion, a retirement and one of a ticket's lists are each refused as a conflict naming the record, and absorb nothing.
        let patched =
            propose(&mut s, "later-1", Proposing::Partial, |w| patch_page(w, &r("page:orphan"), PagePatch { add_links: vec![r("page:overview")], ..Default::default() }).map(|(_, c)| ((), c)));
        assert_eq!(refused(patched.unwrap_err()), ("conflict".to_string(), "page:orphan".to_string()));
        let deleted = propose(&mut s, "later-1", Proposing::Partial, |w| delete_node(w, &r("page:orphan")).map(|c| ((), c)));
        assert_eq!(refused(deleted.unwrap_err()), ("conflict".to_string(), "page:orphan".to_string()));
        let patched =
            propose(&mut s, "later-1", Proposing::Partial, |w| patch_requirement(w, &r("req:fresh"), RequirementPatch { title: Some("T".into()), ..Default::default() }).map(|(_, c)| ((), c)));
        assert_eq!(refused(patched.unwrap_err()), ("conflict".to_string(), "req:fresh".to_string()));
        let retired = propose(&mut s, "later-1", Proposing::Partial, |w| retire_node(w, &r("req:fresh"), "Gone.".into(), "2026-10-08".into()).map(|(_, c)| ((), c)));
        assert_eq!(refused(retired.unwrap_err()), ("conflict".to_string(), "req:fresh".to_string()));
        let listed = propose(&mut s, "later-1", Proposing::Partial, |w| update_ticket(w, &Reference::ticket("gap-1"), touches(vec![r("page:orphan"), r("page:overview")])).map(|(_, c)| ((), c)));
        assert_eq!(refused(listed.unwrap_err()), ("conflict".to_string(), "ticket:gap-1".to_string()));
        assert_eq!(s.proposals["later-1"], held, "nothing absorbed");
    }
}
