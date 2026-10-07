//! # Module contract
//!
//! ```json
//! {"purpose":"Gather the read queries over the graph under one path, one module per seam: lookup, outline, walks, coverage, board and source.","criteria":[{"id":"documented-shapes","statement":"Every read tool answers with the answer type declared for it in the answer module, so the shape is written once and is the same over every transport."}]}
//! ```

mod board;
mod coverage;
mod lookup;
mod outline;
mod site;
mod source;
mod walks;

pub use board::{board, related_tickets, Board, BoardFilter, BoardTicket};
pub use coverage::{module_coverage, requirement_coverage, requirements_covering, tests_checking, CoveringCriterion, CriterionCoverage, ModuleSummary};
pub use lookup::{find, summary, ticket_summary, view, EdgeView, NodeView, Summary};
pub use outline::{outline, Outline, Rollup, Tree};
pub use site::{site, Site, SiteEntry, SitePage, SiteRequirement};
pub use source::{module_source, source_annotations, Annotation, AnnotationKind, Linked};
pub use walks::{intent_of, recipes_resting, rests_on, rests_on_me, Affected, RecipeState, RestsOn, RestsOnMe};
