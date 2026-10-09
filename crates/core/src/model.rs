//! # Module contract
//!
//! ```json
//! {"purpose":"Define the store records that every other part of core is written against; the node kinds and the reference form, the store config, the records the extraction derives and the recipes' records each have a module beneath.","criteria":[{"id":"baseline-record","statement":"A baseline is a date, a required reason and a list of finding identities (rule, check, ref and optional edge, never the reason text); a store without one behaves as one with an empty list."},{"id":"strict-records","statement":"Store records reject unknown fields when deserialised so a misspelled field is an error, not silently dropped."},{"id":"status-names","statement":"A ticket's status is one of inscribed, brewing, simmering and bottled, and is written by that name and read by it or by its name before ticket 0168, open, active, landing or closed, as a start's simmering_in is read as closed_in too, so a store written before the rename loads unchanged."},{"id":"ticket-bounded","statement":"A ticket has exactly a slug, title, kind, priority, status, touches, relies_on, body and decision, and no field for plans, notes, assignees or history."},{"id":"decisions-only-dates","statement":"Pages, requirements and tickets carry no created, modified or closed timestamps, and a store record carries no date but a person's choice: a decision's date, a baseline's date of adoption and a judgement's date; since when someone has a ticket checked out is state kept only while the ticket is worked, not a record of intent."},{"id":"page-record","statement":"A page is slug, title, body (Markdown) and links, which may name only pages, requirements, modules and recipes, in display order without duplicates."},{"id":"requirement-fields","statement":"A requirement is slug, title, a normative statement, scenarios of title/given/when/then, and covered_by naming only criteria, recipes and ingredients."},{"id":"requirement-decisions","statement":"A requirement's decisions are each of id, date, text, the ids of earlier decisions it supersedes and whether it was recorded as an adoption, in the order they were bottled."},{"id":"requirement-retired","statement":"A requirement carries whether it is retired."},{"id":"decision-id-position","statement":"A decision's id is written only where it differs from its position, 1 for the oldest, read as that position where absent, and null for one not yet bottled."},{"id":"decision-in-force","statement":"A decision is in force unless a later one supersedes it."},{"id":"requirement-described","statement":"A requirement is described when it has decisions in force and every one of them is an adoption."},{"id":"start-record","statement":"A start is a ticket's slug and a map from each node's reference to its fingerprint, with no date, and, once the ticket is simmering, the checkout it was closed when: bottled in, by repository path, absent otherwise, read under its former name closed_in too."},{"id":"checked-out-record","statement":"Who has a ticket checked out is exactly a name and when they took it, a non-empty time, written as RFC 3339 text, both required; it is kept by ticket slug apart from the ticket's record, so a ticket stays bounded."},{"id":"proposal-record","statement":"A proposal is a ticket's slug, the pages and requirements it proposes, each a record with the fingerprint of the truth's record it was written against or none for a new one, the pages and requirements it deletes by reference with that fingerprint, the other tickets whose references it edits, each as its touches and relies_on with the fingerprint of the truth's two lists, and for each requirement the decisions it adds, with no id, and the supersessions it links, by id, apart from any record it holds; the current record is the slug of the ticket a write goes into when it names none."}]}
//! ```

use serde::{Deserialize, Deserializer, Serialize, Serializer};

mod config;
mod derived;
mod recipes;
mod reference;

pub use config::*;
pub use derived::*;
pub use recipes::*;
pub use reference::*;

// ---------------------------------------------------------------------------
// Store records
// ---------------------------------------------------------------------------

/// A unit of intent.
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Page {
    pub slug: String,
    pub title: String,
    pub body: String,
    pub links: Vec<Reference>,
}

/// One explicit requirement in the OpenSpec shape.
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Requirement {
    pub slug: String,
    pub title: String,
    pub statement: String,
    pub scenarios: Vec<Scenario>,
    pub covered_by: Vec<Reference>,
    // In the order they reached the truth. Each carries its id, written only where it differs
    // from its position, so a store written before ids reads the same, and an older build reads
    // what this one writes while no decision has gone missing (req:store-requirement-record).
    #[serde(serialize_with = "write_decisions", deserialize_with = "read_decisions")]
    pub decisions: Vec<Decision>,
    /// Retired: it no longer holds. It stays readable with its decisions, the
    /// last saying why, but no page links it and it covers no criterion.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    #[cfg_attr(feature = "ts", ts(as = "Option<bool>", optional))]
    pub retired: bool,
}

impl Requirement {
    /// The ids of the decisions a later one supersedes.
    pub fn superseded(&self) -> std::collections::BTreeSet<u32> {
        self.decisions.iter().flat_map(|d| d.supersedes.iter().copied()).collect()
    }

    /// The decisions in force, oldest first: those no decision supersedes, one not yet bottled among them.
    pub fn in_force(&self) -> Vec<&Decision> {
        let superseded = self.superseded();
        self.decisions.iter().filter(|d| !d.id.is_some_and(|id| superseded.contains(&id))).collect()
    }

    /// The bottled decision whose id is `id`.
    pub fn decision(&self, id: u32) -> Option<&Decision> {
        self.decisions.iter().find(|d| d.id == Some(id))
    }

    /// The id the next decision to bottle takes: above every id it holds, so none is given twice.
    pub fn next_id(&self) -> u32 {
        self.decisions.iter().filter_map(|d| d.id).max().unwrap_or(0) + 1
    }

    /// The record as a proposal holds it: every field but its decisions, which wait apart.
    pub fn without_decisions(&self) -> Requirement {
        Requirement { decisions: Vec::new(), ..self.clone() }
    }

    /// Described, not decided: it has decisions in force and every one is an
    /// adoption, so it says what the code did when it was read, and nobody
    /// has chosen it since.
    pub fn described(&self) -> bool {
        let in_force = self.in_force();
        !in_force.is_empty() && in_force.iter().all(|d| d.adopted)
    }
}

/// A requirement's decisions as they are written: each one's `id` only where it differs from its
/// position, 1 for the oldest, and null for one not yet bottled, as a read through a proposal shows it.
fn write_decisions<S: Serializer>(decisions: &[Decision], s: S) -> Result<S::Ok, S::Error> {
    use serde::ser::{SerializeMap, SerializeSeq};
    struct Written<'a>(usize, &'a Decision);
    impl Serialize for Written<'_> {
        fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
            let Written(position, d) = *self;
            let mut m = s.serialize_map(None)?;
            match d.id {
                Some(id) if id as usize == position => {}
                id => m.serialize_entry("id", &id)?,
            }
            m.serialize_entry("date", &d.date)?;
            m.serialize_entry("text", &d.text)?;
            if !d.supersedes.is_empty() {
                m.serialize_entry("supersedes", &d.supersedes)?;
            }
            if d.adopted {
                m.serialize_entry("adopted", &true)?;
            }
            if d.withdrawn {
                m.serialize_entry("withdrawn", &true)?;
            }
            m.end()
        }
    }
    let mut seq = s.serialize_seq(Some(decisions.len()))?;
    for (i, d) in decisions.iter().enumerate() {
        seq.serialize_element(&Written(i + 1, d))?;
    }
    seq.end()
}

/// A requirement's decisions as they are read: one with no `id` takes its position, as every
/// decision written before ids, and one whose `id` is null has not been bottled.
fn read_decisions<'de, D: Deserializer<'de>>(d: D) -> Result<Vec<Decision>, D::Error> {
    #[derive(Deserialize)]
    #[serde(deny_unknown_fields)]
    struct Read {
        #[serde(default, deserialize_with = "present")]
        id: Option<Option<u32>>,
        date: String,
        text: String,
        #[serde(default)]
        supersedes: Vec<u32>,
        #[serde(default)]
        adopted: bool,
        #[serde(default)]
        withdrawn: bool,
    }
    fn present<'de, D: Deserializer<'de>>(d: D) -> Result<Option<Option<u32>>, D::Error> {
        Option::<u32>::deserialize(d).map(Some)
    }
    let read = Vec::<Read>::deserialize(d)?;
    Ok((1u32..)
        .zip(read)
        .map(|(position, r)| Decision { id: r.id.unwrap_or(Some(position)), date: r.date, text: r.text, supersedes: r.supersedes, adopted: r.adopted, withdrawn: r.withdrawn })
        .collect())
}

#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Scenario {
    pub title: String,
    pub given: String,
    pub when: String,
    pub then: String,
}

/// A dated line saying what was chosen and why.
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Decision {
    // A requirement's decision's id, said `d<n>`, given when it is bottled and never given twice; none on
    // a ticket's decision and on one waiting in a proposal. Among a requirement's decisions, absent
    // means the decision's position, 1 for the oldest, and null one not yet bottled.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[cfg_attr(feature = "ts", ts(as = "Option<Option<u32>>", optional))]
    pub id: Option<u32>,
    pub date: String,
    pub text: String,
    // The ids of earlier decisions of the same requirement this one replaces.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    #[cfg_attr(feature = "ts", ts(as = "Option<Vec<u32>>", optional))]
    pub supersedes: Vec<u32>,
    // Recorded at adoption: the requirement was read off the code and
    // describes what it did then, not what anyone chose. Set only when the
    // decision is recorded.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    #[cfg_attr(feature = "ts", ts(as = "Option<bool>", optional))]
    pub adopted: bool,
    // A change's decision to withdraw it: closed with nothing of it applied, since it is no
    // longer to be made (req:tool-withdraw-ticket). Only on a bottled change's decision.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    #[cfg_attr(feature = "ts", ts(as = "Option<bool>", optional))]
    pub withdrawn: bool,
}

/// A requirement's bottled decision as a node of the graph, `req:<slug>#d<id>`: derived from its
/// requirement on every read, never stored apart from it (req:decision-references).
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct DecisionNode {
    pub requirement: String,
    pub id: u32,
    pub date: String,
    pub text: String,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    #[cfg_attr(feature = "ts", ts(as = "Option<Vec<u32>>", optional))]
    pub supersedes: Vec<u32>,
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    #[cfg_attr(feature = "ts", ts(as = "Option<bool>", optional))]
    pub adopted: bool,
}

impl Decision {
    pub fn new(date: impl Into<String>, text: impl Into<String>) -> Self {
        Decision { id: None, date: date.into(), text: text.into(), supersedes: Vec::new(), adopted: false, withdrawn: false }
    }
}

#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum TicketKind {
    Change,
    Gap,
    Question,
}

#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Priority {
    Next,
    High,
    Later,
}

#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Status {
    /// Filed, and nobody is working it. Read as `open` too, its name before ticket 0168.
    #[serde(alias = "open")]
    Inscribed,
    /// Being worked. Read as `active` too.
    #[serde(alias = "active")]
    Brewing,
    /// Its decision is written and it waits for its change to reach the main
    /// line: brewing in every other respect, and bottled then. Read as `landing` too.
    #[serde(alias = "landing")]
    Simmering,
    /// Done, its decision kept. Read as `closed` too.
    #[serde(alias = "closed")]
    Bottled,
}

impl Status {
    /// Whether a ticket in this status holds a proposal and a start: it is brewing or simmering.
    pub fn is_live(self) -> bool {
        matches!(self, Status::Brewing | Status::Simmering)
    }
    pub fn name(self) -> &'static str {
        match self {
            Status::Inscribed => "inscribed",
            Status::Brewing => "brewing",
            Status::Simmering => "simmering",
            Status::Bottled => "bottled",
        }
    }
}

/// An intended change to the grimoire, or a gap or question about it.
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Ticket {
    pub slug: String,
    pub title: String,
    pub kind: TicketKind,
    pub priority: Priority,
    pub status: Status,
    pub touches: Vec<Reference>,
    pub relies_on: Vec<Reference>,
    pub body: String,
    pub decision: Option<Decision>,
}

impl Ticket {
    /// Whether the ticket is being worked: brewing, or a change simmering.
    /// Only such a ticket holds a proposal, a start and the current
    /// pointer; a gap or question waiting, simmering, for the change that
    /// applies its answer is not worked itself.
    pub fn is_worked(&self) -> bool {
        self.status == Status::Brewing || (self.status == Status::Simmering && self.kind == TicketKind::Change)
    }
}

/// A lint rule. Stored in a baseline, reported in every finding.
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Rule {
    Dangling,
    Uncovered,
    Malformed,
    Unspecified,
}

impl Rule {
    pub const ALL: [Rule; 4] = [Rule::Dangling, Rule::Uncovered, Rule::Malformed, Rule::Unspecified];

    /// The rule's name as it serialises, for keys in `counts`.
    pub fn name(self) -> &'static str {
        match self {
            Rule::Dangling => "dangling",
            Rule::Uncovered => "uncovered",
            Rule::Malformed => "malformed",
            Rule::Unspecified => "unspecified",
        }
    }
}

/// The edge a dangling finding is about.
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FindingEdge {
    pub from: Reference,
    pub to: Reference,
    pub relation: Relation,
}

/// What makes two findings the same one: rule, check, node and edge. The
/// check tells apart two problems on one node (a module unlinked and without
/// a contract); the reason text is left out because it carries parser
/// messages that change while the problem stays the same.
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FindingKey {
    pub rule: Rule,
    pub check: String,
    pub r#ref: Reference,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[cfg_attr(feature = "ts", ts(optional))]
    pub edge: Option<FindingEdge>,
}

/// `baseline.json`: the findings accepted as inherited at adoption.
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Baseline {
    pub date: String,
    pub reason: String,
    pub findings: Vec<FindingKey>,
}

/// `starts/<slug>.json`: the graph's fingerprints when a ticket started, so
/// its close can name the touched nodes that never changed. Undated, like
/// every record but a decision.
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Start {
    pub ticket: String,
    pub fingerprints: std::collections::BTreeMap<String, String>,
    /// The checkout, by its repository path, where the ticket was closed `when: bottled` and set
    /// simmering: it is that checkout's to bottle before the checkout takes another
    /// (req:one-ticket-per-branch). Read as `closed_in` too, its name before ticket 0168.
    #[serde(default, skip_serializing_if = "Option::is_none", alias = "closed_in")]
    #[cfg_attr(feature = "ts", ts(optional))]
    pub simmering_in: Option<String>,
}

/// One who has a ticket checked out, and since when: kept in `checked_out.json` by ticket slug, apart
/// from the ticket's record, only while the ticket is worked (req:ticket-checked-out). The time is
/// state the tool reports, as a copy's fetch time is, not a record of intent.
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CheckedOut {
    /// The name of the token that took it on a team; the person or machine on a store of its own.
    pub by: String,
    /// When, as RFC 3339 text in UTC.
    pub since: String,
}

/// `proposals/<ticket>.json`: a ticket's proposed change to the truth. The
/// pages and requirements as they will be once the ticket is bottled, each with
/// the fingerprint of the truth's record it was written against (none for a
/// record the truth does not have), and the records it deletes as marks
/// carrying that fingerprint. Applied to the truth when the ticket closes.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Proposal {
    pub ticket: String,
    #[serde(default, skip_serializing_if = "std::collections::BTreeMap::is_empty")]
    pub pages: std::collections::BTreeMap<String, Proposed<Page>>,
    #[serde(default, skip_serializing_if = "std::collections::BTreeMap::is_empty")]
    pub requirements: std::collections::BTreeMap<String, Proposed<Requirement>>,
    /// Deleted pages and requirements, by reference, with the fingerprint of
    /// the truth's record when the deletion was proposed.
    #[serde(default, skip_serializing_if = "std::collections::BTreeMap::is_empty")]
    pub deleted: std::collections::BTreeMap<String, String>,
    /// Other tickets' references as this ticket's edits leave them, by slug,
    /// each with the fingerprint of the truth's two lists it was written
    /// against: they reach the ticket when this one is bottled.
    #[serde(default, skip_serializing_if = "std::collections::BTreeMap::is_empty")]
    pub tickets: std::collections::BTreeMap<String, Proposed<TicketReferences>>,
    /// The decisions this ticket adds to requirements, by slug, kept apart from any record it
    /// holds: appended after the truth's when it is bottled, each taking its id then.
    #[serde(default, skip_serializing_if = "std::collections::BTreeMap::is_empty")]
    pub decisions: std::collections::BTreeMap<String, ProposedDecisions>,
}

impl Proposal {
    pub fn new(ticket: &str) -> Self {
        Proposal { ticket: ticket.to_string(), pages: Default::default(), requirements: Default::default(), deleted: Default::default(), tickets: Default::default(), decisions: Default::default() }
    }
    pub fn is_empty(&self) -> bool {
        self.pages.is_empty() && self.requirements.is_empty() && self.deleted.is_empty() && self.tickets.is_empty() && self.decisions.is_empty()
    }
}

/// What a proposal adds to one requirement's decisions: the decisions recorded under the ticket,
/// in the order they were recorded and with no id, and the supersessions linked under it, by
/// the id of the decision that supersedes to the ids it supersedes.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ProposedDecisions {
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub added: Vec<Decision>,
    #[serde(default, skip_serializing_if = "std::collections::BTreeMap::is_empty")]
    pub linked: std::collections::BTreeMap<u32, Vec<u32>>,
}

impl ProposedDecisions {
    pub fn is_empty(&self) -> bool {
        self.added.is_empty() && self.linked.is_empty()
    }
}

/// A ticket's references, its proposal state: what a proposal holds of a
/// ticket whose references it edits.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TicketReferences {
    pub touches: Vec<Reference>,
    pub relies_on: Vec<Reference>,
}

impl TicketReferences {
    pub fn of(t: &Ticket) -> TicketReferences {
        TicketReferences { touches: t.touches.clone(), relies_on: t.relies_on.clone() }
    }
}

/// A record as a proposal holds it, and what it was written against.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Proposed<T> {
    pub record: T,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub base: Option<String>,
}

/// `current.json`: the ticket a write goes into when it names none, the one
/// last started on this store.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Current {
    pub ticket: String,
}

#[cfg(test)]
mod tests {
    use super::*;

    /// # Test contract
    ///
    /// ```json
    /// {"test": "who-has-a-ticket-is-a-name-and-a-time", "checks": ["crates/core/src/model#checked-out-record"]}
    /// ```
    #[test]
    fn who_has_a_ticket_is_a_name_and_a_time() {
        let c: CheckedOut = serde_json::from_str(r#"{"by":"ana","since":"2026-10-08T14:10:00Z"}"#).unwrap();
        assert_eq!(serde_json::to_value(&c).unwrap(), serde_json::json!({ "by": "ana", "since": "2026-10-08T14:10:00Z" }));
        assert!(serde_json::from_str::<CheckedOut>(r#"{"by":"ana"}"#).is_err(), "both are required");
        assert!(serde_json::from_str::<CheckedOut>(r#"{"by":"ana","since":"x","role":"lead"}"#).is_err(), "and nothing else");
    }

    /// # Test contract
    ///
    /// ```json
    /// {"test": "a-status-is-written-new-and-read-under-either-name", "checks": ["crates/core/src/model#status-names"]}
    /// ```
    #[test]
    fn a_status_is_written_new_and_read_under_either_name() {
        let ticket = |status: &str| format!(r#"{{"slug":"t","title":"T","kind":"change","priority":"next","status":"{status}","touches":[],"relies_on":[],"body":"b","decision":null}}"#);
        for (former, now, status) in
            [("open", "inscribed", Status::Inscribed), ("active", "brewing", Status::Brewing), ("landing", "simmering", Status::Simmering), ("closed", "bottled", Status::Bottled)]
        {
            for name in [former, now] {
                let t: Ticket = serde_json::from_str(&ticket(name)).unwrap();
                assert_eq!(t.status, status, "{name} is read");
                assert_eq!(serde_json::to_value(&t).unwrap()["status"], now, "and written as {now}");
            }
            assert_eq!(status.name(), now);
        }
        let start: Start = serde_json::from_str(r#"{"ticket":"t","fingerprints":{},"closed_in":"C:/repo"}"#).unwrap();
        assert_eq!(start.simmering_in.as_deref(), Some("C:/repo"));
        assert_eq!(serde_json::to_value(&start).unwrap()["simmering_in"], "C:/repo");
    }

    /// # Test contract
    ///
    /// ```json
    /// {"test": "records-carry-nothing-else", "checks": ["crates/core/src/model#ticket-bounded", "crates/core/src/model#decisions-only-dates"]}
    /// ```
    #[test]
    fn records_carry_nothing_else() {
        let ticket = |extra: &str| format!(r#"{{"slug":"t","title":"T","kind":"change","priority":"next","status":"open","touches":[],"relies_on":[],"body":"b","decision":null{extra}}}"#);
        assert!(serde_json::from_str::<Ticket>(&ticket("")).is_ok());
        for field in ["assignee", "plan", "notes", "history", "created", "closed"] {
            assert!(serde_json::from_str::<Ticket>(&ticket(&format!(r#","{field}":"x""#))).is_err(), "a ticket has no {field}");
        }
        let page = r#"{"slug":"p","title":"P","body":"b","links":[],"modified":"2026-01-01"}"#;
        assert!(serde_json::from_str::<Page>(page).is_err(), "no record carries a timestamp");
    }

    /// # Test contract
    ///
    /// ```json
    /// {"test": "records-reject-unknown-fields", "checks": ["crates/core/src/model#strict-records"]}
    /// ```
    #[test]
    fn records_reject_unknown_fields() {
        let bad = r#"{"slug":"a","title":"A","body":"b","links":[],"extra":1}"#;
        assert!(serde_json::from_str::<Page>(bad).is_err());
    }

    /// # Test contract
    ///
    /// ```json
    /// {"test": "a-proposal-round-trips", "checks": ["crates/core/src/model#proposal-record"]}
    /// ```
    #[test]
    fn a_proposal_round_trips() {
        let json = r#"{"ticket":"t","pages":{"a":{"record":{"slug":"a","title":"A","body":"b","links":[]},"base":"fp"}},"deleted":{"req:gone":"fp2"}}"#;
        let p: Proposal = serde_json::from_str(json).unwrap();
        assert_eq!((p.ticket.as_str(), p.pages["a"].base.as_deref(), p.deleted["req:gone"].as_str()), ("t", Some("fp"), "fp2"));
        assert!(p.requirements.is_empty() && !p.is_empty());
        let back = serde_json::to_value(&p).unwrap();
        assert!(back.get("requirements").is_none(), "an empty map is left out");
        assert_eq!(serde_json::from_value::<Proposal>(back).unwrap(), p);
        // A new record carries no base; a field the model lacks is refused.
        let new: Proposed<Page> = serde_json::from_str(r#"{"record":{"slug":"a","title":"A","body":"b","links":[]}}"#).unwrap();
        assert_eq!(new.base, None);
        assert!(serde_json::from_str::<Proposal>(r#"{"ticket":"t","extra":1}"#).is_err());
        assert!(Proposal::new("t").is_empty());
        // The decisions it adds, apart from any record: with no id, and links by id; read back the same.
        let json = r#"{"ticket":"t","decisions":{"x":{"added":[{"date":"2026-10-07","text":"Why."}],"linked":{"3":[1,2]}}}}"#;
        let p: Proposal = serde_json::from_str(json).unwrap();
        assert!(!p.is_empty());
        assert_eq!((p.decisions["x"].added[0].id, p.decisions["x"].linked[&3].clone()), (None, vec![1, 2]));
        assert_eq!(serde_json::to_string(&p).unwrap(), json);
        // Another ticket's references, as the edit leaves them, with the truth's lists' fingerprint.
        let edit = r#"{"ticket":"t","tickets":{"0200":{"record":{"touches":["req:x"],"relies_on":[]},"base":"fp3"}}}"#;
        let p: Proposal = serde_json::from_str(edit).unwrap();
        assert_eq!((p.tickets["0200"].record.touches[0].to_string(), p.tickets["0200"].base.as_deref()), ("req:x".to_string(), Some("fp3")));
        assert!(!p.is_empty());
        assert_eq!(serde_json::from_value::<Proposal>(serde_json::to_value(&p).unwrap()).unwrap(), p);
        assert!(serde_json::from_str::<Proposal>(r#"{"ticket":"t","tickets":{"0200":{"record":{"touches":[],"relies_on":[],"body":""}}}}"#).is_err());
        let c: Current = serde_json::from_str(r#"{"ticket":"t"}"#).unwrap();
        assert_eq!(c.ticket, "t");
    }

    /// # Test contract
    ///
    /// ```json
    /// {"test": "a-decisions-id-is-written-only-where-it-is-not-its-position", "checks": ["crates/core/src/model#decision-id-position", "crates/core/src/model#decision-in-force"]}
    /// ```
    #[test]
    fn a_decisions_id_is_written_only_where_it_is_not_its_position() {
        let record = |decisions: &str| format!(r#"{{"slug":"x","title":"X","statement":"It SHALL.","scenarios":[],"covered_by":[],"decisions":{decisions}}}"#);
        // Written before ids: each takes its position, and is written back as it was, so its fingerprint is unchanged.
        let before = record(r#"[{"date":"2026-01-01","text":"a"},{"date":"2026-01-02","text":"b","supersedes":[1]}]"#);
        let q: Requirement = serde_json::from_str(&before).unwrap();
        assert_eq!(q.decisions.iter().map(|d| d.id).collect::<Vec<_>>(), vec![Some(1), Some(2)]);
        assert_eq!(serde_json::to_string(&q).unwrap(), before);
        // An id that is not its position is written, and kept: a decision gone from before it moves nothing.
        let gap = record(r#"[{"date":"2026-01-01","text":"a"},{"id":3,"date":"2026-01-03","text":"c"}]"#);
        let q: Requirement = serde_json::from_str(&gap).unwrap();
        assert_eq!((q.decisions[1].id, q.next_id(), q.decision(2).is_none()), (Some(3), 4, true));
        assert_eq!(serde_json::to_string(&q).unwrap(), gap);
        // One not yet bottled, as a read through a proposal shows it: null, read back as none.
        let waiting = record(r#"[{"date":"2026-01-01","text":"a"},{"id":null,"date":"2026-01-04","text":"w","adopted":true}]"#);
        let q: Requirement = serde_json::from_str(&waiting).unwrap();
        assert_eq!((q.decisions[1].id, q.in_force().len(), q.next_id()), (None, 2, 2));
        assert_eq!(serde_json::to_string(&q).unwrap(), waiting);
        // A ticket's decision carries none and writes none; a field the record lacks is still refused.
        assert_eq!(serde_json::to_string(&Decision::new("2026-01-01", "Done.")).unwrap(), r#"{"date":"2026-01-01","text":"Done."}"#);
        assert!(serde_json::from_str::<Requirement>(&record(r#"[{"date":"2026-01-01","text":"a","why":1}]"#)).is_err());
        // A tool takes an id as a number or as it is said.
        let ids: Vec<DecisionId> = serde_json::from_str(r#"[3, "d4"]"#).unwrap();
        assert_eq!(DecisionId::all(&ids), vec![3, 4]);
        for bad in [r#"0"#, r#""d0""#, r#""4""#, r#""dx""#, r#""d""#, r#""dd3""#, r#""d03""#] {
            assert!(serde_json::from_str::<DecisionId>(bad).is_err(), "{bad}");
        }
    }
}
