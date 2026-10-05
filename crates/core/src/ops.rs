//! # Module contract
//!
//! ```json
//! {"purpose":"Apply the write tools to a store value: put a page or requirement, file, update, start and close a ticket, rename, reorder and delete a node, adopt a baseline, configure the store; a requirement's decisions are kept by the decisions module beneath.","criteria":[{"id":"rename","statement":"Renaming moves a page or requirement to a new slug with every field and decision intact and rewrites the references the store holds to it (page links, tickets that are not closed, roots, the pages the site shows, baseline entries, start snapshots, and every proposal: the copy it holds of the record moves with it, its base following when it rested on the truth's old record, and its links and marks are rewritten, each changed proposal persisted), a list that already named the new reference keeping it once, leaving closed tickets as they were; a new slug that a proposal already holds a record under is a conflict naming the ticket; it persists the new record first and removes the old one last, and a rename to a slug held by the very record being renamed, equal in every field but its slug once its own links to itself are moved too, which a rename that failed part way leaves, completes it."},{"id":"reorder","statement":"Reordering replaces one band's order list with the given tickets, each a ticket of that priority that is not closed, named once, and refuses any other as invalid."},{"id":"configure","statement":"Configuring changes only the roots, languages, scope and site it is given, a list of roots naming both and a pair only the roots it gives, refuses a language the build cannot extract and a site naming a page that does not exist, validates the whole config, and never changes the repository."},{"id":"adopt","statement":"Adopting replaces the baseline with the given findings, dated, with a required reason, and reports how many the previous baseline held."},{"id":"validate-first","statement":"A write validates the whole resulting record before changing the store, and an invalid record changes nothing."},{"id":"never-fails-on-lint","statement":"A write that creates a finding (a dangling edge, or a requirement with no scenario) succeeds; the lint result is the caller's to report."},{"id":"close-keeps-only-decision","statement":"Closing a ticket replaces its body with the empty string and sets its decision; a closed ticket cannot be reopened or edited."},{"id":"patch-is-partial","statement":"patch_page changes only the fields given: added links go at the end without duplicates, removing an absent link is not an error, every other field stays as it was, and adding a link to a retired requirement is refused as invalid."},{"id":"put-page","statement":"put_page creates or replaces a page by slug; replacing is not a conflict; a link to a retired requirement is refused as invalid, since no page links one."},{"id":"put-requirement","statement":"put_requirement creates or replaces a requirement by slug, ignoring decisions and retired on input and preserving the existing ones; a retired requirement is refused as invalid."},{"id":"file-ticket","statement":"file_ticket creates an open ticket with a null decision under a generated id: one past the highest all-digit id in the store, zero-padded to four digits, so filing never collides and never asks for a name."},{"id":"update-ticket","statement":"update_ticket changes only the given title, priority, touches, relies_on or body of a ticket that is not closed, and is a conflict on a closed one."},{"id":"priority-follows","statement":"keep_pace keeps a change that is not closed no lower priority than the gaps and questions it relies on that are not closed: after a change was lowered, each such answer now above it is lowered to the most urgent priority among the changes not closed that rely on it; then a change below any such answer is raised to the most urgent of them; after a gap or question was raised, or filed when a change already names it, every change not closed that relies on it and sits below it is raised to it; a closed ticket is never moved, lowering an answer or raising a change moves nothing, and it answers each slug it moved once."},{"id":"start-ticket","statement":"start_ticket moves a ticket from open to active and is a conflict from any other status."},{"id":"delete-node","statement":"delete_node removes a page, a requirement with no decision, or a ticket that is not closed, a ticket's start and proposal going with it and the current ticket cleared when it was that one; a requirement with decisions (retire it instead), a closed ticket and a gap or question waiting, landing, for the change that applies its answer are invalid, derived kinds are invalid and a missing node is not_found."},{"id":"proposal-writes","statement":"propose runs a write that changes intent on the store as the named active or landing ticket's proposal would leave it and keeps what it changed in that proposal, each record with the fingerprint of the truth's record it rests on and a deletion as a mark, so the truth is untouched; a ticket that is neither, or a gap or question that is landing, is a conflict. A whole-record write rests on the truth as it is now, rebasing a record the proposal already held; a partial write (a patch, a decision, a supersession, a retirement, a deletion) keeps the base the proposal held for the record and is refused as a conflict naming the record when the truth moved under it since it was proposed."},{"id":"closes-when-landed","statement":"decide_ticket writes a decision ahead of a landing: an active ticket becomes landing and a landing one has its decision replaced, keeping its body, proposal and start, and an open or closed ticket is a conflict; a gap or question becomes landing only when a change that is not closed relies on it, invalid otherwise, and only with an empty proposal, invalid otherwise, its proposal then dropped and the current ticket cleared when it was that one; close_ticket refuses a landing gap or question as a conflict."},{"id":"settles-answers","statement":"settle looks only at the given gaps and questions that are landing and that no change not closed relies on: after a change closed it closes each with its decision, its body emptied and its start removed; after a change went away it closes each that a closed change relying on it closed on or after its decision's date, and reopens every other, open with no decision and its start removed, its body followed by the answer's date, why it reopened and the answer's text, cut with an ellipsis to the body's limit; it answers the slugs it closed and reopened."},{"id":"current-ticket","statement":"start_ticket makes the started ticket the store's current one and close_ticket clears it when it closes that one, a landing ticket staying current until then; target_ticket answers the ticket a write belongs to, the one given as a slug or ticket: reference, else the current one, and refuses as invalid when there is neither."},{"id":"close-applies-proposal","statement":"moved names every proposed record whose truth fingerprint differs from the base it was written against, or that was proposed as new and now exists, a record equal to the truth's never among them; apply_proposal writes the proposed records that differ from the truth's and removes the deleted ones, a deleted page leaving the pages the site shows, in one pass and drops the proposal."}]}
//! ```

use serde::Serialize;

mod decisions;

pub use decisions::{record_decision, retire_node, supersede};

use crate::model::*;
use crate::store::{validate_baseline, validate_config, validate_page, validate_requirement, validate_ticket, FieldError, Store};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, thiserror::Error)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[serde(tag = "error", rename_all = "snake_case")]
pub enum ToolError {
    #[error("{message}")]
    NotFound { message: String },
    #[error("{message}")]
    Invalid { message: String, details: Vec<FieldError> },
    #[error("{message}")]
    Conflict {
        message: String,
        #[serde(default)]
        details: Vec<FieldError>,
    },
    #[error("{message}")]
    StoreUnavailable { message: String },
}

impl ToolError {
    pub fn not_found(r: &Reference) -> Self {
        ToolError::NotFound { message: format!("{r} does not exist") }
    }
    pub fn invalid(message: impl Into<String>, details: Vec<FieldError>) -> Self {
        ToolError::Invalid { message: message.into(), details }
    }
    pub fn conflict(message: impl Into<String>) -> Self {
        ToolError::Conflict { message: message.into(), details: vec![] }
    }
    /// A conflict with what conflicts named in `details`, as a close whose
    /// truth moved names the moved records.
    pub fn conflict_with(message: impl Into<String>, details: Vec<FieldError>) -> Self {
        ToolError::Conflict { message: message.into(), details }
    }
    pub fn unavailable(message: impl Into<String>) -> Self {
        ToolError::StoreUnavailable { message: message.into() }
    }
    pub fn code(&self) -> &'static str {
        match self {
            ToolError::NotFound { .. } => "not_found",
            ToolError::Invalid { .. } => "invalid",
            ToolError::Conflict { .. } => "conflict",
            ToolError::StoreUnavailable { .. } => "store_unavailable",
        }
    }
}

fn invalid_if(errors: Vec<FieldError>, what: &str) -> Result<(), ToolError> {
    if errors.is_empty() {
        Ok(())
    } else {
        Err(ToolError::invalid(format!("{what} violates the schema"), errors))
    }
}

/// The stored kinds a write changed, so a caller can persist exactly those.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Changed {
    Page(String),
    Requirement(String),
    Ticket(String),
    Deleted(Reference),
    Baseline,
    Config,
    /// A ticket started: its record and its snapshot.
    Started(String),
    /// A ticket closed: its record, and its snapshot removed.
    Closed(String),
    /// A ticket's proposal changed: written as it now is, or removed when the
    /// store no longer holds one for the ticket.
    Proposed(String),
    /// The current ticket changed: written as it now is, or removed when none.
    Current,
    /// Several changes from one write, persisted in this order.
    Many(Vec<Changed>),
}

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
        Changed::Many(cs) => {
            for c in cs {
                absorb(p, work, truth, c, how);
            }
        }
        // Tickets, the baseline, the config, starts: not intent, never proposed.
        _ => {}
    }
}

/// Whether the truth moved under one record a proposal holds: its base no
/// longer matches the truth's fingerprint, or it was proposed as new and the
/// truth now has it. A proposed record equal to the truth's is not a change
/// and never moved; a deletion of a record already gone has nothing to move.
/// A record the proposal does not hold has not moved.
fn record_moved(truth: &Store, p: &Proposal, r: &Reference) -> bool {
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
/// first, then requirements, then deletions.
pub fn moved(store: &Store, ticket: &str) -> Vec<FieldError> {
    let Some(p) = store.proposals.get(ticket) else { return vec![] };
    let refs = p.pages.keys().map(|s| Reference::page(s)).chain(p.requirements.keys().map(|s| Reference::req(s))).chain(p.deleted.keys().filter_map(|k| k.parse::<Reference>().ok()));
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
    }
    changes.push(Changed::Proposed(ticket.to_string()));
    Changed::Many(changes)
}

/// The ticket a write that changes intent belongs to: the one given, as a
/// slug or a `ticket:` reference, else the store's current ticket; neither
/// is refused, since nothing changes the truth outside a ticket.
pub fn target_ticket(store: &Store, given: Option<&str>) -> Result<String, ToolError> {
    match given.map(|g| g.strip_prefix("ticket:").unwrap_or(g).to_string()).or_else(|| store.current.clone()) {
        Some(t) => Ok(t),
        None => Err(ToolError::invalid(
            "name the ticket this change belongs to: `ticket` is absent and no ticket is current (start one, or pass ticket)",
            vec![FieldError::new("ticket", "absent, and no ticket is current")],
        )),
    }
}

/// The config fields `configure` may change; the repository is not one.
#[derive(Debug, Clone, Default, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ConfigChange {
    pub roots: Option<RootsChange>,
    pub languages: Option<Vec<String>>,
    pub scope: Option<Scope>,
    pub site: Option<Vec<String>>,
}

/// The roots `configure` is given: a list of slugs, as older stores held, which names both, or the
/// pair, of which only the roots given change.
#[derive(Debug, Clone, serde::Deserialize)]
#[serde(untagged)]
pub enum RootsChange {
    List(Vec<String>),
    Pair {
        #[serde(default)]
        grimoire: Option<String>,
        #[serde(default)]
        rites: Option<String>,
    },
}

/// Change the fields given. `supported` is the languages the running build
/// can extract; empty means unknown, and then any language is accepted.
pub fn configure(store: &mut Store, change: ConfigChange, supported: &[String]) -> Result<(StoreConfig, Changed), ToolError> {
    let mut config = store.config.clone();
    match change.roots {
        // A list, as older stores held, names both: a second missing is a rites missing.
        Some(RootsChange::List(list)) => config.roots = list.into(),
        // The pair changes only the roots it gives.
        Some(RootsChange::Pair { grimoire, rites }) => {
            if grimoire.is_some() {
                config.roots.grimoire = grimoire;
            }
            if rites.is_some() {
                config.roots.rites = rites;
            }
        }
        None => {}
    }
    if let Some(languages) = change.languages {
        config.languages = languages;
    }
    if let Some(scope) = change.scope {
        config.scope = scope;
    }
    let mut errors = Vec::new();
    if let Some(site) = change.site {
        // The site shows pages that exist: one that names none would show nothing in its place, unseen.
        for p in site.iter().filter(|p| !store.pages.contains_key(*p)) {
            errors.push(FieldError::new("site", format!("`{p}` names no page")));
        }
        config.site = site;
    }
    errors.extend(validate_config(&config));
    if !supported.is_empty() {
        for l in config.languages.iter().filter(|l| !supported.contains(l)) {
            errors.push(FieldError::new("languages", format!("`{l}` is not supported by this build, which supports {}", supported.join(", "))));
        }
    }
    invalid_if(errors, "config")?;
    store.config = config.clone();
    Ok((config, Changed::Config))
}

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
    let rewrite = |refs: &[Reference]| {
        let mut kept: Vec<Reference> = Vec::with_capacity(refs.len());
        for l in refs {
            let l = if l == r { Reference::new(r.kind, to) } else { l.clone() };
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
    let fingerprint = |s: &Store, slug: &str| match r.kind {
        Kind::Page => s.pages.get(slug).map(crate::fingerprint::of_record),
        _ => s.requirements.get(slug).map(crate::fingerprint::of_record),
    };
    let old_fp = fingerprint(store, &r.id);
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
        let hit = refs.contains(r);
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
    for t in store.tickets.values_mut().filter(|t| t.status != Status::Closed) {
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
            if &k.r#ref == r {
                k.r#ref = new.clone();
                hit = true;
            }
            if let Some(e) = k.edge.as_mut() {
                for end in [&mut e.from, &mut e.to] {
                    if *end == *r {
                        *end = new.clone();
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
    let new_fp = fingerprint(store, to);
    let rebase = |base: Option<String>| if base.is_some() && base == old_fp { new_fp.clone() } else { base };
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
        if hit {
            changes.push(Changed::Proposed(p.ticket.clone()));
        }
    }
    changes.push(Changed::Deleted(r.clone()));
    Ok((new, Changed::Many(changes)))
}

/// Replace one band's order with `order`, every entry a ticket of that
/// priority that is not closed, named once.
pub fn reorder(store: &mut Store, priority: Priority, order: &[Reference]) -> Result<(Vec<String>, Changed), ToolError> {
    let mut errors = Vec::new();
    let mut seen = std::collections::HashSet::new();
    for r in order {
        let ok = r.kind == Kind::Ticket && store.tickets.get(&r.id).is_some_and(|t| t.status != Status::Closed && t.priority == priority);
        if !ok {
            errors.push(FieldError::new("order", format!("{r} is not a {} ticket that is still open, active or landing", format!("{priority:?}").to_lowercase())));
        } else if !seen.insert(&r.id) {
            errors.push(FieldError::new("order", format!("{r} is named twice")));
        }
    }
    invalid_if(errors, "order")?;
    let slugs: Vec<String> = order.iter().map(|r| r.id.clone()).collect();
    *store.config.order.band_mut(priority) = slugs.clone();
    Ok((slugs, Changed::Config))
}

/// Replace the baseline with `findings`. Returns the new baseline and how
/// many findings the previous one held, if there was one.
pub fn adopt(store: &mut Store, findings: Vec<FindingKey>, reason: String, date: String) -> Result<(Baseline, Option<usize>, Changed), ToolError> {
    let baseline = Baseline { date, reason, findings };
    invalid_if(validate_baseline(&baseline), "baseline")?;
    let replaced = store.baseline.replace(baseline.clone()).map(|b| b.findings.len());
    Ok((baseline, replaced, Changed::Baseline))
}

/// No page links a retired requirement: it no longer holds.
fn refuse_retired_links(store: &Store, links: &[Reference]) -> Result<(), ToolError> {
    match links.iter().find(|l| l.kind == Kind::Req && store.requirements.get(&l.id).is_some_and(|q| q.retired)) {
        Some(l) => Err(ToolError::invalid(format!("{l} is retired: it no longer holds, and no page links it"), vec![])),
        None => Ok(()),
    }
}

pub fn put_page(store: &mut Store, page: Page) -> Result<(Page, Changed), ToolError> {
    invalid_if(validate_page(&page), "page")?;
    refuse_retired_links(store, &page.links)?;
    store.pages.insert(page.slug.clone(), page.clone());
    Ok((page.clone(), Changed::Page(page.slug)))
}

/// A partial change to a page. Only given fields change; links are added at
/// the end in the order given, never duplicated, and removing an absent
/// link is not an error.
#[derive(Default)]
pub struct PagePatch {
    pub title: Option<String>,
    pub body: Option<String>,
    pub add_links: Vec<Reference>,
    pub remove_links: Vec<Reference>,
}

pub fn patch_page(store: &mut Store, r: &Reference, patch: PagePatch) -> Result<(Page, Changed), ToolError> {
    if r.kind != Kind::Page {
        return Err(ToolError::invalid(format!("{r} is not a page"), vec![]));
    }
    let existing = store.pages.get(&r.id).ok_or_else(|| ToolError::not_found(r))?;
    let mut page = existing.clone();
    if let Some(t) = patch.title {
        page.title = t;
    }
    if let Some(b) = patch.body {
        page.body = b;
    }
    page.links.retain(|l| !patch.remove_links.contains(l));
    refuse_retired_links(store, &patch.add_links)?;
    for l in patch.add_links {
        if !page.links.contains(&l) {
            page.links.push(l);
        }
    }
    invalid_if(validate_page(&page), "page")?;
    store.pages.insert(page.slug.clone(), page.clone());
    Ok((page, Changed::Page(r.id.clone())))
}

/// `decisions` on the input is ignored; the existing ones are kept.
pub fn put_requirement(store: &mut Store, mut req: Requirement) -> Result<(Requirement, Changed), ToolError> {
    if store.requirements.get(&req.slug).is_some_and(|r| r.retired) {
        return Err(ToolError::invalid(format!("req:{} is retired: it no longer holds and is not rewritten", req.slug), vec![]));
    }
    req.decisions = store.requirements.get(&req.slug).map(|r| r.decisions.clone()).unwrap_or_default();
    req.retired = false;
    invalid_if(validate_requirement(&req), "requirement")?;
    store.requirements.insert(req.slug.clone(), req.clone());
    Ok((req.clone(), Changed::Requirement(req.slug)))
}

pub struct NewTicket {
    pub title: String,
    pub kind: TicketKind,
    pub priority: Priority,
    pub touches: Vec<Reference>,
    pub relies_on: Vec<Reference>,
    pub body: String,
}

/// The next ticket id: one past the highest all-digit id in the store,
/// zero-padded to four digits. Tickets filed before ids were generated keep
/// their names and are not counted.
pub fn next_ticket_id(store: &Store) -> String {
    let max = store.tickets.keys().filter_map(|k| k.parse::<u64>().ok()).max().unwrap_or(0);
    format!("{:04}", max + 1)
}

pub fn file_ticket(store: &mut Store, t: NewTicket) -> Result<(Ticket, Changed), ToolError> {
    let slug = next_ticket_id(store);
    let ticket = Ticket { slug, title: t.title, kind: t.kind, priority: t.priority, status: Status::Open, touches: t.touches, relies_on: t.relies_on, body: t.body, decision: None };
    invalid_if(validate_ticket(&ticket), "ticket")?;
    store.tickets.insert(ticket.slug.clone(), ticket.clone());
    Ok((ticket.clone(), Changed::Ticket(ticket.slug)))
}

#[derive(Default)]
pub struct TicketPatch {
    pub title: Option<String>,
    pub priority: Option<Priority>,
    pub touches: Option<Vec<Reference>>,
    pub relies_on: Option<Vec<Reference>>,
    pub body: Option<String>,
}

fn ticket_mut<'a>(store: &'a mut Store, r: &Reference) -> Result<&'a mut Ticket, ToolError> {
    if r.kind != Kind::Ticket {
        return Err(ToolError::invalid(format!("{r} is not a ticket"), vec![]));
    }
    store.tickets.get_mut(&r.id).ok_or_else(|| ToolError::not_found(r))
}

pub fn update_ticket(store: &mut Store, r: &Reference, patch: TicketPatch) -> Result<(Ticket, Changed), ToolError> {
    let existing = ticket_mut(store, r)?;
    if existing.status == Status::Closed {
        return Err(ToolError::conflict(format!("{r} is closed")));
    }
    let mut t = existing.clone();
    if let Some(v) = patch.title {
        t.title = v;
    }
    if let Some(v) = patch.priority {
        t.priority = v;
    }
    if let Some(v) = patch.touches {
        t.touches = v;
    }
    if let Some(v) = patch.relies_on {
        t.relies_on = v;
    }
    if let Some(v) = patch.body {
        t.body = v;
    }
    invalid_if(validate_ticket(&t), "ticket")?;
    *existing = t.clone();
    Ok((t, Changed::Ticket(r.id.clone())))
}

pub fn start_ticket(store: &mut Store, r: &Reference) -> Result<(Ticket, Changed), ToolError> {
    let t = ticket_mut(store, r)?;
    if t.status != Status::Open {
        return Err(ToolError::conflict(format!("{r} is {:?}, not open", t.status).to_lowercase()));
    }
    t.status = Status::Active;
    let t = t.clone();
    store.current = Some(r.id.clone());
    Ok((t, Changed::Ticket(r.id.clone())))
}

pub fn close_ticket(store: &mut Store, r: &Reference, decision: Decision) -> Result<(Ticket, Changed), ToolError> {
    let existing = ticket_mut(store, r)?;
    if existing.status == Status::Closed {
        return Err(ToolError::conflict(format!("{r} is already closed")));
    }
    // An answer waiting for its change closes when that change lands, never by hand.
    if existing.kind != TicketKind::Change && existing.status == Status::Landing {
        return Err(ToolError::conflict(format!("{r} is an answer waiting for the change that applies it: it closes when that change lands; write a new answer with when: landed")));
    }
    let mut t = existing.clone();
    t.status = Status::Closed;
    t.body = String::new();
    t.decision = Some(decision);
    invalid_if(validate_ticket(&t), "ticket")?;
    *existing = t.clone();
    if store.current.as_deref() == Some(r.id.as_str()) {
        store.current = None;
    }
    Ok((t, Changed::Ticket(r.id.clone())))
}

/// Write a decision ahead of a landing: an active ticket becomes landing and
/// keeps its body, its proposal and its start; a landing one has its
/// decision replaced. A gap or question waits so only for the change that
/// applies its answer: a change not closed must already rely on it, it holds
/// no proposal, and it stops being the current ticket, since nothing of it
/// lands by a commit.
pub fn decide_ticket(store: &mut Store, r: &Reference, decision: Decision) -> Result<(Ticket, Changed), ToolError> {
    let mut t = ticket_mut(store, r)?.clone();
    if !t.status.is_live() {
        return Err(ToolError::conflict(format!("{r} is {}, not active: only a ticket being worked waits for its change to land", t.status.name())));
    }
    let answer = t.kind != TicketKind::Change;
    if answer {
        let kind = format!("{:?}", t.kind).to_lowercase();
        if !applying(store, &r.id).any(|c| c.status != Status::Closed) {
            return Err(ToolError::invalid(format!("{r} is a {kind} no change relies on: file the change that applies its answer first, relying on it, then close {r} with when: landed; an answer that needs no work closes at once, without when"), vec![]));
        }
        if store.proposals.get(&r.id).is_some_and(|p| !(p.pages.is_empty() && p.requirements.is_empty() && p.deleted.is_empty())) {
            return Err(ToolError::invalid(format!("{r} holds a proposal: an answer's spec belongs in the change that applies it, so move it there, or close {r} at once"), vec![]));
        }
    }
    t.status = Status::Landing;
    t.decision = Some(decision);
    invalid_if(validate_ticket(&t), "ticket")?;
    store.tickets.insert(r.id.clone(), t.clone());
    if !answer {
        return Ok((t, Changed::Ticket(r.id.clone())));
    }
    // It is no longer worked: what names it as worked goes first, so a writer
    // stopped part way leaves an active answer, not a waiting one still named.
    let mut changes = Vec::new();
    if store.current.as_deref() == Some(r.id.as_str()) {
        store.current = None;
        changes.push(Changed::Current);
    }
    store.proposals.remove(&r.id);
    store.starts.remove(&r.id);
    changes.extend([Changed::Proposed(r.id.clone()), Changed::Closed(r.id.clone())]);
    Ok((t, Changed::Many(changes)))
}

/// The changes, of any status, that rely on a gap or question and so apply its answer.
fn applying<'a>(store: &'a Store, slug: &str) -> impl Iterator<Item = &'a Ticket> {
    let r = Reference::ticket(slug);
    store.tickets.values().filter(move |c| c.kind == TicketKind::Change && c.relies_on.contains(&r))
}

/// Keep a change no lower priority than the gaps and questions it relies on
/// that are not closed, after `slug` was filed (`before` none) or edited from
/// priority `before`. A change lowered lowers each such answer it now sits
/// below to the most urgent of the changes not closed that rely on it; then
/// a change below any such answer, filed, newly relying on it or held up by
/// another change, is raised to it. A gap or question raised, or filed when a
/// change already names it, raises every change not closed that relies on it. Answers the slugs whose priority
/// moved beyond what was asked, each once.
pub fn keep_pace(store: &mut Store, slug: &str, before: Option<Priority>) -> (Vec<String>, Changed) {
    let mut moved: Vec<String> = Vec::new();
    let Some(t) = store.tickets.get(slug).filter(|t| t.status != Status::Closed).cloned() else {
        return (moved, Changed::Many(vec![]));
    };
    let mut set = |store: &mut Store, s: &str, p: Priority| {
        if let Some(x) = store.tickets.get_mut(s) {
            x.priority = p;
            if !moved.iter().any(|m| m == s) {
                moved.push(s.to_string());
            }
        }
    };
    if t.kind == TicketKind::Change {
        let answers = |store: &Store| -> Vec<(String, Priority)> {
            t.relies_on
                .iter()
                .filter(|r| r.kind == Kind::Ticket)
                .filter_map(|r| store.tickets.get(&r.id))
                .filter(|a| a.kind != TicketKind::Change && a.status != Status::Closed)
                .map(|a| (a.slug.clone(), a.priority))
                .collect()
        };
        if before.is_some_and(|b| t.priority > b) {
            for (a, p) in answers(store) {
                // Lowered with the change, but never below another change still relying on it.
                let floor = applying(store, &a).filter(|c| c.status != Status::Closed).map(|c| c.priority).min().unwrap_or(t.priority);
                if p < t.priority && floor > p {
                    set(store, &a, floor);
                }
            }
        }
        if let Some(top) = answers(store).into_iter().map(|(_, p)| p).min().filter(|top| *top < t.priority) {
            set(store, slug, top);
        }
    } else if before.is_none_or(|b| t.priority < b) {
        let behind: Vec<String> = applying(store, slug).filter(|c| c.status != Status::Closed && c.priority > t.priority).map(|c| c.slug.clone()).collect();
        for c in behind {
            set(store, &c, t.priority);
        }
    }
    let changed = Changed::Many(moved.iter().map(|s| Changed::Ticket(s.clone())).collect());
    (moved, changed)
}

/// Why a change stopped waiting to apply the answers it relies on.
pub enum Settling<'a> {
    /// It closed, and so applied them.
    Closed,
    /// It went away without landing, for the reason given: deleted, or no
    /// longer relying on them.
    Gone(&'a str),
}

/// The gaps and questions a settle closed and reopened, by slug.
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct Settled {
    pub closed: Vec<String>,
    pub reopened: Vec<String>,
}

impl Settled {
    pub fn is_empty(&self) -> bool {
        self.closed.is_empty() && self.reopened.is_empty()
    }
}

/// Settle the answers among `answers` that wait, landing, for the changes
/// applying them, once no change relying on one is left open. After a
/// change closed, each such answer closes with its own decision, since that
/// change applied it. After one went away, it closes if a change relying on
/// it closed on or after the day of its answer, and otherwise reopens for
/// re-examination, its answer and why moved into its body, cut to the body's
/// limit, since an open ticket carries no decision.
pub fn settle(store: &mut Store, answers: &[Reference], how: Settling) -> (Settled, Changed) {
    let mut out = Settled::default();
    let mut changes = Vec::new();
    for r in answers {
        let Some(t) = store.tickets.get(&r.id).filter(|t| r.kind == Kind::Ticket && t.kind != TicketKind::Change && t.status == Status::Landing).cloned() else {
            continue;
        };
        let Some(answer) = t.decision.clone() else { continue };
        if applying(store, &t.slug).any(|c| c.status != Status::Closed) {
            continue;
        }
        let applied = match how {
            Settling::Closed => true,
            Settling::Gone(_) => applying(store, &t.slug).any(|c| c.status == Status::Closed && c.decision.as_ref().is_some_and(|d| d.date >= answer.date)),
        };
        // Nothing may still name it as worked, whatever an interrupted writer left.
        if store.current.as_deref() == Some(t.slug.as_str()) {
            store.current = None;
            changes.push(Changed::Current);
        }
        if store.proposals.remove(&t.slug).is_some() {
            changes.push(Changed::Proposed(t.slug.clone()));
        }
        if applied {
            let mut closed = t.clone();
            closed.status = Status::Closed;
            closed.body = String::new();
            store.tickets.insert(t.slug.clone(), closed);
            store.starts.remove(&t.slug);
            changes.push(Changed::Closed(t.slug.clone()));
            out.closed.push(t.slug.clone());
        } else {
            let why = match how {
                Settling::Gone(why) => why,
                Settling::Closed => "its change closed",
            };
            let mut open = t.clone();
            open.status = Status::Open;
            open.decision = None;
            open.body = reopened_body(&t.body, &answer, why);
            store.tickets.insert(t.slug.clone(), open);
            store.starts.remove(&t.slug);
            changes.push(Changed::Closed(t.slug.clone()));
            out.reopened.push(t.slug.clone());
        }
    }
    (out, Changed::Many(changes))
}

/// A reopened answer's body: what it said, then the answer it had and why
/// nothing applied it, cut at the end to the body's limit.
fn reopened_body(body: &str, answer: &Decision, why: &str) -> String {
    let full = format!("{body}\n\n**Reopened.** Answered {}, then {why}, so no change applied the answer. It was:\n\n{}", answer.date, answer.text);
    let max = crate::store::TICKET_BODY_MAX;
    if full.chars().count() <= max {
        return full;
    }
    let mut cut: String = full.chars().take(max - 1).collect();
    cut.push('…');
    cut
}

pub fn delete_node(store: &mut Store, r: &Reference) -> Result<Changed, ToolError> {
    // Decisions are history: a decided requirement is retired, and a closed ticket stays.
    match r.kind {
        Kind::Req if store.requirements.get(&r.id).is_some_and(|q| !q.decisions.is_empty()) => {
            return Err(ToolError::invalid(format!("{r} has decisions, which are never removed; retire it with retire_node instead"), vec![]));
        }
        Kind::Ticket if store.tickets.get(&r.id).is_some_and(|t| t.status == Status::Closed) => {
            return Err(ToolError::invalid(format!("{r} is closed: a closed ticket is history, already off the board and the lints, and is not deleted"), vec![]));
        }
        // An answer waiting for its change carries the person's decision: it closes or reopens with that change.
        Kind::Ticket if store.tickets.get(&r.id).is_some_and(|t| t.kind != TicketKind::Change && t.status == Status::Landing) => {
            return Err(ToolError::invalid(format!("{r} is an answer waiting for the change that applies it, and carries the person's decision: it closes when that change lands, or reopens if the change goes away, and is not deleted"), vec![]));
        }
        _ => {}
    }
    let removed = match r.kind {
        Kind::Page => store.pages.remove(&r.id).is_some(),
        Kind::Req => store.requirements.remove(&r.id).is_some(),
        Kind::Ticket => store.tickets.remove(&r.id).is_some(),
        _ => return Err(ToolError::invalid(format!("{r} is derived from the repository and cannot be deleted"), vec![])),
    };
    if !removed {
        return Err(ToolError::not_found(r));
    }
    if r.kind == Kind::Ticket {
        // Its start and proposal go with it, and it stops being current.
        store.starts.remove(&r.id);
        store.proposals.remove(&r.id);
        if store.current.as_deref() == Some(r.id.as_str()) {
            store.current = None;
            return Ok(Changed::Many(vec![Changed::Deleted(r.clone()), Changed::Current]));
        }
    }
    Ok(Changed::Deleted(r.clone()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::graph::fixtures::store;

    /// # Test contract
    ///
    /// ```json
    /// {"test": "put-page-creates-and-replaces", "checks": ["crates/core/src/ops#put-page"]}
    /// ```
    #[test]
    fn put_page_creates_and_replaces() {
        let mut s = store();
        let page = |body: &str| Page { slug: "glossary".into(), title: "Glossary".into(), body: body.into(), links: vec![] };
        put_page(&mut s, page("first")).unwrap();
        let (again, _) = put_page(&mut s, page("second")).unwrap();
        assert_eq!(again.body, "second", "replacing by slug is not a conflict");
        assert_eq!(s.pages["glossary"].body, "second");
    }

    /// # Test contract
    ///
    /// ```json
    /// {"test": "patch-page-is-partial", "checks": ["crates/core/src/ops#patch-is-partial"]}
    /// ```
    #[test]
    fn patch_page_is_partial() {
        let mut s = store();
        let r = Reference::page("git-readiness");
        let before = s.pages["git-readiness"].clone();
        let (p, _) = patch_page(
            &mut s,
            &r,
            PagePatch { add_links: vec![Reference::page("orphan"), Reference::req("stale")], remove_links: vec![Reference::req("nope"), Reference::module("src/a/ready")], ..Default::default() },
        )
        .unwrap();
        assert_eq!(p.title, before.title);
        assert_eq!(p.body, before.body);
        // stale was already present: not duplicated; ready removed; orphan appended.
        assert_eq!(p.links.iter().map(|l| l.to_string()).collect::<Vec<_>>(), vec!["req:ready-only-with-commit", "req:stale", "page:orphan"]);
        let (p, _) = patch_page(&mut s, &r, PagePatch { title: Some("Readiness".into()), ..Default::default() }).unwrap();
        assert_eq!(p.title, "Readiness");
        assert_eq!(p.links.len(), 3);
        // Invalid result changes nothing.
        let e = patch_page(&mut s, &r, PagePatch { add_links: vec![Reference::ticket("x")], ..Default::default() }).unwrap_err();
        assert_eq!(e.code(), "invalid");
        assert_eq!(s.pages["git-readiness"].links.len(), 3);
        assert_eq!(patch_page(&mut s, &Reference::page("nope"), PagePatch::default()).unwrap_err().code(), "not_found");
    }

    /// # Test contract
    ///
    /// ```json
    /// {"test": "put-requirement-keeps-decisions", "checks": ["crates/core/src/ops/decisions#decisions-append-only", "crates/core/src/ops#put-requirement"]}
    /// ```
    #[test]
    fn put_requirement_keeps_decisions() {
        let mut s = store();
        let mut r = s.requirements["ready-only-with-commit"].clone();
        r.decisions = vec![];
        r.title = "Changed".into();
        let (out, _) = put_requirement(&mut s, r).unwrap();
        assert_eq!(out.decisions.len(), 1);
        assert_eq!(out.title, "Changed");
    }

    /// # Test contract
    ///
    /// ```json
    /// {"test": "ticket-lifecycle", "checks": ["crates/core/src/ops#close-keeps-only-decision", "crates/core/src/ops#update-ticket", "crates/core/src/ops#start-ticket"]}
    /// ```
    #[test]
    fn ticket_lifecycle() {
        let mut s = store();
        let r = Reference::ticket("phase-62");
        assert_eq!(start_ticket(&mut s, &r).unwrap().0.status, Status::Active);
        assert_eq!(start_ticket(&mut s, &r).unwrap_err().code(), "conflict");
        let (t, _) = update_ticket(&mut s, &r, TicketPatch { priority: Some(Priority::Later), ..Default::default() }).unwrap();
        assert_eq!(t.priority, Priority::Later);
        let (t, _) = close_ticket(&mut s, &r, Decision::new("2026-09-27", "Shipped.")).unwrap();
        assert_eq!(t.status, Status::Closed);
        assert_eq!(t.body, "");
        assert_eq!(update_ticket(&mut s, &r, TicketPatch::default()).unwrap_err().code(), "conflict");
        assert_eq!(close_ticket(&mut s, &r, Decision::new("2026-09-27", "again")).unwrap_err().code(), "conflict");
    }

    /// # Test contract
    ///
    /// ```json
    /// {"test": "file-ticket-generates-ids", "checks": ["crates/core/src/ops#file-ticket"]}
    /// ```
    #[test]
    fn file_ticket_generates_ids() {
        let mut s = store();
        // Existing tickets have names, not numbers: the first generated id is 0001.
        assert_eq!(next_ticket_id(&s), "0001");
        let t = || NewTicket { title: "T".into(), kind: TicketKind::Change, priority: Priority::Next, touches: vec![], relies_on: vec![], body: "b".into() };
        let (a, _) = file_ticket(&mut s, t()).unwrap();
        let (b, _) = file_ticket(&mut s, t()).unwrap();
        assert_eq!(a.slug, "0001");
        assert_eq!(b.slug, "0002");
        assert!(crate::store::is_slug(&a.slug));
        // Deleting the latest does not reuse its number's predecessor's successor incorrectly.
        s.tickets.remove("0002");
        assert_eq!(next_ticket_id(&s), "0002");
        // Ten thousand tickets still get distinct ids, just longer.
        s.tickets.insert("9999".into(), Ticket { slug: "9999".into(), ..a.clone() });
        assert_eq!(next_ticket_id(&s), "10000");
    }

    /// # Test contract
    ///
    /// ```json
    /// {"test": "delete-only-store-kinds", "checks": ["crates/core/src/ops#delete-node"]}
    /// ```
    #[test]
    fn delete_only_store_kinds() {
        let mut s = store();
        assert_eq!(delete_node(&mut s, &Reference::module("src/a/ready")).unwrap_err().code(), "invalid");
        assert!(delete_node(&mut s, &Reference::page("orphan")).is_ok());
        assert_eq!(delete_node(&mut s, &Reference::page("orphan")).unwrap_err().code(), "not_found");
        // A requirement with decisions is retired instead, and a closed ticket stays.
        let e = delete_node(&mut s, &Reference::req("ready-only-with-commit")).unwrap_err();
        assert!(e.code() == "invalid" && e.to_string().contains("retire_node"), "{e}");
        assert!(s.requirements.contains_key("ready-only-with-commit"));
        assert_eq!(delete_node(&mut s, &Reference::ticket("old")).unwrap_err().code(), "invalid");
        assert!(s.tickets.contains_key("old"));
        assert!(delete_node(&mut s, &Reference::req("stale")).is_ok(), "a requirement with no decision can go");
        assert!(delete_node(&mut s, &Reference::ticket("later-1")).is_ok(), "an active ticket can go");
    }

    /// # Test contract
    ///
    /// ```json
    /// {"test": "a-proposal-leaves-the-truth-alone", "checks": ["crates/core/src/ops#proposal-writes", "crates/core/src/ops#current-ticket"]}
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
        close_ticket(&mut s, &Reference::ticket("later-1"), Decision { date: "2026-10-02".into(), text: "Done.".into(), supersedes: vec![], adopted: false }).unwrap();
        assert_eq!(s.current.as_deref(), Some("phase-62"), "closing another ticket leaves it");
        close_ticket(&mut s, &Reference::ticket("phase-62"), Decision { date: "2026-10-02".into(), text: "Done.".into(), supersedes: vec![], adopted: false }).unwrap();
        assert_eq!(s.current, None);
    }

    /// # Test contract
    ///
    /// ```json
    /// {"test": "a-close-applies-the-proposal-unless-the-truth-moved", "checks": ["crates/core/src/ops#close-applies-proposal"]}
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
    /// {"test": "a-rename-and-a-ticket-deletion-reach-proposals", "checks": ["crates/core/src/ops#rename", "crates/core/src/ops#delete-node"]}
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
        start_ticket(&mut s, &Reference::ticket("phase-62")).unwrap();
        s.starts.insert("phase-62".into(), Start { ticket: "phase-62".into(), fingerprints: Default::default() });
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
                status: Status::Open,
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
    /// {"test": "a-landing-ticket-is-renamed-and-reordered-like-an-active-one", "checks": ["crates/core/src/ops#rename", "crates/core/src/ops#reorder", "crates/core/src/ops#delete-node"]}
    /// ```
    #[test]
    fn a_landing_ticket_is_renamed_and_reordered_like_an_active_one() {
        let mut s = crate::graph::fixtures::store();
        // later-1 touches page:orphan; its decision is written and it waits to land.
        let t = s.tickets.get_mut("later-1").unwrap();
        t.status = Status::Landing;
        t.decision = Some(Decision::new("2026-10-03", "Done."));
        assert!(validate_ticket(&s.tickets["later-1"]).is_empty());
        // A rename reaches its references, as it reaches an active ticket's.
        rename_node(&mut s, &Reference::page("orphan"), "kept").unwrap();
        assert_eq!(s.tickets["later-1"].touches, vec![Reference::page("kept")]);
        // It keeps its place in its band's order.
        let (order, _) = reorder(&mut s, Priority::Later, &[Reference::ticket("later-1")]).unwrap();
        assert_eq!(order, vec!["later-1".to_string()]);
        // And it is not history yet: it can be deleted, its decision with it.
        delete_node(&mut s, &Reference::ticket("later-1")).unwrap();
        assert!(!s.tickets.contains_key("later-1"));
    }
}
