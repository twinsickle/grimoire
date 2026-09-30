/**
 * What each view does, against the fixture store serve.mjs builds: a root
 * page, an area with one requirement and two contracted modules, a module
 * without a contract, one test, and tickets open, high, closed and a gap.
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
  await page.goto("/");
  await expect(page.locator(rail)).toBeVisible();
  const outline = await tool<Outline>(page, "outline", { depth: 64 });
  // The root links to /, every other page to its node page, in the outline's order.
  const expected = pagesOf(outline.roots).slice(1).map((t) => t.ref);
  expect(await linkedRefs(page, rail)).toEqual(expected);
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
  const kinds = nodes.map((r) => decodeURIComponent(r.path.slice("/n/".length)).split(":")[0]).sort();
  expect(kinds, "one node page of every kind").toEqual(["criterion", "decl", "file", "module", "page", "req", "test", "ticket"]);
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
