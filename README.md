# Grimoire

Gives a software project its grimoire and its tickets: one graph joining what a
project is meant to do now, what is intended to change, the module contracts
in its code, and the code itself, served to people through a web UI and to
agents through MCP.

Grimoire's own grimoire is its design document and lives in its own store,
not in this repository: run the daemon (below) and read it at
http://127.0.0.1:4242, starting from the goal, architecture, constraints, conventions and
glossary pages, or ask it over the tools. The tool surface and the store
schema are requirements in the grimoire too, under the Tools and Store pages,
and `GET /api/tools` on a running daemon lists every tool with its input
schema. [examples/](./examples/) holds the notes on other projects it is
pointed at. The original proposal and the early schema documents are in the
git history before the grimoire replaced them.

## Layout

A Cargo workspace and one web package.

- `crates/core` — domain model, store, graph and queries, lints, scaffold, and the tool dispatcher. No I/O.
- `crates/extract` — code extractor: tree-sitter parsing, module contracts, language adapters (Rust so far).
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

## Using it on a project

```
grimoire scaffold --repo <path> --store <name>   # skeleton store from the module contracts
grimoire init --repo <path> --store <name>       # install the integration into the repository
grimoire uninstall [--delete-store]              # remove exactly what init wrote; keep the store unless asked
grimoire check                                   # the lints, for hooks and CI; exit 1 on findings
grimoire serve --port 4242                       # the daemon: /api/<tool>, /mcp, /api/events
grimoire tool <name> '<json>'                    # one tool against the store, no daemon
```

A store lives under the platform data directory (`<data>/grimoire/stores/<name>`)
unless `--store` is given a path. Only `init` writes into a repository, and only
the integration: `.grimoire.toml`, the agent skill, a marked section in
`CLAUDE.md` and `AGENTS.md`, the `grimoire` entry in `.mcp.json` and a pre-commit
hook. `uninstall` removes exactly that, leaving everything of yours as it was.

## Development loop

Grimoire is developed pointed at itself: `.grimoire.toml` names the store
`grimoire`, seeded from this repository's own module contracts and refined
through the tools. Its own repository is the first target, its own grimoire the
first store, and the daemon under development is the one its developers and
their agents use. Four layers: content is live (the daemon re-parses a saved
file), the daemon restarts on change (bacon), the UI hot-reloads (Vite), and
agents reach the dev daemon directly (`.mcp.json` and the skill).

```
cargo install bacon          # once
bacon serve                  # rebuild and restart the daemon on every save, port 4242
bacon lint                   # grimoire check on save
bacon test
cd web && npm install && npm run dev   # the UI with hot reload on 5173, proxied to the daemon
```

`bacon serve` also serves a built UI from `web/dist` on 4242 when one exists
(`npm run build`). During UI work use the Vite server on 5173 instead.

An MCP client reads a server's tool list and instructions once, on connect,
and nothing on the server can push an update. A change to tool *behaviour*
reaches a connected agent on its next call, since the transport is stateless.
A change to the tool *set* or to the instructions string needs `/mcp` and a
reconnect in every client, including the agent that made the change.

`.mcp.json` points agents working in this repository at the dev daemon, so
the tools are available as tools once the daemon is up (a fresh session or an
MCP reconnect is needed after adding it). The Claude Code skill in
`.claude/skills/grimoire/` wraps the tools into the start, spec, finish and
file sequences (`/grimoire start <slug>` and so on); it is the source that
`grimoire init` will install into other projects. Without MCP, the same operations
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
