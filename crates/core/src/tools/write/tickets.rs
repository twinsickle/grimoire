//! # Module contract
//!
//! ```json
//! {"purpose":"Answer the tools of a ticket's life, brew_ticket, close_ticket, bottle_ticket and withdraw_ticket, with what each records of the ticket, applies to the truth and settles of the answers it relies on.","criteria":[{"id":"untouched","statement":"brew_ticket records the fingerprint of every node as the ticket's start, but on a state that holds no code, which records none; a close, by close_ticket or by bottle_ticket, answers with the touched nodes whose fingerprint is unchanged and, as unnamed, the intent_of what fingerprint::unnamed gives of its own work, the pages and requirements its proposal held and, for a change, the modules whose code differs from the state's base_fingerprints when a base is set and from the start otherwise, both null when the ticket has no start or the state holds no code, and removes the start."},{"id":"bottles","statement":"close_ticket with when: bottled refuses with a conflict naming the moved records when the truth moved under the proposal, and otherwise writes the decision and leaves the ticket bottling with nothing applied; bottle_ticket closes a simmering ticket as close_ticket does, with the decision it carries and that decision's date, and when the truth moved refuses with the same conflict, the ticket still simmering."},{"id":"withdraws","statement":"withdraw_ticket closes the change it names with ops::withdraw_ticket, refusing a decision with no text or no ISO date as invalid before anything changes, then settles the answers the change relied on as gone, saying it was withdrawn, and answers with the change as bottled and the answers it settled and reopened; it takes no ticket field and applies nothing, whether or not the truth moved under its proposal."},{"id":"bottle-answers","statement":"bottle_ticket bottles the ticket its input names, a conflict when that one is not simmering or is a gap or question waiting for its change, or with none the store's current ticket when it is simmering and otherwise nothing, which is not an error and changes nothing; it answers whether a ticket was bottled, what its close answers, and the changes still simmering, and takes no ticket field."},{"id":"takes-up","statement":"On a machine's copy of a server's intent, or on a store of its own the state is told several checkouts share, brew_ticket on a change already being worked, active or waiting to be bottled, makes it this checkout's current ticket and records its start, changing nothing of the ticket, a store of its own keeping the start it has, since there a ticket's start is the store's, not one checkout's; on a state that holds no code it takes such a change up too, changing nothing of it and setting no current ticket and no start; and bottle_ticket naming no ticket bottles nothing and says a ticket is bottled on the main line, by name; on any other state brew_ticket refuses a ticket that is not inscribed but a brewing gap or question, which it claims, a gap or question waiting for its change among those refused."},{"id":"inscribes-its-change","statement":"close_ticket given change on a gap or question files that change as inscribe_ticket files one, open, relying on the gap or question before the references given, and keeping pace, then writes the decision, followed by a sentence naming that change as what applies it, as close_ticket with when: bottled writes it, in one write: when either part is refused nothing of it is kept, the filed change removed, and the change is persisted first, an inscribed change of that title already relying on the gap or question being taken as the one filed, so a call stopped between its records is completed by running it again; it answers with the gap or question as when: bottled answers and with the change in filed as inscribe_ticket answers it, and what changed names the change before the decision; change given on a change, or with when: now, is invalid before anything changes, and a close without change answers no filed."},{"id":"checks-out","statement":"brew_ticket adds the state's caller, when it has one, to who has the ticket checked out with ops::check_out, and answers as checked_out the list as it then is, empty when nobody is listed."},{"id":"claims","statement":"brew_ticket on an inscribed gap or question makes it brewing, and on a brewing one, on any state, changes nothing of it, and in either case neither makes it the current ticket nor records a start, the current ticket staying what it was, and answers no waiting."},{"id":"close-notes","statement":"close_ticket answers as others each who had the ticket checked out but the state's caller, by name and since when, with a note that it was bottled all the same, and none for a ticket that was not being worked or that lists nobody; the close goes ahead whoever has it."},{"id":"brew-answers","statement":"brew_ticket answers the ticket's rests_on walk, as its own proposal leaves the graph, parted by query::assumptions: as rests_on the nodes kept in full, and as reaches the count and references of the rest, nearest first, with the related gaps and questions and the recipes resting on what it touches."},{"id":"brew-warns","statement":"brew_ticket answers as waiting each change ops::waiting_here gives for this checkout other than the ticket started, with its title and a warning naming it and saying a branch holds one ticket until it is bottled, to bottle it first or start this ticket on a new branch; the start goes ahead whatever waits; and answers as others each who had the ticket checked out before it but the state's caller, by name and since when, with a warning, a ticket being worked with nobody listed as one unknown, and none for a ticket that was inscribed; the start goes ahead whoever has it."}]}
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

/// brew_ticket: a change made current with its start recorded, or taken up where a ticket's start is each checkout's,
/// a gap or question claimed and never made current, the caller added to who has it checked out, answered with what it
/// rests on, the related tickets, the recipes over what it touches, who else has it and what still waits here.
pub(super) fn start(state: &mut State, name: &str, input: Value) -> Result<Outcome, ToolError> {
    let i: RefIn = parse(input)?;
    let ticket = state.store.tickets.get(&i.r#ref.id).filter(|_| i.r#ref.kind == Kind::Ticket).cloned();
    // A gap or question is claimed: never the current ticket, and given no start, since no checkout works it (req:ticket-checked-out).
    let answer_kind = ticket.as_ref().is_some_and(|t| t.kind != TicketKind::Change);
    let worked = ticket.as_ref().is_some_and(Ticket::is_worked);
    // Who had it before this start, while it is worked: each of them but the caller is named.
    let before = state.store.checked_out_of(&i.r#ref.id);
    // On a machine's copy of a server's intent, a change already being worked is taken up
    // in this checkout: it becomes current here and gets this checkout's start, and nothing of it changes.
    // So it is on a store of its own several checkouts share, each with its own current ticket, and on
    // the server itself, which only lists the taker. A gap or question already brewing is claimed again anywhere.
    let taken_up = worked && (answer_kind || state.server().is_some() || state.shares_checkouts() || state.without_code());
    // There the start is the store's, one per ticket: the one it has is kept.
    let keeps_start = taken_up && state.server().is_none() && state.store.starts.contains_key(&i.r#ref.id);
    if !taken_up {
        ops::brew_ticket(state.store_mut(), &i.r#ref)?;
    }
    let id = i.r#ref.id.clone();
    // The ticket's record is written with its start when it has one, and on its own otherwise.
    let mut changes = if taken_up { vec![] } else { vec![Changed::Ticket(id.clone())] };
    if answer_kind {
        // Claimed: nothing of a checkout's own changes.
    } else if state.without_code() {
        // A state serving a team keeps no current ticket and no start: both are each checkout's own.
        state.store_mut().current = None;
        changes.push(Changed::Current);
    } else if keeps_start {
        state.store_mut().current = Some(id.clone());
        changes.push(Changed::Current);
    } else {
        state.store_mut().current = Some(id.clone());
        // Every node, not only what touches lists now: nodes are often
        // added to touches after they changed.
        let fingerprints = crate::fingerprint::all(state.graph()).into_iter().map(|(r, fp)| (r.to_string(), fp)).collect();
        state.store_mut().starts.insert(id.clone(), Start { ticket: id.clone(), fingerprints, simmering_in: None });
        // The started ticket is now current, so writes that name none go into it.
        changes.retain(|c| *c != Changed::Ticket(id.clone()));
        changes.extend([Changed::Started(id.clone()), Changed::Current]);
    }
    // Who took it, and when, as the caller the daemon names (req:ticket-checked-out).
    let caller = state.caller.clone();
    if let Some(who) = &caller {
        if ops::check_out(state.store_mut(), &id, who) {
            changes.push(Changed::CheckedOut);
        }
    }
    let others = others_than(before, caller.as_ref(), |who| format!("{} is checked out by {who}: this start goes ahead, so agree with them who works it", i.r#ref));
    let checked_out = state.store.checked_out_of(&id).unwrap_or_default();
    let changed = match changes.len() {
        0 => None,
        1 => Some(changes.remove(0)),
        _ => Some(Changed::Many(changes)),
    };
    // As the ticket's own proposal leaves the graph: one taken up already carries its spec.
    let g = state.graph_for(Some(&i.r#ref.id))?;
    let node = query::view(g, &i.r#ref).expect("just written");
    // The assumptions to challenge in full; the rest of the blast radius by reference (req:tool-brew-ticket).
    let (rests_on, rest) = query::assumptions(query::rests_on(g, &i.r#ref, None));
    let reaches = Reaches { count: rest.len(), refs: rest };
    let (related, recipes) = match g.get(&i.r#ref) {
        Some(crate::graph::Node::Ticket(t)) => (query::related_tickets(g, t), query::recipes_resting(g, &t.touches)),
        _ => (vec![], vec![]),
    };
    // A branch holds one ticket until it is bottled: a change this checkout still carries is named, and the start goes ahead.
    // A gap or question claimed takes no branch, so nothing waits for it.
    let waiting = if answer_kind { vec![] } else { ops::waiting_here(state.store(), Some(&i.r#ref.id)) }
        .into_iter()
        .map(|t| WaitingHere {
            ticket: t.slug.clone(),
            title: t.title.clone(),
            warning: format!(
                "ticket:{} \"{}\" is still simmering from this checkout: a branch holds one ticket until it is bottled, so bottle it first, or brew {} on a new branch",
                t.slug, t.title, i.r#ref
            ),
        })
        .collect();
    answer(name, Brewed { node, rests_on, reaches, related, recipes, waiting, checked_out, others }, changed)
}

/// Each who had a ticket checked out, `before`, but the caller, with the sentence `say` gives for who and since
/// when; a ticket being worked with nobody listed, started before who was recorded, as one unknown, since an
/// unknown time. None when it was not worked.
fn others_than(before: Option<Vec<CheckedOut>>, caller: Option<&CheckedOut>, say: impl Fn(&str) -> String) -> Vec<Holder> {
    match before {
        None => vec![],
        Some(list) if list.is_empty() => vec![Holder { by: None, since: None, warning: say("someone unknown, from before who has a ticket was recorded") }],
        Some(list) => {
            list.into_iter().filter(|c| caller.is_none_or(|me| me.by != c.by)).map(|c| Holder { warning: say(&format!("{} since {}", c.by, c.since)), by: Some(c.by), since: Some(c.since) }).collect()
        }
    }
}

/// close_ticket: closed at once, its proposal applied, or with when: bottled its decision written to wait for the bottling.
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
        // Read as `landed` too, its name before ticket 0168.
        #[serde(alias = "landed")]
        Bottled,
    }
    #[derive(Deserialize)]
    #[serde(deny_unknown_fields)]
    struct In {
        r#ref: Reference,
        decision: DecisionIn,
        when: Option<When>,
        change: Option<ChangeIn>,
    }
    let i: In = parse(input)?;
    let touched = open_ticket_touches(state, &i.r#ref)?;
    // Whoever else has it checked out is named, and the close goes ahead (req:ticket-checked-out). One unknown is not
    // named here: every ticket started before who was recorded lists nobody, and on a close that is only noise.
    let others: Vec<Holder> = others_than(state.store.checked_out_of(&i.r#ref.id), state.caller.as_ref(), |who| format!("{} was checked out by {who}: it is closed all the same", i.r#ref))
        .into_iter()
        .filter(|h| h.by.is_some())
        .collect();
    // Nothing below may fail once the proposal is applied, so the decision is checked first.
    let decision = Decision::new(i.decision.date.unwrap_or_else(|| today.to_string()), i.decision.text);
    if !crate::store::is_date(&decision.date) || decision.text.trim().is_empty() {
        return Err(ToolError::invalid("a decision is a dated paragraph: text, and an ISO date naming a day", vec![]));
    }
    let change = state.store.tickets.get(&i.r#ref.id).is_some_and(|t| t.kind == TicketKind::Change);
    // The change that applies an answer is filed with it, and the answer then waits for it to bottle (req:answer-bottled-with-its-change).
    let when = match (&i.change, i.when) {
        (Some(_), _) if change => {
            return Err(ToolError::invalid(format!("{} is a change: `change` files the change that applies a gap's or question's answer, so give it when closing a gap or question", i.r#ref), vec![]))
        }
        (Some(_), Some(When::Now)) => {
            return Err(ToolError::invalid("`change` files the change that applies this answer, and the answer then waits for it to bottle: give no `when`, or `when: bottled`", vec![]))
        }
        (Some(_), _) => When::Bottled,
        (None, w) => w.unwrap_or_default(),
    };
    if when == When::Bottled {
        // The decision is written and the ticket waits: nothing reaches the truth until its change is bottled.
        // An answer holds no proposal, so decide_ticket refuses one that does before any comparison.
        let moved = if change { ops::moved(state.store(), &i.r#ref.id) } else { vec![] };
        if !moved.is_empty() {
            return Err(ToolError::conflict_with(format!("{} cannot wait to be bottled: the truth moved under its proposal; read what moved again and propose it again", i.r#ref), moved));
        }
        let (filed, changed) = match i.change {
            None => (None, ops::decide_ticket(state.store_mut(), &i.r#ref, decision)?.1),
            Some(c) => {
                let (slug, moved, changed) = file_and_decide(state, &i.r#ref, decision, c)?;
                (Some((slug, moved)), changed)
            }
        };
        // The change as inscribe_ticket answers it, its warnings judged as lint with no ticket judges them.
        let filed = filed.map(|(slug, moved)| {
            let r = Reference::ticket(&slug);
            let reprioritised = super::summaries_of(state, &moved);
            let (warnings, baselined) = super::ticket_warnings(state, None, &r);
            let node = query::view(state.graph(), &r).expect("just filed");
            Written { node, warnings, baselined, settled: None, reopened: None, reprioritised: Some(reprioritised) }
        });
        // What the check before the bottling will see: the truth with this ticket's proposal on it.
        let (code, start) = (!state.without_code(), state.store.starts.get(&i.r#ref.id).cloned());
        let g = state.graph_for(Some(&i.r#ref.id))?;
        let node = query::view(g, &i.r#ref).expect("just written");
        let (findings, baselined) = split_new(lint::lint_touching(g, &touched));
        let due = code.then(|| due_over(g, &touched, start.as_ref()));
        return answer(name, Closed { node, findings, baselined, untouched: None, unnamed: None, settled: vec![], due, filed, others }, Some(changed));
    }
    let (closed, changed) = close_now(state, &i.r#ref, decision, touched)?;
    answer(name, Closed { others, ..closed }, Some(changed))
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

/// bottle_ticket: the ticket named, or the current one when it is simmering, bottled as close_ticket closes it.
pub(super) fn bottle(state: &mut State, name: &str, input: Value) -> Result<Outcome, ToolError> {
    #[derive(Deserialize)]
    #[serde(deny_unknown_fields)]
    struct In {
        r#ref: Option<Reference>,
    }
    let i: In = parse(input)?;
    // Named, the ticket must be simmering; unnamed, the current ticket is bottled when it is, and otherwise nothing does.
    // On a machine's copy of a server's intent an unnamed bottling bottles nothing: a commit there is not the main line.
    let note = (i.r#ref.is_none() && state.server().is_some()).then(|| "this project names a server: a ticket is bottled on the main line, by name, once its change has merged".to_string());
    let target = match i.r#ref {
        Some(r) => match state.store.tickets.get(&r.id) {
            Some(t) if r.kind == Kind::Ticket && t.status == Status::Simmering && t.kind != TicketKind::Change => {
                return Err(ToolError::conflict(format!("{r} is an answer waiting for the change that applies it: it closes when that change is bottled, and is not bottled itself")))
            }
            Some(t) if r.kind == Kind::Ticket && t.status == Status::Simmering => Some(r),
            Some(t) if r.kind == Kind::Ticket => return Err(ToolError::conflict(format!("{r} is {}, not simmering: its decision is written with close_ticket and when: bottled", t.status.name()))),
            _ => return Err(ToolError::not_found(&r)),
        },
        None if note.is_some() => None,
        None => state.store.current.as_ref().filter(|c| state.store.tickets.get(*c).is_some_and(|t| t.status == Status::Simmering)).map(|c| Reference::ticket(c)),
    };
    let bottled = match target {
        None => None,
        Some(r) => {
            let touched = open_ticket_touches(state, &r)?;
            let decision = state.store.tickets[&r.id].decision.clone().expect("a simmering ticket carries its decision");
            Some(close_now(state, &r, decision, touched)?)
        }
    };
    // An answer waiting for its change is not bottled itself, so only changes wait here.
    let waiting: Vec<query::Summary> = state.store.tickets.values().filter(|t| t.status == Status::Simmering && t.kind == TicketKind::Change).map(query::ticket_summary).collect();
    match bottled {
        None => answer(name, Bottled { bottled: false, node: None, findings: vec![], baselined: 0, untouched: None, unnamed: None, settled: vec![], due: None, waiting, note }, None),
        Some((c, changed)) => answer(
            name,
            Bottled {
                bottled: true,
                node: Some(c.node),
                findings: c.findings,
                baselined: c.baselined,
                untouched: c.untouched,
                unnamed: c.unnamed,
                settled: c.settled,
                due: c.due,
                waiting,
                note: None,
            },
            Some(changed),
        ),
    }
}

/// The nodes a ticket that is not bottled touches; a bottled one is a conflict.
fn open_ticket_touches(state: &State, r: &Reference) -> Result<Vec<Reference>, ToolError> {
    match state.store.tickets.get(&r.id) {
        Some(t) if r.kind == Kind::Ticket && t.status != Status::Bottled => Ok(t.touches.clone()),
        Some(_) if r.kind == Kind::Ticket => Err(ToolError::conflict(format!("{r} is already bottled"))),
        _ => Err(ToolError::not_found(r)),
    }
}

/// Close a ticket at once: its proposal applied to the truth it was written
/// against, or the close refused naming what moved; then what the close
/// answers with and what changed. Shared by close_ticket and bottle_ticket.
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
        .map(|p| p.tickets.keys().filter_map(|s| state.store.tickets.get(s)).filter(|t| t.status != Status::Bottled).map(|t| (t.slug.clone(), t.relies_on.clone(), t.priority)).collect())
        .unwrap_or_default();
    // This ticket's own work, before its proposal is applied: the pages and requirements it proposes, and
    // for a change the code before it, the base's when the checkout has one, else the start's (req:tool-untouched).
    let proposed: Vec<Reference> = state
        .store
        .proposals
        .get(&r.id)
        .map(|p| {
            let written = p.pages.keys().map(|s| Reference::page(s)).chain(p.requirements.keys().chain(p.decisions.keys()).map(|s| Reference::req(s)));
            written.chain(p.deleted.keys().filter_map(|k| k.parse::<Reference>().ok())).collect()
        })
        .unwrap_or_default();
    let change = state.store.tickets.get(&r.id).is_some_and(|t| t.kind == TicketKind::Change);
    let applied = ops::apply_proposal(state.store_mut(), &r.id);
    // Compared before the close, which changes the ticket's own record,
    // and after the proposal was applied, so a proposed change counts as one.
    // With no code held the comparison would cover intent alone and pass over every touched module in silence: unknown, so null.
    // Read the other way, what changed that it never touched, with the intent resting on it.
    let compared = if state.without_code() {
        None
    } else {
        state.store.starts.get(&r.id).cloned().map(|start| {
            let before = change.then(|| state.base_fingerprints().unwrap_or_else(|| crate::fingerprint::of_start(&start.fingerprints)));
            let now = crate::fingerprint::all(state.graph());
            (crate::fingerprint::untouched(&touched, &start.fingerprints, &now), crate::fingerprint::unnamed(&touched, &proposed, before.as_ref(), &now))
        })
    };
    let start = state.store.starts.get(&r.id).cloned();
    let due = (!state.without_code()).then(|| due_over(state.graph(), &touched, start.as_ref()));
    let untouched = compared.as_ref().map(|(u, _)| u.clone());
    let unnamed = compared.map(|(_, n)| query::intent_of(state.graph(), &n));
    let answers = applies(state, r);
    ops::close_ticket(state.store_mut(), r, decision)?;
    state.store_mut().starts.remove(&r.id);
    let mut changes = vec![applied, Changed::Bottled(r.id.clone()), Changed::Current];
    // A bottled change has applied the answers it relies on: each closes once no change applying it is left unbottled.
    let mut settled = vec![];
    if let Some(answers) = answers {
        let (done, more) = ops::settle(state.store_mut(), &answers, ops::Settling::Bottled);
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
            let why = format!("change {slug} stopped relying on it when {} was bottled", r.id);
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
    Ok((Closed { node, findings, baselined, untouched, unnamed, settled, due, filed: None, others: vec![] }, changed))
}

/// The change close_ticket files with an answer: what inscribe_ticket takes, its kind aside, since it is a change.
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ChangeIn {
    title: String,
    body: String,
    touches: Vec<Reference>,
    priority: Priority,
    #[serde(default)]
    relies_on: Vec<Reference>,
}

/// File the change that applies the answer `r` and write the answer's decision, naming that change, to wait for it:
/// one write, so when either part is refused nothing of it is kept. Answers the change's slug, the tickets keeping
/// pace moved, and what changed, the change first, so a writer stopped between leaves an inscribed change relying on an
/// answer still brewing, which is a state the tools make anyway.
fn file_and_decide(state: &mut State, r: &Reference, decision: Decision, c: ChangeIn) -> Result<(String, Vec<String>, Changed), ToolError> {
    let mut relies_on = vec![r.clone()];
    for x in c.relies_on {
        if !relies_on.contains(&x) {
            relies_on.push(x);
        }
    }
    // Written per record, the change first: a call stopped between left it inscribed and the answer brewing, and running
    // the call again completes it with that change rather than filing a second (req:store-io-atomic-writes).
    let left = state.store.tickets.values().find(|t| t.kind == TicketKind::Change && t.status == Status::Inscribed && t.title == c.title && t.relies_on.contains(r)).cloned();
    let fresh = left.is_none();
    let (filed, filed_changed) = match left {
        Some(t) => (t, Changed::Many(vec![])),
        None => {
            let new = ops::NewTicket { title: c.title, kind: TicketKind::Change, priority: c.priority, touches: c.touches, relies_on, body: c.body };
            ops::inscribe_ticket(state.store_mut(), new)?
        }
    };
    // The change's id is known before the decision is written, so the decision names what applies it.
    let text = decision.text.trim_end();
    // A stop inside a closing quote or bracket still ends the sentence.
    let stop = if text.trim_end_matches(['"', '\'', ')', '\u{201d}', '\u{2019}']).ends_with(['.', '!', '?']) { "" } else { "." };
    let decision = Decision::new(decision.date, format!("{text}{stop} Applied by {}.", filed.slug));
    let decided = match ops::decide_ticket(state.store_mut(), r, decision) {
        Ok((_, changed)) => changed,
        Err(e) => {
            // Refused: the change filed for it goes too, and the store is as it was before the call.
            if fresh {
                state.store_mut().tickets.remove(&filed.slug);
            }
            return Err(e);
        }
    };
    // A change filed relying on something more urgent is filed at that priority, as inscribe_ticket files it.
    let (moved, more) = ops::keep_pace(state.store_mut(), &filed.slug, None);
    let mut changes = vec![filed_changed, decided];
    if !moved.is_empty() {
        changes.push(more);
    }
    Ok((filed.slug, moved, Changed::Many(changes)))
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
    /// {"test": "who-has-a-ticket-is-shown-and-never-refuses", "checks": ["crates/core/src/tools/write/tickets#checks-out", "crates/core/src/tools/write/tickets#claims", "crates/core/src/tools/write/tickets#brew-warns", "crates/core/src/tools/write/tickets#close-notes", "crates/core/src/tools/write#releases", "crates/core/src/tools#as-caller", "crates/core/src/tools/read#checked-out", "crates/core/src/query/board#board", "crates/core/src/ops/tickets#checked-out", "crates/core/src/ops/tickets#current-ticket", "crates/core/src/ops/tickets#closes-when-bottled", "req:ticket-checked-out"]}
    /// ```
    #[test]
    fn who_has_a_ticket_is_shown_and_never_refuses() {
        use crate::tools::call_by;
        let mut s = state();
        let day = "2026-10-08";
        let ana = CheckedOut { by: "ana".into(), since: "2026-10-08T14:10:00Z".into() };
        let ben = CheckedOut { by: "ben".into(), since: "2026-10-08T15:00:00Z".into() };
        let start = |s: &mut State, r: &str, who: &CheckedOut| call_by(s, "brew_ticket", json!({ "ref": r }), day, Some(who.clone())).unwrap();
        let names = |v: &Value| v.as_array().unwrap().iter().map(|c| c["by"].as_str().unwrap_or("?").to_string()).collect::<Vec<_>>();
        // A change ana starts is current, with its start, and checked out by her since she took it.
        let out = start(&mut s, "ticket:phase-62", &ana);
        assert_eq!((out.output["checked_out"].clone(), out.output["others"].clone()), (json!([{ "by": "ana", "since": "2026-10-08T14:10:00Z" }]), json!([])));
        let id = || "phase-62".to_string();
        assert_eq!(out.changed, Some(Changed::Many(vec![Changed::Started(id()), Changed::Current, Changed::CheckedOut])));
        assert!(s.caller.is_none(), "the caller is held for that call alone");
        // A gap she claims is brewing and hers, but never current and given no start: the change stays current, and nothing waits on it.
        let out = start(&mut s, "ticket:gap-1", &ana);
        assert_eq!((out.output["node"]["status"].clone(), names(&out.output["checked_out"]), out.output["waiting"].clone()), (json!("brewing"), vec!["ana".to_string()], json!([])));
        assert_eq!((s.store().current.as_deref(), s.store().starts.contains_key("gap-1")), (Some("phase-62"), false));
        // Ben claims it too: told ana has it, and since when, and it goes ahead.
        let out = start(&mut s, "ticket:gap-1", &ben);
        assert_eq!((names(&out.output["checked_out"]), names(&out.output["others"])), (vec!["ana".to_string(), "ben".to_string()], vec!["ana".to_string()]));
        let warning = out.output["others"][0]["warning"].as_str().unwrap();
        assert!(warning.contains("ticket:gap-1 is checked out by ana since 2026-10-08T14:10:00Z") && warning.contains("goes ahead"), "{warning}");
        // Ana again is not listed twice, and keeps when she first took it.
        let later = CheckedOut { since: "2026-10-08T16:00:00Z".into(), ..ana.clone() };
        let out = start(&mut s, "ticket:gap-1", &later);
        assert_eq!((names(&out.output["others"]), out.output["checked_out"][0]["since"].clone()), (vec!["ben".to_string()], json!("2026-10-08T14:10:00Z")));
        // get and the board say who has each ticket being worked; one started before it was recorded lists nobody.
        let get = |s: &mut State, r: &str| call(s, "get", json!({ "ref": r }), day).unwrap().output;
        assert_eq!(names(&get(&mut s, "ticket:gap-1")["checked_out"]), ["ana", "ben"]);
        assert_eq!(get(&mut s, "ticket:later-1")["checked_out"], json!([]));
        assert!(get(&mut s, "page:orphan").get("checked_out").is_none());
        let board = call(&mut s, "board", json!({ "kinds": ["change"] }), day).unwrap().output;
        let row = |b: &Value, list: &str, r: &str| b[list].as_array().unwrap().iter().find(|t| t["ref"] == r).cloned().unwrap();
        assert_eq!((names(&row(&board, "tickets", "ticket:phase-62")["checked_out"]), row(&board, "tickets", "ticket:later-1")["checked_out"].clone()), (vec!["ana".to_string()], json!([])));
        assert_eq!(names(&row(&board, "unattached", "ticket:gap-1")["checked_out"]), ["ana", "ben"]);
        // Claiming one being worked with nobody listed warns of someone unknown.
        s.store_mut().tickets.insert("q".into(), crate::graph::fixtures::ticket("q", TicketKind::Question, Priority::Later, Status::Brewing, &[], &[]));
        let out = start(&mut s, "ticket:q", &ben);
        assert_eq!((out.output["others"][0]["by"].clone(), out.output["others"].as_array().map(Vec::len)), (json!(null), Some(1)));
        assert!(out.output["others"][0]["warning"].as_str().unwrap().contains("someone unknown"));
        // Ben closes the gap ana holds: it closes, with a note naming her, and who had it goes with the close.
        let out = call_by(&mut s, "close_ticket", json!({ "ref": "ticket:gap-1", "decision": { "text": "Answered." } }), day, Some(ben.clone())).unwrap();
        assert_eq!((out.output["node"]["status"].clone(), names(&out.output["others"])), (json!("bottled"), vec!["ana".to_string()]));
        assert!(out.output["others"][0]["warning"].as_str().unwrap().contains("closed all the same"));
        assert!(!s.store().checked_out.contains_key("gap-1"));
        assert!(matches!(&out.changed, Some(Changed::Many(c)) if c.last() == Some(&Changed::CheckedOut)), "{:?}", out.changed);
        // An inscribed question closes with no start, and a close of what nobody holds names no one.
        let (q2, _) =
            ops::inscribe_ticket(s.store_mut(), ops::NewTicket { title: "Why".into(), kind: TicketKind::Question, priority: Priority::Later, touches: vec![], relies_on: vec![], body: "Ask.".into() })
                .unwrap();
        let out = call(&mut s, "close_ticket", json!({ "ref": format!("ticket:{}", q2.slug), "decision": { "text": "Because." } }), day).unwrap();
        assert_eq!((out.output["node"]["status"].clone(), out.output.get("others")), (json!("bottled"), None));
        // The lists themselves: one name once, and gone with the work.
        assert!(!ops::check_out(s.store_mut(), "phase-62", &later), "ana is listed already");
        s.store_mut().tickets.get_mut("phase-62").unwrap().status = Status::Inscribed;
        assert!(ops::release_unworked(s.store_mut()) && !s.store().checked_out.contains_key("phase-62"));
        assert!(!ops::release_unworked(s.store_mut()), "nothing more to drop");
    }

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
        let mut q = ticket("q", TicketKind::Question, Priority::Later, Status::Simmering, &[], &[]);
        q.decision = Some(Decision::new("2026-10-01", "Count lines."));
        let c = ticket("c", TicketKind::Change, Priority::Later, Status::Brewing, &[], &["ticket:q"]);
        s.store_mut().tickets.insert("q".into(), q);
        s.store_mut().tickets.insert("c".into(), c);
        let refused = call(&mut s, "withdraw_ticket", json!({ "ref": "ticket:c", "decision": { "text": " " } }), "2026-10-06");
        assert!(matches!(refused, Err(ToolError::Invalid { .. })), "a decision with no text changes nothing");
        assert_eq!(s.store().tickets["c"].status, Status::Brewing);
        let out = call(&mut s, "withdraw_ticket", json!({ "ref": "ticket:c", "decision": { "text": "Reconsidered: lines are not counted." } }), "2026-10-06").unwrap().output;
        assert_eq!(out["node"]["status"], "bottled");
        assert_eq!(out["node"]["decision"]["withdrawn"], true);
        assert_eq!(out["reopened"][0]["ref"], "ticket:q", "{out}");
        let q = &s.store().tickets["q"];
        assert_eq!(q.status, Status::Inscribed, "a withdrawn change, closed after the answer, never counts as having applied it");
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
        call(&mut s, "brew_ticket", json!({ "ref": "ticket:phase-62" }), "2026-09-28").unwrap();
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
        assert!(matches!(&steps[1], Changed::Bottled(slug) if slug == "phase-62"));
        assert!(matches!(steps.last(), Some(Changed::Current)));
        assert!(s.store().current.is_none(), "closing the current ticket clears it");
        assert_eq!(s.store().pages["orphan"].body, "Changed before being listed.", "the proposal reached the truth");
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
        call(&mut s, "brew_ticket", json!({ "ref": "ticket:phase-62" }), "2026-10-04").unwrap();
        call(&mut s, "update_ticket", json!({ "ref": "ticket:phase-62", "touches": ["page:git-readiness"] }), "2026-10-04").unwrap();
        // The work reaches a module it never named, by its code, and a page, by the proposal.
        let mut files = crate::extraction::fixtures::extraction_files();
        files.iter_mut().find(|f| f.module == "src/a/ready").expect("fixture").digest = "edited".into();
        s.set_extraction(Extraction::from_files(&files));
        call(&mut s, "patch_page", json!({ "ref": "page:orphan", "body": "Reached unnamed." }), "2026-10-04").unwrap();
        // Written bottling, it is not known yet.
        let bottling = call(&mut s, "close_ticket", json!({ "ref": "ticket:phase-62", "decision": { "text": "Done." }, "when": "bottled" }), "2026-10-04").unwrap();
        assert!(bottling.output["unnamed"].is_null());
        let out = call(&mut s, "bottle_ticket", json!({ "ref": "ticket:phase-62" }), "2026-10-04").unwrap();
        let unnamed = out.output["unnamed"].as_array().expect("a list once it is bottled").clone();
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
        call(&mut s, "brew_ticket", json!({ "ref": "ticket:phase-62" }), "2026-10-04").unwrap();
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
        // Bottled at once, never simmering.
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
        call(&mut s, "brew_ticket", json!({ "ref": "ticket:phase-62" }), "2026-10-04").unwrap();
        call(&mut s, "update_ticket", json!({ "ref": "ticket:phase-62", "touches": [] }), "2026-10-04").unwrap();
        call(&mut s, "rename_node", json!({ "ref": "page:orphan", "to": "adopted" }), "2026-10-04").unwrap();
        call(&mut s, "patch_page", json!({ "ref": "page:adopted", "body": "Renamed, then changed." }), "2026-10-04").unwrap();
        let out = call(&mut s, "close_ticket", json!({ "ref": "ticket:phase-62", "decision": { "text": "Done." } }), "2026-10-04").unwrap();
        let refs: Vec<String> = out.output["unnamed"].as_array().unwrap().iter().map(|x| x["ref"].as_str().unwrap().to_string()).collect();
        assert_eq!(refs, vec!["page:adopted".to_string()]);
    }

    /// # Test contract
    ///
    /// ```json
    /// {"test": "unnamed-is-only-this-tickets-own-work", "checks": ["crates/core/src/fingerprint#unnamed", "crates/core/src/tools/write/tickets#untouched", "crates/core/src/tools/state#advisories"]}
    /// ```
    #[test]
    fn unnamed_is_only_this_tickets_own_work() {
        let edited = |modules: &[&str]| {
            let mut files = crate::extraction::fixtures::extraction_files();
            for f in files.iter_mut().filter(|f| modules.contains(&f.module.as_str())) {
                f.digest = format!("{} edited", f.digest);
            }
            Extraction::from_files(&files)
        };
        let close = |s: &mut State, slug: &str| -> Vec<String> {
            let out = call(s, "close_ticket", json!({ "ref": format!("ticket:{slug}"), "decision": { "text": "Done." } }), "2026-10-08").unwrap().output;
            out["unnamed"].as_array().expect("a list").iter().map(|x| x["ref"].as_str().unwrap().to_string()).collect()
        };
        // A change on a branch: another ticket brings a page meanwhile, and the main line's change to layout is merged in.
        let mut s = state();
        call(&mut s, "brew_ticket", json!({ "ref": "ticket:phase-62" }), "2026-10-08").unwrap();
        call(&mut s, "update_ticket", json!({ "ref": "ticket:phase-62", "touches": [] }), "2026-10-08").unwrap();
        s.store_mut().pages.get_mut("orphan").expect("fixture").body = "Another ticket's bottling.".into();
        s.set_base(Some(edited(&["src/a/layout"])));
        s.set_extraction(edited(&["src/a/layout", "src/a/ready"]));
        assert_eq!(close(&mut s, "phase-62"), ["module:src/a/ready", "req:ready-only-with-commit"], "its own module, with the requirement on it; not the page, nor layout as the base has it");
        // With no base, the code before the work is the start's: layout's change counts, the page still does not.
        let mut s = state();
        call(&mut s, "brew_ticket", json!({ "ref": "ticket:phase-62" }), "2026-10-08").unwrap();
        call(&mut s, "update_ticket", json!({ "ref": "ticket:phase-62", "touches": [] }), "2026-10-08").unwrap();
        s.store_mut().pages.get_mut("orphan").expect("fixture").body = "Another ticket's bottling.".into();
        s.set_extraction(edited(&["src/a/layout"]));
        assert_eq!(close(&mut s, "phase-62"), ["module:src/a/layout"]);
        // A gap carries no code and is claimed with no start: what changed in the checkout is not its own work.
        let mut s = state();
        call(&mut s, "brew_ticket", json!({ "ref": "ticket:gap-1" }), "2026-10-08").unwrap();
        s.set_extraction(edited(&["src/a/ready"]));
        let out = call(&mut s, "close_ticket", json!({ "ref": "ticket:gap-1", "decision": { "text": "Done." } }), "2026-10-08").unwrap().output;
        assert_eq!(out["unnamed"], json!(null));
    }

    /// # Test contract
    ///
    /// ```json
    /// {"test": "a-brew-answers-the-assumptions-and-reaches-the-rest", "checks": ["crates/core/src/tools/write/tickets#brew-answers", "req:tool-brew-ticket"]}
    /// ```
    #[test]
    fn a_start_answers_the_assumptions_and_reaches_the_rest() {
        let mut s = state();
        let out = call(&mut s, "brew_ticket", json!({ "ref": "ticket:phase-62" }), "2026-10-08").unwrap().output;
        let full: Vec<&str> = out["rests_on"].as_array().unwrap().iter().map(|x| x["ref"].as_str().unwrap()).collect();
        assert!(full.contains(&"module:src/a/ready") && full.contains(&"req:ready-only-with-commit"), "{full:?}");
        assert!(!full.contains(&"module:src/a/layout"), "an import further away is not in full: {full:?}");
        let refs = out["reaches"]["refs"].as_array().unwrap();
        assert!(refs.contains(&json!("module:src/a/layout")), "{out}");
        assert_eq!(out["reaches"]["count"], refs.len());
        // The rests_on tool still answers the whole walk, with the path.
        let walk = call(&mut s, "rests_on", json!({ "ref": "ticket:phase-62" }), "2026-10-08").unwrap().output;
        let layout = walk["nodes"].as_array().unwrap().iter().find(|x| x["ref"] == "module:src/a/layout").expect("the whole walk").clone();
        assert_eq!(layout["via"].as_array().unwrap().last(), Some(&json!("module:src/a/ready")));
        assert_eq!(walk["nodes"].as_array().unwrap().len(), full.len() + refs.len());
    }

    #[test]
    fn start_and_close_return_context() {
        let mut s = state();
        let out = call(&mut s, "brew_ticket", json!({ "ref": "ticket:phase-62" }), "2026-09-27").unwrap();
        assert!(!out.output["rests_on"].as_array().unwrap().is_empty());
        assert_eq!(out.output["related"].as_array().unwrap().len(), 1);
        let out = call(&mut s, "close_ticket", json!({ "ref": "ticket:phase-62", "decision": { "text": "Done." } }), "2026-09-27").unwrap();
        assert_eq!(out.output["node"]["decision"]["date"], "2026-09-27");
        assert!(out.output["findings"].is_array());
    }

    /// # Test contract
    ///
    /// ```json
    /// {"test": "a-brew-names-a-ticket-still-simmering-here", "checks": ["crates/core/src/tools/write/tickets#brew-warns", "crates/core/src/ops/tickets#waits-here", "crates/core/src/tools/state#advisories"]}
    /// ```
    #[test]
    fn a_brew_names_a_ticket_still_simmering_here() {
        let mut s = state();
        let day = "2026-10-07";
        // gap-1 is made a change here, since a gap or question claimed takes no branch and is told of nothing waiting.
        s.store_mut().tickets.get_mut("gap-1").unwrap().kind = TicketKind::Change;
        // phase-62 is started here, its proposal mends req:stale's dangling criterion, and it is bottled to wait for its bottling.
        assert_eq!(call(&mut s, "brew_ticket", json!({ "ref": "ticket:phase-62" }), day).unwrap().output["waiting"], json!([]));
        let mut stale = s.store().requirements["stale"].clone();
        stale.covered_by = vec!["criterion:src/a/ready#guarantee".parse().unwrap()];
        let mut put = serde_json::to_value(&stale).unwrap();
        put["ticket"] = json!("phase-62");
        put.as_object_mut().unwrap().remove("decisions");
        call(&mut s, "put_requirement", put, day).unwrap();
        let closed = call(&mut s, "close_ticket", json!({ "ref": "ticket:phase-62", "decision": { "text": "Done." }, "when": "bottled" }), day).unwrap();
        assert_eq!(closed.changed, Some(Changed::Many(vec![Changed::Ticket("phase-62".into()), Changed::Started("phase-62".into())])), "the start says where it was bottled");
        assert_eq!(s.store().starts["phase-62"].simmering_in.as_deref(), Some("C:/repo"));
        // While it is current, it is not named as waiting beside its own findings.
        let lint = call(&mut s, "lint", json!({ "kinds": ["dangling"] }), day).unwrap().output;
        assert_eq!(lint["mended"], json!([]));
        // Another ticket started here is told, and starts all the same.
        let started = call(&mut s, "brew_ticket", json!({ "ref": "ticket:gap-1" }), day).unwrap().output;
        assert_eq!(started["node"]["status"], "brewing");
        assert_eq!((started["waiting"][0]["ticket"].clone(), started["waiting"].as_array().map(Vec::len)), (json!("phase-62"), Some(1)));
        let warning = started["waiting"][0]["warning"].as_str().unwrap();
        assert!(warning.contains("ticket:phase-62") && warning.contains("bottle it first") && warning.contains("new branch"), "{warning}");
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
            ops::inscribe_ticket(s.store_mut(), ops::NewTicket { title: "Next".into(), kind: TicketKind::Change, priority: Priority::Later, touches: vec![], relies_on: vec![], body: "More.".into() })
                .unwrap();
        assert_eq!(call(&mut s, "brew_ticket", json!({ "ref": format!("ticket:{}", filed.slug) }), day).unwrap().output["waiting"], json!([]));
        assert_eq!(call(&mut s, "lint", json!({ "kinds": ["dangling"] }), day).unwrap().output["mended"], json!([]));
        // Bottled, it is waiting nowhere.
        s.store_mut().config.repository = "C:/repo".into();
        call(&mut s, "bottle_ticket", json!({ "ref": "ticket:phase-62" }), day).unwrap();
        assert!(ops::waiting_here(s.store(), None).is_empty());
    }

    /// # Test contract
    ///
    /// ```json
    /// {"test": "a-change-simmers-and-the-bottling-bottles-it", "checks": ["crates/core/src/tools/write/tickets#bottles", "crates/core/src/tools/write/tickets#bottle-answers", "crates/core/src/ops/tickets#closes-when-bottled", "crates/core/src/tools/read#ticket-view"]}
    /// ```
    #[test]
    fn a_change_simmers_and_the_bottling_bottles_it() {
        let mut s = state();
        let code = |r: Result<Outcome, ToolError>| r.unwrap_err().code();
        let truth = s.store().pages["orphan"].clone();
        let orphan = |body: &str| json!({ "slug": "orphan", "title": "Orphan", "body": body, "links": [], "ticket": "later-1" });
        let bottled = |text: &str| json!({ "ref": "ticket:later-1", "decision": { "text": text }, "when": "bottled" });
        call(&mut s, "put_page", orphan("proposed"), "2026-10-03").unwrap();
        // Nothing to bottle yet: later-1 is brewing, and no ticket is current. Not an error, and nothing changes.
        let none = call(&mut s, "bottle_ticket", json!({}), "2026-10-03").unwrap();
        assert_eq!((none.output["bottled"].clone(), none.output["node"].clone()), (json!(false), json!(null)));
        assert!(none.changed.is_none());
        assert_eq!(code(call(&mut s, "bottle_ticket", json!({ "ref": "ticket:later-1" }), "2026-10-03")), "conflict", "brewing, not simmering");
        // The decision is written and the ticket waits: its body, proposal and start kept, the truth untouched.
        let out = call(&mut s, "close_ticket", bottled("Done."), "2026-10-03").unwrap();
        assert_eq!((out.output["node"]["status"].clone(), out.output["untouched"].clone()), (json!("simmering"), json!(null)));
        let t = s.store().tickets["later-1"].clone();
        assert_eq!((t.status, t.decision.as_ref().map(|d| d.text.as_str()), t.body.is_empty()), (Status::Simmering, Some("Done."), false));
        assert!(s.store().proposals.contains_key("later-1"));
        assert_eq!(s.store().pages["orphan"], truth);
        // It is still written and read with its proposal, and its decision can be written again.
        call(&mut s, "put_page", orphan("proposed again"), "2026-10-03").unwrap();
        assert_eq!(call(&mut s, "get", json!({ "ref": "page:orphan", "ticket": "later-1" }), "2026-10-03").unwrap().output["node"]["body"], "proposed again");
        call(&mut s, "close_ticket", bottled("Done, said better."), "2026-10-04").unwrap();
        // An inscribed change is not being worked; an inscribed gap needs no start, but no change relies on it here.
        assert_eq!(code(call(&mut s, "close_ticket", json!({ "ref": "ticket:gap-1", "decision": { "text": "A." }, "when": "bottled" }), "2026-10-04")), "invalid");
        assert_eq!(code(call(&mut s, "close_ticket", json!({ "ref": "ticket:phase-62", "decision": { "text": "A." }, "when": "bottled" }), "2026-10-04")), "conflict");
        // The truth moves under it: the bottling is refused naming the record, the ticket is still simmering, and its decision cannot be written again either.
        ops::put_page(s.store_mut(), Page { body: "moved".into(), ..truth.clone() }).unwrap();
        let e = call(&mut s, "bottle_ticket", json!({ "ref": "ticket:later-1" }), "2026-10-05").unwrap_err();
        let v = serde_json::to_value(&e).unwrap();
        assert_eq!((v["error"].clone(), v["details"][0]["field"].clone()), (json!("conflict"), json!("page:orphan")));
        assert_eq!(s.store().tickets["later-1"].status, Status::Simmering);
        assert_eq!(s.store().pages["orphan"].body, "moved", "nothing applied");
        assert_eq!(code(call(&mut s, "close_ticket", bottled("Again."), "2026-10-05")), "conflict");
        // Proposed again over the truth as it now is. Unnamed, only the current ticket is bottled, so it is listed as waiting.
        call(&mut s, "put_page", orphan("proposed"), "2026-10-05").unwrap();
        let waiting = call(&mut s, "bottle_ticket", json!({}), "2026-10-05").unwrap();
        assert_eq!((waiting.output["bottled"].clone(), waiting.output["waiting"][0]["ref"].clone()), (json!(false), json!("ticket:later-1")));
        // Named, it is bottled: closed with the decision written before and that decision's date.
        let out = call(&mut s, "bottle_ticket", json!({ "ref": "ticket:later-1" }), "2026-10-06").unwrap();
        assert_eq!((out.output["bottled"].clone(), out.output["node"]["status"].clone(), out.output["waiting"].clone()), (json!(true), json!("bottled"), json!([])));
        let t = s.store().tickets["later-1"].clone();
        assert_eq!(t.decision.map(|d| (d.date, d.text)), Some(("2026-10-04".to_string(), "Done, said better.".to_string())));
        assert!(t.body.is_empty() && !s.store().proposals.contains_key("later-1") && !s.store().starts.contains_key("later-1"));
        assert_eq!(s.store().pages["orphan"].body, "proposed");
        // The current ticket stays current while simmering, is bottled unnamed, and stops being current.
        call(&mut s, "brew_ticket", json!({ "ref": "ticket:phase-62" }), "2026-10-06").unwrap();
        call(&mut s, "close_ticket", json!({ "ref": "ticket:phase-62", "decision": { "text": "Done." }, "when": "bottled" }), "2026-10-06").unwrap();
        assert_eq!(s.store().current.as_deref(), Some("phase-62"));
        let out = call(&mut s, "bottle_ticket", json!({}), "2026-10-06").unwrap();
        assert_eq!(out.output["bottled"], true);
        assert_eq!(out.output["untouched"], json!(["req:ready-only-with-commit", "module:src/a/ready"]), "compared with its start when it is bottled: what it named and never changed");
        assert!(s.store().current.is_none() && s.store().tickets["phase-62"].status == Status::Bottled);
        // bottle_ticket applies at once and takes no ticket.
        assert_eq!(code(call(&mut s, "bottle_ticket", json!({ "ticket": "later-1" }), "2026-10-06")), "invalid");
    }

    /// # Test contract
    ///
    /// ```json
    /// {"test": "an-answer-waits-for-the-change-that-applies-it", "checks": ["crates/core/src/ops/tickets#closes-when-bottled", "crates/core/src/ops/tickets#settles-answers", "crates/core/src/ops/proposal#proposal-writes", "crates/core/src/ops#delete-node", "crates/core/src/tools/write#settles", "crates/core/src/tools/write/tickets#bottle-answers", "crates/core/src/tools/write#write-responses"]}
    /// ```
    #[test]
    fn an_answer_waits_for_the_change_that_applies_it() {
        let mut s = state();
        let code = |r: Result<Outcome, ToolError>| r.unwrap_err().code();
        let answer = |r: &str, text: &str| json!({ "ref": r, "decision": { "text": text }, "when": "bottled" });
        let refs = |v: &Value| v.as_array().unwrap().iter().map(|x| x["ref"].as_str().unwrap().to_string()).collect::<Vec<_>>();
        call(&mut s, "brew_ticket", json!({ "ref": "ticket:gap-1" }), "2026-10-04").unwrap();
        // No change applies it yet: the answer cannot wait for one.
        let e = call(&mut s, "close_ticket", answer("ticket:gap-1", "Fix it."), "2026-10-04").unwrap_err();
        assert!(e.code() == "invalid" && e.to_string().contains("file the change"), "{e}");
        call(&mut s, "update_ticket", json!({ "ref": "ticket:later-1", "relies_on": ["ticket:gap-1"] }), "2026-10-04").unwrap();
        // Its spec belongs in that change.
        call(&mut s, "put_page", json!({ "slug": "note", "title": "Note", "body": "b", "links": [], "ticket": "gap-1" }), "2026-10-04").unwrap();
        assert_eq!(code(call(&mut s, "close_ticket", answer("ticket:gap-1", "Fix it."), "2026-10-04")), "invalid");
        s.store_mut().proposals.remove("gap-1");
        // Answered: it waits, simmering, never having been current; the change relying on it is ready.
        let out = call(&mut s, "close_ticket", answer("ticket:gap-1", "Fix it."), "2026-10-04").unwrap();
        assert_eq!((out.output["node"]["status"].clone(), out.output["settled"].clone()), (json!("simmering"), json!([])));
        assert!(s.store().current.is_none() && !s.store().starts.contains_key("gap-1"));
        // What names it as worked is persisted before the record that says it waits, so a writer stopped between leaves a brewing answer.
        assert_eq!(out.changed, Some(Changed::Many(vec![Changed::Proposed("gap-1".into()), Changed::Bottled("gap-1".into())])));
        let board = call(&mut s, "board", json!({}), "2026-10-04").unwrap();
        let later = board.output["tickets"].as_array().unwrap().iter().find(|t| t["ref"] == "ticket:later-1").unwrap().clone();
        assert_eq!(later["waits_on"][0]["status"], "simmering");
        // Nothing closes it by hand, nothing is bottled as it, and it takes no spec.
        assert_eq!(code(call(&mut s, "bottle_ticket", json!({ "ref": "ticket:gap-1" }), "2026-10-04")), "conflict");
        assert_eq!(code(call(&mut s, "close_ticket", json!({ "ref": "ticket:gap-1", "decision": { "text": "Done." } }), "2026-10-04")), "conflict");
        assert_eq!(code(call(&mut s, "put_page", json!({ "slug": "note", "title": "Note", "body": "b", "links": [], "ticket": "gap-1" }), "2026-10-04")), "conflict");
        assert_eq!(code(call(&mut s, "delete_node", json!({ "ref": "ticket:gap-1" }), "2026-10-04")), "invalid", "it carries the person's decision");
        assert_eq!(call(&mut s, "bottle_ticket", json!({}), "2026-10-04").unwrap().output["waiting"], json!([]), "only changes wait to be bottled");
        // Two changes apply it: the first closing leaves it waiting for the second.
        let second = call(&mut s, "inscribe_ticket", json!({ "title": "Second", "kind": "change", "priority": "later", "relies_on": ["ticket:gap-1"], "body": "b" }), "2026-10-04").unwrap();
        let second = second.output["node"]["ref"].as_str().unwrap().to_string();
        let out = call(&mut s, "close_ticket", json!({ "ref": "ticket:later-1", "decision": { "text": "Done." } }), "2026-10-05").unwrap();
        assert_eq!(out.output["settled"], json!([]));
        assert_eq!(s.store().tickets["gap-1"].status, Status::Simmering);
        // The second goes away, but the first applied it after its answer: it closes with its own decision.
        let out = call(&mut s, "delete_node", json!({ "ref": second }), "2026-10-05").unwrap();
        assert_eq!((refs(&out.output["settled"]), out.output["reopened"].clone()), (vec!["ticket:gap-1".to_string()], json!([])));
        let gap = s.store().tickets["gap-1"].clone();
        assert_eq!((gap.status, gap.decision.map(|d| d.text), gap.body), (Status::Bottled, Some("Fix it.".to_string()), String::new()));
        // An answer whose only change stops relying on it reopens, its answer in its body.
        let q = call(&mut s, "inscribe_ticket", json!({ "title": "Which?", "kind": "question", "priority": "high", "body": "Asked." }), "2026-10-05").unwrap();
        let q = q.output["node"]["ref"].as_str().unwrap().to_string();
        let c = call(&mut s, "inscribe_ticket", json!({ "title": "Apply", "kind": "change", "priority": "high", "relies_on": [q], "body": "b" }), "2026-10-05").unwrap();
        let c = c.output["node"]["ref"].as_str().unwrap().to_string();
        call(&mut s, "brew_ticket", json!({ "ref": q }), "2026-10-05").unwrap();
        call(&mut s, "close_ticket", answer(&q, "This one."), "2026-10-05").unwrap();
        let out = call(&mut s, "update_ticket", json!({ "ref": c, "relies_on": [] }), "2026-10-06").unwrap();
        assert_eq!((out.output["settled"].clone(), refs(&out.output["reopened"])), (json!([]), vec![q.clone()]));
        let t = s.store().tickets[q.trim_start_matches("ticket:")].clone();
        assert_eq!((t.status, t.decision.is_none()), (Status::Inscribed, true));
        let c_id = c.trim_start_matches("ticket:");
        assert!(
            t.body.starts_with("Asked.") && t.body.contains("Answered 2026-10-05") && t.body.contains(&format!("change {c_id} stopped relying on it")) && t.body.ends_with("This one."),
            "{}",
            t.body
        );
        // And one whose change is bottled closes with it, named in what the simmering answers.
        call(&mut s, "update_ticket", json!({ "ref": c, "relies_on": [q] }), "2026-10-06").unwrap();
        call(&mut s, "brew_ticket", json!({ "ref": q }), "2026-10-06").unwrap();
        call(&mut s, "close_ticket", answer(&q, "That one."), "2026-10-06").unwrap();
        call(&mut s, "brew_ticket", json!({ "ref": c }), "2026-10-06").unwrap();
        call(&mut s, "close_ticket", answer(&c, "Applied."), "2026-10-06").unwrap();
        let out = call(&mut s, "bottle_ticket", json!({}), "2026-10-07").unwrap();
        assert_eq!(refs(&out.output["settled"]), vec![q.clone()]);
        assert_eq!(s.store().tickets[q.trim_start_matches("ticket:")].status, Status::Bottled);
    }

    /// # Test contract
    ///
    /// ```json
    /// {"test": "an-answer-inscribes-the-change-that-applies-it", "checks": ["crates/core/src/tools/write/tickets#inscribes-its-change", "req:answer-bottled-with-its-change", "req:tools-write-responses"]}
    /// ```
    #[test]
    fn an_answer_files_the_change_that_applies_it() {
        let mut s = state();
        let code = |r: Result<Outcome, ToolError>| r.unwrap_err().code();
        let change = json!({ "title": "Apply it", "body": "Not Y.", "touches": ["page:overview", "req:nowhere"], "priority": "next", "relies_on": ["ticket:gap-1", "ticket:later-1"] });
        let close = |when: Option<&str>| {
            let mut i = json!({ "ref": "ticket:gap-1", "decision": { "text": "Do X" }, "change": change.clone() });
            if let Some(w) = when {
                i["when"] = json!(w);
            }
            i
        };
        let tickets = |s: &State| s.store().tickets.len();
        call(&mut s, "brew_ticket", json!({ "ref": "ticket:gap-1" }), "2026-10-08").unwrap();
        let before = tickets(&s);
        // Refused before anything changes: a change is not an answer, and the answer waits for the change it files.
        let mut on_change = close(None);
        on_change["ref"] = json!("ticket:later-1");
        assert_eq!(code(call(&mut s, "close_ticket", on_change, "2026-10-08")), "invalid");
        assert_eq!(code(call(&mut s, "close_ticket", close(Some("now")), "2026-10-08")), "invalid");
        assert_eq!(tickets(&s), before);
        // Refused by the decision's write, with a proposal on the answer: the change filed for it goes too.
        call(&mut s, "put_page", json!({ "slug": "note", "title": "Note", "body": "b", "links": [], "ticket": "gap-1" }), "2026-10-08").unwrap();
        let e = call(&mut s, "close_ticket", close(None), "2026-10-08").unwrap_err();
        assert_eq!(e.code(), "invalid", "{e}");
        assert_eq!(tickets(&s), before, "no change is left filed");
        let gap = s.store().tickets["gap-1"].clone();
        assert_eq!((gap.status, gap.decision), (Status::Brewing, None));
        s.store_mut().proposals.remove("gap-1");
        // One call: the change filed relying on the answer first, the answer waiting for it with a decision naming it.
        let out = call(&mut s, "close_ticket", close(Some("bottled")), "2026-10-08").unwrap();
        let filed = &out.output["filed"];
        let slug = filed["node"]["ref"].as_str().unwrap().trim_start_matches("ticket:").to_string();
        let t = s.store().tickets[&slug].clone();
        assert_eq!((t.kind, t.status, t.title.as_str(), t.body.as_str()), (TicketKind::Change, Status::Inscribed, "Apply it", "Not Y."));
        assert_eq!(t.relies_on, vec![Reference::ticket("gap-1"), Reference::ticket("later-1")], "the answer first, each once");
        let gap = s.store().tickets["gap-1"].clone();
        assert_eq!((gap.status, gap.decision.map(|d| d.text)), (Status::Simmering, Some(format!("Do X. Applied by {slug}."))));
        assert_eq!(out.output["node"]["ref"], "ticket:gap-1");
        assert!(filed["warnings"].as_array().unwrap().iter().any(|w| w["edge"]["to"] == "req:nowhere"), "{filed}");
        assert!(filed["reprioritised"].is_array());
        // The change is persisted before the decision, so a writer stopped between leaves an inscribed change relying on a brewing answer.
        let Some(Changed::Many(changes)) = out.changed else { panic!("many changes") };
        assert_eq!(changes[0], Changed::Ticket(slug.clone()));
        // A call stopped after the change was written, before the decision: running it again completes it with that change.
        call(&mut s, "inscribe_ticket", json!({ "title": "Settle it", "kind": "question", "priority": "later", "body": "b" }), "2026-10-08").unwrap();
        let q = s.store().tickets.values().find(|t| t.title == "Settle it").unwrap().slug.clone();
        let left = call(&mut s, "inscribe_ticket", json!({ "title": "Apply it", "kind": "change", "priority": "later", "relies_on": [format!("ticket:{q}")], "body": "b" }), "2026-10-08").unwrap();
        let left = left.output["node"]["ref"].as_str().unwrap().trim_start_matches("ticket:").to_string();
        call(&mut s, "brew_ticket", json!({ "ref": format!("ticket:{q}") }), "2026-10-08").unwrap();
        let before = tickets(&s);
        let mut again = close(None);
        again["ref"] = json!(format!("ticket:{q}"));
        let out = call(&mut s, "close_ticket", again, "2026-10-08").unwrap();
        assert_eq!((tickets(&s), out.output["filed"]["node"]["ref"].as_str()), (before, Some(format!("ticket:{left}").as_str())), "no second change");
        assert_eq!(s.store().tickets[&q].decision.as_ref().map(|d| d.text.clone()), Some(format!("Do X. Applied by {left}.")));
        // A close without `change` answers no `filed`.
        let out = call(&mut s, "close_ticket", json!({ "ref": "ticket:later-1", "decision": { "text": "Done." } }), "2026-10-08").unwrap();
        assert!(out.output.get("filed").is_none(), "{}", out.output);
    }

    /// # Test contract
    ///
    /// ```json
    /// {"test": "a-checkout-takes-up-a-ticket-and-bottles-nothing-unnamed", "checks": ["crates/core/src/tools/write/tickets#takes-up", "crates/core/src/tools/state#server-copy"]}
    /// ```
    #[test]
    fn a_checkout_takes_up_a_ticket_and_bottles_nothing_unnamed() {
        // On a store of one's own, a ticket already brewing is not started again, and status names no server.
        let mut own = state();
        assert_eq!(call(&mut own, "brew_ticket", json!({ "ref": "ticket:later-1" }), "2026-10-03").unwrap_err().code(), "conflict");
        assert_eq!(call(&mut own, "status", json!({}), "2026-10-03").unwrap().output["server"], json!(null));
        // Told several checkouts share it, it is taken up in this one: current here, the store's start kept.
        own.set_shares_checkouts(true);
        assert!(!own.store().starts.contains_key("later-1"));
        call(&mut own, "brew_ticket", json!({ "ref": "ticket:later-1" }), "2026-10-05").unwrap();
        let start = own.store().starts.get("later-1").cloned().expect("one with no start gets one");
        own.store_mut().current = None;
        let out = call(&mut own, "brew_ticket", json!({ "ref": "ticket:later-1" }), "2026-10-05").unwrap();
        assert_eq!((own.current().as_deref(), own.store().starts.get("later-1")), (Some("later-1"), Some(&start)), "taken up again, the store's start is kept");
        assert_eq!(out.changed, Some(Changed::Current));
        // A machine's copy of a server's intent says so.
        let mut s = state();
        s.set_server(Some(crate::answer::ServerCopy { address: "https://grimoire.example".into(), fetched: Some("2026-10-03T14:02:00Z".into()), reachable: false, refused: false }));
        let status = call(&mut s, "status", json!({}), "2026-10-03").unwrap().output;
        assert_eq!(status["server"], json!({ "address": "https://grimoire.example", "fetched": "2026-10-03T14:02:00Z", "reachable": false, "refused": false }));
        // later-1 is already brewing, started in another checkout: taken up here, it is current and has this checkout's start.
        let before = s.store().tickets["later-1"].clone();
        // Its spec so far is a proposal, and the start answers with it.
        call(&mut s, "put_page", json!({ "slug": "orphan", "title": "Orphan", "body": "as proposed", "links": [], "ticket": "later-1" }), "2026-10-03").unwrap();
        let out = call(&mut s, "brew_ticket", json!({ "ref": "ticket:later-1" }), "2026-10-03").unwrap();
        let orphan = out.output["rests_on"].as_array().unwrap().iter().find(|x| x["ref"] == "page:orphan").cloned().expect("it touches the page");
        assert_eq!(orphan["says"], "as proposed");
        assert_eq!(out.changed, Some(Changed::Many(vec![Changed::Started("later-1".into()), Changed::Current])));
        assert_eq!((s.store().tickets["later-1"].clone(), s.current().as_deref()), (before, Some("later-1")));
        assert!(s.store().starts.contains_key("later-1") && out.output["rests_on"].is_array());
        // An inscribed ticket starts as anywhere, and a bottled one is still refused.
        assert_eq!(call(&mut s, "brew_ticket", json!({ "ref": "ticket:phase-62" }), "2026-10-03").unwrap().output["node"]["status"], "brewing");
        call(&mut s, "close_ticket", json!({ "ref": "ticket:phase-62", "decision": { "text": "Done." } }), "2026-10-03").unwrap();
        assert_eq!(call(&mut s, "brew_ticket", json!({ "ref": "ticket:phase-62" }), "2026-10-03").unwrap_err().code(), "conflict");
        // An answer waiting for its change is not being worked, so it is not taken up.
        let gap = s.store_mut().tickets.get_mut("gap-1").unwrap();
        (gap.status, gap.decision) = (Status::Simmering, Some(Decision::new("2026-10-03", "Fix it.")));
        assert_eq!(call(&mut s, "brew_ticket", json!({ "ref": "ticket:gap-1" }), "2026-10-03").unwrap_err().code(), "conflict");
        // Its current ticket simmering, an unnamed bottling bottles nothing and says where a ticket is bottled; named, it is bottled.
        call(&mut s, "brew_ticket", json!({ "ref": "ticket:later-1" }), "2026-10-03").unwrap();
        call(&mut s, "close_ticket", json!({ "ref": "ticket:later-1", "decision": { "text": "Done." }, "when": "bottled" }), "2026-10-03").unwrap();
        let none = call(&mut s, "bottle_ticket", json!({}), "2026-10-03").unwrap();
        assert_eq!((none.output["bottled"].clone(), none.output["waiting"][0]["ref"].clone()), (json!(false), json!("ticket:later-1")));
        assert!(none.changed.is_none() && none.output["note"].as_str().unwrap().contains("main line"));
        let bottled = call(&mut s, "bottle_ticket", json!({ "ref": "ticket:later-1" }), "2026-10-03").unwrap().output;
        assert_eq!((bottled["bottled"].clone(), bottled.get("note")), (json!(true), None));
        // The baseline is the main line's: nothing here prunes it.
        call(&mut s, "adopt", json!({ "reason": "inherited" }), "2026-10-03").unwrap();
        s.set_extraction(crate::extraction::Extraction::from_files(&Vec::<crate::extraction::FileExtraction>::new()));
        assert!(s.prune_baseline().is_none());
    }
}
