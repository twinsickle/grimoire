//! # Module contract
//!
//! ```json
//! {"purpose":"Read a store directory into a value and write single records back atomically; where a store lives and its write lock each have a module beneath.","criteria":[{"id":"atomic-writes","statement":"A record is written to a temporary file in the same directory and renamed over the old one in one step: a write that finishes or fails leaves no temporary file behind, and a reader running while records are rewritten always finds the record, whole."},{"id":"malformed-collected","statement":"A file that does not parse or violates a field rule is reported as malformed with its path and reason, and the rest of the store still loads."},{"id":"bom-tolerated","statement":"A store file that begins with a UTF-8 byte-order mark loads as if it did not, since hand edits from Windows editors leave one."},{"id":"layout","statement":"A store is a directory holding grimoire.json and pages/, requirements/ and tickets/ with one <slug>.json per record, plus baseline.json once adopted, serve.json while a daemon serves it, starts/<slug>.json and proposals/<slug>.json for each active or landing ticket, current.json naming the current ticket while there is one, tokens.json for a shared daemon's tokens and tokens.lock, kept by the tokens module beneath, and lock, the empty file a writer locks; a record file whose name differs from its record's slug is malformed, and so is a baseline, start, proposal or current that does not parse, or a proposal or current naming a ticket that is neither active nor landing; a deleted ticket's start and proposal files go with its record."},{"id":"migrates","statement":"A version 1 store, written before the tools trimmed, is read with every string trimmed, so nothing in it is lost; the first writer to take its lock trims every record file as JSON, rewriting those that change, including one that does not load for another reason, and sets version 2 last; in a version 2 store an untrimmed string is malformed."},{"id":"whole-store-valid","statement":"write_all writes nothing, and replaces nothing, unless the store is at the current version and its config, every record, every start, every proposal, the current ticket and its baseline would load back, each held under its own slug and a proposal or current naming an active or landing ticket; replacing removes the old store's records, starts, proposals, current ticket and baseline, and those given are written; the tokens file is not what the store says but who may sign in to it, and is left as it is."}]}
//! ```

use std::path::{Path, PathBuf};

use grimoire_core::model::*;
use grimoire_core::ops::Changed;
use grimoire_core::store::{validate_baseline, validate_config, validate_page, validate_proposal, validate_requirement, validate_start, validate_ticket};
use grimoire_core::{Malformed, Store};

mod location;
mod lock;
pub mod tokens;

pub use location::{copies_dir, copy_dir, credentials_file, default_name, find_home, find_named, find_project, project_at, refuse_inside, repository_string, server_address, slashed, store_for_repo, stores_dir, write_project_file, write_project_file_for_server, Home, HOME_ENV, PROJECT_FILE};
pub use lock::{lock, lock_new, StoreLock, LOCK_FILE};

#[derive(Debug, thiserror::Error)]
pub enum StoreIoError {
    #[error("{0}: {1}")]
    Io(PathBuf, std::io::Error),
    #[error("{0} is not a store: grimoire.json is missing")]
    NotAStore(PathBuf),
    #[error("{0}: grimoire.json is invalid: {1}")]
    BadConfig(PathBuf, String),
    #[error("no .grimoire.toml found in {0} or any parent directory")]
    NoProjectFile(PathBuf),
    #[error("{0} is invalid: {1}")]
    BadProjectFile(PathBuf, String),
    #[error("no platform data directory is available to hold stores")]
    NoDataDir,
    #[error("{0} already exists")]
    Exists(PathBuf),
    #[error("{0} would not load back, so nothing was written: {1}")]
    Invalid(String, String),
    #[error("{0} is invalid: {1}")]
    BadTokens(PathBuf, String),
    #[error("store {0} is in use by {1}; it has one writer at a time")]
    Locked(PathBuf, String),
    #[error("store {0} is inside the repository {1}; a store lives outside the repository it describes")]
    InsideRepository(PathBuf, PathBuf),
}

pub const CONFIG_FILE: &str = "grimoire.json";
pub const BASELINE_FILE: &str = "baseline.json";
pub const SERVE_FILE: &str = "serve.json";
pub const STARTS_DIR: &str = "starts";
pub const PROPOSALS_DIR: &str = "proposals";
pub const CURRENT_FILE: &str = "current.json";

/// `serve.json`: which daemon serves this store. A hint, never trusted
/// without asking the daemon itself (see `client::serving`).
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct Serving {
    pub port: u16,
    pub pid: u32,
    /// Where a command on this machine reaches it, when that is not 127.0.0.1: the
    /// address it bound, or `::1` when it listens on every IPv6 address.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub host: Option<String>,
    /// Where in the daemon's library this store's grimoire is served, `/g/<name>`; none
    /// for a daemon serving it on the bare routes (req:machine-library).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub path: Option<String>,
}

impl Serving {
    /// Where a command on this machine reaches the daemon: the address it
    /// bound, or the loopback when it listens there or on every address.
    pub fn addr(&self) -> std::net::SocketAddr {
        let ip = self.host.as_deref().and_then(|h| h.parse().ok()).unwrap_or(std::net::IpAddr::V4(std::net::Ipv4Addr::LOCALHOST));
        std::net::SocketAddr::new(ip, self.port)
    }
}

/// Record that this process serves the store on `port` of this machine's loopback.
pub fn write_serving(store_dir: &Path, port: u16) -> Result<(), StoreIoError> {
    write_serving_at(store_dir, std::net::SocketAddr::from(([127, 0, 0, 1], port)))
}

/// Record that this process serves the store at `addr`.
pub fn write_serving_at(store_dir: &Path, addr: std::net::SocketAddr) -> Result<(), StoreIoError> {
    write_serving_in(store_dir, addr, None)
}

/// Record that this process serves the store at `addr`, its grimoire at `path` in the library.
pub fn write_serving_in(store_dir: &Path, addr: std::net::SocketAddr, path: Option<&str>) -> Result<(), StoreIoError> {
    let ip = addr.ip();
    let host = if ip.is_unspecified() {
        // Every address: reached on this machine by the loopback of the same family.
        ip.is_ipv6().then(|| std::net::Ipv6Addr::LOCALHOST.to_string())
    } else {
        (ip != std::net::IpAddr::V4(std::net::Ipv4Addr::LOCALHOST)).then(|| ip.to_string())
    };
    write_atomic(&store_dir.join(SERVE_FILE), &to_pretty(&Serving { port: addr.port(), pid: std::process::id(), host, path: path.map(str::to_string) }))
}

/// Remove the marker, if it is still this process's.
pub fn remove_serving(store_dir: &Path) {
    if read_serving(store_dir).is_some_and(|s| s.pid == std::process::id()) {
        let _ = std::fs::remove_file(store_dir.join(SERVE_FILE));
    }
}

pub fn read_serving(store_dir: &Path) -> Option<Serving> {
    read_json(&store_dir.join(SERVE_FILE)).ok()
}
fn dir_of(kind: Kind) -> &'static str {
    match kind {
        Kind::Page => "pages",
        Kind::Req => "requirements",
        Kind::Ticket => "tickets",
        _ => unreachable!("only store kinds are persisted"),
    }
}

fn read_json<T: for<'de> serde::Deserialize<'de>>(path: &Path) -> Result<T, String> {
    read_record(path, false)
}

/// A store file as a record, its strings trimmed first when `trim` is set,
/// as they are for a version 1 store, written before the tools trimmed.
fn read_record<T: for<'de> serde::Deserialize<'de>>(path: &Path, trim: bool) -> Result<T, String> {
    let text = std::fs::read_to_string(path).map_err(|e| e.to_string())?;
    // A hand edit from a Windows editor may leave a byte-order mark.
    let mut value: serde_json::Value = serde_json::from_str(text.trim_start_matches('\u{feff}')).map_err(|e| e.to_string())?;
    if trim {
        grimoire_core::store::trim_strings(&mut value);
    }
    serde_json::from_value(value).map_err(|e| e.to_string())
}

/// The store's config, read the way its version says: a version 1 config
/// with its strings trimmed. Validated, so a store that will not load is
/// refused here too.
fn read_config(store_dir: &Path) -> Result<StoreConfig, StoreIoError> {
    let config_path = store_dir.join(CONFIG_FILE);
    if !config_path.is_file() {
        return Err(StoreIoError::NotAStore(store_dir.to_path_buf()));
    }
    let raw: serde_json::Value = read_json(&config_path).map_err(|e| StoreIoError::BadConfig(store_dir.to_path_buf(), e))?;
    let v1 = raw.get("version").and_then(serde_json::Value::as_u64) == Some(u64::from(StoreConfig::TRIMMED_ON_READ));
    let config: StoreConfig = read_record(&config_path, v1).map_err(|e| StoreIoError::BadConfig(store_dir.to_path_buf(), e))?;
    let errors = validate_config(&config);
    if !errors.is_empty() {
        let msg = errors.iter().map(|e| format!("{}: {}", e.field, e.reason)).collect::<Vec<_>>().join("; ");
        return Err(StoreIoError::BadConfig(store_dir.to_path_buf(), msg));
    }
    Ok(config)
}

fn rel(store: &Path, path: &Path) -> String {
    path.strip_prefix(store).unwrap_or(path).to_string_lossy().replace('\\', "/")
}

/// Load a store directory. Malformed files are collected, not fatal; a
/// missing or unreadable `grimoire.json` is.
pub fn load(store_dir: &Path) -> Result<(Store, Vec<Malformed>), StoreIoError> {
    let config = read_config(store_dir)?;
    // A version 1 store is read with its strings trimmed; its first writer rewrites it as version 2.
    let trim = config.version == StoreConfig::TRIMMED_ON_READ;
    let mut store = Store::new(config);
    let mut malformed = Vec::new();

    for kind in [Kind::Page, Kind::Req, Kind::Ticket] {
        let dir = store_dir.join(dir_of(kind));
        let Ok(entries) = std::fs::read_dir(&dir) else { continue };
        let mut paths: Vec<PathBuf> = entries.flatten().map(|e| e.path()).filter(|p| p.extension().is_some_and(|x| x == "json")).collect();
        paths.sort();
        for path in paths {
            let relp = rel(store_dir, &path);
            let stem = path.file_stem().map(|s| s.to_string_lossy().to_string()).unwrap_or_default();
            let result: Result<(), String> = match kind {
                Kind::Page => read_record::<Page>(&path, trim).and_then(|p| check(&p.slug, &stem, validate_page(&p)).map(|_| store.pages.insert(p.slug.clone(), p)).map(|_| ())),
                Kind::Req => read_record::<Requirement>(&path, trim).and_then(|r| check(&r.slug, &stem, validate_requirement(&r)).map(|_| store.requirements.insert(r.slug.clone(), r)).map(|_| ())),
                Kind::Ticket => read_record::<Ticket>(&path, trim).and_then(|t| check(&t.slug, &stem, validate_ticket(&t)).map(|_| store.tickets.insert(t.slug.clone(), t)).map(|_| ())),
                _ => unreachable!(),
            };
            if let Err(reason) = result {
                malformed.push(Malformed { path: relp, reason });
            }
        }
    }
    if let Ok(entries) = std::fs::read_dir(store_dir.join(STARTS_DIR)) {
        let mut paths: Vec<PathBuf> = entries.flatten().map(|e| e.path()).filter(|p| p.extension().is_some_and(|x| x == "json")).collect();
        paths.sort();
        for path in paths {
            let stem = path.file_stem().map(|s| s.to_string_lossy().to_string()).unwrap_or_default();
            match read_record::<Start>(&path, trim) {
                Ok(s) if s.ticket == stem && validate_start(&s).is_empty() => {
                    store.starts.insert(stem, s);
                }
                Ok(s) if s.ticket == stem => malformed.push(Malformed { path: rel(store_dir, &path), reason: validate_start(&s).iter().map(|e| format!("{}: {}", e.field, e.reason)).collect::<Vec<_>>().join("; ") }),
                Ok(s) => malformed.push(Malformed { path: rel(store_dir, &path), reason: format!("file is named `{stem}` but its ticket is `{}`", s.ticket) }),
                Err(reason) => malformed.push(Malformed { path: rel(store_dir, &path), reason }),
            }
        }
    }
    if let Ok(entries) = std::fs::read_dir(store_dir.join(PROPOSALS_DIR)) {
        let mut paths: Vec<PathBuf> = entries.flatten().map(|e| e.path()).filter(|p| p.extension().is_some_and(|x| x == "json")).collect();
        paths.sort();
        for path in paths {
            let stem = path.file_stem().map(|s| s.to_string_lossy().to_string()).unwrap_or_default();
            match read_record::<Proposal>(&path, trim) {
                Ok(p) if p.ticket != stem => malformed.push(Malformed { path: rel(store_dir, &path), reason: format!("file is named `{stem}` but its ticket is `{}`", p.ticket) }),
                Ok(p) if !validate_proposal(&p).is_empty() => malformed.push(Malformed { path: rel(store_dir, &path), reason: validate_proposal(&p).iter().map(|e| format!("{}: {}", e.field, e.reason)).collect::<Vec<_>>().join("; ") }),
                // A proposal belongs to an active or landing ticket; one left behind by a ticket that is gone or closed is not read as intent.
                Ok(p) if !active(&store, &p.ticket) => malformed.push(Malformed { path: rel(store_dir, &path), reason: format!("names ticket `{}`, which is neither active nor landing", p.ticket) }),
                Ok(p) => {
                    store.proposals.insert(stem, p);
                }
                Err(reason) => malformed.push(Malformed { path: rel(store_dir, &path), reason }),
            }
        }
    }
    let current_path = store_dir.join(CURRENT_FILE);
    if current_path.is_file() {
        match read_record::<Current>(&current_path, trim) {
            Ok(c) if !grimoire_core::store::is_slug(&c.ticket) => malformed.push(Malformed { path: CURRENT_FILE.to_string(), reason: format!("`{}` is not a ticket slug", c.ticket) }),
            Ok(c) if !active(&store, &c.ticket) => malformed.push(Malformed { path: CURRENT_FILE.to_string(), reason: format!("names ticket `{}`, which is neither active nor landing", c.ticket) }),
            Ok(c) => store.current = Some(c.ticket),
            Err(reason) => malformed.push(Malformed { path: CURRENT_FILE.to_string(), reason }),
        }
    }
    let baseline_path = store_dir.join(BASELINE_FILE);
    if baseline_path.is_file() {
        let result = read_record::<Baseline>(&baseline_path, trim).and_then(|b| {
            let errors = validate_baseline(&b);
            if errors.is_empty() {
                Ok(b)
            } else {
                Err(errors.iter().map(|e| format!("{}: {}", e.field, e.reason)).collect::<Vec<_>>().join("; "))
            }
        });
        match result {
            Ok(b) => store.baseline = Some(b),
            Err(reason) => malformed.push(Malformed { path: BASELINE_FILE.to_string(), reason }),
        }
    }
    Ok((store, malformed))
}

/// Whether the store holds `ticket` as a ticket being worked, active or a change waiting to land: what a proposal and the current ticket must name.
fn active(store: &Store, ticket: &str) -> bool {
    store.tickets.get(ticket).is_some_and(Ticket::is_worked)
}

/// Rewrite a version 1 store as version 2, for its first writer, which holds
/// its lock: every record whose stored form differs from the record read with
/// its strings trimmed is written again, and the config last, so a failure
/// part way leaves a version 1 store the next writer migrates again. A record
/// that does not load is left as it is, still reported. Returns how many
/// records were rewritten; a store already at version 2 is left alone.
fn migrate(store_dir: &Path) -> Result<usize, StoreIoError> {
    let mut config = read_config(store_dir)?;
    if config.version != StoreConfig::TRIMMED_ON_READ {
        return Ok(0);
    }
    // Every record file, whether or not it loads for some other reason, trimmed as JSON: what
    // a version 1 store was read as is what it now holds, and a record malformed for another
    // reason, once fixed, has no stray whitespace left to fail on.
    let mut files: Vec<PathBuf> = [BASELINE_FILE].iter().map(|f| store_dir.join(f)).filter(|p| p.is_file()).collect();
    for dir in ["pages", "requirements", "tickets", STARTS_DIR, PROPOSALS_DIR] {
        if let Ok(entries) = std::fs::read_dir(store_dir.join(dir)) {
            files.extend(entries.flatten().map(|e| e.path()).filter(|p| p.extension().is_some_and(|x| x == "json")));
        }
    }
    files.sort();
    let mut rewritten = 0;
    for path in files {
        let Ok(text) = std::fs::read_to_string(&path) else { continue };
        let Ok(on_disk) = serde_json::from_str::<serde_json::Value>(text.trim_start_matches('\u{feff}')) else { continue };
        let mut trimmed = on_disk.clone();
        grimoire_core::store::trim_strings(&mut trimmed);
        if trimmed != on_disk {
            write_atomic(&path, &to_pretty(&trimmed))?;
            rewritten += 1;
        }
    }
    config.version = StoreConfig::CURRENT_VERSION;
    write_atomic(&store_dir.join(CONFIG_FILE), &to_pretty(&config))?;
    Ok(rewritten)
}

fn check(slug: &str, stem: &str, errors: Vec<grimoire_core::store::FieldError>) -> Result<(), String> {
    if slug != stem {
        return Err(format!("file is named `{stem}` but its slug is `{slug}`"));
    }
    if errors.is_empty() {
        Ok(())
    } else {
        Err(errors.iter().map(|e| format!("{}: {}", e.field, e.reason)).collect::<Vec<_>>().join("; "))
    }
}

fn write_atomic(path: &Path, text: &str) -> Result<(), StoreIoError> {
    let dir = path.parent().expect("record has a parent directory");
    std::fs::create_dir_all(dir).map_err(|e| StoreIoError::Io(dir.to_path_buf(), e))?;
    let tmp = dir.join(format!(".{}.tmp-{}", path.file_name().unwrap_or_default().to_string_lossy(), std::process::id()));
    // A failed write or rename takes its temporary file with it.
    std::fs::write(&tmp, text).map_err(|e| {
        let _ = std::fs::remove_file(&tmp);
        StoreIoError::Io(tmp.clone(), e)
    })?;
    // Replaces the old record in one step on every platform (MoveFileExW with
    // MOVEFILE_REPLACE_EXISTING on Windows), so there is never a moment with none.
    std::fs::rename(&tmp, path).map_err(|e| {
        let _ = std::fs::remove_file(&tmp);
        StoreIoError::Io(path.to_path_buf(), e)
    })
}

fn to_pretty<T: serde::Serialize>(v: &T) -> String {
    let mut s = serde_json::to_string_pretty(v).expect("serialisable");
    s.push('\n');
    s
}

/// Persist what a write changed.
pub fn persist(store_dir: &Path, store: &Store, changed: &Changed) -> Result<(), StoreIoError> {
    match changed {
        Changed::Page(slug) => {
            let p = store.pages.get(slug).expect("changed page exists");
            write_atomic(&store_dir.join("pages").join(format!("{slug}.json")), &to_pretty(p))
        }
        Changed::Requirement(slug) => {
            let r = store.requirements.get(slug).expect("changed requirement exists");
            write_atomic(&store_dir.join("requirements").join(format!("{slug}.json")), &to_pretty(r))
        }
        Changed::Ticket(slug) => {
            let t = store.tickets.get(slug).expect("changed ticket exists");
            write_atomic(&store_dir.join("tickets").join(format!("{slug}.json")), &to_pretty(t))
        }
        Changed::Config => write_atomic(&store_dir.join(CONFIG_FILE), &to_pretty(&store.config)),
        Changed::Many(changes) => {
            for c in changes {
                persist(store_dir, store, c)?;
            }
            Ok(())
        }
        Changed::Started(slug) => {
            persist(store_dir, store, &Changed::Ticket(slug.clone()))?;
            let s = store.starts.get(slug).expect("a started ticket has a start");
            write_atomic(&store_dir.join(STARTS_DIR).join(format!("{slug}.json")), &to_pretty(s))
        }
        Changed::Closed(slug) => {
            persist(store_dir, store, &Changed::Ticket(slug.clone()))?;
            remove_if_present(&store_dir.join(STARTS_DIR).join(format!("{slug}.json")))
        }
        Changed::Proposed(slug) => match store.proposals.get(slug) {
            Some(p) => write_atomic(&store_dir.join(PROPOSALS_DIR).join(format!("{slug}.json")), &to_pretty(p)),
            None => remove_if_present(&store_dir.join(PROPOSALS_DIR).join(format!("{slug}.json"))),
        },
        Changed::Current => match &store.current {
            Some(t) => write_atomic(&store_dir.join(CURRENT_FILE), &to_pretty(&Current { ticket: t.clone() })),
            None => remove_if_present(&store_dir.join(CURRENT_FILE)),
        },
        Changed::Baseline => match &store.baseline {
            Some(b) => write_atomic(&store_dir.join(BASELINE_FILE), &to_pretty(b)),
            None => remove_if_present(&store_dir.join(BASELINE_FILE)),
        },
        Changed::Deleted(r) => {
            // A deleted ticket's start and proposal go with its record.
            if r.kind == Kind::Ticket {
                remove_if_present(&store_dir.join(STARTS_DIR).join(format!("{}.json", r.id)))?;
                remove_if_present(&store_dir.join(PROPOSALS_DIR).join(format!("{}.json", r.id)))?;
            }
            remove_if_present(&store_dir.join(dir_of(r.kind)).join(format!("{}.json", r.id)))
        }
    }
}

fn remove_if_present(path: &Path) -> Result<(), StoreIoError> {
    match std::fs::remove_file(path) {
        Ok(()) => Ok(()),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(e) => Err(StoreIoError::Io(path.to_path_buf(), e)),
    }
}

/// Write a whole store to an empty or new directory. Refuses a directory
/// that already holds a store unless `replace` is set, in which case the
/// existing records are removed first.
pub fn write_all(store_dir: &Path, store: &Store, replace: bool) -> Result<(), StoreIoError> {
    // Nothing is written, nor anything replaced, unless every record would load back.
    let invalid = |what: String, errors: Vec<grimoire_core::store::FieldError>| match errors.as_slice() {
        [] => Ok(()),
        _ => Err(StoreIoError::Invalid(what, errors.iter().map(|e| format!("{}: {}", e.field, e.reason)).collect::<Vec<_>>().join("; "))),
    };
    invalid("grimoire.json".into(), validate_config(&store.config))?;
    if store.config.version != StoreConfig::CURRENT_VERSION {
        return Err(StoreIoError::Invalid("grimoire.json".into(), format!("a store is written at version {}; version {} is only read", StoreConfig::CURRENT_VERSION, store.config.version)));
    }
    // A record filed under another key than its slug would be written over another, or under a name it does not load from.
    let keyed = |what: &str, key: &str, slug: &str| match key == slug {
        true => Ok(()),
        false => Err(StoreIoError::Invalid(format!("{what} {key}"), format!("is held under `{key}` but its slug is `{slug}`"))),
    };
    for (k, p) in &store.pages {
        keyed("page", k, &p.slug)?;
        invalid(format!("page {}", p.slug), validate_page(p))?;
    }
    for (k, r) in &store.requirements {
        keyed("requirement", k, &r.slug)?;
        invalid(format!("requirement {}", r.slug), validate_requirement(r))?;
    }
    for (k, t) in &store.tickets {
        keyed("ticket", k, &t.slug)?;
        invalid(format!("ticket {}", t.slug), validate_ticket(t))?;
    }
    for (k, s) in &store.starts {
        keyed("start", k, &s.ticket)?;
        invalid(format!("start {}", s.ticket), validate_start(s))?;
    }
    for (k, p) in &store.proposals {
        keyed("proposal", k, &p.ticket)?;
        invalid(format!("proposal {}", p.ticket), validate_proposal(p))?;
        if !active(store, &p.ticket) {
            return Err(StoreIoError::Invalid(format!("proposal {}", p.ticket), "names a ticket that is neither active nor landing".into()));
        }
    }
    if let Some(c) = &store.current {
        if !active(store, c) {
            return Err(StoreIoError::Invalid(CURRENT_FILE.into(), format!("names ticket `{c}`, which is neither active nor landing")));
        }
    }
    if let Some(b) = &store.baseline {
        invalid("baseline.json".into(), validate_baseline(b))?;
    }
    let config_path = store_dir.join(CONFIG_FILE);
    if config_path.exists() {
        if !replace {
            return Err(StoreIoError::Exists(store_dir.to_path_buf()));
        }
        // Replaced whole: the records, the starts, the proposals, the current ticket and the baseline the old store held go.
        for dir in [dir_of(Kind::Page), dir_of(Kind::Req), dir_of(Kind::Ticket), STARTS_DIR, PROPOSALS_DIR] {
            let dir = store_dir.join(dir);
            if dir.is_dir() {
                std::fs::remove_dir_all(&dir).map_err(|e| StoreIoError::Io(dir.clone(), e))?;
            }
        }
        remove_if_present(&store_dir.join(BASELINE_FILE))?;
        remove_if_present(&store_dir.join(CURRENT_FILE))?;
    }
    std::fs::create_dir_all(store_dir).map_err(|e| StoreIoError::Io(store_dir.to_path_buf(), e))?;
    write_atomic(&config_path, &to_pretty(&store.config))?;
    for kind in [Kind::Page, Kind::Req, Kind::Ticket] {
        std::fs::create_dir_all(store_dir.join(dir_of(kind))).map_err(|e| StoreIoError::Io(store_dir.to_path_buf(), e))?;
    }
    for p in store.pages.values() {
        persist(store_dir, store, &Changed::Page(p.slug.clone()))?;
    }
    for r in store.requirements.values() {
        persist(store_dir, store, &Changed::Requirement(r.slug.clone()))?;
    }
    for t in store.tickets.values() {
        persist(store_dir, store, &Changed::Ticket(t.slug.clone()))?;
    }
    for s in store.starts.values() {
        write_atomic(&store_dir.join(STARTS_DIR).join(format!("{}.json", s.ticket)), &to_pretty(s))?;
    }
    for p in store.proposals.values() {
        persist(store_dir, store, &Changed::Proposed(p.ticket.clone()))?;
    }
    if store.current.is_some() {
        persist(store_dir, store, &Changed::Current)?;
    }
    if store.baseline.is_some() {
        persist(store_dir, store, &Changed::Baseline)?;
    }
    Ok(())
}

/// Today as ISO `YYYY-MM-DD`, local time when the offset is known.
pub fn today() -> String {
    let now = time::OffsetDateTime::now_local().unwrap_or_else(|_| time::OffsetDateTime::now_utc());
    let d = now.date();
    format!("{:04}-{:02}-{:02}", d.year(), u8::from(d.month()), d.day())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A fresh folder, unique however many tests run at once, removed when dropped.
    pub(super) fn temp() -> tempfile::TempDir {
        tempfile::Builder::new().prefix("grimoire-store-io-").tempdir().unwrap()
    }

    /// # Test contract
    ///
    /// ```json
    /// {"test": "a-failed-write-leaves-no-temporary-file", "checks": ["crates/server/src/store_io#atomic-writes"]}
    /// ```
    #[test]
    fn a_failed_write_leaves_no_temporary_file() {
        let tmp = temp();
        let dir = tmp.path();
        // A folder where the record should be: the rename fails, and the temporary file goes with it.
        std::fs::create_dir_all(dir.join("pages").join("goal.json").join("x")).unwrap();
        assert!(write_atomic(&dir.join("pages").join("goal.json"), "{}").is_err());
        let left: Vec<_> = std::fs::read_dir(dir.join("pages")).unwrap().flatten().map(|e| e.file_name()).collect();
        assert_eq!(left, vec![std::ffi::OsString::from("goal.json")], "only the folder that was there");
    }

    /// # Test contract
    ///
    /// ```json
    /// {"test": "a-version-1-store-is-migrated-by-its-first-writer", "checks": ["crates/server/src/store_io#migrates", "req:store-field-rules"]}
    /// ```
    #[test]
    fn a_version_1_store_is_read_trimmed_and_migrated_by_its_first_writer() {
        let tmp = temp();
        let dir = tmp.path();
        let (store_dir, repo) = (dir.join("store"), dir.join("repo"));
        std::fs::create_dir_all(&repo).unwrap();
        // Written by an earlier build, whose tools stored strings as they were given.
        let write = |rel: &str, v: serde_json::Value| {
            let path = store_dir.join(rel);
            std::fs::create_dir_all(path.parent().unwrap()).unwrap();
            std::fs::write(path, serde_json::to_string_pretty(&v).unwrap()).unwrap();
        };
        write(CONFIG_FILE, serde_json::json!({"version": 1, "repository": repo.to_string_lossy(), "roots": ["goal"], "languages": ["rust"]}));
        write("pages/goal.json", serde_json::json!({"slug": "goal", "title": "Goal", "body": "What it is.\n", "links": []}));
        write("pages/plan.json", serde_json::json!({"slug": "plan", "title": "Plan", "body": "Steps.", "links": []}));
        write("tickets/0001.json", serde_json::json!({"slug": "0001", "title": "T ", "kind": "change", "priority": "next", "status": "open", "touches": [], "relies_on": [], "body": "b", "decision": null}));
        // A record the old build accepted that fails another rule now: it stays malformed, but loses its stray whitespace too.
        write("requirements/r.json", serde_json::json!({"slug": "r", "title": "R", "statement": "S\n", "scenarios": [], "covered_by": [], "decisions": [{"date": "2026-02-31", "text": "why"}]}));
        // Read: nothing lost, every string trimmed in memory.
        let (store, malformed) = load(&store_dir).unwrap();
        assert_eq!(malformed.len(), 1, "only the impossible date: {malformed:?}");
        assert_eq!((store.pages["goal"].body.as_str(), store.tickets["0001"].title.as_str()), ("What it is.", "T"));
        assert_eq!(store.config.version, StoreConfig::TRIMMED_ON_READ, "reading changes nothing on disk");
        // The first writer migrates: the two untrimmed records are rewritten, the other is not, and the config says version 2.
        let plan_before = std::fs::read(store_dir.join("pages").join("plan.json")).unwrap();
        drop(lock(&store_dir).unwrap());
        let (store, malformed) = load(&store_dir).unwrap();
        assert_eq!(malformed.len(), 1, "{malformed:?}");
        assert!(!malformed[0].reason.contains("whitespace"), "the malformed record was trimmed too: {malformed:?}");
        assert!(std::fs::read_to_string(store_dir.join("requirements").join("r.json")).unwrap().contains("\"S\""));
        assert_eq!(store.config.version, StoreConfig::CURRENT_VERSION);
        assert!(std::fs::read_to_string(store_dir.join("pages").join("goal.json")).unwrap().contains("\"What it is.\""));
        assert_eq!(std::fs::read(store_dir.join("pages").join("plan.json")).unwrap(), plan_before, "a record already trimmed is left alone");
        assert_eq!(migrate(&store_dir).unwrap(), 0, "a version 2 store is left alone");
        // From version 2 on, an untrimmed string on disk is a hand edit, and malformed.
        write("pages/goal.json", serde_json::json!({"slug": "goal", "title": " Goal", "body": "b", "links": []}));
        let (store, malformed) = load(&store_dir).unwrap();
        assert!(!store.pages.contains_key("goal"));
        assert!(malformed[0].reason.contains("title: must not begin or end with whitespace"), "{malformed:?}");
    }

    /// # Test contract
    ///
    /// ```json
    /// {"test": "a-whole-store-that-would-not-load-is-not-written", "checks": ["crates/server/src/store_io#whole-store-valid"]}
    /// ```
    #[test]
    fn a_whole_store_that_would_not_load_is_not_written() {
        let tmp = temp();
        let dir = tmp.path();
        let config = StoreConfig { site: vec![], version: StoreConfig::CURRENT_VERSION, repository: "C:/repo".into(), roots: vec!["goal".into()].into(), languages: vec!["rust ".into()], scope: Scope::default(), order: Default::default() };
        let store = Store::new(config);
        assert!(matches!(write_all(&dir.join("store"), &store, false), Err(StoreIoError::Invalid(..))));
        assert!(!dir.join("store").exists(), "nothing written");
        // Nor is an existing store replaced by one that would not load.
        let mut good = Store::new(StoreConfig { site: vec![], languages: vec!["rust".into()], ..store.config.clone() });
        good.pages.insert("goal".into(), Page { slug: "goal".into(), title: "Goal".into(), body: "b".into(), links: vec![] });
        write_all(&dir.join("store"), &good, false).unwrap();
        let mut bad = good.clone();
        bad.pages.insert("goal".into(), Page { slug: "goal".into(), title: "Goal ".into(), body: "b".into(), links: vec![] });
        assert!(matches!(write_all(&dir.join("store"), &bad, true), Err(StoreIoError::Invalid(..))));
        assert_eq!(load(&dir.join("store")).unwrap().0.pages["goal"].title, "Goal", "the store that was there is untouched");
        // A baseline, a start, a record under another key, and an old version are checked too.
        let refused = |store: &Store| matches!(write_all(&dir.join("store"), store, true), Err(StoreIoError::Invalid(..)));
        let mut b = good.clone();
        b.baseline = Some(Baseline { date: "2026-09-30".into(), reason: "adopted ".into(), findings: vec![] });
        assert!(refused(&b));
        let mut st = good.clone();
        st.starts.insert("0001".into(), Start { ticket: "0001".into(), fingerprints: [("page:goal".to_string(), " f".to_string())].into() });
        assert!(refused(&st));
        let mut k = good.clone();
        k.pages.insert("other".into(), good.pages["goal"].clone());
        assert!(refused(&k));
        let mut v1 = good.clone();
        v1.config.version = StoreConfig::TRIMMED_ON_READ;
        assert!(refused(&v1), "version 1 is only read, never written");
        // A proposal under another key, one that would not load, one naming a ticket that is not active, and a current ticket that is not active.
        let active = Ticket { slug: "0001".into(), title: "T".into(), kind: TicketKind::Change, priority: Priority::Later, status: Status::Active, touches: vec![], relies_on: vec![], body: "b".into(), decision: None };
        let proposal = |ticket: &str, title: &str| Proposal { ticket: ticket.into(), pages: [("goal".to_string(), Proposed { record: Page { slug: "goal".into(), title: title.into(), body: "b".into(), links: vec![] }, base: Some("f".into()) })].into(), requirements: Default::default(), deleted: Default::default() };
        let mut with_ticket = good.clone();
        with_ticket.tickets.insert("0001".into(), active.clone());
        let mut pk = with_ticket.clone();
        pk.proposals.insert("other".into(), proposal("0001", "Goal"));
        assert!(refused(&pk));
        let mut pv = with_ticket.clone();
        pv.proposals.insert("0001".into(), proposal("0001", "Goal "));
        assert!(refused(&pv));
        let mut pt = good.clone();
        pt.proposals.insert("0001".into(), proposal("0001", "Goal"));
        assert!(refused(&pt), "no such ticket");
        let mut pc = with_ticket.clone();
        pc.tickets.get_mut("0001").unwrap().status = Status::Open;
        pc.proposals.insert("0001".into(), proposal("0001", "Goal"));
        assert!(refused(&pc), "not active");
        // A landing ticket keeps its proposal and stays current, so both are written for one.
        let mut landing = with_ticket.clone();
        let t = landing.tickets.get_mut("0001").unwrap();
        t.status = Status::Landing;
        t.decision = Some(grimoire_core::model::Decision::new("2026-10-03", "Done."));
        landing.proposals.insert("0001".into(), proposal("0001", "Goal"));
        landing.current = Some("0001".into());
        assert!(!refused(&landing), "a landing ticket is written with its proposal");
        let mut cu = good.clone();
        cu.current = Some("0001".into());
        assert!(refused(&cu), "no such ticket is current");
        // Replacing writes the starts, proposals, current ticket and baseline given, and leaves none of the old store's behind.
        let mut full = with_ticket.clone();
        full.starts.insert("0001".into(), Start { ticket: "0001".into(), fingerprints: [("page:goal".to_string(), "f".to_string())].into() });
        full.proposals.insert("0001".into(), proposal("0001", "Goal"));
        full.current = Some("0001".into());
        full.baseline = Some(Baseline { date: "2026-09-30".into(), reason: "adopted".into(), findings: vec![] });
        write_all(&dir.join("store"), &full, true).unwrap();
        let back = load(&dir.join("store")).unwrap().0;
        assert!(back.starts.contains_key("0001") && back.baseline.is_some());
        assert_eq!((back.proposals.get("0001"), back.current.as_deref()), (full.proposals.get("0001"), Some("0001")));
        // Who may sign in is not what the store says: a replacement leaves the tokens as they are.
        let token = tokens::add(&dir.join("store"), "ana", true).unwrap();
        write_all(&dir.join("store"), &good, true).unwrap();
        let back = load(&dir.join("store")).unwrap().0;
        assert!(back.starts.is_empty() && back.baseline.is_none() && back.proposals.is_empty() && back.current.is_none(), "replaced whole");
        assert_eq!(tokens::lookup(&dir.join("store"), &token).unwrap().map(|t| t.name), Some("ana".to_string()));
    }

    /// A rename's writes persisted only up to each point, as a failure there
    /// would leave them, the store loaded back from disk, and the rerun
    /// itself stopped at each of its points too.
    /// # Test contract
    ///
    /// ```json
    /// {"test": "a-rename-stopped-anywhere-is-valid-and-completes", "checks": ["crates/core/src/ops#rename", "req:store-io-atomic-writes"]}
    /// ```
    #[test]
    fn a_rename_stopped_anywhere_is_valid_and_completes() {
        let page = |slug: &str, links: &[&str]| Page { slug: slug.into(), title: slug.into(), body: "b".into(), links: links.iter().map(|l| l.parse().unwrap()).collect() };
        let mut store = Store::new(StoreConfig { site: vec![], version: StoreConfig::CURRENT_VERSION, repository: "C:/repo".into(), roots: vec!["goal".into(), "vision".into()].into(), languages: vec!["rust".into()], scope: Scope::default(), order: Default::default() });
        // vision links itself, so the rename moves a link inside the record it moves.
        for p in [page("goal", &["page:vision"]), page("vision", &["page:vision", "page:goal"]), page("plan", &["page:goal", "page:vision"])] {
            store.pages.insert(p.slug.clone(), p);
        }
        let ticket: Ticket = serde_json::from_value(serde_json::json!({"slug": "0001", "title": "T", "kind": "change", "priority": "next", "status": "active", "touches": ["page:vision"], "relies_on": ["page:goal"], "body": "b", "decision": null})).unwrap();
        store.tickets.insert(ticket.slug.clone(), ticket);
        store.baseline = Some(serde_json::from_value(serde_json::json!({"date": "2026-09-30", "reason": "r", "findings": [{"rule": "uncovered", "check": "no-requirement", "ref": "page:vision"}]})).unwrap());
        store.starts.insert("0001".into(), Start { ticket: "0001".into(), fingerprints: [("page:vision".to_string(), "f".to_string()), ("page:goal".to_string(), "g".to_string())].into() });
        // The ticket's proposal holds a changed vision resting on the truth's, and a new page linking both the old and the new slug.
        let base = grimoire_core::fingerprint::of_record(&store.pages["vision"]);
        store.proposals.insert("0001".into(), Proposal { ticket: "0001".into(), pages: [("vision".to_string(), Proposed { record: page("vision", &["page:goal"]), base: Some(base) }), ("draft".to_string(), Proposed { record: page("draft", &["page:vision", "page:aim"]), base: None })].into(), requirements: Default::default(), deleted: Default::default() });
        let vision: Reference = "page:vision".parse().unwrap();
        let rename = |s: &mut Store| match grimoire_core::ops::rename_node(s, &vision, "aim").unwrap().1 {
            Changed::Many(steps) => steps,
            other => vec![other],
        };
        let mut done = store.clone();
        let steps = rename(&mut done);
        assert!(matches!(steps.last(), Some(Changed::Deleted(r)) if *r == vision), "the old record goes last");
        let p = &done.proposals["0001"];
        assert_eq!((p.pages["aim"].record.links.clone(), p.pages["draft"].record.links.clone()), (vec![Reference::page("goal")], vec![Reference::page("aim")]), "the proposal is renamed with it, a link kept once");
        assert_eq!(p.pages["aim"].base, Some(grimoire_core::fingerprint::of_record(&done.pages["aim"])), "its base follows the renamed truth");
        // The store on disk after the original rename stopped after `first` steps and a rerun after `second`.
        let on_disk = |first: usize, rerun: Option<(&Store, &[Changed], usize)>| {
            let tmp = temp();
            let dir = tmp.path();
            write_all(dir, &store, false).unwrap();
            persist(dir, &store, &Changed::Started("0001".into())).unwrap();
            for step in &steps[..first] {
                persist(dir, &done, step).unwrap();
            }
            if let Some((state, again, second)) = rerun {
                for step in &again[..second] {
                    persist(dir, state, step).unwrap();
                }
            }
            let (loaded, malformed) = load(dir).unwrap();
            assert!(malformed.is_empty(), "{malformed:?}");
            loaded
        };
        let valid = |s: &Store, at: &str| {
            let exists = |r: &Reference| s.pages.contains_key(&r.id);
            for p in s.pages.values() {
                assert!(p.links.iter().all(exists), "{at}: {} links a missing page", p.slug);
            }
            for t in s.tickets.values() {
                assert!(t.touches.iter().chain(&t.relies_on).all(exists), "{at}: a ticket names a missing page");
            }
            assert!(s.config.roots.list().iter().all(|r| s.pages.contains_key(*r)), "{at}: a missing root");
            assert!(s.baseline.iter().flat_map(|b| &b.findings).all(|f| exists(&f.r#ref)), "{at}: the baseline names a missing page");
            for st in s.starts.values() {
                assert!(st.fingerprints.keys().all(|k| exists(&k.parse().unwrap())), "{at}: a start names a missing page");
            }
        };
        let finished = |s: &Store, at: &str| {
            assert_eq!((&s.pages, &s.tickets, &s.config.roots, &s.baseline, &s.starts, &s.proposals), (&done.pages, &done.tickets, &done.config.roots, &done.baseline, &done.starts, &done.proposals), "{at}");
        };
        for first in 0..=steps.len() {
            let at = format!("stopped after {first}");
            let partial = on_disk(first, None);
            valid(&partial, &at);
            if !partial.pages.contains_key("vision") {
                assert_eq!(first, steps.len());
                finished(&partial, &at);
                continue;
            }
            // The same rename again, itself stopped at every point, then run to the end.
            let mut rerun = partial.clone();
            let again = rename(&mut rerun);
            for second in 0..=again.len() {
                let at = format!("{at}, rerun stopped after {second}");
                let mut twice = on_disk(first, Some((&rerun, &again, second)));
                valid(&twice, &at);
                if twice.pages.contains_key("vision") {
                    rename(&mut twice);
                }
                finished(&twice, &at);
            }
        }
    }

    /// # Test contract
    ///
    /// ```json
    /// {"test": "round-trip-and-malformed", "checks": ["crates/server/src/store_io#malformed-collected", "crates/server/src/store_io#layout"]}
    /// ```
    #[test]
    fn round_trip_and_malformed() {
        let tmp = temp();
        let dir = tmp.path();
        let mut store = Store::new(StoreConfig { site: vec![], version: StoreConfig::CURRENT_VERSION, repository: "C:/repo".into(), roots: vec!["overview".into()].into(), languages: vec!["rust".into()], scope: Scope::default(), order: Default::default() });
        store.pages.insert("overview".into(), Page { slug: "overview".into(), title: "O".into(), body: "b".into(), links: vec![] });
        write_all(dir, &store, false).unwrap();
        assert!(matches!(write_all(dir, &store, false), Err(StoreIoError::Exists(_))));
        std::fs::write(dir.join("pages").join("bad.json"), "{ not json").unwrap();
        std::fs::write(dir.join("tickets").join("t.json"), r#"{"slug":"other","title":"T","kind":"change","priority":"next","status":"open","touches":[],"relies_on":[],"body":"b","decision":null}"#).unwrap();
        let (loaded, malformed) = load(dir).unwrap();
        assert_eq!(loaded.pages.len(), 1);
        assert_eq!(malformed.len(), 2);
        assert_eq!(malformed[0].path, "pages/bad.json");
        assert!(malformed[1].reason.contains("named `t`"));
        // persist a delete
        persist(dir, &loaded, &Changed::Deleted(Reference::page("overview"))).unwrap();
        assert!(!dir.join("pages").join("overview.json").exists());
        // A proposal and the current ticket round-trip for an active ticket, and are malformed when they name none.
        let mut store = loaded.clone();
        store.tickets.insert("0001".into(), Ticket { slug: "0001".into(), title: "T".into(), kind: TicketKind::Change, priority: Priority::Later, status: Status::Active, touches: vec![], relies_on: vec![], body: "b".into(), decision: None });
        store.proposals.insert("0001".into(), Proposal { ticket: "0001".into(), pages: Default::default(), requirements: Default::default(), deleted: [("page:overview".to_string(), "f".to_string())].into() });
        store.current = Some("0001".into());
        persist(dir, &store, &Changed::Many(vec![Changed::Ticket("0001".into()), Changed::Proposed("0001".into()), Changed::Current])).unwrap();
        let (back, malformed) = load(dir).unwrap();
        assert_eq!((back.proposals.get("0001"), back.current.as_deref()), (store.proposals.get("0001"), Some("0001")));
        assert_eq!(malformed.len(), 2, "the two from before");
        std::fs::write(dir.join(PROPOSALS_DIR).join("0002.json"), r#"{"ticket":"0002"}"#).unwrap();
        std::fs::write(dir.join(PROPOSALS_DIR).join("0003.json"), "{ not json").unwrap();
        std::fs::write(dir.join(CURRENT_FILE), r#"{"ticket":"0002"}"#).unwrap();
        let (back, malformed) = load(dir).unwrap();
        assert!(back.proposals.len() == 1 && back.current.is_none());
        let reasons: Vec<(&str, &str)> = malformed.iter().map(|m| (m.path.as_str(), m.reason.as_str())).collect();
        assert!(reasons.iter().any(|(p, r)| *p == "proposals/0002.json" && r.contains("neither active nor landing")), "{reasons:?}");
        assert!(reasons.iter().any(|(p, _)| *p == "proposals/0003.json"), "{reasons:?}");
        assert!(reasons.iter().any(|(p, r)| *p == CURRENT_FILE && r.contains("neither active nor landing")), "{reasons:?}");
        // Deleting a ticket takes its start and proposal files; a removed proposal or current is removed from disk.
        std::fs::create_dir_all(dir.join(STARTS_DIR)).unwrap();
        std::fs::write(dir.join(STARTS_DIR).join("0001.json"), r#"{"ticket":"0001","fingerprints":{}}"#).unwrap();
        store.tickets.remove("0001");
        store.proposals.remove("0001");
        store.current = None;
        persist(dir, &store, &Changed::Many(vec![Changed::Deleted(Reference::ticket("0001")), Changed::Current])).unwrap();
        for path in [dir.join("tickets").join("0001.json"), dir.join(STARTS_DIR).join("0001.json"), dir.join(PROPOSALS_DIR).join("0001.json"), dir.join(CURRENT_FILE)] {
            assert!(!path.exists(), "{}", path.display());
        }
    }

    /// # Test contract
    ///
    /// ```json
    /// {"test": "tolerates-a-byte-order-mark", "checks": ["crates/server/src/store_io#bom-tolerated"]}
    /// ```
    #[test]
    fn tolerates_a_byte_order_mark() {
        let tmp = temp();
        let dir = tmp.path();
        let store = Store::new(StoreConfig { site: vec![], version: StoreConfig::CURRENT_VERSION, repository: "C:/repo".into(), roots: vec!["overview".into()].into(), languages: vec![], scope: Scope::default(), order: Default::default() });
        write_all(dir, &store, false).unwrap();
        let cfg = dir.join(CONFIG_FILE);
        let text = std::fs::read_to_string(&cfg).unwrap();
        std::fs::write(&cfg, format!("\u{feff}{text}")).unwrap();
        assert!(load(dir).is_ok());
    }

    /// # Test contract
    ///
    /// ```json
    /// {"test": "baseline-round-trips-and-a-broken-one-is-collected", "checks": ["crates/core/src/model#baseline-record", "crates/server/src/store_io#layout", "crates/server/src/store_io#malformed-collected"]}
    /// ```
    #[test]
    fn baseline_round_trips_and_a_broken_one_is_collected() {
        let tmp = temp();
        let dir = tmp.path();
        let mut store = Store::new(StoreConfig { site: vec![], version: StoreConfig::CURRENT_VERSION, repository: "C:/repo".into(), roots: vec!["overview".into()].into(), languages: vec![], scope: Scope::default(), order: Default::default() });
        store.baseline = Some(Baseline {
            date: "2026-09-28".into(),
            reason: "adopted at init".into(),
            findings: vec![FindingKey { rule: Rule::Unspecified, check: "no-scenario".into(), r#ref: Reference::req("x"), edge: None }],
        });
        write_all(dir, &store, false).unwrap();
        let (loaded, malformed) = load(dir).unwrap();
        assert!(malformed.is_empty());
        assert_eq!(loaded.baseline, store.baseline);

        std::fs::write(dir.join(BASELINE_FILE), "{ not json").unwrap();
        let (loaded, malformed) = load(dir).unwrap();
        assert!(loaded.baseline.is_none(), "a broken baseline holds nothing: every finding is new");
        assert_eq!(malformed[0].path, BASELINE_FILE);

        store.baseline = None;
        persist(dir, &store, &Changed::Baseline).unwrap();
        assert!(!dir.join(BASELINE_FILE).exists());
    }

    #[test]
    fn the_serving_marker_is_this_process_s_to_remove() {
        let tmp = temp();
        let dir = tmp.path();
        write_serving(dir, 4242).unwrap();
        assert_eq!(read_serving(dir), Some(Serving { port: 4242, pid: std::process::id(), host: None, path: None }));
        remove_serving(dir);
        assert!(read_serving(dir).is_none());
        std::fs::write(dir.join(SERVE_FILE), r#"{"port":4242,"pid":1}"#).unwrap();
        remove_serving(dir);
        assert!(read_serving(dir).is_some(), "another process's marker is left alone");
    }

    #[test]
    fn config_persists_its_scope() {
        let tmp = temp();
        let dir = tmp.path();
        let mut store = Store::new(StoreConfig { site: vec![], version: StoreConfig::CURRENT_VERSION, repository: "C:/repo".into(), roots: vec!["overview".into()].into(), languages: vec![], scope: Scope::default(), order: Default::default() });
        write_all(dir, &store, false).unwrap();
        let text = std::fs::read_to_string(dir.join(CONFIG_FILE)).unwrap();
        assert!(!text.contains("scope"), "an empty scope is not written");
        store.config.scope = Scope { include: vec!["crates/core".into()], exclude: vec![] };
        persist(dir, &store, &Changed::Config).unwrap();
        let (loaded, _) = load(dir).unwrap();
        assert_eq!(loaded.config.scope.include, vec!["crates/core".to_string()]);
    }

    /// # Test contract
    ///
    /// ```json
    /// {"test": "writes-leave-no-partial-file", "checks": ["crates/server/src/store_io#atomic-writes"]}
    /// ```
    #[test]
    fn writes_leave_no_partial_file() {
        let tmp = temp();
        let dir = tmp.path();
        let mut store = Store::new(StoreConfig { site: vec![], version: StoreConfig::CURRENT_VERSION, repository: "C:/repo".into(), roots: vec!["overview".into()].into(), languages: vec![], scope: Scope::default(), order: Default::default() });
        write_all(dir, &store, false).unwrap();
        let path = dir.join("pages").join("big.json");
        let reader = {
            let path = path.clone();
            std::thread::spawn(move || {
                let mut whole = 0;
                for _ in 0..2000 {
                    // Once the record exists a reader always finds it, whole: the rename replaces it in one step.
                    match std::fs::read_to_string(&path) {
                        Ok(text) => {
                            assert!(serde_json::from_str::<Page>(&text).is_ok(), "a partial record: {} bytes", text.len());
                            whole += 1;
                        }
                        Err(e) => assert!(whole == 0 && e.kind() == std::io::ErrorKind::NotFound, "the record went missing after it was written: {e}"),
                    }
                }
                whole
            })
        };
        for i in 0..200 {
            let body = "x".repeat(20_000 + i * 97);
            store.pages.insert("big".into(), Page { slug: "big".into(), title: "Big".into(), body, links: vec![] });
            persist(dir, &store, &Changed::Page("big".into())).unwrap();
        }
        assert!(reader.join().unwrap() > 0, "the reader saw records");
        let leftovers: Vec<_> = std::fs::read_dir(dir.join("pages")).unwrap().flatten().filter(|e| e.file_name().to_string_lossy().contains(".tmp")).collect();
        assert!(leftovers.is_empty(), "no temporary file is left behind");
    }

    /// # Test contract
    ///
    /// ```json
    /// {"test": "a-start-is-written-and-removed", "checks": ["crates/server/src/store_io#layout", "crates/core/src/model#start-record"]}
    /// ```
    #[test]
    fn a_start_is_written_and_removed() {
        let tmp = temp();
        let dir = tmp.path();
        let mut store = Store::new(StoreConfig { site: vec![], version: StoreConfig::CURRENT_VERSION, repository: "C:/repo".into(), roots: vec!["overview".into()].into(), languages: vec![], scope: Scope::default(), order: Default::default() });
        store.tickets.insert("0040".into(), Ticket { slug: "0040".into(), title: "T".into(), kind: TicketKind::Change, priority: Priority::Next, status: Status::Active, touches: vec![], relies_on: vec![], body: "b".into(), decision: None });
        store.starts.insert("0040".into(), Start { ticket: "0040".into(), fingerprints: [("page:a".to_string(), "1".to_string())].into_iter().collect() });
        write_all(dir, &store, false).unwrap();
        persist(dir, &store, &Changed::Started("0040".into())).unwrap();
        let text = std::fs::read_to_string(dir.join(STARTS_DIR).join("0040.json")).unwrap();
        assert!(!text.contains("date"), "a start carries no date");
        let (loaded, malformed) = load(dir).unwrap();
        assert!(malformed.is_empty(), "{malformed:?}");
        assert_eq!(loaded.starts, store.starts);
        store.starts.remove("0040");
        persist(dir, &store, &Changed::Closed("0040".into())).unwrap();
        assert!(!dir.join(STARTS_DIR).join("0040.json").exists());
    }

    #[test]
    fn today_is_iso() {
        assert!(grimoire_core::store::is_date(&today()));
    }
}
