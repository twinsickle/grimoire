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
- Work is a ticket: `/grimoire start`, `spec`, `finish`. `/grimoire file`
  for anything you find; `/grimoire adopt` for code without contracts.
- Never write the store's files. The tools are the only interface.
- Propose, don't decide. A gap or question closes on the person's answer to
  options you lay out.

Sequences: `.claude/skills/grimoire/SKILL.md`.
<!-- /grimoire -->

## Working on Grimoire itself

Above the marker is what `init` writes into any project. This part is this
repository only.

- Dev loop: `bacon serve --headless` runs the daemon on 4242; `npm run dev`
  in `web/` serves the UI on 5173. The pre-commit hook runs the installed
  binary: after an adapter or schema change, `cargo install --path crates/cli`.
- Touched `web/`? In `web/`: `npm run shots` and look at them;
  `npm run visual` compares with the baselines; `npm run visual:update`
  accepts a change you have seen.
- `npm test` in `web/` runs the Vitest unit tests (logic and source rules);
  `npm run behaviour` builds the UI and runs the Playwright behaviour specs
  against a fixture store it serves itself with the installed `grimoire`
  (`GRIMOIRE_BIN` to use another), so they can start and close tickets.
- Lints beyond the check: `cargo clippy` (no `unwrap` outside tests) and
  `npm run check` in `web/` (tsc and ESLint). Under bacon they are the
  `clippy` and `web` jobs. They gate a push, not a commit: install with
  `cp scripts/pre-push .git/hooks/pre-push`. The pre-commit hook stays
  `grimoire check` alone; `init` owns it.
- The skill and its `references/` are installed verbatim into other
  repositories. Nothing about this repository goes in them; it goes here.
