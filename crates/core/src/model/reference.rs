//! # Module contract
//!
//! ```json
//! {"purpose":"Name every node one way: the kinds of node, the reference form kind:id with its parsing and printing, a decision's id, and the relations between nodes.","criteria":[{"id":"reference-form","statement":"A reference is a string of the form kind:id and parses and prints in exactly that form."},{"id":"kinds","statement":"The node kinds are page, req and ticket (stored) and module, criterion, file, decl, test and recipe (derived); a criterion's id is module id, #, criterion id, a decl's is file path, #, declaration path, a test's is file path, #, the name in its test block, a recipe's its id and an ingredient's recipe id, #, ingredient id, and a decision is of the kind req, its id the requirement's slug, #, d and the decision's id; a criterion's and a test's reference split at the last #, since a criterion id and a test name never hold one, an ingredient's and a decision's at the first, since a recipe id and a slug never hold one, and a decl carries its file, so a # in a file or declaration name never misplaces either part."}]}
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

#[cfg(test)]
mod tests {
    use super::*;

    /// # Test contract
    ///
    /// ```json
    /// {"test": "kinds-and-their-ids", "checks": ["crates/core/src/model/reference#kinds"]}
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
    /// {"test": "reference-round-trips", "checks": ["crates/core/src/model/reference#reference-form"]}
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
    /// {"test": "reference-rejects-bad-forms", "checks": ["crates/core/src/model/reference#reference-form"]}
    /// ```
    #[test]
    fn reference_rejects_bad_forms() {
        assert!("page".parse::<Reference>().is_err());
        assert!("thing:x".parse::<Reference>().is_err());
        assert!("page:".parse::<Reference>().is_err());
    }
}
