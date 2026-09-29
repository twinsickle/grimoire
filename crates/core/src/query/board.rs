//! # Module contract
//!
//! ```json
//! {"purpose":"Answer board and related_tickets: the tickets to work on in order, with what each waits on, and the open gaps and questions that share a ticket's nodes.","criteria":[{"id":"board","statement":"board returns open and active change tickets by default, filterable by status, kind, priority and a touched node, ordered next, high, later; within a band the tickets its order list names come first in that order, then the rest with each after any open ticket it relies on, then active before open, then title, a loop broken by that same rule. Each change carries waits_on, the gaps and questions it relies on with their status; unattached lists the open and active gaps and questions no open or active change relies on; closed lists the requested number of most recently decided closed tickets."},{"id":"related-tickets","statement":"related_tickets returns the open gaps and questions whose touches or relies_on share a reference with the given ticket's touches."}]}
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
    /// How many of the most recently closed tickets to return.
    pub closed: usize,
}

/// The board's answer: the tickets asked for, each change with what it waits
/// on; the open gaps and questions no open change relies on; and the most
/// recently closed tickets, when asked for.
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[derive(Debug, Clone, Serialize)]
pub struct Board {
    pub tickets: Vec<BoardTicket>,
    pub unattached: Vec<Summary>,
    pub closed: Vec<Summary>,
}

/// A ticket on the board. A change carries `waits_on`: the gaps and
/// questions it relies on with their status, from which its readiness
/// follows; it is ready when none of them is still open or active.
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[derive(Debug, Clone, Serialize)]
pub struct BoardTicket {
    #[serde(flatten)]
    pub summary: Summary,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[cfg_attr(feature = "ts", ts(optional))]
    pub waits_on: Option<Vec<Summary>>,
}

pub fn board(g: &Graph, f: &BoardFilter) -> Board {
    let default_status = [Status::Open, Status::Active];
    let default_kinds = [TicketKind::Change];
    let status = f.status.unwrap_or(&default_status);
    let kinds = f.kinds.unwrap_or(&default_kinds);
    let narrowed = |t: &&Ticket| f.priority.is_none_or(|p| p.contains(&t.priority)) && f.touching.is_none_or(|x| t.touches.contains(x) || t.relies_on.contains(x));
    let tickets: Vec<&Ticket> = g.tickets().filter(|t| status.contains(&t.status) && kinds.contains(&t.kind)).filter(narrowed).collect();
    let tickets = ordered(g, tickets)
        .into_iter()
        .map(|t| BoardTicket { summary: ticket_summary(t), waits_on: (t.kind == TicketKind::Change).then(|| waits_on(g, t)) })
        .collect();
    // Gaps and questions an open or active change relies on are attached to it.
    let attached: HashSet<&Reference> = g.tickets().filter(|t| t.kind == TicketKind::Change && t.status != Status::Closed).flat_map(|t| t.relies_on.iter()).collect();
    let unattached = g
        .tickets()
        .filter(|t| t.kind != TicketKind::Change && t.status != Status::Closed)
        .filter(|t| !attached.contains(&Reference::ticket(&t.slug)))
        .filter(narrowed)
        .collect();
    let unattached = ordered(g, unattached).into_iter().map(ticket_summary).collect();
    let mut closed: Vec<&Ticket> = g.tickets().filter(|t| t.status == Status::Closed).filter(narrowed).collect();
    closed.sort_by_key(|t| std::cmp::Reverse(newest_decision(t)));
    let closed = closed.into_iter().take(f.closed).map(ticket_summary).collect();
    Board { tickets, unattached, closed }
}

/// Tickets in board order: next, high, later, each band by its order list,
/// then dependency, then active before open, then title.
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

/// Recency of a closed ticket: its decision's date, then its id, a generated
/// number being newer than any named slug and a larger number newer than a
/// smaller one. A decision carries a date and no time.
fn newest_decision(t: &Ticket) -> (String, Option<u64>, String) {
    let date = t.decision.as_ref().map(|d| d.date.clone()).unwrap_or_default();
    (date, t.slug.parse::<u64>().ok(), t.slug.clone())
}

/// One band: the tickets its order list names, in that order; then the rest,
/// each after any open or active ticket it relies on, choosing among those
/// ready by active before open, then title. A loop of relies_on is broken by
/// the same rule. `band` arrives sorted by status and title.
fn band_order<'a>(g: &Graph, band: Vec<&'a Ticket>, list: &[String]) -> Vec<&'a Ticket> {
    let mut out: Vec<&Ticket> = list.iter().filter_map(|s| band.iter().copied().find(|t| &t.slug == s)).collect();
    let mut rest: Vec<&Ticket> = band.into_iter().filter(|t| !list.contains(&t.slug)).collect();
    let waiting_on = |t: &Ticket, pending: &[&Ticket]| {
        t.relies_on.iter().any(|r| r.kind == Kind::Ticket && r.id != t.slug && pending.iter().any(|p| p.slug == r.id) && matches!(g.get(r), Some(Node::Ticket(x)) if x.status != Status::Closed))
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
        .filter(|t| t.slug != ticket.slug && t.status != Status::Closed && matches!(t.kind, TicketKind::Gap | TicketKind::Question))
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
        board(&g, &BoardFilter { status: None, kinds: None, priority: None, touching: None, closed: 0 }).tickets.into_iter().map(|t| t.summary.r#ref.id).collect()
    }

    /// # Test contract
    ///
    /// ```json
    /// {"test": "a-band-is-ordered-by-list-then-dependency", "checks": ["crates/core/src/query/board#board"]}
    /// ```
    #[test]
    fn a_band_is_ordered_by_list_then_dependency() {
        use Status::*;
        // No list: a ticket after the open ticket it relies on, otherwise active, then title.
        assert_eq!(band(&[], &[("a", Open, &["ticket:c"]), ("b", Open, &[]), ("c", Open, &[])]), vec!["b", "c", "a"]);
        assert_eq!(band(&[], &[("a", Open, &[]), ("z", Active, &[])]), vec!["z", "a"], "active before open");
        // A list comes first, in its order; the rest follow.
        assert_eq!(band(&["b"], &[("a", Open, &["ticket:c"]), ("b", Open, &[]), ("c", Open, &[])]), vec!["b", "c", "a"]);
        assert_eq!(band(&["a", "c"], &[("a", Open, &["ticket:c"]), ("b", Open, &[]), ("c", Open, &[])]), vec!["a", "c", "b"], "a person's order wins over dependency");
        // A loop is broken by title, and a listed ticket that closed is ignored.
        assert_eq!(band(&["gone"], &[("a", Open, &["ticket:b"]), ("b", Open, &["ticket:a"])]), vec!["a", "b"]);
    }

    /// # Test contract
    ///
    /// ```json
    /// {"test": "board-orders-and-filters", "checks": ["crates/core/src/query/board#board"]}
    /// ```
    #[test]
    fn board_orders_and_filters() {
        let g = graph();
        let b = board(&g, &BoardFilter { status: None, kinds: None, priority: None, touching: None, closed: 0 });
        let slugs: Vec<&str> = b.tickets.iter().map(|s| s.summary.r#ref.id.as_str()).collect();
        assert_eq!(slugs, vec!["phase-62", "later-1"]);
        let touching = Reference::module("src/a/ready");
        let b = board(&g, &BoardFilter { status: None, kinds: Some(&[TicketKind::Gap]), priority: None, touching: Some(&touching), closed: 0 });
        assert_eq!(b.tickets.len(), 1);
        assert_eq!(b.tickets[0].summary.r#ref.id, "gap-1");
        assert!(b.tickets[0].waits_on.is_none(), "only a change waits on anything");
    }

    /// # Test contract
    ///
    /// ```json
    /// {"test": "a-change-waits-on-its-gaps-and-questions", "checks": ["crates/core/src/query/board#board"]}
    /// ```
    #[test]
    fn a_change_waits_on_its_gaps_and_questions() {
        use crate::graph::fixtures::ticket;
        let mut s = crate::graph::fixtures::store();
        s.tickets.clear();
        for t in [
            ticket("work", TicketKind::Change, Priority::High, Status::Open, &[], &["ticket:q", "ticket:g-done", "ticket:other", "req:ready-only-with-commit"]),
            ticket("other", TicketKind::Change, Priority::High, Status::Open, &[], &[]),
            ticket("q", TicketKind::Question, Priority::High, Status::Open, &[], &[]),
            ticket("g-done", TicketKind::Gap, Priority::Later, Status::Closed, &[], &[]),
            ticket("loose", TicketKind::Gap, Priority::Later, Status::Open, &[], &[]),
            // A question only a closed change relied on is unattached again.
            ticket("orphaned", TicketKind::Question, Priority::Later, Status::Open, &[], &[]),
            ticket("done", TicketKind::Change, Priority::Later, Status::Closed, &[], &["ticket:orphaned"]),
        ] {
            s.tickets.insert(t.slug.clone(), t);
        }
        let g = Graph::build(&s, &crate::extraction::fixtures::extraction(), &[]);
        let b = board(&g, &BoardFilter { status: None, kinds: None, priority: None, touching: None, closed: 0 });
        let work = b.tickets.iter().find(|t| t.summary.r#ref.id == "work").expect("listed");
        let waits: Vec<(String, Option<Status>)> = work.waits_on.as_ref().expect("a change").iter().map(|w| (w.r#ref.id.clone(), w.status)).collect();
        assert_eq!(waits, vec![("q".to_string(), Some(Status::Open)), ("g-done".to_string(), Some(Status::Closed))], "gaps and questions only, in relies_on order");
        let other = b.tickets.iter().find(|t| t.summary.r#ref.id == "other").expect("listed");
        assert_eq!(other.waits_on.as_deref(), Some(&[][..]), "a ready change waits on nothing");
        let unattached: Vec<&str> = b.unattached.iter().map(|t| t.r#ref.id.as_str()).collect();
        assert_eq!(unattached, vec!["loose", "orphaned"]);
        assert!(b.closed.is_empty(), "closed only when asked for");
    }

    /// # Test contract
    ///
    /// ```json
    /// {"test": "the-most-recently-closed-first", "checks": ["crates/core/src/query/board#board"]}
    /// ```
    #[test]
    fn the_most_recently_closed_first() {
        use crate::graph::fixtures::ticket;
        let mut s = crate::graph::fixtures::store();
        s.tickets.clear();
        for (slug, date) in [("named", "2026-09-28"), ("0009", "2026-09-27"), ("0010", "2026-09-28"), ("0002", "2026-09-28")] {
            let mut t = ticket(slug, TicketKind::Change, Priority::High, Status::Closed, &[], &[]);
            t.decision = Some(Decision { date: date.into(), text: "Done.".into() });
            s.tickets.insert(slug.into(), t);
        }
        let g = Graph::build(&s, &crate::extraction::fixtures::extraction(), &[]);
        let b = board(&g, &BoardFilter { status: None, kinds: None, priority: None, touching: None, closed: 3 });
        let closed: Vec<&str> = b.closed.iter().map(|t| t.r#ref.id.as_str()).collect();
        assert_eq!(closed, vec!["0010", "0002", "named"], "newest date, then the newest generated id, then a named slug");
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
