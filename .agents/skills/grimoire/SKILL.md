---
name: grimoire
description: This skill should be used when working on a project that Grimoire manages (it has a `.grimoire.toml`), whenever the user says "brew ticket", "start ticket", "finish ticket", "close the ticket", "bottle the ticket", "write the spec", "inscribe a ticket", "file a ticket", "resolve", "answer the question", "what does this rest on", "adopt", "write contracts for the existing code", or asks what the project is meant to do. It wraps the Grimoire tools into the sequences that keep the grimoire honest: brew a ticket, write its spec, finish it, inscribe one, resolve a gap or question with the person, and adopt code that predates its contracts or tests.
argument-hint: brew|spec|finish|inscribe|resolve|adopt <ticket-slug, title or area>
---

# Grimoire

Grimoire holds a project's **grimoire** (pages of intent, requirements with
scenarios and dated decisions, the module contracts in the code, the code)
and its **tickets** (changes, gaps, questions) as one graph. The grimoire and
the code record what the project does; a ticket never does. Bottling a ticket
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

**On a team**, the project's `.grimoire.toml` names a `server` and the
grimoire is kept there, shared by everyone. You reach the tools the same
way: the daemon in this checkout reads the server's intent with this
checkout's code and sends every write to the server. Signing in is the
person's, once for each grimoire, with `grimoire login`. A write needs the
server and is refused as `store_unavailable` without it; reads and
`grimoire check` go on from the copy last fetched, and `status` says so
under `server`. A check that names a ticket, as a pipeline's does, fails instead: it is judged only
against the server's intent as it is now.

**Name your checkout.** Pass `checkout`, your working directory as an
absolute path, with every call, by MCP or HTTP. One daemon may serve
several checkouts of the same grimoire, such as worktrees agents work in
side by side, and answers from the code and current ticket of the one you
name. With several served, a call that names none is refused with the list
of checkouts; with one, nothing changes. A project with a store of its own
serves one checkout: working in several at once makes it a team's.

Every tool takes and returns JSON. Errors are `{ "error": code, "message" }`
with codes `not_found`, `invalid` (with `details`), `conflict`,
`store_unavailable`. A write never fails on a lint; it returns the findings
it created in `warnings`. Every warning is work to do now.

## Conventions

- **References** are `kind:id`: `page:<slug>`, `req:<slug>`,
  `ticket:<slug>`, `module:<module id>`, `criterion:<module id>#<criterion id>`,
  `file:<path>`, `decl:<path>#<decl path>`, `recipe:<id>`,
  `recipe:<id>#<ingredient>`. **`find` before every reference
  you write**, in `touches`, `relies_on`, `links` and `covered_by`. A wrong
  guess comes back as a dangling warning; a guess that names the wrong node
  does not. A reference names a node of this project's own grimoire, never
  another project's: what another system promises is written here as a
  requirement of this project's own, held by its own test against that
  system, and prose may link to the other project's site.
- **Slugs** are `^[a-z0-9]+(-[a-z0-9]+)*$`, at most 64 characters.
- **Two roots.** Every grimoire has two, named in its config: the
  **grimoire**, what the project is and must do, its requirements carried
  by the code, each held by the criteria it names in `covered_by` or a
  test that checks it; and the **rites**, how its people work on it, its
  requirements held by the work, which each ticket's closing decision
  shows. What a rule is about decides its root, not how it is held: a
  rule of work goes under the rites and names no criterion, even when a
  tool checks it, conventions included; a rule the code carries goes
  under the grimoire. A page that only informs goes with its subject.
  `status` names both.
- A **requirement**: a normative `statement` (SHALL / MUST), at least one
  scenario (given, when, then; without one it is `unspecified`),
  `covered_by` criteria, dated append-only **decisions** of one paragraph
  each: what was chosen and why. A decision that overturns earlier ones
  names them in `record_decision`'s `supersedes`, by id: `d1`, `d2` and
  so on, each given as the decision is bottled and never given twice, and
  referenced as `req:<slug>#d<n>`; they stay, no longer in force, and
  `rests_on` shows only what is. A decision recorded under a ticket has
  no id until the ticket is bottled, so the ticket cannot supersede it. A requirement that no longer holds is retired with `retire_node`,
  never deleted: `delete_node` refuses one with decisions, and a bottled
  ticket.
- **A write that changes intent goes into a ticket's proposal**, not in the
  truth: `put_page`, `patch_page`, `put_requirement`, `record_decision`,
  `supersede`, `retire_node` and `delete_node` of a page or requirement take
  `ticket` (a slug or `ticket:` reference); absent, they go into the store's
  current ticket, the one last brewed (on a team, last brewed in this
  checkout); with neither they are refused.
  The truth changes when the ticket is bottled. A patch, retirement or
  deletion builds on the proposal's copy of the record; when the truth
  moved under it, the write is refused as a `conflict`: `get` the record
  and put it whole again under the ticket. A decision or supersession
  builds on no copy: it waits in the proposal and is added after whatever
  decisions the truth holds then, so putting a requirement again never
  loses one.
  When the truth moves under a record your ticket already proposes, your
  checkout's check says so at once, as `proposal-moved` on the ticket
  naming each record: `get` it and propose it again, or ask the person
  whether to withdraw the ticket.
  Every read takes `ticket` too and then answers as that proposal would
  leave the graph; `lint` with none uses the current ticket, so a branch is
  judged against its own intent. The baseline, the config, the board's
  order, `rename_node` and every ticket write apply at once and take no
  `ticket`, but for one part of `update_ticket`, below.
- **A ticket's references are proposal state.** Its own `touches` and
  `relies_on` are kept on it and readable everywhere, but the check judges
  them only where the ticket is current, and on the main line when it
  is bottled; `inscribe_ticket` and an edit to a ticket's own references apply at
  once. An edit `update_ticket` makes to *another* ticket's `touches` or
  `relies_on` under a change being worked (named in `ticket`, else the
  current one) waits in that change's proposal, shown on the other ticket
  as a draft, and reaches it when the change is bottled; under a gap, a question
  or no ticket it applies at once. Title, priority and body always apply at
  once.
- A **ticket**: `touches` (what it changes), `relies_on` (what it assumes
  and leaves alone), a Markdown body under 4,000 characters that is
  discarded on close. Kinds `change`, `gap`, `question`; priorities `next`,
  `high`, `later`; statuses `inscribed` (filed, nobody on it), `brewing`
  (being worked), `simmering` (its decision written, waiting for its change
  to reach the main line) and `bottled` (done, its decision kept).
- **A branch holds one ticket** until that ticket is bottled, where the project
  is worked in git: bottled with `grimoire bottle`, which on a team is once
  the branch has merged to the main line.
  The check judges a checkout's code against its current ticket's proposal
  alone, so a second ticket brewed beside one still simmering splits
  the branch's intent, and its check fails on the first one's code. Brew
  the next ticket on a new branch, or wait for the bottling. Work too big
  for one checkout is split into branches of the ticket's branch: each
  helper takes the same ticket up there with `brew_ticket`, shares its one
  proposal and merges back into the ticket's branch, which alone is bottled.
  Work found along the way is inscribed as a ticket of its own. Nothing refuses
  a second ticket: `brew_ticket` answers in `waiting` each ticket this
  checkout still carries, and the check names such a ticket beside a
  finding it would mend, the finding still failing.
- `put_page` and `put_requirement` **replace** the record. To add or remove
  a page's links, `patch_page`; it touches nothing else. To change part of
  a page's body or a ticket's, `patch_page` or `update_ticket` with `append`
  (a paragraph at the end) or `replace` (`{old, new}`, `old` matching the
  body exactly once), not the whole body sent back. For a requirement,
  `get` first and send the whole thing back; existing decisions are kept.
- Module contracts are a JSON block under `# Module contract` in a file's
  leading doc comment. Renaming or removing a criterion leaves the
  requirements naming it dangling; the lint says so. Format per language:
  `references/contracts.md`. Which code change needs which update to the grimoire:
  `references/changes.md`; read it before a ticket that touches a module.
- **If the daemon is rebuilt while you work**, the tools drop out during
  each rebuild. Write the grimoire before editing code; retry a refused call
  after a moment, never edit the store instead. Changed instructions reach
  your MCP client when it reconnects, but a new tool or a changed field may
  reach it only in a new session, since a client may keep the tools it was
  first given. Until then call over HTTP, and say so.
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
edges, the bottled tickets that touched it counted rather than listed;
`get` with `refs` reads several in one call.

**Before editing a module, ask what binds it.** `rests_on_me` on the module
(or a file, or a criterion) answers under `binds` the requirements on its
own criteria, each with its statement and scenarios: what the edit must
keep. This is where drift happens, often at a file the plan never named, so
ask it whenever you open one to change it. The rest of what stands above it
comes by name under `nodes`; `get` the ones near your change. A
requirement's decisions are why it says what it says, not what it says:
read them with `get` when you mean to change that requirement.

## Sequence: brew a ticket

1. `brew_ticket { "ref": "ticket:<slug>" }`. It moves the ticket to
   `brewing` and returns the ticket, `rests_on` (a map of what binds the
   work, nearest first: what it names directly with what each says, every
   requirement below it by its title and how many scenarios and decisions it
   has, and every bottled ticket it relies on with its decision), `reaches`
   (the rest of what is below it, the modules its imports lead to and the
   like, as a count and references; `rests_on` the tool gives the whole walk
   with each path), `constraints` (what holds whichever part changes, each
   with what it says), `related` (the gaps
   and questions not yet bottled touching what it touches) and `recipes` (the recipes over
   what it touches, its own modules' and those of the modules importing them,
   each saying whether it is `due`: what the project
   promises the people who build with it, which this work may break). If
   it answers `waiting`, this checkout still carries a ticket that has not
   been bottled: say so, and bottle that one first or move this work to a new
   branch (see "A branch holds one ticket" above). If the ticket is already
   brewing,
   use `get`, `rests_on` and `related_tickets` instead; on a team,
   `brew_ticket` on a ticket already brewing takes it up in this checkout:
   it becomes the current ticket here and nothing of it changes. The answer's
   `checked_out` is who has the ticket now and since when, you among them;
   if `others` names someone, they have it too: say so before you both work
   it. Nothing refuses the brew.
2. Read `rests_on` as the map of what binds the work. Each node the ticket
   names itself carries `says`, its text (a page's body, a requirement's
   statement and scenarios, a module's contract, an answer waiting for this
   change): that is what the ticket rests on, so read it there rather than
   from memory. Read each requirement further down by its title, and the
   `constraints` in full, and ask whether this ticket's change contradicts
   what one says; `get` a group of them with `refs` to read their scenarios
   where the change comes near. For a requirement you mean to change, and
   each bottled ticket listed, `get` the decisions behind it and ask whether
   they still hold given what this ticket changes. Name the ones you would
   challenge, with why, before planning. This is the moment the question is
   asked; nothing pushes it later. The walk is bounded: a ticket relied on appears
   with its decision and is not expanded, and a page appears with its
   requirements and modules but not its child pages. Module imports are
   followed all the way down, since that is the blast radius; what they
   reach comes under `reaches` by reference, to `get` where it matters.
3. Read `related`. A gap or question that touches the same nodes is context
   for this work and may be bottled by it; say which.
4. For each module in `touches`, `rests_on_me` shows what binds it and the
   blast radius: the requirements on its own criteria in full, then the
   requirements, recipes and pages above it by name, and the other tickets
   not yet bottled reaching it. Ask it again of any other module before you
   edit it. Then `get` the module for its files and declarations before
   opening any of them.
5. Plan the work. Include the grimoire updates in the plan: which pages and
   requirements change, which decisions get recorded, which contracts change.

## Sequence: write the spec

The spec is not a document; it is the grimoire after the change, written before
the code, as the ticket's proposal: every write below names the ticket, or
goes into it as the current one, and the truth is untouched until the close.
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
   is written the reference dangles; that is allowed and expected. Say so. If
   the behaviour goes into a module the ticket does not list, `update_ticket`
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
4. Judge the recipes. Each recipe the brew named as `due`, and any the
   `lint` reports `due-recipe`, is judged now against what it represents: read the recipe
   and its `files` as they are after this work. If it still says truly how
   to use what it represents, `judge_recipe { "ref": "recipe:<id>", "outcome":
   "held" }`, or several at once with `judge_recipe { "judgements": [{ "ref",
   "outcome" }, ...] }`; if not, update the recipe so it does, then `judge_recipe` with
   `"outcome": "updated"`. Judge after the last change to the recipe and its
   files: a judgement holds only of them as they were when it was made, so a
   later edit makes the recipe due again. The judgement is yours or the
   person's, never Grimoire's; the decision below names the recipes judged
   and what was updated.
5. If the project documents a review step (in its rites, or a skill or
   a hook), run it now, before closing: a finding
   that holds is fixed now or filed as its own ticket, and the decision
   below says what the review found. Grimoire has no opinion on what the
   review is.
   If the project has a visual check for a user interface, run it and look
   at the output; a type check proves nothing about how a view renders.
6. `close_ticket { "ref", "decision": { "text" } }`. The decision is one
   paragraph: what was chosen and why, written so someone reading it in a
   year without the ticket body understands. The close applies the ticket's
   proposal to the truth. It is refused as a `conflict` when the truth moved
   under the proposal since a record was written, naming each moved record
   in `details`: `get` each one as the truth now has it, write it again
   under the ticket, and close again. It returns `findings`, the new
   findings on the nodes the ticket touched; if any are present, fix them and
   re-run `lint`. Closing is allowed with findings; leaving them is not.
   `due` lists the recipes the work reached that no judgement holds of as
   they are now: judge each (step 4), since each is a finding.
   `baselined` counts the inherited ones there; they are not this ticket's.
   `unnamed` lists what this ticket's own work changed that `touches`
   never named, its proposal's pages and requirements and the modules its
   code changed since its branch's base (or, with none, since the brew),
   never what other tickets brought, each with what it says and the
   requirements resting on it:
   read them as you read `rests_on` at the brew, since the work reached
   them; one that belonged to the ticket goes into `touches` next time, and
   one whose intent the work broke is a gap to inscribe.
   `untouched` lists the nodes in `touches` that never changed since the
   ticket was brewed. For each, the change was missed (make it, in a new
   ticket, since this one is bottled) or the node did not need changing
   (say so in the decision, and next time remove it from `touches` before
   closing). `null` means it cannot be said: the ticket was brewed before
   snapshots existed, or the daemon that answered holds no code.

   When the change has code that has still to reach the main line, add
   `"when": "bottled"`.
   The decision is written and the ticket becomes `simmering`: nothing reaches
   the truth yet, the ticket keeps its proposal and stays the current one,
   and the check still judges the code against that proposal. Its answer
   carries the findings as the proposal leaves them and no `untouched`. The
   bottling then bottles it with that decision: `grimoire bottle`. The
   post-commit hook runs it after every commit where `init` installed one
   (it leaves a hook of the project's own alone and installs none outside
   git, and says so), and a pipeline runs it as `grimoire bottle <ticket>`
   when the change reaches its main line. Where neither does, run it
   yourself once the change is on the main line. On a team a commit on a machine
   bottles nothing, and `grimoire bottle` with no ticket says so: the ticket
   is bottled by name on the main line, run by the pipeline (which checks the
   change first with `grimoire check --ticket <ticket>`, and prunes the
   team's baseline on the main line with `grimoire check --main-line`;
   never run either of the latter from a branch) or by a person once
   the change has merged. So the truth moves when the code
   reaches the main line, and a commit the check refuses bottles nothing. If the truth moved meanwhile, the bottling
   is refused and the ticket stays `simmering`, reported as `simmering-moved`:
   write the moved records again under the ticket and run
   `grimoire bottle <ticket>`. A change with nothing to bottle, and a gap or
   question whose answer needs no work, close at once, without `when`. A
   gap or question whose answer needs work takes `when: bottled` and is bottled
   when the changes relying on it are (see the resolve sequence).

   A change reconsidered and no longer to be made, before or after its
   decision was written, a bottling the truth moved under among them, is
   not bottled but withdrawn: `withdraw_ticket { "ref", "decision" }`, the
   decision saying why. Nothing of its proposal is applied; it is bottled
   marked withdrawn, and an answer it was to apply reopens unless another
   change applied it. Withdrawing is the person's call, laid out like any
   question; never withdraw a change to make a check pass.
7. Commit the code only when the user has asked for a commit. The store is
   outside the repository; the commit is the code and its contracts. For a
   `simmering` ticket the commit bottles it where the post-commit hook is
   installed and the store is the project's own (on a team the main line
   bottles it), and the hook prints what was bottled and any ticket still
   simmering; if nothing says it was bottled, run `grimoire bottle`.

## Sequence: inscribe a ticket

1. `find` the nodes it touches and relies on; resolve every reference.
2. `inscribe_ticket` with a title, kind, priority, `touches`, `relies_on`, and
   a body of a few paragraphs: the problem, the direction, the decision if
   one was already taken (it is recorded on the requirement later, when the
   work is bottled). The id is generated; do not send a slug. Refer to the
   ticket afterwards by the `ref` the response returns, with its title
   beside it in prose.
3. Any `warnings` in the response are new dangling references. Fix them
   with `update_ticket` now.

## Sequence: resolve a gap or question

A gap or question is closed on the person's decision, never on yours.

1. `brew_ticket`; read what it rests on and the related tickets, as for
   any ticket. On a gap or question it claims it: brewing, with you listed as
   having it checked out, but never the current ticket, so the change your
   branch works on stays current. If `others` names someone, they are
   answering it too: say so. Closing one needs no brew.
2. Lay out two to four resolutions for the person: what each costs, what
   each rules out, and which you would pick and why. Write nothing yet.
3. The person chooses, amends or adds one. Their choice, in their terms, is
   the decision; your reasoning is its why.
4. If the answer needs no work, `close_ticket` with that decision, opening
   "Decided by the user". If it implies work, `close_ticket` the gap or
   question with the decision and `change`, the change that applies it:
   `title`, `touches`, `priority`, a `body` carrying what the person said
   (scope, what to avoid), and any further `relies_on`. One call inscribes that
   change relying on the gap or question, writes the decision ending
   "Applied by <id>." and answers the change in `filed`; refused, it
   changes nothing. Fix any `warnings` in `filed` with `update_ticket`. A
   change already inscribed, relying on it, needs no `change`: close with
   `when: bottled`. Either way the answer waits, `simmering`, and is bottled when
   every change relying on it has been; the change counts it as answered
   and is ready.
   Nothing closes a waiting answer by hand: if its last change is deleted or
   stops relying on it, it reopens with the answer in its body, to be
   examined again. A question is never edited into a change: the record
   that it was asked, and how it was answered, is what the change rests on.
5. With no person to ask (an autonomous run), do not resolve. Add the
   options to the ticket's body with `update_ticket`, as `append`, and leave
   it unanswered.

A change that turns out to be a question is closed with a decision saying
so and the question inscribed, or deleted if nothing was decided.

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
   requirement, `record_decision` with `adopted: true`: read off the code at
   adoption and approved by the user, stating what the code did then rather
   than what it was meant to do. The flag marks the requirement as
   described, not decided, until someone records a decision of their own.
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
- Do not reopen a bottled ticket. Inscribe a new one that `relies_on` it.
- Do not decide a gap or question yourself. Lay out the options; the person
  chooses, and their choice is the decision.
- Do not call `adopt` to make a finding go away. It turns every present
  finding into inherited work; it is the person's call, made with a reason.
