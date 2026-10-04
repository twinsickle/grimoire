//! # Module contract
//!
//! ```json
//! {"purpose":"Gather the project's site from the graph: the pages the store's site setting names, every page beneath each, and their requirements' statements and scenarios, and nothing else.","criteria":[{"id":"site","statement":"site is named by the title of the config's first root page when it exists, and lists the pages the config's site setting names that exist, those the roots' links reach first in the order the roots link them and the rest after in the order named, each followed depth first by every page it links, each page once, with its slug, title and depth beneath the named page it came under, however deep the pages go; it gives in full only the page asked for, else the first, and none for a page not in the site: its slug, title and body and the requirements it links that exist and are not retired, in link order, each with its slug, title, statement and scenarios, and nothing else: no decision, module, criterion or ticket."}]}
//! ```

use std::collections::HashSet;

use serde::Serialize;

use crate::graph::{Graph, Node};
use crate::model::*;

/// The project's site: its name, every page in reading order, and the one page asked for in full.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
pub struct Site {
    /// The title of the store's first root page, when there is one.
    pub name: Option<String>,
    pub pages: Vec<SiteEntry>,
    pub shown: Option<SitePage>,
}

/// A page's place in the site: what it is called, and how deep it sits beneath the named page it came under.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct SiteEntry {
    pub slug: String,
    pub title: String,
    pub depth: usize,
}

/// One page of the site in full: what it says, and the requirements it links.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct SitePage {
    pub slug: String,
    pub title: String,
    pub body: String,
    pub requirements: Vec<SiteRequirement>,
}

/// A requirement as an outsider reads it: what it says, and the cases it holds for.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct SiteRequirement {
    pub slug: String,
    pub title: String,
    pub statement: String,
    pub scenarios: Vec<Scenario>,
}

fn page<'g>(g: &'g Graph, slug: &str) -> Option<&'g Page> {
    match g.get(&Reference::page(slug)) {
        Some(Node::Page(p)) => Some(p),
        _ => None,
    }
}

/// The site the config's `site` setting names, read from the graph, with the page `show` names in full,
/// or the first page when it names none; a page that is not in the site is shown as none.
pub fn site(g: &Graph, config: &StoreConfig, show: Option<&str>) -> Site {
    // Named pages in the order the roots link them, then the rest as named.
    let mut by_root: Vec<&String> = Vec::new();
    for root in &config.roots {
        if config.site.contains(root) {
            by_root.push(root);
        }
        if let Some(r) = page(g, root) {
            by_root.extend(r.links.iter().filter(|l| l.kind == Kind::Page && config.site.contains(&l.id)).map(|l| &l.id));
        }
    }
    let mut named: Vec<&String> = Vec::new();
    for s in by_root.into_iter().chain(config.site.iter()) {
        if !named.contains(&s) {
            named.push(s);
        }
    }
    // Depth first beneath each, each page once; a stack, so no chain of pages is too deep to walk.
    let mut seen: HashSet<&str> = HashSet::new();
    let mut pages = Vec::new();
    for slug in named {
        let mut stack: Vec<(&str, usize)> = vec![(slug.as_str(), 0)];
        while let Some((slug, depth)) = stack.pop() {
            let Some(p) = page(g, slug) else { continue };
            if !seen.insert(slug) {
                continue;
            }
            pages.push(SiteEntry { slug: p.slug.clone(), title: p.title.clone(), depth });
            stack.extend(p.links.iter().rev().filter(|l| l.kind == Kind::Page).map(|l| (l.id.as_str(), depth + 1)));
        }
    }
    let wanted = show.or(pages.first().map(|e| e.slug.as_str()));
    let shown = wanted.filter(|w| pages.iter().any(|e| e.slug == *w)).and_then(|w| page(g, w)).map(|p| SitePage {
        slug: p.slug.clone(),
        title: p.title.clone(),
        body: p.body.clone(),
        requirements: p
            .links
            .iter()
            .filter(|l| l.kind == Kind::Req)
            .filter_map(|l| match g.get(l) {
                Some(Node::Req(q)) if !q.retired => Some(SiteRequirement { slug: q.slug.clone(), title: q.title.clone(), statement: q.statement.clone(), scenarios: q.scenarios.clone() }),
                _ => None,
            })
            .collect(),
    });
    let name = config.roots.first().and_then(|r| page(g, r)).map(|p| p.title.clone());
    Site { name, pages, shown }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// # Test contract
    ///
    /// ```json
    /// {"test": "the-site-is-the-named-pages-and-what-is-beneath", "checks": ["crates/core/src/query/site#site"]}
    /// ```
    #[test]
    fn the_site_is_the_named_pages_and_what_is_beneath() {
        let mut s = crate::graph::fixtures::store();
        // The fixture's root `overview` links `git-readiness`, which links the requirement `ready-only-with-commit` and a module.
        let page = |slug: &str, links: &[&str]| Page { slug: slug.into(), title: slug.to_uppercase(), body: format!("{slug} body"), links: links.iter().map(|l| l.parse().unwrap()).collect() };
        s.pages.insert("extra".into(), page("extra", &["page:git-readiness"]));
        s.requirements.get_mut("ready-only-with-commit").unwrap().decisions.push(Decision::new("2026-10-04", "Why."));
        let g = Graph::build(&s, &crate::extraction::fixtures::extraction(), &[]);
        // Named out of order, with one missing: the roots' order first, then the rest as named; a missing page left out.
        let mut config = s.config.clone();
        config.site = vec!["extra".into(), "nope".into(), "git-readiness".into()];
        let site = site(&g, &config, None);
        // Named by the first root's title, and nothing else of the root.
        assert_eq!(site.name.as_deref(), Some(g.get(&Reference::page(&s.config.roots[0])).map(|n| match n { Node::Page(p) => p.title.clone(), _ => unreachable!() }).unwrap().as_str()));
        let shown: Vec<(&str, usize)> = site.pages.iter().map(|p| (p.slug.as_str(), p.depth)).collect();
        assert_eq!(shown, vec![("git-readiness", 0), ("extra", 0)], "git-readiness is linked from the root, so first; once, though extra links it too");
        // The first page in full when none is asked for.
        let ready = site.shown.as_ref().expect("the first page");
        assert_eq!(ready.slug, "git-readiness");
        let linked: Vec<&str> = ready.requirements.iter().map(|r| r.slug.as_str()).collect();
        assert_eq!(linked[0], "ready-only-with-commit", "in link order: {linked:?}");
        assert!(!ready.requirements[0].scenarios.is_empty() && !ready.requirements[0].statement.is_empty());
        // Nothing but what is said: no decision, module or ticket reaches the answer.
        let json = serde_json::to_string(&site).unwrap();
        assert!(!json.contains("Why.") && !json.contains("module:") && !json.contains("decisions"), "{json}");
        // One page in full, the one asked for; a page outside the site is none.
        assert_eq!(super::site(&g, &config, Some("extra")).shown.map(|p| p.body), Some("extra body".to_string()));
        assert_eq!(super::site(&g, &config, Some("overview")).shown, None, "overview exists, but is not in the site");
        // Beneath a named page, its pages, deeper, in link order.
        config.site = vec!["overview".into()];
        let site = super::site(&g, &config, None);
        assert_eq!(site.pages.iter().map(|p| (p.slug.as_str(), p.depth)).collect::<Vec<_>>()[..2], [("overview", 0), ("git-readiness", 1)]);
        // A chain of pages deeper than a call stack holds is walked.
        for i in 0..100_000 {
            s.pages.insert(format!("deep-{i}"), page(&format!("deep-{i}"), &[&format!("page:deep-{}", i + 1)]));
        }
        let deep = Graph::build(&s, &crate::extraction::fixtures::extraction(), &[]);
        assert_eq!(super::site(&deep, &StoreConfig { site: vec!["deep-0".into()], ..s.config.clone() }, Some("deep-0")).pages.len(), 100_000);
        // A retired requirement is not shown.
        s.requirements.get_mut("ready-only-with-commit").unwrap().retired = true;
        let g = Graph::build(&s, &crate::extraction::fixtures::extraction(), &[]);
        config.site = vec!["git-readiness".into()];
        assert!(!super::site(&g, &config, None).shown.unwrap().requirements.iter().any(|r| r.slug == "ready-only-with-commit"));
        // Nothing named, nothing shown, the name still given.
        let none = super::site(&g, &s.config, None);
        assert!(none.pages.is_empty() && none.shown.is_none() && none.name.is_some());
        assert_eq!(super::site(&g, &StoreConfig { roots: vec![], ..s.config.clone() }, None).name, None, "no root, no name");
        assert_eq!(super::site(&g, &StoreConfig { roots: vec!["missing".into(), s.config.roots[0].clone()], ..s.config.clone() }, None).name, None, "a first root that names no page gives no name, not the next root's");
    }
}
