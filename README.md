# Grimoire

A manual and ticketing tool for developing rusty-harness. See [PROPOSAL.md](./PROPOSAL.md) for
the design.

## Layout

- `packages/core` — domain model, store parser/writer, contract extractor, graph, lints, scaffold.
- `packages/server` — the `grimoire serve` daemon: watcher, API, MCP tools, SSE.
- `packages/web` — React/Vite UI: the map, the board, node pages.
- `packages/cli` — `serve`, `check`, `scaffold`, `init`.

Status: scaffolding only, nothing implemented yet.
