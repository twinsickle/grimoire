//! # Module contract
//!
//! ```json
//! {"purpose":"Apply the write tools to a store value: put a page or requirement, record a decision, file, update, start and close a ticket, delete a node, adopt a baseline, configure the store.","criteria":[{"id":"configure","statement":"Configuring changes only the roots, languages and scope it is given, refuses a language the build cannot extract, validates the whole config, and never changes the repository."},{"id":"adopt","statement":"Adopting replaces the baseline with the given findings, dated, with a required reason, and reports how many the previous baseline held."},{"id":"validate-first","statement":"A write validates the whole resulting record before changing the store, and an invalid record changes nothing."},{"id":"decisions-append-only","statement":"No write replaces or removes a decision; put_requirement preserves the existing decisions and record_decision appends."},{"id":"never-fails-on-lint","statement":"A write that creates a finding (a dangling edge, or a requirement with no scenario) succeeds; the lint result is the caller's to report."},{"id":"close-keeps-only-decision","statement":"Closing a ticket replaces its body with the empty string and sets its decision; a closed ticket cannot be reopened or edited."},{"id":"patch-is-partial","statement":"patch_page changes only the fields given: added links go at the end without duplicates, removing an absent link is not an error, and every other field stays as it was."},{"id":"put-page","statement":"put_page creates or replaces a page by slug; replacing is not a conflict."},{"id":"put-requirement","statement":"put_requirement creates or replaces a requirement by slug, ignoring decisions on input and preserving the existing ones."},{"id":"record-decision","statement":"record_decision appends a dated decision to an existing requirement, defaulting the date to today, and is not_found otherwise."},{"id":"file-ticket","statement":"file_ticket creates an open ticket with a null decision under a generated id: one past the highest all-digit id in the store, zero-padded to four digits, so filing never collides and never asks for a name."},{"id":"update-ticket","statement":"update_ticket changes only the given title, priority, touches, relies_on or body of an open or active ticket, and is a conflict on a closed one."},{"id":"start-ticket","statement":"start_ticket moves a ticket from open to active and is a conflict from any other status."},{"id":"delete-node","statement":"delete_node removes a page, requirement or ticket; derived kinds are invalid and a missing node is not_found."}]}
//! ```

use serde::Serialize;

use crate::model::*;
use crate::store::{validate_baseline, validate_config, validate_page, validate_requirement, validate_ticket, FieldError, Store};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, thiserror::Error)]
#[serde(tag = "error", rename_all = "snake_case")]
pub enum ToolError {
    #[error("{message}")]
    NotFound { message: String },
    #[error("{message}")]
    Invalid {
        message: String,
        details: Vec<FieldError>,
    },
    #[error("{message}")]
    Conflict { message: String },
    #[error("{message}")]
    StoreUnavailable { message: String },
}

impl ToolError {
    pub fn not_found(r: &Reference) -> Self {
        ToolError::NotFound { message: format!("{r} does not exist") }
    }
    pub fn invalid(message: impl Into<String>, details: Vec<FieldError>) -> Self {
        ToolError::Invalid { message: message.into(), details }
    }
    pub fn conflict(message: impl Into<String>) -> Self {
        ToolError::Conflict { message: message.into() }
    }
    pub fn unavailable(message: impl Into<String>) -> Self {
        ToolError::StoreUnavailable { message: message.into() }
    }
    pub fn code(&self) -> &'static str {
        match self {
            ToolError::NotFound { .. } => "not_found",
            ToolError::Invalid { .. } => "invalid",
            ToolError::Conflict { .. } => "conflict",
            ToolError::StoreUnavailable { .. } => "store_unavailable",
        }
    }
}

fn invalid_if(errors: Vec<FieldError>, what: &str) -> Result<(), ToolError> {
    if errors.is_empty() {
        Ok(())
    } else {
        Err(ToolError::invalid(format!("{what} violates the schema"), errors))
    }
}

/// The stored kinds a write changed, so a caller can persist exactly those.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Changed {
    Page(String),
    Requirement(String),
    Ticket(String),
    Deleted(Reference),
    Baseline,
    Config,
    /// A ticket started: its record and its snapshot.
    Started(String),
    /// A ticket closed: its record, and its snapshot removed.
    Closed(String),
}

/// The config fields `configure` may change; the repository is not one.
#[derive(Debug, Clone, Default, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ConfigChange {
    pub roots: Option<Vec<String>>,
    pub languages: Option<Vec<String>>,
    pub scope: Option<Scope>,
}

/// Change the fields given. `supported` is the languages the running build
/// can extract; empty means unknown, and then any language is accepted.
pub fn configure(store: &mut Store, change: ConfigChange, supported: &[String]) -> Result<(StoreConfig, Changed), ToolError> {
    let mut config = store.config.clone();
    if let Some(roots) = change.roots {
        config.roots = roots;
    }
    if let Some(languages) = change.languages {
        config.languages = languages;
    }
    if let Some(scope) = change.scope {
        config.scope = scope;
    }
    let mut errors = validate_config(&config);
    if !supported.is_empty() {
        for l in config.languages.iter().filter(|l| !supported.contains(l)) {
            errors.push(FieldError::new("languages", format!("`{l}` is not supported by this build, which supports {}", supported.join(", "))));
        }
    }
    invalid_if(errors, "config")?;
    store.config = config.clone();
    Ok((config, Changed::Config))
}

/// Replace the baseline with `findings`. Returns the new baseline and how
/// many findings the previous one held, if there was one.
pub fn adopt(store: &mut Store, findings: Vec<FindingKey>, reason: String, date: String) -> Result<(Baseline, Option<usize>, Changed), ToolError> {
    let baseline = Baseline { date, reason, findings };
    invalid_if(validate_baseline(&baseline), "baseline")?;
    let replaced = store.baseline.replace(baseline.clone()).map(|b| b.findings.len());
    Ok((baseline, replaced, Changed::Baseline))
}

pub fn put_page(store: &mut Store, page: Page) -> Result<(Page, Changed), ToolError> {
    invalid_if(validate_page(&page), "page")?;
    store.pages.insert(page.slug.clone(), page.clone());
    Ok((page.clone(), Changed::Page(page.slug)))
}

/// A partial change to a page. Only given fields change; links are added at
/// the end in the order given, never duplicated, and removing an absent
/// link is not an error.
#[derive(Default)]
pub struct PagePatch {
    pub title: Option<String>,
    pub body: Option<String>,
    pub add_links: Vec<Reference>,
    pub remove_links: Vec<Reference>,
}

pub fn patch_page(store: &mut Store, r: &Reference, patch: PagePatch) -> Result<(Page, Changed), ToolError> {
    if r.kind != Kind::Page {
        return Err(ToolError::invalid(format!("{r} is not a page"), vec![]));
    }
    let existing = store.pages.get(&r.id).ok_or_else(|| ToolError::not_found(r))?;
    let mut page = existing.clone();
    if let Some(t) = patch.title {
        page.title = t;
    }
    if let Some(b) = patch.body {
        page.body = b;
    }
    page.links.retain(|l| !patch.remove_links.contains(l));
    for l in patch.add_links {
        if !page.links.contains(&l) {
            page.links.push(l);
        }
    }
    invalid_if(validate_page(&page), "page")?;
    store.pages.insert(page.slug.clone(), page.clone());
    Ok((page, Changed::Page(r.id.clone())))
}

/// `decisions` on the input is ignored; the existing ones are kept.
pub fn put_requirement(store: &mut Store, mut req: Requirement) -> Result<(Requirement, Changed), ToolError> {
    req.decisions = store.requirements.get(&req.slug).map(|r| r.decisions.clone()).unwrap_or_default();
    invalid_if(validate_requirement(&req), "requirement")?;
    store.requirements.insert(req.slug.clone(), req.clone());
    Ok((req.clone(), Changed::Requirement(req.slug)))
}

pub fn record_decision(store: &mut Store, r: &Reference, text: String, date: String) -> Result<(Requirement, Changed), ToolError> {
    if r.kind != Kind::Req {
        return Err(ToolError::invalid(format!("{r} is not a requirement"), vec![]));
    }
    let existing = store.requirements.get(&r.id).ok_or_else(|| ToolError::not_found(r))?;
    let mut req = existing.clone();
    req.decisions.push(Decision { date, text });
    invalid_if(validate_requirement(&req), "requirement")?;
    store.requirements.insert(req.slug.clone(), req.clone());
    Ok((req.clone(), Changed::Requirement(req.slug)))
}

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
    let ticket = Ticket {
        slug,
        title: t.title,
        kind: t.kind,
        priority: t.priority,
        status: Status::Open,
        touches: t.touches,
        relies_on: t.relies_on,
        body: t.body,
        decision: None,
    };
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
    if let Some(v) = patch.body {
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
    Ok((t.clone(), Changed::Ticket(r.id.clone())))
}

pub fn close_ticket(store: &mut Store, r: &Reference, decision: Decision) -> Result<(Ticket, Changed), ToolError> {
    let existing = ticket_mut(store, r)?;
    if existing.status == Status::Closed {
        return Err(ToolError::conflict(format!("{r} is already closed")));
    }
    let mut t = existing.clone();
    t.status = Status::Closed;
    t.body = String::new();
    t.decision = Some(decision);
    invalid_if(validate_ticket(&t), "ticket")?;
    *existing = t.clone();
    Ok((t, Changed::Ticket(r.id.clone())))
}

pub fn delete_node(store: &mut Store, r: &Reference) -> Result<Changed, ToolError> {
    let removed = match r.kind {
        Kind::Page => store.pages.remove(&r.id).is_some(),
        Kind::Req => store.requirements.remove(&r.id).is_some(),
        Kind::Ticket => store.tickets.remove(&r.id).is_some(),
        _ => return Err(ToolError::invalid(format!("{r} is derived from the repository and cannot be deleted"), vec![])),
    };
    if removed {
        Ok(Changed::Deleted(r.clone()))
    } else {
        Err(ToolError::not_found(r))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::graph::fixtures::store;

    /// # Test contract
    ///
    /// ```json
    /// {"test": "put-page-creates-and-replaces", "checks": ["crates/core/src/ops#put-page"]}
    /// ```
    #[test]
    fn put_page_creates_and_replaces() {
        let mut s = store();
        let page = |body: &str| Page { slug: "glossary".into(), title: "Glossary".into(), body: body.into(), links: vec![] };
        put_page(&mut s, page("first")).unwrap();
        let (again, _) = put_page(&mut s, page("second")).unwrap();
        assert_eq!(again.body, "second", "replacing by slug is not a conflict");
        assert_eq!(s.pages["glossary"].body, "second");
    }

    /// # Test contract
    ///
    /// ```json
    /// {"test": "patch-page-is-partial", "checks": ["crates/core/src/ops#patch-is-partial"]}
    /// ```
    #[test]
    fn patch_page_is_partial() {
        let mut s = store();
        let r = Reference::page("git-readiness");
        let before = s.pages["git-readiness"].clone();
        let (p, _) = patch_page(&mut s, &r, PagePatch { add_links: vec![Reference::page("orphan"), Reference::req("stale")], remove_links: vec![Reference::req("nope"), Reference::module("src/a/ready")], ..Default::default() }).unwrap();
        assert_eq!(p.title, before.title);
        assert_eq!(p.body, before.body);
        // stale was already present: not duplicated; ready removed; orphan appended.
        assert_eq!(p.links.iter().map(|l| l.to_string()).collect::<Vec<_>>(), vec!["req:ready-only-with-commit", "req:stale", "page:orphan"]);
        let (p, _) = patch_page(&mut s, &r, PagePatch { title: Some("Readiness".into()), ..Default::default() }).unwrap();
        assert_eq!(p.title, "Readiness");
        assert_eq!(p.links.len(), 3);
        // Invalid result changes nothing.
        let e = patch_page(&mut s, &r, PagePatch { add_links: vec![Reference::ticket("x")], ..Default::default() }).unwrap_err();
        assert_eq!(e.code(), "invalid");
        assert_eq!(s.pages["git-readiness"].links.len(), 3);
        assert_eq!(patch_page(&mut s, &Reference::page("nope"), PagePatch::default()).unwrap_err().code(), "not_found");
    }

    /// # Test contract
    ///
    /// ```json
    /// {"test": "put-requirement-keeps-decisions", "checks": ["crates/core/src/ops#decisions-append-only", "crates/core/src/ops#put-requirement"]}
    /// ```
    #[test]
    fn put_requirement_keeps_decisions() {
        let mut s = store();
        let mut r = s.requirements["ready-only-with-commit"].clone();
        r.decisions = vec![];
        r.title = "Changed".into();
        let (out, _) = put_requirement(&mut s, r).unwrap();
        assert_eq!(out.decisions.len(), 1);
        assert_eq!(out.title, "Changed");
    }

    /// # Test contract
    ///
    /// ```json
    /// {"test": "record-decision-appends-and-validates", "checks": ["crates/core/src/ops#validate-first", "crates/core/src/ops#decisions-append-only", "crates/core/src/ops#record-decision"]}
    /// ```
    #[test]
    fn record_decision_appends_and_validates() {
        let mut s = store();
        let r = Reference::req("ready-only-with-commit");
        let (out, _) = record_decision(&mut s, &r, "Because.".into(), "2026-09-27".into()).unwrap();
        assert_eq!(out.decisions.len(), 2);
        let e = record_decision(&mut s, &r, "x".into(), "not-a-date".into()).unwrap_err();
        assert_eq!(e.code(), "invalid");
        assert_eq!(s.requirements["ready-only-with-commit"].decisions.len(), 2);
        assert_eq!(record_decision(&mut s, &Reference::req("nope"), "x".into(), "2026-01-01".into()).unwrap_err().code(), "not_found");
    }

    /// # Test contract
    ///
    /// ```json
    /// {"test": "ticket-lifecycle", "checks": ["crates/core/src/ops#close-keeps-only-decision", "crates/core/src/ops#update-ticket", "crates/core/src/ops#start-ticket"]}
    /// ```
    #[test]
    fn ticket_lifecycle() {
        let mut s = store();
        let r = Reference::ticket("phase-62");
        assert_eq!(start_ticket(&mut s, &r).unwrap().0.status, Status::Active);
        assert_eq!(start_ticket(&mut s, &r).unwrap_err().code(), "conflict");
        let (t, _) = update_ticket(&mut s, &r, TicketPatch { priority: Some(Priority::Later), ..Default::default() }).unwrap();
        assert_eq!(t.priority, Priority::Later);
        let (t, _) = close_ticket(&mut s, &r, Decision { date: "2026-09-27".into(), text: "Shipped.".into() }).unwrap();
        assert_eq!(t.status, Status::Closed);
        assert_eq!(t.body, "");
        assert_eq!(update_ticket(&mut s, &r, TicketPatch::default()).unwrap_err().code(), "conflict");
        assert_eq!(close_ticket(&mut s, &r, Decision { date: "2026-09-27".into(), text: "again".into() }).unwrap_err().code(), "conflict");
    }

    /// # Test contract
    ///
    /// ```json
    /// {"test": "file-ticket-generates-ids", "checks": ["crates/core/src/ops#file-ticket"]}
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

    /// # Test contract
    ///
    /// ```json
    /// {"test": "delete-only-store-kinds", "checks": ["crates/core/src/ops#delete-node"]}
    /// ```
    #[test]
    fn delete_only_store_kinds() {
        let mut s = store();
        assert_eq!(delete_node(&mut s, &Reference::module("src/a/ready")).unwrap_err().code(), "invalid");
        assert!(delete_node(&mut s, &Reference::page("orphan")).is_ok());
        assert_eq!(delete_node(&mut s, &Reference::page("orphan")).unwrap_err().code(), "not_found");
    }
}
