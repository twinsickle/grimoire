---
name: grimoire
description: This skill should be used when working on a project that Grimoire manages (it has a `.grimoire.toml`), whenever the user says "start ticket", "finish ticket", "close the ticket", "write the spec", "file a ticket", "resolve", "answer the question", "what does this rest on", "adopt", "write contracts for the existing code", or asks what the project is meant to do. It wraps the Grimoire tools into the sequences that keep the grimoire honest: start a ticket, write its spec, finish it, file one, resolve a gap or question with the person, and adopt code that predates its contracts or tests.
argument-hint: start|spec|finish|file|resolve|adopt <ticket-slug, title or area>
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

1. **MCP tools** from the `grimoire` server when the daemon is running and
   the project's MCP configuration names it (`.mcp.json`, or
   `.codex/config.toml` for Codex). Use them as tools.
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
  each: what was chosen and why. A decision that overturns earlier ones
  names them in `record_decision`'s `supersedes`, by position (1 for the
  oldest); they stay, no longer in force, and `rests_on` shows only what
  is. A requirement that no longer holds is retired with `retire_node`,
  never deleted: `delete_node` refuses one with decisions, and a closed
  ticket.
- **A write that changes intent lands in a ticket's proposal**, not in the
  truth: `put_page`, `patch_page`, `put_requirement`, `record_decision`,
  `supersede`, `retire_node` and `delete_node` of a page or requirement take
  `ticket` (a slug or `ticket:` reference); absent, they land in the store's
  current ticket, the one last started; with neither they are refused.
  The truth changes when the ticket closes. A patch, decision,
  supersession, retirement or deletion builds on the proposal's copy of the
  record; when the truth moved under it, the write is refused as a
  `conflict`: `get` the record and put it whole again under the ticket.
  Every read takes `ticket` too and then answers as that proposal would
  leave the graph; `lint` with none uses the current ticket, so a branch is
  judged against its own intent. Tickets, the baseline, the config, the
  board's order and `rename_node` apply at once and take no `ticket`.
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
(depth 2), then `board`. Read the pages each root links, in their order, before
reading code.

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

The spec is not a document; it is the grimoire after the change, written before
the code, as the ticket's proposal: every write below names the ticket, or
lands in it as the current one, and the truth is untouched until the close.
Read the spec back with `ticket` on the reads, or they show the truth.

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
   not this ticket's, unless it touches them; those the project accepted when
   it adopted Grimoire carry `baselined: true`, and the check fails only on
   the rest (`new`).
5. `update_ticket` if `touches` or `relies_on` changed while specifying.

## Sequence: finish a ticket

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
4. If the project documents a review step (in its `CLAUDE.md` or
   `AGENTS.md`, a skill, or a hook), run it now, before closing: a finding
   that holds is fixed now or filed as its own ticket, and the decision
   below says what the review found. Grimoire has no opinion on what the
   review is.
   If the project has a visual check for a user interface, run it and look
   at the output; a type check proves nothing about how a view renders.
5. `close_ticket { "ref", "decision": { "text" } }`. The decision is one
   paragraph: what was chosen and why, written so someone reading it in a
   year without the ticket body understands. The close applies the ticket's
   proposal to the truth. It is refused as a `conflict` when the truth moved
   under the proposal since a record was written, naming each moved record
   in `details`: `get` each one as the truth now has it, write it again
   under the ticket, and close again. It returns `findings`, the new
   findings on the nodes the ticket touched; if any are present, fix them and
   re-run `lint`. Closing is allowed with findings; leaving them is not.
   `baselined` counts the inherited ones there; they are not this ticket's.
   `untouched` lists the nodes in `touches` that never changed since the
   ticket started. For each, the change was missed (make it, in a new
   ticket, since this one is closed) or the node did not need changing
   (say so in the decision, and next time remove it from `touches` before
   closing). `null` means the ticket was started before snapshots existed.
6. Commit the code and the grimoire changes together only when the user has
   asked for a commit. The store is outside the repository; the commit is the
   code and its contracts.

## Sequence: file a ticket

1. `find` the nodes it touches and relies on; resolve every reference.
2. `file_ticket` with a title, kind, priority, `touches`, `relies_on`, and
   a body of a few paragraphs: the problem, the direction, the decision if
   one was already taken (it is recorded on the requirement later, when the
   work lands). The id is generated; do not send a slug. Refer to the
   ticket afterwards by the `ref` the response returns, with its title
   beside it in prose.
3. Any `warnings` in the response are new dangling references. Fix them
   with `update_ticket` now.

## Sequence: resolve a gap or question

A gap or question closes on the person's decision, never on yours.

1. `start_ticket`; read what it rests on and the related tickets, as for
   any ticket.
2. Lay out two to four resolutions for the person: what each costs, what
   each rules out, and which you would pick and why. Write nothing yet.
3. The person chooses, amends or adds one. Their choice, in their terms, is
   the decision; your reasoning is its why.
4. `close_ticket` with that decision, opening "Decided by the user". If the
   answer implies work, `file_ticket` the change with `relies_on` naming the
   gap or question, and put what the person said (scope, what to avoid,
   priority) in its body. A question is never edited into a change: the
   record that it was asked, and how it was answered, is what the change
   rests on.
5. With no person to ask (an autonomous run), do not resolve. Add the
   options to the ticket's body with `update_ticket` and leave it open.

A change that turns out to be a question is closed with a decision saying
so and the question filed, or deleted if nothing was decided.

## Sequence: adopt existing code

For code that predates its contracts or its tests. `init` recorded what it
found as the baseline, so the check passes while this runs; each batch
shrinks the baseline and leaves the check green. Take every batch from
`lint`, never from a list you write: a module or criterion left off a
hand-made list is simply never reached. Note the baseline's count in
`status` before a batch and read it after; a count that did not fall by
what the batch covered is something missed.

### Contracts

1. `lint {"kinds":["uncovered"]}` and take the findings whose `check` is
   `no-contract`; `status` shows the contract scope. Group the modules into
   areas, a directory the person agrees is one area. Propose the areas and
   an order; the person picks the next.
2. Within the area, leaves first: `get` each module for its `imports`, and
   take first those no other uncontracted module in the area imports, so a
   module's dependencies are contracted before it. About ten per batch.
3. For each module, `get` it for its files and declarations and read the
   code. Draft a contract in the format `references/contracts.md` gives for
   its language: a one-sentence purpose, and criteria that each state what
   holds, as something a test could check. For each criterion draft a
   requirement: a SHALL statement, at least one scenario (given, when,
   then) read off the code, `covered_by` the criterion. Describe what the
   code does now, not what it should do.
4. Show the batch: each module's contract and requirements together. Say
   plainly that they were read off the code and describe it as it is,
   faults included. Write only what the person approves, amended as they
   say; a rejected module stays in the baseline.
5. For each approved module: write the contract into the source file as an
   ordinary edit. `find` the area's page; if there is none, propose one and
   `put_page` it once approved, linked from the page above it with
   `patch_page`. `put_requirement` each requirement, then `patch_page` the
   area page with `add_links` for the module and its requirements. On each
   requirement, `record_decision`: read off the code at adoption and
   approved by the user, stating what the code did then rather than what it
   was meant to do.
6. `lint`. The batch's `no-contract` and `unlinked-module` findings are gone
   and pruned from the baseline, and nothing new appears. Anything new is
   this batch's to fix before the next.
7. Commit only when the user asks. Then the next batch, or the next area.

Code that will never carry a contract (generated, vendored) is a scope
question: propose `configure` with an `exclude` to the person. Never run
`scaffold` on an existing store; it replaces the whole store.

### Tests

For an area whose contracts are in, its criteria that no test checks.

1. `lint {"kinds":["uncovered"]}` and take the findings whose `check` is
   `untested`, in this area. `coverage` on each module lists its criteria
   with the tests that already check them.
2. For each criterion, find the test that checks it and read its assertions
   against the criterion's statement. A test's name is not evidence; a
   test block is a claim. If a test holds the criterion, draft a
   `# Test contract` block for it (`references/contracts.md`). If none
   does, draft a test.
3. A criterion about how the parts are arranged, not what the module does
   (a layer does no I/O, one component is the only writer), is a
   constraint: propose moving it to a requirement on the Constraints page
   with no criterion, held by a test that names `req:<slug>`, often one
   that reads the source. A behavioural criterion no test can check:
   propose rewording it to what a test can. Never leave one untested
   because it is hard.
4. Show the batch, write what the person approves, then `lint`. The batch's
   `untested` findings are gone and pruned from the baseline, and nothing
   new appears; a constraint no test holds shows as `unheld`.

## What not to do

- Do not write to the store's files directly. The tools are the only writer.
- Do not put in a ticket what belongs in the grimoire. A ticket's body dies on
  close; a page, requirement or decision does not.
- Do not infer call edges or dependencies the graph does not show. Blast
  radius is at module level, from imports; say so when it matters.
- Do not reopen a closed ticket. File a new one that `relies_on` it.
- Do not decide a gap or question yourself. Lay out the options; the person
  chooses, and their choice is the decision.
- Do not call `adopt` to make a finding go away. It turns every present
  finding into inherited work; it is the person's call, made with a reason.
