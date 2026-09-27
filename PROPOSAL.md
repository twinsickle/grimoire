# Proposal: Grimoire, a manual and ticketing tool for software projects

Status: proposed. Revised 2026-09-27 after review. The first project it is
pointed at is described in [examples/rusty-harness.md](./examples/rusty-harness.md).

## Why

Two problems recur in a project developed largely by agents. Nothing says what
the project is meant to do now except the code and a pile of per-change specs,
some of them dead, with no path from the whole to a piece. And direction
changes often, so earlier assumptions sit unchallenged under new work, while
the future, the gaps and the open questions live in one planning document too
long to read.

## What it is

**Grimoire** ("it's almost magic"): one tool, in its own repository, with two
faces over one model:

- **The manual** is the truth of what a project is meant to do now.
- **A ticket** is an intended change to that truth, or a gap or question
  about it.

When a ticket's work lands, the same piece of work updates the manual. A
ticket is never the record of what the code does; the manual and the code are.

The tool is pointed at a repository. It is strongly visual for a person, and
reachable by an agent through an MCP server and a skill. Nothing in it is
specific to one project, one language or one agent: the same server serves
Claude Code, Codex, Copilot or a person in a browser.

## The model is a graph

Everything the tool does is a query over one graph.

**Nodes come from three places, and the graph is one graph.**

- **The code** supplies files and what each declares: functions, types,
  methods and imports, each with its line. Grimoire parses the repository
  itself with tree-sitter, in process, one file at a time, and writes nothing
  back. Nothing of the code is stored; it is re-read from the repository.
- **The contracts** supply modules and their criteria, read from the
  `# Module contract` blocks in the same parse and attached to the module the
  file belongs to.
- **The store** supplies everything a person or agent wrote: pages of intent,
  requirements with their scenarios and decisions, and tickets.

**Edges are the only structure.** A page links to the pages, requirements or
modules it describes. A requirement links to the criteria that satisfy it. A
criterion belongs to a module, and a module contains the functions and types
its files declare. Modules depend on each other through their imports, which
the language adapter resolves to module identities. A ticket links to the
nodes it touches and the nodes it relies on, at any level. Every edge is written on the node that owns it,
pointing downward, and the tool derives the reverse. So the chain runs
unbroken from a page of intent to a function's line in its file, and back up
from a changed module to every requirement and ticket that rests on it.

**The four jobs are graph queries.**

- Present the current state: the store's pages and requirements joined to the
  code's modules and criteria.
- Present the future: the tickets.
- Map one to the other and call out what is missing: the lints.
- Let agents pull and update: read and write tools over nodes and edges, so
  one server answers "what depends on this module" and "what requirement does
  this serve" alike.

## The manual

By convention the manual reads as four levels, each drillable into the next.

| Level | What it holds | Where it comes from |
|---|---|---|
| Pages | The intention: what the project is, its parts, what each is for. What a person reads for quick understanding. | The store. |
| Requirements | The explicit requirements each page covers, each with scenarios, in the OpenSpec shape because it reads and diffs well. | The store. |
| Modules | The module contracts already in the source: a purpose and criteria per module. | The repository, parsed from the contracts and never duplicated. |
| Code | The functions and types inside each module, and the modules it imports. | The repository, parsed in process. |

The levels are a convention for how pages are written, not a rule the tool
enforces. A page that lists modules is what the manual calls an area; the tool
sees only a page with links.

A requirement carries the decisions behind it: a dated line saying what was
chosen and why, kept on the requirement rather than in a log elsewhere. This
is what gets challenged, and it is the only dated thing in the model. The
tool keeps no history of its own; the decisions are the history that matters.

## Lints

The lints keep the graph honest. They run as a check, not a prompt, and are the
same check whichever direction the graph is being rebuilt from.

- **Dangling**: an edge to a node that does not exist. A requirement naming a
  criterion that was removed, a ticket naming a page that was renamed.
- **Uncovered**: a node no edge reaches. A criterion no requirement names, a
  module no page names, a page nothing links to, a module in the code graph
  that carries no contract.

## Two directions

The store and the repository restore each other, to an extent, with the
emphasis on existing state rather than future plans.

- **Repository to store.** From the module contracts alone the tool builds
  the existing-state skeleton: a page per module group, a requirement per
  criterion, each already linked. The intent and the wording a person gave the
  requirements are not recoverable this way, so the skeleton is something to
  refine, not the manual.
- **Store to repository.** The manual's pages and requirements are enough for
  an agent, reading them over the tools, to build a new project that
  satisfies them and to write its module contracts, which the lints then
  check against the manual. Tickets are not needed for this.

## Tickets

A ticket has bounded fields: a title, a kind (change, gap, question), a
priority band (next, high, later), the nodes it touches, the nodes it relies
on, a body of a few paragraphs at most, and a status (open, active, closed).
Nothing else is required.

What a person sees first is the board: the next tickets, then the high-priority
ones. Gaps and questions are off the first screen. They appear on a ticket's
own page whenever they touch a node that ticket touches, so they are looked at
whenever related work is looked at, without being read otherwise.

A closed ticket keeps only what the manual did not absorb: the decision and
its date. Its plan and working notes go.

## Challenging assumptions

A ticket declares the nodes it relies on. From that the tool answers one
question for whoever is planning or reviewing the work: what does this rest
on, and when and why was each of those decided. Because the graph continues
down into the code, the same question has a code answer: which modules a
ticket's requirements are satisfied by, and which other modules depend on
them, so the reviewer sees the blast radius of a change beside the decisions
it rests on. Blast radius is answered at module level, from imports. Calls
between functions need a resolver that understands the language, and are
left to one. The planner asks it when planning, the reviewer when
challenging the plan, and the person sees it on the ticket. The question is asked at the moment of dependence, not at a fixed
review point, and nothing is pushed into any context unasked.

## How agents reach it

The tools are the only interface to the store. Nothing reads or writes it
except the server, and the server is reached through MCP, the web UI's API, or
the CLI.

- **MCP server** over streamable HTTP, served by the daemon. Read tools: find
  a page, requirement, criterion, module or function; what rests on a node;
  what covers a criterion; a node's neighbours and the path between two
  nodes; the board; a ticket. Write tools: file a ticket, change its status
  or links, record a decision on a requirement, add or modify a page or
  requirement.
- **A skill per agent** wrapping the common sequences: start a ticket, which
  loads what it rests on; finish a ticket, which runs the lints, checks the
  manual was updated, and runs whatever review the project has configured
  before the ticket closes. The MCP server is what every agent shares; the
  skill is a thin client of it, written first for Claude Code and again for
  any agent that has skills.

Whether a store is ever versioned or shared between machines, so that a
clone of a project comes with its manual, is an open question for every
project and is not decided here.

## What a project gets

Grimoire owns its store. A project it is pointed at contains nothing of the
store and refers to nothing in it. The project gets only the integration:

- **Module contracts** in its source, which are the one part of the graph that
  lives in code. A contract is a JSON block with a purpose and criteria under a
  `# Module contract` heading in a file's leading comment.
- **Skills, hooks and agent definitions** that call the tools: start and
  finish a ticket, write a spec, run the check.
- **A small config** saying how to reach the daemon.

Installing these is what `grimoire init` does: `.grimoire.toml`, the skill,
the MCP entry merged into `.mcp.json`, and a `pre-commit` hook that runs
`grimoire check` when the repository has no hook of its own. A git hook rather
than an agent's own hook, because the check is the same for every agent and
for a person, and because the lints are a check, not a prompt. The code is
only ever read; Grimoire writes no index, cache or marker into the repository.

## Visual

- The manual as a drillable map: pages as regions, the pages they link to
  within them, requirements within those, modules within those, with
  coverage shown as fill and the lints as marks on the map. Click through to
  the criterion in its source file, and below it to the module's functions
  and types at their lines.
- The board for tickets, with a graph view from any ticket to the nodes it
  touches and from any node to the tickets that touch it.
- Dark only.

## Architecture

Rust for everything that runs, and TypeScript only for the web UI, which
Node builds and nothing runs. The result is one binary per platform that
carries the built UI inside it, needs no runtime installed, and runs on
Windows, Linux and macOS from the start.

Rust was chosen over TypeScript once the tool came to parse code itself,
since tree-sitter and its grammars compile in natively, and once `grimoire
check` came to run in hooks and CI, where a static binary needs nothing
present. The cost is generated TypeScript types for the UI rather than
shared ones, and the official Rust MCP SDK being younger than the TypeScript
reference, over a surface of a dozen tools.

**One daemon, one core, thin clients.** A single process, `grimoire serve`,
reads the store and parses the target repository, joins them into one graph,
watches the repository and re-parses a file when it changes, and serves three
interfaces from one port: the web UI, a JSON API for it, and MCP over
streamable HTTP. All writes to the
store go through it. A `grimoire check` command runs the lints without the
daemon, for hooks and CI.

**The graph is memory; the store is a persistence detail.** A store for a
real project is a few hundred nodes and the code a few thousand. The full
parse of a few hundred files is a one-time cost of a few seconds on start;
after that a change re-parses one file and re-joins the graph in memory, so
there is no index to rebuild and nothing on disk to keep current. The store's
format is chosen for the daemon's convenience, since nothing else reads it,
and can change without anyone outside Grimoire noticing. It is not a Git
repository and does not live in the target repository.

**One Cargo workspace and one web package.**

- `crates/core`: the domain model, the store, the graph and its queries, the
  lints, and the scaffold that builds a skeleton from contracts. Pure
  functions over an in-memory model, with no I/O. Tests run against a fixture
  store and fixture extraction.
- `crates/extract`: the code extractor and its language adapters, with
  tree-sitter and the grammars compiled in. Reads the paths it is given and
  nothing else.
- `crates/server`: the daemon. Watching, the JSON API, the MCP tools that
  call core, live updates to the UI over server-sent events, and the built
  web UI embedded as static files.
- `crates/cli`: the `grimoire` binary: `serve`, `check`, `scaffold`, `init`.
- `web`: the UI, React and Vite, built by Node at build time into the
  server's embedded files. Three views: the map, the board, and a node page
  showing what rests on it and what it rests on. Dark only. Its API types are
  generated from the Rust model.

Every crate carries module contracts in its own source, so Grimoire can be
pointed at itself.

**Code extraction is generic; language adapters are small.** The extractor
parses a file with tree-sitter and knows one contract grammar: the JSON
block under the `# Module contract` heading. A language adapter
supplies four things: a tree-sitter grammar and the queries that name a
file's functions, types, methods and imports; how to strip its comment
markers; how to map a source path and an import path to a canonical module
identity; and which files are expected to carry a contract at all. Rust is
the first adapter; a TypeScript project needs an adapter of the same four
parts and nothing else.

**What the extractor does and does not claim.** It gives a complete
inventory of what each file declares, with lines, and module dependency from
imports. It does not resolve calls between functions: a parser cannot tell
which `run` a method call reaches, and guessing by name produces wrong edges
that look like right ones. Call edges are a later layer, supplied by a
resolver that understands the language, such as rust-analyzer over LSP for
Rust, started by the daemon, writing nothing to the repository, and joined to
the graph through a seam the extractor already leaves for it.

**Identity and links.** Every store node has a slug, and links are slugs. A
criterion is addressed as module id plus criterion id. Nothing links upward;
the graph derives it. A rename is a ticket's work, and the dangling lint
catches what a rename missed.

**Order of building.** Core with the store, the extractor with the Rust
adapter, and the lints first, proven against a real repository. Then `check`
and `scaffold`, so the round trip exists before any UI. Then the daemon with MCP, so agents can use
it. The web UI last, since everything it shows is already answerable over the
API by then.

The one part expected to need revision is the map view: a nested layout
works for a few hundred nodes and will need rethinking if Grimoire is pointed
at something much larger.

## Development loop

Grimoire is developed pointed at itself. Its own repository is the first
target, its own manual the first store, and the daemon under development is
the one its developers and their agents use. This is possible because the
store lives outside the repository and the repository is only read, so
pointing the tool at its own checkout creates no loop. It is worth doing
because every rough edge is met by the people building the tool, on the tool,
before any other project meets it.

The loop has four layers.

- **Content is live.** The daemon already watches the target repository and
  re-parses a changed file into the graph. Editing Grimoire's own source
  updates Grimoire's own manual within a second, with nothing restarted.
- **The daemon restarts on change.** Rust is rebuilt, not reloaded. A file
  watcher (`cargo watch` or `bacon`) rebuilds and restarts `grimoire serve` on
  save. The graph is memory rebuilt from the store and a few-second parse, and
  store writes are atomic, so a restart costs seconds and loses nothing.
- **The UI hot-reloads.** Vite's dev server, with hot module replacement,
  proxies `/api` and `/mcp` to the running daemon. The embedded UI is only
  for release builds.
- **Agents reach the dev daemon directly.** The repository carries a
  `.grimoire.toml` naming Grimoire's own store and a `.mcp.json` pointing at
  the dev daemon on a fixed local port, so an agent working in this
  repository has the tools as tools and can debug the server it is building
  by calling it. Until MCP exists, the same operations are reachable as JSON
  at `/api/<tool>`, and the store's files can be read directly.

What this requires of the tool, beyond what it already needs:

- `serve` and `check` accept `--store <path>` so a dev store can sit anywhere
  readable, not only in the platform data directory.
- The repository watcher ignores whatever `.gitignore` ignores, `target/`
  above all, since a rebuild churns thousands of files there.
- A `dev` task in the repository runs the daemon watcher and the Vite dev
  server together on the fixed port.
- The self-store is seeded with `grimoire scaffold` the moment `scaffold`
  exists, so the daemon is trialled against a real store from its first run.

## First milestone

The tool, pointed at a real repository, renders the manual's four levels
from a populated store and the parsed repository with the two lints
passing, rebuilds the existing-state skeleton from the module contracts
alone, shows the board with real tickets, answers "what does this ticket rest
on" over MCP down to the modules involved and what depends on them, and the
next piece of that project's work is planned from it. The first such project is in
[examples/rusty-harness.md](./examples/rusty-harness.md).
