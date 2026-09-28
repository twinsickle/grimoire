<!-- grimoire -->
## Grimoire

This project's grimoire (pages, requirements, the module contracts in the
code) is the truth of what it is meant to do. Code and grimoire change in the
same piece of work; the pre-commit hook checks they agree.

- Orient first: `outline` at depth 2, then the goal, architecture and
  constraints pages, then your area. `get` a module before opening its files.
- Every module under `src/` carries a `# Module contract`. New module, new
  contract; changed guarantee, changed criterion; new criterion, a
  requirement covering it.
- Work is a ticket: `/grimoire start`, `spec`, `finish`. `/grimoire file`
  for anything you find.
- Never write the store's files. The tools are the only interface.
- Propose, don't decide. A gap or question closes on the person's answer to
  options you lay out.

Sequences: `.claude/skills/grimoire/SKILL.md`.
<!-- /grimoire -->
