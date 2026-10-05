//! # Module contract
//!
//! ```json
//! {"purpose":"Describe every tool for MCP tools/list and the API: its name, a description and its JSON input schema.","criteria":[{"id":"definitions-match","statement":"Every tool the dispatcher accepts is listed in definitions() with an input schema, and nothing is listed that it does not accept."},{"id":"checkout-offered","statement":"Every tool's input schema offers `checkout`, a string, described as the caller's working directory, which a daemon serving several checkouts of the grimoire uses to choose the caller's, and none requires it."}]}
//! ```

use serde_json::{json, Value};

/// A tool's name, description and JSON input schema, for MCP `tools/list`
/// and for the API's self-description.
#[derive(Debug, Clone, serde::Serialize)]
pub struct ToolDef {
    pub name: &'static str,
    pub description: &'static str,
    pub input_schema: Value,
}

pub fn definitions() -> Vec<ToolDef> {
    let reference =
        json!({ "type": "string", "description": "A node reference of the form kind:id, e.g. page:overview, req:ready-only, module:crates/core/src/model, criterion:<module>#<id>, ticket:phase-62" });
    let refs = json!({ "type": "array", "items": reference });
    let date = json!({ "type": "string", "description": "ISO date YYYY-MM-DD" });
    let decision = json!({ "type": "object", "required": ["text"], "properties": { "text": { "type": "string" }, "date": date }, "additionalProperties": false });
    let scenario = json!({ "type": "object", "required": ["title", "given", "when", "then"], "properties": { "title": {"type":"string"}, "given": {"type":"string"}, "when": {"type":"string"}, "then": {"type":"string"} } });
    // The reads and the writes that change intent take `ticket`: a read then answers as that ticket's proposal would leave the graph; the write lands in its proposal. No other write takes it.
    let ticket = json!({ "type": "string", "description": "A ticket slug or ticket: reference. A read answers as that ticket's proposal would leave the graph; a write that changes intent lands in its proposal. Absent, a write lands in the store's current ticket, the one last started, and is refused when there is none; a read sees the truth, except lint, which uses the current ticket." });
    let obj = |required: &[&str], props: Value| json!({ "type": "object", "required": required, "properties": props });
    // Tools whose input refuses fields it does not know say so, so a misspelt field is an error, not ignored.
    let strict = |required: &[&str], props: Value| json!({ "type": "object", "required": required, "properties": props, "additionalProperties": false });
    let mut defs = vec![
        ToolDef { name: "find", description: "Search nodes by text, optionally by kind. Matches slug, title, statement, purpose, body, declaration path and test name.", input_schema: obj(&["query"], json!({ "query": {"type":"string"}, "kinds": {"type":"array","items":{"type":"string","enum":["page","req","ticket","module","criterion","file","decl","test"]}}, "limit": {"type":"integer","default":20} })) },
        ToolDef { name: "get", description: "One node in full with its edges: `out` is what it links to, `in` is what links to it.", input_schema: obj(&["ref"], json!({ "ref": reference })) },
        ToolDef { name: "outline", description: "The grimoire as a tree from the root pages, with coverage, lint-finding counts (and how many are baselined) and the adoption rollup per node, plus the modules no page links. pages_only keeps only pages in the trees, their counts and rollups unchanged: the whole grimoire's shape in a small answer.", input_schema: obj(&[], json!({ "depth": {"type":"integer","default":2}, "root": reference, "pages_only": {"type":"boolean","default":false} })) },
        ToolDef { name: "rests_on", description: "What a node depends on, transitively downward, with the decisions behind each. The planning and review question. For a ticket, starts from relies_on and touches.", input_schema: obj(&["ref"], json!({ "ref": reference, "depth": {"type":"integer"} })) },
        ToolDef { name: "rests_on_me", description: "Everything that would be affected if this node changed, and the tickets not yet closed that reach it.", input_schema: obj(&["ref"], json!({ "ref": reference })) },
        ToolDef { name: "coverage", description: "What requirements cover a criterion, each criterion of a module with its requirements, or each criterion a requirement names with its module.", input_schema: obj(&["ref"], json!({ "ref": reference })) },
        ToolDef { name: "board", description: "What can be worked on: open, active and landing change tickets ordered next, high, later, each with the gaps and questions it waits on (ready when none is open); the open gaps and questions no change relies on; and, when asked, the most recently closed tickets, each answered gap or question with the changes that followed it (none: nothing was built on the answer). Filter by status, kind, priority or a node they touch.", input_schema: obj(&[], json!({ "status": {"type":"array","items":{"type":"string","enum":["open","active","landing","closed"]}}, "kinds": {"type":"array","items":{"type":"string","enum":["change","gap","question"]}}, "priority": {"type":"array","items":{"type":"string","enum":["next","high","later"]}}, "touching": reference, "closed": {"type":"integer","default":0,"description":"How many of the most recently closed tickets to return"} })) },
        ToolDef { name: "related_tickets", description: "The open gaps and questions that touch what this ticket touches.", input_schema: obj(&["ref"], json!({ "ref": reference })) },
        ToolDef { name: "lint", description: "The dangling, uncovered, malformed and unspecified checks over the whole graph. Same output as `grimoire check`.", input_schema: obj(&[], json!({ "kinds": {"type":"array","items":{"type":"string","enum":["dangling","uncovered","malformed","unspecified"]}} })) },
        ToolDef { name: "module_source", description: "Where a module, criterion, file or declaration lives: file and line, plus the contract line for criteria.", input_schema: obj(&["ref"], json!({ "ref": reference })) },
        ToolDef { name: "put_page", description: "Create or replace a page. Links may point to page, req and module references.", input_schema: obj(&["slug","title","body","links"], json!({ "slug": {"type":"string"}, "title": {"type":"string"}, "body": {"type":"string","description":"Markdown"}, "links": refs })) },
        ToolDef { name: "patch_page", description: "Change a page's title or body, or add and remove links, leaving everything else as it was. The usual way to link a new requirement or page from its area.", input_schema: obj(&["ref"], json!({ "ref": reference, "title": {"type":"string"}, "body": {"type":"string","description":"Markdown"}, "add_links": refs, "remove_links": refs })) },
        ToolDef { name: "put_requirement", description: "Create or replace a requirement. Existing decisions are preserved; use record_decision to add one.", input_schema: obj(&["slug","title","statement","scenarios","covered_by"], json!({ "slug": {"type":"string"}, "title": {"type":"string"}, "statement": {"type":"string","description":"One normative paragraph (SHALL / MUST)"}, "scenarios": {"type":"array","items": scenario}, "covered_by": {"type":"array","items": reference, "description":"criterion references"} })) },
        ToolDef { name: "record_decision", description: "Append a dated decision to a requirement: what was chosen and why. A decision that replaces earlier ones names them in supersedes, by position (1 for the oldest); they stay, no longer in force.", input_schema: strict(&["ref","text"], json!({ "ref": reference, "text": {"type":"string"}, "date": date, "supersedes": {"type":"array","items":{"type":"integer","minimum":1},"description":"Positions of earlier decisions this one replaces"}, "adopted": {"type":"boolean","description":"Recorded at adoption: the requirement was read off the code and describes it, until someone decides it"} })) },
        ToolDef { name: "supersede", description: "Link an existing decision of a requirement to the earlier decisions it replaced, by position (1 for the oldest), without changing any text or date. For a supersession already written in prose; a new decision names what it supersedes in record_decision.", input_schema: strict(&["ref","decision","supersedes"], json!({ "ref": reference, "decision": {"type":"integer","minimum":1}, "supersedes": {"type":"array","items":{"type":"integer","minimum":1}} })) },
        ToolDef { name: "retire_node", description: "Retire a requirement that no longer holds: records a last decision saying why, superseding all the others, marks it retired and removes it from every page. It stays readable by reference with its decisions but covers no criterion, so the criteria it named come back as warnings unless another requirement names them. A requirement with decisions is retired, never deleted.", input_schema: strict(&["ref","text"], json!({ "ref": reference, "text": {"type":"string","description":"Why it no longer holds"}, "date": date })) },
        ToolDef { name: "file_ticket", description: "Create an open ticket: a change, gap or question, with the nodes it touches and relies on. The id is generated; the title is the ticket's name.", input_schema: obj(&["title","kind","priority","body"], json!({ "title": {"type":"string"}, "kind": {"type":"string","enum":["change","gap","question"]}, "priority": {"type":"string","enum":["next","high","later"]}, "touches": refs, "relies_on": refs, "body": {"type":"string","description":"Markdown, at most 4000 characters"} })) },
        ToolDef { name: "update_ticket", description: "Change a ticket that is not closed: its title, priority, touches, relies_on or body. Title, priority and body change at once, and so do a ticket's own touches and relies_on. Touches and relies_on given for another ticket under a change being worked, the one `ticket` names or else the current ticket, wait in that change's proposal and reach the ticket when the change lands; under a gap or question, or no ticket, they apply at once.", input_schema: obj(&["ref"], json!({ "ref": reference, "title": {"type":"string"}, "priority": {"type":"string","enum":["next","high","later"]}, "touches": refs, "relies_on": refs, "body": {"type":"string"} })) },
        ToolDef { name: "start_ticket", description: "Move a ticket from open to active and return what it rests on and the related gaps and questions. It becomes the current ticket, which on a project that names a server is this checkout's own: there a ticket already active or landing is taken up in this checkout, and nothing of it changes.", input_schema: obj(&["ref"], json!({ "ref": reference })) },
        ToolDef { name: "close_ticket", description: "Close a ticket with its decision: its proposal is applied to the truth, or the close is refused as a conflict naming the records whose truth moved since they were proposed. The body is discarded. Returns the new lint findings on the nodes it touched, and how many baselined ones sit there. With when: landed, for a change whose code has still to land, the decision is written and the ticket becomes landing instead, its proposal kept until land_ticket closes it.", input_schema: obj(&["ref","decision"], json!({ "ref": reference, "decision": decision, "when": {"type":"string","enum":["now","landed"],"description":"now (the default) closes at once; landed writes the decision and waits for the change to land"} })) },
        ToolDef { name: "land_ticket", description: "Tell Grimoire a change landed: close the landing ticket with the decision it carries, applying its proposal to the truth, or refuse as a conflict naming the records whose truth moved. With no ref, the current ticket lands when it is landing and otherwise nothing does; on a project that names a server nothing lands without a ref, since a ticket there lands on the main line, and `note` says so. Returns whether one landed, what its close returns, and the tickets still landing.", input_schema: obj(&[], json!({ "ref": reference })) },
        ToolDef { name: "delete_node", description: "Remove a page, a requirement with no decision, or a ticket that is not closed. Returns the dangling edges the deletion created. A requirement with decisions is retired with retire_node instead, and a closed ticket is history and stays.", input_schema: obj(&["ref"], json!({ "ref": reference })) },
        ToolDef { name: "adopt", description: "Record every present finding as the baseline: inherited work the check does not fail on. Only when the person asks; it needs their reason.", input_schema: obj(&["reason"], json!({ "reason": {"type":"string","description":"Why the present findings are accepted as inherited"}, "findings": {"type":"array","description":"The findings to record, each as lint gives it: rule, check, ref and its edge when it has one. Leave it out: the daemon records what it sees. A machine's daemon supplies them to a server that holds no code.","items":{"type":"object","required":["rule","check","ref"],"properties":{"rule":{"type":"string"},"check":{"type":"string"},"ref":reference,"edge":{"type":"object"}}}} })) },
        ToolDef { name: "configure", description: "Change the store config's roots, languages, contract scope or site; only the fields given. Answers with the config and how many findings are now new. The repository is set by init only.", input_schema: obj(&[], json!({ "roots": {"description":"The two roots' page slugs: {grimoire?, rites?}, changing only the roots given, or a list as older stores held, the first the grimoire and the second the rites","oneOf":[{"type":"object","properties":{"grimoire":{"type":"string"},"rites":{"type":"string"}},"additionalProperties":false},{"type":"array","items":{"type":"string"}}]}, "site": {"type":"array","items":{"type":"string"},"description":"The page slugs the project's site shows, each with every page beneath it, when the server is started with --site"}, "languages": {"type":"array","items":{"type":"string"}}, "scope": {"type":"object","properties":{"include":{"type":"array","items":{"type":"string"}},"exclude":{"type":"array","items":{"type":"string"}}},"additionalProperties":false,"description":"Repository-relative path prefixes expected to carry contracts; an empty include means everything"} })) },
        ToolDef { name: "rename_node", description: "Rename a page or requirement, keeping its decisions and rewriting the references the store holds to it. Closed tickets keep the old reference. Answers with the node and the test blocks in the code that still name the old reference, which the agent edits.", input_schema: obj(&["ref","to"], json!({ "ref": reference, "to": {"type":"string","description":"The new slug"} })) },
        ToolDef { name: "reorder", description: "Set the order of one priority band on the board: the tickets listed come first, in that order; the rest follow by dependency, then title.", input_schema: obj(&["priority","order"], json!({ "priority": {"type":"string","enum":["next","high","later"]}, "order": {"type":"array","items": reference} })) },
        ToolDef { name: "source", description: "A node's file as an annotated view: the text of the file holding a module, criterion, file, declaration or test, the node's line, and the lines the graph attaches nodes to (the contract, test blocks, declarations). Reads only files the graph knows.", input_schema: obj(&["ref"], json!({ "ref": reference })) },
        ToolDef { name: "status", description: "Counts of nodes and edges, the repository path, how many imports did not resolve, and the running build's version and supported languages next to the store's languages.", input_schema: obj(&[], json!({})) },
    ];
    for d in defs.iter_mut().filter(|d| !super::is_write(d.name) || super::takes_ticket(d.name)) {
        d.input_schema["properties"]["ticket"] = ticket.clone();
    }
    // Every tool takes the caller's checkout, which the daemon routes by and the tool never sees (req:machine-checkouts).
    let checkout = json!({ "type": "string", "description": "Your working directory, as an absolute path. Pass it with every call: one daemon may serve several checkouts of this grimoire, such as worktrees, and answers from the code and current ticket of the one you name. With several served, a call that names none is refused." });
    for d in defs.iter_mut() {
        d.input_schema["properties"]["checkout"] = checkout.clone();
    }
    defs
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tools::fixtures::state;
    use crate::tools::{call, TOOL_NAMES};

    /// # Test contract
    ///
    /// ```json
    /// {"test": "definitions-cover-the-dispatcher", "checks": ["crates/core/src/tools/definitions#definitions-match"]}
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
    /// {"test": "ticket-is-offered-where-it-is-taken", "checks": ["crates/core/src/tools/write#ticket-named", "crates/core/src/tools/read#ticket-view"]}
    /// ```
    #[test]
    fn ticket_is_offered_where_it_is_taken() {
        for d in definitions() {
            let offered = d.input_schema["properties"].get("ticket").is_some();
            let taken = !crate::tools::is_write(d.name) || crate::tools::takes_ticket(d.name);
            assert_eq!(offered, taken, "{}", d.name);
        }
    }

    /// # Test contract
    ///
    /// ```json
    /// {"test": "every-tool-offers-checkout", "checks": ["crates/core/src/tools/definitions#checkout-offered", "crates/core/src/tools#checkout-ignored"]}
    /// ```
    #[test]
    fn every_tool_offers_checkout() {
        let mut s = state();
        for d in definitions() {
            assert_eq!(d.input_schema["properties"]["checkout"]["type"], "string", "{}", d.name);
            assert!(!d.input_schema["required"].as_array().is_some_and(|r| r.iter().any(|f| f == "checkout")), "{} does not require it", d.name);
        }
        // The tools never see it: a strict input, which refuses fields it does not know, takes it.
        let named = |mut input: Value| {
            input["checkout"] = json!("C:/somewhere/worktree");
            input
        };
        assert!(call(&mut s, "status", named(json!({})), "2026-10-05").is_ok());
        assert!(call(&mut s, "file_ticket", named(json!({ "title": "T", "kind": "gap", "priority": "later", "body": "b" })), "2026-10-05").is_ok());
        let e = call(&mut s, "get", named(json!({ "ref": "req:nope" })), "2026-10-05").unwrap_err();
        assert_eq!(e.code(), "not_found", "checkout is not refused as an unknown field: {e}");
        assert_eq!(call(&mut s, "get", json!({ "ref": "req:nope", "other": 1 }), "2026-10-05").unwrap_err().code(), "invalid", "an unknown field still is");
        // Of the tools, status alone is answered with no checkout named, since it lists the ones to name.
        let any: Vec<&str> = definitions().iter().map(|d| d.name).filter(|n| crate::tools::answers_any_checkout(n)).collect();
        assert_eq!(any, ["status"]);
    }

    /// # Test contract
    ///
    /// ```json
    /// {"test": "the-schemas-know-a-landing-ticket", "checks": ["crates/core/src/tools/definitions#definitions-match"]}
    /// ```
    #[test]
    fn the_schemas_know_a_landing_ticket() {
        let defs = definitions();
        let schema = |name: &str| defs.iter().find(|d| d.name == name).unwrap().input_schema.clone();
        // board can be asked for landing tickets, the status the dispatcher accepts and lists by default.
        assert_eq!(schema("board")["properties"]["status"]["items"]["enum"], serde_json::json!(["open", "active", "landing", "closed"]));
        assert_eq!(schema("close_ticket")["properties"]["when"]["enum"], serde_json::json!(["now", "landed"]));
        // land_ticket needs no ticket named, and applies at once: it offers no `ticket` to scope it.
        assert_eq!(schema("land_ticket")["required"], serde_json::json!([]));
        assert!(schema("land_ticket")["properties"].get("ticket").is_none());
    }
}
