# Store schema

Status: proposed, 2026-09-27. The schema core is built from. Companion to
[TOOLS.md](./TOOLS.md), which is the only interface to it.

## Scope

The store holds what people and agents wrote: pages, requirements and
tickets. Modules, criteria, files and declarations come from the target
repository on every read and are never stored. Both halves are described
here because edges cross between them and the lints check both.

The store is Grimoire's own. Nothing else reads or writes it, so its layout
is chosen for the daemon and may change. Only the node schema and the
reference format are commitments, because they surface through the tools.

## References

Every node is addressed by a reference, a string of the form `kind:id`.
References are the only form a link takes, in the store and in the tools.

| Kind | Id | Example |
|---|---|---|
| `page` | slug | `page:git-readiness` |
| `req` | slug | `req:ready-only-with-commit` |
| `ticket` | slug | `ticket:phase-62` |
| `module` | canonical module id from the language adapter | `module:crates/core/src/git/ready` |
| `criterion` | module id, `#`, criterion id | `criterion:crates/core/src/git/ready#guarantee` |
| `file` | repository-relative path, `/`-separated | `file:crates/core/src/git/ready.rs` |
| `decl` | file path, `#`, declaration path | `decl:crates/core/src/git/ready.rs#Repository::from_top` |

A slug matches `^[a-z0-9]+(-[a-z0-9]+)*$`, at most 64 characters, unique
within its kind. Module ids are whatever the adapter returns and are opaque
to the store. Paths use `/` on every platform.

Links are written on the node that owns them and point downward only. The
graph derives every upward relation. What each kind may link to:

| From | May link to |
|---|---|
| page | page, req, module |
| req | criterion |
| ticket | anything, including other tickets |

## Store nodes

All three are JSON objects. Unknown fields are an error, not ignored, so a
typo in a field name is caught. Every string is UTF-8 and trimmed; empty
required strings are errors. Markdown is allowed in `body` fields and nowhere
else.

### Page

A unit of intent: what something is and what it is for. A page that links
mostly to modules is what the manual calls an area, by convention only.

```json
{
  "slug": "git-readiness",
  "title": "Git readiness",
  "body": "A workspace is usable only when its repository is in a known-good state. ...",
  "links": ["req:ready-only-with-commit", "req:readiness-check-reads-only", "module:crates/core/src/git/ready"]
}
```

| Field | Type | Rule |
|---|---|---|
| `slug` | string | required, slug |
| `title` | string | required, one line |
| `body` | string | required, Markdown, no limit but the UI shows the first paragraph in the map |
| `links` | array of reference | required, may be empty, ordered for display, no duplicates, only `page`, `req`, `module` |

### Requirement

One explicit requirement in the OpenSpec shape: a statement, scenarios, and
the criteria in the code that satisfy it. Carries its own decisions.

```json
{
  "slug": "ready-only-with-commit",
  "title": "A workspace is ready only with a commit",
  "statement": "The system SHALL treat a workspace as Git ready only when its repository has at least one commit, a supported Git, and an identity.",
  "scenarios": [
    {
      "title": "A fresh repository",
      "given": "a workspace whose repository has no commit",
      "when": "readiness is checked",
      "then": "the workspace is not ready and the person is offered the first commit"
    }
  ],
  "covered_by": ["criterion:crates/core/src/git/ready#guarantee"],
  "decisions": [
    {"date": "2026-09-12", "text": "Identity is required rather than invented, so commits are never attributed to a placeholder."}
  ]
}
```

| Field | Type | Rule |
|---|---|---|
| `slug` | string | required, slug |
| `title` | string | required, one line |
| `statement` | string | required, one paragraph, normative (SHALL / MUST) |
| `scenarios` | array of scenario | required, may be empty |
| `covered_by` | array of reference | required, may be empty, only `criterion`, no duplicates |
| `decisions` | array of decision | required, may be empty, ordered oldest first |

Scenario: `title` (required, one line), `given`, `when`, `then` (each
required, one line or short paragraph, plain text).

Decision: `date` (required, ISO `YYYY-MM-DD`), `text` (required, one
paragraph: what was chosen and why). Decisions are append-only through the
tools; editing one is a store-level correction, not a tool.

A requirement with an empty `covered_by` is a requirement the code does not
yet satisfy. That is legal and is what a ticket exists to change. The
uncovered lint reports the reverse, a criterion no requirement names.

### Ticket

An intended change to the manual, or a gap or question about it.

```json
{
  "slug": "phase-62",
  "title": "Sparse checkouts that exclude records",
  "kind": "change",
  "priority": "next",
  "status": "open",
  "touches": ["req:ready-only-with-commit", "module:crates/core/src/git/ready"],
  "relies_on": ["req:readiness-check-reads-only", "page:git-readiness"],
  "body": "Sparse checkouts can exclude the record folder. Readiness must refuse them with a named rule.",
  "decision": null
}
```

| Field | Type | Rule |
|---|---|---|
| `slug` | string | required, slug |
| `title` | string | required, one line |
| `kind` | `change` / `gap` / `question` | required |
| `priority` | `next` / `high` / `later` | required |
| `status` | `open` / `active` / `closed` | required |
| `touches` | array of reference | required, may be empty, any kind, no duplicates |
| `relies_on` | array of reference | required, may be empty, any kind, no duplicates |
| `body` | string | required, Markdown, at most 4,000 characters |
| `decision` | decision or null | required; must be non-null when `status` is `closed`, null otherwise |

`touches` is what the ticket will change. `relies_on` is what it assumes
true and does not change. The same reference may appear in both, which
means the ticket both depends on and alters it; that is worth knowing and is
shown, not forbidden.

On close, `body` is replaced by the empty string and `decision` is set. The
plan and notes go, as the proposal says. A closed ticket cannot be reopened;
a new ticket links to it in `relies_on` if the decision is being revisited.

## Derived nodes

Produced by `crates/extract` on every read of the repository. Listed so the
edge rules and the tools' outputs are defined; none of this is written to the
store.

| Kind | Fields |
|---|---|
| `module` | `id`, `files` (array of file reference), `purpose` (string or null when the contract is missing or malformed), `imports` (array of module reference, resolved by the adapter; unresolved imports are dropped and counted) |
| `criterion` | `module`, `id`, `statement` |
| `file` | `path`, `module`, `language` |
| `decl` | `file`, `path` (declaration path within the file, adapter-defined, such as `Repository::from_top`), `kind` (`function`, `method`, `type`, `trait`, `const`, adapter-extensible), `line` |

Derived edges: module contains file, file contains decl, module imports
module, criterion belongs to module. A module that is expected to carry a
contract but has none or a malformed one still exists as a node, with
`purpose` null and a lint finding.

## Lints

Both run over the joined graph. Each finding names one node or edge, the
rule, and a one-line reason. Findings are stable across runs so hooks can
diff them.

**Dangling.** An edge whose target does not exist.

- A page linking to a page, requirement or module that does not exist.
- A requirement naming a criterion whose module or id does not exist.
- A ticket touching or relying on any reference that does not exist. Closed
  tickets are exempt, since the manual moves on after them.

**Uncovered.** A node no edge reaches.

- A criterion no requirement names.
- A module no page links to.
- A page no page links to, other than pages marked as roots. A root is a page
  whose slug is listed in the store config's `roots`; there must be at least
  one.
- A module expected to carry a contract that has none or a malformed one.

Requirements no page links to are also reported as uncovered. A requirement
with an empty `covered_by` is not a finding; see above.

**Malformed.** A store file that does not parse or violates a field rule.
Reported by the same command, since a broken file is a broken graph.

## Store layout

One directory per store. Subject to change; the tools are the contract.

```
<store>/
  grimoire.json
  pages/<slug>.json
  requirements/<slug>.json
  tickets/<slug>.json
```

`grimoire.json`:

```json
{
  "version": 1,
  "repository": "C:/Users/me/Projects/example",
  "roots": ["overview"],
  "languages": ["rust"]
}
```

`repository` is the absolute path of the target repository. `languages`
selects the adapters. `version` is the schema version; the daemon refuses a
store with a version it does not know. Files are written atomically (write
to a temporary file, rename) and the daemon is the only writer.

The default location for a store is the platform data directory,
`<data>/grimoire/stores/<name>/`, where `<name>` is chosen at `init`. The
target repository gets a `.grimoire.toml` naming the store so that
`grimoire check` and the hooks find it:

```toml
store = "example"
```

That file is the only thing Grimoire writes into a repository, and it is
written once by `init`.
