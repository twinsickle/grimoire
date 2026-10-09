//! # Module contract
//!
//! ```json
//! {"purpose":"Carry a ticket through its life: file, update, start, decide and close it, keep a change's priority with the answers it relies on, settle those answers when it is bottled or goes, and name the ticket a write belongs to.","criteria":[{"id":"inscribe-ticket","statement":"inscribe_ticket creates an inscribed ticket with a null decision under a generated id: one past the highest all-digit id in the store, zero-padded to four digits, so filing never collides and never asks for a name."},{"id":"update-ticket","statement":"update_ticket changes only the given title, priority, touches, relies_on or body of a ticket that is not bottled, the body given whole or edited in part as BodyEdit does, and is a conflict on a bottled one."},{"id":"brew-ticket","statement":"brew_ticket moves a ticket from inscribed to brewing and is a conflict from any other status."},{"id":"close-keeps-only-decision","statement":"Closing a ticket replaces its body with the empty string and sets its decision; a bottled ticket cannot be reopened or edited."},{"id":"lowered-change-lowers-its-answers","statement":"keep_pace, after a change was lowered, lowers each answer it relies on, that is not bottled, now above it to the most urgent priority among the changes not bottled that rely on it."},{"id":"change-below-answer-raised","statement":"keep_pace raises a change that is not bottled below any gap or question it relies on that is not bottled to the most urgent of them."},{"id":"raised-answer-raises-its-changes","statement":"keep_pace, after a gap or question was raised, raises every change not bottled that relies on it and sits below it to it."},{"id":"filed-answer-raises-changes-naming-it","statement":"keep_pace, after a gap or question was filed when a change already names it, raises every change not bottled that relies on it and sits below it to it."},{"id":"bottled-never-moved","statement":"keep_pace never moves a bottled ticket."},{"id":"lowering-answer-or-raising-change-moves-nothing","statement":"keep_pace moves nothing when an answer is lowered or a change is raised."},{"id":"answers-each-slug-once","statement":"keep_pace answers each slug it moved once."},{"id":"brewing-ticket-simmers","statement":"decide_ticket writes a decision ahead of a bottling: a brewing ticket becomes simmering."},{"id":"inscribed-answer-simmers","statement":"decide_ticket makes an inscribed gap or question simmering too, since closing one needs no start."},{"id":"simmering-decision-replaced","statement":"decide_ticket on a simmering ticket replaces its decision, keeping its body, proposal and start."},{"id":"inscribed-change-or-bottled-conflict","statement":"decide_ticket on an inscribed change or a bottled ticket is a conflict."},{"id":"answer-needs-unbottled-change","statement":"A gap or question becomes simmering only when a change that is not bottled relies on it, invalid otherwise."},{"id":"answer-needs-empty-proposal","statement":"A gap or question becomes simmering only with an empty proposal, invalid otherwise."},{"id":"answer-simmering-drops-proposal-clears-current","statement":"A gap or question made simmering has its proposal dropped and the current ticket cleared when it was that one."},{"id":"close-refuses-simmering-answer","statement":"close_ticket refuses a simmering gap or question as a conflict."},{"id":"withdraw","statement":"withdraw_ticket closes an inscribed, brewing or simmering change with nothing of it applied: its decision marked withdrawn, its body emptied, its proposal and start dropped and the current ticket cleared when it was this one; a bottled ticket is a conflict and a gap or question invalid, nothing changing."},{"id":"settles-only-waiting-answers","statement":"settle looks only at the given gaps and questions that are simmering and that no change not bottled relies on."},{"id":"closes-answers-after-change-closed","statement":"After a change closed, settle closes each with its decision, its body emptied and its start removed."},{"id":"closes-answers-an-applied-change-closed","statement":"After a change went away, deleted, withdrawn or no longer relying on them, settle closes each that a bottled change relying on it, not withdrawn, closed on or after its decision's date."},{"id":"reopens-the-rest","statement":"After a change went away, settle reopens every other answer, inscribed with no decision and its start removed, its body followed by the answer's date, why it reopened and the answer's text."},{"id":"reopened-body-cut-to-limit","statement":"A reopened answer's body is cut with an ellipsis to the body's limit."},{"id":"answers-closed-and-reopened","statement":"settle answers the slugs it closed and reopened."},{"id":"current-ticket","statement":"brew_ticket makes the started change the store's current one, and never a gap or question, which it leaves as it was, and close_ticket clears it when it closes that one, a simmering ticket staying current until then; target_ticket answers the ticket a write belongs to, the one given as a slug or ticket: reference, else the current one, and refuses as invalid when there is neither."},{"id":"checked-out","statement":"check_out adds who took a ticket, and when, to the end of that ticket's checked_out list unless that name is listed already, which keeps when they first took it, answering whether the list changed; release_unworked removes the list of every ticket that is gone or no longer being worked, answering whether any went."},{"id":"waits-here","statement":"decide_ticket records on a change's start, when it has one, the checkout it was last decided in, the store's repository, as simmering_in, answering the start among what changed when it moved; waiting_here answers, in slug order, every simmering ticket whose start names this store's repository as simmering_in, but the one given, the same folder spelt with the other slash, a trailing one or another case counting as one."}]}
//! ```

use super::*;

pub struct NewTicket {
    pub title: String,
    pub kind: TicketKind,
    pub priority: Priority,
    pub touches: Vec<Reference>,
    pub relies_on: Vec<Reference>,
    pub body: String,
}

/// The next ticket id: one past the highest all-digit id in the store,
/// zero-padded to four digits. Tickets filed before ids were generated keep
/// their names and are not counted.
pub fn next_ticket_id(store: &Store) -> String {
    let max = store.tickets.keys().filter_map(|k| k.parse::<u64>().ok()).max().unwrap_or(0);
    format!("{:04}", max + 1)
}

pub fn inscribe_ticket(store: &mut Store, t: NewTicket) -> Result<(Ticket, Changed), ToolError> {
    let slug = next_ticket_id(store);
    let ticket = Ticket { slug, title: t.title, kind: t.kind, priority: t.priority, status: Status::Inscribed, touches: t.touches, relies_on: t.relies_on, body: t.body, decision: None };
    invalid_if(validate_ticket(&ticket), "ticket")?;
    store.tickets.insert(ticket.slug.clone(), ticket.clone());
    Ok((ticket.clone(), Changed::Ticket(ticket.slug)))
}

#[derive(Default)]
pub struct TicketPatch {
    pub title: Option<String>,
    pub priority: Option<Priority>,
    pub touches: Option<Vec<Reference>>,
    pub relies_on: Option<Vec<Reference>>,
    pub body: Option<String>,
    pub edit: BodyEdit,
}

fn ticket_mut<'a>(store: &'a mut Store, r: &Reference) -> Result<&'a mut Ticket, ToolError> {
    if r.kind != Kind::Ticket {
        return Err(ToolError::invalid(format!("{r} is not a ticket"), vec![]));
    }
    store.tickets.get_mut(&r.id).ok_or_else(|| ToolError::not_found(r))
}

pub fn update_ticket(store: &mut Store, r: &Reference, patch: TicketPatch) -> Result<(Ticket, Changed), ToolError> {
    let existing = ticket_mut(store, r)?;
    if existing.status == Status::Bottled {
        return Err(ToolError::conflict(format!("{r} is bottled")));
    }
    let mut t = existing.clone();
    if let Some(v) = patch.title {
        t.title = v;
    }
    if let Some(v) = patch.priority {
        t.priority = v;
    }
    if let Some(v) = patch.touches {
        t.touches = v;
    }
    if let Some(v) = patch.relies_on {
        t.relies_on = v;
    }
    if let Some(v) = patch.edit.apply_to(&t.body, patch.body)? {
        t.body = v;
    }
    invalid_if(validate_ticket(&t), "ticket")?;
    *existing = t.clone();
    Ok((t, Changed::Ticket(r.id.clone())))
}

pub fn brew_ticket(store: &mut Store, r: &Reference) -> Result<(Ticket, Changed), ToolError> {
    let t = ticket_mut(store, r)?;
    if t.status != Status::Inscribed {
        return Err(ToolError::conflict(format!("{r} is {:?}, not inscribed", t.status).to_lowercase()));
    }
    t.status = Status::Brewing;
    let t = t.clone();
    // A gap or question is claimed, never made current: answering one never displaces the change a checkout works on.
    if t.kind == TicketKind::Change {
        store.current = Some(r.id.clone());
    }
    Ok((t, Changed::Ticket(r.id.clone())))
}

pub fn close_ticket(store: &mut Store, r: &Reference, decision: Decision) -> Result<(Ticket, Changed), ToolError> {
    let existing = ticket_mut(store, r)?;
    if existing.status == Status::Bottled {
        return Err(ToolError::conflict(format!("{r} is already bottled")));
    }
    // An answer waiting for its change closes when that change is bottled, never by hand.
    if existing.kind != TicketKind::Change && existing.status == Status::Simmering {
        return Err(ToolError::conflict(format!("{r} is an answer waiting for the change that applies it: it closes when that change is bottled; write a new answer with when: bottled")));
    }
    let mut t = existing.clone();
    t.status = Status::Bottled;
    t.body = String::new();
    t.decision = Some(decision);
    invalid_if(validate_ticket(&t), "ticket")?;
    *existing = t.clone();
    if store.current.as_deref() == Some(r.id.as_str()) {
        store.current = None;
    }
    Ok((t, Changed::Ticket(r.id.clone())))
}

/// Withdraw a change that is no longer to be made: close it with `decision`, marked withdrawn,
/// applying nothing of it, its proposal and start dropped (req:tool-withdraw-ticket). What it
/// relied on is the caller's to settle, as for a change that went away.
pub fn withdraw_ticket(store: &mut Store, r: &Reference, mut decision: Decision) -> Result<(Ticket, Changed), ToolError> {
    let existing = ticket_mut(store, r)?;
    if existing.status == Status::Bottled {
        return Err(ToolError::conflict(format!("{r} is already bottled")));
    }
    if existing.kind != TicketKind::Change {
        return Err(ToolError::invalid(format!("{r} is not a change: only a change is withdrawn; a gap or question closes with close_ticket"), vec![]));
    }
    decision.withdrawn = true;
    let mut t = existing.clone();
    t.status = Status::Bottled;
    t.body = String::new();
    t.decision = Some(decision);
    invalid_if(validate_ticket(&t), "ticket")?;
    *existing = t.clone();
    // Persisted in this order, the closed record last, so a write stopped part way never leaves a
    // bottled ticket still current or holding a proposal, which the store would refuse to load.
    let mut changes = Vec::new();
    if store.current.as_deref() == Some(r.id.as_str()) {
        store.current = None;
        changes.push(Changed::Current);
    }
    if store.proposals.remove(&r.id).is_some() {
        changes.push(Changed::Proposed(r.id.clone()));
    }
    store.starts.remove(&r.id);
    changes.push(Changed::Bottled(r.id.clone()));
    Ok((t, Changed::Many(changes)))
}

/// Write a decision ahead of a bottling: a brewing ticket becomes simmering and
/// keeps its body, its proposal and its start; a simmering one has its
/// decision replaced. A gap or question waits so only for the change that
/// applies its answer: a change not bottled must already rely on it, it holds
/// no proposal, and it stops being the current ticket, since nothing of it
/// is bottled by a commit.
pub fn decide_ticket(store: &mut Store, r: &Reference, decision: Decision) -> Result<(Ticket, Changed), ToolError> {
    let mut t = ticket_mut(store, r)?.clone();
    // Closing a gap or question needs no start (req:ticket-checked-out).
    let unstarted_answer = t.kind != TicketKind::Change && t.status == Status::Inscribed;
    if !t.status.is_live() && !unstarted_answer {
        return Err(ToolError::conflict(format!("{r} is {}, not brewing: only a ticket being worked waits for its change to bottle", t.status.name())));
    }
    let answer = t.kind != TicketKind::Change;
    if answer {
        let kind = format!("{:?}", t.kind).to_lowercase();
        if !applying(store, &r.id).any(|c| c.status != Status::Bottled) {
            return Err(ToolError::invalid(format!("{r} is a {kind} no change relies on: file the change that applies its answer first, relying on it, then close {r} with when: bottled, or give that change to this close as `change`, which files it in the same write; an answer that needs no work closes at once, without when"), vec![]));
        }
        if store.proposals.get(&r.id).is_some_and(|p| !(p.pages.is_empty() && p.requirements.is_empty() && p.deleted.is_empty())) {
            return Err(ToolError::invalid(format!("{r} holds a proposal: an answer's spec belongs in the change that applies it, so move it there, or close {r} at once"), vec![]));
        }
    }
    t.status = Status::Simmering;
    t.decision = Some(decision);
    invalid_if(validate_ticket(&t), "ticket")?;
    store.tickets.insert(r.id.clone(), t.clone());
    if !answer {
        // The checkout it was decided in carries it until it is bottled, so that checkout takes no other meanwhile
        // without being told (req:one-ticket-per-branch); its start, this checkout's own, says which.
        let repository = store.config.repository.clone();
        return Ok(match store.starts.get_mut(&r.id) {
            Some(start) if start.simmering_in.as_deref() != Some(repository.as_str()) => {
                start.simmering_in = Some(repository);
                (t, Changed::Many(vec![Changed::Ticket(r.id.clone()), Changed::Started(r.id.clone())]))
            }
            _ => (t, Changed::Ticket(r.id.clone())),
        });
    }
    // It is no longer worked: what names it as worked goes first, so a writer
    // stopped part way leaves a brewing answer, not a waiting one still named.
    let mut changes = Vec::new();
    if store.current.as_deref() == Some(r.id.as_str()) {
        store.current = None;
        changes.push(Changed::Current);
    }
    store.proposals.remove(&r.id);
    store.starts.remove(&r.id);
    changes.extend([Changed::Proposed(r.id.clone()), Changed::Bottled(r.id.clone())]);
    Ok((t, Changed::Many(changes)))
}

/// Add `who` to those who have ticket `slug` checked out, unless that name is listed already, which keeps
/// when they first took it. Whether the list changed.
pub fn check_out(store: &mut Store, slug: &str, who: &CheckedOut) -> bool {
    let list = store.checked_out.entry(slug.to_string()).or_default();
    if list.iter().any(|c| c.by == who.by) {
        return false;
    }
    list.push(who.clone());
    true
}

/// Drop who has a ticket checked out for every ticket gone or no longer being worked. Whether any went.
pub fn release_unworked(store: &mut Store) -> bool {
    let before = store.checked_out.len();
    let tickets = &store.tickets;
    store.checked_out.retain(|slug, list| !list.is_empty() && tickets.get(slug).is_some_and(Ticket::is_worked));
    store.checked_out.len() != before
}

/// The changes this checkout closed `when: bottled` that have not been bottled, but `except`, in slug order:
/// each simmering ticket whose start names this store's repository as where it was bottled, the same
/// folder spelt with the other slash, a trailing one or another case counting as one.
pub fn waiting_here<'a>(store: &'a Store, except: Option<&str>) -> Vec<&'a Ticket> {
    let here = same_place(&store.config.repository);
    store
        .tickets
        .values()
        .filter(|t| t.status == Status::Simmering && Some(t.slug.as_str()) != except)
        .filter(|t| store.starts.get(&t.slug).and_then(|s| s.simmering_in.as_deref()).is_some_and(|c| same_place(c) == here))
        .collect()
}

fn same_place(path: &str) -> String {
    path.replace('\\', "/").trim_end_matches('/').to_lowercase()
}

/// The changes, of any status, that rely on a gap or question and so apply its answer.
fn applying<'a>(store: &'a Store, slug: &str) -> impl Iterator<Item = &'a Ticket> {
    let r = Reference::ticket(slug);
    store.tickets.values().filter(move |c| c.kind == TicketKind::Change && c.relies_on.contains(&r))
}

/// Keep a change no lower priority than the gaps and questions it relies on
/// that are not bottled, after `slug` was filed (`before` none) or edited from
/// priority `before`. A change lowered lowers each such answer it now sits
/// below to the most urgent of the changes not bottled that rely on it; then
/// a change below any such answer, filed, newly relying on it or held up by
/// another change, is raised to it. A gap or question raised, or filed when a
/// change already names it, raises every change not bottled that relies on it. Answers the slugs whose priority
/// moved beyond what was asked, each once.
pub fn keep_pace(store: &mut Store, slug: &str, before: Option<Priority>) -> (Vec<String>, Changed) {
    let mut moved: Vec<String> = Vec::new();
    let Some(t) = store.tickets.get(slug).filter(|t| t.status != Status::Bottled).cloned() else {
        return (moved, Changed::Many(vec![]));
    };
    let mut set = |store: &mut Store, s: &str, p: Priority| {
        if let Some(x) = store.tickets.get_mut(s) {
            x.priority = p;
            if !moved.iter().any(|m| m == s) {
                moved.push(s.to_string());
            }
        }
    };
    if t.kind == TicketKind::Change {
        let answers = |store: &Store| -> Vec<(String, Priority)> {
            t.relies_on
                .iter()
                .filter(|r| r.kind == Kind::Ticket)
                .filter_map(|r| store.tickets.get(&r.id))
                .filter(|a| a.kind != TicketKind::Change && a.status != Status::Bottled)
                .map(|a| (a.slug.clone(), a.priority))
                .collect()
        };
        if before.is_some_and(|b| t.priority > b) {
            for (a, p) in answers(store) {
                // Lowered with the change, but never below another change still relying on it.
                let floor = applying(store, &a).filter(|c| c.status != Status::Bottled).map(|c| c.priority).min().unwrap_or(t.priority);
                if p < t.priority && floor > p {
                    set(store, &a, floor);
                }
            }
        }
        if let Some(top) = answers(store).into_iter().map(|(_, p)| p).min().filter(|top| *top < t.priority) {
            set(store, slug, top);
        }
    } else if before.is_none_or(|b| t.priority < b) {
        let behind: Vec<String> = applying(store, slug).filter(|c| c.status != Status::Bottled && c.priority > t.priority).map(|c| c.slug.clone()).collect();
        for c in behind {
            set(store, &c, t.priority);
        }
    }
    let changed = Changed::Many(moved.iter().map(|s| Changed::Ticket(s.clone())).collect());
    (moved, changed)
}

/// Why a change stopped waiting to apply the answers it relies on.
pub enum Settling<'a> {
    /// It was bottled, and so applied them.
    Bottled,
    /// It went away without being bottled, for the reason given: deleted, or no
    /// longer relying on them.
    Gone(&'a str),
}

/// The gaps and questions a settle bottled and reopened, by slug.
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct Settled {
    pub bottled: Vec<String>,
    pub reopened: Vec<String>,
}

impl Settled {
    pub fn is_empty(&self) -> bool {
        self.bottled.is_empty() && self.reopened.is_empty()
    }
}

/// Settle the answers among `answers` that wait, simmering, for the changes
/// applying them, once no change relying on one is left unbottled. After a
/// change was bottled, each such answer is bottled with its own decision, since that
/// change applied it. After one went away, it is bottled if a change relying on
/// it was bottled on or after the day of its answer, and otherwise reopens for
/// re-examination, its answer and why moved into its body, cut to the body's
/// limit, since an inscribed ticket carries no decision.
pub fn settle(store: &mut Store, answers: &[Reference], how: Settling) -> (Settled, Changed) {
    let mut out = Settled::default();
    let mut changes = Vec::new();
    for r in answers {
        let Some(t) = store.tickets.get(&r.id).filter(|t| r.kind == Kind::Ticket && t.kind != TicketKind::Change && t.status == Status::Simmering).cloned() else {
            continue;
        };
        let Some(answer) = t.decision.clone() else { continue };
        if applying(store, &t.slug).any(|c| c.status != Status::Bottled) {
            continue;
        }
        let applied = match how {
            Settling::Bottled => true,
            // A withdrawn change applied nothing, so it never counts as having applied the answer.
            Settling::Gone(_) => applying(store, &t.slug).any(|c| c.status == Status::Bottled && c.decision.as_ref().is_some_and(|d| !d.withdrawn && d.date >= answer.date)),
        };
        // Nothing may still name it as worked, whatever an interrupted writer left.
        if store.current.as_deref() == Some(t.slug.as_str()) {
            store.current = None;
            changes.push(Changed::Current);
        }
        if store.proposals.remove(&t.slug).is_some() {
            changes.push(Changed::Proposed(t.slug.clone()));
        }
        if applied {
            let mut bottled = t.clone();
            bottled.status = Status::Bottled;
            bottled.body = String::new();
            store.tickets.insert(t.slug.clone(), bottled);
            store.starts.remove(&t.slug);
            changes.push(Changed::Bottled(t.slug.clone()));
            out.bottled.push(t.slug.clone());
        } else {
            let why = match how {
                Settling::Gone(why) => why,
                Settling::Bottled => "its change was bottled",
            };
            let mut inscribed = t.clone();
            inscribed.status = Status::Inscribed;
            inscribed.decision = None;
            inscribed.body = reopened_body(&t.body, &answer, why);
            store.tickets.insert(t.slug.clone(), inscribed);
            store.starts.remove(&t.slug);
            changes.push(Changed::Bottled(t.slug.clone()));
            out.reopened.push(t.slug.clone());
        }
    }
    (out, Changed::Many(changes))
}

/// A reopened answer's body: what it said, then the answer it had and why
/// nothing applied it, cut at the end to the body's limit.
fn reopened_body(body: &str, answer: &Decision, why: &str) -> String {
    let full = format!("{body}\n\n**Reopened.** Answered {}, then {why}, so no change applied the answer. It was:\n\n{}", answer.date, answer.text);
    let max = crate::store::TICKET_BODY_MAX;
    if full.chars().count() <= max {
        return full;
    }
    let mut cut: String = full.chars().take(max - 1).collect();
    cut.push('…');
    cut
}

/// The ticket a write that changes intent belongs to: the one given, as a
/// slug or a `ticket:` reference, else the store's current ticket; neither
/// is refused, since nothing changes the truth outside a ticket.
pub fn target_ticket(store: &Store, given: Option<&str>) -> Result<String, ToolError> {
    match given.map(|g| g.strip_prefix("ticket:").unwrap_or(g).to_string()).or_else(|| store.current.clone()) {
        Some(t) => Ok(t),
        None => Err(ToolError::invalid(
            "name the ticket this change belongs to: `ticket` is absent and no ticket is current (start one, or pass ticket)",
            vec![FieldError::new("ticket", "absent, and no ticket is current")],
        )),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::graph::fixtures::store;

    /// # Test contract
    ///
    /// ```json
    /// {"test": "a-change-is-withdrawn-with-nothing-applied", "checks": ["crates/core/src/ops/tickets#withdraw"]}
    /// ```
    #[test]
    fn a_change_is_withdrawn_with_nothing_applied() {
        let mut s = store();
        // later-1 is being worked: current, with a start and a proposal of a page the truth lacks.
        s.current = Some("later-1".into());
        s.starts.insert("later-1".into(), Start { ticket: "later-1".into(), fingerprints: Default::default(), simmering_in: None });
        crate::ops::propose(&mut s, "later-1", crate::ops::Proposing::Whole, |w| {
            crate::ops::put_page(w, Page { slug: "never".into(), title: "Never".into(), body: "b".into(), links: vec![] }).map(|(_, c)| ((), c))
        })
        .unwrap();
        let (t, _) = withdraw_ticket(&mut s, &Reference::ticket("later-1"), Decision::new("2026-10-06", "Reconsidered.")).unwrap();
        assert_eq!((t.status, t.body.as_str(), t.decision.as_ref().map(|d| d.withdrawn)), (Status::Bottled, "", Some(true)));
        assert!(!s.proposals.contains_key("later-1") && !s.starts.contains_key("later-1") && s.current.is_none());
        assert!(!s.pages.contains_key("never"), "nothing of its proposal reached the truth");
        // The closed record is persisted last, after the current ticket and the proposal it no longer holds.
        let mut s2 = store();
        s2.current = Some("later-1".into());
        crate::ops::propose(&mut s2, "later-1", crate::ops::Proposing::Whole, |w| {
            crate::ops::put_page(w, Page { slug: "never".into(), title: "Never".into(), body: "b".into(), links: vec![] }).map(|(_, c)| ((), c))
        })
        .unwrap();
        let (_, changed) = withdraw_ticket(&mut s2, &Reference::ticket("later-1"), Decision::new("2026-10-06", "Reconsidered.")).unwrap();
        assert_eq!(changed, Changed::Many(vec![Changed::Current, Changed::Proposed("later-1".into()), Changed::Bottled("later-1".into())]));
        // Not twice, and not an answer.
        assert!(matches!(withdraw_ticket(&mut s, &Reference::ticket("later-1"), Decision::new("2026-10-06", "Again.")), Err(ToolError::Conflict { .. })));
        let gap = s.tickets.values().find(|t| t.kind != TicketKind::Change && t.status != Status::Bottled).map(|t| t.slug.clone()).unwrap();
        let before = s.tickets[&gap].clone();
        assert!(matches!(withdraw_ticket(&mut s, &Reference::ticket(&gap), Decision::new("2026-10-06", "No.")), Err(ToolError::Invalid { .. })));
        assert_eq!(s.tickets[&gap], before);
    }

    /// # Test contract
    ///
    /// ```json
    /// {"test": "ticket-lifecycle", "checks": ["crates/core/src/ops/tickets#close-keeps-only-decision", "crates/core/src/ops/tickets#update-ticket", "crates/core/src/ops/tickets#brew-ticket"]}
    /// ```
    #[test]
    fn ticket_lifecycle() {
        let mut s = store();
        let r = Reference::ticket("phase-62");
        assert_eq!(brew_ticket(&mut s, &r).unwrap().0.status, Status::Brewing);
        assert_eq!(brew_ticket(&mut s, &r).unwrap_err().code(), "conflict");
        let (t, _) = update_ticket(&mut s, &r, TicketPatch { priority: Some(Priority::Later), ..Default::default() }).unwrap();
        assert_eq!(t.priority, Priority::Later);
        // The body edited in part: a passage that must match once, then a paragraph after it.
        s.tickets.get_mut("phase-62").unwrap().body = "The problem.".into();
        let edit = |replace: Option<(&str, &str)>, append: Option<&str>| TicketPatch {
            edit: BodyEdit { replace: replace.map(|(o, n)| Replace { old: o.into(), new: n.into() }), append: append.map(Into::into) },
            ..Default::default()
        };
        let (t, _) = update_ticket(&mut s, &r, edit(Some(("problem", "gap")), Some("The options."))).unwrap();
        assert_eq!((t.body.as_str(), t.priority), ("The gap.\n\nThe options.", Priority::Later));
        assert_eq!(update_ticket(&mut s, &r, edit(Some(("problem", "x")), None)).unwrap_err().code(), "invalid");
        assert_eq!(update_ticket(&mut s, &r, TicketPatch { body: Some("Whole.".into()), ..edit(None, Some("More.")) }).unwrap_err().code(), "invalid");
        assert_eq!(s.tickets["phase-62"].body, "The gap.\n\nThe options.");
        let (t, _) = close_ticket(&mut s, &r, Decision::new("2026-09-27", "Shipped.")).unwrap();
        assert_eq!(t.status, Status::Bottled);
        assert_eq!(t.body, "");
        assert_eq!(update_ticket(&mut s, &r, TicketPatch::default()).unwrap_err().code(), "conflict");
        assert_eq!(close_ticket(&mut s, &r, Decision::new("2026-09-27", "again")).unwrap_err().code(), "conflict");
    }

    /// # Test contract
    ///
    /// ```json
    /// {"test": "inscribe-ticket-generates-ids", "checks": ["crates/core/src/ops/tickets#inscribe-ticket"]}
    /// ```
    #[test]
    fn inscribe_ticket_generates_ids() {
        let mut s = store();
        // Existing tickets have names, not numbers: the first generated id is 0001.
        assert_eq!(next_ticket_id(&s), "0001");
        let t = || NewTicket { title: "T".into(), kind: TicketKind::Change, priority: Priority::Next, touches: vec![], relies_on: vec![], body: "b".into() };
        let (a, _) = inscribe_ticket(&mut s, t()).unwrap();
        let (b, _) = inscribe_ticket(&mut s, t()).unwrap();
        assert_eq!(a.slug, "0001");
        assert_eq!(b.slug, "0002");
        assert!(crate::store::is_slug(&a.slug));
        // Deleting the latest does not reuse its number's predecessor's successor incorrectly.
        s.tickets.remove("0002");
        assert_eq!(next_ticket_id(&s), "0002");
        // Ten thousand tickets still get distinct ids, just longer.
        s.tickets.insert("9999".into(), Ticket { slug: "9999".into(), ..a.clone() });
        assert_eq!(next_ticket_id(&s), "10000");
    }

    /// # Test contract
    ///
    /// ```json
    /// {"test": "an-inscribed-answer-simmers-without-a-start", "checks": ["crates/core/src/ops/tickets#inscribed-answer-simmers"]}
    /// ```
    #[test]
    fn an_inscribed_answer_simmers_without_a_start() {
        let mut s = store();
        // A change not bottled relies on the gap, which is inscribed and never started.
        s.tickets.get_mut("later-1").unwrap().relies_on = vec![Reference::ticket("gap-1")];
        assert_eq!(s.tickets["gap-1"].status, Status::Inscribed);
        assert!(!s.starts.contains_key("gap-1"));
        let (t, _) = decide_ticket(&mut s, &Reference::ticket("gap-1"), Decision::new("2026-10-08", "Fix it.")).unwrap();
        assert_eq!((t.status, t.decision.as_ref().map(|d| d.text.as_str())), (Status::Simmering, Some("Fix it.")));
        assert_eq!(s.tickets["gap-1"].status, Status::Simmering);
    }

    /// # Test contract
    ///
    /// ```json
    /// {"test": "an-answer-made-simmering-drops-its-proposal-and-current", "checks": ["crates/core/src/ops/tickets#answer-simmering-drops-proposal-clears-current"]}
    /// ```
    #[test]
    fn an_answer_made_simmering_drops_its_proposal_and_current() {
        let mut s = store();
        s.tickets.get_mut("later-1").unwrap().relies_on = vec![Reference::ticket("gap-1")];
        // The gap is the current ticket and holds a proposal, empty, since an answer with a spec is refused.
        s.current = Some("gap-1".into());
        s.proposals.insert("gap-1".into(), Proposal::new("gap-1"));
        decide_ticket(&mut s, &Reference::ticket("gap-1"), Decision::new("2026-10-08", "Fix it.")).unwrap();
        assert!(!s.proposals.contains_key("gap-1"), "its proposal is gone");
        assert!(s.current.is_none(), "it is no longer the current ticket");
        // Another current ticket is left alone.
        let mut s = store();
        s.tickets.get_mut("later-1").unwrap().relies_on = vec![Reference::ticket("gap-1")];
        s.current = Some("later-1".into());
        decide_ticket(&mut s, &Reference::ticket("gap-1"), Decision::new("2026-10-08", "Fix it.")).unwrap();
        assert_eq!(s.current.as_deref(), Some("later-1"));
    }

    /// # Test contract
    ///
    /// ```json
    /// {"test": "keep-pace-answers-each-slug-it-moved-once", "checks": ["crates/core/src/ops/tickets#answers-each-slug-once"]}
    /// ```
    #[test]
    fn keep_pace_answers_each_slug_it_moved_once() {
        let mut s = store();
        // A change that names the same gap twice: lowered, it would lower that gap once for each naming.
        s.tickets.get_mut("gap-1").unwrap().priority = Priority::Next;
        s.tickets.get_mut("later-1").unwrap().relies_on = vec![Reference::ticket("gap-1"), Reference::ticket("gap-1")];
        let (moved, changed) = keep_pace(&mut s, "later-1", Some(Priority::Next));
        assert_eq!(moved, vec!["gap-1".to_string()]);
        assert_eq!(s.tickets["gap-1"].priority, Priority::Later);
        assert_eq!(changed, Changed::Many(vec![Changed::Ticket("gap-1".into())]));
    }
}
