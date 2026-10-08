# Run the daemon, or a team's server, with serve

How do agents and people reach a grimoire while they work? Run `grimoire serve`. On a machine it is the daemon: it watches the repository, keeps the graph current, and serves the tools over MCP and HTTP, and the web UI for reading when the build carries one (a release build does) or `--ui-dir` names a built one. Run with `--shared`, it is a team's server, which keeps the intent and holds no code.

On a machine, from the repository:

```sh
grimoire serve                     # http://127.0.0.1:4242/g/<name>/: MCP at mcp, the tools at api/<tool>
```

A team's server, keeping every grimoire under a folder and serving one's site:

```sh
grimoire serve --shared --library /data --listen 0.0.0.0:4243 --site 0.0.0.0:4244 --site-of shop
```

**On a machine.** Started in a checkout, it finds the grimoire from `.grimoire.toml` and listens on the loopback at `--port`, 4242 by default. One daemon serves the machine's library: each grimoire a checkout on the machine names at `/g/<name>/`, which is where the MCP entry `init` wrote points (`/g/<name>/mcp`), and this checkout's own on the bare API and MCP routes as well; the UI at the daemon's own address is the Library of them. Given `--store`, it serves that store alone, with no library. On a project that names a server, it reads a copy of the server's intent with this checkout's code and sends every write to the server, signed in with the token `grimoire login` kept. It is the store's one writer: a command run while it serves goes through it.

**A team's server.** `--shared` serves intent only, with no repository beside it and no UI, and admits callers by a named token (`grimoire token`). `--listen` is the address it listens on, plain HTTP, so put a proxy in front for HTTPS and for anything facing the internet. `--library <folder>` serves every store under the folder, each grimoire at `/g/<name>/` behind its own tokens.

**The site.** `--site <address>` also serves how the project is meant to be used, with no sign-in: the pages `configure` names in `site`, and the catalog of recipes. With `--library`, `--site-of <name>` says whose site it is. Who may reach that address is the deployment's to decide.

**When not to.** For a check in a hook or a pipeline, `grimoire check` needs no daemon.

## Recipe

```json
{"id":"serve","kind":"command","ingredients":[{"id":"daemon","says":"With neither --shared nor --store, the machine's daemon on the loopback --port, 4242 by default, serving each grimoire of the machine at /g/<name>/: MCP at mcp, the tools at api/<tool>, and the UI for reading from a release build or --ui-dir."},{"id":"store","says":"--store <name or path> serves that store alone, with no library."},{"id":"shared","says":"--shared serves intent only, no repository and no UI, admitting callers by token; --listen sets its address."},{"id":"library","says":"--library <folder> with --shared serves every store under the folder, each at /g/<name>/."},{"id":"site","says":"--site <address> serves the project's site with no sign-in; with --library, --site-of names whose."}],"files":["crates/cli/src/main.rs",".claude/skills/grimoire/SKILL.md"],"uses":["project-file"]}
```
