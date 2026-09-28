//! # Module contract
//!
//! ```json
//! {"purpose":"Build the existing-state skeleton of a store from module contracts alone: the story pages, a page per module group, a requirement per criterion, all linked.","criteria":[{"id":"contracts-only","statement":"The skeleton is a function of the extraction and a fixed story outline; it carries no intent the contracts did not, only the places intent goes."},{"id":"story-outline","statement":"The root page links, in reading order, to goal, architecture, constraints, conventions and glossary pages seeded with prompting text, and the module-group pages hang under architecture."},{"id":"lint-clean","statement":"The skeleton passes the dangling lint and leaves no contracted module or criterion uncovered; its requirements carry no scenarios, so each is reported as unspecified."}]}
//! ```

use std::collections::{BTreeMap, BTreeSet};

use crate::extraction::Extraction;
use crate::model::*;
use crate::store::{is_slug, Store, SLUG_MAX};

pub const ROOT_SLUG: &str = "overview";
pub const ARCHITECTURE_SLUG: &str = "architecture";

/// The story pages every grimoire starts with, in reading order, with the
/// prompt each carries until someone writes it. The convention: what this
/// is, what it does, and how.
pub const STORY: [(&str, &str, &str); 5] = [
    ("goal", "Goal", "What this project is and what it is not, as short statements a reader can check against the code. No motivation, no history, no milestones. This is the first page a person or agent reads."),
    (ARCHITECTURE_SLUG, "Architecture", "A high-level inventory of what exists and why: one line saying what the system is; the pipeline, how data moves through the parts; the layers with their responsibilities and what each is built from; the design principles behind them; and the physical layout. Every line checkable against the repository. The pages below are its areas, one per module group, seeded from the module contracts."),
    ("constraints", "Constraints", "What must always or never hold, whichever part is being changed. Link the cross-cutting requirements here so a planner sees them together."),
    ("conventions", "Conventions", "How things are done here, as opposed to what must hold: naming, the shape of pages and decisions, how contracts are written, commits, code style. Breaking one is inconsistent, not wrong; a convention a lint could check belongs under constraints as a requirement instead."),
    ("glossary", "Glossary", "The words this project uses with a specific meaning, one line each."),
];

/// Turn any path-ish text into a slug, or `node` if nothing survives.
pub fn slugify(s: &str) -> String {
    let mut out = String::new();
    let mut dash = true;
    for c in s.chars().flat_map(|c| c.to_lowercase()) {
        if c.is_ascii_lowercase() || c.is_ascii_digit() {
            out.push(c);
            dash = false;
        } else if !dash {
            out.push('-');
            dash = true;
        }
    }
    let out = out.trim_end_matches('-').to_string();
    let out: String = out.chars().take(SLUG_MAX).collect();
    let out = out.trim_end_matches('-').to_string();
    if is_slug(&out) {
        out
    } else {
        "node".to_string()
    }
}

/// A group is a module id's parent, with a trailing `src` folded away so a
/// crate's direct modules sit on the crate's page.
fn group_of(module_id: &str) -> String {
    fold_src(parent_of(module_id))
}

/// The group a group sits under.
fn parent_group(group: &str) -> String {
    fold_src(parent_of(group))
}

fn parent_of(path: &str) -> &str {
    match path.rsplit_once('/') {
        Some((p, _)) => p,
        None => "",
    }
}

fn fold_src(path: &str) -> String {
    path.strip_suffix("/src").or_else(|| if path == "src" { Some("") } else { None }).unwrap_or(path).to_string()
}

fn title_of(path: &str) -> String {
    let last = path.rsplit('/').next().unwrap_or(path);
    let mut t: Vec<char> = last.replace(['_', '-'], " ").chars().collect();
    if let Some(c) = t.first_mut() {
        *c = c.to_ascii_uppercase();
    }
    t.into_iter().collect()
}

struct Slugs(BTreeSet<String>);

impl Slugs {
    fn unique(&mut self, base: &str) -> String {
        let base = slugify(base);
        if self.0.insert(base.clone()) {
            return base;
        }
        for n in 2.. {
            let candidate: String = format!("{}-{n}", base.chars().take(SLUG_MAX - 4).collect::<String>().trim_end_matches('-'));
            if self.0.insert(candidate.clone()) {
                return candidate;
            }
        }
        unreachable!()
    }
}

/// Build the skeleton. `repository` and `languages` are copied into the
/// config so the result is a complete store.
pub fn scaffold(x: &Extraction, repository: &str, languages: &[String]) -> Store {
    let mut store = Store::new(StoreConfig {
        version: StoreConfig::CURRENT_VERSION,
        repository: repository.to_string(),
        roots: vec![ROOT_SLUG.to_string()],
        languages: languages.to_vec(),
        scope: Scope::default(),
    });

    let mut page_slugs = Slugs(BTreeSet::from([ROOT_SLUG.to_string()]));
    for (slug, _, _) in STORY {
        page_slugs.0.insert(slug.to_string());
    }
    let mut req_slugs = Slugs(BTreeSet::new());

    // Every group path that must exist, including ancestors.
    let mut groups: BTreeSet<String> = BTreeSet::new();
    for m in x.modules.values().filter(|m| m.purpose.is_some()) {
        let mut g = group_of(&m.id);
        loop {
            groups.insert(g.clone());
            if g.is_empty() {
                break;
            }
            g = parent_group(&g);
        }
    }

    let mut group_slug: BTreeMap<String, String> = BTreeMap::new();
    for g in &groups {
        let slug = if g.is_empty() { ROOT_SLUG.to_string() } else { page_slugs.unique(g) };
        group_slug.insert(g.clone(), slug);
    }

    // Group pages are keyed by group path; the story pages by a key that no
    // group path can be, so they share one map.
    let mut pages: BTreeMap<String, Page> = BTreeMap::new();
    for g in &groups {
        let slug = group_slug[g].clone();
        let (title, body) = if g.is_empty() {
            ("Overview".to_string(), format!("Skeleton built from the module contracts of `{repository}`. Refine it: the story pages below say what belongs in each, and the area pages under architecture hold no intent yet."))
        } else {
            (title_of(g), format!("Modules under `{g}/`."))
        };
        pages.insert(g.clone(), Page { slug, title, body, links: Vec::new() });
    }
    let root_key = String::new();
    pages.entry(root_key.clone()).or_insert_with(|| Page {
        slug: ROOT_SLUG.to_string(),
        title: "Overview".to_string(),
        body: format!("Skeleton for `{repository}`. No module contracts were found; the story pages below say what belongs in each."),
        links: Vec::new(),
    });
    for (slug, title, prompt) in STORY {
        let key = format!("story:{slug}");
        pages.insert(key, Page { slug: slug.to_string(), title: title.to_string(), body: prompt.to_string(), links: Vec::new() });
        pages.get_mut(&root_key).expect("root exists").links.push(Reference::page(slug));
    }

    // Child groups link from their parent, in path order; top-level groups
    // hang under architecture rather than the root.
    let architecture_key = format!("story:{ARCHITECTURE_SLUG}");
    for g in &groups {
        if g.is_empty() {
            continue;
        }
        let parent = parent_group(g);
        let parent_key = if parent.is_empty() { architecture_key.clone() } else { parent };
        let slug = group_slug[g].clone();
        pages.get_mut(&parent_key).expect("parent group exists").links.push(Reference::page(&slug));
    }

    // Modules and their requirements.
    for m in x.modules.values().filter(|m| m.purpose.is_some()) {
        let tail = m.id.rsplit('/').next().unwrap_or(&m.id);
        let page = pages.get_mut(&group_of(&m.id)).expect("group exists");
        for c in &m.criteria {
            let slug = req_slugs.unique(&format!("{tail}-{}", c.id));
            let req = Requirement {
                slug: slug.clone(),
                title: title_of(&c.id),
                statement: c.statement.clone(),
                scenarios: Vec::new(),
                covered_by: vec![Reference::criterion(&c.module, &c.id)],
                decisions: Vec::new(),
            };
            page.links.push(Reference::req(&slug));
            store.requirements.insert(slug, req);
        }
        page.links.push(Reference::module(&m.id));
    }

    for p in pages.into_values() {
        store.pages.insert(p.slug.clone(), p);
    }
    store
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::extraction::fixtures::extraction;
    use crate::graph::Graph;
    use crate::lint::{lint, Rule};

    #[test]
    fn groups() {
        assert_eq!(group_of("crates/core/src/model"), "crates/core");
        assert_eq!(group_of("crates/core/src/git/ready"), "crates/core/src/git");
        assert_eq!(group_of("crates/core/src/git"), "crates/core");
        assert_eq!(group_of("crates/core/src"), "crates/core");
        assert_eq!(group_of("src/foo"), "");
        assert_eq!(group_of("src"), "");
        assert_eq!(parent_group("crates/core/src/git"), "crates/core");
        assert_eq!(parent_group("crates/core"), "crates");
        assert_eq!(parent_group("crates"), "");
    }

    #[test]
    fn skeleton_is_lint_clean_for_contracted_modules() {
        let x = extraction();
        let s = scaffold(&x, "C:/repo", &["rust".into()]);
        assert!(s.pages.contains_key("overview"));
        assert_eq!(s.requirements.len(), 3);
        // The story outline: root links the four story pages in order, and
        // the top-level group hangs under architecture.
        let root_links: Vec<String> = s.pages["overview"].links.iter().map(|r| r.to_string()).collect();
        assert_eq!(root_links, vec!["page:goal", "page:architecture", "page:constraints", "page:conventions", "page:glossary"]);
        assert!(s.pages["architecture"].links.iter().any(|r| r.kind == Kind::Page && r.id == "src-a"), "{:?}", s.pages["architecture"].links);
        assert!(s.pages["overview"].links.iter().all(|r| r.id != "src-a"));
        assert!(s.pages["goal"].body.contains("What this project is and what it is not"));
        let g = Graph::build(&s, &x, &[]);
        let f = lint(&g, &[Rule::Dangling]);
        assert!(f.is_empty(), "{f:?}");
        let f = lint(&g, &[Rule::Uncovered]);
        // Only the uncontracted module remains uncovered.
        assert!(f.iter().all(|f| f.r#ref.id == "src/b/unrelated"), "{f:?}");
        // Every scaffolded requirement is left for a person to give a case.
        let f = lint(&g, &[Rule::Unspecified]);
        assert_eq!(f.len(), s.requirements.len());
    }

    #[test]
    fn slugify_cases() {
        assert_eq!(slugify("crates/core/src/git"), "crates-core-src-git");
        assert_eq!(slugify("Ready_Only"), "ready-only");
        assert_eq!(slugify("///"), "node");
    }
}
