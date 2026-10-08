//! # Module contract
//!
//! ```json
//! {"purpose":"Declare the answer every tool returns as a typed value, so each answer's shape is written once, is the same over every transport, and is exported to the UI as TypeScript.","criteria":[{"id":"typed-answers","statement":"Every tool answers with a declared answer type that names it; each tool is named by exactly one answer type, and the dispatcher refuses, in debug builds and tests, to answer a tool with any other."},{"id":"generated-current","statement":"The TypeScript the answer types and the tool error produce, every type they reach plus a ToolAnswers map from each tool's name to its answer, is exactly the committed web/src/generated/tools.ts; `cargo test -p grimoire-core --test typescript -- --ignored` rewrites it."}]}
//! ```

use std::collections::BTreeMap;

use serde::Serialize;

use crate::graph::SourceLocation;
use crate::lint::Finding;
use crate::model::{Judgement, Priority, Reference, StoreConfig};
use crate::query::{Annotation, Board, CoveringCriterion, CriterionCoverage, NodeView, Outline, RecipeState, RestsOn, RestsOnMe, Summary};

/// A tool's answer: a value of a declared type, listing the tools that answer with it.
pub trait Answer: Serialize {
    const TOOLS: &'static [&'static str];
}

/// `find`: the matching nodes, best first.
#[derive(Debug, Clone, Serialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
pub struct NodesAnswer {
    pub nodes: Vec<Summary>,
}

/// `get` and `record_decision`: one node in full. A ticket comes with its
/// proposal as drafts; a page, requirement or ticket with the tickets whose
/// proposals would change it.
#[derive(Debug, Clone, Serialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
pub struct NodeAnswer {
    pub node: NodeView,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[cfg_attr(feature = "ts", ts(optional))]
    pub proposal: Option<ProposalView>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[cfg_attr(feature = "ts", ts(optional))]
    pub drafts: Option<Vec<Reference>>,
    // A ticket being worked: who has it checked out and since when, empty when who is unknown (req:ticket-checked-out).
    #[serde(skip_serializing_if = "Option::is_none")]
    #[cfg_attr(feature = "ts", ts(optional))]
    pub checked_out: Option<Vec<crate::model::CheckedOut>>,
}

/// A ticket's proposal as a reader sees it: each proposed page and
/// requirement, the records it would delete, and the other tickets whose
/// references it would change, each naming `touches` or `relies_on`.
#[derive(Debug, Clone, Serialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
pub struct ProposalView {
    pub pages: Vec<Draft>,
    pub requirements: Vec<Draft>,
    pub deleted: Vec<Reference>,
    pub tickets: Vec<Draft>,
}

/// One proposed record: new to the truth, or changed, naming the fields
/// that differ from the truth's record.
#[derive(Debug, Clone, Serialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
pub struct Draft {
    pub r#ref: Reference,
    pub title: String,
    pub new: bool,
    pub differs: Vec<String>,
}

/// `rests_on`: what a node rests on, nearest first.
#[derive(Debug, Clone, Serialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
pub struct RestsOnAnswer {
    pub nodes: Vec<RestsOn>,
}

/// `coverage`: for a criterion, what names and checks it; for a module, each
/// criterion's coverage; for a requirement, the criteria it names.
#[derive(Debug, Clone, Serialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[serde(untagged)]
pub enum CoverageAnswer {
    OfCriterion { requirements: Vec<Summary>, tests: Vec<Summary> },
    OfModule { criteria: Vec<CriterionCoverage> },
    OfRequirement { criteria: Vec<CoveringCriterion> },
}

/// `related_tickets`: the gaps and questions not yet bottled sharing a node with the ticket.
#[derive(Debug, Clone, Serialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
pub struct TicketsAnswer {
    pub tickets: Vec<Summary>,
}

/// `lint`: the findings, their counts per rule, and how many are new.
#[derive(Debug, Clone, Serialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
pub struct LintAnswer {
    pub findings: Vec<Finding>,
    // What other work causes, shown apart and never counted as new (req:check-own-work).
    pub advisories: Vec<crate::advisories::Advisory>,
    // Each finding above that a change this checkout closed `when: bottled`, not bottled yet, would mend; it still fails (req:check-own-work).
    pub mended: Vec<crate::advisories::Mended>,
    // Signs that something may be wrong, apart from the findings: never failing, never baselined (req:lint-omens).
    pub omens: Vec<crate::lint::Omen>,
    pub counts: BTreeMap<String, usize>,
    pub new: usize,
    // True on a daemon that holds no code, where findings only code can show are left out.
    pub without_code: bool,
}

/// A baseline as `status` and `adopt` show it; the entries stay in the store.
#[derive(Debug, Clone, Serialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
pub struct BaselineSummary {
    pub date: String,
    pub reason: String,
    pub count: usize,
}

/// `status`: the store, the build and the graph at a glance.
#[derive(Debug, Clone, Serialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
pub struct StatusAnswer {
    pub repository: String,
    pub roots: crate::model::Roots,
    pub languages: Vec<String>,
    pub version: String,
    pub supported_languages: Vec<String>,
    // Each configured language, in the config's order, with the tier this build reads it at.
    pub tiers: Vec<LanguageTier>,
    // How many nodes of each kind.
    pub nodes: BTreeMap<String, usize>,
    pub edges: usize,
    pub unresolved_imports: usize,
    pub malformed: usize,
    pub scope: crate::model::Scope,
    pub baseline: Option<BaselineSummary>,
    // True on a daemon that serves a store for a team and holds no code.
    pub shared: bool,
    // Set on a machine whose intent is a copy of a team's server's.
    pub server: Option<ServerCopy>,
    // This checkout's current ticket, the one last started here; none when no ticket is.
    pub current: Option<CurrentTicket>,
    // This grimoire's name in its daemon's library; none on a daemon serving one store alone.
    pub grimoire: Option<String>,
    // Every grimoire that daemon serves, once for each checkout, in name order; empty on a daemon serving one store alone.
    pub library: Vec<LibraryEntry>,
}

/// A configured language and the tier the running build reads it at:
/// 2 declarations and imports, 1 declarations alone, 0 comments alone.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
pub struct LanguageTier {
    pub language: String,
    // None when the build cannot read the language at any tier.
    pub tier: Option<u8>,
}

/// One grimoire of a daemon's library, as `status` lists it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
pub struct LibraryEntry {
    pub name: String,
    pub repository: String,
    // Where it comes from: the team's server it is a copy of, that server's address without a grimoire's /g/<name>; none for a store on this machine.
    pub server: Option<String>,
    // That checkout's current ticket, for the checkouts of the grimoire answering; none for another grimoire's.
    pub current: Option<CurrentTicket>,
}

/// A checkout's current ticket, the one last started there, as `status` names it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
pub struct CurrentTicket {
    pub id: String,
    pub title: String,
}

/// What a machine's intent is a copy of: the server's address, when the copy
/// was last fetched (UTC, RFC 3339; none when it never was), whether the
/// server answered the last time it was asked, and whether it then refused
/// this machine's token, which is not the same as being away.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
pub struct ServerCopy {
    pub address: String,
    pub fetched: Option<String>,
    pub reachable: bool,
    pub refused: bool,
}

/// Every write of a page, requirement or ticket: the node as it now is, the
/// new findings it created, and how many inherited ones sit on it. An edit of
/// a change's relies_on adds the waiting answers it closed and reopened, and
/// filing or editing a ticket the tickets whose priority it moved besides.
#[derive(Debug, Clone, Serialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
pub struct Written {
    pub node: NodeView,
    pub warnings: Vec<Finding>,
    pub baselined: usize,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[cfg_attr(feature = "ts", ts(optional))]
    pub settled: Option<Vec<Summary>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[cfg_attr(feature = "ts", ts(optional))]
    pub reopened: Option<Vec<Summary>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[cfg_attr(feature = "ts", ts(optional))]
    pub reprioritised: Option<Vec<Summary>>,
}

/// `brew_ticket`: the ticket, what it rests on, and the gaps and questions near it.
#[derive(Debug, Clone, Serialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
pub struct Brewed {
    pub node: NodeView,
    // The assumptions to challenge, in full: what the ticket names directly, every requirement reached and every node carrying a decision.
    pub rests_on: Vec<RestsOn>,
    // The rest of what it rests on, by reference alone; the rests_on tool answers the whole walk (req:tool-brew-ticket).
    pub reaches: Reaches,
    pub related: Vec<Summary>,
    // The recipes resting on what the ticket touches, each with whether it is due (req:recipe-in-tickets).
    pub recipes: Vec<RecipeState>,
    // The changes this checkout closed `when: bottled` that have not been bottled: a warning each, never a refusal (req:tool-brew-ticket).
    pub waiting: Vec<WaitingHere>,
    // Who has the ticket checked out now, the caller among them, and since when (req:ticket-checked-out).
    pub checked_out: Vec<crate::model::CheckedOut>,
    // Each who had it checked out before this start but the caller: a warning each, never a refusal.
    pub others: Vec<Holder>,
}

/// Someone else who has a ticket checked out, met by a start or a close: who and since when, both null for one
/// started before who was recorded, and the sentence to show (req:ticket-checked-out).
#[derive(Debug, Clone, Serialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
pub struct Holder {
    pub by: Option<String>,
    pub since: Option<String>,
    pub warning: String,
}

/// The rest of a ticket's walk, nearest first: how many nodes, and their references.
#[derive(Debug, Clone, Serialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
pub struct Reaches {
    pub count: usize,
    pub refs: Vec<Reference>,
}

/// A change this checkout closed `when: bottled` that has not been bottled yet, with the warning a start here gives.
#[derive(Debug, Clone, Serialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
pub struct WaitingHere {
    pub ticket: String,
    pub title: String,
    pub warning: String,
}

/// `close_ticket`: the bottled ticket, new findings on what it touched, the
/// touched nodes that never changed and what changed that it never touched
/// with the intent resting on it (each null when it was never started), the
/// gaps and questions whose answers closed with it, and for an answer closed
/// with the change that applies it, that change.
#[derive(Debug, Clone, Serialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
pub struct Closed {
    pub node: NodeView,
    pub findings: Vec<Finding>,
    pub baselined: usize,
    pub untouched: Option<Vec<Reference>>,
    pub unnamed: Option<Vec<RestsOn>>,
    pub settled: Vec<Summary>,
    // The recipes it reached that are due: above what it touched, or changed since it started; null with no code held.
    pub due: Option<Vec<RecipeState>>,
    // A gap or question closed with `change`: that change, filed in the same write, as inscribe_ticket answers it.
    #[serde(skip_serializing_if = "Option::is_none")]
    #[cfg_attr(feature = "ts", ts(optional))]
    pub filed: Option<Written>,
    // Each but the caller who had the ticket checked out, with a note that it was closed all the same (req:ticket-checked-out).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    #[cfg_attr(feature = "ts", ts(as = "Option<Vec<Holder>>", optional))]
    pub others: Vec<Holder>,
}

/// `bottle_ticket`: whether a ticket was bottled, and when one was what its close
/// answers; then the changes still simmering.
#[derive(Debug, Clone, Serialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
pub struct Bottled {
    pub bottled: bool,
    pub node: Option<NodeView>,
    pub findings: Vec<Finding>,
    pub baselined: usize,
    pub untouched: Option<Vec<Reference>>,
    pub unnamed: Option<Vec<RestsOn>>,
    pub settled: Vec<Summary>,
    pub due: Option<Vec<RecipeState>>,
    pub waiting: Vec<Summary>,
    // Why nothing was bottled, when the reason is not that no ticket was waiting.
    #[serde(skip_serializing_if = "Option::is_none")]
    #[cfg_attr(feature = "ts", ts(optional))]
    pub note: Option<String>,
}

/// A test block in the code that still names a renamed node.
#[derive(Debug, Clone, Serialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
pub struct StillNamed {
    pub test: Reference,
    pub file: String,
    pub line: u32,
}

/// `rename_node`: the node under its new name, and the code still naming the old one.
#[derive(Debug, Clone, Serialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
pub struct Renamed {
    pub node: NodeView,
    pub still_named_in: Vec<StillNamed>,
}

/// `reorder`: the band and its order as set.
#[derive(Debug, Clone, Serialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
pub struct Reordered {
    pub priority: Priority,
    pub order: Vec<Reference>,
}

/// `configure`: the config as it now is, and how many findings are new.
#[derive(Debug, Clone, Serialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
pub struct Configured {
    pub config: StoreConfig,
    pub new: usize,
}

/// `adopt`: the baseline as recorded, and how many entries it replaced (null when there was none).
#[derive(Debug, Clone, Serialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
pub struct Adopted {
    pub baseline: BaselineSummary,
    pub replaced: Option<usize>,
}

/// `delete_node`: what was deleted, and the edges it left dangling; a
/// deleted change adds the waiting answers it closed and reopened.
#[derive(Debug, Clone, Serialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
pub struct Deleted {
    pub deleted: Reference,
    pub warnings: Vec<Finding>,
    pub baselined: usize,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[cfg_attr(feature = "ts", ts(optional))]
    pub settled: Option<Vec<Summary>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[cfg_attr(feature = "ts", ts(optional))]
    pub reopened: Option<Vec<Summary>>,
}

/// `withdraw_ticket`: the change as bottled, with nothing of it applied, and the answers it was
/// waiting to apply that closed, another change having applied them, or reopened.
#[derive(Debug, Clone, Serialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
pub struct Withdrawn {
    pub node: NodeView,
    pub settled: Vec<Summary>,
    pub reopened: Vec<Summary>,
}

/// `judge_recipe`: the recipe judged, the judgement recorded, and whether the recipe is still due
/// over this checkout's code, null where none is held.
#[derive(Debug, Clone, Serialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
pub struct Judged {
    pub recipe: Reference,
    pub judgement: Judgement,
    pub due: Option<bool>,
}

/// `source`: a node's file, its text, the node's line, and what the graph attaches to the file.
#[derive(Debug, Clone, Serialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
pub struct SourceAnswer {
    pub file: Reference,
    pub language: String,
    pub text: String,
    pub focus: u32,
    // True when the file has more lines than the config's plain-file limit: shown unhighlighted.
    pub plain: bool,
    pub annotations: Vec<Annotation>,
}

/// Each answer type with the tools that answer with it. The generated
/// `ToolAnswers` map and the check that every tool has one are built from this.
macro_rules! answers {
    ($($ty:ty => [$($tool:literal),+]),+ $(,)?) => {
        $(impl Answer for $ty {
            const TOOLS: &'static [&'static str] = &[$($tool),+];
        })+

        /// Every tool with the TypeScript name of its answer, in declaration order.
        #[cfg(feature = "ts")]
        pub fn tool_answers(cfg: &ts_rs::Config) -> Vec<(&'static str, String)> {
            let mut out = Vec::new();
            $(for tool in <$ty as Answer>::TOOLS {
                out.push((*tool, <$ty as ts_rs::TS>::name(cfg)));
            })+
            out
        }

        /// Visit every answer type, to reach every type they contain.
        #[cfg(feature = "ts")]
        fn visit_answers(v: &mut impl ts_rs::TypeVisitor) {
            $(v.visit::<$ty>();)+
        }
    };
}

answers! {
    NodesAnswer => ["find"],
    NodeAnswer => ["get", "record_decision", "supersede"],
    Outline => ["outline"],
    RestsOnAnswer => ["rests_on"],
    RestsOnMe => ["rests_on_me"],
    CoverageAnswer => ["coverage"],
    Board => ["board"],
    TicketsAnswer => ["related_tickets"],
    LintAnswer => ["lint"],
    SourceLocation => ["module_source"],
    SourceAnswer => ["source"],
    StatusAnswer => ["status"],
    Written => ["put_page", "patch_page", "put_requirement", "inscribe_ticket", "update_ticket", "retire_node"],
    Brewed => ["brew_ticket"],
    Closed => ["close_ticket"],
    Bottled => ["bottle_ticket"],
    Renamed => ["rename_node"],
    Reordered => ["reorder"],
    Configured => ["configure"],
    Adopted => ["adopt"],
    Deleted => ["delete_node"],
    Withdrawn => ["withdraw_ticket"],
    Judged => ["judge_recipe"],
}

/// The UI's generated TypeScript: every type the answers and the tool error
/// reach, each declared once and sorted by name, then the `ToolAnswers` map
/// from each tool's name to its answer. Only builds the text; writing and
/// comparing the file is a test's, since the core does no I/O.
#[cfg(feature = "ts")]
pub fn typescript() -> String {
    use std::any::TypeId;
    use std::collections::{BTreeSet, HashSet};
    use ts_rs::{Config, TypeVisitor, TS};

    struct Collect<'a> {
        cfg: &'a Config,
        ids: HashSet<TypeId>,
        names: BTreeSet<String>,
        decls: BTreeMap<String, String>,
    }
    impl TypeVisitor for Collect<'_> {
        fn visit<T: TS + 'static + ?Sized>(&mut self) {
            if !self.ids.insert(TypeId::of::<T>()) {
                return;
            }
            // A type with an output path is one of ours, derived and declarable;
            // strings, numbers, vectors and maps are inlined by name.
            if T::output_path().is_some() {
                let name = T::ident(self.cfg);
                if self.names.insert(name.clone()) {
                    self.decls.insert(name, T::decl(self.cfg));
                }
            }
            T::visit_dependencies(self);
            T::visit_generics(self);
        }
    }

    let cfg = Config::new().with_large_int("number");
    let mut c = Collect { cfg: &cfg, ids: HashSet::new(), names: BTreeSet::new(), decls: BTreeMap::new() };
    visit_answers(&mut c);
    // What a tool answers when it refuses: the other half of every answer.
    c.visit::<crate::ops::ToolError>();
    let mut out = String::from("// Generated from the Rust answer types by crates/core/src/answer.rs.\n// Do not edit: `cargo test -p grimoire-core --test typescript -- --ignored` rewrites it.\n\n");
    for decl in c.decls.values() {
        out.push_str("export ");
        out.push_str(decl.trim_end());
        out.push_str("\n\n");
    }
    out.push_str("/** Each tool's answer, by tool name. */\nexport type ToolAnswers = {\n");
    for (tool, ty) in tool_answers(&cfg) {
        out.push_str(&format!("  {tool}: {ty};\n"));
    }
    out.push_str("};\n");
    out
}
