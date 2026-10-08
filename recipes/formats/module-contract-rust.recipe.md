# Write a module contract in Rust

How do I say what a Rust module promises, so Grimoire can hold the code to it? Put a `# Module contract` heading and one line of JSON in the file's leading `//!` comment: a purpose and the criteria a test or a reviewer can check.

```rust
//! # Module contract
//!
//! ```json
//! {"purpose":"Decide whether a workspace's repository is ready to use.","criteria":[{"id":"ready-only-with-commit","statement":"A workspace is ready only when its repository has at least one commit."},{"id":"reads-only","statement":"Readiness checks never write to the repository."}]}
//! ```

use std::path::Path;
```

**Where it goes.** The first `//!` block of the file, before any code. The crate root's contract lives in `lib.rs` or `main.rs` and is the contract of the module `<crate>/src`.

**Which files need one.** Every `.rs` file under a workspace member's `src/`. Not `tests.rs`, `*_tests.rs`, anything under a `tests/` folder, `build.rs`, `examples/` or `benches/`. To leave a folder out on purpose, such as generated code, narrow the contract scope with `configure`.

**What the module is called.** Its path from the workspace root without `.rs`: `crates/core/src/model.rs` is `crates/core/src/model`. A `mod.rs` folds into its folder, `tests.rs` into the module it tests. A criterion is then `criterion:crates/core/src/model#<id>`, the name a requirement's `covered_by` and a test's `checks` use.

**The JSON.** Strict: an unknown field, a missing purpose, an empty `criteria` or an id used twice makes the contract malformed and the check reports the module. Keep it on one line; it diffs cleanly and is easy to see.

**When not to.** A criterion about how parts are arranged (a layer does no I/O, one component is the only writer) is not one module's promise: write it as a requirement on your constraints page, held by a test that names the requirement.

To say which criteria a Rust test checks, see the test contract recipe.

## Recipe

```json
{"id":"module-contract-rust","kind":"format","ingredients":[{"id":"heading","says":"`# Module contract` in the file's leading //! comment, followed by a fenced json block."},{"id":"purpose","says":"One sentence: what the module is for, not what it contains."},{"id":"criteria","says":"At least one; each an id, a slug unique in the module, and a statement, one checkable sentence in the present tense."},{"id":"module-id","says":"The path from the workspace root without .rs; mod.rs folds into its folder, a crate root into its src folder."},{"id":"coverage","says":"Every .rs under a workspace member's src/ carries one, but tests.rs, *_tests.rs, tests/, build.rs, examples/ and benches/."}],"files":["crates/extract/src/rust.rs","crates/extract/src/contract.rs",".claude/skills/grimoire/references/contracts.md"],"uses":["test-contract"]}
```
