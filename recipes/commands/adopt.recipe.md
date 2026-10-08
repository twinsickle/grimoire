# Accept what is there today with adopt

How do I start using Grimoire on code that already has findings, modules with no contract, criteria no test checks, without the check failing on all of them at once? Record them as the baseline: inherited work, marked baselined, which never fails the check. From then on the check fails only on new findings, and the baseline shrinks as the code is brought in.

```sh
grimoire adopt --reason "adopted at the start: contracts come area by area"
```

**What it does.** Replaces the store's baseline with every finding present now, dated today, keeping the reason you give. A baseline it replaces is counted and gone. It is a write like any other: through the daemon serving the store when one runs, and on a team through the server, with the findings this checkout's code produces.

**You rarely need it.** `init` adopts a new store once, with the reason "adopted at init", so the hook it installs passes on the next commit. The baseline then only shrinks: whatever the code no longer produces is pruned from it.

**When not to.** Never to make a failing check pass: that turns a finding your work created into inherited work nobody owns. It is the person's call, made with a reason. To bring adopted code in, contracts first and then tests, a batch at a time, follow the skill's adopt sequence.

## Recipe

```json
{"id":"adopt","kind":"command","ingredients":[{"id":"reason","says":"--reason, required: why the present findings are accepted as inherited work."},{"id":"baseline","says":"Replaces the baseline with every finding present now, dated today; baselined findings never fail the check."},{"id":"shrinks","says":"The baseline only shrinks afterwards, pruned of what the code no longer produces."}],"files":["crates/cli/src/main.rs",".claude/skills/grimoire/SKILL.md"]}
```
