//! # Module contract
//!
//! ```json
//! {"purpose":"Define the node kinds, references and store records that every other part of core is written against.","criteria":[{"id":"baseline-record","statement":"A baseline is a date, a required reason and a list of finding identities (rule, check, ref and optional edge, never the reason text); a store without one behaves as one with an empty list."},{"id":"reference-form","statement":"A reference is a string of the form kind:id and parses and prints in exactly that form."},{"id":"strict-records","statement":"Store records reject unknown fields when deserialised so a misspelled field is an error, not silently dropped."},{"id":"ticket-bounded","statement":"A ticket has exactly a slug, title, kind, priority, status, touches, relies_on, body and decision, and no field for plans, notes, assignees or history."},{"id":"decisions-only-dates","statement":"Pages, requirements and tickets carry no created, modified or closed timestamps, and a store record carries no date but a person's choice: a decision's date, a baseline's date of adoption and a judgement's date."},{"id":"kinds","statement":"The node kinds are page, req and ticket (stored) and module, criterion, file, decl, test and recipe (derived); a criterion's id is module id, #, criterion id, a decl's is file path, #, declaration path, a test's is file path, #, the name in its test block, a recipe's its id and an ingredient's recipe id, #, ingredient id, and a decision is of the kind req, its id the requirement's slug, #, d and the decision's id; a criterion's and a test's reference split at the last #, since a criterion id and a test name never hold one, an ingredient's and a decision's at the first, since a recipe id and a slug never hold one, and a decl carries its file, so a # in a file or declaration name never misplaces either part."},{"id":"page-record","statement":"A page is slug, title, body (Markdown) and links, which may name only pages, requirements, modules and recipes, in display order without duplicates."},{"id":"requirement-record","statement":"A requirement is slug, title, a normative statement, scenarios of title/given/when/then, covered_by naming only criteria, recipes and ingredients, decisions of id, date, text, the ids of earlier decisions each supersedes and whether it was recorded as an adoption, in the order they landed, and whether it is retired; a decision's id is written only where it differs from its position, 1 for the oldest, read as that position where absent, and null for one not yet landed; a decision is in force unless a later one supersedes it, and a requirement is described when it has decisions in force and every one of them is an adoption."},{"id":"config-record","statement":"The store config is version, the repository's absolute path, its two roots, the languages to extract, an optional contract scope of include and exclude prefixes, the page slugs its site shows, optional large-file and plain-file line limits, 1,000 and 5,000 when absent, and the project's recipe kinds, each with the kinds it may use; a version this build does not know is refused."},{"id":"start-record","statement":"A start is a ticket's slug and a map from each node's reference to its fingerprint, with no date, and, once the ticket was closed when: landed, the checkout it was closed in, by repository path, absent otherwise."},{"id":"proposal-record","statement":"A proposal is a ticket's slug, the pages and requirements it proposes, each a record with the fingerprint of the truth's record it was written against or none for a new one, the pages and requirements it deletes by reference with that fingerprint, the other tickets whose references it edits, each as its touches and relies_on with the fingerprint of the truth's two lists, and for each requirement the decisions it adds, with no id, and the supersessions it links, by id, apart from any record it holds; the current record is the slug of the ticket a write lands in when it names none."},{"id":"judgement-record","statement":"A judgement is exactly a recipe's id, the fingerprint it was judged at, an ISO date, the ticket it was judged in and its outcome, held or updated; a recipe read from the repository carries its digest, the fingerprint a judgement is matched against."},{"id":"two-roots","statement":"A config's roots are the pair `{grimoire, rites}`, either of which may be missing, and never the same page; one held as a list of slugs is read with its first as the grimoire and its second as the rites, any further one dropped, and is written as the pair; list gives the named roots, the grimoire first, and rename renames each named root that matches."}]}
//! ```

use std::fmt;
use std::str::FromStr;

use serde::{Deserialize, Deserializer, Serialize, Serializer};

/// The kinds of node in the graph. The first three live in the store; the
/// rest are derived from the repository on every read.
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Kind {
    Page,
    Req,
    Ticket,
    Module,
    Criterion,
    File,
    Decl,
    Test,
    /// A recipe, or one of its ingredients when its id holds `#`.
    Recipe,
}

impl Kind {
    pub fn as_str(self) -> &'static str {
        match self {
            Kind::Page => "page",
            Kind::Req => "req",
            Kind::Ticket => "ticket",
            Kind::Module => "module",
            Kind::Criterion => "criterion",
            Kind::File => "file",
            Kind::Decl => "decl",
            Kind::Test => "test",
            Kind::Recipe => "recipe",
        }
    }

    pub fn parse(s: &str) -> Option<Kind> {
        Some(match s {
            "page" => Kind::Page,
            "req" => Kind::Req,
            "ticket" => Kind::Ticket,
            "module" => Kind::Module,
            "criterion" => Kind::Criterion,
            "file" => Kind::File,
            "decl" => Kind::Decl,
            "test" => Kind::Test,
            "recipe" => Kind::Recipe,
            _ => return None,
        })
    }

    /// Kinds that live in the store and can be written by the tools.
    pub fn is_stored(self) -> bool {
        matches!(self, Kind::Page | Kind::Req | Kind::Ticket)
    }

    /// Whether nodes of this kind come from the code and not the store.
    pub fn is_derived(self) -> bool {
        matches!(self, Kind::Module | Kind::Criterion | Kind::File | Kind::Decl | Kind::Test | Kind::Recipe)
    }

    pub const ALL: [Kind; 9] = [Kind::Page, Kind::Req, Kind::Ticket, Kind::Module, Kind::Criterion, Kind::File, Kind::Decl, Kind::Test, Kind::Recipe];
}

impl fmt::Display for Kind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

/// A reference to a node: `kind:id`. The only form a link takes.
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(type = "string", rename = "Ref"))]
pub struct Reference {
    pub kind: Kind,
    pub id: String,
}

impl Reference {
    pub fn new(kind: Kind, id: impl Into<String>) -> Self {
        Reference { kind, id: id.into() }
    }
    pub fn page(slug: &str) -> Self {
        Self::new(Kind::Page, slug)
    }
    pub fn req(slug: &str) -> Self {
        Self::new(Kind::Req, slug)
    }
    pub fn ticket(slug: &str) -> Self {
        Self::new(Kind::Ticket, slug)
    }
    pub fn module(id: &str) -> Self {
        Self::new(Kind::Module, id)
    }
    pub fn criterion(module: &str, id: &str) -> Self {
        Self::new(Kind::Criterion, format!("{module}#{id}"))
    }
    pub fn file(path: &str) -> Self {
        Self::new(Kind::File, path)
    }
    pub fn decl(file: &str, path: &str) -> Self {
        Self::new(Kind::Decl, format!("{file}#{path}"))
    }
    pub fn test(file: &str, name: &str) -> Self {
        Self::new(Kind::Test, format!("{file}#{name}"))
    }
    pub fn recipe(id: &str) -> Self {
        Self::new(Kind::Recipe, id)
    }
    pub fn ingredient(recipe: &str, id: &str) -> Self {
        Self::new(Kind::Recipe, format!("{recipe}#{id}"))
    }
    /// A requirement's decision, `req:<slug>#d<id>`: said as decision 3 of the requirement.
    pub fn decision(slug: &str, id: u32) -> Self {
        Self::new(Kind::Req, format!("{slug}#d{id}"))
    }

    /// For a decision's reference, the requirement's slug and the decision's id.
    pub fn as_decision(&self) -> Option<(&str, u32)> {
        let (slug, id) = self.id.split_once('#').filter(|_| self.kind == Kind::Req)?;
        Some((slug, decision_id(id.strip_prefix('d')?)?))
    }

    /// For `criterion` and `test` references, the module or file before the
    /// last `#` and the criterion id or test name after it, which never hold
    /// one; for an ingredient, the recipe before the first `#`, since a recipe
    /// id never holds one, and the ingredient after it; for a decision, the
    /// requirement before the first `#`, since a slug never holds one, and the
    /// decision after it. `None` for a recipe
    /// itself and for other kinds, a `decl` among them: a file and a
    /// declaration path may both hold `#`, so a declaration's file is read
    /// from its node, never from its reference.
    pub fn split_hash(&self) -> Option<(&str, &str)> {
        match self.kind {
            Kind::Criterion | Kind::Test => self.id.rsplit_once('#'),
            Kind::Recipe | Kind::Req => self.id.split_once('#'),
            _ => None,
        }
    }
}

/// A decision's id from its digits, `3`; `None` for anything else, 0 and a leading 0 among them.
pub fn decision_id(digits: &str) -> Option<u32> {
    if digits.is_empty() || digits.starts_with('0') || !digits.bytes().all(|b| b.is_ascii_digit()) {
        return None;
    }
    digits.parse().ok()
}

/// A decision's id as a tool takes it: a number, `3`, or as it is said, `"d3"`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DecisionId(pub u32);

impl DecisionId {
    pub fn all(ids: &[DecisionId]) -> Vec<u32> {
        ids.iter().map(|d| d.0).collect()
    }
}

impl<'de> Deserialize<'de> for DecisionId {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        #[derive(Deserialize)]
        #[serde(untagged)]
        enum Either {
            Number(u32),
            Said(String),
        }
        let id = match Either::deserialize(d).map_err(|_| serde::de::Error::custom("a decision's id is a number from 1 or d<n>"))? {
            Either::Number(n) => (n > 0).then_some(n),
            Either::Said(s) => s.strip_prefix('d').and_then(decision_id),
        };
        id.map(DecisionId).ok_or_else(|| serde::de::Error::custom("a decision's id is a number from 1 or d<n>"))
    }
}

impl fmt::Display for Reference {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}:{}", self.kind, self.id)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("invalid reference `{0}`: expected kind:id with a known kind")]
pub struct ReferenceError(pub String);

impl FromStr for Reference {
    type Err = ReferenceError;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let (kind, id) = s.split_once(':').ok_or_else(|| ReferenceError(s.to_string()))?;
        let kind = Kind::parse(kind).ok_or_else(|| ReferenceError(s.to_string()))?;
        if id.is_empty() {
            return Err(ReferenceError(s.to_string()));
        }
        Ok(Reference { kind, id: id.to_string() })
    }
}

impl Serialize for Reference {
    fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        s.collect_str(self)
    }
}

impl<'de> Deserialize<'de> for Reference {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        let s = String::deserialize(d)?;
        s.parse().map_err(serde::de::Error::custom)
    }
}

/// The relation an edge carries. Written on the owning node, pointing
/// downward; the graph derives the reverse.
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Relation {
    Links,
    CoveredBy,
    Touches,
    ReliesOn,
    Contains,
    Imports,
    BelongsTo,
    /// A test to a criterion it checks. Written on the test, pointing down,
    /// as a requirement's `covered_by` is.
    Checks,
    /// A recipe to another recipe it uses. Written in the recipe's block.
    Uses,
    /// A recipe to a path its `files` names: the implementation it represents. Only a finding's
    /// edge names it, for a path not found; the graph draws no such edge.
    Represents,
    /// A decision to an earlier decision of its requirement it replaces. Written in its `supersedes`.
    Supersedes,
}

impl Relation {
    pub fn as_str(self) -> &'static str {
        match self {
            Relation::Links => "links",
            Relation::CoveredBy => "covered_by",
            Relation::Touches => "touches",
            Relation::ReliesOn => "relies_on",
            Relation::Contains => "contains",
            Relation::Imports => "imports",
            Relation::BelongsTo => "belongs_to",
            Relation::Checks => "checks",
            Relation::Uses => "uses",
            Relation::Represents => "represents",
            Relation::Supersedes => "supersedes",
        }
    }
}

impl fmt::Display for Relation {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

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

    /// The decisions in force, oldest first: those no decision supersedes, one not yet landed among them.
    pub fn in_force(&self) -> Vec<&Decision> {
        let superseded = self.superseded();
        self.decisions.iter().filter(|d| !d.id.is_some_and(|id| superseded.contains(&id))).collect()
    }

    /// The landed decision whose id is `id`.
    pub fn decision(&self, id: u32) -> Option<&Decision> {
        self.decisions.iter().find(|d| d.id == Some(id))
    }

    /// The id the next decision to land takes: above every id it holds, so none is given twice.
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
/// position, 1 for the oldest, and null for one not yet landed, as a read through a proposal shows it.
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
/// decision written before ids, and one whose `id` is null has not landed.
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
    // A requirement's decision's id, said `d<n>`, given when it lands and never given twice; none on
    // a ticket's decision and on one waiting in a proposal. Among a requirement's decisions, absent
    // means the decision's position, 1 for the oldest, and null one not yet landed.
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
    // longer to be made (req:tool-withdraw-ticket). Only on a closed change's decision.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    #[cfg_attr(feature = "ts", ts(as = "Option<bool>", optional))]
    pub withdrawn: bool,
}

/// A requirement's landed decision as a node of the graph, `req:<slug>#d<id>`: derived from its
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

/// What judging a recipe found: it still says truly how to use what it
/// represents, or it was updated to say so.
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum JudgementOutcome {
    Held,
    Updated,
}

/// One judgement of a recipe against its files (req:store-judgement-record): a
/// person's or an agent's act, so dated, and bound to the state it judged by the
/// fingerprint of the recipe and its files then.
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Judgement {
    pub recipe: String,
    pub fingerprint: String,
    pub date: String,
    pub ticket: String,
    pub outcome: JudgementOutcome,
}

/// `judgements/<recipe>.json`: one recipe's judgements, oldest first.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Judgements {
    pub recipe: String,
    pub judgements: Vec<Judgement>,
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
    Open,
    Active,
    /// Its decision is written and its change has yet to land: active in
    /// every other respect, and closed by the landing.
    Landing,
    Closed,
}

impl Status {
    /// Whether a ticket in this status holds a proposal and a start: it is being worked or waits to land.
    pub fn is_live(self) -> bool {
        matches!(self, Status::Active | Status::Landing)
    }
    pub fn name(self) -> &'static str {
        match self {
            Status::Open => "open",
            Status::Active => "active",
            Status::Landing => "landing",
            Status::Closed => "closed",
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
    /// Whether the ticket is being worked: active, or a change waiting to
    /// land. Only such a ticket holds a proposal, a start and the current
    /// pointer; a gap or question waiting, landing, for the change that
    /// applies its answer is not worked itself.
    pub fn is_worked(&self) -> bool {
        self.status == Status::Active || (self.status == Status::Landing && self.kind == TicketKind::Change)
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
    /// The checkout, by its repository path, where the ticket was closed `when: landed`: it is
    /// that checkout's to land before the checkout takes another (req:one-ticket-per-branch).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[cfg_attr(feature = "ts", ts(optional))]
    pub closed_in: Option<String>,
}

/// `proposals/<ticket>.json`: a ticket's proposed change to the truth. The
/// pages and requirements as they will be once the ticket lands, each with
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
    /// against: they reach the ticket when this one lands.
    #[serde(default, skip_serializing_if = "std::collections::BTreeMap::is_empty")]
    pub tickets: std::collections::BTreeMap<String, Proposed<TicketReferences>>,
    /// The decisions this ticket adds to requirements, by slug, kept apart from any record it
    /// holds: appended after the truth's when it lands, each taking its id then.
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

/// `current.json`: the ticket a write lands in when it names none, the one
/// last started on this store.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Current {
    pub ticket: String,
}

/// A store's two roots: the grimoire, the root page of what the project is and must do, whose
/// requirements the code carries, and the rites, the root page of how its people work on it. Either may be missing, which
/// the check reports; a config holding a list of slugs, as stores did before, reads its first as the
/// grimoire and its second as the rites.
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
pub struct Roots {
    #[serde(skip_serializing_if = "Option::is_none")]
    #[cfg_attr(feature = "ts", ts(optional))]
    pub grimoire: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[cfg_attr(feature = "ts", ts(optional))]
    pub rites: Option<String>,
}

impl Roots {
    pub fn new(grimoire: &str, rites: &str) -> Roots {
        Roots { grimoire: Some(grimoire.to_string()), rites: Some(rites.to_string()) }
    }

    /// The roots that are named, the grimoire first.
    pub fn list(&self) -> Vec<&String> {
        self.grimoire.iter().chain(self.rites.iter()).collect()
    }

    pub fn contains(&self, slug: &str) -> bool {
        self.list().iter().any(|r| *r == slug)
    }

    /// Each named root that is `from`, renamed `to`; whether any was.
    pub fn rename(&mut self, from: &str, to: &str) -> bool {
        let mut hit = false;
        for r in [&mut self.grimoire, &mut self.rites].into_iter().flatten() {
            if r == from {
                *r = to.to_string();
                hit = true;
            }
        }
        hit
    }
}

/// The roots as an older build reads them, a list, when both are named, and as the pair only when
/// one is missing, which an older build could not have held: for what a server hands a machine.
pub fn roots_as_older_builds_read<S: Serializer>(roots: &Roots, s: S) -> Result<S::Ok, S::Error> {
    match (&roots.grimoire, &roots.rites) {
        (Some(g), Some(r)) => [g, r].serialize(s),
        _ => roots.serialize(s),
    }
}

/// A list of slugs, as stores held before the two roots: the first is the grimoire, the second the rites.
impl From<Vec<String>> for Roots {
    fn from(list: Vec<String>) -> Roots {
        let mut it = list.into_iter();
        Roots { grimoire: it.next(), rites: it.next() }
    }
}

impl<'de> Deserialize<'de> for Roots {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Roots, D::Error> {
        #[derive(Deserialize)]
        #[serde(deny_unknown_fields)]
        struct Pair {
            #[serde(default)]
            grimoire: Option<String>,
            #[serde(default)]
            rites: Option<String>,
        }
        #[derive(Deserialize)]
        #[serde(untagged)]
        enum Shape {
            List(Vec<String>),
            Pair(Pair),
        }
        Ok(match Shape::deserialize(d).map_err(|_| serde::de::Error::custom("roots is `{grimoire, rites}`, each a page slug, or a list of slugs as older stores held"))? {
            Shape::List(list) => list.into(),
            Shape::Pair(p) => Roots { grimoire: p.grimoire, rites: p.rites },
        })
    }
}

/// `grimoire.json` at the root of a store.
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct StoreConfig {
    pub version: u32,
    pub repository: String,
    pub roots: Roots,
    pub languages: Vec<String>,
    #[serde(default, skip_serializing_if = "Scope::is_empty")]
    pub scope: Scope,
    #[serde(default, skip_serializing_if = "Order::is_empty")]
    pub order: Order,
    // The pages the project's site shows, as page slugs.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    #[cfg_attr(feature = "ts", ts(as = "Option<Vec<String>>", optional))]
    pub site: Vec<String>,
    // A file of this many lines or more, its tests aside, is an omen; DEFAULT_LARGE_FILE when absent.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[cfg_attr(feature = "ts", ts(optional))]
    pub large_file: Option<u32>,
    // A file of more lines than this is shown plain, unhighlighted; DEFAULT_PLAIN_FILE when absent.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[cfg_attr(feature = "ts", ts(optional))]
    pub plain_file: Option<u32>,
    // The project's own kinds of recipe, each with the kinds its recipes may use (req:recipe-kinds).
    #[serde(default, skip_serializing_if = "std::collections::BTreeMap::is_empty")]
    #[cfg_attr(feature = "ts", ts(as = "Option<std::collections::BTreeMap<String, RecipeKind>>", optional))]
    pub recipe_kinds: std::collections::BTreeMap<String, RecipeKind>,
}

/// A kind of recipe the project declares: the kinds its recipes may use.
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RecipeKind {
    #[serde(default)]
    pub uses: Vec<String>,
}

/// The order a person set within each priority band, as ticket slugs. A
/// ticket that has closed or changed band is ignored where it is read.
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Order {
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub next: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub high: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub later: Vec<String>,
}

impl Order {
    pub fn is_empty(&self) -> bool {
        self.next.is_empty() && self.high.is_empty() && self.later.is_empty()
    }

    pub fn band(&self, p: Priority) -> &[String] {
        match p {
            Priority::Next => &self.next,
            Priority::High => &self.high,
            Priority::Later => &self.later,
        }
    }

    pub fn band_mut(&mut self, p: Priority) -> &mut Vec<String> {
        match p {
            Priority::Next => &mut self.next,
            Priority::High => &mut self.high,
            Priority::Later => &mut self.later,
        }
    }
}

/// Which paths are expected to carry contracts: repository-relative,
/// `/`-separated prefixes matched on whole segments. An empty `include`
/// means everything.
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Scope {
    #[serde(default)]
    pub include: Vec<String>,
    #[serde(default)]
    pub exclude: Vec<String>,
}

impl Scope {
    pub fn is_empty(&self) -> bool {
        self.include.is_empty() && self.exclude.is_empty()
    }

    /// Whether a module with these files is in scope: some file under an
    /// include (or no include at all) and none under an exclude.
    pub fn covers(&self, files: &[String]) -> bool {
        let under = |f: &str, p: &str| f == p || f.strip_prefix(p).is_some_and(|rest| rest.starts_with('/'));
        let included = self.include.is_empty() || files.iter().any(|f| self.include.iter().any(|p| under(f, p)));
        let excluded = files.iter().any(|f| self.exclude.iter().any(|p| under(f, p)));
        included && !excluded
    }
}

impl StoreConfig {
    /// Version 2 holds only trimmed strings. A version 1 store, from before
    /// the write tools trimmed, is read with its strings trimmed and is
    /// rewritten as version 2 by the first writer to take its lock.
    pub const CURRENT_VERSION: u32 = 2;
    pub const TRIMMED_ON_READ: u32 = 1;
    /// A file of this many lines or more, its tests aside, is an omen when the config names no limit.
    pub const DEFAULT_LARGE_FILE: u32 = 1_000;
    /// A file of more lines than this is shown plain when the config names no limit.
    pub const DEFAULT_PLAIN_FILE: u32 = 5_000;

    /// The large-file limit in force: the config's, or the default.
    pub fn large_file(&self) -> u32 {
        self.large_file.unwrap_or(Self::DEFAULT_LARGE_FILE)
    }

    /// The plain-file limit in force: the config's, or the default.
    pub fn plain_file(&self) -> u32 {
        self.plain_file.unwrap_or(Self::DEFAULT_PLAIN_FILE)
    }
}

// ---------------------------------------------------------------------------
// Derived records, produced by extraction and never stored
// ---------------------------------------------------------------------------

/// A module contract as it appears in source.
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Contract {
    pub purpose: String,
    pub criteria: Vec<ContractCriterion>,
}

#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ContractCriterion {
    pub id: String,
    pub statement: String,
}

#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Criterion {
    pub module: String,
    pub id: String,
    pub statement: String,
}

#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Module {
    pub id: String,
    pub files: Vec<String>,
    pub purpose: Option<String>,
    pub criteria: Vec<Criterion>,
    pub imports: Vec<String>,
    /// Whether the adapter expects this module to carry a contract.
    pub expects_contract: bool,
    /// Why the contract is missing or malformed, when `purpose` is `None`
    /// and one was expected.
    pub contract_error: Option<String>,
    /// The file and line of the `# Module contract` heading.
    pub contract_at: Option<(String, u32)>,
}

#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct File {
    pub path: String,
    pub module: String,
    pub language: String,
    pub decls: Vec<Decl>,
    /// The fingerprint of the file's text as read.
    #[serde(default)]
    pub digest: String,
    // Whether its adapter expects this file itself to carry a contract: a test file folded into a module does not.
    #[serde(default)]
    pub expects_contract: bool,
    // How many lines the file has, and how many of them its adapter tells are test code.
    #[serde(default)]
    pub lines: u32,
    #[serde(default)]
    pub test_lines: u32,
}

/// A test, from a `# Test contract` block: its name, where the block is, and
/// the criteria it checks as `module id#criterion id`.
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Test {
    pub file: String,
    pub name: String,
    pub line: u32,
    pub checks: Vec<String>,
}

/// A `# Test contract` block that could not be read, and why.
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TestError {
    pub file: String,
    pub line: u32,
    pub reason: String,
}

/// A recipe, read from a `.recipe.md` file: something the project offers the people who build with it.
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Recipe {
    pub id: String,
    pub kind: String,
    // The prose's own first heading, or the id.
    pub title: String,
    // The prose for people, Markdown, without the title's heading and the recipe block.
    pub text: String,
    pub ingredients: Vec<Ingredient>,
    // The implementation it represents: repository paths, each a file or a folder.
    pub files: Vec<String>,
    // The ids of the recipes it uses.
    pub uses: Vec<String>,
    // The `.recipe.md` file it is written in, and the line of its `Recipe` heading.
    pub file: String,
    pub line: u32,
    // The paths of `files` not found in the checkout when it was read.
    pub missing: Vec<String>,
    // The fingerprint of the recipe file and of every file its `files` name, as read: what a judgement is matched against.
    pub digest: String,
}

// One part of a recipe: an id and what it says.
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Ingredient {
    pub recipe: String,
    pub id: String,
    pub says: String,
}

// A `.recipe.md` file that could not be read as a recipe, and why.
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RecipeError {
    pub file: String,
    pub line: u32,
    pub reason: String,
}

#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Decl {
    pub path: String,
    pub kind: String,
    pub line: u32,
    /// The file it is declared in, set where the file is known, so no one
    /// recovers it from the reference.
    #[serde(default)]
    pub file: String,
}

#[cfg(test)]
mod tests {
    use super::*;

    /// # Test contract
    ///
    /// ```json
    /// {"test": "a-config-names-two-roots", "checks": ["crates/core/src/model#two-roots"]}
    /// ```
    #[test]
    fn a_config_names_two_roots() {
        let read = |json: &str| serde_json::from_str::<Roots>(json).unwrap();
        // The pair as written now, either half missing.
        assert_eq!(read(r#"{"grimoire":"overview","rites":"rites"}"#), Roots::new("overview", "rites"));
        assert_eq!(read(r#"{"grimoire":"overview"}"#), Roots { grimoire: Some("overview".into()), rites: None });
        // A list, as older stores held: the first the grimoire, the second the rites, any further one dropped.
        assert_eq!(read(r#"["overview","handbook","extra"]"#), Roots::new("overview", "handbook"));
        assert_eq!(read(r#"["overview"]"#), Roots { grimoire: Some("overview".into()), rites: None });
        assert!(serde_json::from_str::<Roots>(r#"{"grimoire":"a","other":"b"}"#).is_err(), "nothing else is a root");
        // Written as the pair, whatever it was read as.
        assert_eq!(serde_json::to_string(&read(r#"["overview","handbook"]"#)).unwrap(), r#"{"grimoire":"overview","rites":"handbook"}"#);
        // The named roots, the grimoire first; a rename moves whichever matches.
        let mut r = Roots::new("overview", "rites");
        assert_eq!(r.list(), [&"overview".to_string(), &"rites".to_string()]);
        assert!(r.rename("rites", "craft") && !r.rename("nowhere", "x"));
        assert_eq!(r, Roots::new("overview", "craft"));
        assert!(r.contains("craft") && !r.contains("rites"));
    }

    #[test]
    fn scope_matches_whole_segments() {
        let s = |inc: &[&str], exc: &[&str]| Scope { include: inc.iter().map(|x| x.to_string()).collect(), exclude: exc.iter().map(|x| x.to_string()).collect() };
        let files = |fs: &[&str]| fs.iter().map(|x| x.to_string()).collect::<Vec<_>>();
        assert!(Scope::default().covers(&files(&["anything/at/all.rs"])), "no scope is everything");
        let core = s(&["crates/core"], &[]);
        assert!(core.covers(&files(&["crates/core/src/lib.rs"])));
        assert!(!core.covers(&files(&["crates/core-extra/src/lib.rs"])), "whole segments only");
        assert!(!core.covers(&files(&["crates/cli/src/main.rs"])));
        let carved = s(&["crates/core"], &["crates/core/src/generated"]);
        assert!(!carved.covers(&files(&["crates/core/src/generated/parser.rs"])));
        assert!(carved.covers(&files(&["crates/core/src/lint.rs"])));
        assert!(!s(&[], &["vendor"]).covers(&files(&["vendor/x.rs"])), "an exclude alone narrows everything");
    }

    /// # Test contract
    ///
    /// ```json
    /// {"test": "kinds-and-their-ids", "checks": ["crates/core/src/model#kinds"]}
    /// ```
    #[test]
    fn kinds_and_their_ids() {
        // A decision is of the kind req, its id the requirement's slug, #, d and its id, split at the first #.
        let d = Reference::decision("store-config-record", 3);
        assert_eq!(d.to_string(), "req:store-config-record#d3");
        assert_eq!(("req:store-config-record#d3".parse::<Reference>().unwrap().as_decision(), d.split_hash()), (Some(("store-config-record", 3)), Some(("store-config-record", "d3"))));
        assert_eq!((Reference::req("x").as_decision(), "req:x#3".parse::<Reference>().unwrap().as_decision(), "req:x#d0".parse::<Reference>().unwrap().as_decision()), (None, None, None));
        assert_eq!("req:x#dd3".parse::<Reference>().unwrap().as_decision(), None);
        for k in Kind::ALL {
            assert_eq!(Kind::parse(k.as_str()), Some(k));
            assert_eq!(k.is_stored(), matches!(k, Kind::Page | Kind::Req | Kind::Ticket));
        }
        assert_eq!(Reference::criterion("src/a", "c").to_string(), "criterion:src/a#c");
        assert_eq!(Reference::decl("src/a.rs", "Repo::open").to_string(), "decl:src/a.rs#Repo::open");
        assert_eq!(Reference::test("src/a.rs", "opens").to_string(), "test:src/a.rs#opens");
        assert_eq!(Reference::test("src/a.rs", "opens").split_hash(), Some(("src/a.rs", "opens")));
        // A recipe, and an ingredient of the same kind, split at the first #.
        assert!(Kind::Recipe.is_derived() && !Kind::Recipe.is_stored());
        assert_eq!(Reference::recipe("button").to_string(), "recipe:button");
        assert_eq!(Reference::recipe("button").split_hash(), None);
        assert_eq!(Reference::ingredient("button", "label").to_string(), "recipe:button#label");
        assert_eq!("recipe:button#label".parse::<Reference>().unwrap().split_hash(), Some(("button", "label")));
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
    /// {"test": "a-config-without-a-scope-loads", "checks": ["crates/core/src/model#config-record"]}
    /// ```
    #[test]
    fn a_config_without_a_scope_loads() {
        let c: StoreConfig = serde_json::from_str(r#"{"version":1,"repository":"C:/repo","roots":["overview"],"languages":["rust"]}"#).unwrap();
        assert!(c.scope.is_empty());
        assert!(!serde_json::to_string(&c).unwrap().contains("scope"), "an empty scope is not written");
        assert!(c.recipe_kinds.is_empty() && !serde_json::to_string(&c).unwrap().contains("recipe_kinds"), "no recipe kinds, none written");
        let kinds: StoreConfig =
            serde_json::from_str(r#"{"version":2,"repository":"C:/repo","roots":["overview"],"languages":[],"recipe_kinds":{"component":{"uses":["component","token"]},"token":{}}}"#).unwrap();
        assert_eq!(kinds.recipe_kinds["component"].uses, ["component", "token"]);
        assert!(kinds.recipe_kinds["token"].uses.is_empty());
        assert!(serde_json::from_str::<RecipeKind>(r#"{"uses":[],"colour":"red"}"#).is_err(), "a kind holds its uses and nothing else");
    }

    /// # Test contract
    ///
    /// ```json
    /// {"test": "line-limits-are-optional-with-defaults", "checks": ["crates/core/src/model#config-record"]}
    /// ```
    #[test]
    fn line_limits_are_optional_with_defaults() {
        let c: StoreConfig = serde_json::from_str(r#"{"version":2,"repository":"C:/repo","roots":["overview"],"languages":["rust"]}"#).unwrap();
        assert_eq!((c.large_file(), c.plain_file()), (1_000, 5_000));
        assert!(!serde_json::to_string(&c).unwrap().contains("_file"), "an absent limit is not written");
        let c: StoreConfig = serde_json::from_str(r#"{"version":2,"repository":"C:/repo","roots":["overview"],"languages":["rust"],"large_file":600}"#).unwrap();
        assert_eq!((c.large_file(), c.plain_file()), (600, 5_000));
    }

    /// # Test contract
    ///
    /// ```json
    /// {"test": "reference-round-trips", "checks": ["crates/core/src/model#reference-form"]}
    /// ```
    #[test]
    fn reference_round_trips() {
        let r: Reference = "criterion:crates/core/src/git/ready#guarantee".parse().unwrap();
        assert_eq!(r.kind, Kind::Criterion);
        assert_eq!(r.split_hash(), Some(("crates/core/src/git/ready", "guarantee")));
        assert_eq!(r.to_string(), "criterion:crates/core/src/git/ready#guarantee");
        let json = serde_json::to_string(&r).unwrap();
        assert_eq!(json, "\"criterion:crates/core/src/git/ready#guarantee\"");
        let back: Reference = serde_json::from_str(&json).unwrap();
        assert_eq!(back, r);
    }

    /// # Test contract
    ///
    /// ```json
    /// {"test": "reference-rejects-bad-forms", "checks": ["crates/core/src/model#reference-form"]}
    /// ```
    #[test]
    fn reference_rejects_bad_forms() {
        assert!("page".parse::<Reference>().is_err());
        assert!("thing:x".parse::<Reference>().is_err());
        assert!("page:".parse::<Reference>().is_err());
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
    /// {"test": "a-decisions-id-is-written-only-where-it-is-not-its-position", "checks": ["crates/core/src/model#requirement-record"]}
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
        // One not yet landed, as a read through a proposal shows it: null, read back as none.
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
