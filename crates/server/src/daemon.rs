//! # Module contract
//!
//! ```json
//! {"purpose":"Own the live state of one store and one repository: load both, apply tool calls, persist writes, re-extract changed files and tell listeners what changed.","criteria":[{"id":"persist-after-write","statement":"A write is persisted to the store directory before its result is returned, and a failed persist reloads the store from disk so memory never drifts from it."},{"id":"one-file-reextract","statement":"A changed source file is re-extracted alone and the extraction re-joined; unchanged files are not re-read."},{"id":"events","statement":"Every store write and every graph rebuild is announced to listeners with what changed."}]}
//! ```

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::Mutex;

use grimoire_core::tools;
use grimoire_core::{Extraction, FileExtraction, State, ToolError};
use grimoire_extract::Extractor;
use serde::Serialize;
use serde_json::Value;
use tokio::sync::broadcast;

use crate::store_io::{self, StoreIoError};

#[derive(Debug, Clone, Serialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Event {
    /// The store changed through a tool.
    Store { tool: String, changed: Vec<String> },
    /// The repository changed and the graph was rebuilt.
    Graph { changed: Vec<String> },
}

pub struct Daemon {
    pub store_dir: PathBuf,
    pub repo: PathBuf,
    pub extractor: Extractor,
    files: Mutex<BTreeMap<String, FileExtraction>>,
    state: Mutex<State>,
    events: broadcast::Sender<Event>,
}

#[derive(Debug, thiserror::Error)]
pub enum OpenError {
    #[error(transparent)]
    Store(#[from] StoreIoError),
    #[error("repository {0} does not exist")]
    NoRepository(PathBuf),
    #[error(transparent)]
    Extract(#[from] grimoire_extract::ExtractError),
}

/// What loading produced, for the CLI as well as the daemon.
pub struct Loaded {
    pub state: State,
    pub files: BTreeMap<String, FileExtraction>,
    pub extractor: Extractor,
    pub repo: PathBuf,
    pub extract_errors: Vec<grimoire_extract::ExtractError>,
}

/// Load a store and extract its repository once.
pub fn load(store_dir: &Path) -> Result<Loaded, OpenError> {
    let (store, malformed) = store_io::load(store_dir)?;
    let repo = PathBuf::from(&store.config.repository);
    if !repo.is_dir() {
        return Err(OpenError::NoRepository(repo));
    }
    let extractor = Extractor::for_languages(&repo, &store.config.languages);
    let (extracted, extract_errors) = extractor.extract_all()?;
    let files: BTreeMap<String, FileExtraction> = extracted.into_iter().map(|f| (f.path.clone(), f)).collect();
    let extraction = Extraction::from_files(files.values());
    Ok(Loaded { state: State::new(store, extraction, malformed), files, extractor, repo, extract_errors })
}

impl Daemon {
    pub fn open(store_dir: &Path) -> Result<(Daemon, Vec<grimoire_extract::ExtractError>), OpenError> {
        let loaded = load(store_dir)?;
        let (events, _) = broadcast::channel(256);
        Ok((
            Daemon {
                store_dir: store_dir.to_path_buf(),
                repo: loaded.repo,
                extractor: loaded.extractor,
                files: Mutex::new(loaded.files),
                state: Mutex::new(loaded.state),
                events,
            },
            loaded.extract_errors,
        ))
    }

    pub fn subscribe(&self) -> broadcast::Receiver<Event> {
        self.events.subscribe()
    }

    /// Run one tool, persisting any write.
    pub fn call(&self, name: &str, input: Value) -> Result<Value, ToolError> {
        let today = store_io::today();
        let mut state = self.state.lock().expect("state lock");
        let outcome = tools::call(&mut state, name, input, &today)?;
        if let Some(changed) = &outcome.changed {
            if let Err(e) = store_io::persist(&self.store_dir, state.store(), changed) {
                tracing::error!("persist failed, reloading store from disk: {e}");
                match store_io::load(&self.store_dir) {
                    Ok((store, malformed)) => state.set_store(store, malformed),
                    Err(e2) => tracing::error!("reload failed too: {e2}"),
                }
                return Err(ToolError::unavailable(format!("the store could not be written: {e}")));
            }
            let what = match changed {
                grimoire_core::ops::Changed::Page(s) => format!("page:{s}"),
                grimoire_core::ops::Changed::Requirement(s) => format!("req:{s}"),
                grimoire_core::ops::Changed::Ticket(s) => format!("ticket:{s}"),
                grimoire_core::ops::Changed::Deleted(r) => r.to_string(),
            };
            let _ = self.events.send(Event::Store { tool: name.to_string(), changed: vec![what] });
        }
        Ok(outcome.output)
    }

    /// Re-extract the given repository-relative paths (deleted ones are
    /// removed) and re-join the graph.
    pub fn reextract(&self, paths: &[String]) {
        let mut changed = Vec::new();
        {
            let mut files = self.files.lock().expect("files lock");
            for rel in paths {
                if !self.extractor.handles(rel) {
                    continue;
                }
                let full = self.repo.join(rel.replace('/', std::path::MAIN_SEPARATOR_STR));
                if full.is_file() {
                    match self.extractor.extract_path(rel) {
                        Ok(f) => {
                            if files.get(rel) != Some(&f) {
                                files.insert(rel.clone(), f);
                                changed.push(rel.clone());
                            }
                        }
                        Err(e) => tracing::warn!("{e}"),
                    }
                } else if files.remove(rel).is_some() {
                    changed.push(rel.clone());
                }
            }
            if changed.is_empty() {
                return;
            }
            let extraction = Extraction::from_files(files.values());
            self.state.lock().expect("state lock").set_extraction(extraction);
        }
        tracing::info!("re-extracted {} file(s): {}", changed.len(), changed.join(", "));
        let _ = self.events.send(Event::Graph { changed });
    }

    /// Repository-relative form of an absolute path inside the repository.
    pub fn relative(&self, path: &Path) -> Option<String> {
        grimoire_extract::relative(&self.repo, path)
    }
}
