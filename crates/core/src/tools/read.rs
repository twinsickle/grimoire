//! # Module contract
//!
//! ```json
//! {"purpose":"Answer the read tools: each parses its input, checks the reference it names and answers with a query over the current graph.","criteria":[{"id":"status","statement":"status returns the repository path, roots, languages, node counts by kind, edge count, unresolved import count, malformed file count, the contract scope and the baseline's date, reason and count, plus the build's version and supported_languages as the loader told State, so the answer is the same over every transport."},{"id":"lint-tool","statement":"lint runs the selected rules (all by default) and returns the findings with counts per rule; it is the same function grimoire check prints."},{"id":"source","statement":"source answers for a module, criterion, file, declaration or test with its file's text from the reader the caller supplied, the node's line as the focus, and the file's annotations; it asks the reader only for a file the graph knows and never for more than 2 MB, refuses a larger file or one over 20,000 lines as invalid, and answers store_unavailable when there is no reader or the reader fails."}]}
//! ```

use serde::Deserialize;
use serde_json::Value;

use super::{answer, parse, Outcome, RefIn, SourceError, State, SOURCE_LIMIT, SOURCE_LINES};
use crate::answer::*;
use crate::lint::{self, Rule};
use crate::model::*;
use crate::ops::ToolError;
use crate::query::{self, BoardFilter};

/// Run the read tool `name`.
pub(super) fn call(state: &mut State, name: &str, input: Value) -> Result<Outcome, ToolError> {
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
            answer(name, NodesAnswer { nodes: query::find(g, &i.query, i.kinds.as_deref(), i.limit.unwrap_or(20)) }, None)
        }
        "get" => {
            let i: RefIn = parse(input)?;
            let g = state.graph();
            let node = query::view(g, &i.r#ref).ok_or_else(|| ToolError::not_found(&i.r#ref))?;
            answer(name, NodeAnswer { node }, None)
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
            let g = state.graph();
            answer(name, query::outline(g, i.depth.unwrap_or(2), i.root.as_ref(), i.pages_only), None)
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
            answer(name, RestsOnAnswer { nodes: query::rests_on(g, &i.r#ref, i.depth) }, None)
        }
        "rests_on_me" => {
            let i: RefIn = parse(input)?;
            let g = state.graph();
            if !g.contains(&i.r#ref) {
                return Err(ToolError::not_found(&i.r#ref));
            }
            answer(name, query::rests_on_me(g, &i.r#ref), None)
        }
        "coverage" => {
            let i: RefIn = parse(input)?;
            let g = state.graph();
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
                #[serde(default)]
                closed: usize,
            }
            let i: In = parse(input)?;
            let g = state.graph();
            let f = BoardFilter { status: i.status.as_deref(), kinds: i.kinds.as_deref(), priority: i.priority.as_deref(), touching: i.touching.as_ref(), closed: i.closed };
            answer(name, query::board(g, &f), None)
        }
        "related_tickets" => {
            let i: RefIn = parse(input)?;
            let g = state.graph();
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
            let g = state.graph();
            let rules = i.kinds.unwrap_or_else(|| Rule::ALL.to_vec());
            let findings = lint::lint(g, &rules);
            let (counts, new) = (lint::counts(&findings), lint::new_count(&findings));
            answer(name, LintAnswer { findings, counts, new }, None)
        }
        "module_source" => {
            let i: RefIn = parse(input)?;
            let g = state.graph();
            if !matches!(i.r#ref.kind, Kind::Module | Kind::Criterion | Kind::File | Kind::Decl | Kind::Test) {
                return Err(ToolError::invalid(format!("{} is not a module, criterion, file, declaration or test", i.r#ref), vec![]));
            }
            let loc = query::module_source(g, &i.r#ref).ok_or_else(|| ToolError::not_found(&i.r#ref))?;
            answer(name, loc, None)
        }
        "source" => {
            let i: RefIn = parse(input)?;
            if !matches!(i.r#ref.kind, Kind::Module | Kind::Criterion | Kind::File | Kind::Decl | Kind::Test) {
                return Err(ToolError::invalid(format!("{} is not a module, criterion, file, declaration or test", i.r#ref), vec![]));
            }
            let reader = state.source.clone();
            let g = state.graph();
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
            if lines > SOURCE_LINES {
                return Err(ToolError::invalid(format!("{} has {lines} lines, over the {SOURCE_LINES} a source view shows", loc.file), vec![]));
            }
            let focus = if i.r#ref.kind == Kind::File { 1 } else { loc.line };
            let annotations = query::source_annotations(g, &loc.file);
            answer(name, SourceAnswer { file, language, text, focus, annotations }, None)
        }
        "status" => {
            let config = state.store.config.clone();
            let build = state.build.clone();
            let g = state.graph();
            let nodes = Kind::ALL.iter().map(|k| (k.to_string(), g.nodes.keys().filter(|r| r.kind == *k).count())).collect();
            let status = StatusAnswer {
                repository: config.repository,
                roots: config.roots,
                languages: config.languages,
                version: build.version,
                supported_languages: build.supported_languages,
                nodes,
                edges: g.edges.len(),
                unresolved_imports: g.unresolved_imports,
                malformed: g.malformed.len(),
                scope: config.scope,
                baseline: state.store.baseline.as_ref().map(baseline_summary),
            };
            answer(name, status, None)
        }
        other => Err(ToolError::invalid(format!("unknown tool `{other}`"), vec![])),
    }
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
        let text = (1..=25).map(|n| format!("line {n}")).collect::<Vec<_>>().join("
");
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
        // Too large to show, in bytes or in lines.
        s.set_source(Arc::new(Memory(vec![("src/a/ready.rs", "x".repeat(SOURCE_LIMIT + 1))])));
        assert!(matches!(call(&mut s, "source", json!({ "ref": "file:src/a/ready.rs" }), "2026-09-29").unwrap_err(), ToolError::Invalid { .. }));
        s.set_source(Arc::new(Memory(vec![("src/a/ready.rs", "\n".repeat(SOURCE_LINES + 1))])));
        assert!(matches!(call(&mut s, "source", json!({ "ref": "file:src/a/ready.rs" }), "2026-09-29").unwrap_err(), ToolError::Invalid { .. }));
    }
}
