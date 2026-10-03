//! # Module contract
//!
//! ```json
//! {"purpose":"Answer the write tools: each applies one operation to the store and answers with the node as it now exists, the findings the write created and what changed for the caller to persist.","criteria":[{"id":"write-responses","statement":"Every write answers with the node as it now exists, but for land_ticket when no ticket was waiting, which answers with none; put_page, patch_page, put_requirement, file_ticket and update_ticket add as warnings the new findings the write created on that node (its dangling edges, and unspecified for a requirement with no scenario) and a count of the baselined ones, start_ticket adds rests_on and related, close_ticket adds the new findings on the nodes it touched, a count of the baselined ones and the touched nodes that never changed, or with when: landed the findings as its proposal leaves them and no such list, land_ticket answers whether a ticket landed, what its close answers when one did, and the tickets still landing, record_decision and supersede answer with the requirement alone, retire_node adds as warnings the new findings on the criteria the retired requirement named and on the tests that still hold it, and a count of the baselined ones, delete_node returns the deleted reference and the new dangling edges left behind with the same count, adopt returns the new baseline's date, reason and count with the count it replaced, and configure returns the config with the count of findings now new."},{"id":"untouched","statement":"start_ticket records the fingerprint of every node as the ticket's start; a close, by close_ticket or by land_ticket, answers with the touched nodes whose fingerprint is unchanged, or null when the ticket has no start, and removes the start."},{"id":"ticket-named","statement":"A write that changes intent (put_page, patch_page, put_requirement, record_decision, supersede, retire_node, and delete_node of a page or requirement) lands in the proposal of the ticket its input names, else the store's current ticket, and is refused as invalid with neither; only these writes take ticket, any other write refusing it as a field it does not know, and delete_node of a ticket refuses it; it answers with the node as the proposal holds it, and close_ticket refuses with a conflict naming the moved records when the truth moved under the proposal, else applies it before closing."},{"id":"lands","statement":"close_ticket with when: landed refuses with a conflict naming the moved records when the truth moved under the proposal, and otherwise writes the decision and leaves the ticket landing with nothing applied; land_ticket closes a landing ticket as close_ticket does, with the decision it carries and that decision's date, and when the truth moved refuses with the same conflict, the ticket still landing."},{"id":"land-answers","statement":"land_ticket lands the ticket its input names, a conflict when that one is not landing, or with none the store's current ticket when it is landing and otherwise nothing, which is not an error and changes nothing; it answers whether a ticket landed, what its close answers, and the tickets still landing, and takes no ticket field."}]}
//! ```

use serde::Deserialize;
use serde_json::Value;

use super::read::baseline_summary;
use super::{answer, parse, Outcome, RefIn, State};
use crate::answer::*;
use crate::lint::{self, Rule};
use crate::model::*;
use crate::ops::{self, Changed, ToolError};
use crate::store::FieldError;
use crate::query;

/// Run the write tool `name`. `today` is the ISO date used when a decision has none.
/// A write that changes intent lands in the proposal of the `ticket` its input
/// names, else the store's current ticket, and is refused with neither; the
/// truth changes only when a ticket closes.
pub(super) fn call(state: &mut State, name: &str, mut input: Value, today: &str) -> Result<Outcome, ToolError> {
    // Only a write that changes intent takes `ticket`; any other write refuses it with the fields it does not know.
    let ticket = if super::proposes(name) { super::take_ticket(&mut input)? } else { None };
    match name {
        "put_page" => {
            let page: Page = parse(input)?;
            let t = ops::target_ticket(state.store(), ticket.as_deref())?;
            let (node, changed) = ops::propose(state.store_mut(), &t, ops::Proposing::Whole, |s| ops::put_page(s, page))?;
            let r = Reference::page(&node.slug);
            written(name, state, Some(&t), &r, changed)
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
            let t = ops::target_ticket(state.store(), ticket.as_deref())?;
            let (_, changed) = ops::propose(state.store_mut(), &t, ops::Proposing::Partial, |s| ops::patch_page(s, &i.r#ref, patch))?;
            written(name, state, Some(&t), &i.r#ref, changed)
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
            let req = Requirement { slug: i.slug, title: i.title, statement: i.statement, scenarios: i.scenarios, covered_by: i.covered_by, decisions: vec![], retired: false };
            let t = ops::target_ticket(state.store(), ticket.as_deref())?;
            let (node, changed) = ops::propose(state.store_mut(), &t, ops::Proposing::Whole, |s| ops::put_requirement(s, req))?;
            let r = Reference::req(&node.slug);
            written(name, state, Some(&t), &r, changed)
        }
        "record_decision" => {
            #[derive(Deserialize)]
            #[serde(deny_unknown_fields)]
            struct In {
                r#ref: Reference,
                text: String,
                date: Option<String>,
                #[serde(default)]
                supersedes: Vec<u32>,
            }
            let i: In = parse(input)?;
            let t = ops::target_ticket(state.store(), ticket.as_deref())?;
            let date = i.date.unwrap_or_else(|| today.to_string());
            let (node, changed) = ops::propose(state.store_mut(), &t, ops::Proposing::Partial, |s| ops::record_decision(s, &i.r#ref, i.text, date, i.supersedes))?;
            let r = Reference::req(&node.slug);
            let node = query::view(state.graph_for(Some(&t))?, &r).expect("just written");
            answer(name, NodeAnswer { node, proposal: None, drafts: None }, Some(changed))
        }
        "supersede" => {
            #[derive(Deserialize)]
            #[serde(deny_unknown_fields)]
            struct In {
                r#ref: Reference,
                decision: u32,
                supersedes: Vec<u32>,
            }
            let i: In = parse(input)?;
            let t = ops::target_ticket(state.store(), ticket.as_deref())?;
            let (_, changed) = ops::propose(state.store_mut(), &t, ops::Proposing::Partial, |s| ops::supersede(s, &i.r#ref, i.decision, &i.supersedes))?;
            let node = query::view(state.graph_for(Some(&t))?, &i.r#ref).expect("just written");
            answer(name, NodeAnswer { node, proposal: None, drafts: None }, Some(changed))
        }
        "retire_node" => {
            #[derive(Deserialize)]
            #[serde(deny_unknown_fields)]
            struct In {
                r#ref: Reference,
                text: String,
                date: Option<String>,
            }
            let i: In = parse(input)?;
            let t = ops::target_ticket(state.store(), ticket.as_deref())?;
            // What retiring leaves behind: the criteria it named, and the tests that hold it.
            let mut named: Vec<Reference> = state.store().applied(&t).requirements.get(&i.r#ref.id).map(|q| q.covered_by.clone()).unwrap_or_default();
            named.extend(state.graph_for(Some(&t))?.inbound_with(&i.r#ref, crate::model::Relation::Checks).map(|e| e.from.clone()));
            let date = i.date.unwrap_or_else(|| today.to_string());
            let (_, changed) = ops::propose(state.store_mut(), &t, ops::Proposing::Partial, |s| ops::retire_node(s, &i.r#ref, i.text, date))?;
            let g = state.graph_for(Some(&t))?;
            let node = query::view(g, &i.r#ref).expect("just written");
            let (warnings, baselined) = split_new(lint::lint_touching(g, &named));
            answer(name, Written { node, warnings, baselined }, Some(changed))
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
            written(name, state, None, &r, changed)
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
            written(name, state, None, &i.r#ref, changed)
        }
        "start_ticket" => {
            let i: RefIn = parse(input)?;
            ops::start_ticket(state.store_mut(), &i.r#ref)?;
            // Every node, not only what touches lists now: nodes are often
            // added to touches after they changed.
            let fingerprints = crate::fingerprint::all(state.graph()).into_iter().map(|(r, fp)| (r.to_string(), fp)).collect();
            state.store_mut().starts.insert(i.r#ref.id.clone(), Start { ticket: i.r#ref.id.clone(), fingerprints });
            // The started ticket is now current, so writes that name none land in it.
            let changed = Changed::Many(vec![Changed::Started(i.r#ref.id.clone()), Changed::Current]);
            let g = state.graph();
            let node = query::view(g, &i.r#ref).expect("just written");
            let rests_on = query::rests_on(g, &i.r#ref, None);
            let related = match g.get(&i.r#ref) {
                Some(crate::graph::Node::Ticket(t)) => query::related_tickets(g, t),
                _ => vec![],
            };
            answer(name, Started { node, rests_on, related }, Some(changed))
        }
        "close_ticket" => {
            #[derive(Deserialize)]
            #[serde(deny_unknown_fields)]
            struct DecisionIn {
                text: String,
                date: Option<String>,
            }
            #[derive(Deserialize, Default, PartialEq)]
            #[serde(rename_all = "lowercase")]
            enum When {
                #[default]
                Now,
                Landed,
            }
            #[derive(Deserialize)]
            #[serde(deny_unknown_fields)]
            struct In {
                r#ref: Reference,
                decision: DecisionIn,
                #[serde(default)]
                when: When,
            }
            let i: In = parse(input)?;
            let touched = open_ticket_touches(state, &i.r#ref)?;
            // Nothing below may fail once the proposal is applied, so the decision is checked first.
            let decision = Decision::new(i.decision.date.unwrap_or_else(|| today.to_string()), i.decision.text);
            if !crate::store::is_date(&decision.date) || decision.text.trim().is_empty() {
                return Err(ToolError::invalid("a decision is a dated paragraph: text, and an ISO date naming a day", vec![]));
            }
            if i.when == When::Landed {
                // The decision is written and the ticket waits: nothing reaches the truth until its change lands.
                let moved = ops::moved(state.store(), &i.r#ref.id);
                if !moved.is_empty() {
                    return Err(ToolError::conflict_with(format!("{} cannot wait to land: the truth moved under its proposal; read what moved again and propose it again", i.r#ref), moved));
                }
                let (_, changed) = ops::decide_ticket(state.store_mut(), &i.r#ref, decision)?;
                // What the check before the landing will see: the truth with this ticket's proposal on it.
                let g = state.graph_for(Some(&i.r#ref.id))?;
                let node = query::view(g, &i.r#ref).expect("just written");
                let (findings, baselined) = split_new(lint::lint_touching(g, &touched));
                return answer(name, Closed { node, findings, baselined, untouched: None }, Some(changed));
            }
            let (closed, changed) = close_now(state, &i.r#ref, decision, touched)?;
            answer(name, closed, Some(changed))
        }
        "land_ticket" => {
            #[derive(Deserialize)]
            #[serde(deny_unknown_fields)]
            struct In {
                r#ref: Option<Reference>,
            }
            let i: In = parse(input)?;
            // Named, the ticket must be landing; unnamed, the current ticket lands when it is, and otherwise nothing does.
            let target = match i.r#ref {
                Some(r) => match state.store.tickets.get(&r.id) {
                    Some(t) if r.kind == Kind::Ticket && t.status == Status::Landing => Some(r),
                    Some(t) if r.kind == Kind::Ticket => return Err(ToolError::conflict(format!("{r} is {}, not landing: its decision is written with close_ticket and when: landed", t.status.name()))),
                    _ => return Err(ToolError::not_found(&r)),
                },
                None => state.store.current.as_ref().filter(|c| state.store.tickets.get(*c).is_some_and(|t| t.status == Status::Landing)).map(|c| Reference::ticket(c)),
            };
            let landed = match target {
                None => None,
                Some(r) => {
                    let touched = open_ticket_touches(state, &r)?;
                    let decision = state.store.tickets[&r.id].decision.clone().expect("a landing ticket carries its decision");
                    Some(close_now(state, &r, decision, touched)?)
                }
            };
            let waiting: Vec<query::Summary> = state.store.tickets.values().filter(|t| t.status == Status::Landing).map(query::ticket_summary).collect();
            match landed {
                None => answer(name, Landed { landed: false, node: None, findings: vec![], baselined: 0, untouched: None, waiting }, None),
                Some((c, changed)) => answer(name, Landed { landed: true, node: Some(c.node), findings: c.findings, baselined: c.baselined, untouched: c.untouched, waiting }, Some(changed)),
            }
        }
        "rename_node" => {
            #[derive(Deserialize)]
            #[serde(deny_unknown_fields)]
            struct In {
                r#ref: Reference,
                to: String,
            }
            let i: In = parse(input)?;
            // What the code names is read before the rename; the tool never edits it.
            let g = state.graph();
            let still_named_in: Vec<StillNamed> = g
                .inbound_with(&i.r#ref, crate::model::Relation::Checks)
                .filter_map(|e| match g.get(&e.from) {
                    Some(crate::graph::Node::Test(t)) => Some(StillNamed { test: e.from.clone(), file: t.file.clone(), line: t.line }),
                    _ => None,
                })
                .collect();
            let (new, changed) = ops::rename_node(state.store_mut(), &i.r#ref, &i.to)?;
            let node = query::view(state.graph(), &new).expect("just renamed");
            answer(name, Renamed { node, still_named_in }, Some(changed))
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
            let order = slugs.iter().map(|s| Reference::ticket(s)).collect();
            answer(name, Reordered { priority: i.priority, order }, Some(changed))
        }
        "configure" => {
            let change: ops::ConfigChange = parse(input)?;
            let supported = state.build.supported_languages.clone();
            let (config, changed) = ops::configure(state.store_mut(), change, &supported)?;
            let new = lint::new_count(&lint::lint(state.graph(), &Rule::ALL));
            answer(name, Configured { config, new }, Some(changed))
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
            answer(name, Adopted { baseline: baseline_summary(&baseline), replaced }, Some(changed))
        }
        "delete_node" => {
            let i: RefIn = parse(input)?;
            // A ticket is not intent and goes at once; a page or requirement is a proposed deletion.
            let view = match i.r#ref.kind {
                Kind::Ticket if ticket.is_some() => return Err(ToolError::invalid("`ticket` does not apply: a ticket is deleted at once, with its start and proposal", vec![FieldError::new("ticket", "not taken when deleting a ticket")])),
                Kind::Ticket => None,
                _ => Some(ops::target_ticket(state.store(), ticket.as_deref())?),
            };
            // Edges that pointed at the node become dangling: collect them first.
            let pointing: Vec<Reference> = state.graph_for(view.as_deref())?.inbound(&i.r#ref).map(|e| e.from.clone()).collect();
            let changed = match &view {
                None => ops::delete_node(state.store_mut(), &i.r#ref)?,
                Some(t) => ops::propose(state.store_mut(), t, ops::Proposing::Partial, |s| ops::delete_node(s, &i.r#ref).map(|c| ((), c)))?.1,
            };
            let g = state.graph_for(view.as_deref())?;
            let mut warnings = Vec::new();
            for from in pointing {
                warnings.extend(lint::dangling_from(g, &from).into_iter().filter(|f| f.edge.as_ref().is_some_and(|e| e.to == i.r#ref)));
            }
            let (warnings, baselined) = split_new(warnings);
            answer(name, Deleted { deleted: i.r#ref, warnings, baselined }, Some(changed))
        }
        other => Err(ToolError::invalid(format!("unknown tool `{other}`"), vec![])),
    }
}

/// Answer a write with the node as it now is in `view`: the proposal a
/// change of intent landed in, or the truth for a ticket's own record.
fn written(name: &str, state: &mut State, view: Option<&str>, r: &Reference, changed: Changed) -> Result<Outcome, ToolError> {
    let g = state.graph_for(view)?;
    let node = query::view(g, r).expect("just written");
    let (warnings, baselined) = split_new(lint::written(g, r));
    answer(name, Written { node, warnings, baselined }, Some(changed))
}

/// The new findings, which are the caller's to act on, and how many
/// baselined ones were left out: inherited work, listed by `lint`.
/// The nodes a ticket that is not closed touches; a closed one is a conflict.
fn open_ticket_touches(state: &State, r: &Reference) -> Result<Vec<Reference>, ToolError> {
    match state.store.tickets.get(&r.id) {
        Some(t) if r.kind == Kind::Ticket && t.status != Status::Closed => Ok(t.touches.clone()),
        Some(_) if r.kind == Kind::Ticket => Err(ToolError::conflict(format!("{r} is already closed"))),
        _ => Err(ToolError::not_found(r)),
    }
}

/// Close a ticket at once: its proposal applied to the truth it was written
/// against, or the close refused naming what moved; then what the close
/// answers with and what changed. Shared by close_ticket and land_ticket.
fn close_now(state: &mut State, r: &Reference, decision: Decision, touched: Vec<Reference>) -> Result<(Closed, Changed), ToolError> {
    // The proposal applies only onto the truth it was written against.
    let moved = ops::moved(state.store(), &r.id);
    if !moved.is_empty() {
        return Err(ToolError::conflict_with(format!("{r} cannot close: the truth moved under its proposal; read what moved again and propose it again"), moved));
    }
    let applied = ops::apply_proposal(state.store_mut(), &r.id);
    // Compared before the close, which changes the ticket's own record,
    // and after the proposal landed, so a proposed change counts as one.
    let untouched = state.store.starts.get(&r.id).cloned().map(|start| crate::fingerprint::untouched(&touched, &start.fingerprints, &crate::fingerprint::all(state.graph())));
    ops::close_ticket(state.store_mut(), r, decision)?;
    state.store_mut().starts.remove(&r.id);
    let changed = Changed::Many(vec![applied, Changed::Closed(r.id.clone()), Changed::Current]);
    let g = state.graph();
    let node = query::view(g, r).expect("just written");
    let (findings, baselined) = split_new(lint::lint_touching(g, &touched));
    Ok((Closed { node, findings, baselined, untouched }, changed))
}

fn split_new(findings: Vec<lint::Finding>) -> (Vec<lint::Finding>, usize) {
    let total = findings.len();
    let new: Vec<lint::Finding> = findings.into_iter().filter(|f| !f.baselined).collect();
    let baselined = total - new.len();
    (new, baselined)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    use crate::extraction::Extraction;
    use crate::tools::call;
    use crate::tools::fixtures::{built, state};

    /// # Test contract
    ///
    /// ```json
    /// {"test": "write-then-read-sees-it", "checks": ["crates/core/src/ops#never-fails-on-lint", "crates/core/src/query/lookup#node-view", "crates/core/src/query/lookup#get", "crates/core/src/tools/state#graph-current"]}
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
        let req = |scenarios: Value| json!({ "slug": "new-rule", "title": "New", "statement": "It SHALL hold.", "scenarios": scenarios, "covered_by": [], "ticket": "later-1" });
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
    /// {"test": "adopt-baselines-what-is-present", "checks": ["crates/core/src/ops#adopt", "crates/core/src/tools/read#status", "crates/core/src/tools/read#lint-tool", "crates/core/src/model#baseline-record"]}
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
    /// {"test": "a-fix-prunes-and-a-regression-is-new", "checks": ["crates/core/src/tools/read#lint-tool"]}
    /// ```
    #[test]
    fn a_fix_prunes_and_a_regression_is_new() {
        let mut s = state();
        call(&mut s, "adopt", json!({ "reason": "adopted" }), "2026-09-28").unwrap();
        assert!(s.prune_baseline().is_none(), "nothing fixed yet");
        // Fix the one unspecified requirement, under the active ticket, and land it by closing.
        let scenario = json!([{ "title": "T", "given": "g", "when": "w", "then": "t" }]);
        call(&mut s, "put_requirement", json!({ "slug": "stale", "title": "Stale", "statement": "It SHALL.", "scenarios": scenario, "covered_by": ["criterion:src/a/ready#gone"], "ticket": "later-1" }), "2026-09-28").unwrap();
        assert!(s.prune_baseline().is_none(), "a proposal fixes nothing in the truth yet");
        call(&mut s, "close_ticket", json!({ "ref": "ticket:later-1", "decision": { "text": "Fixed." } }), "2026-09-28").unwrap();
        assert!(matches!(s.prune_baseline(), Some(Changed::Baseline)));
        // Break it again on a started ticket: lint with no ticket judges the current one, and it is new.
        call(&mut s, "start_ticket", json!({ "ref": "ticket:phase-62" }), "2026-09-28").unwrap();
        call(&mut s, "put_requirement", json!({ "slug": "stale", "title": "Stale", "statement": "It SHALL.", "scenarios": [], "covered_by": ["criterion:src/a/ready#gone"] }), "2026-09-28").unwrap();
        let lint = call(&mut s, "lint", json!({ "kinds": ["unspecified"] }), "2026-09-28").unwrap().output;
        assert_eq!(lint["new"], 1);
        assert_eq!(lint["findings"][0]["baselined"], false);
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
    /// {"test": "widening-the-scope-makes-new-findings", "checks": ["crates/core/src/tools/read#status"]}
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

    /// # Test contract
    ///
    /// ```json
    /// {"test": "inherited-findings-are-counted", "checks": ["crates/core/src/tools/write#write-responses"]}
    /// ```
    #[test]
    fn inherited_findings_are_counted_not_handed_over() {
        let mut s = state();
        call(&mut s, "adopt", json!({ "reason": "adopted" }), "2026-09-28").unwrap();
        let out = call(&mut s, "close_ticket", json!({ "ref": "ticket:phase-62", "decision": { "text": "Done." } }), "2026-09-28").unwrap().output;
        assert_eq!(out["findings"], json!([]), "everything on the touched nodes is inherited");
        assert!(out["baselined"].as_u64().unwrap() > 0);
        let w = call(&mut s, "put_requirement", json!({ "slug": "stale", "title": "Stale", "statement": "It SHALL.", "scenarios": [], "covered_by": ["criterion:src/a/ready#gone"], "ticket": "later-1" }), "2026-09-28").unwrap().output;
        assert_eq!(w["warnings"], json!([]), "stale was already dangling and unspecified when adopted");
        assert_eq!(w["baselined"], 2);
        let fresh = call(&mut s, "put_requirement", json!({ "slug": "fresh", "title": "Fresh", "statement": "It SHALL.", "scenarios": [], "covered_by": [], "ticket": "later-1" }), "2026-09-28").unwrap().output;
        assert_eq!(fresh["warnings"][0]["check"], "no-scenario", "new work is handed over");
        assert_eq!(fresh["baselined"], 0);
    }

    /// # Test contract
    ///
    /// ```json
    /// {"test": "record-decision-defaults-to-today", "checks": ["crates/core/src/ops/decisions#record-decision"]}
    /// ```
    #[test]
    fn record_decision_defaults_to_today() {
        let mut s = state();
        let out = call(&mut s, "record_decision", json!({ "ref": "req:ready-only-with-commit", "text": "Why.", "ticket": "later-1" }), "2026-09-28").unwrap().output;
        let last = out["node"]["decisions"].as_array().unwrap().last().unwrap().clone();
        assert_eq!(last, json!({ "date": "2026-09-28", "text": "Why." }));
        // Proposed with its requirement: the truth has no new decision until the ticket closes.
        assert_eq!(s.store().requirements["ready-only-with-commit"].decisions.len(), 1);
        call(&mut s, "close_ticket", json!({ "ref": "ticket:later-1", "decision": { "text": "Done." } }), "2026-09-29").unwrap();
        let landed = s.store().requirements["ready-only-with-commit"].decisions.last().unwrap();
        assert_eq!((landed.date.as_str(), landed.text.as_str()), ("2026-09-28", "Why."), "it keeps the date it was recorded");
    }

    /// # Test contract
    ///
    /// ```json
    /// {"test": "a-close-names-what-never-changed", "checks": ["crates/core/src/tools/write#untouched"]}
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
        // The close applies the proposal, then closes the ticket, then clears the current ticket.
        let Some(Changed::Many(steps)) = out.changed else { panic!("a close is several changes") };
        assert!(matches!(steps.first(), Some(Changed::Many(applied)) if applied.iter().any(|c| matches!(c, Changed::Page(p) if p == "orphan")) && applied.iter().any(|c| matches!(c, Changed::Requirement(q) if q == "ready-only-with-commit"))), "{steps:?}");
        assert!(matches!(&steps[1], Changed::Closed(slug) if slug == "phase-62"));
        assert!(matches!(steps.last(), Some(Changed::Current)));
        assert!(s.store().current.is_none(), "closing the current ticket clears it");
        assert_eq!(s.store().pages["orphan"].body, "Changed before being listed.", "the proposal landed in the truth");
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

    /// # Test contract
    ///
    /// ```json
    /// {"test": "a-rename-keeps-decisions-and-rewrites-references", "checks": ["crates/core/src/ops#rename"]}
    /// ```
    #[test]
    fn a_rename_keeps_decisions_and_rewrites_references() {
        let mut s = state();
        // ready-only-with-commit: one dated decision, linked from git-readiness,
        // touched by the open ticket phase-62; and a closed ticket names it too.
        call(&mut s, "file_ticket", json!({ "title": "Old", "kind": "change", "priority": "later", "touches": ["req:ready-only-with-commit"], "body": "b" }), "2026-09-28").unwrap();
        call(&mut s, "start_ticket", json!({ "ref": "ticket:0001" }), "2026-09-28").unwrap();
        call(&mut s, "close_ticket", json!({ "ref": "ticket:0001", "decision": { "text": "Done." } }), "2026-09-28").unwrap();
        call(&mut s, "start_ticket", json!({ "ref": "ticket:phase-62" }), "2026-09-28").unwrap();
        call(&mut s, "adopt", json!({ "reason": "adopted" }), "2026-09-28").unwrap();
        let before = s.store().requirements["ready-only-with-commit"].clone();
        let out = call(&mut s, "rename_node", json!({ "ref": "req:ready-only-with-commit", "to": "ready-with-a-commit" }), "2026-09-28").unwrap();
        assert_eq!(out.output["node"]["ref"], "req:ready-with-a-commit");
        let after = &s.store().requirements["ready-with-a-commit"];
        assert_eq!(after.decisions, before.decisions, "decisions keep their dates");
        assert!(!s.store().requirements.contains_key("ready-only-with-commit"));
        assert!(s.store().pages["git-readiness"].links.contains(&Reference::req("ready-with-a-commit")));
        assert!(s.store().tickets["phase-62"].touches.contains(&Reference::req("ready-with-a-commit")), "an open ticket follows");
        assert!(s.store().tickets["0001"].touches.contains(&Reference::req("ready-only-with-commit")), "a closed ticket is history");
        assert!(s.store().starts["phase-62"].fingerprints.contains_key("req:ready-with-a-commit"), "the start snapshot follows");
        let lint = call(&mut s, "lint", json!({}), "2026-09-28").unwrap().output;
        assert_eq!(lint["new"], 0, "inherited findings stay inherited");
        // The changes persist new record first and old record last.
        let Some(Changed::Many(cs)) = out.changed else { panic!("a rename is several changes") };
        assert!(matches!(cs.first(), Some(Changed::Requirement(x)) if x == "ready-with-a-commit"));
        assert!(matches!(cs.last(), Some(Changed::Deleted(r)) if r == &Reference::req("ready-only-with-commit")));
    }

    #[test]
    fn rename_refuses_what_it_may_not_do() {
        let mut s = state();
        let code = |r: Result<Outcome, ToolError>| serde_json::to_value(r.unwrap_err()).unwrap()["error"].clone();
        assert_eq!(code(call(&mut s, "rename_node", json!({ "ref": "ticket:phase-62", "to": "x" }), "2026-09-28")), "invalid");
        assert_eq!(code(call(&mut s, "rename_node", json!({ "ref": "module:src/a/ready", "to": "x" }), "2026-09-28")), "invalid");
        assert_eq!(code(call(&mut s, "rename_node", json!({ "ref": "page:nope", "to": "x" }), "2026-09-28")), "not_found");
        assert_eq!(code(call(&mut s, "rename_node", json!({ "ref": "page:orphan", "to": "overview" }), "2026-09-28")), "conflict");
        assert_eq!(code(call(&mut s, "rename_node", json!({ "ref": "page:orphan", "to": "Not A Slug" }), "2026-09-28")), "invalid");
        // A copy of the record under the new slug, as a failed rename leaves, is no conflict; a copy that differs in anything is.
        let orphan = s.store().pages["orphan"].clone();
        // The copy is in the truth, as a failed rename leaves it: written to the store directly, not proposed.
        let copy = |body: &str| Page { slug: "kept".into(), title: orphan.title.clone(), body: body.into(), links: orphan.links.clone() };
        ops::put_page(s.store_mut(), copy("changed since")).unwrap();
        assert_eq!(code(call(&mut s, "rename_node", json!({ "ref": "page:orphan", "to": "kept" }), "2026-09-28")), "conflict");
        ops::put_page(s.store_mut(), copy(&orphan.body)).unwrap();
        call(&mut s, "rename_node", json!({ "ref": "page:orphan", "to": "kept" }), "2026-09-28").unwrap();
        assert!(!s.store().pages.contains_key("orphan") && s.store().pages["kept"].body == orphan.body);
        // A root page's rename moves the root.
        call(&mut s, "rename_node", json!({ "ref": "page:overview", "to": "home" }), "2026-09-28").unwrap();
        assert_eq!(s.store().config.roots, vec!["home".to_string()]);
    }

    #[test]
    fn a_rename_reports_the_tests_that_name_it() {
        let mut s = state();
        let mut files = crate::extraction::fixtures::extraction_files();
        files[0].tests.push(Test { file: "src/a/ready.rs".into(), name: "holds".into(), line: 50, checks: vec!["req:stale".into()] });
        s.set_extraction(Extraction::from_files(&files));
        let out = call(&mut s, "rename_node", json!({ "ref": "req:stale", "to": "fresh" }), "2026-09-28").unwrap();
        assert_eq!(out.output["still_named_in"], json!([{ "test": "test:src/a/ready.rs#holds", "file": "src/a/ready.rs", "line": 50 }]));
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
        let out = call(&mut s, "delete_node", json!({ "ref": "page:git-readiness", "ticket": "later-1" }), "2026-09-27").unwrap();
        let w = out.output["warnings"].as_array().unwrap();
        assert!(w.iter().any(|f| f["ref"] == "page:overview"));
        assert!(w.iter().any(|f| f["ref"] == "ticket:phase-62"));
    }

    /// # Test contract
    ///
    /// ```json
    /// {"test": "retire-answers-with-what-it-leaves-uncovered", "checks": ["crates/core/src/tools/write#write-responses"]}
    /// ```
    #[test]
    fn retire_answers_with_what_it_leaves_uncovered() {
        let mut s = state();
        // A constraint held by a test block: retiring it leaves that test holding nothing.
        let mut files = crate::extraction::fixtures::extraction_files();
        files[0].tests.push(Test { file: "src/a/ready.rs".into(), name: "holds".into(), line: 50, checks: vec!["req:stale".into()] });
        s.set_extraction(Extraction::from_files(&files));
        let out = call(&mut s, "retire_node", json!({ "ref": "req:stale", "text": "Gone.", "ticket": "later-1" }), "2026-09-29").unwrap().output;
        assert!(out["warnings"].as_array().expect("warnings").iter().any(|w| w["check"] == "checks-retired" && w["ref"] == "test:src/a/ready.rs#holds"), "{out}");
        let e = call(&mut s, "delete_node", json!({ "ref": "req:ready-only-with-commit", "ticket": "later-1" }), "2026-09-29").unwrap_err();
        assert!(e.to_string().contains("retire_node"), "{e}");
        let out = call(&mut s, "retire_node", json!({ "ref": "req:ready-only-with-commit", "text": "No longer holds.", "ticket": "later-1" }), "2026-09-29").unwrap().output;
        assert_eq!(out["node"]["retired"], true);
        assert_eq!(out["node"]["decisions"].as_array().map(Vec::len), Some(2));
        assert_eq!(out["node"]["decisions"][1]["supersedes"], json!([1]));
        assert!(out["warnings"].as_array().expect("warnings").iter().any(|w| w["ref"] == "criterion:src/a/ready#guarantee" && w["check"] == "uncovered-criterion"), "{out}");
        // Still readable by reference, in the proposal; the truth has not moved.
        assert_eq!(call(&mut s, "get", json!({ "ref": "req:ready-only-with-commit", "ticket": "later-1" }), "2026-09-29").unwrap().output["node"]["retired"], true);
        assert_ne!(call(&mut s, "get", json!({ "ref": "req:ready-only-with-commit" }), "2026-09-29").unwrap().output["node"]["retired"], true);
        let linked = call(&mut s, "supersede", json!({ "ref": "req:ready-only-with-commit", "decision": 2, "supersedes": [1], "ticket": "later-1" }), "2026-09-29").unwrap_err();
        // A ticket's decision takes no supersedes, and says so rather than dropping it.
        let closing = call(&mut s, "close_ticket", json!({ "ref": "ticket:phase-62", "decision": { "text": "Done.", "supersedes": [1] } }), "2026-09-29").unwrap_err();
        assert!(matches!(closing, ToolError::Invalid { .. }), "{closing}");
        // A misspelt field is refused rather than silently dropping the supersession.
        let typo = call(&mut s, "record_decision", json!({ "ref": "req:stale", "text": "x", "supercedes": [1], "ticket": "later-1" }), "2026-09-29").unwrap_err();
        assert!(matches!(typo, ToolError::Invalid { .. }), "{typo}");
        assert!(matches!(linked, ToolError::Invalid { .. }), "a retired requirement takes nothing more");
    }

    /// # Test contract
    ///
    /// ```json
    /// {"test": "a-write-lands-in-its-ticket-and-the-close-applies-it", "checks": ["crates/core/src/tools/write#ticket-named"]}
    /// ```
    #[test]
    fn a_write_lands_in_its_ticket_and_the_close_applies_it() {
        let mut s = state();
        let code = |r: Result<Outcome, ToolError>| r.unwrap_err().code();
        let truth = s.store().pages["orphan"].clone();
        let orphan = |body: &str| json!({ "slug": "orphan", "title": "Orphan", "body": body, "links": [], "ticket": "later-1" });
        let out = call(&mut s, "put_page", orphan("proposed"), "2026-10-02").unwrap();
        assert_eq!(out.output["node"]["body"], "proposed", "answered as the proposal holds it");
        assert!(matches!(out.changed, Some(Changed::Proposed(ref t)) if t == "later-1"));
        assert_eq!(s.store().pages["orphan"], truth, "the truth is untouched");
        // Without a ticket and with none current: refused, naming the field.
        let mut bare = orphan("x");
        bare.as_object_mut().unwrap().remove("ticket");
        assert_eq!(code(call(&mut s, "put_page", bare.clone(), "2026-10-02")), "invalid");
        // With a current ticket, it lands there.
        call(&mut s, "start_ticket", json!({ "ref": "ticket:phase-62" }), "2026-10-02").unwrap();
        call(&mut s, "put_page", bare, "2026-10-02").unwrap();
        assert_eq!(s.store().proposals["phase-62"].pages["orphan"].record.body, "x");
        // No other write takes the field, nor does deleting a ticket.
        assert_eq!(code(call(&mut s, "update_ticket", json!({ "ref": "ticket:phase-62", "title": "T", "ticket": "later-1" }), "2026-10-02")), "invalid");
        assert_eq!(code(call(&mut s, "rename_node", json!({ "ref": "page:orphan", "to": "o", "ticket": "later-1" }), "2026-10-02")), "invalid");
        assert_eq!(code(call(&mut s, "delete_node", json!({ "ref": "ticket:gap-1", "ticket": "later-1" }), "2026-10-02")), "invalid");
        assert_eq!(code(call(&mut s, "close_ticket", json!({ "ref": "ticket:phase-62", "decision": { "text": "Done." }, "ticket": "later-1" }), "2026-10-02")), "invalid");
        assert!(s.store().tickets.contains_key("gap-1") && s.store().tickets["phase-62"].status == Status::Active);
        // The truth moves under later-1's proposal: its close is a conflict naming the record.
        ops::put_page(s.store_mut(), Page { body: "moved".into(), ..truth.clone() }).unwrap();
        let e = call(&mut s, "close_ticket", json!({ "ref": "ticket:later-1", "decision": { "text": "Done." } }), "2026-10-02").unwrap_err();
        let v = serde_json::to_value(&e).unwrap();
        assert_eq!((v["error"].clone(), v["details"][0]["field"].clone()), (json!("conflict"), json!("page:orphan")));
        assert_eq!(s.store().tickets["later-1"].status, Status::Active, "not closed");
        // Proposed again over the truth as it now is, the close applies it.
        call(&mut s, "put_page", orphan("proposed"), "2026-10-02").unwrap();
        call(&mut s, "close_ticket", json!({ "ref": "ticket:later-1", "decision": { "text": "Done." } }), "2026-10-02").unwrap();
        assert_eq!(s.store().pages["orphan"].body, "proposed");
        assert!(!s.store().proposals.contains_key("later-1"));
        assert_eq!(s.store().current.as_deref(), Some("phase-62"), "another ticket's close leaves the current one");
        // Deleting the current ticket clears it and drops its proposal and start.
        let out = call(&mut s, "delete_node", json!({ "ref": "ticket:phase-62" }), "2026-10-02").unwrap();
        assert!(matches!(out.changed, Some(Changed::Many(ref cs)) if matches!(cs.last(), Some(Changed::Current))), "{:?}", out.changed);
        assert!(s.store().current.is_none() && !s.store().proposals.contains_key("phase-62") && !s.store().starts.contains_key("phase-62"));
    }

    /// # Test contract
    ///
    /// ```json
    /// {"test": "a-change-waits-to-land-and-the-landing-closes-it", "checks": ["crates/core/src/tools/write#lands", "crates/core/src/tools/write#land-answers", "crates/core/src/ops#closes-when-landed", "crates/core/src/tools/read#ticket-view"]}
    /// ```
    #[test]
    fn a_change_waits_to_land_and_the_landing_closes_it() {
        let mut s = state();
        let code = |r: Result<Outcome, ToolError>| r.unwrap_err().code();
        let truth = s.store().pages["orphan"].clone();
        let orphan = |body: &str| json!({ "slug": "orphan", "title": "Orphan", "body": body, "links": [], "ticket": "later-1" });
        let landed = |text: &str| json!({ "ref": "ticket:later-1", "decision": { "text": text }, "when": "landed" });
        call(&mut s, "put_page", orphan("proposed"), "2026-10-03").unwrap();
        // Nothing to land yet: later-1 is active, and no ticket is current. Not an error, and nothing changes.
        let none = call(&mut s, "land_ticket", json!({}), "2026-10-03").unwrap();
        assert_eq!((none.output["landed"].clone(), none.output["node"].clone()), (json!(false), json!(null)));
        assert!(none.changed.is_none());
        assert_eq!(code(call(&mut s, "land_ticket", json!({ "ref": "ticket:later-1" }), "2026-10-03")), "conflict", "active, not landing");
        // The decision is written and the ticket waits: its body, proposal and start kept, the truth untouched.
        let out = call(&mut s, "close_ticket", landed("Done."), "2026-10-03").unwrap();
        assert_eq!((out.output["node"]["status"].clone(), out.output["untouched"].clone()), (json!("landing"), json!(null)));
        let t = s.store().tickets["later-1"].clone();
        assert_eq!((t.status, t.decision.as_ref().map(|d| d.text.as_str()), t.body.is_empty()), (Status::Landing, Some("Done."), false));
        assert!(s.store().proposals.contains_key("later-1"));
        assert_eq!(s.store().pages["orphan"], truth);
        // It is still written and read with its proposal, and its decision can be written again.
        call(&mut s, "put_page", orphan("proposed again"), "2026-10-03").unwrap();
        assert_eq!(call(&mut s, "get", json!({ "ref": "page:orphan", "ticket": "later-1" }), "2026-10-03").unwrap().output["node"]["body"], "proposed again");
        call(&mut s, "close_ticket", landed("Done, said better."), "2026-10-04").unwrap();
        // A gap has nothing to land, and an open ticket is not being worked.
        assert_eq!(code(call(&mut s, "close_ticket", json!({ "ref": "ticket:gap-1", "decision": { "text": "A." }, "when": "landed" }), "2026-10-04")), "invalid");
        assert_eq!(code(call(&mut s, "close_ticket", json!({ "ref": "ticket:phase-62", "decision": { "text": "A." }, "when": "landed" }), "2026-10-04")), "conflict");
        // The truth moves under it: the landing is refused naming the record, the ticket is still landing, and its decision cannot be written again either.
        ops::put_page(s.store_mut(), Page { body: "moved".into(), ..truth.clone() }).unwrap();
        let e = call(&mut s, "land_ticket", json!({ "ref": "ticket:later-1" }), "2026-10-05").unwrap_err();
        let v = serde_json::to_value(&e).unwrap();
        assert_eq!((v["error"].clone(), v["details"][0]["field"].clone()), (json!("conflict"), json!("page:orphan")));
        assert_eq!(s.store().tickets["later-1"].status, Status::Landing);
        assert_eq!(s.store().pages["orphan"].body, "moved", "nothing applied");
        assert_eq!(code(call(&mut s, "close_ticket", landed("Again."), "2026-10-05")), "conflict");
        // Proposed again over the truth as it now is. Unnamed, only the current ticket lands, so it is listed as waiting.
        call(&mut s, "put_page", orphan("proposed"), "2026-10-05").unwrap();
        let waiting = call(&mut s, "land_ticket", json!({}), "2026-10-05").unwrap();
        assert_eq!((waiting.output["landed"].clone(), waiting.output["waiting"][0]["ref"].clone()), (json!(false), json!("ticket:later-1")));
        // Named, it lands: closed with the decision written before and that decision's date.
        let out = call(&mut s, "land_ticket", json!({ "ref": "ticket:later-1" }), "2026-10-06").unwrap();
        assert_eq!((out.output["landed"].clone(), out.output["node"]["status"].clone(), out.output["waiting"].clone()), (json!(true), json!("closed"), json!([])));
        let t = s.store().tickets["later-1"].clone();
        assert_eq!(t.decision.map(|d| (d.date, d.text)), Some(("2026-10-04".to_string(), "Done, said better.".to_string())));
        assert!(t.body.is_empty() && !s.store().proposals.contains_key("later-1") && !s.store().starts.contains_key("later-1"));
        assert_eq!(s.store().pages["orphan"].body, "proposed");
        // The current ticket stays current while landing, lands unnamed, and stops being current.
        call(&mut s, "start_ticket", json!({ "ref": "ticket:phase-62" }), "2026-10-06").unwrap();
        call(&mut s, "close_ticket", json!({ "ref": "ticket:phase-62", "decision": { "text": "Done." }, "when": "landed" }), "2026-10-06").unwrap();
        assert_eq!(s.store().current.as_deref(), Some("phase-62"));
        let out = call(&mut s, "land_ticket", json!({}), "2026-10-06").unwrap();
        assert_eq!(out.output["landed"], true);
        assert_eq!(out.output["untouched"], json!(["req:ready-only-with-commit", "module:src/a/ready"]), "compared with its start when it lands: what it named and never changed");
        assert!(s.store().current.is_none() && s.store().tickets["phase-62"].status == Status::Closed);
        // land_ticket applies at once and takes no ticket.
        assert_eq!(code(call(&mut s, "land_ticket", json!({ "ticket": "later-1" }), "2026-10-06")), "invalid");
    }
}
