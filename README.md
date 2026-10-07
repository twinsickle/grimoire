# Grimoire

Gives a software project its grimoire and its tickets: one graph joining what a
project is meant to do now, what is intended to change, the module contracts
in its code, and the code itself, served to people through a web UI and to
agents through MCP.

Grimoire's own grimoire is its design document and lives in its own store,
not in this repository: run the daemon (below) and read it at
http://127.0.0.1:4242, starting from the pages the root links, in their order, or ask it over the
tools. The tool surface and the store
schema are requirements in the grimoire too, under the Tools and Store pages,
and `GET /api/tools` on a running daemon lists every tool with its input
schema. [examples/](./examples/) holds the notes on other projects it is
pointed at. The original proposal and the early schema documents are in the
git history before the grimoire replaced them.

## Layout

A Cargo workspace and one web package.

- `crates/core` — domain model, store, graph and queries, lints, scaffold, and the tool dispatcher. No I/O.
- `crates/extract` — code extractor: tree-sitter parsing, module contracts, language adapters (Rust, TypeScript and JavaScript, Java) and a comment table for the languages read by their comments alone.
- `crates/server` — the `grimoire serve` daemon: store I/O, watcher, JSON API, MCP over streamable HTTP, SSE.
- `crates/cli` — the `grimoire` binary: `serve`, `check`, `scaffold`, `init`, `tool`.
- `web` — React/Vite UI: the map, the board, node pages. Dark only. `npm run build` writes `web/dist`, which `serve --ui-dir web/dist` serves.
- `examples/` — per-project notes: what each target repository has, and how it is migrated.

## Building and testing

```
cargo build --workspace
cargo test --workspace
cargo run -- --help
```

A release binary carries the web UI inside it: `sh scripts/release` builds it.
`--ui-dir` serves another build instead.

## Using it on a project

```
grimoire init --repo <path> --store <name>       # make the store (a skeleton from the module contracts) and install the integration
grimoire scaffold --repo <path> --store <name>   # build a skeleton store on its own, before init; --force rebuilds one, replacing it
grimoire uninstall [--delete-store]              # remove exactly what init wrote; keep the store unless asked
grimoire check                                   # the lints, for hooks and CI; exit 1 on findings
grimoire serve --port 4242                       # the daemon: the UI, /api/<tool>, /mcp, /api/events
grimoire tool <name> '<json>'                    # one tool against the store, no daemon
```

A store lives under the platform data directory (`<data>/grimoire/stores/<name>`)
unless `--store` is given a path. Only `init` writes into a repository, and only
the integration: `.grimoire.toml`, the agent skill, a marked section in
`CLAUDE.md` and `AGENTS.md`, the `grimoire` entry in `.mcp.json` and a pre-commit
hook. `uninstall` removes exactly that, leaving everything of yours as it was.

### For a team

One server holds the team's intent and no code; each person's daemon reads it
with their own checkout's code and sends its writes there.

```
grimoire serve --shared --store <path> --listen <addr:port>   # on the host: intent only, callers sign in with a token
grimoire serve --shared --library <dir> --listen <addr:port>  # or the team's library: every store under <dir>, each at /g/<folder>
grimoire token add <name> [--read-only] --store <path>        # on the host: a token, printed once
grimoire init --server https://grimoire.example               # in the repository, once: the project file names the server; commit it
                                                              # (https://grimoire.example/g/<name> for a grimoire of a library)
grimoire login                                                # each person, once: the token is kept outside the repository
grimoire serve                                                # each person: the daemon in their checkout, as for any project
grimoire check --ticket <ticket>                              # the pipeline, before a change merges: its code against the ticket's proposal
grimoire land <ticket>                                        # the pipeline, on the main line, with GRIMOIRE_TOKEN set
grimoire check --main-line                                    # the pipeline, on every main-line build: prunes the team's baseline of what is fixed
```

A pipeline signs in by `GRIMOIRE_TOKEN`, a read-only token being enough for
the check, and names the same ticket to both commands; a change that names
none is checked against the truth. The main line's runs need a write token:
`grimoire land` and `grimoire check --main-line` tell the server which
inherited findings the main line has fixed, and it drops them from the
baseline. Run them only on the main line. The server speaks plain HTTP: put a proxy in front for HTTPS. A write needs the
server; offline, reads and `grimoire check` go on from the copy last fetched,
except a check that names a ticket, which fails rather than judge a change
against an old copy.
`GRIMOIRE_HOME` names one directory for everything Grimoire keeps on a machine,
in place of the platform's.

## Development loop

Grimoire is developed pointed at itself, in the setup a team uses: its
grimoire is the store `grimoire`, seeded from this repository's own module
contracts and refined through the tools, served by a shared server on this
machine, which `.grimoire.toml` names. The daemon under development reads that
intent with this checkout's code and sends its writes there, and is the one
its developers and their agents use. Four layers: content is live (the daemon
re-parses a saved file), the daemon restarts on change (bacon), the UI
hot-reloads (Vite), and agents reach the dev daemon directly (`.mcp.json` and
the skill).

```
cargo install bacon                    # once
cargo install --path crates/cli        # the build the server and the hooks run
cd web && npm install && cd ..         # once
node scripts/dev.mjs                   # the server on 4243, the daemon on 4242 rebuilt on every save, the UI on 5173
bacon lint                             # grimoire check on save
bacon test
```

`node scripts/dev.mjs` starts the server from a copy of the installed build,
signs you in to it the first time with a token of your own, and stops
everything together. A change to
the server's side takes effect when the loop is next started after
`cargo install --path crates/cli`. A commit lands no ticket here, as on any
team: `grimoire land <ticket>` does, after the commit.

`bacon serve` also serves a built UI from `web/dist` on 4242 when one exists
(`npm run build`). During UI work use the Vite server on 5173 instead.

An MCP client reads a server's tool list and instructions once, on connect,
and nothing on the server can push an update. A change to tool *behaviour*
reaches a connected agent on its next call, since the transport is stateless.
A change to the tool *set* or to the instructions string needs `/mcp` and a
reconnect in every client, including the agent that made the change.

`.mcp.json` (Claude Code) and `.codex/config.toml` (Codex, once the project
is trusted in your own Codex configuration) point agents working in this
repository at the dev daemon, so the tools are available as tools once the
daemon is up (a fresh session or an MCP reconnect is needed after adding it).
The skill in `.claude/skills/grimoire/` wraps the tools into the start, spec,
finish, file, resolve and adopt sequences; it is the source that
`grimoire init` installs into other projects, at `.claude/skills/grimoire/`
and `.agents/skills/grimoire/` for Codex. Without MCP, the same operations
are `POST http://127.0.0.1:4242/api/<tool>` with the tool's JSON input, or
`GET /api/<tool>?ref=...` for reads; `GET /api/tools` lists them.

To create the self-store from scratch on a new machine:

```
cargo run -- scaffold --repo . --store grimoire
cargo run -- init --repo . --store grimoire
cargo run -- check
```

Status: core, extractor, CLI, daemon and a first web UI implemented. Not yet
done: embedding the UI in the release binary, generating the UI's types from
the Rust model, and a graph view on the ticket page.
