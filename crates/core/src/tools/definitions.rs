//! # Module contract
//!
//! ```json
//! {"purpose":"Describe every tool for MCP tools/list and the API: its name, a description and its JSON input schema.","criteria":[{"id":"definitions-match","statement":"Every tool the dispatcher accepts is listed in definitions() with an input schema, and nothing is listed that it does not accept."}]}
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
    let reference = json!({ "type": "string", "description": "A node reference of the form kind:id, e.g. page:overview, req:ready-only, module:crates/core/src/model, criterion:<module>#<id>, ticket:phase-62" });
    let refs = json!({ "type": "array", "items": reference });
    let date = json!({ "type": "string", "description": "ISO date YYYY-MM-DD" });
    let decision = json!({ "type": "object", "required": ["text"], "properties": { "text": { "type": "string" }, "date": date } });
    let scenario = json!({ "type": "object", "required": ["title", "given", "when", "then"], "properties": { "title": {"type":"string"}, "given": {"type":"string"}, "when": {"type":"string"}, "then": {"type":"string"} } });
    let obj = |required: &[&str], props: Value| json!({ "type": "object", "required": required, "properties": props });
    vec![
        ToolDef { name: "find", description: "Search nodes by text, optionally by kind. Matches slug, title, statement, purpose, body, declaration path and test name.", input_schema: obj(&["query"], json!({ "query": {"type":"string"}, "kinds": {"type":"array","items":{"type":"string","enum":["page","req","ticket","module","criterion","file","decl","test"]}}, "limit": {"type":"integer","default":20} })) },
        ToolDef { name: "get", description: "One node in full with its edges: `out` is what it links to, `in` is what links to it.", input_schema: obj(&["ref"], json!({ "ref": reference })) },
        ToolDef { name: "outline", description: "The grimoire as a tree from the root pages, with coverage, lint-finding counts (and how many are baselined) and the adoption rollup per node, plus the modules no page links.", input_schema: obj(&[], json!({ "depth": {"type":"integer","default":2}, "root": reference })) },
        ToolDef { name: "rests_on", description: "What a node depends on, transitively downward, with the decisions behind each. The planning and review question. For a ticket, starts from relies_on and touches.", input_schema: obj(&["ref"], json!({ "ref": reference, "depth": {"type":"integer"} })) },
        ToolDef { name: "rests_on_me", description: "Everything that would be affected if this node changed, and the open tickets that reach it.", input_schema: obj(&["ref"], json!({ "ref": reference })) },
        ToolDef { name: "coverage", description: "What requirements cover a criterion, each criterion of a module with its requirements, or each criterion a requirement names with its module.", input_schema: obj(&["ref"], json!({ "ref": reference })) },
        ToolDef { name: "board", description: "What can be worked on: open and active change tickets ordered next, high, later, each with the gaps and questions it waits on (ready when none is open); the open gaps and questions no change relies on; and, when asked, the most recently closed tickets, each answered gap or question with the changes that followed it (none: nothing was built on the answer). Filter by status, kind, priority or a node they touch.", input_schema: obj(&[], json!({ "status": {"type":"array","items":{"type":"string","enum":["open","active","closed"]}}, "kinds": {"type":"array","items":{"type":"string","enum":["change","gap","question"]}}, "priority": {"type":"array","items":{"type":"string","enum":["next","high","later"]}}, "touching": reference, "closed": {"type":"integer","default":0,"description":"How many of the most recently closed tickets to return"} })) },
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
        ToolDef { name: "rename_node", description: "Rename a page or requirement, keeping its decisions and rewriting the references the store holds to it. Closed tickets keep the old reference. Answers with the node and the test blocks in the code that still name the old reference, which the agent edits.", input_schema: obj(&["ref","to"], json!({ "ref": reference, "to": {"type":"string","description":"The new slug"} })) },
        ToolDef { name: "reorder", description: "Set the order of one priority band on the board: the tickets listed come first, in that order; the rest follow by dependency, then title.", input_schema: obj(&["priority","order"], json!({ "priority": {"type":"string","enum":["next","high","later"]}, "order": {"type":"array","items": reference} })) },
        ToolDef { name: "source", description: "A node's file as an annotated view: the text of the file holding a module, criterion, file, declaration or test, the node's line, and the lines the graph attaches nodes to (the contract, test blocks, declarations). Reads only files the graph knows.", input_schema: obj(&["ref"], json!({ "ref": reference })) },
        ToolDef { name: "status", description: "Counts of nodes and edges, the repository path, how many imports did not resolve, and the running build's version and supported languages next to the store's languages.", input_schema: obj(&[], json!({})) },
    ]
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tools::{call, TOOL_NAMES};
    use crate::tools::fixtures::state;

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
}
