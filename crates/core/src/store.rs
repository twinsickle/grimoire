//! # Module contract
//!
//! ```json
//! {"purpose":"Hold the store's pages, requirements, tickets and baseline as values, and read a ticket's proposal over them; validating each record against the schema has a module beneath.","criteria":[{"id":"settles-proposals","statement":"settle_proposals reads a proposal written before decisions left the record as this build keeps it: each proposed requirement keeps no decisions, and those of its decisions the truth does not hold, by date and text, become decisions the proposal adds, with no id and ahead of any it already adds, each superseding only what it named among the decisions the truth holds; settling a settled store changes nothing."}]}
//! ```

use std::collections::BTreeMap;

use serde::Serialize;

use crate::model::*;

mod validate;

pub use validate::*;

/// The store as a value: the config and every record, keyed by slug.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Store {
    pub config: StoreConfig,
    pub pages: BTreeMap<String, Page>,
    pub requirements: BTreeMap<String, Requirement>,
    pub tickets: BTreeMap<String, Ticket>,
    /// The findings accepted as inherited at adoption; none until `adopt`.
    pub baseline: Option<Baseline>,
    /// The snapshot taken when each brewing or simmering ticket started, by slug.
    pub starts: BTreeMap<String, Start>,
    /// Each brewing or simmering ticket's proposed change to the truth, by slug.
    pub proposals: BTreeMap<String, Proposal>,
    /// The ticket a write goes into when it names none: the one last started.
    pub current: Option<String>,
    /// Each judged recipe's judgements, oldest first, by recipe id (req:store-judgement-record).
    pub judgements: BTreeMap<String, Vec<Judgement>>,
    /// The recipes the main line last sent a team's server, for its site; none until it first sends them (req:recipe-main-line-snapshot).
    pub main_line_recipes: Option<MainLineRecipes>,
    /// Who has each ticket being worked checked out and since when, by slug, apart from its record (req:ticket-checked-out).
    pub checked_out: BTreeMap<String, Vec<CheckedOut>>,
}

impl Store {
    /// The store as `ticket`'s proposal would leave it: proposed records in
    /// place of the truth's, deleted ones gone. The truth itself when the
    /// ticket has no proposal.
    pub fn applied(&self, ticket: &str) -> Store {
        let mut s = self.clone();
        if let Some(p) = self.proposals.get(ticket) {
            for (slug, q) in &p.pages {
                s.pages.insert(slug.clone(), q.record.clone());
            }
            for slug in p.requirements.keys().chain(p.decisions.keys()) {
                if let Some(q) = with_decisions(self.requirements.get(slug), p, slug, Bottling::Waiting) {
                    s.requirements.insert(slug.clone(), q);
                }
            }
            for key in p.deleted.keys() {
                if let Ok(r) = key.parse::<Reference>() {
                    match r.kind {
                        Kind::Page => {
                            s.pages.remove(&r.id);
                        }
                        Kind::Req => {
                            s.requirements.remove(&r.id);
                        }
                        _ => {}
                    }
                }
            }
            // Its edits to other tickets' references, on the tickets they edit; one since closed or gone takes none.
            for (slug, q) in &p.tickets {
                if let Some(t) = s.tickets.get_mut(slug).filter(|t| t.status != Status::Bottled) {
                    t.touches = q.record.touches.clone();
                    t.relies_on = q.record.relies_on.clone();
                }
            }
        }
        s
    }

    /// Who has ticket `slug` checked out, while it is being worked: the list as kept, empty when who is
    /// unknown, as for one started before it was recorded; none for a ticket not being worked (req:ticket-checked-out).
    pub fn checked_out_of(&self, slug: &str) -> Option<Vec<CheckedOut>> {
        self.tickets.get(slug).filter(|t| t.is_worked()).map(|_| self.checked_out.get(slug).cloned().unwrap_or_default())
    }

    /// For each ticket whose references a proposal would change at its
    /// close, the tickets proposing it, in slug order: an edit equal to the
    /// truth's lists, or to a ticket since closed or gone, changes nothing.
    pub fn ticket_drafts(&self) -> BTreeMap<String, Vec<String>> {
        let mut out: BTreeMap<String, Vec<String>> = BTreeMap::new();
        for p in self.proposals.values() {
            for (slug, q) in &p.tickets {
                if self.tickets.get(slug).is_some_and(|t| t.status != Status::Bottled && TicketReferences::of(t) != q.record) {
                    out.entry(slug.clone()).or_default().push(p.ticket.clone());
                }
            }
        }
        out
    }
}

impl Default for StoreConfig {
    fn default() -> Self {
        StoreConfig {
            version: StoreConfig::CURRENT_VERSION,
            repository: String::new(),
            roots: Roots::default(),
            scope: Scope::default(),
            order: Order::default(),
            languages: Vec::new(),
            site: Vec::new(),
            large_file: None,
            plain_file: None,
            recipe_kinds: Default::default(),
        }
    }
}

/// A store file that did not parse or violated a field rule. Carried into the
/// graph so the `malformed` lint can report it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Malformed {
    /// Store-relative path, `/`-separated.
    pub path: String,
    pub reason: String,
}

/// Whether a requirement read with its decisions is taken as it waits in a proposal, the
/// decisions it adds with no id, or as it is bottled, each taking the next id.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Bottling {
    Waiting,
    Bottled,
}

/// Requirement `slug` as proposal `p` leaves it over `truth`, the truth's record now: the
/// record it proposes, or the truth's, with the truth's decisions, the supersessions it links
/// added to them, and after them the decisions it adds (req:close-applies-proposal). Bottled, each
/// added decision takes the next id, and a retirement's, the last, supersedes every decision
/// before it. `None` when there is no record to hold them.
pub fn with_decisions(truth: Option<&Requirement>, p: &Proposal, slug: &str, bottling: Bottling) -> Option<Requirement> {
    let mut q = match (p.requirements.get(slug), truth) {
        (Some(proposed), _) => proposed.record.clone(),
        (None, Some(t)) => t.clone(),
        (None, None) => return None,
    };
    q.decisions = truth.map(|t| t.decisions.clone()).unwrap_or_default();
    let Some(adds) = p.decisions.get(slug) else { return Some(q) };
    for (id, ids) in &adds.linked {
        if let Some(d) = q.decisions.iter_mut().find(|d| d.id == Some(*id)) {
            for x in ids {
                if !d.supersedes.contains(x) {
                    d.supersedes.push(*x);
                }
            }
            d.supersedes.sort_unstable();
        }
    }
    for d in &adds.added {
        let id = (bottling == Bottling::Bottled).then(|| q.next_id());
        q.decisions.push(Decision { id, ..d.clone() });
    }
    // A retirement supersedes every decision before it: as it is bottled, and read through the ticket, every one bottled by then.
    let retiring = q.retired && !truth.is_some_and(|t| t.retired);
    if retiring && !adds.added.is_empty() {
        let earlier: Vec<u32> = q.decisions[..q.decisions.len() - 1].iter().filter_map(|d| d.id).collect();
        if let Some(last) = q.decisions.last_mut() {
            last.supersedes = earlier;
        }
    }
    Some(q)
}

impl Store {
    pub fn new(config: StoreConfig) -> Self {
        Store { config, ..Default::default() }
    }

    /// Read every proposal written before decisions left the record (req:ticket-proposal): a
    /// proposed requirement holding decisions keeps none, and those the truth does not hold, by
    /// date and text, become decisions the proposal adds, ahead of any it already adds. Whatever
    /// the truth holds the proposal never had to carry, so nothing of it is lost.
    pub fn settle_proposals(&mut self) {
        for p in self.proposals.values_mut() {
            for (slug, q) in p.requirements.iter_mut().filter(|(_, q)| !q.record.decisions.is_empty()) {
                let truth = self.requirements.get(slug);
                let held = |d: &Decision| truth.is_some_and(|t| t.decisions.iter().any(|x| x.date == d.date && x.text == d.text));
                let record = std::mem::take(&mut q.record.decisions);
                // The positions of its own decisions: one superseding another of its own named a decision with no id yet, so names nothing.
                let own_positions: Vec<u32> = (1u32..).zip(&record).filter(|(_, d)| !held(d)).map(|(p, _)| p).collect();
                let own: Vec<Decision> =
                    record.into_iter().filter(|d| !held(d)).map(|d| Decision { id: None, supersedes: d.supersedes.iter().copied().filter(|p| !own_positions.contains(p)).collect(), ..d }).collect();
                if !own.is_empty() {
                    let adds = p.decisions.entry(slug.clone()).or_default();
                    adds.added.splice(0..0, own);
                }
            }
        }
    }

    /// Whether a stored node exists. Derived kinds always return `false`.
    pub fn contains(&self, r: &Reference) -> bool {
        match r.kind {
            Kind::Page => self.pages.contains_key(&r.id),
            Kind::Req => self.requirements.contains_key(&r.id),
            Kind::Ticket => self.tickets.contains_key(&r.id),
            _ => false,
        }
    }
}
