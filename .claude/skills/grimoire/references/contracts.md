# Module contracts

A module contract is the code level of the grimoire: a purpose and criteria,
written in the source file, read by Grimoire on every change and never
stored. Requirements name criteria in `covered_by`; the lints report a
criterion no requirement names and a module expected to carry a contract
that has none.

## The grammar, for every language

In the file's **leading documentation comment**, a heading `# Module
contract` followed by a fenced `json` block holding one object:

```json
{"purpose":"One sentence: what this module is for.","criteria":[{"id":"a-slug","statement":"One checkable sentence about the module's behaviour."}]}
```

- `purpose`: one sentence, non-empty.
- `criteria`: a list, may be empty. Each `id` is a slug
  (`^[a-z0-9]+(-[a-z0-9]+)*$`), unique within the module. Each `statement`
  is one sentence stating something someone could check against the code:
  a guarantee, a boundary, a rule. Not a description of what the code does
  line by line.
- The block is strict JSON: unknown fields, a missing `purpose` or a
  duplicate id make the contract malformed, and the module is reported.
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
