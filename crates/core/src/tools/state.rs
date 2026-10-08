//! # Module contract
//!
//! ```json
//! {"purpose":"Hold what the tools operate on: the store, the extraction and the build, with the graph cached between calls and a reader the caller supplies for source.","criteria":[{"id":"graph-current","statement":"A read after a write sees the write; the cached graph is rebuilt whenever the store or the extraction changes."},{"id":"without-code","statement":"A state told it holds no code marks every graph it builds so, the truth's and a proposal's alike, never prunes the baseline, answers lint with without_code and status with shared, and refuses adopt as invalid."},{"id":"no-shared-current","statement":"A state that holds no code keeps nothing that is a checkout's own, no current ticket and no start: those its store arrived with are dropped, brew_ticket sets and records none, and a write that changes intent and names no ticket is refused as invalid."},{"id":"prune-to","statement":"prune_baseline_to drops from the baseline every entry the given findings lack and adds none, answering that the baseline changed and how many entries went, and nothing when none went; a read after it sees the smaller baseline."},{"id":"server-copy","statement":"A state told it is a machine's copy of a server's intent answers status with the server's address, when the intent was fetched, whether the server answered last and whether it refused this machine's token, and never prunes the baseline; any other state answers status with no server."},{"id":"omens","statement":"lint answers the omens of the graph read with the ticket it is given apart from its findings, whatever rules are selected, counting none of them in counts or new."},{"id":"advisories","statement":"lint judges the graph read with the ticket it is given and answers its findings and advisories: those advisories::own moves out against the truth alone judged over the base's code when a base is set, and those advisories::others gives for every other ticket brewing or simmering, judged over this state's code, its counts and new taken from the findings that stay, and as mended what advisories::mended gives over those findings, each waiting change's proposal laid beneath the ticket's and judged over this state's code; set_base sets or clears the base's code, and base_fingerprints answers the fingerprints of the truth over it while it is set."},{"id":"library","statement":"A state told it is served in a library answers status with its name there and every grimoire of that library, name, repository and the server it comes from, in name order, each checkout of its own grimoire with its current ticket's id and title, its own as it is now and another's as tell_checkout_tickets last told it, none for another grimoire's; a state told nothing answers with no name and an empty library."}]}
//! ```

use std::collections::BTreeMap;

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
    /// The languages this build reads, by an adapter or by their comments.
    pub supported_languages: Vec<String>,
    /// The tier this build reads each supported language at (req:extract-tiers).
    pub tiers: std::collections::BTreeMap<String, u8>,
}

impl Default for Build {
    fn default() -> Self {
        Build { version: env!("CARGO_PKG_VERSION").to_string(), supported_languages: vec![], tiers: Default::default() }
    }
}

/// Everything the tools operate on, with the graph cached between calls.
#[derive(Debug, Default)]
pub struct State {
    pub(super) store: Store,
    extraction: Extraction,
    /// The code this checkout's branch was cut from, when it has a base (req:check-own-work).
    base: Option<Extraction>,
    malformed: Vec<Malformed>,
    pub(super) build: Build,
    graph: Option<Graph>,
    /// The graph as one ticket's proposal would leave it, cached like the truth's.
    view: Option<(String, Graph)>,
    pub(super) source: Option<std::sync::Arc<dyn SourceReader>>,
    /// This grimoire's name in its daemon's library, and the library, when it is served in one.
    pub(super) library: Option<(String, Vec<crate::answer::LibraryEntry>)>,
    /// The current ticket of each other checkout of this store, by repository, as the daemon last told it.
    pub(super) checkout_tickets: BTreeMap<String, crate::answer::CurrentTicket>,
    /// Set by a daemon that holds no code: every graph is marked so, and the baseline is left alone.
    without_code: bool,
    /// Set on a machine whose store is a copy of a team's server's intent.
    server: Option<crate::answer::ServerCopy>,
    /// A store of its own that several checkouts share, each with its own current ticket.
    shares_checkouts: bool,
    /// Who is calling, and when, for the one call call_by runs (req:ticket-checked-out).
    pub(super) caller: Option<crate::model::CheckedOut>,
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
        State {
            store,
            extraction,
            base: None,
            malformed,
            build: Build::default(),
            graph: None,
            view: None,
            source: None,
            without_code: false,
            server: None,
            library: None,
            checkout_tickets: BTreeMap::new(),
            shares_checkouts: false,
            caller: None,
        }
    }
    /// The store's current ticket, the one last started.
    pub fn current(&self) -> Option<String> {
        self.store.current.clone()
    }
    /// The current ticket with its title, as `status` names it.
    pub fn current_ticket(&self) -> Option<crate::answer::CurrentTicket> {
        let id = self.store.current.as_ref()?;
        Some(crate::answer::CurrentTicket { id: id.clone(), title: self.store.tickets.get(id).map(|t| t.title.clone()).unwrap_or_default() })
    }
    /// Tell the state the current ticket of each other checkout of its grimoire, by repository,
    /// for `status`: a checkout read with a ticket is recorded, one read with none forgotten, and
    /// one not read this time keeps what it was last told. The graph does not depend on it.
    pub fn tell_checkout_tickets(&mut self, read: Vec<(String, Option<crate::answer::CurrentTicket>)>) {
        for (repository, ticket) in read {
            match ticket {
                Some(t) => {
                    self.checkout_tickets.insert(repository, t);
                }
                None => {
                    self.checkout_tickets.remove(&repository);
                }
            }
        }
    }
    /// The graph for a view: the truth when `ticket` is none, else the truth
    /// as that ticket's proposal would leave it. A ticket that is neither
    /// brewing nor simmering is a conflict, one that does not exist not_found.
    pub fn graph_for(&mut self, ticket: Option<&str>) -> Result<&Graph, crate::ops::ToolError> {
        let Some(ticket) = ticket else { return Ok(self.graph()) };
        match self.store.tickets.get(ticket) {
            None => return Err(crate::ops::ToolError::not_found(&crate::model::Reference::ticket(ticket))),
            Some(t) if !t.status.is_live() => {
                return Err(crate::ops::ToolError::conflict(format!("ticket:{ticket} is {}, neither brewing nor simmering: only a ticket being worked has a proposal to read", t.status.name())))
            }
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
    /// Set or clear the code this checkout's branch was cut from.
    pub fn set_base(&mut self, base: Option<Extraction>) {
        self.base = base;
    }

    /// The fingerprint of every node of the truth over the base's code, when a base is set: what a
    /// close compares this checkout's code with to tell its own work (req:tool-untouched).
    pub fn base_fingerprints(&self) -> Option<std::collections::BTreeMap<crate::model::Reference, String>> {
        self.base.as_ref().map(|b| crate::fingerprint::all(&Graph::build(&self.store, b, &self.malformed)))
    }

    /// The `lint` answer for the graph read with `ticket`: the findings this checkout's work causes,
    /// and as advisories what other work causes, intent that reached the main line since the base and each other ticket
    /// being worked (req:check-own-work).
    pub fn lint(&mut self, ticket: Option<&str>, rules: &[crate::lint::Rule]) -> Result<crate::answer::LintAnswer, crate::ops::ToolError> {
        use crate::{advisories, lint};
        let g = self.graph_for(ticket)?;
        let (findings, omens) = (lint::lint(g, rules), lint::omens(g));
        let without_code = self.without_code;
        let base = self.base.as_ref().map(|b| {
            let mut g = Graph::build(&self.store, b, &self.malformed);
            self.flag_languages(&mut g);
            g.without_code = without_code;
            lint::lint(&g, rules)
        });
        let (findings, mut advisories) = advisories::own(findings, base.as_deref(), &self.store, ticket);
        let truth = lint::lint(self.graph(), rules);
        let others = advisories::others(&self.store, ticket, &truth, &findings, |t| {
            let mut g = Graph::with_proposal(&self.store, t, &self.extraction, &self.malformed);
            self.flag_languages(&mut g);
            g.without_code = without_code;
            lint::lint(&g, rules)
        });
        advisories.extend(others);
        // What a change still simmering from this checkout would mend, laid beneath this ticket's proposal.
        let mended = advisories::mended(&self.store, ticket, &findings, |t| {
            let beneath = self.store.applied(t);
            let mut g = match ticket {
                Some(ticket) => Graph::with_proposal(&beneath, ticket, &self.extraction, &self.malformed),
                None => Graph::build(&beneath, &self.extraction, &self.malformed),
            };
            self.flag_languages(&mut g);
            g.without_code = without_code;
            lint::lint(&g, rules)
        });
        let (counts, new) = (lint::counts(&findings), lint::new_count(&findings));
        Ok(crate::answer::LintAnswer { findings, advisories, mended, omens, counts, new, without_code })
    }

    fn build_graph(&self, store: &Store) -> Graph {
        let mut g = Graph::build(store, &self.extraction, &self.malformed);
        self.flag_languages(&mut g);
        g.without_code = self.without_code;
        g
    }
    /// Say this state holds no code, as a shared daemon's does: what only
    /// code can show is then left out of every answer, never called missing.
    /// Say the store is a store of its own that several checkouts share, so a ticket another
    /// checkout is working can be taken up in this one.
    pub fn set_shares_checkouts(&mut self, shares: bool) {
        self.shares_checkouts = shares;
    }
    pub fn shares_checkouts(&self) -> bool {
        self.shares_checkouts
    }
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
    /// Tell the state it is served in a library, as `name`, beside the grimoires
    /// of `library`, which `status` lists in name order. The graph does not depend on it.
    pub fn set_library(&mut self, name: String, mut library: Vec<crate::answer::LibraryEntry>) {
        library.sort_by(|a, b| a.name.cmp(&b.name));
        self.library = Some((name, library));
    }
    /// Tell the state what build is running. The graph does not depend on it.
    pub fn set_build(&mut self, build: Build) {
        self.build = build;
    }
    /// The store files that did not load, as the graph reports them.
    pub fn malformed(&self) -> &[Malformed] {
        &self.malformed
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
    /// Drop from the baseline every entry `present` lacks, as the main line
    /// tells a store that holds no code; never adds. Returns what changed and
    /// how many entries went, when any did.
    pub fn prune_baseline_to(&mut self, present: &std::collections::HashSet<crate::model::FindingKey>) -> Option<(Changed, usize)> {
        let before = self.store.baseline.as_ref()?.findings.len();
        let pruned = lint::pruned_to(self.store.baseline.as_ref(), present)?;
        let went = before - pruned.findings.len();
        self.store_mut().baseline = Some(pruned);
        Some((Changed::Baseline, went))
    }
    /// Replace whole the recipes the main line last sent, as a store that holds no code is told
    /// them (req:recipe-main-line-snapshot). Returns what changed, none when they are as held.
    pub fn set_main_line_recipes(&mut self, recipes: crate::model::MainLineRecipes) -> Option<Changed> {
        if self.store.main_line_recipes.as_ref() == Some(&recipes) {
            return None;
        }
        self.store_mut().main_line_recipes = Some(recipes);
        Some(Changed::MainLineRecipes)
    }
    /// Hold `list` as who has ticket `slug` checked out, as a server answered it: who called is only the server's
    /// to say, so a machine's copy takes the server's list (req:ticket-checked-out).
    pub fn set_checked_out(&mut self, slug: &str, list: Vec<crate::model::CheckedOut>) {
        if list.is_empty() {
            self.store_mut().checked_out.remove(slug);
        } else {
            self.store_mut().checked_out.insert(slug.to_string(), list);
        }
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
    /// {"test": "lint-answers-what-other-work-causes-apart", "checks": ["crates/core/src/tools/state#advisories"]}
    /// ```
    #[test]
    fn lint_answers_what_other_work_causes_apart() {
        let mut s = State::new(store(), extraction(), vec![]);
        let dangling = json!({ "kinds": ["dangling"] });
        // With no base, req:stale's missing criterion is this checkout's finding.
        let lint = call(&mut s, "lint", dangling.clone(), "2026-10-06").unwrap().output;
        assert_eq!((lint["new"].as_u64(), lint["advisories"].as_array().map(Vec::len)), (Some(1), Some(0)), "{lint}");
        // The base's code lacks the criterion too: not this checkout's doing.
        s.set_base(Some(extraction()));
        let lint = call(&mut s, "lint", dangling.clone(), "2026-10-06").unwrap().output;
        assert_eq!((lint["new"].as_u64(), lint["counts"]["dangling"].as_u64()), (Some(0), Some(0)), "{lint}");
        assert_eq!(lint["advisories"][0]["from"]["kind"], "since_base");
        assert_eq!(lint["advisories"][0]["finding"]["ref"], "req:stale");
        // Another ticket being worked, whose proposal deletes the page req:stale's page links: what it would bring.
        s.set_base(None);
        s.store_mut().tickets.get_mut("phase-62").unwrap().status = crate::model::Status::Brewing;
        crate::ops::propose(s.store_mut(), "phase-62", crate::ops::Proposing::Partial, |w| crate::ops::delete_node(w, &crate::model::Reference::page("orphan")).map(|c| ((), c))).unwrap();
        s.store_mut().pages.get_mut("git-readiness").unwrap().links.push("page:orphan".parse().unwrap());
        let lint = call(&mut s, "lint", dangling, "2026-10-06").unwrap().output;
        let from: Vec<String> =
            lint["advisories"].as_array().unwrap().iter().map(|a| format!("{} {}", a["from"]["ticket"].as_str().unwrap_or(""), a["finding"]["ref"].as_str().unwrap_or(""))).collect();
        assert_eq!(from, vec!["phase-62 page:git-readiness"], "{lint}");
        assert_eq!(lint["new"].as_u64(), Some(1), "an advisory never counts as new");
    }

    /// # Test contract
    ///
    /// ```json
    /// {"test": "lint-answers-omens-apart", "checks": ["crates/core/src/tools/state#omens", "crates/core/src/tools/read#lint-tool"]}
    /// ```
    #[test]
    fn lint_answers_omens_apart() {
        let mut s = State::new(store(), extraction(), vec![]);
        let before = call(&mut s, "lint", json!({}), "2026-10-06").unwrap().output;
        assert_eq!(before["omens"], json!([]));
        // Every fixture file has 40 lines: at a limit of 40, the in-scope ones are omens.
        s.store_mut().config.large_file = Some(40);
        let after = call(&mut s, "lint", json!({ "kinds": ["dangling"] }), "2026-10-06").unwrap().output;
        let on: Vec<&str> = after["omens"].as_array().unwrap().iter().map(|o| o["ref"].as_str().unwrap()).collect();
        assert_eq!(on, vec!["file:src/a/layout.rs", "file:src/a/ready.rs", "file:src/b/unrelated.rs"], "whatever kinds names");
        let all = call(&mut s, "lint", json!({}), "2026-10-06").unwrap().output;
        assert_eq!((&all["new"], &all["counts"]), (&before["new"], &before["counts"]), "counted in neither");
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
