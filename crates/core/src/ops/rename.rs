//! # Module contract
//!
//! ```json
//! {"purpose":"Rename a page or requirement and rewrite every reference the store holds to it: the truth's records, the proposals, the baseline and the starts.","criteria":[{"id":"rename","statement":"Renaming moves a page or requirement to a new slug with every field and decision intact and rewrites the references the store holds to it, and to a requirement's decisions (page links, tickets that are not bottled, roots, the pages the site shows, baseline entries, start snapshots, and every proposal: the copy it holds of the record moves with it, its base following when it rested on the truth's old record, its links, marks and edits to other tickets' references are rewritten, and an edit's base follows its ticket when it rested on that ticket's lists before the rename rewrote them, each changed proposal persisted), a list that already named the new reference keeping it once, leaving bottled tickets as they were; a new slug that a proposal already holds a record under is a conflict naming the ticket; it persists the new record first and removes the old one last, and a rename to a slug held by the very record being renamed, equal in every field but its slug once its own links to itself are moved too, which a rename that failed part way leaves, completes it."}]}
//! ```

use super::*;

/// Rename a page or requirement to `to`, keeping every field and decision,
/// and rewrite the references the store holds. Closed tickets keep the old
/// reference as history. The changes come back in the order they must be
/// persisted: the new record, what refers to it, and the old record last, so
/// a failure part way leaves both rather than a dangling link.
pub fn rename_node(store: &mut Store, r: &Reference, to: &str) -> Result<(Reference, Changed), ToolError> {
    if !matches!(r.kind, Kind::Page | Kind::Req) {
        return Err(ToolError::invalid(format!("{r} cannot be renamed: only pages and requirements can"), vec![]));
    }
    let exists = |s: &Store, slug: &str| match r.kind {
        Kind::Page => s.pages.contains_key(slug),
        _ => s.requirements.contains_key(slug),
    };
    if !exists(store, &r.id) {
        return Err(ToolError::not_found(r));
    }
    if !crate::store::is_slug(to) {
        return Err(ToolError::invalid("the new slug is not a slug", vec![FieldError::new("to", "must match ^[a-z0-9]+(-[a-z0-9]+)*$ and be at most 64 characters")]));
    }
    // A taken slug is a conflict, unless what holds it is this very record,
    // left there by a rename that failed part way: then this call finishes it.
    // The comparison is with the record as the rename would write it, so a page
    // that links itself, whose link the rename moved too, still compares equal.
    // A list is rewritten the one way everywhere: the old reference becomes the new, kept once, so a rerun compares with what the first run wrote.
    // What a reference becomes: the record's own, or one of a requirement's decisions, req:<slug>#d<n>.
    let renamed = |l: &Reference| -> Option<Reference> {
        if l == r {
            return Some(Reference::new(r.kind, to));
        }
        match l.as_decision() {
            Some((slug, id)) if r.kind == Kind::Req && slug == r.id => Some(Reference::decision(to, id)),
            _ => None,
        }
    };
    let rewrite = |refs: &[Reference]| {
        let mut kept: Vec<Reference> = Vec::with_capacity(refs.len());
        for l in refs {
            let l = renamed(l).unwrap_or_else(|| l.clone());
            if !kept.contains(&l) {
                kept.push(l);
            }
        }
        kept
    };
    let as_renamed = |links: &[Reference]| rewrite(links);
    let renamed_already = match r.kind {
        Kind::Page => store.pages.get(to).is_some_and(|there| store.pages.get(&r.id).is_some_and(|p| &Page { slug: to.to_string(), links: as_renamed(&p.links), ..p.clone() } == there)),
        _ => store.requirements.get(to).is_some_and(|there| store.requirements.get(&r.id).is_some_and(|q| &Requirement { slug: to.to_string(), ..q.clone() } == there)),
    };
    if exists(store, to) && !renamed_already {
        return Err(ToolError::conflict(format!("{} already exists", Reference::new(r.kind, to))));
    }
    // A proposal holding a record under the new slug would lose it to the rename;
    // one a rename that failed part way already moved there is this rename's own.
    let held_in = store.proposals.values().find(|p| match r.kind {
        Kind::Page => p.pages.contains_key(to),
        _ => p.requirements.contains_key(to),
    });
    if let Some(p) = held_in.filter(|_| !renamed_already) {
        return Err(ToolError::conflict(format!("{} is proposed by ticket:{}: close or change that ticket before renaming onto it", Reference::new(r.kind, to), p.ticket)));
    }
    let new = Reference::new(r.kind, to);
    // The truth's fingerprint before and after, so a proposal that rested on the old record rests on the renamed one.
    // A requirement's base leaves its decisions out, or, written before they left the record, holds them: both follow.
    let fingerprint = |s: &Store, slug: &str| match r.kind {
        Kind::Page => s.pages.get(slug).map(crate::fingerprint::of_record),
        _ => s.requirements.get(slug).map(crate::fingerprint::of_record),
    };
    let base_of = |s: &Store, slug: &str| match r.kind {
        Kind::Page => s.pages.get(slug).map(crate::fingerprint::of_record),
        _ => s.requirements.get(slug).map(super::requirement_base),
    };
    let (old_fp, old_base) = (fingerprint(store, &r.id), base_of(store, &r.id));
    let mut changes = Vec::new();
    // The record itself, under its new slug.
    match r.kind {
        Kind::Page => {
            let mut p = store.pages.remove(&r.id).expect("checked above");
            p.slug = to.to_string();
            store.pages.insert(to.to_string(), p);
            changes.push(Changed::Page(to.to_string()));
        }
        _ => {
            let mut q = store.requirements.remove(&r.id).expect("checked above");
            q.slug = to.to_string();
            store.requirements.insert(to.to_string(), q);
            changes.push(Changed::Requirement(to.to_string()));
        }
    }
    let swap = |refs: &mut Vec<Reference>| {
        let hit = refs.iter().any(|l| renamed(l).is_some());
        if hit {
            *refs = rewrite(refs);
        }
        hit
    };
    for p in store.pages.values_mut() {
        if swap(&mut p.links) {
            changes.push(Changed::Page(p.slug.clone()));
        }
    }
    // Each ticket's lists as they were, so an edit a proposal holds that rested on them follows the rename.
    let lists_before: std::collections::BTreeMap<String, String> = store.tickets.iter().map(|(s, t)| (s.clone(), crate::fingerprint::of_record(&TicketReferences::of(t)))).collect();
    for t in store.tickets.values_mut().filter(|t| t.status != Status::Bottled) {
        let a = swap(&mut t.touches);
        let b = swap(&mut t.relies_on);
        if a || b {
            changes.push(Changed::Ticket(t.slug.clone()));
        }
    }
    if r.kind == Kind::Page && store.config.roots.rename(&r.id, to) {
        changes.push(Changed::Config);
    }
    if r.kind == Kind::Page && store.config.site.contains(&r.id) {
        // The site follows its page, named once.
        let mut site: Vec<String> = Vec::new();
        for p in store.config.site.drain(..).map(|p| if p == r.id { to.to_string() } else { p }) {
            if !site.contains(&p) {
                site.push(p);
            }
        }
        store.config.site = site;
        changes.push(Changed::Config);
    }
    if let Some(b) = store.baseline.as_mut() {
        let mut hit = false;
        for k in b.findings.iter_mut() {
            if let Some(n) = renamed(&k.r#ref) {
                k.r#ref = n;
                hit = true;
            }
            if let Some(e) = k.edge.as_mut() {
                for end in [&mut e.from, &mut e.to] {
                    if let Some(n) = renamed(end) {
                        *end = n;
                        hit = true;
                    }
                }
            }
        }
        if hit {
            changes.push(Changed::Baseline);
        }
    }
    let (old_key, new_key) = (r.to_string(), new.to_string());
    for s in store.starts.values_mut() {
        if let Some(fp) = s.fingerprints.remove(&old_key) {
            s.fingerprints.insert(new_key.clone(), fp);
            changes.push(Changed::Started(s.ticket.clone()));
        }
    }
    // Every proposal: its copy of the record moves with it, its base following
    // when it rested on the truth's old record; its links and marks are rewritten.
    let (new_fp, new_base) = (fingerprint(store, to), base_of(store, to));
    let rebase = |base: Option<String>| {
        if base.is_some() && base == old_fp {
            new_fp.clone()
        } else if base.is_some() && base == old_base {
            new_base.clone()
        } else {
            base
        }
    };
    for p in store.proposals.values_mut() {
        let mut hit = false;
        match r.kind {
            Kind::Page => {
                if let Some(mut q) = p.pages.remove(&r.id) {
                    q.record.slug = to.to_string();
                    swap(&mut q.record.links);
                    q.base = rebase(q.base);
                    p.pages.insert(to.to_string(), q);
                    hit = true;
                }
            }
            _ => {
                if let Some(mut q) = p.requirements.remove(&r.id) {
                    q.record.slug = to.to_string();
                    q.base = rebase(q.base);
                    p.requirements.insert(to.to_string(), q);
                    hit = true;
                }
                // The decisions it adds go with it.
                if let Some(adds) = p.decisions.remove(&r.id) {
                    p.decisions.insert(to.to_string(), adds);
                    hit = true;
                }
            }
        }
        for q in p.pages.values_mut() {
            if swap(&mut q.record.links) {
                hit = true;
            }
        }
        if let Some(fp) = p.deleted.remove(&old_key) {
            p.deleted.insert(new_key.clone(), rebase(Some(fp)).expect("a mark keeps a fingerprint"));
            hit = true;
        }
        for (slug, q) in p.tickets.iter_mut() {
            let a = swap(&mut q.record.touches);
            let b = swap(&mut q.record.relies_on);
            let now = store.tickets.get(slug).map(|t| crate::fingerprint::of_record(&TicketReferences::of(t)));
            // Rested on the lists before this rename, or, rerun after one stopped part way with the truth's lists already
            // rewritten, on those lists with the new reference read back as the old.
            let unrenamed = store.tickets.get(slug).map(|t| {
                let back = |refs: &[Reference]| refs.iter().map(|x| if *x == new { r.clone() } else { x.clone() }).collect();
                crate::fingerprint::of_record(&TicketReferences { touches: back(&t.touches), relies_on: back(&t.relies_on) })
            });
            if q.base.is_some() && (q.base.as_ref() == lists_before.get(slug) || q.base == unrenamed) && q.base != now {
                q.base = now;
                hit = true;
            }
            if a || b {
                hit = true;
            }
        }
        if hit {
            changes.push(Changed::Proposed(p.ticket.clone()));
        }
    }
    changes.push(Changed::Deleted(r.clone()));
    Ok((new, Changed::Many(changes)))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::graph::fixtures::store;

    /// # Test contract
    ///
    /// ```json
    /// {"test": "a-rename-and-a-ticket-deletion-reach-proposals", "checks": ["crates/core/src/ops/rename#rename", "crates/core/src/ops#delete-node"]}
    /// ```
    #[test]
    fn a_rename_and_a_ticket_deletion_reach_proposals() {
        let mut s = store();
        let orphan = s.pages["orphan"].clone();
        // later-1 proposes a change to orphan, a new page linking it, and a deletion of git-readiness.
        propose(&mut s, "later-1", Proposing::Whole, |w| put_page(w, Page { body: "proposed".into(), ..orphan.clone() })).unwrap();
        propose(&mut s, "later-1", Proposing::Whole, |w| {
            put_page(w, Page { slug: "aim".into(), title: "Aim".into(), body: "b".into(), links: vec![Reference::page("orphan"), Reference::page("git-readiness")] })
        })
        .unwrap();
        propose(&mut s, "later-1", Proposing::Partial, |w| delete_node(w, &Reference::page("git-readiness")).map(|c| ((), c))).unwrap();
        s.config.site = vec!["orphan".into(), "lost".into(), "overview".into()];
        let (_, changed) = rename_node(&mut s, &Reference::page("orphan"), "lost").unwrap();
        let Changed::Many(cs) = changed else { panic!("many") };
        assert!(cs.iter().any(|c| matches!(c, Changed::Proposed(t) if t == "later-1")), "{cs:?}");
        assert!(cs.contains(&Changed::Config) && s.config.site == vec!["lost".to_string(), "overview".into()], "the site follows its page, once: {:?}", s.config.site);
        let p = &s.proposals["later-1"];
        assert!(!p.pages.contains_key("orphan"));
        assert_eq!((p.pages["lost"].record.slug.as_str(), p.pages["lost"].record.body.as_str()), ("lost", "proposed"));
        assert_eq!(p.pages["lost"].base, Some(crate::fingerprint::of_record(&s.pages["lost"])), "the base follows the renamed truth");
        assert_eq!(p.pages["aim"].record.links[0], Reference::page("lost"), "a proposed link is rewritten");
        assert!(moved(&s, "later-1").is_empty());
        // A slug a proposal holds a record under cannot be renamed onto; a page linking both old and new keeps one link.
        let e = rename_node(&mut s, &Reference::page("git-readiness"), "aim").unwrap_err();
        assert!(e.code() == "conflict" && e.to_string().contains("ticket:later-1"), "{e}");
        assert!(s.pages.contains_key("git-readiness") && s.proposals["later-1"].pages.contains_key("aim"), "nothing changed");
        propose(&mut s, "later-1", Proposing::Partial, |w| patch_page(w, &Reference::page("aim"), PagePatch { add_links: vec![Reference::page("readiness")], ..Default::default() })).unwrap();
        // A mark is rewritten too.
        rename_node(&mut s, &Reference::page("git-readiness"), "readiness").unwrap();
        let p = &s.proposals["later-1"];
        assert!(p.deleted.contains_key("page:readiness") && !p.deleted.contains_key("page:git-readiness"));
        assert_eq!(p.pages["aim"].record.links, vec![Reference::page("lost"), Reference::page("readiness")], "the link it already had to the new slug is kept once");
        assert!(moved(&s, "later-1").is_empty());
        // Deleting the ticket takes its start, its proposal and its currency with it.
        brew_ticket(&mut s, &Reference::ticket("phase-62")).unwrap();
        s.starts.insert("phase-62".into(), Start { ticket: "phase-62".into(), fingerprints: Default::default(), simmering_in: None });
        let lost = s.pages["lost"].clone();
        propose(&mut s, "phase-62", Proposing::Whole, |w| put_page(w, Page { body: "theirs".into(), ..lost })).unwrap();
        let changed = delete_node(&mut s, &Reference::ticket("phase-62")).unwrap();
        assert!(matches!(&changed, Changed::Many(cs) if matches!(cs[0], Changed::Deleted(ref r) if r.id == "phase-62") && matches!(cs[1], Changed::Current)), "{changed:?}");
        assert!(!s.tickets.contains_key("phase-62") && !s.starts.contains_key("phase-62") && !s.proposals.contains_key("phase-62"));
        assert_eq!(s.current, None);
        assert!(s.proposals.contains_key("later-1"), "another ticket's proposal stays");
        // Deleting a ticket that is not current leaves the current one.
        s.tickets.insert(
            "gap-2".into(),
            Ticket {
                slug: "gap-2".into(),
                title: "G".into(),
                kind: TicketKind::Gap,
                priority: Priority::Later,
                status: Status::Inscribed,
                touches: vec![],
                relies_on: vec![],
                body: "b".into(),
                decision: None,
            },
        );
        s.current = Some("later-1".into());
        assert!(matches!(delete_node(&mut s, &Reference::ticket("gap-2")).unwrap(), Changed::Deleted(_)));
        assert_eq!(s.current.as_deref(), Some("later-1"));
    }

    /// # Test contract
    ///
    /// ```json
    /// {"test": "a-simmering-ticket-is-renamed-and-reordered-like-a-brewing-one", "checks": ["crates/core/src/ops/rename#rename", "crates/core/src/ops#reorder", "crates/core/src/ops#delete-node"]}
    /// ```
    #[test]
    fn a_simmering_ticket_is_renamed_and_reordered_like_a_brewing_one() {
        let mut s = crate::graph::fixtures::store();
        // later-1 touches page:orphan; its decision is written and it waits to be bottled.
        let t = s.tickets.get_mut("later-1").unwrap();
        t.status = Status::Simmering;
        t.decision = Some(Decision::new("2026-10-03", "Done."));
        assert!(validate_ticket(&s.tickets["later-1"]).is_empty());
        // A rename reaches its references, as it reaches a brewing ticket's.
        rename_node(&mut s, &Reference::page("orphan"), "kept").unwrap();
        assert_eq!(s.tickets["later-1"].touches, vec![Reference::page("kept")]);
        // It keeps its place in its band's order.
        let (order, _) = reorder(&mut s, Priority::Later, &[Reference::ticket("later-1")]).unwrap();
        assert_eq!(order, vec!["later-1".to_string()]);
        // And it is not history yet: it can be deleted, its decision with it.
        delete_node(&mut s, &Reference::ticket("later-1")).unwrap();
        assert!(!s.tickets.contains_key("later-1"));
    }

    /// # Test contract
    ///
    /// ```json
    /// {"test": "a-rename-takes-a-requirements-decisions-with-it", "checks": ["crates/core/src/ops/rename#rename"]}
    /// ```
    #[test]
    fn a_rename_takes_a_requirements_decisions_with_it() {
        let mut s = store();
        let x = Reference::req("ready-only-with-commit");
        let restated = Requirement { statement: "Restated.".into(), ..s.requirements["ready-only-with-commit"].clone() };
        propose(&mut s, "later-1", Proposing::Whole, |w| put_requirement(w, restated)).unwrap();
        propose(&mut s, "later-1", Proposing::Partial, |w| record_decision(w, &x, "Ours.".into(), "2026-10-05".into(), vec![1], false)).unwrap();
        s.tickets.get_mut("phase-62").unwrap().relies_on.push("req:ready-only-with-commit#d1".parse().unwrap());
        rename_node(&mut s, &x, "ready").unwrap();
        assert!(s.tickets["phase-62"].relies_on.contains(&Reference::decision("ready", 1)), "a reference to one of its decisions follows it");
        let p = &s.proposals["later-1"];
        assert!(p.decisions.contains_key("ready") && !p.decisions.contains_key("ready-only-with-commit"), "the decisions it adds go with it");
        assert_eq!(p.requirements["ready"].base, Some(requirement_base(&s.requirements["ready"])), "its base follows, its decisions left out");
        assert!(moved(&s, "later-1").is_empty());
    }
}
