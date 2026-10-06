//! # Module contract
//!
//! ```json
//! {"purpose":"Hold the graph of pages, requirements, tickets, modules and criteria, and answer queries and lints over it.","criteria":[{"id":"root-reexports","statement":"The crate root re-exports what the other crates build on: Graph, Store, Malformed, Extraction, FileExtraction, ToolError, State, Build and every model type, each reached as grimoire_core::<name>."}]}
//! ```
//!
//! Layout: [`model`] defines the records and references; [`store`] holds and
//! validates the stored ones; [`extraction`] joins per-file extraction into
//! modules; [`graph`] joins store and extraction; [`query`] and [`lint`] read
//! the graph; [`ops`] write the store; [`export`] hands a store's intent to a machine; [`scaffold`] builds a store from
//! contracts; [`tools`] dispatches all of it by name over JSON.

pub mod advisories;
pub mod answer;
pub mod export;
pub mod extraction;
pub mod fingerprint;
pub mod graph;
pub mod lint;
pub mod model;
pub mod ops;
pub mod query;
pub mod scaffold;
pub mod store;
pub mod tools;

pub use extraction::{Extraction, FileExtraction};
pub use graph::Graph;
pub use model::*;
pub use ops::ToolError;
pub use store::{Malformed, Store};
pub use tools::{Build, State};
