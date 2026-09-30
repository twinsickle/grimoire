<!-- grimoire -->
## Grimoire

This project's grimoire (pages, requirements, the module contracts in the
code) is the truth of what it is meant to do. Code and grimoire change in the
same piece of work; the pre-commit hook checks they agree.

- Orient first: `outline` at depth 2, then the goal, architecture,
  constraints and conventions pages, then your area. `get` a module before opening its files.
- Every module in the contract scope carries a `# Module contract`. New
  module, new contract; changed guarantee, changed criterion; new criterion, a
  requirement covering it.
- Work is a ticket: the skill's start, spec and finish sequences. File a
  ticket for anything you find; adopt code without contracts.
- Never write the store's files. The tools are the only interface.
- Propose, don't decide. A gap or question closes on the person's answer to
  options you lay out (the skill's resolve sequence).

Sequences: `.claude/skills/grimoire/SKILL.md`.
<!-- /grimoire -->

## Working on Grimoire itself

Above the marker is what `init` writes into any project. This part is this
repository only.

- Dev loop: `node scripts/dev.mjs` (or `npm run dev:all` in `web/`) runs
  both in one console, output prefixed, stopped together by Ctrl+C:
  `bacon serve --headless`, the daemon on 4242, and `npm run dev`, the UI
  on 5173. The pre-commit hook runs the installed
  binary: after an adapter or schema change, `cargo install --path crates/cli`.
- A release binary carries the built UI: `sh scripts/release` builds `web/`,
  then `cargo build --release`, and checks the binary serves its UI. A debug
  build reads `web/dist` from disk, so `bacon serve` alone serves the last
  built UI on 4242; `npm run dev` is still the hot-reloading one.
- Changed a tool's answer? `web/src/generated/tools.ts` is generated from the
  Rust answer types in `crates/core/src/answer.rs`; a test fails until
  `cargo test -p grimoire-core --test typescript -- --ignored`
  rewrites it.
  Commit the file with the change.
- Touched `web/`? In `web/`: `npm run shots` and look at them;
  `npm run visual` compares with the baselines; `npm run visual:update`
  accepts a change you have seen. All three build the UI and serve the
  behaviour fixture themselves with the installed `grimoire`, so no dev
  loop is needed and a baseline changes only when rendering does. The
  routes they cover are `web/behaviour/routes.ts`, shared with the
  behaviour specs.
- `npm test` in `web/` runs the Vitest unit tests (logic and source rules);
  `npm run behaviour` builds the UI and runs the Playwright behaviour specs
  against a fixture store it serves itself with the installed `grimoire`
  (`GRIMOIRE_BIN` to use another), so they can start and close tickets.
  Reinstall after a Rust change first, or they test yesterday's daemon.
- Lints beyond the check: `cargo clippy` (no `unwrap` outside tests) and
  `npm run check` in `web/` (tsc and ESLint). Under bacon they are the
  `clippy` and `web` jobs. They gate a push, not a commit: install with
  `cp scripts/pre-push .git/hooks/pre-push`. The pre-commit hook stays
  `grimoire check` alone; `init` owns it.
- The skill and its `references/` are installed verbatim into other
  repositories. Nothing about this repository goes in them; it goes here.
