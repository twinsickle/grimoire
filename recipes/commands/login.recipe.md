# Sign in to a team's server with login, and make tokens with token

How does a person or a pipeline reach a team's grimoire? A team's server admits callers by named tokens. Whoever runs the server makes one per person or pipeline with `grimoire token`; each person signs in once with `grimoire login`.

On the server, by whoever runs it:

```sh
grimoire token add alice --store /data/shop          # prints the token once
grimoire token add reader --read-only --store /data/shop  # may read, never write
grimoire token list --store /data/shop
grimoire token revoke alice --store /data/shop
```

On each person's machine, in the project:

```sh
grimoire login            # paste the token; it is read from standard input
```

**`token`.** `add` makes a token, prints it once and keeps only its digest; `--read-only` makes one that may read and not write. `revoke` refuses it from then on, with no restart. `list` prints each name and what it may do. Tokens are kept beside the store under a lock of their own, so they change while the server runs. A pipeline that prunes the baseline, with `grimoire land` or `grimoire check --main-line`, needs a token that may write.

**`login`.** Takes the token from `--token` or standard input, for the server the project's `.grimoire.toml` names or the one `--server` gives, asks the server with it before keeping it, and keeps it outside any repository, in the person's credentials file. A token refused, a server that does not answer or an empty token keeps nothing and fails. `grimoire logout` forgets it.

**In a pipeline**, set `GRIMOIRE_TOKEN` rather than logging in.

## Recipe

```json
{"id":"login","kind":"command","ingredients":[{"id":"token-add","says":"grimoire token add <name> makes a token, printed once with only its digest kept; --read-only for one that may only read."},{"id":"token-revoke","says":"grimoire token revoke <name> refuses it from then on; token list prints each name and what it may do."},{"id":"login","says":"grimoire login checks the token with the server, then keeps it in the person's credentials file, outside any repository; logout forgets it."},{"id":"environment","says":"GRIMOIRE_TOKEN, when set, is the token a command sends."}],"files":["crates/cli/src/main.rs","crates/cli/src/team.rs",".claude/skills/grimoire/SKILL.md"],"uses":["project-file"]}
```
