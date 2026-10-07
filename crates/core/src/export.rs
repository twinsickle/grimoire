//! # Module contract
//!
//! ```json
//! {"purpose":"Hand a store's intent from a server to a machine: the intent as one value, the store a machine keeps joined from it and what is the machine's own, and what differs between two stores, for the writer of that copy.","criteria":[{"id":"whole-intent","statement":"exported gives every page, requirement, ticket and proposal, every recipe's judgements, the baseline, and the config's version, roots, languages, scope, order, site, line limits and recipe kinds, and nothing that belongs to a machine: no repository path, no current ticket and no start."},{"id":"joined","statement":"joined makes the store a machine keeps: the server's intent, the recipes' judgements among it, its config's site, line limits and recipe kinds included, the machine's repository, and of the machine's own only what still applies, its current ticket and its starts for tickets the server still has being worked, active or a change waiting to land; a proposal whose ticket is not being worked is left out, so a gap or question waiting, landing, for its change is no one's current ticket, and a store version this build does not write is refused."},{"id":"older-builds","statement":"exported gives the proposals as a build older than decision ids keeps them, each requirement a proposal holds or adds decisions to held whole, with the truth's decisions, the links it adds added and the decisions it adds after them, each at its position, resting on the truth's whole record when it rests on the truth as it is, and gives them as this build keeps them beside, in kept_proposals, which an older build ignores; joined reads kept_proposals in their place when it is there, and without it reads the proposals as written before decisions left the record."},{"id":"difference","statement":"difference names every record, proposal, start, recipe's judgements, the current ticket, the baseline and the config that differ between two stores, as changes that persisted over the first leave the second, a deleted ticket's start and proposal going with it, in an order that leaves a store that loads wherever it stops: what names a ticket as being worked, the current ticket and a proposal, is removed before the ticket's record changes and written after it; two equal stores differ in nothing."}]}
//! ```

use serde::{Deserialize, Serialize};

use crate::model::*;
use crate::ops::Changed;
use crate::store::Store;

/// A store's intent as one value: what a server hands a machine. Nothing of
/// a machine is in it, so it reads the same wherever it is fetched.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Exported {
    pub version: u32,
    // A list while both are named, so a machine on an older build still reads a newer server's export.
    #[serde(serialize_with = "crate::model::roots_as_older_builds_read")]
    pub roots: Roots,
    pub languages: Vec<String>,
    #[serde(default)]
    pub scope: Scope,
    #[serde(default)]
    pub order: Order,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub site: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub large_file: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub plain_file: Option<u32>,
    #[serde(default, skip_serializing_if = "std::collections::BTreeMap::is_empty")]
    pub recipe_kinds: std::collections::BTreeMap<String, RecipeKind>,
    pub pages: Vec<Page>,
    pub requirements: Vec<Requirement>,
    pub tickets: Vec<Ticket>,
    // The proposals as an older build keeps them, decisions inside each record, so a machine on one still reads them.
    pub proposals: Vec<Proposal>,
    // The proposals as this build keeps them, decisions apart: ignored by an older machine, read by this one in place of the above.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub kept_proposals: Option<Vec<Proposal>>,
    pub baseline: Option<Baseline>,
    // Each judged recipe's judgements; absent from an older server's export, and ignored by an older machine.
    #[serde(default, skip_serializing_if = "std::collections::BTreeMap::is_empty")]
    pub judgements: std::collections::BTreeMap<String, Vec<Judgement>>,
}

/// The intent `store` holds, with nothing of the machine it is on.
pub fn exported(store: &Store) -> Exported {
    Exported {
        version: store.config.version,
        roots: store.config.roots.clone(),
        languages: store.config.languages.clone(),
        scope: store.config.scope.clone(),
        order: store.config.order.clone(),
        site: store.config.site.clone(),
        large_file: store.config.large_file,
        plain_file: store.config.plain_file,
        recipe_kinds: store.config.recipe_kinds.clone(),
        pages: store.pages.values().cloned().collect(),
        requirements: store.requirements.values().cloned().collect(),
        tickets: store.tickets.values().cloned().collect(),
        proposals: store.proposals.values().map(|p| for_older_builds(store, p)).collect(),
        kept_proposals: Some(store.proposals.values().cloned().collect()),
        baseline: store.baseline.clone(),
        judgements: store.judgements.clone(),
    }
}

/// `p` as a build older than decision ids keeps a proposal (req:remote-export-for-older-builds):
/// each requirement it holds or adds decisions to, whole, with the truth's decisions, its links
/// and then its own, at their positions, resting on the truth's whole record when it rests on the
/// truth as it is, which is what an older build compares.
fn for_older_builds(store: &Store, p: &Proposal) -> Proposal {
    let mut older = Proposal { decisions: Default::default(), ..p.clone() };
    let slugs: std::collections::BTreeSet<&String> = p.requirements.keys().chain(p.decisions.keys()).collect();
    for slug in slugs {
        let truth = store.requirements.get(slug);
        let Some(record) = crate::store::with_decisions(truth, p, slug, crate::store::Landing::Landed) else { continue };
        let base = match (p.requirements.get(slug).map(|q| q.base.clone()), truth) {
            (Some(Some(b)), Some(t)) if b == crate::ops::requirement_base(t) => Some(crate::fingerprint::of_record(t)),
            (Some(base), _) => base,
            (None, t) => t.map(crate::fingerprint::of_record),
        };
        older.requirements.insert(slug.clone(), Proposed { record, base });
    }
    older
}

impl Exported {
    /// The store a machine keeps: this intent, `repository` as the checkout
    /// it is joined with, and what is the machine's own in `own`, its current
    /// ticket and its starts, as far as the server still has those tickets
    /// active or landing.
    pub fn joined(self, own: &Store, repository: &str) -> Result<Store, String> {
        if self.version > StoreConfig::CURRENT_VERSION {
            return Err(format!("the server's store is version {}, and this build writes version {}: update grimoire on this machine", self.version, StoreConfig::CURRENT_VERSION));
        }
        let config = StoreConfig {
            large_file: self.large_file,
            plain_file: self.plain_file,
            version: StoreConfig::CURRENT_VERSION,
            repository: repository.to_string(),
            roots: self.roots,
            languages: self.languages,
            scope: self.scope,
            order: self.order,
            site: self.site,
            recipe_kinds: self.recipe_kinds,
        };
        let mut store = Store::new(config);
        store.pages = self.pages.into_iter().map(|p| (p.slug.clone(), p)).collect();
        store.requirements = self.requirements.into_iter().map(|r| (r.slug.clone(), r)).collect();
        store.tickets = self.tickets.into_iter().map(|t| (t.slug.clone(), t)).collect();
        let live = |slug: &str| store.tickets.get(slug).is_some_and(Ticket::is_worked);
        // As this build keeps them; from an older server, as written before decisions left the record, and settled.
        let proposals = self.kept_proposals.unwrap_or(self.proposals);
        store.proposals = proposals.into_iter().filter(|p| live(&p.ticket)).map(|p| (p.ticket.clone(), p)).collect();
        store.baseline = self.baseline;
        store.judgements = self.judgements;
        store.current = own.current.clone().filter(|c| live(c));
        store.starts = own.starts.iter().filter(|(slug, _)| live(slug)).map(|(slug, s)| (slug.clone(), s.clone())).collect();
        store.settle_proposals();
        Ok(store)
    }
}

/// What differs between two stores, as the changes that, persisted over
/// `old`, leave `new`. The order leaves a store that loads wherever a writer
/// stops: what names a ticket as being worked, the current ticket and a
/// proposal, goes before the ticket's record changes and comes after it, and
/// deletions come before writes, so a slug that moved kind is free.
pub fn difference(old: &Store, new: &Store) -> Vec<Changed> {
    let mut out = Vec::new();
    // No longer current, and proposals no longer held: before the tickets they name stop being worked.
    if old.current != new.current && new.current.is_none() {
        out.push(Changed::Current);
    }
    for slug in old.proposals.keys().filter(|s| !new.proposals.contains_key(*s) && new.tickets.contains_key(*s)) {
        out.push(Changed::Proposed(slug.clone()));
    }
    for slug in old.pages.keys().filter(|s| !new.pages.contains_key(*s)) {
        out.push(Changed::Deleted(Reference::page(slug)));
    }
    for slug in old.requirements.keys().filter(|s| !new.requirements.contains_key(*s)) {
        out.push(Changed::Deleted(Reference::req(slug)));
    }
    // A deleted ticket takes its start and proposal with it.
    for slug in old.tickets.keys().filter(|s| !new.tickets.contains_key(*s)) {
        out.push(Changed::Deleted(Reference::ticket(slug)));
    }
    out.extend(new.pages.iter().filter(|(s, p)| old.pages.get(*s) != Some(p)).map(|(s, _)| Changed::Page(s.clone())));
    out.extend(new.requirements.iter().filter(|(s, r)| old.requirements.get(*s) != Some(r)).map(|(s, _)| Changed::Requirement(s.clone())));
    out.extend(new.tickets.iter().filter(|(s, t)| old.tickets.get(*s) != Some(t)).map(|(s, _)| Changed::Ticket(s.clone())));
    // Proposals written or changed: after the tickets they belong to.
    out.extend(new.proposals.iter().filter(|(s, p)| old.proposals.get(*s) != Some(p)).map(|(s, _)| Changed::Proposed(s.clone())));
    let starts: std::collections::BTreeSet<&String> = old.starts.keys().chain(new.starts.keys()).collect();
    for slug in starts {
        if old.starts.get(slug) == new.starts.get(slug) || !new.tickets.contains_key(slug) {
            continue;
        }
        // Written with its ticket's record, or removed beside it.
        out.push(if new.starts.contains_key(slug) { Changed::Started(slug.clone()) } else { Changed::Closed(slug.clone()) });
    }
    if old.current != new.current && new.current.is_some() {
        out.push(Changed::Current);
    }
    let judged: std::collections::BTreeSet<&String> = old.judgements.keys().chain(new.judgements.keys()).collect();
    out.extend(judged.into_iter().filter(|r| old.judgements.get(*r) != new.judgements.get(*r)).map(|r| Changed::Judged(r.clone())));
    if old.baseline != new.baseline {
        out.push(Changed::Baseline);
    }
    if old.config != new.config {
        out.push(Changed::Config);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::graph::fixtures::store;

    fn judgement(recipe: &str) -> Judgement {
        Judgement { recipe: recipe.into(), fingerprint: "0123456789abcdef".into(), date: "2026-10-07".into(), ticket: "later-1".into(), outcome: JudgementOutcome::Held }
    }

    /// # Test contract
    ///
    /// ```json
    /// {"test": "an-export-holds-the-intent-and-nothing-of-a-machine", "checks": ["crates/core/src/export#whole-intent"]}
    /// ```
    #[test]
    fn an_export_holds_the_intent_and_nothing_of_a_machine() {
        let mut s = store();
        s.current = Some("later-1".into());
        s.starts.insert("later-1".into(), Start { ticket: "later-1".into(), fingerprints: Default::default(), closed_in: None });
        s.baseline = Some(Baseline { date: "2026-10-03".into(), reason: "adopted".into(), findings: vec![] });
        s.config.site = vec!["overview".into()];
        s.config.large_file = Some(600);
        let e = exported(&s);
        assert_eq!((e.pages.len(), e.requirements.len(), e.tickets.len(), e.proposals.len()), (s.pages.len(), s.requirements.len(), s.tickets.len(), s.proposals.len()));
        assert_eq!((e.roots.clone(), e.languages.clone(), e.baseline.clone(), e.site.clone()), (s.config.roots.clone(), s.config.languages.clone(), s.baseline.clone(), s.config.site.clone()));
        assert_eq!((e.large_file, e.plain_file), (Some(600), None), "the line limits too");
        s.config.recipe_kinds.insert("component".into(), RecipeKind { uses: vec![] });
        let e = exported(&s);
        assert_eq!((e.pages.len(), e.requirements.len(), e.tickets.len(), e.proposals.len()), (s.pages.len(), s.requirements.len(), s.tickets.len(), s.proposals.len()));
        assert_eq!((e.roots.clone(), e.languages.clone(), e.baseline.clone(), e.site.clone()), (s.config.roots.clone(), s.config.languages.clone(), s.baseline.clone(), s.config.site.clone()));
        assert_eq!(e.recipe_kinds, s.config.recipe_kinds, "the recipe kinds too");
        assert!(e.judgements.is_empty() && !serde_json::to_string(&e).unwrap().contains("judgements"), "none judged, none written: an older machine reads it as before");
        s.judgements.insert("button".into(), vec![judgement("button")]);
        let e = exported(&s);
        assert_eq!(e.judgements, s.judgements, "the recipes' judgements");
        let text = serde_json::to_string(&e).unwrap();
        // Both roots named: written as the list an older machine reads; one missing, as the pair, which reads back the same.
        assert_eq!(serde_json::to_value(&e).unwrap()["roots"], serde_json::json!(["overview", "rites"]));
        let one = Exported { roots: Roots { grimoire: Some("overview".into()), rites: None }, ..e.clone() };
        assert_eq!(serde_json::to_value(&one).unwrap()["roots"], serde_json::json!({ "grimoire": "overview" }));
        assert_eq!(serde_json::from_value::<Exported>(serde_json::to_value(&one).unwrap()).unwrap(), one);
        assert!(!text.contains(&s.config.repository) && !text.contains("fingerprints") && !text.contains("\"current\""), "no repository, start or current ticket");
        // It reads back as it was written, so a machine holds what the server does.
        assert_eq!(serde_json::from_str::<Exported>(&text).unwrap(), e);
    }

    /// # Test contract
    ///
    /// ```json
    /// {"test": "an-older-build-reads-what-a-server-hands-it", "checks": ["crates/core/src/export#older-builds"]}
    /// ```
    #[test]
    fn an_older_build_reads_what_a_server_hands_it() {
        let mut server = store();
        let x = Reference::req("ready-only-with-commit");
        let truth = server.requirements["ready-only-with-commit"].clone();
        crate::ops::propose(&mut server, "later-1", crate::ops::Proposing::Partial, |w| crate::ops::record_decision(w, &x, "Ours.".into(), "2026-10-05".into(), vec![], false)).unwrap();
        let e = exported(&server);
        // As an older build keeps it: no field it does not know, the record whole with the truth's decision and then the ticket's, resting on the truth's whole record.
        let older = serde_json::to_value(&e.proposals).unwrap();
        assert!(older[0].get("decisions").is_none(), "{older}");
        let held = &e.proposals[0].requirements["ready-only-with-commit"];
        assert_eq!(held.record.decisions.iter().map(|d| (d.id, d.text.as_str())).collect::<Vec<_>>(), vec![(Some(1), "Identity is required rather than invented."), (Some(2), "Ours.")]);
        assert!(!serde_json::to_string(&held.record).unwrap().contains("\"id\""), "each at its position, so no id is written");
        assert_eq!(held.base, Some(crate::fingerprint::of_record(&truth)));
        // This build reads them as the server keeps them, beside.
        let text = serde_json::to_string(&e).unwrap();
        let joined = serde_json::from_str::<Exported>(&text).unwrap().joined(&Store::default(), "C:/checkout").unwrap();
        assert_eq!(joined.proposals, server.proposals);
        // From an older server, which hands only the first: read as written before decisions left the record.
        let from_older = Exported { kept_proposals: None, ..e.clone() };
        let joined = from_older.joined(&Store::default(), "C:/checkout").unwrap();
        let p = &joined.proposals["later-1"];
        assert_eq!(p.decisions["ready-only-with-commit"].added.iter().map(|d| (d.id, d.text.as_str())).collect::<Vec<_>>(), vec![(None, "Ours.")]);
        assert!(p.requirements["ready-only-with-commit"].record.decisions.is_empty());
        assert!(crate::ops::moved(&joined, "later-1").is_empty());
    }

    /// # Test contract
    ///
    /// ```json
    /// {"test": "a-machines-store-joins-the-intent-with-its-own", "checks": ["crates/core/src/export#joined"]}
    /// ```
    #[test]
    fn a_machines_store_joins_the_intent_with_its_own() {
        let mut server = store();
        server.config.site = vec!["overview".into()];
        server.config.plain_file = Some(3_000);
        server.config.recipe_kinds.insert("quest".into(), RecipeKind { uses: vec!["quest".into()] });
        let live: String = server.tickets.values().find(|t| t.status.is_live()).map(|t| t.slug.clone()).expect("the fixture has an active ticket");
        let other: String = server.tickets.values().find(|t| !t.status.is_live()).map(|t| t.slug.clone()).expect("and one that is not");
        let start = |slug: &str| Start { ticket: slug.to_string(), fingerprints: Default::default(), closed_in: None };
        let mut own = Store { current: Some(live.clone()), ..Default::default() };
        own.starts.insert(live.clone(), start(&live));
        own.starts.insert(other.clone(), start(&other));
        let joined = exported(&server).joined(&own, "C:/checkout").unwrap();
        assert_eq!((joined.config.repository.as_str(), &joined.config.site), ("C:/checkout", &server.config.site), "the machine's checkout, and the server's site");
        assert_eq!(joined.config.plain_file, Some(3_000), "and its line limits");
        assert_eq!(joined.config.recipe_kinds, server.config.recipe_kinds, "and its recipe kinds");
        let mut judged = server.clone();
        judged.judgements.insert("button".into(), vec![judgement("button")]);
        assert_eq!(exported(&judged).joined(&own, "C:/checkout").unwrap().judgements, judged.judgements, "and the recipes' judgements");
        assert_eq!((&joined.pages, &joined.requirements, &joined.tickets, &joined.proposals), (&server.pages, &server.requirements, &server.tickets, &server.proposals));
        assert_eq!(joined.current.as_deref(), Some(live.as_str()));
        assert_eq!(joined.starts.keys().collect::<Vec<_>>(), vec![&live], "a start for a ticket no longer being worked is dropped");
        // A current ticket the server has closed, or never had, is no longer current here.
        own.current = Some(other);
        assert_eq!(exported(&server).joined(&own, "C:/checkout").unwrap().current, None);
        own.current = Some("never-filed".into());
        assert_eq!(exported(&server).joined(&own, "C:/checkout").unwrap().current, None);
        // An answer waiting, landing, for the change that applies it is no one's current ticket, and keeps no start.
        let mut answered = server.clone();
        let gap = answered.tickets.values_mut().find(|t| t.kind != TicketKind::Change).expect("the fixture has a gap");
        (gap.status, gap.decision) = (Status::Landing, Some(Decision::new("2026-10-04", "Fix it.")));
        let gap = gap.slug.clone();
        own.current = Some(gap.clone());
        own.starts.insert(gap.clone(), start(&gap));
        let joined = exported(&answered).joined(&own, "C:/checkout").unwrap();
        assert_eq!((joined.current, joined.starts.contains_key(&gap)), (None, false));
        // A newer store than this build writes is refused, not half read.
        let mut newer = exported(&server);
        newer.version = StoreConfig::CURRENT_VERSION + 1;
        assert!(newer.joined(&own, "C:/checkout").unwrap_err().contains("update grimoire"));
    }

    /// # Test contract
    ///
    /// ```json
    /// {"test": "the-difference-is-what-changed-and-no-more", "checks": ["crates/core/src/export#difference"]}
    /// ```
    #[test]
    fn the_difference_is_what_changed_and_no_more() {
        let old = store();
        assert!(difference(&old, &old).is_empty());
        let mut new = old.clone();
        let page = new.pages.keys().next().cloned().unwrap();
        new.pages.get_mut(&page).unwrap().title = "Renamed".into();
        let gone = new.requirements.keys().next().cloned().unwrap();
        new.requirements.remove(&gone);
        let live: String = new.tickets.values().find(|t| t.status.is_live()).map(|t| t.slug.clone()).unwrap();
        new.current = Some(live.clone());
        new.starts.insert(live.clone(), Start { ticket: live.clone(), fingerprints: Default::default(), closed_in: None });
        new.baseline = Some(Baseline { date: "2026-10-03".into(), reason: "adopted".into(), findings: vec![] });
        new.config.roots.rites = Some("extra".into());
        new.judgements.insert("button".into(), vec![judgement("button")]);
        let d = difference(&old, &new);
        assert_eq!(
            d,
            vec![Changed::Deleted(Reference::req(&gone)), Changed::Page(page), Changed::Started(live.clone()), Changed::Current, Changed::Judged("button".into()), Changed::Baseline, Changed::Config]
        );
        assert!(difference(&new, &old).contains(&Changed::Judged("button".into())), "judgements gone are named too");
        // Back again: the start goes beside its ticket, and a deleted ticket takes its own with it.
        assert!(difference(&new, &old).contains(&Changed::Closed(live.clone())));
        let mut without = new.clone();
        without.tickets.remove(&live);
        without.starts.remove(&live);
        without.proposals.remove(&live);
        without.current = None;
        let d = difference(&new, &without);
        assert!(d.contains(&Changed::Deleted(Reference::ticket(&live))) && !d.iter().any(|c| matches!(c, Changed::Closed(_) | Changed::Proposed(_))), "{d:?}");
        // A ticket closed elsewhere: it stops being current and loses its proposal before its record says closed,
        // so a writer stopped anywhere between leaves a store that loads.
        let mut worked = new.clone();
        worked.proposals.insert(live.clone(), Proposal::new(&live));
        let mut closed = worked.clone();
        closed.tickets.get_mut(&live).unwrap().status = Status::Closed;
        closed.proposals.remove(&live);
        closed.starts.remove(&live);
        closed.current = None;
        assert_eq!(difference(&worked, &closed), vec![Changed::Current, Changed::Proposed(live.clone()), Changed::Ticket(live.clone()), Changed::Closed(live.clone())]);
        // And taken up again: the record first, then what names it.
        assert_eq!(difference(&closed, &worked), vec![Changed::Ticket(live.clone()), Changed::Proposed(live.clone()), Changed::Started(live.clone()), Changed::Current]);
    }
}
