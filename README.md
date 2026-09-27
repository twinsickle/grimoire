# Grimoire

A manual and ticketing tool for software projects: one graph joining what a
project is meant to do now, what is intended to change, the module contracts
in its code, and the code itself, served to people through a web UI and to
agents through MCP. See [PROPOSAL.md](./PROPOSAL.md) for the design and
[examples/](./examples/) for the projects it is pointed at.

## Layout

A Cargo workspace and one web package.

- `crates/core` — domain model, store, graph and queries, lints, scaffold. No I/O.
- `crates/extract` — code extractor: tree-sitter parsing, module contracts, language adapters.
- `crates/server` — the `grimoire serve` daemon: watcher, API, MCP tools, SSE, embedded web UI.
- `crates/cli` — the `grimoire` binary: `serve`, `check`, `scaffold`, `init`.
- `web` — React/Vite UI, built by Node at build time into the server. Dark only.
- `examples/` — per-project notes: what each target repository has, and how it is migrated.

## Building

```
cargo build --workspace
cargo run -- --help
```

The web package is built separately and only needed for the UI.

Status: scaffolding only, nothing implemented yet.
