# Example: rusty-harness

The first project Grimoire is pointed at, and the one whose problems produced
the proposal. Everything here is specific to that repository; nothing in it is
part of Grimoire.

## The problems it had

Forty per-phase specs under `specs/`, some dead, with nothing but the code
saying what the harness is meant to do now and no path from the whole to a
piece. A 2,000-line `PLAN.md` holding the future, the gaps (§6.1) and the open
questions (§6.2) in one document too long to read. Direction changed often, so
earlier assumptions sat unchallenged under new work.

## What it already has

- **Module contracts** in every implementation module, per the Phase 21 spec
  (`specs/phase-21-module-acceptance-criteria.md`): a `# Module contract`
  heading in the leading `//!` block with a strict JSON object of `purpose`
  and `criteria`. Around a hundred modules across three crates. This is the
  code level of the manual, already present.
- **Canonical module identity** rules, defined in that spec's M4: the
  workspace-relative path with `.rs` or a terminal `/mod.rs` removed; a
  `tests.rs` or `*_tests.rs` maps to its parent; crate roots map to their
  `src` directory. The Rust language adapter implements these.
- **Coverage rules**, defined in M2: every module source under a workspace
  member's `src/` carries a contract; test bodies and `tests/` directories do
  not. The Rust adapter implements these too, so the uncovered lint can report
  a module with no contract.
- `SYSTEM.md` and `LIFECYCLE.md`, which record intent and become the top of
  the manual.
- An open question in `PLAN.md` §6.2 about giving agents a code graph, naming
  Graphify. Grimoire answers it for the harness's own development with its
  own in-process extraction; see the trial below for why not Graphify.
  Whether the shipped harness gives its own agents a code graph stays the
  harness's question.

## Migration

Nothing in the repository is migrated or deleted before Grimoire exists and
can hold it. The migration is the first thing Grimoire is used for once it
reaches its first milestone.

- `PLAN.md` becomes tickets in the store: each phase a change ticket, §6.1
  gaps as gap tickets, §6.2 questions as question tickets, each linked to the
  nodes it touches. `PLAN.md` and `PLAN-completed.md` are then deleted; Git
  keeps them.
- `SYSTEM.md` and `LIFECYCLE.md` become the manual's top pages, in the store.
- `specs/` is mined for requirements that are still true, which go into the
  store by area, then deleted. Superseded specs are not carried.
- `trials/` and `design/` are decided ticket by ticket; the default is delete.
- Module contracts stay as they are. After migration the repository holds
  code, module contracts, the Grimoire integration, and a short pointer to the
  tool.

The authoring is the larger half of the work: distilling forty specs and the
plan into requirements, decisions and tickets is something an agent can draft
and a person has to judge.

## Integration installed into the repository

What `grimoire init` puts there, plus what the harness adopts alongside:

- The Claude Code skill and hooks that call the Grimoire tools.
- The harness's planner, plan reviewer and reviewer prompts reused as Claude
  Code agent definitions under `.claude/agents/`.
- A Codex review skill, run by the finish-ticket sequence when a ticket's
  requirements are written and when its implementation is done, before the
  ticket closes. This is the project-configured review the generic skill
  calls; it is not part of Grimoire.
- IntelliJ's index, already reachable through MCP, in place of text search.

## First milestone, for this project

Grimoire pointed at the rusty-harness repository renders the manual's four
levels from the migrated store and the parsed crates with the two lints
passing, rebuilds the
existing-state skeleton from the module contracts alone, shows the board with
the migrated tickets, answers "what does this ticket rest on" over MCP, and
the next phase of rusty-harness is planned from it.

## Graphify trial, 2026-09-27, and why it was not adopted

[Graphify](https://github.com/Graphify-Labs/graphify) 0.9.70 was run
code-only with clustering off over the checkout, on Windows with Python 3.14,
to see whether it could supply the code level of the graph.

- Full build: 603 files, 5.0 s. Incremental after touching one file: 6.7 s,
  since the whole graph is rewritten regardless. No model calls.
- 539 Rust files, 539 file nodes, 7,970 Rust nodes in all. Ids carry the full
  path, so the 124 `mod.rs` and 115 `tests.rs` files do not collide.
- `git/ready.rs` as the spot check: its eight qualified calls into `layout`
  and its method calls on `Git` produced no edges; four calls to bare-name
  imported free functions did. `use super::layout` produced an edge to a
  sourceless stub, not to `layout.rs`. A `String` reference resolved to an
  unrelated type of that name in `tokens/mod.rs`.
- `specs/`, `design/` and `trials/` were ingested as 1,559 document nodes and
  348 concept nodes.
- It wrote `graphify-out/` with a graph, a manifest, a parse cache and a root
  marker into the checkout, and its design keeps them current through Git
  hooks. Its `update` command has no option to put them elsewhere.

Why not adopted: it leaves state in the target repository, which the proposal
rules out; the part that worked, the file inventory, is a small amount of
tree-sitter code Grimoire can run in process; the part that did not, call
resolution, needs a language-aware resolver that Graphify is not; and an
incremental update rewrote the whole graph. The output was deleted and the
package uninstalled after the trial. The findings shaped the extractor's
scope in the proposal: inventory and imports, no guessed calls.

## Notes for Grimoire's own development

- The harness's rule against a Node runtime does not apply to Grimoire: this
  is the separate project that rule pointed Node tooling to.
- Tests in `core` run against fixtures. An opt-in test, gated on an
  environment variable naming a rusty-harness checkout, runs the Rust adapter
  and extractor over the real crates.
