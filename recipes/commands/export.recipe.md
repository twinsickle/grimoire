# Hand a grimoire to someone with export, and take it up with init --from

How do I give someone a project's grimoire when they will use it on their own, with no team's server? Export it to one file and hand them the file; in their clone of the project, `init --from` makes a store of their own from it.

```sh
grimoire export --out ../shop.json
```

and on their machine, in their clone:

```sh
grimoire init --store shop --from shop.json
```

**What the file holds.** All of the intent: every page, requirement with its decisions, and ticket, open and bottled, with the proposals of the work under way, the baseline, the recipes' judgements and the store's configuration, its roots and languages among it. Nothing of a machine, no repository path, no current ticket, and nothing of the people: who has which ticket checked out is left out. It is in the form a team's server hands each machine, the same intent with who has which ticket checked out left out, so a build that reads one reads the other. Without `--out` it is written to standard output. Export writes nothing into the repository or the store: an `--out` inside either is refused, the file is written beside `--out` and renamed over it, so a link there is replaced rather than written through, and a store of its own holding a record that does not load is not exported at all, since the file would hand on less than the grimoire holds.

**From a team's grimoire.** In a checkout of a project that names a server, export writes the server's intent as it is now; only when the server cannot be reached does it write this checkout's copy, as last fetched, saying so. A read-only token is enough.

**Taking it up.** `init --from` makes a new store, never merging into one that exists: name a store that is not there yet. The store holds the file's intent with this checkout as its repository, and init then does what it always does, writing `.grimoire.toml` and the integration and adopting the findings present, since the baseline the file carried was judged against someone else's code. A ticket that was brewing arrives brewing, with its proposal, and is nobody's current ticket here: `brew_ticket` takes it up, making it current with a start of its own. A file that is not an export, or one from a newer build's store, is refused before anything is written, and `--from` is never given beside `--server` or `--empty`.

## Recipe

```json
{"id":"export","kind":"command","ingredients":[{"id":"out","says":"--out <file>: where the export is written, outside the repository and the store; standard output when not given."},{"id":"holds","says":"All the intent, open work and proposals included, as a team's server hands a machine; nothing of a machine and not who has a ticket checked out."},{"id":"team","says":"On a project that names a server, the server's intent read now; the checkout's copy, saying so, only when the server cannot be reached."},{"id":"from","says":"grimoire init --store <new store> --from <file> makes a store of one's own from the file, joined with this checkout, and adopts it; an existing store is refused."}],"files":["crates/cli/src/main.rs","crates/cli/src/init/command.rs","crates/core/src/export.rs"]}
```
