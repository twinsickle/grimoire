# Write a module contract in TypeScript or JavaScript

How do I say what a TypeScript or JavaScript module promises? Put a `# Module contract` heading and one line of JSON in the comment at the top of the file, before any import: a purpose and the criteria a test or a reviewer can check.

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

**Where it goes.** The leading `/** */` block, or a run of `//` lines, at the top of the file. A `#!` line may come first.

**Name the language.** `typescript` and `javascript` are named apart in the store config's languages (`configure`), and are read the same way. Name `javascript` only if your JavaScript files should carry contracts too.

**Which files need one.** Every `.ts`, `.tsx`, `.mts` or `.cts` file, and with `javascript` named every `.js`, `.jsx`, `.mjs` or `.cjs` file, under a package's `src/`, a package being a folder with a `package.json`. Not `*.test.*`, `*.spec.*`, declaration files (`*.d.ts` and the like), anything under `__tests__/`, and files outside `src/` such as `vite.config.ts`.

**What the module is called.** Its path from the repository root without the extension, the same in both languages; an `index` file folds into its folder. `web/src/api.ts` is `web/src/api`, and a criterion is `criterion:web/src/api#<id>`.

**The JSON.** Strict: an unknown field, a missing purpose, an empty `criteria` or an id used twice makes the contract malformed, and the check reports the module. Keep it on one line.

**When not to.** A rule about how the parts are arranged is a constraint, a requirement held by a test, not one module's criterion.

To say which criteria a spec checks, even one whose tests are generated in a loop, see the test contract recipe.

## Recipe

```json
{"id":"module-contract-typescript","kind":"format","ingredients":[{"id":"heading","says":"`# Module contract` in the leading /** */ block or run of // lines, before any import, followed by a fenced json block."},{"id":"purpose","says":"One sentence: what the module is for, not what it contains."},{"id":"criteria","says":"At least one; each an id, a slug unique in the module, and a statement, one checkable sentence in the present tense."},{"id":"module-id","says":"The path from the repository root without its extension; index folds into its folder."},{"id":"coverage","says":"Every TypeScript file, and JavaScript file when javascript is named, under a package's src/, but tests, specs, declaration files and __tests__/."}],"files":["crates/extract/src/typescript.rs","crates/extract/src/contract.rs",".claude/skills/grimoire/references/contracts.md"],"uses":["test-contract"]}
```
