//! # Module contract
//!
//! ```json
//! {"purpose":"Dispatch every tool by name over JSON so the MCP server, the JSON API and the CLI share one implementation and one set of definitions.","criteria":[{"id":"one-dispatcher","statement":"A tool name and a JSON input produce the same JSON output whichever transport delivered them."},{"id":"graph-current","statement":"A read after a write sees the write; the cached graph is rebuilt whenever the store or the extraction changes."},{"id":"definitions-match","statement":"Every tool the dispatcher accepts is listed in definitions() with an input schema, and nothing is listed that it does not accept."},{"id":"error-codes","statement":"Every error is {error, message, details?} with error one of not_found, invalid, conflict or store_unavailable, and invalid carries the violated fields in details."},{"id":"write-responses","statement":"Every write answers with the node as it now exists; put_page, patch_page, put_requirement, file_ticket and update_ticket add the dangling warnings the write created, start_ticket adds rests_on and related, close_ticket adds the findings on the nodes it touched, delete_node returns the deleted reference and the dangling edges left behind."},{"id":"status","statement":"status returns the repository path, roots, languages, node counts by kind, edge count, unresolved import count and malformed file count, plus the build's version and supported_languages as the loader told State, so the answer is the same over every transport."},{"id":"lint-tool","statement":"lint runs the selected rules (all by default) and returns the findings with counts per rule; it is the same function grimoire check prints."}]}
//! ```

use serde::Deserialize;
use serde_json::{json, Value};

use crate::extraction::Extraction;
use crate::graph::Graph;
use crate::lint::{self, Rule};
use crate::model::*;
use crate::ops::{self, Changed, ToolError};
use crate::query::{self, BoardFilter};
use crate::store::{Malformed, Store};

/// What the running build is: facts the loader knows and core cannot find
/// out for itself without I/O. `status` reports them next to the store's
/// `languages` so a store naming a language this build lacks is visible.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub struct Build {
    /// The crate version; every crate in the workspace shares one.
    pub version: String,
    /// The languages this build has an extraction adapter for.
    pub supported_languages: Vec<String>,
}

impl Default for Build {
    fn default() -> Self {
        Build { version: env!("CARGO_PKG_VERSION").to_string(), supported_languages: vec![] }
    }
}

/// Everything the tools operate on, with the graph cached between calls.
#[derive(Debug, Default)]
pub struct State {
    store: Store,
    extraction: Extraction,
    malformed: Vec<Malformed>,
    build: Build,
    graph: Option<Graph>,
}

impl State {
    pub fn new(store: Store, extraction: Extraction, malformed: Vec<Malformed>) -> Self {
        State { store, extraction, malformed, build: Build::default(), graph: None }
    }
    pub fn store(&self) -> &Store {
        &self.store
    }
    pub fn extraction(&self) -> &Extraction {
        &self.extraction
    }
    pub fn build(&self) -> &Build {
        &self.build
    }
    /// Tell the state what build is running. The graph does not depend on it.
    pub fn set_build(&mut self, build: Build) {
        self.build = build;
    }
    pub fn set_store(&mut self, store: Store, malformed: Vec<Malformed>) {
        self.store = store;
        self.malformed = malformed;
        self.graph = None;
    }
    pub fn set_extraction(&mut self, extraction: Extraction) {
        self.extraction = extraction;
        self.graph = None;
    }
    pub fn graph(&mut self) -> &Graph {
        if self.graph.is_none() {
            self.graph = Some(Graph::build(&self.store, &self.extraction, &self.malformed));
        }
        self.graph.as_ref().expect("just built")
    }
    fn store_mut(&mut self) -> &mut Store {
        self.graph = None;
        &mut self.store
    }
}

/// What a call produced: the JSON answer and, for writes, what changed so the
/// caller can persist it.
#[derive(Debug)]
pub struct Outcome {
    pub output: Value,
    pub changed: Option<Changed>,
}

fn ok(v: Value) -> Result<Outcome, ToolError> {
    Ok(Outcome { output: v, changed: None })
}

fn parse<T: for<'de> Deserialize<'de>>(input: Value) -> Result<T, ToolError> {
    serde_json::from_value(input).map_err(|e| ToolError::invalid(format!("input does not match the tool's schema: {e}"), vec![]))
}

fn to_value<T: serde::Serialize>(v: &T) -> Value {
    serde_json::to_value(v).expect("serialisable")
}

pub const TOOL_NAMES: [&str; 20] = [
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
    "put_page",
    "patch_page",
    "put_requirement",
    "record_decision",
    "file_ticket",
    "update_ticket",
    "start_ticket",
    "close_ticket",
    "delete_node",
    "status",
];

pub fn is_write(name: &str) -> bool {
    matches!(name, "put_page" | "patch_page" | "put_requirement" | "record_decision" | "file_ticket" | "update_ticket" | "start_ticket" | "close_ticket" | "delete_node")
}

/// Run one tool. `today` is the ISO date used when a decision has none.
pub fn call(state: &mut State, name: &str, input: Value, today: &str) -> Result<Outcome, ToolError> {
    match name {
        "find" => {
            #[derive(Deserialize)]
            struct In {
                query: String,
                kinds: Option<Vec<Kind>>,
                limit: Option<usize>,
            }
            let i: In = parse(input)?;
            let g = state.graph();
            ok(json!({ "nodes": query::find(g, &i.query, i.kinds.as_deref(), i.limit.unwrap_or(20)) }))
        }
        "get" => {
            let i: RefIn = parse(input)?;
            let g = state.graph();
            let v = query::view(g, &i.r#ref).ok_or_else(|| ToolError::not_found(&i.r#ref))?;
            ok(json!({ "node": v }))
        }
        "outline" => {
            #[derive(Deserialize)]
            struct In {
                depth: Option<usize>,
                root: Option<Reference>,
            }
            let i: In = parse(input)?;
            if let Some(r) = &i.root {
                if r.kind != Kind::Page {
                    return Err(ToolError::invalid(format!("{r} is not a page"), vec![]));
                }
            }
            let g = state.graph();
            ok(json!({ "roots": query::outline(g, i.depth.unwrap_or(2), i.root.as_ref()) }))
        }
        "rests_on" => {
            #[derive(Deserialize)]
            struct In {
                r#ref: Reference,
                depth: Option<usize>,
            }
            let i: In = parse(input)?;
            let g = state.graph();
            if !g.contains(&i.r#ref) {
                return Err(ToolError::not_found(&i.r#ref));
            }
            ok(json!({ "nodes": query::rests_on(g, &i.r#ref, i.depth) }))
        }
        "rests_on_me" => {
            let i: RefIn = parse(input)?;
            let g = state.graph();
            if !g.contains(&i.r#ref) {
                return Err(ToolError::not_found(&i.r#ref));
            }
            ok(to_value(&query::rests_on_me(g, &i.r#ref)))
        }
        "coverage" => {
            let i: RefIn = parse(input)?;
            let g = state.graph();
            match (i.r#ref.kind, g.get(&i.r#ref)) {
                (Kind::Criterion, Some(_)) => ok(json!({ "requirements": query::requirements_covering(g, &i.r#ref) })),
                (Kind::Module, Some(crate::graph::Node::Module(m))) => ok(json!({ "criteria": query::module_coverage(g, m) })),
                (Kind::Criterion | Kind::Module, None) => Err(ToolError::not_found(&i.r#ref)),
                _ => Err(ToolError::invalid(format!("{} is not a criterion or module", i.r#ref), vec![])),
            }
        }
        "board" => {
            #[derive(Deserialize)]
            struct In {
                status: Option<Vec<Status>>,
                kinds: Option<Vec<TicketKind>>,
                priority: Option<Vec<Priority>>,
                touching: Option<Reference>,
            }
            let i: In = parse(input)?;
            let g = state.graph();
            let f = BoardFilter { status: i.status.as_deref(), kinds: i.kinds.as_deref(), priority: i.priority.as_deref(), touching: i.touching.as_ref() };
            ok(json!({ "tickets": query::board(g, &f) }))
        }
        "related_tickets" => {
            let i: RefIn = parse(input)?;
            let g = state.graph();
            match g.get(&i.r#ref) {
                Some(crate::graph::Node::Ticket(t)) => ok(json!({ "tickets": query::related_tickets(g, t) })),
                Some(_) => Err(ToolError::invalid(format!("{} is not a ticket", i.r#ref), vec![])),
                None => Err(ToolError::not_found(&i.r#ref)),
            }
        }
        "lint" => {
            #[derive(Deserialize)]
            struct In {
                kinds: Option<Vec<Rule>>,
            }
            let i: In = parse(input)?;
            let g = state.graph();
            let rules = i.kinds.unwrap_or_else(|| Rule::ALL.to_vec());
            let findings = lint::lint(g, &rules);
            ok(json!({ "findings": findings, "counts": query::counts(&findings) }))
        }
        "module_source" => {
            let i: RefIn = parse(input)?;
            let g = state.graph();
            if !matches!(i.r#ref.kind, Kind::Module | Kind::Criterion | Kind::File | Kind::Decl) {
                return Err(ToolError::invalid(format!("{} is not a module, criterion, file or declaration", i.r#ref), vec![]));
            }
            let loc = query::module_source(g, &i.r#ref).ok_or_else(|| ToolError::not_found(&i.r#ref))?;
            ok(to_value(&loc))
        }
        "status" => {
            let config = state.store.config.clone();
            let build = state.build.clone();
            let g = state.graph();
            let mut kinds = serde_json::Map::new();
            for k in Kind::ALL {
                kinds.insert(k.to_string(), Value::from(g.nodes.keys().filter(|r| r.kind == k).count()));
            }
            ok(json!({
                "repository": config.repository,
                "roots": config.roots,
                "languages": config.languages,
                "version": build.version,
                "supported_languages": build.supported_languages,
                "nodes": kinds,
                "edges": g.edges.len(),
                "unresolved_imports": g.unresolved_imports,
                "malformed": g.malformed.len(),
            }))
        }

        // -- writes ---------------------------------------------------------
        "put_page" => {
            let page: Page = parse(input)?;
            let (node, changed) = ops::put_page(state.store_mut(), page)?;
            let r = Reference::page(&node.slug);
            written(state, &r, changed)
        }
        "patch_page" => {
            #[derive(Deserialize)]
            #[serde(deny_unknown_fields)]
            struct In {
                r#ref: Reference,
                title: Option<String>,
                body: Option<String>,
                add_links: Option<Vec<Reference>>,
                remove_links: Option<Vec<Reference>>,
            }
            let i: In = parse(input)?;
            let patch = ops::PagePatch { title: i.title, body: i.body, add_links: i.add_links.unwrap_or_default(), remove_links: i.remove_links.unwrap_or_default() };
            let (_, changed) = ops::patch_page(state.store_mut(), &i.r#ref, patch)?;
            written(state, &i.r#ref, changed)
        }
        "put_requirement" => {
            #[derive(Deserialize)]
            #[serde(deny_unknown_fields)]
            struct In {
                slug: String,
                title: String,
                statement: String,
                scenarios: Vec<Scenario>,
                covered_by: Vec<Reference>,
                #[allow(dead_code)]
                decisions: Option<Vec<Decision>>,
            }
            let i: In = parse(input)?;
            let req = Requirement { slug: i.slug, title: i.title, statement: i.statement, scenarios: i.scenarios, covered_by: i.covered_by, decisions: vec![] };
            let (node, changed) = ops::put_requirement(state.store_mut(), req)?;
            let r = Reference::req(&node.slug);
            written(state, &r, changed)
        }
        "record_decision" => {
            #[derive(Deserialize)]
            struct In {
                r#ref: Reference,
                text: String,
                date: Option<String>,
            }
            let i: In = parse(input)?;
            let (node, changed) = ops::record_decision(state.store_mut(), &i.r#ref, i.text, i.date.unwrap_or_else(|| today.to_string()))?;
            let r = Reference::req(&node.slug);
            let view = query::view(state.graph(), &r).expect("just written");
            Ok(Outcome { output: json!({ "node": view }), changed: Some(changed) })
        }
        "file_ticket" => {
            #[derive(Deserialize)]
            #[serde(deny_unknown_fields)]
            struct In {
                slug: String,
                title: String,
                kind: TicketKind,
                priority: Priority,
                touches: Option<Vec<Reference>>,
                relies_on: Option<Vec<Reference>>,
                body: String,
            }
            let i: In = parse(input)?;
            let t = ops::NewTicket { slug: i.slug, title: i.title, kind: i.kind, priority: i.priority, touches: i.touches.unwrap_or_default(), relies_on: i.relies_on.unwrap_or_default(), body: i.body };
            let (node, changed) = ops::file_ticket(state.store_mut(), t)?;
            let r = Reference::ticket(&node.slug);
            written(state, &r, changed)
        }
        "update_ticket" => {
            #[derive(Deserialize)]
            #[serde(deny_unknown_fields)]
            struct In {
                r#ref: Reference,
                title: Option<String>,
                priority: Option<Priority>,
                touches: Option<Vec<Reference>>,
                relies_on: Option<Vec<Reference>>,
                body: Option<String>,
            }
            let i: In = parse(input)?;
            let patch = ops::TicketPatch { title: i.title, priority: i.priority, touches: i.touches, relies_on: i.relies_on, body: i.body };
            let (_, changed) = ops::update_ticket(state.store_mut(), &i.r#ref, patch)?;
            written(state, &i.r#ref, changed)
        }
        "start_ticket" => {
            let i: RefIn = parse(input)?;
            let (_, changed) = ops::start_ticket(state.store_mut(), &i.r#ref)?;
            let g = state.graph();
            let view = query::view(g, &i.r#ref).expect("just written");
            let rests = query::rests_on(g, &i.r#ref, None);
            let related = match g.get(&i.r#ref) {
                Some(crate::graph::Node::Ticket(t)) => query::related_tickets(g, t),
                _ => vec![],
            };
            Ok(Outcome { output: json!({ "node": view, "rests_on": rests, "related": related }), changed: Some(changed) })
        }
        "close_ticket" => {
            #[derive(Deserialize)]
            struct DecisionIn {
                text: String,
                date: Option<String>,
            }
            #[derive(Deserialize)]
            struct In {
                r#ref: Reference,
                decision: DecisionIn,
            }
            let i: In = parse(input)?;
            let touched: Vec<Reference> = match state.store.tickets.get(&i.r#ref.id) {
                Some(t) if i.r#ref.kind == Kind::Ticket => t.touches.clone(),
                _ => return Err(ToolError::not_found(&i.r#ref)),
            };
            let decision = Decision { date: i.decision.date.unwrap_or_else(|| today.to_string()), text: i.decision.text };
            let (_, changed) = ops::close_ticket(state.store_mut(), &i.r#ref, decision)?;
            let g = state.graph();
            let view = query::view(g, &i.r#ref).expect("just written");
            let findings = lint::lint_touching(g, &touched);
            Ok(Outcome { output: json!({ "node": view, "findings": findings }), changed: Some(changed) })
        }
        "delete_node" => {
            let i: RefIn = parse(input)?;
            // Edges that pointed at the node become dangling: collect them first.
            let pointing: Vec<Reference> = state.graph().inbound(&i.r#ref).map(|e| e.from.clone()).collect();
            let changed = ops::delete_node(state.store_mut(), &i.r#ref)?;
            let g = state.graph();
            let mut warnings = Vec::new();
            for from in pointing {
                warnings.extend(lint::dangling_from(g, &from).into_iter().filter(|f| f.edge.as_ref().is_some_and(|e| e.to == i.r#ref)));
            }
            Ok(Outcome { output: json!({ "deleted": i.r#ref, "warnings": warnings }), changed: Some(changed) })
        }
        other => Err(ToolError::invalid(format!("unknown tool `{other}`"), vec![])),
    }
}

#[derive(Deserialize)]
struct RefIn {
    r#ref: Reference,
}

fn written(state: &mut State, r: &Reference, changed: Changed) -> Result<Outcome, ToolError> {
    let g = state.graph();
    let view = query::view(g, r).expect("just written");
    let warnings = lint::dangling_from(g, r);
    Ok(Outcome { output: json!({ "node": view, "warnings": warnings }), changed: Some(changed) })
}

/// A tool's name, description and JSON input schema, for MCP `tools/list`
/// and for the API's self-description.
#[derive(Debug, Clone, serde::Serialize)]
pub struct ToolDef {
    pub name: &'static str,
    pub description: &'static str,
    pub input_schema: Value,
}

pub fn definitions() -> Vec<ToolDef> {
    let reference = json!({ "type": "string", "description": "A node reference of the form kind:id, e.g. page:overview, req:ready-only, module:crates/core/src/model, criterion:<module>#<id>, ticket:phase-62" });
    let refs = json!({ "type": "array", "items": reference });
    let date = json!({ "type": "string", "description": "ISO date YYYY-MM-DD" });
    let decision = json!({ "type": "object", "required": ["text"], "properties": { "text": { "type": "string" }, "date": date } });
    let scenario = json!({ "type": "object", "required": ["title", "given", "when", "then"], "properties": { "title": {"type":"string"}, "given": {"type":"string"}, "when": {"type":"string"}, "then": {"type":"string"} } });
    let obj = |required: &[&str], props: Value| json!({ "type": "object", "required": required, "properties": props });
    vec![
        ToolDef { name: "find", description: "Search nodes by text, optionally by kind. Matches slug, title, statement, purpose, body and declaration path.", input_schema: obj(&["query"], json!({ "query": {"type":"string"}, "kinds": {"type":"array","items":{"type":"string","enum":["page","req","ticket","module","criterion","file","decl"]}}, "limit": {"type":"integer","default":20} })) },
        ToolDef { name: "get", description: "One node in full with its edges: `out` is what it links to, `in` is what links to it.", input_schema: obj(&["ref"], json!({ "ref": reference })) },
        ToolDef { name: "outline", description: "The manual as a tree from the root pages, with coverage and lint-finding counts per node.", input_schema: obj(&[], json!({ "depth": {"type":"integer","default":2}, "root": reference })) },
        ToolDef { name: "rests_on", description: "What a node depends on, transitively downward, with the decisions behind each. The planning and review question. For a ticket, starts from relies_on and touches.", input_schema: obj(&["ref"], json!({ "ref": reference, "depth": {"type":"integer"} })) },
        ToolDef { name: "rests_on_me", description: "Everything that would be affected if this node changed, and the open tickets that reach it.", input_schema: obj(&["ref"], json!({ "ref": reference })) },
        ToolDef { name: "coverage", description: "What requirements cover a criterion, or each criterion of a module.", input_schema: obj(&["ref"], json!({ "ref": reference })) },
        ToolDef { name: "board", description: "Tickets for the first screen: open and active change tickets ordered next, high, later. Filter by status, kind, priority or a node they touch.", input_schema: obj(&[], json!({ "status": {"type":"array","items":{"type":"string","enum":["open","active","closed"]}}, "kinds": {"type":"array","items":{"type":"string","enum":["change","gap","question"]}}, "priority": {"type":"array","items":{"type":"string","enum":["next","high","later"]}}, "touching": reference })) },
        ToolDef { name: "related_tickets", description: "The open gaps and questions that touch what this ticket touches.", input_schema: obj(&["ref"], json!({ "ref": reference })) },
        ToolDef { name: "lint", description: "The dangling, uncovered and malformed checks over the whole graph. Same output as `grimoire check`.", input_schema: obj(&[], json!({ "kinds": {"type":"array","items":{"type":"string","enum":["dangling","uncovered","malformed"]}} })) },
        ToolDef { name: "module_source", description: "Where a module, criterion, file or declaration lives: file and line, plus the contract line for criteria.", input_schema: obj(&["ref"], json!({ "ref": reference })) },
        ToolDef { name: "put_page", description: "Create or replace a page. Links may point to page, req and module references.", input_schema: obj(&["slug","title","body","links"], json!({ "slug": {"type":"string"}, "title": {"type":"string"}, "body": {"type":"string","description":"Markdown"}, "links": refs })) },
        ToolDef { name: "patch_page", description: "Change a page's title or body, or add and remove links, leaving everything else as it was. The usual way to link a new requirement or page from its area.", input_schema: obj(&["ref"], json!({ "ref": reference, "title": {"type":"string"}, "body": {"type":"string","description":"Markdown"}, "add_links": refs, "remove_links": refs })) },
        ToolDef { name: "put_requirement", description: "Create or replace a requirement. Existing decisions are preserved; use record_decision to add one.", input_schema: obj(&["slug","title","statement","scenarios","covered_by"], json!({ "slug": {"type":"string"}, "title": {"type":"string"}, "statement": {"type":"string","description":"One normative paragraph (SHALL / MUST)"}, "scenarios": {"type":"array","items": scenario}, "covered_by": {"type":"array","items": reference, "description":"criterion references"} })) },
        ToolDef { name: "record_decision", description: "Append a dated decision to a requirement: what was chosen and why.", input_schema: obj(&["ref","text"], json!({ "ref": reference, "text": {"type":"string"}, "date": date })) },
        ToolDef { name: "file_ticket", description: "Create an open ticket: a change, gap or question, with the nodes it touches and relies on.", input_schema: obj(&["slug","title","kind","priority","body"], json!({ "slug": {"type":"string"}, "title": {"type":"string"}, "kind": {"type":"string","enum":["change","gap","question"]}, "priority": {"type":"string","enum":["next","high","later"]}, "touches": refs, "relies_on": refs, "body": {"type":"string","description":"Markdown, at most 4000 characters"} })) },
        ToolDef { name: "update_ticket", description: "Change an open or active ticket's title, priority, touches, relies_on or body.", input_schema: obj(&["ref"], json!({ "ref": reference, "title": {"type":"string"}, "priority": {"type":"string","enum":["next","high","later"]}, "touches": refs, "relies_on": refs, "body": {"type":"string"} })) },
        ToolDef { name: "start_ticket", description: "Move a ticket from open to active and return what it rests on and the related gaps and questions.", input_schema: obj(&["ref"], json!({ "ref": reference })) },
        ToolDef { name: "close_ticket", description: "Close a ticket with its decision. The body is discarded. Returns the lint findings on the nodes it touched.", input_schema: obj(&["ref","decision"], json!({ "ref": reference, "decision": decision })) },
        ToolDef { name: "delete_node", description: "Remove a page, requirement or ticket. Returns the dangling edges the deletion created.", input_schema: obj(&["ref"], json!({ "ref": reference })) },
        ToolDef { name: "status", description: "Counts of nodes and edges, the repository path, how many imports did not resolve, and the running build's version and supported languages next to the store's languages.", input_schema: obj(&[], json!({})) },
    ]
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::extraction::fixtures::extraction;
    use crate::graph::fixtures::store;

    fn state() -> State {
        State::new(store(), extraction(), vec![])
    }

    #[test]
    fn definitions_cover_the_dispatcher() {
        let defs = definitions();
        let mut names: Vec<&str> = defs.iter().map(|d| d.name).collect();
        names.sort();
        let mut expected = TOOL_NAMES.to_vec();
        expected.sort();
        assert_eq!(names, expected);
        let mut s = state();
        for n in TOOL_NAMES {
            let r = call(&mut s, n, json!({}), "2026-09-27");
            // Either it worked with an empty input or it complained about input, never "unknown tool".
            if let Err(e) = r {
                assert_ne!(e.to_string(), format!("unknown tool `{n}`"));
            }
        }
    }

    #[test]
    fn status_reports_the_build_it_was_told() {
        let mut s = state();
        // Untold, the version is this crate's and no language is claimed.
        let out = call(&mut s, "status", json!({}), "2026-09-27").unwrap();
        assert_eq!(out.output["version"], env!("CARGO_PKG_VERSION"));
        assert_eq!(out.output["supported_languages"], json!([]));
        assert!(out.output["languages"].is_array());

        s.set_build(Build { version: "9.9.9".into(), supported_languages: vec!["rust".into()] });
        let out = call(&mut s, "status", json!({}), "2026-09-27").unwrap();
        assert_eq!(out.output["version"], "9.9.9");
        assert_eq!(out.output["supported_languages"], json!(["rust"]));
        // The rest of the answer is untouched by the build.
        assert!(out.output["nodes"].is_object());
        assert!(out.output["edges"].is_number());
    }

    #[test]
    fn write_then_read_sees_it() {
        let mut s = state();
        let out = call(&mut s, "file_ticket", json!({ "slug": "new-1", "title": "New", "kind": "change", "priority": "high", "touches": ["page:nope"], "body": "x" }), "2026-09-27").unwrap();
        assert_eq!(out.output["warnings"].as_array().unwrap().len(), 1);
        assert!(matches!(out.changed, Some(Changed::Ticket(ref s)) if s == "new-1"));
        let got = call(&mut s, "get", json!({ "ref": "ticket:new-1" }), "2026-09-27").unwrap();
        assert_eq!(got.output["node"]["status"], "open");
        let board = call(&mut s, "board", json!({}), "2026-09-27").unwrap();
        assert!(board.output["tickets"].as_array().unwrap().iter().any(|t| t["ref"] == "ticket:new-1"));
    }

    #[test]
    fn start_and_close_return_context() {
        let mut s = state();
        let out = call(&mut s, "start_ticket", json!({ "ref": "ticket:phase-62" }), "2026-09-27").unwrap();
        assert!(!out.output["rests_on"].as_array().unwrap().is_empty());
        assert_eq!(out.output["related"].as_array().unwrap().len(), 1);
        let out = call(&mut s, "close_ticket", json!({ "ref": "ticket:phase-62", "decision": { "text": "Done." } }), "2026-09-27").unwrap();
        assert_eq!(out.output["node"]["decision"]["date"], "2026-09-27");
        assert!(out.output["findings"].is_array());
    }

    #[test]
    fn delete_reports_new_dangling() {
        let mut s = state();
        let out = call(&mut s, "delete_node", json!({ "ref": "page:git-readiness" }), "2026-09-27").unwrap();
        let w = out.output["warnings"].as_array().unwrap();
        assert!(w.iter().any(|f| f["ref"] == "page:overview"));
        assert!(w.iter().any(|f| f["ref"] == "ticket:phase-62"));
    }

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
