//! # Module contract
//!
//! ```json
//! {"purpose":"Read a recipe file: the fixed-format block Grimoire checks, and the prose around it for the people who build with the project.","criteria":[{"id":"format","statement":"A recipe file holds one heading whose text is Recipe, at any level and outside a fenced block opened with backticks or tildes, followed by a fenced json block of exactly id and kind, each a slug, and optionally ingredients of {id, says}, files and uses, each id a slug used once, says non-empty, each file a repository-relative /-separated path used once and each use a slug used once; any other field, a required one missing, no heading, no block, a second Recipe heading or invalid JSON is reported as the file's error with the heading's line and the reason, never as no recipe."},{"id":"prose","statement":"A recipe's title is the text of the first level-one heading outside a fenced block that is not the Recipe heading, or its id when there is none, and its text is the rest of the file without that heading, the Recipe heading and the block, blank lines at its ends trimmed."},{"id":"files-looked-for","statement":"Each path of a recipe's files is looked for under the repository root when the recipe is read, as a file or a folder, and those not found are kept with the recipe as missing, in the order the block gives them; nothing is written."}]}
//! ```

use std::path::Path;

use grimoire_core::model::{Ingredient, Recipe, RecipeError};
use grimoire_core::store::{is_scope_prefix, is_slug};
use grimoire_core::FileExtraction;
use serde::Deserialize;

/// The name every recipe file's name ends with.
pub const SUFFIX: &str = ".recipe.md";

/// Whether a repository-relative path names a recipe file: its name ends `.recipe.md`.
pub fn is_recipe_path(rel_path: &str) -> bool {
    rel_path.rsplit('/').next().is_some_and(|name| name.len() > SUFFIX.len() && name.ends_with(SUFFIX))
}

/// The block as written: what the format allows and nothing else.
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Block {
    id: String,
    kind: String,
    #[serde(default)]
    ingredients: Vec<IngredientBlock>,
    #[serde(default)]
    files: Vec<String>,
    #[serde(default)]
    uses: Vec<String>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct IngredientBlock {
    id: String,
    says: String,
}

/// A Markdown heading: its level and its text, or `None`.
fn heading(line: &str) -> Option<(usize, &str)> {
    let t = line.trim_start();
    let level = t.chars().take_while(|c| *c == '#').count();
    if level == 0 || level > 6 {
        return None;
    }
    let rest = &t[level..];
    (rest.is_empty() || rest.starts_with(' ')).then(|| (level, rest.trim().trim_end_matches('#').trim()))
}

fn is_fence(line: &str) -> bool {
    line.trim_start().starts_with("```")
}

/// The fence a line opens or closes a block with: backticks or tildes, as Markdown allows both,
/// and how many, since a block is closed only by a run of its own character at least as long.
fn fence_marker(line: &str) -> Option<(char, usize, &str)> {
    let t = line.trim_start();
    let c = t.chars().next().filter(|c| *c == '`' || *c == '~')?;
    let n = t.chars().take_while(|x| *x == c).count();
    (n >= 3).then(|| (c, n, t[n..].trim()))
}

/// Read a recipe file's text. The paths of `files` are not looked for here: `missing` is empty.
pub fn parse(rel_path: &str, text: &str) -> Result<Recipe, RecipeError> {
    let text = text.strip_prefix('\u{feff}').unwrap_or(text);
    let lines: Vec<&str> = text.lines().collect();
    let error = |line: usize, reason: String| RecipeError { file: rel_path.to_string(), line: line as u32 + 1, reason };
    // Headings outside fenced blocks, so an example in the prose is never read as one.
    let mut headings: Vec<(usize, usize, &str)> = Vec::new();
    // The fence open, if any: closed only by its own character, as long or longer, with nothing after it.
    let mut fenced: Option<(char, usize)> = None;
    for (i, line) in lines.iter().enumerate() {
        let marker = fence_marker(line);
        match (fenced, marker) {
            (None, Some((c, n, _))) => fenced = Some((c, n)),
            (Some((open, len)), Some((c, n, rest))) if c == open && n >= len && rest.is_empty() => fenced = None,
            (Some(_), _) => {}
            (None, None) => {
                if let Some((level, t)) = heading(line) {
                    headings.push((i, level, t));
                }
            }
        }
    }
    // The heading is `Recipe` exactly: the format is checked as written.
    let recipe_headings: Vec<usize> = headings.iter().filter(|(_, _, t)| *t == "Recipe").map(|(i, _, _)| *i).collect();
    let at = match recipe_headings.as_slice() {
        [] => return Err(error(0, "no `Recipe` heading: a recipe file holds a heading `Recipe` followed by a fenced json block".to_string())),
        [one] => *one,
        [_, second, ..] => return Err(error(*second, format!("a second `Recipe` heading; a file holds one recipe, its heading at line {}", recipe_headings[0] + 1))),
    };
    // The fenced block after the heading, blank lines between.
    let mut open = at + 1;
    while open < lines.len() && lines[open].trim().is_empty() {
        open += 1;
    }
    if open >= lines.len() || !is_fence(lines[open]) {
        return Err(error(at, "the `Recipe` heading is not followed by a ``` fenced json block".to_string()));
    }
    let lang = lines[open].trim().trim_start_matches('`').trim();
    if !lang.eq_ignore_ascii_case("json") {
        let named = if lang.is_empty() { "names no language".to_string() } else { format!("is `{lang}`") };
        return Err(error(at, format!("the fenced block {named}: a recipe's block is ```json")));
    }
    let close = (open + 1..lines.len()).find(|i| lines[*i].trim() == "```").ok_or_else(|| error(at, "the fenced block is not closed".to_string()))?;
    let body = lines[open + 1..close].join("\n");
    let block: Block = serde_json::from_str(&body).map_err(|e| error(at, format!("invalid recipe JSON: {e}")))?;
    let reasons = check(&block);
    if !reasons.is_empty() {
        return Err(error(at, reasons.join("; ")));
    }
    // The title is the prose's own first level-one heading, or the id.
    let title_at = headings.iter().find(|(i, level, _)| *i != at && *level == 1).map(|(i, _, t)| (*i, t.to_string()));
    let kept: Vec<&str> = lines.iter().enumerate().filter(|(i, _)| *i != at && !(open..=close).contains(i) && title_at.as_ref().is_none_or(|(t, _)| t != i)).map(|(_, l)| *l).collect();
    let first = kept.iter().position(|l| !l.trim().is_empty()).unwrap_or(kept.len());
    let last = kept.iter().rposition(|l| !l.trim().is_empty()).map_or(first, |i| i + 1);
    let prose = kept[first..last.max(first)].join("\n");
    Ok(Recipe {
        title: title_at.map(|(_, t)| t).filter(|t| !t.is_empty()).unwrap_or_else(|| block.id.clone()),
        text: prose,
        ingredients: block.ingredients.iter().map(|i| Ingredient { recipe: block.id.clone(), id: i.id.clone(), says: i.says.trim().to_string() }).collect(),
        files: block.files,
        uses: block.uses,
        file: rel_path.to_string(),
        line: at as u32 + 1,
        missing: Vec::new(),
        id: block.id,
        kind: block.kind,
    })
}

/// Every rule the block breaks.
fn check(b: &Block) -> Vec<String> {
    let mut out = Vec::new();
    for (field, v) in [("id", &b.id), ("kind", &b.kind)] {
        if !is_slug(v) {
            out.push(format!("`{field}` `{v}` is not a slug"));
        }
    }
    let twice = |items: &[&String]| -> Option<String> { items.iter().enumerate().find(|(i, x)| items[..*i].contains(x)).map(|(_, x)| x.to_string()) };
    let ids: Vec<&String> = b.ingredients.iter().map(|i| &i.id).collect();
    for i in &b.ingredients {
        if !is_slug(&i.id) {
            out.push(format!("the ingredient `{}` is not a slug", i.id));
        }
        if i.says.trim().is_empty() {
            out.push(format!("the ingredient `{}` says nothing", i.id));
        }
    }
    if let Some(x) = twice(&ids) {
        out.push(format!("the ingredient `{x}` is named twice"));
    }
    for f in &b.files {
        if !is_scope_prefix(f) {
            out.push(format!("`{f}` in files is not a repository-relative path like `src/button`"));
        }
    }
    if let Some(x) = twice(&b.files.iter().collect::<Vec<_>>()) {
        out.push(format!("`{x}` is named twice in files"));
    }
    for u in &b.uses {
        if !is_slug(u) {
            out.push(format!("`{u}` in uses is not a recipe id"));
        }
    }
    if let Some(x) = twice(&b.uses.iter().collect::<Vec<_>>()) {
        out.push(format!("`{x}` is named twice in uses"));
    }
    out
}

/// The paths of `files` not found under `root`, as a file or a folder, in the order given.
pub fn missing(root: &Path, files: &[String]) -> Vec<String> {
    files.iter().filter(|f| !root.join(f.replace('/', std::path::MAIN_SEPARATOR_STR)).exists()).cloned().collect()
}

/// Extract one recipe file whose text is already read: its record joins as the recipe, never as a module.
pub fn extract(root: &Path, rel_path: &str, text: &str) -> FileExtraction {
    let recipe = parse(rel_path, text).map(|mut r| {
        r.missing = missing(root, &r.files);
        r
    });
    FileExtraction {
        path: rel_path.to_string(),
        language: "recipe".to_string(),
        module: String::new(),
        decls: Vec::new(),
        imports: Vec::new(),
        expects_contract: false,
        digest: grimoire_core::fingerprint::of_text(text),
        tests: Vec::new(),
        test_errors: Vec::new(),
        contract: None,
        contract_line: None,
        lines: u32::try_from(text.lines().count()).unwrap_or(u32::MAX),
        test_lines: 0,
        recipe: Some(recipe),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const BUTTON: &str = "# Button

What a person presses to act. Not for navigation: use a link.

## Recipe

```json
{\"id\":\"button\",\"kind\":\"component\",\"ingredients\":[{\"id\":\"label\",\"says\":\"The text on the button.\"}],\"files\":[\"src/button\"],\"uses\":[\"icon\"]}
```

## Example

```md
# Recipe
not a heading, inside a fence
```
";

    /// # Test contract
    ///
    /// ```json
    /// {"test": "a-recipe-reads-its-block", "checks": ["crates/extract/src/recipe#format"]}
    /// ```
    #[test]
    fn a_recipe_reads_its_block() {
        let r = parse("ui/button.recipe.md", BUTTON).unwrap();
        assert_eq!((r.id.as_str(), r.kind.as_str(), r.line), ("button", "component", 5));
        assert_eq!(r.ingredients, [Ingredient { recipe: "button".into(), id: "label".into(), says: "The text on the button.".into() }]);
        assert_eq!((r.files.clone(), r.uses.clone()), (vec!["src/button".to_string()], vec!["icon".to_string()]));
        // The optional fields left out are empty.
        let bare = parse("q.recipe.md", "## Recipe\n```json\n{\"id\":\"quest\",\"kind\":\"quest\"}\n```\n").unwrap();
        assert!(bare.ingredients.is_empty() && bare.files.is_empty() && bare.uses.is_empty());
        assert!(is_recipe_path("ui/button.recipe.md") && !is_recipe_path("docs/recipe.md") && !is_recipe_path(".recipe.md") && !is_recipe_path("README.md"));
    }

    /// # Test contract
    ///
    /// ```json
    /// {"test": "a-file-not-in-the-format-says-why", "checks": ["crates/extract/src/recipe#format"]}
    /// ```
    #[test]
    fn a_file_not_in_the_format_says_why() {
        let bad = |text: &str| parse("x.recipe.md", text).unwrap_err();
        let block = |json: &str| format!("# X\n\n# Recipe\n\n```json\n{json}\n```\n");
        let e = bad(&block("{\"id\":\"x\",\"kind\":\"k\",\"owner\":\"design\"}"));
        assert!(e.reason.contains("owner") && e.line == 3, "{e:?}");
        assert!(bad(&block("{\"id\":\"x\"}")).reason.contains("kind"), "a required field missing");
        assert!(bad(&block("{\"id\":\"x\",\"kind\":\"k\"")).reason.contains("invalid recipe JSON"));
        assert!(bad(&block("{\"id\":\"Not A Slug\",\"kind\":\"k\"}")).reason.contains("not a slug"));
        let e = bad(&block("{\"id\":\"x\",\"kind\":\"k\",\"ingredients\":[{\"id\":\"a\",\"says\":\"\"},{\"id\":\"a\",\"says\":\"b\"}],\"files\":[\"../out\",\"src\",\"src\"],\"uses\":[\"y\",\"y\"]}"));
        for part in ["says nothing", "`a` is named twice", "`../out`", "`src` is named twice in files", "`y` is named twice in uses"] {
            assert!(e.reason.contains(part), "{part}: {}", e.reason);
        }
        assert!(bad("# X\n\nJust prose.\n").reason.contains("no `Recipe` heading"));
        assert!(bad("# Recipe\n\nNo block.\n").reason.contains("not followed"));
        assert!(bad("# Recipe\n```yaml\nid: x\n```\n").reason.contains("is `yaml`"));
        assert!(bad("# Recipe\n```\n{\"id\":\"x\",\"kind\":\"k\"}\n```\n").reason.contains("names no language"), "the block says it is json");
        // A heading inside a fence of tildes is no heading, nor is one inside backticks a tilde fence holds.
        let quoted = "# Real\n\n~~~md\n# Recipe\n```\n# Title\n~~~\n\n## Recipe\n```json\n{\"id\":\"q\",\"kind\":\"k\"}\n```\n";
        let r = parse("q.recipe.md", quoted).unwrap();
        assert_eq!((r.title.as_str(), r.line), ("Real", 9));
        // A shorter run of the same character inside a longer fence closes nothing.
        let nested = "# Real\n\n````md\n```\n# Recipe\n```\n````\n\n## Recipe\n```json\n{\"id\":\"q\",\"kind\":\"k\"}\n```\n";
        assert_eq!(parse("q.recipe.md", nested).unwrap().line, 9);
        // The heading is Recipe as written, not another casing.
        assert!(bad("# recipe\n```json\n{\"id\":\"x\",\"kind\":\"k\"}\n```\n").reason.contains("no `Recipe` heading"));
        assert!(bad("# Recipe\n```json\n{}\n").reason.contains("not closed"));
        let two = bad(&format!("{}\n## Recipe\n```json\n{{\"id\":\"y\",\"kind\":\"k\"}}\n```\n", block("{\"id\":\"x\",\"kind\":\"k\"}")));
        assert!(two.reason.contains("second") && two.line == 9, "{two:?}");
    }

    /// # Test contract
    ///
    /// ```json
    /// {"test": "the-prose-is-for-people", "checks": ["crates/extract/src/recipe#prose"]}
    /// ```
    #[test]
    fn the_prose_is_for_people() {
        let r = parse("ui/button.recipe.md", BUTTON).unwrap();
        assert_eq!(r.title, "Button");
        assert!(r.text.starts_with("What a person presses"), "{}", r.text);
        assert!(!r.text.contains("\"id\"") && !r.text.contains("## Recipe") && !r.text.contains("# Button"), "{}", r.text);
        assert!(r.text.contains("## Example") && r.text.ends_with("```"), "the rest is kept: {}", r.text);
        // With no heading of its own, the title is the id.
        let bare = parse("q.recipe.md", "\n## Recipe\n```json\n{\"id\":\"quest\",\"kind\":\"quest\"}\n```\n\nFind the key.\n").unwrap();
        assert_eq!((bare.title.as_str(), bare.text.as_str()), ("quest", "Find the key."));
    }

    /// # Test contract
    ///
    /// ```json
    /// {"test": "the-files-it-names-are-looked-for", "checks": ["crates/extract/src/recipe#files-looked-for"]}
    /// ```
    #[test]
    fn the_files_it_names_are_looked_for() {
        let dir = tempfile::Builder::new().prefix("grimoire-recipe-").tempdir().unwrap();
        std::fs::create_dir_all(dir.path().join("src").join("button")).unwrap();
        std::fs::write(dir.path().join("src").join("icon.ts"), "export {};\n").unwrap();
        let text = "# Recipe\n```json\n{\"id\":\"b\",\"kind\":\"k\",\"files\":[\"src/old.ts\",\"src/button\",\"src/icon.ts\",\"gone\"]}\n```\n";
        let x = extract(dir.path(), "b.recipe.md", text);
        let r = x.recipe.clone().unwrap().unwrap();
        assert_eq!(r.missing, ["src/old.ts", "gone"], "a folder and a file are found, in the order given");
        assert_eq!(x.module, "", "no module's");
        assert_eq!(x.digest, grimoire_core::fingerprint::of_text(text));
        let names: Vec<_> = std::fs::read_dir(dir.path()).unwrap().map(|e| e.unwrap().file_name()).collect();
        assert_eq!(names.len(), 1, "nothing written");
    }
}
