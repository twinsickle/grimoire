//! # Module contract
//!
//! ```json
//! {"purpose":"Gather the project's site from the graph: the pages the store's site setting names, every page beneath each, and their requirements' statements and scenarios, and nothing else.","criteria":[{"id":"site","statement":"site is named by the title of the config's grimoire root page when it exists, and lists the pages the config's site setting names that exist, those the roots' links reach first in the order the roots link them and the rest after in the order named, each followed depth first by every page it links, each page once, with its slug, title and depth beneath the named page it came under, however deep the pages go; it gives in full only the page asked for, else the first, and none for a page not in the site: its slug, title and body and the requirements it links that exist and are not retired, in link order, each with its slug, title, statement and scenarios, and nothing else: no decision, module, criterion or ticket."},{"id":"recipes","statement":"site and recipe read the recipes the graph holds, or, given the main line's recipes, those alone, each as a recipe read with no files; site lists every recipe as the catalog, ordered by kind, then title, then id, each with its id, title, kind, the first paragraph of its text that is not a heading, and whether it is due, and gives with the page shown the recipes it links that exist, an ingredient's link placing its recipe, each once, in link order; recipe gives one recipe in full by its id, none for an id no recipe has: its id, title, kind and text, its ingredients each by id and what it says, the recipes it uses that exist in the order it names them and those that use it ordered by title, its last judgement's date and outcome when it has one, and whether it is due, and nothing of its files, file or line."}]}
//! ```

use std::collections::HashSet;

use serde::Serialize;

use crate::graph::{Graph, Node};
use crate::model::*;

/// The project's site: its name, every page in reading order, the catalog of recipes, and the one page asked for in full.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
pub struct Site {
    /// The title of the store's grimoire root page, when there is one.
    pub name: Option<String>,
    pub pages: Vec<SiteEntry>,
    /// Every recipe the code reads, by kind, then title.
    pub recipes: Vec<SiteRecipeEntry>,
    pub shown: Option<SitePage>,
}

/// A recipe as the catalog lists it: what it is called, its kind, the first thing it says, and whether it is due.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct SiteRecipeEntry {
    pub id: String,
    pub title: String,
    pub kind: String,
    pub summary: String,
    pub due: bool,
}

/// One recipe in full, as someone who builds with the project reads it: never its files.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct SiteRecipe {
    pub id: String,
    pub title: String,
    pub kind: String,
    pub text: String,
    pub ingredients: Vec<SiteIngredient>,
    pub uses: Vec<SiteRecipeEntry>,
    pub used_by: Vec<SiteRecipeEntry>,
    pub judged: Option<SiteJudged>,
    pub due: bool,
}

/// An ingredient: its id and what it says.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct SiteIngredient {
    pub id: String,
    pub says: String,
}

/// When a recipe was last judged, and how.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct SiteJudged {
    pub date: String,
    pub outcome: JudgementOutcome,
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
    /// The recipes the page links, which it places.
    pub recipes: Vec<SiteRecipeEntry>,
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
/// or the first page when it names none; a page that is not in the site is shown as none. Its recipes are
/// the graph's, or `sent`'s alone when given: the main line's, for a daemon that holds no code.
pub fn site(g: &Graph, config: &StoreConfig, show: Option<&str>, sent: Option<&MainLineRecipes>) -> Site {
    let catalog = Catalog::of(g, sent);
    // Named pages in the order the roots link them, then the rest as named.
    let mut by_root: Vec<&String> = Vec::new();
    for root in config.roots.list() {
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
        recipes: {
            // An ingredient's link places its recipe, once.
            let mut ids: Vec<&str> = Vec::new();
            for l in p.links.iter().filter(|l| l.kind == Kind::Recipe) {
                let id = l.id.split_once('#').map_or(l.id.as_str(), |(r, _)| r);
                if !ids.contains(&id) {
                    ids.push(id);
                }
            }
            ids.into_iter().filter_map(|id| catalog.get(id)).map(|r| entry(g, r)).collect()
        },
    });
    let name = config.roots.grimoire.as_ref().and_then(|r| page(g, r)).map(|p| p.title.clone());
    let mut recipes: Vec<SiteRecipeEntry> = catalog.0.iter().map(|r| entry(g, r)).collect();
    recipes.sort_by(|a, b| (&a.kind, &a.title, &a.id).cmp(&(&b.kind, &b.title, &b.id)));
    Site { name, pages, recipes, shown }
}

/// The recipes a site shows: the graph's, read from code, or the main line's as it sent them, each by a distinct id.
struct Catalog(Vec<Recipe>);

impl Catalog {
    fn of(g: &Graph, sent: Option<&MainLineRecipes>) -> Catalog {
        Catalog(match sent {
            Some(m) => m.recipes.iter().map(SentRecipe::as_recipe).collect(),
            None => g
                .nodes
                .values()
                .filter_map(|n| match n {
                    Node::Recipe(r) => Some(r.clone()),
                    _ => None,
                })
                .collect(),
        })
    }

    fn get(&self, id: &str) -> Option<&Recipe> {
        self.0.iter().find(|r| r.id == id)
    }
}

/// The first paragraph of a recipe's prose, outside a heading or a fenced block: what the catalog says of it.
fn summary(text: &str) -> String {
    let mut paragraph = String::new();
    // The fence's character while inside one: a block opened with backticks is closed only by backticks.
    let mut fence: Option<char> = None;
    for line in text.lines().map(str::trim) {
        let heading = line.trim_start_matches('#').is_empty() && line.starts_with('#') || line.split_once(' ').is_some_and(|(h, _)| !h.is_empty() && h.len() <= 6 && h.chars().all(|c| c == '#'));
        if let Some(c) = ['`', '~'].into_iter().find(|c| line.starts_with(&c.to_string().repeat(3))) {
            match fence {
                None => fence = Some(c),
                Some(open) if open == c => fence = None,
                Some(_) => {}
            }
        } else if fence.is_none() && !line.is_empty() && !heading {
            if !paragraph.is_empty() {
                paragraph.push(' ');
            }
            paragraph.push_str(line);
            continue;
        }
        if !paragraph.is_empty() {
            break;
        }
    }
    paragraph
}

fn entry(g: &Graph, r: &Recipe) -> SiteRecipeEntry {
    SiteRecipeEntry { id: r.id.clone(), title: r.title.clone(), kind: r.kind.clone(), summary: summary(&r.text), due: g.is_due(r) }
}

/// One recipe in full, by its id: what it says, its ingredients, how it is connected to other recipes and when it was last judged; never its files.
/// Read from the graph, or from `sent` alone when given, as `site` reads them.
pub fn recipe(g: &Graph, id: &str, sent: Option<&MainLineRecipes>) -> Option<SiteRecipe> {
    let catalog = Catalog::of(g, sent);
    let r = catalog.get(id)?;
    let mut used_by: Vec<SiteRecipeEntry> = catalog.0.iter().filter(|o| o.uses.iter().any(|u| u == id)).map(|o| entry(g, o)).collect();
    used_by.sort_by(|a, b| (&a.title, &a.id).cmp(&(&b.title, &b.id)));
    Some(SiteRecipe {
        id: r.id.clone(),
        title: r.title.clone(),
        kind: r.kind.clone(),
        text: r.text.clone(),
        ingredients: r.ingredients.iter().map(|i| SiteIngredient { id: i.id.clone(), says: i.says.clone() }).collect(),
        uses: r.uses.iter().filter_map(|u| catalog.get(u)).map(|u| entry(g, u)).collect(),
        used_by,
        judged: g.judgements.get(id).and_then(|all| all.last()).map(|j| SiteJudged { date: j.date.clone(), outcome: j.outcome }),
        due: g.is_due(r),
    })
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
        let site = site(&g, &config, None, None);
        // Named by the grimoire root's title, and nothing else of it.
        assert_eq!(
            site.name.as_deref(),
            Some(
                g.get(&Reference::page(s.config.roots.grimoire.as_ref().unwrap()))
                    .map(|n| match n {
                        Node::Page(p) => p.title.clone(),
                        _ => unreachable!(),
                    })
                    .unwrap()
                    .as_str()
            )
        );
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
        assert_eq!(super::site(&g, &config, Some("extra"), None).shown.map(|p| p.body), Some("extra body".to_string()));
        assert_eq!(super::site(&g, &config, Some("overview"), None).shown, None, "overview exists, but is not in the site");
        // Beneath a named page, its pages, deeper, in link order.
        config.site = vec!["overview".into()];
        let site = super::site(&g, &config, None, None);
        assert_eq!(site.pages.iter().map(|p| (p.slug.as_str(), p.depth)).collect::<Vec<_>>()[..2], [("overview", 0), ("git-readiness", 1)]);
        // A chain of pages deeper than a call stack holds is walked.
        for i in 0..100_000 {
            s.pages.insert(format!("deep-{i}"), page(&format!("deep-{i}"), &[&format!("page:deep-{}", i + 1)]));
        }
        let deep = Graph::build(&s, &crate::extraction::fixtures::extraction(), &[]);
        assert_eq!(super::site(&deep, &StoreConfig { site: vec!["deep-0".into()], ..s.config.clone() }, Some("deep-0"), None).pages.len(), 100_000);
        // A retired requirement is not shown.
        s.requirements.get_mut("ready-only-with-commit").unwrap().retired = true;
        let g = Graph::build(&s, &crate::extraction::fixtures::extraction(), &[]);
        config.site = vec!["git-readiness".into()];
        assert!(!super::site(&g, &config, None, None).shown.unwrap().requirements.iter().any(|r| r.slug == "ready-only-with-commit"));
        // Nothing named, nothing shown, the name still given.
        let none = super::site(&g, &s.config, None, None);
        assert!(none.pages.is_empty() && none.shown.is_none() && none.name.is_some());
        assert_eq!(super::site(&g, &StoreConfig { roots: vec![].into(), ..s.config.clone() }, None, None).name, None, "no root, no name");
        assert_eq!(
            super::site(&g, &StoreConfig { roots: vec!["missing".into(), s.config.roots.grimoire.clone().unwrap()].into(), ..s.config.clone() }, None, None).name,
            None,
            "a grimoire root that names no page gives no name, not the rites' title"
        );
    }

    /// # Test contract
    ///
    /// ```json
    /// {"test": "the-catalog-is-every-recipe-and-a-recipe-is-what-it-says", "checks": ["crates/core/src/query/site#recipes"]}
    /// ```
    #[test]
    fn the_catalog_is_every_recipe_and_a_recipe_is_what_it_says() {
        use crate::extraction::fixtures::{extraction_files, recipe_file};
        use crate::extraction::Extraction;
        let mut files = extraction_files();
        files.push(recipe_file("ui/button.recipe.md", "button", "component", &["icon", "nowhere"], &["src/a"], &[]));
        files.push(recipe_file("ui/icon.recipe.md", "icon", "component", &[], &["src/b"], &[]));
        files.push(recipe_file("content/quest.recipe.md", "quest", "content", &["button"], &[], &[]));
        // Two more: one titled as button is, and one using it, so the orders are seen.
        files.push(recipe_file("ui/abutton.recipe.md", "abutton", "component", &["icon"], &[], &[]));
        files.push(recipe_file("ui/aaa.recipe.md", "aaa", "component", &["button"], &[], &[]));
        for f in files.iter_mut() {
            if let Some(Ok(r)) = f.recipe.as_mut() {
                match r.id.as_str() {
                    "abutton" => r.title = "BUTTON".into(),
                    "aaa" => r.title = "ZED".into(),
                    "button" => r.uses = vec!["nowhere".into(), "quest".into(), "icon".into()],
                    _ => {}
                }
            }
        }
        if let Some(r) = files.iter_mut().find_map(|f| f.recipe.as_mut().and_then(|r| r.as_mut().ok()).filter(|r| r.id == "icon")) {
            r.text = "# Usage\n\n```json\n~~~\n\nnot this\n```\n\n#1 icon,\non one line.\n\nMore.".into();
        }
        let mut s = crate::graph::fixtures::store();
        let judged = |fingerprint: &str, date: &str, outcome| Judgement { recipe: "button".into(), fingerprint: fingerprint.into(), date: date.into(), ticket: "later-1".into(), outcome };
        let digest = crate::fingerprint::of_text("ui/button.recipe.md:button");
        s.judgements.insert("button".into(), vec![judged("old", "2026-10-01", JudgementOutcome::Updated), judged(&digest, "2026-10-07", JudgementOutcome::Held)]);
        s.pages.get_mut("git-readiness").unwrap().links.extend(["recipe:icon#label".parse().unwrap(), "recipe:icon".parse().unwrap(), "recipe:gone".parse().unwrap()]);
        let g = Graph::build(&s, &Extraction::from_files(&files), &[]);
        let mut config = s.config.clone();
        config.site = vec!["git-readiness".into()];
        let site = site(&g, &config, None, None);
        // Every recipe, by kind, then title, whether a page places it or not.
        let listed: Vec<(&str, &str, bool)> = site.recipes.iter().map(|r| (r.kind.as_str(), r.id.as_str(), r.due)).collect();
        assert_eq!(
            listed,
            [("component", "abutton", true), ("component", "button", false), ("component", "icon", true), ("component", "aaa", true), ("content", "quest", true)],
            "by kind, title, then id"
        );
        assert_eq!(site.recipes[2].summary, "#1 icon, on one line.", "the first paragraph, past a heading and a block another fence does not close");
        assert_eq!(site.recipes[1].summary, "How to use button.");
        // The page shown lists the recipes it links that exist.
        assert_eq!(site.shown.unwrap().recipes.iter().map(|r| r.id.as_str()).collect::<Vec<_>>(), ["icon"], "an ingredient's link places its recipe, once");
        // One recipe in full: what it says, how it is connected, when last judged.
        let button = recipe(&g, "button", None).expect("button");
        assert_eq!((button.title.as_str(), button.kind.as_str(), button.text.as_str()), ("BUTTON", "component", "How to use button."));
        assert_eq!(button.ingredients, [SiteIngredient { id: "label".into(), says: "The text shown.".into() }]);
        assert_eq!(button.uses.iter().map(|r| r.id.as_str()).collect::<Vec<_>>(), ["quest", "icon"], "in the order named, a use leading nowhere left out");
        assert_eq!(button.used_by.iter().map(|r| r.id.as_str()).collect::<Vec<_>>(), ["quest", "aaa"], "by title: QUEST before ZED");
        assert_eq!(button.judged, Some(SiteJudged { date: "2026-10-07".into(), outcome: JudgementOutcome::Held }), "the last judgement");
        assert!(!button.due);
        let icon = recipe(&g, "icon", None).expect("icon");
        assert!(icon.judged.is_none() && icon.due && icon.used_by.iter().any(|r| r.id == "button"));
        assert_eq!(recipe(&g, "gone", None), None);
        // Nothing of its files reaches the answer.
        let json = serde_json::to_string(&button).unwrap();
        assert!(!json.contains("src/a") && !json.contains("ui/button.recipe.md") && !json.contains("\"line\""), "{json}");
        // Given the main line's recipes, those alone, over a graph that holds no code: due from the digest sent.
        let read = |id: &str| match g.get(&Reference::recipe(id)) {
            Some(Node::Recipe(r)) => SentRecipe::from(r),
            _ => panic!("{id} is read"),
        };
        let mut moved = read("icon");
        moved.uses = vec!["button".into()];
        let sent = MainLineRecipes { recipes: vec![read("button"), moved] };
        let plain = Graph::build(&s, &crate::extraction::fixtures::extraction(), &[]);
        assert!(super::site(&plain, &config, None, None).recipes.is_empty(), "no code, no recipe of its own");
        let listed: Vec<(String, bool)> = super::site(&plain, &config, None, Some(&sent)).recipes.into_iter().map(|r| (r.id, r.due)).collect();
        assert_eq!(listed, [("button".to_string(), false), ("icon".to_string(), true)], "judged at the digest sent, so not due");
        let button = recipe(&plain, "button", Some(&sent)).expect("button, as sent");
        assert_eq!(button.judged, Some(SiteJudged { date: "2026-10-07".into(), outcome: JudgementOutcome::Held }));
        assert_eq!(button.used_by.iter().map(|r| r.id.as_str()).collect::<Vec<_>>(), ["icon"], "connected as the main line's recipes are");
        assert_eq!(super::site(&plain, &config, None, Some(&sent)).shown.unwrap().recipes.iter().map(|r| r.id.as_str()).collect::<Vec<_>>(), ["icon"]);
        // Given them, the graph's own are not read.
        let only = MainLineRecipes { recipes: vec![read("button")] };
        assert_eq!(super::site(&g, &config, None, Some(&only)).recipes.iter().map(|r| r.id.as_str()).collect::<Vec<_>>(), ["button"]);
        assert_eq!(recipe(&g, "icon", Some(&only)), None);
    }
}
