//! # Module contract
//!
//! ```json
//! {"purpose":"Apply the write tools to a store value: put pages and requirements and patch pages, reorder the board, delete a node, adopt a baseline and configure the store; proposals, a ticket's life, rename and a requirement's decisions each have a module beneath.","criteria":[{"id":"reorder","statement":"Reordering replaces one band's order list with the given tickets, each a ticket of that priority that is not closed, named once, and refuses any other as invalid."},{"id":"configure","statement":"Configuring changes only the roots, languages, scope, site, line limits and recipe kinds it is given, a list of roots naming both and a pair only the roots it gives, the recipe kinds replaced whole, refuses a language the build cannot extract and a site naming a page that does not exist, validates the whole config, and never changes the repository."},{"id":"adopt","statement":"Adopting replaces the baseline with the given findings, dated, with a required reason, and reports how many the previous baseline held."},{"id":"judge","statement":"Judging appends a judgement to its recipe's judgements, after validating them whole, refusing a ticket the store does not hold as not_found and a closed one as a conflict, and changes nothing else."},{"id":"validate-first","statement":"A write validates the whole resulting record before changing the store, and an invalid record changes nothing."},{"id":"never-fails-on-lint","statement":"A write that creates a finding (a dangling edge, or a requirement with no scenario) succeeds; the lint result is the caller's to report."},{"id":"patch-is-partial","statement":"patch_page changes only the fields given: added links go at the end without duplicates, removing an absent link is not an error, every other field stays as it was, and adding a link to a retired requirement is refused as invalid; its body is edited in part as BodyEdit does: replace swaps the one passage old for new, refused as invalid when old is empty, absent or present more than once, saying which and how many times, append adds its text after the body's trailing whitespace is dropped and a blank line, or alone on an empty body, the replace applied first, and either given beside a whole body is invalid."},{"id":"put-page","statement":"put_page creates or replaces a page by slug; replacing is not a conflict; a link to a retired requirement is refused as invalid, since no page links one."},{"id":"put-requirement","statement":"put_requirement creates or replaces a requirement by slug, ignoring decisions and retired on input and preserving the existing ones; a retired requirement is refused as invalid."},{"id":"delete-node","statement":"delete_node removes a page, a requirement with no decision, or a ticket that is not closed, a ticket's start and proposal going with it and the current ticket cleared when it was that one; a requirement with decisions (retire it instead), a closed ticket and a gap or question waiting, landing, for the change that applies its answer are invalid, derived kinds are invalid and a missing node is not_found."}]}
//! ```

use serde::{Deserialize, Serialize};

mod decisions;
mod proposal;
mod rename;
mod tickets;

pub use decisions::{record_decision, retire_node, supersede};
pub use proposal::*;
pub use rename::*;
pub use tickets::*;

use crate::model::*;
use crate::store::{validate_baseline, validate_config, validate_page, validate_requirement, validate_ticket, FieldError, Store};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, thiserror::Error)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[serde(tag = "error", rename_all = "snake_case")]
pub enum ToolError {
    #[error("{message}")]
    NotFound { message: String },
    #[error("{message}")]
    Invalid { message: String, details: Vec<FieldError> },
    #[error("{message}")]
    Conflict {
        message: String,
        #[serde(default)]
        details: Vec<FieldError>,
    },
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
        ToolError::Conflict { message: message.into(), details: vec![] }
    }
    /// A conflict with what conflicts named in `details`, as a close whose
    /// truth moved names the moved records.
    pub fn conflict_with(message: impl Into<String>, details: Vec<FieldError>) -> Self {
        ToolError::Conflict { message: message.into(), details }
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
    /// A ticket's proposal changed: written as it now is, or removed when the
    /// store no longer holds one for the ticket.
    Proposed(String),
    /// The current ticket changed: written as it now is, or removed when none.
    Current,
    /// A recipe's judgements changed: written as they now are, by recipe id.
    Judged(String),
    /// The recipes the main line last sent were replaced: written whole.
    MainLineRecipes,
    /// Several changes from one write, persisted in this order.
    Many(Vec<Changed>),
}

/// The config fields `configure` may change; the repository is not one.
#[derive(Debug, Clone, Default, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ConfigChange {
    pub roots: Option<RootsChange>,
    pub languages: Option<Vec<String>>,
    pub scope: Option<Scope>,
    pub site: Option<Vec<String>>,
    pub large_file: Option<u32>,
    pub plain_file: Option<u32>,
    pub recipe_kinds: Option<std::collections::BTreeMap<String, crate::model::RecipeKind>>,
}

/// The roots `configure` is given: a list of slugs, as older stores held, which names both, or the
/// pair, of which only the roots given change.
#[derive(Debug, Clone, serde::Deserialize)]
#[serde(untagged)]
pub enum RootsChange {
    List(Vec<String>),
    Pair {
        #[serde(default)]
        grimoire: Option<String>,
        #[serde(default)]
        rites: Option<String>,
    },
}

/// Change the fields given. `supported` is the languages the running build
/// can extract; empty means unknown, and then any language is accepted.
pub fn configure(store: &mut Store, change: ConfigChange, supported: &[String]) -> Result<(StoreConfig, Changed), ToolError> {
    let mut config = store.config.clone();
    match change.roots {
        // A list, as older stores held, names both: a second missing is a rites missing.
        Some(RootsChange::List(list)) => config.roots = list.into(),
        // The pair changes only the roots it gives.
        Some(RootsChange::Pair { grimoire, rites }) => {
            if grimoire.is_some() {
                config.roots.grimoire = grimoire;
            }
            if rites.is_some() {
                config.roots.rites = rites;
            }
        }
        None => {}
    }
    if let Some(languages) = change.languages {
        config.languages = languages;
    }
    if let Some(scope) = change.scope {
        config.scope = scope;
    }
    if change.large_file.is_some() {
        config.large_file = change.large_file;
    }
    if change.plain_file.is_some() {
        config.plain_file = change.plain_file;
    }
    // Replaced whole: the kinds and what each may use are one rule (req:recipe-kinds).
    if let Some(kinds) = change.recipe_kinds {
        config.recipe_kinds = kinds;
    }
    let mut errors = Vec::new();
    if let Some(site) = change.site {
        // The site shows pages that exist: one that names none would show nothing in its place, unseen.
        for p in site.iter().filter(|p| !store.pages.contains_key(*p)) {
            errors.push(FieldError::new("site", format!("`{p}` names no page")));
        }
        config.site = site;
    }
    errors.extend(validate_config(&config));
    if !supported.is_empty() {
        for l in config.languages.iter().filter(|l| !supported.contains(l)) {
            errors.push(FieldError::new("languages", format!("`{l}` is not supported by this build, which supports {}", supported.join(", "))));
        }
    }
    invalid_if(errors, "config")?;
    store.config = config.clone();
    Ok((config, Changed::Config))
}

/// Replace one band's order with `order`, every entry a ticket of that
/// priority that is not closed, named once.
pub fn reorder(store: &mut Store, priority: Priority, order: &[Reference]) -> Result<(Vec<String>, Changed), ToolError> {
    let mut errors = Vec::new();
    let mut seen = std::collections::HashSet::new();
    for r in order {
        let ok = r.kind == Kind::Ticket && store.tickets.get(&r.id).is_some_and(|t| t.status != Status::Closed && t.priority == priority);
        if !ok {
            errors.push(FieldError::new("order", format!("{r} is not a {} ticket that is still open, active or landing", format!("{priority:?}").to_lowercase())));
        } else if !seen.insert(&r.id) {
            errors.push(FieldError::new("order", format!("{r} is named twice")));
        }
    }
    invalid_if(errors, "order")?;
    let slugs: Vec<String> = order.iter().map(|r| r.id.clone()).collect();
    *store.config.order.band_mut(priority) = slugs.clone();
    Ok((slugs, Changed::Config))
}

/// Replace the baseline with `findings`. Returns the new baseline and how
/// many findings the previous one held, if there was one.
pub fn adopt(store: &mut Store, findings: Vec<FindingKey>, reason: String, date: String) -> Result<(Baseline, Option<usize>, Changed), ToolError> {
    let baseline = Baseline { date, reason, findings };
    invalid_if(validate_baseline(&baseline), "baseline")?;
    let replaced = store.baseline.replace(baseline.clone()).map(|b| b.findings.len());
    Ok((baseline, replaced, Changed::Baseline))
}

/// Record a judgement of a recipe: appended to its judgements, made in a ticket the store holds and
/// that is not closed (req:tool-judge-recipe).
pub fn judge(store: &mut Store, judgement: Judgement) -> Result<(Judgement, Changed), ToolError> {
    match store.tickets.get(&judgement.ticket) {
        None => return Err(ToolError::not_found(&Reference::ticket(&judgement.ticket))),
        Some(t) if t.status == Status::Closed => return Err(ToolError::conflict(format!("ticket:{} is closed: a recipe is judged in a ticket that is not closed", judgement.ticket))),
        Some(_) => {}
    }
    let recipe = judgement.recipe.clone();
    let mut all = store.judgements.get(&recipe).cloned().unwrap_or_default();
    all.push(judgement.clone());
    invalid_if(crate::store::validate_judgements(&Judgements { recipe: recipe.clone(), judgements: all.clone() }), "judgement")?;
    store.judgements.insert(recipe.clone(), all);
    Ok((judgement, Changed::Judged(recipe)))
}

/// No page links a retired requirement: it no longer holds.
fn refuse_retired_links(store: &Store, links: &[Reference]) -> Result<(), ToolError> {
    match links.iter().find(|l| l.kind == Kind::Req && store.requirements.get(&l.id).is_some_and(|q| q.retired)) {
        Some(l) => Err(ToolError::invalid(format!("{l} is retired: it no longer holds, and no page links it"), vec![])),
        None => Ok(()),
    }
}

pub fn put_page(store: &mut Store, page: Page) -> Result<(Page, Changed), ToolError> {
    invalid_if(validate_page(&page), "page")?;
    refuse_retired_links(store, &page.links)?;
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
    pub edit: BodyEdit,
    pub add_links: Vec<Reference>,
    pub remove_links: Vec<Reference>,
}

/// One passage of a body swapped for another: `old` must be in the body exactly once.
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Replace {
    pub old: String,
    pub new: String,
}

/// A body edited in part, as patch_page and update_ticket take it (req:ops-patch-is-partial):
/// one passage replaced, then a paragraph appended.
#[derive(Debug, Clone, Default)]
pub struct BodyEdit {
    pub replace: Option<Replace>,
    pub append: Option<String>,
}

impl BodyEdit {
    pub fn is_empty(&self) -> bool {
        self.replace.is_none() && self.append.is_none()
    }

    /// The body after this edit, or after the whole `body` given instead; both at once is invalid,
    /// since the edit would have no settled body to apply to.
    pub fn apply_to(self, current: &str, body: Option<String>) -> Result<Option<String>, ToolError> {
        if self.is_empty() {
            return Ok(body);
        }
        if body.is_some() {
            let field = if self.replace.is_some() { "replace" } else { "append" };
            return Err(ToolError::invalid(format!("give a whole body, or edit it in part with {field}, not both"), vec![FieldError::new(field, "not taken beside body")]));
        }
        let mut out = current.to_string();
        if let Some(Replace { old, new }) = self.replace {
            if old.is_empty() {
                return Err(ToolError::invalid("replace.old is empty: give the passage to replace", vec![FieldError::new("replace.old", "empty")]));
            }
            match out.matches(old.as_str()).count() {
                1 => out = out.replacen(old.as_str(), &new, 1),
                0 => return Err(ToolError::invalid("replace.old is not in the body: give the passage exactly as the body has it", vec![FieldError::new("replace.old", "not in the body")])),
                n => {
                    return Err(ToolError::invalid(
                        format!("replace.old is in the body {n} times: give more of the passage, so it matches once"),
                        vec![FieldError::new("replace.old", format!("in the body {n} times"))],
                    ))
                }
            }
        }
        if let Some(text) = self.append {
            let kept = out.trim_end();
            out = if kept.is_empty() { text } else { format!("{kept}\n\n{text}") };
        }
        Ok(Some(out))
    }
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
    if let Some(b) = patch.edit.apply_to(&page.body, patch.body)? {
        page.body = b;
    }
    page.links.retain(|l| !patch.remove_links.contains(l));
    refuse_retired_links(store, &patch.add_links)?;
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
    if store.requirements.get(&req.slug).is_some_and(|r| r.retired) {
        return Err(ToolError::invalid(format!("req:{} is retired: it no longer holds and is not rewritten", req.slug), vec![]));
    }
    req.decisions = store.requirements.get(&req.slug).map(|r| r.decisions.clone()).unwrap_or_default();
    req.retired = false;
    invalid_if(validate_requirement(&req), "requirement")?;
    store.requirements.insert(req.slug.clone(), req.clone());
    Ok((req.clone(), Changed::Requirement(req.slug)))
}

pub fn delete_node(store: &mut Store, r: &Reference) -> Result<Changed, ToolError> {
    // Decisions are history: a decided requirement is retired, and a closed ticket stays.
    match r.kind {
        Kind::Req if store.requirements.get(&r.id).is_some_and(|q| !q.decisions.is_empty()) => {
            return Err(ToolError::invalid(format!("{r} has decisions, which are never removed; retire it with retire_node instead"), vec![]));
        }
        Kind::Ticket if store.tickets.get(&r.id).is_some_and(|t| t.status == Status::Closed) => {
            return Err(ToolError::invalid(format!("{r} is closed: a closed ticket is history, already off the board and the lints, and is not deleted"), vec![]));
        }
        // An answer waiting for its change carries the person's decision: it closes or reopens with that change.
        Kind::Ticket if store.tickets.get(&r.id).is_some_and(|t| t.kind != TicketKind::Change && t.status == Status::Landing) => {
            return Err(ToolError::invalid(format!("{r} is an answer waiting for the change that applies it, and carries the person's decision: it closes when that change lands, or reopens if the change goes away, and is not deleted"), vec![]));
        }
        _ => {}
    }
    let removed = match r.kind {
        Kind::Page => store.pages.remove(&r.id).is_some(),
        Kind::Req => store.requirements.remove(&r.id).is_some(),
        Kind::Ticket => store.tickets.remove(&r.id).is_some(),
        _ => return Err(ToolError::invalid(format!("{r} is derived from the repository and cannot be deleted"), vec![])),
    };
    if !removed {
        return Err(ToolError::not_found(r));
    }
    if r.kind == Kind::Ticket {
        // Its start and proposal go with it, and it stops being current.
        store.starts.remove(&r.id);
        store.proposals.remove(&r.id);
        if store.current.as_deref() == Some(r.id.as_str()) {
            store.current = None;
            return Ok(Changed::Many(vec![Changed::Deleted(r.clone()), Changed::Current]));
        }
    }
    Ok(Changed::Deleted(r.clone()))
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
        let (p, _) = patch_page(
            &mut s,
            &r,
            PagePatch { add_links: vec![Reference::page("orphan"), Reference::req("stale")], remove_links: vec![Reference::req("nope"), Reference::module("src/a/ready")], ..Default::default() },
        )
        .unwrap();
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
    /// {"test": "patch-page-edits-the-body-in-part", "checks": ["crates/core/src/ops#patch-is-partial"]}
    /// ```
    #[test]
    fn patch_page_edits_the_body_in_part() {
        let mut s = store();
        let r = Reference::page("git-readiness");
        s.pages.get_mut("git-readiness").unwrap().body = "The old rule holds.\n\nIt is kept, and it is read.".into();
        let before = s.pages["git-readiness"].clone();
        let edit = |replace: Option<(&str, &str)>, append: Option<&str>| PagePatch {
            edit: BodyEdit { replace: replace.map(|(o, n)| Replace { old: o.into(), new: n.into() }), append: append.map(Into::into) },
            ..Default::default()
        };
        // One passage swapped; the rest, the title and the links as they were.
        let (p, _) = patch_page(&mut s, &r, edit(Some(("old rule", "new rule")), None)).unwrap();
        assert_eq!(p.body, "The new rule holds.\n\nIt is kept, and it is read.");
        assert_eq!((p.title, p.links), (before.title.clone(), before.links.clone()));
        // Absent, matching twice, or empty: refused, saying which, and nothing changes.
        let refused = |s: &mut Store, old: &str| match patch_page(s, &r, edit(Some((old, "x")), Some("Never."))) {
            Err(ToolError::Invalid { message, .. }) => message,
            other => panic!("expected invalid, got {other:?}"),
        };
        assert!(refused(&mut s, "old rule").contains("not in the body"));
        assert!(refused(&mut s, "is ").contains("2 times"));
        assert!(refused(&mut s, "").contains("empty"));
        assert_eq!(s.pages["git-readiness"].body, "The new rule holds.\n\nIt is kept, and it is read.");
        // Appended as its own paragraph, after a replace in the same call.
        let (p, _) = patch_page(&mut s, &r, edit(Some(("new rule", "rule")), Some("Second."))).unwrap();
        assert_eq!(p.body, "The rule holds.\n\nIt is kept, and it is read.\n\nSecond.");
        // On an empty body, the text alone.
        s.pages.get_mut("git-readiness").unwrap().body = String::new();
        assert_eq!(patch_page(&mut s, &r, edit(None, Some("First."))).unwrap().0.body, "First.");
        // Beside a whole body, either is refused.
        let both = PagePatch { body: Some("Whole.".into()), ..edit(None, Some("More.")) };
        assert_eq!(patch_page(&mut s, &r, both).unwrap_err().code(), "invalid");
        assert_eq!(s.pages["git-readiness"].body, "First.");
    }

    /// # Test contract
    ///
    /// ```json
    /// {"test": "put-requirement-keeps-decisions", "checks": ["crates/core/src/ops/decisions#decisions-append-only", "crates/core/src/ops#put-requirement"]}
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
    /// {"test": "delete-only-store-kinds", "checks": ["crates/core/src/ops#delete-node"]}
    /// ```
    #[test]
    fn delete_only_store_kinds() {
        let mut s = store();
        assert_eq!(delete_node(&mut s, &Reference::module("src/a/ready")).unwrap_err().code(), "invalid");
        assert!(delete_node(&mut s, &Reference::page("orphan")).is_ok());
        assert_eq!(delete_node(&mut s, &Reference::page("orphan")).unwrap_err().code(), "not_found");
        // A requirement with decisions is retired instead, and a closed ticket stays.
        let e = delete_node(&mut s, &Reference::req("ready-only-with-commit")).unwrap_err();
        assert!(e.code() == "invalid" && e.to_string().contains("retire_node"), "{e}");
        assert!(s.requirements.contains_key("ready-only-with-commit"));
        assert_eq!(delete_node(&mut s, &Reference::ticket("old")).unwrap_err().code(), "invalid");
        assert!(s.tickets.contains_key("old"));
        assert!(delete_node(&mut s, &Reference::req("stale")).is_ok(), "a requirement with no decision can go");
        assert!(delete_node(&mut s, &Reference::ticket("later-1")).is_ok(), "an active ticket can go");
    }
}
