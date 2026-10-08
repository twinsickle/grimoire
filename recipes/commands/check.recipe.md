# Check the code against its grimoire with check

How do I find out whether my change leaves the code and its grimoire connected, in a hook or a pipeline, with no daemon running? Run `grimoire check`. It runs the same lints as the `lint` tool and exits 1 when there are findings that are not inherited, so a commit or a build stops on them. `init` installs it as the pre-commit hook.

```sh
grimoire check                     # before a commit: this branch's own work
grimoire check --ticket 0200       # a pipeline, before the change is bottled
grimoire check --main-line         # a pipeline, on every main-line build
```

**What it judges.** On a branch, the code against the truth plus the current ticket's proposal, so a branch is judged against its own intent; and only the findings the branch's own work causes fail it. Findings caused by other work, on the main line since the branch began or by another ticket, are printed as advisories and fail nothing. Each finding a simmering change of this checkout would mend is said beside it, still failing. Omens, signs that something may be wrong such as a file grown past the size limit, are printed and fail nothing.

**`--ticket <id>`** judges the code against that ticket's proposal, as a pipeline does before the change is bottled; it fails when the ticket is neither brewing nor simmering, or, on a team, when the server cannot be read as it is now.

**`--main-line`** judges against the truth whatever the current ticket. On a team it also prunes the server's baseline of what the code no longer produces, which needs a write token, and sends the server the recipes the code reads, for its site; a prune the server refuses fails it, a recipe snapshot it does not take is only a warning. Run it on the main line only, never from a branch.

**The baseline.** Findings recorded as inherited, with `grimoire adopt` or at init, are marked baselined and never fail the check. The baseline only shrinks: what the code no longer produces is pruned from it.

**Other options.** `--rules dangling,uncovered,malformed,unspecified` keeps only those rules; `--json` prints the same shape as the `lint` tool; `--store` names a store other than the project file's.

**When it fails for no sensible reason**, a build that cannot read a configured language says so on its first line: the installed `grimoire` is older than the store's languages.

## Recipe

```json
{"id":"check","kind":"command","ingredients":[{"id":"exit","says":"Exits 1 when there are findings outside the baseline, or with --main-line on a team when the prune fails; omens and advisories never fail it."},{"id":"own-work","says":"With no option, judges the branch's code against the current ticket's proposal and fails only on what its own work causes."},{"id":"ticket","says":"--ticket <id>: judges against that ticket's proposal, as a pipeline does before it is bottled."},{"id":"main-line","says":"--main-line: judges against the truth; on a team prunes the server's baseline and sends it the recipes, with a write token."},{"id":"json","says":"--json prints the lint tool's shape; --rules keeps only the rules named."}],"files":["crates/cli/src/main.rs","crates/cli/src/check.rs",".claude/skills/grimoire/SKILL.md"],"uses":["project-file"]}
```
