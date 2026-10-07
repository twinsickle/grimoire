# Module contracts

A module contract is the code level of the grimoire: a purpose and criteria,
written in the source file, read by Grimoire on every change and never
stored. Requirements name criteria in `covered_by` and tests name them in a
test contract; the lints report a criterion no requirement names, a
criterion no test checks, and a module expected to carry a contract that has
none.

## The grammar, for every language

In the file's **leading documentation comment**, a heading `# Module
contract` followed by a fenced `json` block holding one object:

```json
{"purpose":"One sentence: what this module is for.","criteria":[{"id":"a-slug","statement":"One checkable sentence about the module's behaviour."}]}
```

- `purpose`: one sentence, non-empty.
- `criteria`: a list of at least one; a contract promises something. Each
  `id` is a slug (`^[a-z0-9]+(-[a-z0-9]+)*$`), unique within the module.
  Each `statement` is one sentence stating something someone could check
  against the code: a guarantee, a boundary, a rule. Not a description of
  what the code does line by line.
- The block is strict JSON: unknown fields, a missing `purpose`, an empty
  `criteria` or a duplicate id make the contract malformed, and the module
  is reported. A module with nothing of its own to promise (a pure
  re-export) states one criterion, or is taken out of the contract scope
  with `configure`, which is the person's call.
- A criterion is addressed as `criterion:<module id>#<id>`. Renaming an id
  is a change to the grimoire: the requirement naming it goes dangling and the lint
  says so; update the requirement's `covered_by` in the same piece of work.

Keep it on one line inside the fence; the extractor does not care, but
diffs stay readable and a contract that is easy to see is a contract that
gets kept current.

## Rust

The leading `//!` block of the file. The crate root's contract lives in
`lib.rs` or `main.rs` and is the contract of the module `<crate>/src`.

```rust
//! # Module contract
//!
//! ```json
//! {"purpose":"Decide whether a workspace's repository is ready to use.","criteria":[{"id":"ready-only-with-commit","statement":"A workspace is ready only when its repository has at least one commit."},{"id":"reads-only","statement":"Readiness checks never write to the repository."}]}
//! ```

use std::path::Path;
```

Expected to carry a contract: every `.rs` under a workspace member's
`src/`. Not expected: `tests.rs`, `*_tests.rs`, anything under a `tests/`
directory, `build.rs`, `examples/`, `benches/`. Module identity is the path
without `.rs`; `mod.rs` folds into its directory; test files fold into the
module they test.

## TypeScript

The leading `/** */` block (or a run of `//` lines) at the top of the file,
before any import.

```ts
/**
 * # Module contract
 *
 * ```json
 * {"purpose":"Reach the daemon: one function per tool and one shared event stream.","criteria":[{"id":"tools-only","statement":"Every request the UI makes is a tool call by name; there is no UI-only endpoint."}]}
 * ```
 */

import { useEffect } from "react";
```

Expected to carry a contract: every `.ts` or `.tsx` under a package's
`src/` (a package is a directory with `package.json`). Not expected:
`*.test.*`, `*.spec.*`, `*.d.ts`, anything under `__tests__/`, and files
outside `src/` such as `vite.config.ts`. Module identity is the path without
extension; `index` folds into its directory.

## Java

A top-level `/** */` Javadoc, outside every type's body: the first one
holding the heading, so a class's own Javadoc after the package and imports
carries it, and a `/* */` licence header above is never taken for it. A
package's contract goes in the Javadoc of its `package-info.java`.

```java
package com.example.core;

import com.example.core.util.Strings;

/**
 * # Module contract
 *
 * ```json
 * {"purpose":"Greet people by name.","criteria":[{"id":"greets","statement":"A greeting names the person greeted."}]}
 * ```
 */
public class Greeter { /* ... */ }
```

Expected to carry a contract: every `.java` under a `src/main/java`
directory, at any depth. Not expected: `package-info.java` and
`module-info.java` (either may carry one), anything under `src/test`, and
files elsewhere. Module identity is the path without `.java`;
`package-info.java` folds into its directory, the package. Imports resolve
through the packages the repository's own files declare; an on-demand
import stands for every type file of its package and its
`package-info.java`.

## Languages read by their comments alone

A language with no adapter of its own but a known comment syntax is read
by its comments alone (tier 0): HTML, HTL, XML, CSS and SCSS so far. Name it
in the config's languages as any other; `status` shows the tier each runs
at. The contract goes in the file's first comment, which may follow only
whitespace and, in markup, the prolog (XML declaration, processing
instructions, doctype); test blocks go in any comment. Such a file gives
no declarations and no imports.

````html
<!--
  # Module contract

  ```json
  {"purpose":"Render a teaser card.","criteria":[{"id":"links-to-target","statement":"A teaser links to the page it names."}]}
  ```
-->
<div class="teaser">...</div>
````

In HTL, `<!--/* */-->` works as well, and is never sent to the page.
HTML and HTL both read `.html`; a project naming both reads it by the
one it names first.
Expected to carry a contract: every file of the language; narrow it with
the contract scope through `configure`. Module identity is the path with
its extension, so a component's markup and its styles are two modules.

## Test contracts

A test says which criteria it checks in a `# Test contract` block, in any
comment, usually the one directly above the test. The grammar is the same
for every language and every test framework; Grimoire never reads the test
itself.

```json
{"test": "a-slug", "checks": ["<module id>#<criterion id>"]}
```

- `test`: a slug naming the test, unique within its file. The test is
  `test:<file>#<test>`; the name survives renaming the function or moving
  lines, so pick one that says what is checked.
- `checks`: one or more criteria, each as module id, `#`, criterion id, the
  same module id a requirement's `covered_by` uses after `criterion:`; or a
  constraint as `req:<slug>`.
- One block per comment. A block names the test that follows its comment,
  so two blocks in one comment (one run of line comments counts as one) are
  both reported malformed and neither is credited. A test that checks
  several criteria lists them all in one block's `checks`.
- A criterion says what one module does. A rule about how the parts are
  arranged (a layer does no I/O, one component is the only writer) is a
  constraint: a requirement on the Constraints page with no criterion, held
  by a test that names it, often one that reads the source. A constraint no
  test holds is reported as `unheld`.
- A broken block (bad JSON, a name that is not a slug or is used twice in
  the file, an empty `checks`) is reported as malformed. A check naming a
  criterion that does not exist is reported dangling.

A Rust unit test:

```rust
    /// # Test contract
    ///
    /// ```json
    /// {"test": "ready-needs-a-commit", "checks": ["src/ready#ready-only-with-commit"]}
    /// ```
    #[test]
    fn empty_repository_is_not_ready() { /* ... */ }
```

A TypeScript spec, even one whose tests are generated in a loop:

```ts
/**
 * # Test contract
 *
 * ```json
 * {"test": "every-view-matches-its-baseline", "checks": ["src/App#every-view-has-a-baseline"]}
 * ```
 */
for (const v of views) test(v.name, async ({ page }) => { /* ... */ });
```

## Recipes

What a project offers the people who build with it (a component, a
content schema, a kind of game content) is declared in a recipe: a
Markdown file anywhere in the repository whose name ends `.recipe.md`,
read whatever the project's languages. The prose is for those people:
what the thing is for and when not to use it; its first level-one
heading is the recipe's title. A heading `Recipe`, at any level, and the
fenced `json` block after it hold what Grimoire reads:

```json
{"id":"button","kind":"component","ingredients":[{"id":"label","says":"The text on the button, one line."}],"files":["src/components/button"],"uses":["icon"]}
```

- `id` and `kind` are slugs; the kind is one the project declares with
  `configure {"recipe_kinds": {"component": {"uses": ["component", "icon"]}, ...}}`,
  each kind naming the kinds its recipes may use.
- `ingredients` are its parts, each an id and what it `says`;
  `files` are the paths, files or folders, of the implementation it
  represents; `uses` names other recipes by id. All three may be left out.
- The block is strict, as a contract is. The recipe is `recipe:<id>`, an
  ingredient `recipe:<id>#<ingredient>`. A page links a recipe to place
  it; a requirement may name a recipe or an ingredient in `covered_by`.
- A recipe is judged, not tested: no test names it and the code carries
  no marker. The check reports only its connection: a kind not declared,
  a use its kind may not make, a use or a path that leads nowhere, a
  recipe no page links, a file not in the format, an id used twice.

## Writing a good criterion

- One sentence, one claim. "Writes are atomic" and "malformed files are
  reported, not fatal" are two criteria, not one.
- State the guarantee, not the mechanism, unless the mechanism is the
  guarantee ("written to a temporary file and renamed into place" is the
  point of an atomic write).
- If you cannot imagine a test or a review question for it, it is not a
  criterion; it is a comment.
- Three to five per module is typical. A module with twelve is usually two
  modules.
