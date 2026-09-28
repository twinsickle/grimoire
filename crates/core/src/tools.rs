//! # Module contract
//!
//! ```json
//! {"purpose":"Dispatch every tool by name over JSON so the MCP server, the JSON API and the CLI share one implementation and one set of definitions.","criteria":[{"id":"graph-current","statement":"A read after a write sees the write; the cached graph is rebuilt whenever the store or the extraction changes."},{"id":"definitions-match","statement":"Every tool the dispatcher accepts is listed in definitions() with an input schema, and nothing is listed that it does not accept."},{"id":"error-codes","statement":"Every error is {error, message, details?} with error one of not_found, invalid, conflict or store_unavailable, and invalid carries the violated fields in details."},{"id":"write-responses","statement":"Every write answers with the node as it now exists; put_page, patch_page, put_requirement, file_ticket and update_ticket add as warnings the new findings the write created on that node (its dangling edges, and unspecified for a requirement with no scenario) and a count of the baselined ones, start_ticket adds rests_on and related, close_ticket adds the new findings on the nodes it touched, a count of the baselined ones and the touched nodes that never changed, delete_node returns the deleted reference and the new dangling edges left behind with the same count, adopt returns the new baseline's date, reason and count with the count it replaced, and configure returns the config with the count of findings now new."},{"id":"status","statement":"status returns the repository path, roots, languages, node counts by kind, edge count, unresolved import count, malformed file count, the contract scope and the baseline's date, reason and count, plus the build's version and supported_languages as the loader told State, so the answer is the same over every transport."},{"id":"lint-tool","statement":"lint runs the selected rules (all by default) and returns the findings with counts per rule; it is the same function grimoire check prints."},{"id":"untouched","statement":"start_ticket records the fingerprint of every node as the ticket's start; close_ticket answers with the touched nodes whose fingerprint is unchanged, or null when the ticket has no start, and removes the start."}]}
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
            let mut g = Graph::build(&self.store, &self.extraction, &self.malformed);
            let supported = &self.build.supported_languages;
            // An unknown build (tests, or a caller that never said) flags nothing.
            if !supported.is_empty() {
                g.unsupported_languages = self.store.config.languages.iter().filter(|l| !supported.contains(l)).cloned().collect();
                g.supported_languages = supported.clone();
            }
            self.graph = Some(g);
        }
        self.graph.as_ref().expect("just built")
    }
    /// Drop from the baseline every finding no longer produced. Returns what
    /// changed when it shrank, so the owner of the store persists it.
    pub fn prune_baseline(&mut self) -> Option<Changed> {
        self.graph();
        let g = self.graph.as_ref().expect("just built");
        let pruned = lint::pruned(g, self.store.baseline.as_ref())?;
        self.store_mut().baseline = Some(pruned);
        Some(Changed::Baseline)
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

pub const TOOL_NAMES: [&str; 23] = [
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
    "adopt",
    "configure",
    "reorder",
    "status",
];

pub fn is_write(name: &str) -> bool {
    matches!(name, "put_page" | "patch_page" | "put_requirement" | "record_decision" | "file_ticket" | "update_ticket" | "start_ticket" | "close_ticket" | "delete_node" | "adopt" | "configure" | "reorder")
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
                (Kind::Criterion, Some(_)) => ok(json!({ "requirements": query::requirements_covering(g, &i.r#ref), "tests": query::tests_checking(g, &i.r#ref) })),
                (Kind::Module, Some(crate::graph::Node::Module(m))) => ok(json!({ "criteria": query::module_coverage(g, m) })),
                (Kind::Req, Some(crate::graph::Node::Req(q))) => ok(json!({ "criteria": query::requirement_coverage(g, q) })),
                (Kind::Criterion | Kind::Module | Kind::Req, None) => Err(ToolError::not_found(&i.r#ref)),
                _ => Err(ToolError::invalid(format!("{} is not a criterion, module or requirement", i.r#ref), vec![])),
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
            ok(json!({ "findings": findings, "counts": query::counts(&findings), "new": lint::new_count(&findings) }))
        }
        "module_source" => {
            let i: RefIn = parse(input)?;
            let g = state.graph();
            if !matches!(i.r#ref.kind, Kind::Module | Kind::Criterion | Kind::File | Kind::Decl | Kind::Test) {
                return Err(ToolError::invalid(format!("{} is not a module, criterion, file, declaration or test", i.r#ref), vec![]));
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
                "scope": config.scope,
                "baseline": baseline_summary(state.store.baseline.as_ref()),
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
                title: String,
                kind: TicketKind,
                priority: Priority,
                touches: Option<Vec<Reference>>,
                relies_on: Option<Vec<Reference>>,
                body: String,
            }
            if input.get("slug").is_some() {
                return Err(ToolError::invalid("file_ticket no longer takes a slug: the id is generated. Give a title.", vec![]));
            }
            let i: In = parse(input)?;
            let t = ops::NewTicket { title: i.title, kind: i.kind, priority: i.priority, touches: i.touches.unwrap_or_default(), relies_on: i.relies_on.unwrap_or_default(), body: i.body };
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
            ops::start_ticket(state.store_mut(), &i.r#ref)?;
            // Every node, not only what touches lists now: nodes are often
            // added to touches after they changed.
            let fingerprints = crate::fingerprint::all(state.graph()).into_iter().map(|(r, fp)| (r.to_string(), fp)).collect();
            state.store_mut().starts.insert(i.r#ref.id.clone(), Start { ticket: i.r#ref.id.clone(), fingerprints });
            let changed = Changed::Started(i.r#ref.id.clone());
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
            // Compared before the close, which changes the ticket's own record.
            let untouched = state.store.starts.get(&i.r#ref.id).cloned().map(|start| crate::fingerprint::untouched(&touched, &start.fingerprints, &crate::fingerprint::all(state.graph())));
            let decision = Decision { date: i.decision.date.unwrap_or_else(|| today.to_string()), text: i.decision.text };
            ops::close_ticket(state.store_mut(), &i.r#ref, decision)?;
            state.store_mut().starts.remove(&i.r#ref.id);
            let changed = Changed::Closed(i.r#ref.id.clone());
            let g = state.graph();
            let view = query::view(g, &i.r#ref).expect("just written");
            let (findings, baselined) = split_new(lint::lint_touching(g, &touched));
            Ok(Outcome { output: json!({ "node": view, "findings": findings, "baselined": baselined, "untouched": untouched }), changed: Some(changed) })
        }
        "reorder" => {
            #[derive(Deserialize)]
            #[serde(deny_unknown_fields)]
            struct In {
                priority: Priority,
                order: Vec<Reference>,
            }
            let i: In = parse(input)?;
            let (slugs, changed) = ops::reorder(state.store_mut(), i.priority, &i.order)?;
            let order: Vec<String> = slugs.iter().map(|s| format!("ticket:{s}")).collect();
            Ok(Outcome { output: json!({ "priority": i.priority, "order": order }), changed: Some(changed) })
        }
        "configure" => {
            let change: ops::ConfigChange = parse(input)?;
            let supported = state.build.supported_languages.clone();
            let (config, changed) = ops::configure(state.store_mut(), change, &supported)?;
            let new = lint::new_count(&lint::lint(state.graph(), &Rule::ALL));
            Ok(Outcome { output: json!({ "config": config, "new": new }), changed: Some(changed) })
        }
        "adopt" => {
            #[derive(Deserialize)]
            #[serde(deny_unknown_fields)]
            struct In {
                #[serde(default)]
                reason: String,
            }
            let i: In = parse(input)?;
            let keys = lint::lint(state.graph(), &Rule::ALL).iter().map(lint::Finding::key).collect();
            let (baseline, replaced, changed) = ops::adopt(state.store_mut(), keys, i.reason, today.to_string())?;
            Ok(Outcome { output: json!({ "baseline": baseline_summary(Some(&baseline)), "replaced": replaced }), changed: Some(changed) })
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
            let (warnings, baselined) = split_new(warnings);
            Ok(Outcome { output: json!({ "deleted": i.r#ref, "warnings": warnings, "baselined": baselined }), changed: Some(changed) })
        }
        other => Err(ToolError::invalid(format!("unknown tool `{other}`"), vec![])),
    }
}

/// `{date, reason, count}` of a baseline, or null. What `status` and
/// `adopt` show; the entries themselves stay in the store.
fn baseline_summary(b: Option<&Baseline>) -> Value {
    b.map_or(Value::Null, |b| json!({ "date": b.date, "reason": b.reason, "count": b.findings.len() }))
}

#[derive(Deserialize)]
struct RefIn {
    r#ref: Reference,
}

fn written(state: &mut State, r: &Reference, changed: Changed) -> Result<Outcome, ToolError> {
    let g = state.graph();
    let view = query::view(g, r).expect("just written");
    let (warnings, baselined) = split_new(lint::written(g, r));
    Ok(Outcome { output: json!({ "node": view, "warnings": warnings, "baselined": baselined }), changed: Some(changed) })
}

/// The new findings, which are the caller's to act on, and how many
/// baselined ones were left out: inherited work, listed by `lint`.
fn split_new(findings: Vec<lint::Finding>) -> (Vec<lint::Finding>, usize) {
    let total = findings.len();
    let new: Vec<lint::Finding> = findings.into_iter().filter(|f| !f.baselined).collect();
    let baselined = total - new.len();
    (new, baselined)
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
        ToolDef { name: "find", description: "Search nodes by text, optionally by kind. Matches slug, title, statement, purpose, body, declaration path and test name.", input_schema: obj(&["query"], json!({ "query": {"type":"string"}, "kinds": {"type":"array","items":{"type":"string","enum":["page","req","ticket","module","criterion","file","decl","test"]}}, "limit": {"type":"integer","default":20} })) },
        ToolDef { name: "get", description: "One node in full with its edges: `out` is what it links to, `in` is what links to it.", input_schema: obj(&["ref"], json!({ "ref": reference })) },
        ToolDef { name: "outline", description: "The grimoire as a tree from the root pages, with coverage and lint-finding counts per node.", input_schema: obj(&[], json!({ "depth": {"type":"integer","default":2}, "root": reference })) },
        ToolDef { name: "rests_on", description: "What a node depends on, transitively downward, with the decisions behind each. The planning and review question. For a ticket, starts from relies_on and touches.", input_schema: obj(&["ref"], json!({ "ref": reference, "depth": {"type":"integer"} })) },
        ToolDef { name: "rests_on_me", description: "Everything that would be affected if this node changed, and the open tickets that reach it.", input_schema: obj(&["ref"], json!({ "ref": reference })) },
        ToolDef { name: "coverage", description: "What requirements cover a criterion, each criterion of a module with its requirements, or each criterion a requirement names with its module.", input_schema: obj(&["ref"], json!({ "ref": reference })) },
        ToolDef { name: "board", description: "Tickets for the first screen: open and active change tickets ordered next, high, later. Filter by status, kind, priority or a node they touch.", input_schema: obj(&[], json!({ "status": {"type":"array","items":{"type":"string","enum":["open","active","closed"]}}, "kinds": {"type":"array","items":{"type":"string","enum":["change","gap","question"]}}, "priority": {"type":"array","items":{"type":"string","enum":["next","high","later"]}}, "touching": reference })) },
        ToolDef { name: "related_tickets", description: "The open gaps and questions that touch what this ticket touches.", input_schema: obj(&["ref"], json!({ "ref": reference })) },
        ToolDef { name: "lint", description: "The dangling, uncovered, malformed and unspecified checks over the whole graph. Same output as `grimoire check`.", input_schema: obj(&[], json!({ "kinds": {"type":"array","items":{"type":"string","enum":["dangling","uncovered","malformed","unspecified"]}} })) },
        ToolDef { name: "module_source", description: "Where a module, criterion, file or declaration lives: file and line, plus the contract line for criteria.", input_schema: obj(&["ref"], json!({ "ref": reference })) },
        ToolDef { name: "put_page", description: "Create or replace a page. Links may point to page, req and module references.", input_schema: obj(&["slug","title","body","links"], json!({ "slug": {"type":"string"}, "title": {"type":"string"}, "body": {"type":"string","description":"Markdown"}, "links": refs })) },
        ToolDef { name: "patch_page", description: "Change a page's title or body, or add and remove links, leaving everything else as it was. The usual way to link a new requirement or page from its area.", input_schema: obj(&["ref"], json!({ "ref": reference, "title": {"type":"string"}, "body": {"type":"string","description":"Markdown"}, "add_links": refs, "remove_links": refs })) },
        ToolDef { name: "put_requirement", description: "Create or replace a requirement. Existing decisions are preserved; use record_decision to add one.", input_schema: obj(&["slug","title","statement","scenarios","covered_by"], json!({ "slug": {"type":"string"}, "title": {"type":"string"}, "statement": {"type":"string","description":"One normative paragraph (SHALL / MUST)"}, "scenarios": {"type":"array","items": scenario}, "covered_by": {"type":"array","items": reference, "description":"criterion references"} })) },
        ToolDef { name: "record_decision", description: "Append a dated decision to a requirement: what was chosen and why.", input_schema: obj(&["ref","text"], json!({ "ref": reference, "text": {"type":"string"}, "date": date })) },
        ToolDef { name: "file_ticket", description: "Create an open ticket: a change, gap or question, with the nodes it touches and relies on. The id is generated; the title is the ticket's name.", input_schema: obj(&["title","kind","priority","body"], json!({ "title": {"type":"string"}, "kind": {"type":"string","enum":["change","gap","question"]}, "priority": {"type":"string","enum":["next","high","later"]}, "touches": refs, "relies_on": refs, "body": {"type":"string","description":"Markdown, at most 4000 characters"} })) },
        ToolDef { name: "update_ticket", description: "Change an open or active ticket's title, priority, touches, relies_on or body.", input_schema: obj(&["ref"], json!({ "ref": reference, "title": {"type":"string"}, "priority": {"type":"string","enum":["next","high","later"]}, "touches": refs, "relies_on": refs, "body": {"type":"string"} })) },
        ToolDef { name: "start_ticket", description: "Move a ticket from open to active and return what it rests on and the related gaps and questions.", input_schema: obj(&["ref"], json!({ "ref": reference })) },
        ToolDef { name: "close_ticket", description: "Close a ticket with its decision. The body is discarded. Returns the new lint findings on the nodes it touched, and how many baselined ones sit there.", input_schema: obj(&["ref","decision"], json!({ "ref": reference, "decision": decision })) },
        ToolDef { name: "delete_node", description: "Remove a page, requirement or ticket. Returns the dangling edges the deletion created.", input_schema: obj(&["ref"], json!({ "ref": reference })) },
        ToolDef { name: "adopt", description: "Record every present finding as the baseline: inherited work the check does not fail on. Only when the person asks; it needs their reason.", input_schema: obj(&["reason"], json!({ "reason": {"type":"string","description":"Why the present findings are accepted as inherited"} })) },
        ToolDef { name: "configure", description: "Change the store config's roots, languages or contract scope; only the fields given. Answers with the config and how many findings are now new. The repository is set by init only.", input_schema: obj(&[], json!({ "roots": {"type":"array","items":{"type":"string"}}, "languages": {"type":"array","items":{"type":"string"}}, "scope": {"type":"object","properties":{"include":{"type":"array","items":{"type":"string"}},"exclude":{"type":"array","items":{"type":"string"}}},"additionalProperties":false,"description":"Repository-relative path prefixes expected to carry contracts; an empty include means everything"} })) },
        ToolDef { name: "reorder", description: "Set the order of one priority band on the board: the tickets listed come first, in that order; the rest follow by dependency, then title.", input_schema: obj(&["priority","order"], json!({ "priority": {"type":"string","enum":["next","high","later"]}, "order": {"type":"array","items": reference} })) },
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

    /// # Test contract
    ///
    /// ```json
    /// {"test": "definitions-cover-the-dispatcher", "checks": ["crates/core/src/tools#definitions-match"]}
    /// ```
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

    /// # Test contract
    ///
    /// ```json
    /// {"test": "status-reports-the-build-it-was-told", "checks": ["crates/core/src/tools#status"]}
    /// ```
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

    /// # Test contract
    ///
    /// ```json
    /// {"test": "coverage-of-each-kind", "checks": ["crates/core/src/query#coverage"]}
    /// ```
    #[test]
    fn coverage_of_each_kind() {
        let mut s = state();
        let c = |s: &mut State, r: &str| call(s, "coverage", json!({ "ref": r }), "2026-09-27");
        let out = c(&mut s, "criterion:src/a/ready#guarantee").unwrap();
        assert_eq!(out.output["requirements"][0]["ref"], "req:ready-only-with-commit");
        let out = c(&mut s, "module:src/a/ready").unwrap();
        assert_eq!(out.output["criteria"].as_array().unwrap().len(), 2);
        let out = c(&mut s, "req:ready-only-with-commit").unwrap();
        assert_eq!(
            out.output,
            json!({ "criteria": [{
                "ref": "criterion:src/a/ready#guarantee",
                "statement": "Ready only with a commit.",
                "module": { "ref": "module:src/a/ready", "purpose": "Decide readiness." }
            }] })
        );
        assert!(matches!(c(&mut s, "req:nope"), Err(ToolError::NotFound { .. })));
        assert!(matches!(c(&mut s, "page:overview"), Err(ToolError::Invalid { .. })));
    }

    /// # Test contract
    ///
    /// ```json
    /// {"test": "write-then-read-sees-it", "checks": ["crates/core/src/ops#never-fails-on-lint", "crates/core/src/query#node-view", "crates/core/src/query#get", "crates/core/src/tools#graph-current"]}
    /// ```
    #[test]
    fn write_then_read_sees_it() {
        let mut s = state();
        let out = call(&mut s, "file_ticket", json!({ "title": "New", "kind": "change", "priority": "high", "touches": ["page:nope"], "body": "x" }), "2026-09-27").unwrap();
        assert_eq!(out.output["warnings"].as_array().unwrap().len(), 1);
        assert!(matches!(out.changed, Some(Changed::Ticket(ref s)) if s == "0001"));
        let got = call(&mut s, "get", json!({ "ref": "ticket:0001" }), "2026-09-27").unwrap();
        assert_eq!(got.output["node"]["status"], "open");
        // Edges carry titles so an id is never read alone.
        assert_eq!(got.output["node"]["out"][0]["ref"], "page:nope");
        assert!(got.output["node"]["out"][0]["title"].is_null(), "a dangling target has no title");
        let page = call(&mut s, "get", json!({ "ref": "page:git-readiness" }), "2026-09-27").unwrap();
        assert!(page.output["node"]["in"].as_array().unwrap().iter().any(|e| e["ref"] == "ticket:phase-62" && e["title"] == "PHASE-62"));
        let board = call(&mut s, "board", json!({}), "2026-09-27").unwrap();
        assert!(board.output["tickets"].as_array().unwrap().iter().any(|t| t["ref"] == "ticket:0001"));
        let e = call(&mut s, "file_ticket", json!({ "slug": "x", "title": "New", "kind": "change", "priority": "high", "body": "x" }), "2026-09-27").unwrap_err();
        assert!(e.to_string().contains("no longer takes a slug"));
    }

    /// # Test contract
    ///
    /// ```json
    /// {"test": "put-requirement-warns-when-unspecified", "checks": ["crates/core/src/ops#never-fails-on-lint"]}
    /// ```
    #[test]
    fn put_requirement_warns_when_unspecified() {
        let mut s = state();
        let req = |scenarios: Value| json!({ "slug": "new-rule", "title": "New", "statement": "It SHALL hold.", "scenarios": scenarios, "covered_by": [] });
        let out = call(&mut s, "put_requirement", req(json!([])), "2026-09-27").unwrap();
        let w = out.output["warnings"].as_array().unwrap();
        assert_eq!(w.len(), 1);
        assert_eq!(w[0]["rule"], "unspecified");
        assert_eq!(w[0]["ref"], "req:new-rule");
        let scenario = json!([{ "title": "T", "given": "g", "when": "w", "then": "t" }]);
        let out = call(&mut s, "put_requirement", req(scenario), "2026-09-27").unwrap();
        assert!(out.output["warnings"].as_array().unwrap().is_empty());
    }

    /// # Test contract
    ///
    /// ```json
    /// {"test": "adopt-baselines-what-is-present", "checks": ["crates/core/src/ops#adopt", "crates/core/src/tools#status", "crates/core/src/tools#lint-tool", "crates/core/src/model#baseline-record"]}
    /// ```
    #[test]
    fn adopt_baselines_what_is_present() {
        let mut s = state();
        let before = call(&mut s, "lint", json!({}), "2026-09-28").unwrap().output;
        let total = before["findings"].as_array().unwrap().len();
        assert_eq!(before["new"], total, "nothing baselined yet");
        assert!(call(&mut s, "status", json!({}), "2026-09-28").unwrap().output["baseline"].is_null());

        let e = call(&mut s, "adopt", json!({}), "2026-09-28").unwrap_err();
        assert!(matches!(e, ToolError::Invalid { .. }), "a reason is required");

        let out = call(&mut s, "adopt", json!({ "reason": "adopted at init" }), "2026-09-28").unwrap();
        assert_eq!(out.output, json!({ "baseline": { "date": "2026-09-28", "reason": "adopted at init", "count": total }, "replaced": null }));
        assert!(matches!(out.changed, Some(Changed::Baseline)));
        let after = call(&mut s, "lint", json!({}), "2026-09-28").unwrap().output;
        assert_eq!(after["new"], 0);
        assert!(after["findings"].as_array().unwrap().iter().all(|f| f["baselined"] == true));
        assert_eq!(call(&mut s, "status", json!({}), "2026-09-28").unwrap().output["baseline"]["count"], total);

        let again = call(&mut s, "adopt", json!({ "reason": "again" }), "2026-09-29").unwrap();
        assert_eq!(again.output["replaced"], total);
    }

    /// # Test contract
    ///
    /// ```json
    /// {"test": "a-fix-prunes-and-a-regression-is-new", "checks": ["crates/core/src/tools#lint-tool"]}
    /// ```
    #[test]
    fn a_fix_prunes_and_a_regression_is_new() {
        let mut s = state();
        call(&mut s, "adopt", json!({ "reason": "adopted" }), "2026-09-28").unwrap();
        assert!(s.prune_baseline().is_none(), "nothing fixed yet");
        // Fix the one unspecified requirement.
        let scenario = json!([{ "title": "T", "given": "g", "when": "w", "then": "t" }]);
        call(&mut s, "put_requirement", json!({ "slug": "stale", "title": "Stale", "statement": "It SHALL.", "scenarios": scenario, "covered_by": ["criterion:src/a/ready#gone"] }), "2026-09-28").unwrap();
        assert!(matches!(s.prune_baseline(), Some(Changed::Baseline)));
        // Break it again: now it is new.
        call(&mut s, "put_requirement", json!({ "slug": "stale", "title": "Stale", "statement": "It SHALL.", "scenarios": [], "covered_by": ["criterion:src/a/ready#gone"] }), "2026-09-28").unwrap();
        let lint = call(&mut s, "lint", json!({ "kinds": ["unspecified"] }), "2026-09-28").unwrap().output;
        assert_eq!(lint["new"], 1);
        assert_eq!(lint["findings"][0]["baselined"], false);
    }

    fn built(mut s: State, languages: &[&str]) -> State {
        s.set_build(Build { version: "0.0.0".into(), supported_languages: languages.iter().map(|l| l.to_string()).collect() });
        s
    }

    /// # Test contract
    ///
    /// ```json
    /// {"test": "configure-changes-only-what-it-is-given", "checks": ["crates/core/src/ops#configure"]}
    /// ```
    #[test]
    fn configure_changes_only_what_it_is_given() {
        let mut s = built(state(), &["rust", "typescript"]);
        let before = s.store().config.clone();
        let out = call(&mut s, "configure", json!({ "languages": ["rust", "typescript"] }), "2026-09-28").unwrap();
        assert!(matches!(out.changed, Some(Changed::Config)));
        assert_eq!(out.output["config"]["languages"], json!(["rust", "typescript"]));
        assert_eq!(s.store().config.roots, before.roots);
        assert_eq!(s.store().config.repository, before.repository);
    }

    /// # Test contract
    ///
    /// ```json
    /// {"test": "configure-refuses-what-it-may-not-change", "checks": ["crates/core/src/ops#configure", "crates/core/src/ops#validate-first"]}
    /// ```
    #[test]
    fn configure_refuses_what_it_may_not_change() {
        let mut s = built(state(), &["rust"]);
        let before = s.store().config.clone();
        let unsupported = call(&mut s, "configure", json!({ "languages": ["python"] }), "2026-09-28").unwrap_err();
        assert!(matches!(unsupported, ToolError::Invalid { .. }));
        assert!(unsupported.to_string().contains("config"), "{unsupported}");
        let repo = call(&mut s, "configure", json!({ "repository": "D:/elsewhere" }), "2026-09-28").unwrap_err();
        assert!(repo.to_string().contains("repository"), "{repo}");
        let prefix = call(&mut s, "configure", json!({ "scope": { "include": ["../outside"] } }), "2026-09-28").unwrap_err();
        assert!(serde_json::to_value(&prefix).unwrap()["details"][0]["field"] == "scope.include");
        assert_eq!(s.store().config, before, "nothing changed");
    }

    /// # Test contract
    ///
    /// ```json
    /// {"test": "widening-the-scope-makes-new-findings", "checks": ["crates/core/src/tools#status"]}
    /// ```
    #[test]
    fn widening_the_scope_makes_new_findings() {
        let mut s = built(state(), &["rust"]);
        call(&mut s, "configure", json!({ "scope": { "include": ["src/a"] } }), "2026-09-28").unwrap();
        call(&mut s, "adopt", json!({ "reason": "adopted with src/a in scope" }), "2026-09-28").unwrap();
        let out = call(&mut s, "configure", json!({ "scope": { "include": ["src/a", "src/b"] } }), "2026-09-28").unwrap();
        assert_eq!(out.output["new"], 2, "src/b/unrelated is now expected to carry a contract and a page");
        assert_eq!(call(&mut s, "status", json!({}), "2026-09-28").unwrap().output["scope"]["include"], json!(["src/a", "src/b"]));
    }

    #[test]
    fn a_language_the_build_lacks_is_a_finding() {
        let mut s = built(state(), &["rust"]);
        s.store_mut().config.languages = vec!["rust".into(), "typescript".into()];
        let lint = call(&mut s, "lint", json!({ "kinds": ["malformed"] }), "2026-09-28").unwrap().output;
        assert_eq!(lint["findings"][0]["check"], "unsupported-language");
        let unknown_build = State::new(store(), extraction(), vec![]);
        let mut u = unknown_build;
        u.store_mut().config.languages = vec!["typescript".into()];
        assert!(call(&mut u, "lint", json!({ "kinds": ["malformed"] }), "2026-09-28").unwrap().output["findings"].as_array().unwrap().is_empty(), "an unknown build flags nothing");
    }

    /// # Test contract
    ///
    /// ```json
    /// {"test": "coverage-lists-the-tests-of-a-criterion", "checks": ["crates/core/src/query#find", "crates/core/src/query#coverage"]}
    /// ```
    #[test]
    fn coverage_lists_the_tests_of_a_criterion() {
        let mut s = state();
        let out = call(&mut s, "coverage", json!({ "ref": "criterion:src/a/ready#guarantee" }), "2026-09-28").unwrap().output;
        assert_eq!(out["tests"][0]["ref"], "test:src/a/ready.rs#ready-needs-a-commit");
        let module = call(&mut s, "coverage", json!({ "ref": "module:src/a/ready" }), "2026-09-28").unwrap().output;
        let reads_only = module["criteria"].as_array().unwrap().iter().find(|c| c["ref"] == "criterion:src/a/ready#reads-only").unwrap().clone();
        assert!(reads_only["tests"].as_array().unwrap().is_empty());
        let found = call(&mut s, "find", json!({ "query": "needs-a-commit", "kinds": ["test"] }), "2026-09-28").unwrap().output;
        assert_eq!(found["nodes"][0]["ref"], "test:src/a/ready.rs#ready-needs-a-commit");
    }

    /// # Test contract
    ///
    /// ```json
    /// {"test": "inherited-findings-are-counted", "checks": ["crates/core/src/tools#write-responses"]}
    /// ```
    #[test]
    fn inherited_findings_are_counted_not_handed_over() {
        let mut s = state();
        call(&mut s, "adopt", json!({ "reason": "adopted" }), "2026-09-28").unwrap();
        let out = call(&mut s, "close_ticket", json!({ "ref": "ticket:phase-62", "decision": { "text": "Done." } }), "2026-09-28").unwrap().output;
        assert_eq!(out["findings"], json!([]), "everything on the touched nodes is inherited");
        assert!(out["baselined"].as_u64().unwrap() > 0);
        let w = call(&mut s, "put_requirement", json!({ "slug": "stale", "title": "Stale", "statement": "It SHALL.", "scenarios": [], "covered_by": ["criterion:src/a/ready#gone"] }), "2026-09-28").unwrap().output;
        assert_eq!(w["warnings"], json!([]), "stale was already dangling and unspecified when adopted");
        assert_eq!(w["baselined"], 2);
        let fresh = call(&mut s, "put_requirement", json!({ "slug": "fresh", "title": "Fresh", "statement": "It SHALL.", "scenarios": [], "covered_by": [] }), "2026-09-28").unwrap().output;
        assert_eq!(fresh["warnings"][0]["check"], "no-scenario", "new work is handed over");
        assert_eq!(fresh["baselined"], 0);
    }

    /// # Test contract
    ///
    /// ```json
    /// {"test": "record-decision-defaults-to-today", "checks": ["crates/core/src/ops#record-decision"]}
    /// ```
    #[test]
    fn record_decision_defaults_to_today() {
        let mut s = state();
        let out = call(&mut s, "record_decision", json!({ "ref": "req:ready-only-with-commit", "text": "Why." }), "2026-09-28").unwrap().output;
        let last = out["node"]["decisions"].as_array().unwrap().last().unwrap().clone();
        assert_eq!(last, json!({ "date": "2026-09-28", "text": "Why." }));
    }

    /// # Test contract
    ///
    /// ```json
    /// {"test": "the-graph-follows-the-extraction", "checks": ["crates/core/src/tools#graph-current"]}
    /// ```
    #[test]
    fn the_graph_follows_the_extraction() {
        let mut s = state();
        assert!(call(&mut s, "get", json!({ "ref": "module:src/b/unrelated" }), "2026-09-28").is_ok());
        let files: Vec<_> = crate::extraction::fixtures::extraction_files().into_iter().filter(|f| f.path != "src/b/unrelated.rs").collect();
        s.set_extraction(Extraction::from_files(&files));
        let gone = call(&mut s, "get", json!({ "ref": "module:src/b/unrelated" }), "2026-09-28").unwrap_err();
        assert!(matches!(gone, ToolError::NotFound { .. }), "a read after an extraction change sees it");
    }

    /// # Test contract
    ///
    /// ```json
    /// {"test": "a-close-names-what-never-changed", "checks": ["crates/core/src/tools#untouched"]}
    /// ```
    #[test]
    fn a_close_names_what_never_changed() {
        let mut s = state();
        call(&mut s, "start_ticket", json!({ "ref": "ticket:phase-62" }), "2026-09-28").unwrap();
        assert!(s.store().starts.contains_key("phase-62"), "the start is recorded");
        // phase-62 touches req:ready-only-with-commit and module:src/a/ready. Change
        // the requirement, and add to touches a page changed before it was listed.
        call(&mut s, "patch_page", json!({ "ref": "page:orphan", "body": "Changed before being listed." }), "2026-09-28").unwrap();
        call(&mut s, "put_requirement", json!({ "slug": "ready-only-with-commit", "title": "Changed", "statement": "It SHALL.", "scenarios": [{ "title": "T", "given": "g", "when": "w", "then": "t" }], "covered_by": ["criterion:src/a/ready#guarantee"] }), "2026-09-28").unwrap();
        call(&mut s, "update_ticket", json!({ "ref": "ticket:phase-62", "touches": ["req:ready-only-with-commit", "module:src/a/ready", "page:orphan", "req:never-was"] }), "2026-09-28").unwrap();
        let out = call(&mut s, "close_ticket", json!({ "ref": "ticket:phase-62", "decision": { "text": "Done." } }), "2026-09-28").unwrap();
        assert_eq!(out.output["untouched"], json!(["module:src/a/ready"]), "the requirement and the page changed; a reference that never existed is the lint's");
        assert!(matches!(out.changed, Some(Changed::Closed(ref slug)) if slug == "phase-62"));
        assert!(!s.store().starts.contains_key("phase-62"), "the start is removed on close");
    }

    #[test]
    fn a_ticket_with_no_start_answers_null() {
        let mut s = state();
        let out = call(&mut s, "close_ticket", json!({ "ref": "ticket:later-1", "decision": { "text": "Done." } }), "2026-09-28").unwrap();
        assert!(out.output["untouched"].is_null(), "unknown, not none");
    }

    /// # Test contract
    ///
    /// ```json
    /// {"test": "reorder-sets-one-band", "checks": ["crates/core/src/ops#reorder"]}
    /// ```
    #[test]
    fn reorder_sets_one_band() {
        let mut s = state();
        // phase-62 is next and open; later-1 is later and active; old is closed.
        let out = call(&mut s, "reorder", json!({ "priority": "next", "order": ["ticket:phase-62"] }), "2026-09-28").unwrap();
        assert_eq!(out.output, json!({ "priority": "next", "order": ["ticket:phase-62"] }));
        assert!(matches!(out.changed, Some(Changed::Config)));
        assert_eq!(s.store().config.order.next, vec!["phase-62".to_string()]);
        for bad in [json!(["ticket:later-1"]), json!(["ticket:old"]), json!(["ticket:phase-62", "ticket:phase-62"]), json!(["page:overview"])] {
            let e = call(&mut s, "reorder", json!({ "priority": "next", "order": bad }), "2026-09-28").unwrap_err();
            assert!(matches!(e, ToolError::Invalid { .. }), "{bad} is refused");
        }
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

    /// # Test contract
    ///
    /// ```json
    /// {"test": "errors-carry-codes", "checks": ["crates/core/src/query#get", "crates/core/src/tools#error-codes"]}
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
