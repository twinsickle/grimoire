//! # Module contract
//!
//! ```json
//! {"purpose":"Join the store and the extraction into one graph of nodes and edges and derive every reverse edge.","criteria":[{"id":"contract-scope","statement":"A module is expected to carry a contract only when its adapter expects one and the store config's scope covers it."},{"id":"downward-only-input","statement":"Edges are read only from the node that owns them; every upward relation is derived here and nowhere else."},{"id":"dangling-kept","statement":"An edge whose target does not exist is kept in the graph so the lints can report it."}]}
//! ```

use std::collections::{BTreeMap, HashMap, HashSet};

use serde::Serialize;

use crate::extraction::Extraction;
use crate::model::*;
use crate::store::{Malformed, Store};

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
}

impl Node {
    pub fn kind(&self) -> Kind {
        match self {
            Node::Page(_) => Kind::Page,
            Node::Req(_) => Kind::Req,
            Node::Ticket(_) => Kind::Ticket,
            Node::Module(_) => Kind::Module,
            Node::Criterion(_) => Kind::Criterion,
            Node::File(_) => Kind::File,
            Node::Decl(_) => Kind::Decl,
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
        }
    }
}

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
    pub malformed: Vec<Malformed>,
    pub unresolved_imports: usize,
    /// The store's baseline, as identities to match findings against.
    pub baseline: HashSet<FindingKey>,
    /// Configured languages the running build cannot extract, and the ones
    /// it can; set by whoever knows the build (`State`), empty otherwise.
    pub unsupported_languages: Vec<String>,
    pub supported_languages: Vec<String>,
}

impl Graph {
    pub fn build(store: &Store, extraction: &Extraction, malformed: &[Malformed]) -> Graph {
        let mut g = Graph {
            roots: store.config.roots.clone(),
            malformed: malformed.to_vec(),
            unresolved_imports: extraction.unresolved_imports,
            baseline: store.baseline.iter().flat_map(|b| b.findings.iter().cloned()).collect(),
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
            for c in &r.covered_by {
                g.edge(&from, c, Relation::CoveredBy);
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
        for f in extraction.files.values() {
            let from = Reference::file(&f.path);
            for d in &f.decls {
                let dref = Reference::decl(&f.path, &d.path);
                g.edge(&from, &dref, Relation::Contains);
                g.nodes.insert(dref, Node::Decl(d.clone()));
            }
            g.nodes.insert(from, Node::File(f.clone()));
        }
        g
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
            (Kind::Decl, Node::Decl(d)) => {
                let (file, _) = r.split_hash()?;
                Some(SourceLocation { file: file.to_string(), line: d.line, contract_line: None })
            }
            _ => None,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct SourceLocation {
    pub file: String,
    pub line: u32,
    #[serde(skip_serializing_if = "Option::is_none")]
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
        let scenarios = if slug == "stale" {
            vec![]
        } else {
            vec![Scenario { title: "It holds".into(), given: "a repository".into(), when: format!("{slug} is checked"), then: "it holds".into() }]
        };
        Requirement {
            slug: slug.into(),
            title: slug.to_uppercase(),
            statement: format!("The system SHALL {slug}."),
            scenarios,
            covered_by: covered_by.iter().map(|l| l.parse().unwrap()).collect(),
            decisions: decisions.iter().map(|(d, t)| Decision { date: d.to_string(), text: t.to_string() }).collect(),
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
            body: if status == Status::Closed { String::new() } else { "Do it.".into() },
            decision: if status == Status::Closed { Some(Decision { date: "2026-01-01".into(), text: "Done.".into() }) } else { None },
        }
    }

    /// overview -> git-readiness -> {ready-only-with-commit -> guarantee, module ready}.
    /// `orphan` page is linked by nothing. `stale` req names a criterion that
    /// does not exist and has no scenario. Criterion `reads-only` and `stable` are uncovered.
    pub fn store() -> Store {
        let mut s = Store::new(StoreConfig { version: 1, repository: "C:/repo".into(), roots: vec!["overview".into()], languages: vec!["rust".into()], scope: Scope::default() });
        for p in [
            page("overview", &["page:git-readiness"]),
            page("git-readiness", &["req:ready-only-with-commit", "req:stale", "module:src/a/ready"]),
            page("orphan", &[]),
        ] {
            s.pages.insert(p.slug.clone(), p);
        }
        for r in [
            req("ready-only-with-commit", &["criterion:src/a/ready#guarantee"], &[("2026-09-12", "Identity is required rather than invented.")]),
            req("stale", &["criterion:src/a/ready#gone"], &[]),
        ] {
            s.requirements.insert(r.slug.clone(), r);
        }
        for t in [
            ticket("phase-62", TicketKind::Change, Priority::Next, Status::Open, &["req:ready-only-with-commit", "module:src/a/ready"], &["page:git-readiness"]),
            ticket("gap-1", TicketKind::Gap, Priority::Later, Status::Open, &["module:src/a/ready"], &[]),
            ticket("later-1", TicketKind::Change, Priority::Later, Status::Active, &["page:orphan"], &[]),
            ticket("old", TicketKind::Change, Priority::High, Status::Closed, &["page:renamed-away"], &[]),
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

    #[test]
    fn keeps_dangling_edges() {
        let g = graph();
        let stale = Reference::req("stale");
        let e = g.out(&stale).next().unwrap();
        assert!(!g.contains(&e.to));
    }

    #[test]
    fn source_of_criterion_is_the_contract_line() {
        let g = graph();
        let loc = g.source_of(&Reference::criterion("src/a/ready", "guarantee")).unwrap();
        assert_eq!(loc.file, "src/a/ready.rs");
        assert_eq!(loc.contract_line, Some(1));
        let d = g.source_of(&Reference::decl("src/a/ready.rs", "run")).unwrap();
        assert_eq!(d.line, 10);
    }
}
