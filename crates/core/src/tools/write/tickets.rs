//! # Module contract
//!
//! ```json
//! {"purpose":"Answer the tools of a ticket's life, start_ticket, close_ticket, land_ticket and withdraw_ticket, with what each records of the ticket, applies to the truth and settles of the answers it relies on.","criteria":[{"id":"untouched","statement":"start_ticket records the fingerprint of every node as the ticket's start, but on a state that holds no code, which records none; a close, by close_ticket or by land_ticket, answers with the touched nodes whose fingerprint is unchanged and, as unnamed, the intent_of what changed that it never touched, both null when the ticket has no start or the state holds no code, and removes the start."},{"id":"lands","statement":"close_ticket with when: landed refuses with a conflict naming the moved records when the truth moved under the proposal, and otherwise writes the decision and leaves the ticket landing with nothing applied; land_ticket closes a landing ticket as close_ticket does, with the decision it carries and that decision's date, and when the truth moved refuses with the same conflict, the ticket still landing."},{"id":"withdraws","statement":"withdraw_ticket closes the change it names with ops::withdraw_ticket, refusing a decision with no text or no ISO date as invalid before anything changes, then settles the answers the change relied on as gone, saying it was withdrawn, and answers with the change as closed and the answers it settled and reopened; it takes no ticket field and applies nothing, whether or not the truth moved under its proposal."},{"id":"land-answers","statement":"land_ticket lands the ticket its input names, a conflict when that one is not landing or is a gap or question waiting for its change, or with none the store's current ticket when it is landing and otherwise nothing, which is not an error and changes nothing; it answers whether a ticket landed, what its close answers, and the changes still landing, and takes no ticket field."},{"id":"takes-up","statement":"On a machine's copy of a server's intent, or on a store of its own the state is told several checkouts share, start_ticket on a ticket already being worked, active or a change waiting to land, makes it this checkout's current ticket and records its start, changing nothing of the ticket, a store of its own keeping the start it has, since there a ticket's start is the store's, not one checkout's, and land_ticket naming no ticket lands nothing and says a ticket lands on the main line, by name; on any other state start_ticket refuses a ticket that is not open, a gap or question waiting for its change among them."},{"id":"start-warns","statement":"start_ticket answers as waiting each change ops::waiting_here gives for this checkout other than the ticket started, with its title and a warning naming it and saying a branch holds one ticket until it lands, to land it first or start this ticket on a new branch; the start goes ahead whatever waits."}]}
//! ```

use serde::Deserialize;
use serde_json::Value;

use super::{applies, split_new, summaries};
use crate::answer::*;
use crate::lint;
use crate::model::*;
use crate::ops::{self, Changed, ToolError};
use crate::query;
use crate::tools::{answer, parse, Outcome, RefIn, State};

/// start_ticket: the ticket made current with its start recorded, or taken up where a ticket's start is each checkout's,
/// answered with what it rests on, the related tickets, the recipes over what it touches and what still waits here.
pub(super) fn start(state: &mut State, name: &str, input: Value) -> Result<Outcome, ToolError> {
    let i: RefIn = parse(input)?;
    // On a machine's copy of a server's intent, a ticket already being worked is taken up
    // in this checkout: it becomes current here and gets this checkout's start, and nothing of it changes.
    // So it is on a store of its own several checkouts share, each with its own current ticket.
    let taken_up = (state.server().is_some() || state.shares_checkouts()) && i.r#ref.kind == Kind::Ticket && state.store.tickets.get(&i.r#ref.id).is_some_and(Ticket::is_worked);
    // There the start is the store's, one per ticket: the one it has is kept.
    let keeps_start = taken_up && state.server().is_none() && state.store.starts.contains_key(&i.r#ref.id);
    if taken_up {
        state.store_mut().current = Some(i.r#ref.id.clone());
    } else {
        ops::start_ticket(state.store_mut(), &i.r#ref)?;
    }
    let changed = if keeps_start {
        Changed::Current
    } else if state.without_code() {
        // A state serving a team keeps no current ticket and no start: both are each checkout's own.
        state.store_mut().current = None;
        Changed::Many(vec![Changed::Ticket(i.r#ref.id.clone()), Changed::Current])
    } else {
        // Every node, not only what touches lists now: nodes are often
        // added to touches after they changed.
        let fingerprints = crate::fingerprint::all(state.graph()).into_iter().map(|(r, fp)| (r.to_string(), fp)).collect();
        state.store_mut().starts.insert(i.r#ref.id.clone(), Start { ticket: i.r#ref.id.clone(), fingerprints, closed_in: None });
        // The started ticket is now current, so writes that name none land in it.
        Changed::Many(vec![Changed::Started(i.r#ref.id.clone()), Changed::Current])
    };
    // As the ticket's own proposal leaves the graph: one taken up already carries its spec.
    let g = state.graph_for(Some(&i.r#ref.id))?;
    let node = query::view(g, &i.r#ref).expect("just written");
    let rests_on = query::rests_on(g, &i.r#ref, None);
    let (related, recipes) = match g.get(&i.r#ref) {
        Some(crate::graph::Node::Ticket(t)) => (query::related_tickets(g, t), query::recipes_resting(g, &t.touches)),
        _ => (vec![], vec![]),
    };
    // A branch holds one ticket until it lands: a change this checkout still carries is named, and the start goes ahead.
    let waiting = ops::waiting_here(state.store(), Some(&i.r#ref.id))
        .into_iter()
        .map(|t| WaitingHere {
            ticket: t.slug.clone(),
            title: t.title.clone(),
            warning: format!(
                "ticket:{} \"{}\" is still waiting to land from this checkout: a branch holds one ticket until it lands, so land it first, or start {} on a new branch",
                t.slug, t.title, i.r#ref
            ),
        })
        .collect();
    answer(name, Started { node, rests_on, related, recipes, waiting }, Some(changed))
}

/// close_ticket: closed at once, its proposal applied, or with when: landed its decision written to wait for the landing.
pub(super) fn close(state: &mut State, name: &str, input: Value, today: &str) -> Result<Outcome, ToolError> {
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
        // An answer holds no proposal, so decide_ticket refuses one that does before any comparison.
        let change = state.store.tickets.get(&i.r#ref.id).is_some_and(|t| t.kind == TicketKind::Change);
        let moved = if change { ops::moved(state.store(), &i.r#ref.id) } else { vec![] };
        if !moved.is_empty() {
            return Err(ToolError::conflict_with(format!("{} cannot wait to land: the truth moved under its proposal; read what moved again and propose it again", i.r#ref), moved));
        }
        let (_, changed) = ops::decide_ticket(state.store_mut(), &i.r#ref, decision)?;
        // What the check before the landing will see: the truth with this ticket's proposal on it.
        let (code, start) = (!state.without_code(), state.store.starts.get(&i.r#ref.id).cloned());
        let g = state.graph_for(Some(&i.r#ref.id))?;
        let node = query::view(g, &i.r#ref).expect("just written");
        let (findings, baselined) = split_new(lint::lint_touching(g, &touched));
        let due = code.then(|| due_over(g, &touched, start.as_ref()));
        return answer(name, Closed { node, findings, baselined, untouched: None, unnamed: None, settled: vec![], due }, Some(changed));
    }
    let (closed, changed) = close_now(state, &i.r#ref, decision, touched)?;
    answer(name, closed, Some(changed))
}

/// withdraw_ticket: the change closed with nothing applied, and the answers it relied on settled as gone.
pub(super) fn withdraw(state: &mut State, name: &str, input: Value, today: &str) -> Result<Outcome, ToolError> {
    #[derive(Deserialize)]
    #[serde(deny_unknown_fields)]
    struct DecisionIn {
        text: String,
        date: Option<String>,
    }
    #[derive(Deserialize)]
    #[serde(deny_unknown_fields)]
    struct In {
        r#ref: Reference,
        decision: DecisionIn,
    }
    let i: In = parse(input)?;
    let decision = Decision::new(i.decision.date.unwrap_or_else(|| today.to_string()), i.decision.text);
    if !crate::store::is_date(&decision.date) || decision.text.trim().is_empty() {
        return Err(ToolError::invalid("a decision is a dated paragraph: text, and an ISO date naming a day", vec![]));
    }
    // Nothing of it is applied, so nothing is compared with the truth: a change the truth moved under is withdrawn as any other.
    let answers = applies(state, &i.r#ref);
    let (_, mut changed) = ops::withdraw_ticket(state.store_mut(), &i.r#ref, decision)?;
    let (mut settled, mut reopened) = (vec![], vec![]);
    if let Some(answers) = answers {
        let why = format!("change {} was withdrawn", i.r#ref.id);
        let (s, more) = ops::settle(state.store_mut(), &answers, ops::Settling::Gone(&why));
        if !s.is_empty() {
            changed = Changed::Many(vec![changed, more]);
        }
        (settled, reopened) = summaries(state, &s);
    }
    let g = state.graph_for(None)?;
    let node = query::view(g, &i.r#ref).expect("just closed");
    answer(name, crate::answer::Withdrawn { node, settled, reopened }, Some(changed))
}

/// land_ticket: the ticket named, or the current one when it is landing, closed as close_ticket closes it.
pub(super) fn land(state: &mut State, name: &str, input: Value) -> Result<Outcome, ToolError> {
    #[derive(Deserialize)]
    #[serde(deny_unknown_fields)]
    struct In {
        r#ref: Option<Reference>,
    }
    let i: In = parse(input)?;
    // Named, the ticket must be landing; unnamed, the current ticket lands when it is, and otherwise nothing does.
    // On a machine's copy of a server's intent an unnamed landing lands nothing: a commit there is not the main line.
    let note = (i.r#ref.is_none() && state.server().is_some()).then(|| "this project names a server: a ticket lands on the main line, by name, once its change has merged".to_string());
    let target = match i.r#ref {
        Some(r) => match state.store.tickets.get(&r.id) {
            Some(t) if r.kind == Kind::Ticket && t.status == Status::Landing && t.kind != TicketKind::Change => {
                return Err(ToolError::conflict(format!("{r} is an answer waiting for the change that applies it: it closes when that change lands, and is not landed itself")))
            }
            Some(t) if r.kind == Kind::Ticket && t.status == Status::Landing => Some(r),
            Some(t) if r.kind == Kind::Ticket => return Err(ToolError::conflict(format!("{r} is {}, not landing: its decision is written with close_ticket and when: landed", t.status.name()))),
            _ => return Err(ToolError::not_found(&r)),
        },
        None if note.is_some() => None,
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
    // An answer waiting for its change is not landed itself, so only changes wait here.
    let waiting: Vec<query::Summary> = state.store.tickets.values().filter(|t| t.status == Status::Landing && t.kind == TicketKind::Change).map(query::ticket_summary).collect();
    match landed {
        None => answer(name, Landed { landed: false, node: None, findings: vec![], baselined: 0, untouched: None, unnamed: None, settled: vec![], due: None, waiting, note }, None),
        Some((c, changed)) => answer(
            name,
            Landed { landed: true, node: Some(c.node), findings: c.findings, baselined: c.baselined, untouched: c.untouched, unnamed: c.unnamed, settled: c.settled, due: c.due, waiting, note: None },
            Some(changed),
        ),
    }
}

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
    // The tickets its edits reach, with what each relied on and its priority before, to settle and keep pace after.
    let edited: Vec<(String, Vec<Reference>, Priority)> = state
        .store
        .proposals
        .get(&r.id)
        .map(|p| p.tickets.keys().filter_map(|s| state.store.tickets.get(s)).filter(|t| t.status != Status::Closed).map(|t| (t.slug.clone(), t.relies_on.clone(), t.priority)).collect())
        .unwrap_or_default();
    let applied = ops::apply_proposal(state.store_mut(), &r.id);
    // Compared before the close, which changes the ticket's own record,
    // and after the proposal landed, so a proposed change counts as one.
    // With no code held the comparison would cover intent alone and pass over every touched module in silence: unknown, so null.
    // Read the other way, what changed that it never touched, with the intent resting on it.
    let compared = if state.without_code() {
        None
    } else {
        state.store.starts.get(&r.id).cloned().map(|start| {
            let now = crate::fingerprint::all(state.graph());
            (crate::fingerprint::untouched(&touched, &start.fingerprints, &now), crate::fingerprint::unnamed(&touched, &start.fingerprints, &now))
        })
    };
    let start = state.store.starts.get(&r.id).cloned();
    let due = (!state.without_code()).then(|| due_over(state.graph(), &touched, start.as_ref()));
    let untouched = compared.as_ref().map(|(u, _)| u.clone());
    let unnamed = compared.map(|(_, n)| query::intent_of(state.graph(), &n));
    let answers = applies(state, r);
    ops::close_ticket(state.store_mut(), r, decision)?;
    state.store_mut().starts.remove(&r.id);
    let mut changes = vec![applied, Changed::Closed(r.id.clone()), Changed::Current];
    // A closed change has applied the answers it relies on: each closes once no change applying it is left open.
    let mut settled = vec![];
    if let Some(answers) = answers {
        let (done, more) = ops::settle(state.store_mut(), &answers, ops::Settling::Closed);
        if !done.is_empty() {
            changes.push(more);
        }
        settled = summaries(state, &done).0;
    }
    // Its edits reached other tickets as update_ticket would have: answers one no longer relies on settle, and it keeps pace.
    for (slug, before, was) in edited {
        let now = state.store.tickets.get(&slug).map(|t| t.relies_on.clone()).unwrap_or_default();
        let dropped: Vec<Reference> = before.into_iter().filter(|a| !now.contains(a)).collect();
        if state.store.tickets.get(&slug).is_some_and(|t| t.kind == TicketKind::Change) && !dropped.is_empty() {
            let why = format!("change {slug} stopped relying on it when {} landed", r.id);
            let (done, more) = ops::settle(state.store_mut(), &dropped, ops::Settling::Gone(&why));
            if !done.is_empty() {
                changes.push(more);
            }
            settled.extend(summaries(state, &done).0);
        }
        let (moved, more) = ops::keep_pace(state.store_mut(), &slug, Some(was));
        if !moved.is_empty() {
            changes.push(more);
        }
    }
    let changed = Changed::Many(changes);
    let g = state.graph();
    let node = query::view(g, r).expect("just written");
    let (findings, baselined) = split_new(lint::lint_touching(g, &touched));
    Ok((Closed { node, findings, baselined, untouched, unnamed, settled, due }, changed))
}

/// The recipes a ticket's work reached that are due: those resting on what it touched, and those
/// whose recipe or files changed since its start, which the start's fingerprints tell (req:recipe-in-tickets).
fn due_over(g: &crate::graph::Graph, touched: &[Reference], start: Option<&Start>) -> Vec<query::RecipeState> {
    let mut nodes = touched.to_vec();
    if let Some(start) = start {
        let now = crate::fingerprint::all(g);
        let changed = now.iter().filter(|(r, fp)| matches!(g.get(r), Some(crate::graph::Node::Recipe(_))) && start.fingerprints.get(&r.to_string()) != Some(*fp));
        nodes.extend(changed.map(|(r, _)| r.clone()));
    }
    query::recipes_resting(g, &nodes).into_iter().filter(|x| x.due).collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    use crate::extraction::Extraction;
    use crate::tools::call;
    use crate::tools::fixtures::state;

    /// # Test contract
    ///
    /// ```json
    /// {"test": "a-withdrawn-change-applies-no-answer", "checks": ["crates/core/src/tools/write/tickets#withdraws", "crates/core/src/ops/tickets#settles-answers"]}
    /// ```
    #[test]
    fn a_withdrawn_change_applies_no_answer() {
        use crate::graph::fixtures::ticket;
        let mut s = state();
        // Question q answered and waiting for change c, the only change relying on it.
        let mut q = ticket("q", TicketKind::Question, Priority::Later, Status::Landing, &[], &[]);
        q.decision = Some(Decision::new("2026-10-01", "Count lines."));
        let c = ticket("c", TicketKind::Change, Priority::Later, Status::Active, &[], &["ticket:q"]);
        s.store_mut().tickets.insert("q".into(), q);
        s.store_mut().tickets.insert("c".into(), c);
        let refused = call(&mut s, "withdraw_ticket", json!({ "ref": "ticket:c", "decision": { "text": " " } }), "2026-10-06");
        assert!(matches!(refused, Err(ToolError::Invalid { .. })), "a decision with no text changes nothing");
        assert_eq!(s.store().tickets["c"].status, Status::Active);
        let out = call(&mut s, "withdraw_ticket", json!({ "ref": "ticket:c", "decision": { "text": "Reconsidered: lines are not counted." } }), "2026-10-06").unwrap().output;
        assert_eq!(out["node"]["status"], "closed");
        assert_eq!(out["node"]["decision"]["withdrawn"], true);
        assert_eq!(out["reopened"][0]["ref"], "ticket:q", "{out}");
        let q = &s.store().tickets["q"];
        assert_eq!(q.status, Status::Open, "a withdrawn change, closed after the answer, never counts as having applied it");
        assert!(q.body.contains("change c was withdrawn"), "{}", q.body);
    }

    /// # Test contract
    ///
    /// ```json
    /// {"test": "a-close-names-what-never-changed", "checks": ["crates/core/src/tools/write/tickets#untouched"]}
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
        assert!(
            matches!(steps.first(), Some(Changed::Many(applied)) if applied.iter().any(|c| matches!(c, Changed::Page(p) if p == "orphan")) && applied.iter().any(|c| matches!(c, Changed::Requirement(q) if q == "ready-only-with-commit"))),
            "{steps:?}"
        );
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
        assert!(out.output["unnamed"].is_null(), "unknown, not none");
    }

    /// # Test contract
    ///
    /// ```json
    /// {"test": "a-close-names-what-changed-unnamed", "checks": ["crates/core/src/fingerprint#unnamed", "crates/core/src/query/walks#intent-of", "crates/core/src/tools/write/tickets#untouched"]}
    /// ```
    #[test]
    fn a_close_names_what_changed_unnamed() {
        let mut s = state();
        call(&mut s, "start_ticket", json!({ "ref": "ticket:phase-62" }), "2026-10-04").unwrap();
        call(&mut s, "update_ticket", json!({ "ref": "ticket:phase-62", "touches": ["page:git-readiness"] }), "2026-10-04").unwrap();
        // The work reaches a module it never named, by its code, and a page, by the proposal.
        let mut files = crate::extraction::fixtures::extraction_files();
        files.iter_mut().find(|f| f.module == "src/a/ready").expect("fixture").digest = "edited".into();
        s.set_extraction(Extraction::from_files(&files));
        call(&mut s, "patch_page", json!({ "ref": "page:orphan", "body": "Reached unnamed." }), "2026-10-04").unwrap();
        // Written landing, it is not known yet.
        let landing = call(&mut s, "close_ticket", json!({ "ref": "ticket:phase-62", "decision": { "text": "Done." }, "when": "landed" }), "2026-10-04").unwrap();
        assert!(landing.output["unnamed"].is_null());
        let out = call(&mut s, "land_ticket", json!({ "ref": "ticket:phase-62" }), "2026-10-04").unwrap();
        let unnamed = out.output["unnamed"].as_array().expect("a list once it lands").clone();
        let refs: Vec<&str> = unnamed.iter().map(|x| x["ref"].as_str().unwrap()).collect();
        assert_eq!(refs, vec!["page:orphan", "module:src/a/ready", "req:ready-only-with-commit"], "the page, the module and the requirement on it, in reference order; nothing named in touches");
        assert_eq!(unnamed[0]["says"], "Reached unnamed.", "the page as the proposal left it");
        assert!(unnamed[1]["says"].as_str().unwrap().starts_with("Decide readiness."), "the module's contract");
        assert_eq!(unnamed[2]["via"], json!(["module:src/a/ready"]));
        assert!(unnamed[2]["says"].is_string() && unnamed[2]["decisions"].is_array(), "the requirement, with what it says and why");
        assert_eq!(out.output["untouched"], json!(["page:git-readiness"]));
    }

    /// # Test contract
    ///
    /// ```json
    /// {"test": "unnamed-counts-each-node-once-and-every-end", "checks": ["crates/core/src/fingerprint#unnamed", "crates/core/src/query/walks#intent-of"]}
    /// ```
    #[test]
    fn unnamed_counts_each_node_once_and_every_end() {
        let mut s = state();
        call(&mut s, "start_ticket", json!({ "ref": "ticket:phase-62" }), "2026-10-04").unwrap();
        call(&mut s, "update_ticket", json!({ "ref": "ticket:phase-62", "touches": ["module:src/a/layout"] }), "2026-10-04").unwrap();
        // One requirement on two modules that both change, a page created, a page removed, a page renamed.
        call(&mut s, "put_requirement", json!({ "slug": "ready-only-with-commit", "title": "Both", "statement": "It SHALL.", "scenarios": [{ "title": "T", "given": "g", "when": "w", "then": "t" }], "covered_by": ["criterion:src/a/ready#guarantee", "criterion:src/a/layout#stable"] }), "2026-10-04").unwrap();
        let mut files = crate::extraction::fixtures::extraction_files();
        for f in files.iter_mut().filter(|f| f.module == "src/a/ready" || f.module == "src/a/layout") {
            f.digest = format!("{} edited", f.digest);
        }
        s.set_extraction(Extraction::from_files(&files));
        call(&mut s, "put_page", json!({ "slug": "fresh", "title": "Fresh", "body": "New.", "links": [] }), "2026-10-04").unwrap();
        call(&mut s, "delete_node", json!({ "ref": "page:orphan" }), "2026-10-04").unwrap();
        // Closed at once, not landed.
        let out = call(&mut s, "close_ticket", json!({ "ref": "ticket:phase-62", "decision": { "text": "Done." } }), "2026-10-04").unwrap();
        let unnamed = out.output["unnamed"].as_array().unwrap().clone();
        let refs: Vec<&str> = unnamed.iter().map(|x| x["ref"].as_str().unwrap()).collect();
        // The layout module is touched: not unnamed, though the requirement it shares with ready is reached through ready, once.
        assert_eq!(refs, vec!["page:fresh", "page:orphan", "req:ready-only-with-commit", "module:src/a/ready"], "{refs:?}");
        let orphan = unnamed.iter().find(|x| x["ref"] == "page:orphan").unwrap();
        assert!(orphan["says"].is_null() && orphan["decisions"] == json!([]), "a removed page carries its reference alone: {orphan}");
        // A requirement given is not listed again through a module.
        assert_eq!(refs.iter().filter(|r| **r == "req:ready-only-with-commit").count(), 1);
        // A rename is named by its new slug.
        let mut s = state();
        call(&mut s, "start_ticket", json!({ "ref": "ticket:phase-62" }), "2026-10-04").unwrap();
        call(&mut s, "update_ticket", json!({ "ref": "ticket:phase-62", "touches": [] }), "2026-10-04").unwrap();
        call(&mut s, "rename_node", json!({ "ref": "page:orphan", "to": "adopted" }), "2026-10-04").unwrap();
        call(&mut s, "patch_page", json!({ "ref": "page:adopted", "body": "Renamed, then changed." }), "2026-10-04").unwrap();
        let out = call(&mut s, "close_ticket", json!({ "ref": "ticket:phase-62", "decision": { "text": "Done." } }), "2026-10-04").unwrap();
        let refs: Vec<String> = out.output["unnamed"].as_array().unwrap().iter().map(|x| x["ref"].as_str().unwrap().to_string()).collect();
        assert_eq!(refs, vec!["page:adopted".to_string()]);
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

    /// # Test contract
    ///
    /// ```json
    /// {"test": "a-start-names-a-ticket-still-waiting-to-land-here", "checks": ["crates/core/src/tools/write/tickets#start-warns", "crates/core/src/ops/tickets#waits-here", "crates/core/src/tools/state#advisories"]}
    /// ```
    #[test]
    fn a_start_names_a_ticket_still_waiting_to_land_here() {
        let mut s = state();
        let day = "2026-10-07";
        // phase-62 is started here, its proposal mends req:stale's dangling criterion, and it is closed to wait for its landing.
        assert_eq!(call(&mut s, "start_ticket", json!({ "ref": "ticket:phase-62" }), day).unwrap().output["waiting"], json!([]));
        let mut stale = s.store().requirements["stale"].clone();
        stale.covered_by = vec!["criterion:src/a/ready#guarantee".parse().unwrap()];
        let mut put = serde_json::to_value(&stale).unwrap();
        put["ticket"] = json!("phase-62");
        put.as_object_mut().unwrap().remove("decisions");
        call(&mut s, "put_requirement", put, day).unwrap();
        let closed = call(&mut s, "close_ticket", json!({ "ref": "ticket:phase-62", "decision": { "text": "Done." }, "when": "landed" }), day).unwrap();
        assert_eq!(closed.changed, Some(Changed::Many(vec![Changed::Ticket("phase-62".into()), Changed::Started("phase-62".into())])), "the start says where it was closed");
        assert_eq!(s.store().starts["phase-62"].closed_in.as_deref(), Some("C:/repo"));
        // While it is current, it is not named as waiting beside its own findings.
        let lint = call(&mut s, "lint", json!({ "kinds": ["dangling"] }), day).unwrap().output;
        assert_eq!(lint["mended"], json!([]));
        // Another ticket started here is told, and starts all the same.
        let started = call(&mut s, "start_ticket", json!({ "ref": "ticket:gap-1" }), day).unwrap().output;
        assert_eq!(started["node"]["status"], "active");
        assert_eq!((started["waiting"][0]["ticket"].clone(), started["waiting"].as_array().map(Vec::len)), (json!("phase-62"), Some(1)));
        let warning = started["waiting"][0]["warning"].as_str().unwrap();
        assert!(warning.contains("ticket:phase-62") && warning.contains("land it first") && warning.contains("new branch"), "{warning}");
        // The finding it would mend still fails here, and is named beside it.
        let lint = call(&mut s, "lint", json!({ "kinds": ["dangling"] }), day).unwrap().output;
        assert_eq!(lint["new"], 1, "{lint}");
        assert_eq!(lint["findings"][0]["ref"], "req:stale");
        assert_eq!((lint["mended"][0]["ticket"].clone(), lint["mended"][0]["finding"]["ref"].clone(), lint["mended"].as_array().map(Vec::len)), (json!("phase-62"), json!("req:stale"), Some(1)));
        // The waiting change's proposal lies beneath the current ticket's: where both propose req:stale, the current one's stands.
        let mut own = serde_json::to_value(&s.store().requirements["stale"]).unwrap();
        own["title"] = json!("Stale, still");
        own["ticket"] = json!("gap-1");
        own.as_object_mut().unwrap().remove("decisions");
        call(&mut s, "put_requirement", own, day).unwrap();
        assert_eq!(call(&mut s, "lint", json!({ "kinds": ["dangling"] }), day).unwrap().output["mended"], json!([]));
        s.store_mut().proposals.remove("gap-1");
        // The same folder spelt otherwise is this checkout; another checkout is told nothing.
        s.store_mut().config.repository = r"c:\REPO\".into();
        assert_eq!(ops::waiting_here(s.store(), None).len(), 1);
        assert!(ops::waiting_here(s.store(), Some("phase-62")).is_empty(), "but the one given");
        s.store_mut().config.repository = "C:/elsewhere".into();
        let (filed, _) =
            ops::file_ticket(s.store_mut(), ops::NewTicket { title: "Next".into(), kind: TicketKind::Change, priority: Priority::Later, touches: vec![], relies_on: vec![], body: "More.".into() })
                .unwrap();
        assert_eq!(call(&mut s, "start_ticket", json!({ "ref": format!("ticket:{}", filed.slug) }), day).unwrap().output["waiting"], json!([]));
        assert_eq!(call(&mut s, "lint", json!({ "kinds": ["dangling"] }), day).unwrap().output["mended"], json!([]));
        // Landed, it is waiting nowhere.
        s.store_mut().config.repository = "C:/repo".into();
        call(&mut s, "land_ticket", json!({ "ref": "ticket:phase-62" }), day).unwrap();
        assert!(ops::waiting_here(s.store(), None).is_empty());
    }

    /// # Test contract
    ///
    /// ```json
    /// {"test": "a-change-waits-to-land-and-the-landing-closes-it", "checks": ["crates/core/src/tools/write/tickets#lands", "crates/core/src/tools/write/tickets#land-answers", "crates/core/src/ops/tickets#closes-when-landed", "crates/core/src/tools/read#ticket-view"]}
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
        // An open ticket, gap or change, is not being worked.
        assert_eq!(code(call(&mut s, "close_ticket", json!({ "ref": "ticket:gap-1", "decision": { "text": "A." }, "when": "landed" }), "2026-10-04")), "conflict");
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

    /// # Test contract
    ///
    /// ```json
    /// {"test": "an-answer-waits-for-the-change-that-applies-it", "checks": ["crates/core/src/ops/tickets#closes-when-landed", "crates/core/src/ops/tickets#settles-answers", "crates/core/src/ops/proposal#proposal-writes", "crates/core/src/ops#delete-node", "crates/core/src/tools/write#settles", "crates/core/src/tools/write/tickets#land-answers", "crates/core/src/tools/write#write-responses"]}
    /// ```
    #[test]
    fn an_answer_waits_for_the_change_that_applies_it() {
        let mut s = state();
        let code = |r: Result<Outcome, ToolError>| r.unwrap_err().code();
        let answer = |r: &str, text: &str| json!({ "ref": r, "decision": { "text": text }, "when": "landed" });
        let refs = |v: &Value| v.as_array().unwrap().iter().map(|x| x["ref"].as_str().unwrap().to_string()).collect::<Vec<_>>();
        call(&mut s, "start_ticket", json!({ "ref": "ticket:gap-1" }), "2026-10-04").unwrap();
        // No change applies it yet: the answer cannot wait for one.
        let e = call(&mut s, "close_ticket", answer("ticket:gap-1", "Fix it."), "2026-10-04").unwrap_err();
        assert!(e.code() == "invalid" && e.to_string().contains("file the change"), "{e}");
        call(&mut s, "update_ticket", json!({ "ref": "ticket:later-1", "relies_on": ["ticket:gap-1"] }), "2026-10-04").unwrap();
        // Its spec belongs in that change.
        call(&mut s, "put_page", json!({ "slug": "note", "title": "Note", "body": "b", "links": [], "ticket": "gap-1" }), "2026-10-04").unwrap();
        assert_eq!(code(call(&mut s, "close_ticket", answer("ticket:gap-1", "Fix it."), "2026-10-04")), "invalid");
        s.store_mut().proposals.remove("gap-1");
        // Answered: it waits, landing, and stops being current; the change relying on it is ready.
        let out = call(&mut s, "close_ticket", answer("ticket:gap-1", "Fix it."), "2026-10-04").unwrap();
        assert_eq!((out.output["node"]["status"].clone(), out.output["settled"].clone()), (json!("landing"), json!([])));
        assert!(s.store().current.is_none() && !s.store().starts.contains_key("gap-1"));
        // What names it as worked is persisted before the record that says it waits, so a writer stopped between leaves an active answer.
        assert_eq!(out.changed, Some(Changed::Many(vec![Changed::Current, Changed::Proposed("gap-1".into()), Changed::Closed("gap-1".into())])));
        let board = call(&mut s, "board", json!({}), "2026-10-04").unwrap();
        let later = board.output["tickets"].as_array().unwrap().iter().find(|t| t["ref"] == "ticket:later-1").unwrap().clone();
        assert_eq!(later["waits_on"][0]["status"], "landing");
        // Nothing closes it by hand, nothing lands as it, and it takes no spec.
        assert_eq!(code(call(&mut s, "land_ticket", json!({ "ref": "ticket:gap-1" }), "2026-10-04")), "conflict");
        assert_eq!(code(call(&mut s, "close_ticket", json!({ "ref": "ticket:gap-1", "decision": { "text": "Done." } }), "2026-10-04")), "conflict");
        assert_eq!(code(call(&mut s, "put_page", json!({ "slug": "note", "title": "Note", "body": "b", "links": [], "ticket": "gap-1" }), "2026-10-04")), "conflict");
        assert_eq!(code(call(&mut s, "delete_node", json!({ "ref": "ticket:gap-1" }), "2026-10-04")), "invalid", "it carries the person's decision");
        assert_eq!(call(&mut s, "land_ticket", json!({}), "2026-10-04").unwrap().output["waiting"], json!([]), "only changes wait to land");
        // Two changes apply it: the first closing leaves it waiting for the second.
        let second = call(&mut s, "file_ticket", json!({ "title": "Second", "kind": "change", "priority": "later", "relies_on": ["ticket:gap-1"], "body": "b" }), "2026-10-04").unwrap();
        let second = second.output["node"]["ref"].as_str().unwrap().to_string();
        let out = call(&mut s, "close_ticket", json!({ "ref": "ticket:later-1", "decision": { "text": "Done." } }), "2026-10-05").unwrap();
        assert_eq!(out.output["settled"], json!([]));
        assert_eq!(s.store().tickets["gap-1"].status, Status::Landing);
        // The second goes away, but the first applied it after its answer: it closes with its own decision.
        let out = call(&mut s, "delete_node", json!({ "ref": second }), "2026-10-05").unwrap();
        assert_eq!((refs(&out.output["settled"]), out.output["reopened"].clone()), (vec!["ticket:gap-1".to_string()], json!([])));
        let gap = s.store().tickets["gap-1"].clone();
        assert_eq!((gap.status, gap.decision.map(|d| d.text), gap.body), (Status::Closed, Some("Fix it.".to_string()), String::new()));
        // An answer whose only change stops relying on it reopens, its answer in its body.
        let q = call(&mut s, "file_ticket", json!({ "title": "Which?", "kind": "question", "priority": "high", "body": "Asked." }), "2026-10-05").unwrap();
        let q = q.output["node"]["ref"].as_str().unwrap().to_string();
        let c = call(&mut s, "file_ticket", json!({ "title": "Apply", "kind": "change", "priority": "high", "relies_on": [q], "body": "b" }), "2026-10-05").unwrap();
        let c = c.output["node"]["ref"].as_str().unwrap().to_string();
        call(&mut s, "start_ticket", json!({ "ref": q }), "2026-10-05").unwrap();
        call(&mut s, "close_ticket", answer(&q, "This one."), "2026-10-05").unwrap();
        let out = call(&mut s, "update_ticket", json!({ "ref": c, "relies_on": [] }), "2026-10-06").unwrap();
        assert_eq!((out.output["settled"].clone(), refs(&out.output["reopened"])), (json!([]), vec![q.clone()]));
        let t = s.store().tickets[q.trim_start_matches("ticket:")].clone();
        assert_eq!((t.status, t.decision.is_none()), (Status::Open, true));
        let c_id = c.trim_start_matches("ticket:");
        assert!(
            t.body.starts_with("Asked.") && t.body.contains("Answered 2026-10-05") && t.body.contains(&format!("change {c_id} stopped relying on it")) && t.body.ends_with("This one."),
            "{}",
            t.body
        );
        // And one whose change lands closes with it, named in what the landing answers.
        call(&mut s, "update_ticket", json!({ "ref": c, "relies_on": [q] }), "2026-10-06").unwrap();
        call(&mut s, "start_ticket", json!({ "ref": q }), "2026-10-06").unwrap();
        call(&mut s, "close_ticket", answer(&q, "That one."), "2026-10-06").unwrap();
        call(&mut s, "start_ticket", json!({ "ref": c }), "2026-10-06").unwrap();
        call(&mut s, "close_ticket", answer(&c, "Applied."), "2026-10-06").unwrap();
        let out = call(&mut s, "land_ticket", json!({}), "2026-10-07").unwrap();
        assert_eq!(refs(&out.output["settled"]), vec![q.clone()]);
        assert_eq!(s.store().tickets[q.trim_start_matches("ticket:")].status, Status::Closed);
    }

    /// # Test contract
    ///
    /// ```json
    /// {"test": "a-checkout-takes-up-a-ticket-and-lands-nothing-unnamed", "checks": ["crates/core/src/tools/write/tickets#takes-up", "crates/core/src/tools/state#server-copy"]}
    /// ```
    #[test]
    fn a_checkout_takes_up_a_ticket_and_lands_nothing_unnamed() {
        // On a store of one's own, a ticket already active is not started again, and status names no server.
        let mut own = state();
        assert_eq!(call(&mut own, "start_ticket", json!({ "ref": "ticket:later-1" }), "2026-10-03").unwrap_err().code(), "conflict");
        assert_eq!(call(&mut own, "status", json!({}), "2026-10-03").unwrap().output["server"], json!(null));
        // Told several checkouts share it, it is taken up in this one: current here, the store's start kept.
        own.set_shares_checkouts(true);
        assert!(!own.store().starts.contains_key("later-1"));
        call(&mut own, "start_ticket", json!({ "ref": "ticket:later-1" }), "2026-10-05").unwrap();
        let start = own.store().starts.get("later-1").cloned().expect("one with no start gets one");
        own.store_mut().current = None;
        let out = call(&mut own, "start_ticket", json!({ "ref": "ticket:later-1" }), "2026-10-05").unwrap();
        assert_eq!((own.current().as_deref(), own.store().starts.get("later-1")), (Some("later-1"), Some(&start)), "taken up again, the store's start is kept");
        assert_eq!(out.changed, Some(Changed::Current));
        // A machine's copy of a server's intent says so.
        let mut s = state();
        s.set_server(Some(crate::answer::ServerCopy { address: "https://grimoire.example".into(), fetched: Some("2026-10-03T14:02:00Z".into()), reachable: false, refused: false }));
        let status = call(&mut s, "status", json!({}), "2026-10-03").unwrap().output;
        assert_eq!(status["server"], json!({ "address": "https://grimoire.example", "fetched": "2026-10-03T14:02:00Z", "reachable": false, "refused": false }));
        // later-1 is already active, started in another checkout: taken up here, it is current and has this checkout's start.
        let before = s.store().tickets["later-1"].clone();
        // Its spec so far is a proposal, and the start answers with it.
        call(&mut s, "put_page", json!({ "slug": "orphan", "title": "Orphan", "body": "as proposed", "links": [], "ticket": "later-1" }), "2026-10-03").unwrap();
        let out = call(&mut s, "start_ticket", json!({ "ref": "ticket:later-1" }), "2026-10-03").unwrap();
        let orphan = out.output["rests_on"].as_array().unwrap().iter().find(|x| x["ref"] == "page:orphan").cloned().expect("it touches the page");
        assert_eq!(orphan["says"], "as proposed");
        assert_eq!(out.changed, Some(Changed::Many(vec![Changed::Started("later-1".into()), Changed::Current])));
        assert_eq!((s.store().tickets["later-1"].clone(), s.current().as_deref()), (before, Some("later-1")));
        assert!(s.store().starts.contains_key("later-1") && out.output["rests_on"].is_array());
        // An open ticket starts as anywhere, and a closed one is still refused.
        assert_eq!(call(&mut s, "start_ticket", json!({ "ref": "ticket:phase-62" }), "2026-10-03").unwrap().output["node"]["status"], "active");
        call(&mut s, "close_ticket", json!({ "ref": "ticket:phase-62", "decision": { "text": "Done." } }), "2026-10-03").unwrap();
        assert_eq!(call(&mut s, "start_ticket", json!({ "ref": "ticket:phase-62" }), "2026-10-03").unwrap_err().code(), "conflict");
        // An answer waiting for its change is not being worked, so it is not taken up.
        let gap = s.store_mut().tickets.get_mut("gap-1").unwrap();
        (gap.status, gap.decision) = (Status::Landing, Some(Decision::new("2026-10-03", "Fix it.")));
        assert_eq!(call(&mut s, "start_ticket", json!({ "ref": "ticket:gap-1" }), "2026-10-03").unwrap_err().code(), "conflict");
        // Its current ticket landing, an unnamed landing lands nothing and says where a ticket lands; named, it lands.
        call(&mut s, "start_ticket", json!({ "ref": "ticket:later-1" }), "2026-10-03").unwrap();
        call(&mut s, "close_ticket", json!({ "ref": "ticket:later-1", "decision": { "text": "Done." }, "when": "landed" }), "2026-10-03").unwrap();
        let none = call(&mut s, "land_ticket", json!({}), "2026-10-03").unwrap();
        assert_eq!((none.output["landed"].clone(), none.output["waiting"][0]["ref"].clone()), (json!(false), json!("ticket:later-1")));
        assert!(none.changed.is_none() && none.output["note"].as_str().unwrap().contains("main line"));
        let landed = call(&mut s, "land_ticket", json!({ "ref": "ticket:later-1" }), "2026-10-03").unwrap().output;
        assert_eq!((landed["landed"].clone(), landed.get("note")), (json!(true), None));
        // The baseline is the main line's: nothing here prunes it.
        call(&mut s, "adopt", json!({ "reason": "inherited" }), "2026-10-03").unwrap();
        s.set_extraction(crate::extraction::Extraction::from_files(&Vec::<crate::extraction::FileExtraction>::new()));
        assert!(s.prune_baseline().is_none());
    }
}
