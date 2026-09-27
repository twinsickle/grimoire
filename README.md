# Grimoire

A manual and ticketing tool for software projects: one graph joining what a
project is meant to do now, what is intended to change, the module contracts
in its code, and the code itself, served to people through a web UI and to
agents through MCP. See [PROPOSAL.md](./PROPOSAL.md) for the design,
[docs/STORE.md](./docs/STORE.md) for the store schema,
[docs/TOOLS.md](./docs/TOOLS.md) for the MCP tools, and
[examples/](./examples/) for the projects it is pointed at.

## Layout

A Cargo workspace and one web package.

- `crates/core` — domain model, store, graph and queries, lints, scaffold, and the tool dispatcher. No I/O.
- `crates/extract` — code extractor: tree-sitter parsing, module contracts, language adapters (Rust so far).
- `crates/server` — the `grimoire serve` daemon: store I/O, watcher, JSON API, MCP over streamable HTTP, SSE.
- `crates/cli` — the `grimoire` binary: `serve`, `check`, `scaffold`, `init`, `tool`.
- `web` — React/Vite UI, not yet scaffolded. Built last, per the proposal.
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
grimoire init --repo <path> --store <name>       # write .grimoire.toml into the repository
grimoire check                                   # the lints, for hooks and CI; exit 1 on findings
grimoire serve --port 4242                       # the daemon: /api/<tool>, /mcp, /api/events
grimoire tool <name> '<json>'                    # one tool against the store, no daemon
```

A store lives under the platform data directory (`<data>/grimoire/stores/<name>`)
unless `--store` is given a path. The only file written into a repository is
`.grimoire.toml`, by `init`.

## Development loop

Grimoire is developed pointed at itself: `.grimoire.toml` names the store
`grimoire`, seeded from this repository's own module contracts. See
"Development loop" in the proposal.

```
cargo install bacon          # once
bacon serve                  # rebuild and restart the daemon on every save, port 4242
bacon lint                   # grimoire check on save
bacon test
```

`.mcp.json` points agents working in this repository at the dev daemon, so
the tools are available as tools once the daemon is up (a fresh session or an
MCP reconnect is needed after adding it). Without MCP, the same operations
are `POST http://127.0.0.1:4242/api/<tool>` with the tool's JSON input, or
`GET /api/<tool>?ref=...` for reads; `GET /api/tools` lists them.

To create the self-store from scratch on a new machine:

```
cargo run -- scaffold --repo . --store grimoire
cargo run -- init --repo . --store grimoire
cargo run -- check
```

Status: core, extractor, CLI and daemon implemented; web UI not started.
