//! # Module contract
//!
//! ```json
//! {"purpose":"Answer board and related_tickets: the tickets to work on in order, with what each waits on, and the gaps and questions not yet bottled that share a ticket's nodes.","criteria":[{"id":"board-default-and-filters","statement":"board returns inscribed, brewing and simmering change tickets by default, filterable by status, kind, priority and a touched node."},{"id":"board-order","statement":"board orders change tickets next, high, later; within a band the tickets its order list names come first in that order, then the rest with each after any ticket not bottled that it relies on, then simmering before brewing before inscribed, then title, a loop broken by that same rule."},{"id":"board-waits-on","statement":"Each change on the board carries waits_on, the gaps and questions it relies on with their status, a simmering one being answered and waiting for the change."},{"id":"board-unattached","statement":"board's unattached lists the gaps and questions not bottled that no change not bottled relies on, ordered as the changes are, by band and then by the band's order list, as rows of the same shape as tickets."},{"id":"board-bottled","statement":"board's bottled lists the requested number of most recently decided bottled tickets, each bottled gap or question among them carrying followed_by, the change tickets of any status that rely on it in id order, empty when none does."},{"id":"board-drafts","statement":"A ticket in board's tickets or bottled whose references the proposal of a brewing or simmering ticket would change carries drafts, those tickets, its own references staying the truth's."},{"id":"board-checked-out","statement":"Each ticket on the board being worked, brewing or a change simmering, carries checked_out, who has it checked out as the graph holds it, empty when none is held, and every other ticket none."},{"id":"related-tickets","statement":"related_tickets returns the gaps and questions not yet bottled whose touches or relies_on share a reference with the given ticket's touches."}]}
//! ```

use std::collections::HashSet;

use serde::Serialize;

use super::lookup::{ticket_summary, Summary};
use crate::graph::{Graph, Node};
use crate::model::*;

pub struct BoardFilter<'a> {
    pub status: Option<&'a [Status]>,
    pub kinds: Option<&'a [TicketKind]>,
    pub priority: Option<&'a [Priority]>,
    pub touching: Option<&'a Reference>,
    /// How many of the most recently bottled tickets to return.
    pub bottled: usize,
}

/// The board's answer: the tickets asked for, each change with what it waits
/// on; the gaps and questions not yet bottled that no change not yet bottled relies on; and the most
/// recently bottled tickets, when asked for, each answered gap or question
/// with the changes that followed it.
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[derive(Debug, Clone, Serialize)]
pub struct Board {
    pub tickets: Vec<BoardTicket>,
    pub unattached: Vec<BoardTicket>,
    pub bottled: Vec<BoardTicket>,
}

/// A ticket on the board. A change carries `waits_on`: the gaps and
/// questions it relies on with their status, from which its readiness
/// follows; it is ready when none of them is still inscribed or brewing. A
/// bottled gap or question carries `followed_by`: the changes that rely on
/// it, of any status. Empty, the answer has nothing built on it, which is
/// either an answer that needed no work or one whose work was never filed.
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[derive(Debug, Clone, Serialize)]
pub struct BoardTicket {
    #[serde(flatten)]
    pub summary: Summary,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[cfg_attr(feature = "ts", ts(optional))]
    pub waits_on: Option<Vec<Summary>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[cfg_attr(feature = "ts", ts(optional))]
    pub followed_by: Option<Vec<Summary>>,
    /// The tickets whose proposals would change this ticket's references:
    /// what it touches and relies on here is the truth's, and an edit waits.
    #[serde(skip_serializing_if = "Option::is_none")]
    #[cfg_attr(feature = "ts", ts(optional))]
    pub drafts: Option<Vec<Reference>>,
    // A ticket being worked: who has it checked out and since when, empty when who is unknown (req:ticket-checked-out).
    #[serde(skip_serializing_if = "Option::is_none")]
    #[cfg_attr(feature = "ts", ts(optional))]
    pub checked_out: Option<Vec<crate::model::CheckedOut>>,
}

/// Who has `t` checked out, while it is being worked.
fn checked_out(g: &Graph, t: &Ticket) -> Option<Vec<crate::model::CheckedOut>> {
    t.is_worked().then(|| g.checked_out.get(&t.slug).cloned().unwrap_or_default())
}

/// A ticket as the board lists it.
fn row(g: &Graph, t: &Ticket, waits_on: Option<Vec<Summary>>, followed_by: Option<Vec<Summary>>) -> BoardTicket {
    BoardTicket { summary: ticket_summary(t), waits_on, followed_by, drafts: drafts(g, t), checked_out: checked_out(g, t) }
}

/// The tickets whose proposals would change `t`'s references, if any.
fn drafts(g: &Graph, t: &Ticket) -> Option<Vec<Reference>> {
    g.drafted.get(&t.slug).filter(|d| !d.is_empty()).map(|d| d.iter().map(|s| Reference::ticket(s)).collect())
}

pub fn board(g: &Graph, f: &BoardFilter) -> Board {
    let default_status = [Status::Inscribed, Status::Brewing, Status::Simmering];
    let default_kinds = [TicketKind::Change];
    let status = f.status.unwrap_or(&default_status);
    let kinds = f.kinds.unwrap_or(&default_kinds);
    let narrowed = |t: &&Ticket| f.priority.is_none_or(|p| p.contains(&t.priority)) && f.touching.is_none_or(|x| t.touches.contains(x) || t.relies_on.contains(x));
    let tickets: Vec<&Ticket> = g.tickets().filter(|t| status.contains(&t.status) && kinds.contains(&t.kind)).filter(narrowed).collect();
    let tickets = ordered(g, tickets).into_iter().map(|t| row(g, t, (t.kind == TicketKind::Change).then(|| waits_on(g, t)), None)).collect();
    // Gaps and questions a change that is not bottled relies on are attached to it.
    let attached: HashSet<&Reference> = g.tickets().filter(|t| t.kind == TicketKind::Change && t.status != Status::Bottled).flat_map(|t| t.relies_on.iter()).collect();
    let unattached = g.tickets().filter(|t| t.kind != TicketKind::Change && t.status != Status::Bottled).filter(|t| !attached.contains(&Reference::ticket(&t.slug))).filter(narrowed).collect();
    let unattached = ordered(g, unattached).into_iter().map(|t| row(g, t, None, None)).collect();
    let mut bottled: Vec<&Ticket> = g.tickets().filter(|t| t.status == Status::Bottled).filter(narrowed).collect();
    bottled.sort_by_key(|t| std::cmp::Reverse(newest_decision(t)));
    let bottled = bottled.into_iter().take(f.bottled).map(|t| row(g, t, None, (t.kind != TicketKind::Change).then(|| followed_by(g, t)))).collect();
    Board { tickets, unattached, bottled }
}

/// Tickets in board order: next, high, later, each band by its order list,
/// then dependency, then brewing before inscribed, then title.
fn ordered<'a>(g: &Graph, mut tickets: Vec<&'a Ticket>) -> Vec<&'a Ticket> {
    tickets.sort_by(|a, b| a.priority.cmp(&b.priority).then_with(|| b.status.cmp(&a.status)).then_with(|| a.title.cmp(&b.title)));
    let mut out = Vec::new();
    for p in [Priority::Next, Priority::High, Priority::Later] {
        let band: Vec<&Ticket> = tickets.iter().copied().filter(|t| t.priority == p).collect();
        out.extend(band_order(g, band, g.order.band(p)));
    }
    out
}

/// The gaps and questions a ticket relies on, in its relies_on order, with
/// their status. A reference to a ticket that does not exist is left out; the
/// dangling lint reports it.
fn waits_on(g: &Graph, t: &Ticket) -> Vec<Summary> {
    t.relies_on
        .iter()
        .filter_map(|r| match g.get(r) {
            Some(Node::Ticket(x)) if x.kind != TicketKind::Change => Some(ticket_summary(x)),
            _ => None,
        })
        .collect()
}

/// The changes, of any status, that rely on a gap or question, in id order.
fn followed_by(g: &Graph, t: &Ticket) -> Vec<Summary> {
    let r = Reference::ticket(&t.slug);
    let mut v: Vec<&Ticket> = g.tickets().filter(|c| c.kind == TicketKind::Change && c.relies_on.contains(&r)).collect();
    v.sort_by(|a, b| a.slug.cmp(&b.slug));
    v.into_iter().map(ticket_summary).collect()
}

/// Recency of a bottled ticket: its decision's date, then its id, a generated
/// number being newer than any named slug and a larger number newer than a
/// smaller one. A decision carries a date and no time.
fn newest_decision(t: &Ticket) -> (String, Option<u64>, String) {
    let date = t.decision.as_ref().map(|d| d.date.clone()).unwrap_or_default();
    (date, t.slug.parse::<u64>().ok(), t.slug.clone())
}

/// One band: the tickets its order list names, in that order; then the rest,
/// each after any ticket not bottled that it relies on, choosing among those
/// ready by simmering before brewing before inscribed, then title. A loop of relies_on is broken by
/// the same rule. `band` arrives sorted by status and title.
fn band_order<'a>(g: &Graph, band: Vec<&'a Ticket>, list: &[String]) -> Vec<&'a Ticket> {
    let mut out: Vec<&Ticket> = list.iter().filter_map(|s| band.iter().copied().find(|t| &t.slug == s)).collect();
    let mut rest: Vec<&Ticket> = band.into_iter().filter(|t| !list.contains(&t.slug)).collect();
    let waiting_on = |t: &Ticket, pending: &[&Ticket]| {
        t.relies_on.iter().any(|r| r.kind == Kind::Ticket && r.id != t.slug && pending.iter().any(|p| p.slug == r.id) && matches!(g.get(r), Some(Node::Ticket(x)) if x.status != Status::Bottled))
    };
    while !rest.is_empty() {
        // The first ready ticket in status-then-title order, or, when every one
        // waits on another (a loop), the first of them.
        let i = rest.iter().position(|t| !waiting_on(t, &rest)).unwrap_or(0);
        out.push(rest.remove(i));
    }
    out
}

pub fn related_tickets(g: &Graph, ticket: &Ticket) -> Vec<Summary> {
    let touches: HashSet<&Reference> = ticket.touches.iter().collect();
    let mut v: Vec<&Ticket> = g
        .tickets()
        .filter(|t| t.slug != ticket.slug && t.status != Status::Bottled && matches!(t.kind, TicketKind::Gap | TicketKind::Question))
        .filter(|t| t.touches.iter().chain(t.relies_on.iter()).any(|x| touches.contains(x)))
        .collect();
    v.sort_by(|a, b| a.priority.cmp(&b.priority).then_with(|| a.title.cmp(&b.title)));
    v.into_iter().map(ticket_summary).collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::graph::fixtures::graph;

    fn band(order: &[&str], tickets: &[(&str, Status, &[&str])]) -> Vec<String> {
        let mut s = crate::graph::fixtures::store();
        s.tickets.clear();
        for (slug, status, relies) in tickets {
            let rel: Vec<&str> = relies.to_vec();
            let mut t = crate::graph::fixtures::ticket(slug, TicketKind::Change, Priority::High, *status, &[], &rel);
            t.title = slug.to_uppercase();
            s.tickets.insert(slug.to_string(), t);
        }
        s.config.order.high = order.iter().map(|o| o.to_string()).collect();
        let g = Graph::build(&s, &crate::extraction::fixtures::extraction(), &[]);
        board(&g, &BoardFilter { status: None, kinds: None, priority: None, touching: None, bottled: 0 }).tickets.into_iter().map(|t| t.summary.r#ref.id).collect()
    }

    /// # Test contract
    ///
    /// ```json
    /// {"test": "a-band-is-ordered-by-list-then-dependency", "checks": ["crates/core/src/query/board#board-order"]}
    /// ```
    #[test]
    fn a_band_is_ordered_by_list_then_dependency() {
        use Status::*;
        // No list: a ticket after the ticket not yet bottled it relies on, otherwise brewing first, then title.
        assert_eq!(band(&[], &[("a", Inscribed, &["ticket:c"]), ("b", Inscribed, &[]), ("c", Inscribed, &[])]), vec!["b", "c", "a"]);
        assert_eq!(band(&[], &[("a", Inscribed, &[]), ("z", Brewing, &[])]), vec!["z", "a"], "brewing before inscribed");
        assert_eq!(band(&[], &[("a", Inscribed, &[]), ("m", Status::Simmering, &[]), ("z", Brewing, &[])]), vec!["m", "z", "a"], "a simmering change is listed, and first");
        // A list comes first, in its order; the rest follow.
        assert_eq!(band(&["b"], &[("a", Inscribed, &["ticket:c"]), ("b", Inscribed, &[]), ("c", Inscribed, &[])]), vec!["b", "c", "a"]);
        assert_eq!(band(&["a", "c"], &[("a", Inscribed, &["ticket:c"]), ("b", Inscribed, &[]), ("c", Inscribed, &[])]), vec!["a", "c", "b"], "a person's order wins over dependency");
        // A loop is broken by title, and a listed ticket that closed is ignored.
        assert_eq!(band(&["gone"], &[("a", Inscribed, &["ticket:b"]), ("b", Inscribed, &["ticket:a"])]), vec!["a", "b"]);
    }

    /// # Test contract
    ///
    /// ```json
    /// {"test": "board-orders-and-filters", "checks": ["crates/core/src/query/board#board-default-and-filters", "crates/core/src/query/board#board-order"]}
    /// ```
    #[test]
    fn board_orders_and_filters() {
        let g = graph();
        let b = board(&g, &BoardFilter { status: None, kinds: None, priority: None, touching: None, bottled: 0 });
        let slugs: Vec<&str> = b.tickets.iter().map(|s| s.summary.r#ref.id.as_str()).collect();
        assert_eq!(slugs, vec!["phase-62", "later-1"]);
        let touching = Reference::module("src/a/ready");
        let b = board(&g, &BoardFilter { status: None, kinds: Some(&[TicketKind::Gap]), priority: None, touching: Some(&touching), bottled: 0 });
        assert_eq!(b.tickets.len(), 1);
        assert_eq!(b.tickets[0].summary.r#ref.id, "gap-1");
        assert!(b.tickets[0].waits_on.is_none(), "only a change waits on anything");
    }

    /// # Test contract
    ///
    /// ```json
    /// {"test": "a-change-waits-on-its-gaps-and-questions", "checks": ["crates/core/src/query/board#board-waits-on", "crates/core/src/query/board#board-unattached"]}
    /// ```
    #[test]
    fn a_change_waits_on_its_gaps_and_questions() {
        use crate::graph::fixtures::ticket;
        let mut s = crate::graph::fixtures::store();
        s.tickets.clear();
        for t in [
            ticket("work", TicketKind::Change, Priority::High, Status::Inscribed, &[], &["ticket:q", "ticket:g-done", "ticket:g-waits", "ticket:other", "req:ready-only-with-commit"]),
            ticket("other", TicketKind::Change, Priority::High, Status::Inscribed, &[], &[]),
            ticket("q", TicketKind::Question, Priority::High, Status::Inscribed, &[], &[]),
            ticket("g-done", TicketKind::Gap, Priority::Later, Status::Bottled, &[], &[]),
            // Answered, and waiting for the change that applies it: not unattached, and not holding the change back.
            ticket("g-waits", TicketKind::Gap, Priority::Later, Status::Simmering, &[], &[]),
            ticket("loose", TicketKind::Gap, Priority::Later, Status::Inscribed, &[], &[]),
            // A question only a bottled change relied on is unattached again.
            ticket("orphaned", TicketKind::Question, Priority::Later, Status::Inscribed, &[], &[]),
            ticket("done", TicketKind::Change, Priority::Later, Status::Bottled, &[], &["ticket:orphaned"]),
        ] {
            s.tickets.insert(t.slug.clone(), t);
        }
        let g = Graph::build(&s, &crate::extraction::fixtures::extraction(), &[]);
        let b = board(&g, &BoardFilter { status: None, kinds: None, priority: None, touching: None, bottled: 0 });
        let work = b.tickets.iter().find(|t| t.summary.r#ref.id == "work").expect("listed");
        let waits: Vec<(String, Option<Status>)> = work.waits_on.as_ref().expect("a change").iter().map(|w| (w.r#ref.id.clone(), w.status)).collect();
        assert_eq!(
            waits,
            vec![("q".to_string(), Some(Status::Inscribed)), ("g-done".to_string(), Some(Status::Bottled)), ("g-waits".to_string(), Some(Status::Simmering))],
            "gaps and questions only, in relies_on order"
        );
        // Only q still holds it back: a simmering answer is answered, waiting for this change.
        let unanswered: Vec<&str> = work.waits_on.as_ref().expect("a change").iter().filter(|w| matches!(w.status, Some(Status::Inscribed | Status::Brewing))).map(|w| w.r#ref.id.as_str()).collect();
        assert_eq!(unanswered, vec!["q"]);
        let other = b.tickets.iter().find(|t| t.summary.r#ref.id == "other").expect("listed");
        assert_eq!(other.waits_on.as_deref(), Some(&[][..]), "a ready change waits on nothing");
        let unattached: Vec<&str> = b.unattached.iter().map(|t| t.summary.r#ref.id.as_str()).collect();
        assert_eq!(unattached, vec!["loose", "orphaned"]);
        assert!(b.bottled.is_empty(), "closed only when asked for");
        // Ranked as the changes are: the band's order list first, whichever list a ticket is in.
        s.config.order.later = vec!["orphaned".into(), "other".into(), "loose".into()];
        let g = Graph::build(&s, &crate::extraction::fixtures::extraction(), &[]);
        let b = board(&g, &BoardFilter { status: None, kinds: None, priority: None, touching: None, bottled: 0 });
        let unattached: Vec<&str> = b.unattached.iter().map(|t| t.summary.r#ref.id.as_str()).collect();
        assert_eq!(unattached, vec!["orphaned", "loose"], "the order list ranks the unsettled too");
    }

    /// # Test contract
    ///
    /// ```json
    /// {"test": "the-most-recently-bottled-first", "checks": ["crates/core/src/query/board#board-bottled"]}
    /// ```
    #[test]
    fn the_most_recently_bottled_first() {
        use crate::graph::fixtures::ticket;
        let mut s = crate::graph::fixtures::store();
        s.tickets.clear();
        for (slug, date) in [("named", "2026-09-28"), ("0009", "2026-09-27"), ("0010", "2026-09-28"), ("0002", "2026-09-28")] {
            let mut t = ticket(slug, TicketKind::Change, Priority::High, Status::Bottled, &[], &[]);
            t.decision = Some(Decision::new(date, "Done."));
            s.tickets.insert(slug.into(), t);
        }
        let g = Graph::build(&s, &crate::extraction::fixtures::extraction(), &[]);
        let b = board(&g, &BoardFilter { status: None, kinds: None, priority: None, touching: None, bottled: 3 });
        let closed: Vec<&str> = b.bottled.iter().map(|t| t.summary.r#ref.id.as_str()).collect();
        assert_eq!(closed, vec!["0010", "0002", "named"], "newest date, then the newest generated id, then a named slug");
    }

    /// # Test contract
    ///
    /// ```json
    /// {"test": "an-answer-carries-the-changes-that-followed-it", "checks": ["crates/core/src/query/board#board-bottled"]}
    /// ```
    #[test]
    fn an_answer_carries_the_changes_that_followed_it() {
        use crate::graph::fixtures::ticket;
        let mut s = crate::graph::fixtures::store();
        s.tickets.clear();
        let closed = |slug: &str, kind: TicketKind, relies: &[&str]| {
            let mut t = ticket(slug, kind, Priority::Later, Status::Bottled, &[], relies);
            t.decision = Some(Decision::new("2026-09-29", "Answered."));
            t
        };
        for t in [
            closed("q-built", TicketKind::Question, &[]),
            closed("q-alone", TicketKind::Question, &[]),
            closed("g-open", TicketKind::Gap, &[]),
            closed("done", TicketKind::Change, &["ticket:q-built"]),
            ticket("b-work", TicketKind::Change, Priority::High, Status::Inscribed, &[], &["ticket:q-built", "ticket:g-open"]),
            ticket("a-work", TicketKind::Change, Priority::High, Status::Brewing, &[], &["ticket:q-built"]),
        ] {
            s.tickets.insert(t.slug.clone(), t);
        }
        let g = Graph::build(&s, &crate::extraction::fixtures::extraction(), &[]);
        let b = board(&g, &BoardFilter { status: None, kinds: None, priority: None, touching: None, bottled: 10 });
        let followed = |slug: &str| b.bottled.iter().find(|t| t.summary.r#ref.id == slug).expect("closed").followed_by.as_ref().map(|v| v.iter().map(|s| s.r#ref.id.clone()).collect::<Vec<_>>());
        assert_eq!(followed("q-built"), Some(vec!["a-work".into(), "b-work".into(), "done".into()]), "changes of any status, in id order");
        assert_eq!(followed("g-open"), Some(vec!["b-work".into()]));
        assert_eq!(followed("q-alone"), Some(vec![]), "an answer with nothing built on it");
        assert_eq!(followed("done"), None, "only a gap or question is followed");
        assert!(b.tickets.iter().all(|t| t.followed_by.is_none()), "only the closed list carries it");
    }

    /// # Test contract
    ///
    /// ```json
    /// {"test": "related-finds-gaps-on-shared-nodes", "checks": ["crates/core/src/query/board#related-tickets"]}
    /// ```
    #[test]
    fn related_finds_gaps_on_shared_nodes() {
        let g = graph();
        let t = match g.get(&Reference::ticket("phase-62")).unwrap() {
            Node::Ticket(t) => t.clone(),
            _ => unreachable!(),
        };
        let r = related_tickets(&g, &t);
        assert_eq!(r.len(), 1);
        assert_eq!(r[0].r#ref.id, "gap-1");
    }
}
