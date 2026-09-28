/**
 * What each view does, against the fixture store serve.mjs builds: a root
 * page, an area with one requirement and two contracted modules, a module
 * without a contract, one test, and tickets open, high, closed and a gap.
 * Every reference is a link to /n/<ref>, so a view's structure can be read
 * back from its links and compared with the tool answer it was drawn from.
 */
import { expect, test, type Page } from "@playwright/test";

type Tree = { ref: string; children: Tree[]; checks: string[] };

async function tool<T>(page: Page, name: string, input: object = {}): Promise<T> {
  const res = await page.request.post(`/api/${name}`, { data: input });
  return (await res.json()) as T;
}

/** The references a part of the page links to, in document order. */
async function linkedRefs(page: Page, scope = "main"): Promise<string[]> {
  const hrefs = await page.locator(`${scope} a[href^="/n/"]`).evaluateAll((as) => as.map((a) => a.getAttribute("href") ?? ""));
  return hrefs.map((h) => decodeURIComponent(h.slice("/n/".length)));
}

function flatten(trees: Tree[]): Tree[] {
  return trees.flatMap((t) => [t, ...flatten(t.children)]);
}

/**
 * # Test contract
 *
 * ```json
 * {"test": "four-views-and-nothing-else", "checks": ["web/src/App#four-views"]}
 * ```
 */
test("the four views are served, and nothing else", async ({ page }) => {
  for (const [path, marker] of [
    ["/", "#contracts"],
    ["/map", "text=Depth"],
    ["/board", "table"],
    ["/n/page%3Agoal", "h1"],
  ] as const) {
    await page.goto(path);
    await expect(page.locator(marker).first()).toBeVisible();
  }
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
  await page.goto("/map");
  await expect(lit()).toHaveCount(1);
  await expect(lit()).toHaveText("Map");
  await page.goto("/");
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
  for (const path of ["/", "/map", "/board", "/n/module%3Asrc%2Fready", "/n/ticket%3A0001"]) {
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

/**
 * # Test contract
 *
 * ```json
 * {"test": "board-opens-on-change-tickets", "checks": ["web/src/views/Board#first-screen"]}
 * ```
 */
test("the board opens on open and active change tickets; gaps are a toggle away", async ({ page }) => {
  await page.goto("/board");
  await expect(page.getByRole("link", { name: "Make readiness faster" })).toBeVisible();
  await expect(page.getByRole("link", { name: "Readiness ignores submodules" })).toHaveCount(0);
  await expect(page.getByRole("link", { name: "Name the readiness module" })).toHaveCount(0);
  await page.getByRole("button", { name: "gap" }).click();
  await expect(page.getByRole("link", { name: "Readiness ignores submodules" })).toBeVisible();
});

/**
 * # Test contract
 *
 * ```json
 * {"test": "board-keeps-the-tools-order", "checks": ["web/src/views/Board#order-from-tool"]}
 * ```
 */
test("the board's rows are in the order the board tool returns", async ({ page }) => {
  await page.goto("/board");
  await expect(page.locator("tbody tr").first()).toBeVisible();
  const shown = await linkedRefs(page, "tbody");
  const answer = await tool<{ tickets: { ref: string }[] }>(page, "board", { kinds: ["change"], status: ["open", "active"], priority: ["next", "high", "later"] });
  expect(shown).toEqual(answer.tickets.map((t) => t.ref));
});

/**
 * # Test contract
 *
 * ```json
 * {"test": "summary-reads-in-the-roots-order", "checks": ["web/src/views/Home#story-order"]}
 * ```
 */
test("the summary renders the story in the root page's link order", async ({ page }) => {
  await page.goto("/");
  await expect(page.locator("#contracts")).toBeVisible();
  const root = await tool<{ node: { links: string[] } }>(page, "get", { ref: "page:overview" });
  const sections = await page.locator("main section[id^='page-']").evaluateAll((s) => s.map((x) => x.id));
  expect(sections).toEqual(root.node.links.filter((l) => l.startsWith("page:")).map((l) => l.replace(/[^a-z0-9]+/gi, "-")));
  // The area under architecture is a card, and the requirement on a story page is listed compactly.
  await expect(page.locator("main a[href='/n/page%3Areadiness']").first()).toBeVisible();
});

/**
 * # Test contract
 *
 * ```json
 * {"test": "contracts-listed-once-by-area", "checks": ["web/src/views/Home#contracts-grouped"]}
 * ```
 */
test("every module with a purpose is listed once, under the page that links it", async ({ page }) => {
  await page.goto("/");
  const contracts = page.locator("#contracts");
  await expect(contracts).toBeVisible();
  const refs = await linkedRefs(page, "#contracts");
  const modules = refs.filter((r) => r.startsWith("module:"));
  expect(modules.sort()).toEqual(["module:src", "module:src/ready"]);
  expect(refs.some((r) => r.startsWith("file:") || r.startsWith("decl:")), "no files or declarations").toBe(false);
  await expect(contracts.locator("h3")).toContainText(["Readiness"]);
});

/**
 * # Test contract
 *
 * ```json
 * {"test": "map-is-the-outline", "checks": ["web/src/views/Map#outline-driven"]}
 * ```
 */
test("the map is drawn from outline alone", async ({ page }) => {
  await page.goto("/map");
  await expect(page.getByText("Depth")).toBeVisible();
  await expect(page.locator("main a[href='/n/criterion%3Asrc%2Fready%23reads-only']")).toBeVisible();
  const outline = await tool<{ roots: Tree[] }>(page, "outline", { depth: 4 });
  const expected = flatten(outline.roots).map((t) => t.ref);
  expect(new Set(await linkedRefs(page))).toEqual(new Set(expected));
});

/**
 * # Test contract
 *
 * ```json
 * {"test": "map-marks-uncovered-and-untested", "checks": ["web/src/views/Map#marks"]}
 * ```
 */
test("the map marks a criterion uncovered and untested from outline's checks", async ({ page }) => {
  await page.goto("/map");
  const leaf = (id: string) => page.locator(`main a[href='/n/criterion%3Asrc%2Fready%23${id}']`).locator("xpath=ancestor::div[1]");
  await expect(leaf("reads-only")).toContainText("uncovered");
  await expect(leaf("reads-only")).toContainText("untested");
  await expect(leaf("needs-commit")).not.toContainText("uncovered");
  await expect(leaf("needs-commit")).not.toContainText("untested");
  await expect(page.locator("main").getByText("% covered").first()).toBeVisible();
});

/**
 * # Test contract
 *
 * ```json
 * {"test": "every-kind-has-a-page", "checks": ["web/src/views/Node#every-kind"]}
 * ```
 */
test("every node kind has a page", async ({ page }) => {
  const refs = [
    "page:readiness",
    "req:ready-needs-commit",
    "ticket:0001",
    "module:src/ready",
    "criterion:src/ready#needs-commit",
    "file:src/ready.rs",
    "decl:src/ready.rs#ready",
    "test:src/ready.rs#ready-needs-a-commit",
  ];
  for (const ref of refs) {
    await page.goto(`/n/${encodeURIComponent(ref)}`);
    await expect(page.locator("h1"), ref).toBeVisible();
    await expect(page.locator("main"), ref).not.toContainText("does not exist");
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
