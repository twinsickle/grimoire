//! # Module contract
//!
//! ```json
//! {"purpose":"Join the store and the extraction into one graph of nodes and edges and derive every reverse edge.","criteria":[{"id":"tests","statement":"Each test is a node test:<file>#<name>, contained by its file, with a checks edge to each criterion or requirement it names; what a criterion or requirement is checked by is derived from those edges."},{"id":"contract-scope","statement":"A module is expected to carry a contract only when its adapter expects one and the store config's scope covers it."},{"id":"downward-only-input","statement":"Edges are read only from the node that owns them, the one edited when the relationship changes; every reverse relation is derived here and nowhere else."},{"id":"dangling-kept","statement":"An edge whose target does not exist is kept in the graph so the lints can report it."},{"id":"retired-covers-nothing","statement":"A retired requirement makes no covered_by edge, so it covers no criterion; its covered_by stays in its record as history."},{"id":"with-proposal","statement":"with_proposal builds the graph from the store as a ticket's proposal would leave it, the proposed pages and requirements in place of the truth's and the deleted ones gone, joined with the same extraction, and each ticket whose references it edits with the references the edit holds, so every query and lint answers over a branch's own intent when asked to; such a graph names as judged that ticket and the tickets it edits, and the truth's graph names none; it names the ticket it is read with, and holds that ticket's moved records when it is brewing as well as every simmering ticket's."},{"id":"recipes","statement":"Each recipe is a node recipe:<id> with a uses edge to recipe:<id> for each id it uses and a represents edge to each module holding a file at or under a path its files name, and each of its ingredients a node recipe:<id>#<ingredient> that belongs to it; of two recipes with one id the first by file path is the node and every file holding that id is kept for the lint, with the recipe files that could not be read, the config's recipe kinds and the store's judgements of recipes."},{"id":"decision-node","statement":"Each bottled decision of a requirement is a node req:<slug>#d<id> that belongs to its requirement."},{"id":"decision-supersedes-edge","statement":"A decision node has a supersedes edge to req:<slug>#d<n> for each id it supersedes, kept when that decision is not there so the lint reports it."},{"id":"decision-unbottled-no-node","statement":"A decision not yet bottled is no node."},{"id":"decision-title","statement":"A decision node's title is how it is said, decision <id> of <slug>."},{"id":"decision-not-found","statement":"find matches nothing of a decision node."},{"id":"rites","statement":"A page or requirement is a rite when it is under the rites root and not under the grimoire's, under meaning a page reached from that root through page links, the root included, or a requirement one of them links; one under both is the code's, and with a root missing nothing is under it."}]}
//! ```

use std::collections::{BTreeMap, HashMap, HashSet};

use serde::Serialize;

use crate::extraction::Extraction;
use crate::model::*;
use crate::store::{Malformed, Store};

#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(untagged)]
pub enum Node {
    Page(Page),
    Req(Requirement),
    Ticket(Ticket),
    Module(Module),
    Criterion(Criterion),
    File(File),
    Decl(Decl),
    Test(Test),
    Recipe(Recipe),
    Ingredient(Ingredient),
    Decision(DecisionNode),
}

impl Node {
    pub fn kind(&self) -> Kind {
        match self {
            Node::Page(_) => Kind::Page,
            Node::Req(_) | Node::Decision(_) => Kind::Req,
            Node::Ticket(_) => Kind::Ticket,
            Node::Module(_) => Kind::Module,
            Node::Criterion(_) => Kind::Criterion,
            Node::File(_) => Kind::File,
            Node::Decl(_) => Kind::Decl,
            Node::Test(_) => Kind::Test,
            Node::Recipe(_) | Node::Ingredient(_) => Kind::Recipe,
        }
    }

    /// The one line that names the node: title, statement, purpose or path.
    pub fn title(&self) -> String {
        match self {
            Node::Page(p) => p.title.clone(),
            Node::Req(r) => r.title.clone(),
            Node::Ticket(t) => t.title.clone(),
            Node::Module(m) => m.purpose.clone().unwrap_or_else(|| m.id.clone()),
            Node::Criterion(c) => c.statement.clone(),
            Node::File(f) => f.path.clone(),
            Node::Decl(d) => d.path.clone(),
            Node::Test(t) => t.name.clone(),
            Node::Recipe(r) => r.title.clone(),
            Node::Ingredient(i) => i.says.clone(),
            Node::Decision(d) => format!("decision {} of {}", d.id, d.requirement),
        }
    }

    /// Every text a search matches against.
    pub fn searchable(&self) -> Vec<&str> {
        match self {
            Node::Page(p) => vec![&p.slug, &p.title, &p.body],
            Node::Req(r) => vec![&r.slug, &r.title, &r.statement],
            Node::Ticket(t) => vec![&t.slug, &t.title, &t.body],
            Node::Module(m) => {
                let mut v = vec![m.id.as_str()];
                if let Some(p) = &m.purpose {
                    v.push(p);
                }
                v
            }
            Node::Criterion(c) => vec![&c.id, &c.statement],
            Node::File(f) => vec![&f.path],
            Node::Decl(d) => vec![&d.path],
            Node::Test(t) => vec![&t.name],
            Node::Recipe(r) => vec![&r.id, &r.title, &r.text],
            Node::Ingredient(i) => vec![&i.id, &i.says],
            // Read through its requirement: a decision's text is not what a reference is found by.
            Node::Decision(_) => vec![],
        }
    }
}

#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize)]
pub struct Edge {
    pub from: Reference,
    pub to: Reference,
    pub relation: Relation,
}

/// The joined graph. Built from values, holds values.
#[derive(Debug, Clone, Default)]
pub struct Graph {
    pub nodes: BTreeMap<Reference, Node>,
    pub edges: Vec<Edge>,
    out: HashMap<Reference, Vec<usize>>,
    inbound: HashMap<Reference, Vec<usize>>,
    pub roots: Vec<String>,
    /// The pages and requirements under the rites root and not under the grimoire's: the rites.
    pub rites: HashSet<Reference>,
    /// The two roots as the config names them, for the lint to say which is missing.
    pub named_roots: Roots,
    pub malformed: Vec<Malformed>,
    pub unresolved_imports: usize,
    /// The store's baseline, as identities to match findings against.
    pub baseline: HashSet<FindingKey>,
    /// The graph was built by a daemon that holds no code: what only code can
    /// show is unknown, not missing, and the lint leaves it out.
    pub without_code: bool,
    /// Bottling tickets the truth moved under, each with the records that moved, for the lint.
    pub simmering_moved: BTreeMap<String, Vec<String>>,
    /// Configured languages the running build cannot extract, and the ones
    /// it can; set by whoever knows the build (`State`), empty otherwise.
    pub unsupported_languages: Vec<String>,
    pub supported_languages: Vec<String>,
    /// Test blocks that could not be read, for the lint.
    pub test_errors: Vec<TestError>,
    /// Recipe files that could not be read, for the lint.
    pub recipe_errors: Vec<RecipeError>,
    /// Each recipe id held by more than one file, with every such file by path, for the lint.
    pub recipe_duplicates: BTreeMap<String, Vec<String>>,
    /// The project's recipe kinds, from the config, for the lint.
    pub recipe_kinds: BTreeMap<String, RecipeKind>,
    /// Each judged recipe's judgements, from the store, to tell which recipes are due.
    pub judgements: BTreeMap<String, Vec<Judgement>>,
    /// The order set within each priority band, for the board.
    pub order: Order,
    /// The tickets whose references the lint judges: none on the truth's
    /// graph; read with a ticket, that ticket and those whose references its
    /// proposal edits, since a ticket's references are its proposal state.
    pub judged: HashSet<String>,
    /// The ticket the graph is read with, none for the truth's: a finding only its owner can mend,
    /// as a bottling whose truth moved, is judged on this one alone (req:lint-simmering-moved).
    pub read_with: Option<String>,
    /// For each ticket whose references a proposal would change, the tickets
    /// proposing it.
    pub drafted: BTreeMap<String, Vec<String>>,
    /// Who has each ticket checked out and since when, by slug, as the store keeps it (req:ticket-checked-out).
    pub checked_out: BTreeMap<String, Vec<crate::model::CheckedOut>>,
    /// A file of this many lines or more, its tests aside, is an omen: the config's limit or its default.
    pub large_file: u32,
    /// A file of more lines than this is shown plain.
    pub plain_file: u32,
}

impl Graph {
    /// The graph as `ticket`'s proposal would leave it: the truth with the
    /// proposed records in place and the deleted ones gone, over the same
    /// extraction. The truth's graph when the ticket has no proposal.
    pub fn with_proposal(store: &Store, ticket: &str, extraction: &Extraction, malformed: &[Malformed]) -> Graph {
        let mut g = Graph::build(&store.applied(ticket), extraction, malformed);
        // Judged against the truth itself: with its own proposal laid on top, a ticket's records never look moved.
        g.simmering_moved = crate::ops::simmering_moved(store);
        // The ticket read with is told when the truth moved under it while it is still brewing too (req:lint-simmering-moved).
        if store.tickets.get(ticket).is_some_and(|t| t.status == crate::model::Status::Brewing) {
            let refs: Vec<String> = crate::ops::moved(store, ticket).into_iter().map(|e| e.field).collect();
            if !refs.is_empty() {
                g.simmering_moved.insert(ticket.to_string(), refs);
            }
        }
        g.drafted = store.ticket_drafts();
        // Its references, and those its edits leave other tickets with, are what this reading judges.
        g.read_with = Some(ticket.to_string());
        g.judged = std::iter::once(ticket.to_string()).chain(store.proposals.get(ticket).into_iter().flat_map(|p| p.tickets.keys().cloned())).collect();
        g
    }

    pub fn build(store: &Store, extraction: &Extraction, malformed: &[Malformed]) -> Graph {
        let mut g = Graph {
            read_with: None,
            roots: store.config.roots.list().into_iter().cloned().collect(),
            named_roots: store.config.roots.clone(),
            malformed: malformed.to_vec(),
            unresolved_imports: extraction.unresolved_imports,
            baseline: store.baseline.iter().flat_map(|b| b.findings.iter().cloned()).collect(),
            order: store.config.order.clone(),
            simmering_moved: crate::ops::simmering_moved(store),
            drafted: store.ticket_drafts(),
            checked_out: store.checked_out.clone(),
            large_file: store.config.large_file(),
            plain_file: store.config.plain_file(),
            ..Default::default()
        };

        for p in store.pages.values() {
            let from = Reference::page(&p.slug);
            for l in &p.links {
                g.edge(&from, l, Relation::Links);
            }
            g.nodes.insert(from, Node::Page(p.clone()));
        }
        for r in store.requirements.values() {
            let from = Reference::req(&r.slug);
            // A retired requirement no longer holds, so it covers nothing.
            for c in r.covered_by.iter().filter(|_| !r.retired) {
                g.edge(&from, c, Relation::CoveredBy);
            }
            // Each bottled decision, by its id: what a reference to one names (req:decision-references).
            for d in &r.decisions {
                let Some(id) = d.id else { continue };
                let dref = Reference::decision(&r.slug, id);
                g.edge(&dref, &from, Relation::BelongsTo);
                for s in &d.supersedes {
                    g.edge(&dref, &Reference::decision(&r.slug, *s), Relation::Supersedes);
                }
                let node = DecisionNode { requirement: r.slug.clone(), id, date: d.date.clone(), text: d.text.clone(), supersedes: d.supersedes.clone(), adopted: d.adopted };
                g.nodes.insert(dref, Node::Decision(node));
            }
            g.nodes.insert(from, Node::Req(r.clone()));
        }
        for t in store.tickets.values() {
            let from = Reference::ticket(&t.slug);
            for x in &t.touches {
                g.edge(&from, x, Relation::Touches);
            }
            for x in &t.relies_on {
                g.edge(&from, x, Relation::ReliesOn);
            }
            g.nodes.insert(from, Node::Ticket(t.clone()));
        }
        for m in extraction.modules.values() {
            let from = Reference::module(&m.id);
            for f in &m.files {
                g.edge(&from, &Reference::file(f), Relation::Contains);
            }
            for i in &m.imports {
                g.edge(&from, &Reference::module(i), Relation::Imports);
            }
            for c in &m.criteria {
                let cref = Reference::criterion(&c.module, &c.id);
                g.edge(&cref, &from, Relation::BelongsTo);
                g.nodes.insert(cref, Node::Criterion(c.clone()));
            }
            let mut m = m.clone();
            m.expects_contract &= store.config.scope.covers(&m.files);
            g.nodes.insert(from, Node::Module(m));
        }
        for t in &extraction.tests {
            let from = Reference::test(&t.file, &t.name);
            g.edge(&Reference::file(&t.file), &from, Relation::Contains);
            for c in &t.checks {
                // A constraint is held by naming its requirement; anything else names a criterion.
                let to = match c.strip_prefix("req:") {
                    Some(slug) => Reference::req(slug),
                    None => {
                        let (module, id) = c.rsplit_once('#').unwrap_or((c.as_str(), ""));
                        Reference::criterion(module, id)
                    }
                };
                g.edge(&from, &to, Relation::Checks);
            }
            g.nodes.insert(from, Node::Test(t.clone()));
        }
        g.test_errors = extraction.test_errors.clone();
        g.recipes(store, extraction);
        for f in extraction.files.values() {
            let from = Reference::file(&f.path);
            for d in &f.decls {
                let dref = Reference::decl(&f.path, &d.path);
                g.edge(&from, &dref, Relation::Contains);
                g.nodes.insert(dref, Node::Decl(Decl { file: f.path.clone(), ..d.clone() }));
            }
            g.nodes.insert(from, Node::File(f.clone()));
        }
        // What sits under the rites and not under the grimoire: the rites, held by the work.
        let (grimoire, rites) = (g.under(store.config.roots.grimoire.as_deref()), g.under(store.config.roots.rites.as_deref()));
        g.rites = rites.difference(&grimoire).cloned().collect();
        g
    }

    /// Each recipe a node with its ingredients and uses; the first of one id by path, the rest kept as duplicates.
    fn recipes(&mut self, store: &Store, extraction: &Extraction) {
        self.recipe_errors = extraction.recipe_errors.clone();
        self.recipe_kinds = store.config.recipe_kinds.clone();
        self.judgements = store.judgements.clone();
        let mut by_path: Vec<&Recipe> = extraction.recipes.iter().collect();
        by_path.sort_by(|a, b| a.file.cmp(&b.file));
        let mut files_of: BTreeMap<&str, Vec<String>> = BTreeMap::new();
        for r in by_path {
            let held = files_of.entry(&r.id).or_default();
            held.push(r.file.clone());
            if held.len() > 1 {
                continue;
            }
            let from = Reference::recipe(&r.id);
            for u in &r.uses {
                self.edge(&from, &Reference::recipe(u), Relation::Uses);
            }
            // What it represents, at the level the blast radius is worked at: each module holding a file it names (req:recipe-in-tickets).
            for m in extraction.modules.values().filter(|m| m.files.iter().any(|f| r.files.iter().any(|named| at_or_under(f, named)))) {
                self.edge(&from, &Reference::module(&m.id), Relation::Represents);
            }
            for i in &r.ingredients {
                let iref = Reference::ingredient(&r.id, &i.id);
                self.edge(&iref, &from, Relation::BelongsTo);
                self.nodes.insert(iref, Node::Ingredient(i.clone()));
            }
            self.nodes.insert(from, Node::Recipe(r.clone()));
        }
        self.recipe_duplicates = files_of.into_iter().filter(|(_, f)| f.len() > 1).map(|(id, f)| (id.to_string(), f)).collect();
    }

    /// Whether a recipe is due: no judgement of it was made at its digest as it is now (req:recipe-due).
    pub fn is_due(&self, x: &Recipe) -> bool {
        !self.judgements.get(&x.id).is_some_and(|all| all.iter().any(|j| j.fingerprint == x.digest))
    }

    fn edge(&mut self, from: &Reference, to: &Reference, relation: Relation) {
        let idx = self.edges.len();
        self.edges.push(Edge { from: from.clone(), to: to.clone(), relation });
        self.out.entry(from.clone()).or_default().push(idx);
        self.inbound.entry(to.clone()).or_default().push(idx);
    }

    pub fn get(&self, r: &Reference) -> Option<&Node> {
        self.nodes.get(r)
    }

    pub fn contains(&self, r: &Reference) -> bool {
        self.nodes.contains_key(r)
    }

    /// Edges owned by `r`, in the order the node wrote them.
    pub fn out(&self, r: &Reference) -> impl Iterator<Item = &Edge> {
        self.out.get(r).into_iter().flatten().map(|i| &self.edges[*i])
    }

    /// Edges pointing at `r`, derived.
    pub fn inbound(&self, r: &Reference) -> impl Iterator<Item = &Edge> {
        self.inbound.get(r).into_iter().flatten().map(|i| &self.edges[*i])
    }

    pub fn out_with(&self, r: &Reference, relation: Relation) -> impl Iterator<Item = &Edge> {
        self.out(r).filter(move |e| e.relation == relation)
    }

    pub fn inbound_with(&self, r: &Reference, relation: Relation) -> impl Iterator<Item = &Edge> {
        self.inbound(r).filter(move |e| e.relation == relation)
    }

    pub fn tickets(&self) -> impl Iterator<Item = &Ticket> {
        self.nodes.values().filter_map(|n| match n {
            Node::Ticket(t) => Some(t),
            _ => None,
        })
    }

    /// The pages and requirements under the page `root`: every page reached from it through page
    /// links, itself included, and every requirement one of them links. Empty when it names no page.
    fn under(&self, root: Option<&str>) -> HashSet<Reference> {
        let mut seen: HashSet<Reference> = HashSet::new();
        let mut stack: Vec<Reference> = root.map(Reference::page).filter(|r| self.contains(r)).into_iter().collect();
        while let Some(r) = stack.pop() {
            if !seen.insert(r.clone()) {
                continue;
            }
            for e in self.out(&r).filter(|e| e.relation == Relation::Links) {
                match e.to.kind {
                    Kind::Page if self.contains(&e.to) => stack.push(e.to.clone()),
                    Kind::Req if self.contains(&e.to) => {
                        seen.insert(e.to.clone());
                    }
                    _ => {}
                }
            }
        }
        seen
    }

    /// Whether a page or requirement is under the rites and not under the grimoire: a rite, held by the work rather than the code.
    pub fn is_rite(&self, r: &Reference) -> bool {
        self.rites.contains(r)
    }

    pub fn is_root(&self, slug: &str) -> bool {
        self.roots.iter().any(|r| r == slug)
    }

    /// The line a criterion or declaration lives at, for click-through.
    pub fn source_of(&self, r: &Reference) -> Option<SourceLocation> {
        match (r.kind, self.get(r)?) {
            (Kind::Module, Node::Module(m)) => {
                let (file, line) = m.contract_at.clone().or_else(|| m.files.first().map(|f| (f.clone(), 1)))?;
                Some(SourceLocation { file, line, contract_line: m.contract_at.as_ref().map(|c| c.1) })
            }
            (Kind::Criterion, Node::Criterion(c)) => {
                let m = match self.get(&Reference::module(&c.module))? {
                    Node::Module(m) => m,
                    _ => return None,
                };
                let (file, line) = m.contract_at.clone()?;
                Some(SourceLocation { file, line, contract_line: Some(line) })
            }
            (Kind::File, Node::File(f)) => Some(SourceLocation { file: f.path.clone(), line: 1, contract_line: None }),
            (Kind::Decl, Node::Decl(d)) => Some(SourceLocation { file: d.file.clone(), line: d.line, contract_line: None }),
            (Kind::Test, Node::Test(t)) => Some(SourceLocation { file: t.file.clone(), line: t.line, contract_line: None }),
            (Kind::Recipe, Node::Recipe(r)) => Some(SourceLocation { file: r.file.clone(), line: r.line, contract_line: None }),
            _ => None,
        }
    }
}

/// Whether a repository path is `named` or lies in the folder it names.
fn at_or_under(path: &str, named: &str) -> bool {
    path == named || path.strip_prefix(named).is_some_and(|rest| rest.starts_with('/'))
}

#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct SourceLocation {
    pub file: String,
    pub line: u32,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[cfg_attr(feature = "ts", ts(optional))]
    pub contract_line: Option<u32>,
}

#[cfg(test)]
pub(crate) mod fixtures {
    use super::*;
    use crate::extraction::fixtures::extraction;
    use std::collections::BTreeMap;

    pub fn page(slug: &str, links: &[&str]) -> Page {
        Page { slug: slug.into(), title: slug.to_uppercase(), body: format!("About {slug}."), links: links.iter().map(|l| l.parse().unwrap()).collect() }
    }

    /// A requirement with one scenario; `stale` has none, so the unspecified
    /// rule has something to report.
    pub fn req(slug: &str, covered_by: &[&str], decisions: &[(&str, &str)]) -> Requirement {
        let scenarios = if slug == "stale" { vec![] } else { vec![Scenario { title: "It holds".into(), given: "a repository".into(), when: format!("{slug} is checked"), then: "it holds".into() }] };
        Requirement {
            slug: slug.into(),
            title: slug.to_uppercase(),
            statement: format!("The system SHALL {slug}."),
            scenarios,
            covered_by: covered_by.iter().map(|l| l.parse().unwrap()).collect(),
            decisions: (1u32..).zip(decisions).map(|(id, (d, t))| Decision { id: Some(id), ..Decision::new(d.to_string(), t.to_string()) }).collect(),
            retired: false,
        }
    }

    pub fn ticket(slug: &str, kind: TicketKind, priority: Priority, status: Status, touches: &[&str], relies_on: &[&str]) -> Ticket {
        Ticket {
            slug: slug.into(),
            title: slug.to_uppercase(),
            kind,
            priority,
            status,
            touches: touches.iter().map(|l| l.parse().unwrap()).collect(),
            relies_on: relies_on.iter().map(|l| l.parse().unwrap()).collect(),
            body: if status == Status::Bottled { String::new() } else { "Do it.".into() },
            decision: if matches!(status, Status::Simmering | Status::Bottled) { Some(Decision::new("2026-01-01", "Done.")) } else { None },
        }
    }

    /// overview -> git-readiness -> {ready-only-with-commit -> guarantee, module ready}.
    /// `orphan` page is linked by nothing. `stale` req names a criterion that
    /// does not exist and has no scenario. Criterion `reads-only` and `stable` are uncovered.
    pub fn store() -> Store {
        let mut s = Store::new(StoreConfig {
            large_file: None,
            plain_file: None,
            site: vec![],
            version: StoreConfig::CURRENT_VERSION,
            repository: "C:/repo".into(),
            roots: Roots::new("overview", "rites"),
            languages: vec!["rust".into()],
            scope: Scope::default(),
            order: Default::default(),
            recipe_kinds: Default::default(),
        });
        for p in [
            page("overview", &["page:git-readiness"]),
            page("git-readiness", &["req:ready-only-with-commit", "req:stale", "module:src/a/ready"]),
            page("orphan", &[]),
            // The second root every store has, with nothing beneath it yet.
            page("rites", &[]),
        ] {
            s.pages.insert(p.slug.clone(), p);
        }
        for r in
            [req("ready-only-with-commit", &["criterion:src/a/ready#guarantee"], &[("2026-09-12", "Identity is required rather than invented.")]), req("stale", &["criterion:src/a/ready#gone"], &[])]
        {
            s.requirements.insert(r.slug.clone(), r);
        }
        for t in [
            ticket("phase-62", TicketKind::Change, Priority::Next, Status::Inscribed, &["req:ready-only-with-commit", "module:src/a/ready"], &["page:git-readiness"]),
            ticket("gap-1", TicketKind::Gap, Priority::Later, Status::Inscribed, &["module:src/a/ready"], &[]),
            ticket("later-1", TicketKind::Change, Priority::Later, Status::Brewing, &["page:orphan"], &[]),
            ticket("old", TicketKind::Change, Priority::High, Status::Bottled, &["page:renamed-away"], &[]),
        ] {
            s.tickets.insert(t.slug.clone(), t);
        }
        let _: BTreeMap<String, Page> = BTreeMap::new();
        s
    }

    pub fn graph() -> Graph {
        Graph::build(&store(), &extraction(), &[])
    }
}

#[cfg(test)]
mod tests {
    use super::fixtures::graph;
    use super::*;

    /// # Test contract
    ///
    /// ```json
    /// {"test": "tests-point-down-at-criteria", "checks": ["crates/core/src/graph#tests", "crates/core/src/query/source#module-source"]}
    /// ```
    #[test]
    fn tests_point_down_at_criteria() {
        let g = graph();
        let t = Reference::test("src/a/ready.rs", "ready-needs-a-commit");
        assert!(matches!(g.get(&t), Some(Node::Test(_))));
        assert!(g.inbound_with(&t, Relation::Contains).any(|e| e.from == Reference::file("src/a/ready.rs")));
        let criterion = Reference::criterion("src/a/ready", "guarantee");
        assert!(g.inbound_with(&criterion, Relation::Checks).any(|e| e.from == t), "the reverse is derived");
        assert_eq!(g.source_of(&t).map(|s| (s.file, s.line)), Some(("src/a/ready.rs".to_string(), 20)));
    }

    /// # Test contract
    ///
    /// ```json
    /// {"test": "derives-reverse-edges", "checks": ["crates/core/src/graph#downward-only-input"]}
    /// ```
    #[test]
    fn derives_reverse_edges() {
        let g = graph();
        let module = Reference::module("src/a/ready");
        let inbound: Vec<_> = g.inbound(&module).map(|e| (e.from.to_string(), e.relation)).collect();
        assert!(inbound.contains(&("page:git-readiness".to_string(), Relation::Links)));
        assert!(inbound.contains(&("ticket:phase-62".to_string(), Relation::Touches)));
        assert!(inbound.contains(&("criterion:src/a/ready#guarantee".to_string(), Relation::BelongsTo)));
        assert!(g.inbound_with(&Reference::module("src/a/layout"), Relation::Imports).any(|e| e.from == module));
    }

    /// # Test contract
    ///
    /// ```json
    /// {"test": "keeps-dangling-edges", "checks": ["crates/core/src/graph#dangling-kept"]}
    /// ```
    #[test]
    fn keeps_dangling_edges() {
        let g = graph();
        let stale = Reference::req("stale");
        let e = g.out(&stale).next().unwrap();
        assert!(!g.contains(&e.to));
    }

    /// # Test contract
    ///
    /// ```json
    /// {"test": "source-of-criterion-is-the-contract-line", "checks": ["crates/core/src/query/source#module-source"]}
    /// ```
    #[test]
    fn source_of_criterion_is_the_contract_line() {
        let g = graph();
        let loc = g.source_of(&Reference::criterion("src/a/ready", "guarantee")).unwrap();
        assert_eq!(loc.file, "src/a/ready.rs");
        assert_eq!(loc.contract_line, Some(1));
        let d = g.source_of(&Reference::decl("src/a/ready.rs", "run")).unwrap();
        assert_eq!(d.line, 10);
    }

    /// # Test contract
    ///
    /// ```json
    /// {"test": "a-hash-in-a-file-name-misplaces-nothing", "checks": ["crates/core/src/model/reference#kinds-split-last", "crates/core/src/model/reference#kinds-decl-file", "crates/core/src/query/source#module-source"]}
    /// ```
    #[test]
    fn a_hash_in_a_file_name_misplaces_nothing() {
        use crate::extraction::fixtures::{contract, file};
        // A module whose file name holds #, with a criterion, a test block and
        // a declaration whose own path holds # (a TypeScript private field).
        let mut f = file("src/a#b.ts", "src/a#b", &[], contract("Hold a hash.", &[("kept", "It holds.")]));
        f.language = "typescript".into();
        f.decls = vec![Decl { path: "Box::#secret".into(), kind: "method".into(), line: 7, file: "src/a#b.ts".into() }];
        f.tests = vec![Test { file: "src/a#b.ts".into(), name: "holds".into(), line: 20, checks: vec!["src/a#b#kept".into()] }];
        let g = Graph::build(&crate::graph::fixtures::store(), &Extraction::from_files(&[f]), &[]);
        let criterion = Reference::criterion("src/a#b", "kept");
        assert!(g.contains(&criterion));
        assert_eq!(criterion.split_hash(), Some(("src/a#b", "kept")));
        let test = Reference::test("src/a#b.ts", "holds");
        assert_eq!(test.split_hash(), Some(("src/a#b.ts", "holds")));
        assert!(g.out(&test).any(|e| e.relation == Relation::Checks && e.to == criterion), "the check names the criterion, not src/a");
        let decl = Reference::decl("src/a#b.ts", "Box::#secret");
        assert_eq!(decl.split_hash(), None, "a declaration's file is on its node");
        let loc = g.source_of(&decl).expect("the declaration is found");
        assert_eq!((loc.file.as_str(), loc.line), ("src/a#b.ts", 7));
        assert_eq!(g.source_of(&criterion).map(|l| l.file), Some("src/a#b.ts".to_string()));
    }

    /// # Test contract
    ///
    /// ```json
    /// {"test": "a-recipe-is-a-node-with-its-ingredients", "checks": ["crates/core/src/graph#recipes", "crates/core/src/model/reference#kinds-ids"]}
    /// ```
    #[test]
    fn a_recipe_is_a_node_with_its_ingredients() {
        use crate::extraction::fixtures::{extraction_files, recipe_file};
        let mut files = extraction_files();
        files.push(recipe_file("ui/button.recipe.md", "button", "component", &["icon", "nowhere"], &[], &[]));
        files.push(recipe_file("ui/icon.recipe.md", "icon", "component", &[], &[], &[]));
        files.push(recipe_file("z/button.recipe.md", "button", "token", &[], &[], &[]));
        let mut s = super::fixtures::store();
        s.config.recipe_kinds.insert("component".into(), RecipeKind { uses: vec!["component".into()] });
        let g = Graph::build(&s, &Extraction::from_files(&files), &[]);
        let button = Reference::recipe("button");
        assert!(matches!(g.get(&button), Some(Node::Recipe(r)) if r.file == "ui/button.recipe.md" && r.kind == "component"), "the first by path is the node");
        let label = Reference::ingredient("button", "label");
        assert!(matches!(g.get(&label), Some(Node::Ingredient(i)) if i.recipe == "button"));
        assert!(g.out_with(&label, Relation::BelongsTo).any(|e| e.to == button));
        assert!(g.inbound_with(&Reference::recipe("icon"), Relation::Uses).any(|e| e.from == button), "the reverse is derived");
        assert!(g.out_with(&button, Relation::Uses).any(|e| e.to == Reference::recipe("nowhere")), "a use leading nowhere is kept");
        assert_eq!(g.recipe_duplicates["button"], ["ui/button.recipe.md", "z/button.recipe.md"]);
        assert!(g.recipe_kinds.contains_key("component"));
        assert_eq!(g.get(&button).map(|n| n.kind()), Some(Kind::Recipe));
        assert_eq!(g.source_of(&button).map(|s| (s.file, s.line)), Some(("ui/button.recipe.md".to_string(), 3)));
        assert!(!g.nodes.keys().any(|r| r.kind == Kind::Module && r.id.is_empty()), "a recipe file is no module");
    }

    /// # Test contract
    ///
    /// ```json
    /// {"test": "a-recipe-represents-the-modules-holding-its-files", "checks": ["crates/core/src/graph#recipes", "req:recipe-in-tickets"]}
    /// ```
    #[test]
    fn a_recipe_represents_the_modules_holding_its_files() {
        use crate::extraction::fixtures::{extraction_files, recipe_file};
        let mut files = extraction_files();
        // A folder, a file, a path of no module, and a name that only begins like a folder's.
        files.push(recipe_file("ui/panel.recipe.md", "panel", "component", &[], &["src/a", "src/b/unrelated.rs", "docs/notes.txt", "src/a/rea"], &[]));
        files.push(recipe_file("ui/ready.recipe.md", "ready", "component", &[], &["src/a/ready.rs"], &[]));
        let mut s = super::fixtures::store();
        s.judgements.insert(
            "ready".into(),
            vec![Judgement { recipe: "ready".into(), fingerprint: "0123456789abcdef".into(), date: "2026-10-07".into(), ticket: "later-1".into(), outcome: JudgementOutcome::Held }],
        );
        let g = Graph::build(&s, &Extraction::from_files(&files), &[]);
        let ends = |id: &str| -> Vec<String> { g.out_with(&Reference::recipe(id), Relation::Represents).map(|e| e.to.to_string()).collect() };
        assert_eq!(ends("panel"), ["module:src/a/layout", "module:src/a/ready", "module:src/b/unrelated"]);
        assert_eq!(ends("ready"), ["module:src/a/ready"]);
        assert!(g.inbound_with(&Reference::module("src/a/ready"), Relation::Represents).any(|e| e.from == Reference::recipe("ready")), "the reverse is derived");
        assert_eq!(g.judgements["ready"].len(), 1, "the store's judgements are held for the lint");
        assert!(at_or_under("src/a/ready.rs", "src/a") && !at_or_under("src/ab/x.rs", "src/a") && !at_or_under("src/a/ready.rs", "src/a/rea"));
    }

    /// # Test contract
    ///
    /// ```json
    /// {"test": "a-proposal-is-read-as-a-branch", "checks": ["crates/core/src/graph#with-proposal"]}
    /// ```
    #[test]
    fn a_proposal_is_read_as_a_branch() {
        let mut s = super::fixtures::store();
        let ext = crate::extraction::fixtures::extraction();
        crate::ops::propose(&mut s, "later-1", crate::ops::Proposing::Whole, |w| {
            crate::ops::put_page(w, Page { slug: "aim".into(), title: "Aim".into(), body: "b".into(), links: vec![Reference::module("src/a/ready")] })
        })
        .unwrap();
        crate::ops::propose(&mut s, "later-1", crate::ops::Proposing::Partial, |w| crate::ops::delete_node(w, &Reference::page("orphan")).map(|c| ((), c))).unwrap();
        let branch = Graph::with_proposal(&s, "later-1", &ext, &[]);
        assert!(branch.get(&Reference::page("aim")).is_some() && branch.get(&Reference::page("orphan")).is_none());
        assert!(branch.inbound(&Reference::module("src/a/ready")).any(|e| e.from == Reference::page("aim")), "joined with the same extraction");
        let truth = Graph::build(&s, &ext, &[]);
        assert!(truth.get(&Reference::page("aim")).is_none() && truth.get(&Reference::page("orphan")).is_some());
        // A ticket with no proposal is the truth.
        assert!(Graph::with_proposal(&s, "phase-62", &ext, &[]).get(&Reference::page("orphan")).is_some());
    }

    /// # Test contract
    ///
    /// ```json
    /// {"test": "a-decision-is-a-node-references-name", "checks": ["crates/core/src/graph#decision-node", "crates/core/src/graph#decision-supersedes-edge", "crates/core/src/graph#decision-unbottled-no-node", "crates/core/src/graph#decision-title", "crates/core/src/graph#decision-not-found", "crates/core/src/lint#dangling-edge"]}
    /// ```
    #[test]
    fn a_decision_is_a_node_references_name() {
        let mut s = fixtures::store();
        let q = s.requirements.get_mut("ready-only-with-commit").unwrap();
        q.decisions.push(Decision { id: Some(2), supersedes: vec![1], ..Decision::new("2026-10-01", "Instead.") });
        // One with no id, as a decision waiting in a proposal is read through it, is no node.
        q.decisions.push(Decision::new("2026-10-02", "Waiting."));
        s.tickets.get_mut("later-1").unwrap().relies_on = vec!["req:ready-only-with-commit#d2".parse().unwrap(), "req:ready-only-with-commit#d9".parse().unwrap()];
        let g = Graph::build(&s, &crate::extraction::fixtures::extraction(), &[]);
        let d2 = Reference::decision("ready-only-with-commit", 2);
        let Some(Node::Decision(n)) = g.get(&d2) else { panic!("d2 is a node") };
        assert_eq!((n.requirement.as_str(), n.id, n.text.as_str(), n.supersedes.clone()), ("ready-only-with-commit", 2, "Instead.", vec![1]));
        assert_eq!((g.get(&d2).unwrap().kind(), g.get(&d2).unwrap().title()), (Kind::Req, "decision 2 of ready-only-with-commit".to_string()));
        assert!(g.out_with(&d2, Relation::BelongsTo).any(|e| e.to == Reference::req("ready-only-with-commit")));
        assert!(g.out_with(&d2, Relation::Supersedes).any(|e| e.to == Reference::decision("ready-only-with-commit", 1)));
        assert!(g.inbound_with(&d2, Relation::ReliesOn).any(|e| e.from == Reference::ticket("later-1")), "a ticket relying on it is an edge to it");
        assert!(!g.contains(&Reference::decision("ready-only-with-commit", 3)), "one not bottled is no node");
        assert!(crate::query::find(&g, "Instead", None, 10).is_empty(), "find matches nothing of a decision");
        // One naming a decision its requirement does not hold dangles: a ticket's, read with it, and a supersession.
        let read = Graph::with_proposal(&s, "later-1", &crate::extraction::fixtures::extraction(), &[]);
        let dangling = |g: &Graph| crate::lint::lint(g, &[crate::lint::Rule::Dangling]).into_iter().filter_map(|f| f.edge.map(|e| format!("{} {}", e.from, e.to))).collect::<Vec<_>>();
        assert!(dangling(&read).contains(&"ticket:later-1 req:ready-only-with-commit#d9".to_string()), "{:?}", dangling(&read));
        assert!(!dangling(&read).iter().any(|f| f.ends_with("#d2")));
        s.requirements.get_mut("ready-only-with-commit").unwrap().decisions.remove(0);
        let g = Graph::build(&s, &crate::extraction::fixtures::extraction(), &[]);
        assert!(dangling(&g).contains(&"req:ready-only-with-commit#d2 req:ready-only-with-commit#d1".to_string()), "{:?}", dangling(&g));
    }
}
