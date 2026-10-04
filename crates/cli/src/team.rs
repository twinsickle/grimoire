//! # Module contract
//!
//! ```json
//! {"purpose":"What a command does on a project that names a team's server: sign a person in and out, open the checkout's copy of the server's intent with the checkout's code, and run a tool through the server.","criteria":[{"id":"login","statement":"login takes a token from --token, else from standard input, for the server the project names or the one given with --server, asks that server with it before keeping it, and keeps it in the person's credentials file under the server's address; a token the server refuses, a server that does not answer or an empty token keeps nothing and fails; logout removes the kept token and says so when there was none."},{"id":"opens-the-copy","statement":"A command on a project that names a server, with no daemon serving the checkout's copy, takes the copy's lock when it can and syncs the copy from the server first; when the server does not answer and the checkout holds a copy it reads the copy, saying on standard error that the server was not reached and when the copy was fetched, except a check that names a ticket, which fails rather than read a copy it could not fetch now, or one a daemon beside it cannot say is current; with no copy, or a token the server refuses, it fails; when the daemon serving the checkout holds the lock, the command asks that daemon what it knows of the server and says the same, failing when the server refused the token; it never prunes the baseline."},{"id":"through-the-server","statement":"tool, adopt and land on a project that names a server run on the daemon serving the checkout's copy when there is one; otherwise a read tool answers over the copy with the checkout's code, and a write tool goes through the server under the copy's lock, refused when another process holds the lock, and refused as store_unavailable when the server does not answer."}]}
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

/// The server a sign-in is for: the one given, else the one the project names.
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

/// Keep a person's token for a server, once the server has taken it.
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

/// Forget the token kept for a server.
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
        let answered = client::call_at(addr, "status", &json!({}));
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
                let (ok, v) = client::call_at(addr, name, &input)?;
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
        remote::write_through(&server, &team.copy, &mut opened.loaded.state, name, input, &today).map(|(output, _)| output)
    } else {
        grimoire_core::tools::call(&mut opened.loaded.state, name, input, &today).map(|out| out.output)
    };
    match result {
        Ok(output) => Ok(Ok(output)),
        Err(e) => Ok(Err(serde_json::to_value(&e)?)),
    }
}

/// The checkout a path is in, for messages.
pub fn says(team: &Team) -> String {
    format!("this project's grimoire is the server's at {}", team.address)
}

/// Whether the project at `repo` names a server.
pub fn names_a_server(repo: &Path) -> bool {
    matches!(store_io::find_home(repo), Ok((_, Home::Server { .. })))
}
