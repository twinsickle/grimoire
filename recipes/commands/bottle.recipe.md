# Tell Grimoire a change is on the main line with bottle

How is a ticket bottled once its code is in? Its decision was written when the work finished, leaving it simmering; `grimoire bottle` bottles it, applying its proposal to the truth, so the grimoire moves when the code does and not before.

```sh
grimoire bottle            # after a commit, one person: the current ticket if it is simmering
grimoire bottle 0200       # on a team's main line, once the change has merged
```

**One person.** `init` installs `grimoire bottle` as the post-commit hook, so the commit is the bottling: the pre-commit check judges the code against the ticket's proposal, and the post-commit hook bottles the ticket. A commit with no ticket simmering says so and exits 0. A merge runs neither hook, so run `grimoire bottle` after one; a hook of your own that init left alone needs the line added.

**A team.** A commit on a machine bottles nothing, and `grimoire bottle` with no ticket says the main line does. Bottle a ticket by name where the change has merged, from the pipeline or by hand: the server bottles it, then the command prunes the server's baseline from the main line's code and sends the server the recipes that code reads, for its site.

**When it is refused.** It exits non-zero when the ticket named is not simmering, when the store cannot be reached, and when the truth moved under a record the ticket proposes since it was written, printing the records that moved: write each again under the ticket and run `grimoire bottle <ticket>` again. On a team, a bottling taken whose prune then failed also exits non-zero, saying the ticket was bottled. A recipe snapshot the server did not take is a warning and never fails it.

`grimoire land`, the command's name before, still runs it, so a hook or a pipeline written for it goes on working.

## Recipe

```json
{"id":"bottle","kind":"command","ingredients":[{"id":"ticket","says":"The ticket to bottle, a slug or ticket: reference; with none, the current ticket when it is simmering."},{"id":"hook","says":"Installed by init as the post-commit hook, so one person's commit bottles the ticket; a merge runs no hook."},{"id":"team","says":"On a team, bottles only a ticket named on the main line, then prunes the server's baseline and sends it the recipes."},{"id":"refused","says":"Exits non-zero when the ticket is not simmering, the store cannot be reached, or the truth moved, naming the records to write again."}],"files":["crates/cli/src/main.rs","crates/cli/src/check.rs","crates/cli/src/team.rs",".claude/skills/grimoire/SKILL.md"]}
```
