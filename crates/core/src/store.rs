//! # Module contract
//!
//! ```json
//! {"purpose":"Hold the store's pages, requirements, tickets and baseline as values and validate each record against the schema.","criteria":[{"id":"scope-rules","statement":"A scope prefix is a non-empty repository-relative path, /-separated, with no . or .. segment, no leading or trailing slash and no drive."},{"id":"whole-record","statement":"Validation reports every violated field rule of a record in one pass, not only the first."},{"id":"slug-rule","statement":"A slug matches ^[a-z0-9]+(-[a-z0-9]+)*$, is at most 64 characters, and is unique within its kind."},{"id":"field-rules","statement":"Titles are one non-empty line, required strings are non-empty after trimming, a date is ISO YYYY-MM-DD, a ticket body is at most 4000 characters, and a ticket's decision is present exactly when its status is closed."}]}
//! ```

use std::collections::BTreeMap;

use serde::Serialize;

use crate::model::*;

/// The store as a value: the config and every record, keyed by slug.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Store {
    pub config: StoreConfig,
    pub pages: BTreeMap<String, Page>,
    pub requirements: BTreeMap<String, Requirement>,
    pub tickets: BTreeMap<String, Ticket>,
    /// The findings accepted as inherited at adoption; none until `adopt`.
    pub baseline: Option<Baseline>,
    /// The snapshot taken when each active ticket started, by slug.
    pub starts: BTreeMap<String, Start>,
}

impl Default for StoreConfig {
    fn default() -> Self {
        StoreConfig {
            version: StoreConfig::CURRENT_VERSION,
            repository: String::new(),
            roots: Vec::new(),
            scope: Scope::default(),
            order: Order::default(),
            languages: Vec::new(),
        }
    }
}

/// A store file that did not parse or violated a field rule. Carried into the
/// graph so the `malformed` lint can report it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Malformed {
    /// Store-relative path, `/`-separated.
    pub path: String,
    pub reason: String,
}

/// One violated field rule.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct FieldError {
    pub field: String,
    pub reason: String,
}

impl FieldError {
    pub(crate) fn new(field: &str, reason: impl Into<String>) -> Self {
        FieldError { field: field.to_string(), reason: reason.into() }
    }
}

pub const SLUG_MAX: usize = 64;
pub const TICKET_BODY_MAX: usize = 4000;

pub fn is_slug(s: &str) -> bool {
    if s.is_empty() || s.len() > SLUG_MAX {
        return false;
    }
    let mut prev_dash = true; // disallow leading dash
    for c in s.chars() {
        if c.is_ascii_lowercase() || c.is_ascii_digit() {
            prev_dash = false;
        } else if c == '-' && !prev_dash {
            prev_dash = true;
        } else {
            return false;
        }
    }
    !prev_dash
}

/// ISO `YYYY-MM-DD` with a plausible month and day.
pub fn is_date(s: &str) -> bool {
    let b = s.as_bytes();
    if b.len() != 10 || b[4] != b'-' || b[7] != b'-' {
        return false;
    }
    let digits = |r: std::ops::Range<usize>| s[r].chars().all(|c| c.is_ascii_digit());
    if !(digits(0..4) && digits(5..7) && digits(8..10)) {
        return false;
    }
    let m: u32 = s[5..7].parse().unwrap_or(0);
    let d: u32 = s[8..10].parse().unwrap_or(0);
    (1..=12).contains(&m) && (1..=31).contains(&d)
}

fn check_slug(errors: &mut Vec<FieldError>, field: &str, s: &str) {
    if !is_slug(s) {
        errors.push(FieldError::new(field, "must match ^[a-z0-9]+(-[a-z0-9]+)*$ and be at most 64 characters"));
    }
}

fn check_required(errors: &mut Vec<FieldError>, field: &str, s: &str) {
    if s.trim().is_empty() {
        errors.push(FieldError::new(field, "is required and may not be empty"));
    }
}

fn check_one_line(errors: &mut Vec<FieldError>, field: &str, s: &str) {
    check_required(errors, field, s);
    if s.contains('\n') {
        errors.push(FieldError::new(field, "must be one line"));
    }
}

fn check_refs(errors: &mut Vec<FieldError>, field: &str, refs: &[Reference], allowed: Option<&[Kind]>) {
    let mut seen = std::collections::HashSet::new();
    for r in refs {
        if !seen.insert(r) {
            errors.push(FieldError::new(field, format!("duplicate reference {r}")));
        }
        if let Some(allowed) = allowed {
            if !allowed.contains(&r.kind) {
                errors.push(FieldError::new(field, format!("{r} is not an allowed kind here")));
            }
        }
    }
}

fn check_decision(errors: &mut Vec<FieldError>, field: &str, d: &Decision) {
    if !is_date(&d.date) {
        errors.push(FieldError::new(&format!("{field}.date"), "must be an ISO date YYYY-MM-DD"));
    }
    check_required(errors, &format!("{field}.text"), &d.text);
}

pub fn validate_page(p: &Page) -> Vec<FieldError> {
    let mut e = Vec::new();
    check_slug(&mut e, "slug", &p.slug);
    check_one_line(&mut e, "title", &p.title);
    check_required(&mut e, "body", &p.body);
    check_refs(&mut e, "links", &p.links, Some(&[Kind::Page, Kind::Req, Kind::Module]));
    e
}

pub fn validate_requirement(r: &Requirement) -> Vec<FieldError> {
    let mut e = Vec::new();
    check_slug(&mut e, "slug", &r.slug);
    check_one_line(&mut e, "title", &r.title);
    check_required(&mut e, "statement", &r.statement);
    for (i, s) in r.scenarios.iter().enumerate() {
        check_one_line(&mut e, &format!("scenarios[{i}].title"), &s.title);
        check_required(&mut e, &format!("scenarios[{i}].given"), &s.given);
        check_required(&mut e, &format!("scenarios[{i}].when"), &s.when);
        check_required(&mut e, &format!("scenarios[{i}].then"), &s.then);
    }
    check_refs(&mut e, "covered_by", &r.covered_by, Some(&[Kind::Criterion]));
    for (i, d) in r.decisions.iter().enumerate() {
        check_decision(&mut e, &format!("decisions[{i}]"), d);
    }
    for w in r.decisions.windows(2) {
        if w[0].date > w[1].date {
            e.push(FieldError::new("decisions", "must be ordered oldest first"));
            break;
        }
    }
    e
}

pub fn validate_ticket(t: &Ticket) -> Vec<FieldError> {
    let mut e = Vec::new();
    check_slug(&mut e, "slug", &t.slug);
    check_one_line(&mut e, "title", &t.title);
    check_refs(&mut e, "touches", &t.touches, None);
    check_refs(&mut e, "relies_on", &t.relies_on, None);
    if t.body.chars().count() > TICKET_BODY_MAX {
        e.push(FieldError::new("body", format!("at most {TICKET_BODY_MAX} characters")));
    }
    match (t.status, &t.decision) {
        (Status::Closed, None) => e.push(FieldError::new("decision", "required when status is closed")),
        (Status::Closed, Some(d)) => check_decision(&mut e, "decision", d),
        (_, Some(_)) => e.push(FieldError::new("decision", "must be null unless status is closed")),
        (_, None) => {
            check_required(&mut e, "body", &t.body);
        }
    }
    e
}

pub fn validate_baseline(b: &Baseline) -> Vec<FieldError> {
    let mut e = Vec::new();
    if !is_date(&b.date) {
        e.push(FieldError::new("date", "must be an ISO date YYYY-MM-DD"));
    }
    check_required(&mut e, "reason", &b.reason);
    e
}

pub fn validate_config(c: &StoreConfig) -> Vec<FieldError> {
    let mut e = Vec::new();
    if c.version != StoreConfig::CURRENT_VERSION {
        e.push(FieldError::new("version", format!("unknown store version {}; this build knows {}", c.version, StoreConfig::CURRENT_VERSION)));
    }
    check_required(&mut e, "repository", &c.repository);
    if c.roots.is_empty() {
        e.push(FieldError::new("roots", "at least one root page is required"));
    }
    for r in &c.roots {
        check_slug(&mut e, "roots", r);
    }
    for (field, prefixes) in [("scope.include", &c.scope.include), ("scope.exclude", &c.scope.exclude)] {
        for p in prefixes {
            if !is_scope_prefix(p) {
                e.push(FieldError::new(field, format!("`{p}` is not a repository-relative path like `crates/core`")));
            }
        }
    }
    e
}

/// A repository-relative, `/`-separated path with nothing that could climb
/// out of the repository or name another drive.
pub fn is_scope_prefix(p: &str) -> bool {
    !p.is_empty() && !p.contains('\\') && !p.contains(':') && p.split('/').all(|s| !s.is_empty() && s != "." && s != "..")
}

impl Store {
    pub fn new(config: StoreConfig) -> Self {
        Store { config, ..Default::default() }
    }

    /// Whether a stored node exists. Derived kinds always return `false`.
    pub fn contains(&self, r: &Reference) -> bool {
        match r.kind {
            Kind::Page => self.pages.contains_key(&r.id),
            Kind::Req => self.requirements.contains_key(&r.id),
            Kind::Ticket => self.tickets.contains_key(&r.id),
            _ => false,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// # Test contract
    ///
    /// ```json
    /// {"test": "scope-prefixes-stay-inside-the-repository", "checks": ["crates/core/src/store#scope-rules"]}
    /// ```
    #[test]
    fn scope_prefixes_stay_inside_the_repository() {
        for ok in ["crates/core", "web", "a/b/c"] {
            assert!(is_scope_prefix(ok), "{ok}");
        }
        for bad in ["", "/abs", "../out", "a/../b", "./a", "a/", "C:/x", "a\\b"] {
            assert!(!is_scope_prefix(bad), "{bad}");
        }
    }

    /// # Test contract
    ///
    /// ```json
    /// {"test": "requirement-rules", "checks": ["crates/core/src/model#requirement-record"]}
    /// ```
    #[test]
    fn requirement_rules() {
        let req = |covered_by: Vec<Reference>, dates: &[&str]| Requirement {
            slug: "r".into(),
            title: "R".into(),
            statement: "It SHALL hold.".into(),
            scenarios: vec![],
            covered_by,
            decisions: dates.iter().map(|d| Decision { date: d.to_string(), text: "why".into() }).collect(),
        };
        assert!(validate_requirement(&req(vec![Reference::criterion("m", "c")], &["2026-01-01", "2026-02-01"])).is_empty());
        let e = validate_requirement(&req(vec![Reference::module("m")], &[]));
        assert_eq!(e[0].field, "covered_by", "covered_by names only criteria");
        let e = validate_requirement(&req(vec![], &["2026-02-01", "2026-01-01"]));
        assert_eq!(e[0].field, "decisions", "decisions are oldest first");
    }

    /// # Test contract
    ///
    /// ```json
    /// {"test": "config-rules", "checks": ["crates/core/src/model#config-record"]}
    /// ```
    #[test]
    fn config_rules() {
        let config = |version: u32, roots: Vec<String>| StoreConfig { version, repository: "C:/repo".into(), roots, languages: vec![], scope: Scope::default(), order: Default::default() };
        assert!(validate_config(&config(StoreConfig::CURRENT_VERSION, vec!["overview".into()])).is_empty());
        assert_eq!(validate_config(&config(99, vec!["overview".into()]))[0].field, "version", "an unknown version is refused");
        assert_eq!(validate_config(&config(StoreConfig::CURRENT_VERSION, vec![]))[0].field, "roots");
    }

    /// # Test contract
    ///
    /// ```json
    /// {"test": "field-rules", "checks": ["crates/core/src/store#field-rules"]}
    /// ```
    #[test]
    fn field_rules() {
        let page = |title: &str| Page { slug: "p".into(), title: title.into(), body: "b".into(), links: vec![] };
        assert!(validate_page(&page("One line")).iter().all(|e| e.field != "title"));
        assert!(validate_page(&page("Two\nlines")).iter().any(|e| e.field == "title"));
        assert!(validate_page(&page("   ")).iter().any(|e| e.field == "title"), "required strings are non-empty after trimming");
        let ticket = |body: String| Ticket { slug: "t".into(), title: "T".into(), kind: TicketKind::Change, priority: Priority::Next, status: Status::Open, touches: vec![], relies_on: vec![], body, decision: None };
        assert!(validate_ticket(&ticket("x".repeat(TICKET_BODY_MAX))).is_empty());
        assert_eq!(validate_ticket(&ticket("x".repeat(TICKET_BODY_MAX + 1)))[0].field, "body");
    }

    /// # Test contract
    ///
    /// ```json
    /// {"test": "slug-rules", "checks": ["crates/core/src/store#slug-rule"]}
    /// ```
    #[test]
    fn slug_rules() {
        assert!(is_slug("a"));
        assert!(is_slug("git-readiness-2"));
        assert!(!is_slug("-a"));
        assert!(!is_slug("a-"));
        assert!(!is_slug("a--b"));
        assert!(!is_slug("A"));
        assert!(!is_slug(""));
        assert!(!is_slug(&"a".repeat(65)));
    }

    /// # Test contract
    ///
    /// ```json
    /// {"test": "date-rules", "checks": ["crates/core/src/store#field-rules"]}
    /// ```
    #[test]
    fn date_rules() {
        assert!(is_date("2026-09-27"));
        assert!(!is_date("2026-13-01"));
        assert!(!is_date("26-09-27"));
        assert!(!is_date("2026/09/27"));
    }

    /// # Test contract
    ///
    /// ```json
    /// {"test": "ticket-decision-rules", "checks": ["crates/core/src/store#field-rules"]}
    /// ```
    #[test]
    fn ticket_decision_rules() {
        let mut t = Ticket {
            slug: "t".into(),
            title: "T".into(),
            kind: TicketKind::Change,
            priority: Priority::Next,
            status: Status::Open,
            touches: vec![],
            relies_on: vec![],
            body: "b".into(),
            decision: None,
        };
        assert!(validate_ticket(&t).is_empty());
        t.status = Status::Closed;
        assert_eq!(validate_ticket(&t).len(), 1);
        t.decision = Some(Decision { date: "2026-01-01".into(), text: "done".into() });
        t.body.clear();
        assert!(validate_ticket(&t).is_empty());
        t.status = Status::Open;
        assert_eq!(validate_ticket(&t)[0].field, "decision");
    }

    /// # Test contract
    ///
    /// ```json
    /// {"test": "page-link-kinds", "checks": ["crates/core/src/model#page-record", "crates/core/src/store#whole-record"]}
    /// ```
    #[test]
    fn page_link_kinds() {
        let p = Page {
            slug: "p".into(),
            title: "P".into(),
            body: "b".into(),
            links: vec![Reference::ticket("x"), Reference::page("q"), Reference::page("q")],
        };
        let e = validate_page(&p);
        assert_eq!(e.len(), 2);
    }
}
