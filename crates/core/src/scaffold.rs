//! # Module contract
//!
//! ```json
//! {"purpose":"Build the existing-state skeleton of a store from module contracts alone: a page per module group, a requirement per criterion, all linked.","criteria":[{"id":"contracts-only","statement":"The skeleton is a function of the extraction and nothing else; it carries no intent the contracts did not."},{"id":"lint-clean","statement":"The skeleton passes the dangling lint and leaves no contracted module or criterion uncovered."}]}
//! ```

use std::collections::{BTreeMap, BTreeSet};

use crate::extraction::Extraction;
use crate::model::*;
use crate::store::{is_slug, Store, SLUG_MAX};

pub const ROOT_SLUG: &str = "overview";

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
    });

    let mut page_slugs = Slugs(BTreeSet::from([ROOT_SLUG.to_string()]));
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

    let mut pages: BTreeMap<String, Page> = BTreeMap::new();
    for g in &groups {
        let slug = group_slug[g].clone();
        let (title, body) = if g.is_empty() {
            ("Overview".to_string(), format!("Skeleton built from the module contracts of `{repository}`. Refine it: this page and the ones below hold no intent yet."))
        } else {
            (title_of(g), format!("Modules under `{g}/`."))
        };
        pages.insert(g.clone(), Page { slug, title, body, links: Vec::new() });
    }

    // Child groups link from their parent, in path order.
    for g in &groups {
        if g.is_empty() {
            continue;
        }
        let parent = parent_group(g);
        let slug = group_slug[g].clone();
        pages.get_mut(&parent).expect("parent group exists").links.push(Reference::page(&slug));
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
        let g = Graph::build(&s, &x, &[]);
        let f = lint(&g, &[Rule::Dangling]);
        assert!(f.is_empty(), "{f:?}");
        let f = lint(&g, &[Rule::Uncovered]);
        // Only the uncontracted module remains uncovered.
        assert!(f.iter().all(|f| f.r#ref.id == "src/b/unrelated"), "{f:?}");
    }

    #[test]
    fn slugify_cases() {
        assert_eq!(slugify("crates/core/src/git"), "crates-core-src-git");
        assert_eq!(slugify("Ready_Only"), "ready-only");
        assert_eq!(slugify("///"), "node");
    }
}
