//! # Module contract
//!
//! ```json
//! {"purpose":"Define the node kinds, references and store records that every other part of core is written against.","criteria":[{"id":"reference-form","statement":"A reference is a string of the form kind:id and parses and prints in exactly that form."},{"id":"strict-records","statement":"Store records reject unknown fields when deserialised so a misspelled field is an error, not silently dropped."},{"id":"ticket-bounded","statement":"A ticket has exactly a slug, title, kind, priority, status, touches, relies_on, body and decision, and no field for plans, notes, assignees or history."},{"id":"decisions-only-dates","statement":"A decision's date is the only date in any store record; no record carries created, modified or closed timestamps."}]}
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
            _ => return None,
        })
    }

    /// Kinds that live in the store and can be written by the tools.
    pub fn is_stored(self) -> bool {
        matches!(self, Kind::Page | Kind::Req | Kind::Ticket)
    }

    pub const ALL: [Kind; 7] = [
        Kind::Page,
        Kind::Req,
        Kind::Ticket,
        Kind::Module,
        Kind::Criterion,
        Kind::File,
        Kind::Decl,
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

    /// For `criterion` and `decl` references, the part before `#` and the
    /// part after. `None` for other kinds or when `#` is missing.
    pub fn split_hash(&self) -> Option<(&str, &str)> {
        match self.kind {
            Kind::Criterion | Kind::Decl => self.id.split_once('#'),
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

/// An intended change to the manual, or a gap or question about it.
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

/// `grimoire.json` at the root of a store.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct StoreConfig {
    pub version: u32,
    pub repository: String,
    pub roots: Vec<String>,
    pub languages: Vec<String>,
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

    #[test]
    fn reference_rejects_bad_forms() {
        assert!("page".parse::<Reference>().is_err());
        assert!("thing:x".parse::<Reference>().is_err());
        assert!("page:".parse::<Reference>().is_err());
    }

    #[test]
    fn records_reject_unknown_fields() {
        let bad = r#"{"slug":"a","title":"A","body":"b","links":[],"extra":1}"#;
        assert!(serde_json::from_str::<Page>(bad).is_err());
    }
}
