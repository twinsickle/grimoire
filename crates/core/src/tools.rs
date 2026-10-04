//! # Module contract
//!
//! ```json
//! {"purpose":"Dispatch every tool by name over JSON so the MCP server, the JSON API and the CLI share one implementation and one set of definitions: a read tool to the read side, a write tool to the write side.","criteria":[{"id":"error-codes","statement":"Every error is {error, message, details?} with error one of not_found, invalid, conflict or store_unavailable, and invalid carries the violated fields in details."},{"id":"trims-input","statement":"Before a write tool runs, every string in its input, at any depth, has its leading and trailing whitespace taken off, so what it stores is trimmed; a read tool's input is left as given."}]}
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

pub const TOOL_NAMES: [&str; 28] = [
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
    "land_ticket",
    "delete_node",
    "rename_node",
    "adopt",
    "configure",
    "reorder",
    "status",
];

/// The writes that change intent and so land in a ticket's proposal; they, and the reads, take `ticket`.
pub fn proposes(name: &str) -> bool {
    matches!(name, "put_page" | "patch_page" | "put_requirement" | "record_decision" | "supersede" | "retire_node" | "delete_node")
}

pub fn is_write(name: &str) -> bool {
    matches!(name, "put_page" | "patch_page" | "put_requirement" | "record_decision" | "supersede" | "retire_node" | "file_ticket" | "update_ticket" | "start_ticket" | "close_ticket" | "land_ticket" | "delete_node" | "rename_node" | "adopt" | "configure" | "reorder")
}

/// The writes that cannot be taken back by another write: a deletion, and a close or landing, which applies a proposal and discards the ticket's body.
pub fn is_destructive(name: &str) -> bool {
    matches!(name, "delete_node" | "close_ticket" | "land_ticket")
}

/// Run one tool. `today` is the ISO date used when a decision has none.
pub fn call(state: &mut State, name: &str, mut input: Value, today: &str) -> Result<Outcome, ToolError> {
    if is_write(name) {
        // Every string a write stores is trimmed here, once, so no write tool can forget.
        crate::store::trim_strings(&mut input);
        write::call(state, name, input, today)
    } else {
        read::call(state, name, input)
    }
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct RefIn {
    r#ref: Reference,
}

/// The `ticket` a call names, taken off its input so the rest parses as
/// before: for a read, the proposal to answer over; for a write that changes
/// intent, the proposal to land in. A slug or a `ticket:` reference.
fn take_ticket(input: &mut Value) -> Result<Option<String>, ToolError> {
    match input.as_object_mut().and_then(|o| o.remove("ticket")) {
        None | Some(Value::Null) => Ok(None),
        Some(Value::String(s)) => Ok(Some(s.strip_prefix("ticket:").unwrap_or(&s).to_string())),
        Some(other) => Err(ToolError::invalid(format!("`ticket` must be a ticket slug or reference, not {other}"), vec![])),
    }
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
        let e = call(&mut s, "put_page", json!({ "slug": "Bad Slug", "title": "", "body": "b", "links": [], "ticket": "later-1" }), "2026-09-27").unwrap_err();
        let v = serde_json::to_value(&e).unwrap();
        assert_eq!(v["error"], "invalid");
        assert_eq!(v["details"].as_array().unwrap().len(), 2);
        // A change of intent outside any ticket is refused, naming the field.
        let e = call(&mut s, "put_page", json!({ "slug": "aim", "title": "Aim", "body": "b", "links": [] }), "2026-09-27").unwrap_err();
        let v = serde_json::to_value(&e).unwrap();
        assert_eq!((v["error"].clone(), v["details"][0]["field"].clone()), (json!("invalid"), json!("ticket")));
    }

    /// # Test contract
    ///
    /// ```json
    /// {"test": "writes-trim-their-input", "checks": ["crates/core/src/tools#trims-input"]}
    /// ```
    #[test]
    fn writes_trim_their_input() {
        let mut s = state();
        // Written under the fixture's active ticket, so read from its proposal.
        call(&mut s, "put_page", json!({ "slug": " aim ", "title": "Aim ", "body": "\n**Why.** b\n\n", "links": [" page:overview"], "ticket": " later-1 " }), "2026-09-30").unwrap();
        let page = &s.store().proposals["later-1"].pages["aim"].record;
        assert_eq!((page.title.as_str(), page.body.as_str(), page.links[0].to_string()), ("Aim", "**Why.** b", "page:overview".to_string()));
        // Strings at any depth: a requirement's scenarios.
        let req = json!({ "slug": "r", "title": "R", "statement": "It SHALL hold.\n", "scenarios": [{ "title": " t", "given": "g ", "when": "w", "then": "t\t" }], "covered_by": [], "ticket": "later-1" });
        call(&mut s, "put_requirement", req, "2026-09-30").unwrap();
        let r = &s.store().proposals["later-1"].requirements["r"].record;
        assert_eq!((r.statement.as_str(), r.scenarios[0].title.as_str(), r.scenarios[0].given.as_str(), r.scenarios[0].then.as_str()), ("It SHALL hold.", "t", "g", "t"));
        // A read's input is as given: a reference with a space is not a reference.
        assert!(call(&mut s, "get", json!({ "ref": " page:aim" }), "2026-09-30").is_err());
    }
}
