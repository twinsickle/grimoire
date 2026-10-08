# Declare a recipe

How do I tell the people who build with my project how to use what it offers, a component, a content schema, a command, and have Grimoire tell me when that advice may have gone stale? Write a recipe: a Markdown file anywhere in the repository whose name ends `.recipe.md`. The prose is for those people; a heading `Recipe` and the JSON block after it are what Grimoire reads.

````markdown
# Use the button

A button starts an action on the page it is on. For a link to another page, use the link instead.

## Recipe

```json
{"id":"button","kind":"component","ingredients":[{"id":"label","says":"The text on the button, one line."}],"files":["src/components/button"],"uses":["icon"]}
```
````

**The prose.** Say what the thing is for and when not to use it. The first level-one heading is the recipe's title, and the first paragraph is what a catalog shows of it; the id stands in when there is no title.

**The block.** The heading is `Recipe` exactly, at any level, followed by a fenced `json` block. The block is strict:

- `id` and `kind`, each a slug. The recipe is `recipe:<id>`.
- `ingredients`, its parts, each an `id` and what it `says`. An ingredient is `recipe:<id>#<ingredient>`.
- `files`, the paths of the implementation it represents, files or folders, in any language, from the repository root with `/`.
- `uses`, the other recipes it builds on, by id.

The last three may be left out.

**Kinds are yours.** Declare them once with `configure`, each naming the kinds its recipes may use:

```json
{"recipe_kinds": {"component": {"uses": ["component", "icon"]}, "icon": {"uses": []}}}
```

**Place it.** A page links the recipe, as it links a requirement, or it is reported as unlinked. A requirement may name a recipe or an ingredient in its `covered_by`, so its decisions say why the promise is as it is.

**Judge it.** A recipe is judged, not tested. Whenever it or a file it names changes, it is due until someone reads it against its files and records with `judge_recipe` that it held or was updated. The check reports only its connection: a kind not declared, a use its kind may not make, a use or a path that leads nowhere, a recipe no page links, a file not in the format, an id used twice, and a recipe due.

## Recipe

```json
{"id":"recipe-file","kind":"format","ingredients":[{"id":"file-name","says":"A Markdown file anywhere in the repository whose name ends .recipe.md."},{"id":"prose","says":"For the people who build with the project; its first level-one heading is the title, its first paragraph the summary."},{"id":"block","says":"A heading Recipe at any level, then a fenced json block of id and kind, and optionally ingredients, files and uses, strictly."},{"id":"kinds","says":"Declared by the project with configure's recipe_kinds, each naming the kinds its recipes may use."},{"id":"judgement","says":"Due whenever the recipe or a file it names changes, until judge_recipe records that it held or was updated."}],"files":["crates/extract/src/recipe.rs",".claude/skills/grimoire/references/contracts.md"]}
```
