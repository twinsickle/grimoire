//! # Module contract
//!
//! ```json
//! {"purpose":"What a command does on a project that names a team's server: sign a person in and out, open the checkout's copy of the server's intent with the checkout's code, and run a tool through the server.","criteria":[{"id":"token-from-flag-else-stdin","statement":"login takes a token from --token, else from standard input."},{"id":"address-from-project","statement":"login signs in to the grimoire address the project names."},{"id":"address-from-server-flag","statement":"login signs in to the address given with --server, in place of the one the project names."},{"id":"asked-of-the-server-before-kept","statement":"login asks the server at the address with the token before keeping it."},{"id":"kept-under-address","statement":"login keeps the token in the person's credentials file under the address, so each grimoire of a server is signed in to on its own."},{"id":"refused-token-keeps-nothing","statement":"login keeps nothing and fails when the server refuses the token."},{"id":"unanswering-server-keeps-nothing","statement":"login keeps nothing and fails when the server does not answer."},{"id":"empty-token-keeps-nothing","statement":"login keeps nothing and fails when the token is empty."},{"id":"logout-removes-token","statement":"logout removes the kept token."},{"id":"logout-says-when-none","statement":"logout says so when there was no kept token."},{"id":"copy-synced-first","statement":"A command on a project that names a server, with no daemon serving the checkout's copy, takes the copy's lock when it can and syncs the copy from the server first."},{"id":"copy-read-when-away","statement":"When the server does not answer and the checkout holds a copy, a command on a project that names a server reads the copy, saying on standard error that the server was not reached and when the copy was fetched."},{"id":"copy-named-check-fails","statement":"A check that names a ticket fails rather than read a copy it could not fetch now, or one a daemon beside it cannot say is current."},{"id":"copy-none-or-refused","statement":"A command on a project that names a server, with no copy, or a token the server refuses, fails."},{"id":"copy-daemon-asked","statement":"When the daemon serving the checkout holds the lock, a command on a project that names a server asks that daemon what it knows of the server and says the same, failing when the server refused the token."},{"id":"copy-never-prunes","statement":"A command on a project that names a server never prunes the baseline."},{"id":"read-now","statement":"read_now fetches the server's intent into memory and joins it with the checkout's code, reading and writing nothing of the checkout's copy, and answers it with the digest that names the intent it read; it fails when the server cannot be reached or refuses the token."},{"id":"prune-sends-findings-with-digest","statement":"prune_from_main_line lints the checkout against the server's truth as read_now gives it and sends the server every finding it produced with the digest of that intent, over POST /api/prune."},{"id":"prune-retried-when-intent-moved","statement":"prune_from_main_line, when the server answers that the intent moved since, and only then, reads it again and sends again, up to three tries, failing after the third."},{"id":"prune-answers-what-it-judged","statement":"prune_from_main_line answers the intent and code it last judged, the one the prune was sent against, with how many baseline entries went or why the prune failed."},{"id":"prune-unreadable-language-sends-nothing","statement":"prune_from_main_line sends nothing when a configured language is one the build cannot read, and answers why the prune failed."},{"id":"prune-refused-says-why","statement":"prune_from_main_line answers why the prune failed when the server refuses it otherwise than as the intent having moved, or does not answer."},{"id":"prune-fails-outright-only-when-intent-unreadable","statement":"prune_from_main_line fails outright only when the intent cannot be read."},{"id":"sends-recipes","statement":"send_recipes sends the server every recipe the graph it is given reads, each by its id, kind, title, text, ingredients by id and what each says, uses and digest, and nothing of its files, its file or its line, over POST /api/recipes, and answers how many the server holds, or why it was not taken: the server refusing it, an older one knowing no such route among them, or not answering."},{"id":"runs-on-the-daemon-when-there-is-one","statement":"tool, adopt and bottle on a project that names a server run on the daemon serving the checkout's copy when there is one."},{"id":"read-tool-answers-over-the-copy","statement":"Without such a daemon, a read tool on a project that names a server answers over the copy with the checkout's code."},{"id":"write-tool-goes-through-the-server","statement":"Without such a daemon, a write tool on a project that names a server goes through the server under the copy's lock."},{"id":"write-refused-while-lock-held","statement":"Without such a daemon, a write tool on a project that names a server is refused when another process holds the copy's lock."},{"id":"write-refused-when-server-away","statement":"Without such a daemon, a write tool on a project that names a server is refused as store_unavailable when the server does not answer."}]}
//! ```

use std::io::BufRead;
use std::path::Path;

use anyhow::{bail, Context};
use grimoire_core::answer::ServerCopy;
use grimoire_server::remote::{self, RemoteError, Server};
use grimoire_server::store_io::{self, Home, StoreIoError, StoreLock};
use grimoire_server::{client, Loaded};
use serde_json::{json, Value};

/// A project that names a server, as a command sees it: the server's address,
/// the checkout, and where the checkout's copy of the server's intent is kept.
pub struct Team {
    pub address: String,
    pub repo: std::path::PathBuf,
    pub copy: std::path::PathBuf,
}

impl Team {
    /// The server, signed in as this person.
    pub fn server(&self) -> anyhow::Result<Server> {
        Ok(Server::signed_in(&self.address, &store_io::credentials_file()?))
    }
}

/// The grimoire address a sign-in is for: the one given, else the one the project names.
fn address(given: Option<&str>) -> anyhow::Result<String> {
    match given {
        Some(s) => store_io::server_address(s).map_err(anyhow::Error::msg),
        None => match store_io::find_home(&std::env::current_dir()?) {
            Ok((_, Home::Server { address, .. })) => Ok(address),
            Ok((repo, Home::Store(_))) => bail!("the project at {} names a store on this machine, not a server; there is nothing to sign in to. Name a server with --server", repo.display()),
            Err(StoreIoError::NoProjectFile(_)) => bail!("no project here names a server; name one with --server"),
            Err(e) => Err(e.into()),
        },
    }
}

/// Keep a person's token for a grimoire's address, once the server has taken it.
pub fn login(server: Option<&str>, token: Option<String>) -> anyhow::Result<()> {
    let address = address(server)?;
    let token = match token {
        Some(t) => t,
        None => {
            eprintln!("token for {address} (as `grimoire token add` printed it), then Enter:");
            let mut line = String::new();
            std::io::stdin().lock().read_line(&mut line).context("reading the token from standard input")?;
            line
        }
    };
    let token = token.trim().to_string();
    if token.is_empty() {
        bail!("no token was given; nothing was kept");
    }
    // Asked with the token before it is kept, so a mistyped one is caught now.
    match Server::new(&address, Some(token.clone())).call("status", &json!({})) {
        Ok(Ok(status)) => {
            if status["shared"] != true {
                eprintln!("note: {address} answers as a daemon on a person's machine, which admits any local caller; a team's server is started with `grimoire serve --shared`");
            }
        }
        Ok(Err(refused)) => bail!("the server at {address} refused the token: {}. Nothing was kept", refused["message"].as_str().unwrap_or("no reason given")),
        Err(RemoteError::Refused(_, why)) => bail!("the server at {address} refused the token: {why}. Nothing was kept"),
        Err(e) => bail!("{e}. Nothing was kept"),
    }
    let file = store_io::credentials_file()?;
    remote::keep_token(&file, &address, &token).with_context(|| format!("writing {}", file.display()))?;
    println!("signed in to {address}; the token is kept in {}", file.display());
    Ok(())
}

/// Forget the token kept for a grimoire's address.
pub fn logout(server: Option<&str>) -> anyhow::Result<()> {
    let address = address(server)?;
    let file = store_io::credentials_file()?;
    if remote::forget_token(&file, &address).with_context(|| format!("writing {}", file.display()))? {
        println!("signed out of {address}");
    } else {
        println!("no token was kept for {address}");
    }
    Ok(())
}

/// The checkout's copy, opened for a command: its lock when this command got
/// it, and the copy loaded with the checkout's code.
pub struct Opened {
    pub loaded: Loaded,
    pub lock: Option<StoreLock>,
}

/// Open the checkout's copy of the server's intent for a command with no
/// daemon beside it: synced first when this command can take the copy's lock
/// and the server answers; read as it is when the server does not answer,
/// unless `fresh`, when only the server's intent as it is now will do.
pub fn open(team: &Team, server: &Server, fresh: bool) -> anyhow::Result<Opened> {
    let held = team.copy.join(store_io::CONFIG_FILE).is_file();
    store_io::refuse_inside(&team.copy, &team.repo)?;
    let lock = match remote::lock_copy(&team.copy) {
        Ok(lock) => Some(lock),
        // A daemon still starting, or another command: the copy is read as it is.
        Err(StoreIoError::Locked(..)) if held => None,
        Err(e) => return Err(e.into()),
    };
    let mut reachable = true;
    let repository = store_io::repository_string(&team.repo)?;
    let away = |why: String| {
        let fetched = remote::copy_note(&team.copy).map_or("at a time it does not record".to_string(), |n| format!("as of {}", n.fetched));
        eprintln!("warning: {why}; reading this checkout's copy of its intent, {fetched}");
    };
    let stale = |why: String| anyhow::anyhow!("{why}, and a check that names a ticket is judged only against the server's intent as it is now, never a copy fetched before");
    if lock.is_some() {
        match remote::sync(server, &team.copy, &repository, None) {
            Ok(_) => {}
            Err(e @ RemoteError::Unreachable(..)) if fresh => return Err(stale(e.to_string())),
            Err(e @ RemoteError::Unreachable(..)) if held => {
                reachable = false;
                away(e.to_string());
            }
            Err(e @ RemoteError::Unreachable(..)) => bail!("{e}, and this checkout holds no copy of its intent yet"),
            Err(e) => return Err(e.into()),
        }
    } else if let client::Probe::Serving(addr) = client::probe(&team.copy, &repository) {
        // The daemon serving this checkout holds the copy and keeps it synced: what it knows of the
        // server is what this command says, never a server taken to be there because nobody asked.
        let answered = client::call_to(&addr, "status", &json!({}));
        if fresh && !matches!(&answered, Ok((true, s)) if s["server"]["reachable"] == true) && !matches!(&answered, Ok((true, s)) if s["server"]["refused"] == true) {
            return Err(stale(format!("the daemon serving this checkout at {addr} cannot say the server at {} is reachable", team.address)));
        }
        if let Ok((true, status)) = answered {
            if status["server"]["refused"] == true {
                bail!("the server at {} refused this machine's token, so its intent is not read here: sign in with `grimoire login`", team.address);
            }
            if status["server"]["reachable"] == false {
                if fresh {
                    return Err(stale(format!("the server at {} cannot be reached, as the daemon serving this checkout says", team.address)));
                }
                reachable = false;
                away(format!("the server at {} cannot be reached, as the daemon serving this checkout says", team.address));
            }
        }
    } else if fresh {
        return Err(stale(format!("another process holds this checkout's copy at {}, so it could not be fetched", team.copy.display())));
    }
    let mut loaded = grimoire_server::load(&team.copy)?;
    loaded.state.set_server(Some(ServerCopy { address: team.address.clone(), fetched: remote::copy_note(&team.copy).map(|n| n.fetched), reachable, refused: false }));
    Ok(Opened { loaded, lock })
}

/// Run one tool on a project that names a server: on the daemon serving the
/// checkout's copy when there is one; otherwise a read over the copy, and a
/// write through the server. The inner error is the tool's error object.
pub fn run_tool(team: &Team, name: &str, input: Value) -> anyhow::Result<Result<Value, Value>> {
    if team.copy.join(store_io::CONFIG_FILE).is_file() {
        match client::probe(&team.copy, &store_io::repository_string(&team.repo)?) {
            client::Probe::Serving(addr) => {
                let (ok, v) = client::call_to(&addr, name, &input)?;
                return Ok(if ok { Ok(v) } else { Err(v) });
            }
            client::Probe::Refused(addr, why) => bail!("a daemon serves this checkout at {addr} and refused this command: {why}"),
            client::Probe::None => {}
        }
    }
    let server = team.server()?;
    let mut opened = open(team, &server, false)?;
    let today = store_io::today();
    let result = if grimoire_core::tools::is_write(name) {
        if opened.lock.is_none() {
            bail!("another process holds this checkout's copy at {}; run the command again once it has finished", team.copy.display());
        }
        // A close names only this ticket's own work, told from the base's code (req:tool-untouched).
        if matches!(name, "close_ticket" | "bottle_ticket") {
            if let Err(e) = grimoire_server::base::give(&mut opened.loaded.state, &opened.loaded.repo) {
                eprintln!("warning: {e}");
            }
        }
        remote::write_through(&server, &team.copy, &mut opened.loaded.state, name, input, &today).map(|(output, _)| output)
    } else {
        // A lint is the check's answer, judged by this checkout's own work (req:check-own-work).
        if name == "lint" {
            if let Err(e) = grimoire_server::base::give(&mut opened.loaded.state, &opened.loaded.repo) {
                eprintln!("warning: {e}");
            }
        }
        grimoire_core::tools::call(&mut opened.loaded.state, name, input, &today).map(|out| out.output)
    };
    match result {
        Ok(output) => Ok(Ok(output)),
        Err(e) => Ok(Err(serde_json::to_value(&e)?)),
    }
}

/// The server's intent as it is now, fetched into memory and joined with this
/// checkout's code, never the checkout's copy, which a daemon beside it may
/// not have synced yet; with the digest that names the intent read.
pub fn read_now(team: &Team, server: &Server) -> anyhow::Result<(Loaded, String)> {
    let exported = server.export()?;
    let digest = remote::intent_digest(&exported);
    let repository = store_io::repository_string(&team.repo)?;
    let store = exported.joined(&grimoire_core::Store::default(), &repository).map_err(anyhow::Error::msg)?;
    let mut loaded = grimoire_server::load_store(store, Vec::new())?;
    loaded.state.set_server(Some(ServerCopy { address: team.address.clone(), fetched: None, reachable: true, refused: false }));
    Ok((loaded, digest))
}

/// How many times the main line reads the intent again when it moved under a prune.
const PRUNE_TRIES: u32 = 3;

/// What the main line did: the intent and code it judged, the one its prune
/// was sent against, so the check reports what was pruned from; and whether
/// the prune was taken, with how many entries went.
pub struct MainLine {
    pub loaded: Loaded,
    pub pruned: anyhow::Result<u64>,
}

/// One try at a prune: done, or refused because the intent moved under it.
enum Attempt<T> {
    Done(T),
    Moved,
}

/// Try `attempt` until it is done, at most `tries` times, trying again only
/// when the intent moved under it; any error ends it at once.
fn retrying<T>(tries: u32, pause: std::time::Duration, mut attempt: impl FnMut() -> anyhow::Result<Attempt<T>>) -> anyhow::Result<T> {
    for n in 1..=tries {
        match attempt()? {
            Attempt::Done(t) => return Ok(t),
            Attempt::Moved if n < tries => std::thread::sleep(pause),
            Attempt::Moved => {}
        }
    }
    bail!("the baseline was not pruned: the intent moved under it {tries} times running; run it again")
}

/// Prune the server's baseline to what this checkout's code produces against
/// the server's truth as it is now: the main line telling a server that holds
/// no code what is fixed. The prune names the intent it was judged against
/// and the server refuses it when that intent has moved, so it is read and
/// sent again, a few times. Refused, sending nothing, by a build that cannot
/// read one of the project's languages, since every finding in it would look
/// fixed. Fails outright only when the intent cannot be read at all.
pub fn prune_from_main_line(team: &Team, server: &Server) -> anyhow::Result<MainLine> {
    retrying(PRUNE_TRIES, std::time::Duration::from_millis(300), || {
        let (mut loaded, intent) = read_now(team, server).map_err(|e| anyhow::anyhow!("the baseline was not pruned: {e}"))?;
        let findings = grimoire_core::lint::lint(loaded.state.graph(), &grimoire_core::lint::Rule::ALL);
        if let Some(f) = findings.iter().find(|f| f.check == "unsupported-language") {
            let refused = anyhow::anyhow!("the baseline was not pruned: {}, so every finding in that language would look fixed. Run this with a build that reads it", f.reason);
            return Ok(Attempt::Done(MainLine { loaded, pruned: Err(refused) }));
        }
        let present: Vec<_> = findings.iter().map(grimoire_core::lint::Finding::key).collect();
        let pruned = match server.call("prune", &json!({ "present": present, "intent": intent })) {
            Ok(Ok(answer)) => Ok(answer["pruned"].as_u64().unwrap_or(0)),
            Ok(Err(refused)) if moved(&refused) => return Ok(Attempt::Moved),
            Ok(Err(refused)) => Err(anyhow::anyhow!("the server at {} refused the prune: {}", team.address, refused["message"].as_str().unwrap_or("no reason given"))),
            Err(e) => Err(anyhow::anyhow!("the baseline was not pruned: {e}")),
        };
        Ok(Attempt::Done(MainLine { loaded, pruned }))
    })
}

/// Send the server the recipes the main line's code reads, as `loaded` holds them, so a team's
/// site, served by a server that holds no code, shows them (req:recipe-main-line-snapshot): never
/// their files. Answers how many the server now holds, or why it did not take them.
pub fn send_recipes(team: &Team, server: &Server, loaded: &mut Loaded) -> anyhow::Result<u64> {
    use grimoire_core::graph::Node;
    use grimoire_core::model::{MainLineRecipes, SentRecipe};
    let recipes: Vec<SentRecipe> = loaded
        .state
        .graph()
        .nodes
        .values()
        .filter_map(|n| match n {
            Node::Recipe(r) => Some(SentRecipe::from(r)),
            _ => None,
        })
        .collect();
    match server.call("recipes", &serde_json::to_value(MainLineRecipes { recipes })?) {
        Ok(Ok(answer)) => Ok(answer["recipes"].as_u64().unwrap_or(0)),
        Ok(Err(refused)) => bail!("the server at {} did not take the main line's recipes: {}", team.address, refused["message"].as_str().unwrap_or("no reason given")),
        Err(e) => bail!("the main line's recipes were not sent: {e}"),
    }
}

/// Whether a server refused a prune because its intent moved since it was read.
fn moved(refused: &Value) -> bool {
    refused["error"] == "conflict" && refused["message"].as_str().is_some_and(|m| m.contains("intent moved"))
}

/// The checkout a path is in, for messages.
pub fn says(team: &Team) -> String {
    format!("this project's grimoire is the server's at {}", team.address)
}

/// Whether the project at `repo` names a server.
pub fn names_a_server(repo: &Path) -> bool {
    matches!(store_io::find_home(repo), Ok((_, Home::Server { .. })))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// # Test contract
    ///
    /// ```json
    /// {"test": "a-prune-is-tried-again-only-when-the-intent-moved", "checks": ["crates/cli/src/team#prune-retried-when-intent-moved"]}
    /// ```
    #[test]
    fn a_prune_is_tried_again_only_when_the_intent_moved() {
        let none = std::time::Duration::ZERO;
        // Moved twice, then taken: three tries, and the answer of the last.
        let mut calls = 0;
        let taken = retrying(3, none, || {
            calls += 1;
            Ok(if calls < 3 { Attempt::Moved } else { Attempt::Done(calls) })
        });
        assert_eq!((taken.unwrap(), calls), (3, 3));
        // Moved every time: it stops at the limit and says so.
        let mut calls = 0;
        let gave_up = retrying::<()>(3, none, || {
            calls += 1;
            Ok(Attempt::Moved)
        });
        assert!(gave_up.unwrap_err().to_string().contains("moved under it 3 times"));
        assert_eq!(calls, 3);
        // Any other failure ends it at once.
        let mut calls = 0;
        let failed = retrying::<()>(3, none, || {
            calls += 1;
            bail!("unreachable")
        });
        assert!(failed.is_err() && calls == 1);
        // Only the server's own word that the intent moved counts as moved.
        assert!(moved(&json!({ "error": "conflict", "message": "the intent moved since the main line read it: read it again and send the prune again" })));
        assert!(!moved(&json!({ "error": "conflict", "message": "only a shared daemon takes a prune from the main line" })));
        assert!(!moved(&json!({ "error": "invalid", "message": "the intent moved" })));
    }

    /// One request read whole from `stream`: its head and body as text.
    fn read_request(stream: &mut std::net::TcpStream) -> String {
        use std::io::Read;
        let mut got = Vec::new();
        let mut chunk = [0u8; 4096];
        loop {
            let n = stream.read(&mut chunk).unwrap_or(0);
            got.extend_from_slice(&chunk[..n]);
            let text = String::from_utf8_lossy(&got).to_string();
            let Some((head, body)) = text.split_once("\r\n\r\n") else {
                if n == 0 {
                    return text;
                }
                continue;
            };
            let wanted: usize = head.lines().find_map(|l| l.to_ascii_lowercase().strip_prefix("content-length:").and_then(|v| v.trim().parse().ok())).unwrap_or(0);
            if body.len() >= wanted || n == 0 {
                return text;
            }
        }
    }

    /// A server that answers the intent it is asked for and, to a prune, answers `prune` as a status
    /// and body, or nothing at all: the connection closed. Its address.
    fn serving(prune: Option<(u16, &'static str)>) -> String {
        use std::io::Write;
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let address = format!("http://{}", listener.local_addr().unwrap());
        let intent = serde_json::to_string(&grimoire_core::export::exported(&grimoire_core::Store::default())).unwrap();
        std::thread::spawn(move || {
            for mut stream in listener.incoming().flatten() {
                let head = read_request(&mut stream);
                let (status, body) = if head.starts_with("GET /api/export") {
                    (200, intent.clone())
                } else if let Some((status, body)) = prune {
                    (status, body.to_string())
                } else {
                    continue; // asked to prune, it says nothing: the stream is dropped
                };
                let reply = format!("HTTP/1.1 {status} X\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}", body.len());
                let _ = stream.write_all(reply.as_bytes());
            }
        });
        address
    }

    /// # Test contract
    ///
    /// ```json
    /// {"test": "a-prune-the-server-does-not-take-says-why", "checks": ["crates/cli/src/team#prune-refused-says-why"]}
    /// ```
    #[test]
    fn a_prune_the_server_does_not_take_says_why() {
        let repo = tempfile::tempdir().unwrap();
        let try_prune = |address: String| {
            let team = Team { address: address.clone(), repo: repo.path().to_path_buf(), copy: repo.path().join("copy") };
            prune_from_main_line(&team, &Server::new(&address, Some("t".into()))).map(|line| line.pruned)
        };
        // Refused otherwise than as the intent having moved: the reason it gave.
        let refused = try_prune(serving(Some((409, r#"{"error":"conflict","message":"only a shared daemon takes a prune from the main line"}"#)))).expect("a refusal is answered, not failed");
        let said = refused.unwrap_err().to_string();
        assert!(said.contains("refused the prune") && said.contains("only a shared daemon takes a prune"), "{said}");
        // The server does not answer the prune: why, not a failure of the whole.
        let silent = try_prune(serving(None)).expect("a server that does not answer is answered, not failed");
        let said = silent.unwrap_err().to_string();
        assert!(said.starts_with("the baseline was not pruned: "), "{said}");
    }
}
