//! # Module contract
//!
//! ```json
//! {"purpose":"Hold the store's pages, requirements, tickets and baseline as values and validate each record against the schema.","criteria":[{"id":"scope-rules","statement":"A scope prefix is a non-empty repository-relative path, /-separated, with no . or .. segment, no leading or trailing slash and no drive."},{"id":"whole-record","statement":"Validation reports every violated field rule of a record in one pass, not only the first."},{"id":"slug-rule","statement":"A slug matches ^[a-z0-9]+(-[a-z0-9]+)*$, is at most 64 characters, and is unique within its kind."},{"id":"field-rules","statement":"Titles are one non-empty line, required strings are non-empty and do not begin or end with whitespace, nor does a stored reference as written or a start's key, a date is ISO YYYY-MM-DD naming a day the calendar has, leap years included, a ticket body is at most 4000 characters and empty once closed, a decision supersedes each earlier one once, and a ticket's decision is present exactly when its status is closed."},{"id":"config-rules","statement":"The config's repository is a path written as absolute on some platform, a drive, a leading / or a UNC prefix, judged by its spelling the same way everywhere, and its roots and its ticket order are slugs and its languages non-empty trimmed names; a relative repository is refused."}]}
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
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
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

/// ISO `YYYY-MM-DD` naming a day the calendar has, leap years included.
pub fn is_date(s: &str) -> bool {
    let b = s.as_bytes();
    if b.len() != 10 || b[4] != b'-' || b[7] != b'-' {
        return false;
    }
    let digits = |r: std::ops::Range<usize>| s[r].chars().all(|c| c.is_ascii_digit());
    if !(digits(0..4) && digits(5..7) && digits(8..10)) {
        return false;
    }
    let y: u32 = s[0..4].parse().unwrap_or(0);
    let m: u32 = s[5..7].parse().unwrap_or(0);
    let d: u32 = s[8..10].parse().unwrap_or(0);
    let leap = (y.is_multiple_of(4) && !y.is_multiple_of(100)) || y.is_multiple_of(400);
    let days = match m {
        1 | 3 | 5 | 7 | 8 | 10 | 12 => 31,
        4 | 6 | 9 | 11 => 30,
        2 if leap => 29,
        2 => 28,
        _ => return false,
    };
    (1..=days).contains(&d)
}

/// A path written as absolute on some platform: a drive (`C:/` or `C:\`),
/// a network share (`\\server\share` or `//server/share`, naming both), or
/// a leading `/`. Read by its spelling, the same everywhere, since the core
/// does no I/O and a store must validate on every platform. The device
/// prefixes `\\?\` and `\\.\` are not a repository's name.
pub fn is_absolute_path(p: &str) -> bool {
    let b = p.as_bytes();
    let drive = b.len() >= 3 && b[0].is_ascii_alphabetic() && b[1] == b':' && (b[2] == b'/' || b[2] == b'\\');
    let share = p.strip_prefix(r"\\").or_else(|| p.strip_prefix("//")).map(|rest| {
        let mut parts = rest.split(['/', '\\']);
        let (server, share) = (parts.next().unwrap_or(""), parts.next().unwrap_or(""));
        !server.is_empty() && server != "?" && server != "." && !share.is_empty()
    });
    match share {
        Some(named) => named,
        None => drive || p.starts_with('/'),
    }
}

fn check_slug(errors: &mut Vec<FieldError>, field: &str, s: &str) {
    if !is_slug(s) {
        errors.push(FieldError::new(field, "must match ^[a-z0-9]+(-[a-z0-9]+)*$ and be at most 64 characters"));
    }
}

fn check_required(errors: &mut Vec<FieldError>, field: &str, s: &str) {
    if s.trim().is_empty() {
        errors.push(FieldError::new(field, "is required and may not be empty"));
    } else {
        check_trimmed(errors, field, s);
    }
}

/// The write tools trim every string, so an untrimmed one on disk is a hand edit.
fn check_trimmed(errors: &mut Vec<FieldError>, field: &str, s: &str) {
    if s != s.trim() {
        errors.push(FieldError::new(field, "must not begin or end with whitespace"));
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
        // A file's name may begin with a space, so the parser takes one; a stored reference written with whitespace at its ends is a hand edit.
        let written = r.to_string();
        if written != written.trim() {
            errors.push(FieldError::new(field, format!("`{written}` must not begin or end with whitespace")));
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
        // A decision supersedes only earlier ones, by position from 1.
        if d.supersedes.iter().any(|&p| p == 0 || p as usize > i) {
            e.push(FieldError::new(&format!("decisions[{i}].supersedes"), "may name only earlier decisions, by position from 1"));
        }
        if d.supersedes.iter().enumerate().any(|(j, p)| d.supersedes[..j].contains(p)) {
            e.push(FieldError::new(&format!("decisions[{i}].supersedes"), "names each earlier decision once"));
        }
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
        (Status::Closed, Some(d)) => {
            check_decision(&mut e, "decision", d);
            if !t.body.is_empty() {
                e.push(FieldError::new("body", "must be empty once the ticket is closed, since closing discards it"));
            }
            // A ticket has one decision, so there is nothing earlier for it to supersede.
            if !d.supersedes.is_empty() {
                e.push(FieldError::new("decision.supersedes", "a ticket's one decision supersedes nothing"));
            }
        }
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
    for (i, f) in b.findings.iter().enumerate() {
        check_required(&mut e, &format!("findings[{i}].check"), &f.check);
    }
    e
}

/// A started ticket's snapshot: its ticket's slug, and a fingerprint for
/// each node, keyed by its reference.
pub fn validate_start(s: &Start) -> Vec<FieldError> {
    let mut e = Vec::new();
    check_slug(&mut e, "ticket", &s.ticket);
    for (key, print) in &s.fingerprints {
        if key.parse::<Reference>().is_err() {
            e.push(FieldError::new("fingerprints", format!("`{key}` is not a reference")));
        }
        check_trimmed(&mut e, "fingerprints", key);
        check_required(&mut e, &format!("fingerprints[{key}]"), print);
    }
    e
}

/// Every string in `v`, however deep, with its leading and trailing
/// whitespace taken off: what the write tools do to their input, and what
/// reading a version 1 store does to its records.
pub fn trim_strings(v: &mut serde_json::Value) {
    use serde_json::Value;
    match v {
        Value::String(s) if s.trim().len() != s.len() => *s = s.trim().to_string(),
        Value::Array(items) => items.iter_mut().for_each(trim_strings),
        Value::Object(fields) => fields.values_mut().for_each(trim_strings),
        _ => {}
    }
}

pub fn validate_config(c: &StoreConfig) -> Vec<FieldError> {
    let mut e = Vec::new();
    if c.version != StoreConfig::CURRENT_VERSION && c.version != StoreConfig::TRIMMED_ON_READ {
        e.push(FieldError::new("version", format!("unknown store version {}; this build knows {} and reads {}", c.version, StoreConfig::CURRENT_VERSION, StoreConfig::TRIMMED_ON_READ)));
    }
    check_required(&mut e, "repository", &c.repository);
    if !c.repository.trim().is_empty() && !is_absolute_path(&c.repository) {
        e.push(FieldError::new("repository", format!("`{}` is not an absolute path; the repository is named by where it is, like `C:/work/app` or `/home/me/app`", c.repository)));
    }
    for l in &c.languages {
        check_required(&mut e, "languages", l);
    }
    if c.roots.is_empty() {
        e.push(FieldError::new("roots", "at least one root page is required"));
    }
    for r in &c.roots {
        check_slug(&mut e, "roots", r);
    }
    for (band, slugs) in [("order.next", &c.order.next), ("order.high", &c.order.high), ("order.later", &c.order.later)] {
        for s in slugs {
            check_slug(&mut e, band, s);
        }
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
    !p.is_empty() && !p.contains('\\') && !p.contains(':') && p.split('/').all(|s| !s.is_empty() && s == s.trim() && s != "." && s != "..")
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
        for bad in ["", "/abs", "../out", "a/../b", "./a", "a/", "C:/x", "a\\b", " src", "src ", "a/ b"] {
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
            decisions: dates.iter().map(|d| Decision::new(d.to_string(), "why")).collect(),
            retired: false,
        };
        assert!(validate_requirement(&req(vec![Reference::criterion("m", "c")], &["2026-01-01", "2026-02-01"])).is_empty());
        let e = validate_requirement(&req(vec![Reference::module("m")], &[]));
        assert_eq!(e[0].field, "covered_by", "covered_by names only criteria");
        let e = validate_requirement(&req(vec![], &["2026-02-01", "2026-01-01"]));
        assert_eq!(e[0].field, "decisions", "decisions are oldest first");
        // A decision supersedes only earlier ones, by position from 1.
        let mut r = req(vec![], &["2026-01-01", "2026-02-01"]);
        r.decisions[1].supersedes = vec![1];
        assert!(validate_requirement(&r).is_empty());
        for bad in [vec![2], vec![0], vec![3]] {
            r.decisions[1].supersedes = bad.clone();
            assert_eq!(validate_requirement(&r).first().map(|e| e.field.as_str()), Some("decisions[1].supersedes"), "{bad:?}");
        }
        // Empty and false are left out of the record.
        let json = serde_json::to_value(req(vec![], &["2026-01-01"])).expect("serialisable");
        assert!(json.get("retired").is_none() && json["decisions"][0].get("supersedes").is_none(), "{json}");
    }

    /// # Test contract
    ///
    /// ```json
    /// {"test": "config-rules", "checks": ["crates/core/src/model#config-record", "crates/core/src/store#config-rules"]}
    /// ```
    #[test]
    fn config_rules() {
        let config = |version: u32, roots: Vec<String>| StoreConfig { version, repository: "C:/repo".into(), roots, languages: vec![], scope: Scope::default(), order: Default::default() };
        assert!(validate_config(&config(StoreConfig::CURRENT_VERSION, vec!["overview".into()])).is_empty());
        assert_eq!(validate_config(&config(99, vec!["overview".into()]))[0].field, "version", "an unknown version is refused");
        assert!(validate_config(&config(StoreConfig::TRIMMED_ON_READ, vec!["overview".into()])).is_empty(), "version 1 is read, to be migrated");
        assert_eq!(validate_config(&config(StoreConfig::CURRENT_VERSION, vec![]))[0].field, "roots");
        // The repository is absolute by its spelling, the same on every platform.
        let at = |repository: &str| StoreConfig { repository: repository.into(), ..config(StoreConfig::CURRENT_VERSION, vec!["overview".into()]) };
        for absolute in ["C:/repo", r"c:\repo", "/home/me/repo", r"\\server\share\repo", r"\\server\share", "//server/share/repo"] {
            assert!(validate_config(&at(absolute)).is_empty(), "{absolute}");
        }
        for relative in ["repo", "./repo", "../repo", "C:repo", "~/repo", r"\\", r"\\server", "//server/", r"\\?\C:\repo", r"\\.\pipe\x", "//?/x/y"] {
            let errors = validate_config(&at(relative));
            assert!(errors.iter().any(|e| e.field == "repository" && e.reason.contains("not an absolute path")), "{relative}: {errors:?}");
        }
        let languages = StoreConfig { languages: vec![" rust".into()], ..config(StoreConfig::CURRENT_VERSION, vec!["overview".into()]) };
        assert_eq!(validate_config(&languages)[0].field, "languages");
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
        // Untrimmed is malformed, wherever the string sits: a hand edit, since the tools trim.
        let untrimmed = |errors: Vec<FieldError>, field: &str| errors.iter().any(|e| e.field == field && e.reason == "must not begin or end with whitespace");
        assert!(untrimmed(validate_page(&page(" Goal")), "title"));
        assert!(untrimmed(validate_page(&Page { body: "b\n".into(), ..page("Goal") }), "body"));
        let mut req = Requirement { slug: "r".into(), title: "R".into(), statement: "S ".into(), scenarios: vec![Scenario { title: "t".into(), given: "g".into(), when: " w".into(), then: "t".into() }], covered_by: vec![], decisions: vec![Decision::new("2026-09-30", "why\n")], retired: false };
        let errors = validate_requirement(&req);
        assert!(untrimmed(errors.clone(), "statement") && untrimmed(errors.clone(), "scenarios[0].when") && untrimmed(errors, "decisions[0].text"));
        req.statement = "S".into();
        req.scenarios[0].when = "w".into();
        req.decisions[0].text = "why".into();
        assert!(validate_requirement(&req).is_empty());
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
        // A day the calendar has, leap years included.
        assert!(!is_date("2026-02-31") && !is_date("2026-04-31") && !is_date("2026-00-10") && !is_date("2026-01-00"));
        assert!(is_date("2026-01-31") && is_date("2026-12-31"));
        assert!(is_date("2024-02-29") && is_date("2000-02-29"), "leap years");
        assert!(!is_date("2026-02-29") && !is_date("1900-02-29"), "not leap years");
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
        t.decision = Some(Decision::new("2026-01-01", "done"));
        t.body.clear();
        assert!(validate_ticket(&t).is_empty());
        // A ticket's one decision has nothing earlier to supersede.
        t.decision = Some(Decision { supersedes: vec![1], ..Decision::new("2026-01-01", "done") });
        assert_eq!(validate_ticket(&t)[0].field, "decision.supersedes");
        t.decision = Some(Decision::new("2026-01-01", "done"));
        // Closing discards the body, so a closed ticket holding one was not closed by the tools.
        t.body = " ".into();
        assert_eq!(validate_ticket(&t)[0].field, "body");
        t.body.clear();
        t.status = Status::Open;
        assert_eq!(validate_ticket(&t)[0].field, "decision");
    }

    /// # Test contract
    ///
    /// ```json
    /// {"test": "every-stored-string-is-trimmed", "checks": ["crates/core/src/store#field-rules"]}
    /// ```
    #[test]
    fn every_stored_string_is_trimmed() {
        // A reference written with whitespace at its ends is a hand edit; a file whose name begins with a space is not.
        let linking = |link: &str| Page { slug: "p".into(), title: "P".into(), body: "b".into(), links: vec![link.parse().unwrap()] };
        assert!(validate_page(&linking("page:goal ")).iter().any(|e| e.field == "links"));
        assert!(validate_page(&linking("module:src/a")).is_empty());
        assert!(" foo.rs".parse::<Reference>().is_err() && "file: foo.rs".parse::<Reference>().is_ok(), "a file named with a leading space still has a reference");
        let order = StoreConfig { version: StoreConfig::CURRENT_VERSION, repository: "C:/repo".into(), roots: vec!["overview".into()], languages: vec![], scope: Scope::default(), order: Order { next: vec!["0055 ".into()], ..Default::default() } };
        assert_eq!(validate_config(&order)[0].field, "order.next");
        // A baseline finding's check, and a start's ticket and fingerprints.
        let baseline: Baseline = serde_json::from_value(serde_json::json!({"date": "2026-09-30", "reason": "r", "findings": [{"rule": "uncovered", "check": "no-contract ", "ref": "module:src/a"}]})).unwrap();
        assert_eq!(validate_baseline(&baseline)[0].field, "findings[0].check");
        let start = |key: &str, print: &str| Start { ticket: "0001".into(), fingerprints: [(key.to_string(), print.to_string())].into() };
        assert!(validate_start(&start("page:goal", "f1")).is_empty());
        assert!(!validate_start(&start("page:goal ", "f1")).is_empty(), "a key that is not a reference");
        assert!(!validate_start(&start("page:goal", " f1")).is_empty(), "an untrimmed fingerprint");
        assert!(!validate_start(&Start { ticket: "Not A Slug".into(), fingerprints: Default::default() }).is_empty());
        // A decision supersedes each earlier one once.
        let mut req = Requirement { slug: "r".into(), title: "R".into(), statement: "S".into(), scenarios: vec![], covered_by: vec![], decisions: vec![Decision::new("2026-09-01", "a"), Decision::new("2026-09-02", "b"), Decision { supersedes: vec![1, 2], ..Decision::new("2026-09-03", "c") }], retired: false };
        assert!(validate_requirement(&req).is_empty());
        req.decisions[2].supersedes = vec![1, 1];
        assert!(validate_requirement(&req).iter().any(|e| e.reason == "names each earlier decision once"));
        // What the tools and a version 1 read do to a record: every string, at any depth.
        let mut v = serde_json::json!({"title": " T ", "list": ["a\n", 1, {"deep": "\tx"}]});
        trim_strings(&mut v);
        assert_eq!(v, serde_json::json!({"title": "T", "list": ["a", 1, {"deep": "x"}]}));
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
