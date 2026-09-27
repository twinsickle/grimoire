<!-- grimoire -->
## Grimoire

This project is managed by Grimoire. Its **manual** (pages of intent,
requirements, the module contracts in the code) is the truth of what the
project is meant to do; the code and the manual move together in the same
piece of work, and the pre-commit hook checks that they agree.

- **Orient before you read code**: `outline` at depth 2, then the goal,
  architecture and constraints pages, then the area you are working in.
- **Every module under `src/` carries a `# Module contract`** in its leading
  comment: a purpose and criteria. A new module gets one; a changed
  guarantee changes one; a new criterion gets a requirement covering it.
- **Work is a ticket.** `/grimoire start <slug>` before planning,
  `/grimoire spec` to write the manual before the code, `/grimoire finish`
  to lint, confirm and close. `/grimoire file` for anything you find.
- **Never write the store's files.** The tools (MCP `grimoire`, or
  `POST /api/<tool>`, or `grimoire tool`) are the only interface.
- **Propose, don't decide.** A gap or question is resolved by laying out
  options for the person; their answer becomes the decision.

The skill at `.claude/skills/grimoire/SKILL.md` has the sequences; its
`references/` have the contract format and the change-to-update table.
<!-- /grimoire -->
