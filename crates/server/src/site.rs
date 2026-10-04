//! # Module contract
//!
//! ```json
//! {"purpose":"Serve the project's site: how it is meant to be used, rendered from the truth at each request as plain HTML on an address of its own, with nothing else served there.","criteria":[{"id":"renders","statement":"Each page of the site renders as one self-contained HTML document with no script: the site's name, linking /, above a list of every page of the site in order, indented by depth, linking each at /<slug>, then the page's title, its body as Markdown with raw HTML shown as escaped text, a link kept only to a page of the site and any other link or image shown as its text, and each requirement it links by title, statement and scenarios; its browser title is the page's title then the site's name; every text from the store is escaped, and an empty site renders a page saying the project shows nothing yet."},{"id":"only-the-site","statement":"The site's router answers GET (and so HEAD) / with the first page of the site and /<slug> with that page when it is in the site, and every other path and method with 404, so nothing but the site is served on its address, no API, event stream, MCP or builder's UI, and no caller is asked for a token."}]}
//! ```

use std::sync::Arc;

use axum::extract::{Path, State};
use axum::http::{header, StatusCode};
use axum::response::{IntoResponse, Response};
use axum::routing::get;
use axum::Router;
use grimoire_core::query::{Site, SiteEntry, SitePage};
use pulldown_cmark::{html, CowStr, Event, Options, Parser, Tag, TagEnd};

use crate::daemon::Daemon;

/// The site's router: its pages and nothing else.
pub fn router(daemon: Arc<Daemon>) -> Router {
    Router::new().route("/", get(first)).route("/{slug}", get(page)).fallback(elsewhere).method_not_allowed_fallback(elsewhere).with_state(daemon)
}

async fn first(State(daemon): State<Arc<Daemon>>) -> Response {
    let site = daemon.site(None);
    match &site.shown {
        Some(p) => html_page(render(&site, p)),
        None => html_page(empty(site.name.as_deref())),
    }
}

async fn page(State(daemon): State<Arc<Daemon>>, Path(slug): Path<String>) -> Response {
    let site = daemon.site(Some(&slug));
    match &site.shown {
        Some(p) => html_page(render(&site, p)),
        None => not_found(site.name.as_deref()),
    }
}

/// Any other path or method: not found, under the site's name.
async fn elsewhere(State(daemon): State<Arc<Daemon>>) -> Response {
    // An empty slug names no page, so only the list and the name are gathered.
    not_found(daemon.site(Some("")).name.as_deref())
}

fn not_found(name: Option<&str>) -> Response {
    (StatusCode::NOT_FOUND, [(header::CONTENT_TYPE, "text/html; charset=utf-8")], document("Not found", name, "<main><h1>Not found</h1><p>This page is not part of the site. <a href=\"/\">Start at the beginning.</a></p></main>")).into_response()
}

fn html_page(body: String) -> Response {
    // No script runs here, and the page loads nothing from anywhere else.
    ([(header::CONTENT_TYPE, "text/html; charset=utf-8"), (header::CONTENT_SECURITY_POLICY, "default-src 'none'; style-src 'unsafe-inline'"), (header::CACHE_CONTROL, "no-cache")], body).into_response()
}

/// Text from the store, safe to put in HTML.
fn escape(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        match c {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            '\'' => out.push_str("&#39;"),
            c => out.push(c),
        }
    }
    out
}

/// A body as Markdown, any raw HTML in it shown as text, so a page can run nothing. A link stays a
/// link only to a page of the site, `/<slug>` or `<slug>`; any other link, and an image, is its text,
/// so the site leads nowhere else and fetches nothing.
fn markdown(body: &str, pages: &[SiteEntry]) -> String {
    let mut kept = false;
    let events = Parser::new_ext(body, Options::ENABLE_TABLES | Options::ENABLE_STRIKETHROUGH).filter_map(|e| match e {
        Event::Html(t) | Event::InlineHtml(t) => Some(Event::Text(t)),
        Event::Start(Tag::Link { link_type, dest_url, title, id }) => {
            let slug = dest_url.strip_prefix('/').unwrap_or(&dest_url);
            kept = pages.iter().any(|p| p.slug == slug);
            kept.then(|| Event::Start(Tag::Link { link_type, dest_url: CowStr::from(format!("/{slug}")), title, id }))
        }
        Event::End(TagEnd::Link) => kept.then_some(Event::End(TagEnd::Link)),
        Event::Start(Tag::Image { .. }) | Event::End(TagEnd::Image) => None,
        e => Some(e),
    });
    let mut out = String::new();
    html::push_html(&mut out, events);
    out
}

fn render(site: &Site, current: &SitePage) -> String {
    let mut nav = String::from("<nav aria-label=\"Pages\">");
    if let Some(name) = &site.name {
        nav.push_str(&format!("<p class=\"name\"><a href=\"/\">{}</a></p>", escape(name)));
    }
    nav.push_str("<ul>");
    for p in &site.pages {
        let here = if p.slug == current.slug { " aria-current=\"page\"" } else { "" };
        nav.push_str(&format!("<li style=\"margin-left:{}em\"><a href=\"/{}\"{here}>{}</a></li>", p.depth, escape(&p.slug), escape(&p.title)));
    }
    nav.push_str("</ul></nav>");
    let mut main = format!("<main><h1>{}</h1><div class=\"body\">{}</div>", escape(&current.title), markdown(&current.body, &site.pages));
    for r in &current.requirements {
        main.push_str(&format!("<section class=\"req\" id=\"{}\"><h2>{}</h2><p>{}</p>", escape(&r.slug), escape(&r.title), escape(&r.statement)));
        for s in &r.scenarios {
            main.push_str(&format!(
                "<div class=\"scenario\"><strong>{}</strong><div><b>Given</b> {}</div><div><b>When</b> {}</div><div><b>Then</b> {}</div></div>",
                escape(&s.title),
                escape(&s.given),
                escape(&s.when),
                escape(&s.then)
            ));
        }
        main.push_str("</section>");
    }
    main.push_str("</main>");
    document(&current.title, site.name.as_deref(), &format!("<div class=\"layout\">{nav}{main}</div>"))
}

fn empty(name: Option<&str>) -> String {
    document("Nothing yet", name, "<main><h1>Nothing yet</h1><p>This project shows nothing here yet.</p></main>")
}

/// A whole document in the builder's UI's colours, styled inline: nothing is fetched. Its browser
/// title is the page's, then the site's name.
fn document(title: &str, name: Option<&str>, content: &str) -> String {
    let title = match name {
        Some(name) => format!("{title} · {name}"),
        None => title.to_string(),
    };
    format!(
        "<!doctype html><html lang=\"en\"><head><meta charset=\"utf-8\"><meta name=\"viewport\" content=\"width=device-width, initial-scale=1\"><title>{}</title><style>{STYLE}</style></head><body>{content}</body></html>",
        escape(&title)
    )
}

const STYLE: &str = "\
:root{color-scheme:dark}\
body{margin:0;background:#0f1115;color:#d7dbe3;font:15px/1.6 system-ui,-apple-system,\"Segoe UI\",Roboto,sans-serif}\
a{color:#7aa2f7}\
.layout{display:flex;flex-wrap:wrap;gap:32px;max-width:1100px;margin:0 auto;padding:32px 16px}\
nav{flex:0 1 220px}nav .name{margin:0 0 12px;font-size:18px;font-weight:600}nav .name a{color:#d7dbe3}nav ul{list-style:none;margin:0;padding:0}nav li{padding:2px 0}nav a{text-decoration:none}nav a[aria-current]{color:#d7dbe3;font-weight:600}\
main{flex:999 1 560px;min-width:0;max-width:760px}\
h1{font-size:26px;margin:0 0 16px}h2{font-size:17px;margin:0 0 6px;color:#c3a6ff}\
.body code,.req code{background:#1e232c;padding:1px 5px;border-radius:3px;font-family:ui-monospace,\"Cascadia Code\",Consolas,monospace}\
.body pre{background:#161a21;border:1px solid #2e3542;border-radius:6px;padding:10px 12px;overflow:auto}\
.req{border-top:1px solid #2e3542;margin-top:24px;padding-top:16px}\
.scenario{background:#161a21;border:1px solid #2e3542;border-radius:6px;padding:8px 12px;margin:8px 0}\
.scenario b{color:#8b93a3;font-size:12px;text-transform:uppercase;letter-spacing:.04em;margin-right:6px}\
";

#[cfg(test)]
mod tests {
    use super::*;
    use grimoire_core::model::Scenario;
    use grimoire_core::query::SiteRequirement;

    fn site(shown: usize) -> Site {
        let req = SiteRequirement { slug: "board".into(), title: "board".into(), statement: "It SHALL <list>.".into(), scenarios: vec![Scenario { title: "T".into(), given: "g".into(), when: "w".into(), then: "t".into() }] };
        let body = "**What it is.** <script>alert(1)</script> See [tickets](/tickets), [the board](board), [elsewhere](https://example.com), <https://example.com/auto>, [run](javascript:alert(1)) and ![a picture](https://example.com/p.png).";
        let pages = [SitePage { slug: "goal".into(), title: "Goal".into(), body: body.into(), requirements: vec![] }, SitePage { slug: "tickets".into(), title: "Tickets".into(), body: "A ticket.".into(), requirements: vec![req] }];
        Site {
            name: Some("Grim & co".into()),
            pages: vec![SiteEntry { slug: "goal".into(), title: "Goal".into(), depth: 0 }, SiteEntry { slug: "tickets".into(), title: "Tickets".into(), depth: 1 }],
            shown: Some(pages[shown].clone()),
        }
    }

    /// # Test contract
    ///
    /// ```json
    /// {"test": "a-site-page-renders-what-it-says-and-runs-nothing", "checks": ["crates/server/src/site#renders"]}
    /// ```
    #[test]
    fn a_site_page_renders_what_it_says_and_runs_nothing() {
        let s = site(0);
        let goal = render(&s, s.shown.as_ref().unwrap());
        assert!(goal.contains("<strong>What it is.</strong>"), "Markdown is rendered");
        assert!(!goal.contains("<script") && goal.contains("&lt;script&gt;"), "raw HTML is shown as text: {goal}");
        assert!(goal.contains("<a href=\"/goal\" aria-current=\"page\">Goal</a>") && goal.contains("margin-left:1em\"><a href=\"/tickets\">Tickets</a>"), "every page listed, by depth");
        // A link leads only to a page of the site; any other, and an image, is its text.
        assert!(goal.contains("<a href=\"/tickets\">tickets</a>"), "{goal}");
        assert!(goal.contains("the board") && !goal.contains("href=\"/board\""), "board is not a page of the site: {goal}");
        assert!(!goal.contains("example.com/auto\"") && !goal.contains("href=\"https") && !goal.contains("javascript:") && !goal.contains("<img"), "{goal}");
        assert!(goal.contains("elsewhere") && goal.contains("run") && goal.contains("a picture"), "their text stays: {goal}");
        let s = site(1);
        let tickets = render(&s, s.shown.as_ref().unwrap());
        assert!(tickets.contains("It SHALL &lt;list&gt;.") && tickets.contains("<b>Given</b> g"), "a requirement's statement and scenarios, escaped: {tickets}");
        // Named by the root, above the list and in the browser title, escaped.
        assert!(tickets.contains("<title>Tickets · Grim &amp; co</title>") && tickets.contains("<p class=\"name\"><a href=\"/\">Grim &amp; co</a></p><ul>"), "{tickets}");
        assert!(empty(Some("Grim")).contains("shows nothing here yet") && empty(Some("Grim")).contains("<title>Nothing yet · Grim</title>"));
        assert!(empty(None).contains("<title>Nothing yet</title>"), "no root, no name");
    }

    /// # Test contract
    ///
    /// ```json
    /// {"test": "the-site-serves-its-pages-and-nothing-else", "checks": ["crates/server/src/site#only-the-site", "crates/server/src#site-listener"]}
    /// ```
    #[tokio::test]
    async fn the_site_serves_its_pages_and_nothing_else() {
        use grimoire_core::model::{Page, Reference, Requirement, Scope, StoreConfig};
        use grimoire_core::Store;
        let tmp = tempfile::Builder::new().prefix("grimoire-site-").tempdir().unwrap();
        let dir = tmp.path().join("store");
        let page = |slug: &str, links: &[&str]| Page { slug: slug.into(), title: slug.to_uppercase(), body: format!("The {slug} page."), links: links.iter().map(|l| l.parse::<Reference>().unwrap()).collect() };
        let config = StoreConfig { site: vec!["goal".into(), "guarantees".into()], version: StoreConfig::CURRENT_VERSION, repository: "C:/a/checkout".into(), roots: vec!["overview".into()], languages: vec![], scope: Scope::default(), order: Default::default() };
        let mut store = Store::new(config);
        for p in [page("overview", &["page:goal", "page:guarantees", "page:architecture"]), page("goal", &[]), page("guarantees", &["page:tickets"]), page("tickets", &["req:board", "module:src/ops"]), page("architecture", &[])] {
            store.pages.insert(p.slug.clone(), p);
        }
        let board = Requirement { slug: "board".into(), title: "board".into(), statement: "It SHALL list.".into(), scenarios: vec![Scenario { title: "T".into(), given: "g".into(), when: "w".into(), then: "t".into() }], covered_by: vec![], decisions: vec![grimoire_core::model::Decision::new("2026-10-04", "A reason nobody outside reads.")], retired: false };
        store.requirements.insert("board".into(), board);
        crate::store_io::write_all(&dir, &store, false).unwrap();
        let daemon = Arc::new(Daemon::open_shared(&dir).unwrap());
        let writer = daemon.clone();
        // The site on an address of its own, as serve opens it; an address already taken is refused.
        let at = crate::serve_site(daemon.clone(), "127.0.0.1:0".parse().unwrap()).await.unwrap();
        assert!(crate::serve_site(daemon, at).await.is_err(), "the address is taken");
        let get = move |path: &'static str| {
            tokio::task::spawn_blocking(move || {
                let agent: ureq::Agent = ureq::Agent::config_builder().http_status_as_error(false).build().into();
                let mut r = agent.get(&format!("http://{at}{path}")).call().unwrap();
                (r.status().as_u16(), r.body_mut().read_to_string().unwrap_or_default())
            })
        };
        let (status, first) = get("/").await.unwrap();
        assert_eq!(status, 200);
        let order: Vec<usize> = ["/goal\"", "/guarantees\"", "/tickets\""].iter().map(|l| first.find(l).unwrap_or_else(|| panic!("{l} listed: {first}"))).collect();
        assert!(order.windows(2).all(|w| w[0] < w[1]) && first.contains("The goal page."), "the first page, and every page in order");
        assert!(first.contains("<title>GOAL · OVERVIEW</title>") && !first.contains("The overview page."), "named by the root, and nothing else of it: {first}");
        let (status, tickets) = get("/tickets").await.unwrap();
        assert_eq!(status, 200);
        assert!(tickets.contains("It SHALL list.") && tickets.contains("<b>Given</b> g"), "{tickets}");
        assert!(!tickets.contains("A reason nobody outside reads.") && !tickets.contains("src/ops"), "no decision and no module: {tickets}");
        // Nothing deeper, and nothing but the site.
        for path in ["/architecture", "/overview", "/api/status", "/api/tools", "/mcp", "/api/events", "/board"] {
            let (status, body) = get(path).await.unwrap();
            assert_eq!(status, 404, "{path} is not served on the site's address");
            assert!(body.contains("<title>Not found · OVERVIEW</title>"), "under the site's name: {body}");
        }
        for path in ["/api/status", "/goal", "/"] {
            let post = tokio::task::spawn_blocking(move || {
                let agent: ureq::Agent = ureq::Agent::config_builder().http_status_as_error(false).build().into();
                agent.post(&format!("http://{at}{path}")).header("content-type", "application/json").send("{}").unwrap().status().as_u16()
            });
            assert_eq!(post.await.unwrap(), 404, "a POST to {path}: nothing is written or called on the site's address");
        }
        // Current at each request: a change to the setting shows on the next one, with nothing restarted.
        assert_eq!(writer.call("configure", serde_json::json!({ "site": ["nope"] })).unwrap_err().code(), "invalid", "a page that does not exist");
        writer.call("configure", serde_json::json!({ "site": ["goal"] })).unwrap();
        let (_, now) = get("/").await.unwrap();
        assert!(now.contains("/goal\"") && !now.contains("/guarantees\""), "{now}");
        assert_eq!(get("/tickets").await.unwrap().0, 404, "no longer part of the site");
    }
}
