//! # Module contract
//!
//! ```json
//! {"purpose":"Answer the write tools: each applies one operation to the store and answers with the node's summary as it now exists, the findings the write created and what changed for the caller to persist; a ticket's life, brew_ticket, close_ticket, bottle_ticket and withdraw_ticket, has a module beneath.","criteria":[{"id":"releases","statement":"Every write that leaves a ticket gone or no longer being worked removes who had it checked out with ops::release_unworked, in the same write, after its own changes; a write that leaves every list as it was changes none."},{"id":"answers-summary","statement":"Every write answers with the node's summary as it now exists, its ref and title and a ticket's kind, priority and status, never the whole record, but brew_ticket, which answers the ticket in full, and bottle_ticket when no ticket was waiting, which answers with none."},{"id":"answers-warnings","statement":"put_page, patch_page, put_requirement, patch_requirement, record_decision, supersede, inscribe_ticket and update_ticket add as warnings the new findings the write created on that node (its dangling edges, and unspecified for a requirement with no scenario) and a count of the baselined ones, a ticket write judging them over the proposal an update_ticket's edit waits in, else as lint does with no ticket, over the store's current ticket's proposal when there is one and the truth when there is none, the ticket written judged whatever the reading."},{"id":"answers-reprioritised","statement":"inscribe_ticket and update_ticket add reprioritised, the tickets whose priority the write moved beyond the one asked."},{"id":"answers-drafted","statement":"update_ticket whose edit to another ticket's touches or relies_on waits in a change's proposal adds drafted, that change's summary and a note that the edit reaches the ticket when the change is bottled; an edit applied at once adds none."},{"id":"answers-brew","statement":"brew_ticket adds rests_on, related and recipes, the recipes resting on what it touches with whether each is due, and waiting."},{"id":"answers-close","statement":"close_ticket adds the new findings on the nodes it touched, a count of the baselined ones, the touched nodes that never changed, settled, the gaps and questions that closed with it, and due, the recipes due among those resting on what it touched and those whose fingerprint changed since its start, null on a state that holds no code; with when: bottled the findings and due as its proposal leaves them, no such list and settled empty; and given change on a gap or question filed, that change as inscribe_ticket answers it."},{"id":"answers-bottle","statement":"bottle_ticket answers whether a ticket was bottled, what its close answers when one did, and the changes still simmering."},{"id":"answers-judged","statement":"judge_recipe answers judged, for each recipe judged the recipe's reference, the judgement and whether the recipe is still due, null on a state that holds no code."},{"id":"answers-settled","statement":"update_ticket and delete_node of a change add settled and reopened, the answers they closed and reopened."},{"id":"answers-retire","statement":"retire_node adds as warnings the new findings on the criteria the retired requirement named and on the tests that still hold it, and a count of the baselined ones."},{"id":"answers-delete","statement":"delete_node returns the deleted reference and the new dangling edges left behind with a count of the baselined ones."},{"id":"answers-adopt","statement":"adopt returns the new baseline's date, reason and count with the count it replaced."},{"id":"answers-configure","statement":"configure returns the config with the count of findings now new."},{"id":"intent-writes-proposed","statement":"A write that changes intent (put_page, patch_page, put_requirement, patch_requirement, record_decision, supersede, retire_node, and delete_node of a page or requirement) goes into the proposal of the ticket its input names, else the store's current ticket, is refused as invalid with neither, and answers with the node as the proposal holds it."},{"id":"ticket-taken-where","statement":"Only the writes that change intent, update_ticket and judge_recipe take ticket; any other write refuses it as a field it does not know, and delete_node of a ticket refuses it."},{"id":"close-applies-or-conflicts","statement":"close_ticket refuses with a conflict naming the moved records when the truth moved under the proposal, and otherwise applies it before closing."},{"id":"proposes-another-tickets-references","statement":"update_ticket given touches or relies_on of another ticket under a change being worked, the one its input names in ticket or else the current ticket, proposes them in that change's proposal."},{"id":"both-lists-rest-on-truth","statement":"update_ticket writing both of another ticket's lists in a change's proposal rests on the truth as it is now."},{"id":"one-list-rests-on-proposal","statement":"update_ticket writing one of another ticket's lists in a change's proposal rests on the proposal's copy."},{"id":"answers-as-proposal-leaves-it","statement":"update_ticket of another ticket's references under a change answers with the ticket as that proposal leaves it and with warnings judged over it."},{"id":"applies-the-rest-at-once","statement":"update_ticket applies at once a ticket's title, priority and body, the references of the ticket itself, and references given under a gap or question or under no ticket."},{"id":"named-ticket-errors","statement":"update_ticket under a named ticket that does not exist is not_found and under one not being worked a conflict."},{"id":"settles-answers-bottled","statement":"A change closed by close_ticket or bottle_ticket settles the answers it relies on as bottled."},{"id":"settles-answers-gone","statement":"A change deleted, or one whose relies_on update_ticket shortens, or the close of another change whose proposal edits it shortens, settles the answers it relied on and no longer does as gone, with why."},{"id":"close-answers-among-settled","statement":"A close lists the answers it closed among settled."},{"id":"edited-change-keeps-pace","statement":"A ticket a close edits keeps pace with what it now relies on."},{"id":"settle-persisted-after-write","statement":"Each of these writes persists what the settle changed after the write's own changes."},{"id":"reprioritised","statement":"inscribe_ticket keeps pace for the ticket it filed and update_ticket for the ticket it edited, from the priority it had, after settling any answers the edit dropped; each answers the tickets keep_pace moved as reprioritised, with their priorities as they now are, and persists them after the write's own changes."},{"id":"judges-records","statement":"judge_recipe records with ops::judge a judgement of the recipe its ref names, with the outcome given, today's date and the ticket its input names or else the current ticket, refused as invalid with neither; the fingerprint is the one its input supplies, else the recipe's digest in the graph."},{"id":"judges-several","statement":"judge_recipe given judgements records each, in their order, its fingerprint the one the item supplies, else the recipe's digest, having checked every one before recording any so that one refused records none; given both ref and judgements, or neither, it is invalid."},{"id":"judges-refusals","statement":"judge_recipe refuses a ref that is not a recipe, an ingredient among them, as invalid, a recipe the graph does not hold on a state that holds code as not_found, and on a state that holds no code a call supplying no fingerprint as invalid."},{"id":"adopt-findings","statement":"adopt records the findings its caller supplies when it supplies them, and otherwise every finding present; a state that holds no code refuses an adopt that supplies none as invalid."}]}
//! ```

use serde::Deserialize;
use serde_json::Value;

mod tickets;

use super::read::baseline_summary;
use super::{answer, parse, Outcome, RefIn, State};
use crate::answer::*;
use crate::lint::{self, Rule};
use crate::model::*;
use crate::ops::{self, Changed, ToolError};
use crate::query;
use crate::store::FieldError;

/// Run the write tool `name`. `today` is the ISO date used when a decision has none.
/// A write that changes intent goes into the proposal of the `ticket` its input
/// names, else the store's current ticket, and is refused with neither; the
/// truth changes only when a ticket closes.
pub(super) fn call(state: &mut State, name: &str, input: Value, today: &str) -> Result<Outcome, ToolError> {
    let mut out = run(state, name, input, today)?;
    // Who has a ticket checked out goes with the write that ended its work: closed, withdrawn, deleted or waiting for its change.
    if ops::release_unworked(state.store_mut()) {
        out.changed = Some(match out.changed.take() {
            Some(changed) => Changed::Many(vec![changed, Changed::CheckedOut]),
            None => Changed::CheckedOut,
        });
    }
    Ok(out)
}

fn run(state: &mut State, name: &str, mut input: Value, today: &str) -> Result<Outcome, ToolError> {
    // Only a write that changes intent takes `ticket`; any other write refuses it with the fields it does not know.
    let ticket = if super::takes_ticket(name) { super::take_ticket(&mut input)? } else { None };
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
                append: Option<String>,
                replace: Option<ops::Replace>,
                add_links: Option<Vec<Reference>>,
                remove_links: Option<Vec<Reference>>,
            }
            let i: In = parse(input)?;
            let edit = ops::BodyEdit { replace: i.replace, append: i.append };
            let patch = ops::PagePatch { title: i.title, body: i.body, edit, add_links: i.add_links.unwrap_or_default(), remove_links: i.remove_links.unwrap_or_default() };
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
        "patch_requirement" => {
            #[derive(Deserialize)]
            #[serde(deny_unknown_fields)]
            struct In {
                r#ref: Reference,
                title: Option<String>,
                statement: Option<String>,
                put_scenarios: Option<Vec<Scenario>>,
                remove_scenarios: Option<Vec<String>>,
                add_covered_by: Option<Vec<Reference>>,
                remove_covered_by: Option<Vec<Reference>>,
            }
            let i: In = parse(input)?;
            let patch = ops::RequirementPatch {
                title: i.title,
                statement: i.statement,
                put_scenarios: i.put_scenarios.unwrap_or_default(),
                remove_scenarios: i.remove_scenarios.unwrap_or_default(),
                add_covered_by: i.add_covered_by.unwrap_or_default(),
                remove_covered_by: i.remove_covered_by.unwrap_or_default(),
            };
            let t = ops::target_ticket(state.store(), ticket.as_deref())?;
            // Built on the proposal's copy, as every partial write is (req:ticket-proposal).
            let (_, changed) = ops::propose(state.store_mut(), &t, ops::Proposing::Partial, |s| ops::patch_requirement(s, &i.r#ref, patch))?;
            written(name, state, Some(&t), &i.r#ref, changed)
        }
        "record_decision" => {
            #[derive(Deserialize)]
            #[serde(deny_unknown_fields)]
            struct In {
                r#ref: Reference,
                text: String,
                date: Option<String>,
                #[serde(default)]
                supersedes: Vec<DecisionId>,
                #[serde(default)]
                adopted: bool,
            }
            let i: In = parse(input)?;
            let t = ops::target_ticket(state.store(), ticket.as_deref())?;
            let date = i.date.unwrap_or_else(|| today.to_string());
            let (node, changed) = ops::propose(state.store_mut(), &t, ops::Proposing::Partial, |s| ops::record_decision(s, &i.r#ref, i.text, date, DecisionId::all(&i.supersedes), i.adopted))?;
            let r = Reference::req(&node.slug);
            written(name, state, Some(&t), &r, changed)
        }
        "supersede" => {
            #[derive(Deserialize)]
            #[serde(deny_unknown_fields)]
            struct In {
                r#ref: Reference,
                decision: DecisionId,
                supersedes: Vec<DecisionId>,
            }
            let i: In = parse(input)?;
            let t = ops::target_ticket(state.store(), ticket.as_deref())?;
            let (_, changed) = ops::propose(state.store_mut(), &t, ops::Proposing::Partial, |s| ops::supersede(s, &i.r#ref, i.decision.0, &DecisionId::all(&i.supersedes)))?;
            written(name, state, Some(&t), &i.r#ref, changed)
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
            let node = query::summary_at(g, &i.r#ref).expect("just written");
            let (warnings, baselined) = split_new(lint::lint_touching(g, &named));
            answer(name, Written { node, warnings, baselined, settled: None, reopened: None, reprioritised: None, drafted: None }, Some(changed))
        }
        "inscribe_ticket" => {
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
                return Err(ToolError::invalid("inscribe_ticket no longer takes a slug: the id is generated. Give a title.", vec![FieldError::new("slug", "not taken: the id is generated")]));
            }
            let i: In = parse(input)?;
            let t = ops::NewTicket { title: i.title, kind: i.kind, priority: i.priority, touches: i.touches.unwrap_or_default(), relies_on: i.relies_on.unwrap_or_default(), body: i.body };
            let (node, changed) = ops::inscribe_ticket(state.store_mut(), t)?;
            let r = Reference::ticket(&node.slug);
            // A change filed relying on something more urgent is filed at that priority.
            let (moved, more) = ops::keep_pace(state.store_mut(), &node.slug, None);
            let reprioritised = summaries_of(state, &moved);
            let changed = if moved.is_empty() { changed } else { Changed::Many(vec![changed, more]) };
            let node = query::summary_at(state.graph(), &r).expect("just written");
            let (warnings, baselined) = ticket_warnings(state, None, &r);
            answer(name, Written { node, warnings, baselined, settled: None, reopened: None, reprioritised: Some(reprioritised), drafted: None }, Some(changed))
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
                append: Option<String>,
                replace: Option<ops::Replace>,
            }
            let i: In = parse(input)?;
            // Another ticket's references, edited under a change being worked, wait in that change's proposal.
            let under = proposing_for(state, &i.r#ref, ticket.as_deref(), i.touches.is_some() || i.relies_on.is_some())?;
            let before = applies(state, &i.r#ref);
            let was = state.store.tickets.get(&i.r#ref.id).map(|t| t.priority);
            let (refs, now) = match &under {
                Some(_) => ((i.touches, i.relies_on), (None, None)),
                None => ((None, None), (i.touches, i.relies_on)),
            };
            let edit = ops::BodyEdit { replace: i.replace, append: i.append };
            let patch = ops::TicketPatch { title: i.title, priority: i.priority, touches: now.0, relies_on: now.1, body: i.body, edit };
            let held = state.store.tickets.get(&i.r#ref.id).cloned();
            let (_, changed) = ops::update_ticket(state.store_mut(), &i.r#ref, patch)?;
            let mut changes = vec![changed];
            if let Some(change) = &under {
                // A write of both lists rests on the truth as it is now; a write of one builds on the proposal's copy.
                let how = if refs.0.is_some() && refs.1.is_some() { ops::Proposing::Whole } else { ops::Proposing::Partial };
                let patch = ops::TicketPatch { touches: refs.0, relies_on: refs.1, ..Default::default() };
                match ops::propose(state.store_mut(), change, how, |s| ops::update_ticket(s, &i.r#ref, patch)) {
                    Ok((_, proposed)) => changes.push(proposed),
                    Err(e) => {
                        // Refused, the call changes nothing: the fields written at once are put back as they were.
                        if let Some(t) = held {
                            state.store_mut().tickets.insert(t.slug.clone(), t);
                        }
                        return Err(e);
                    }
                }
            }
            // An answer this change no longer relies on may be left with no change to apply it.
            let (mut settled, mut reopened) = (None, None);
            if let Some(before) = before {
                let now = state.store.tickets.get(&i.r#ref.id).map(|t| t.relies_on.clone()).unwrap_or_default();
                let dropped: Vec<Reference> = before.into_iter().filter(|r| !now.contains(r)).collect();
                let why = format!("change {} stopped relying on it", i.r#ref.id);
                let (done, more) = ops::settle(state.store_mut(), &dropped, ops::Settling::Gone(&why));
                if !done.is_empty() {
                    changes.push(more);
                }
                let (s, r) = summaries(state, &done);
                (settled, reopened) = (Some(s), Some(r));
            }
            // The priorities it relates keep pace with it.
            let (moved, more) = ops::keep_pace(state.store_mut(), &i.r#ref.id, was);
            if !moved.is_empty() {
                changes.push(more);
            }
            let reprioritised = Some(summaries_of(state, &moved));
            let changed = if changes.len() == 1 { changes.remove(0) } else { Changed::Many(changes) };
            // As the change it waits in would leave it, when it waits in one.
            let node = query::summary_at(state.graph_for(under.as_deref())?, &i.r#ref).expect("just written");
            let (warnings, baselined) = ticket_warnings(state, under.as_deref(), &i.r#ref);
            // Drafted into a change, the edit is said to wait there, so no caller takes it as made (req:tool-update-ticket).
            let drafted = under.as_ref().and_then(|change| {
                query::summary_at(state.graph(), &Reference::ticket(change))
                    .map(|c| Drafted { note: format!("the edit to {}'s references waits in {}'s proposal and reaches {} when {} is bottled", i.r#ref, c.r#ref, i.r#ref, c.r#ref), change: c })
            });
            answer(name, Written { node, warnings, baselined, settled, reopened, reprioritised, drafted }, Some(changed))
        }
        "brew_ticket" => tickets::start(state, name, input),
        "close_ticket" => tickets::close(state, name, input, today),
        "withdraw_ticket" => tickets::withdraw(state, name, input, today),
        "bottle_ticket" => tickets::bottle(state, name, input),
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
            let node = query::summary_at(state.graph(), &new).expect("just renamed");
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
        "judge_recipe" => {
            #[derive(Deserialize)]
            #[serde(deny_unknown_fields)]
            struct Item {
                r#ref: Reference,
                outcome: JudgementOutcome,
                fingerprint: Option<String>,
            }
            #[derive(Deserialize)]
            #[serde(deny_unknown_fields)]
            struct In {
                r#ref: Option<Reference>,
                outcome: Option<JudgementOutcome>,
                fingerprint: Option<String>,
                judgements: Option<Vec<Item>>,
            }
            let i: In = parse(input)?;
            let items = match (i.r#ref, i.outcome, i.judgements) {
                (Some(r#ref), Some(outcome), None) => vec![Item { r#ref, outcome, fingerprint: i.fingerprint }],
                (None, None, Some(items)) if i.fingerprint.is_none() && !items.is_empty() => items,
                _ => {
                    return Err(ToolError::invalid(
                        "give `ref` and `outcome` for one recipe, or `judgements` for several, each `{ref, outcome}` with its own fingerprint",
                        vec![FieldError::new("judgements", "give this, or ref and outcome, not both")],
                    ))
                }
            };
            let Some(ticket) = ticket.or_else(|| state.current()) else {
                return Err(ToolError::invalid("a recipe is judged in a ticket: name it in `ticket`, or start one", vec![FieldError::new("ticket", "absent, and no ticket is current")]));
            };
            // Every one checked before any is recorded: one refused records none (req:ops-validate-first).
            let code = !state.without_code();
            let mut judgements = Vec::new();
            for item in items {
                if item.r#ref.kind != Kind::Recipe || item.r#ref.id.contains('#') {
                    return Err(ToolError::invalid(format!("{} is not a recipe: judge_recipe judges a recipe whole, as recipe:<id>", item.r#ref), vec![FieldError::new("ref", "not a recipe")]));
                }
                // The recipe as this checkout's code reads it; a state with no code knows none.
                let digest = match state.graph().get(&item.r#ref) {
                    Some(crate::graph::Node::Recipe(x)) => Some(x.digest.clone()),
                    _ if code => return Err(ToolError::not_found(&item.r#ref)),
                    _ => None,
                };
                let Some(fingerprint) = item.fingerprint.or(digest) else {
                    return Err(ToolError::invalid(
                        "this daemon holds no code, so it cannot fingerprint the recipe: judge it from a machine that has the code, which supplies the fingerprint",
                        vec![FieldError::new("fingerprint", "absent, and this daemon holds no code to take it from")],
                    ));
                };
                judgements.push(Judgement { recipe: item.r#ref.id.clone(), fingerprint, date: today.to_string(), ticket: ticket.clone(), outcome: item.outcome });
            }
            // What ops::judge refuses of a later one, an ill-formed fingerprint, puts the earlier ones back.
            let held = state.store().judgements.clone();
            let mut recorded = Vec::new();
            for judgement in judgements {
                match ops::judge(state.store_mut(), judgement) {
                    Ok(done) => recorded.push(done),
                    Err(e) => {
                        state.store_mut().judgements = held;
                        return Err(e);
                    }
                }
            }
            let mut judged = Vec::new();
            let mut changes = Vec::new();
            for (judgement, changed) in recorded {
                changes.push(changed);
                let recipe = Reference::recipe(&judgement.recipe);
                let g = state.graph();
                let due = match g.get(&recipe) {
                    Some(crate::graph::Node::Recipe(x)) if code => Some(g.is_due(x)),
                    _ => None,
                };
                judged.push(Judged { recipe, judgement, due });
            }
            let changed = if changes.len() == 1 { changes.remove(0) } else { Changed::Many(changes) };
            answer(name, JudgedAll { judged }, Some(changed))
        }
        "adopt" => {
            #[derive(Deserialize)]
            #[serde(deny_unknown_fields)]
            struct In {
                #[serde(default)]
                reason: String,
                findings: Option<Vec<FindingKey>>,
            }
            let i: In = parse(input)?;
            let keys = match i.findings {
                // Supplied by a caller that holds the code, as a machine does for the server it writes through.
                Some(keys) => keys,
                // Every present finding is what a baseline is, and a daemon with no code cannot see them all.
                None if state.without_code() => {
                    return Err(ToolError::invalid(
                        "this daemon holds no code, so it cannot see every finding: adopt from a machine that has the code, which supplies them",
                        vec![FieldError::new("findings", "absent, and this daemon holds no code to find them")],
                    ))
                }
                None => lint::lint(state.graph(), &Rule::ALL).iter().map(lint::Finding::key).collect(),
            };
            let (baseline, replaced, changed) = ops::adopt(state.store_mut(), keys, i.reason, today.to_string())?;
            answer(name, Adopted { baseline: baseline_summary(&baseline), replaced }, Some(changed))
        }
        "delete_node" => {
            let i: RefIn = parse(input)?;
            // A ticket is not intent and goes at once; a page or requirement is a proposed deletion.
            let view = match i.r#ref.kind {
                Kind::Ticket if ticket.is_some() => {
                    return Err(ToolError::invalid(
                        "`ticket` does not apply: a ticket is deleted at once, with its start and proposal",
                        vec![FieldError::new("ticket", "not taken when deleting a ticket")],
                    ))
                }
                Kind::Ticket => None,
                _ => Some(ops::target_ticket(state.store(), ticket.as_deref())?),
            };
            // Edges that pointed at the node become dangling: collect them first.
            let pointing: Vec<Reference> = state.graph_for(view.as_deref())?.inbound(&i.r#ref).map(|e| e.from.clone()).collect();
            let answers = applies(state, &i.r#ref);
            let mut changed = match &view {
                None => ops::delete_node(state.store_mut(), &i.r#ref)?,
                Some(t) => ops::propose(state.store_mut(), t, ops::Proposing::Partial, |s| ops::delete_node(s, &i.r#ref).map(|c| ((), c)))?.1,
            };
            // A deleted change applies no answer, and one it alone was applying is left with none.
            let (settled, reopened) = match answers {
                Some(answers) => {
                    let why = format!("change {} was deleted before it was bottled", i.r#ref.id);
                    let (settled, more) = ops::settle(state.store_mut(), &answers, ops::Settling::Gone(&why));
                    if !settled.is_empty() {
                        changed = Changed::Many(vec![changed, more]);
                    }
                    let (s, r) = summaries(state, &settled);
                    (Some(s), Some(r))
                }
                None => (None, None),
            };
            let g = state.graph_for(view.as_deref())?;
            let mut warnings = Vec::new();
            for from in pointing {
                warnings.extend(lint::dangling_from(g, &from).into_iter().filter(|f| f.edge.as_ref().is_some_and(|e| e.to == i.r#ref)));
            }
            let (warnings, baselined) = split_new(warnings);
            answer(name, Deleted { deleted: i.r#ref, warnings, baselined, settled, reopened }, Some(changed))
        }
        other => Err(ToolError::invalid(format!("unknown tool `{other}`"), vec![])),
    }
}

/// Answer a write with the node's summary as it now is in `view`: the proposal a
/// change of intent went into, or the truth for a ticket's own record.
fn written(name: &str, state: &mut State, view: Option<&str>, r: &Reference, changed: Changed) -> Result<Outcome, ToolError> {
    let g = state.graph_for(view)?;
    let node = query::summary_at(g, r).expect("just written");
    let (warnings, baselined) = split_new(lint::written(g, r));
    answer(name, Written { node, warnings, baselined, settled: None, reopened: None, reprioritised: None, drafted: None }, Some(changed))
}

/// A ticket write's findings, judged over the branch `lint` judges with no
/// ticket: the change the edit waits in, else the current ticket's proposal
/// when there is one, else the truth, so a warning agrees with the check
/// that runs where the ticket is read. The ticket written is judged whatever
/// the reading, so a mistyped reference is said to whoever typed it.
fn ticket_warnings(state: &mut State, under: Option<&str>, r: &Reference) -> (Vec<lint::Finding>, usize) {
    let view = under.map(str::to_string).or_else(|| state.current());
    // The write is made by now, so an error here would leave memory ahead of the disk; none can come.
    let g = state.graph_for(view.as_deref()).expect("the ticket read is brewing or simmering: the store refuses any other, and a settle that closes it clears it");
    split_new(lint::written(g, r))
}

/// The change an edit to `r`'s references waits in: the ticket given, else
/// the current one, when it is a change being worked and not `r` itself. A
/// ticket's own references, and those edited under a gap or question or
/// under no ticket, apply at once. A ticket given that does not exist is
/// not_found, and one not being worked a conflict.
fn proposing_for(state: &State, r: &Reference, given: Option<&str>, refs: bool) -> Result<Option<String>, ToolError> {
    if let Some(g) = given {
        match state.store.tickets.get(g) {
            None => return Err(ToolError::not_found(&Reference::ticket(g))),
            Some(t) if !t.is_worked() => return Err(ToolError::conflict(format!("ticket:{g} is {}, not being worked: an edit waits only in a ticket being worked", t.status.name()))),
            _ => {}
        }
    }
    let under = given.map(str::to_string).or_else(|| state.current());
    Ok(under.filter(|u| refs && *u != r.id && state.store.tickets.get(u).is_some_and(|t| t.kind == TicketKind::Change && t.is_worked())))
}

/// What a change relies on, which includes the answers it applies; none for
/// any other node, whose edit or deletion settles no answer.
fn applies(state: &State, r: &Reference) -> Option<Vec<Reference>> {
    state.store.tickets.get(&r.id).filter(|t| r.kind == Kind::Ticket && t.kind == TicketKind::Change).map(|t| t.relies_on.clone())
}

/// Tickets by slug, as summaries.
fn summaries_of(state: &State, slugs: &[String]) -> Vec<query::Summary> {
    slugs.iter().filter_map(|s| state.store.tickets.get(s)).map(query::ticket_summary).collect()
}

/// A settle's closed and reopened answers, as summaries.
fn summaries(state: &State, settled: &ops::Settled) -> (Vec<query::Summary>, Vec<query::Summary>) {
    (summaries_of(state, &settled.bottled), summaries_of(state, &settled.reopened))
}

/// The new findings, which are the caller's to act on, and how many
/// baselined ones were left out: inherited work, listed by `lint`.
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
        let out = call(&mut s, "inscribe_ticket", json!({ "title": "New", "kind": "change", "priority": "high", "touches": ["page:nope"], "body": "x" }), "2026-09-27").unwrap();
        assert_eq!(out.output["warnings"].as_array().unwrap().len(), 1);
        assert!(matches!(out.changed, Some(Changed::Ticket(ref s)) if s == "0001"));
        let got = call(&mut s, "get", json!({ "ref": "ticket:0001" }), "2026-09-27").unwrap();
        assert_eq!(got.output["node"]["status"], "inscribed");
        // Edges carry titles so an id is never read alone.
        assert_eq!(got.output["node"]["out"][0]["ref"], "page:nope");
        assert!(got.output["node"]["out"][0]["title"].is_null(), "a dangling target has no title");
        let page = call(&mut s, "get", json!({ "ref": "page:git-readiness" }), "2026-09-27").unwrap();
        assert!(page.output["node"]["in"].as_array().unwrap().iter().any(|e| e["ref"] == "ticket:phase-62" && e["title"] == "PHASE-62"));
        let board = call(&mut s, "board", json!({}), "2026-09-27").unwrap();
        assert!(board.output["tickets"].as_array().unwrap().iter().any(|t| t["ref"] == "ticket:0001"));
        let e = call(&mut s, "inscribe_ticket", json!({ "slug": "x", "title": "New", "kind": "change", "priority": "high", "body": "x" }), "2026-09-27").unwrap_err();
        assert!(e.to_string().contains("no longer takes a slug"));
    }

    /// # Test contract
    ///
    /// ```json
    /// {"test": "a-requirement-is-patched-through-the-tools", "checks": ["crates/core/src/tools/write#intent-writes-proposed", "crates/core/src/tools/write#answers-warnings"]}
    /// ```
    #[test]
    fn a_requirement_is_patched_through_the_tools() {
        let mut s = state();
        let truth = s.store().requirements["ready-only-with-commit"].clone();
        let out = call(
            &mut s,
            "patch_requirement",
            json!({ "ref": "req:ready-only-with-commit", "put_scenarios": [{ "title": "Another", "given": "g", "when": "w", "then": "t" }], "add_covered_by": ["criterion:src/a/ready#gone"], "ticket": "later-1" }),
            "2026-10-08",
        )
        .unwrap()
        .output;
        // Its summary, and the dangling edge it made as a warning.
        assert_eq!(out["node"], json!({ "ref": "req:ready-only-with-commit", "title": truth.title }));
        assert!(out["warnings"].as_array().unwrap().iter().any(|w| w["edge"]["to"] == "criterion:src/a/ready#gone"), "{out}");
        // Into the ticket's proposal; the truth as it was.
        assert_eq!(s.store().requirements["ready-only-with-commit"], truth);
        let got = call(&mut s, "get", json!({ "ref": "req:ready-only-with-commit", "ticket": "later-1" }), "2026-10-08").unwrap().output;
        assert_eq!(got["node"]["scenarios"].as_array().unwrap().last().unwrap()["title"], "Another");
        let texts: Vec<&str> = got["node"]["decisions"].as_array().unwrap().iter().map(|d| d["text"].as_str().unwrap()).collect();
        assert_eq!(texts, truth.decisions.iter().map(|d| d.text.as_str()).collect::<Vec<_>>(), "its decisions untouched");
        // Taken only as a write that changes intent: refused with no ticket at all.
        s.store_mut().current = None;
        assert_eq!(call(&mut s, "patch_requirement", json!({ "ref": "req:ready-only-with-commit", "title": "T" }), "2026-10-08").unwrap_err().code(), "invalid");
    }

    /// # Test contract
    ///
    /// ```json
    /// {"test": "a-body-is-edited-in-part-through-the-tools", "checks": ["crates/core/src/ops#patch-body", "crates/core/src/ops/tickets#update-ticket"]}
    /// ```
    #[test]
    fn a_body_is_edited_in_part_through_the_tools() {
        let mut s = state();
        let truth = s.store().pages["git-readiness"].body.clone();
        // Under a ticket, the edit reads the body as its proposal holds it, and the truth is left alone.
        call(&mut s, "patch_page", json!({ "ref": "page:git-readiness", "body": "One.\n\nTwo.", "ticket": "later-1" }), "2026-10-08").unwrap();
        let out = call(&mut s, "patch_page", json!({ "ref": "page:git-readiness", "replace": { "old": "Two.", "new": "Three." }, "append": "Four.", "ticket": "later-1" }), "2026-10-08").unwrap();
        assert_eq!(out.output["node"]["ref"], "page:git-readiness");
        let got = call(&mut s, "get", json!({ "ref": "page:git-readiness", "ticket": "later-1" }), "2026-10-08").unwrap().output;
        assert_eq!(got["node"]["body"], "One.\n\nThree.\n\nFour.");
        assert_eq!(s.store().pages["git-readiness"].body, truth);
        let e = call(&mut s, "patch_page", json!({ "ref": "page:git-readiness", "replace": { "old": "Two.", "new": "x" }, "ticket": "later-1" }), "2026-10-08").unwrap_err();
        assert_eq!(e.code(), "invalid");
        assert!(e.to_string().contains("not in the body"));
        // A ticket's body, at once.
        let before = s.store().tickets["phase-62"].clone();
        let out = call(&mut s, "update_ticket", json!({ "ref": "ticket:phase-62", "append": "The options." }), "2026-10-08").unwrap();
        assert!(out.output["node"].get("body").is_none(), "the ticket's summary, never its body: {}", out.output);
        let body = s.store().tickets["phase-62"].body.clone();
        assert!(body.ends_with("\n\nThe options.") || (before.body.is_empty() && body == "The options."), "{body}");
        assert_eq!(s.store().tickets["phase-62"].touches, before.touches);
        let e = call(&mut s, "update_ticket", json!({ "ref": "ticket:phase-62", "body": "Whole.", "replace": { "old": "The options.", "new": "x" } }), "2026-10-08").unwrap_err();
        assert_eq!(e.code(), "invalid");
        assert_eq!(s.store().tickets["phase-62"].body, body);
    }

    /// # Test contract
    ///
    /// ```json
    /// {"test": "a-ticket-write-warns-as-lint-would", "checks": ["crates/core/src/tools/write#answers-warnings"]}
    /// ```
    #[test]
    fn a_ticket_write_warns_as_lint_would() {
        let mut s = state();
        let scenario = json!([{ "title": "T", "given": "g", "when": "w", "then": "t" }]);
        call(&mut s, "put_requirement", json!({ "slug": "only-proposed", "title": "P", "statement": "It SHALL hold.", "scenarios": scenario, "covered_by": [], "ticket": "later-1" }), "2026-10-04")
            .unwrap();
        let file = |s: &mut State| {
            let out =
                call(s, "inscribe_ticket", json!({ "title": "Names it", "kind": "change", "priority": "later", "touches": ["req:only-proposed", "req:nowhere"], "body": "x" }), "2026-10-04").unwrap();
            let warned: Vec<String> = out.output["warnings"].as_array().unwrap().iter().map(|w| w["edge"]["to"].as_str().unwrap_or_default().to_string()).collect();
            let lint = call(s, "lint", json!({ "kinds": ["dangling"] }), "2026-10-04").unwrap();
            let slug = out.output["node"]["ref"].as_str().unwrap().trim_start_matches("ticket:").to_string();
            let flagged: Vec<String> =
                lint.output["findings"].as_array().unwrap().iter().filter(|f| f["ref"] == format!("ticket:{slug}")).map(|f| f["edge"]["to"].as_str().unwrap_or_default().to_string()).collect();
            let sorted = |mut v: Vec<String>| {
                v.sort();
                v
            };
            (sorted(warned), sorted(flagged))
        };
        // With the proposing ticket current, only what nothing holds dangles. The write warns of it at once;
        // the check leaves the filed ticket's references to whoever reads it, since they are its proposal state.
        s.store_mut().current = Some("later-1".into());
        let (warned, flagged) = file(&mut s);
        assert_eq!(warned, vec!["req:nowhere".to_string()]);
        assert!(flagged.is_empty(), "no check here reads the filed ticket: {flagged:?}");
        // With no current ticket the write judges the truth, which holds neither.
        s.store_mut().current = None;
        let (warned, flagged) = file(&mut s);
        assert_eq!(warned, vec!["req:nowhere".to_string(), "req:only-proposed".to_string()]);
        assert!(flagged.is_empty());
        // update_ticket of another ticket under the current change judges over that change's proposal, where the edit waits,
        // and so does the check here, since the proposal edits that ticket.
        s.store_mut().current = Some("later-1".into());
        let out = call(&mut s, "update_ticket", json!({ "ref": "ticket:0002", "touches": ["req:only-proposed", "req:nowhere"] }), "2026-10-04").unwrap();
        let warned: Vec<&str> = out.output["warnings"].as_array().unwrap().iter().map(|w| w["edge"]["to"].as_str().unwrap_or_default()).collect();
        assert_eq!(warned, vec!["req:nowhere"]);
        let lint = call(&mut s, "lint", json!({ "kinds": ["dangling"] }), "2026-10-04").unwrap();
        let flagged: Vec<&str> = lint.output["findings"].as_array().unwrap().iter().filter(|f| f["ref"] == "ticket:0002").map(|f| f["edge"]["to"].as_str().unwrap_or_default()).collect();
        assert_eq!(flagged, warned, "the write warns of what the check here reports");
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
    /// {"test": "put-requirement-is-checked-whole-and-warns-of-a-dangling-edge", "checks": ["crates/core/src/ops#validate-first", "crates/core/src/tools/write#answers-warnings", "req:tool-put-requirement"]}
    /// ```
    #[test]
    fn put_requirement_is_checked_whole_and_warns_of_a_dangling_edge() {
        let mut s = state();
        let before = s.store().requirements.len();
        let input = json!({ "slug": "new-rule", "title": "New", "statement": "", "scenarios": [{ "title": "T", "given": "g", "when": "w", "then": "" }], "covered_by": [], "ticket": "later-1" });
        let err = call(&mut s, "put_requirement", input, "2026-09-27").unwrap_err();
        assert_eq!(err.code(), "invalid");
        let ToolError::Invalid { details, .. } = &err else { panic!("{err:?}") };
        let fields: Vec<&str> = details.iter().map(|d| d.field.as_str()).collect();
        assert!(fields.contains(&"statement") && fields.iter().any(|f| f.ends_with("then")), "both fields named: {fields:?}");
        assert_eq!(s.store().requirements.len(), before, "nothing is written");
        assert!(s.store().proposals.values().all(|p| !p.requirements.contains_key("new-rule")), "nor proposed");
        let scenario = json!([{ "title": "T", "given": "g", "when": "w", "then": "t" }]);
        let input =
            json!({ "slug": "new-rule", "title": "New", "statement": "It SHALL hold.", "scenarios": scenario, "covered_by": ["criterion:crates/nowhere/src/lib#nothing"], "ticket": "later-1" });
        let out = call(&mut s, "put_requirement", input, "2026-09-27").unwrap();
        let w = out.output["warnings"].as_array().unwrap();
        assert!(w.iter().any(|f| f["rule"] == "dangling" && f["ref"] == "req:new-rule"), "the dangling edge: {w:?}");
    }

    /// # Test contract
    ///
    /// ```json
    /// {"test": "adopt-baselines-what-is-present", "checks": ["crates/core/src/ops#adopt", "crates/core/src/tools/read#status", "crates/core/src/tools/read#lint-tool", "crates/core/src/model#baseline-record", "crates/core/src/tools/write#answers-adopt"]}
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
        // Fix the one unspecified requirement, under the brewing ticket, and bottle it by closing.
        let scenario = json!([{ "title": "T", "given": "g", "when": "w", "then": "t" }]);
        call(
            &mut s,
            "put_requirement",
            json!({ "slug": "stale", "title": "Stale", "statement": "It SHALL.", "scenarios": scenario, "covered_by": ["criterion:src/a/ready#gone"], "ticket": "later-1" }),
            "2026-09-28",
        )
        .unwrap();
        assert!(s.prune_baseline().is_none(), "a proposal fixes nothing in the truth yet");
        call(&mut s, "close_ticket", json!({ "ref": "ticket:later-1", "decision": { "text": "Fixed." } }), "2026-09-28").unwrap();
        assert!(matches!(s.prune_baseline(), Some(Changed::Baseline)));
        // Break it again on a started ticket: lint with no ticket judges the current one, and it is new.
        call(&mut s, "brew_ticket", json!({ "ref": "ticket:phase-62" }), "2026-09-28").unwrap();
        call(&mut s, "put_requirement", json!({ "slug": "stale", "title": "Stale", "statement": "It SHALL.", "scenarios": [], "covered_by": ["criterion:src/a/ready#gone"] }), "2026-09-28").unwrap();
        let lint = call(&mut s, "lint", json!({ "kinds": ["unspecified"] }), "2026-09-28").unwrap().output;
        assert_eq!(lint["new"], 1);
        assert_eq!(lint["findings"][0]["baselined"], false);
    }

    /// # Test contract
    ///
    /// ```json
    /// {"test": "configure-changes-only-what-it-is-given", "checks": ["crates/core/src/ops#configure", "crates/core/src/model/config#config-statement-limit"]}
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
        // The pair changes only the root it gives; a list, as older stores held, names both.
        call(&mut s, "configure", json!({ "roots": { "rites": "orphan" } }), "2026-09-28").unwrap();
        assert_eq!(s.store().config.roots, crate::model::Roots::new("overview", "orphan"));
        call(&mut s, "configure", json!({ "roots": ["orphan"] }), "2026-09-28").unwrap();
        assert_eq!(s.store().config.roots, crate::model::Roots { grimoire: Some("orphan".into()), rites: None }, "a list's missing second is a rites missing");
        // A line limit is set alone, the others left as they were.
        call(&mut s, "configure", json!({ "large_file": 600 }), "2026-09-28").unwrap();
        assert_eq!((s.store().config.large_file, s.store().config.plain_file), (Some(600), None));
        call(&mut s, "configure", json!({ "plain_file": 4000 }), "2026-09-28").unwrap();
        assert_eq!((s.store().config.large_file, s.store().config.plain_file), (Some(600), Some(4000)));
        // The statement limit is set alone, the line limits left as they were.
        call(&mut s, "configure", json!({ "long_statement": 200 }), "2026-09-28").unwrap();
        assert_eq!((s.store().config.long_statement, s.store().config.large_file, s.store().config.plain_file), (Some(200), Some(600), Some(4000)));
        // The recipe kinds are replaced whole, and leave the rest alone.
        call(&mut s, "configure", json!({ "recipe_kinds": { "component": { "uses": ["component", "token"] }, "token": {} } }), "2026-09-28").unwrap();
        assert_eq!(s.store().config.recipe_kinds.keys().collect::<Vec<_>>(), ["component", "token"]);
        let out = call(&mut s, "configure", json!({ "recipe_kinds": { "quest": { "uses": [] } } }), "2026-09-28").unwrap();
        assert_eq!(out.output["config"]["recipe_kinds"], json!({ "quest": { "uses": [] } }));
        assert_eq!(s.store().config.languages, ["rust", "typescript"]);
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
        let one = call(&mut s, "configure", json!({ "roots": { "rites": "overview" } }), "2026-09-28").unwrap_err();
        let detail = &serde_json::to_value(&one).unwrap()["details"][0];
        assert!(detail["field"] == "roots" && detail["reason"].as_str().unwrap().contains("two pages"), "the grimoire and the rites are two pages: {detail}");
        let prefix = call(&mut s, "configure", json!({ "scope": { "include": ["../outside"] } }), "2026-09-28").unwrap_err();
        assert!(serde_json::to_value(&prefix).unwrap()["details"][0]["field"] == "scope.include");
        let zero = call(&mut s, "configure", json!({ "plain_file": 0 }), "2026-09-28").unwrap_err();
        assert!(serde_json::to_value(&zero).unwrap()["details"][0]["field"] == "plain_file", "{zero}");
        let kinds = call(&mut s, "configure", json!({ "recipe_kinds": { "component": { "uses": ["token"] } } }), "2026-09-28").unwrap_err();
        assert!(serde_json::to_value(&kinds).unwrap()["details"][0]["field"] == "recipe_kinds", "a use of a kind not declared");
        assert_eq!(s.store().config, before, "nothing changed");
    }

    /// # Test contract
    ///
    /// ```json
    /// {"test": "widening-the-scope-makes-new-findings", "checks": ["crates/core/src/tools/read#status", "crates/core/src/tools/write#answers-configure"]}
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
    /// {"test": "inherited-findings-are-counted", "checks": ["crates/core/src/tools/write#answers-warnings", "crates/core/src/tools/write#answers-close"]}
    /// ```
    #[test]
    fn inherited_findings_are_counted_not_handed_over() {
        let mut s = state();
        call(&mut s, "adopt", json!({ "reason": "adopted" }), "2026-09-28").unwrap();
        let out = call(&mut s, "close_ticket", json!({ "ref": "ticket:phase-62", "decision": { "text": "Done." } }), "2026-09-28").unwrap().output;
        assert_eq!(out["findings"], json!([]), "everything on the touched nodes is inherited");
        assert!(out["baselined"].as_u64().unwrap() > 0);
        let w = call(
            &mut s,
            "put_requirement",
            json!({ "slug": "stale", "title": "Stale", "statement": "It SHALL.", "scenarios": [], "covered_by": ["criterion:src/a/ready#gone"], "ticket": "later-1" }),
            "2026-09-28",
        )
        .unwrap()
        .output;
        assert_eq!(w["warnings"], json!([]), "stale was already dangling and unspecified when adopted");
        assert_eq!(w["baselined"], 2);
        let fresh = call(&mut s, "put_requirement", json!({ "slug": "fresh", "title": "Fresh", "statement": "It SHALL.", "scenarios": [], "covered_by": [], "ticket": "later-1" }), "2026-09-28")
            .unwrap()
            .output;
        assert_eq!(fresh["warnings"][0]["check"], "no-scenario", "new work is handed over");
        assert_eq!(fresh["baselined"], 0);
    }

    /// # Test contract
    ///
    /// ```json
    /// {"test": "an-omen-reaches-no-baseline-warning-or-close", "checks": ["crates/core/src/lint#omens"]}
    /// ```
    #[test]
    fn an_omen_reaches_no_baseline_warning_or_close() {
        let mut s = state();
        // Every fixture file has 40 lines: at a limit of 40, each file in scope is an omen.
        s.store_mut().config.large_file = Some(40);
        let omens = call(&mut s, "lint", json!({}), "2026-10-06").unwrap().output["omens"].as_array().unwrap().len();
        assert!(omens > 0, "the fixture has omens");
        let file = |v: &serde_json::Value| v.as_array().unwrap().iter().any(|f| f["ref"].as_str().unwrap_or("").starts_with("file:"));
        call(&mut s, "adopt", json!({ "reason": "adopted" }), "2026-10-06").unwrap();
        assert!(!s.store().baseline.as_ref().unwrap().findings.iter().any(|k| k.r#ref.kind == Kind::File), "no baseline holds an omen");
        // A write and a close on the module whose file is an omen hand none over.
        let w = call(&mut s, "update_ticket", json!({ "ref": "ticket:phase-62", "touches": ["module:src/a/ready", "file:src/a/ready.rs"] }), "2026-10-06").unwrap().output;
        assert!(!file(&w["warnings"]), "{w}");
        let out = call(&mut s, "close_ticket", json!({ "ref": "ticket:phase-62", "decision": { "text": "Done." } }), "2026-10-06").unwrap().output;
        assert!(!file(&out["findings"]), "{out}");
        assert_eq!(call(&mut s, "lint", json!({}), "2026-10-06").unwrap().output["omens"].as_array().unwrap().len(), omens, "still reported, apart");
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
        assert_eq!(out["node"], json!({ "ref": "req:ready-only-with-commit", "title": s.store().requirements["ready-only-with-commit"].title }), "the requirement's summary");
        let got = call(&mut s, "get", json!({ "ref": "req:ready-only-with-commit", "ticket": "later-1" }), "2026-09-28").unwrap().output;
        let last = got["node"]["decisions"].as_array().unwrap().last().unwrap().clone();
        assert_eq!(last, json!({ "id": null, "date": "2026-09-28", "text": "Why." }), "waiting in the ticket, with no id yet");
        // Kept apart in the proposal: the truth has no new decision until the ticket closes, and no copy of the record is proposed.
        assert_eq!(s.store().requirements["ready-only-with-commit"].decisions.len(), 1);
        assert!(!s.store().proposals["later-1"].requirements.contains_key("ready-only-with-commit"));
        // Superseding names ids, as numbers or as they are said; one the requirement does not hold is refused.
        call(&mut s, "record_decision", json!({ "ref": "req:ready-only-with-commit", "text": "Instead.", "supersedes": ["d1"], "ticket": "later-1" }), "2026-09-28").unwrap();
        let got = call(&mut s, "get", json!({ "ref": "req:ready-only-with-commit", "ticket": "later-1" }), "2026-09-28").unwrap().output;
        assert_eq!(got["node"]["decisions"][2]["supersedes"], json!([1]));
        assert_eq!(
            call(&mut s, "record_decision", json!({ "ref": "req:ready-only-with-commit", "text": "x", "supersedes": ["d2"], "ticket": "later-1" }), "2026-09-28").unwrap_err().code(),
            "invalid",
            "d2 has not been bottled"
        );
        assert_eq!(
            call(&mut s, "record_decision", json!({ "ref": "req:ready-only-with-commit", "text": "x", "supersedes": ["two"], "ticket": "later-1" }), "2026-09-28").unwrap_err().code(),
            "invalid"
        );
        call(&mut s, "close_ticket", json!({ "ref": "ticket:later-1", "decision": { "text": "Done." } }), "2026-09-29").unwrap();
        let bottled = &s.store().requirements["ready-only-with-commit"].decisions;
        assert_eq!((bottled[1].id, bottled[1].date.as_str(), bottled[1].text.as_str()), (Some(2), "2026-09-28", "Why."), "it keeps the date it was recorded and takes the next id");
        assert_eq!((bottled[2].id, bottled[2].supersedes.clone()), (Some(3), vec![1]));
    }

    /// # Test contract
    ///
    /// ```json
    /// {"test": "reorder-sets-one-band", "checks": ["crates/core/src/ops#reorder"]}
    /// ```
    #[test]
    fn reorder_sets_one_band() {
        let mut s = state();
        // phase-62 is next and inscribed; later-1 is later and brewing; old is bottled.
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
    /// {"test": "a-rename-keeps-decisions-and-rewrites-references", "checks": ["crates/core/src/ops/rename#rename-moves", "crates/core/src/ops/rename#rename-rewrites"]}
    /// ```
    #[test]
    fn a_rename_keeps_decisions_and_rewrites_references() {
        let mut s = state();
        // ready-only-with-commit: one dated decision, linked from git-readiness,
        // touched by the inscribed ticket phase-62; and a bottled ticket names it too.
        call(&mut s, "inscribe_ticket", json!({ "title": "Old", "kind": "change", "priority": "later", "touches": ["req:ready-only-with-commit"], "body": "b" }), "2026-09-28").unwrap();
        call(&mut s, "brew_ticket", json!({ "ref": "ticket:0001" }), "2026-09-28").unwrap();
        call(&mut s, "close_ticket", json!({ "ref": "ticket:0001", "decision": { "text": "Done." } }), "2026-09-28").unwrap();
        call(&mut s, "brew_ticket", json!({ "ref": "ticket:phase-62" }), "2026-09-28").unwrap();
        call(&mut s, "adopt", json!({ "reason": "adopted" }), "2026-09-28").unwrap();
        let before = s.store().requirements["ready-only-with-commit"].clone();
        let out = call(&mut s, "rename_node", json!({ "ref": "req:ready-only-with-commit", "to": "ready-with-a-commit" }), "2026-09-28").unwrap();
        assert_eq!(out.output["node"]["ref"], "req:ready-with-a-commit");
        let after = &s.store().requirements["ready-with-a-commit"];
        assert_eq!(after.decisions, before.decisions, "decisions keep their dates");
        assert!(!s.store().requirements.contains_key("ready-only-with-commit"));
        assert!(s.store().pages["git-readiness"].links.contains(&Reference::req("ready-with-a-commit")));
        assert!(s.store().tickets["phase-62"].touches.contains(&Reference::req("ready-with-a-commit")), "an inscribed ticket follows");
        assert!(s.store().tickets["0001"].touches.contains(&Reference::req("ready-only-with-commit")), "a bottled ticket is history");
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
        assert_eq!(s.store().config.roots.grimoire.as_deref(), Some("home"));
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

    /// # Test contract
    ///
    /// ```json
    /// {"test": "delete-reports-new-dangling", "checks": ["crates/core/src/tools/write#answers-delete"]}
    /// ```
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
    /// {"test": "retire-answers-with-what-it-leaves-uncovered", "checks": ["crates/core/src/tools/write#answers-retire"]}
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
        assert_eq!(out["node"]["ref"], "req:ready-only-with-commit");
        let got = call(&mut s, "get", json!({ "ref": "req:ready-only-with-commit", "ticket": "later-1" }), "2026-09-29").unwrap().output;
        assert_eq!(got["node"]["retired"], true);
        assert_eq!(got["node"]["decisions"].as_array().map(Vec::len), Some(2));
        assert_eq!(got["node"]["decisions"][1]["supersedes"], json!([1]));
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
    /// {"test": "a-write-goes-into-its-ticket-and-the-close-applies-it", "checks": ["crates/core/src/tools/write#intent-writes-proposed", "crates/core/src/tools/write#close-applies-or-conflicts", "crates/core/src/tools/write#answers-summary"]}
    /// ```
    #[test]
    fn a_write_goes_into_its_ticket_and_the_close_applies_it() {
        let mut s = state();
        let code = |r: Result<Outcome, ToolError>| r.unwrap_err().code();
        let truth = s.store().pages["orphan"].clone();
        let orphan = |body: &str| json!({ "slug": "orphan", "title": "Orphan", "body": body, "links": [], "ticket": "later-1" });
        let out = call(&mut s, "put_page", orphan("proposed"), "2026-10-02").unwrap();
        assert_eq!(out.output["node"], json!({ "ref": "page:orphan", "title": "Orphan" }), "the page's summary, as the proposal holds it");
        assert!(matches!(out.changed, Some(Changed::Proposed(ref t)) if t == "later-1"));
        assert_eq!(s.store().pages["orphan"], truth, "the truth is untouched");
        // Without a ticket and with none current: refused, naming the field.
        let mut bare = orphan("x");
        bare.as_object_mut().unwrap().remove("ticket");
        assert_eq!(code(call(&mut s, "put_page", bare.clone(), "2026-10-02")), "invalid");
        // With a current ticket, it is bottled there.
        call(&mut s, "brew_ticket", json!({ "ref": "ticket:phase-62" }), "2026-10-02").unwrap();
        call(&mut s, "put_page", bare, "2026-10-02").unwrap();
        assert_eq!(s.store().proposals["phase-62"].pages["orphan"].record.body, "x");
        // No other write takes the field but update_ticket, nor does deleting a ticket.
        assert_eq!(code(call(&mut s, "inscribe_ticket", json!({ "title": "T", "kind": "gap", "priority": "later", "body": "b", "ticket": "later-1" }), "2026-10-02")), "invalid");
        assert_eq!(code(call(&mut s, "rename_node", json!({ "ref": "page:orphan", "to": "o", "ticket": "later-1" }), "2026-10-02")), "invalid");
        assert_eq!(code(call(&mut s, "delete_node", json!({ "ref": "ticket:gap-1", "ticket": "later-1" }), "2026-10-02")), "invalid");
        assert_eq!(code(call(&mut s, "close_ticket", json!({ "ref": "ticket:phase-62", "decision": { "text": "Done." }, "ticket": "later-1" }), "2026-10-02")), "invalid");
        assert!(s.store().tickets.contains_key("gap-1") && s.store().tickets["phase-62"].status == Status::Brewing);
        // The truth moves under later-1's proposal: its close is a conflict naming the record.
        ops::put_page(s.store_mut(), Page { body: "moved".into(), ..truth.clone() }).unwrap();
        let e = call(&mut s, "close_ticket", json!({ "ref": "ticket:later-1", "decision": { "text": "Done." } }), "2026-10-02").unwrap_err();
        let v = serde_json::to_value(&e).unwrap();
        assert_eq!((v["error"].clone(), v["details"][0]["field"].clone()), (json!("conflict"), json!("page:orphan")));
        assert_eq!(s.store().tickets["later-1"].status, Status::Brewing, "not bottled");
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
    /// {"test": "an-edit-drafted-into-a-change-says-where-it-went", "checks": ["crates/core/src/tools/write#answers-drafted", "req:tool-update-ticket", "req:write-names-ticket"]}
    /// ```
    #[test]
    fn an_edit_drafted_into_a_change_says_where_it_went() {
        let mut s = state();
        let day = "2026-10-05";
        // No change being worked: the edit applies at once and the answer says nothing of drafts.
        let out = call(&mut s, "update_ticket", json!({ "ref": "ticket:phase-62", "touches": ["page:orphan"] }), day).unwrap();
        assert!(out.output.get("drafted").is_none(), "{}", out.output);
        // later-1 current: its own references apply at once, with nothing said.
        s.store_mut().current = Some("later-1".into());
        let own = call(&mut s, "update_ticket", json!({ "ref": "ticket:later-1", "touches": ["page:orphan"] }), day).unwrap();
        assert!(own.output.get("drafted").is_none(), "{}", own.output);
        // Another ticket's references wait in later-1's proposal, and the answer names it and when they apply.
        let out = call(&mut s, "update_ticket", json!({ "ref": "ticket:phase-62", "relies_on": ["page:git-readiness"] }), day).unwrap();
        assert_eq!(out.output["drafted"]["change"]["ref"], "ticket:later-1", "{}", out.output);
        let note = out.output["drafted"]["note"].as_str().unwrap();
        assert_eq!(note, "the edit to ticket:phase-62's references waits in ticket:later-1's proposal and reaches ticket:phase-62 when ticket:later-1 is bottled");
        // A title or a body alone applies at once, so nothing is drafted.
        for edit in [json!({ "ref": "ticket:phase-62", "title": "Renamed" }), json!({ "ref": "ticket:phase-62", "body": "A new body." })] {
            let out = call(&mut s, "update_ticket", edit, day).unwrap();
            assert!(out.output.get("drafted").is_none(), "{}", out.output);
        }
        // A change named in `ticket`, the current one being none: the edit waits there, and the answer names it.
        s.store_mut().current = None;
        let named = call(&mut s, "update_ticket", json!({ "ref": "ticket:phase-62", "touches": ["page:overview"], "ticket": "later-1" }), day).unwrap();
        assert_eq!(named.output["drafted"]["change"]["ref"], "ticket:later-1", "{}", named.output);
        // Under a gap being worked an edit applies at once, and nothing is drafted.
        s.store_mut().tickets.get_mut("gap-1").unwrap().status = Status::Brewing;
        let gap = call(&mut s, "update_ticket", json!({ "ref": "ticket:phase-62", "touches": ["page:orphan"], "ticket": "gap-1" }), day).unwrap();
        assert!(gap.output.get("drafted").is_none(), "{}", gap.output);
        // A refused edit answers an error, and its change's proposal is as it was.
        let held = s.store().proposals.get("later-1").cloned();
        assert!(call(&mut s, "update_ticket", json!({ "ref": "ticket:phase-62", "touches": ["page:orphan"], "ticket": "nowhere" }), day).is_err());
        assert_eq!(s.store().proposals.get("later-1").cloned(), held);
    }

    /// # Test contract
    ///
    /// ```json
    /// {"test": "an-edit-to-another-ticket-waits-in-its-change", "checks": ["crates/core/src/tools/write#proposes-another-tickets-references", "crates/core/src/tools/write#both-lists-rest-on-truth", "crates/core/src/tools/write#one-list-rests-on-proposal", "crates/core/src/tools/write#answers-as-proposal-leaves-it", "crates/core/src/tools/write#applies-the-rest-at-once", "crates/core/src/tools/write#named-ticket-errors", "crates/core/src/ops/proposal#ticket-references-kept-as-lists", "crates/core/src/ops/proposal#whole-write-rebases", "crates/core/src/ops/proposal#partial-write-refused-when-moved", "crates/core/src/ops/proposal#apply-edited-tickets-take-references", "crates/core/src/tools/write#settles-answers-gone", "crates/core/src/tools/read#ticket-view", "crates/core/src/query/board#board-drafts"]}
    /// ```
    #[test]
    fn an_edit_to_another_ticket_waits_in_its_change() {
        let mut s = state();
        let day = "2026-10-05";
        let code = |r: Result<Outcome, ToolError>| r.unwrap_err().code();
        let refs = |v: &[&str]| v.iter().map(|r| r.parse::<Reference>().unwrap()).collect::<Vec<_>>();
        // Under an answer an edit applies at once, so the change applying it is seen relying on it.
        s.store_mut().tickets.get_mut("gap-1").unwrap().status = Status::Brewing;
        call(&mut s, "update_ticket", json!({ "ref": "ticket:phase-62", "relies_on": ["page:git-readiness", "ticket:gap-1"], "ticket": "gap-1" }), day).unwrap();
        assert_eq!(s.store().tickets["phase-62"].relies_on, refs(&["page:git-readiness", "ticket:gap-1"]));
        assert!(!s.store().proposals.contains_key("gap-1"));
        call(&mut s, "close_ticket", json!({ "ref": "ticket:gap-1", "decision": { "text": "Answered." }, "when": "bottled" }), day).unwrap();
        // A ticket's own references are written on its record.
        s.store_mut().current = Some("later-1".into());
        call(&mut s, "update_ticket", json!({ "ref": "ticket:later-1", "touches": ["page:orphan", "page:overview"] }), day).unwrap();
        assert_eq!(s.store().tickets["later-1"].touches, refs(&["page:orphan", "page:overview"]));
        assert!(s.store().proposals.get("later-1").is_none_or(|p| p.tickets.is_empty()));
        // Another ticket's wait in the current change's proposal; its title changes at once.
        let out = call(&mut s, "update_ticket", json!({ "ref": "ticket:phase-62", "touches": ["page:orphan"], "title": "Renamed" }), day).unwrap();
        assert_eq!(out.output["node"]["title"], "Renamed", "its summary, answered as the change would leave it");
        assert!(out.output["node"].get("touches").is_none(), "never the whole record");
        let truth = s.store().tickets["phase-62"].clone();
        assert_eq!((truth.title.as_str(), truth.touches.clone()), ("Renamed", refs(&["req:ready-only-with-commit", "module:src/a/ready"])));
        assert_eq!(s.store().proposals["later-1"].tickets["phase-62"].record.touches, refs(&["page:orphan"]));
        // Readable everywhere, marked as proposed.
        let got = call(&mut s, "get", json!({ "ref": "ticket:phase-62" }), day).unwrap().output;
        assert_eq!((got["node"]["touches"].clone(), got["drafts"].clone()), (json!(["req:ready-only-with-commit", "module:src/a/ready"]), json!(["ticket:later-1"])));
        assert_eq!(call(&mut s, "get", json!({ "ref": "ticket:phase-62", "ticket": "later-1" }), day).unwrap().output["node"]["touches"], json!(["page:orphan"]));
        let change = call(&mut s, "get", json!({ "ref": "ticket:later-1" }), day).unwrap().output;
        assert_eq!(change["proposal"]["tickets"], json!([{ "ref": "ticket:phase-62", "title": "Renamed", "new": false, "differs": ["touches"] }]));
        let board = call(&mut s, "board", json!({}), day).unwrap().output;
        let row = board["tickets"].as_array().unwrap().iter().find(|t| t["ref"] == "ticket:phase-62").unwrap().clone();
        assert_eq!(row["drafts"], json!(["ticket:later-1"]));
        assert!(board["tickets"].as_array().unwrap().iter().all(|t| t["ref"] == "ticket:phase-62" || t.get("drafts").is_none()));
        // A ticket named that is not being worked is refused; one that does not exist is not found.
        assert_eq!(code(call(&mut s, "update_ticket", json!({ "ref": "ticket:phase-62", "touches": [], "ticket": "old" }), day)), "conflict");
        assert_eq!(code(call(&mut s, "update_ticket", json!({ "ref": "ticket:phase-62", "touches": [], "ticket": "nope" }), day)), "not_found");
        // One list builds on the proposal's copy, refused once the truth moved under it; both rest on the truth as it is now.
        s.store_mut().current = None;
        call(&mut s, "update_ticket", json!({ "ref": "ticket:phase-62", "touches": ["page:overview"] }), day).unwrap();
        assert_eq!(s.store().tickets["phase-62"].touches, refs(&["page:overview"]), "with no ticket, at once");
        s.store_mut().current = Some("later-1".into());
        let e = call(&mut s, "update_ticket", json!({ "ref": "ticket:phase-62", "relies_on": ["page:git-readiness"] }), day).unwrap_err();
        assert!(matches!(&e, ToolError::Conflict { details, .. } if details.iter().any(|d| d.field == "ticket:phase-62")), "{e:?}");
        assert_eq!(code(call(&mut s, "close_ticket", json!({ "ref": "ticket:later-1", "decision": { "text": "Done." } }), day)), "conflict", "the close sees it moved");
        // Refused, the call changes nothing, not even the fields it would have written at once.
        let e = call(&mut s, "update_ticket", json!({ "ref": "ticket:phase-62", "relies_on": [], "title": "Lost" }), day).unwrap_err();
        assert_eq!(e.code(), "conflict");
        assert_eq!(s.store().tickets["phase-62"].title, "Renamed");
        call(&mut s, "update_ticket", json!({ "ref": "ticket:phase-62", "touches": ["page:orphan"], "relies_on": ["page:git-readiness"] }), day).unwrap();
        // The close applies the edit, and the answer it dropped is settled: no change applies it now, so it reopens.
        let closed = call(&mut s, "close_ticket", json!({ "ref": "ticket:later-1", "decision": { "text": "Done." } }), day).unwrap();
        let edited = &s.store().tickets["phase-62"];
        assert_eq!((edited.touches.clone(), edited.relies_on.clone()), (refs(&["page:orphan"]), refs(&["page:git-readiness"])));
        assert_eq!(s.store().tickets["gap-1"].status, Status::Inscribed, "{}", closed.output);
        assert!(matches!(closed.changed, Some(Changed::Many(ref cs)) if cs.iter().any(|c| matches!(c, Changed::Many(m) if m.contains(&Changed::Ticket("phase-62".into()))))));
        assert!(call(&mut s, "get", json!({ "ref": "ticket:phase-62" }), day).unwrap().output.get("drafts").is_none());
    }

    /// # Test contract
    ///
    /// ```json
    /// {"test": "a-change-keeps-pace-with-what-it-relies-on", "checks": ["crates/core/src/ops/tickets#lowered-change-lowers-its-answers", "crates/core/src/ops/tickets#change-below-answer-raised", "crates/core/src/ops/tickets#raised-answer-raises-its-changes", "crates/core/src/ops/tickets#filed-answer-raises-changes-naming-it", "crates/core/src/ops/tickets#bottled-never-moved", "crates/core/src/ops/tickets#lowering-answer-or-raising-change-moves-nothing", "crates/core/src/tools/write#reprioritised", "crates/core/src/tools/write#answers-reprioritised"]}
    /// ```
    #[test]
    fn a_change_keeps_pace_with_what_it_relies_on() {
        let mut s = state();
        let day = "2026-10-04";
        let file = |s: &mut State, kind: &str, priority: &str, relies_on: &[&str]| {
            let out = call(s, "inscribe_ticket", json!({ "title": "T", "kind": kind, "priority": priority, "relies_on": relies_on, "body": "b" }), day).unwrap();
            (out.output["node"]["ref"].as_str().unwrap().to_string(), out.output["reprioritised"].clone())
        };
        let priority = |s: &State, r: &str| s.store().tickets[r.trim_start_matches("ticket:")].priority;
        let moved = |v: &Value| v.as_array().unwrap().iter().map(|x| (x["ref"].as_str().unwrap().to_string(), x["priority"].as_str().unwrap().to_string())).collect::<Vec<_>>();
        // Filed relying on something more urgent: filed at that priority, and said so.
        let (q, none) = file(&mut s, "question", "next", &[]);
        assert_eq!(none, json!([]));
        let (c, out) = file(&mut s, "change", "later", &[&q]);
        assert_eq!((priority(&s, &c), moved(&out)), (Priority::Next, vec![(c.clone(), "next".to_string())]));
        // Coming to rely on something more urgent raises a change.
        let (g, _) = file(&mut s, "gap", "high", &[]);
        let (d, _) = file(&mut s, "change", "later", &[]);
        let out = call(&mut s, "update_ticket", json!({ "ref": d, "relies_on": [g] }), day).unwrap();
        assert_eq!(moved(&out.output["reprioritised"]), vec![(d.clone(), "high".to_string())]);
        // Each ticket moved is persisted after the write's own change.
        let id = |r: &str| r.trim_start_matches("ticket:").to_string();
        assert_eq!(out.changed, Some(Changed::Many(vec![Changed::Ticket(id(&d)), Changed::Many(vec![Changed::Ticket(id(&d))])])));
        // Raising a gap raises the inscribed changes relying on it, and not a bottled one.
        call(&mut s, "update_ticket", json!({ "ref": d, "priority": "later" }), day).unwrap();
        assert_eq!((priority(&s, &d), priority(&s, &g)), (Priority::Later, Priority::Later), "lowering a change lowers what it relies on");
        let (done, _) = file(&mut s, "change", "later", &[&g]);
        call(&mut s, "close_ticket", json!({ "ref": done, "decision": { "text": "Done." } }), day).unwrap();
        let out = call(&mut s, "update_ticket", json!({ "ref": g, "priority": "high" }), day).unwrap();
        assert_eq!(moved(&out.output["reprioritised"]), vec![(d.clone(), "high".to_string())]);
        assert_eq!(priority(&s, &done), Priority::Later, "a bottled change is never moved");
        // Lowering a gap, or raising a change, moves nothing else.
        assert_eq!(call(&mut s, "update_ticket", json!({ "ref": g, "priority": "later" }), day).unwrap().output["reprioritised"], json!([]));
        assert_eq!(priority(&s, &d), Priority::High);
        assert_eq!(call(&mut s, "update_ticket", json!({ "ref": d, "priority": "next" }), day).unwrap().output["reprioritised"], json!([]));
        // Another change holds the question up: the one lowered is raised back to it, and the question stays.
        let out = call(&mut s, "update_ticket", json!({ "ref": c, "relies_on": [q, g] }), day).unwrap();
        assert_eq!(out.output["reprioritised"], json!([]));
        call(&mut s, "update_ticket", json!({ "ref": d, "relies_on": [g, q] }), day).unwrap();
        let out = call(&mut s, "update_ticket", json!({ "ref": d, "priority": "later" }), day).unwrap();
        assert_eq!(moved(&out.output["reprioritised"]), vec![(d.clone(), "next".to_string())], "q stays next, held there by c, so d is raised back to it");
        assert_eq!((priority(&s, &q), priority(&s, &d)), (Priority::Next, Priority::Next));
        // A lowered change takes its answer down with it, but only as far as another change relying on it allows.
        let (a, _) = file(&mut s, "gap", "next", &[]);
        let (lone, _) = file(&mut s, "change", "next", &[&a]);
        let out = call(&mut s, "update_ticket", json!({ "ref": lone, "priority": "later" }), day).unwrap();
        assert_eq!(moved(&out.output["reprioritised"]), vec![(a.clone(), "later".to_string())]);
        call(&mut s, "update_ticket", json!({ "ref": a, "priority": "next" }), day).unwrap();
        let (other, _) = file(&mut s, "change", "high", &[&a]);
        assert_eq!((priority(&s, &lone), priority(&s, &other)), (Priority::Next, Priority::Next));
        call(&mut s, "update_ticket", json!({ "ref": other, "priority": "high" }), day).unwrap();
        assert_eq!((priority(&s, &a), priority(&s, &other)), (Priority::Next, Priority::Next), "lone still holds it at next, so other is raised back");
        // A gap filed under a name a change already relies on raises that change, as one raised does.
        // The change takes the next id, so the gap it names is the one after.
        let next_id = format!("ticket:{:04}", ops::next_ticket_id(s.store()).parse::<u64>().unwrap() + 1);
        let (waiting, _) = file(&mut s, "change", "later", &[&next_id]);
        let (filed, out) = file(&mut s, "gap", "next", &[]);
        assert_eq!(filed, next_id);
        assert_eq!(moved(&out), vec![(waiting.clone(), "next".to_string())]);
        // An edit that moves nothing else answers no list beyond its own.
        let out = call(&mut s, "update_ticket", json!({ "ref": c, "title": "Renamed" }), day).unwrap();
        assert_eq!(out.output["reprioritised"], json!([]));
    }

    /// # Test contract
    ///
    /// ```json
    /// {"test": "a-reopened-answer-is-cut-to-the-body-limit", "checks": ["crates/core/src/ops/tickets#reopens-the-rest", "crates/core/src/ops/tickets#reopened-body-cut-to-limit"]}
    /// ```
    #[test]
    fn a_reopened_answer_is_cut_to_the_body_limit() {
        let mut store = crate::store::Store::default();
        let gap = |body: String| Ticket {
            slug: "g".into(),
            title: "G".into(),
            kind: TicketKind::Gap,
            priority: Priority::Later,
            status: Status::Simmering,
            touches: vec![],
            relies_on: vec![],
            body,
            decision: Some(Decision::new("2026-10-04", "x".repeat(3000))),
        };
        store.tickets.insert("g".into(), gap("y".repeat(3000)));
        let (settled, _) = ops::settle(&mut store, &[Reference::ticket("g")], ops::Settling::Gone("change c was deleted before it was bottled"));
        assert_eq!(settled.reopened, vec!["g".to_string()]);
        let t = &store.tickets["g"];
        assert_eq!((t.body.chars().count(), t.body.ends_with('…')), (crate::store::TICKET_BODY_MAX, true));
        assert!(crate::store::validate_ticket(t).is_empty(), "a reopened answer is a valid inscribed ticket");
    }

    /// # Test contract
    ///
    /// ```json
    /// {"test": "a-state-with-no-code-says-so-and-leaves-the-baseline", "checks": ["crates/core/src/tools/state#without-code"]}
    /// ```
    #[test]
    fn a_state_with_no_code_says_so_and_leaves_the_baseline() {
        // The fixture's store with every finding adopted, then served by a daemon that holds no code.
        let mut s = state();
        call(&mut s, "adopt", json!({ "reason": "inherited" }), "2026-10-03").unwrap();
        let adopted = s.store().baseline.clone().unwrap();
        assert!(adopted.findings.iter().any(|f| f.check == "untested"), "the baseline holds what only code produces");
        let mut shared = State::new(s.store().clone(), crate::extraction::Extraction::from_files(&Vec::<crate::extraction::FileExtraction>::new()), vec![]);
        shared.set_without_code(true);
        // Nothing is pruned: a finding it cannot produce is not thereby fixed.
        assert!(shared.prune_baseline().is_none());
        assert_eq!(shared.store().baseline.as_ref(), Some(&adopted));
        // It says what it is, on the truth and through a ticket's proposal alike.
        assert_eq!(call(&mut shared, "status", json!({}), "2026-10-03").unwrap().output["shared"], true);
        for input in [json!({}), json!({ "ticket": "later-1" })] {
            let lint = call(&mut shared, "lint", input, "2026-10-03").unwrap().output;
            assert_eq!(lint["without_code"], true);
            let checks: Vec<&str> = lint["findings"].as_array().unwrap().iter().map(|f| f["check"].as_str().unwrap()).collect();
            assert!(
                !checks.contains(&"unheld") && !lint["findings"].as_array().unwrap().iter().any(|f| f["edge"]["to"].as_str().is_some_and(|t| t.starts_with("criterion:") || t.starts_with("module:"))),
                "{checks:?}"
            );
        }
        // A close there cannot say which touched nodes never changed: the code is among them and it holds none.
        let closed = call(&mut shared, "close_ticket", json!({ "ref": "ticket:later-1", "decision": { "text": "Done." } }), "2026-10-03").unwrap().output;
        assert_eq!((closed["node"]["status"].clone(), closed["untouched"].clone(), closed["unnamed"].clone()), (json!("bottled"), json!(null), json!(null)));
        // And it will not take what it sees for the whole baseline.
        assert_eq!(call(&mut shared, "adopt", json!({ "reason": "again" }), "2026-10-03").unwrap_err().code(), "invalid");
        assert_eq!(shared.store().baseline.as_ref(), Some(&adopted));
        // A state with the code answers as before.
        assert_eq!(call(&mut s, "status", json!({}), "2026-10-03").unwrap().output["shared"], false);
        assert_eq!(call(&mut s, "lint", json!({}), "2026-10-03").unwrap().output["without_code"], false);
    }
    /// # Test contract
    ///
    /// ```json
    /// {"test": "a-state-serving-a-team-keeps-no-current-ticket", "checks": ["crates/core/src/tools/state#no-shared-current"]}
    /// ```
    #[test]
    fn a_state_serving_a_team_keeps_no_current_ticket() {
        // A store as it arrives on a host, its machine's current ticket with it.
        let mut store = state().store().clone();
        store.current = Some("later-1".into());
        store.starts.insert("later-1".into(), Start { ticket: "later-1".into(), fingerprints: Default::default(), simmering_in: None });
        let mut shared = State::new(store, crate::extraction::Extraction::from_files(&Vec::<crate::extraction::FileExtraction>::new()), vec![]);
        shared.set_without_code(true);
        assert_eq!(shared.current(), None, "dropped: it was one checkout's");
        assert!(shared.store().starts.is_empty(), "and so was its start");
        let page = |ticket: Option<&str>| {
            let mut p = json!({ "slug": "aim", "title": "Aim", "body": "b", "links": [] });
            if let Some(t) = ticket {
                p["ticket"] = json!(t);
            }
            p
        };
        // A start sets none, so a write still has to name its ticket.
        let started = call(&mut shared, "brew_ticket", json!({ "ref": "ticket:phase-62" }), "2026-10-03").unwrap();
        assert_eq!(started.output["node"]["status"], "brewing");
        assert_eq!((shared.current(), shared.store().current.clone()), (None, None));
        assert!(shared.store().starts.is_empty(), "nor a start: the snapshot is the checkout's, with its code");
        assert_eq!(started.changed, Some(Changed::Many(vec![Changed::Ticket("phase-62".into()), Changed::Current])));
        let e = call(&mut shared, "put_page", page(None), "2026-10-03").unwrap_err();
        assert_eq!((e.code(), serde_json::to_value(&e).unwrap()["details"][0]["field"].clone()), ("invalid", json!("ticket")));
        assert!(call(&mut shared, "put_page", page(Some("phase-62")), "2026-10-03").is_ok());
        // And an unnamed bottling bottles nothing there, having no current ticket to bottle.
        call(&mut shared, "close_ticket", json!({ "ref": "ticket:phase-62", "decision": { "text": "Done." }, "when": "bottled" }), "2026-10-03").unwrap();
        assert_eq!(call(&mut shared, "bottle_ticket", json!({}), "2026-10-03").unwrap().output["bottled"], false);
        assert_eq!(call(&mut shared, "bottle_ticket", json!({ "ref": "ticket:phase-62" }), "2026-10-03").unwrap().output["bottled"], true);
    }

    /// # Test contract
    ///
    /// ```json
    /// {"test": "adopt-records-the-findings-it-is-given", "checks": ["crates/core/src/tools/write#adopt-findings"]}
    /// ```
    #[test]
    fn adopt_records_the_findings_it_is_given() {
        // What a machine with the code sees.
        let mut machine = state();
        let seen = call(&mut machine, "lint", json!({}), "2026-10-03").unwrap().output["findings"].as_array().unwrap().clone();
        let keys: Vec<Value> = seen
            .iter()
            .map(|f| {
                let mut k = json!({ "rule": f["rule"], "check": f["check"], "ref": f["ref"] });
                if !f["edge"].is_null() {
                    k["edge"] = f["edge"].clone();
                }
                k
            })
            .collect();
        assert!(seen.iter().any(|f| f["check"] == "untested"), "among them what only code produces");
        // A state with no code takes them as the baseline, and refuses to adopt without them.
        let mut shared = State::new(machine.store().clone(), crate::extraction::Extraction::from_files(&Vec::<crate::extraction::FileExtraction>::new()), vec![]);
        shared.set_without_code(true);
        let e = call(&mut shared, "adopt", json!({ "reason": "inherited" }), "2026-10-03").unwrap_err();
        assert_eq!((e.code(), serde_json::to_value(&e).unwrap()["details"][0]["field"].clone()), ("invalid", json!("findings")));
        let out = call(&mut shared, "adopt", json!({ "reason": "inherited", "findings": keys }), "2026-10-03").unwrap().output;
        assert_eq!(out["baseline"]["count"], seen.len());
        // The same baseline the machine would have recorded for itself.
        call(&mut machine, "adopt", json!({ "reason": "inherited" }), "2026-10-03").unwrap();
        assert_eq!(shared.store().baseline, machine.store().baseline);
        // A finding that is not one is refused.
        assert_eq!(call(&mut shared, "adopt", json!({ "reason": "inherited", "findings": [{ "rule": "nonsense" }] }), "2026-10-03").unwrap_err().code(), "invalid");
    }

    /// The fixture's extraction with a recipe `ready` representing src/a/ready.rs, its digest `digest`.
    fn with_recipe(digest: &str) -> Extraction {
        let mut files = crate::extraction::fixtures::extraction_files();
        let mut recipe = crate::extraction::fixtures::recipe_file("ui/ready.recipe.md", "ready", "component", &[], &["src/a/ready.rs"], &[]);
        if let Some(Ok(r)) = recipe.recipe.as_mut() {
            r.digest = digest.to_string();
        }
        files.push(recipe);
        Extraction::from_files(&files)
    }

    /// # Test contract
    ///
    /// ```json
    /// {"test": "several-recipes-are-judged-in-one-call", "checks": ["crates/core/src/tools/write#judges-several", "crates/core/src/tools/write#judges-records", "req:tool-judge-recipe", "crates/core/src/tools/write#answers-judged"]}
    /// ```
    #[test]
    fn several_recipes_are_judged_in_one_call() {
        let mut files = crate::extraction::fixtures::extraction_files();
        for id in ["ready", "card"] {
            files.push(crate::extraction::fixtures::recipe_file(&format!("ui/{id}.recipe.md"), id, "component", &[], &["src/a/ready.rs"], &[]));
        }
        let mut s = State::new(crate::graph::fixtures::store(), Extraction::from_files(&files), vec![]);
        // One refused, none recorded: every one is checked before any is.
        let two = json!({ "judgements": [{ "ref": "recipe:ready", "outcome": "held" }, { "ref": "recipe:ghost", "outcome": "held" }], "ticket": "later-1" });
        assert_eq!(call(&mut s, "judge_recipe", two, "2026-10-08").unwrap_err().code(), "not_found");
        let bad = json!({ "judgements": [{ "ref": "recipe:ready", "outcome": "held" }, { "ref": "recipe:card", "outcome": "held", "fingerprint": "xyz" }], "ticket": "later-1" });
        assert_eq!(call(&mut s, "judge_recipe", bad, "2026-10-08").unwrap_err().code(), "invalid");
        assert!(s.store().judgements.values().all(Vec::is_empty), "nothing recorded: {:?}", s.store().judgements);
        // Both judged, in the order given, neither due.
        let both = json!({ "judgements": [{ "ref": "recipe:ready", "outcome": "held" }, { "ref": "recipe:card", "outcome": "updated" }], "ticket": "later-1" });
        let out = call(&mut s, "judge_recipe", both, "2026-10-08").unwrap();
        let judged: Vec<(String, String, bool)> = out.output["judged"]
            .as_array()
            .unwrap()
            .iter()
            .map(|j| (j["recipe"].as_str().unwrap().to_string(), j["judgement"]["outcome"].as_str().unwrap().to_string(), j["due"].as_bool().unwrap()))
            .collect();
        assert_eq!(judged, [("recipe:ready".into(), "held".into(), false), ("recipe:card".into(), "updated".into(), false)]);
        assert!(matches!(&out.changed, Some(Changed::Many(c)) if c.len() == 2));
        // The single form and the list together, or an empty list: invalid.
        assert_eq!(call(&mut s, "judge_recipe", json!({ "ref": "recipe:ready", "outcome": "held", "judgements": [], "ticket": "later-1" }), "2026-10-08").unwrap_err().code(), "invalid");
        assert_eq!(call(&mut s, "judge_recipe", json!({ "judgements": [], "ticket": "later-1" }), "2026-10-08").unwrap_err().code(), "invalid");
    }

    /// # Test contract
    ///
    /// ```json
    /// {"test": "a-recipe-is-judged-in-a-ticket", "checks": ["crates/core/src/tools/write#judges-records", "crates/core/src/tools/write#judges-refusals", "crates/core/src/ops#judge", "req:tool-judge-recipe"]}
    /// ```
    #[test]
    fn a_recipe_is_judged_in_a_ticket() {
        let mut s = State::new(crate::graph::fixtures::store(), with_recipe("00000000000000aa"), vec![]);
        let code = |r: Result<crate::tools::Outcome, ToolError>| r.unwrap_err().code();
        // No ticket named and none current.
        let e = call(&mut s, "judge_recipe", json!({ "ref": "recipe:ready", "outcome": "held" }), "2026-10-07").unwrap_err();
        assert_eq!((e.code(), serde_json::to_value(&e).unwrap()["details"][0]["field"].clone()), ("invalid", json!("ticket")));
        // A recipe is judged whole, and only one the code holds.
        assert_eq!(code(call(&mut s, "judge_recipe", json!({ "ref": "recipe:ready#label", "outcome": "held", "ticket": "later-1" }), "2026-10-07")), "invalid");
        assert_eq!(code(call(&mut s, "judge_recipe", json!({ "ref": "page:overview", "outcome": "held", "ticket": "later-1" }), "2026-10-07")), "invalid");
        assert_eq!(code(call(&mut s, "judge_recipe", json!({ "ref": "recipe:gone", "outcome": "held", "ticket": "later-1" }), "2026-10-07")), "not_found");
        // A ticket the store lacks, and a bottled one.
        assert_eq!(code(call(&mut s, "judge_recipe", json!({ "ref": "recipe:ready", "outcome": "held", "ticket": "nope" }), "2026-10-07")), "not_found");
        assert_eq!(code(call(&mut s, "judge_recipe", json!({ "ref": "recipe:ready", "outcome": "held", "ticket": "old" }), "2026-10-07")), "conflict");
        assert!(s.store().judgements.is_empty(), "nothing refused was recorded");
        let due = |s: &mut State| call(s, "lint", json!({}), "2026-10-07").unwrap().output["findings"].as_array().unwrap().iter().any(|f| f["check"] == "due-recipe");
        assert!(due(&mut s), "never judged");
        // Judged in the current ticket, at its digest: applied at once, in no proposal, and no longer due.
        call(&mut s, "brew_ticket", json!({ "ref": "ticket:phase-62" }), "2026-10-07").unwrap();
        let out = call(&mut s, "judge_recipe", json!({ "ref": "recipe:ready", "outcome": "held" }), "2026-10-07").unwrap();
        assert!(matches!(&out.changed, Some(Changed::Judged(r)) if r == "ready"));
        assert_eq!(out.output["judged"][0]["recipe"], "recipe:ready");
        assert_eq!(out.output["judged"][0]["judgement"], json!({ "recipe": "ready", "fingerprint": "00000000000000aa", "date": "2026-10-07", "ticket": "phase-62", "outcome": "held" }));
        assert_eq!(out.output["judged"][0]["due"], false);
        assert!(s.store().proposals.get("phase-62").is_none_or(|p| p.is_empty()), "a judgement waits in no proposal");
        assert!(!due(&mut s));
        // Its files change: due again, and judged again as updated, appended after the first.
        s.set_extraction(with_recipe("00000000000000bb"));
        assert!(due(&mut s));
        call(&mut s, "judge_recipe", json!({ "ref": "recipe:ready", "outcome": "updated", "ticket": "ticket:later-1" }), "2026-10-08").unwrap();
        let all = &s.store().judgements["ready"];
        let seen: Vec<(&str, &str, JudgementOutcome)> = all.iter().map(|j| (j.fingerprint.as_str(), j.ticket.as_str(), j.outcome)).collect();
        assert_eq!(seen, [("00000000000000aa", "phase-62", JudgementOutcome::Held), ("00000000000000bb", "later-1", JudgementOutcome::Updated)]);
        // A fingerprint supplied is taken as given, as a machine supplies it to a server, and checked for its form.
        call(&mut s, "judge_recipe", json!({ "ref": "recipe:ready", "outcome": "held", "fingerprint": "00000000000000cc" }), "2026-10-08").unwrap();
        assert_eq!(s.store().judgements["ready"].last().unwrap().fingerprint, "00000000000000cc");
        assert_eq!(code(call(&mut s, "judge_recipe", json!({ "ref": "recipe:ready", "outcome": "held", "fingerprint": "xyz" }), "2026-10-08")), "invalid");
        // A state with no code takes a judgement only with its fingerprint, and says nothing of whether it is due.
        let mut shared = State::new(s.store().clone(), Extraction::from_files(&Vec::<crate::extraction::FileExtraction>::new()), vec![]);
        shared.set_without_code(true);
        let e = call(&mut shared, "judge_recipe", json!({ "ref": "recipe:ready", "outcome": "held", "ticket": "later-1" }), "2026-10-08").unwrap_err();
        assert_eq!((e.code(), serde_json::to_value(&e).unwrap()["details"][0]["field"].clone()), ("invalid", json!("fingerprint")));
        let out = call(&mut shared, "judge_recipe", json!({ "ref": "recipe:ready", "outcome": "held", "ticket": "later-1", "fingerprint": "00000000000000dd" }), "2026-10-08").unwrap().output;
        assert_eq!(out["judged"][0]["due"], json!(null));
    }

    /// # Test contract
    ///
    /// ```json
    /// {"test": "a-start-and-a-close-name-the-recipes", "checks": ["req:recipe-in-tickets", "crates/core/src/tools/write#answers-brew", "crates/core/src/tools/write#answers-close", "crates/core/src/tools/write#answers-bottle"]}
    /// ```
    #[test]
    fn a_start_and_a_close_name_the_recipes() {
        let mut s = State::new(crate::graph::fixtures::store(), with_recipe("00000000000000aa"), vec![]);
        // phase-62 touches module:src/a/ready, which the recipe represents.
        let started = call(&mut s, "brew_ticket", json!({ "ref": "ticket:phase-62" }), "2026-10-07").unwrap().output;
        assert_eq!(started["recipes"], json!([{ "ref": "recipe:ready", "title": "READY", "due": true, "via": "module:src/a/ready" }]));
        // Judged, it is said not due; with when: bottled the close names none due.
        call(&mut s, "judge_recipe", json!({ "ref": "recipe:ready", "outcome": "held" }), "2026-10-07").unwrap();
        let bottling = call(&mut s, "close_ticket", json!({ "ref": "ticket:phase-62", "decision": { "text": "Done." }, "when": "bottled" }), "2026-10-07").unwrap().output;
        assert_eq!(bottling["due"], json!([]));
        // A ticket touching nothing below the recipe; its files change after it started, so its close names it all the same.
        call(&mut s, "inscribe_ticket", json!({ "title": "Elsewhere", "kind": "change", "priority": "later", "touches": ["page:orphan"], "body": "b" }), "2026-10-07").unwrap();
        let started = call(&mut s, "brew_ticket", json!({ "ref": "ticket:0001" }), "2026-10-07").unwrap().output;
        assert_eq!(started["recipes"], json!([]));
        s.set_extraction(with_recipe("00000000000000bb"));
        let closed = call(&mut s, "close_ticket", json!({ "ref": "ticket:0001", "decision": { "text": "Done." } }), "2026-10-07").unwrap().output;
        assert_eq!(closed["due"], json!([{ "ref": "recipe:ready", "title": "READY", "due": true, "via": "recipe:ready" }]));
        // With no code held it cannot be said.
        let mut shared = State::new(s.store().clone(), Extraction::from_files(&Vec::<crate::extraction::FileExtraction>::new()), vec![]);
        shared.set_without_code(true);
        let closed = call(&mut shared, "bottle_ticket", json!({ "ref": "ticket:phase-62" }), "2026-10-07").unwrap().output;
        assert_eq!(closed["due"], json!(null));
    }

    /// The records a change persists, in order, each leaf once, whatever the nesting.
    fn leaves(c: &Changed) -> Vec<Changed> {
        match c {
            Changed::Many(cs) => cs.iter().flat_map(leaves).collect(),
            other => vec![other.clone()],
        }
    }

    /// # Test contract
    ///
    /// ```json
    /// {"test": "a-ticket-a-close-edits-keeps-pace", "checks": ["crates/core/src/tools/write#edited-change-keeps-pace"]}
    /// ```
    #[test]
    fn a_ticket_a_close_edits_keeps_pace() {
        let mut s = state();
        let day = "2026-10-05";
        // A question more urgent than phase-62, which is later; later-1 is being worked and is current.
        s.store_mut().tickets.get_mut("phase-62").unwrap().priority = Priority::Later;
        let q =
            call(&mut s, "inscribe_ticket", json!({ "title": "Which?", "kind": "question", "priority": "next", "body": "Asked." }), day).unwrap().output["node"]["ref"].as_str().unwrap().to_string();
        s.store_mut().current = Some("later-1".into());
        // The edit waits in later-1's proposal: the truth's ticket does not yet rely on the question, so nothing moves.
        call(&mut s, "update_ticket", json!({ "ref": "ticket:phase-62", "relies_on": [q] }), day).unwrap();
        assert_eq!(s.store().tickets["phase-62"].priority, Priority::Later);
        let closed = call(&mut s, "close_ticket", json!({ "ref": "ticket:later-1", "decision": { "text": "Done." } }), day).unwrap();
        let edited = &s.store().tickets["phase-62"];
        assert_eq!(edited.relies_on, vec![q.parse::<Reference>().unwrap()]);
        assert_eq!(edited.priority, Priority::Next, "it now relies on a more urgent question, and is raised to it");
        // What moved is persisted with the close.
        assert!(leaves(closed.changed.as_ref().unwrap()).contains(&Changed::Ticket("phase-62".into())));
    }

    /// # Test contract
    ///
    /// ```json
    /// {"test": "what-a-settle-changed-is-persisted-after-the-write", "checks": ["crates/core/src/tools/write#settle-persisted-after-write"]}
    /// ```
    #[test]
    fn what_a_settle_changed_is_persisted_after_the_write() {
        let day = "2026-10-05";
        // gap-1 waits, simmering, for phase-62, the only change relying on it.
        let waiting = || {
            let mut s = state();
            s.store_mut().tickets.get_mut("gap-1").unwrap().status = Status::Brewing;
            call(&mut s, "update_ticket", json!({ "ref": "ticket:phase-62", "relies_on": ["page:git-readiness", "ticket:gap-1"], "ticket": "gap-1" }), day).unwrap();
            call(&mut s, "close_ticket", json!({ "ref": "ticket:gap-1", "decision": { "text": "Answered." }, "when": "bottled" }), day).unwrap();
            assert_eq!(s.store().tickets["gap-1"].status, Status::Simmering);
            s
        };
        let position = |order: &[Changed], c: Changed| order.iter().position(|x| *x == c).unwrap_or_else(|| panic!("{c:?} is persisted in {order:?}"));
        let (change, gap) = (Changed::Ticket("phase-62".into()), Changed::Bottled("gap-1".into()));
        // update_ticket shortening relies_on: its own record, then the answer it reopened.
        let mut s = waiting();
        let out = call(&mut s, "update_ticket", json!({ "ref": "ticket:phase-62", "relies_on": ["page:git-readiness"] }), day).unwrap();
        assert_eq!(s.store().tickets["gap-1"].status, Status::Inscribed);
        let order = leaves(out.changed.as_ref().unwrap());
        assert!(position(&order, change.clone()) < position(&order, gap.clone()), "{order:?}");
        // delete_node of the change: the deletion, then the answer.
        let mut s = waiting();
        let out = call(&mut s, "delete_node", json!({ "ref": "ticket:phase-62" }), day).unwrap();
        assert_eq!(s.store().tickets["gap-1"].status, Status::Inscribed);
        let order = leaves(out.changed.as_ref().unwrap());
        assert!(position(&order, Changed::Deleted("ticket:phase-62".parse().unwrap())) < position(&order, gap.clone()), "{order:?}");
        // close_ticket of the change: its close, then the answer it settled.
        let mut s = waiting();
        call(&mut s, "brew_ticket", json!({ "ref": "ticket:phase-62" }), day).unwrap();
        let out = call(&mut s, "close_ticket", json!({ "ref": "ticket:phase-62", "decision": { "text": "Done." } }), day).unwrap();
        assert_eq!(s.store().tickets["gap-1"].status, Status::Bottled);
        let order = leaves(out.changed.as_ref().unwrap());
        assert!(position(&order, Changed::Bottled("phase-62".into())) < position(&order, gap.clone()), "{order:?}");
        assert_eq!(order.last(), Some(&gap), "nothing is persisted after what the settle changed");
    }
}
