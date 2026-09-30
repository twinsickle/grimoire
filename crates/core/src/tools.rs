//! # Module contract
//!
//! ```json
//! {"purpose":"Dispatch every tool by name over JSON so the MCP server, the JSON API and the CLI share one implementation and one set of definitions: a read tool to the read side, a write tool to the write side.","criteria":[{"id":"error-codes","statement":"Every error is {error, message, details?} with error one of not_found, invalid, conflict or store_unavailable, and invalid carries the violated fields in details."}]}
//! ```

use serde::Deserialize;
use serde_json::Value;

use crate::answer::Answer;
use crate::model::Reference;
use crate::ops::{Changed, ToolError};

mod definitions;
mod read;
mod state;
mod write;

pub use definitions::{definitions, ToolDef};
pub use state::{Build, SourceError, SourceReader, State, SOURCE_LIMIT, SOURCE_LINES};

/// What a call produced: the JSON answer and, for writes, what changed so the
/// caller can persist it.
#[derive(Debug)]
pub struct Outcome {
    pub output: Value,
    pub changed: Option<Changed>,
}

/// Answer tool `name` with `a`, which must be the answer type declared for
/// it; a debug build refuses any other, so every answer has its declared shape.
fn answer<A: Answer>(name: &str, a: A, changed: Option<Changed>) -> Result<Outcome, ToolError> {
    debug_assert!(A::TOOLS.contains(&name), "`{name}` answered with the type declared for {:?}", A::TOOLS);
    Ok(Outcome { output: to_value(&a), changed })
}

fn parse<T: for<'de> Deserialize<'de>>(input: Value) -> Result<T, ToolError> {
    serde_json::from_value(input).map_err(|e| ToolError::invalid(format!("input does not match the tool's schema: {e}"), vec![]))
}

fn to_value<T: serde::Serialize>(v: &T) -> Value {
    serde_json::to_value(v).expect("serialisable")
}

pub const TOOL_NAMES: [&str; 27] = [
    "find",
    "get",
    "outline",
    "rests_on",
    "rests_on_me",
    "coverage",
    "board",
    "related_tickets",
    "lint",
    "module_source",
    "source",
    "put_page",
    "patch_page",
    "put_requirement",
    "record_decision",
    "supersede",
    "retire_node",
    "file_ticket",
    "update_ticket",
    "start_ticket",
    "close_ticket",
    "delete_node",
    "rename_node",
    "adopt",
    "configure",
    "reorder",
    "status",
];

pub fn is_write(name: &str) -> bool {
    matches!(name, "put_page" | "patch_page" | "put_requirement" | "record_decision" | "supersede" | "retire_node" | "file_ticket" | "update_ticket" | "start_ticket" | "close_ticket" | "delete_node" | "rename_node" | "adopt" | "configure" | "reorder")
}

/// Run one tool. `today` is the ISO date used when a decision has none.
pub fn call(state: &mut State, name: &str, input: Value, today: &str) -> Result<Outcome, ToolError> {
    if is_write(name) {
        write::call(state, name, input, today)
    } else {
        read::call(state, name, input)
    }
}

#[derive(Deserialize)]
struct RefIn {
    r#ref: Reference,
}

#[cfg(test)]
pub(crate) mod fixtures {
    use super::{Build, State};
    use crate::extraction::fixtures::extraction;
    use crate::graph::fixtures::store;

    pub fn state() -> State {
        State::new(store(), extraction(), vec![])
    }

    pub fn built(mut s: State, languages: &[&str]) -> State {
        s.set_build(Build { version: "0.0.0".into(), supported_languages: languages.iter().map(|l| l.to_string()).collect() });
        s
    }
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::call;
    use super::fixtures::state;

    /// # Test contract
    ///
    /// ```json
    /// {"test": "errors-carry-codes", "checks": ["crates/core/src/query/lookup#get", "crates/core/src/tools#error-codes"]}
    /// ```
    #[test]
    fn errors_carry_codes() {
        let mut s = state();
        let e = call(&mut s, "get", json!({ "ref": "page:missing" }), "2026-09-27").unwrap_err();
        assert_eq!(serde_json::to_value(&e).unwrap()["error"], "not_found");
        let e = call(&mut s, "put_page", json!({ "slug": "Bad Slug", "title": "", "body": "b", "links": [] }), "2026-09-27").unwrap_err();
        let v = serde_json::to_value(&e).unwrap();
        assert_eq!(v["error"], "invalid");
        assert_eq!(v["details"].as_array().unwrap().len(), 2);
    }
}
