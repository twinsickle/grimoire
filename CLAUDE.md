<!-- grimoire -->
## Grimoire

This project's grimoire (pages, requirements, the module contracts in the
code) is the truth of what it is meant to do. Code and grimoire change in the
same piece of work; the pre-commit hook finds what is left unconnected.

- Orient first: `outline` at depth 2, then the pages each root links,
  in their order, then your area. `get` a module before opening its files.
- Every module in the contract scope carries a `# Module contract`. New
  module, new contract; changed guarantee, changed criterion; new criterion, a
  requirement covering it.
- Work is a ticket: the skill's start, spec and finish sequences. A write
  that changes intent lands in its ticket's proposal and reaches the truth
  when the ticket closes. File a ticket for anything you find; adopt code
  without contracts.
- Never write the store's files. The tools are the only interface.
- Propose, don't decide. A gap or question closes on the person's answer to
  options you lay out (the skill's resolve sequence).

Sequences: `.claude/skills/grimoire/SKILL.md`.
<!-- /grimoire -->

## Working on Grimoire itself

Above the marker is what `init` writes into any project. This part is this
repository only.

- Dev loop: `node scripts/dev.mjs` (or `npm run dev:all` in `web/`) runs
  all three in one console, output prefixed, stopped together by Ctrl+C:
  the shared server on 4243 with the project's site on 4244,
  `bacon serve --headless`, the daemon on 4242,
  and `npm run dev`, the UI on 5173. The pre-commit hook runs the installed
  binary: after an adapter or schema change, `cargo install --path crates/cli`.
- Team setup: the rites' `setup` page carries the rule; this is how it is
  run here. `.grimoire.toml` names the server on this machine, which the
  loop starts from a copy of the installed build, signing you in the first
  time with a token of your own, named `dev-…`, and revoking none. The daemon on 4242 reads the server's
  intent with this checkout's code and sends every write there; the tools,
  the HTTP API and the review are reached on 4242 as before. A commit lands
  nothing: after it, run `grimoire land <ticket>`, on master with the commit
  made, since it also prunes the server's baseline from this checkout's
  code; `grimoire check --main-line` prunes it the same way with no ticket
  to land. A change to the server's
  side takes effect after `cargo install --path crates/cli`: the loop
  watches the installed build and restarts the server alone on it, the
  daemon and the UI running on. With the server away, reads and `grimoire check` go on from the
  copy last fetched (`check --ticket` fails instead) and every write is
  refused; to go back to a store of
  its own, `.grimoire.toml` names `store = "grimoire"` again, and the loop
  then starts no server.
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
- Review before close: the rites' `review` page carries the rule; this is
  how it is run here. Run Codex read-only,
  `codex exec -s read-only -C <repo> -o <out.md> --json - < <prompt>`, with
  a prompt naming the ticket, the goal page and the nodes it touched and
  relied on, telling it to read the grimoire only through the read-only
  tools over HTTP (never the store's files, never a write tool) and to
  ignore the untracked `.codex/`. Check each finding against the code
  before acting. Say what the review found and what was done in the
  ticket's closing decision, and in the commit message when there is one.
- Questions start from the goal: the rites' `questions` page carries the
  rule; this is how it is run here. Before laying out options on a gap or
  question, read the goal page and say what it needs whatever the tool, and
  name the assumption the present design rests on. Tooling already in use
  (git, a code host, a library) is machinery to weigh, never a requirement.
  Then lay out the options: weigh what can be reused before anything is
  built; hold one that needs adapters, plugins or a second product against
  the goal's "what it is not" before recommending it; and weigh one that
  bears on how people use the tool toward the fewest steps a person has to
  do, the fewest ways it can fail and the least work it asks ahead of time,
  for a team, some setup being accepted. Say what the question came down to
  in its closing decision.
