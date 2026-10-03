//! # Module contract
//!
//! ```json
//! {"purpose":"Hold what the tools operate on: the store, the extraction and the build, with the graph cached between calls and a reader the caller supplies for source.","criteria":[{"id":"graph-current","statement":"A read after a write sees the write; the cached graph is rebuilt whenever the store or the extraction changes."},{"id":"without-code","statement":"A state told it holds no code marks every graph it builds so, the truth's and a proposal's alike, never prunes the baseline, answers lint with without_code and status with shared, and refuses adopt as invalid."},{"id":"no-shared-current","statement":"A state that holds no code keeps nothing that is a checkout's own, no current ticket and no start: those its store arrived with are dropped, start_ticket sets and records none, and a write that changes intent and names no ticket is refused as invalid."},{"id":"server-copy","statement":"A state told it is a machine's copy of a server's intent answers status with the server's address, when the intent was fetched, whether the server answered last and whether it refused this machine's token, and never prunes the baseline; any other state answers status with no server."}]}
//! ```

use crate::extraction::Extraction;
use crate::graph::Graph;
use crate::lint;
use crate::ops::Changed;
use crate::store::{Malformed, Store};

/// What the running build is: facts the loader knows and core cannot find
/// out for itself without I/O. `status` reports them next to the store's
/// `languages` so a store naming a language this build lacks is visible.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub struct Build {
    /// The crate version; every crate in the workspace shares one.
    pub version: String,
    /// The languages this build has an extraction adapter for.
    pub supported_languages: Vec<String>,
}

impl Default for Build {
    fn default() -> Self {
        Build { version: env!("CARGO_PKG_VERSION").to_string(), supported_languages: vec![] }
    }
}

/// Everything the tools operate on, with the graph cached between calls.
#[derive(Debug, Default)]
pub struct State {
    pub(super) store: Store,
    extraction: Extraction,
    malformed: Vec<Malformed>,
    pub(super) build: Build,
    graph: Option<Graph>,
    /// The graph as one ticket's proposal would leave it, cached like the truth's.
    view: Option<(String, Graph)>,
    pub(super) source: Option<std::sync::Arc<dyn SourceReader>>,
    /// Set by a daemon that holds no code: every graph is marked so, and the baseline is left alone.
    without_code: bool,
    /// Set on a machine whose store is a copy of a team's server's intent.
    server: Option<crate::answer::ServerCopy>,
}

/// Reads a repository file's text for the `source` tool. The core does no
/// I/O, so its caller supplies this: the daemon and the CLI read the
/// repository, tests read memory. `path` is a file the graph knows; the
/// reader reads no more than `limit` bytes, saying so when the file is larger.
pub trait SourceReader: Send + Sync + std::fmt::Debug {
    fn read(&self, path: &str, limit: usize) -> Result<String, SourceError>;
}

/// Why a reader gave no text.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SourceError {
    /// The file is larger than the limit, in bytes; it was not read.
    TooLarge(u64),
    /// It could not be read, and why.
    Unreadable(String),
}

/// The largest file `source` returns, in bytes and in lines: past these the
/// view would not be read anyway, and a page of that many rows would stall.
pub const SOURCE_LIMIT: usize = 2 * 1024 * 1024;
pub const SOURCE_LINES: usize = 20_000;

impl State {
    pub fn new(store: Store, extraction: Extraction, malformed: Vec<Malformed>) -> Self {
        State { store, extraction, malformed, build: Build::default(), graph: None, view: None, source: None, without_code: false, server: None }
    }
    /// The store's current ticket, the one last started.
    pub fn current(&self) -> Option<String> {
        self.store.current.clone()
    }
    /// The graph for a view: the truth when `ticket` is none, else the truth
    /// as that ticket's proposal would leave it. A ticket that is neither
    /// active nor landing is a conflict, one that does not exist not_found.
    pub fn graph_for(&mut self, ticket: Option<&str>) -> Result<&Graph, crate::ops::ToolError> {
        let Some(ticket) = ticket else { return Ok(self.graph()) };
        match self.store.tickets.get(ticket) {
            None => return Err(crate::ops::ToolError::not_found(&crate::model::Reference::ticket(ticket))),
            Some(t) if !t.status.is_live() => return Err(crate::ops::ToolError::conflict(format!("ticket:{ticket} is {}, neither active nor landing: only a ticket being worked has a proposal to read", t.status.name()))),
            _ => {}
        }
        if self.view.as_ref().is_none_or(|(t, _)| t != ticket) {
            let mut g = Graph::with_proposal(&self.store, ticket, &self.extraction, &self.malformed);
            self.flag_languages(&mut g);
            g.without_code = self.without_code;
            self.view = Some((ticket.to_string(), g));
        }
        Ok(&self.view.as_ref().expect("just built").1)
    }
    fn build_graph(&self, store: &Store) -> Graph {
        let mut g = Graph::build(store, &self.extraction, &self.malformed);
        self.flag_languages(&mut g);
        g.without_code = self.without_code;
        g
    }
    /// Say this state holds no code, as a shared daemon's does: what only
    /// code can show is then left out of every answer, never called missing.
    pub fn set_without_code(&mut self, without_code: bool) {
        self.without_code = without_code;
        self.drop_shared_current();
        self.graph = None;
        self.view = None;
    }
    /// The current ticket and the starts are each checkout's own: a state serving a team keeps none.
    fn drop_shared_current(&mut self) {
        if self.without_code {
            self.store.current = None;
            self.store.starts.clear();
        }
    }
    /// Say this state is a machine's copy of a server's intent, or no longer is.
    /// The graph does not depend on it.
    pub fn set_server(&mut self, server: Option<crate::answer::ServerCopy>) {
        self.server = server;
    }
    pub fn server(&self) -> Option<&crate::answer::ServerCopy> {
        self.server.as_ref()
    }
    pub fn without_code(&self) -> bool {
        self.without_code
    }
    /// An unknown build (tests, or a caller that never said) flags nothing.
    fn flag_languages(&self, g: &mut Graph) {
        let supported = &self.build.supported_languages;
        if !supported.is_empty() {
            g.unsupported_languages = self.store.config.languages.iter().filter(|l| !supported.contains(l)).cloned().collect();
            g.supported_languages = supported.clone();
        }
    }
    pub fn store(&self) -> &Store {
        &self.store
    }
    pub fn extraction(&self) -> &Extraction {
        &self.extraction
    }
    pub fn build(&self) -> &Build {
        &self.build
    }
    /// Give the state a way to read the repository's files, for `source`.
    pub fn set_source(&mut self, reader: std::sync::Arc<dyn SourceReader>) {
        self.source = Some(reader);
    }
    /// Tell the state what build is running. The graph does not depend on it.
    pub fn set_build(&mut self, build: Build) {
        self.build = build;
    }
    pub fn set_store(&mut self, store: Store, malformed: Vec<Malformed>) {
        self.store = store;
        self.malformed = malformed;
        self.drop_shared_current();
        self.graph = None;
        self.view = None;
    }
    pub fn set_extraction(&mut self, extraction: Extraction) {
        self.extraction = extraction;
        self.graph = None;
        self.view = None;
    }
    pub fn graph(&mut self) -> &Graph {
        if self.graph.is_none() {
            self.graph = Some(self.build_graph(&self.store));
        }
        self.graph.as_ref().expect("just built")
    }
    /// Drop from the baseline every finding no longer produced. Returns what
    /// changed when it shrank, so the owner of the store persists it.
    pub fn prune_baseline(&mut self) -> Option<Changed> {
        // With no code held, a finding not produced here is not thereby fixed;
        // nor is one fixed on a machine's branch fixed on the main line.
        if self.without_code || self.server.is_some() {
            return None;
        }
        self.graph();
        let g = self.graph.as_ref().expect("just built");
        let pruned = lint::pruned(g, self.store.baseline.as_ref())?;
        self.store_mut().baseline = Some(pruned);
        Some(Changed::Baseline)
    }
    pub(super) fn store_mut(&mut self) -> &mut Store {
        self.graph = None;
        self.view = None;
        &mut self.store
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    use crate::extraction::fixtures::extraction;
    use crate::graph::fixtures::store;
    use crate::ops::ToolError;
    use crate::tools::call;
    use crate::tools::fixtures::{built, state};

    #[test]
    fn a_language_the_build_lacks_is_a_finding() {
        let mut s = built(state(), &["rust"]);
        s.store_mut().config.languages = vec!["rust".into(), "typescript".into()];
        let lint = call(&mut s, "lint", json!({ "kinds": ["malformed"] }), "2026-09-28").unwrap().output;
        assert_eq!(lint["findings"][0]["check"], "unsupported-language");
        let unknown_build = State::new(store(), extraction(), vec![]);
        let mut u = unknown_build;
        u.store_mut().config.languages = vec!["typescript".into()];
        assert!(call(&mut u, "lint", json!({ "kinds": ["malformed"] }), "2026-09-28").unwrap().output["findings"].as_array().unwrap().is_empty(), "an unknown build flags nothing");
    }

    /// # Test contract
    ///
    /// ```json
    /// {"test": "the-graph-follows-the-extraction", "checks": ["crates/core/src/tools/state#graph-current"]}
    /// ```
    #[test]
    fn the_graph_follows_the_extraction() {
        let mut s = state();
        assert!(call(&mut s, "get", json!({ "ref": "module:src/b/unrelated" }), "2026-09-28").is_ok());
        let files: Vec<_> = crate::extraction::fixtures::extraction_files().into_iter().filter(|f| f.path != "src/b/unrelated.rs").collect();
        s.set_extraction(Extraction::from_files(&files));
        let gone = call(&mut s, "get", json!({ "ref": "module:src/b/unrelated" }), "2026-09-28").unwrap_err();
        assert!(matches!(gone, ToolError::NotFound { .. }), "a read after an extraction change sees it");
    }
}
