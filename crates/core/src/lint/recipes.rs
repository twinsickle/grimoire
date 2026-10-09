//! # Module contract
//!
//! ```json
//! {"purpose":"Report how each recipe is connected, never what it says: its kind, its uses, its files, its place, its format and whether a judgement holds of it as it is.","criteria":[{"id":"undeclared-kind","statement":"A recipe whose kind the config's recipe kinds do not declare is malformed with the check undeclared-kind, on the recipe, naming the kind."},{"id":"use-not-allowed","statement":"A recipe of a declared kind that uses a recipe whose kind its own kind's uses do not name is malformed with the check use-not-allowed, on the recipe, naming the uses edge to the recipe used and both kinds; a use leading nowhere is left to the dangling edge, and a recipe of an undeclared kind is reported for that alone."},{"id":"missing-file","statement":"Each path of a recipe's files not found in the checkout when it was read is dangling with the check missing-file, on the recipe, naming an edge represents to file:<path>, so two such paths are two findings."},{"id":"unlinked-recipe","statement":"A recipe that no page links is uncovered with the check unlinked-recipe; a requirement naming it does not place it."},{"id":"malformed-recipe","statement":"A recipe file that could not be read as a recipe is malformed with the check malformed-recipe, on file:<path>, with the line and the reason."},{"id":"due-recipe","statement":"A recipe whose digest no judgement of it was made at is uncovered with the check due-recipe, on the recipe, saying it was never judged when it has no judgement and otherwise that it changed since its last judgement, naming that judgement's date and ticket; a judgement made at any earlier digest holds whenever the recipe's digest is that again."},{"id":"duplicate-recipe","statement":"A recipe id held by more than one file is malformed with the check duplicate-recipe, on the recipe, naming every file that holds it."}]}
//! ```

use super::{Finding, Rule};
use crate::graph::{Graph, Node};
use crate::model::*;

/// The recipes the graph holds, with their references.
fn recipes(g: &Graph) -> impl Iterator<Item = (&Reference, &Recipe)> {
    g.nodes.iter().filter_map(|(r, n)| match n {
        Node::Recipe(x) => Some((r, x)),
        _ => None,
    })
}

/// A path of a recipe's files that was not found.
pub(super) fn dangling(g: &Graph, out: &mut Vec<Finding>) {
    for (r, x) in recipes(g) {
        for path in &x.missing {
            let edge = FindingEdge { from: r.clone(), to: Reference::file(path), relation: Relation::Represents };
            out.push(Finding { edge: Some(edge), ..Finding::node(Rule::Dangling, "missing-file", r, format!("{r} represents {path}, which the checkout does not hold")) });
        }
    }
}

/// A recipe no page places, and one no judgement holds of as it is now.
pub(super) fn uncovered(g: &Graph, out: &mut Vec<Finding>) {
    for (r, x) in recipes(g) {
        if g.inbound_with(r, Relation::Links).next().is_none() {
            out.push(Finding::node(Rule::Uncovered, "unlinked-recipe", r, "no page links to this recipe"));
        }
        if g.is_due(x) {
            let reason = match g.judgements.get(&x.id).and_then(|all| all.last()) {
                None => "never judged: judge whether it still says truly how to use what it represents, with `judge_recipe`".to_string(),
                Some(last) => format!(
                    "the recipe or its files changed since it was last judged, on {} in ticket:{}: judge it again with `judge_recipe`, confirming it held or updating it to match",
                    last.date, last.ticket
                ),
            };
            out.push(Finding::node(Rule::Uncovered, "due-recipe", r, reason));
        }
    }
}

/// What breaks the declared rules: the kind, a use, the format, the id.
pub(super) fn malformed(g: &Graph, out: &mut Vec<Finding>) {
    for (r, x) in recipes(g) {
        let Some(kind) = g.recipe_kinds.get(&x.kind) else {
            let declared: Vec<&str> = g.recipe_kinds.keys().map(String::as_str).collect();
            let known = if declared.is_empty() { "none is declared".to_string() } else { format!("those declared are {}", declared.join(", ")) };
            let reason = format!("the kind `{}` is not a recipe kind the config declares ({known}): declare it with `configure {{\"recipe_kinds\": ...}}`", x.kind);
            out.push(Finding::node(Rule::Malformed, "undeclared-kind", r, reason));
            continue;
        };
        for e in g.out_with(r, Relation::Uses) {
            let Some(Node::Recipe(used)) = g.get(&e.to) else { continue };
            if !kind.uses.contains(&used.kind) {
                let edge = FindingEdge { from: r.clone(), to: e.to.clone(), relation: Relation::Uses };
                let reason = format!("a recipe of kind `{}` uses {}, of kind `{}`, which `{}` may not use", x.kind, e.to, used.kind, x.kind);
                out.push(Finding { edge: Some(edge), ..Finding::node(Rule::Malformed, "use-not-allowed", r, reason) });
            }
        }
    }
    for e in &g.recipe_errors {
        out.push(Finding::node(Rule::Malformed, "malformed-recipe", &Reference::file(&e.file), format!("line {}: {}", e.line, e.reason)));
    }
    for (id, files) in &g.recipe_duplicates {
        out.push(Finding::node(Rule::Malformed, "duplicate-recipe", &Reference::recipe(id), format!("the recipe id `{id}` is held by {}; the first is the recipe", files.join(", "))));
    }
}

#[cfg(test)]
mod tests {
    use super::super::lint;
    use super::*;
    use crate::extraction::fixtures::{extraction_files, recipe_file};
    use crate::extraction::Extraction;

    /// The fixture's store with `component` (using components and tokens) and `token` (using nothing)
    /// declared, its page `git-readiness` placing `button`, over these recipe files.
    fn graph_with(recipes: Vec<crate::extraction::FileExtraction>) -> Graph {
        let mut s = crate::graph::fixtures::store();
        s.config.recipe_kinds.insert("component".into(), RecipeKind { uses: vec!["component".into(), "token".into()] });
        s.config.recipe_kinds.insert("token".into(), RecipeKind { uses: vec![] });
        s.pages.get_mut("git-readiness").unwrap().links.push(Reference::recipe("button"));
        let mut files = extraction_files();
        files.extend(recipes);
        Graph::build(&s, &Extraction::from_files(&files), &[])
    }

    fn found<'a>(f: &'a [Finding], check: &str, r: &Reference) -> Vec<&'a Finding> {
        f.iter().filter(|x| x.check == check && &x.r#ref == r).collect()
    }

    /// # Test contract
    ///
    /// ```json
    /// {"test": "a-kind-never-declared", "checks": ["crates/core/src/lint/recipes#undeclared-kind"]}
    /// ```
    #[test]
    fn a_kind_never_declared() {
        let g = graph_with(vec![recipe_file("ui/button.recipe.md", "button", "widget", &["icon"], &[], &[]), recipe_file("ui/icon.recipe.md", "icon", "token", &[], &[], &[])]);
        let f = lint(&g, &Rule::ALL);
        let hit = found(&f, "undeclared-kind", &Reference::recipe("button"));
        assert_eq!(hit.len(), 1);
        assert_eq!(hit[0].rule, Rule::Malformed);
        assert!(hit[0].reason.contains("`widget`") && hit[0].reason.contains("component, token"), "{}", hit[0].reason);
        assert!(found(&f, "undeclared-kind", &Reference::recipe("icon")).is_empty(), "a declared kind");
        assert!(found(&f, "use-not-allowed", &Reference::recipe("button")).is_empty(), "an undeclared kind is reported for that alone");
    }

    /// # Test contract
    ///
    /// ```json
    /// {"test": "a-use-its-kind-may-not-make", "checks": ["crates/core/src/lint/recipes#use-not-allowed", "crates/core/src/lint#dangling-edge-reported"]}
    /// ```
    #[test]
    fn a_use_its_kind_may_not_make() {
        let g = graph_with(vec![
            recipe_file("ui/button.recipe.md", "button", "component", &["colour", "icon"], &[], &[]),
            recipe_file("ui/colour.recipe.md", "colour", "token", &["button", "gone"], &[], &[]),
            recipe_file("ui/icon.recipe.md", "icon", "component", &[], &[], &[]),
        ]);
        let f = lint(&g, &Rule::ALL);
        assert!(found(&f, "use-not-allowed", &Reference::recipe("button")).is_empty(), "a component may use a token and a component");
        let hit = found(&f, "use-not-allowed", &Reference::recipe("colour"));
        assert_eq!(hit.len(), 1, "{hit:?}");
        let e = hit[0].edge.as_ref().expect("names the edge");
        assert_eq!((e.to.clone(), e.relation), (Reference::recipe("button"), Relation::Uses));
        // A use leading nowhere is an ordinary dangling edge.
        let d = found(&f, "dangling-edge", &Reference::recipe("colour"));
        assert_eq!(d.len(), 1);
        assert_eq!(d[0].edge.as_ref().map(|e| e.to.clone()), Some(Reference::recipe("gone")));
    }

    /// # Test contract
    ///
    /// ```json
    /// {"test": "a-file-not-found-is-dangling", "checks": ["crates/core/src/lint/recipes#missing-file"]}
    /// ```
    #[test]
    fn a_file_not_found_is_dangling() {
        let g = graph_with(vec![recipe_file("ui/button.recipe.md", "button", "component", &[], &["src/a/ready.rs", "src/old.ts", "src/gone"], &["src/old.ts", "src/gone"])]);
        let f = lint(&g, &Rule::ALL);
        let hit = found(&f, "missing-file", &Reference::recipe("button"));
        let ends: Vec<String> = hit.iter().map(|x| x.edge.as_ref().unwrap().to.to_string()).collect();
        assert_eq!(ends, ["file:src/gone", "file:src/old.ts"], "two paths, two findings");
        assert!(hit.iter().all(|x| x.rule == Rule::Dangling && x.edge.as_ref().unwrap().relation == Relation::Represents));
        assert_ne!(hit[0].key(), hit[1].key(), "told apart in a baseline");
    }

    /// # Test contract
    ///
    /// ```json
    /// {"test": "a-recipe-no-page-places", "checks": ["crates/core/src/lint/recipes#unlinked-recipe"]}
    /// ```
    #[test]
    fn a_recipe_no_page_places() {
        let g = graph_with(vec![recipe_file("ui/button.recipe.md", "button", "component", &[], &[], &[]), recipe_file("ui/icon.recipe.md", "icon", "component", &[], &[], &[])]);
        let f = lint(&g, &Rule::ALL);
        assert!(found(&f, "unlinked-recipe", &Reference::recipe("button")).is_empty(), "a page links it");
        let hit = found(&f, "unlinked-recipe", &Reference::recipe("icon"));
        assert_eq!(hit.len(), 1);
        assert_eq!(hit[0].rule, Rule::Uncovered);
        // An ingredient is no recipe: it is placed by its recipe.
        assert!(!f.iter().any(|x| x.r#ref == Reference::ingredient("icon", "label")));
    }

    /// # Test contract
    ///
    /// ```json
    /// {"test": "a-recipe-is-due-until-judged-as-it-is", "checks": ["crates/core/src/lint/recipes#due-recipe", "req:recipe-due"]}
    /// ```
    #[test]
    fn a_recipe_is_due_until_judged_as_it_is() {
        let file = recipe_file("ui/button.recipe.md", "button", "component", &[], &[], &[]);
        let Some(Ok(button)) = file.recipe.clone() else { panic!("a recipe") };
        let judged =
            |fingerprint: &str, date: &str| Judgement { recipe: "button".into(), fingerprint: fingerprint.into(), date: date.into(), ticket: "later-1".into(), outcome: JudgementOutcome::Held };
        let due = |judgements: Vec<Judgement>| {
            let mut g = graph_with(vec![file.clone()]);
            if !judgements.is_empty() {
                g.judgements.insert("button".into(), judgements);
            }
            let f = lint(&g, &Rule::ALL);
            found(&f, "due-recipe", &Reference::recipe("button")).into_iter().cloned().collect::<Vec<_>>()
        };
        // Never judged.
        let hit = due(vec![]);
        assert_eq!(hit.len(), 1);
        assert_eq!(hit[0].rule, Rule::Uncovered);
        assert!(hit[0].reason.contains("never judged"), "{}", hit[0].reason);
        // Judged at another state: changed since, naming the last judgement.
        let hit = due(vec![judged("0000000000000000", "2026-10-01"), judged("1111111111111111", "2026-10-05")]);
        assert_eq!(hit.len(), 1);
        assert!(hit[0].reason.contains("changed since") && hit[0].reason.contains("2026-10-05") && hit[0].reason.contains("ticket:later-1"), "{}", hit[0].reason);
        // A judgement at its digest holds, its last or any earlier one: as on a branch merged back to a state judged before.
        assert!(due(vec![judged(&button.digest, "2026-10-06")]).is_empty());
        assert!(due(vec![judged(&button.digest, "2026-10-01"), judged("1111111111111111", "2026-10-05")]).is_empty(), "an earlier judgement holds again");
    }

    /// # Test contract
    ///
    /// ```json
    /// {"test": "a-file-not-in-the-format","checks": ["crates/core/src/lint/recipes#malformed-recipe"]}
    /// ```
    #[test]
    fn a_file_not_in_the_format() {
        let mut broken = recipe_file("docs/broken.recipe.md", "x", "component", &[], &[], &[]);
        broken.recipe = Some(Err(RecipeError { file: "docs/broken.recipe.md".into(), line: 4, reason: "the block is not JSON".into() }));
        let g = graph_with(vec![broken]);
        let f = lint(&g, &Rule::ALL);
        let hit = found(&f, "malformed-recipe", &Reference::file("docs/broken.recipe.md"));
        assert_eq!(hit.len(), 1);
        assert_eq!(hit[0].reason, "line 4: the block is not JSON");
    }

    /// # Test contract
    ///
    /// ```json
    /// {"test": "a-requirement-a-recipe-carries-is-held", "checks": ["crates/core/src/lint#unheld"]}
    /// ```
    #[test]
    fn a_requirement_a_recipe_carries_is_held() {
        let mut s = crate::graph::fixtures::store();
        let mut q = s.requirements["ready-only-with-commit"].clone();
        q.slug = "label-is-one-line".into();
        q.covered_by = vec![Reference::ingredient("button", "label")];
        s.requirements.insert(q.slug.clone(), q);
        s.pages.get_mut("git-readiness").unwrap().links.push(Reference::req("label-is-one-line"));
        let mut files = extraction_files();
        files.push(recipe_file("ui/button.recipe.md", "button", "component", &[], &[], &[]));
        let g = Graph::build(&s, &Extraction::from_files(&files), &[]);
        let f = lint(&g, &Rule::ALL);
        assert!(!f.iter().any(|x| x.r#ref == Reference::req("label-is-one-line")), "{f:?}");
    }

    /// # Test contract
    ///
    /// ```json
    /// {"test": "one-id-in-two-files", "checks": ["crates/core/src/lint/recipes#duplicate-recipe"]}
    /// ```
    #[test]
    fn one_id_in_two_files() {
        let g = graph_with(vec![recipe_file("ui/button.recipe.md", "button", "component", &[], &[], &[]), recipe_file("old/button.recipe.md", "button", "component", &[], &[], &[])]);
        let f = lint(&g, &Rule::ALL);
        let hit = found(&f, "duplicate-recipe", &Reference::recipe("button"));
        assert_eq!(hit.len(), 1);
        assert!(hit[0].reason.contains("old/button.recipe.md, ui/button.recipe.md"), "{}", hit[0].reason);
    }
}
