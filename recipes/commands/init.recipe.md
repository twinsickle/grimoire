# Set up Grimoire in a repository with init, and take it out with uninstall

How do I start using Grimoire in a repository? Run `grimoire init` at its root. It names the grimoire in `.grimoire.toml` and installs what your agents and your commits need; `grimoire uninstall` takes exactly that back out.

For one person, with a store on this machine:

```sh
grimoire init --store shop --languages rust,typescript
```

For a team whose grimoire a server keeps:

```sh
grimoire init --server https://grimoire.example.com/g/shop
grimoire login
```

**What it writes.** Only the integration, each path listed as it goes: `.grimoire.toml`; the agent skill and its references, for Claude Code under `.claude/skills/grimoire` and for Codex under `.agents/skills/grimoire`; a marked section in `CLAUDE.md` and `AGENTS.md`, which sends every session to the grimoire; the MCP entry pointing at the daemon in `.mcp.json` and `.codex/config.toml`; and, in a git repository, a pre-commit hook running `grimoire check` and a post-commit hook running `grimoire land`. A file of yours is merged into, never overwritten, and a hook of your own is left alone with a note saying what to add to it. No other command writes into the repository.

**The store.** With `--store`, a bare name is kept under Grimoire's data folder, a path is taken as it is; a store that does not exist is made, with the skeleton read off the module contracts already in the code in the `--languages` given (Rust when none), or with `--empty` holding only its two roots, to write a grimoire from nothing. Then init adopts: it records every present finding as inherited work, so the hook it installs passes on the next commit and the check fails only on new findings. With `--server`, no store is made and nothing adopted; the server keeps the grimoire.

**Other options.** `--name` names the grimoire, the same on every machine (by default the name the address carries in `/g/<name>`, else the name `.grimoire.toml` already holds, else the folder's); `--port` is the daemon's port written into the MCP entry, 4242 by default; `--repo` runs it for another folder.

**Run it again** to update the installed skill after upgrading Grimoire, or in a second checkout of the same project: it keeps the store's baseline and adds the checkout.

**Taking it out.** `grimoire uninstall` removes exactly what init wrote and nothing of yours, the marked section cut out of your guidance files to the byte. The store is kept unless you add `--delete-store`, which deletes it only when it is a store naming this repository and no daemon started for it alone serves it: a daemon serving it in the machine's library is asked to let it go, and while another command holds it the deletion is queued and happens once nothing does. A team's grimoire is the server's and is never deleted from here.

## Recipe

```json
{"id":"init","kind":"command","ingredients":[{"id":"store","says":"--store <name or path>: a store of the project's own, made if it does not exist, from the module contracts in --languages or empty with --empty."},{"id":"server","says":"--server <address>: the team's server keeps the grimoire; no store is made here, and never beside --store."},{"id":"name","says":"--name: the grimoire's name, the same on every machine; defaults to the address's /g/<name>, else the project file's, else the folder's."},{"id":"integration","says":"Writes .grimoire.toml, the skill for each harness, the marked guidance section, the MCP entries and the pre-commit and post-commit hooks, merging into what is there."},{"id":"adopts","says":"A new store's present findings become its baseline, so the next commit's check passes."},{"id":"uninstall","says":"grimoire uninstall removes exactly what init wrote; --delete-store also deletes a store of the project's own, queued while something holds it."}],"files":["crates/cli/src/main.rs","crates/cli/src/init.rs","crates/cli/src/init","crates/cli/src/uninstall.rs",".claude/skills/grimoire/SKILL.md",".claude/skills/grimoire/references/guidance-section.md"],"uses":["project-file"]}
```
