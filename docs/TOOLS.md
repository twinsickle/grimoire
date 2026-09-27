# MCP tools

Status: proposed, 2026-09-27. The only interface to the store. The JSON API
the web UI uses exposes the same operations with the same names and shapes,
so this document defines both. Companion to [STORE.md](./STORE.md).

## Conventions

- Served by `grimoire serve` over MCP streamable HTTP at `/mcp`, and as
  JSON over HTTP at `/api/<tool>` for the UI.
- Every input and output is a JSON object. References use the `kind:id`
  form from the store schema. Dates are ISO `YYYY-MM-DD`.
- Every tool that returns a node returns it in the same shape: the stored or
  derived fields, plus `ref`, plus `in` and `out`, the derived edge lists.
  `out` is what the node links to, `in` is what links to it, each an array
  of `{ "ref", "relation" }`. Relations are `links`, `covered_by`,
  `touches`, `relies_on`, `contains`, `imports`, `belongs_to`.
- Errors are `{ "error": code, "message": text, "details"?: any }`. Codes:
  `not_found`, `invalid` (a field rule was violated; `details` lists the
  fields), `conflict` (a slug already exists, or a status transition is not
  allowed), `store_unavailable`.
- Write tools validate the whole node against the schema before writing.
  A write that would create a dangling edge is allowed and returns the
  resulting lint findings in `warnings`, because the reference may be about
  to be created by the same piece of work. A write never fails on a lint.
- Every write is atomic and the response is the node as it now exists.
- Tools are idempotent where that is meaningful: putting an identical page
  twice is one page.

## Read tools

### `find`

Search nodes by text, optionally by kind.

Input: `{ "query": string, "kinds"?: [kind], "limit"?: int (default 20) }`.
Matches slug, title, statement, purpose, body and declaration path, case
insensitive. Output: `{ "nodes": [node summary] }`, each summary being
`ref`, `title` or `statement` or `purpose`, and for tickets `kind`,
`priority`, `status`. Ordered by match quality.

### `get`

One node in full with its edges.

Input: `{ "ref": reference }`. Output: `{ "node": node }`. `not_found` if
the reference does not resolve.

### `outline`

The manual as a tree from the roots, for the map and for an agent orienting
itself.

Input: `{ "depth"?: int (default 2), "root"?: page reference }`. Output:
`{ "roots": [tree] }`, where a tree is `{ "ref", "title", "coverage",
"findings", "children": [tree] }`. `coverage` is, for a page, the fraction
of criteria under it that some requirement names; for a requirement, the
count of criteria in `covered_by`. `findings` is the count of lint findings
on or below the node. Children follow the page's `links` order; modules
appear as leaves with their criteria.

### `rests_on`

What a node depends on, transitively downward, with the decisions behind
each. The planning and review question.

Input: `{ "ref": reference, "depth"?: int (default unlimited) }`.
Output: `{ "nodes": [ { "ref", "title", "via": [reference path from the
input], "decisions": [decision] } ] }`. For a ticket, starts from
`relies_on` and `touches`. For a page, follows `links`. For a requirement,
follows `covered_by` to criteria and then to modules and their imports.
Requirements contribute their decisions; other nodes contribute none.
Ordered nearest first, then by decision date descending.

### `rests_on_me`

The reverse: everything that would be affected if this node changed.

Input: `{ "ref": reference }`. Output: `{ "nodes": [ { "ref", "title",
"relation" } ], "tickets": [ ticket summary ] }`. For a module, includes
modules that import it and requirements covered by its criteria and the
pages above those. `tickets` lists open and active tickets whose `touches`
or `relies_on` reach the node.

### `coverage`

What covers a criterion or a module.

Input: `{ "ref": criterion or module reference }`. Output: for a criterion,
`{ "requirements": [ requirement summary ] }`; for a module, `{ "criteria":
[ { "ref", "statement", "requirements": [ requirement summary ] } ] }`,
with an empty list where a criterion is uncovered.

### `board`

Tickets for the first screen.

Input: `{ "status"?: [status] (default open and active), "kinds"?: [kind]
(default change), "priority"?: [priority], "touching"?: reference }`.
Output: `{ "tickets": [ ticket summary ] }` ordered `next`, `high`, `later`,
then `active` before `open`, then title. `touching` restricts to tickets
whose `touches` or `relies_on` includes the reference, which is how a
ticket page finds the gaps and questions that touch what it touches.

### `related_tickets`

The gaps and questions to look at alongside a ticket.

Input: `{ "ref": ticket reference }`. Output: `{ "tickets": [ ticket
summary ] }`: every open or active ticket of kind `gap` or `question` whose
`touches` or `relies_on` shares a reference with this ticket's `touches`,
excluding itself.

### `lint`

The two lints and the malformed check over the whole graph.

Input: `{ "kinds"?: ["dangling", "uncovered", "malformed"] }`. Output:
`{ "findings": [ { "rule", "ref", "edge"?: { "from", "to", "relation" },
"reason" } ], "counts": { rule: int } }`. Findings are ordered by rule, then
reference, so two runs diff cleanly. This is what `grimoire check` prints.

### `module_source`

Where a criterion or declaration lives, for click-through.

Input: `{ "ref": module, criterion, file or decl reference }`. Output:
`{ "file": path, "line": int, "contract_line"?: int }`. The contract line
is the line of the `# Module contract` heading, so a criterion opens on its
contract.

## Write tools

### `put_page`

Create or replace a page.

Input: the page object from the schema. Output: `{ "node": page,
"warnings": [finding] }`. `conflict` is not raised on replace; the slug is
the identity.

### `put_requirement`

Create or replace a requirement. Decisions are not replaced by this tool:
`decisions` is ignored on input and preserved from the existing node, so
that decisions are append-only. Use `record_decision` to add one.

Input: the requirement object without `decisions`. Output: `{ "node":
requirement, "warnings": [finding] }`.

### `record_decision`

Append a dated decision to a requirement.

Input: `{ "ref": requirement reference, "text": string, "date"?: date
(default today) }`. Output: `{ "node": requirement }`. `not_found` if the
requirement does not exist.

### `file_ticket`

Create a ticket.

Input: `{ "slug", "title", "kind", "priority", "touches"?, "relies_on"?,
"body" }`. Status is `open` and `decision` is null. Output: `{ "node":
ticket, "warnings": [finding] }`. `conflict` if the slug exists.

### `update_ticket`

Change an open or active ticket's fields other than status and decision.

Input: `{ "ref": ticket reference, "title"?, "priority"?, "touches"?,
"relies_on"?, "body"? }`. Only given fields change. Output: `{ "node":
ticket, "warnings": [finding] }`. `conflict` if the ticket is closed.

### `start_ticket`

Move a ticket from `open` to `active`.

Input: `{ "ref": ticket reference }`. Output: `{ "node": ticket,
"rests_on": [as from rests_on], "related": [ticket summary] }`. Returns in
one call what the start-ticket skill loads. `conflict` if not `open`.

### `close_ticket`

Close a ticket with its decision. The body is discarded.

Input: `{ "ref": ticket reference, "decision": { "text", "date"? } }`.
Output: `{ "node": ticket, "findings": [finding] }`. `findings` is the
result of `lint` restricted to the nodes the ticket touched, so the
finish-ticket skill sees in the same call whether the manual was left
consistent. `conflict` if not `open` or `active`. Closing is allowed with
findings present; refusing to close is the skill's decision, not the
tool's.

### `delete_node`

Remove a page, requirement or ticket.

Input: `{ "ref": reference }`. Output: `{ "deleted": reference,
"warnings": [finding] }`, the warnings being the dangling edges the deletion
created. Only store kinds can be deleted; derived nodes return `invalid`.

## Not tools

- **Reading the repository.** Extraction is the daemon's job on every change
  and needs no tool. `get` on a module or file returns the current parse.
- **Committing.** Grimoire knows nothing of version control.
- **Renaming a slug.** A rename is a new node and a delete, with the dangling
  lint catching what the rename missed. The UI may offer it as one action
  that calls both tools and rewrites links; the tools stay primitive.
- **Reopening a closed ticket.** A new ticket relies on the closed one.
- **Running the project's review.** The finish-ticket skill runs whatever
  the project configured; Grimoire has no opinion on it.

## Skill sequences over these tools

For reference, since the skills are thin clients. Start a ticket:
`start_ticket`, then read `rests_on` and `related` from its output. Finish
a ticket: `lint`, confirm every reference in `touches` was updated in this
piece of work (the skill's judgement, from the diff and the nodes),
`close_ticket` with the decision, then the project's review if configured.
Write a spec: `put_page` and `put_requirement` for the nodes the ticket
touches, `record_decision` for each choice made, and `lint`.
