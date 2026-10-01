//! # Module contract
//!
//! ```json
//! {"purpose":"Read a store directory into a value and write single records back atomically; where a store lives and its write lock each have a module beneath.","criteria":[{"id":"atomic-writes","statement":"A record is written to a temporary file in the same directory and renamed over the old one in one step: a write that finishes or fails leaves no temporary file behind, and a reader running while records are rewritten always finds the record, whole."},{"id":"malformed-collected","statement":"A file that does not parse or violates a field rule is reported as malformed with its path and reason, and the rest of the store still loads."},{"id":"bom-tolerated","statement":"A store file that begins with a UTF-8 byte-order mark loads as if it did not, since hand edits from Windows editors leave one."},{"id":"layout","statement":"A store is a directory holding grimoire.json and pages/, requirements/ and tickets/ with one <slug>.json per record, plus baseline.json once adopted, serve.json while a daemon serves it, starts/<slug>.json for each active ticket, and lock, the empty file a writer locks; a record file whose name differs from its record's slug is malformed, and so is a baseline or start that does not parse."}]}
//! ```

use std::path::{Path, PathBuf};

use grimoire_core::model::*;
use grimoire_core::ops::Changed;
use grimoire_core::store::{validate_baseline, validate_config, validate_page, validate_requirement, validate_ticket};
use grimoire_core::{Malformed, Store};

mod location;
mod lock;

pub use location::{find_project, refuse_inside, store_for_repo, stores_dir, write_project_file, PROJECT_FILE};
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
    #[error("store {0} is in use by {1}; it has one writer at a time")]
    Locked(PathBuf, String),
    #[error("store {0} is inside the repository {1}; a store lives outside the repository it describes")]
    InsideRepository(PathBuf, PathBuf),
}

pub const CONFIG_FILE: &str = "grimoire.json";
pub const BASELINE_FILE: &str = "baseline.json";
pub const SERVE_FILE: &str = "serve.json";
pub const STARTS_DIR: &str = "starts";

/// `serve.json`: which daemon serves this store. A hint, never trusted
/// without asking the daemon itself (see `client::serving`).
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct Serving {
    pub port: u16,
    pub pid: u32,
}

/// Record that this process serves the store on `port`.
pub fn write_serving(store_dir: &Path, port: u16) -> Result<(), StoreIoError> {
    write_atomic(&store_dir.join(SERVE_FILE), &to_pretty(&Serving { port, pid: std::process::id() }))
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
    let text = std::fs::read_to_string(path).map_err(|e| e.to_string())?;
    // A hand edit from a Windows editor may leave a byte-order mark.
    serde_json::from_str(text.trim_start_matches('\u{feff}')).map_err(|e| e.to_string())
}

fn rel(store: &Path, path: &Path) -> String {
    path.strip_prefix(store).unwrap_or(path).to_string_lossy().replace('\\', "/")
}

/// Load a store directory. Malformed files are collected, not fatal; a
/// missing or unreadable `grimoire.json` is.
pub fn load(store_dir: &Path) -> Result<(Store, Vec<Malformed>), StoreIoError> {
    let config_path = store_dir.join(CONFIG_FILE);
    if !config_path.is_file() {
        return Err(StoreIoError::NotAStore(store_dir.to_path_buf()));
    }
    let config: StoreConfig = read_json(&config_path).map_err(|e| StoreIoError::BadConfig(store_dir.to_path_buf(), e))?;
    let errors = validate_config(&config);
    if !errors.is_empty() {
        let msg = errors.iter().map(|e| format!("{}: {}", e.field, e.reason)).collect::<Vec<_>>().join("; ");
        return Err(StoreIoError::BadConfig(store_dir.to_path_buf(), msg));
    }
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
                Kind::Page => read_json::<Page>(&path).and_then(|p| check(&p.slug, &stem, validate_page(&p)).map(|_| store.pages.insert(p.slug.clone(), p)).map(|_| ())),
                Kind::Req => read_json::<Requirement>(&path).and_then(|r| check(&r.slug, &stem, validate_requirement(&r)).map(|_| store.requirements.insert(r.slug.clone(), r)).map(|_| ())),
                Kind::Ticket => read_json::<Ticket>(&path).and_then(|t| check(&t.slug, &stem, validate_ticket(&t)).map(|_| store.tickets.insert(t.slug.clone(), t)).map(|_| ())),
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
            match read_json::<Start>(&path) {
                Ok(s) if s.ticket == stem => {
                    store.starts.insert(stem, s);
                }
                Ok(s) => malformed.push(Malformed { path: rel(store_dir, &path), reason: format!("file is named `{stem}` but its ticket is `{}`", s.ticket) }),
                Err(reason) => malformed.push(Malformed { path: rel(store_dir, &path), reason }),
            }
        }
    }
    let baseline_path = store_dir.join(BASELINE_FILE);
    if baseline_path.is_file() {
        let result = read_json::<Baseline>(&baseline_path).and_then(|b| {
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
        Changed::Baseline => match &store.baseline {
            Some(b) => write_atomic(&store_dir.join(BASELINE_FILE), &to_pretty(b)),
            None => remove_if_present(&store_dir.join(BASELINE_FILE)),
        },
        Changed::Deleted(r) => {
            let path = store_dir.join(dir_of(r.kind)).join(format!("{}.json", r.id));
            match std::fs::remove_file(&path) {
                Ok(()) => Ok(()),
                Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
                Err(e) => Err(StoreIoError::Io(path, e)),
            }
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
    let config_path = store_dir.join(CONFIG_FILE);
    if config_path.exists() {
        if !replace {
            return Err(StoreIoError::Exists(store_dir.to_path_buf()));
        }
        for kind in [Kind::Page, Kind::Req, Kind::Ticket] {
            let dir = store_dir.join(dir_of(kind));
            if dir.is_dir() {
                std::fs::remove_dir_all(&dir).map_err(|e| StoreIoError::Io(dir.clone(), e))?;
            }
        }
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

    pub(super) fn temp() -> PathBuf {
        let d = std::env::temp_dir().join(format!("grimoire-store-io-{}-{}", std::process::id(), rand_suffix()));
        std::fs::create_dir_all(&d).unwrap();
        d
    }

    fn rand_suffix() -> u128 {
        std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos()
    }

    /// # Test contract
    ///
    /// ```json
    /// {"test": "a-failed-write-leaves-no-temporary-file", "checks": ["crates/server/src/store_io#atomic-writes"]}
    /// ```
    #[test]
    fn a_failed_write_leaves_no_temporary_file() {
        let dir = temp();
        // A folder where the record should be: the rename fails, and the temporary file goes with it.
        std::fs::create_dir_all(dir.join("pages").join("goal.json").join("x")).unwrap();
        assert!(write_atomic(&dir.join("pages").join("goal.json"), "{}").is_err());
        let left: Vec<_> = std::fs::read_dir(dir.join("pages")).unwrap().flatten().map(|e| e.file_name()).collect();
        assert_eq!(left, vec![std::ffi::OsString::from("goal.json")], "only the folder that was there");
        std::fs::remove_dir_all(&dir).unwrap();
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
        let mut store = Store::new(StoreConfig { version: 1, repository: "C:/repo".into(), roots: vec!["goal".into(), "vision".into()], languages: vec!["rust".into()], scope: Scope::default(), order: Default::default() });
        // vision links itself, so the rename moves a link inside the record it moves.
        for p in [page("goal", &["page:vision"]), page("vision", &["page:vision", "page:goal"]), page("plan", &["page:goal", "page:vision"])] {
            store.pages.insert(p.slug.clone(), p);
        }
        let ticket: Ticket = serde_json::from_value(serde_json::json!({"slug": "0001", "title": "T", "kind": "change", "priority": "next", "status": "active", "touches": ["page:vision"], "relies_on": ["page:goal"], "body": "b", "decision": null})).unwrap();
        store.tickets.insert(ticket.slug.clone(), ticket);
        store.baseline = Some(serde_json::from_value(serde_json::json!({"date": "2026-09-30", "reason": "r", "findings": [{"rule": "uncovered", "check": "no-requirement", "ref": "page:vision"}]})).unwrap());
        store.starts.insert("0001".into(), Start { ticket: "0001".into(), fingerprints: [("page:vision".to_string(), "f".to_string()), ("page:goal".to_string(), "g".to_string())].into() });
        let vision: Reference = "page:vision".parse().unwrap();
        let rename = |s: &mut Store| match grimoire_core::ops::rename_node(s, &vision, "aim").unwrap().1 {
            Changed::Many(steps) => steps,
            other => vec![other],
        };
        let mut done = store.clone();
        let steps = rename(&mut done);
        assert!(matches!(steps.last(), Some(Changed::Deleted(r)) if *r == vision), "the old record goes last");
        // The store on disk after the original rename stopped after `first` steps and a rerun after `second`.
        let on_disk = |first: usize, rerun: Option<(&Store, &[Changed], usize)>| {
            let dir = temp();
            write_all(&dir, &store, false).unwrap();
            persist(&dir, &store, &Changed::Started("0001".into())).unwrap();
            for step in &steps[..first] {
                persist(&dir, &done, step).unwrap();
            }
            if let Some((state, again, second)) = rerun {
                for step in &again[..second] {
                    persist(&dir, state, step).unwrap();
                }
            }
            let (loaded, malformed) = load(&dir).unwrap();
            assert!(malformed.is_empty(), "{malformed:?}");
            std::fs::remove_dir_all(&dir).unwrap();
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
            assert!(s.config.roots.iter().all(|r| s.pages.contains_key(r)), "{at}: a missing root");
            assert!(s.baseline.iter().flat_map(|b| &b.findings).all(|f| exists(&f.r#ref)), "{at}: the baseline names a missing page");
            for st in s.starts.values() {
                assert!(st.fingerprints.keys().all(|k| exists(&k.parse().unwrap())), "{at}: a start names a missing page");
            }
        };
        let finished = |s: &Store, at: &str| {
            assert_eq!((&s.pages, &s.tickets, &s.config.roots, &s.baseline, &s.starts), (&done.pages, &done.tickets, &done.config.roots, &done.baseline, &done.starts), "{at}");
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
        let dir = temp();
        let mut store = Store::new(StoreConfig { version: 1, repository: "C:/repo".into(), roots: vec!["overview".into()], languages: vec!["rust".into()], scope: Scope::default(), order: Default::default() });
        store.pages.insert("overview".into(), Page { slug: "overview".into(), title: "O".into(), body: "b".into(), links: vec![] });
        write_all(&dir, &store, false).unwrap();
        assert!(matches!(write_all(&dir, &store, false), Err(StoreIoError::Exists(_))));
        std::fs::write(dir.join("pages").join("bad.json"), "{ not json").unwrap();
        std::fs::write(dir.join("tickets").join("t.json"), r#"{"slug":"other","title":"T","kind":"change","priority":"next","status":"open","touches":[],"relies_on":[],"body":"b","decision":null}"#).unwrap();
        let (loaded, malformed) = load(&dir).unwrap();
        assert_eq!(loaded.pages.len(), 1);
        assert_eq!(malformed.len(), 2);
        assert_eq!(malformed[0].path, "pages/bad.json");
        assert!(malformed[1].reason.contains("named `t`"));
        // persist a delete
        persist(&dir, &loaded, &Changed::Deleted(Reference::page("overview"))).unwrap();
        assert!(!dir.join("pages").join("overview.json").exists());
        std::fs::remove_dir_all(&dir).unwrap();
    }

    /// # Test contract
    ///
    /// ```json
    /// {"test": "tolerates-a-byte-order-mark", "checks": ["crates/server/src/store_io#bom-tolerated"]}
    /// ```
    #[test]
    fn tolerates_a_byte_order_mark() {
        let dir = temp();
        let store = Store::new(StoreConfig { version: 1, repository: "C:/repo".into(), roots: vec!["overview".into()], languages: vec![], scope: Scope::default(), order: Default::default() });
        write_all(&dir, &store, false).unwrap();
        let cfg = dir.join(CONFIG_FILE);
        let text = std::fs::read_to_string(&cfg).unwrap();
        std::fs::write(&cfg, format!("\u{feff}{text}")).unwrap();
        assert!(load(&dir).is_ok());
        std::fs::remove_dir_all(&dir).unwrap();
    }

    /// # Test contract
    ///
    /// ```json
    /// {"test": "baseline-round-trips-and-a-broken-one-is-collected", "checks": ["crates/core/src/model#baseline-record", "crates/server/src/store_io#layout", "crates/server/src/store_io#malformed-collected"]}
    /// ```
    #[test]
    fn baseline_round_trips_and_a_broken_one_is_collected() {
        let dir = temp();
        let mut store = Store::new(StoreConfig { version: 1, repository: "C:/repo".into(), roots: vec!["overview".into()], languages: vec![], scope: Scope::default(), order: Default::default() });
        store.baseline = Some(Baseline {
            date: "2026-09-28".into(),
            reason: "adopted at init".into(),
            findings: vec![FindingKey { rule: Rule::Unspecified, check: "no-scenario".into(), r#ref: Reference::req("x"), edge: None }],
        });
        write_all(&dir, &store, false).unwrap();
        let (loaded, malformed) = load(&dir).unwrap();
        assert!(malformed.is_empty());
        assert_eq!(loaded.baseline, store.baseline);

        std::fs::write(dir.join(BASELINE_FILE), "{ not json").unwrap();
        let (loaded, malformed) = load(&dir).unwrap();
        assert!(loaded.baseline.is_none(), "a broken baseline holds nothing: every finding is new");
        assert_eq!(malformed[0].path, BASELINE_FILE);

        store.baseline = None;
        persist(&dir, &store, &Changed::Baseline).unwrap();
        assert!(!dir.join(BASELINE_FILE).exists());
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn the_serving_marker_is_this_process_s_to_remove() {
        let dir = temp();
        write_serving(&dir, 4242).unwrap();
        assert_eq!(read_serving(&dir), Some(Serving { port: 4242, pid: std::process::id() }));
        remove_serving(&dir);
        assert!(read_serving(&dir).is_none());
        std::fs::write(dir.join(SERVE_FILE), r#"{"port":4242,"pid":1}"#).unwrap();
        remove_serving(&dir);
        assert!(read_serving(&dir).is_some(), "another process's marker is left alone");
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn config_persists_its_scope() {
        let dir = temp();
        let mut store = Store::new(StoreConfig { version: 1, repository: "C:/repo".into(), roots: vec!["overview".into()], languages: vec![], scope: Scope::default(), order: Default::default() });
        write_all(&dir, &store, false).unwrap();
        let text = std::fs::read_to_string(dir.join(CONFIG_FILE)).unwrap();
        assert!(!text.contains("scope"), "an empty scope is not written");
        store.config.scope = Scope { include: vec!["crates/core".into()], exclude: vec![] };
        persist(&dir, &store, &Changed::Config).unwrap();
        let (loaded, _) = load(&dir).unwrap();
        assert_eq!(loaded.config.scope.include, vec!["crates/core".to_string()]);
        std::fs::remove_dir_all(&dir).unwrap();
    }

    /// # Test contract
    ///
    /// ```json
    /// {"test": "writes-leave-no-partial-file", "checks": ["crates/server/src/store_io#atomic-writes"]}
    /// ```
    #[test]
    fn writes_leave_no_partial_file() {
        let dir = temp();
        let mut store = Store::new(StoreConfig { version: 1, repository: "C:/repo".into(), roots: vec!["overview".into()], languages: vec![], scope: Scope::default(), order: Default::default() });
        write_all(&dir, &store, false).unwrap();
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
            persist(&dir, &store, &Changed::Page("big".into())).unwrap();
        }
        assert!(reader.join().unwrap() > 0, "the reader saw records");
        let leftovers: Vec<_> = std::fs::read_dir(dir.join("pages")).unwrap().flatten().filter(|e| e.file_name().to_string_lossy().contains(".tmp")).collect();
        assert!(leftovers.is_empty(), "no temporary file is left behind");
        std::fs::remove_dir_all(&dir).unwrap();
    }

    /// # Test contract
    ///
    /// ```json
    /// {"test": "a-start-is-written-and-removed", "checks": ["crates/server/src/store_io#layout", "crates/core/src/model#start-record"]}
    /// ```
    #[test]
    fn a_start_is_written_and_removed() {
        let dir = temp();
        let mut store = Store::new(StoreConfig { version: 1, repository: "C:/repo".into(), roots: vec!["overview".into()], languages: vec![], scope: Scope::default(), order: Default::default() });
        store.tickets.insert("0040".into(), Ticket { slug: "0040".into(), title: "T".into(), kind: TicketKind::Change, priority: Priority::Next, status: Status::Active, touches: vec![], relies_on: vec![], body: "b".into(), decision: None });
        store.starts.insert("0040".into(), Start { ticket: "0040".into(), fingerprints: [("page:a".to_string(), "1".to_string())].into_iter().collect() });
        write_all(&dir, &store, false).unwrap();
        persist(&dir, &store, &Changed::Started("0040".into())).unwrap();
        let text = std::fs::read_to_string(dir.join(STARTS_DIR).join("0040.json")).unwrap();
        assert!(!text.contains("date"), "a start carries no date");
        let (loaded, malformed) = load(&dir).unwrap();
        assert!(malformed.is_empty(), "{malformed:?}");
        assert_eq!(loaded.starts, store.starts);
        store.starts.remove("0040");
        persist(&dir, &store, &Changed::Closed("0040".into())).unwrap();
        assert!(!dir.join(STARTS_DIR).join("0040.json").exists());
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn today_is_iso() {
        assert!(grimoire_core::store::is_date(&today()));
    }
}
