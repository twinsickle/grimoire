//! # Module contract
//!
//! ```json
//! {"purpose":"Serve the project's site: how it is meant to be used, rendered from the truth at each request as plain HTML on an address of its own, with nothing else served there.","criteria":[{"id":"renders","statement":"Each page of the site renders as one self-contained HTML document with no script: the site's name, linking /, above a list of every page of the site in order, indented by depth, linking each at /<slug>, then the page's title, its body as Markdown with raw HTML shown as escaped text, a link kept only to somewhere on the site, a page, the catalog, a recipe or an anchor on the same page, and any other link or image shown as its text, and each requirement it links by title, statement and scenarios; its browser title is the page's title then the site's name; every text from the store is escaped, and an empty site renders a page saying the project shows nothing yet; while the site holds any recipe, the list of pages ends with a link to the catalog, a page lists the recipes it links after its requirements, each linking its address, each with any anchor kept; on a recipe's page the catalog's link is marked current."},{"id":"recipe-pages","statement":"The catalog renders every recipe grouped under its kind, each by its title linking /recipes/<id>, with the first paragraph of its prose as Markdown and a mark when it is due; a recipe's page renders its title, its kind, when it was last judged and whether it held or was updated, or that it never was, whether it is due, its prose as Markdown as a body is, each ingredient by its id and what it says under the anchor of its id, and the recipes it uses and those that use it, each linking its address; nothing of its files is rendered, and every text is escaped."},{"id":"only-the-site","statement":"The site's router answers GET (and so HEAD) / with the first page of the site, /<slug> with that page when it is in the site, /recipes/ with the catalog, and /recipes too unless the site holds a page recipes, which it then answers, and /recipes/<id> with that recipe when the code holds it, and every other path and method with 404, so nothing but the site is served on its address, no API, event stream, MCP or builder's UI, and no caller is asked for a token."},{"id":"library-site","statement":"router_in serves, as router does, the site of the grimoire a library holds by the name given, looked up as each request arrives, so a grimoire taken or let go after it started is followed; while the library holds none by that name, every path answers 404 with a page saying that grimoire is not served here."}]}
//! ```

use std::sync::Arc;

use axum::extract::{Path, State};
use axum::http::{header, StatusCode};
use axum::response::{IntoResponse, Response};
use axum::routing::get;
use axum::Router;
use grimoire_core::model::JudgementOutcome;
use grimoire_core::query::{Site, SitePage, SiteRecipe, SiteRecipeEntry};
use pulldown_cmark::{html, CowStr, Event, Options, Parser, Tag, TagEnd};

use crate::daemon::Daemon;
use crate::library::live::Live;

/// Whose site a router serves.
enum Source {
    /// One daemon's, for as long as it serves.
    Daemon(Arc<Daemon>),
    /// The grimoire a library holds by this name, whichever that is as a request arrives.
    Library(Arc<Live>, String),
}

impl Source {
    fn daemon(&self) -> Option<Arc<Daemon>> {
        match self {
            Source::Daemon(d) => Some(d.clone()),
            Source::Library(live, name) => live.daemon_of(name),
        }
    }

    /// Not found, under the site's name, or saying whose site is not served yet.
    fn unserved(&self) -> Response {
        match self {
            Source::Library(_, name) => (
                StatusCode::NOT_FOUND,
                [(header::CONTENT_TYPE, "text/html; charset=utf-8")],
                document("Not served", None, &format!("<main><h1>Not served</h1><p>This server holds no grimoire <code>{}</code> yet, so its site is not served here.</p></main>", escape(name))),
            )
                .into_response(),
            Source::Daemon(_) => not_found(None),
        }
    }
}

/// The site's router: its pages and nothing else.
pub fn router(daemon: Arc<Daemon>) -> Router {
    over(Source::Daemon(daemon))
}

/// The router of the site of the grimoire `name` in a team's library, looked up at each request.
pub fn router_in(live: Arc<Live>, name: String) -> Router {
    over(Source::Library(live, name))
}

fn over(source: Source) -> Router {
    Router::new()
        .route("/", get(first))
        .route("/{slug}", get(page))
        .route("/recipes/", get(catalog))
        .route("/recipes/{id}", get(recipe))
        .fallback(elsewhere)
        .method_not_allowed_fallback(elsewhere)
        .with_state(Arc::new(source))
}

async fn first(State(source): State<Arc<Source>>) -> Response {
    let Some(daemon) = source.daemon() else { return source.unserved() };
    let site = daemon.site(None);
    match &site.shown {
        Some(p) => html_page(render(&site, p)),
        None => html_page(empty(&site)),
    }
}

async fn page(State(source): State<Arc<Source>>, Path(slug): Path<String>) -> Response {
    let Some(daemon) = source.daemon() else { return source.unserved() };
    let site = daemon.site(Some(&slug));
    match &site.shown {
        Some(p) => html_page(render(&site, p)),
        // A page of the site called recipes keeps its address; otherwise it is the catalog's too.
        None if slug == "recipes" => html_page(render_catalog(&site)),
        None => not_found(site.name.as_deref()),
    }
}

async fn catalog(State(source): State<Arc<Source>>) -> Response {
    let Some(daemon) = source.daemon() else { return source.unserved() };
    html_page(render_catalog(&daemon.site(Some(""))))
}

async fn recipe(State(source): State<Arc<Source>>, Path(id): Path<String>) -> Response {
    let Some(daemon) = source.daemon() else { return source.unserved() };
    match daemon.site_recipe(&id) {
        (site, Some(r)) => html_page(render_recipe(&site, &r)),
        (site, None) => not_found(site.name.as_deref()),
    }
}

/// Any other path or method: not found, under the site's name.
async fn elsewhere(State(source): State<Arc<Source>>) -> Response {
    let Some(daemon) = source.daemon() else { return source.unserved() };
    // An empty slug names no page, so only the list and the name are gathered.
    not_found(daemon.site(Some("")).name.as_deref())
}

fn not_found(name: Option<&str>) -> Response {
    (
        StatusCode::NOT_FOUND,
        [(header::CONTENT_TYPE, "text/html; charset=utf-8")],
        document("Not found", name, "<main><h1>Not found</h1><p>This page is not part of the site. <a href=\"/\">Start at the beginning.</a></p></main>"),
    )
        .into_response()
}

fn html_page(body: String) -> Response {
    // No script runs here, and the page loads nothing from anywhere else.
    ([(header::CONTENT_TYPE, "text/html; charset=utf-8"), (header::CONTENT_SECURITY_POLICY, "default-src 'none'; style-src 'unsafe-inline'"), (header::CACHE_CONTROL, "no-cache")], body)
        .into_response()
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

/// Where a link in a body leads on the site, when it leads anywhere on it: a page of the site, `/<slug>`
/// or `<slug>`, the catalog, or a recipe's address, each with any anchor kept.
fn on_the_site(dest: &str, site: &Site) -> Option<String> {
    // An anchor on the same page, such as an ingredient's, leads nowhere off it.
    if dest.starts_with('#') {
        return Some(dest.to_string());
    }
    let (path, anchor) = dest.split_once('#').map_or((dest, None), |(p, a)| (p, Some(a)));
    let path = path.strip_prefix('/').unwrap_or(path);
    let recipes = !site.recipes.is_empty();
    let kept =
        site.pages.iter().any(|p| p.slug == path) || (recipes && (path == "recipes/" || path == "recipes")) || path.strip_prefix("recipes/").is_some_and(|id| site.recipes.iter().any(|r| r.id == id));
    kept.then(|| match anchor {
        Some(a) => format!("/{path}#{a}"),
        None => format!("/{path}"),
    })
}

/// A body as Markdown, any raw HTML in it shown as text, so a page can run nothing. A link stays a
/// link only to somewhere on the site; any other link, and an image, is its text, so the site leads
/// nowhere else and fetches nothing.
fn markdown(body: &str, site: &Site) -> String {
    let mut kept = false;
    let events = Parser::new_ext(body, Options::ENABLE_TABLES | Options::ENABLE_STRIKETHROUGH).filter_map(|e| match e {
        Event::Html(t) | Event::InlineHtml(t) => Some(Event::Text(t)),
        Event::Start(Tag::Link { link_type, dest_url, title, id }) => {
            let to = on_the_site(&dest_url, site);
            kept = to.is_some();
            to.map(|to| Event::Start(Tag::Link { link_type, dest_url: CowStr::from(to), title, id }))
        }
        Event::End(TagEnd::Link) => kept.then_some(Event::End(TagEnd::Link)),
        Event::Start(Tag::Image { .. }) | Event::End(TagEnd::Image) => None,
        e => Some(e),
    });
    let mut out = String::new();
    html::push_html(&mut out, events);
    out
}

/// Where in the site a document is: a page, the catalog or a recipe in it, or anywhere else.
#[derive(PartialEq)]
enum Here<'a> {
    Page(&'a str),
    Catalog,
    Recipe,
    Elsewhere,
}

/// The list of pages, the catalog's link after it while there are recipes, beside what is shown.
fn layout(site: &Site, here: Here, title: &str, main: &str) -> String {
    let mut nav = String::from("<nav aria-label=\"Pages\">");
    if let Some(name) = &site.name {
        nav.push_str(&format!("<p class=\"name\"><a href=\"/\">{}</a></p>", escape(name)));
    }
    nav.push_str("<ul>");
    for p in &site.pages {
        let current = if here == Here::Page(&p.slug) { " aria-current=\"page\"" } else { "" };
        nav.push_str(&format!("<li style=\"margin-left:{}em\"><a href=\"/{}\"{current}>{}</a></li>", p.depth, escape(&p.slug), escape(&p.title)));
    }
    nav.push_str("</ul>");
    if !site.recipes.is_empty() {
        let current = if here == Here::Catalog || here == Here::Recipe { " aria-current=\"page\"" } else { "" };
        nav.push_str(&format!("<p class=\"catalog\"><a href=\"/recipes/\"{current}>Recipes</a></p>"));
    }
    nav.push_str("</nav>");
    document(title, site.name.as_deref(), &format!("<div class=\"layout\">{nav}<main>{main}</main></div>"))
}

/// A recipe as a list item: its title linking its address, its kind, and a mark when it is due.
fn recipe_item(r: &SiteRecipeEntry, with_kind: bool) -> String {
    let kind = if with_kind { format!(" <span class=\"kind\">{}</span>", escape(&r.kind)) } else { String::new() };
    let due = if r.due { " <span class=\"due\">due</span>" } else { "" };
    format!("<a href=\"/recipes/{}\">{}</a>{kind}{due}", escape(&r.id), escape(&r.title))
}

fn render(site: &Site, current: &SitePage) -> String {
    let mut main = format!("<h1>{}</h1><div class=\"body\">{}</div>", escape(&current.title), markdown(&current.body, site));
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
    if !current.recipes.is_empty() {
        main.push_str("<section class=\"req\"><h2>Recipes</h2><ul class=\"recipes\">");
        for r in &current.recipes {
            main.push_str(&format!("<li>{}</li>", recipe_item(r, true)));
        }
        main.push_str("</ul></section>");
    }
    layout(site, Here::Page(&current.slug), &current.title, &main)
}

/// The catalog: every recipe under its kind, each with the first thing it says.
fn render_catalog(site: &Site) -> String {
    let mut main = String::from("<h1>Recipes</h1><p class=\"lead\">What this project offers the people who build with it, and how to use each.</p>");
    if site.recipes.is_empty() {
        main.push_str("<p>This project declares no recipes yet.</p>");
    }
    let mut kind: Option<&str> = None;
    for r in &site.recipes {
        if kind != Some(r.kind.as_str()) {
            if kind.is_some() {
                main.push_str("</ul></section>");
            }
            main.push_str(&format!("<section class=\"kind-group\"><h2>{}</h2><ul class=\"recipes\">", escape(&r.kind)));
            kind = Some(&r.kind);
        }
        main.push_str(&format!("<li>{}<div class=\"summary\">{}</div></li>", recipe_item(r, false), markdown(&r.summary, site)));
    }
    if kind.is_some() {
        main.push_str("</ul></section>");
    }
    layout(site, Here::Catalog, "Recipes", &main)
}

/// When a recipe was last judged, and whether what it says was checked against what it represents as it is now.
fn judged(r: &SiteRecipe) -> String {
    let last = match &r.judged {
        Some(j) if j.outcome == JudgementOutcome::Held => format!("Last judged on {}: it held.", escape(&j.date)),
        Some(j) => format!("Last judged on {}: it was updated to match.", escape(&j.date)),
        None => "Never judged.".to_string(),
    };
    let state = match (r.due, &r.judged) {
        (false, _) => "<span class=\"current\">Current</span>: checked against what it represents as it is now.",
        (true, Some(_)) => "<span class=\"due\">Due</span>: it or what it represents changed since, so it has not been checked as it is now.",
        (true, None) => "<span class=\"due\">Due</span>: not yet checked against what it represents.",
    };
    format!("<p class=\"judged\">{last} {state}</p>")
}

/// One recipe: what it says, its ingredients each at its anchor, and the recipes it uses and that use it.
fn render_recipe(site: &Site, r: &SiteRecipe) -> String {
    let mut main = format!("<h1>{}</h1><p class=\"meta\"><span class=\"kind\">{}</span> <code>{}</code></p>{}", escape(&r.title), escape(&r.kind), escape(&r.id), judged(r));
    main.push_str(&format!("<div class=\"body\">{}</div>", markdown(&r.text, site)));
    if !r.ingredients.is_empty() {
        main.push_str("<section class=\"req\"><h2>Ingredients</h2>");
        for i in &r.ingredients {
            main.push_str(&format!("<div class=\"ingredient\" id=\"{}\"><code>{}</code> {}</div>", escape(&i.id), escape(&i.id), escape(&i.says)));
        }
        main.push_str("</section>");
    }
    for (heading, list) in [("Uses", &r.uses), ("Used by", &r.used_by)] {
        if !list.is_empty() {
            main.push_str(&format!("<section class=\"req\"><h2>{heading}</h2><ul class=\"recipes\">"));
            for u in list.iter() {
                main.push_str(&format!("<li>{}</li>", recipe_item(u, true)));
            }
            main.push_str("</ul></section>");
        }
    }
    layout(site, Here::Recipe, &r.title, &main)
}

fn empty(site: &Site) -> String {
    layout(site, Here::Elsewhere, "Nothing yet", "<h1>Nothing yet</h1><p>This project shows nothing here yet.</p>")
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
nav .catalog{margin:12px 0 0;padding-top:10px;border-top:1px solid #2e3542}nav .catalog a{text-decoration:none}nav .catalog a[aria-current]{color:#d7dbe3;font-weight:600}\
.lead,.meta{color:#8b93a3}.kind{color:#8b93a3;font-size:12px;text-transform:uppercase;letter-spacing:.04em}\
.meta code{color:#8b93a3;font-size:13px;margin-left:6px}\
.due{color:#e0af68;font-weight:600}.current{color:#9ece6a;font-weight:600}li .due{font-size:12px;text-transform:uppercase;letter-spacing:.04em}\
.judged{background:#161a21;border:1px solid #2e3542;border-radius:6px;padding:8px 12px}\
ul.recipes{list-style:none;margin:0;padding:0}ul.recipes li{padding:6px 0}ul.recipes li .kind,ul.recipes li .due{margin-left:8px}\
.summary p{margin:2px 0 0;color:#aab1bf}.kind-group{margin-top:20px}\
.ingredient{padding:6px 0}.ingredient code{color:#c3a6ff;margin-right:8px}.ingredient:target{background:#1e232c;border-radius:4px}\
";

#[cfg(test)]
mod tests {
    use super::*;
    use grimoire_core::model::Scenario;
    use grimoire_core::query::{SiteEntry, SiteIngredient, SiteJudged, SiteRequirement};

    fn site(shown: usize) -> Site {
        let req = SiteRequirement {
            slug: "board".into(),
            title: "board".into(),
            statement: "It SHALL <list>.".into(),
            scenarios: vec![Scenario { title: "T".into(), given: "g".into(), when: "w".into(), then: "t".into() }],
        };
        let body = "**What it is.** <script>alert(1)</script> See [tickets](/tickets), [the board](board), [elsewhere](https://example.com), <https://example.com/auto>, [run](javascript:alert(1)) and ![a picture](https://example.com/p.png).";
        let pages = [
            SitePage { slug: "goal".into(), title: "Goal".into(), body: body.into(), requirements: vec![], recipes: vec![] },
            SitePage { slug: "tickets".into(), title: "Tickets".into(), body: "A ticket.".into(), requirements: vec![req], recipes: vec![] },
        ];
        Site {
            name: Some("Grim & co".into()),
            pages: vec![SiteEntry { slug: "goal".into(), title: "Goal".into(), depth: 0 }, SiteEntry { slug: "tickets".into(), title: "Tickets".into(), depth: 1 }],
            recipes: vec![],
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
        let nothing = Site { name: Some("Grim".into()), ..Site::default() };
        assert!(empty(&nothing).contains("shows nothing here yet") && empty(&nothing).contains("<title>Nothing yet · Grim</title>"));
        assert!(!empty(&nothing).contains("/recipes/"), "no recipes, no catalog: {}", empty(&nothing));
        assert!(empty(&Site::default()).contains("<title>Nothing yet</title>"), "no root, no name");
    }

    fn entry(id: &str, kind: &str, due: bool) -> SiteRecipeEntry {
        SiteRecipeEntry { id: id.into(), title: id.to_uppercase(), kind: kind.into(), summary: format!("How to use **{id}** <b>here</b>."), due }
    }

    /// # Test contract
    ///
    /// ```json
    /// {"test": "the-catalog-and-a-recipe-render-what-they-say", "checks": ["crates/server/src/site#recipe-pages", "crates/server/src/site#renders"]}
    /// ```
    #[test]
    fn the_catalog_and_a_recipe_render_what_they_say() {
        let mut s = site(0);
        s.recipes = vec![entry("button", "component", false), entry("icon", "component", true), entry("quest", "content & more", true)];
        // The list of pages ends with the catalog's link while there are recipes, on every page.
        let goal = render(&s, s.shown.as_ref().unwrap());
        assert!(goal.contains("</ul><p class=\"catalog\"><a href=\"/recipes/\">Recipes</a></p></nav>"), "{goal}");
        assert!(empty(&s).contains("href=\"/recipes/\""), "the empty page links it too");
        // The catalog: every recipe under its kind, its first paragraph as Markdown, escaped, and due marked.
        let catalog = render_catalog(&s);
        assert!(catalog.contains("<a href=\"/recipes/\" aria-current=\"page\">Recipes</a>"), "{catalog}");
        let component = catalog.find("<h2>component</h2>").expect("the component kind");
        let content = catalog.find("<h2>content &amp; more</h2>").expect("the content kind, escaped");
        let (button, icon, quest) = (catalog.find("/recipes/button\"").unwrap(), catalog.find("/recipes/icon\"").unwrap(), catalog.find("/recipes/quest\"").unwrap());
        assert!(component < button && button < icon && icon < content && content < quest, "grouped by kind, in order: {catalog}");
        assert!(catalog.contains("<a href=\"/recipes/button\">BUTTON</a><div class=\"summary\"><p>How to use <strong>button</strong> &lt;b&gt;here&lt;/b&gt;.</p>"), "{catalog}");
        assert!(catalog.contains("<a href=\"/recipes/icon\">ICON</a> <span class=\"due\">due</span>"), "{catalog}");
        assert!(render_catalog(&Site::default()).contains("declares no recipes yet"));
        // A recipe's page: what it says, when last judged, its ingredients at their anchors, what it uses and what uses it.
        let button = SiteRecipe {
            id: "button".into(),
            title: "Button <1>".into(),
            kind: "component".into(),
            text: "Press it, [its label](#label). See [the icon](/recipes/icon#name), [the catalog](/recipes/), [goal](goal#top), [a missing one](/recipes/nope) and [away](https://example.com)."
                .into(),
            ingredients: vec![SiteIngredient { id: "label".into(), says: "The text <shown>.".into() }],
            uses: vec![entry("icon", "component", true)],
            used_by: vec![entry("quest", "content", true)],
            judged: Some(SiteJudged { date: "2026-10-07".into(), outcome: JudgementOutcome::Held }),
            due: false,
        };
        let page = render_recipe(&s, &button);
        assert!(page.contains("<h1>Button &lt;1&gt;</h1>") && page.contains("<title>Button &lt;1&gt; · Grim &amp; co</title>"), "{page}");
        assert!(page.contains("<span class=\"kind\">component</span>") && page.contains("Last judged on 2026-10-07: it held.") && page.contains("Current</span>"), "{page}");
        assert!(page.contains("<div class=\"ingredient\" id=\"label\"><code>label</code> The text &lt;shown&gt;.</div>"), "{page}");
        assert!(
            page.contains("<a href=\"/recipes/icon#name\">the icon</a>") && page.contains("<a href=\"/recipes/\">the catalog</a>") && page.contains("<a href=\"/goal#top\">goal</a>"),
            "links on the site stay: {page}"
        );
        assert!(!page.contains("href=\"/recipes/nope\"") && !page.contains("href=\"https") && page.contains("a missing one"), "others are their text: {page}");
        let uses = page.find("<h2>Uses</h2>").unwrap();
        let used_by = page.find("<h2>Used by</h2>").unwrap();
        assert!(uses < page.find("/recipes/icon\">ICON</a>").unwrap() && used_by < page.find("/recipes/quest\">QUEST</a>").unwrap() && uses < used_by, "{page}");
        let updated = render_recipe(&s, &SiteRecipe { judged: Some(SiteJudged { date: "2026-10-01".into(), outcome: JudgementOutcome::Updated }), due: true, ..button.clone() });
        assert!(updated.contains("it was updated to match.") && updated.contains("<span class=\"due\">Due</span>: it or what it represents changed since"), "{updated}");
        let never = render_recipe(&s, &SiteRecipe { judged: None, due: true, ..button });
        assert!(never.contains("Never judged.") && never.contains("not yet checked"), "{never}");
        assert!(page.contains("<a href=\"#label\">its label</a>") && page.contains("<a href=\"/recipes/\" aria-current=\"page\">Recipes</a>"), "{page}");
        // A page lists the recipes it links, after its requirements.
        let mut tickets = s.clone();
        tickets.shown = Some(SitePage { recipes: vec![entry("button", "component", false)], ..site(1).shown.unwrap() });
        let t = render(&tickets, tickets.shown.as_ref().unwrap());
        assert!(t.find("<h2>Recipes</h2>").unwrap() > t.find("It SHALL").unwrap() && t.contains("<li><a href=\"/recipes/button\">BUTTON</a> <span class=\"kind\">component</span></li>"), "{t}");
    }

    /// # Test contract
    ///
    /// ```json
    /// {"test": "the-recipes-are-served-at-their-addresses", "checks": ["crates/server/src/site#only-the-site", "req:site"]}
    /// ```
    #[tokio::test]
    async fn the_recipes_are_served_at_their_addresses() {
        use grimoire_core::model::{Page, Reference, Scope, StoreConfig};
        use grimoire_core::Store;
        let tmp = tempfile::Builder::new().prefix("grimoire-site-").tempdir().unwrap();
        let repo = tmp.path().join("repo");
        std::fs::create_dir_all(repo.join("ui")).unwrap();
        let recipe = |id: &str, prose: &str, uses: &str| {
            format!("# {}\n\n{prose}\n\n## Recipe\n\n```json\n{{\"id\":\"{id}\",\"kind\":\"component\",\"ingredients\":[{{\"id\":\"label\",\"says\":\"The text.\"}}],\"files\":[\"ui/{id}.ts\"],\"uses\":[{uses}]}}\n```\n", id.to_uppercase())
        };
        std::fs::write(repo.join("ui").join("button.recipe.md"), recipe("button", "A button.", "\"icon\"")).unwrap();
        std::fs::write(repo.join("ui").join("icon.recipe.md"), recipe("icon", "An icon.", "")).unwrap();
        std::fs::write(repo.join("ui").join("button.ts"), "export const button = 1;\n").unwrap();
        let dir = tmp.path().join("store");
        let config = StoreConfig {
            large_file: None,
            plain_file: None,
            site: vec!["goal".into()],
            version: StoreConfig::CURRENT_VERSION,
            repository: repo.to_string_lossy().replace('\\', "/"),
            roots: vec!["overview".into()].into(),
            languages: vec![],
            scope: Scope::default(),
            order: Default::default(),
            recipe_kinds: Default::default(),
        };
        let mut store = Store::new(config);
        for (slug, links) in [("overview", vec!["page:goal"]), ("goal", vec!["recipe:button"]), ("recipes", vec![])] {
            store
                .pages
                .insert(slug.into(), Page { slug: slug.into(), title: slug.to_uppercase(), body: format!("The {slug} page."), links: links.iter().map(|l| l.parse::<Reference>().unwrap()).collect() });
        }
        store.tickets.insert(
            "work".into(),
            grimoire_core::model::Ticket {
                slug: "work".into(),
                title: "Work".into(),
                kind: grimoire_core::model::TicketKind::Change,
                priority: grimoire_core::model::Priority::Later,
                status: grimoire_core::model::Status::Active,
                touches: vec![],
                relies_on: vec![],
                body: "b".into(),
                decision: None,
            },
        );
        store.current = Some("work".into());
        crate::store_io::write_all(&dir, &store, false).unwrap();
        let daemon = Arc::new(Daemon::open(&dir).unwrap().0);
        daemon.call("judge_recipe", serde_json::json!({ "ref": "recipe:button", "outcome": "held" })).unwrap();
        let writer = daemon.clone();
        let at = crate::serve_site(router(daemon), "127.0.0.1:0".parse().unwrap()).await.unwrap();
        let get = move |path: &'static str| {
            tokio::task::spawn_blocking(move || {
                let agent: ureq::Agent = ureq::Agent::config_builder().http_status_as_error(false).build().into();
                let mut r = agent.get(&format!("http://{at}{path}")).call().unwrap();
                (r.status().as_u16(), r.body_mut().read_to_string().unwrap_or_default())
            })
        };
        // The catalog, at both its addresses while no page of the site is called recipes; the page links it.
        let (status, catalog) = get("/recipes/").await.unwrap();
        assert_eq!(status, 200);
        assert!(catalog.contains("<h2>component</h2>") && catalog.contains("<a href=\"/recipes/button\">BUTTON</a>") && catalog.contains("<p>A button.</p>"), "{catalog}");
        assert!(catalog.contains("<a href=\"/recipes/icon\">ICON</a> <span class=\"due\">due</span>"), "never judged, so due: {catalog}");
        assert_eq!(get("/recipes").await.unwrap(), (200, catalog.clone()));
        let (_, goal) = get("/").await.unwrap();
        assert!(goal.contains("<a href=\"/recipes/\">Recipes</a>") && goal.contains("<li><a href=\"/recipes/button\">BUTTON</a>"), "{goal}");
        // A recipe at its address: judged, what it uses and what uses it, and none of its files.
        let (status, button) = get("/recipes/button").await.unwrap();
        assert_eq!(status, 200);
        assert!(button.contains("it held.") && button.contains("Current</span>") && button.contains("id=\"label\"") && button.contains("/recipes/icon\">ICON</a>"), "{button}");
        assert!(!button.contains("button.ts") && !button.contains("button.recipe.md"), "no file: {button}");
        let (_, icon) = get("/recipes/icon").await.unwrap();
        assert!(icon.contains("Never judged.") && icon.contains("<h2>Used by</h2>") && icon.contains("/recipes/button\">BUTTON</a>"), "{icon}");
        for path in ["/recipes/nope", "/recipes/button/label", "/recipes/button/", "/recipes/Not%20a%20slug"] {
            assert_eq!(get(path).await.unwrap().0, 404, "{path}");
        }
        // Moved, with its prose changed: the same address, and due.
        std::fs::create_dir_all(repo.join("lib")).unwrap();
        std::fs::remove_file(repo.join("ui").join("button.recipe.md")).unwrap();
        std::fs::write(repo.join("lib").join("button.recipe.md"), recipe("button", "A button, pressed.", "\"icon\"")).unwrap();
        writer.catch_up().unwrap();
        let (status, moved) = get("/recipes/button").await.unwrap();
        assert_eq!(status, 200);
        assert!(moved.contains("A button, pressed.") && moved.contains("<span class=\"due\">Due</span>"), "{moved}");
        // A page of the site called recipes keeps its own address; the catalog stays at /recipes/.
        writer.call("configure", serde_json::json!({ "site": ["goal", "recipes"] })).unwrap();
        let (_, page) = get("/recipes").await.unwrap();
        assert!(page.contains("The recipes page."), "{page}");
        assert!(get("/recipes/").await.unwrap().1.contains("<h1>Recipes</h1>"));
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
        let page = |slug: &str, links: &[&str]| Page {
            slug: slug.into(),
            title: slug.to_uppercase(),
            body: format!("The {slug} page."),
            links: links.iter().map(|l| l.parse::<Reference>().unwrap()).collect(),
        };
        let config = StoreConfig {
            large_file: None,
            plain_file: None,
            site: vec!["goal".into(), "guarantees".into()],
            version: StoreConfig::CURRENT_VERSION,
            repository: "C:/a/checkout".into(),
            roots: vec!["overview".into()].into(),
            languages: vec![],
            scope: Scope::default(),
            order: Default::default(),
            recipe_kinds: Default::default(),
        };
        let mut store = Store::new(config);
        for p in [
            page("overview", &["page:goal", "page:guarantees", "page:architecture"]),
            page("goal", &[]),
            page("guarantees", &["page:tickets"]),
            page("tickets", &["req:board", "module:src/ops"]),
            page("architecture", &[]),
        ] {
            store.pages.insert(p.slug.clone(), p);
        }
        let board = Requirement {
            slug: "board".into(),
            title: "board".into(),
            statement: "It SHALL list.".into(),
            scenarios: vec![Scenario { title: "T".into(), given: "g".into(), when: "w".into(), then: "t".into() }],
            covered_by: vec![],
            decisions: vec![grimoire_core::model::Decision::new("2026-10-04", "A reason nobody outside reads.")],
            retired: false,
        };
        store.requirements.insert("board".into(), board);
        crate::store_io::write_all(&dir, &store, false).unwrap();
        let daemon = Arc::new(Daemon::open_shared(&dir).unwrap());
        let writer = daemon.clone();
        // The site on an address of its own, as serve opens it; an address already taken is refused.
        let at = crate::serve_site(router(daemon.clone()), "127.0.0.1:0".parse().unwrap()).await.unwrap();
        assert!(crate::serve_site(router(daemon), at).await.is_err(), "the address is taken");
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
