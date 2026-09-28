//! # Module contract
//!
//! ```json
//! {"purpose":"Define the node kinds, references and store records that every other part of core is written against.","criteria":[{"id":"baseline-record","statement":"A baseline is a date, a required reason and a list of finding identities (rule, check, ref and optional edge, never the reason text); a store without one behaves as one with an empty list."},{"id":"reference-form","statement":"A reference is a string of the form kind:id and parses and prints in exactly that form."},{"id":"strict-records","statement":"Store records reject unknown fields when deserialised so a misspelled field is an error, not silently dropped."},{"id":"ticket-bounded","statement":"A ticket has exactly a slug, title, kind, priority, status, touches, relies_on, body and decision, and no field for plans, notes, assignees or history."},{"id":"decisions-only-dates","statement":"A decision's date is the only date in any store record; no record carries created, modified or closed timestamps."},{"id":"kinds","statement":"The node kinds are page, req and ticket (stored) and module, criterion, file, decl and test (derived); a criterion's id is module id, #, criterion id, a decl's is file path, #, declaration path, and a test's is file path, #, the name in its test block."},{"id":"page-record","statement":"A page is slug, title, body (Markdown) and links, which may name only pages, requirements and modules, in display order without duplicates."},{"id":"requirement-record","statement":"A requirement is slug, title, a normative statement, scenarios of title/given/when/then, covered_by naming only criteria, and decisions of date and text ordered oldest first."},{"id":"config-record","statement":"The store config is version, the repository's absolute path, at least one root page slug, the languages to extract and an optional contract scope of include and exclude prefixes; a version this build does not know is refused."}]}
//! ```

use std::fmt;
use std::str::FromStr;

use serde::{Deserialize, Deserializer, Serialize, Serializer};

/// The kinds of node in the graph. The first three live in the store; the
/// rest are derived from the repository on every read.
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
            _ => return None,
        })
    }

    /// Kinds that live in the store and can be written by the tools.
    pub fn is_stored(self) -> bool {
        matches!(self, Kind::Page | Kind::Req | Kind::Ticket)
    }

    pub const ALL: [Kind; 8] = [
        Kind::Page,
        Kind::Req,
        Kind::Ticket,
        Kind::Module,
        Kind::Criterion,
        Kind::File,
        Kind::Decl,
        Kind::Test,
    ];
}

impl fmt::Display for Kind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

/// A reference to a node: `kind:id`. The only form a link takes.
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
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

    /// For `criterion` and `decl` references, the part before `#` and the
    /// part after. `None` for other kinds or when `#` is missing.
    pub fn split_hash(&self) -> Option<(&str, &str)> {
        match self.kind {
            Kind::Criterion | Kind::Decl | Kind::Test => self.id.split_once('#'),
            _ => None,
        }
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
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Page {
    pub slug: String,
    pub title: String,
    pub body: String,
    pub links: Vec<Reference>,
}

/// One explicit requirement in the OpenSpec shape.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Requirement {
    pub slug: String,
    pub title: String,
    pub statement: String,
    pub scenarios: Vec<Scenario>,
    pub covered_by: Vec<Reference>,
    pub decisions: Vec<Decision>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Scenario {
    pub title: String,
    pub given: String,
    pub when: String,
    pub then: String,
}

/// A dated line saying what was chosen and why.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Decision {
    pub date: String,
    pub text: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum TicketKind {
    Change,
    Gap,
    Question,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Priority {
    Next,
    High,
    Later,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Status {
    Open,
    Active,
    Closed,
}

/// An intended change to the grimoire, or a gap or question about it.
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

/// A lint rule. Stored in a baseline, reported in every finding.
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
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FindingKey {
    pub rule: Rule,
    pub check: String,
    pub r#ref: Reference,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub edge: Option<FindingEdge>,
}

/// `baseline.json`: the findings accepted as inherited at adoption.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Baseline {
    pub date: String,
    pub reason: String,
    pub findings: Vec<FindingKey>,
}

/// `grimoire.json` at the root of a store.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct StoreConfig {
    pub version: u32,
    pub repository: String,
    pub roots: Vec<String>,
    pub languages: Vec<String>,
    #[serde(default, skip_serializing_if = "Scope::is_empty")]
    pub scope: Scope,
}

/// Which paths are expected to carry contracts: repository-relative,
/// `/`-separated prefixes matched on whole segments. An empty `include`
/// means everything.
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
    pub const CURRENT_VERSION: u32 = 1;
}

// ---------------------------------------------------------------------------
// Derived records, produced by extraction and never stored
// ---------------------------------------------------------------------------

/// A module contract as it appears in source.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Contract {
    pub purpose: String,
    pub criteria: Vec<ContractCriterion>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ContractCriterion {
    pub id: String,
    pub statement: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Criterion {
    pub module: String,
    pub id: String,
    pub statement: String,
}

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

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct File {
    pub path: String,
    pub module: String,
    pub language: String,
    pub decls: Vec<Decl>,
}

/// A test, from a `# Test contract` block: its name, where the block is, and
/// the criteria it checks as `module id#criterion id`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Test {
    pub file: String,
    pub name: String,
    pub line: u32,
    pub checks: Vec<String>,
}

/// A `# Test contract` block that could not be read, and why.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TestError {
    pub file: String,
    pub line: u32,
    pub reason: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Decl {
    pub path: String,
    pub kind: String,
    pub line: u32,
}

#[cfg(test)]
mod tests {
    use super::*;

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
        for k in Kind::ALL {
            assert_eq!(Kind::parse(k.as_str()), Some(k));
            assert_eq!(k.is_stored(), matches!(k, Kind::Page | Kind::Req | Kind::Ticket));
        }
        assert_eq!(Reference::criterion("src/a", "c").to_string(), "criterion:src/a#c");
        assert_eq!(Reference::decl("src/a.rs", "Repo::open").to_string(), "decl:src/a.rs#Repo::open");
        assert_eq!(Reference::test("src/a.rs", "opens").to_string(), "test:src/a.rs#opens");
        assert_eq!(Reference::test("src/a.rs", "opens").split_hash(), Some(("src/a.rs", "opens")));
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
}
