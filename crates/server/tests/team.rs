//! A team as it is set up: a shared server holding the intent, and machines
//! that each join it with their own checkout's code.
// An integration test is not cfg(test), so clippy's tests exemption misses it.
#![allow(clippy::unwrap_used)]

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{Duration, Instant};

use grimoire_core::model::{Page, Priority, Reference, Requirement, Scenario, Scope, Start, Status, StoreConfig, Ticket, TicketKind};
use grimoire_core::ops::Changed;
use grimoire_core::Store;
use grimoire_server::daemon::{Daemon, Event, OpenError};
use grimoire_server::remote::{self, RemoteError, Server};
use grimoire_server::{http, store_io};
use serde_json::{json, Value};

/// A team's server: a store with no repository beside it, behind tokens.
struct Host {
    address: String,
    daemon: Arc<Daemon>,
    store: PathBuf,
    writer: String,
    reader: String,
    /// Dropping it stops the server, as a host going away does.
    runtime: Option<tokio::runtime::Runtime>,
    _tmp: tempfile::TempDir,
}

impl Host {
    fn as_writer(&self) -> Server {
        Server::new(&self.address, Some(self.writer.clone()))
    }
    fn stop(&mut self) {
        self.runtime.take();
    }
}

fn ticket(slug: &str, status: Status) -> Ticket {
    Ticket { slug: slug.into(), title: format!("Ticket {slug}"), kind: TicketKind::Change, priority: Priority::Later, status, touches: vec![], relies_on: vec![], body: "b".into(), decision: None }
}

/// The server, over a store as it arrives on a host: its machine's checkout
/// path, current ticket and start with it. One requirement names a criterion
/// only a checkout's code declares.
fn host() -> Host {
    let tmp = tempfile::Builder::new().prefix("grimoire-team-host-").tempdir().unwrap();
    let store = tmp.path().join("store");
    let config = StoreConfig {
        site: vec![],
        version: StoreConfig::CURRENT_VERSION,
        repository: "C:/a/checkout/on/another/machine".into(),
        roots: vec!["overview".into()].into(),
        languages: vec!["rust".into()],
        scope: Scope::default(),
        order: Default::default(),
    };
    let mut s = Store::new(config);
    s.pages.insert("overview".into(), Page { slug: "overview".into(), title: "Overview".into(), body: "What it is.".into(), links: vec![Reference::req("says-a"), Reference::module("src/a")] });
    s.requirements.insert(
        "says-a".into(),
        Requirement {
            slug: "says-a".into(),
            title: "It says a".into(),
            statement: "It SHALL say a.".into(),
            scenarios: vec![Scenario { title: "t".into(), given: "g".into(), when: "w".into(), then: "t".into() }],
            covered_by: vec![Reference::criterion("src/a", "says")],
            decisions: vec![],
            retired: false,
        },
    );
    for (slug, status) in [("0001", Status::Open), ("0002", Status::Open), ("0003", Status::Active)] {
        s.tickets.insert(slug.into(), ticket(slug, status));
    }
    s.current = Some("0003".into());
    s.starts.insert("0003".into(), Start { ticket: "0003".into(), fingerprints: Default::default() });
    store_io::write_all(&store, &s, false).unwrap();
    let writer = store_io::tokens::add(&store, "ana", true).unwrap();
    let reader = store_io::tokens::add(&store, "pipeline-check", false).unwrap();
    let daemon = Arc::new(Daemon::open_shared(&store).unwrap());
    let runtime = tokio::runtime::Runtime::new().unwrap();
    let listener = runtime.block_on(tokio::net::TcpListener::bind("127.0.0.1:0")).unwrap();
    let address = format!("http://127.0.0.1:{}", listener.local_addr().unwrap().port());
    let app = http::router(daemon.clone(), http::Ui::Shared, Arc::new(grimoire_server::access::Tokens { store_dir: store.clone() }));
    runtime.spawn(async move { axum::serve(listener, app).await });
    Host { address, daemon, store, writer, reader, runtime: Some(runtime), _tmp: tmp }
}

const CONTRACTED: &str = "//! # Module contract\n//!\n//! ```json\n//! {\"purpose\":\"Say a.\",\"criteria\":[{\"id\":\"says\",\"statement\":\"It says a.\"}]}\n//! ```\npub fn a() {}\n";

/// One person's checkout, with the code, and where its copy of the server's intent goes.
struct Machine {
    repo: PathBuf,
    copy: PathBuf,
    _tmp: tempfile::TempDir,
}

fn machine() -> Machine {
    let tmp = tempfile::Builder::new().prefix("grimoire-team-machine-").tempdir().unwrap();
    let repo = tmp.path().join("repo");
    std::fs::create_dir_all(repo.join("src")).unwrap();
    std::fs::write(repo.join("Cargo.toml"), "[package]\nname = \"m\"\nversion = \"0.1.0\"\n").unwrap();
    std::fs::write(repo.join("src").join("lib.rs"), "pub mod a;\n").unwrap();
    std::fs::write(repo.join("src").join("a.rs"), CONTRACTED).unwrap();
    Machine { copy: tmp.path().join("copy"), repo, _tmp: tmp }
}

impl Machine {
    fn open(&self, server: Server) -> Arc<Daemon> {
        Arc::new(Daemon::open_remote(&self.copy, &self.repo, server).unwrap().0)
    }
    fn repository(&self) -> String {
        store_io::repository_string(&self.repo).unwrap()
    }
    fn holds(&self, file: &str) -> bool {
        self.copy.join(file).is_file()
    }
}

fn code(e: grimoire_core::ToolError) -> &'static str {
    e.code()
}

fn page(slug: &str) -> Value {
    json!({ "slug": slug, "title": "T", "body": "b", "links": [] })
}

/// Wait for `done` to hold, for as long as a follower may take.
fn eventually(what: &str, mut done: impl FnMut() -> bool) {
    let deadline = Instant::now() + Duration::from_secs(30);
    while !done() {
        assert!(Instant::now() < deadline, "never: {what}");
        std::thread::sleep(Duration::from_millis(100));
    }
}

/// # Test contract
///
/// ```json
/// {"test": "a-machine-joins-the-servers-intent-with-its-own-code", "checks": ["crates/server/src/remote#sync", "crates/server/src/http#export", "crates/server/src/daemon#shared"]}
/// ```
#[test]
fn a_machine_joins_the_servers_intent_with_its_own_code() {
    let h = host();
    // The store arrived with one checkout's current ticket and start: a server keeps neither, in the store either.
    assert!(!h.store.join("current.json").exists() && !h.store.join("starts").join("0003.json").exists());
    // The whole intent in one read, to a caller the trust step admits, a reader among them, and nothing of a machine in it.
    assert!(matches!(Server::new(&h.address, None).export(), Err(RemoteError::Refused(..))));
    let exported = Server::new(&h.address, Some(h.reader.clone())).export().unwrap();
    assert_eq!((exported.pages.len(), exported.requirements.len(), exported.tickets.len()), (1, 1, 3));
    assert!(!serde_json::to_string(&exported).unwrap().contains("another/machine"));
    // A route, not a tool: an agent is never offered a whole store in one answer.
    assert!(!grimoire_core::tools::definitions().iter().any(|d| d.name == "export") && !grimoire_core::tools::TOOL_NAMES.contains(&"export"));
    // The first sync makes the copy: laid out as a store, recording this checkout, noting its server.
    let m = machine();
    let server = h.as_writer();
    std::fs::create_dir_all(&m.copy).unwrap();
    let first = remote::sync(&server, &m.copy, &m.repository(), None).unwrap();
    assert_eq!(first.changed, vec![Changed::Config]);
    let (copied, malformed) = store_io::load(&m.copy).unwrap();
    assert!(malformed.is_empty(), "{malformed:?}");
    assert_eq!(copied.config.repository, m.repository());
    assert_eq!((copied.pages.len(), copied.requirements.len(), copied.tickets.len(), copied.current.clone()), (1, 1, 3, None));
    let note = remote::copy_note(&m.copy).unwrap();
    assert_eq!((note.server.as_str(), note.fetched.ends_with('Z')), (h.address.as_str(), true));
    // After that only what differs is written: nothing, then the one ticket the server gained.
    assert!(remote::sync(&server, &m.copy, &m.repository(), None).unwrap().changed.is_empty());
    let filed = h.daemon.call("file_ticket", json!({ "title": "From the team", "kind": "gap", "priority": "later", "body": "b" })).unwrap();
    let slug = filed["node"]["ref"].as_str().unwrap().trim_start_matches("ticket:").to_string();
    assert_eq!(remote::sync(&server, &m.copy, &m.repository(), None).unwrap().changed, vec![Changed::Ticket(slug)]);
    // What does not load is in no difference: a write stopped part way, or a hand's edit. The sync makes it the server's again.
    std::fs::write(m.copy.join("current.json"), r#"{"ticket":"0002"}"#).unwrap();
    std::fs::create_dir_all(m.copy.join("proposals")).unwrap();
    std::fs::write(m.copy.join("proposals").join("0002.json"), r#"{"ticket":"0002"}"#).unwrap();
    std::fs::write(m.copy.join("pages").join("overview.json"), "not a record").unwrap();
    std::fs::write(m.copy.join("pages").join("stray.json"), "{}").unwrap();
    assert_eq!(store_io::load(&m.copy).unwrap().1.len(), 4, "four files that do not load");
    remote::sync(&server, &m.copy, &m.repository(), None).unwrap();
    let (mended, malformed) = store_io::load(&m.copy).unwrap();
    assert!(malformed.is_empty(), "{malformed:?}");
    assert!(mended.pages.contains_key("overview") && !m.holds("pages/stray.json") && !m.holds("current.json") && !m.holds("proposals/0002.json"));
    // Opened, the machine answers over the whole graph: the server's intent and its own code.
    let d = m.open(h.as_writer());
    let coverage = d.call("coverage", json!({ "ref": "criterion:src/a#says" })).unwrap();
    assert_eq!(coverage["requirements"][0]["ref"], "req:says-a", "{coverage}");
    assert_eq!(h.daemon.call("coverage", json!({ "ref": "criterion:src/a#says" })).unwrap_err().code(), "store_unavailable", "which the server itself cannot");
    let status = d.call("status", json!({})).unwrap();
    assert_eq!((status["server"]["address"].clone(), status["server"]["reachable"].clone(), status["shared"].clone()), (json!(h.address), json!(true), json!(false)));
    assert!(status["server"]["fetched"].is_string() && status["nodes"]["module"].as_u64().unwrap() > 0, "{status}");
    assert!(!d.sync_now().unwrap(), "nothing changed on the server, so nothing changes here");
}

/// # Test contract
///
/// ```json
/// {"test": "a-write-goes-to-the-server-and-is-answered-from-the-machine", "checks": ["crates/server/src/remote#writes-through", "crates/core/src/ops#adopt"]}
/// ```
#[test]
fn a_write_goes_to_the_server_and_is_answered_from_the_machine() {
    let h = host();
    let m = machine();
    let d = m.open(h.as_writer());
    // A start is the server's write, answered with what the machine's graph shows.
    let started = d.call("start_ticket", json!({ "ref": "ticket:0001" })).unwrap();
    assert_eq!(started["node"]["status"], "active");
    assert!(started["rests_on"].is_array());
    assert_eq!(h.daemon.call("get", json!({ "ref": "ticket:0001" })).unwrap()["node"]["status"], "active");
    assert!(m.holds("starts/0001.json") && !h.store.join("starts").join("0001.json").exists(), "the start is the checkout's, with its code: the server records none");
    // A requirement naming a criterion the code does not declare: taken by the server, which holds no code,
    // and answered with the dangling edge only the machine can see.
    let req = json!({ "slug": "says-b", "title": "It says b", "statement": "It SHALL say b.", "scenarios": [{ "title": "t", "given": "g", "when": "w", "then": "t" }], "covered_by": ["criterion:src/a#gone"] });
    let written = d.call("put_requirement", req).unwrap();
    assert!(written["warnings"].as_array().unwrap().iter().any(|w| w["edge"]["to"] == "criterion:src/a#gone"), "{written}");
    assert!(h.daemon.call("get", json!({ "ref": "req:says-b", "ticket": "0001" })).is_ok(), "in the ticket's proposal on the server");
    assert!(m.holds("proposals/0001.json"), "and in the copy, by the sync");
    // An id the server generates is the one answered.
    let filed = d.call("file_ticket", json!({ "title": "Found on the way", "kind": "gap", "priority": "later", "body": "b" })).unwrap();
    let filed_ref = filed["node"]["ref"].as_str().unwrap().to_string();
    assert_eq!(h.daemon.call("get", json!({ "ref": filed_ref })).unwrap()["node"]["title"], "Found on the way");
    // The copy behind the server: a teammate retitled the ticket and nothing here has fetched that.
    // The write is the server's, so the answer is the server's too, never one worked out from the stale copy.
    h.daemon.call("update_ticket", json!({ "ref": filed_ref, "title": "Retitled by a teammate" })).unwrap();
    assert_eq!(d.call("get", json!({ "ref": filed_ref })).unwrap()["node"]["title"], "Found on the way", "not fetched yet");
    let updated = d.call("update_ticket", json!({ "ref": filed_ref, "priority": "high" })).unwrap();
    assert_eq!((updated["node"]["title"].clone(), updated["node"]["priority"].clone()), (json!("Retitled by a teammate"), json!("high")), "{updated}");
    assert_eq!(d.call("get", json!({ "ref": filed_ref })).unwrap()["node"]["title"], "Retitled by a teammate", "and the copy has followed");
    // Refused by the server: its error is the answer, and nothing changes here.
    let mut refused = page("aim");
    refused["ticket"] = json!("0002");
    assert_eq!(code(d.call("put_page", refused).unwrap_err()), "conflict", "0002 is not being worked");
    assert!(!m.holds("proposals/0002.json"));
    assert!(d.call("get", json!({ "ref": "page:aim", "ticket": "0001" })).is_err());
    // A token that may only read is refused its write by the server.
    let reading = machine();
    let r = reading.open(Server::new(&h.address, Some(h.reader.clone())));
    assert!(r.call("board", json!({})).is_ok());
    let e = r.call("file_ticket", json!({ "title": "T", "kind": "gap", "priority": "later", "body": "b" })).unwrap_err();
    assert!(e.to_string().contains("may only read"), "{e}");
    // adopt carries what the machine sees, which the server alone could not list.
    let seen = d.call("lint", json!({ "ticket": null })).unwrap();
    let truth = h.daemon.call("lint", json!({})).unwrap()["findings"].as_array().unwrap().len();
    let adopted = d.call("adopt", json!({ "reason": "inherited" })).unwrap();
    let on_server = store_io::load(&h.store).unwrap().0.baseline.unwrap();
    assert_eq!(adopted["baseline"]["count"].as_u64().unwrap() as usize, on_server.findings.len());
    assert!(
        on_server.findings.len() > truth && on_server.findings.iter().any(|f| f.r#ref.to_string().starts_with("module:") || f.r#ref.to_string().starts_with("criterion:")),
        "what only code shows is in it: {seen}"
    );
    // The decision written, the ticket waits; landed by name, the close is answered with what the machine's start shows.
    assert_eq!(d.call("close_ticket", json!({ "ref": "ticket:0001", "decision": { "text": "Done." }, "when": "landed" })).unwrap()["node"]["status"], "landing");
    let landed = d.call("land_ticket", json!({ "ref": "ticket:0001" })).unwrap();
    assert_eq!((landed["landed"].clone(), landed["node"]["status"].clone()), (json!(true), json!("closed")));
    assert!(landed["untouched"].is_array() && landed["unnamed"].is_array(), "the machine holds the start and the code: {landed}");
    assert_eq!(h.daemon.call("get", json!({ "ref": "ticket:0001" })).unwrap()["node"]["status"], "closed");
    assert!(h.daemon.call("get", json!({ "ref": "req:says-b" })).is_ok(), "applied to the server's truth");
    assert!(!m.holds("current.json") && !m.holds("starts/0001.json") && !m.holds("proposals/0001.json"));
}

/// # Test contract
///
/// ```json
/// {"test": "a-close-is-judged-with-the-languages-the-server-now-reads", "checks": ["crates/server/src/remote#writes-through", "req:remote-writes-through-the-server"]}
/// ```
#[test]
fn a_close_is_judged_with_the_languages_the_server_now_reads() {
    let h = host();
    let m = machine();
    std::fs::write(
        m.repo.join("src").join("b.ts"),
        "export function b() {}
",
    )
    .unwrap();
    let d = m.open(h.as_writer());
    // Started while the grimoire reads Rust alone: the TypeScript module is not in the graph.
    d.call("start_ticket", json!({ "ref": "ticket:0001" })).unwrap();
    d.call("update_ticket", json!({ "ref": "ticket:0001", "touches": ["module:src/b"] })).unwrap();
    assert!(d.call("get", json!({ "ref": "module:src/b" })).is_err());
    // A teammate has the server read TypeScript too, and this copy has not heard of it.
    h.daemon.call("configure", json!({ "languages": ["rust", "typescript"] })).unwrap();
    // The close is judged with the languages the server now reads: the module it touched is there, and changed.
    let closed = d.call("close_ticket", json!({ "ref": "ticket:0001", "decision": { "text": "Done." } })).unwrap();
    let edge = closed["node"]["out"].as_array().unwrap().iter().find(|e| e["ref"] == "module:src/b").cloned().unwrap();
    assert!(!edge["title"].is_null(), "judged with TypeScript read, the module it touched is in the graph: {closed}");
    assert!(d.call("get", json!({ "ref": "module:src/b" })).is_ok());
}

/// # Test contract
///
/// ```json
/// {"test": "the-current-ticket-is-each-checkouts-own", "checks": ["crates/server/src/remote#current-is-the-checkouts"]}
/// ```
#[test]
fn the_current_ticket_is_each_checkouts_own() {
    let h = host();
    let (ana, ben) = (machine(), machine());
    let (a, b) = (ana.open(h.as_writer()), ben.open(h.as_writer()));
    a.call("start_ticket", json!({ "ref": "ticket:0001" })).unwrap();
    b.call("start_ticket", json!({ "ref": "ticket:0002" })).unwrap();
    // Each write that names no ticket lands in its own checkout's.
    a.call("put_page", page("from-ana")).unwrap();
    b.call("put_page", page("from-ben")).unwrap();
    assert!(h.daemon.call("get", json!({ "ref": "page:from-ana", "ticket": "0001" })).is_ok());
    assert!(h.daemon.call("get", json!({ "ref": "page:from-ben", "ticket": "0002" })).is_ok());
    assert!(h.daemon.call("get", json!({ "ref": "page:from-ben", "ticket": "0001" })).is_err());
    // An edit to another ticket's references is sent naming the checkout's current change, and waits in its proposal there.
    a.call("update_ticket", json!({ "ref": "ticket:0003", "touches": ["page:overview"] })).unwrap();
    let edited = h.daemon.call("get", json!({ "ref": "ticket:0003" })).unwrap();
    assert_eq!((edited["node"]["touches"].clone(), edited["drafts"].clone()), (json!([]), json!(["ticket:0001"])));
    assert_eq!(h.daemon.call("get", json!({ "ref": "ticket:0003", "ticket": "0001" })).unwrap()["node"]["touches"], json!(["page:overview"]));
    // A title, or the current ticket's own references, is sent as given and changes at once.
    a.call("update_ticket", json!({ "ref": "ticket:0003", "title": "Renamed" })).unwrap();
    a.call("update_ticket", json!({ "ref": "ticket:0001", "touches": ["page:overview"] })).unwrap();
    let truth = |r: &str| h.daemon.call("get", json!({ "ref": r })).unwrap()["node"].clone();
    assert_eq!((truth("ticket:0003")["title"].clone(), truth("ticket:0001")["touches"].clone()), (json!("Renamed"), json!(["page:overview"])));
    // The server keeps none: there a write has to name its ticket.
    assert!(!h.store.join("current.json").exists());
    assert_eq!(code(h.daemon.call("put_page", page("unnamed")).unwrap_err()), "invalid");
    // Kept in each copy, with the checkout's own start.
    assert!(std::fs::read_to_string(ana.copy.join("current.json")).unwrap().contains("0001") && ana.holds("starts/0001.json"));
    assert!(std::fs::read_to_string(ben.copy.join("current.json")).unwrap().contains("0002") && !ben.holds("starts/0001.json"));
    // A ticket already being worked is taken up in a second checkout: the server would refuse to start it again.
    assert_eq!(code(h.daemon.call("start_ticket", json!({ "ref": "ticket:0001" })).unwrap_err()), "conflict");
    let taken = b.call("start_ticket", json!({ "ref": "ticket:0001" })).unwrap();
    assert_eq!(taken["node"]["status"], "active");
    assert!(ben.holds("starts/0001.json") && std::fs::read_to_string(ben.copy.join("current.json")).unwrap().contains("0001"));
    b.call("put_page", page("from-ben-too")).unwrap();
    assert!(h.daemon.call("get", json!({ "ref": "page:from-ben-too", "ticket": "0001" })).is_ok());
    // A sync leaves what is the checkout's own.
    a.sync_now().unwrap();
    a.call("put_page", page("still-ana")).unwrap();
    assert!(h.daemon.call("get", json!({ "ref": "page:still-ana", "ticket": "0001" })).is_ok());
    // An unnamed landing lands nothing on a team, and is not sent: the ticket is still landing on the server.
    a.call("close_ticket", json!({ "ref": "ticket:0001", "decision": { "text": "Done." }, "when": "landed" })).unwrap();
    let none = a.call("land_ticket", json!({})).unwrap();
    assert_eq!((none["landed"].clone(), none["waiting"][0]["ref"].clone()), (json!(false), json!("ticket:0001")));
    assert!(none["note"].as_str().unwrap().contains("main line"));
    assert_eq!(h.daemon.call("get", json!({ "ref": "ticket:0001" })).unwrap()["node"]["status"], "landing");
    // Landed on the main line, by name, with a pipeline's token: every checkout's current ticket and start for it go.
    assert_eq!(h.daemon.call("land_ticket", json!({ "ref": "ticket:0001" })).unwrap()["landed"], true);
    assert!(a.sync_now().unwrap() && b.sync_now().unwrap());
    for m in [&ana, &ben] {
        assert!(!m.holds("current.json") && !m.holds("starts/0001.json"));
    }
    assert_eq!(code(a.call("put_page", page("after")).unwrap_err()), "invalid", "no ticket is current here any more");
}

/// # Test contract
///
/// ```json
/// {"test": "a-teammates-write-arrives-without-a-reload", "checks": ["crates/server/src/remote#follows-the-server"]}
/// ```
#[test]
fn a_teammates_write_arrives_without_a_reload() {
    let mut h = host();
    let (ana, ben) = (machine(), machine());
    let (a, b) = (ana.open(h.as_writer()), ben.open(h.as_writer()));
    remote::follow(&b);
    let mut heard = b.subscribe();
    a.call("start_ticket", json!({ "ref": "ticket:0001" })).unwrap();
    a.call("put_page", page("from-ana")).unwrap();
    // Nobody asks ben's daemon to sync: it hears the server and fetches.
    eventually("ana's page shows on ben's machine", || b.call("get", json!({ "ref": "page:from-ana", "ticket": "0001" })).is_ok());
    let mut announced = false;
    while let Ok(event) = heard.try_recv() {
        announced |= matches!(event, Event::Store { tool, changed } if tool == "sync" && changed.contains(&"ticket:0001".to_string()));
    }
    assert!(announced, "and says so on its own stream, for the interface");
    // The server goes away: ben's daemon says so, and goes on answering from its copy.
    h.stop();
    eventually("the server is seen to be gone", || b.call("status", json!({})).unwrap()["server"]["reachable"] == false);
    assert!(b.call("get", json!({ "ref": "page:from-ana", "ticket": "0001" })).is_ok());
    let mut said = false;
    while let Ok(event) = heard.try_recv() {
        said |= matches!(event, Event::Store { tool, changed } if tool == "sync" && changed == vec!["server".to_string()]);
    }
    assert!(said, "and tells its listeners, so the interface asks status again");
}

/// # Test contract
///
/// ```json
/// {"test": "offline-a-machine-reads-its-copy-and-writes-nothing", "checks": ["crates/server/src/remote#offline"]}
/// ```
#[test]
fn offline_a_machine_reads_its_copy_and_writes_nothing() {
    let mut h = host();
    let m = machine();
    let d = m.open(h.as_writer());
    d.call("start_ticket", json!({ "ref": "ticket:0001" })).unwrap();
    d.call("put_page", page("aim")).unwrap();
    // A token the server refuses does not open, copy or no copy.
    assert!(matches!(Daemon::open_remote(&m.copy, &m.repo, Server::new(&h.address, Some("grm_not-held".into()))), Err(OpenError::Server(RemoteError::Refused(..)))));
    let fetched = d.call("status", json!({})).unwrap()["server"]["fetched"].clone();
    h.stop();
    // Reads go on, from the copy; a write is refused, naming the server, and changes nothing.
    assert_eq!(d.call("get", json!({ "ref": "page:aim", "ticket": "0001" })).unwrap()["node"]["title"], "T");
    let e = d.call("put_page", page("more")).unwrap_err();
    assert_eq!(e.code(), "store_unavailable");
    assert!(e.to_string().contains(&h.address), "{e}");
    assert!(d.call("get", json!({ "ref": "page:more", "ticket": "0001" })).is_err());
    assert_eq!(code(d.call("start_ticket", json!({ "ref": "ticket:0002" })).unwrap_err()), "store_unavailable");
    let status = d.call("status", json!({})).unwrap();
    assert_eq!((status["server"]["reachable"].clone(), status["server"]["fetched"].clone()), (json!(false), fetched.clone()), "unreachable, and when the copy was fetched");
    // What is the checkout's own needs no server: an unnamed landing still answers.
    assert_eq!(d.call("land_ticket", json!({})).unwrap()["landed"], false);
    // Opened anew with the server still away, the machine reads the copy it holds: the check still runs.
    drop(d);
    let again = m.open(h.as_writer());
    let status = again.call("status", json!({})).unwrap();
    assert_eq!((status["server"]["reachable"].clone(), status["server"]["fetched"].clone()), (json!(false), fetched));
    assert!(again.call("lint", json!({})).unwrap()["findings"].is_array());
    assert!(again.call("get", json!({ "ref": "page:aim", "ticket": "0001" })).is_ok(), "its current ticket's proposal with it");
    // A machine that holds no copy has nothing to read.
    let fresh = machine();
    assert!(matches!(Daemon::open_remote(&fresh.copy, &fresh.repo, h.as_writer()), Err(OpenError::Server(RemoteError::Unreachable(..)))));
    assert!(!Path::new(&fresh.copy).join("grimoire.json").exists());
}

/// A stand-in for a server that gives these answers, one to each request in turn, and is then gone.
fn answers_then_goes(answers: Vec<Value>) -> String {
    use std::io::{Read, Write};
    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let address = format!("http://{}", listener.local_addr().unwrap());
    std::thread::spawn(move || {
        for answer in answers {
            let Ok((mut s, _)) = listener.accept() else { return };
            // The whole request, however it is framed: read until the caller has nothing more to send.
            s.set_read_timeout(Some(Duration::from_millis(500))).unwrap();
            let mut buf = [0u8; 8192];
            while matches!(s.read(&mut buf), Ok(n) if n > 0) {}
            let body = answer.to_string();
            let _ = write!(s, "HTTP/1.1 200 OK\r\ncontent-type: application/json\r\nconnection: close\r\ncontent-length: {}\r\n\r\n{body}", body.len());
        }
    });
    address
}

/// A stand-in for a server that takes one write and is gone before it can be asked for its intent.
fn takes_one_write_and_goes(answer: Value) -> String {
    answers_then_goes(vec![answer])
}

/// # Test contract
///
/// ```json
/// {"test": "a-write-the-server-took-is-answered-when-the-copy-lags", "checks": ["crates/server/src/remote#writes-through"]}
/// ```
#[test]
fn a_write_the_server_took_is_answered_when_the_copy_lags() {
    let h = host();
    let m = machine();
    drop(m.open(h.as_writer()));
    let before = store_io::load(&m.copy).unwrap().0;
    // A teammate has retitled a ticket this machine has not fetched; this machine changes its priority.
    // The server takes the write and does not answer again.
    let gone = Server::new(
        &takes_one_write_and_goes(json!({ "node": { "ref": "ticket:0001", "title": "Retitled by a teammate", "priority": "high" }, "warnings": [], "baselined": 0 })),
        Some(h.writer.clone()),
    );
    let mut loaded = grimoire_server::load(&m.copy).unwrap();
    let (answer, changed) = remote::write_through(&gone, &m.copy, &mut loaded.state, "update_ticket", json!({ "ref": "ticket:0001", "priority": "high" }), "2026-10-03").unwrap();
    // The write was made, so it is answered, with the server's answer: an error would invite it a second time.
    assert_eq!((answer["node"]["title"].clone(), answer["node"]["priority"].clone()), (json!("Retitled by a teammate"), json!("high")));
    // The machine's own run of it, made over the stale ticket, is kept nowhere: memory holds what the copy does,
    // so a read now and a read after a restart say the same, and the next sync brings the write.
    assert!(changed.is_empty());
    assert_eq!(*loaded.state.store(), before);
    assert_eq!(store_io::load(&m.copy).unwrap().0, before);
    let server = loaded.state.server().unwrap();
    assert!(!server.reachable && !server.refused, "and the server is known to be away");
    // But a start: the server says whether the ticket is being worked, takes the start, and is gone. What the
    // start leaves here is held nowhere else, so the machine's run of it is kept, in memory and in the copy.
    let intent = serde_json::to_value(h.as_writer().export().unwrap()).unwrap();
    let gone = Server::new(&answers_then_goes(vec![intent, json!({ "node": { "ref": "ticket:0002", "status": "active" }, "rests_on": [], "related": [] })]), Some(h.writer.clone()));
    let (answer, changed) = remote::write_through(&gone, &m.copy, &mut loaded.state, "start_ticket", json!({ "ref": "ticket:0002" }), "2026-10-03").unwrap();
    assert_eq!(answer["node"]["ref"], "ticket:0002");
    assert_eq!(changed, vec![Changed::Many(vec![Changed::Started("0002".into()), Changed::Current])]);
    assert_eq!(loaded.state.current().as_deref(), Some("0002"));
    let (kept, malformed) = store_io::load(&m.copy).unwrap();
    assert!(malformed.is_empty(), "{malformed:?}");
    assert_eq!((kept.current.as_deref(), kept.starts.contains_key("0002"), kept.tickets["0002"].status), (Some("0002"), true, Status::Active));
    assert_eq!(kept, *loaded.state.store(), "memory holds what the copy does");
}

/// # Test contract
///
/// ```json
/// {"test": "a-refused-token-is-not-a-server-that-is-away", "checks": ["crates/server/src/remote#offline", "crates/server/src/daemon#refused-reads-nothing", "crates/server/src/remote#token-for-the-server", "crates/server/src/daemon#as-the-caller"]}
/// ```
#[test]
fn a_refused_token_is_not_a_server_that_is_away() {
    let mut h = host();
    let m = machine();
    // Signed in as a person is: the token kept in their credentials file.
    let credentials = m.copy.parent().unwrap().join("credentials.json");
    remote::keep_token(&credentials, &h.address, &h.writer).unwrap();
    let d = m.open(Server::kept(&h.address, &credentials));
    assert!(d.call("board", json!({})).is_ok());
    // Revoked while the daemon serves: the next time the server is asked, here by a write, it refuses,
    // and that is told apart from a token that may only read, which is refused its write and no more.
    assert!(store_io::tokens::revoke(&h.store, "ana").unwrap());
    let e = d.call("file_ticket", json!({ "title": "T", "kind": "gap", "priority": "later", "body": "b" })).unwrap_err();
    assert_eq!(e.code(), "store_unavailable");
    assert!(e.to_string().contains("grimoire login") && !e.to_string().contains("waits until"), "{e}");
    let reading = machine();
    let r = reading.open(Server::new(&h.address, Some(h.reader.clone())));
    assert_eq!(code(r.call("file_ticket", json!({ "title": "T", "kind": "gap", "priority": "later", "body": "b" })).unwrap_err()), "invalid");
    assert_eq!(r.call("status", json!({})).unwrap()["server"]["refused"], false);
    assert!(r.call("board", json!({})).is_ok(), "a reader goes on reading");
    // A caller with a token of its own, as a command run with GRIMOIRE_TOKEN is: its write goes signed in as it, not as the daemon.
    let pipeline = store_io::tokens::add(&h.store, "pipeline", true).unwrap();
    let filed = json!({ "title": "From a pipeline", "kind": "gap", "priority": "later", "body": "b" });
    assert!(r.call_as("file_ticket", filed.clone(), Some(pipeline.clone())).is_ok(), "the reader's daemon, the pipeline's token");
    assert_eq!(code(r.call_as("file_ticket", filed.clone(), None).unwrap_err()), "invalid", "with none, the daemon's own");
    assert_eq!(code(d.call_as("file_ticket", filed, Some(h.reader.clone())).unwrap_err()), "invalid", "and a token that may only read is not lent the daemon's");
    let status = d.call("status", json!({})).unwrap();
    assert_eq!((status["server"]["refused"].clone(), status["server"]["reachable"].clone()), (json!(true), json!(false)), "said as refused, in status, which still answers");
    // Its copy is not read on in silence, and nothing is written.
    let e = d.call("board", json!({})).unwrap_err();
    assert_eq!(e.code(), "store_unavailable");
    assert!(e.to_string().contains("grimoire login"), "{e}");
    assert_eq!(code(d.call("file_ticket", json!({ "title": "T", "kind": "gap", "priority": "later", "body": "b" })).unwrap_err()), "store_unavailable");
    // The person signs in again: the daemon finds the new token by itself, and takes up where it was.
    let again = store_io::tokens::add(&h.store, "ana-again", true).unwrap();
    remote::keep_token(&credentials, &h.address, &again).unwrap();
    assert!(d.call("board", json!({})).is_ok(), "asked again on the next call, with nobody telling it to");
    let status = d.call("status", json!({})).unwrap();
    assert_eq!((status["server"]["reachable"].clone(), status["server"]["refused"].clone()), (json!(true), json!(false)));
    assert!(d.call("file_ticket", json!({ "title": "T", "kind": "gap", "priority": "later", "body": "b" })).is_ok());
    // Refused again, and then the server goes away before it can be asked again: still refused, not merely offline,
    // so the copy is not read on. Only a server that takes the token ends it.
    assert!(store_io::tokens::revoke(&h.store, "ana-again").unwrap());
    assert_eq!(code(d.call("file_ticket", json!({ "title": "T", "kind": "gap", "priority": "later", "body": "b" })).unwrap_err()), "store_unavailable");
    h.stop();
    assert_eq!(code(d.call("board", json!({})).unwrap_err()), "store_unavailable");
    let status = d.call("status", json!({})).unwrap();
    assert_eq!((status["server"]["reachable"].clone(), status["server"]["refused"].clone()), (json!(false), json!(true)));
}
