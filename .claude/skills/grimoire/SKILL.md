---
name: grimoire
description: This skill should be used when working on a project that Grimoire manages (it has a `.grimoire.toml`), whenever the user says "start ticket", "finish ticket", "close the ticket", "write the spec", "file a ticket", "what does this rest on", or asks what the project is meant to do. It wraps the Grimoire tools into the three sequences that keep the grimoire honest: start a ticket, write its spec, finish it.
argument-hint: start|spec|finish|file <ticket-slug or title>
---

# Grimoire

Grimoire holds a project's **grimoire** (pages of intent, requirements with
scenarios and dated decisions, the module contracts in the code, the code)
and its **tickets** (changes, gaps, questions) as one graph. The grimoire and
the code record what the project does; a ticket never does. Landing a ticket
updates the grimoire in the same piece of work. This skill is a thin client of
the tools.

## Reaching the tools

In order of preference:

1. **MCP tools** named `mcp__grimoire__<tool>` when the daemon is running and
   the project's `.mcp.json` names it. Use them as tools.
2. **HTTP**: `POST http://127.0.0.1:4242/api/<tool>` with the JSON input as
   the body (port from `.mcp.json` if it differs). `GET /api/tools` lists them.
3. **CLI, no daemon**: `grimoire tool <name> '<json>'` in the repository.
   Reads and writes the store directly; use it when nothing is running.

Every tool takes and returns JSON. Errors are `{ "error": code, "message" }`
with codes `not_found`, `invalid` (with `details`), `conflict`,
`store_unavailable`. A write never fails on a lint; it returns the findings
it created in `warnings`. Every warning is work to do now.

## Conventions

- **References** are `kind:id`: `page:<slug>`, `req:<slug>`,
  `ticket:<slug>`, `module:<module id>`, `criterion:<module id>#<criterion id>`,
  `file:<path>`, `decl:<path>#<decl path>`. **`find` before every reference
  you write**, in `touches`, `relies_on`, `links` and `covered_by`. A wrong
  guess comes back as a dangling warning; a guess that names the wrong node
  does not.
- **Slugs** are `^[a-z0-9]+(-[a-z0-9]+)*$`, at most 64 characters.
- A **requirement**: a normative `statement` (SHALL / MUST), at least one
  scenario (given, when, then; without one it is `unspecified`),
  `covered_by` criteria, dated append-only **decisions** of one paragraph
  each: what was chosen and why.
- A **ticket**: `touches` (what it changes), `relies_on` (what it assumes
  and leaves alone), a Markdown body under 4,000 characters that is
  discarded on close. Kinds `change`, `gap`, `question`; priorities `next`,
  `high`, `later`.
- `put_page` and `put_requirement` **replace** the record. To add or remove
  a page's links, `patch_page`; it touches nothing else. For a requirement,
  `get` first and send the whole thing back; existing decisions are kept.
- Module contracts are a JSON block under `# Module contract` in a file's
  leading doc comment. Renaming or removing a criterion leaves the
  requirements naming it dangling; the lint says so. Format per language:
  `references/contracts.md`. Which code change needs which update to the grimoire:
  `references/changes.md`; read it before a ticket that touches a module.
- **If the daemon is rebuilt while you work**, the tools drop out during
  each rebuild. Write the grimoire before editing code; retry a refused call
  after a moment, never edit the store instead. A new tool or changed
  instructions reach your MCP client only when it reconnects; call over
  HTTP until then, and say so.
- **A `check` that fails for no sensible reason**: compare `status`'s
  `version` and `supported_languages` with the store's `languages`. A stale
  installed binary is the usual cause.

## Orienting

When the task is unclear or new to the session: `status`, then `outline`
(depth 2), then `board`. The root page links the story in reading order:
`goal`, `architecture`, `constraints`, `glossary`, then the areas under
architecture. Read those pages before reading code.

**Before opening a source file, `get` its module.** The module node gives
its purpose, its criteria, its files and what it imports; `get` on a file
gives every declaration with its line; `find` with `kinds: ["decl"]` finds
a function or type by name across the repository; `module_source` gives a
criterion's line. Use these to decide which file and which declaration to
read, and read the file only then. Grep answers "where is this string";
the graph answers "which module owns this behaviour and what does it
promise", which is the question a change actually asks. The graph does not
answer "who calls this"; that is grep or the language server, by design.
`find` locates a node by any text. `get` shows a node with its `in` and `out`
edges.

## Sequence: start a ticket

`/grimoire start <slug>`

1. `start_ticket { "ref": "ticket:<slug>" }`. It moves the ticket to
   `active` and returns the ticket, `rests_on` (everything below it, nearest
   first, with the decisions behind each requirement) and `related` (open gaps
   and questions touching what it touches). If the ticket is already active,
   use `get`, `rests_on` and `related_tickets` instead.
2. Read `rests_on` as a list of assumptions. For each requirement with
   decisions, and each closed ticket listed, ask whether the decision still
   holds given what this ticket changes. Name the ones you would challenge,
   with why, before planning. This is the moment the question is asked;
   nothing pushes it later. The walk is bounded: a ticket relied on appears
   with its decision and is not expanded, and a page appears with its
   requirements and modules but not its child pages. Module imports are
   followed all the way down, since that is the blast radius.
3. Read `related`. A gap or question that touches the same nodes is context
   for this work and may be closed by it; say which.
4. For each module in `touches`, `rests_on_me` shows the blast radius: the
   requirements and pages above it and the other open tickets reaching it.
   Then `get` the module for its files and declarations before opening any
   of them.
5. Plan the work. Include the grimoire updates in the plan: which pages and
   requirements change, which decisions get recorded, which contracts change.

## Sequence: write the spec

`/grimoire spec <slug>`

The spec is not a document; it is the grimoire after the change, written before
the code.

1. For each node in the ticket's `touches`: `get` it. For a page,
   `put_page` with the new intent and links. For a requirement,
   `put_requirement` with the new statement, scenarios and `covered_by`. For a
   new requirement, also `patch_page` the page that covers its area with
   `add_links`, or it is `uncovered`.
2. For every choice made while writing, `record_decision` on the requirement
   it belongs to: what was chosen and why, one paragraph. Date defaults to
   today.
3. A criterion the code will need to satisfy goes into the module contract in
   the source, and the requirement's `covered_by` names it. Until the code
   lands the reference dangles; that is allowed and expected. Say so. If
   the behaviour lands in a module the ticket does not list, `update_ticket`
   to add it to `touches` first; `touches` is a claim about what changed and
   has to stay true.
4. `lint`. Fix every finding this work created. Findings that predate it are
   not this ticket's, unless it touches them.
5. `update_ticket` if `touches` or `relies_on` changed while specifying.

## Sequence: finish a ticket

`/grimoire finish <slug>`

1. `get ticket:<slug>`. Copy from its body anything the grimoire still needs;
   the body is discarded on close.
2. `lint`. Resolve what this work created: a criterion added to a contract
   needs a requirement naming it; a criterion renamed or removed leaves a
   dangling `covered_by`; a new page needs a page linking to it.
3. Walk `touches`. For each reference, confirm it was updated in this piece
   of work: `get` the node and compare with the diff. A page whose intent
   changed but whose body did not is the failure this step exists to catch.
   Fix it, or remove the reference from `touches` with `update_ticket` and
   say why it did not need changing.
4. `close_ticket { "ref", "decision": { "text" } }`. The decision is one
   paragraph: what was chosen and why, written so someone reading it in a
   year without the ticket body understands. It returns `findings` limited to
   the nodes the ticket touched; if any are present, fix them and re-run
   `lint`. Closing is allowed with findings; leaving them is not.
5. If the project documents a review step (in its `CLAUDE.md`, a skill, or
   a hook), run it now. Grimoire has no opinion on what the review is.
   If the project has a visual check for a user interface, run it and look
   at the output; a type check proves nothing about how a view renders.
6. Commit the code and the grimoire changes together only when the user has
   asked for a commit. The store is outside the repository; the commit is the
   code and its contracts.

## Sequence: file a ticket

`/grimoire file <title>`

1. `find` the nodes it touches and relies on; resolve every reference.
2. `file_ticket` with a title, kind, priority, `touches`, `relies_on`, and
   a body of a few paragraphs: the problem, the direction, the decision if
   one was already taken (it is recorded on the requirement later, when the
   work lands). The id is generated; do not send a slug. Refer to the
   ticket afterwards by the `ref` the response returns, with its title
   beside it in prose.
3. Any `warnings` in the response are dangling references. Fix them with
   `update_ticket` now.

## What not to do

- Do not write to the store's files directly. The tools are the only writer.
- Do not put in a ticket what belongs in the grimoire. A ticket's body dies on
  close; a page, requirement or decision does not.
- Do not infer call edges or dependencies the graph does not show. Blast
  radius is at module level, from imports; say so when it matters.
- Do not reopen a closed ticket. File a new one that `relies_on` it.
