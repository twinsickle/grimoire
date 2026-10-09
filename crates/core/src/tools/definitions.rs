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
    let reference = json!({ "type": "string", "description": "A node reference of the form kind:id, e.g. page:overview, req:ready-only, module:crates/core/src/model, criterion:<module>#<id>, req:<slug>#d3 (a decision), ticket:phase-62" });
    let refs = json!({ "type": "array", "items": reference });
    let date = json!({ "type": "string", "description": "ISO date YYYY-MM-DD" });
    let decision_id = json!({ "oneOf": [{"type":"integer","minimum":1}, {"type":"string","pattern":"^d[1-9][0-9]*$"}], "description": "A decision's id: d3, or 3" });
    let decision = json!({ "type": "object", "required": ["text"], "properties": { "text": { "type": "string" }, "date": date }, "additionalProperties": false });
    let scenario = json!({ "type": "object", "required": ["title", "given", "when", "then"], "properties": { "title": {"type":"string"}, "given": {"type":"string"}, "when": {"type":"string"}, "then": {"type":"string"} } });
    // The reads and the writes that change intent take `ticket`: a read then answers as that ticket's proposal would leave the graph; the write goes into its proposal. No other write takes it.
    let ticket = json!({ "type": "string", "description": "A ticket slug or ticket: reference. A read answers as that ticket's proposal would leave the graph; a write that changes intent goes into its proposal. Absent, a write goes into the store's current ticket, the one last started, and is refused when there is none; a read sees the truth, except lint, which uses the current ticket." });
    // A body edited in part, as patch_page and update_ticket take it; neither is given beside a whole body.
    let append = json!({ "type": "string", "description": "Markdown added at the end of the body as a new paragraph, after a blank line" });
    let replace = json!({ "type": "object", "required": ["old", "new"], "properties": { "old": {"type":"string","description":"The passage to replace, exactly as the body has it; refused unless it is in the body exactly once"}, "new": {"type":"string","description":"What replaces it"} }, "additionalProperties": false, "description": "One passage of the body swapped for another, applied before append" });
    let outcome = json!({"type":"string","enum":["held","updated"],"description":"held: it still says truly how to use what it represents; updated: it was changed to say so"});
    let fingerprint =
        json!({"type":"string","description":"Leave it out: the daemon takes the recipe's digest from this checkout's code. A machine's daemon supplies it to a server that holds no code."});
    let obj = |required: &[&str], props: Value| json!({ "type": "object", "required": required, "properties": props });
    // Tools whose input refuses fields it does not know say so, so a misspelt field is an error, not ignored.
    let strict = |required: &[&str], props: Value| json!({ "type": "object", "required": required, "properties": props, "additionalProperties": false });
    let mut defs = vec![
        ToolDef { name: "find", description: "Search nodes by text, optionally by kind. Matches slug, title, statement, purpose, body, declaration path and test name.", input_schema: obj(&["query"], json!({ "query": {"type":"string"}, "kinds": {"type":"array","items":{"type":"string","enum":["page","req","ticket","module","criterion","file","decl","test","recipe"]}}, "limit": {"type":"integer","default":20} })) },
        ToolDef { name: "get", description: "One node in full with its edges: `out` is what it links to, `in` is what links to it, the bottled tickets that touched or relied on it counted per relation under `bottled_tickets`. Give `ref` for one node, or `refs` for several in one call, answered as `nodes` in the order given with the references not held under `missing`. A requirement's decisions, why it says what it says, are read here.", input_schema: obj(&[], json!({ "ref": reference, "refs": {"type":"array","items": reference, "description":"Several references, read in one call; give this or ref"} })) },
        ToolDef { name: "outline", description: "The grimoire as a tree from the root pages, with coverage, lint-finding counts (and how many are baselined) and the adoption rollup per node, plus the modules no page links. pages_only keeps only pages in the trees, their counts and rollups unchanged: the whole grimoire's shape in a small answer.", input_schema: obj(&[], json!({ "depth": {"type":"integer","default":2}, "root": reference, "pages_only": {"type":"boolean","default":false} })) },
        ToolDef { name: "rests_on", description: "What a node depends on, transitively downward, nearest first, each with its path: a requirement by its title and how many scenarios and decisions in force it has, read with get; a bottled ticket with its decision; what the start names directly with what it says. The planning and review question. For a ticket, starts from relies_on and touches.", input_schema: obj(&["ref"], json!({ "ref": reference, "depth": {"type":"integer"} })) },
        ToolDef { name: "rests_on_me", description: "What binds a node and what would be affected if it changed. Ask it before editing a module, file or criterion: `binds` gives the requirements on its own criteria with their statements and scenarios, what the edit must keep; `nodes` the rest of what stands above it by name; `counted` the tests, criteria, files and declarations reached; `tickets` those not yet bottled that reach it.", input_schema: obj(&["ref"], json!({ "ref": reference })) },
        ToolDef { name: "coverage", description: "What requirements cover a criterion, each criterion of a module with its requirements, or each criterion a requirement names with its module.", input_schema: obj(&["ref"], json!({ "ref": reference })) },
        ToolDef { name: "board", description: "What can be worked on: inscribed, brewing and simmering change tickets ordered next, high, later, each with the gaps and questions it waits on (ready when none is still inscribed or brewing); the gaps and questions not yet bottled that no change relies on; and, when asked, the most recently bottled tickets, each answered gap or question with the changes that followed it (none: nothing was built on the answer). Filter by status, kind, priority or a node they touch.", input_schema: obj(&[], json!({ "status": {"type":"array","items":{"type":"string","enum":["inscribed","brewing","simmering","bottled"]}}, "kinds": {"type":"array","items":{"type":"string","enum":["change","gap","question"]}}, "priority": {"type":"array","items":{"type":"string","enum":["next","high","later"]}}, "touching": reference, "bottled": {"type":"integer","default":0,"description":"How many of the most recently bottled tickets to return"} })) },
        ToolDef { name: "related_tickets", description: "The gaps and questions not yet bottled that touch what this ticket touches.", input_schema: obj(&["ref"], json!({ "ref": reference })) },
        ToolDef { name: "lint", description: "The dangling, uncovered, malformed and unspecified checks over the whole graph. Same output as `grimoire check`.", input_schema: obj(&[], json!({ "kinds": {"type":"array","items":{"type":"string","enum":["dangling","uncovered","malformed","unspecified"]}} })) },
        ToolDef { name: "module_source", description: "Where a module, criterion, file or declaration lives: file and line, plus the contract line for criteria.", input_schema: obj(&["ref"], json!({ "ref": reference })) },
        ToolDef { name: "put_page", description: "Create or replace a page. Links may point to page, req, module and recipe references.", input_schema: obj(&["slug","title","body","links"], json!({ "slug": {"type":"string"}, "title": {"type":"string"}, "body": {"type":"string","description":"Markdown"}, "links": refs })) },
        ToolDef { name: "patch_page", description: "Change a page's title or body, or add and remove links, leaving everything else as it was. The usual way to link a new requirement or page from its area. The body is given whole, or edited in part: replace swaps one passage, which must be in the body exactly once, and append adds a paragraph at its end, so a small edit needs no get and no resending of the rest.", input_schema: obj(&["ref"], json!({ "ref": reference, "title": {"type":"string"}, "body": {"type":"string","description":"Markdown, replacing the whole body"}, "append": append, "replace": replace, "add_links": refs, "remove_links": refs })) },
        ToolDef { name: "put_requirement", description: "Create or replace a requirement. Existing decisions are preserved; use record_decision to add one.", input_schema: obj(&["slug","title","statement","scenarios","covered_by"], json!({ "slug": {"type":"string"}, "title": {"type":"string"}, "statement": {"type":"string","description":"One line with SHALL or MUST naming what the requirement is about: the heading its scenarios are grouped under, each rule it makes being a scenario"}, "scenarios": {"type":"array","items": scenario}, "covered_by": {"type":"array","items": reference, "description":"criterion references, or recipe and ingredient references: recipe:<id>, recipe:<id>#<ingredient>"} })) },
        ToolDef { name: "record_decision", description: "Append a dated decision to a requirement: what was chosen and why. A decision that replaces earlier ones names them in supersedes, by id (d1, d2, ..., or 1, 2, ...); they stay, no longer in force. Under a ticket it waits in the proposal with no id, and takes the next id when the ticket is bottled, after whatever decisions the truth holds then; a decision is referenced as req:<slug>#d<n>.", input_schema: strict(&["ref","text"], json!({ "ref": reference, "text": {"type":"string"}, "date": date, "supersedes": {"type":"array","items":decision_id,"description":"Ids of earlier decisions of the requirement this one replaces, bottled ones only"}, "adopted": {"type":"boolean","description":"Recorded at adoption: the requirement was read off the code and describes it, until someone decides it"} })) },
        ToolDef { name: "supersede", description: "Link a bottled decision of a requirement to the earlier decisions it replaced, by id (d1, d2, ..., or 1, 2, ...), without changing any text, date or id. For a supersession already written in prose; a new decision names what it supersedes in record_decision. Under a ticket the link waits in the proposal until it is bottled.", input_schema: strict(&["ref","decision","supersedes"], json!({ "ref": reference, "decision": decision_id, "supersedes": {"type":"array","items":decision_id} })) },
        ToolDef { name: "retire_node", description: "Retire a requirement that no longer holds: records a last decision saying why, superseding all the others (under a ticket, every one the truth holds when it is bottled), marks it retired and removes it from every page. It stays readable by reference with its decisions but covers no criterion, so the criteria it named come back as warnings unless another requirement names them. A requirement with decisions is retired, never deleted.", input_schema: strict(&["ref","text"], json!({ "ref": reference, "text": {"type":"string","description":"Why it no longer holds"}, "date": date })) },
        ToolDef { name: "inscribe_ticket", description: "Create an inscribed ticket: a change, gap or question, with the nodes it touches and relies on. The id is generated; the title is the ticket's name.", input_schema: obj(&["title","kind","priority","body"], json!({ "title": {"type":"string"}, "kind": {"type":"string","enum":["change","gap","question"]}, "priority": {"type":"string","enum":["next","high","later"]}, "touches": refs, "relies_on": refs, "body": {"type":"string","description":"Markdown, at most 4000 characters"} })) },
        ToolDef { name: "update_ticket", description: "Change a ticket that is not bottled: its title, priority, touches, relies_on or body. The body is given whole, or edited in part: replace swaps one passage, which must be in the body exactly once, and append adds a paragraph at its end. Title, priority and body change at once, and so do a ticket's own touches and relies_on. Touches and relies_on given for another ticket under a change being worked, the one `ticket` names or else the current ticket, wait in that change's proposal and reach the ticket when the change is bottled; under a gap or question, or no ticket, they apply at once.", input_schema: obj(&["ref"], json!({ "ref": reference, "title": {"type":"string"}, "priority": {"type":"string","enum":["next","high","later"]}, "touches": refs, "relies_on": refs, "body": {"type":"string","description":"Markdown, replacing the whole body"}, "append": append, "replace": replace })) },
        ToolDef { name: "brew_ticket", description: "Move a ticket from inscribed to brewing and return what it rests on and the related gaps and questions: under rests_on a map of what binds the work, what it names directly with what each says, every requirement it reaches by its title and how many scenarios and decisions it has (read with get), and each bottled ticket relied on with its decision; under reaches the rest of the walk by reference alone (the rests_on tool gives it whole); under constraints what holds whichever part changes, with what it says; with the recipes above what it touches. A change becomes the current ticket, which on a project that names a server is this checkout's own: there a change already brewing or simmering is taken up in this checkout, and nothing of it changes. A gap or question is claimed instead: brewing, never the current ticket, and a brewing one is claimed again. The caller is listed in `checked_out`, who has it and since when; `others` names everyone else who has it, with a warning, and the brew goes ahead. `waiting` names each ticket this checkout closed `when: bottled` that has not been bottled: a branch holds one ticket until it is bottled, so bottle that one first or brew this on a new branch; the brew goes ahead either way.", input_schema: obj(&["ref"], json!({ "ref": reference })) },
        ToolDef { name: "close_ticket", description: "Close a ticket with its decision: its proposal is applied to the truth, or the close is refused as a conflict naming the records whose truth moved since they were proposed. Bottled at once, its body is discarded. A gap or question closes with no start; `others` names whoever else had the ticket checked out, and the close goes ahead. Returns the new lint findings on the nodes it touched, how many baselined ones sit there, and once it closes, as unnamed, what its own work changed that touches never named: its proposal's pages and requirements and, for a change, the modules its code changed since its branch's base. With when: bottled, for a change whose code has still to reach the main line, the decision is written and the ticket becomes simmering instead, its proposal kept until bottle_ticket bottles it. A gap or question whose answer needs work waits, simmering, for a change relying on it: file that change first, or give it here as `change`, which files it relying on the gap or question, writes the decision naming it (\"Applied by <id>.\") and answers it in `filed`, all as one write, refused whole; run again after a call stopped part way, an inscribed change of that title already relying on it is taken as the one filed, as it is.", input_schema: obj(&["ref","decision"], json!({ "ref": reference, "decision": decision, "when": {"type":"string","enum":["now","bottled"],"description":"now (the default) bottles it at once; bottled writes the decision and leaves it simmering until it is bottled, as `change` implies"}, "change": {"type":"object","additionalProperties":false,"required":["title","body","touches","priority"],"description":"Closing a gap or question only: the change that applies its answer, filed in the same write, relying on the gap or question and on any relies_on given","properties":{"title":{"type":"string"},"body":{"type":"string","description":"Markdown, at most 4000 characters: what the person said, scope and what to avoid"},"touches":refs,"priority":{"type":"string","enum":["next","high","later"]},"relies_on":refs}} })) },
        ToolDef { name: "withdraw_ticket", description: "Withdraw a change that is no longer to be made: close it with a decision saying why, applying nothing of its proposal, which is dropped with its start. Its decision is marked withdrawn, so the board and its page show it apart from a change that was bottled. The gaps and questions it was waiting to apply close only when another change applied them, and otherwise reopen saying why. A bottled ticket is refused as a conflict, and a gap or question, which closes with close_ticket, as invalid. Returns the bottled change and the answers it settled and reopened.", input_schema: obj(&["ref","decision"], json!({ "ref": reference, "decision": decision })) },
        ToolDef { name: "bottle_ticket", description: "Tell Grimoire a change reached the main line: bottle the simmering ticket with the decision it carries, applying its proposal to the truth, or refuse as a conflict naming the records whose truth moved. With no ref, the current ticket is bottled when it is simmering and otherwise none is; on a project that names a server nothing is bottled without a ref, since a ticket there is bottled on the main line, and `note` says so. Returns whether one was bottled, what its close returns, and the tickets still simmering.", input_schema: obj(&[], json!({ "ref": reference })) },
        ToolDef { name: "delete_node", description: "Remove a page, a requirement with no decision, or a ticket that is not bottled. Returns the dangling edges the deletion created. A requirement with decisions is retired with retire_node instead, and a bottled ticket is history and stays.", input_schema: obj(&["ref"], json!({ "ref": reference })) },
        ToolDef { name: "adopt", description: "Record every present finding as the baseline: inherited work the check does not fail on. Only when the person asks; it needs their reason.", input_schema: obj(&["reason"], json!({ "reason": {"type":"string","description":"Why the present findings are accepted as inherited"}, "findings": {"type":"array","description":"The findings to record, each as lint gives it: rule, check, ref and its edge when it has one. Leave it out: the daemon records what it sees. A machine's daemon supplies them to a server that holds no code.","items":{"type":"object","required":["rule","check","ref"],"properties":{"rule":{"type":"string"},"check":{"type":"string"},"ref":reference,"edge":{"type":"object"}}}} })) },
        ToolDef { name: "configure", description: "Change the store config's roots, languages, contract scope, site, line limits or recipe kinds; only the fields given. Answers with the config and how many findings are now new. The repository is set by init only.", input_schema: obj(&[], json!({ "roots": {"description":"The two roots' page slugs: {grimoire?, rites?}, changing only the roots given, or a list as older stores held, the first the grimoire and the second the rites","oneOf":[{"type":"object","properties":{"grimoire":{"type":"string"},"rites":{"type":"string"}},"additionalProperties":false},{"type":"array","items":{"type":"string"}}]}, "recipe_kinds": {"type":"object","additionalProperties":{"type":"object","properties":{"uses":{"type":"array","items":{"type":"string"}}},"additionalProperties":false},"description":"The project's own kinds of recipe, each a slug naming the declared kinds its recipes may use, replaced whole"}, "site": {"type":"array","items":{"type":"string"},"description":"The page slugs the project's site shows, each with every page beneath it, when the server is started with --site"}, "languages": {"type":"array","items":{"type":"string"}}, "scope": {"type":"object","properties":{"include":{"type":"array","items":{"type":"string"}},"exclude":{"type":"array","items":{"type":"string"}}},"additionalProperties":false,"description":"Repository-relative path prefixes expected to carry contracts; an empty include means everything"}, "large_file": {"type":"integer","minimum":1,"description":"A file of this many lines or more, its tests aside, is an omen: a candidate for splitting along its seams; 1000 when unset"}, "plain_file": {"type":"integer","minimum":1,"description":"A file of more lines than this is shown in the UI without highlighting; 5000 when unset"} })) },
        ToolDef { name: "judge_recipe", description: "Record a judgement of a recipe against what it represents: that it still says truly how to use it (held), or that it was changed to say so (updated). The judgement is kept with the fingerprint of the recipe and its files now, the date and the ticket it was made in, and applies at once; a recipe no judgement holds of as it is now is due, a finding. Judge each recipe due over what a ticket touched before closing it. Give `ref` and `outcome` for one, or `judgements` for several in one call, every one checked before any is recorded; either way it answers `judged`, a list.", input_schema: strict(&[], json!({ "ref": { "type": "string", "description": "The recipe judged, as recipe:<id>; a recipe is judged whole, never an ingredient" }, "outcome": outcome.clone(), "fingerprint": fingerprint.clone(), "judgements": {"type":"array","minItems":1,"description":"Several recipes judged in one call; give this or ref and outcome","items":{"type":"object","required":["ref","outcome"],"additionalProperties":false,"properties":{"ref":{"type":"string","description":"The recipe judged, as recipe:<id>"},"outcome":outcome,"fingerprint":fingerprint}}} })) },
        ToolDef { name: "rename_node", description: "Rename a page or requirement, keeping its decisions and rewriting the references the store holds to it. Bottled tickets keep the old reference. Answers with the node and the test blocks in the code that still name the old reference, which the agent edits.", input_schema: obj(&["ref","to"], json!({ "ref": reference, "to": {"type":"string","description":"The new slug"} })) },
        ToolDef { name: "reorder", description: "Set the order of one priority band on the board: the tickets listed come first, in that order; the rest follow by dependency, then title.", input_schema: obj(&["priority","order"], json!({ "priority": {"type":"string","enum":["next","high","later"]}, "order": {"type":"array","items": reference} })) },
        ToolDef { name: "source", description: "A node's file as an annotated view: the text of the file holding a module, criterion, file, declaration or test, the node's line, and the lines the graph attaches nodes to (the contract, test blocks, declarations). Reads only files the graph knows.", input_schema: obj(&["ref"], json!({ "ref": reference })) },
        ToolDef { name: "status", description: "Counts of nodes and edges, the repository path, how many imports did not resolve, and the running build's version and supported languages next to the store's languages.", input_schema: obj(&[], json!({})) },
    ];
    for d in defs.iter_mut().filter(|d| !super::is_write(d.name) || super::takes_ticket(d.name)) {
        d.input_schema["properties"]["ticket"] = ticket.clone();
    }
    // A judgement names the ticket it was made in; it goes into no proposal.
    if let Some(d) = defs.iter_mut().find(|d| d.name == "judge_recipe") {
        d.input_schema["properties"]["ticket"] = json!({ "type": "string", "description": "The ticket the judgement is made in, a slug or ticket: reference; absent, the current ticket, and refused when there is none. The judgement applies at once, in no proposal." });
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
        // configure offers every field it takes.
        let configure = defs.iter().find(|d| d.name == "configure").expect("configure is listed");
        for field in ["roots", "languages", "scope", "site", "large_file", "plain_file", "recipe_kinds"] {
            assert!(configure.input_schema["properties"].get(field).is_some(), "configure offers {field}");
        }
    }

    /// # Test contract
    ///
    /// ```json
    /// {"test": "ticket-is-offered-where-it-is-taken", "checks": ["crates/core/src/tools/write#ticket-taken-where", "crates/core/src/tools/read#ticket-view"]}
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
        assert!(call(&mut s, "inscribe_ticket", named(json!({ "title": "T", "kind": "gap", "priority": "later", "body": "b" })), "2026-10-05").is_ok());
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
    /// {"test": "the-schemas-know-a-simmering-ticket", "checks": ["crates/core/src/tools/definitions#definitions-match"]}
    /// ```
    #[test]
    fn the_schemas_know_a_simmering_ticket() {
        let defs = definitions();
        let schema = |name: &str| defs.iter().find(|d| d.name == name).unwrap().input_schema.clone();
        // board can be asked for simmering tickets, the status the dispatcher accepts and lists by default.
        assert_eq!(schema("board")["properties"]["status"]["items"]["enum"], serde_json::json!(["inscribed", "brewing", "simmering", "bottled"]));
        assert_eq!(schema("close_ticket")["properties"]["when"]["enum"], serde_json::json!(["now", "bottled"]));
        // bottle_ticket needs no ticket named, and applies at once: it offers no `ticket` to scope it.
        assert_eq!(schema("bottle_ticket")["required"], serde_json::json!([]));
        assert!(schema("bottle_ticket")["properties"].get("ticket").is_none());
    }

    /// # Test contract
    ///
    /// ```json
    /// {"test": "the-schemas-offer-every-form-a-tool-takes", "checks": ["crates/core/src/tools/definitions#definitions-match", "req:tool-get", "req:tool-judge-recipe"]}
    /// ```
    #[test]
    fn the_schemas_offer_every_form_a_tool_takes() {
        let defs = definitions();
        let schema = |name: &str| defs.iter().find(|d| d.name == name).unwrap().input_schema.clone();
        // Several at once is a form the tool takes, so the schema offers it and requires neither form's field.
        let get = schema("get");
        assert_eq!((get["required"].clone(), get["properties"]["refs"]["type"].clone()), (json!([]), json!("array")));
        let judge = schema("judge_recipe");
        assert_eq!(judge["required"], json!([]));
        assert_eq!(judge["properties"]["judgements"]["items"]["required"], json!(["ref", "outcome"]));
        assert!(judge["properties"]["judgements"]["items"]["properties"].get("fingerprint").is_some());
        // The batch forms the schema offers, the dispatcher takes. The two forms stay alternatives the dispatcher
        // enforces, refusing both or neither as invalid: a tool schema may not branch at its top level for every client.
        let mut s = state();
        assert!(call(&mut s, "get", json!({ "refs": ["req:stale"] }), "2026-10-08").is_ok());
        let e = call(&mut s, "judge_recipe", json!({ "judgements": [{ "ref": "recipe:none", "outcome": "held" }], "ticket": "later-1" }), "2026-10-08").unwrap_err();
        assert_eq!(e.code(), "not_found", "taken, and refused only for the recipe it names: {e}");
    }
}
