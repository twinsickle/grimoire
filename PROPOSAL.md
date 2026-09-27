# Proposal: Grimoire, a manual and ticketing tool for developing rusty-harness

Status: proposed. The artifact of the Phase 61 discussion on 2026-09-27.

## Why

Two problems in how rusty-harness is developed. Nothing says what the harness
is meant to do now except the code, forty per-phase specs of which some are
dead, and no path from the whole to a piece. And the direction has changed
often, so earlier assumptions sit unchallenged under new work. `PLAN.md`
holds the future, the gaps and the open questions in one document too long to
read.

## What it is

**Grimoire** ("it's almost magic"): one new project, in its own repository,
with two faces over one model:

- **The manual** is the truth of what a project is meant to do now.
- **A ticket** is an intended change to that truth, or a gap or question
  about it.

That is the lifecycle's own model, spec truth plus changes against it, applied
to the harness's development from outside. When a ticket's work lands, the
same change updates the manual. A ticket is never the record of what the code
does; the manual and the code are.

The tool is pointed at a repository. It is strongly visual for a person, and
reachable by an agent through an MCP server and a skill. Files pointing at
files is the experience it exists to replace. Nothing in it is specific to the
harness or to one agent: the same store serves Claude Code, Codex, Copilot or
a person with an editor.

## The manual

Three levels, each drillable into the next.

| Level | What it holds | Where it lives |
|---|---|---|
| Sections | The intention: what the project is, its parts, what each is for. A page a person reads for quick understanding. | The tool's store. |
| Use cases | The explicit requirements each section covers, each with scenarios, written as OpenSpec writes them because that shape reads and diffs well. | The tool's store. |
| Code | The module contracts already in the source: a purpose and criteria per module. | The repository. Parsed from the `# Module contract` blocks, never duplicated. |

The repository keeps only what the code currently satisfies, which is the
module level. Everything above it is the tool's, held in a store outside the
repository. The store is itself a Git repository of plain files, so the
history of how the manual and the roadmap changed is kept and an agent
without the tool can still read everything.

Links run downward by identity: a section names its areas, a requirement names
the module criteria that cover it. The tool derives the upward view. Two lints
keep the chain honest and run as a check, not a prompt:

- **Dangling**: a requirement naming a criterion that no longer exists.
- **Uncovered**: a criterion no requirement names, and an area no section names.

A requirement carries the decisions behind it: a dated line saying what was
chosen and why, kept on the requirement rather than in a log elsewhere. This
is what gets challenged.

## Two directions

The store and the repository restore each other, to an extent, with the
emphasis on existing state rather than future plans.

- **Repository to store.** From the module contracts alone the tool rebuilds
  the existing-state layer: an area per module group, a requirement per
  criterion, each already linked. Sections and the wording a person gave the
  requirements are not recoverable this way, so the rebuild is a skeleton to
  refine, not the manual itself.
- **Store to repository.** The manual's sections and requirements are enough
  for an agent to build a new project that satisfies them and to write its
  module contracts, which the lints then check against the manual. Tickets
  and future intent are not needed for this and are not promised to survive.

Both directions are the same check the lints run every day, so neither is a
disaster path exercised only after a loss.

## Tickets

A ticket has bounded fields: a title, a kind (phase, gap, question), a
priority band (next, high, later), the manual nodes it touches or relies on,
a body of a few paragraphs at most, and its status. Nothing else is required.

What a person sees first is the board: the next tickets, then the high-priority
ones. Gaps and questions are off the first screen. They appear on a ticket's
own page whenever they touch a node that ticket touches, which is the check
the user wanted: looked at whenever a phase is looked at, without being read
otherwise.

A closed ticket keeps only what the manual did not absorb: the decision and
its date. Its plan and working notes go, as the lifecycle discards a change's.

## Challenging assumptions

A ticket declares the manual nodes it touches or relies on. From that the
tool answers one question for whoever is planning or reviewing the work: what
does this rest on, and when and why was each of those decided. The planner
asks it when planning, the reviewer when challenging the plan, and the person
sees it on the ticket. The question is asked at the moment of dependence, not
at a fixed review point, and nothing is pushed into any context unasked.

## How agents reach it

- **MCP server**, local, over stdio. Read tools: find a section, requirement
  or criterion; what rests on a node; what covers a criterion; the board;
  a ticket. Write tools: file a ticket, change its status or links, record a
  decision on a requirement, add or modify a requirement. The manual is edited
  through the server or directly as files in the store; either way the change
  that alters the code alters the manual in the same piece of work.
- **A skill per agent** wrapping the common sequences: start a ticket, which
  loads what it rests on; finish a ticket, which runs the lints, checks the
  manual was updated, and calls the Codex review before the ticket closes.
  The MCP server is what every agent shares; the skill is a thin client of
  it, written first for Claude Code and again for any agent that has skills.

## Visual

- The manual as a drillable map: sections as regions, areas within them,
  requirements within those, with coverage shown as fill and the lints as
  marks on the map. Click through to the criterion in its source file.
- The board for tickets, with a graph view from any ticket to the nodes it
  touches and from any node to the tickets that touch it.
- Dark only, like the harness.

## Architecture

TypeScript throughout. The harness's rule against a Node runtime does not
apply: this is the separate project that rule pointed Node tooling to. Runs
on Windows, Linux and macOS from the start.

**One daemon, one core, thin clients.** A single long-running process,
`grimoire serve`, owns everything stateful: it reads the store and the target
repository, holds the index, watches both for changes, and serves three
interfaces from one port: the web UI, a JSON API for it, and MCP over
streamable HTTP. Claude Code and Codex both speak HTTP MCP, so no stdio shim
is needed at first; if one becomes useful it is a proxy that starts the
daemon. One process doing all writes means no two processes race on the same
files. A `grimoire check` command runs the lints without the daemon, for hooks
and CI.

**Files are the database; the index is memory.** The store is a Git
repository of Markdown with frontmatter: one file per section, one spec file
per area holding its requirements, scenarios and dated decisions, one file per
ticket. The whole store for rusty-harness is a few hundred nodes, so the index
is an in-memory graph rebuilt from the files in milliseconds on start and on
every watched change. There is no SQLite: no native build per platform,
nothing to migrate, and "rebuild from files" is the only path rather than a
recovery path. Every write goes to a file and the watcher reindexes, so the
UI, the API and MCP never disagree.

**Packages in one repository.**

- `core`: the domain model, the store parser and writer, the contract
  extractor, the graph and its queries, the lints, and the scaffold that
  builds a skeleton from contracts. Pure functions over an in-memory model,
  with no I/O beyond reading the paths it is given. All the tests live here,
  run against a fixture store and against the real rusty-harness crates.
- `server`: the daemon. Watching, the API, the MCP tool definitions that call
  core, and live updates to the UI over server-sent events so a change on
  disk shows without a refresh.
- `web`: React and Vite. Three views: the map, a nested layout of sections,
  areas and requirements with coverage as fill and lint marks; the board; and
  a node page showing what rests on it and what it rests on. Dark only.
- `cli`: `serve`, `check`, `scaffold`, `init`.

**Contract extraction stays generic.** A contract is a JSON block under a
`# Module contract` heading in a file's leading comment. The extractor knows
one grammar and, per language, only how to strip comment markers, so a
TypeScript project needs a comment-syntax entry and nothing else. Module
identity is the file's path relative to its package root, as the harness
already has it.

**Identity and links.** Every node has a slug that is its filename, and links
are slugs: a section lists its areas, a requirement lists the module criteria
that cover it as module id plus criterion id. Nothing links upward in the
files; the index derives it. A rename is a ticket's work, and the dangling
lint catches what a rename missed.

**Order of building.** Core with the parser, extractor and lints first,
proven against the rusty-harness repository's contracts. Then `check` and `scaffold`, so
the round trip exists before any UI. Then the daemon with MCP, so agents can
use it. The web UI last, since everything it shows is already answerable over
the API by then.

The one part expected to need revision is the map view: a nested layout
works for a few hundred nodes and will need rethinking if Grimoire is pointed
at something much larger.

## Migration

- `PLAN.md` becomes tickets in the store: each phase a ticket, §6.1 gaps as
  gap tickets, §6.2 questions as question tickets, each linked to the nodes it
  touches. `PLAN.md` and `PLAN-completed.md` are then deleted; Git keeps them.
- `SYSTEM.md` and `LIFECYCLE.md` become the manual's top level, in the store.
  They already record intent, which is what a section is.
- `specs/` is mined for requirements that are still true, which go into the
  store by area, then deleted. Superseded specs are not carried.
- `trials/` and `design/` are decided ticket by ticket; the default is delete.
- Module contracts stay as they are. After migration the repository holds
  code, module contracts and a short pointer to the tool in place of this
  proposal.

## Also adopted in rusty-harness, outside this project

- The planner, plan reviewer and reviewer prompts reused as Claude Code agent
  definitions under `.claude/agents/`.
- A Codex review skill, triggered when a ticket's requirements are written
  and when its implementation is done, before the ticket closes.
- IntelliJ's index, already reachable through MCP, in place of text search.

## Timing

Nothing in the rusty-harness repository is migrated or deleted before Grimoire exists and
can hold it. `SYSTEM.md`, `LIFECYCLE.md`, `PLAN.md` and `specs/` stay as they
are until then, and the migration above is the first thing Grimoire is used
for once it reaches its first milestone.

## First milestone

The tool, pointed at the rusty-harness repository, renders the manual's three
levels from the migrated store with the two lints passing, rebuilds the
existing-state skeleton from the module contracts alone, shows the board with
the migrated tickets, answers "what does this ticket rest on" over MCP, and a
ticket for the next phase of rusty-harness is planned from it.
