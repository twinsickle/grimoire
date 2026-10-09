//! # Module contract
//!
//! ```json
//! {"purpose":"Answer the read tools: each parses its input, checks the reference it names and answers with a query over the current graph.","criteria":[{"id":"status","statement":"status returns the repository path, roots, languages, node counts by kind, edge count, unresolved import count, malformed file count, the contract scope and the baseline's date, reason and count, this checkout's current ticket's id and title, the grimoire's name in its daemon's library and that library's grimoires, plus the build's version and supported_languages as the loader told State, and tiers, each configured language in the config's order with the tier the build was told it reads it at or none, so the answer is the same over every transport."},{"id":"lint-tool","statement":"lint runs the selected rules (all by default) and returns the findings with counts per rule, and the omens apart; it is the same function grimoire check prints."},{"id":"source","statement":"source answers for a module, criterion, file, declaration or test with its file's text from the reader the caller supplied, the node's line as the focus, the file's annotations, and plain true exactly when the text has more lines than the config's plain-file limit; it asks the reader only for a file the graph knows and never for more than 2 MB, refuses a larger file or one over 20,000 lines as invalid, and answers store_unavailable when there is no reader or the reader fails."},{"id":"ticket-view","statement":"Every read tool takes an optional ticket and answers over the graph as that brewing or simmering ticket's proposal would leave it, a ticket that is neither being a conflict; absent, over the truth, except lint, which uses the store's current ticket when there is one. get names in drafts, however the node is read, the tickets whose proposals would change a page, a requirement or a ticket's references, and gives a ticket its own proposal, the tickets whose references it would change listed with the lists that differ."},{"id":"checked-out","statement":"get on a ticket being worked, brewing or a change simmering, answers checked_out, who has it checked out and since when as the store keeps them, empty when none is kept; on any other ticket or node it answers none."},{"id":"get-several","statement":"get given refs answers nodes, each as get given that one reference answers it, in the order given, and missing, each reference the graph does not hold, in the order given, refusing none for it; given both ref and refs, or neither, it is invalid."},{"id":"without-code-refused","statement":"On a state that holds no code, get, rests_on and rests_on_me of a module, criterion, file, declaration or test, and coverage, module_source and source of anything, are refused as store_unavailable saying the daemon holds no code, never answered as not found."},{"id":"without-code-outline","statement":"On a state that holds no code, outline answers over the intent with without_code true."},{"id":"without-code-walk-title","statement":"On a state that holds no code, a node from the code that a walk or a tree reaches is titled as not held here, not as missing."}]}
//! ```

use serde::Deserialize;
use serde_json::Value;

use super::{answer, parse, Outcome, RefIn, SourceError, State, SOURCE_LIMIT, SOURCE_LINES};
use crate::answer::*;
use crate::lint::Rule;
use crate::model::*;
use crate::ops::ToolError;
use crate::query::{self, BoardFilter};
use crate::store::FieldError;

/// Run the read tool `name`. A `ticket` in the input picks the view: the
/// graph as that ticket's proposal would leave it; absent, the truth, except
/// that `lint` with none uses the store's current ticket, so the check on a
/// branch judges the branch's code against the branch's own intent.
/// What a daemon that holds no code answers when asked about the code: it
/// is not missing, it is not held here.
fn no_code(what: impl std::fmt::Display) -> ToolError {
    ToolError::unavailable(format!("this daemon holds no code, so it cannot answer about {what}: ask a daemon that has the repository"))
}

/// Refuse a question about a node that comes from the code, on a state that holds none.
fn no_code_for(state: &State, r: &Reference) -> Result<(), ToolError> {
    if state.without_code() && r.kind.is_derived() {
        return Err(no_code(r));
    }
    Ok(())
}

/// One node as `get` answers it, or none when the graph does not hold it.
fn got(state: &mut State, view: Option<&str>, r: &Reference) -> Result<Option<NodeAnswer>, ToolError> {
    no_code_for(state, r)?;
    let (proposal, drafts) = drafts_of(state.store(), r);
    let g = state.graph_for(view)?;
    let Some(node) = query::view(g, r) else { return Ok(None) };
    // Who has a ticket being worked checked out: what the store keeps beside the ticket, not in it (req:ticket-checked-out).
    let checked_out = (r.kind == Kind::Ticket).then(|| state.store().checked_out_of(&r.id)).flatten();
    Ok(Some(NodeAnswer { node, proposal, drafts, checked_out }))
}

pub(super) fn call(state: &mut State, name: &str, mut input: Value) -> Result<Outcome, ToolError> {
    let view = match super::take_ticket(&mut input)? {
        Some(t) => Some(t),
        None if name == "lint" => state.current(),
        None => None,
    };
    let view = view.as_deref();
    match name {
        "find" => {
            #[derive(Deserialize)]
            struct In {
                query: String,
                kinds: Option<Vec<Kind>>,
                limit: Option<usize>,
            }
            let i: In = parse(input)?;
            let g = state.graph_for(view)?;
            answer(name, NodesAnswer { nodes: query::find(g, &i.query, i.kinds.as_deref(), i.limit.unwrap_or(20)) }, None)
        }
        "get" => {
            #[derive(Deserialize)]
            #[serde(deny_unknown_fields)]
            struct In {
                r#ref: Option<Reference>,
                refs: Option<Vec<Reference>>,
            }
            let i: In = parse(input)?;
            match (i.r#ref, i.refs) {
                (Some(r), None) => {
                    let node = got(state, view, &r)?.ok_or_else(|| ToolError::not_found(&r))?;
                    answer(name, GetAnswer::One(Box::new(node)), None)
                }
                // A group in one call: what the graph lacks is listed, never a refusal of the rest (req:tool-get).
                (None, Some(refs)) => {
                    let (mut nodes, mut missing) = (Vec::new(), Vec::new());
                    for r in refs {
                        match got(state, view, &r)? {
                            Some(node) => nodes.push(node),
                            None => missing.push(r),
                        }
                    }
                    answer(name, GetAnswer::Several { nodes, missing }, None)
                }
                _ => Err(ToolError::invalid("give `ref` for one node or `refs` for several, not both or neither", vec![FieldError::new("ref", "give this or refs")])),
            }
        }
        "outline" => {
            #[derive(Deserialize)]
            struct In {
                depth: Option<usize>,
                root: Option<Reference>,
                #[serde(default)]
                pages_only: bool,
            }
            let i: In = parse(input)?;
            if let Some(r) = &i.root {
                if r.kind != Kind::Page {
                    return Err(ToolError::invalid(format!("{r} is not a page"), vec![]));
                }
            }
            let g = state.graph_for(view)?;
            answer(name, query::outline(g, i.depth.unwrap_or(2), i.root.as_ref(), i.pages_only), None)
        }
        "rests_on" => {
            #[derive(Deserialize)]
            struct In {
                r#ref: Reference,
                depth: Option<usize>,
            }
            let i: In = parse(input)?;
            no_code_for(state, &i.r#ref)?;
            let g = state.graph_for(view)?;
            if !g.contains(&i.r#ref) {
                return Err(ToolError::not_found(&i.r#ref));
            }
            answer(name, RestsOnAnswer { nodes: query::rests_on(g, &i.r#ref, i.depth) }, None)
        }
        "rests_on_me" => {
            let i: RefIn = parse(input)?;
            no_code_for(state, &i.r#ref)?;
            let g = state.graph_for(view)?;
            if !g.contains(&i.r#ref) {
                return Err(ToolError::not_found(&i.r#ref));
            }
            answer(name, query::rests_on_me(g, &i.r#ref), None)
        }
        "coverage" => {
            let i: RefIn = parse(input)?;
            // Coverage is what the code says of the intent; with no code there is no answer to give.
            if state.without_code() {
                return Err(no_code("coverage"));
            }
            let g = state.graph_for(view)?;
            match (i.r#ref.kind, g.get(&i.r#ref)) {
                (Kind::Criterion, Some(_)) => answer(name, CoverageAnswer::OfCriterion { requirements: query::requirements_covering(g, &i.r#ref), tests: query::tests_checking(g, &i.r#ref) }, None),
                (Kind::Module, Some(crate::graph::Node::Module(m))) => answer(name, CoverageAnswer::OfModule { criteria: query::module_coverage(g, m) }, None),
                (Kind::Req, Some(crate::graph::Node::Req(q))) => answer(name, CoverageAnswer::OfRequirement { criteria: query::requirement_coverage(g, q) }, None),
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
                // Read as `closed` too, its name before ticket 0168.
                #[serde(default, alias = "closed")]
                bottled: usize,
            }
            let i: In = parse(input)?;
            let g = state.graph_for(view)?;
            let f = BoardFilter { status: i.status.as_deref(), kinds: i.kinds.as_deref(), priority: i.priority.as_deref(), touching: i.touching.as_ref(), bottled: i.bottled };
            answer(name, query::board(g, &f), None)
        }
        "related_tickets" => {
            let i: RefIn = parse(input)?;
            let g = state.graph_for(view)?;
            match g.get(&i.r#ref) {
                Some(crate::graph::Node::Ticket(t)) => answer(name, TicketsAnswer { tickets: query::related_tickets(g, t) }, None),
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
            let rules = i.kinds.unwrap_or_else(|| Rule::ALL.to_vec());
            let lint = state.lint(view, &rules)?;
            answer(name, lint, None)
        }
        "module_source" => {
            let i: RefIn = parse(input)?;
            if state.without_code() {
                return Err(no_code("where anything is in the source"));
            }
            let g = state.graph_for(view)?;
            if !matches!(i.r#ref.kind, Kind::Module | Kind::Criterion | Kind::File | Kind::Decl | Kind::Test) {
                return Err(ToolError::invalid(format!("{} is not a module, criterion, file, declaration or test", i.r#ref), vec![]));
            }
            let loc = query::module_source(g, &i.r#ref).ok_or_else(|| ToolError::not_found(&i.r#ref))?;
            answer(name, loc, None)
        }
        "source" => {
            let i: RefIn = parse(input)?;
            if state.without_code() {
                return Err(no_code("the source"));
            }
            if !matches!(i.r#ref.kind, Kind::Module | Kind::Criterion | Kind::File | Kind::Decl | Kind::Test) {
                return Err(ToolError::invalid(format!("{} is not a module, criterion, file, declaration or test", i.r#ref), vec![]));
            }
            let reader = state.source.clone();
            let g = state.graph_for(view)?;
            let loc = query::module_source(g, &i.r#ref).ok_or_else(|| ToolError::not_found(&i.r#ref))?;
            let file = Reference::file(&loc.file);
            let language = match g.get(&file) {
                Some(crate::graph::Node::File(f)) => f.language.clone(),
                _ => return Err(ToolError::not_found(&file)),
            };
            let reader = reader.ok_or_else(|| ToolError::unavailable("this caller cannot read the repository's files"))?;
            let text = match reader.read(&loc.file, SOURCE_LIMIT) {
                Ok(text) => text,
                Err(SourceError::TooLarge(bytes)) => return Err(ToolError::invalid(format!("{} is {bytes} bytes, over the {SOURCE_LIMIT} a source view shows", loc.file), vec![])),
                Err(SourceError::Unreadable(why)) => return Err(ToolError::unavailable(format!("{} could not be read: {why}", loc.file))),
            };
            let lines = text.lines().count();
            let plain = lines > g.plain_file as usize;
            if lines > SOURCE_LINES {
                return Err(ToolError::invalid(format!("{} has {lines} lines, over the {SOURCE_LINES} a source view shows", loc.file), vec![]));
            }
            let focus = if i.r#ref.kind == Kind::File { 1 } else { loc.line };
            let annotations = query::source_annotations(g, &loc.file);
            answer(name, SourceAnswer { file, language, text, focus, plain, annotations }, None)
        }
        "status" => {
            let config = state.store.config.clone();
            let build = state.build.clone();
            let g = state.graph_for(view)?;
            // A decision is of the kind req but is no requirement: counted with none.
            let nodes = Kind::ALL.iter().map(|k| (k.to_string(), g.nodes.keys().filter(|r| r.kind == *k && r.as_decision().is_none()).count())).collect();
            let status = StatusAnswer {
                repository: config.repository,
                roots: config.roots,
                tiers: config.languages.iter().map(|l| crate::answer::LanguageTier { language: l.clone(), tier: build.tiers.get(l).copied() }).collect(),
                languages: config.languages,
                version: build.version,
                supported_languages: build.supported_languages,
                nodes,
                edges: g.edges.len(),
                unresolved_imports: g.unresolved_imports,
                malformed: g.malformed.len(),
                scope: config.scope,
                baseline: state.store.baseline.as_ref().map(baseline_summary),
                shared: state.without_code(),
                server: state.server().cloned(),
                current: state.current_ticket(),
                grimoire: state.library.as_ref().map(|(name, _)| name.clone()),
                library: library_with_tickets(state),
            };
            answer(name, status, None)
        }
        other => Err(ToolError::invalid(format!("unknown tool `{other}`"), vec![])),
    }
}

/// The library as `status` lists it: each checkout of this grimoire with its current ticket,
/// this checkout's as it is now and another's as the state was last told; another grimoire's with none.
fn library_with_tickets(state: &State) -> Vec<crate::answer::LibraryEntry> {
    let Some((name, all)) = state.library.as_ref() else { return Vec::new() };
    let here = &state.store.config.repository;
    all.iter()
        .map(|e| {
            let current = if &e.name != name {
                None
            } else if &e.repository == here {
                state.current_ticket()
            } else {
                state.checkout_tickets.get(&e.repository).cloned()
            };
            crate::answer::LibraryEntry { current, ..e.clone() }
        })
        .collect()
}

/// What `get` adds about drafts: a ticket's own proposal, or for a page or
/// requirement the tickets whose proposals would change it. A proposed
/// record equal to the truth's, or a deletion of what the truth no longer
/// has, changes nothing at the close and is no draft.
fn drafts_of(store: &crate::store::Store, r: &Reference) -> (Option<ProposalView>, Option<Vec<Reference>>) {
    match r.kind {
        Kind::Ticket => {
            // Its references are drafts too, when another ticket's proposal edits them.
            let tickets: Vec<Reference> = store.ticket_drafts().remove(&r.id).unwrap_or_default().iter().map(|t| Reference::ticket(t)).collect();
            (store.proposals.get(&r.id).map(|p| proposal_view(store, p)), if tickets.is_empty() { None } else { Some(tickets) })
        }
        Kind::Page | Kind::Req => {
            let tickets: Vec<Reference> = store.proposals.values().filter(|p| changes(store, p, r)).map(|p| Reference::ticket(&p.ticket)).collect();
            (None, if tickets.is_empty() { None } else { Some(tickets) })
        }
        _ => (None, None),
    }
}

/// Whether `p` would change `r` in the truth at its close.
fn changes(store: &crate::store::Store, p: &Proposal, r: &Reference) -> bool {
    match r.kind {
        Kind::Page => p.pages.get(&r.id).is_some_and(|q| store.pages.get(&r.id) != Some(&q.record)) || (p.deleted.contains_key(&r.to_string()) && store.pages.contains_key(&r.id)),
        Kind::Req => {
            p.requirements.get(&r.id).is_some_and(|q| store.requirements.get(&r.id).map(Requirement::without_decisions).as_ref() != Some(&q.record.without_decisions()))
                || p.decisions.get(&r.id).is_some_and(|d| !d.is_empty())
                || (p.deleted.contains_key(&r.to_string()) && store.requirements.contains_key(&r.id))
        }
        _ => false,
    }
}

fn proposal_view(store: &crate::store::Store, p: &Proposal) -> ProposalView {
    let pages = p
        .pages
        .values()
        .filter(|q| changes(store, p, &Reference::page(&q.record.slug)))
        .map(|q| {
            let truth = store.pages.get(&q.record.slug);
            let differs = truth
                .map(|t| {
                    [("title", t.title != q.record.title), ("body", t.body != q.record.body), ("links", t.links != q.record.links)]
                        .into_iter()
                        .filter(|(_, d)| *d)
                        .map(|(f, _)| f.to_string())
                        .collect()
                })
                .unwrap_or_default();
            Draft { r#ref: Reference::page(&q.record.slug), title: q.record.title.clone(), new: truth.is_none(), differs }
        })
        .collect();
    // A requirement it holds a record of, or adds decisions to, read as it would be bottled.
    let slugs: std::collections::BTreeSet<&String> = p.requirements.keys().chain(p.decisions.keys()).collect();
    let requirements = slugs
        .into_iter()
        .filter(|slug| changes(store, p, &Reference::req(slug)))
        .filter_map(|slug| crate::store::with_decisions(store.requirements.get(slug), p, slug, crate::store::Bottling::Waiting))
        .map(|q| {
            let truth = store.requirements.get(&q.slug);
            let differs = truth
                .map(|t| {
                    [
                        ("title", t.title != q.title),
                        ("statement", t.statement != q.statement),
                        ("scenarios", t.scenarios != q.scenarios),
                        ("covered_by", t.covered_by != q.covered_by),
                        ("decisions", t.decisions != q.decisions),
                        ("retired", t.retired != q.retired),
                    ]
                    .into_iter()
                    .filter(|(_, d)| *d)
                    .map(|(f, _)| f.to_string())
                    .collect()
                })
                .unwrap_or_default();
            Draft { r#ref: Reference::req(&q.slug), title: q.title.clone(), new: truth.is_none(), differs }
        })
        .collect();
    let deleted = p.deleted.keys().filter_map(|k| k.parse::<Reference>().ok()).filter(|r| changes(store, p, r)).collect();
    // The tickets whose references it would change; an edit equal to the truth's lists, or to a ticket since closed or gone, changes nothing.
    let tickets = p
        .tickets
        .iter()
        .filter_map(|(slug, q)| {
            let t = store.tickets.get(slug).filter(|t| t.status != Status::Bottled)?;
            let differs: Vec<String> =
                [("touches", t.touches != q.record.touches), ("relies_on", t.relies_on != q.record.relies_on)].into_iter().filter(|(_, d)| *d).map(|(f, _)| f.to_string()).collect();
            (!differs.is_empty()).then(|| Draft { r#ref: Reference::ticket(slug), title: t.title.clone(), new: false, differs })
        })
        .collect();
    ProposalView { pages, requirements, deleted, tickets }
}

/// What `status` and `adopt` show of a baseline; the entries stay in the store.
pub(super) fn baseline_summary(b: &Baseline) -> BaselineSummary {
    BaselineSummary { date: b.date.clone(), reason: b.reason.clone(), count: b.findings.len() }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;

    use serde_json::json;

    use crate::tools::fixtures::state;
    use crate::tools::{call, Build, SourceReader};

    /// # Test contract
    ///
    /// ```json
    /// {"test": "status-reports-the-build-it-was-told", "checks": ["crates/core/src/tools/read#status"]}
    /// ```
    #[test]
    fn status_reports_the_build_it_was_told() {
        let mut s = state();
        // Untold, the version is this crate's and no language is claimed.
        let out = call(&mut s, "status", json!({}), "2026-09-27").unwrap();
        assert_eq!(out.output["version"], env!("CARGO_PKG_VERSION"));
        assert_eq!(out.output["supported_languages"], json!([]));
        assert!(out.output["languages"].is_array());

        s.set_build(Build { version: "9.9.9".into(), supported_languages: vec!["rust".into(), "css".into()], tiers: [("rust".to_string(), 2), ("css".to_string(), 0)].into() });
        let out = call(&mut s, "status", json!({}), "2026-09-27").unwrap();
        assert_eq!(out.output["version"], "9.9.9");
        assert_eq!(out.output["supported_languages"], json!(["rust", "css"]));
        // Each configured language with its tier, in the config's order; one the build cannot read has none.
        let languages: Vec<String> = serde_json::from_value(out.output["languages"].clone()).unwrap();
        let tiers: Vec<serde_json::Value> = languages.iter().map(|l| json!({"language": l, "tier": match l.as_str() { "rust" => json!(2), "css" => json!(0), _ => json!(null) }})).collect();
        assert_eq!(out.output["tiers"], json!(tiers));
        // The rest of the answer is untouched by the build.
        assert!(out.output["nodes"].is_object());
        assert!(out.output["edges"].is_number());
    }

    /// # Test contract
    ///
    /// ```json
    /// {"test": "status-names-the-grimoire-and-its-library", "checks": ["crates/core/src/tools/state#library", "crates/core/src/tools/read#status"]}
    /// ```
    #[test]
    fn status_names_the_grimoire_and_its_library() {
        let mut s = state();
        // Served alone: no name, no library, and no ticket current.
        let out = call(&mut s, "status", json!({}), "2026-10-05").unwrap();
        assert_eq!((out.output["grimoire"].clone(), out.output["library"].clone(), out.output["current"].clone()), (json!(null), json!([]), json!(null)));
        // Served in a library, which is listed in name order, once for each checkout.
        let entry = |name: &str, repository: &str| crate::answer::LibraryEntry { name: name.into(), repository: repository.into(), server: None, current: None };
        s.set_library(
            "shop".into(),
            vec![entry("shop", "C:/repo"), entry("shop", "C:/wt"), crate::answer::LibraryEntry { server: Some("https://team.example".into()), ..entry("ledger", "C:/ledger") }],
        );
        let out = call(&mut s, "status", json!({}), "2026-10-05").unwrap();
        assert_eq!(out.output["grimoire"], "shop");
        assert_eq!(
            out.output["library"],
            json!([
                { "name": "ledger", "repository": "C:/ledger", "server": "https://team.example", "current": null },
                { "name": "shop", "repository": "C:/repo", "server": null, "current": null },
                { "name": "shop", "repository": "C:/wt", "server": null, "current": null }
            ])
        );
        // Each checkout of this grimoire with its current ticket: this one's as it is now, another's as last told; another grimoire's none.
        s.store.current = Some("later-1".into());
        let told = crate::answer::CurrentTicket { id: "0007".into(), title: "Seven".into() };
        s.tell_checkout_tickets(vec![("C:/wt".to_string(), Some(told.clone())), ("C:/ledger".to_string(), Some(told))]);
        let out = call(&mut s, "status", json!({}), "2026-10-05").unwrap();
        assert_eq!(out.output["current"], json!({ "id": "later-1", "title": "LATER-1" }));
        let currents: Vec<serde_json::Value> = out.output["library"].as_array().unwrap().iter().map(|e| e["current"].clone()).collect();
        assert_eq!(currents, vec![json!(null), json!({ "id": "later-1", "title": "LATER-1" }), json!({ "id": "0007", "title": "Seven" })]);
        // Told again of one checkout with none, and of nothing for the other, which keeps what was last told.
        s.tell_checkout_tickets(vec![("C:/wt".to_string(), None)]);
        assert_eq!(call(&mut s, "status", json!({}), "2026-10-05").unwrap().output["library"][2]["current"], json!(null));
        s.tell_checkout_tickets(vec![("C:/wt".to_string(), Some(crate::answer::CurrentTicket { id: "0008".into(), title: "Eight".into() }))]);
        s.tell_checkout_tickets(vec![]);
        assert_eq!(call(&mut s, "status", json!({}), "2026-10-05").unwrap().output["library"][2]["current"]["id"], "0008");
    }

    /// # Test contract
    ///
    /// ```json
    /// {"test": "coverage-of-each-kind", "checks": ["crates/core/src/query/coverage#coverage"]}
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
    /// {"test": "coverage-lists-the-tests-of-a-criterion", "checks": ["crates/core/src/query/lookup#find", "crates/core/src/query/coverage#coverage"]}
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

    /// A reader over a fixed set of files, as the daemon's reads the repository.
    #[derive(Debug)]
    struct Memory(Vec<(&'static str, String)>);

    impl SourceReader for Memory {
        fn read(&self, path: &str, limit: usize) -> Result<String, SourceError> {
            let text = self.0.iter().find(|(p, _)| *p == path).map(|(_, t)| t.clone()).ok_or_else(|| SourceError::Unreadable(format!("no file {path}")))?;
            if text.len() > limit {
                return Err(SourceError::TooLarge(text.len() as u64));
            }
            Ok(text)
        }
    }

    /// # Test contract
    ///
    /// ```json
    /// {"test": "source-answers-a-nodes-file-with-its-annotations", "checks": ["crates/core/src/tools/read#source", "crates/core/src/query/source#source-annotations"]}
    /// ```
    #[test]
    fn source_answers_a_nodes_file_with_its_annotations() {
        let mut s = state();
        // No reader: the caller cannot read files.
        let e = call(&mut s, "source", json!({ "ref": "criterion:src/a/ready#guarantee" }), "2026-09-29").unwrap_err();
        assert!(matches!(e, ToolError::StoreUnavailable { .. }), "{e:?}");
        let text = (1..=25).map(|n| format!("line {n}")).collect::<Vec<_>>().join(
            "
",
        );
        s.set_source(Arc::new(Memory(vec![("src/a/ready.rs", text.clone())])));
        let out = call(&mut s, "source", json!({ "ref": "criterion:src/a/ready#guarantee" }), "2026-09-29").unwrap().output;
        assert_eq!(out["file"], "file:src/a/ready.rs");
        assert_eq!(out["language"], "rust");
        assert_eq!(out["text"], text);
        assert_eq!(out["focus"], 1, "a criterion's line is its contract heading");
        // The contract with its criteria, the declaration, and the test block with what it checks, by line.
        let lines: Vec<(u64, String)> = out["annotations"].as_array().unwrap().iter().map(|a| (a["line"].as_u64().unwrap(), a["kind"].as_str().unwrap().to_string())).collect();
        assert_eq!(lines, vec![(1, "contract".into()), (10, "decl".into()), (20, "test".into())]);
        let contract = &out["annotations"][0];
        assert_eq!(contract["ref"], "module:src/a/ready");
        let criteria: Vec<&str> = contract["links"].as_array().unwrap().iter().map(|l| l["ref"].as_str().unwrap()).collect();
        assert_eq!(criteria, vec!["criterion:src/a/ready#guarantee", "criterion:src/a/ready#reads-only"]);
        assert_eq!(out["annotations"][2]["links"][0]["ref"], "criterion:src/a/ready#guarantee", "a test block links what it checks");
        // A declaration focuses its own line; a file its first.
        assert_eq!(call(&mut s, "source", json!({ "ref": "decl:src/a/ready.rs#run" }), "2026-09-29").unwrap().output["focus"], 10);
        assert_eq!(call(&mut s, "source", json!({ "ref": "file:src/a/ready.rs" }), "2026-09-29").unwrap().output["focus"], 1);
        // Not a node with a file; a node that does not exist; a reader that fails.
        assert!(matches!(call(&mut s, "source", json!({ "ref": "page:overview" }), "2026-09-29").unwrap_err(), ToolError::Invalid { .. }));
        assert!(matches!(call(&mut s, "source", json!({ "ref": "decl:src/nope.rs#x" }), "2026-09-29").unwrap_err(), ToolError::NotFound { .. }));
        assert!(matches!(call(&mut s, "source", json!({ "ref": "file:src/a/layout.rs" }), "2026-09-29").unwrap_err(), ToolError::StoreUnavailable { .. }));
        // Past the plain-file limit it is shown, marked plain; at it, not.
        assert_eq!(call(&mut s, "source", json!({ "ref": "file:src/a/ready.rs" }), "2026-09-29").unwrap().output["plain"], false);
        s.store_mut().config.plain_file = Some(2);
        s.set_source(Arc::new(Memory(vec![("src/a/ready.rs", "a\nb\n".into())])));
        assert_eq!(call(&mut s, "source", json!({ "ref": "file:src/a/ready.rs" }), "2026-09-29").unwrap().output["plain"], false);
        s.set_source(Arc::new(Memory(vec![("src/a/ready.rs", "a\nb\nc".into())])));
        let out = call(&mut s, "source", json!({ "ref": "file:src/a/ready.rs" }), "2026-09-29").unwrap().output;
        assert_eq!((out["plain"].as_bool(), out["text"].as_str()), (Some(true), Some("a\nb\nc")), "the whole text, marked plain");
        // Too large to show, in bytes or in lines.
        s.set_source(Arc::new(Memory(vec![("src/a/ready.rs", "x".repeat(SOURCE_LIMIT + 1))])));
        assert!(matches!(call(&mut s, "source", json!({ "ref": "file:src/a/ready.rs" }), "2026-09-29").unwrap_err(), ToolError::Invalid { .. }));
        s.set_source(Arc::new(Memory(vec![("src/a/ready.rs", "\n".repeat(SOURCE_LINES + 1))])));
        assert!(matches!(call(&mut s, "source", json!({ "ref": "file:src/a/ready.rs" }), "2026-09-29").unwrap_err(), ToolError::Invalid { .. }));
    }

    /// # Test contract
    ///
    /// ```json
    /// {"test": "get-reads-several-at-once", "checks": ["crates/core/src/tools/read#get-several", "req:tool-get"]}
    /// ```
    #[test]
    fn get_reads_several_at_once() {
        let mut s = state();
        let out = call(&mut s, "get", json!({ "refs": ["req:stale", "page:nope", "req:ready-only-with-commit", "ticket:later-1"] }), "2026-10-08").unwrap().output;
        let got: Vec<&str> = out["nodes"].as_array().unwrap().iter().map(|n| n["node"]["ref"].as_str().unwrap()).collect();
        assert_eq!(got, ["req:stale", "req:ready-only-with-commit", "ticket:later-1"], "in the order given");
        assert_eq!(out["missing"], json!(["page:nope"]), "listed, not refused");
        // Each as get of it alone answers it.
        let alone = call(&mut s, "get", json!({ "ref": "ticket:later-1" }), "2026-10-08").unwrap().output;
        assert_eq!(out["nodes"][2], alone);
        // Both or neither: invalid.
        assert_eq!(call(&mut s, "get", json!({ "ref": "req:stale", "refs": ["req:stale"] }), "2026-10-08").unwrap_err().code(), "invalid");
        assert_eq!(call(&mut s, "get", json!({}), "2026-10-08").unwrap_err().code(), "invalid");
        assert_eq!(call(&mut s, "get", json!({ "ref": "page:nope" }), "2026-10-08").unwrap_err().code(), "not_found", "one alone is still not_found");
    }

    /// # Test contract
    ///
    /// ```json
    /// {"test": "reads-take-a-ticket", "checks": ["crates/core/src/tools/read#ticket-view"]}
    /// ```
    #[test]
    fn reads_take_a_ticket() {
        let mut s = state();
        let code = |r: Result<crate::tools::Outcome, crate::ops::ToolError>| r.unwrap_err().code();
        call(&mut s, "put_page", json!({ "slug": "orphan", "title": "ORPHAN", "body": "proposed", "links": ["page:nowhere"], "ticket": "later-1" }), "2026-10-02").unwrap();
        // The truth, naming the tickets that would change it; the branch, as proposed.
        let truth = call(&mut s, "get", json!({ "ref": "page:orphan" }), "2026-10-02").unwrap().output;
        assert_ne!(truth["node"]["body"], "proposed");
        assert_eq!(truth["drafts"], json!(["ticket:later-1"]));
        let branch = call(&mut s, "get", json!({ "ref": "page:orphan", "ticket": "ticket:later-1" }), "2026-10-02").unwrap().output;
        assert_eq!(branch["node"]["body"], "proposed");
        assert_eq!(branch["drafts"], json!(["ticket:later-1"]), "the tickets touching the node, however it is read");
        // The ticket's page carries its proposal.
        let t = call(&mut s, "get", json!({ "ref": "ticket:later-1" }), "2026-10-02").unwrap().output;
        assert_eq!(t["proposal"]["pages"], json!([{ "ref": "page:orphan", "title": "ORPHAN", "new": false, "differs": ["body", "links"] }]));
        // A record put back equal to the truth's changes nothing at the close, so it is no draft.
        let same = s.store().pages["orphan"].clone();
        call(&mut s, "put_page", json!({ "slug": "orphan", "title": same.title, "body": same.body, "links": same.links, "ticket": "later-1" }), "2026-10-02").unwrap();
        assert!(call(&mut s, "get", json!({ "ref": "page:orphan" }), "2026-10-02").unwrap().output.get("drafts").is_none());
        assert_eq!(call(&mut s, "get", json!({ "ref": "ticket:later-1" }), "2026-10-02").unwrap().output["proposal"]["pages"], json!([]));
        call(&mut s, "put_page", json!({ "slug": "orphan", "title": "ORPHAN", "body": "proposed", "links": ["page:nowhere"], "ticket": "later-1" }), "2026-10-02").unwrap();
        assert_eq!(code(call(&mut s, "get", json!({ "ref": "page:orphan", "ticket": "phase-62" }), "2026-10-02")), "conflict", "an inscribed ticket has no branch");
        assert_eq!(code(call(&mut s, "get", json!({ "ref": "page:orphan", "ticket": "nope" }), "2026-10-02")), "not_found");
        assert_eq!(code(call(&mut s, "get", json!({ "ref": "page:orphan", "ticket": 7 }), "2026-10-02")), "invalid");
        // lint: the truth without a ticket, the branch with one, the current ticket's branch when there is one.
        let dangling = |s: &mut State, input: serde_json::Value| call(s, "lint", input, "2026-10-02").unwrap().output["counts"]["dangling"].as_u64().unwrap();
        let before = dangling(&mut s, json!({}));
        assert_eq!(dangling(&mut s, json!({ "ticket": "later-1" })), before + 1);
        s.store_mut().current = Some("later-1".into());
        assert_eq!(dangling(&mut s, json!({})), before + 1);
        // rests_on with it says what the proposal says, without it what the truth does.
        let orphan_says = |s: &mut State, input: serde_json::Value| {
            let out = call(s, "rests_on", input, "2026-10-02").unwrap().output;
            out["nodes"].as_array().cloned().unwrap_or_default().into_iter().find(|x| x["ref"] == "page:orphan").map(|x| x["says"].clone())
        };
        assert_eq!(orphan_says(&mut s, json!({ "ref": "ticket:later-1", "ticket": "later-1" })), Some(json!("proposed")));
        assert_ne!(orphan_says(&mut s, json!({ "ref": "ticket:later-1" })), Some(json!("proposed")));
        // Every read takes it: a few of the rest.
        for (name, input) in
            [("find", json!({ "query": "orphan" })), ("outline", json!({ "depth": 1 })), ("rests_on", json!({ "ref": "page:orphan" })), ("coverage", json!({ "ref": "module:src/a/ready" }))]
        {
            let mut with = input.clone();
            with["ticket"] = json!("later-1");
            assert!(call(&mut s, name, with, "2026-10-02").is_ok(), "{name}");
            let mut bad = input;
            bad["ticket"] = json!("phase-62");
            assert_eq!(code(call(&mut s, name, bad, "2026-10-02")), "conflict", "{name}");
        }
    }

    /// # Test contract
    ///
    /// ```json
    /// {"test": "with-no-code-a-question-about-the-code-is-refused", "checks": ["crates/core/src/tools/read#without-code-refused", "crates/core/src/tools/read#without-code-outline", "crates/core/src/tools/read#without-code-walk-title"]}
    /// ```
    #[test]
    fn with_no_code_a_question_about_the_code_is_refused_not_answered_missing() {
        use crate::tools::call;
        use serde_json::json;
        let truth = crate::tools::fixtures::state();
        let mut s = State::new(truth.store().clone(), crate::extraction::Extraction::from_files(&Vec::<crate::extraction::FileExtraction>::new()), vec![]);
        s.set_without_code(true);
        let code = |r: Result<crate::tools::Outcome, ToolError>| r.unwrap_err();
        // About a node from the code, or anything only the code answers: refused, saying why, never "does not exist".
        for (tool, input) in [
            ("get", json!({ "ref": "module:src/a/ready" })),
            ("get", json!({ "ref": "criterion:src/a/ready#guarantee" })),
            ("rests_on", json!({ "ref": "module:src/a/ready" })),
            ("rests_on_me", json!({ "ref": "criterion:src/a/ready#guarantee" })),
            ("coverage", json!({ "ref": "req:ready-only-with-commit" })),
            ("module_source", json!({ "ref": "module:src/a/ready" })),
            ("source", json!({ "ref": "file:src/a/ready.rs" })),
        ] {
            let e = code(call(&mut s, tool, input.clone(), "2026-10-03"));
            assert_eq!(e.code(), "store_unavailable", "{tool} {input}");
            assert!(serde_json::to_value(&e).unwrap()["message"].as_str().unwrap().contains("holds no code"), "{tool}");
        }
        // About intent: answered. A walk that reaches the code says it is not held here.
        assert_eq!(call(&mut s, "get", json!({ "ref": "req:ready-only-with-commit" }), "2026-10-03").unwrap().output["node"]["slug"], "ready-only-with-commit");
        let walk = call(&mut s, "rests_on", json!({ "ref": "req:ready-only-with-commit" }), "2026-10-03").unwrap().output;
        let titles: Vec<&str> = walk["nodes"].as_array().unwrap().iter().map(|n| n["title"].as_str().unwrap()).collect();
        assert!(titles.iter().any(|t| t.starts_with("(no code held) criterion:")) && !titles.iter().any(|t| t.starts_with("(missing)")), "{titles:?}");
        assert_eq!(call(&mut s, "outline", json!({}), "2026-10-03").unwrap().output["without_code"], true);
        // A state with the code answers them all, and a missing node there is missing.
        let mut with = crate::tools::fixtures::state();
        assert!(call(&mut with, "get", json!({ "ref": "module:src/a/ready" }), "2026-10-03").is_ok());
        assert_eq!(code(call(&mut with, "get", json!({ "ref": "module:src/nope" }), "2026-10-03")).code(), "not_found");
        assert_eq!(call(&mut with, "outline", json!({}), "2026-10-03").unwrap().output["without_code"], false);
    }
}
