//! # Module contract
//!
//! ```json
//! {"purpose":"Carry a ticket through its life: file, update, start, decide and close it, keep a change's priority with the answers it relies on, settle those answers when it lands or goes, and name the ticket a write belongs to.","criteria":[{"id":"file-ticket","statement":"file_ticket creates an open ticket with a null decision under a generated id: one past the highest all-digit id in the store, zero-padded to four digits, so filing never collides and never asks for a name."},{"id":"update-ticket","statement":"update_ticket changes only the given title, priority, touches, relies_on or body of a ticket that is not closed, the body given whole or edited in part as BodyEdit does, and is a conflict on a closed one."},{"id":"start-ticket","statement":"start_ticket moves a ticket from open to active and is a conflict from any other status."},{"id":"close-keeps-only-decision","statement":"Closing a ticket replaces its body with the empty string and sets its decision; a closed ticket cannot be reopened or edited."},{"id":"priority-follows","statement":"keep_pace keeps a change that is not closed no lower priority than the gaps and questions it relies on that are not closed: after a change was lowered, each such answer now above it is lowered to the most urgent priority among the changes not closed that rely on it; then a change below any such answer is raised to the most urgent of them; after a gap or question was raised, or filed when a change already names it, every change not closed that relies on it and sits below it is raised to it; a closed ticket is never moved, lowering an answer or raising a change moves nothing, and it answers each slug it moved once."},{"id":"closes-when-landed","statement":"decide_ticket writes a decision ahead of a landing: an active ticket becomes landing and a landing one has its decision replaced, keeping its body, proposal and start, and an open or closed ticket is a conflict; a gap or question becomes landing only when a change that is not closed relies on it, invalid otherwise, and only with an empty proposal, invalid otherwise, its proposal then dropped and the current ticket cleared when it was that one; close_ticket refuses a landing gap or question as a conflict."},{"id":"withdraw","statement":"withdraw_ticket closes an open, active or landing change with nothing of it applied: its decision marked withdrawn, its body emptied, its proposal and start dropped and the current ticket cleared when it was this one; a closed ticket is a conflict and a gap or question invalid, nothing changing."},{"id":"settles-answers","statement":"settle looks only at the given gaps and questions that are landing and that no change not closed relies on: after a change closed it closes each with its decision, its body emptied and its start removed; after a change went away, deleted, withdrawn or no longer relying on them, it closes each that a closed change relying on it, not withdrawn, closed on or after its decision's date, and reopens every other, open with no decision and its start removed, its body followed by the answer's date, why it reopened and the answer's text, cut with an ellipsis to the body's limit; it answers the slugs it closed and reopened."},{"id":"current-ticket","statement":"start_ticket makes the started ticket the store's current one and close_ticket clears it when it closes that one, a landing ticket staying current until then; target_ticket answers the ticket a write belongs to, the one given as a slug or ticket: reference, else the current one, and refuses as invalid when there is neither."},{"id":"waits-here","statement":"decide_ticket records on a change's start, when it has one, the checkout it was last decided in, the store's repository, as closed_in, answering the start among what changed when it moved; waiting_here answers, in slug order, every landing ticket whose start names this store's repository as closed_in, but the one given, the same folder spelt with the other slash, a trailing one or another case counting as one."}]}
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

pub fn file_ticket(store: &mut Store, t: NewTicket) -> Result<(Ticket, Changed), ToolError> {
    let slug = next_ticket_id(store);
    let ticket = Ticket { slug, title: t.title, kind: t.kind, priority: t.priority, status: Status::Open, touches: t.touches, relies_on: t.relies_on, body: t.body, decision: None };
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
    if existing.status == Status::Closed {
        return Err(ToolError::conflict(format!("{r} is closed")));
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

pub fn start_ticket(store: &mut Store, r: &Reference) -> Result<(Ticket, Changed), ToolError> {
    let t = ticket_mut(store, r)?;
    if t.status != Status::Open {
        return Err(ToolError::conflict(format!("{r} is {:?}, not open", t.status).to_lowercase()));
    }
    t.status = Status::Active;
    let t = t.clone();
    store.current = Some(r.id.clone());
    Ok((t, Changed::Ticket(r.id.clone())))
}

pub fn close_ticket(store: &mut Store, r: &Reference, decision: Decision) -> Result<(Ticket, Changed), ToolError> {
    let existing = ticket_mut(store, r)?;
    if existing.status == Status::Closed {
        return Err(ToolError::conflict(format!("{r} is already closed")));
    }
    // An answer waiting for its change closes when that change lands, never by hand.
    if existing.kind != TicketKind::Change && existing.status == Status::Landing {
        return Err(ToolError::conflict(format!("{r} is an answer waiting for the change that applies it: it closes when that change lands; write a new answer with when: landed")));
    }
    let mut t = existing.clone();
    t.status = Status::Closed;
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
    if existing.status == Status::Closed {
        return Err(ToolError::conflict(format!("{r} is already closed")));
    }
    if existing.kind != TicketKind::Change {
        return Err(ToolError::invalid(format!("{r} is not a change: only a change is withdrawn; a gap or question closes with close_ticket"), vec![]));
    }
    decision.withdrawn = true;
    let mut t = existing.clone();
    t.status = Status::Closed;
    t.body = String::new();
    t.decision = Some(decision);
    invalid_if(validate_ticket(&t), "ticket")?;
    *existing = t.clone();
    // Persisted in this order, the closed record last, so a write stopped part way never leaves a
    // closed ticket still current or holding a proposal, which the store would refuse to load.
    let mut changes = Vec::new();
    if store.current.as_deref() == Some(r.id.as_str()) {
        store.current = None;
        changes.push(Changed::Current);
    }
    if store.proposals.remove(&r.id).is_some() {
        changes.push(Changed::Proposed(r.id.clone()));
    }
    store.starts.remove(&r.id);
    changes.push(Changed::Closed(r.id.clone()));
    Ok((t, Changed::Many(changes)))
}

/// Write a decision ahead of a landing: an active ticket becomes landing and
/// keeps its body, its proposal and its start; a landing one has its
/// decision replaced. A gap or question waits so only for the change that
/// applies its answer: a change not closed must already rely on it, it holds
/// no proposal, and it stops being the current ticket, since nothing of it
/// lands by a commit.
pub fn decide_ticket(store: &mut Store, r: &Reference, decision: Decision) -> Result<(Ticket, Changed), ToolError> {
    let mut t = ticket_mut(store, r)?.clone();
    if !t.status.is_live() {
        return Err(ToolError::conflict(format!("{r} is {}, not active: only a ticket being worked waits for its change to land", t.status.name())));
    }
    let answer = t.kind != TicketKind::Change;
    if answer {
        let kind = format!("{:?}", t.kind).to_lowercase();
        if !applying(store, &r.id).any(|c| c.status != Status::Closed) {
            return Err(ToolError::invalid(format!("{r} is a {kind} no change relies on: file the change that applies its answer first, relying on it, then close {r} with when: landed, or give that change to this close as `change`, which files it in the same write; an answer that needs no work closes at once, without when"), vec![]));
        }
        if store.proposals.get(&r.id).is_some_and(|p| !(p.pages.is_empty() && p.requirements.is_empty() && p.deleted.is_empty())) {
            return Err(ToolError::invalid(format!("{r} holds a proposal: an answer's spec belongs in the change that applies it, so move it there, or close {r} at once"), vec![]));
        }
    }
    t.status = Status::Landing;
    t.decision = Some(decision);
    invalid_if(validate_ticket(&t), "ticket")?;
    store.tickets.insert(r.id.clone(), t.clone());
    if !answer {
        // The checkout it was decided in carries it until it lands, so that checkout takes no other meanwhile
        // without being told (req:one-ticket-per-branch); its start, this checkout's own, says which.
        let repository = store.config.repository.clone();
        return Ok(match store.starts.get_mut(&r.id) {
            Some(start) if start.closed_in.as_deref() != Some(repository.as_str()) => {
                start.closed_in = Some(repository);
                (t, Changed::Many(vec![Changed::Ticket(r.id.clone()), Changed::Started(r.id.clone())]))
            }
            _ => (t, Changed::Ticket(r.id.clone())),
        });
    }
    // It is no longer worked: what names it as worked goes first, so a writer
    // stopped part way leaves an active answer, not a waiting one still named.
    let mut changes = Vec::new();
    if store.current.as_deref() == Some(r.id.as_str()) {
        store.current = None;
        changes.push(Changed::Current);
    }
    store.proposals.remove(&r.id);
    store.starts.remove(&r.id);
    changes.extend([Changed::Proposed(r.id.clone()), Changed::Closed(r.id.clone())]);
    Ok((t, Changed::Many(changes)))
}

/// The changes this checkout closed `when: landed` that have not landed, but `except`, in slug order:
/// each landing ticket whose start names this store's repository as where it was closed, the same
/// folder spelt with the other slash, a trailing one or another case counting as one.
pub fn waiting_here<'a>(store: &'a Store, except: Option<&str>) -> Vec<&'a Ticket> {
    let here = same_place(&store.config.repository);
    store
        .tickets
        .values()
        .filter(|t| t.status == Status::Landing && Some(t.slug.as_str()) != except)
        .filter(|t| store.starts.get(&t.slug).and_then(|s| s.closed_in.as_deref()).is_some_and(|c| same_place(c) == here))
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
/// that are not closed, after `slug` was filed (`before` none) or edited from
/// priority `before`. A change lowered lowers each such answer it now sits
/// below to the most urgent of the changes not closed that rely on it; then
/// a change below any such answer, filed, newly relying on it or held up by
/// another change, is raised to it. A gap or question raised, or filed when a
/// change already names it, raises every change not closed that relies on it. Answers the slugs whose priority
/// moved beyond what was asked, each once.
pub fn keep_pace(store: &mut Store, slug: &str, before: Option<Priority>) -> (Vec<String>, Changed) {
    let mut moved: Vec<String> = Vec::new();
    let Some(t) = store.tickets.get(slug).filter(|t| t.status != Status::Closed).cloned() else {
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
                .filter(|a| a.kind != TicketKind::Change && a.status != Status::Closed)
                .map(|a| (a.slug.clone(), a.priority))
                .collect()
        };
        if before.is_some_and(|b| t.priority > b) {
            for (a, p) in answers(store) {
                // Lowered with the change, but never below another change still relying on it.
                let floor = applying(store, &a).filter(|c| c.status != Status::Closed).map(|c| c.priority).min().unwrap_or(t.priority);
                if p < t.priority && floor > p {
                    set(store, &a, floor);
                }
            }
        }
        if let Some(top) = answers(store).into_iter().map(|(_, p)| p).min().filter(|top| *top < t.priority) {
            set(store, slug, top);
        }
    } else if before.is_none_or(|b| t.priority < b) {
        let behind: Vec<String> = applying(store, slug).filter(|c| c.status != Status::Closed && c.priority > t.priority).map(|c| c.slug.clone()).collect();
        for c in behind {
            set(store, &c, t.priority);
        }
    }
    let changed = Changed::Many(moved.iter().map(|s| Changed::Ticket(s.clone())).collect());
    (moved, changed)
}

/// Why a change stopped waiting to apply the answers it relies on.
pub enum Settling<'a> {
    /// It closed, and so applied them.
    Closed,
    /// It went away without landing, for the reason given: deleted, or no
    /// longer relying on them.
    Gone(&'a str),
}

/// The gaps and questions a settle closed and reopened, by slug.
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct Settled {
    pub closed: Vec<String>,
    pub reopened: Vec<String>,
}

impl Settled {
    pub fn is_empty(&self) -> bool {
        self.closed.is_empty() && self.reopened.is_empty()
    }
}

/// Settle the answers among `answers` that wait, landing, for the changes
/// applying them, once no change relying on one is left open. After a
/// change closed, each such answer closes with its own decision, since that
/// change applied it. After one went away, it closes if a change relying on
/// it closed on or after the day of its answer, and otherwise reopens for
/// re-examination, its answer and why moved into its body, cut to the body's
/// limit, since an open ticket carries no decision.
pub fn settle(store: &mut Store, answers: &[Reference], how: Settling) -> (Settled, Changed) {
    let mut out = Settled::default();
    let mut changes = Vec::new();
    for r in answers {
        let Some(t) = store.tickets.get(&r.id).filter(|t| r.kind == Kind::Ticket && t.kind != TicketKind::Change && t.status == Status::Landing).cloned() else {
            continue;
        };
        let Some(answer) = t.decision.clone() else { continue };
        if applying(store, &t.slug).any(|c| c.status != Status::Closed) {
            continue;
        }
        let applied = match how {
            Settling::Closed => true,
            // A withdrawn change applied nothing, so it never counts as having applied the answer.
            Settling::Gone(_) => applying(store, &t.slug).any(|c| c.status == Status::Closed && c.decision.as_ref().is_some_and(|d| !d.withdrawn && d.date >= answer.date)),
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
            let mut closed = t.clone();
            closed.status = Status::Closed;
            closed.body = String::new();
            store.tickets.insert(t.slug.clone(), closed);
            store.starts.remove(&t.slug);
            changes.push(Changed::Closed(t.slug.clone()));
            out.closed.push(t.slug.clone());
        } else {
            let why = match how {
                Settling::Gone(why) => why,
                Settling::Closed => "its change closed",
            };
            let mut open = t.clone();
            open.status = Status::Open;
            open.decision = None;
            open.body = reopened_body(&t.body, &answer, why);
            store.tickets.insert(t.slug.clone(), open);
            store.starts.remove(&t.slug);
            changes.push(Changed::Closed(t.slug.clone()));
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
        s.starts.insert("later-1".into(), Start { ticket: "later-1".into(), fingerprints: Default::default(), closed_in: None });
        crate::ops::propose(&mut s, "later-1", crate::ops::Proposing::Whole, |w| {
            crate::ops::put_page(w, Page { slug: "never".into(), title: "Never".into(), body: "b".into(), links: vec![] }).map(|(_, c)| ((), c))
        })
        .unwrap();
        let (t, _) = withdraw_ticket(&mut s, &Reference::ticket("later-1"), Decision::new("2026-10-06", "Reconsidered.")).unwrap();
        assert_eq!((t.status, t.body.as_str(), t.decision.as_ref().map(|d| d.withdrawn)), (Status::Closed, "", Some(true)));
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
        assert_eq!(changed, Changed::Many(vec![Changed::Current, Changed::Proposed("later-1".into()), Changed::Closed("later-1".into())]));
        // Not twice, and not an answer.
        assert!(matches!(withdraw_ticket(&mut s, &Reference::ticket("later-1"), Decision::new("2026-10-06", "Again.")), Err(ToolError::Conflict { .. })));
        let gap = s.tickets.values().find(|t| t.kind != TicketKind::Change && t.status != Status::Closed).map(|t| t.slug.clone()).unwrap();
        let before = s.tickets[&gap].clone();
        assert!(matches!(withdraw_ticket(&mut s, &Reference::ticket(&gap), Decision::new("2026-10-06", "No.")), Err(ToolError::Invalid { .. })));
        assert_eq!(s.tickets[&gap], before);
    }

    /// # Test contract
    ///
    /// ```json
    /// {"test": "ticket-lifecycle", "checks": ["crates/core/src/ops/tickets#close-keeps-only-decision", "crates/core/src/ops/tickets#update-ticket", "crates/core/src/ops/tickets#start-ticket"]}
    /// ```
    #[test]
    fn ticket_lifecycle() {
        let mut s = store();
        let r = Reference::ticket("phase-62");
        assert_eq!(start_ticket(&mut s, &r).unwrap().0.status, Status::Active);
        assert_eq!(start_ticket(&mut s, &r).unwrap_err().code(), "conflict");
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
        assert_eq!(t.status, Status::Closed);
        assert_eq!(t.body, "");
        assert_eq!(update_ticket(&mut s, &r, TicketPatch::default()).unwrap_err().code(), "conflict");
        assert_eq!(close_ticket(&mut s, &r, Decision::new("2026-09-27", "again")).unwrap_err().code(), "conflict");
    }

    /// # Test contract
    ///
    /// ```json
    /// {"test": "file-ticket-generates-ids", "checks": ["crates/core/src/ops/tickets#file-ticket"]}
    /// ```
    #[test]
    fn file_ticket_generates_ids() {
        let mut s = store();
        // Existing tickets have names, not numbers: the first generated id is 0001.
        assert_eq!(next_ticket_id(&s), "0001");
        let t = || NewTicket { title: "T".into(), kind: TicketKind::Change, priority: Priority::Next, touches: vec![], relies_on: vec![], body: "b".into() };
        let (a, _) = file_ticket(&mut s, t()).unwrap();
        let (b, _) = file_ticket(&mut s, t()).unwrap();
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
}
