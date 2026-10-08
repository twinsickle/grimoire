# Say which criteria a test checks

How do I tell Grimoire that a test holds a criterion, so the criterion stops being reported as untested? Put a `# Test contract` heading and one line of JSON in a comment, usually the one directly above the test. The grammar is the same in every language and every test framework; Grimoire never reads the test itself.

```json
{"test": "a-slug", "checks": ["<module id>#<criterion id>"]}
```

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

**The name.** `test` is a slug, used once in its file. The test is then `test:<file>#<name>`, so the name survives renaming the function or moving lines: pick one that says what is checked.

**What it checks.** `checks` lists one or more criteria, each as the module id, `#` and the criterion id, the same name a requirement's `covered_by` uses after `criterion:`. A test that holds a constraint, a rule about how the parts are arranged that no single module promises, names the requirement instead, as `req:<slug>`.

**One block per comment.** A block names the test that follows its comment, so two blocks in one comment, where a run of line comments counts as one, are both reported and neither is credited. A test that checks several criteria lists them all in one block.

**What is reported.** Bad JSON, a name that is not a slug or is used twice in the file, or an empty `checks` is malformed; a check naming a criterion that does not exist is dangling.

## Recipe

```json
{"id":"test-contract","kind":"format","ingredients":[{"id":"heading","says":"`# Test contract` in any comment, usually the one directly above the test, followed by a fenced json block."},{"id":"test","says":"A slug naming the test, unique in its file; the test is test:<file>#<test>."},{"id":"checks","says":"One or more criteria as <module id>#<criterion id>, or a constraint as req:<slug>."},{"id":"one-per-comment","says":"One block per comment; two in one comment are both reported and neither is credited."}],"files":["crates/extract/src/contract.rs",".claude/skills/grimoire/references/contracts.md"]}
```
