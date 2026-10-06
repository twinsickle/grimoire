//! # Module contract
//!
//! ```json
//! {"purpose":"Keep a write that changes intent in its ticket's proposal, leaving the truth alone, and apply the proposal to the truth when the ticket closes or lands, refusing what moved under it.","criteria":[{"id":"proposal-writes","statement":"propose runs a write that changes intent on the store as the named active or landing ticket's proposal would leave it and keeps what it changed in that proposal, each record with the fingerprint of the truth's record it rests on, a deletion as a mark and a ticket whose references it changed as that ticket's touches and relies_on with the fingerprint of the truth's two lists, so the truth is untouched; a ticket that is neither, or a gap or question that is landing, is a conflict. A whole-record write rests on the truth as it is now, rebasing a record the proposal already held; a partial write (a patch, a decision, a supersession, a retirement, a deletion, one of a ticket's two lists) keeps the base the proposal held for the record and is refused as a conflict naming the record when the truth moved under it since it was proposed."},{"id":"close-applies-proposal","statement":"moved names every proposed record, and every ticket whose references the proposal edits, whose truth fingerprint differs from the base it was written against, or that was proposed as new and now exists, a record or edit equal to the truth's, and an edit to a ticket since closed or gone, never among them; apply_proposal writes the proposed records that differ from the truth's, removes the deleted ones, a deleted page leaving the pages the site shows, and gives each edited ticket that is not closed the references its edit holds, in one pass, and drops the proposal."}]}
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
/// rests on. The truth is untouched. The ticket must be active or landing, and a
/// partial write on a record whose truth moved since it was proposed is a
/// conflict naming the record.
pub fn propose<T>(store: &mut Store, ticket: &str, how: Proposing, op: impl FnOnce(&mut Store) -> Result<(T, Changed), ToolError>) -> Result<(T, Changed), ToolError> {
    match store.tickets.get(ticket) {
        None => return Err(ToolError::not_found(&Reference::ticket(ticket))),
        Some(t) if t.kind != TicketKind::Change && t.status == Status::Landing => {
            return Err(ToolError::conflict(format!("ticket:{ticket} is an answer waiting for the change that applies it: its spec belongs in that change")))
        }
        Some(t) if !t.status.is_live() => return Err(ToolError::conflict(format!("ticket:{ticket} is {}, neither active nor landing: a change belongs to a ticket being worked", t.status.name()))),
        _ => {}
    }
    let mut work = store.applied(ticket);
    let (out, changed) = op(&mut work)?;
    let mut proposal = store.proposals.get(ticket).cloned().unwrap_or_else(|| Proposal::new(ticket));
    if how == Proposing::Partial {
        let stale: Vec<FieldError> = changed_refs(&changed)
            .into_iter()
            .filter(|r| record_moved(store, &proposal, r))
            .map(|r| FieldError::new(&r.to_string(), "changed in the truth since it was proposed; get it as the truth now has it and put it again under the ticket"))
            .collect();
        if !stale.is_empty() {
            return Err(ToolError::conflict_with("the truth moved under this proposal's record since it was proposed; get it as the truth now has it and put it again under the ticket", stale));
        }
    }
    absorb(&mut proposal, &work, store, &changed, how);
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

/// Move what an op changed in the working store into the proposal. A whole
/// write rests on the truth as it is now; a partial write keeps the base the
/// proposal already held for the record, or its deletion mark's.
fn absorb(p: &mut Proposal, work: &Store, truth: &Store, changed: &Changed, how: Proposing) {
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
                let held = p.deleted.remove(&Reference::req(slug).to_string()).or_else(|| p.requirements.get(slug).and_then(|q| q.base.clone()));
                let now = truth.requirements.get(slug).map(crate::fingerprint::of_record);
                let base = if how == Proposing::Whole { now } else { held.or(now) };
                p.requirements.insert(slug.clone(), Proposed { record: rec.clone(), base });
            }
        }
        Changed::Deleted(r) => {
            let (held, now) = match r.kind {
                Kind::Page => (p.pages.remove(&r.id).and_then(|q| q.base), truth.pages.get(&r.id).map(crate::fingerprint::of_record)),
                Kind::Req => (p.requirements.remove(&r.id).and_then(|q| q.base), truth.requirements.get(&r.id).map(crate::fingerprint::of_record)),
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
                absorb(p, work, truth, c, how);
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
        let (Some(q), Some(t)) = (p.tickets.get(&r.id), truth.tickets.get(&r.id).filter(|t| t.status != Status::Closed)) else { return false };
        let now = TicketReferences::of(t);
        return now != q.record && q.base.as_deref() != Some(crate::fingerprint::of_record(&now).as_str());
    }
    let now = match r.kind {
        Kind::Page => truth.pages.get(&r.id).map(crate::fingerprint::of_record),
        Kind::Req => truth.requirements.get(&r.id).map(crate::fingerprint::of_record),
        _ => return false,
    };
    if let Some(base) = p.deleted.get(&r.to_string()) {
        return now.as_ref().is_some_and(|n| n != base);
    }
    let (base, same) = match r.kind {
        Kind::Page => match p.pages.get(&r.id) {
            Some(q) => (q.base.clone(), truth.pages.get(&r.id) == Some(&q.record)),
            None => return false,
        },
        _ => match p.requirements.get(&r.id) {
            Some(q) => (q.base.clone(), truth.requirements.get(&r.id) == Some(&q.record)),
            None => return false,
        },
    };
    !same && base != now
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

/// Every landing ticket whose proposal rests on a truth that has since
/// moved, with the records that moved: its landing would be refused.
pub fn landing_moved(store: &Store) -> std::collections::BTreeMap<String, Vec<String>> {
    store
        .tickets
        .values()
        .filter(|t| t.status == Status::Landing)
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
        for (slug, q) in p.pages {
            if store.pages.get(&slug) != Some(&q.record) {
                store.pages.insert(slug.clone(), q.record);
                changes.push(Changed::Page(slug));
            }
        }
        for (slug, q) in p.requirements {
            if store.requirements.get(&slug) != Some(&q.record) {
                store.requirements.insert(slug.clone(), q.record);
                changes.push(Changed::Requirement(slug));
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
            if let Some(t) = store.tickets.get_mut(&slug).filter(|t| t.status != Status::Closed) {
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
    /// {"test": "a-proposal-leaves-the-truth-alone", "checks": ["crates/core/src/ops/proposal#proposal-writes", "crates/core/src/ops/tickets#current-ticket"]}
    /// ```
    #[test]
    fn a_proposal_leaves_the_truth_alone() {
        let mut s = store();
        let before = s.pages["orphan"].clone();
        let orphan = |body: &str| Page { body: body.into(), ..before.clone() };
        // later-1 is active in the fixture: the write runs, and the truth does not move.
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
        // An open ticket is a conflict, a missing one not found.
        assert_eq!(propose(&mut s, "phase-62", Proposing::Whole, |w| put_page(w, orphan("x"))).unwrap_err().code(), "conflict");
        assert_eq!(propose(&mut s, "nope", Proposing::Whole, |w| put_page(w, orphan("x"))).unwrap_err().code(), "not_found");
        // The current ticket: a start sets it, the target falls back to it, a close of it clears it.
        assert_eq!(target_ticket(&s, Some("ticket:phase-62")).unwrap(), "phase-62");
        assert_eq!(target_ticket(&s, Some("phase-62")).unwrap(), "phase-62");
        assert_eq!(target_ticket(&s, None).unwrap_err().code(), "invalid");
        start_ticket(&mut s, &Reference::ticket("phase-62")).unwrap();
        assert_eq!(s.current.as_deref(), Some("phase-62"));
        assert_eq!(target_ticket(&s, None).unwrap(), "phase-62");
        close_ticket(&mut s, &Reference::ticket("later-1"), Decision { date: "2026-10-02".into(), text: "Done.".into(), supersedes: vec![], adopted: false, withdrawn: false }).unwrap();
        assert_eq!(s.current.as_deref(), Some("phase-62"), "closing another ticket leaves it");
        close_ticket(&mut s, &Reference::ticket("phase-62"), Decision { date: "2026-10-02".into(), text: "Done.".into(), supersedes: vec![], adopted: false, withdrawn: false }).unwrap();
        assert_eq!(s.current, None);
    }

    /// # Test contract
    ///
    /// ```json
    /// {"test": "a-close-applies-the-proposal-unless-the-truth-moved", "checks": ["crates/core/src/ops/proposal#close-applies-proposal"]}
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
    /// {"test": "an-edit-to-a-ticket-is-proposed-moved-renamed-and-applied", "checks": ["crates/core/src/ops/proposal#proposal-writes", "crates/core/src/ops/proposal#close-applies-proposal", "crates/core/src/ops/rename#rename"]}
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
        s.tickets.get_mut("gap-1").unwrap().status = Status::Closed;
        s.tickets.get_mut("gap-1").unwrap().decision = Some(Decision::new("2026-10-05", "Closed."));
        assert!(moved(&s, "later-1").is_empty());
        let Changed::Many(cs) = apply_proposal(&mut s, "later-1") else { panic!("one pass") };
        assert_eq!(s.tickets["phase-62"].touches, vec![r("page:orphan"), r("req:ready")]);
        assert_eq!(s.tickets["gap-1"].touches, vec![r("page:overview")], "a closed ticket takes no edit");
        assert_eq!(cs, vec![Changed::Ticket("phase-62".into()), Changed::Proposed("later-1".into())]);
    }
}
