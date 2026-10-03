/**
 * What each view does, against the fixture store serve.mjs builds: a root
 * page and a second root with one page under it, an area with one
 * requirement and two contracted modules, a module
 * without a contract, one test, and tickets open, high, closed and a gap, the
 * first active with a proposal and one closed touching a requirement and
 * relying on a page that no longer exist.
 * Every finding at that point is adopted as the baseline, and one requirement
 * added after it, so the views have inherited and new findings to tell apart.
 * Every reference is a link to /n/<ref>, so a view's structure can be read
 * back from its links and compared with the tool answer it was drawn from.
 */
import { expect, test, type Page } from "@playwright/test";
import { ROUTES } from "./routes";

type Rollup = { modules: number; contracted: number; criteria: number; tested: number; requirements: number; held: number };
type Tree = { ref: string; title: string; children: Tree[]; checks: string[]; findings: number; baselined: number; rollup: Rollup };
type Outline = { roots: Tree[]; unplaced: Tree[] };

async function tool<T>(page: Page, name: string, input: object = {}): Promise<T> {
  const res = await page.request.post(`/api/${name}`, { data: input });
  return (await res.json()) as T;
}

/** The references a part of the page links to, in document order. */
async function linkedRefs(page: Page, scope = "main"): Promise<string[]> {
  const hrefs = await page.locator(`${scope} a[href^="/n/"]`).evaluateAll((as) => as.map((a) => a.getAttribute("href") ?? ""));
  return hrefs.map((h) => decodeURIComponent(h.slice("/n/".length)));
}

/** The pages of an outline, depth first in link order: the order of the rail. */
function pagesOf(trees: Tree[]): Tree[] {
  return trees.filter((t) => t.ref.startsWith("page:")).flatMap((t) => [t, ...pagesOf(t.children)]);
}

function find(trees: Tree[], ref: string): Tree | undefined {
  for (const t of trees) {
    if (t.ref === ref) return t;
    const below = find(t.children, ref);
    if (below) return below;
  }
  return undefined;
}

const rail = "nav[aria-label='Pages']";

/**
 * # Test contract
 *
 * ```json
 * {"test": "three-views-and-nothing-else", "checks": ["web/src/App#three-views"]}
 * ```
 */
test("the three views are served, a page is the grimoire, and nothing else", async ({ page }) => {
  for (const [path, marker] of [
    ["/", rail],
    ["/n/page%3Agoal", rail],
    ["/unplaced", rail],
    ["/board", "section[id^='band-']"],
    ["/n/req%3Aready-needs-commit", "h1"],
  ] as const) {
    await page.goto(path);
    await expect(page.locator(marker).first(), path).toBeVisible();
  }
  // A requirement's node page has no rail; a page's is the grimoire view.
  await page.goto("/n/req%3Aready-needs-commit");
  await expect(page.locator(rail)).toHaveCount(0);
  await page.goto("/map");
  await expect(page).toHaveURL(/\/$/);
  await expect(page.locator(rail)).toBeVisible();
  await page.goto("/nowhere");
  await expect(page.locator("header")).toBeVisible();
  await expect(page.locator("main *")).toHaveCount(0);
});

/**
 * # Test contract
 *
 * ```json
 * {"test": "the-application-is-all-the-page-holds", "checks": ["web/src/main#mounts-app"]}
 * ```
 */
test("the application is mounted into #root, and the page holds nothing else", async ({ page }) => {
  for (const { path, content } of ROUTES) {
    await page.goto(path);
    await expect(page.locator(`#root ${content}`).first(), path).toBeVisible();
    await expect(page.locator("#root header").first(), path).toBeVisible();
    // The page's own markup is the mount point and the script that fills it.
    await expect(page.locator("body > :not(script)"), path).toHaveCount(1);
    await expect(page.locator("body > #root"), path).toHaveCount(1);
  }
});

/**
 * # Test contract
 *
 * ```json
 * {"test": "one-tab-is-lit", "checks": ["web/src/App#lit-tab"]}
 * ```
 */
test("exactly one tab is lit, following the section a node was reached from", async ({ page }) => {
  const lit = () => page.locator('header [aria-current="page"]');
  await page.goto("/");
  await expect(lit()).toHaveCount(1);
  await expect(lit()).toContainText("Grimoire");
  await expect(page.locator("header nav a")).toHaveText(["Grimoire", "Board"]);
  await page.goto("/n/page%3Areadiness");
  await expect(lit()).toContainText("Grimoire");
  // Reached from the board, a ticket keeps Board lit.
  await page.goto("/board");
  await page.getByRole("link", { name: "Make readiness faster" }).click();
  await expect(lit()).toHaveText("Board");
  // Opened cold, a requirement lights Grimoire.
  await page.goto("/n/req%3Aready-needs-commit");
  await expect(lit()).toContainText("Grimoire");
});

/**
 * # Test contract
 *
 * ```json
 * {"test": "header-tells-new-from-inherited", "checks": ["web/src/App#header-findings"]}
 * ```
 */
test("the header shows new findings as a warning and inherited ones dimmed", async ({ page }) => {
  const lint = await tool<{ findings: unknown[]; new: number }>(page, "lint");
  const inherited = lint.findings.length - lint.new;
  expect(lint.new, "the fixture adds new work after adopting").toBeGreaterThan(0);
  expect(inherited).toBeGreaterThan(0);
  await page.goto("/board");
  const fresh = page.locator("header").getByText(`${lint.new} new`, { exact: true });
  await expect(fresh).toBeVisible();
  await expect(fresh).toHaveClass(/text-bad/);
  const old = page.locator("header").getByText(`${inherited} inherited`, { exact: true });
  await expect(old).toBeVisible();
  await expect(old).not.toHaveClass(/text-bad/);
});

/**
 * # Test contract
 *
 * ```json
 * {"test": "search-finds-and-enter-opens", "checks": ["web/src/App#search-is-find"]}
 * ```
 */
test("search offers find's results as links, and Enter opens the first", async ({ page }) => {
  await page.goto("/");
  const finding = page.waitForRequest((r) => r.url().endsWith("/api/find"));
  await page.getByPlaceholder("Find a page, requirement, module, ticket…").fill("readiness");
  await finding;
  const first = (await tool<{ nodes: { ref: string }[] }>(page, "find", { query: "readiness", limit: 12 })).nodes[0].ref;
  await expect(page.locator(`header a[href="/n/${encodeURIComponent(first)}"]`)).toBeVisible();
  await page.getByPlaceholder("Find a page, requirement, module, ticket…").press("Enter");
  await expect(page).toHaveURL(`/n/${encodeURIComponent(first)}`);
});

/**
 * # Test contract
 *
 * ```json
 * {"test": "search-drops-an-earlier-answer", "checks": ["web/src/App#search-is-find"]}
 * ```
 */
test("an answer to earlier text arriving last is dropped, and Enter opens the latest's first", async ({ page }) => {
  // find for "rea" is held until "readiness" has been answered, then answers with a node that exists nowhere.
  let release = () => {};
  const held = new Promise<void>((r) => (release = r));
  await page.route("**/api/find", async (route) => {
    if (route.request().postDataJSON().query !== "rea") return route.continue();
    await held;
    await route.fulfill({ json: { nodes: [{ ref: "page:stale", title: "Stale answer" }] } });
  });
  await page.goto("/");
  const search = page.getByPlaceholder("Find a page, requirement, module, ticket…");
  const early = page.waitForRequest((r) => r.url().endsWith("/api/find") && r.postDataJSON().query === "rea");
  await search.fill("rea");
  await early;
  const answered = page.waitForResponse((r) => r.url().endsWith("/api/find") && r.request().postDataJSON().query === "readiness");
  await search.fill("readiness");
  await answered;
  const first = (await tool<{ nodes: { ref: string }[] }>(page, "find", { query: "readiness", limit: 12 })).nodes[0].ref;
  await expect(page.locator(`header a[href="/n/${encodeURIComponent(first)}"]`)).toBeVisible();
  const late = page.waitForResponse((r) => r.url().endsWith("/api/find") && r.request().postDataJSON().query === "rea");
  release();
  await late;
  await expect(page.locator("header a[href='/n/page%3Astale']")).toHaveCount(0);
  await expect(page.locator(`header a[href="/n/${encodeURIComponent(first)}"]`)).toBeVisible();
  await search.press("Enter");
  await expect(page).toHaveURL(`/n/${encodeURIComponent(first)}`);
});

/**
 * # Test contract
 *
 * ```json
 * {"test": "search-offers-nothing-for-text-not-yet-answered", "checks": ["web/src/App#search-is-find"]}
 * ```
 */
test("while the latest text is unanswered, earlier results are not offered and Enter opens nothing", async ({ page }) => {
  // find for "goal" is held, so "readiness"'s answer is the only one there is.
  await page.route("**/api/find", async (route) => {
    if (route.request().postDataJSON().query !== "goal") return route.continue();
    await new Promise(() => {});
  });
  await page.goto("/board");
  const search = page.getByPlaceholder("Find a page, requirement, module, ticket…");
  await search.fill("readiness");
  const first = (await tool<{ nodes: { ref: string }[] }>(page, "find", { query: "readiness", limit: 12 })).nodes[0].ref;
  const hit = page.locator(`header a[href="/n/${encodeURIComponent(first)}"]`);
  await expect(hit).toBeVisible();
  const asked = page.waitForRequest((r) => r.url().endsWith("/api/find") && r.postDataJSON().query === "goal");
  await search.fill("goal");
  await expect(hit).toHaveCount(0);
  await search.press("Enter");
  await asked;
  await search.press("Enter");
  await expect(page).toHaveURL("/board");
});

/**
 * # Test contract
 *
 * ```json
 * {"test": "every-request-is-a-tool", "checks": ["web/src/api#tools-only"]}
 * ```
 */
test("every request the UI makes is a tool call or the event stream", async ({ page }) => {
  const tools = (await (await page.request.get("/api/tools")).json()) as { tools: { name: string }[] };
  const names = new Set(tools.tools.map((t) => t.name));
  const api: string[] = [];
  page.on("request", (r) => {
    const path = new URL(r.url()).pathname;
    if (path.startsWith("/api/")) api.push(`${r.method()} ${path}`);
  });
  for (const { path } of ROUTES) {
    await page.goto(path);
    await page.waitForLoadState("domcontentloaded");
    await page.waitForTimeout(400);
  }
  expect(api.length).toBeGreaterThan(5);
  for (const r of api) {
    const [method, path] = r.split(" ");
    const name = path.slice("/api/".length);
    expect(name === "events" ? method === "GET" : method === "POST" && names.has(name), r).toBe(true);
  }
});

/**
 * # Test contract
 *
 * ```json
 * {"test": "one-event-stream-per-page", "checks": ["web/src/api#one-stream"]}
 * ```
 */
test("a page holds one event stream however many queries it mounts", async ({ page }) => {
  let streams = 0;
  page.on("request", (r) => {
    if (new URL(r.url()).pathname === "/api/events") streams++;
  });
  // A module page mounts a dozen queries: the node, rests_on, rests_on_me, coverage, lint...
  await page.goto("/n/module%3Asrc%2Fready");
  await expect(page.locator("h1")).toBeVisible();
  await page.waitForTimeout(500);
  expect(streams).toBe(1);
});

/**
 * # Test contract
 *
 * ```json
 * {"test": "references-are-tagged-links", "checks": ["web/src/components#ref-is-a-link"]}
 * ```
 */
test("a reference is a link to its node, tagged with its kind", async ({ page }) => {
  await page.goto("/n/ticket%3A0001");
  const link = page.locator('main a[href="/n/module%3Asrc%2Fready"]').first();
  await expect(link).toBeVisible();
  await expect(link.locator("xpath=..")).toContainText("module");
  await link.click();
  await expect(page).toHaveURL("/n/module%3Asrc%2Fready");
  // The one exception: a closed ticket's reference to a node that no longer exists keeps its kind tag and is text, marked.
  await page.goto("/n/ticket%3A0004");
  const gone = page.locator("main [data-gone='req:ready-old-name']").first();
  await expect(gone).toHaveText("reqready-old-nameno longer exists");
  await expect(gone.locator("a")).toHaveCount(0);
  await expect(page.locator("main a[href='/n/req%3Aready-old-name']")).toHaveCount(0);
});

/**
 * # Test contract
 *
 * ```json
 * {"test": "bodies-are-markdown", "checks": ["web/src/components#markdown-bodies"]}
 * ```
 */
test("a page body is rendered as Markdown", async ({ page }) => {
  await page.goto("/n/page%3Agoal");
  await expect(page.locator("main strong", { hasText: "What it is." })).toBeVisible();
  await expect(page.locator("main li", { hasText: "A fixture for the UI's behaviour tests." })).toBeVisible();
  await expect(page.locator("main")).not.toContainText("**What it is.**");
});

/** A row on the board, found by its ticket reference. */
function row(page: Page, ref: string) {
  return page.locator(`li[data-ticket="${ref}"]`);
}

/** The ticket references in a band, top to bottom, as the board draws them. */
async function bandRefs(page: Page, band: string): Promise<string[]> {
  return page.locator(`#band-${band} li[data-ticket]`).evaluateAll((ls) => ls.map((l) => l.getAttribute("data-ticket") ?? ""));
}

type BoardAnswer = { tickets: { ref: string; priority: string; waits_on?: { ref: string; status: string }[] }[]; unattached: { ref: string }[]; closed: { ref: string }[] };

/**
 * # Test contract
 *
 * ```json
 * {"test": "board-shows-readiness-and-the-unattached", "checks": ["web/src/views/Board#first-screen"]}
 * ```
 */
test("the board lists changes with their readiness, then the unattached gaps, with filters", async ({ page }) => {
  await page.goto("/board");
  // The per-workspace report waits on the submodules question; Explain readiness waits on nothing.
  await expect(row(page, "ticket:0006")).toContainText("waits on 1");
  await expect(row(page, "ticket:0006")).toContainText("1 question");
  await expect(row(page, "ticket:0002")).toContainText("ready");
  // The gap no change relies on is unattached; the question a change relies on is not.
  await expect(page.locator("#unattached li[data-ticket='ticket:0003']")).toBeVisible();
  await expect(page.locator("#unattached li[data-ticket='ticket:0005']")).toHaveCount(0);
  // Closed tickets are hidden until asked for.
  await expect(page.getByRole("link", { name: "Name the readiness module" })).toHaveCount(0);
  await page.getByRole("button", { name: /^Ready/ }).click();
  await expect(row(page, "ticket:0002")).toHaveCount(0);
  await expect(row(page, "ticket:0006")).toBeVisible();
  await page.getByRole("button", { name: /^Ready/ }).click();
  await page.getByRole("button", { name: /^Closed/ }).click();
  await expect(page.locator("#closed").getByRole("link", { name: "Name the readiness module" })).toBeVisible();
  // An answered question no change relies on is marked; a gap a change was built on, and a change, are not.
  await expect(page.locator("#closed li[data-ticket='ticket:0008']")).toContainText("nothing built on it");
  // What the mark means is read out, not only shown on hover.
  await expect(page.locator("#closed li[data-ticket='ticket:0008']")).toContainText("it needed no work, or its work was never filed");
  await expect(page.locator("#closed li[data-ticket='ticket:0009']")).not.toContainText("nothing built on it");
  await expect(page.locator("#closed li[data-ticket='ticket:0010']")).not.toContainText("nothing built on it");
});

/**
 * # Test contract
 *
 * ```json
 * {"test": "a-board-row-opens-into-the-ticket", "checks": ["web/src/views/Board#ticket-in-place"]}
 * ```
 */
test("a board row opens into the whole ticket, with what it waits on and its actions", async ({ page }) => {
  await page.goto("/board");
  await page.getByRole("button", { name: "Show Report readiness per workspace" }).click();
  const opened = row(page, "ticket:0006");
  await expect(opened.getByText("Waits on")).toBeVisible();
  await expect(opened.getByRole("link", { name: "Should readiness count submodules?" })).toBeVisible();
  await expect(opened).toContainText("open");
  await expect(opened).toContainText("Needs refinement: it waits on whether submodules count.");
  await expect(opened.locator("a[href='/n/module%3Asrc%2Fready']").first()).toBeVisible();
  await expect(opened.getByRole("button", { name: "Start" })).toBeVisible();
  await expect(opened.getByRole("link", { name: "Open the ticket's page" })).toHaveAttribute("href", "/n/ticket%3A0006");
  // A gap opens the same way, and can be closed from there.
  await page.getByRole("button", { name: "Show Readiness ignores submodules" }).click();
  const gap = row(page, "ticket:0003");
  await expect(gap).toContainText("A gap, off the first screen.");
  await gap.getByLabel("Decision").fill("Submodules are out of scope for readiness.");
  await gap.getByRole("button", { name: "Close", exact: true }).click();
  await expect(page.locator("#unattached li[data-ticket='ticket:0003']")).toHaveCount(0);
  expect((await tool<{ node: { status: string } }>(page, "get", { ref: "ticket:0003" })).node.status).toBe("closed");
  // A closed ticket opens the same way: what it touched that still exists is a link, what no longer exists is marked and is none.
  await page.getByRole("button", { name: /^Closed/ }).click();
  await page.getByRole("button", { name: "Show Name the readiness module" }).click();
  const closed = page.locator("#closed li[data-ticket='ticket:0004']");
  await expect(closed.locator("a[href='/n/module%3Asrc%2Fready']").first()).toBeVisible();
  // One it touched and one it relied on, each listed under its own heading.
  for (const [ref, href] of [
    ["req:ready-old-name", "/n/req%3Aready-old-name"],
    ["page:old-naming", "/n/page%3Aold-naming"],
  ]) {
    await expect(closed.locator(`[data-gone='${ref}']`), ref).toContainText("no longer exists");
    await expect(closed.locator(`[data-gone='${ref}'] a`), ref).toHaveCount(0);
    await expect(closed.locator(`a[href='${href}']`), ref).toHaveCount(0);
  }
});

/**
 * # Test contract
 *
 * ```json
 * {"test": "board-keeps-the-tools-order", "checks": ["web/src/views/Board#order-from-tool"]}
 * ```
 */
test("the board keeps the tool's order, and a change is moved by dragging or from the keyboard", async ({ page }) => {
  await page.goto("/board");
  const answer = async () => tool<BoardAnswer>(page, "board");
  const inBand = async (band: string) => (await answer()).tickets.filter((x) => x.priority === band).map((x) => x.ref);
  await expect(row(page, "ticket:0007")).toBeVisible();
  expect(await bandRefs(page, "later")).toEqual(await inBand("later"));
  expect(await inBand("later")).toEqual(["ticket:0007", "ticket:0006"]);
  const grip = (ref: string) => row(page, ref).getByRole("button", { name: /^Move / });

  // Dropped on itself, top half or bottom half: nothing changes.
  await grip("ticket:0006").dragTo(row(page, "ticket:0006"), { targetPosition: { x: 40, y: 4 } });
  await grip("ticket:0006").dragTo(row(page, "ticket:0006"), { targetPosition: { x: 40, y: 30 } });
  await page.waitForTimeout(300);
  expect(await inBand("later")).toEqual(["ticket:0007", "ticket:0006"]);

  // Dragged within its band: the band's order is set.
  await grip("ticket:0006").dragTo(row(page, "ticket:0007"), { targetPosition: { x: 40, y: 4 } });
  await expect.poll(() => inBand("later")).toEqual(["ticket:0006", "ticket:0007"]);
  await expect.poll(() => bandRefs(page, "later")).toEqual(["ticket:0006", "ticket:0007"]);

  // From the keyboard: Alt+Down moves it one place, and says so.
  await grip("ticket:0006").focus();
  await page.keyboard.press("Alt+ArrowDown");
  await expect.poll(() => inBand("later")).toEqual(["ticket:0007", "ticket:0006"]);
  await expect(page.locator("[aria-live]")).toContainText("moved to position 2 of 2 in later");

  // Two quick presses move it two places, the second from where the first left it:
  // up within later, then past the edge into the end of high.
  await grip("ticket:0006").focus();
  await page.keyboard.press("Alt+ArrowUp");
  await page.keyboard.press("Alt+ArrowUp");
  await expect.poll(async () => (await answer()).tickets.find((x) => x.ref === "ticket:0006")?.priority).toBe("high");
  await expect.poll(() => inBand("high")).toEqual(["ticket:0002", "ticket:0006"]);
  expect(await inBand("later")).toEqual(["ticket:0007"]);
  // Back down past the edge: the start of later.
  await grip("ticket:0006").focus();
  await page.keyboard.press("Alt+ArrowDown");
  await expect.poll(() => inBand("later")).toEqual(["ticket:0006", "ticket:0007"]);

  // Dropped in another band: its priority changes and it lands where it was dropped.
  await grip("ticket:0007").dragTo(row(page, "ticket:0002"), { targetPosition: { x: 40, y: 4 } });
  await expect.poll(async () => (await answer()).tickets.find((x) => x.ref === "ticket:0007")?.priority).toBe("high");
  await expect.poll(() => inBand("high")).toEqual(["ticket:0007", "ticket:0002"]);
  await expect.poll(() => bandRefs(page, "high")).toEqual(["ticket:0007", "ticket:0002"]);
  expect(await inBand("later")).toEqual(["ticket:0006"]);
});

/**
 * # Test contract
 *
 * ```json
 * {"test": "summary-reads-in-the-roots-order", "checks": ["web/src/views/Home#story-order"]}
 * ```
 */
test("the root shows the grimoire's own level: its body, its bars and its pages as cards in link order", async ({ page }) => {
  await page.goto("/");
  await expect(page.locator("#pages")).toBeVisible();
  const root = await tool<{ node: { links: string[] } }>(page, "get", { ref: "page:overview" });
  const cards = await linkedRefs(page, "#pages");
  expect(cards).toEqual(root.node.links.filter((l) => l.startsWith("page:")));
  // No story page is opened out at the root: none of its requirements or modules is listed.
  expect((await linkedRefs(page)).some((r) => r.startsWith("req:") || r.startsWith("module:"))).toBe(false);
  // Each card carries its page's rollup, and the whole grimoire's rollup heads the page.
  const outline = await tool<Outline>(page, "outline", { depth: 64 });
  const architecture = find(outline.roots, "page:architecture")!.rollup;
  const card = page.locator("#pages a[href='/n/page%3Aarchitecture']");
  await expect(card).toContainText(`${architecture.contracted} of ${architecture.modules}`);
  await expect(card).toContainText(`${architecture.tested} of ${architecture.criteria}`);
  const whole = outline.roots[0].rollup;
  await expect(page.getByRole("meter", { name: "Requirements held" }).first()).toHaveAttribute("aria-valuemax", String(whole.requirements));
  // With a second root, the root is still the first root's level: the second is no card and none of its pages shows.
  expect(outline.roots.map((t) => t.ref)).toEqual(["page:overview", "page:handbook"]);
  await expect(page.locator("main h1").first()).toHaveText(outline.roots[0].title);
  expect(cards).not.toContain("page:handbook");
  await expect(page.locator(`main a[href^="/n/page%3A"]:not(${rail} a)`)).toHaveCount(cards.length);
});

/**
 * # Test contract
 *
 * ```json
 * {"test": "contracts-listed-once-by-area", "checks": ["web/src/views/Home#contracts-grouped"]}
 * ```
 */
test("a page lists each module it links once, with its criteria beneath it", async ({ page }) => {
  await page.goto("/n/page%3Areadiness");
  const modules = page.locator("#modules");
  await expect(modules).toBeVisible();
  const refs = await linkedRefs(page, "#modules");
  expect(refs.filter((r) => r.startsWith("module:")).sort()).toEqual(["module:src", "module:src/ready"]);
  expect(refs.filter((r) => r.startsWith("criterion:src/ready#")).sort()).toEqual(["criterion:src/ready#needs-commit", "criterion:src/ready#reads-only"]);
  expect((await linkedRefs(page)).some((r) => r.startsWith("file:") || r.startsWith("decl:")), "no files or declarations").toBe(false);
  await expect(page.locator("main")).toContainText("Whether a workspace is ready.");
  // A long body shows five lines and a Show more; a short one shows whole.
  const more = page.getByRole("button", { name: "Show more" });
  await expect(page.locator("[data-folded]")).toHaveCount(1);
  await more.click();
  await expect(page.locator("[data-folded]")).toHaveCount(0);
  await expect(page.getByRole("button", { name: "Show less" })).toBeVisible();
  await page.goto("/n/page%3Agoal");
  await expect(page.locator("main strong", { hasText: "What it is." })).toBeVisible();
  await expect(page.getByRole("button", { name: "Show more" })).toHaveCount(0);
});

/**
 * # Test contract
 *
 * ```json
 * {"test": "rail-is-the-outline", "checks": ["web/src/views/Home#outline-driven"]}
 * ```
 */
test("the rail is the outline's pages, ending with the unplaced modules", async ({ page }) => {
  const asked: object[] = [];
  page.on("request", (r) => {
    if (r.url().endsWith("/api/outline")) asked.push(r.postDataJSON());
  });
  await page.goto("/");
  await expect(page.locator(rail)).toBeVisible();
  const outline = await tool<Outline>(page, "outline", { depth: 64 });
  // The rail reads pages only; the root beside it, its own outline at depth 2.
  await expect(page.locator("main h1").first()).toHaveText(outline.roots[0].title);
  expect(asked).toContainEqual({ depth: 64, pages_only: true });
  expect(asked).toContainEqual({ root: outline.roots[0].ref, depth: 2 });
  expect(asked).not.toContainEqual({ depth: 64 });
  // The root links to /, every other page to its node page, in the outline's order.
  const expected = pagesOf(outline.roots).slice(1).map((t) => t.ref);
  expect(await linkedRefs(page, rail)).toEqual(expected);
  // Nested as the tree: each page's entry sits as deep in the rail's lists as the page in the outline.
  const depths = (trees: Tree[], d = 0): string[] => trees.filter((t) => t.ref.startsWith("page:")).flatMap((t) => [`${d} ${t.ref}`, ...depths(t.children, d + 1)]);
  const nested = await page.locator(`${rail} li > a`).evaluateAll((as) =>
    as.map((a) => {
      let d = -1;
      for (let e: Element | null = a; e && e.tagName !== "NAV"; e = e.parentElement) if (e.tagName === "LI") d++;
      const href = a.getAttribute("href") ?? "";
      return `${d} ${href === "/" ? "root" : decodeURIComponent(href.slice("/n/".length))}`;
    }),
  );
  expect(nested).toEqual(depths(outline.roots).map((s, i) => (i === 0 ? s.replace(outline.roots[0].ref, "root") : s)));
  expect(nested.some((s) => s.startsWith("2 "))).toBe(true);
  // Every root at the top level, the first linking to / and each later one to its own page.
  expect(nested.filter((s) => s.startsWith("0 "))).toEqual(["0 root", "0 page:handbook"]);
  expect(nested).toContain("1 page:runbook");
  expect(outline.unplaced.map((t) => t.ref)).toEqual(["module:src/loose"]);
  const unplaced = page.locator(rail).getByRole("link", { name: /Unplaced/ });
  await expect(unplaced).toContainText(String(outline.unplaced.length));
  await unplaced.click();
  await expect(page).toHaveURL(/\/unplaced$/);
  await expect(page.locator("main a[href='/n/module%3Asrc%2Floose']")).toBeVisible();
  await expect(page.locator("main")).toContainText("no contract");
});

/**
 * # Test contract
 *
 * ```json
 * {"test": "a-page-under-a-later-root-opens-from-it", "checks": ["web/src/views/Home#outline-driven"]}
 * ```
 */
test("a page under a later root opens beside the rail, its breadcrumb from that root", async ({ page }) => {
  await page.goto("/n/page%3Arunbook");
  await expect(page.locator("main h1").first()).toHaveText("Runbook");
  const crumbs = page.getByRole("navigation", { name: "Breadcrumb" });
  await expect(crumbs).toHaveText("Handbook /");
  await expect(crumbs.getByRole("link", { name: "Handbook" })).toHaveAttribute("href", "/n/page%3Ahandbook");
  await expect(page.locator(rail).locator("a[aria-current='page']")).toHaveAttribute("href", "/n/page%3Arunbook");
  // The later root is its own page beside the rail, with no breadcrumb.
  await crumbs.getByRole("link", { name: "Handbook" }).click();
  await expect(page.locator("main h1").first()).toHaveText("Handbook");
  await expect(page.getByRole("navigation", { name: "Breadcrumb" })).toHaveCount(0);
  await expect(page.locator("#pages a[href='/n/page%3Arunbook']")).toBeVisible();
});

/**
 * # Test contract
 *
 * ```json
 * {"test": "a-root-or-a-page-reached-twice-is-one-entry", "checks": ["web/src/views/Home#outline-driven"]}
 * ```
 */
test("a root linked from another root opens as a root, and a page reached twice marks one entry", async ({ page }) => {
  // The rail's answer, rewritten so overview also links handbook and handbook also links goal.
  await page.route("**/api/outline", async (route) => {
    if (!route.request().postDataJSON().pages_only) return route.continue();
    const answer = (await (await route.fetch()).json()) as Outline;
    const [overview, handbook] = answer.roots;
    const goal = overview.children.find((t) => t.ref === "page:goal")!;
    handbook.children.push(goal);
    overview.children.push(handbook);
    await route.fulfill({ json: answer });
  });
  const current = page.locator(`${rail} a[aria-current='page']`);
  await page.goto("/n/page%3Ahandbook");
  await expect(page.locator("main h1").first()).toHaveText("Handbook");
  await expect(page.getByRole("navigation", { name: "Breadcrumb" })).toHaveCount(0);
  await expect(page.locator(`${rail} a[href='/n/page%3Ahandbook']`)).toHaveCount(2);
  await expect(current).toHaveCount(1);
  // The current entry is the top-level root, not the one nested under overview.
  expect(await current.evaluate((a) => a.closest("ul")?.parentElement?.tagName)).toBe("NAV");
  await page.goto("/n/page%3Agoal");
  await expect(page.getByRole("navigation", { name: "Breadcrumb" })).toHaveText("Fixture /");
  await expect(page.locator(`${rail} a[href='/n/page%3Agoal']`)).toHaveCount(3);
  await expect(current).toHaveCount(1);
  await expect(current).toHaveAttribute("href", "/n/page%3Agoal");
  // The one under the first root, which the breadcrumb names: a direct child of the root's list.
  expect(await current.evaluate((a) => a.closest("ul")?.closest("li")?.querySelector("a")?.getAttribute("href"))).toBe("/");
});

/**
 * # Test contract
 *
 * ```json
 * {"test": "a-page-pane-never-shows-another-page", "checks": ["web/src/views/Home#outline-driven"]}
 * ```
 */
test("going from one page to another never shows the first under the second", async ({ page }) => {
  // The outline rooted at architecture is held, so the pane can only be the old page's or loading.
  await page.route("**/api/outline", async (route) => {
    if (route.request().postDataJSON().root !== "page:architecture") return route.continue();
    await new Promise(() => {});
  });
  await page.goto("/n/page%3Agoal");
  const heading = page.locator("main h1").first();
  await expect(heading).toHaveText("Goal");
  const asked = page.waitForRequest((r) => r.url().endsWith("/api/outline") && r.postDataJSON().root === "page:architecture");
  await page.locator(rail).getByRole("link", { name: "Architecture" }).click();
  await asked;
  await expect(page).toHaveURL("/n/page%3Aarchitecture");
  await expect(page.locator("main")).toContainText("Loading…");
  await expect(page.locator("main h1", { hasText: "Goal" })).toHaveCount(0);
});

/**
 * # Test contract
 *
 * ```json
 * {"test": "marks-new-inherited-and-untested", "checks": ["web/src/views/Home#marks"]}
 * ```
 */
test("pages mark new findings apart from inherited ones, and criteria and requirements by what holds them", async ({ page }) => {
  await page.goto("/n/page%3Areadiness");
  const outline = await tool<Outline>(page, "outline", { depth: 64 });
  const area = find(outline.roots, "page:readiness")!;
  expect(area.baselined, "the fixture's adopted findings").toBeGreaterThan(0);
  expect(area.findings - area.baselined, "the requirement added after adopting").toBe(1);
  const inRail = page.locator(rail).locator("a[href='/n/page%3Areadiness']");
  await expect(inRail).toContainText("1 new");
  await expect(inRail).toContainText(String(area.baselined));
  const chip = (id: string) => page.locator(`#modules a[href='/n/criterion%3Asrc%2Fready%23${id}']`);
  await expect(chip("reads-only")).toContainText("untested");
  await expect(chip("reads-only")).toContainText("inherited");
  await expect(chip("needs-commit")).not.toContainText("untested");
  const req = (slug: string) => page.locator("main li", { has: page.locator(`a[href='/n/req%3A${slug}']`) });
  await expect(req("ready-needs-commit")).toContainText("1 criterion");
  await expect(req("not-built")).toContainText("uncovered");
  // A constraint names no criterion; a test holds it, so it is not a warning.
  await page.goto("/n/page%3Aconstraints");
  await expect(req("ready-writes-nothing")).toContainText("held by 1 test");
  await expect(req("ready-writes-nothing")).not.toContainText("uncovered");
});

/**
 * # Test contract
 *
 * ```json
 * {"test": "every-kind-has-a-page", "checks": ["web/src/views/Node#every-kind"]}
 * ```
 */
test("every node kind has a page", async ({ page }) => {
  const nodes = ROUTES.filter((r) => r.path.startsWith("/n/"));
  const kinds = [...new Set(nodes.map((r) => decodeURIComponent(r.path.slice("/n/".length)).split(":")[0]))].sort();
  expect(kinds, "a node page of every kind").toEqual(["criterion", "decl", "file", "module", "page", "req", "test", "ticket"]);
  for (const { path } of nodes) {
    await page.goto(path);
    await expect(page.locator("h1"), path).toBeVisible();
    await expect(page.locator("main"), path).not.toContainText("does not exist");
  }
  await expect(page.locator("main")).toContainText("Checks");
});

/**
 * # Test contract
 *
 * ```json
 * {"test": "tickets-start-and-close-from-their-page", "checks": ["web/src/views/Node#ticket-actions"]}
 * ```
 */
test("an open ticket is started and closed from its page; a closed one offers neither", async ({ page }) => {
  await page.goto("/n/ticket%3A0002");
  await page.getByRole("button", { name: "Start" }).click();
  await expect.poll(async () => (await tool<{ node: { status: string } }>(page, "get", { ref: "ticket:0002" })).node.status).toBe("active");
  await expect(page.getByRole("button", { name: "Start" })).toHaveCount(0);
  await page.getByPlaceholder("Decision: what was chosen and why. Closing discards the body.").fill("Explained in the area page.");
  await page.getByRole("button", { name: "Close" }).click();
  await expect.poll(async () => (await tool<{ node: { status: string } }>(page, "get", { ref: "ticket:0002" })).node.status).toBe("closed");
  await page.goto("/n/ticket%3A0004");
  await expect(page.getByRole("heading", { name: "Name the readiness module" })).toBeVisible();
  await expect(page.getByRole("button", { name: "Start" })).toHaveCount(0);
  await expect(page.getByRole("button", { name: "Close" })).toHaveCount(0);
});

/**
 * # Test contract
 *
 * ```json
 * {"test": "a-closed-tickets-gone-reference-is-marked", "checks": ["web/src/views/Node#stale-reference-marked", "web/src/views/TicketGraph#on-ticket-page"]}
 * ```
 */
test("a closed ticket's reference that leads nowhere is marked and not a link; an open ticket's stays a link", async ({ page }) => {
  // The fixture's closed ticket touches a module that exists and a requirement that does not.
  const closed = await tool<{ node: { status: string; touches: string[]; out: { ref: string; title: string | null }[] } }>(page, "get", { ref: "ticket:0004" });
  expect(closed.node.status).toBe("closed");
  expect(closed.node.out.find((e) => e.ref === "req:ready-old-name")?.title, "get gives the missing end no title").toBeNull();
  await page.goto("/n/ticket%3A0004");
  await expect(page.getByRole("heading", { name: "Name the readiness module" })).toBeVisible();
  // Under Touches, Rests on and Links out: text with the mark, never a link.
  const gone = page.locator("main [data-gone='req:ready-old-name']:not([data-graph-node])");
  await expect(gone).toHaveCount(3);
  for (const mark of await gone.all()) {
    await expect(mark).toContainText("ready-old-name");
    await expect(mark).toContainText("no longer exists");
    await expect(mark.locator("a")).toHaveCount(0);
  }
  // In the graph: a box that says so and is no link, beside the module's, which is one.
  const graph = page.getByRole("region", { name: "What this ticket rests on" });
  const box = graph.locator("[data-graph-node='req:ready-old-name']");
  await expect(box).toHaveText("reqready-old-name");
  await expect(box).toHaveAttribute("aria-label", "req ready-old-name, no longer exists, under Name the readiness module");
  expect(await box.evaluate((e) => e.tagName)).toBe("SPAN");
  expect(await graph.locator("[data-graph-node='module:src/ready']").evaluate((e) => e.tagName)).toBe("A");
  // Under Relies on too: the page it relied on is gone, marked there, in Rests on and in Links out, and drawn so.
  const relied = page.locator("main [data-gone='page:old-naming']:not([data-graph-node])");
  await expect(relied).toHaveCount(3);
  await expect(page.locator("main h2", { hasText: "Relies on" }).locator("xpath=following-sibling::ul[1]").locator("[data-gone='page:old-naming']")).toContainText("no longer exists");
  await expect(page.locator("main h2", { hasText: "Touches" }).locator("xpath=following-sibling::ul[1]").locator("[data-gone='req:ready-old-name']")).toContainText("no longer exists");
  expect(await graph.locator("[data-graph-node='page:old-naming']").evaluate((e) => e.tagName)).toBe("SPAN");
  const refs = await linkedRefs(page);
  expect(refs).not.toContain("req:ready-old-name");
  expect(refs).not.toContain("page:old-naming");
  expect(refs, "what still exists is a link as before").toContain("module:src/ready");
  // The same answer for a ticket still open: the check reports that one, and the page links it as before.
  await page.route("**/api/get", async (route) => {
    if (route.request().postDataJSON().ref !== "ticket:0004") return route.continue();
    await route.fulfill({ json: { ...closed, node: { ...closed.node, status: "open", decision: null } } });
  });
  await page.reload();
  await expect(page.getByRole("heading", { name: "Name the readiness module" })).toBeVisible();
  await expect(page.locator("main [data-gone]")).toHaveCount(0);
  expect((await linkedRefs(page)).filter((r) => r === "req:ready-old-name"), "the graph, Touches, Rests on and Links out").toHaveLength(4);
  expect((await linkedRefs(page)).filter((r) => r === "page:old-naming"), "the graph, Relies on, Rests on and Links out").toHaveLength(4);
});

/**
 * # Test contract
 *
 * ```json
 * {"test": "a-hostile-body-renders-inert", "checks": ["web/src/components#markdown-inert"]}
 * ```
 */
test("a body with script in it renders as Markdown and runs nothing", async ({ page }) => {
  // Filed last: a new gap would otherwise join the unattached list other specs read.
  const body = [
    "**Still bold.**",
    "",
    '<img src="x" onerror="window.__pwned = true">',
    "",
    "<script>window.__pwned = true</script>",
    "",
    "[a link](javascript:window.__pwned=true)",
    "",
    '<a href="https://example.com/ok">a web link</a>',
    "",
    '<p style="position:fixed;inset:0">covers the page</p>',
    "",
    '<svg onload="window.__pwned = true"><circle r="4"></circle></svg>',
    "",
    '<iframe src="https://example.com"></iframe>',
    "",
    '<form action="https://example.com"><button>go</button></form>',
    "",
    '<img alt="data" src="data:image/png;base64,iVBORw0KGgo=">',
    "",
    '<img alt="set" src="https://example.com/a.png" srcset="https://example.com/a.png 1x, javascript:alert(1) 2x">',
  ].join("\n");
  const filed = await tool<{ node: { ref: string } }>(page, "file_ticket", { title: "A hostile body", kind: "gap", priority: "later", touches: [], body });
  await page.goto(`/n/${encodeURIComponent(filed.node.ref)}`);
  const md = page.locator("main .md").first();
  await expect(md.locator("strong", { hasText: "Still bold." })).toBeVisible();
  await page.waitForTimeout(300);
  expect(await page.evaluate(() => (window as unknown as { __pwned?: boolean }).__pwned)).toBeUndefined();
  await expect(md.locator("script")).toHaveCount(0);
  await expect(md.locator("[onerror]")).toHaveCount(0);
  expect(await md.locator("a").evaluateAll((as) => as.map((a) => a.getAttribute("href") ?? ""))).not.toContainEqual(expect.stringMatching(/^javascript:/i));
  await expect(md.locator('a[href="https://example.com/ok"]')).toBeVisible();
  for (const forbidden of ["[style]", "svg", "iframe", "form", "[srcset]", 'img[src^="data:"]']) {
    await expect(md.locator(forbidden), forbidden).toHaveCount(0);
  }
  await expect(md.getByText("covers the page")).toBeVisible();
});

/**
 * # Test contract
 *
 * ```json
 * {"test": "a-real-drag-into-an-empty-band", "checks": ["web/src/views/Board#order-from-tool"]}
 * ```
 */
test("a change dragged with the mouse into an empty band lands there and shows at once", async ({ page }) => {
  // A real pointer drag, not dispatched events: a browser abandons a drag whose
  // layout changes inside dragstart, and an empty band appears when one begins.
  await page.goto("/board");
  const row = (ref: string) => page.locator(`li[data-ticket="${ref}"]`);
  const answer = async () => tool<BoardAnswer>(page, "board");
  // Every priority as it was, put back afterwards so later specs see the fixture unchanged.
  const was = (await answer()).tickets.map((x) => ({ ref: x.ref, priority: x.priority }));
  try {
    for (const t of was.filter((x) => x.priority === "next")) {
      await tool(page, "update_ticket", { ref: t.ref, priority: "high" });
    }
    await expect(page.locator("#band-next")).toHaveCount(0);
    const moving = (await answer()).tickets.find((t) => t.priority === "later")!.ref;
    const from = (await row(moving).getByRole("button", { name: /^Move / }).boundingBox())!;
    await page.mouse.move(from.x + 5, from.y + 5);
    await page.mouse.down();
    await page.mouse.move(from.x + 10, from.y + 10, { steps: 3 });
    const zone = page.locator("#band-next li", { hasText: "Drop here" });
    await expect(zone).toBeVisible();
    const to = (await zone.boundingBox())!;
    await page.mouse.move(to.x + 20, to.y + 10, { steps: 10 });
    await page.mouse.up();
    await expect.poll(async () => (await answer()).tickets.find((t) => t.ref === moving)?.priority).toBe("next");
    await expect(page.locator(`#band-next li[data-ticket="${moving}"]`)).toBeVisible();
  } finally {
    const now = (await answer()).tickets;
    for (const w of was) {
      if (now.find((x) => x.ref === w.ref)?.priority !== w.priority) await tool(page, "update_ticket", { ref: w.ref, priority: w.priority });
    }
  }
});

/**
 * # Test contract
 *
 * ```json
 * {"test": "drill-down-ends-at-the-annotated-source", "checks": ["web/src/views/Node#annotated-source"]}
 * ```
 */
test("drilling down from a declaration ends at its line in the annotated source", async ({ page }) => {
  // From the declaration's page to its line in the file.
  await page.goto("/n/decl%3Asrc%2Fready.rs%23ready");
  await page.getByRole("link", { name: "src/ready.rs:7" }).click();
  await expect(page).toHaveURL(/\/n\/file%3Asrc%2Fready\.rs#L7$/);
  const focused = page.locator("tr[aria-current='location']");
  await expect(focused).toHaveAttribute("id", "L7");
  await expect(focused).toContainText("pub fn ready() -> bool");
  // Highlighted as text: keywords and the function name toned, the contract a comment on every line.
  await expect(focused.locator("[data-tone='keyword']").first()).toHaveText("pub");
  await expect(focused.locator("[data-tone='function']")).toHaveText("ready");
  for (const line of [1, 3, 4, 5]) await expect(page.locator(`tr#L${line} [data-tone='comment']`)).not.toHaveCount(0);
  await expect(page.locator("tr#L11 [data-tone='meta']")).toContainText("cfg");
  // The contract's JSON is highlighted as JSON inside its comment.
  await expect(page.locator("tr#L4 [data-tone='meta']").first()).toHaveText('"purpose"');
  await expect(page.locator("tr#L4 [data-tone='comment']").first()).toHaveText("//! ");
  // The file's lines, each annotated line marked and linked back into the grimoire.
  const source = page.getByRole("region", { name: "Source of src/ready.rs" });
  // The contract's JSON laid out over several rows under its one line number, and nothing scrolls sideways: long lines wrap.
  expect(await page.locator("tr#L4 td:last-child > div").count()).toBeGreaterThan(3);
  await expect(page.locator("tr#L5 td:first-child")).toHaveText("5");
  expect(await source.evaluate((el) => el.scrollWidth <= el.clientWidth)).toBe(true);
  const contract = source.locator("[data-annotation='contract']");
  await expect(contract.locator("a[href='/n/module%3Asrc%2Fready']")).toBeVisible();
  await expect(contract.locator("a[href='/n/criterion%3Asrc%2Fready%23needs-commit']")).toBeVisible();
  await expect(contract.locator("a[href='/n/criterion%3Asrc%2Fready%23reads-only']")).toBeVisible();
  const tests = source.locator("[data-annotation='test']");
  await expect(tests).toHaveCount(2);
  await expect(tests.locator("a[href='/n/criterion%3Asrc%2Fready%23needs-commit']")).toBeVisible();
  await expect(tests.locator("a[href='/n/req%3Aready-writes-nothing']")).toBeVisible();
  await expect(source.locator("[data-annotation='decl']").first()).toContainText("ready");
  // Nothing on it edits: no form, no text box, no editor link.
  await expect(source.locator("textarea, input, [contenteditable='true'], a[href^='vscode:']")).toHaveCount(0);
  // Clicking a line number marks that line, and Back and Forward follow.
  const marked = () => page.locator("tr[aria-current='location']");
  await page.getByRole("link", { name: "Line 12", exact: true }).click();
  await expect(page).toHaveURL(/#L12$/);
  await expect(marked()).toHaveCount(1);
  await expect(marked()).toHaveAttribute("id", "L12");
  await page.goBack();
  await expect(page).toHaveURL(/#L7$/);
  await expect(marked()).toHaveAttribute("id", "L7");
  await page.goForward();
  await expect(marked()).toHaveAttribute("id", "L12");
  // An annotated line is described by its marks.
  await expect(source.locator("tr#L1")).toHaveAttribute("aria-describedby", "marks-L1");
  // The criterion, module and test pages each link to their line.
  await page.goto("/n/criterion%3Asrc%2Fready%23needs-commit");
  await expect(page.locator("a[href='/n/file%3Asrc%2Fready.rs#L1']")).toBeVisible();
  await page.goto("/n/module%3Asrc%2Fready");
  await expect(page.locator("a[href='/n/file%3Asrc%2Fready.rs#L1']")).toBeVisible();
  await page.goto("/n/test%3Asrc%2Fready.rs%23ready-needs-a-commit");
  await expect(page.locator("a[href='/n/file%3Asrc%2Fready.rs#L13']")).toBeVisible();
});

/**
 * # Test contract
 *
 * ```json
 * {"test": "a-ticket-page-draws-its-graph", "checks": ["web/src/views/TicketGraph#on-ticket-page"]}
 * ```
 */
test("a ticket's page draws what it rests on, and a node in the graph opens its page", async ({ page }) => {
  await page.goto("/n/ticket%3A0006");
  const graph = page.getByRole("region", { name: "What this ticket rests on" });
  // The ticket at the left, what it relies on and touches beside it, the module's criteria a column further.
  await expect(graph.locator("[data-graph-node='ticket:0006']")).toBeVisible();
  await expect(graph.locator("[data-graph-node='ticket:0005']")).toContainText("Should readiness count submodules?");
  await expect(graph.locator("[data-graph-node='module:src/ready']")).toBeVisible();
  const [ticket, module] = await Promise.all([graph.locator("[data-graph-node='ticket:0006']").boundingBox(), graph.locator("[data-graph-node='module:src/ready']").boundingBox()]);
  expect(module!.x).toBeGreaterThan(ticket!.x);
  // A small walk is drawn whole: nothing to unfold.
  await expect(graph.getByRole("button", { name: /^Show the / })).toHaveCount(0);
  // Each node says what it hangs under, so the lines are read out too.
  await expect(graph.getByRole("link", { name: "ticket Should readiness count submodules?, under Report readiness per workspace" })).toBeVisible();
  await graph.locator("[data-graph-node='ticket:0005']").click();
  await expect(page).toHaveURL(/\/n\/ticket%3A0005$/);
  await expect(page.getByRole("heading", { level: 1 })).toHaveText("Should readiness count submodules?");
});

/**
 * # Test contract
 *
 * ```json
 * {"test": "a-large-graph-folds-and-unfolds", "checks": ["web/src/views/TicketGraph#on-ticket-page"]}
 * ```
 */
test("a ticket resting on many nodes folds them into counts that unfold, and folds them again", async ({ page }) => {
  // The fixture is small, so the walk the graph asks for is a synthetic one of 46 nodes: a page with 44 requirements and a module.
  const via = ["ticket:0006", "page:readiness"];
  const nodes = [
    { ref: "page:readiness", title: "Readiness", via: ["ticket:0006"], decisions: [] },
    ...Array.from({ length: 44 }, (_, i) => ({ ref: `req:r${i}`, title: `Requirement ${i}`, via, decisions: [] })),
    { ref: "module:src/ready", title: "Decide readiness", via, decisions: [] },
  ];
  await page.route("**/api/rests_on", async (route) => {
    const input = route.request().postDataJSON();
    if (input.depth !== undefined) return route.continue();
    await route.fulfill({ json: { nodes } });
  });
  await page.goto("/n/ticket%3A0006");
  const graph = page.getByRole("region", { name: "What this ticket rests on" });
  await expect(graph).toContainText("46 nodes it rests on, deeper branches folded");
  await expect(graph.locator("[data-graph-node='page:readiness']")).toBeVisible();
  await expect(graph.locator("[data-graph-node^='req:']")).toHaveCount(0);
  await expect(graph.getByRole("button", { name: "Fold all" })).toHaveCount(0);
  await graph.getByRole("button", { name: "Show the 44 requirements under Readiness" }).click();
  await expect(graph.locator("[data-graph-node^='req:']")).toHaveCount(44);
  await expect(graph.getByRole("button", { name: "Show the 1 module under Readiness" })).toBeVisible();
  await graph.getByRole("button", { name: "Fold all" }).click();
  await expect(graph.locator("[data-graph-node^='req:']")).toHaveCount(0);
  await expect(graph.getByRole("button", { name: "Fold all" })).toHaveCount(0);
});

/**
 * # Test contract
 *
 * ```json
 * {"test": "rests-on-folds-and-cuts-nothing", "checks": ["web/src/views/Node#rests-on"]}
 * ```
 */
test("the Rests on panel shows every node with every decision, folding past twenty and earlier decisions", async ({ page }) => {
  // A synthetic answer of 46 nodes; the last a requirement with two decisions in force, newest first.
  const nodes = [
    ...Array.from({ length: 45 }, (_, i) => ({ ref: `req:r${i}`, title: `Requirement ${i}`, via: [], decisions: [] })),
    {
      ref: "req:last",
      title: "The last requirement",
      via: [],
      decisions: [
        { date: "2026-09-30", text: "The newer decision." },
        { date: "2026-09-15", text: "The middle decision." },
        { date: "2026-09-01", text: "The older decision." },
      ],
    },
  ];
  await page.route("**/api/rests_on", async (route) => {
    if (route.request().postDataJSON().depth !== 3) return route.continue();
    await route.fulfill({ json: { nodes } });
  });
  await page.goto("/n/req%3Aready-needs-commit");
  const panel = page.getByRole("list", { name: "Rests on" });
  await expect(panel.locator(":scope > li")).toHaveCount(20);
  // The first twenty, in the tool's order.
  expect(await panel.locator(":scope > li").evaluateAll((ls) => ls.map((l) => l.querySelector("a")?.textContent?.trim()))).toEqual(Array.from({ length: 20 }, (_, i) => expect.stringContaining(`Requirement ${i}`)));
  const all = page.getByRole("button", { name: "Show all 46" });
  await all.click();
  await expect(panel.locator(":scope > li")).toHaveCount(46);
  const last = panel.locator(":scope > li").last();
  await expect(last).toContainText("The last requirement");
  await expect(last).toContainText("The newer decision.");
  await expect(last).not.toContainText("The middle decision.");
  await expect(last).not.toContainText("The older decision.");
  await last.getByRole("button", { name: "2 earlier decisions" }).click();
  const text = await last.innerText();
  expect(text.indexOf("The newer decision.")).toBeGreaterThan(-1);
  expect(text.indexOf("The newer decision.")).toBeLessThan(text.indexOf("The middle decision."));
  expect(text.indexOf("The middle decision.")).toBeLessThan(text.indexOf("The older decision."));
  await page.getByRole("button", { name: "Show fewer" }).click();
  await expect(panel.locator(":scope > li")).toHaveCount(20);
});

/**
 * # Test contract
 *
 * ```json
 * {"test": "a-ticket-resting-on-nothing-draws-no-graph", "checks": ["web/src/views/TicketGraph#on-ticket-page"]}
 * ```
 */
test("a ticket that rests on nothing draws no graph", async ({ page }) => {
  await page.goto("/n/ticket%3A0008");
  await expect(page.getByRole("heading", { level: 1 })).toHaveText("Should readiness follow symlinks?");
  await expect(page.getByText("Nothing below this node.")).toBeVisible();
  await expect(page.getByRole("region", { name: "What this ticket rests on" })).toHaveCount(0);
});

/**
 * # Test contract
 *
 * ```json
 * {"test": "a-declaration-links-to-its-own-file", "checks": ["web/src/types#reference-helpers"]}
 * ```
 */
test("a declaration's page links to its own file, though the file's name and the declaration's hold #", async ({ page }) => {
  // The fixture has no such file, so the declaration is answered for it.
  const ref = "decl:src/a#b.ts#Box::#secret";
  await page.route("**/api/get", async (route) => {
    if (route.request().postDataJSON().ref !== ref) return route.continue();
    await route.fulfill({ json: { node: { ref, path: "Box::#secret", kind: "method", line: 9, file: "src/a#b.ts", in: [], out: [] } } });
  });
  await page.goto(`/n/${encodeURIComponent(ref)}`);
  const link = page.getByRole("link", { name: "src/a#b.ts:9" });
  await expect(link).toBeVisible();
  await expect(link).toHaveAttribute("href", `/n/${encodeURIComponent("file:src/a#b.ts")}#L9`);
});

/**
 * # Test contract
 *
 * ```json
 * {"test": "a-requirement-shows-decisions-in-force", "checks": ["web/src/views/Node#decisions-in-force"]}
 * ```
 */
test("a requirement's page shows its decisions in force and folds the superseded ones; a retired one says so", async ({ page }) => {
  // The fixture has no superseded decision, so the requirements are answered for it.
  const req = (slug: string, retired: boolean) => ({
    ref: `req:${slug}`,
    slug,
    title: slug,
    statement: "It SHALL hold.",
    scenarios: [],
    covered_by: [],
    decisions: [
      { date: "2026-01-01", text: "First, overturned." },
      { date: "2026-01-02", text: "Second, still holds." },
      { date: "2026-01-03", text: "Third, instead of the first.", supersedes: [1] },
    ],
    ...(retired ? { retired: true } : {}),
    in: [],
    out: [],
  });
  await page.route("**/api/get", async (route) => {
    const ref = route.request().postDataJSON().ref;
    if (ref === "req:revised") return route.fulfill({ json: { node: req("revised", false) } });
    if (ref === "req:gone") return route.fulfill({ json: { node: req("gone", true) } });
    return route.continue();
  });
  await page.goto("/n/req%3Arevised");
  const main = page.locator("main");
  await expect(main.getByText("Third, instead of the first.")).toBeVisible();
  await expect(main.getByText("Second, still holds.")).toBeVisible();
  // The overturned decision is folded, saying what superseded it.
  await expect(main.getByText("First, overturned.")).toBeHidden();
  await main.getByText("1 superseded").click();
  await expect(main.getByText("First, overturned.")).toBeVisible();
  await expect(main.getByText("#1 · superseded by #3")).toBeVisible();
  await expect(main.getByText("retired", { exact: true })).toHaveCount(0);
  await page.goto("/n/req%3Agone");
  await expect(page.locator("main").getByText("retired", { exact: true })).toBeVisible();
});

/**
 * # Test contract
 *
 * ```json
 * {"test": "a-ticket-lists-its-proposal-and-a-node-reads-as-proposed", "checks": ["web/src/views/Node#draft-shown"]}
 * ```
 */
test("an active ticket lists what it proposes, and a node page reads it as proposed", async ({ page }) => {
  await page.goto("/n/ticket%3A0001");
  const main = page.locator("main");
  await expect(main.getByRole("heading", { name: "Proposed" })).toBeVisible();
  const rows = main.locator("h2:has-text('Proposed') + ul > li");
  await expect(rows).toHaveCount(3);
  await expect(rows.filter({ hasText: "Readiness" })).toContainText("changes body");
  await expect(rows.filter({ hasText: "Detached heads" })).toContainText("new");
  const req = rows.filter({ hasText: "Ready only with a commit" });
  await expect(req).toContainText("changes title");
  const asProposed = req.getByRole("link", { name: "as proposed" });
  await expect(asProposed).toHaveAttribute("href", "/n/req%3Aready-needs-commit?ticket=0001");
  await asProposed.click();
  await expect(page).toHaveURL(/req%3Aready-needs-commit\?ticket=0001/);
  await expect(main.locator("h1")).toHaveText("Ready only with a commit, detached too");
  await expect(main.getByText("draft", { exact: true })).toBeVisible();
  await main.getByRole("link", { name: "the truth" }).click();
  await expect(main.locator("h1")).toHaveText("Ready only with a commit");
  await expect(main.getByText("proposes a change to this")).toBeVisible();
  await expect(main.getByRole("link", { name: "read it as proposed" })).toHaveAttribute("href", "/n/req%3Aready-needs-commit?ticket=0001");
  // An open ticket has nothing proposed and no section.
  await page.goto("/n/ticket%3A0006");
  await expect(main.getByRole("heading", { name: "Proposed" })).toHaveCount(0);
});

/**
 * # Test contract
 *
 * ```json
 * {"test": "a-page-names-its-draft-and-reads-as-proposed", "checks": ["web/src/views/Home#draft-line"]}
 * ```
 */
test("a page names the ticket that would change it, and reads as proposed on request", async ({ page }) => {
  await page.goto("/n/page%3Areadiness");
  const main = page.locator("main");
  const proposed = main.getByText("Proposed: readiness also counts a detached head.");
  await expect(main.getByText("proposes a change to this")).toBeVisible();
  await expect(proposed).toHaveCount(0);
  await main.getByRole("link", { name: "read it as proposed" }).click();
  await expect(page).toHaveURL(/ticket=0001/);
  await expect(proposed).toBeVisible();
  await expect(main.getByText("draft", { exact: true })).toBeVisible();
  await expect(page.locator(rail)).toBeVisible();
  // A page no ticket proposes to change carries no line.
  await page.goto("/n/page%3Agoal");
  await expect(main.getByText("proposes a change to this")).toHaveCount(0);
  // A page the proposal adds has no place in the truth's outline, and opens through the ticket.
  await page.goto("/n/page%3Adetached");
  await expect(main.getByText("is not in the outline")).toBeVisible();
  await page.goto("/n/page%3Adetached?ticket=0001");
  await expect(main.locator("h1")).toHaveText("Detached heads");
  await expect(main.getByText("draft", { exact: true })).toBeVisible();
  await expect(page.locator(rail)).toBeVisible();
});
