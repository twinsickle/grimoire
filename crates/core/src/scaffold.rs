//! # Module contract
//!
//! ```json
//! {"purpose":"Build the existing-state skeleton of a store from module contracts alone: the story pages, a page per module group, a requirement per criterion, all linked.","criteria":[{"id":"contracts-only","statement":"The skeleton is a function of the extraction and a fixed story outline; it carries no intent the contracts did not, only the places intent goes."},{"id":"story-outline","statement":"The root page links, in reading order, to goal, guarantees, glossary, architecture and constraints pages seeded with prompting text, the goal's prompt naming it the shallowest depth and the guarantees' saying a guarantee may be unheld, and the module-group pages hang under architecture; and the rites root, a page rites linked from nothing, says what the rites are for, that the grimoire is what the project is and must do and the rites how its people work on it, and that a rule of work is put there, and links a conventions page whose prompt says it holds how things are written, naming, pages and decisions, contracts, commits and code style, that a convention a tool checks stays there, and that a rule about what the code must do or how it is built belongs in the grimoire, under constraints when it holds whichever part is changed, with both roots named in the config."},{"id":"lint-clean","statement":"The skeleton passes the dangling lint and leaves no contracted module or criterion without a page or requirement; its requirements carry no scenarios and its criteria no tests, so each is reported as unspecified or untested."},{"id":"requirement-slugs","statement":"A requirement's slug is the last segment of its module's id that is not src, then its criterion's id, so a crate root crates/cli/src names its requirements cli-…; a slug already taken gets -2, -3 and on."}]}
//! ```

use std::collections::{BTreeMap, BTreeSet};

use crate::extraction::Extraction;
use crate::model::*;
use crate::store::{is_slug, Store, SLUG_MAX};

pub const ROOT_SLUG: &str = "overview";
pub const ARCHITECTURE_SLUG: &str = "architecture";
/// The rites root, the second of the two roots every grimoire has, and what it says until someone writes it.
pub const RITES_SLUG: &str = "rites";
pub const RITES_PROMPT: &str = "How this project's code is developed: the practices its work follows the same way every time, so that each piece of it can be judged before it lands. The grimoire beside it says what the project is and must do; this says how its people work on it. A requirement here is held by the work, which each ticket's closing decision shows, not by the code; put a rule of work here, through a ticket, rather than in an agent's own guidance file.";
/// The first page under the rites, and what it says until someone writes it.
pub const CONVENTIONS: (&str, &str, &str) = ("conventions", "Conventions", "How things are written here: naming, the shape of pages and decisions, how contracts are written, commits, code style. Breaking one is inconsistent, not wrong. A convention a tool checks stays here, held by its gate; a rule about what the code must do or how it is built belongs in the grimoire, under constraints when it holds whichever part is changed.");

/// The story pages every grimoire starts with, in reading order, with the
/// prompt each carries until someone writes it. The convention: what this
/// is, what it promises, the words it uses, how it is built and what must
/// hold. The first three read without the last two. How things are done is
/// the rites', not the story's.
pub const STORY: [(&str, &str, &str); 5] = [
    ("goal", "Goal", "What this project is and what it is not, for the people who use it, as short statements a reader can check against the code. This is the shallowest depth: how it is built belongs to the architecture, what it promises to the guarantees. No motivation, no history, no milestones. This is the first page a person or agent reads."),
    ("guarantees", "Guarantees", "What this project promises the people who use it: one page per guarantee beneath this one, each linking the requirements that make the promise. A guarantee is intent the code is held to; it may be unheld, and then it says so."),
    ("glossary", "Glossary", "The words this project uses with a specific meaning, one line each. It defines the words the pages after it use; the goal and the guarantees read without it."),
    (ARCHITECTURE_SLUG, "Architecture", "A high-level inventory of what exists and why: one line saying what the system is; the pipeline, how data moves through the parts; the layers with their responsibilities and what each is built from; the design principles behind them; and the physical layout. Every line checkable against the repository. The pages below are its areas, one per module group, seeded from the module contracts."),
    (crate::lint::CONSTRAINTS_PAGE, "Constraints", "What must always or never hold, whichever part is being changed. Link the cross-cutting requirements here so a planner sees them together."),
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
            let suffix = format!("-{n}");
            let candidate: String = format!("{}{suffix}", base.chars().take(SLUG_MAX - suffix.len()).collect::<String>().trim_end_matches('-'));
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
        large_file: None,
        plain_file: None,
        version: StoreConfig::CURRENT_VERSION,
        repository: repository.to_string(),
        roots: Roots::new(ROOT_SLUG, RITES_SLUG),
        languages: languages.to_vec(),
        scope: Scope::default(),
        order: Default::default(),
        site: Vec::new(),
    });

    let mut page_slugs = Slugs(BTreeSet::from([ROOT_SLUG.to_string(), RITES_SLUG.to_string(), CONVENTIONS.0.to_string()]));
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
            (
                "Overview".to_string(),
                format!(
                    "Skeleton built from the module contracts of `{repository}`. Refine it: the story pages below say what belongs in each, and the area pages under architecture hold no intent yet."
                ),
            )
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
    // The rites, a root of its own beside the story: nothing links it, and it links the conventions.
    let (conventions, title, prompt) = CONVENTIONS;
    pages.insert("rites:".to_string(), Page { slug: RITES_SLUG.to_string(), title: "Rites".to_string(), body: RITES_PROMPT.to_string(), links: vec![Reference::page(conventions)] });
    pages.insert(format!("rites:{conventions}"), Page { slug: conventions.to_string(), title: title.to_string(), body: prompt.to_string(), links: Vec::new() });
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
        // A crate root's id ends in src, which says nothing: name it by its crate.
        let tail = m.id.rsplit('/').find(|s| *s != "src").unwrap_or(&m.id);
        let page = pages.get_mut(&group_of(&m.id)).expect("group exists");
        for c in &m.criteria {
            let slug = req_slugs.unique(&format!("{tail}-{}", c.id));
            let req = Requirement {
                slug: slug.clone(),
                title: title_of(&c.id),
                // Trimmed as every stored string is: a contract may hold a stray space.
                statement: c.statement.trim().to_string(),
                scenarios: Vec::new(),
                covered_by: vec![Reference::criterion(&c.module, &c.id)],
                decisions: Vec::new(),
                retired: false,
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

    /// # Test contract
    ///
    /// ```json
    /// {"test": "the-skeleton-carries-only-what-the-contracts-say", "checks": ["crates/core/src/scaffold#contracts-only"]}
    /// ```
    #[test]
    fn the_skeleton_carries_only_what_the_contracts_say() {
        let x = extraction();
        let s = scaffold(&x, "C:/repo", &["rust".into()]);
        // Each requirement states its criterion verbatim, with no case and no decision.
        for r in s.requirements.values() {
            let c = &r.covered_by[0];
            let (module, id) = c.split_hash().expect("covered_by names a criterion");
            let criterion = x.modules[module].criteria.iter().find(|k| k.id == id).expect("the criterion exists");
            assert_eq!(r.statement, criterion.statement);
            assert!(r.scenarios.is_empty() && r.decisions.is_empty());
        }
        assert_eq!(scaffold(&x, "C:/repo", &["rust".into()]), s, "a function of the extraction and nothing else");
    }

    /// # Test contract
    ///
    /// ```json
    /// {"test": "a-crate-root-is-named-by-its-crate", "checks": ["crates/core/src/scaffold#requirement-slugs"]}
    /// ```
    #[test]
    fn a_crate_root_is_named_by_its_crate() {
        use crate::extraction::fixtures::{contract, file};
        let one = || contract("A crate.", &[("boundary", "It holds.")]);
        let files = vec![
            file("crates/cli/src/main.rs", "crates/cli/src", &[], one()),
            file("crates/server/src/lib.rs", "crates/server/src", &[], one()),
            file("crates/server/src/cli.rs", "crates/server/src/cli", &[], one()),
            file("src/lib.rs", "src", &[], one()),
        ];
        let s = scaffold(&Extraction::from_files(&files), "C:/repo", &["rust".into()]);
        let named = |module: &str| s.requirements.values().find(|r| r.covered_by[0] == Reference::criterion(module, "boundary")).map(|r| r.slug.clone());
        assert_eq!(named("crates/cli/src").as_deref(), Some("cli-boundary"));
        assert_eq!(named("crates/server/src").as_deref(), Some("server-boundary"));
        assert_eq!(named("src").as_deref(), Some("src-boundary"), "a lone src has nothing better");
        // Two modules ending in the same name: the second takes a suffix.
        let mut clash = [named("crates/server/src/cli"), named("crates/cli/src")].map(|s| s.unwrap_or_default());
        clash.sort();
        assert_eq!(clash, ["cli-boundary".to_string(), "cli-boundary-2".to_string()]);
    }

    /// # Test contract
    ///
    /// ```json
    /// {"test": "skeleton-is-lint-clean-for-contracted-modules", "checks": ["crates/core/src/scaffold#lint-clean", "crates/core/src/scaffold#story-outline"]}
    /// ```
    #[test]
    fn skeleton_is_lint_clean_for_contracted_modules() {
        let x = extraction();
        let s = scaffold(&x, "C:/repo", &["rust".into()]);
        assert!(s.pages.contains_key("overview"));
        // The second root, beside the story and linked from none of it, named with the first.
        assert_eq!(s.config.roots, Roots::new("overview", "rites"));
        let rites = &s.pages["rites"].body;
        for clause in ["held by the work", "what the project is and must do", "how its people work on it", "put a rule of work here"] {
            assert!(rites.contains(clause), "the rites' prompt says {clause:?}: {rites}");
        }
        // The conventions are the rites' first page: how things are written is work.
        assert_eq!(s.pages["rites"].links, vec![Reference::page("conventions")]);
        let conventions = &s.pages["conventions"].body;
        for clause in [
            "naming",
            "pages and decisions",
            "contracts",
            "commits",
            "code style",
            "A convention a tool checks stays here",
            "what the code must do or how it is built belongs in the grimoire",
            "under constraints",
        ] {
            assert!(conventions.contains(clause), "the conventions' prompt says {clause:?}: {conventions}");
        }
        assert!(!s.pages.values().any(|p| p.links.contains(&Reference::page("rites"))), "a root, linked from nothing");
        assert_eq!(s.requirements.len(), 3);
        // The story outline: root links the five story pages in order, and
        // the top-level group hangs under architecture.
        let root_links: Vec<String> = s.pages["overview"].links.iter().map(|r| r.to_string()).collect();
        assert_eq!(root_links, vec!["page:goal", "page:guarantees", "page:glossary", "page:architecture", "page:constraints"]);
        assert!(s.pages["architecture"].links.iter().any(|r| r.kind == Kind::Page && r.id == "src-a"), "{:?}", s.pages["architecture"].links);
        assert!(s.pages["overview"].links.iter().all(|r| r.id != "src-a"));
        // The prompts name the depths: the goal is for the people who use the
        // project and leaves how it is built to the architecture; a guarantee
        // may be unheld; the glossary defines the words the later pages use.
        let goal = &s.pages["goal"].body;
        for clause in ["What this project is and what it is not", "for the people who use it", "shallowest depth", "belongs to the architecture", "to the guarantees"] {
            assert!(goal.contains(clause), "the goal's prompt says {clause:?}: {goal}");
        }
        let guarantees = &s.pages["guarantees"].body;
        assert!(guarantees.contains("promises the people who use it") && guarantees.contains("may be unheld"), "{guarantees}");
        assert!(s.pages["glossary"].body.contains("the words the pages after it use"), "{}", s.pages["glossary"].body);
        let g = Graph::build(&s, &x, &[]);
        let f = lint(&g, &[Rule::Dangling]);
        assert!(f.is_empty(), "{f:?}");
        let f = lint(&g, &[Rule::Uncovered]);
        // Only the uncontracted module remains uncovered, apart from the
        // criteria no test checks yet: the scaffold cannot write tests.
        assert!(f.iter().filter(|f| f.check != "untested").all(|f| f.r#ref.id == "src/b/unrelated"), "{f:?}");
        assert!(f.iter().any(|f| f.check == "untested"));
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
