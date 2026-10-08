# Name a project's grimoire in .grimoire.toml

How does a repository say where its grimoire is kept? A file `.grimoire.toml` at the repository's root names it: a store of the project's own on this machine, or a team's server. `grimoire init` writes it, and every command and the daemon find it by walking up from the folder they run in. Commit it, so everyone who checks the project out reaches the same grimoire.

A team's grimoire, kept on a server:

```toml
name = "shop"
server = "https://grimoire.example.com/g/shop"
```

One person's, kept in a store on this machine:

```toml
name = "shop"
store = "shop"
```

**`name`.** The grimoire's name, a slug, the same on every machine; the agent's MCP entry carries it as `/g/<name>/mcp`. Left out, it is the store's when that is a bare name, else the repository folder's, made a slug.

**`store` or `server`, never both.** A file naming both, or neither, is refused.

- `store` is a bare name, kept in Grimoire's data folder as `stores/<name>`, the data folder being `GRIMOIRE_HOME` when it is set and the platform's data folder for Grimoire otherwise; or an absolute path; or a path relative to the repository. A store inside the repository it describes is refused: the grimoire is kept outside the code.
- `server` is an `http://` or `https://` address with a host, and no query or fragment, ending `/g/<name>` when the server keeps a library of several grimoires. This machine keeps a copy of the server's intent outside the repository, and every write goes to the server, once you have signed in with `grimoire login`.

**When not to edit it by hand.** To point a checkout at another store or server, run `grimoire init` again, which checks what it is pointing at; `grimoire uninstall` removes the file with the rest of the integration.

## Recipe

```json
{"id":"project-file","kind":"format","ingredients":[{"id":"name","says":"The grimoire's name, a slug, the same on every machine; defaults to a bare store name, else the repository folder's."},{"id":"store","says":"A store of the project's own: a bare name, kept as stores/<name> in Grimoire's data folder, an absolute path, or a path relative to the repository, never inside it."},{"id":"server","says":"A team's server: an http:// or https:// address with a host, ending /g/<name> for a grimoire of a library; never beside store."}],"files":["crates/server/src/store_io/location.rs"]}
```
