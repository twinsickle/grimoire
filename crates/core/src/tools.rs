//! # Module contract
//!
//! ```json
//! {"purpose":"Dispatch every tool by name over JSON so the MCP server, the JSON API and the CLI share one implementation and one set of definitions: a read tool to the read side, a write tool to the write side.","criteria":[{"id":"as-caller","statement":"call_by runs a tool as call does with its caller, who is calling and when, held on the state for that call alone and cleared whatever it answers; call runs one with no caller."},{"id":"error-codes","statement":"Every error is {error, message, details?} with error one of not_found, invalid, conflict or store_unavailable, and invalid carries the violated fields in details."},{"id":"checkout-ignored","statement":"call takes `checkout` off the input before any tool runs, since it only routes a call to its checkout's daemon, so no tool sees it and a tool that refuses unknown fields accepts it; answers_any_checkout says which tools a daemon answers with none named, status alone."},{"id":"trims-input","statement":"Before a write tool runs, every string in its input, at any depth, has its leading and trailing whitespace taken off, so what it stores is trimmed, but for the passage a body edit replaces and its replacement, `replace`, which are matched and put exactly as given; a read tool's input is left as given."},{"id":"former-names","statement":"call answers a tool's former name, file_ticket, start_ticket or land_ticket, as the tool it now names, inscribe_ticket, brew_ticket or bottle_ticket, and canonical gives that tool for a former name and the name itself for any other; no former name is in the tools listed or defined."},{"id":"schema-errors","statement":"An input that does not match a tool's schema is refused as invalid naming the path of the field it is about, in the message and in details, as scenarios[0].title, a missing or unknown field named beneath the path it was met at; and a field the tool does not know is answered with the fields the dispatcher takes beside the tool's own, checkout for every tool and ticket for a read and for a write that takes it."}]}
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

/// Parse a tool's input, saying where it does not match: the path of the field, as `scenarios[0].title`,
/// in the message and in `details`.
fn parse<T: for<'de> Deserialize<'de>>(input: Value) -> Result<T, ToolError> {
    serde_path_to_error::deserialize(input).map_err(|e| {
        let reason = e.inner().to_string();
        let at = field_path(&e.path().to_string(), &reason);
        match at {
            Some(at) => ToolError::invalid(format!("input does not match the tool's schema at `{at}`: {reason}"), vec![crate::store::FieldError::new(&at, reason)]),
            None => ToolError::invalid(format!("input does not match the tool's schema: {reason}"), vec![]),
        }
    })
}

/// The path of the field an error is about: where it was met, and for a missing or unknown field
/// the field itself beneath it; none at the input's own top with no field named.
fn field_path(path: &str, reason: &str) -> Option<String> {
    let path = if path == "." || path == "?" { "" } else { path };
    let named = ["missing field `", "unknown field `"].iter().find_map(|lead| reason.strip_prefix(lead)).and_then(|rest| rest.split('`').next());
    match (path, named) {
        ("", None) => None,
        ("", Some(field)) => Some(field.to_string()),
        (path, Some(field)) if !path.ends_with(&format!(".{field}")) && path != field => Some(format!("{path}.{field}")),
        (path, _) => Some(path.to_string()),
    }
}

/// The fields every tool takes beside its own, which the dispatcher takes off before it parses:
/// said with a field the tool does not know, since the tool's own list leaves them out.
fn with_dispatcher_fields(name: &str, e: ToolError) -> ToolError {
    match e {
        ToolError::Invalid { message, details } if message.contains("unknown field `") => {
            let also = if !is_write(name) || takes_ticket(name) { "`checkout` and `ticket`" } else { "`checkout`" };
            ToolError::Invalid { message: format!("{message}; {name} also takes {also}"), details }
        }
        other => other,
    }
}

fn to_value<T: serde::Serialize>(v: &T) -> Value {
    serde_json::to_value(v).expect("serialisable")
}

pub const TOOL_NAMES: [&str; 31] = [
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
    "patch_requirement",
    "record_decision",
    "supersede",
    "retire_node",
    "inscribe_ticket",
    "update_ticket",
    "brew_ticket",
    "close_ticket",
    "bottle_ticket",
    "withdraw_ticket",
    "delete_node",
    "rename_node",
    "adopt",
    "configure",
    "reorder",
    "judge_recipe",
    "status",
];

/// The names the ticket tools had before 0168, each with the tool it now names: still answered,
/// so a script or an agent given the old name goes on working, but listed nowhere.
pub const FORMER_NAMES: [(&str, &str); 3] = [("file_ticket", "inscribe_ticket"), ("start_ticket", "brew_ticket"), ("land_ticket", "bottle_ticket")];

/// The tool `name` names: itself, or for a former name the tool it now names.
pub fn canonical(name: &str) -> &str {
    FORMER_NAMES.iter().find(|(former, _)| *former == name).map_or(name, |(_, now)| now)
}

/// The writes that change intent and so go into a ticket's proposal; they, and the reads, take `ticket`.
pub fn proposes(name: &str) -> bool {
    matches!(name, "put_page" | "patch_page" | "put_requirement" | "patch_requirement" | "record_decision" | "supersede" | "retire_node" | "delete_node")
}

/// The writes that take `ticket`: those that change intent, update_ticket,
/// whose edits to another ticket's references wait in a change's proposal, and
/// judge_recipe, whose judgement names the ticket it was made in.
pub fn takes_ticket(name: &str) -> bool {
    proposes(name) || name == "update_ticket" || name == "judge_recipe"
}

pub fn is_write(name: &str) -> bool {
    matches!(
        name,
        "put_page"
            | "patch_page"
            | "put_requirement"
            | "patch_requirement"
            | "record_decision"
            | "supersede"
            | "retire_node"
            | "inscribe_ticket"
            | "update_ticket"
            | "brew_ticket"
            | "close_ticket"
            | "bottle_ticket"
            | "withdraw_ticket"
            | "delete_node"
            | "rename_node"
            | "adopt"
            | "configure"
            | "reorder"
            | "judge_recipe"
    )
}

/// The tools a daemon serving several checkouts of a grimoire answers with no `checkout` named:
/// `status`, which lists the checkouts a caller may name (req:machine-checkouts).
pub fn answers_any_checkout(name: &str) -> bool {
    name == "status"
}

/// The writes that cannot be taken back by another write: a deletion, and a close or bottling, which applies a proposal and discards the ticket's body.
pub fn is_destructive(name: &str) -> bool {
    matches!(name, "delete_node" | "close_ticket" | "bottle_ticket" | "withdraw_ticket")
}

/// Run one tool. `today` is the ISO date used when a decision has none.
pub fn call(state: &mut State, name: &str, input: Value, today: &str) -> Result<Outcome, ToolError> {
    call_by(state, name, input, today, None)
}

/// `call`, as `caller`, who is calling and when: a start adds them to who has its ticket checked out, and a start
/// or a close names the others who have it (req:ticket-checked-out). Set for this call alone, whatever it answers.
pub fn call_by(state: &mut State, name: &str, input: Value, today: &str, caller: Option<crate::model::CheckedOut>) -> Result<Outcome, ToolError> {
    let name = canonical(name);
    state.caller = caller;
    let answered = dispatch(state, name, input, today);
    state.caller = None;
    answered
}

fn dispatch(state: &mut State, name: &str, mut input: Value, today: &str) -> Result<Outcome, ToolError> {
    // The caller's checkout routed the call here; the tool has no use for it.
    if let Some(o) = input.as_object_mut() {
        o.remove("checkout");
    }
    let answered = if is_write(name) {
        // Every string a write stores is trimmed here, once, so no write tool can forget; but a body edit's
        // passage and its replacement, matched against the body as it is, which the whole record's validation then judges.
        let replace = input.as_object_mut().and_then(|o| o.remove("replace"));
        crate::store::trim_strings(&mut input);
        if let (Some(replace), Some(o)) = (replace, input.as_object_mut()) {
            o.insert("replace".to_string(), replace);
        }
        write::call(state, name, input, today)
    } else {
        read::call(state, name, input)
    };
    answered.map_err(|e| with_dispatcher_fields(name, e))
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct RefIn {
    r#ref: Reference,
}

/// The `ticket` a call names, taken off its input so the rest parses as
/// before: for a read, the proposal to answer over; for a write that changes
/// intent, the proposal to go into. A slug or a `ticket:` reference.
fn take_ticket(input: &mut Value) -> Result<Option<String>, ToolError> {
    match input.as_object_mut().and_then(|o| o.remove("ticket")) {
        None | Some(Value::Null) => Ok(None),
        Some(Value::String(s)) => Ok(Some(s.strip_prefix("ticket:").unwrap_or(&s).to_string())),
        Some(other) => Err(ToolError::invalid(format!("`ticket` must be a ticket slug or reference, not {other}"), vec![crate::store::FieldError::new("ticket", "not a ticket slug or reference")])),
    }
}

#[cfg(test)]
mod schema_errors {
    use super::*;
    use serde_json::json;

    /// # Test contract
    ///
    /// ```json
    /// {"test": "a-schema-error-names-the-fields-path", "checks": ["crates/core/src/tools#schema-errors"]}
    /// ```
    #[test]
    fn a_schema_error_names_the_fields_path() {
        let mut s = fixtures::state();
        let invalid = |r: Result<Outcome, ToolError>| match r {
            Err(ToolError::Invalid { message, details }) => (message, details.into_iter().map(|d| d.field).collect::<Vec<_>>()),
            other => panic!("expected invalid, got {other:?}"),
        };
        // A scenario without its title: the path says which.
        let req = json!({ "slug": "r", "title": "R", "statement": "It SHALL.", "scenarios": [{ "given": "g", "when": "w", "then": "t" }], "covered_by": [], "ticket": "later-1" });
        let (message, fields) = invalid(call(&mut s, "put_requirement", req, "2026-10-06"));
        assert!(message.contains("`scenarios[0].title`") && message.contains("missing field `title`"), "{message}");
        assert_eq!(fields, ["scenarios[0].title"]);
        // A field missing at the top: named alone.
        let (message, fields) = invalid(call(&mut s, "rests_on_me", json!({}), "2026-10-06"));
        assert!(message.contains("at `ref`"), "{message}");
        assert_eq!(fields, ["ref"]);
        // A wrong type deep down: the path to it.
        let req = json!({ "slug": "r", "title": "R", "statement": "It SHALL.", "scenarios": [{ "title": 3, "given": "g", "when": "w", "then": "t" }], "covered_by": [], "ticket": "later-1" });
        let (message, _) = invalid(call(&mut s, "put_requirement", req, "2026-10-06"));
        assert!(message.contains("`scenarios[0].title`"), "{message}");
    }

    /// # Test contract
    ///
    /// ```json
    /// {"test": "the-unknown-field-hint-names-the-dispatchers-fields", "checks": ["crates/core/src/tools#schema-errors"]}
    /// ```
    #[test]
    fn the_unknown_field_hint_names_the_dispatchers_fields() {
        let mut s = fixtures::state();
        let mut message = |name: &str, input: Value| match call(&mut s, name, input, "2026-10-06") {
            Err(ToolError::Invalid { message, .. }) => message,
            other => panic!("expected invalid, got {other:?}"),
        };
        let m = message("put_page", json!({ "slug": "p", "title": "P", "body": "b", "links": [], "colour": "red" }));
        assert!(m.contains("unknown field `colour`") && m.contains("also takes `checkout` and `ticket`"), "{m}");
        let m = message("get", json!({ "ref": "page:p", "colour": "red" }));
        assert!(m.contains("also takes `checkout` and `ticket`"), "a read takes ticket too: {m}");
        // A ticket of the wrong type names the field too.
        match call(&mut s, "get", json!({ "ref": "page:p", "ticket": 42 }), "2026-10-06") {
            Err(ToolError::Invalid { details, .. }) => assert_eq!(details.iter().map(|d| d.field.as_str()).collect::<Vec<_>>(), ["ticket"]),
            other => panic!("expected invalid, got {other:?}"),
        }
        let mut message = |name: &str, input: Value| match call(&mut s, name, input, "2026-10-06") {
            Err(ToolError::Invalid { message, .. }) => message,
            other => panic!("expected invalid, got {other:?}"),
        };
        let m = message("reorder", json!({ "priority": "next", "order": [], "colour": "red" }));
        assert!(m.contains("also takes `checkout`") && !m.contains("`ticket`"), "{m}");
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
        s.set_build(Build { version: "0.0.0".into(), supported_languages: languages.iter().map(|l| l.to_string()).collect(), tiers: languages.iter().map(|l| (l.to_string(), 2)).collect() });
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
    /// {"test": "a-former-tool-name-is-answered-and-listed-nowhere", "checks": ["crates/core/src/tools#former-names"]}
    /// ```
    #[test]
    fn a_former_tool_name_is_answered_and_listed_nowhere() {
        let mut s = state();
        // Filed, started and bottled by the names they had before: each answered as the tool it now names.
        let filed = call(&mut s, "file_ticket", json!({ "title": "Old names", "kind": "change", "priority": "later", "body": "b" }), "2026-10-08").unwrap().output;
        let r = filed["node"]["ref"].clone();
        assert_eq!(filed["node"]["status"], "inscribed");
        assert_eq!(call(&mut s, "start_ticket", json!({ "ref": r }), "2026-10-08").unwrap().output["node"]["status"], "brewing");
        call(&mut s, "close_ticket", json!({ "ref": r, "decision": { "text": "Done." }, "when": "landed" }), "2026-10-08").unwrap();
        let bottled = call(&mut s, "land_ticket", json!({ "ref": r }), "2026-10-08").unwrap().output;
        assert_eq!((bottled["bottled"].clone(), bottled["node"]["status"].clone()), (json!(true), json!("bottled")));
        for (former, now) in super::FORMER_NAMES {
            assert_eq!(super::canonical(former), now);
            assert!(!super::TOOL_NAMES.contains(&former) && !super::definitions().iter().any(|d| d.name == former), "{former} is listed nowhere");
        }
        assert_eq!(super::canonical("get"), "get");
    }

    /// # Test contract
    ///
    /// ```json
    /// {"test": "writes-trim-their-input", "checks": ["crates/core/src/tools#trims-input"]}
    /// ```
    #[test]
    fn writes_trim_their_input() {
        let mut s = state();
        // Written under the fixture's brewing ticket, so read from its proposal.
        call(&mut s, "put_page", json!({ "slug": " aim ", "title": "Aim ", "body": "\n**Why.** b\n\n", "links": [" page:overview"], "ticket": " later-1 " }), "2026-09-30").unwrap();
        let page = &s.store().proposals["later-1"].pages["aim"].record;
        assert_eq!((page.title.as_str(), page.body.as_str(), page.links[0].to_string()), ("Aim", "**Why.** b", "page:overview".to_string()));
        // Strings at any depth: a requirement's scenarios.
        let req =
            json!({ "slug": "r", "title": "R", "statement": "It SHALL hold.\n", "scenarios": [{ "title": " t", "given": "g ", "when": "w", "then": "t\t" }], "covered_by": [], "ticket": "later-1" });
        call(&mut s, "put_requirement", req, "2026-09-30").unwrap();
        let r = &s.store().proposals["later-1"].requirements["r"].record;
        assert_eq!((r.statement.as_str(), r.scenarios[0].title.as_str(), r.scenarios[0].given.as_str(), r.scenarios[0].then.as_str()), ("It SHALL hold.", "t", "g", "t"));
        // But a body edit's passage and its replacement, matched exactly as given: the space after "b" goes with it.
        call(&mut s, "patch_page", json!({ "ref": "page:aim", "body": "a b c", "ticket": "later-1" }), "2026-09-30").unwrap();
        call(&mut s, "patch_page", json!({ "ref": "page:aim", "replace": { "old": "b ", "new": "" }, "ticket": "later-1" }), "2026-09-30").unwrap();
        assert_eq!(s.store().proposals["later-1"].pages["aim"].record.body, "a c");
        // A read's input is as given: a reference with a space is not a reference.
        assert!(call(&mut s, "get", json!({ "ref": " page:aim" }), "2026-09-30").is_err());
    }
}
