/**
 * What each view does, against the fixture store serve.mjs builds: a root
 * page and a second root with one page under it, an area with one
 * requirement and two contracted modules, a module
 * without a contract, one test, and tickets inscribed, high, bottled and a gap, the
 * first brewing with a proposal, one simmering, and one bottled touching a
 * requirement and relying on a page that no longer exist.
 * Every finding at that point is adopted as the baseline, and one requirement
 * added after it, so the views have inherited and new findings to tell apart.
 * Every reference is a link to /n/<ref>, so a view's structure can be read
 * back from its links and compared with the tool answer it was drawn from.
 *
 * A page is not finished when its heading shows: each panel loads by its own
 * query, in no fixed order. So a check here names the element it means, by
 * role, by exact text or by the list it sits in, never the first of several
 * that different panels draw; what is read from the page once is read after
 * something from the same query is there, or polled; and an absence is
 * checked only once the query that would have drawn the thing has answered.
 */
import { expect, test, type Locator, type Page } from "@playwright/test";
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
  const hrefs = await page.locator(`${scope} a[href^="/g/repo/n/"]`).evaluateAll((as) => as.map((a) => a.getAttribute("href") ?? ""));
  return hrefs.map((h) => decodeURIComponent(h.slice("/g/repo/n/".length)));
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
 * {"test": "three-views-and-nothing-else", "checks": ["web/src/App#grimoire-routes", "web/src/App#old-addresses-redirect", "web/src/App#own-address-is-the-library", "web/src/App#own-address-forwards-to-default", "web/src/App#store-alone-at-own-address"]}
 * ```
 */
test("the three views are served, a page is the grimoire, and nothing else", async ({ page }) => {
  for (const [path, marker] of [
    ["/g/repo/", rail],
    ["/g/repo/n/page%3Agoal", rail],
    ["/g/repo/unplaced", rail],
    ["/g/repo/cauldron", "section[id^='band-']"],
    ["/g/repo/n/req%3Aready-needs-commit", "h1"],
    ["/g/repo/findings", "[data-group='new']"],
  ] as const) {
    await page.goto(path);
    await expect(page.locator(marker).first(), path).toBeVisible();
  }
  // A requirement's node page has no rail; a page's is the grimoire view.
  await page.goto("/g/repo/n/req%3Aready-needs-commit");
  await expect(page.locator("main h1")).toBeVisible();
  await expect(page.locator(rail)).toHaveCount(0);
  await page.goto("/g/repo/map");
  await expect(page).toHaveURL(/\/g\/repo\/?$/);
  await expect(page.locator(rail)).toBeVisible();
  // The cauldron's earlier address still opens it.
  await page.goto("/g/repo/board");
  await expect(page).toHaveURL(/\/cauldron$/);
  await expect(page.locator("section[id^='band-']").first()).toBeVisible();
  await page.goto("/g/repo/nowhere");
  await expect(page.locator("header")).toBeVisible();
  await expect(page.locator("main *")).toHaveCount(0);
  // The daemon's own address is the Library, and every other path there opens the default grimoire's at its own address.
  await page.goto("/");
  await expect(page.locator("[data-library] [data-spine]").first()).toBeVisible();
  await expect(page.locator("header button[aria-label^='Where you are']")).toHaveCount(0);
  for (const [path, at] of [
    ["/cauldron", "/g/repo/cauldron"],
    ["/n/req%3Aready-needs-commit?ticket=0001", "/g/repo/n/req%3Aready-needs-commit?ticket=0001"],
    ["/unplaced", "/g/repo/unplaced"],
    ["/n/file%3Asrc%2Fready.rs#L7", "/g/repo/n/file%3Asrc%2Fready.rs#L7"],
  ]) {
    await page.goto(path);
    await expect(page, path).toHaveURL(at);
  }
  await page.goto("/unplaced");
  await expect(page.locator(rail)).toBeVisible();
  // A store served alone names no grimoire and has no library: its views stay at the daemon's own address, with no way to all projects.
  await page.route("**/api/status", async (route) => {
    const fetched = await route.fetch().catch(() => null);
    if (!fetched) return;
    await route.fulfill({ json: { ...(await fetched.json()), grimoire: null, library: [] } }).catch(() => {});
  });
  await page.goto("/cauldron");
  await expect(page.locator("section[id^='band-']").first()).toBeVisible();
  await expect(page).toHaveURL(/127\.0\.0\.1:\d+\/cauldron$/);
  await page.goto("/");
  await expect(page.locator(rail)).toBeVisible();
  await expect(page.locator("[data-library]")).toHaveCount(0);
  await expect(page.locator("header button[aria-label^='Where you are']")).toBeVisible();
  await expect(page.locator("[data-shelf-rail]")).toHaveCount(0);
  // A library of one grimoire is still the Library at /, its one book opening it at its own address.
  await page.unrouteAll({ behavior: "ignoreErrors" });
  await page.route("**/api/status", async (route) => {
    const fetched = await route.fetch().catch(() => null);
    if (!fetched) return;
    const answer = await fetched.json();
    await route.fulfill({ json: { ...answer, library: answer.library.filter((g: { name: string }) => g.name === "repo") } }).catch(() => {});
  });
  await page.goto("/");
  await expect(page.locator("[data-spine]")).toHaveCount(1);
  await expect(page.locator("[data-spine]")).toHaveAttribute("href", "/g/repo/");
  await page.unrouteAll({ behavior: "ignoreErrors" });
});

/**
 * # Test contract
 *
 * ```json
 * {"test": "a-grimoire-of-the-library-at-its-own-address", "checks": ["web/src/App#links-stay-under-the-grimoire", "web/src/App#header-shows-no-node-counts", "web/src/App#pill-names-grimoire", "web/src/App#pill-names-checkout-of-several", "web/src/App#pill-shows-current-ticket", "web/src/App#panel-opens-with-shadow", "web/src/App#panel-closes-on-escape", "web/src/App#panel-shows-server-state", "web/src/App#panel-lists-this-grimoires-checkouts", "web/src/App#panel-checkout-line", "web/src/App#panel-marks-checkout-on-screen", "web/src/App#panel-has-no-link-to-other-projects", "web/src/api#grimoire-base"]}
 * ```
 */
test("a grimoire of the library is read at its own address, and the header lists its checkouts alone", async ({ page }) => {
  // The fixture's daemon serves a library of four: its own checkout, `repo`, and `ledger`, `almanac` and `bestiary`.
  const calls: string[] = [];
  page.on("request", (r) => {
    const path = new URL(r.url()).pathname;
    if (path.includes("/api/")) calls.push(path);
  });
  await page.goto("/g/ledger/cauldron");
  await expect(page.locator("main h1")).toHaveText("Cauldron");
  const pill = page.locator("header button[aria-label^='Where you are']");
  // One checkout: the project alone, its checkout not named again.
  await expect(pill.locator(".font-bold")).toHaveText("ledger");
  await expect(pill).not.toContainText("/");
  // No node counts: nobody acts on them.
  await expect(page.locator("header")).not.toContainText("pages ·");
  await pill.click();
  const panel = page.getByRole("dialog", { name: "Checkouts" });
  await expect(panel).toHaveClass(/--color-shade/);
  await expect(panel).toContainText("this machine");
  await expect(panel.getByRole("listitem")).toHaveCount(1);
  await expect(panel.getByRole("listitem")).toContainText("ledger no ticket");
  // The header holds this project alone: `repo` is another project, not listed.
  await expect(panel).not.toContainText("repo");
  // The way to other projects is the shelf rail, not the header.
  await expect(panel.getByRole("link")).toHaveCount(3);
  await expect(panel).not.toContainText("All projects");
  // Escape closes it.
  await page.keyboard.press("Escape");
  await expect(panel).toHaveCount(0);
  // Every call and the event stream went to ledger's address, none to the bare routes.
  expect(calls.length).toBeGreaterThan(0);
  expect(calls.filter((c) => !c.startsWith("/g/ledger/api/")), "calls outside ledger's address").toEqual([]);
  // Its links stay under its address: the grimoire tab opens ledger's grimoire.
  await page.locator("header nav a").first().click();
  await expect(page).toHaveURL(/\/g\/ledger\/?$/);
  // `repo` at its own address: its current ticket in the pill and in its one checkout's line.
  const status = await tool<{ current: { id: string; title: string } }>(page, "status");
  await page.goto("/g/repo/");
  await expect(page.locator(rail)).toBeVisible();
  await expect(pill).toContainText(`repo ${status.current.id}`);
  await expect(pill).not.toContainText("/");
  await pill.click();
  await expect(panel.locator("[aria-current]")).toHaveText(`repo ${status.current.id} ${status.current.title}`);
  const goal = await tool<{ node: { title: string } }>(page, "get", { ref: "page:goal" });
  await page.goto("/g/repo/n/page%3Agoal");
  await expect(page.locator("main h1").first()).toContainText(goal.node.title);
});

/**
 * # Test contract
 *
 * ```json
 * {"test": "the-shelf-rail-leads-to-every-project", "checks": ["web/src/views/Library#rail-mark-links-to-library", "web/src/views/Library#rail-shelves-by-place", "web/src/views/Library#rail-shelf-titled-with-place", "web/src/views/Library#rail-shelf-named-for-screen-reader", "web/src/views/Library#rail-book-per-grimoire-in-name-order", "web/src/views/Library#rail-book-shows-initial", "web/src/views/Library#rail-book-titled-with-name", "web/src/views/Library#rail-book-cloth-of-spine", "web/src/views/Library#rail-book-opens-grimoire", "web/src/views/Library#rail-current-book-ringed", "web/src/views/Library#rail-current-book-marked-current", "web/src/views/Library#rail-book-slides-on-hover", "web/src/views/Library#rail-book-slides-on-focus", "web/src/views/Library#rail-focus-outlined", "web/src/App#views-beside-the-shelf-rail"]}
 * ```
 */
test("the shelf rail beside every page of a grimoire leads to the Library and to each project, shelf by shelf", async ({ page }) => {
  const status = await tool<{ library: { name: string; server: string | null }[] }>(page, "status");
  const places = [...new Set(status.library.map((g) => g.server))].sort((a, b) => (a === null ? -1 : b === null ? 1 : a.localeCompare(b)));
  const namesAt = (place: string | null) => [...new Set(status.library.filter((g) => g.server === place).map((g) => g.name))].sort();
  const shelfRail = page.locator("[data-shelf-rail]");
  // Beside the header and the view on every view of a grimoire, not inside either.
  for (const path of ["/g/repo/", "/g/repo/n/page%3Agoal", "/g/repo/unplaced", "/g/repo/cauldron", "/g/repo/findings", "/g/repo/n/req%3Aready-needs-commit"]) {
    await page.goto(path);
    await expect(shelfRail, path).toBeVisible();
    await expect(page.locator("header [data-shelf-rail], main [data-shelf-rail]")).toHaveCount(0);
  }
  // One shelf per place, this machine first, each titled with its place, its books in name order with their initials.
  const shelves = shelfRail.locator("[data-shelf]");
  await expect(shelves).toHaveCount(places.length);
  for (const [i, place] of places.entries()) {
    const names = namesAt(place);
    const label = place === null ? "this machine" : place.replace(/^https?:\/\//, "");
    await expect(shelves.nth(i)).toHaveAttribute("title", label);
    // Named for a screen reader as the Library names its stacks.
    await expect(shelves.nth(i).getByRole("list")).toHaveAccessibleName(place === null ? "On this machine" : `From ${label}`);
    const books = shelves.nth(i).locator("[data-rail-book]");
    await expect(books).toHaveText(names.map((n) => n.charAt(0).toUpperCase()));
    for (const [j, name] of names.entries()) {
      await expect(books.nth(j)).toHaveAttribute("title", name);
      await expect(books.nth(j)).toHaveAttribute("href", `/g/${name}/`);
    }
  }
  // The grimoire on screen is ringed and current, and no other.
  await expect(shelfRail.locator("[aria-current='page']")).toHaveAttribute("title", "repo");
  await expect(shelfRail.locator("[aria-current='page']")).toHaveClass(/ring-accent/);
  // A book is in its Library spine's cloth, so it looks the same in both places.
  const cloth = (el: Element) => getComputedStyle(el).backgroundColor;
  const railCloth = await shelfRail.locator("[data-rail-book][title='ledger']").evaluate(cloth);
  // One press opens another project, ringed there.
  await shelfRail.locator("[data-rail-book][title='ledger']").click();
  await expect(page).toHaveURL(/\/g\/ledger\/$/);
  await expect(page.locator("[data-shelf-rail] [aria-current='page']")).toHaveAttribute("title", "ledger");
  // The mark leads back to the Library, where the same grimoire wears the same cloth.
  await page.locator("[data-shelf-rail]").getByRole("link", { name: "Library", exact: true }).click();
  await expect(page).toHaveURL(/^http:\/\/[^/]+\/$/);
  expect(await page.locator("[data-spine]", { hasText: "ledger" }).evaluate(cloth)).toBe(railCloth);
  // The Library is where the rail leads, so it has none of its own.
  await expect(page.locator("[data-shelf-rail]")).toHaveCount(0);
  // A book slides out on hover where motion is allowed, and stays still where reduced motion is asked for.
  await page.goto("/g/repo/cauldron");
  const ledger = page.locator("[data-rail-book][title='ledger']");
  const moved = () => ledger.evaluate((el) => getComputedStyle(el).translate);
  await page.emulateMedia({ reducedMotion: "reduce" });
  await ledger.hover();
  await page.waitForTimeout(300);
  expect(await moved()).toBe("none");
  await page.mouse.move(400, 400);
  await page.emulateMedia({ reducedMotion: "no-preference" });
  await ledger.hover();
  await expect.poll(moved).not.toBe("none");
  // Keyboard focus slides it out too, outlined in the accent colour.
  await page.mouse.move(400, 400);
  await expect.poll(moved).toBe("none");
  await ledger.focus();
  await page.keyboard.press("Shift+Tab");
  await page.keyboard.press("Tab");
  await expect(ledger).toBeFocused();
  await expect.poll(moved).not.toBe("none");
  const accent = await page.evaluate(() => getComputedStyle(document.documentElement).getPropertyValue("--color-accent").trim());
  const rgb = (hex: string) => `rgb(${[1, 3, 5].map((i) => parseInt(hex.slice(i, i + 2), 16)).join(", ")})`;
  expect(await ledger.evaluate((el) => getComputedStyle(el).outlineColor)).toBe(rgb(accent));
  // A library of one grimoire still has the rail, the way back to the Library always there.
  await page.route("**/api/status", async (route) => {
    const fetched = await route.fetch().catch(() => null);
    if (!fetched) return;
    const answer = await fetched.json();
    await route.fulfill({ json: { ...answer, library: answer.library.filter((g: { name: string }) => g.name === "repo") } }).catch(() => {});
  });
  await page.reload();
  await expect(page.locator("[data-shelf-rail] [data-shelf]")).toHaveCount(1);
  await expect(page.locator("[data-shelf-rail] [data-rail-book]")).toHaveCount(1);
  await expect(page.locator("[data-shelf-rail]").getByRole("link", { name: "Library", exact: true })).toHaveAttribute("href", "/");
  await page.unrouteAll({ behavior: "ignoreErrors" });
});

/**
 * # Test contract
 *
 * ```json
 * {"test": "the-library-stacks-the-machines-grimoires", "checks": ["web/src/views/Library#stacks-by-place", "web/src/views/Library#stack-labelled-with-place", "web/src/views/Library#stack-spine-per-grimoire", "web/src/views/Library#spine-marked-with-checkouts", "web/src/views/Library#library-header-holds-book-and-name", "web/src/views/Library#library-header-has-no-project-parts", "web/src/views/Library#spine-opens", "web/src/views/Library#jingle", "web/src/App#own-address-is-the-library"]}
 * ```
 */
test("the Library stands the machine's grimoires in a stack for each place they come from, each spine opening its grimoire", async ({ page }) => {
  // The shelf rail's mark opens the Library.
  await page.goto("/g/ledger/cauldron");
  await page.locator("[data-shelf-rail]").getByRole("link", { name: "Library", exact: true }).click();
  await expect(page).toHaveURL(/^http:\/\/[^/]+\/$/);
  // A stack for each place status names, this machine first, each grimoire one spine in name order.
  const status = await tool<{ library: { name: string; server: string | null }[] }>(page, "status");
  const places = [...new Set(status.library.map((g) => g.server))].sort((a, b) => (a === null ? -1 : b === null ? 1 : a.localeCompare(b)));
  const stacks = page.locator("[data-stack]");
  await expect(stacks).toHaveCount(places.length);
  for (const [i, place] of places.entries()) {
    const names = [...new Set(status.library.filter((g) => g.server === place).map((g) => g.name))].sort();
    await expect(stacks.nth(i).locator("[data-spine]")).toHaveText(names);
    await expect(stacks.nth(i).locator("figcaption")).toHaveText(place === null ? "this machine" : place.replace(/^https?:\/\//, ""));
  }
  await expect(page.locator("[data-spine]")).toHaveText(places.flatMap((p) => [...new Set(status.library.filter((g) => g.server === p).map((g) => g.name))].sort()));
  // Grimoire's own page: no project's pill, tabs or search, and the jingle beneath the stacks.
  await expect(page.locator("header")).toContainText("Grimoire");
  // The book mark beside the name: the one figure in the header, drawn as the book icon, to the name's left.
  const mark = page.locator("header svg");
  await expect(mark).toHaveCount(1);
  const [markBox, nameBox] = await Promise.all([mark.boundingBox(), page.locator("header").getByText("Grimoire", { exact: true }).boundingBox()]);
  expect(markBox!.x + markBox!.width, "the book beside the name, before it").toBeLessThanOrEqual(nameBox!.x + 1);
  expect(markBox!.y, "level with the name").toBeLessThan(nameBox!.y + nameBox!.height);
  expect(markBox!.y + markBox!.height, "level with the name").toBeGreaterThan(nameBox!.y);
  const libraryBook = await mark.evaluate((el) => el.innerHTML);
  await page.goto("/g/repo/");
  expect(libraryBook, "the book icon the tabs draw").toBe(await page.locator("header nav a").first().locator("svg").evaluate((el) => el.innerHTML));
  await page.goBack();
  await expect(page.locator("[data-stack]").first()).toBeVisible();
  await expect(page.locator("header button, header nav, header input")).toHaveCount(0);
  await expect(page.getByText("It's almost magic.", { exact: true })).toHaveCount(1);
  // The glint beside it twinkles where motion is allowed, and is still where reduced motion is asked for.
  const twinkles = () => page.evaluate(() => document.getAnimations().filter((a) => a instanceof CSSAnimation && a.animationName === "twinkle").length);
  await expect.poll(twinkles).toBe(1);
  await page.emulateMedia({ reducedMotion: "reduce" });
  await expect.poll(twinkles).toBe(0);
  await page.emulateMedia({ reducedMotion: "no-preference" });
  // A grimoire served for several checkouts is one spine, marked with their count; one served once is not marked.
  await page.route("**/api/status", async (route) => {
    // A request the reload abandons is left alone.
    const fetched = await route.fetch().catch(() => null);
    if (!fetched) return;
    const answer = await fetched.json();
    const ledgers = answer.library.filter((g: { name: string }) => g.name === "ledger");
    answer.library.push({ ...ledgers[0], repository: `${ledgers[0].repository}-worktree` });
    await route.fulfill({ json: answer }).catch(() => {});
  });
  await page.reload();
  await expect(page.locator("[data-spine]", { hasText: "ledger" })).toHaveText("ledger 2 checkouts");
  await expect(page.locator("[data-spine]", { hasText: "almanac" })).toHaveText("almanac");
  await page.unrouteAll({ behavior: "ignoreErrors" });
  await page.reload();
  await expect(page.locator("[data-spine]", { hasText: "ledger" })).toHaveText("ledger");
  // A spine slides out on hover where motion is allowed, and stays still where reduced motion is asked for.
  const ledger = page.locator("[data-spine]", { hasText: "ledger" });
  const moved = () => ledger.evaluate((el) => getComputedStyle(el).translate);
  await page.emulateMedia({ reducedMotion: "reduce" });
  await ledger.hover();
  await page.waitForTimeout(300);
  expect(await moved()).toBe("none");
  await page.mouse.move(0, 0);
  await page.emulateMedia({ reducedMotion: "no-preference" });
  await ledger.hover();
  await expect.poll(moved).not.toBe("none");
  // Keyboard focus slides it out too, ringed in the accent colour.
  await page.mouse.move(0, 0);
  await ledger.focus();
  await page.keyboard.press("Shift+Tab");
  await page.keyboard.press("Tab");
  await expect.poll(moved).not.toBe("none");
  const accent = await page.evaluate(() => getComputedStyle(document.documentElement).getPropertyValue("--color-accent").trim());
  const ring = await ledger.evaluate((el) => [getComputedStyle(el).outlineStyle, getComputedStyle(el).outlineColor]);
  expect(ring[0]).toBe("solid");
  expect(ring[1]).toBe(await page.evaluate((c) => { const d = document.createElement("div"); d.style.color = c; document.body.append(d); const rgb = getComputedStyle(d).color; d.remove(); return rgb; }, accent));
  // Its full name is its title, where a narrow spine cuts it short.
  await expect(ledger).toHaveAttribute("title", "ledger");
  // Pressed, it opens that grimoire at its own address.
  await expect(ledger).toHaveAttribute("href", "/g/ledger/");
  await ledger.click();
  await expect(page).toHaveURL(/\/g\/ledger\/$/);
  await expect(page.locator("main [data-empty-state]")).toBeVisible();
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
 * {"test": "almost-magic-only-where-nothing-is-yet", "checks": ["web/src/components#empty-state-icon-large", "web/src/components#empty-state-glint-beside-icon", "web/src/components#empty-state-glint-twinkles-with-motion", "web/src/components#empty-state-shows-direction", "web/src/components#empty-state-backticks-as-code", "web/src/components#empty-state-jingle-beneath", "web/src/components#jingle-drawn-only-by-empty-states-and-library", "web/src/components#jingle-in-accent-colour", "web/src/views/Home#empty-grimoire", "web/src/views/Board#empty-cauldron"]}
 * ```
 */
test("\"It's almost magic\" shows where a whole view has nothing yet, and nowhere else", async ({ page }) => {
  const jingle = page.getByText("It's almost magic.", { exact: true });
  const empty = page.locator("main [data-empty-state]");
  // The empty state as drawn: the section's own icon, the header's tab icon (0 the book, 1 the cauldron) drawn large, with a
  // glint beside it in the jingle's accent colour, then the direction, then the jingle beneath, in that order down the view.
  const drawn = async (tab: 0 | 1) => {
    const svgs = empty.locator("svg");
    const icon = svgs.first();
    const markup = (l: typeof icon) => l.evaluate((el) => el.innerHTML);
    expect(await markup(icon), "the section's own icon").toBe(await markup(page.locator("header nav a").nth(tab).locator("svg")));
    const iconBox = (await icon.boundingBox())!;
    expect(iconBox.height, "drawn large").toBeGreaterThan(3 * (await page.locator("header nav a").nth(tab).locator("svg").boundingBox())!.height);
    const glint = icon.locator("xpath=following-sibling::*[local-name()='svg']").first();
    await expect(glint, "a glint beside the icon").toBeVisible();
    // A glint, drawn as the smaller glint near it is, not some other figure in its place.
    expect(await markup(glint), "the glint's own figure").toBe(await markup(icon.locator("xpath=following-sibling::*[local-name()='svg']").nth(1)));
    expect(await markup(glint), "the glint's own figure").not.toBe(await markup(icon));
    // Beside it: level with the icon, reaching past its right edge and no further away than the glint is wide.
    const glintBox = (await glint.boundingBox())!;
    expect(glintBox.y + glintBox.height, "the glint level with the icon").toBeGreaterThan(iconBox.y);
    expect(glintBox.y, "the glint level with the icon").toBeLessThan(iconBox.y + iconBox.height);
    expect(glintBox.x + glintBox.width, "the glint at the icon's side").toBeGreaterThan(iconBox.x + iconBox.width);
    expect(glintBox.x, "the glint at the icon's side").toBeLessThan(iconBox.x + iconBox.width + glintBox.width);
    // The accent colour is the token's, resolved by the page, not merely whatever the jingle happens to be.
    const accent = await page.evaluate(() => {
      const probe = document.body.appendChild(document.createElement("span"));
      probe.style.color = "var(--color-accent)";
      const resolved = getComputedStyle(probe).color;
      probe.remove();
      return resolved;
    });
    const color = (l: typeof icon) => l.evaluate((el) => getComputedStyle(el).color);
    // The glint is painted, fill and every path, in the accent colour, not merely given it as its text colour.
    const paint = await glint.evaluate((el) => [el, ...el.querySelectorAll("path")].map((e) => [getComputedStyle(e).fill, getComputedStyle(e).stroke]));
    for (const [fill, stroke] of paint) {
      expect(fill, "the glint in the accent colour").toBe(accent);
      expect(stroke, "the glint drawn with no other colour").toBe("none");
    }
    expect(await color(jingle), "the jingle in the accent colour").toBe(accent);
    const direction = empty.locator("p").filter({ hasNotText: "It's almost magic." });
    await expect(direction).toHaveCount(1);
    const [d, j] = [(await direction.boundingBox())!, (await jingle.boundingBox())!];
    expect(d.y, "the direction beneath the icon").toBeGreaterThanOrEqual(iconBox.y + iconBox.height);
    expect(j.y, "the jingle beneath the direction").toBeGreaterThanOrEqual(d.y + d.height);
    return glint;
  };
  // What the glint is doing: each running animation's name, whether it lasts, and whether its keyframes change how it looks.
  const twinkle = (glint: Locator) =>
    glint.evaluate((el) =>
      el.getAnimations().map((a) => {
        const effect = a.effect as KeyframeEffect;
        const looks = new Set(effect.getKeyframes().map((k) => `${k.opacity ?? ""}|${k.transform ?? ""}`));
        return {
          name: (a as CSSAnimation).animationName,
          running: a.playState === "running" && a.playbackRate !== 0,
          lasts: Number(effect.getComputedTiming().duration) > 0,
          moves: looks.size > 1,
        };
      }),
    );
  const moving = [{ name: "twinkle", running: true, lasts: true, moves: true }];
  // ledger, the empty grimoire of the fixture's library: no pages, so its grimoire view is the empty state, with no rail.
  await page.goto("/g/ledger/");
  await expect(empty).toContainText("has no page yet");
  // Each span between backticks, the root's slug twice, is set as code, and no backtick is left in the text.
  await expect(empty.locator("code")).toHaveText(["overview", "overview"]);
  await expect(empty.locator("p").first()).toHaveText("The grimoire root overview has no page yet: put a page overview under a ticket.");
  await expect(empty).not.toContainText("`");
  await expect(jingle).toHaveCount(1);
  await expect(page.locator(rail)).toHaveCount(0);
  const glint = await drawn(0);
  // The glint twinkles where motion is allowed, and is still where the person asks for reduced motion.
  expect(await twinkle(glint)).toEqual(moving);
  await page.emulateMedia({ reducedMotion: "reduce" });
  expect(await twinkle(glint)).toEqual([]);
  await page.emulateMedia({ reducedMotion: "no-preference" });
  // Its cauldron: the filters, then the empty state in place of the bands and the Unsettled section.
  await page.goto("/g/ledger/cauldron");
  await expect(page.locator("main h1")).toHaveText("Cauldron");
  await expect(empty.locator("p").first()).toHaveText("The cauldron is empty: no change is inscribed or brewing and no gap or question waits. A ticket an agent inscribes shows here.");
  await expect(jingle).toHaveCount(1);
  const glinting = await drawn(1);
  expect(await twinkle(glinting)).toEqual(moving);
  await page.emulateMedia({ reducedMotion: "reduce" });
  expect(await twinkle(glinting)).toEqual([]);
  await page.emulateMedia({ reducedMotion: "no-preference" });
  await expect(page.locator("section[id^='band-'], #unattached")).toHaveCount(0);
  // Unplaced and a page asked for by name in the empty grimoire say what is missing, plainly, with no jingle.
  for (const path of ["/g/ledger/unplaced", "/g/ledger/n/page%3Aoverview"]) {
    await page.goto(path);
    await expect(page.locator("main"), path).toContainText("has no page yet");
    await expect(page.locator("[data-empty-state]"), path).toHaveCount(0);
    await expect(jingle, path).toHaveCount(0);
  }
  // A change inscribed and bottled in ledger leaves its cauldron empty, and the recently bottled still show it when asked for.
  const ledger = async (name: string, input: object) => (await page.request.post(`/g/ledger/api/${name}`, { data: input })).json();
  const filed = await ledger("inscribe_ticket", { title: "Brewed and done", kind: "change", priority: "later", touches: [], body: "Bottled at once." });
  await ledger("brew_ticket", { ref: filed.node.ref });
  await ledger("close_ticket", { ref: filed.node.ref, decision: { text: "Done.", date: "2026-01-20" } });
  await page.goto("/g/ledger/cauldron");
  await expect(page.locator("main [data-empty-state]")).toContainText("The cauldron is empty");
  await page.getByRole("button", { name: "Bottled", exact: true }).click();
  await expect(page.locator("#bottled")).toContainText("Brewed and done");
  await expect(jingle).toHaveCount(1);
  // A grimoire with things in it shows the jingle nowhere: not its views, its node pages, nor a search that finds nothing.
  // The Library is Grimoire's own page, and carries the jingle by design.
  for (const { path, content } of ROUTES.filter((r) => !r.path.startsWith("/g/ledger/") && r.path !== "/")) {
    await page.goto(path);
    await page.locator(content).first().waitFor();
    await expect(page.locator("[data-empty-state]"), path).toHaveCount(0);
    await expect(jingle, path).toHaveCount(0);
  }
  await page.getByPlaceholder("Find a page, requirement, module, ticket…").fill("zzqx-nothing-is-called-this");
  await page.waitForTimeout(400);
  await expect(jingle).toHaveCount(0);
});

/**
 * # Test contract
 *
 * ```json
 * {"test": "one-tab-is-lit", "checks": ["web/src/App#tabs-are-grimoire-and-cauldron", "web/src/App#exactly-one-tab-lit", "web/src/App#tab-lit-by-view", "web/src/App#tab-lit-on-a-node-page"]}
 * ```
 */
test("exactly one tab is lit, following the section a node was reached from", async ({ page }) => {
  const lit = () => page.locator('header [aria-current="page"]');
  await page.goto("/g/repo/");
  await expect(lit()).toHaveCount(1);
  await expect(lit()).toContainText("Grimoire");
  await expect(page.locator("header nav a")).toHaveText(["Grimoire", "Cauldron"]);
  await page.goto("/g/repo/n/page%3Areadiness");
  await expect(lit()).toContainText("Grimoire");
  // Reached from the board, a ticket keeps Board lit.
  await page.goto("/g/repo/cauldron");
  await page.getByRole("link", { name: "Make readiness faster" }).click();
  await expect(lit()).toHaveText("Cauldron");
  // Opened cold, a requirement lights Grimoire.
  await page.goto("/g/repo/n/req%3Aready-needs-commit");
  await expect(lit()).toContainText("Grimoire");
  // The findings are the grimoire's, wherever they were opened from.
  await page.goto("/g/repo/cauldron");
  await page.goto("/g/repo/findings");
  await expect(lit()).toContainText("Grimoire");
});

/**
 * # Test contract
 *
 * ```json
 * {"test": "header-tells-new-from-inherited", "checks": ["web/src/App#header-pill-shows-new", "web/src/App#header-panel-counts", "web/src/App#header-counts-open-findings"]}
 * ```
 */
test("the header shows new findings as a warning and inherited ones dimmed", async ({ page }) => {
  const lint = await tool<{ findings: unknown[]; new: number }>(page, "lint");
  const inherited = lint.findings.length - lint.new;
  expect(lint.new, "the fixture adds new work after adopting").toBeGreaterThan(0);
  expect(inherited).toBeGreaterThan(0);
  await page.goto("/g/repo/cauldron");
  const pill = page.locator("header button[aria-label^='Where you are']");
  const glance = pill.getByText(`${lint.new} new`, { exact: true });
  await expect(glance).toBeVisible();
  await expect(glance).toHaveClass(/text-bad/);
  await pill.click();
  const panel = page.getByRole("dialog", { name: "Checkouts" });
  const fresh = panel.getByText(`${lint.new} new`, { exact: true });
  await expect(fresh).toHaveClass(/text-bad/);
  const old = panel.getByText(`${inherited} inherited`, { exact: true });
  await expect(old).toBeVisible();
  await expect(old).not.toHaveClass(/text-bad/);
  // Each count opens the one view of them all, and the panel closes behind it.
  for (const name of [`${lint.new} new`, `${inherited} inherited`, /may conflict$/]) {
    await page.goto("/g/repo/cauldron");
    await pill.click();
    await panel.getByRole("link", { name }).click();
    await expect(page).toHaveURL(/\/g\/repo\/findings$/);
    await expect(panel).toHaveCount(0);
    await expect(page.locator("[data-group='new']")).toBeVisible();
  }
});
/**
 * # Test contract
 *
 * ```json
 * {"test": "header-shows-other-work-apart", "checks": ["web/src/App#header-advisories"]}
 * ```
 */
test("the header shows what other work may conflict apart from the findings, not as a warning", async ({ page }) => {
  const lint = await tool<{ advisories: unknown[] }>(page, "lint");
  expect(lint.advisories.length, "the fixture's simmering ticket proposes a criterion its code lacks").toBeGreaterThan(0);
  await page.goto("/g/repo/cauldron");
  // At a glance only what is new; the rest in the pill's panel.
  await expect(page.locator("header button[aria-label^='Where you are']")).not.toContainText("may conflict");
  await page.locator("header button[aria-label^='Where you are']").click();
  const conflicts = page.getByRole("dialog", { name: "Checkouts" }).getByText(`${lint.advisories.length} may conflict`);
  await expect(conflicts).toBeVisible();
  await expect(conflicts).not.toHaveClass(/text-bad/);
  await expect(conflicts).toHaveAttribute("title", /does not fail the check/);
});
/**
 * # Test contract
 *
 * ```json
 * {"test": "a-node-shows-other-work-on-it", "checks": ["web/src/views/Node#advisories-shown"]}
 * ```
 */
test("a node's page says under its title the other work that may conflict with it, naming the ticket", async ({ page }) => {
  type Advisory = { from: { kind: string; ticket?: string }; finding: { ref: string } };
  const lint = await tool<{ advisories: Advisory[] }>(page, "lint");
  const on = lint.advisories.find((a) => a.finding.ref === "req:ready-writes-nothing" && a.from.kind === "ticket");
  expect(on, "the simmering ticket's repointed requirement").toBeTruthy();
  await page.goto("/g/repo/n/req%3Aready-writes-nothing");
  const item = page.locator("[aria-label='Trouble here'] [data-mark-line='conflict']", { hasText: "criterion:src/ready#not-written-yet" });
  await expect(item).toContainText("may conflict");
  await expect(item.locator(`a[href$="/n/${encodeURIComponent(`ticket:${on!.from.ticket}`)}"]`)).toBeVisible();
  await expect(item).not.toHaveClass(/text-bad/);
  // Said once, under the title: no list of it further down.
  await expect(page.getByRole("heading", { name: "Other work may conflict" })).toHaveCount(0);
});

/**
 * # Test contract
 *
 * ```json
 * {"test": "a-node-says-its-trouble-under-its-title", "checks": ["web/src/views/Node#findings-shown", "web/src/views/Marks#words", "req:web-findings-in-words"]}
 * ```
 */
test("a node's page says its own findings under its title, new as a warning and inherited never", async ({ page }) => {
  await page.goto("/g/repo/n/req%3Aready-names-its-errors");
  const lines = page.locator("[aria-label='Trouble here'] li");
  await expect(lines).toHaveCount(1);
  const fresh = lines.first();
  await expect(fresh).toHaveAttribute("data-mark-line", "new");
  await expect(fresh).toContainText("dangling-edge");
  await expect(fresh).toContainText("criterion:src/ready#names-errors");
  await expect(fresh.getByText("new", { exact: true })).toHaveClass(/text-bad/);
  // Drawn in the bad colour, not only classed so: the theme's --color-bad.
  await expect(fresh.getByText("new", { exact: true })).toHaveCSS("color", "rgb(224, 108, 117)");
  // Under the title, before the statement and the graph.
  await expect(page.locator("section[aria-label='Graph']")).toBeVisible();
  const order = await page.locator("main h1, main [aria-label='Trouble here'], main section[aria-label='Graph']").evaluateAll((els) => els.map((e) => e.tagName));
  expect(order).toEqual(["H1", "UL", "SECTION"]);
  await expect(page.getByRole("heading", { name: "Lint findings" })).toHaveCount(0);
  // A module with two inherited findings says both, neither as a warning.
  await page.goto(`/g/repo/n/${encodeURIComponent("module:src/loose")}`);
  const old = page.locator("[aria-label='Trouble here'] li");
  await expect(old).toHaveCount(2);
  for (const l of await old.all()) {
    await expect(l).toHaveAttribute("data-mark-line", "inherited");
    await expect(l.locator(".text-bad")).toHaveCount(0);
  }
  // A node with no trouble says nothing.
  await page.goto("/g/repo/n/req%3Aready-needs-commit");
  await expect(page.locator("section[aria-label='Graph']")).toBeVisible();
  await expect(page.locator("[aria-label='Trouble here']")).toHaveCount(0);
});

/**
 * # Test contract
 *
 * ```json
 * {"test": "the-graph-marks-where-trouble-sits", "checks": ["web/src/views/NodeGraph#box-carries-a-mark-dot-per-kind", "web/src/views/NodeGraph#box-border-is-mark-border", "web/src/views/NodeGraph#box-name-ends-with-its-marks", "web/src/views/NodeGraph#legend-shown-above-the-graph", "web/src/views/Marks#mark-dot-draws-each-kind", "web/src/views/Marks#mark-dot-title-says-reasons", "web/src/views/Marks#legend", "req:web-findings-on-graph"]}
 * ```
 */
test("the graph marks each box with its node's trouble, its border in the gravest mark's colour, under a legend", async ({ page }) => {
  await page.goto("/g/repo/n/req%3Aready-names-its-errors");
  const graph = page.getByRole("region", { name: "Graph" });
  await expect(graph.getByLabel("Marks")).toHaveText(/new.*inherited.*recipe due.*may conflict.*omen/);
  const centre = graph.locator("[data-graph-node='req:ready-names-its-errors']");
  await expect(centre).toHaveAttribute("data-marks", "new");
  await expect(centre).toHaveClass(/border-bad/);
  await expect(centre.locator("[data-mark='new']")).toHaveAttribute("title", /^new\n.*names-errors, which do/s);
  await expect(centre.locator("[data-mark='new']")).toHaveClass(/bg-bad/);
  // A box with no trouble carries no mark and keeps its plain border.
  const quiet = graph.locator("[data-graph-node='page:readiness']");
  await expect(quiet).toBeVisible();
  await expect(quiet).not.toHaveAttribute("data-marks", /./);
  await expect(quiet.locator("[data-mark]")).toHaveCount(0);
  // Other work: a dashed ring, and a dashed border in the accent colour; the box's name reads its marks out.
  await page.goto("/g/repo/n/req%3Aready-writes-nothing");
  const other = page.getByRole("region", { name: "Graph" }).locator("[data-graph-node='req:ready-writes-nothing']");
  await expect(other).toHaveAttribute("data-marks", "conflict");
  await expect(other).toHaveClass(/border-accent/);
  await expect(other).toHaveClass(/border-dashed/);
  await expect(other).toHaveAttribute("aria-label", /marked may conflict$/);
  await expect(other.locator("[data-mark='conflict']")).toHaveClass(/border-dashed/);
  // Each mark in the legend has its own shape and colour.
  const legend = page.getByRole("region", { name: "Graph" }).getByLabel("Marks");
  await expect(legend.locator("[data-mark='inherited']")).toHaveClass(/bg-fg-faint/);
  await expect(legend.locator("[data-mark='due']")).toHaveClass(/bg-recipe/);
  await expect(legend.locator("[data-mark='due']")).toHaveClass(/outline-recipe/);
  await expect(legend.locator("[data-mark='omen']")).toHaveClass(/border-fg-dim/);
  await expect(legend.locator("[data-mark='omen']")).toHaveClass(/skew-x/);
  // An inherited finding on a box beside the node: a grey dot and a faint border, nothing red.
  await page.goto(`/g/repo/n/${encodeURIComponent("module:src/ready")}`);
  const recipe = page.getByRole("region", { name: "Graph" }).locator("[data-graph-node='recipe:commit-rule']");
  await expect(recipe).toHaveAttribute("data-marks", "inherited");
  await expect(recipe).toHaveClass(/border-fg-faint/);
  await expect(recipe.locator("[data-mark='inherited']")).toHaveAttribute("title", /^inherited\n/);
  await expect(recipe.locator(".bg-bad, .border-bad")).toHaveCount(0);
});

/**
 * # Test contract
 *
 * ```json
 * {"test": "the-findings-list", "checks": ["web/src/views/Findings#findings-counts-above-the-view", "web/src/views/Findings#list-groups-the-findings", "web/src/views/Findings#list-folds-inherited-and-omens", "web/src/views/Findings#findings-row-links-to-its-node", "req:web-findings-view"]}
 * ```
 */
test("the findings view lists what fails the check first, then recipes due and other work, with the inherited and omens folded", async ({ page }) => {
  type F = { ref: string; baselined: boolean; check: string; reason: string };
  const lint = await tool<{ findings: F[]; advisories: unknown[]; omens: unknown[]; new: number }>(page, "lint");
  await page.goto("/g/repo/findings");
  await expect(page.getByRole("group", { name: "Read as" }).getByRole("button", { name: "List" })).toHaveAttribute("aria-pressed", "true");
  // The count above is the header's: every finding that fails the check, recipes due among them.
  await expect(page.locator("[data-count='new']")).toHaveText(`${lint.new} new`);
  const failing = lint.findings.filter((x) => !x.baselined && x.check !== "due-recipe");
  const fresh = page.locator("[data-group='new'] li");
  await expect(fresh).toHaveCount(failing.length);
  for (const f of failing) {
    const row = page.locator(`[data-group='new'] a[href='/g/repo/n/${encodeURIComponent(f.ref)}']`, { hasText: f.check });
    await expect(row).toHaveAttribute("data-mark-row", "new");
    await expect(row.locator("[data-mark='new']")).toBeVisible();
    await expect(row.getByText("new", { exact: true })).toHaveClass(/text-bad/);
    await expect(row).toContainText(f.reason);
  }
  await expect(page.locator("[data-group='due']")).toContainText("No recipe is due to be judged.");
  const other = page.locator("[data-group='conflict'] li");
  await expect(other).toHaveCount(lint.advisories.length);
  await expect(other.first()).toContainText("ticket 0011");
  await expect(other.first().locator("a")).toHaveAttribute("data-mark-row", "conflict");
  await expect(other.first().getByText("may conflict", { exact: true })).not.toHaveClass(/text-bad/);
  // The inherited and the omens are folded, each with its count.
  const inherited = page.locator("details[data-group='inherited']");
  await expect(inherited.locator("summary")).toHaveText(`${lint.findings.length - lint.new} inherited`);
  await expect(inherited).not.toHaveAttribute("open");
  await inherited.locator("summary").click();
  await expect(inherited.locator("[data-mark-row='inherited']")).toHaveCount(lint.findings.length - lint.new);
  await expect(page.locator("details[data-group='omen'] summary")).toHaveText(`${lint.omens.length} ${lint.omens.length === 1 ? "omen" : "omens"}`);
  // A row opens its node's page.
  await page.locator("[data-group='new'] a", { hasText: "dangling-edge" }).click();
  await expect(page).toHaveURL(/\/n\/req%3Aready-names-its-errors$/);
});

/**
 * # Test contract
 *
 * ```json
 * {"test": "the-findings-tree-remembered", "checks": ["web/src/views/Findings#tree-draws-the-outline", "web/src/views/Findings#tree-shows-trouble-only-by-default", "web/src/views/Findings#tree-lists-what-the-outline-lacks", "web/src/views/Findings#pressing-a-row-selects-its-node", "web/src/views/Findings#selected-node-shown-beside-the-tree", "web/src/views/Findings#switch-remembered", "web/src/marks#trouble-only-cuts-the-outline", "web/src/marks#outside-outline-names-marked-nodes", "req:web-findings-view"]}
 * ```
 */
test("the findings view reads as the marked tree, cut to where trouble is, and the browser remembers the choice", async ({ page }) => {
  await page.goto("/g/repo/findings");
  await page.getByRole("group", { name: "Read as" }).getByRole("button", { name: "Tree" }).click();
  const tree = page.getByRole("navigation", { name: "Marked outline" });
  const marked = tree.locator("[data-tree-node='req:ready-names-its-errors']").first();
  await expect(marked).toHaveAttribute("data-marks", "new");
  await expect(tree.locator("[data-tree-node='page:readiness']").first()).toBeVisible();
  // Only where there is trouble: a requirement with none is not drawn until everything is asked for.
  await expect(tree.locator("[data-tree-node='req:ready-needs-commit']")).toHaveCount(0);
  await page.getByRole("button", { name: "Only where there is trouble" }).click();
  await expect(tree.locator("[data-tree-node='req:ready-needs-commit']").first()).toBeVisible();
  // What the outline does not hold is listed after it.
  await expect(page.getByRole("list", { name: "Not in the outline" }).locator("[data-tree-node='file:src/ready.rs']")).toHaveAttribute("data-marks", "omen");
  // A node chosen is shown beside the tree, with its trouble in words and the way to its page.
  const beside = page.getByRole("complementary", { name: "Selected" });
  await expect(beside).toContainText("Choose a node");
  await marked.click();
  await expect(page).toHaveURL(/\?at=req%3Aready-names-its-errors$/);
  await expect(beside.locator("[data-mark-line='new']")).toContainText("dangling-edge");
  await expect(beside.getByRole("link", { name: "Open its page" })).toHaveAttribute("href", "/g/repo/n/req%3Aready-names-its-errors");
  // Opened again, it reads as the tree; storage gone, as the list.
  await page.goto("/g/repo/findings");
  await expect(page.getByRole("button", { name: "Tree" })).toHaveAttribute("aria-pressed", "true");
  await expect(tree).toBeVisible();
  await page.evaluate(() => localStorage.clear());
  await page.reload();
  await expect(page.getByRole("button", { name: "List" })).toHaveAttribute("aria-pressed", "true");
  await expect(page.locator("[data-group='new']")).toBeVisible();
});

/**
 * # Test contract
 *
 * ```json
 * {"test": "the-findings-switch-without-storage", "checks": ["web/src/views/Findings#switch-remembered"]}
 * ```
 */
test("the findings view opens on the list and its switch still works when the browser's storage refuses", async ({ page }) => {
  await page.addInitScript(() => {
    Storage.prototype.getItem = () => {
      throw new Error("storage refused");
    };
    Storage.prototype.setItem = () => {
      throw new Error("storage refused");
    };
  });
  await page.goto("/g/repo/findings");
  await expect(page.getByRole("button", { name: "List" })).toHaveAttribute("aria-pressed", "true");
  await expect(page.locator("[data-group='new']")).toBeVisible();
  await page.getByRole("button", { name: "Tree" }).click();
  await expect(page.getByRole("button", { name: "Tree" })).toHaveAttribute("aria-pressed", "true");
  await expect(page.getByRole("navigation", { name: "Marked outline" })).toBeVisible();
  await page.getByRole("button", { name: "List" }).click();
  await expect(page.locator("[data-group='new']")).toBeVisible();
});

/**
 * # Test contract
 *
 * ```json
 * {"test": "search-finds-and-enter-opens", "checks": ["web/src/App#search-is-find"]}
 * ```
 */
test("search offers find's results as links, and Enter opens the first", async ({ page }) => {
  await page.goto("/g/repo/");
  const finding = page.waitForRequest((r) => r.url().endsWith("/api/find"));
  await page.getByPlaceholder("Find a page, requirement, module, ticket…").fill("readiness");
  await finding;
  const first = (await tool<{ nodes: { ref: string }[] }>(page, "find", { query: "readiness", limit: 12 })).nodes[0].ref;
  await expect(page.locator(`header a[href="/g/repo/n/${encodeURIComponent(first)}"]`)).toBeVisible();
  await page.getByPlaceholder("Find a page, requirement, module, ticket…").press("Enter");
  await expect(page).toHaveURL(`/g/repo/n/${encodeURIComponent(first)}`);
});

/**
 * # Test contract
 *
 * ```json
 * {"test": "slash-puts-the-cursor-in-search", "checks": ["web/src/App#search-is-find"]}
 * ```
 */
test("/ puts the cursor in the search from anywhere but a field", async ({ page }) => {
  await page.goto("/g/repo/cauldron");
  await expect(page.locator("section[id^='band-']").first()).toBeVisible();
  const search = page.getByPlaceholder("Find a page, requirement, module, ticket…");
  await page.locator("main").click({ position: { x: 5, y: 5 } });
  await page.keyboard.press("/");
  await expect(search).toBeFocused();
  await expect(search).toHaveValue("");
  // In the field, / is typed.
  await page.keyboard.type("a/b");
  await expect(search).toHaveValue("a/b");
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
  await page.goto("/g/repo/");
  const search = page.getByPlaceholder("Find a page, requirement, module, ticket…");
  const early = page.waitForRequest((r) => r.url().endsWith("/api/find") && r.postDataJSON().query === "rea");
  await search.fill("rea");
  await early;
  const answered = page.waitForResponse((r) => r.url().endsWith("/api/find") && r.request().postDataJSON().query === "readiness");
  await search.fill("readiness");
  await answered;
  const first = (await tool<{ nodes: { ref: string }[] }>(page, "find", { query: "readiness", limit: 12 })).nodes[0].ref;
  await expect(page.locator(`header a[href="/g/repo/n/${encodeURIComponent(first)}"]`)).toBeVisible();
  const late = page.waitForResponse((r) => r.url().endsWith("/api/find") && r.request().postDataJSON().query === "rea");
  release();
  await late;
  await expect(page.locator("header a[href='/g/repo/n/page%3Astale']")).toHaveCount(0);
  await expect(page.locator(`header a[href="/g/repo/n/${encodeURIComponent(first)}"]`)).toBeVisible();
  await search.press("Enter");
  await expect(page).toHaveURL(`/g/repo/n/${encodeURIComponent(first)}`);
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
  await page.goto("/g/repo/cauldron");
  const search = page.getByPlaceholder("Find a page, requirement, module, ticket…");
  await search.fill("readiness");
  const first = (await tool<{ nodes: { ref: string }[] }>(page, "find", { query: "readiness", limit: 12 })).nodes[0].ref;
  const hit = page.locator(`header a[href="/g/repo/n/${encodeURIComponent(first)}"]`);
  await expect(hit).toBeVisible();
  const asked = page.waitForRequest((r) => r.url().endsWith("/api/find") && r.postDataJSON().query === "goal");
  await search.fill("goal");
  await expect(hit).toHaveCount(0);
  await search.press("Enter");
  await asked;
  await search.press("Enter");
  await expect(page).toHaveURL("/g/repo/cauldron");
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
    // A grimoire's calls go under its /g/<name>/api/, the daemon's own under /api/.
    const path = new URL(r.url()).pathname.replace(/^\/g\/[^/]+/, "");
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
    if (new URL(r.url()).pathname.endsWith("/api/events")) streams++;
  });
  // A module page mounts a dozen queries: the node, rests_on, rests_on_me, coverage, lint...
  await page.goto("/g/repo/n/module%3Asrc%2Fready");
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
  // In the brewing ticket's Proposed list, the requirement it proposes a change to.
  await page.goto("/g/repo/n/ticket%3A0001");
  const proposed = page.locator("main h2", { hasText: "Proposed" }).locator("xpath=following-sibling::ul[1]");
  const link = proposed.locator('a[href="/g/repo/n/req%3Aready-needs-commit"]');
  await expect(link).toBeVisible();
  await expect(link.locator("xpath=..")).toContainText("req");
  await link.click();
  await expect(page).toHaveURL("/g/repo/n/req%3Aready-needs-commit");
  // The one exception: a bottled ticket's reference to a node that no longer exists keeps its kind tag and is text, marked.
  await page.goto("/g/repo/cauldron");
  await page.getByRole("button", { name: /^Bottled/ }).click();
  await page.getByRole("button", { name: "Show Name the readiness module" }).click();
  const gone = page.locator("#bottled li[data-ticket='ticket:0004'] [data-gone='req:ready-old-name']").first();
  await expect(gone).toHaveText("reqready-old-nameno longer exists");
  await expect(gone.locator("a")).toHaveCount(0);
});

/**
 * # Test contract
 *
 * ```json
 * {"test": "bodies-are-markdown", "checks": ["web/src/components#markdown-bodies"]}
 * ```
 */
test("a page body is rendered as Markdown", async ({ page }) => {
  await page.goto("/g/repo/n/page%3Agoal");
  await expect(page.locator("main strong", { hasText: "What it is." })).toBeVisible();
  await expect(page.locator("main li", { hasText: "A fixture for the UI's behaviour tests." })).toBeVisible();
  await expect(page.locator("main")).not.toContainText("**What it is.**");
});

/**
 * # Test contract
 *
 * ```json
 * {"test": "a-recipes-page-and-where-it-is-placed", "checks": ["web/src/views/Node#recipe-page-parts", "web/src/views/Node#ingredient-page-shows-its-recipe", "web/src/views/Node#every-kind", "web/src/components#markdown-bodies", "web/src/views/Home#page-lists-its-requirements", "web/src/views/Home#page-lists-the-recipes-it-places", "web/src/views/Home#requirement-shows-what-it-names"]}
 * ```
 */
test("a recipe's page says what it is, then its graph, prose, ingredients and files; a page places it", async ({ page }) => {
  await page.goto("/g/repo/n/recipe%3Aready-check");
  const main = page.locator("main");
  await expect(page.locator("h1")).toHaveText("Ready check");
  // What it is, above the graph: its kind, where it is written, and the path not found.
  await expect(main).toContainText("A check recipe, written in src/ready.recipe.md at line 7");
  await expect(main).toContainText("Not found in the checkout: src/report.rs");
  const order = await main.locator("section[aria-label='Graph'], h2, .md").evaluateAll((els) => els.map((e) => (e.matches(".md") ? "prose" : e.matches("section") ? "graph" : e.textContent)));
  // In that order: the graph, then the prose, its ingredients and its files.
  const at = (what: string | null) => order.indexOf(what);
  expect([at("graph"), at("prose"), at("Ingredients"), at("Files")].every((n, i, all) => n >= 0 && (i === 0 || n > all[i - 1])), `${order}`).toBe(true);
  // The prose is Markdown, rendered; each ingredient links to its own page, which names its recipe.
  await expect(main.locator(".md p", { hasText: "for asking whether a build will pass" })).toBeVisible();
  await expect(main.locator(".md strong", { hasText: "Not" })).toBeVisible();
  await expect(main).not.toContainText("**Not**");
  await expect(main).not.toContainText('"kind"');
  await main.getByRole("link", { name: "#answer" }).click();
  await expect(page.locator("h1")).toHaveText("Yes or no, never maybe.");
  await expect(main.locator("p", { hasText: "In" }).getByRole("link", { name: "ready-check" })).toHaveAttribute("href", /recipe%3Aready-check$/);
  // The page that places it lists it with its title, and a requirement it carries counts it as a recipe.
  await page.goto("/g/repo/n/page%3Areadiness");
  await expect(page.locator("#recipes a", { hasText: "ready-check" })).toHaveAttribute("href", /recipe%3Aready-check$/);
  await expect(page.locator("#recipes")).toContainText("Ready check");
  await expect(page.locator("#requirements li", { hasText: "Readiness answers plainly" })).toContainText("1 recipe");
  await expect(page.locator("#requirements li", { hasText: "Readiness answers plainly" })).not.toContainText("uncovered");
});

/** A row on the board, found by its ticket reference. */
function row(page: Page, ref: string) {
  return page.locator(`li[data-ticket="${ref}"]`);
}

/** The ticket references in a band, top to bottom, as the board draws them. */
async function bandRefs(page: Page, band: string): Promise<string[]> {
  return page.locator(`#band-${band} li[data-ticket]`).evaluateAll((ls) => ls.map((l) => l.getAttribute("data-ticket") ?? ""));
}

type BoardAnswer = { tickets: { ref: string; priority: string; waits_on?: { ref: string; status: string }[] }[]; unattached: { ref: string }[]; bottled: { ref: string }[] };

/**
 * # Test contract
 *
 * ```json
 * {"test": "board-shows-readiness-and-the-unattached", "checks": ["web/src/views/Board#changes-by-priority", "web/src/views/Board#unsettled-listed", "web/src/views/Board#reliance-lines", "web/src/views/Board#filters", "web/src/views/Board#nothing-built-on"]}
 * ```
 */
test("the board lists changes with their readiness, then the unattached gaps, with filters", async ({ page }) => {
  await page.goto("/g/repo/cauldron");
  // The per-workspace report waits on the submodules question; Explain readiness waits on nothing.
  await expect(row(page, "ticket:0006")).toContainText("waits on 1");
  // What it waits on shows under its row without opening it: kind, title, own priority and status, and in no band of its own.
  const line = row(page, "ticket:0006").locator("[data-wait='ticket:0005']");
  await expect(line).toBeVisible();
  await expect(line).toContainText("question");
  await expect(line).toContainText("0005");
  await expect(line.getByRole("link", { name: "Should readiness count submodules?" })).toHaveAttribute("href", "/g/repo/n/ticket%3A0005");
  await expect(line).toContainText("later");
  await expect(line).toContainText("inscribed");
  await expect(row(page, "ticket:0006").getByRole("button", { name: /^Hide/ })).toHaveCount(0);
  await expect(page.locator("li[data-ticket='ticket:0005']")).toHaveCount(0);
  await expect(row(page, "ticket:0002")).toContainText("ready");
  await expect(row(page, "ticket:0002").locator("[data-wait]")).toHaveCount(0);
  // The gap no change relies on is unattached; the question a change relies on is not.
  await expect(page.locator("#unattached li[data-ticket='ticket:0003']")).toBeVisible();
  await expect(page.locator("#unattached li[data-ticket='ticket:0005']")).toHaveCount(0);
  // Bottled tickets are hidden until asked for.
  await expect(page.getByRole("link", { name: "Name the readiness module" })).toHaveCount(0);
  await page.getByRole("button", { name: /^Ready/ }).click();
  await expect(row(page, "ticket:0002")).toHaveCount(0);
  await expect(row(page, "ticket:0006")).toBeVisible();
  await page.getByRole("button", { name: /^Ready/ }).click();
  await page.getByRole("button", { name: /^Bottled/ }).click();
  await expect(page.locator("#bottled").getByRole("link", { name: "Name the readiness module" })).toBeVisible();
  // An answered question no change relies on is marked; a gap a change was built on, and a change, are not.
  await expect(page.locator("#bottled li[data-ticket='ticket:0008']")).toContainText("nothing built on it");
  // What the mark means is read out, not only shown on hover.
  await expect(page.locator("#bottled li[data-ticket='ticket:0008']")).toContainText("it needed no work, or its work was never filed");
  await expect(page.locator("#bottled li[data-ticket='ticket:0009']")).not.toContainText("nothing built on it");
  await expect(page.locator("#bottled li[data-ticket='ticket:0010']")).not.toContainText("nothing built on it");
});

/**
 * # Test contract
 *
 * ```json
 * {"test": "a-line-says-whether-what-a-change-relies-on-is-answered", "checks": ["web/src/views/Board#reliance-lines"]}
 * ```
 */
test("a line under a change says whether what it relies on is inscribed, answered or bottled, with its priority", async ({ page }) => {
  // The tool's own answer, with question 0005 set to each status in turn: the store the other specs share is not touched.
  // One handler for the whole test, reading the status of the pass it answers in: swapping handlers between passes
  // left a moment with none, and a refetch an event started then reached the page as the real answer.
  let status: string = "inscribed";
  await page.route("**/api/board", async (route) => {
    const response = await route.fetch();
    const board = (await response.json()) as BoardAnswer;
    for (const t of board.tickets) for (const w of t.waits_on ?? []) if (w.ref === "ticket:0005") w.status = status as typeof w.status;
    await route.fulfill({ response, json: board });
  });
  for (const [now, said, ready] of [
    ["simmering", "answered", true],
    ["bottled", "bottled", true],
    ["inscribed", "inscribed", false],
  ] as const) {
    status = now;
    await page.goto("/g/repo/cauldron");
    const line = row(page, "ticket:0006").locator("[data-wait='ticket:0005']");
    await expect(line, now).toContainText(said);
    await expect(line, now).toContainText("later");
    await expect(row(page, "ticket:0006"), now).toContainText(ready ? "ready" : "waits on 1");
  }
});

/**
 * # Test contract
 *
 * ```json
 * {"test": "a-board-row-opens-into-the-ticket", "checks": ["web/src/views/Board#ticket-in-place", "web/src/components#shows-the-line", "web/src/components#copy-reads-copied", "web/src/components#copy-refused-says-by-hand"]}
 * ```
 */
test("a board row opens into the whole ticket, with what it waits on and its next step for an agent", async ({ page }) => {
  await page.goto("/g/repo/cauldron");
  await page.getByRole("button", { name: "Show Report readiness per workspace" }).click();
  const opened = row(page, "ticket:0006");
  // The panel's heading, by role: the row's badge reads "waits on 1" before the panel has loaded at all.
  await expect(opened.getByRole("heading", { name: "Waits on" })).toBeVisible();
  // The panel's own list, past the line under the row that names the question too.
  const panel = opened.locator("li:not([data-wait])", { has: page.getByRole("link", { name: "Should readiness count submodules?" }) });
  await expect(panel.getByRole("link", { name: "Should readiness count submodules?" })).toBeVisible();
  // The question's own line says it is inscribed.
  await expect(panel).toContainText("inscribed");
  await expect(opened).toContainText("Needs refinement: it waits on whether submodules count.");
  await expect(opened.locator("a[href='/g/repo/n/module%3Asrc%2Fready']").first()).toBeVisible();
  // Its next step is a line for the agent, never a start or a close.
  await expect(opened.locator("[data-handoff]")).toHaveText('Brew ticket 0006 ("Report readiness per workspace") with the grimoire skill.');
  await expect(opened.getByRole("button", { name: "Start", exact: true })).toHaveCount(0);
  await expect(opened.getByRole("link", { name: "Open the ticket's page" })).toHaveAttribute("href", "/g/repo/n/ticket%3A0006");
  // A gap opens the same way, its line asking for the options to be laid out, and nothing closes it from here.
  await page.getByRole("button", { name: "Show Readiness ignores submodules" }).click();
  const gap = row(page, "ticket:0003");
  await expect(gap).toContainText("A gap, off the first screen.");
  await expect(gap.locator("[data-handoff]")).toHaveText('Resolve the gap 0003 ("Readiness ignores submodules") with the grimoire skill: lay out the options for me to choose from.');
  await expect(gap.getByLabel("Decision")).toHaveCount(0);
  await expect(gap.getByRole("button", { name: "Close", exact: true })).toHaveCount(0);
  // Copied from the row as from the page; and where the browser refuses, it says to copy it by hand.
  await page.context().grantPermissions(["clipboard-read", "clipboard-write"]);
  await opened.getByRole("button", { name: /^Copy: Brew ticket 0006/ }).click();
  await expect(opened.getByRole("button", { name: /^Copy: / })).toHaveText("Copied");
  expect(await page.evaluate(() => navigator.clipboard.readText())).toBe('Brew ticket 0006 ("Report readiness per workspace") with the grimoire skill.');
  await page.evaluate(() => {
    navigator.clipboard.writeText = () => Promise.reject(new Error("refused"));
  });
  await gap.getByRole("button", { name: /^Copy: Resolve the gap 0003/ }).click();
  await expect(gap.getByRole("alert")).toHaveText("The browser would not copy it: select the line and copy it by hand.");
  await expect(gap.getByRole("button", { name: /^Copy: / })).toHaveText("Copy");
  // Closed where it is decided, through the tools, the row leaves the unsettled at once; the specs after this one rely on it bottled.
  await tool(page, "close_ticket", { ref: "ticket:0003", decision: { text: "Submodules are out of scope for readiness." } });
  await expect(page.locator("#unattached li[data-ticket='ticket:0003']")).toHaveCount(0);
  expect((await tool<{ node: { status: string } }>(page, "get", { ref: "ticket:0003" })).node.status).toBe("bottled");
  // A bottled ticket opens the same way: what it touched that still exists is a link, what no longer exists is marked and is none.
  await page.getByRole("button", { name: /^Bottled/ }).click();
  await page.getByRole("button", { name: "Show Name the readiness module" }).click();
  const closed = page.locator("#bottled li[data-ticket='ticket:0004']");
  await expect(closed.locator("a[href='/g/repo/n/module%3Asrc%2Fready']").first()).toBeVisible();
  // One it touched and one it relied on, each listed under its own heading.
  for (const [ref, href] of [
    ["req:ready-old-name", "/g/repo/n/req%3Aready-old-name"],
    ["page:old-naming", "/g/repo/n/page%3Aold-naming"],
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
 * {"test": "board-keeps-the-tools-order", "checks": ["web/src/views/Board#order-not-resorted", "web/src/views/Board#drag-within-band", "web/src/views/Board#drop-in-another-band", "web/src/views/Board#keyboard-move", "web/src/views/Board#moves-one-at-a-time", "web/src/views/Board#moved-among-own-list-only", "web/src/views/Board#focus-follows"]}
 * ```
 */
test("the board keeps the tool's order, and a change is moved by dragging or from the keyboard", async ({ page }) => {
  await page.goto("/g/repo/cauldron");
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
  // Drawn in its new place, its handle has focus still.
  await expect.poll(() => bandRefs(page, "later")).toEqual(["ticket:0007", "ticket:0006"]);
  await expect(grip("ticket:0006")).toBeFocused();

  // Two quick presses move it two places, the second from where the first left it:
  // up within later, then past the edge into the end of high. No handle is focused anew from here on.
  await page.keyboard.press("Alt+ArrowUp");
  await page.keyboard.press("Alt+ArrowUp");
  await expect.poll(async () => (await answer()).tickets.find((x) => x.ref === "ticket:0006")?.priority).toBe("high");
  await expect.poll(() => inBand("high")).toEqual(["ticket:0002", "ticket:0006"]);
  expect(await inBand("later")).toEqual(["ticket:0007"]);
  // Drawn again in another band, where its row is a new one: focus is on its handle there,
  // so the next press moves it without the handle being found again.
  await expect.poll(() => bandRefs(page, "high")).toEqual(["ticket:0002", "ticket:0006"]);
  await expect(grip("ticket:0006")).toBeFocused();
  // Back down past the edge: the start of later.
  await page.keyboard.press("Alt+ArrowDown");
  await expect.poll(() => inBand("later")).toEqual(["ticket:0006", "ticket:0007"]);
  await expect.poll(() => bandRefs(page, "later")).toEqual(["ticket:0006", "ticket:0007"]);
  await expect(grip("ticket:0006")).toBeFocused();

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
 * {"test": "an-adopted-requirement-reads-as-described", "checks": ["web/src/views/Node#adoption-marked", "web/src/views/Node#described-requirement-page-marked", "web/src/views/Home#described-requirement-marked", "web/src/views/Home#described-rollup-adds-a-bar"]}
 * ```
 */
test("an adopted requirement reads as described, on its page and where a page lists it", async ({ page }) => {
  // The tools' own answers, with ready-needs-commit adopted: the store the other specs share is not touched.
  const adoption = { date: "2026-10-04", text: "Read off the code at adoption.", adopted: true };
  await page.route("**/api/get", async (route) => {
    const response = await route.fetch();
    const body = await response.json();
    if (body.node?.ref === "req:ready-needs-commit") body.node.decisions = [adoption];
    await route.fulfill({ response, json: body });
  });
  await page.route("**/api/outline", async (route) => {
    const response = await route.fetch();
    const body = await response.json();
    const walk = (t: { ref: string; rollup: { described: number; requirements: number }; children?: unknown[] }, under: boolean): number => {
      const own = t.ref === "req:ready-needs-commit" ? 1 : 0;
      const below = (t.children ?? []).reduce((n: number, c) => n + walk(c as never, under), 0);
      t.rollup.described = own + below;
      return t.rollup.described;
    };
    for (const root of body.roots) walk(root, false);
    await route.fulfill({ response, json: body });
  });
  await page.goto("/g/repo/n/req%3Aready-needs-commit");
  await expect(page.locator("[data-described]")).toContainText("described");
  await expect(page.locator("main")).toContainText("d1 · adoption");
  await page.goto("/g/repo/n/page%3Areadiness");
  const req = (slug: string) => page.locator("main li", { has: page.locator(`a[href='/g/repo/n/req%3A${slug}']`) });
  await expect(req("ready-needs-commit")).toContainText("described");
  await expect(req("not-built")).not.toContainText("described");
  // Decided is the outline's count: none of the fixture's requirements has a decision of its own, so none is decided.
  const decided = page.getByRole("meter", { name: "Requirements decided" }).first();
  await expect(decided).toBeVisible();
  await expect(decided).toHaveAttribute("aria-valuenow", "0");
});

/**
 * # Test contract
 *
 * ```json
 * {"test": "a-drop-the-rule-raises-back-stays", "checks": ["web/src/views/Board#drop-raised-back-stays"]}
 * ```
 */
test("a change the priority rule raises back stays where the tool put it, and the board says why", async ({ page }) => {
  await page.goto("/g/repo/cauldron");
  const grip = (ref: string) => row(page, ref).getByRole("button", { name: /^Move / });
  // update_ticket answers as the rule would when another change holds what 0002 relies on at high: 0002 raised back.
  const reorders: unknown[] = [];
  await page.route("**/api/update_ticket", async (route) => {
    const node = (await tool<{ node: unknown }>(page, "get", { ref: "ticket:0002" })).node;
    await route.fulfill({ json: { node, warnings: [], baselined: 0, reprioritised: [{ ref: "ticket:0002", title: "Explain readiness", kind: "change", priority: "high", status: "inscribed" }] } });
  });
  await page.route("**/api/reorder", async (route) => {
    reorders.push(route.request().postDataJSON());
    await route.continue();
  });
  await grip("ticket:0002").focus();
  await page.keyboard.press("Alt+ArrowDown");
  await expect(page.locator("[aria-live]")).toContainText("Explain readiness stays in high: a gap or question it relies on is held there by another change.");
  expect(reorders).toEqual([]);
  expect((await tool<BoardAnswer>(page, "board")).tickets.find((x) => x.ref === "ticket:0002")?.priority).toBe("high");
});

/**
 * # Test contract
 *
 * ```json
 * {"test": "a-drop-in-another-band-announces-what-moved-with-it", "checks": ["web/src/views/Board#drop-announces-carried"]}
 * ```
 */
test("a change dropped in another band announces the gaps and questions moved with it", async ({ page }) => {
  await page.goto("/g/repo/cauldron");
  const grip = (ref: string) => row(page, ref).getByRole("button", { name: /^Move / });
  // update_ticket answers as the rule would when 0002 carries the question it relies on into the band with it.
  const summary = (ref: string, title: string, kind: string) => ({ ref, title, kind, priority: "later", status: "inscribed" });
  await page.route("**/api/update_ticket", async (route) => {
    const node = (await tool<{ node: unknown }>(page, "get", { ref: "ticket:0002" })).node;
    const reprioritised = [summary("ticket:0002", "Explain readiness", "change"), summary("ticket:0005", "Should readiness count submodules?", "question")];
    await route.fulfill({ json: { node, warnings: [], baselined: 0, reprioritised } });
  });
  // The order is not written, so the shared store is left as the other specs expect it.
  await page.route("**/api/reorder", (route) => route.fulfill({ json: {} }));
  await grip("ticket:0002").focus();
  await page.keyboard.press("Alt+ArrowDown");
  const said = page.locator("[aria-live]");
  await expect(said).toContainText("Explain readiness moved to position");
  await expect(said).toContainText("in later.");
  await expect(said).toContainText("Moved with it: Should readiness count submodules? to later.");
  await expect(said).not.toContainText("Moved with it: Explain readiness");
});

/**
 * # Test contract
 *
 * ```json
 * {"test": "an-answer-waiting-for-its-change-hands-nothing-over", "checks": ["web/src/views/Node#answer-waiting-no-line", "web/src/components#waiting-answer-no-line"]}
 * ```
 */
test("a gap waiting for the change that applies it shows no line to hand over and says it is bottled with that change", async ({ page }) => {
  // A gap of the test's own, deleted at the end, so it holds whatever the specs before it closed.
  const ref = (await tool<{ node: { ref: string } }>(page, "inscribe_ticket", { title: "Answered, waiting", kind: "gap", priority: "later", touches: [], body: "Waits for its change." })).node.ref;
  try {
    // The tool's own answer, with the gap simmering with a decision: the store the other specs share is not touched.
    await page.route("**/api/get", async (route) => {
      const response = await route.fetch();
      const body = await response.json();
      if (body.node?.ref === ref) {
        body.node.status = "simmering";
        body.node.decision = { date: "2026-01-20", text: "Decided, and waiting." };
      }
      await route.fulfill({ response, json: body });
    });
    const said = "Answered: bottled when the change applying it is.";
    // On its page.
    await page.goto(`/g/repo/n/${encodeURIComponent(ref)}`);
    const main = page.locator("main");
    await expect(main).toContainText("simmering");
    await expect(main).toContainText("written, waiting for its change to be bottled");
    await expect(main.getByText(said)).toBeVisible();
    await expect(main.locator("[data-handoff]")).toHaveCount(0);
    await expect(main.getByRole("button", { name: /^Copy: / })).toHaveCount(0);
    // Opened on the board, where the same component draws it.
    await page.goto("/g/repo/cauldron");
    await page.getByRole("button", { name: "Show Answered, waiting" }).click();
    const opened = row(page, ref);
    await expect(opened.getByText(said)).toBeVisible();
    await expect(opened.locator("[data-handoff]")).toHaveCount(0);
    await expect(opened.getByRole("button", { name: /^Copy: / })).toHaveCount(0);
  } finally {
    await tool(page, "delete_node", { ref });
  }
});

/**
 * # Test contract
 *
 * ```json
 * {"test": "summary-reads-in-the-roots-order", "checks": ["web/src/views/Home#story-order"]}
 * ```
 */
test("the root shows the grimoire's own level: its body, its bars and its pages as cards in link order", async ({ page }) => {
  await page.goto("/g/repo/");
  await expect(page.locator("#pages")).toBeVisible();
  const root = await tool<{ node: { links: string[] } }>(page, "get", { ref: "page:overview" });
  const cards = await linkedRefs(page, "#pages");
  expect(cards).toEqual(root.node.links.filter((l) => l.startsWith("page:")));
  // Each card carries its page's rollup, and the whole grimoire's rollup heads the page.
  const outline = await tool<Outline>(page, "outline", { depth: 64 });
  const architecture = find(outline.roots, "page:architecture")!.rollup;
  const card = page.locator("#pages a[href='/g/repo/n/page%3Aarchitecture']");
  await expect(card).toContainText(`${architecture.contracted} of ${architecture.modules}`);
  await expect(card).toContainText(`${architecture.tested} of ${architecture.criteria}`);
  // A card shows one paragraph of its page: the first longer than forty characters, past a short bold opening line,
  // or the first when none is, its bold, code, list and link marks taken out.
  const opening = (slug: string) => page.locator(`#pages a[href='/g/repo/n/page%3A${slug}'] p`);
  await expect(opening("constraints")).toHaveText("The ready module writes nothing, whichever part of the project is changed.");
  await expect(opening("goal")).toHaveText("What it is.");
  const whole = outline.roots[0].rollup;
  await expect(page.getByRole("meter", { name: "Requirements held" }).first()).toHaveAttribute("aria-valuemax", String(whole.requirements));
  // With a second root, the root is still the first root's level: the second is no card and none of its pages shows.
  expect(outline.roots.map((t) => t.ref)).toEqual(["page:overview", "page:handbook"]);
  await expect(page.locator("main h1").first()).toHaveText(outline.roots[0].title);
  expect(cards).not.toContain("page:handbook");
  await expect(page.locator(`main a[href^="/g/repo/n/page%3A"]:not(${rail} a)`)).toHaveCount(cards.length);
  // No story page is opened out at the root: none of its requirements or modules is listed. Said last, with the whole root drawn.
  expect((await linkedRefs(page)).some((r) => r.startsWith("req:") || r.startsWith("module:"))).toBe(false);
});

/**
 * # Test contract
 *
 * ```json
 * {"test": "contracts-listed-once-by-area", "checks": ["web/src/views/Home#page-body-rendered-with-bars", "web/src/views/Home#page-lists-each-module-once", "web/src/views/Home#long-body-folded-to-five-lines", "web/src/views/Home#files-and-declarations-not-shown"]}
 * ```
 */
test("a page lists each module it links once, with its criteria beneath it", async ({ page }) => {
  await page.goto("/g/repo/n/page%3Areadiness");
  const modules = page.locator("#modules");
  await expect(modules).toBeVisible();
  const listed = async (prefix: string) => (await linkedRefs(page, "#modules")).filter((r) => r.startsWith(prefix)).sort();
  await expect.poll(() => listed("module:")).toEqual(["module:src", "module:src/ready"]);
  await expect.poll(() => listed("criterion:src/ready#")).toEqual(["criterion:src/ready#needs-commit", "criterion:src/ready#reads-only"]);
  expect((await linkedRefs(page)).some((r) => r.startsWith("file:") || r.startsWith("decl:")), "no files or declarations").toBe(false);
  await expect(page.locator("main")).toContainText("Whether a workspace is ready.");
  // A long body shows five lines and a Show more; a short one shows whole.
  const more = page.getByRole("button", { name: "Show more" });
  await expect(page.locator("[data-folded]")).toHaveCount(1);
  await more.click();
  await expect(page.locator("[data-folded]")).toHaveCount(0);
  await expect(page.getByRole("button", { name: "Show less" })).toBeVisible();
  await page.goto("/g/repo/n/page%3Agoal");
  await expect(page.locator("main strong", { hasText: "What it is." })).toBeVisible();
  await expect(page.getByRole("button", { name: "Show more" })).toHaveCount(0);
});

/**
 * # Test contract
 *
 * ```json
 * {"test": "rail-is-the-outline", "checks": ["web/src/views/Home#rail-reads-pages-only-outline", "web/src/views/Home#rail-lists-every-root", "web/src/views/Home#pane-drawn-from-outline-rooted-at-page"]}
 * ```
 */
test("the rail is the outline's pages, ending with the unplaced modules", async ({ page }) => {
  const asked: object[] = [];
  page.on("request", (r) => {
    // What the view asks, without the checkout every call names.
    if (r.url().endsWith("/api/outline")) {
      const input = r.postDataJSON() as Record<string, unknown>;
      delete input.checkout;
      asked.push(input);
    }
  });
  await page.goto("/g/repo/");
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
      return `${d} ${href === "/g/repo/" || href === "/g/repo" ? "root" : decodeURIComponent(href.slice("/g/repo/n/".length))}`;
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
  await expect(page.locator("main a[href='/g/repo/n/module%3Asrc%2Floose']")).toBeVisible();
  await expect(page.locator("main")).toContainText("no contract");
});

/**
 * # Test contract
 *
 * ```json
 * {"test": "a-page-under-a-later-root-opens-from-it", "checks": ["web/src/views/Home#page-opens-with-breadcrumb-from-its-root"]}
 * ```
 */
test("a page under a later root opens beside the rail, its breadcrumb from that root", async ({ page }) => {
  await page.goto("/g/repo/n/page%3Arunbook");
  await expect(page.locator("main h1").first()).toHaveText("Runbook");
  const crumbs = page.getByRole("navigation", { name: "Breadcrumb" });
  await expect(crumbs).toHaveText("Handbook /");
  await expect(crumbs.getByRole("link", { name: "Handbook" })).toHaveAttribute("href", "/g/repo/n/page%3Ahandbook");
  await expect(page.locator(rail).locator("a[aria-current='page']")).toHaveAttribute("href", "/g/repo/n/page%3Arunbook");
  // The later root is its own page beside the rail, with no breadcrumb.
  await crumbs.getByRole("link", { name: "Handbook" }).click();
  await expect(page.locator("main h1").first()).toHaveText("Handbook");
  await expect(page.getByRole("navigation", { name: "Breadcrumb" })).toHaveCount(0);
  await expect(page.locator("#pages a[href='/g/repo/n/page%3Arunbook']")).toBeVisible();
});

/**
 * # Test contract
 *
 * ```json
 * {"test": "a-root-or-a-page-reached-twice-is-one-entry", "checks": ["web/src/views/Home#rail-lists-every-root", "web/src/views/Home#page-opens-with-breadcrumb-from-its-root"]}
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
  await page.goto("/g/repo/n/page%3Ahandbook");
  await expect(page.locator("main h1").first()).toHaveText("Handbook");
  await expect(page.locator(`${rail} a[href='/g/repo/n/page%3Ahandbook']`)).toHaveCount(2);
  await expect(current).toHaveCount(1);
  // The breadcrumb is made from the rail's answer: its absence is said once that is here.
  await expect(page.getByRole("navigation", { name: "Breadcrumb" })).toHaveCount(0);
  // The current entry is the top-level root, not the one nested under overview.
  expect(await current.evaluate((a) => a.closest("ul")?.parentElement?.tagName)).toBe("NAV");
  await page.goto("/g/repo/n/page%3Agoal");
  await expect(page.getByRole("navigation", { name: "Breadcrumb" })).toHaveText("Fixture /");
  await expect(page.locator(`${rail} a[href='/g/repo/n/page%3Agoal']`)).toHaveCount(3);
  await expect(current).toHaveCount(1);
  await expect(current).toHaveAttribute("href", "/g/repo/n/page%3Agoal");
  // The one under the first root, which the breadcrumb names: a direct child of the root's list.
  expect(await current.evaluate((a) => a.closest("ul")?.closest("li")?.querySelector("a")?.getAttribute("href"))).toBe("/g/repo");
});

/**
 * # Test contract
 *
 * ```json
 * {"test": "a-page-pane-never-shows-another-page", "checks": ["web/src/views/Home#pane-drawn-from-outline-rooted-at-page"]}
 * ```
 */
test("going from one page to another never shows the first under the second", async ({ page }) => {
  // The outline rooted at architecture is held, so the pane can only be the old page's or loading.
  await page.route("**/api/outline", async (route) => {
    if (route.request().postDataJSON().root !== "page:architecture") return route.continue();
    await new Promise(() => {});
  });
  await page.goto("/g/repo/n/page%3Agoal");
  const heading = page.locator("main h1").first();
  await expect(heading).toHaveText("Goal");
  const asked = page.waitForRequest((r) => r.url().endsWith("/api/outline") && r.postDataJSON().root === "page:architecture");
  await page.locator(rail).getByRole("link", { name: "Architecture" }).click();
  await asked;
  await expect(page).toHaveURL("/g/repo/n/page%3Aarchitecture");
  await expect(page.locator("main")).toContainText("Loading…");
  await expect(page.locator("main h1", { hasText: "Goal" })).toHaveCount(0);
});

/**
 * # Test contract
 *
 * ```json
 * {"test": "marks-new-inherited-and-untested", "checks": ["web/src/views/Home#rail-shows-findings-apart", "web/src/views/Home#criterion-marked-from-checks", "web/src/views/Home#requirement-shows-what-it-names", "web/src/components#badge-tone", "req:web-badge-tone"]}
 * ```
 */
test("pages mark new findings apart from inherited ones, and criteria and requirements by what holds them", async ({ page }) => {
  await page.goto("/g/repo/n/page%3Areadiness");
  const outline = await tool<Outline>(page, "outline", { depth: 64 });
  const area = find(outline.roots, "page:readiness")!;
  expect(area.baselined, "the fixture's adopted findings").toBeGreaterThan(0);
  expect(area.findings - area.baselined, "the requirement added after adopting").toBe(1);
  const inRail = page.locator(rail).locator("a[href='/g/repo/n/page%3Areadiness']");
  await expect(inRail).toContainText("1 new");
  // The count's badge is drawn in the bad colour, not the dim foreground its base once set beside it (req:web-badge-tone).
  await expect(inRail.getByText("1 new", { exact: true })).toHaveCSS("color", "rgb(224, 108, 117)");
  await expect(inRail).toContainText(String(area.baselined));
  const chip = (id: string) => page.locator(`#modules a[href='/g/repo/n/criterion%3Asrc%2Fready%23${id}']`);
  await expect(chip("reads-only")).toContainText("untested");
  await expect(chip("reads-only")).toContainText("inherited");
  await expect(chip("needs-commit")).not.toContainText("untested");
  const req = (slug: string) => page.locator("main li", { has: page.locator(`a[href='/g/repo/n/req%3A${slug}']`) });
  await expect(req("ready-needs-commit")).toContainText("1 criterion");
  await expect(req("not-built")).toContainText("uncovered");
  // A constraint names no criterion; a test holds it, so it is not a warning.
  await page.goto("/g/repo/n/page%3Aconstraints");
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
  const nodes = ROUTES.filter((r) => r.path.startsWith("/g/repo/n/"));
  const kinds = [...new Set(nodes.map((r) => decodeURIComponent(r.path.slice("/g/repo/n/".length)).split(":")[0]))].sort();
  expect(kinds, "a node page of every kind").toEqual(["criterion", "decl", "file", "module", "page", "recipe", "req", "test", "ticket"]);
  // An ingredient is of the kind recipe: its route is there too, as one of the recipe's.
  expect(nodes.some((r) => decodeURIComponent(r.path).includes("recipe:ready-check#")), "an ingredient's page").toBe(true);
  for (const { path } of nodes) {
    await page.goto(path);
    await expect(page.locator("h1"), path).toBeVisible();
    // The node itself is found; a reason quoting what does not exist, as an advisory's does, is not this.
    const ref = decodeURIComponent(path.slice("/g/repo/n/".length));
    await expect(page.locator("main"), path).not.toContainText(`${ref} does not exist`);
  }
});

/**
 * # Test contract
 *
 * ```json
 * {"test": "a-ticket-hands-its-next-step-to-an-agent", "checks": ["web/src/views/Node#no-start-or-close", "web/src/views/Node#next-step-line", "web/src/components#shows-the-line", "web/src/components#copy-reads-copied", "web/src/components#bottled-shows-nothing", "web/src/components#calls-no-tool"]}
 * ```
 */
test("a ticket's page hands its next step to an agent, and starts and closes nothing itself", async ({ page }) => {
  const status = async () => (await tool<{ node: { status: string } }>(page, "get", { ref: "ticket:0002" })).node.status;
  const line = page.locator("main [data-handoff]");
  await page.goto("/g/repo/n/ticket%3A0002");
  await expect(page.getByRole("heading", { name: "Next step" })).toBeVisible();
  await expect(line).toHaveText('Brew ticket 0002 ("Explain readiness") with the grimoire skill.');
  for (const name of ["Start", "Brew", "Close", "Bottle"]) await expect(page.getByRole("button", { name, exact: true })).toHaveCount(0);
  await expect(page.getByLabel("Decision")).toHaveCount(0);

  // Copy puts the line on the clipboard, says so, and writes nothing.
  await page.context().grantPermissions(["clipboard-read", "clipboard-write"]);
  await page.getByRole("button", { name: /^Copy: Brew ticket 0002/ }).click();
  await expect(page.getByRole("button", { name: /^Copy: / })).toHaveText("Copied");
  await expect(page.locator("main [role=status]")).toHaveText("Copied to the clipboard.");
  expect(await page.evaluate(() => navigator.clipboard.readText())).toBe('Brew ticket 0002 ("Explain readiness") with the grimoire skill.');
  expect(await status()).toBe("inscribed");

  // Brewed and bottled where the work is done, through the tools: the page follows, with the next step for each.
  await tool(page, "brew_ticket", { ref: "ticket:0002" });
  await expect(line).toHaveText('Continue ticket 0002 ("Explain readiness") with the grimoire skill.');
  // A new line has not been copied: the button says Copy again.
  await expect(page.getByRole("button", { name: /^Copy: Continue ticket 0002/ })).toHaveText("Copy");
  await tool(page, "close_ticket", { ref: "ticket:0002", decision: { text: "Explained in the area page." } });
  await expect.poll(status).toBe("bottled");
  await expect(page.getByRole("heading", { name: "Next step" })).toHaveCount(0);
  await expect(line).toHaveCount(0);

  // A bottled ticket has no next step.
  await page.goto("/g/repo/n/ticket%3A0004");
  await expect(page.getByRole("heading", { name: "Name the readiness module" })).toBeVisible();
  await expect(line).toHaveCount(0);
});

/**
 * # Test contract
 *
 * ```json
 * {"test": "a-bottled-tickets-gone-reference-is-marked", "checks": ["web/src/views/Node#stale-reference-marked", "web/src/views/NodeGraph#gone-reference-drawn-dimmed"]}
 * ```
 */
test("a bottled ticket's reference that leads nowhere is drawn marked and not a link; an unbottled ticket's stays a link", async ({ page }) => {
  // The fixture's bottled ticket touches a module that exists and a requirement that does not, and relied on a page that is gone.
  const closed = await tool<{ node: { status: string; touches: string[]; out: { ref: string; title: string | null }[] } }>(page, "get", { ref: "ticket:0004" });
  expect(closed.node.status).toBe("bottled");
  expect(closed.node.out.find((e) => e.ref === "req:ready-old-name")?.title, "get gives the missing end no title").toBeNull();
  await page.goto("/g/repo/n/ticket%3A0004");
  await expect(page.getByRole("heading", { name: "Name the readiness module" })).toBeVisible();
  // In the graph: a box that says so and is no link, beside the module's, which is one.
  const graph = page.getByRole("region", { name: "Graph" });
  const box = graph.locator("[data-graph-node='req:ready-old-name']");
  await expect(box).toHaveText("reqready-old-name");
  await expect(box).toHaveAttribute("aria-label", "req ready-old-name, no longer exists, under Name the readiness module");
  expect(await box.evaluate((e) => e.tagName)).toBe("SPAN");
  expect(await graph.locator("[data-graph-node='page:old-naming']").evaluate((e) => e.tagName)).toBe("SPAN");
  expect(await graph.locator("[data-graph-node='module:src/ready']").evaluate((e) => e.tagName)).toBe("A");
  const refs = await linkedRefs(page);
  expect(refs).not.toContain("req:ready-old-name");
  expect(refs).not.toContain("page:old-naming");
  expect(refs, "what still exists is a link as before").toContain("module:src/ready");
  // The same answer for a ticket still inscribed: the check reports that one, and the graph links it as before.
  await page.route("**/api/get", async (route) => {
    if (route.request().postDataJSON().ref !== "ticket:0004") return route.continue();
    await route.fulfill({ json: { ...closed, node: { ...closed.node, status: "inscribed", decision: null } } });
  });
  await page.reload();
  await expect(page.getByRole("heading", { name: "Name the readiness module" })).toBeVisible();
  await expect.poll(async () => (await linkedRefs(page)).includes("req:ready-old-name"), "the graph links it").toBe(true);
  await expect.poll(async () => (await linkedRefs(page)).includes("page:old-naming")).toBe(true);
  await expect(page.locator("main [data-gone]")).toHaveCount(0);
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
  const filed = await tool<{ node: { ref: string } }>(page, "inscribe_ticket", { title: "A hostile body", kind: "gap", priority: "later", touches: [], body });
  await page.goto(`/g/repo/n/${encodeURIComponent(filed.node.ref)}`);
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
 * {"test": "a-real-drag-into-an-empty-band", "checks": ["web/src/views/Board#drop-in-another-band"]}
 * ```
 */
test("a change dragged with the mouse into an empty band lands there and shows at once", async ({ page }) => {
  // A real pointer drag, not dispatched events: a browser abandons a drag whose
  // layout changes inside dragstart, and an empty band appears when one begins.
  await page.goto("/g/repo/cauldron");
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
 * {"test": "drill-down-ends-at-the-annotated-source", "checks": ["web/src/views/Source#source-shows-numbered-lines", "web/src/views/Source#source-is-syntax-highlighted", "web/src/views/Source#long-lines-wrap", "web/src/views/Source#contract-json-laid-out-under-its-line", "web/src/views/Source#annotated-lines-link-their-nodes", "web/src/views/Source#line-anchor-scrolls-and-marks", "web/src/views/Node#pages-link-to-their-line", "web/src/views/Node#file-page-is-annotated-source"]}
 * ```
 */
test("drilling down from a declaration ends at its line in the annotated source", async ({ page }) => {
  // From the declaration's page to its line in the file.
  await page.goto("/g/repo/n/decl%3Asrc%2Fready.rs%23ready");
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
  await expect(contract.locator("a[href='/g/repo/n/module%3Asrc%2Fready']")).toBeVisible();
  await expect(contract.locator("a[href='/g/repo/n/criterion%3Asrc%2Fready%23needs-commit']")).toBeVisible();
  await expect(contract.locator("a[href='/g/repo/n/criterion%3Asrc%2Fready%23reads-only']")).toBeVisible();
  const tests = source.locator("[data-annotation='test']");
  await expect(tests).toHaveCount(2);
  await expect(tests.locator("a[href='/g/repo/n/criterion%3Asrc%2Fready%23needs-commit']")).toBeVisible();
  await expect(tests.locator("a[href='/g/repo/n/req%3Aready-writes-nothing']")).toBeVisible();
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
  await page.goto("/g/repo/n/criterion%3Asrc%2Fready%23needs-commit");
  await expect(page.locator("a[href='/g/repo/n/file%3Asrc%2Fready.rs#L1']")).toBeVisible();
  await page.goto("/g/repo/n/module%3Asrc%2Fready");
  await expect(page.locator("a[href='/g/repo/n/file%3Asrc%2Fready.rs#L1']")).toBeVisible();
  await page.goto("/g/repo/n/test%3Asrc%2Fready.rs%23ready-needs-a-commit");
  await expect(page.locator("a[href='/g/repo/n/file%3Asrc%2Fready.rs#L13']")).toBeVisible();
});

/**
 * # Test contract
 *
 * ```json
 * {"test": "a-file-waits-for-its-highlighting-and-falls-back-to-plain", "checks": ["web/src/views/Source#source-says-loading-until-highlighted", "web/src/views/Source#source-plain-when-grammar-fails", "web/src/views/Source#annotated-lines-link-their-nodes", "web/src/views/Source#line-anchor-scrolls-and-marks"]}
 * ```
 */
test("a file says Loading until it is highlighted, and is shown plain when the highlighter cannot load", async ({ page }) => {
  // Hold the highlighter's WASM, then refuse it.
  let release: () => void = () => {};
  const held = new Promise<void>((resolve) => (release = resolve));
  await page.route("**/assets/onig-*.wasm", async (route) => {
    await held;
    await route.abort();
  });
  await page.goto("/g/repo/n/file%3Asrc%2Fready.rs#L7");
  await expect(page.getByText("Loading…")).toBeVisible();
  await expect(page.getByRole("region", { name: "Source of src/ready.rs" })).toHaveCount(0);
  release();
  // Plain: every line, numbered, marked and linked, with no tone.
  const region = page.getByRole("region", { name: "Source of src/ready.rs" });
  await expect(region.locator("tr#L7")).toContainText("pub fn ready() -> bool");
  await expect(page.locator("tr[aria-current='location']")).toHaveAttribute("id", "L7");
  await expect(region.locator("[data-annotation='contract'] a[href='/g/repo/n/module%3Asrc%2Fready']")).toBeVisible();
  await expect(region.locator("[data-tone]")).toHaveCount(0);
  await expect(page.getByText("Loading…")).toHaveCount(0);
});

/**
 * # Test contract
 *
 * ```json
 * {"test": "a-ticket-page-draws-its-graph", "checks": ["web/src/views/NodeGraph#graph-draws-what-it-rests-on-to-the-right", "web/src/views/NodeGraph#box-is-a-link-naming-how-it-connects"]}
 * ```
 */
test("a ticket's page draws what it rests on, and a node in the graph opens its page", async ({ page }) => {
  await page.goto("/g/repo/n/ticket%3A0006");
  const graph = page.getByRole("region", { name: "Graph" });
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
 * {"test": "a-large-graph-folds-and-unfolds", "checks": ["web/src/views/NodeGraph#right-walk-over-forty-folded", "web/src/views/NodeGraph#fold-all-button-while-unfolded"]}
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
  await page.goto("/g/repo/n/ticket%3A0006");
  const graph = page.getByRole("region", { name: "Graph" });
  await expect(graph).toContainText("rests on 46, deeper branches folded");
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
 * {"test": "a-ticket-resting-on-nothing-draws-no-graph", "checks": ["web/src/views/NodeGraph#no-graph-when-nothing-either-side"]}
 * ```
 */
test("a ticket that rests on nothing, and that nothing rests on, draws no graph", async ({ page }) => {
  // The graph asks its own walk, with no depth: its absence is said once that has answered.
  const walked = page.waitForResponse((r) => r.url().endsWith("/api/rests_on") && r.request().postDataJSON().depth === undefined);
  await page.goto("/g/repo/n/ticket%3A0008");
  await expect(page.getByRole("heading", { level: 1 })).toHaveText("Should readiness follow symlinks?");
  await walked;
  // The answer has arrived; the page still has to take it in. Nothing shows while it does, so the absence is said after a pause.
  await page.waitForTimeout(300);
  await expect(page.getByRole("region", { name: "Graph" })).toHaveCount(0);
});

/**
 * # Test contract
 *
 * ```json
 * {"test": "a-module-outside-the-scope-reads-as-such", "checks": ["web/src/views/Node#scope-outside-quiet"]}
 * ```
 */
test("a module outside the contract scope reads as such on its page, never as a contract error", async ({ page }) => {
  // src/generated.rs is excluded from the scope and carries no contract; the answer still names why none was found.
  const answer = await tool<{ node: { expects_contract: boolean } }>(page, "get", { ref: "module:src/generated" });
  expect(answer.node.expects_contract).toBe(false);
  await page.goto("/g/repo/n/module%3Asrc%2Fgenerated");
  const main = page.locator("main");
  await expect(main.locator("[data-contract-scope='outside']")).toHaveText("Outside the contract scope: no contract expected.");
  await expect(main.getByText(/^Contract:/)).toHaveCount(0);
  await expect(main.getByText("no `# Module contract` block found")).toHaveCount(0);

  // A module in the scope with no contract still shows its error.
  await page.goto("/g/repo/n/module%3Asrc%2Floose");
  await expect(page.locator("main").getByText("Contract: no `# Module contract` block found", { exact: true })).toBeVisible();
  await expect(page.locator("main [data-contract-scope]")).toHaveCount(0);
});

/** The module page of src/generated, which no contract is asked of, as an answer that changes it would give it. */
async function generatedAnswered(page: Page, change: (node: Record<string, unknown>) => void) {
  await page.route("**/api/get", async (route) => {
    if (route.request().postDataJSON().ref !== "module:src/generated") return route.continue();
    const answer = await (await route.fetch()).json();
    change(answer.node);
    await route.fulfill({ json: answer }).catch(() => {});
  });
  await page.goto("/g/repo/n/module%3Asrc%2Fgenerated");
}

/**
 * # Test contract
 *
 * ```json
 * {"test": "a-module-outside-the-scope-that-carries-a-contract-shows-it", "checks": ["web/src/views/Node#scope-outside-carrying-shown"]}
 * ```
 */
test("a module outside the contract scope that carries a contract shows it as any other", async ({ page }) => {
  await generatedAnswered(page, (node) => {
    node.purpose = "Generate the tables.";
    node.contract_at = ["src/generated.rs", 1];
    node.contract_error = null;
    node.criteria = [{ module: "src/generated", id: "makes-tables", statement: "It makes the tables." }];
  });
  const main = page.locator("main");
  await expect(main.getByRole("heading", { name: "Criteria" })).toBeVisible();
  await expect(main.getByText("It makes the tables.")).toBeVisible();
  await expect(main.getByText("Contract at")).toBeVisible();
  await expect(main.locator("[data-contract-scope]")).toHaveCount(0);
  await expect(main.getByText("Outside the contract scope")).toHaveCount(0);
  await expect(main.getByText(/^Contract:/)).toHaveCount(0);
});

/**
 * # Test contract
 *
 * ```json
 * {"test": "a-module-outside-the-scope-with-an-unreadable-block-shows-it", "checks": ["web/src/views/Node#scope-unreadable-shows-error"]}
 * ```
 */
test("a module outside the contract scope whose contract block cannot be read shows that error", async ({ page }) => {
  await generatedAnswered(page, (node) => {
    node.purpose = null;
    node.contract_at = ["src/generated.rs", 1];
    node.contract_error = "src/generated.rs: expected value at line 1 column 1";
  });
  const main = page.locator("main");
  await expect(main.getByText("Contract: src/generated.rs: expected value at line 1 column 1", { exact: true })).toBeVisible();
  await expect(main.locator("[data-contract-scope]")).toHaveCount(0);
  await expect(main.getByText("Outside the contract scope")).toHaveCount(0);
});

/**
 * # Test contract
 *
 * ```json
 * {"test": "a-node-page-is-what-it-is-then-its-graph", "checks": ["web/src/views/Node#requirement-page-parts", "web/src/views/Node#module-page-parts", "web/src/views/Node#no-panel-of-edges", "web/src/views/NodeGraph#graph-draws-what-it-rests-on-to-the-right", "web/src/views/NodeGraph#graph-draws-what-rests-on-it-to-the-left", "web/src/views/NodeGraph#left-leaves-out-own-criteria-and-files", "web/src/views/NodeGraph#module-left-includes-requirements-behind-its-criteria", "web/src/views/NodeGraph#graph-read-through-a-proposal", "web/src/views/NodeGraph#box-is-a-link-naming-how-it-connects"]}
 * ```
 */
test("a node's page is what it is, then its graph centred on it, then the rest it holds, and nothing else", async ({ page }) => {
  // A requirement: its statement, then the graph, then its scenarios; no panels of what it rests on or what rests on it.
  await page.goto("/g/repo/n/req%3Aready-needs-commit");
  const graph = page.getByRole("region", { name: "Graph" });
  await expect(graph.locator("[data-graph-node='req:ready-needs-commit']")).toBeVisible();
  const main = page.locator("main");
  const text = await main.innerText();
  const statement = text.indexOf("A workspace SHALL be ready only when it has a commit.");
  const scenarios = text.indexOf("Scenarios");
  const graphY = (await graph.boundingBox())!.y;
  const statementY = (await main.getByText("A workspace SHALL be ready only when it has a commit.").boundingBox())!.y;
  const scenariosY = (await main.getByRole("heading", { name: "Scenarios" }).boundingBox())!.y;
  expect(statement).toBeGreaterThan(-1);
  expect(scenarios).toBeGreaterThan(statement);
  expect(statementY).toBeLessThan(graphY);
  expect(graphY).toBeLessThan(scenariosY);
  for (const gone of ["Rests on", "Rests on this", "Links out", "Linked from", "Covered by"]) {
    await expect(main.getByRole("heading", { name: gone, exact: true })).toHaveCount(0);
  }
  // Centred: the page linking it to the left, the criterion covering it to the right.
  const [centre, linker, covering] = await Promise.all([
    graph.locator("[data-graph-node='req:ready-needs-commit']").boundingBox(),
    graph.locator("[data-graph-node='page:readiness']").boundingBox(),
    graph.locator("[data-graph-node='criterion:src/ready#needs-commit']").boundingBox(),
  ]);
  expect(linker!.x).toBeLessThan(centre!.x);
  expect(covering!.x).toBeGreaterThan(centre!.x);
  await expect(graph.getByRole("link", { name: "page Readiness, rests on Ready only with a commit" })).toBeVisible();

  // A module: what rests on it to the left, the requirements behind its criteria among them; its own criteria are what it holds.
  await page.goto("/g/repo/n/module%3Asrc%2Fready");
  const mod = page.getByRole("region", { name: "Graph" });
  await expect(mod.locator("[data-graph-node='req:ready-needs-commit']")).toBeVisible();
  await expect(mod.locator("[data-graph-node='page:readiness']")).toBeVisible();
  await expect(mod.locator("[data-graph-node^='criterion:']")).toHaveCount(0);
  await expect(page.locator("main").getByRole("heading", { name: "Criteria" })).toBeVisible();
  // A box re-centres the graph on its node.
  await mod.locator("[data-graph-node='req:ready-needs-commit']").click();
  await expect(page).toHaveURL(/\/n\/req%3Aready-needs-commit$/);
  await expect(page.getByRole("region", { name: "Graph" }).locator("[data-graph-node='req:ready-needs-commit']")).toBeVisible();

  // Read through a ticket's proposal, the graph is asked through it too; and the walk of all that rests on a node is never asked.
  const asked: { tool: string; ticket?: string }[] = [];
  page.on("request", (r) => {
    const tool = r.url().split("/api/")[1];
    if (tool && r.method() === "POST") asked.push({ tool, ticket: (r.postDataJSON() ?? {}).ticket });
  });
  await page.goto("/g/repo/n/req%3Aready-needs-commit?ticket=0001");
  await expect(page.getByRole("region", { name: "Graph" }).locator("[data-graph-node='req:ready-needs-commit']")).toBeVisible();
  expect(asked.filter((a) => a.tool === "rests_on").map((a) => a.ticket)).toContain("0001");
  expect(asked.some((a) => a.tool === "rests_on_me"), "no walk of all that rests on it").toBe(false);
});

/**
 * # Test contract
 *
 * ```json
 * {"test": "a-large-file-is-shown-as-an-omen", "checks": ["web/src/views/Node#omens-shown", "web/src/views/Home#omens-shown-apart"]}
 * ```
 */
test("a file past the large-file limit is shown as an omen, dimmed and apart from the findings", async ({ page }) => {
  // The fixture's limit is ten lines: src/ready.rs is ten, with its tests aside.
  const omen = "[aria-label='Trouble here'] [data-mark-line='omen']";
  await page.goto(`/g/repo/n/${encodeURIComponent("file:src/ready.rs")}`);
  const omens = page.locator(omen);
  await expect(omens).toHaveCount(1);
  await expect(omens).toContainText("large-file");
  await expect(omens).toContainText("10 lines, 32 with its tests");
  await expect(omens).toHaveClass(/text-fg-dim/);
  await expect(omens.locator(".text-bad")).toHaveCount(0);
  // Its module's page says it too, naming the file.
  await page.goto(`/g/repo/n/${encodeURIComponent("module:src/ready")}`);
  await expect(page.locator(omen)).toHaveCount(1);
  await expect(page.locator(`${omen} a[href='/g/repo/n/file%3Asrc%2Fready.rs']`)).toBeVisible();
  // A module whose files are not omens says none.
  await page.goto(`/g/repo/n/${encodeURIComponent("module:src")}`);
  await expect(page.locator("section[aria-label='Graph']")).toBeVisible();
  await expect(page.locator(omen)).toHaveCount(0);
  // The rail counts it on each page above the module, dimmed, never as a warning.
  await page.goto("/g/repo/");
  const rail = page.locator("nav[aria-label='Pages'] a[href='/g/repo/n/page%3Areadiness'] [data-omens]");
  await expect(rail).toHaveText("1 omen");
  await expect(rail).toHaveClass(/text-fg-faint/);
});

/**
 * # Test contract
 *
 * ```json
 * {"test": "a-file-past-the-plain-limit-is-shown-plain", "checks": ["web/src/views/Source#plain-source"]}
 * ```
 */
test("a file past the plain-file limit is shown plain, its numbers, marks and #L kept", async ({ page }) => {
  // The fixture's file is short, so its source answer is marked plain, as a long file's would be.
  await page.route("**/api/source", async (route) => {
    const response = await route.fetch();
    await route.fulfill({ response, json: { ...(await response.json()), plain: true } });
  });
  await page.goto(`/g/repo/n/${encodeURIComponent("file:src/ready.rs")}#L7`);
  await expect(page.locator("#plain-note")).toContainText("Shown plain: at 32 lines");
  const source = page.getByRole("region", { name: "Source of src/ready.rs" });
  await expect(source.locator("[data-tone]")).toHaveCount(0);
  // The contract's JSON stays on its one line, as written.
  expect(await page.locator("tr#L4 td:last-child > div").count()).toBe(1);
  await expect(page.locator("tr[aria-current='location']")).toHaveAttribute("id", "L7");
  await expect(source.locator("[data-annotation='contract'] a[href='/g/repo/n/module%3Asrc%2Fready']")).toBeVisible();
  await expect(source.locator("[data-annotation='test']")).toHaveCount(2);
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
  await page.goto(`/g/repo/n/${encodeURIComponent(ref)}`);
  const link = page.getByRole("link", { name: "src/a#b.ts:9" });
  await expect(link).toBeVisible();
  await expect(link).toHaveAttribute("href", `/g/repo/n/${encodeURIComponent("file:src/a#b.ts")}#L9`);
});

/**
 * # Test contract
 *
 * ```json
 * {"test": "a-requirement-shows-decisions-in-force", "checks": ["web/src/views/Node#decisions-in-force-newest-first", "web/src/views/Node#waiting-decision-marked", "web/src/views/Node#superseded-decisions-folded", "web/src/views/Node#retired-requirement-marked", "web/src/views/Node#requirement-page-parts", "web/src/views/Node#decision-page-parts"]}
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
    // Read through a ticket: the truth's d1, with d2 gone, and the ticket's own, waiting with no id.
    if (ref === "req:waiting") {
      const decisions = [{ date: "2026-01-01", text: "The truth's first." }, { id: 3, date: "2026-01-03", text: "The truth's third." }, { id: null, date: "2026-01-04", text: "Ours, waiting.", supersedes: [1] }];
      return route.fulfill({ json: { node: { ...req("waiting", false), decisions } } });
    }
    if (ref === "req:revised#d3") {
      return route.fulfill({ json: { node: { ref, requirement: "revised", id: 3, date: "2026-01-03", text: "Third, instead of the first.", supersedes: [1], in: [], out: [] } } });
    }
    return route.continue();
  });
  await page.goto("/g/repo/n/req%3Arevised");
  const main = page.locator("main");
  await expect(main.getByText("Third, instead of the first.")).toBeVisible();
  await expect(main.getByText("Second, still holds.")).toBeVisible();
  // The overturned decision is folded, saying what superseded it.
  await expect(main.getByText("First, overturned.")).toBeHidden();
  await main.getByText("1 superseded").click();
  await expect(main.getByText("First, overturned.")).toBeVisible();
  await expect(main.getByText("d1 · superseded by d3")).toBeVisible();
  await expect(main.getByText("d3", { exact: true })).toBeVisible();
  await expect(main.getByText("retired", { exact: true })).toHaveCount(0);
  // Ids are carried, not positions, and a decision waiting in a ticket has none yet.
  await page.goto("/g/repo/n/req%3Awaiting");
  await expect(main.getByText("Ours, waiting.")).toBeVisible();
  await expect(main.getByText("waiting for its ticket to be bottled", { exact: true })).toBeVisible();
  await expect(main.getByText("d3", { exact: true })).toBeVisible();
  await main.getByText("1 superseded").click();
  await expect(main.getByText("d1 · superseded by a decision waiting for its ticket to be bottled")).toBeVisible();
  // A decision's own page: whose it is, what it says and what it supersedes.
  await page.goto("/g/repo/n/req%3Arevised%23d3");
  await expect(main.getByRole("heading", { name: "Decision d3 of revised" })).toBeVisible();
  await expect(main.getByText("Third, instead of the first.")).toBeVisible();
  await expect(main.getByText("Supersedes")).toBeVisible();
  await page.goto("/g/repo/n/req%3Agone");
  await expect(page.locator("main").getByText("retired", { exact: true })).toBeVisible();
});

/**
 * # Test contract
 *
 * ```json
 * {"test": "a-ticket-lists-its-proposal-and-a-node-reads-as-proposed", "checks": ["web/src/views/Node#ticket-page-lists-its-proposal", "web/src/views/Node#node-opened-with-ticket-reads-as-proposed", "web/src/views/Node#node-names-tickets-that-would-change-it"]}
 * ```
 */
test("a brewing ticket lists what it proposes, and a node page reads it as proposed", async ({ page }) => {
  await page.goto("/g/repo/n/ticket%3A0001");
  const main = page.locator("main");
  await expect(main.getByRole("heading", { name: "Proposed" })).toBeVisible();
  const rows = main.locator("h2:has-text('Proposed') + ul > li");
  await expect(rows).toHaveCount(3);
  await expect(rows.filter({ hasText: "Readiness" })).toContainText("changes body");
  await expect(rows.filter({ hasText: "Detached heads" })).toContainText("new");
  const req = rows.filter({ hasText: "Ready only with a commit" });
  await expect(req).toContainText("changes title");
  const asProposed = req.getByRole("link", { name: "as proposed" });
  await expect(asProposed).toHaveAttribute("href", "/g/repo/n/req%3Aready-needs-commit?ticket=0001");
  await asProposed.click();
  await expect(page).toHaveURL(/req%3Aready-needs-commit\?ticket=0001/);
  await expect(main.locator("h1")).toHaveText("Ready only with a commit, detached too");
  await expect(main.getByText("draft", { exact: true })).toBeVisible();
  await main.getByRole("link", { name: "the truth" }).click();
  await expect(main.locator("h1")).toHaveText("Ready only with a commit");
  await expect(main.getByText("proposes a change to this")).toBeVisible();
  await expect(main.getByRole("link", { name: "read it as proposed" })).toHaveAttribute("href", "/g/repo/n/req%3Aready-needs-commit?ticket=0001");
  // An inscribed ticket has nothing proposed and no section.
  await page.goto("/g/repo/n/ticket%3A0006");
  await expect(main.locator("h1")).toHaveText("Report readiness per workspace");
  await expect(main.getByRole("heading", { name: "Next step" })).toBeVisible();
  await expect(main.getByRole("heading", { name: "Proposed" })).toHaveCount(0);
});

/**
 * # Test contract
 *
 * ```json
 * {"test": "an-edit-to-another-ticket-is-shown-as-proposed", "checks": ["web/src/views/Node#ticket-page-lists-its-proposal", "web/src/views/Node#node-names-tickets-that-would-change-it", "web/src/views/Board#ticket-in-place"]}
 * ```
 */
test("a ticket another proposes to edit is named on its page and in the cauldron, and listed in the proposal", async ({ page }) => {
  type Got = { node: { touches: string[]; relies_on: string[] }; drafts?: string[] };
  const before = (await tool<Got>(page, "get", { ref: "ticket:0006" })).node;
  // Ticket 0001's proposal repoints 0006's touches; 0006 keeps the truth's until 0001 is bottled.
  await tool(page, "update_ticket", { ref: "ticket:0006", touches: [...before.touches, "page:readiness"], ticket: "0001" });
  expect((await tool<Got>(page, "get", { ref: "ticket:0006" })).drafts).toEqual(["ticket:0001"]);
  const main = page.locator("main");
  await page.goto("/g/repo/n/ticket%3A0001");
  const edit = main.locator("h2:has-text('Proposed') + ul > li").filter({ hasText: "Report readiness per workspace" });
  await expect(edit).toContainText("changes touches");
  await expect(edit.getByRole("link", { name: "as proposed" })).toHaveAttribute("href", "/g/repo/n/ticket%3A0006?ticket=0001");
  await page.goto("/g/repo/n/ticket%3A0006");
  await expect(main.getByText("proposes a change to this")).toBeVisible();
  await expect(main.getByRole("link", { name: "read it as proposed" })).toHaveAttribute("href", "/g/repo/n/ticket%3A0006?ticket=0001");
  await page.goto("/g/repo/cauldron");
  await page.getByRole("button", { name: "Show Report readiness per workspace" }).click();
  const opened = row(page, "ticket:0006");
  await expect(opened.getByText("proposes a change to this")).toBeVisible();
  // Put back as the truth has it, the edit changes nothing and is no draft; the specs after this one see 0006 as it was.
  await tool(page, "update_ticket", { ref: "ticket:0006", touches: before.touches, relies_on: before.relies_on, ticket: "0001" });
  expect((await tool<Got>(page, "get", { ref: "ticket:0006" })).drafts).toBeUndefined();
});

/**
 * # Test contract
 *
 * ```json
 * {"test": "a-page-names-its-draft-and-reads-as-proposed", "checks": ["web/src/views/Home#draft-line", "web/src/components#badge-tone", "req:web-badge-tone"]}
 * ```
 */
test("a page names the ticket that would change it, and reads as proposed on request", async ({ page }) => {
  await page.goto("/g/repo/n/page%3Areadiness");
  const main = page.locator("main");
  const proposed = main.getByText("Proposed: readiness also counts a detached head.");
  await expect(main.getByText("proposes a change to this")).toBeVisible();
  await expect(proposed).toHaveCount(0);
  await main.getByRole("link", { name: "read it as proposed" }).click();
  await expect(page).toHaveURL(/ticket=0001/);
  await expect(proposed).toBeVisible();
  await expect(main.getByText("draft", { exact: true })).toBeVisible();
  // Drawn in the warn colour, not only classed so: the theme's --color-warn, never the dim foreground beside it.
  await expect(main.getByText("draft", { exact: true })).toHaveCSS("color", "rgb(224, 168, 74)");
  await expect(main.getByText("draft", { exact: true })).toHaveCSS("border-top-color", "rgb(224, 168, 74)");
  await expect(page.locator(rail)).toBeVisible();
  // A page no ticket proposes to change carries no line.
  await page.goto("/g/repo/n/page%3Agoal");
  await expect(main.locator("strong", { hasText: "What it is." })).toBeVisible();
  await expect(main.getByText("proposes a change to this")).toHaveCount(0);
  // A page the proposal adds has no place in the truth's outline, and opens through the ticket.
  await page.goto("/g/repo/n/page%3Adetached");
  await expect(main.getByText("is not in the outline")).toBeVisible();
  await page.goto("/g/repo/n/page%3Adetached?ticket=0001");
  await expect(main.locator("h1")).toHaveText("Detached heads");
  await expect(main.getByText("draft", { exact: true })).toBeVisible();
  await expect(page.locator(rail)).toBeVisible();
});

/**
 * # Test contract
 *
 * ```json
 * {"test": "a-simmering-ticket-shows-its-decision-as-waiting", "checks": ["web/src/views/Node#no-start-or-close", "web/src/views/Node#next-step-line", "web/src/views/Node#simmering-decision-shown", "web/src/views/Board#simmering-waits-to-be-bottled"]}
 * ```
 */
test("a simmering ticket shows its decision as waiting to be bottled, on its page and on the board", async ({ page }) => {
  // Inscribed and bottled here, last of all, so the fixture's own tickets are as the other specs expect them.
  const ref = (await tool<{ node: { ref: string } }>(page, "inscribe_ticket", { title: "Wait to be bottled", kind: "change", priority: "high", touches: ["module:src/ready"], body: "Its decision is written before its change is bottled." })).node.ref;
  await tool(page, "brew_ticket", { ref });
  await tool(page, "close_ticket", { ref, decision: { text: "Decided, and waiting.", date: "2026-01-20" }, when: "bottled" });
  await page.goto(`/g/repo/n/${encodeURIComponent(ref)}`);
  const main = page.locator("main");
  await expect(main).toContainText("simmering");
  await expect(main).toContainText("Decided, and waiting.");
  await expect(main).toContainText("written, waiting for its change to be bottled");
  // Not bottled yet: its body and proposal are kept, and its next step is the bottling, handed to the agent.
  await expect(main).toContainText("Its decision is written before its change is bottled.");
  await expect(page.getByRole("heading", { name: "Proposed" })).toBeVisible();
  await expect(main.locator("[data-handoff]")).toHaveText(`Ticket ${ref.slice("ticket:".length)} ("Wait to be bottled") is simmering: bottle it with the grimoire skill once its change is on the main line.`);
  for (const name of ["Start", "Brew", "Close", "Bottle"]) await expect(page.getByRole("button", { name, exact: true })).toHaveCount(0);
  // On the board it is listed with the changes, marked as waiting to be bottled.
  await page.goto("/g/repo/cauldron");
  await expect(row(page, ref)).toContainText("waiting to be bottled");
  await expect(row(page, ref)).toContainText("simmering");
  await expect(row(page, ref)).not.toContainText("ready");
  // The bottling bottles it with the decision written before.
  const bottled = await tool<{ bottled: boolean; node: { status: string } }>(page, "bottle_ticket", { ref });
  expect([bottled.bottled, bottled.node.status]).toEqual([true, "bottled"]);
  const got = await tool<{ node: { decision: { text: string } } }>(page, "get", { ref });
  expect(got.node.decision.text).toBe("Decided, and waiting.");
});

/**
 * # Test contract
 *
 * ```json
 * {"test": "the-tab-names-what-is-open", "checks": ["web/src/title#tab-title-names-what-is-open", "web/src/title#tab-title-grimoire-alone-when-nothing-named", "web/src/title#node-name-is-title-or-id", "web/src/title#use-tab-title-sets-document-title", "web/src/title#root-view-has-no-name", "web/src/title#unplaced-view-named-unplaced", "web/src/title#library-view-named-library", "web/src/title#page-view-named-by-its-title-as-read", "web/src/title#node-view-named-by-node-name", "web/src/title#cauldron-view-named-cauldron", "web/src/title#findings-view-named-findings"]}
 * ```
 */
test("the tab names what is open, then Grimoire, and carries the book", async ({ page, request }) => {
  const named: [string, string][] = [
    ["/g/repo/", "Grimoire"],
    ["/g/repo/cauldron", "Cauldron · Grimoire"],
    ["/g/repo/findings", "Findings · Grimoire"],
    ["/g/repo/unplaced", "Unplaced · Grimoire"],
    ["/", "Library · Grimoire"],
    ["/g/repo/n/page%3Areadiness", "Readiness · Grimoire"],
    ["/g/repo/n/ticket%3A0006", "Report readiness per workspace · Grimoire"],
    // A module has no title: its id.
    ["/g/repo/n/module%3Asrc%2Fready", "src/ready · Grimoire"],
    // A page a proposal adds, read through it, by its proposed title; outside the outline without it, by its id.
    ["/g/repo/n/page%3Adetached?ticket=0001", "Detached heads · Grimoire"],
    ["/g/repo/n/page%3Adetached", "detached · Grimoire"],
    // Back at the grimoire's root, nothing of the last page is left.
    ["/g/repo/", "Grimoire"],
  ];
  for (const [path, title] of named) {
    await page.goto(path);
    await expect(page, path).toHaveTitle(title);
  }
  // The icon is the UI's own file, an SVG.
  await expect(page.locator('link[rel="icon"]')).toHaveAttribute("href", "/favicon.svg");
  const icon = await request.get("/favicon.svg");
  expect(icon.ok()).toBe(true);
  expect(await icon.text()).toContain("<svg");
});

/** The unsettled gaps and questions drawn under a band, in document order. */
async function unsettledRefs(page: Page, band: string): Promise<string[]> {
  return page.locator(`#unsettled-${band} li[data-ticket]`).evaluateAll((ls) => ls.map((l) => l.getAttribute("data-ticket") ?? ""));
}

/**
 * # Test contract
 *
 * ```json
 * {"test": "unsettled-gaps-and-questions-are-ranked", "checks": ["web/src/views/Board#unsettled-listed", "web/src/views/Board#order-not-resorted", "web/src/views/Board#drag-within-band", "web/src/views/Board#keyboard-move", "web/src/views/Board#moved-among-own-list-only", "web/src/views/Board#focus-follows"]}
 * ```
 */
test("the unsettled gaps and questions are ranked by band, apart from the changes", async ({ page }) => {
  // Two questions and two changes of the test's own in later, deleted at the end, so it holds whatever the specs before it moved.
  const file = async (title: string, kind: string) => (await tool<{ node: { ref: string } }>(page, "inscribe_ticket", { title, kind, priority: "later", touches: [], body: "Ranked by a spec." })).node.ref;
  const a = await file("Rank me A", "question");
  const b = await file("Rank me B", "question");
  // An earlier spec may leave a question of its own unsettled in later; this spec looks only at its own.
  const c = await file("Rank change C", "change");
  const d = await file("Rank change D", "change");
  try {
    const answer = async () => tool<BoardAnswer & { unattached: { ref: string; priority: string }[] }>(page, "board");
    const ours = [a, b];
    const mine = (refs: string[]) => refs.filter((r) => ours.includes(r));
    const unsettledIn = async (band: string) => mine((await answer()).unattached.filter((x) => x.priority === band).map((x) => x.ref));
    const drawn = async (band: string) => mine(await unsettledRefs(page, band));
    const changesIn = async (band: string) => (await answer()).tickets.filter((x) => x.priority === band).map((x) => x.ref);
    const grip = (ref: string) => row(page, ref).getByRole("button", { name: /^Move / });
    // A real pointer drag, the target measured once the drag has begun: an empty band appears then, and moves the rows.
    const dragOnto = async (ref: string, onto: string) => {
      const from = (await grip(ref).boundingBox())!;
      await page.mouse.move(from.x + 5, from.y + 5);
      await page.mouse.down();
      await page.mouse.move(from.x + 10, from.y + 10, { steps: 3 });
      await page.waitForTimeout(100);
      const to = (await row(page, onto).boundingBox())!;
      await page.mouse.move(to.x + 40, to.y + 4, { steps: 10 });
      await page.mouse.up();
    };
    await page.goto("/g/repo/cauldron");

    // Grouped by priority, in the tool's order: with none set, by title.
    await expect.poll(() => drawn("later")).toEqual([a, b]);

    // Dragged within its band: the band's order is set, and drawn so; the changes' order in the band is written with it, unchanged.
    const changesBefore = await changesIn("later");
    await dragOnto(b, a);
    await expect.poll(() => unsettledIn("later")).toEqual([b, a]);
    expect(await changesIn("later")).toEqual(changesBefore);
    await expect.poll(() => drawn("later")).toEqual([b, a]);

    // A change's move writes the band's whole order: the questions' ranking in it stays, though by title A would come first.
    const changes = await changesIn("later");
    expect(changes).toEqual(expect.arrayContaining([c, d]));
    await grip(changes[0]).focus();
    await page.keyboard.press("Alt+ArrowDown");
    await expect.poll(() => changesIn("later")).toEqual([changes[1], changes[0], ...changes.slice(2)]);
    expect(await unsettledIn("later")).toEqual([b, a]);

    // From the keyboard, past the edge: the end of high, its priority changed, its handle still focused.
    // Up to the top of later first, past any question an earlier spec left above it, one press a place.
    const above = (await answer()).unattached.filter((x) => x.priority === "later").map((x) => x.ref).indexOf(b);
    await grip(b).focus();
    for (let i = 0; i <= above; i++) {
      await page.keyboard.press("Alt+ArrowUp");
      if (i < above) await expect.poll(async () => (await answer()).unattached.filter((x) => x.priority === "later").map((x) => x.ref).indexOf(b)).toBe(above - i - 1);
    }
    await expect.poll(() => unsettledIn("high")).toEqual([b]);
    await expect(page.locator("[aria-live]")).toContainText(/moved to position \d+ of \d+ in high/);
    await expect.poll(() => drawn("high")).toEqual([b]);
    await expect(grip(b)).toBeFocused();

    // Never among the other list: a question dropped on a change's row changes nothing.
    const before = await answer();
    await dragOnto(a, c);
    await page.waitForTimeout(300);
    const after = await answer();
    expect(after.unattached.map((x) => x.ref)).toEqual(before.unattached.map((x) => x.ref));
    expect(after.tickets.map((x) => x.ref)).toEqual(before.tickets.map((x) => x.ref));
  } finally {
    for (const ref of [a, b, c, d]) await tool(page, "delete_node", { ref });
  }
});

/**
 * Answer every tool call the page makes as the dev proxy does while the daemon is away, an empty
 * `500 text/plain`; the stream already open is left alone, so an event still makes the page fetch.
 */
async function daemonAway(page: Page) {
  await page.route("**/api/**", (route) => (new URL(route.request().url()).pathname.endsWith("/api/events") ? route.continue() : route.fulfill({ status: 500, contentType: "text/plain", body: "" })));
}

/**
 * # Test contract
 *
 * ```json
 * {"test": "the-page-follows-the-daemon-back", "checks": ["web/src/api#stream-reopens", "req:web-one-stream"]}
 * ```
 */
test("a stream the browser closed is opened again, and the page is current once the daemon answers", async ({ page }) => {
  // Every attempt at the stream is answered as the proxy answers in a rebuild, which closes it for good.
  let refused = 0;
  const events = (url: URL) => url.pathname.endsWith("/api/events");
  await page.route(events, (route) => {
    refused++;
    return route.fulfill({ status: 500, contentType: "text/plain", body: "" });
  });
  // A gap of the spec's own, so earlier specs' writes to the fixture do not matter.
  const own = await tool<{ node: { ref: string } }>(page, "inscribe_ticket", { title: "Seen before the rebuild", kind: "gap", priority: "later", touches: [], body: "On the board from the start." });
  await page.goto("/g/repo/cauldron");
  await expect(page.locator(`#unattached li[data-ticket='${own.node.ref}']`)).toBeVisible();
  await expect.poll(() => refused, { message: "the page tries the stream again after it was closed" }).toBeGreaterThan(1);
  // The daemon answers again: the stream is opened, and a write elsewhere shows without a reload.
  await page.unroute(events);
  const filed = await tool<{ node: { ref: string } }>(page, "inscribe_ticket", { title: "Back after a rebuild", kind: "gap", priority: "later", touches: [], body: "Filed once the stream was back." });
  await expect(page.locator(`#unattached li[data-ticket='${filed.node.ref}']`)).toBeVisible({ timeout: 20_000 });
});

/**
 * # Test contract
 *
 * ```json
 * {"test": "the-page-says-the-daemon-is-not-answering", "checks": ["web/src/api#unreachable-when-daemon-away", "web/src/api#no-answer-is-json-syntax-error", "web/src/api#away-on-unreachable-call", "web/src/api#away-ends-on-newer-answer", "web/src/api#away-keeps-last-data", "web/src/App#header-away", "req:web-daemon-away"]}
 * ```
 */
test("while the daemon is away the page says so, keeps what it showed, and recovers", async ({ page }) => {
  const own = await tool<{ node: { ref: string } }>(page, "inscribe_ticket", { title: "Shown before the daemon went", kind: "gap", priority: "later", touches: [], body: "Stays shown while it is away." });
  await page.goto("/g/repo/cauldron");
  const gap = page.locator(`#unattached li[data-ticket='${own.node.ref}']`);
  await expect(gap).toBeVisible();
  const header = page.locator("header");
  await expect(header).not.toContainText("not answering");
  // Tool calls now meet the proxy's empty 500; the open stream still carries the event that makes the board fetch again.
  await daemonAway(page);
  const away = await tool<{ node: { ref: string } }>(page, "inscribe_ticket", { title: "Written while away", kind: "gap", priority: "later", touches: [], body: "Its event makes the board fetch." });
  await expect(header).toContainText("daemon not answering · retrying");
  // Its dot turns to the bad colour while it is away, and back once it answers.
  await expect(header.locator("button[aria-label^='Where you are'] .bg-bad")).toHaveCount(1);
  await expect(gap).toBeVisible();
  await expect(page.locator(`#unattached li[data-ticket='${away.node.ref}']`)).toHaveCount(0);
  await expect(page.locator("body")).not.toContainText("JSON");
  // Back, and no further event comes: the page's own retry finds it, clears the header and shows what was written meanwhile.
  await page.unrouteAll();
  await expect(page.locator(`#unattached li[data-ticket='${away.node.ref}']`)).toBeVisible({ timeout: 10_000 });
  await expect(header).not.toContainText("not answering");
});

/**
 * # Test contract
 *
 * ```json
 * {"test": "a-page-opened-with-no-daemon-says-so", "checks": ["web/src/api#unreachable-when-daemon-away", "web/src/api#no-answer-is-json-syntax-error", "web/src/api#away-on-unreachable-call", "web/src/api#away-reports-unreachable-without-data", "req:web-daemon-away"]}
 * ```
 */
test("a page opened while the daemon cannot be reached says so in each of its views, never a JSON error", async ({ page }) => {
  await page.route("**/api/**", (route) => route.abort("connectionrefused"));
  // The grimoire, the cauldron and a node page: each view says so in its place.
  // A bare path too: never told where the default grimoire lives, the page shows the views, which say so.
  for (const path of ["/g/repo/", "/g/repo/cauldron", "/g/repo/n/module%3Asrc%2Fready", "/cauldron"]) {
    await page.goto(path);
    await expect(page.locator("main"), path).toContainText("The daemon is not answering");
    await expect(page.locator("header"), path).toContainText("daemon not answering · retrying");
    await expect(page.locator("body"), path).not.toContainText("JSON");
  }
});

/**
 * # Test contract
 *
 * ```json
 * {"test": "a-withdrawn-change-reads-as-withdrawn", "checks": ["web/src/components#withdrawn-tag"]}
 * ```
 */
test("a withdrawn change reads as withdrawn on its page and in the cauldron, apart from one that was bottled", async ({ page }) => {
  // Last of all: a ticket it bottles would push the fixture's own out of the recently bottled that earlier specs read.
  // A change of the spec's own, filed and withdrawn through the tools.
  const filed = await tool<{ node: { ref: string } }>(page, "inscribe_ticket", { title: "Count bytes too", kind: "change", priority: "later", touches: [], body: "Reconsidered before it began." });
  await tool(page, "withdraw_ticket", { ref: filed.node.ref, decision: { text: "Bytes are not what anyone counts." } });
  await page.goto(`/g/repo/n/${encodeURIComponent(filed.node.ref)}`);
  await expect(page.locator("main").getByText("withdrawn", { exact: true })).toBeVisible();
  await expect(page.locator("main")).toContainText("withdrawn: nothing of it was applied");
  await page.goto("/g/repo/cauldron");
  await page.getByRole("button", { name: /^Bottled/ }).click();
  const row = page.locator(`#bottled li[data-ticket='${filed.node.ref}']`);
  await expect(row).toContainText("withdrawn");
  // A change that was bottled still reads as bottled.
  await expect(page.locator("#bottled li[data-ticket='ticket:0010']")).not.toContainText("withdrawn");
});

/**
 * # Test contract
 *
 * ```json
 * {"test": "who-has-a-ticket-is-shown", "checks": ["web/src/views/Board#checked-out", "web/src/views/Node#checked-out", "req:ticket-checked-out"]}
 * ```
 */
test("who has a ticket checked out is shown on its row and its page, and not on one no one works", async ({ page }) => {
  const said = /checked out by alex · since \d\d:\d\d/;
  await page.goto("/g/repo/cauldron");
  // The draft is brewing, brewed by the person on this machine; the inscribed change beside it is no one's.
  await expect(row(page, "ticket:0001").locator("[data-checked-out]")).toHaveText(said);
  await expect(row(page, "ticket:0006").locator("[data-checked-out]")).toHaveCount(0);
  const main = page.locator("main");
  await page.goto("/g/repo/n/ticket%3A0001");
  await expect(main.locator("[data-checked-out]")).toHaveText(said);
  await page.goto("/g/repo/n/ticket%3A0006");
  await expect(main.locator("p").first()).toBeVisible();
  await expect(main.locator("[data-checked-out]")).toHaveCount(0);
});

/** Each locator is on the page and lower than the one before it. */
async function inOrder(...parts: Locator[]) {
  const tops: number[] = [];
  for (const part of parts) {
    await expect(part).toBeVisible();
    tops.push((await part.boundingBox())!.y);
  }
  tops.forEach((y, i) => i > 0 && expect(y, `part ${i} sits below part ${i - 1}: ${tops.join(", ")}`).toBeGreaterThan(tops[i - 1]));
}

/** A colour of the theme as the browser computes it: the colour of text, or of a border, given the classes that draw it. */
async function computed(page: Page, classes: string, of: "color" | "borderTopColor"): Promise<string> {
  return page.evaluate(
    ([c, prop]) => {
      const probe = document.createElement("div");
      probe.className = c;
      document.body.append(probe);
      const rgb = getComputedStyle(probe)[prop as "color"];
      probe.remove();
      return rgb;
    },
    [classes, of],
  );
}

/** The lint answer with findings of the spec's own added, each new, as the header counts them. */
async function lintAlso(page: Page, refs: string[]) {
  await page.route("**/api/lint", async (route) => {
    const response = await route.fetch();
    const answer = await response.json();
    const own = refs.map((ref) => ({ rule: "dangling", check: "dangling-edge", ref, reason: "a mark the spec added", baselined: false }));
    await route.fulfill({ response, json: { ...answer, findings: [...answer.findings, ...own], new: answer.new + own.length } }).catch(() => {});
  });
}

/** The node's page with `extra` added to what rests on it, as an answer of more links would give it. */
async function restedOnBy(page: Page, ref: string, extra: { ref: string; relation: string; title: string }[]) {
  await page.route("**/api/get", async (route) => {
    if (route.request().postDataJSON().ref !== ref) return route.continue();
    const answer = await (await route.fetch()).json();
    answer.node.in = [...answer.node.in, ...extra];
    await route.fulfill({ json: answer }).catch(() => {});
  });
}

/**
 * # Test contract
 *
 * ```json
 * {"test": "a-bare-address-goes-where-status-last-said", "checks": ["web/src/App#status-answer-remembered"]}
 * ```
 */
test("a bare address goes where status last said it went", async ({ page }) => {
  // Never told: the bare address stays, and the view says the daemon is not answering.
  await page.route("**/api/**", (route) => route.abort("connectionrefused"));
  await page.goto("/cauldron");
  await expect(page.locator("main")).toContainText("The daemon is not answering");
  expect(new URL(page.url()).pathname).toBe("/cauldron");
  // Told once, status's answer is kept in the browser.
  await page.unrouteAll({ behavior: "ignoreErrors" });
  await page.goto("/cauldron");
  await expect(page).toHaveURL(/\/g\/repo\/cauldron$/);
  await expect(page.locator("section[id^='band-']").first()).toBeVisible();
  // Away again, the same bare address goes to the grimoire it went to before.
  await page.route("**/api/**", (route) => route.abort("connectionrefused"));
  await page.goto("/cauldron");
  await expect(page).toHaveURL(/\/g\/repo\/cauldron$/);
  await expect(page.locator("main")).toContainText("The daemon is not answering");
  await page.unrouteAll({ behavior: "ignoreErrors" });
});

/**
 * # Test contract
 *
 * ```json
 * {"test": "the-tab-lit-follows-the-view-opened", "checks": ["web/src/App#exactly-one-tab-lit", "web/src/App#tab-lit-by-view"]}
 * ```
 */
test("one tab is lit: Cauldron on its own address and cold tickets", async ({ page }) => {
  const lit = page.locator("header nav [aria-current='page']");
  for (const [path, tab] of [
    ["/g/repo/cauldron", "Cauldron"],
    ["/g/repo/n/ticket%3A0006", "Cauldron"],
    ["/g/repo/n/ticket%3A0004", "Cauldron"],
    ["/g/repo/findings", "Grimoire"],
    ["/g/repo/unplaced", "Grimoire"],
  ] as const) {
    await page.goto(path);
    await expect(lit, path).toHaveCount(1);
    await expect(lit, path).toContainText(tab);
    await expect(page.locator("header nav a").filter({ hasText: tab === "Cauldron" ? "Grimoire" : "Cauldron" }), path).not.toHaveAttribute("aria-current", "page");
  }
});

/**
 * # Test contract
 *
 * ```json
 * {"test": "the-pill-shows-new-only-when-there-are-any", "checks": ["web/src/App#header-pill-shows-new"]}
 * ```
 */
test("the header's pill says N new only while there are any", async ({ page }) => {
  const pill = page.locator("header button[aria-label^='Where you are']");
  const lint = await tool<{ new: number }>(page, "lint");
  expect(lint.new, "the fixture has new findings").toBeGreaterThan(0);
  await page.goto("/g/repo/cauldron");
  const said = pill.getByText(`${lint.new} new`, { exact: true });
  await expect(said).toBeVisible();
  await expect(said).toHaveCSS("color", "rgb(224, 108, 117)");
  // Every finding inherited: the answer's new is 0, and the pill says nothing of it.
  await page.route("**/api/lint", async (route) => {
    const response = await route.fetch();
    const answer = await response.json();
    await route.fulfill({ response, json: { ...answer, new: 0, findings: answer.findings.map((f: object) => ({ ...f, baselined: true })) } }).catch(() => {});
  });
  const answered = page.waitForResponse((r) => r.url().endsWith("/api/lint"));
  await page.goto("/g/repo/cauldron");
  await answered;
  await expect(page.locator("section[id^='band-']").first()).toBeVisible();
  await page.waitForTimeout(300);
  await expect(pill).toBeVisible();
  await expect(pill.getByText(/\bnew\b/)).toHaveCount(0);
  await expect(pill).not.toHaveAttribute("aria-label", /\bnew\b/);
  await page.unrouteAll({ behavior: "ignoreErrors" });
});

/**
 * # Test contract
 *
 * ```json
 * {"test": "a-page-shows-the-pages-it-links-as-cards", "checks": ["web/src/views/Home#page-links-shown-as-cards"]}
 * ```
 */
test("a page shows the pages it links as cards", async ({ page }) => {
  for (const ref of ["page:architecture", "page:handbook"]) {
    const links = (await tool<{ node: { links: string[] } }>(page, "get", { ref })).node.links.filter((l) => l.startsWith("page:"));
    expect(links.length, ref).toBeGreaterThan(0);
    await page.goto(`/g/repo/n/${encodeURIComponent(ref)}`);
    await expect(page.locator("#pages"), ref).toBeVisible();
    expect(await linkedRefs(page, "#pages"), ref).toEqual(links);
  }
  // A card holds its page's title and its first paragraph.
  await page.goto("/g/repo/n/page%3Aarchitecture");
  const card = page.locator("#pages a[href='/g/repo/n/page%3Areadiness']");
  await expect(card).toContainText("Readiness");
  await expect(card.locator("p")).not.toHaveText("");
  // A page that links no page has no cards.
  await page.goto("/g/repo/n/page%3Agoal");
  await expect(page.locator("main h1")).toHaveText("Goal");
  await expect(page.locator("#pages")).toHaveCount(0);
});

/**
 * # Test contract
 *
 * ```json
 * {"test": "a-rite-says-it-is-held-by-the-work", "checks": ["web/src/views/Home#rite-says-held-by-the-work"]}
 * ```
 */
test("a rite says it is held by the work, and no other does", async ({ page }) => {
  const listed = page.locator("#requirements li");
  await page.goto("/g/repo/n/page%3Areadiness");
  await expect(listed.first()).toBeVisible();
  const reqs = await listed.count();
  expect(reqs).toBeGreaterThan(1);
  await expect(page.locator("#requirements").getByText("held by the work")).toHaveCount(0);
  // The outline marks the requirements of the page as the rites'.
  await page.route("**/api/outline", async (route) => {
    const response = await route.fetch();
    const answer = await response.json();
    const mark = (t: { ref: string; rites?: boolean; children: unknown[] }) => {
      if (t.ref.startsWith("req:")) t.rites = true;
      (t.children as (typeof t)[]).forEach(mark);
    };
    answer.roots.forEach(mark);
    await route.fulfill({ response, json: answer }).catch(() => {});
  });
  await page.goto("/g/repo/n/page%3Areadiness");
  await expect(listed).toHaveCount(reqs);
  for (const row of await listed.all()) {
    await expect(row.getByText("held by the work", { exact: true })).toBeVisible();
    await expect(row).not.toContainText("uncovered");
  }
  await page.unrouteAll({ behavior: "ignoreErrors" });
});

/**
 * # Test contract
 *
 * ```json
 * {"test": "a-mark-chip-is-bad-for-new-and-dim-for-the-rest", "checks": ["web/src/views/Marks#mark-chip-names-a-mark"]}
 * ```
 */
test("a mark's chip is in the bad colour for new, dim for the rest", async ({ page }) => {
  await page.goto("/g/repo/");
  const bad = await computed(page, "text-bad", "color");
  const dim = await computed(page, "text-fg-dim", "color");
  const line = await computed(page, "border border-line", "borderTopColor");
  expect(bad).toBe("rgb(224, 108, 117)");
  expect(dim).not.toBe(bad);
  const chipOf = (name: string) => page.locator("[aria-label='Trouble here'] li").getByText(name, { exact: true }).first();
  await page.goto("/g/repo/n/req%3Aready-names-its-errors");
  const fresh = chipOf("new");
  await expect(fresh).toBeVisible();
  await expect(fresh).toHaveCSS("color", bad);
  await expect(fresh).toHaveCSS("border-top-color", bad);
  for (const [path, name] of [
    ["module:src/loose", "inherited"],
    ["req:ready-writes-nothing", "may conflict"],
    ["file:src/ready.rs", "omen"],
  ]) {
    await page.goto(`/g/repo/n/${encodeURIComponent(path)}`);
    const chip = chipOf(name);
    await expect(chip, name).toBeVisible();
    await expect(chip, name).toHaveCSS("color", dim);
    await expect(chip, name).toHaveCSS("border-top-color", line);
  }
});

/**
 * # Test contract
 *
 * ```json
 * {"test": "trouble-comes-between-the-title-and-what-the-node-is", "checks": ["web/src/views/Node#trouble-said-under-the-title"]}
 * ```
 */
test("trouble in words sits under the title, above what it is", async ({ page }) => {
  const trouble = page.locator("[aria-label='Trouble here']");
  // A requirement: its statement is what it is.
  const req = await tool<{ node: { statement: string } }>(page, "get", { ref: "req:ready-names-its-errors" });
  await page.goto("/g/repo/n/req%3Aready-names-its-errors");
  await inOrder(page.locator("main h1"), trouble, page.locator("main").getByText(req.node.statement, { exact: true }).last());
  // A module with no contract: the error it shows about its contract is what it is.
  await page.goto("/g/repo/n/module%3Asrc%2Floose");
  await inOrder(page.locator("main h1"), trouble, page.locator("main").getByText(/^Contract: /));
});

/**
 * # Test contract
 *
 * ```json
 * {"test": "a-ticket-page-is-its-status-graph-body-decision-step-proposal", "checks": ["web/src/views/Node#ticket-page-parts"]}
 * ```
 */
test("a ticket's page parts come in their order", async ({ page }) => {
  const ticket = (await tool<{ node: { priority: string; kind: string; status: string; body: string; decision: { text: string } } }>(page, "get", { ref: "ticket:0011" })).node;
  expect(ticket.status, "the fixture's simmering ticket").toBe("simmering");
  await page.goto("/g/repo/n/ticket%3A0011");
  const main = page.locator("main");
  const lead = main.locator("p").filter({ hasText: ticket.priority }).filter({ hasText: ticket.kind }).filter({ hasText: ticket.status }).first();
  await expect(lead).toHaveText(new RegExp(`${ticket.priority}\\s*·\\s*${ticket.kind}\\s*·\\s*${ticket.status}`, "i"));
  await inOrder(
    lead,
    main.getByRole("region", { name: "Graph" }),
    main.getByText(ticket.body, { exact: true }),
    main.getByText(ticket.decision.text),
    main.getByRole("heading", { name: "Next step" }),
    main.getByRole("heading", { name: "Proposed" }),
  );
});

/**
 * # Test contract
 *
 * ```json
 * {"test": "a-criterion-page-is-its-statement-and-place-then-its-graph", "checks": ["web/src/views/Node#criterion-page-parts"]}
 * ```
 */
test("a criterion's page is its statement and place, then graph", async ({ page }) => {
  const ref = "criterion:src/ready#needs-commit";
  const node = (await tool<{ node: { statement: string } }>(page, "get", { ref })).node;
  const written = await tool<{ file: string; line: number }>(page, "module_source", { ref });
  await page.goto(`/g/repo/n/${encodeURIComponent(ref)}`);
  const main = page.locator("main");
  const title = main.locator("h1");
  await expect(title).toHaveText(node.statement);
  const where = main.locator("p").filter({ hasText: /^In / });
  await expect(where.getByRole("link", { name: "src/ready", exact: true })).toHaveAttribute("href", "/g/repo/n/module%3Asrc%2Fready");
  const place = where.getByRole("link", { name: `${written.file}:${written.line}` });
  await expect(place).toHaveAttribute("href", `/g/repo/n/${encodeURIComponent(`file:${written.file}`)}#L${written.line}`);
  await inOrder(title, where, main.getByRole("region", { name: "Graph" }));
});

/**
 * # Test contract
 *
 * ```json
 * {"test": "a-module-page-is-its-contract-and-place-graph-then-criteria", "checks": ["web/src/views/Node#module-page-parts"]}
 * ```
 */
test("a module's page is contract and place, graph, criteria", async ({ page }) => {
  const node = (await tool<{ node: { purpose: string; contract_at: [string, number]; criteria: { id: string; statement: string }[] } }>(page, "get", { ref: "module:src/ready" })).node;
  expect(node.criteria.length).toBeGreaterThan(1);
  await page.goto("/g/repo/n/module%3Asrc%2Fready");
  const main = page.locator("main");
  const title = main.locator("h1");
  await expect(title).toHaveText(node.purpose);
  const [file, line] = node.contract_at;
  const where = main.locator("p").filter({ hasText: /^Contract at / });
  await expect(where.getByRole("link", { name: `${file}:${line}` })).toHaveAttribute("href", `/g/repo/n/${encodeURIComponent(`file:${file}`)}#L${line}`);
  const rows = node.criteria.map((c) => main.locator("h2:has-text('Criteria') + ul > li").filter({ has: page.getByRole("link", { name: `#${c.id}`, exact: true }) }));
  await inOrder(title, where, main.getByRole("region", { name: "Graph" }), main.getByRole("heading", { name: "Criteria" }), ...rows);
  for (const [i, c] of node.criteria.entries()) {
    await expect(rows[i].getByRole("link", { name: `#${c.id}`, exact: true })).toHaveAttribute("href", `/g/repo/n/${encodeURIComponent(`criterion:src/ready#${c.id}`)}`);
    await expect(rows[i]).toContainText(c.statement);
  }
});

/**
 * # Test contract
 *
 * ```json
 * {"test": "the-graph-marks-neither-a-gone-box-nor-a-folded-count", "checks": ["web/src/views/NodeGraph#gone-box-and-folded-count-carry-no-marks"]}
 * ```
 */
test("a gone box and a folded count carry no marks", async ({ page }) => {
  // Trouble found on the gone requirement and on 44 requirements folded into a count, and on the box beside each as the control.
  await lintAlso(page, ["req:ready-old-name", "module:src/ready", "page:readiness", ...Array.from({ length: 44 }, (_, i) => `req:r${i}`)]);
  await page.goto("/g/repo/n/ticket%3A0004");
  const graph = page.getByRole("region", { name: "Graph" });
  const gone = graph.locator("[data-gone='req:ready-old-name']");
  await expect(gone).toBeVisible();
  await expect(graph.locator("[data-graph-node='module:src/ready']")).toHaveAttribute("data-marks", "new");
  await expect(gone.locator("[data-mark]")).toHaveCount(0);
  await expect(gone).not.toHaveAttribute("data-marks", /./);
  await expect(gone).not.toHaveAttribute("aria-label", /marked/);
  // A walk of 46 folds its 44 requirements into a count.
  const via = ["ticket:0006", "page:readiness"];
  const nodes = [
    { ref: "page:readiness", title: "Readiness", via: ["ticket:0006"], decisions: [] },
    ...Array.from({ length: 44 }, (_, i) => ({ ref: `req:r${i}`, title: `Requirement ${i}`, via, decisions: [] })),
  ];
  await page.route("**/api/rests_on", async (route) => {
    if (route.request().postDataJSON().depth !== undefined) return route.continue();
    await route.fulfill({ json: { nodes } });
  });
  await page.goto("/g/repo/n/ticket%3A0006");
  await expect(graph.locator("[data-graph-node='page:readiness']")).toHaveAttribute("data-marks", "new");
  const count = graph.getByRole("button", { name: "Show the 44 requirements under Readiness" });
  await expect(count).toBeVisible();
  await expect(count.locator("[data-mark]")).toHaveCount(0);
  await expect(count).not.toHaveAttribute("data-marks", /./);
  // Unfolded, the same requirements are marked.
  await count.click();
  await expect(graph.locator("[data-graph-node='req:r0']")).toHaveAttribute("data-marks", "new");
  await page.unrouteAll({ behavior: "ignoreErrors" });
});

/**
 * # Test contract
 *
 * ```json
 * {"test": "the-left-draws-only-tickets-being-worked", "checks": ["web/src/views/NodeGraph#left-tickets-only-those-being-worked"]}
 * ```
 */
test("the left draws only tickets being worked or waited on", async ({ page }) => {
  type Row = { ref: string; waits_on?: { ref: string }[] };
  const board = await tool<{ tickets: Row[]; unattached: Row[] }>(page, "board", {});
  const live = new Set([...board.tickets, ...board.unattached].flatMap((t) => [t.ref, ...(t.waits_on ?? []).map((w) => w.ref)]));
  const node = (await tool<{ node: { in: { ref: string; relation: string }[] } }>(page, "get", { ref: "module:src/ready" })).node;
  // Two tickets that are bottled, which the board does not list, added to what the answer says rests on the module.
  const history = ["ticket:0004", "ticket:0010"];
  expect(history.filter((t) => live.has(t)), "bottled tickets are not on the board").toEqual([]);
  await restedOnBy(page, "module:src/ready", history.map((ref) => ({ ref, relation: "touches", title: `Bottled ${ref}` })));
  const expected = node.in.filter((e) => e.ref.startsWith("ticket:") && live.has(e.ref)).map((e) => e.ref).sort();
  expect(expected.length, "tickets being worked").toBeGreaterThan(1);
  await page.goto("/g/repo/n/module%3Asrc%2Fready");
  const graph = page.getByRole("region", { name: "Graph" });
  await expect(graph.locator("[data-graph-node='req:ready-needs-commit']")).toBeVisible();
  const fold = graph.getByRole("button", { name: /^Show the \d+ tickets resting on / });
  if (await fold.count()) await fold.click();
  await expect(graph.locator("[data-graph-node^='ticket:']").first()).toBeVisible();
  await expect.poll(async () => (await graph.locator("[data-graph-node^='ticket:']").evaluateAll((as) => as.map((a) => a.getAttribute("data-graph-node") ?? ""))).sort()).toEqual(expected);
});

/**
 * # Test contract
 *
 * ```json
 * {"test": "a-left-group-over-six-is-a-count-that-unfolds", "checks": ["web/src/views/NodeGraph#left-group-over-six-folded"]}
 * ```
 */
test("on the left a group over six is a count that unfolds", async ({ page }) => {
  const ref = "criterion:src/ready#reads-only";
  const extra = [
    ...Array.from({ length: 7 }, (_, i) => ({ ref: `req:many${i}`, relation: "covers", title: `Many ${i}` })),
    ...Array.from({ length: 6 }, (_, i) => ({ ref: `page:few${i}`, relation: "links", title: `Few ${i}` })),
  ];
  await restedOnBy(page, ref, extra);
  await page.goto(`/g/repo/n/${encodeURIComponent(ref)}`);
  const graph = page.getByRole("region", { name: "Graph" });
  const count = graph.getByRole("button", { name: /^Show the 7 requirements resting on / });
  await expect(count).toBeVisible();
  await expect(graph.locator("[data-graph-node^='req:many']")).toHaveCount(0);
  // Six of a kind are drawn whole.
  await expect(graph.locator("[data-graph-node^='page:few']")).toHaveCount(6);
  await expect(graph.getByRole("button", { name: /^Show the 6 pages/ })).toHaveCount(0);
  await count.click();
  await expect(graph.locator("[data-graph-node^='req:many']")).toHaveCount(7);
  await expect(count).toHaveCount(0);
  await graph.getByRole("button", { name: "Fold all" }).click();
  await expect(graph.locator("[data-graph-node^='req:many']")).toHaveCount(0);
  await expect(graph.getByRole("button", { name: /^Show the 7 requirements resting on / })).toBeVisible();
  await page.unrouteAll({ behavior: "ignoreErrors" });
});

/**
 * # Test contract
 *
 * ```json
 * {"test": "a-file-keeps-its-last-lines-while-changed-text-is-highlighted", "checks": ["web/src/views/Source#source-keeps-last-lines-meanwhile"]}
 * ```
 */
test("a file keeps its last lines while changed text is highlighted", async ({ page }) => {
  let changed = false;
  await page.route("**/api/source", async (route) => {
    const response = await route.fetch();
    const answer = await response.json();
    await route.fulfill({ response, json: changed ? { ...answer, text: `${answer.text}// text-changed-meanwhile\n` } : answer }).catch(() => {});
  });
  await page.goto(`/g/repo/n/${encodeURIComponent("file:src/ready.rs")}`);
  const region = page.getByRole("region", { name: "Source of src/ready.rs" });
  await expect(region.locator("tr#L7")).toContainText("pub fn ready() -> bool");
  await expect(region.locator("[data-tone]").first()).toBeVisible();
  // From here the lines are watched: they must never be gone, nor the page say Loading.
  await page.evaluate(() => {
    const w = window as unknown as { __gaps: number };
    w.__gaps = 0;
    const seen = () => {
      if (!document.querySelector("[role=region][aria-label='Source of src/ready.rs'] tr#L7") || document.body.innerText.includes("Loading…")) w.__gaps++;
    };
    new MutationObserver(seen).observe(document.body, { childList: true, subtree: true, characterData: true });
  });
  // Its text changes: the answer to the next source call has a line more, arriving with the event a write makes.
  changed = true;
  await tool(page, "inscribe_ticket", { title: "Written while the file is open", kind: "gap", priority: "later", touches: [], body: "Its event makes the file's page ask for its source again." });
  await expect(region).toContainText("text-changed-meanwhile");
  await expect(region.locator("tr#L7")).toContainText("pub fn ready() -> bool");
  expect(await page.evaluate(() => (window as unknown as { __gaps: number }).__gaps), "the lines were gone for a moment").toBe(0);
  await page.unrouteAll({ behavior: "ignoreErrors" });
});

const pillOf = (page: Page) => page.locator("header button[aria-label^='Where you are']");

/** The colour a theme token resolves to, as the browser computes it. */
async function tokenColour(page: Page, token: string): Promise<string> {
  return page.evaluate((name) => {
    const probe = document.createElement("span");
    probe.style.backgroundColor = `var(${name})`;
    document.body.appendChild(probe);
    const colour = getComputedStyle(probe).backgroundColor;
    probe.remove();
    return colour;
  }, token);
}

/** Add checkouts to the library status lists, the page's own answer otherwise. */
async function withLibrary(page: Page, extra: { name: string; repository: string; server: string | null }[], onlyHere = false) {
  await page.route("**/api/status", async (route) => {
    const fetched = await route.fetch().catch(() => null);
    if (!fetched) return;
    const answer = await fetched.json();
    await route.fulfill({ json: { ...answer, library: [...(onlyHere ? answer.library.filter((g: { server: string | null }) => g.server === null) : answer.library), ...extra.map((e) => ({ ...e, current: null }))] } }).catch(() => {});
  });
}

/** The Library's own hash of a name, so a test can tell which cloth and offset a name is given. */
function hashOf(name: string): number {
  let h = 2166136261;
  for (const c of name) h = Math.imul(h ^ c.charCodeAt(0), 16777619);
  return h >>> 0;
}
const CLOTH_TOKENS = ["--color-syn-keyword", "--color-syn-string", "--color-syn-number", "--color-syn-type", "--color-syn-meta"];
const SPINE_OFFSETS = [-20, -12, -5, 4, 11, 19];
const RAIL_OFFSETS = [-4, -3, -1, 1, 3, 4];

/**
 * # Test contract
 *
 * ```json
 * {"test": "the-header-is-pill-tabs-search", "checks": ["web/src/App#header-three-parts"]}
 * ```
 */
test("the header holds the pill left, the tabs centred, the search right", async ({ page }) => {
  await page.goto("/g/repo/cauldron");
  const header = page.locator("header");
  const search = page.getByLabel("Find");
  await expect(pillOf(page)).toBeVisible();
  const [h, p, n, s] = await Promise.all([header.boundingBox(), pillOf(page).boundingBox(), header.locator("nav").boundingBox(), search.boundingBox()]);
  expect(p!.x + p!.width, "the pill ends before the tabs begin").toBeLessThanOrEqual(n!.x);
  expect(n!.x + n!.width, "the tabs end before the search begins").toBeLessThanOrEqual(s!.x);
  // The pill at the left edge, the search at the right, the tabs on the header's middle.
  expect(p!.x - h!.x).toBeLessThan(24);
  expect(h!.x + h!.width - (s!.x + s!.width)).toBeLessThan(24);
  expect(Math.abs(n!.x + n!.width / 2 - (h!.x + h!.width / 2))).toBeLessThan(2);
});

/**
 * # Test contract
 *
 * ```json
 * {"test": "the-pill-has-a-chevron", "checks": ["web/src/App#pill-has-chevron"]}
 * ```
 */
test("the pill holds a chevron after what it names", async ({ page }) => {
  await page.goto("/g/repo/cauldron");
  const chevron = pillOf(page).locator("svg");
  await expect(chevron).toHaveCount(1);
  await expect(chevron).toBeVisible();
  const [named, mark] = await Promise.all([pillOf(page).locator(".font-bold").boundingBox(), chevron.boundingBox()]);
  expect(mark!.x, "to the right of the name").toBeGreaterThan(named!.x + named!.width);
  // A chevron: one open stroke down to a point and up again, not a filled shape.
  const stroke = chevron.locator("path");
  await expect(stroke).toHaveCount(1);
  await expect(stroke).toHaveAttribute("d", "M2 3.5 5 6.5 8 3.5");
  await expect(stroke).toHaveAttribute("fill", "none");
});

/**
 * # Test contract
 *
 * ```json
 * {"test": "the-pill-names-the-checkout-of-several", "checks": ["web/src/App#pill-names-checkout-of-several"]}
 * ```
 */
test("the pill names the checkout on screen after the project when several", async ({ page }) => {
  const status = await tool<{ repository: string; grimoire: string }>(page, "status");
  const folder = status.repository.replace(/[\\/]+$/, "").split(/[\\/]/).pop()!;
  await withLibrary(page, [{ name: status.grimoire, repository: `${status.repository}-second`, server: null }]);
  await page.goto("/g/repo/cauldron");
  await expect(pillOf(page).locator(".font-bold").first()).toHaveText(status.grimoire);
  // After the grimoire's name: a slash, then the folder of the checkout on screen, not the other.
  await expect(pillOf(page).locator("span.text-fg-faint", { hasText: "/" })).toHaveCount(1);
  await expect(pillOf(page)).toContainText(`${status.grimoire} / ${folder}`);
  await expect(pillOf(page)).not.toContainText(`${folder}-second`);
  await page.unrouteAll({ behavior: "ignoreErrors" });
});

/**
 * # Test contract
 *
 * ```json
 * {"test": "the-pill-dot-is-bad-when-it-should-be", "checks": ["web/src/App#pill-dot-colour"]}
 * ```
 */
test("the pill's dot is ok, and bad for a lost server, a failed status or an absent daemon", async ({ page }) => {
  const status = await tool<{ repository: string }>(page, "status");
  const dot = () => pillOf(page).locator("span.rounded-full").first();
  const colour = () => dot().evaluate((el) => getComputedStyle(el).backgroundColor);
  // The checkout is named in the address, so that status failing leaves the other calls as they were.
  await page.goto(`/g/repo/cauldron?checkout=${encodeURIComponent(status.repository)}`);
  await expect(pillOf(page).locator(".font-bold")).toBeVisible();
  const ok = await tokenColour(page, "--color-ok");
  const bad = await tokenColour(page, "--color-bad");
  expect(ok).not.toBe(bad);
  expect(await colour()).toBe(ok);
  // A server that did not answer.
  await page.route("**/api/status", async (route) => {
    const fetched = await route.fetch().catch(() => null);
    if (!fetched) return;
    await route.fulfill({ json: { ...(await fetched.json()), server: { address: "http://10.0.0.9:4242", fetched: null, reachable: false, refused: false } } }).catch(() => {});
  });
  await page.reload();
  await expect(pillOf(page)).toHaveAttribute("title", "server unreachable");
  expect(await colour()).toBe(bad);
  // A status that fails.
  await page.unrouteAll({ behavior: "ignoreErrors" });
  await page.route("**/api/status", (route) => route.fulfill({ status: 404, contentType: "text/plain", body: "gone" }));
  await page.reload();
  await expect(pillOf(page)).toContainText("status answered 404");
  expect(await colour()).toBe(bad);
  // A daemon that is away.
  await page.unrouteAll({ behavior: "ignoreErrors" });
  await page.route("**/api/**", (route) => route.abort("connectionrefused"));
  await page.reload();
  await expect(pillOf(page)).toContainText("daemon not answering");
  expect(await colour()).toBe(bad);
  await page.unrouteAll({ behavior: "ignoreErrors" });
});

/**
 * # Test contract
 *
 * ```json
 * {"test": "the-panel-shows-the-finding-counts", "checks": ["web/src/App#panel-shows-finding-counts"]}
 * ```
 */
test("the pill's panel gives the new, inherited and may-conflict counts", async ({ page }) => {
  const lint = await tool<{ findings: unknown[]; advisories: unknown[]; new: number }>(page, "lint");
  await page.goto("/g/repo/cauldron");
  await pillOf(page).click();
  const panel = page.getByRole("dialog", { name: "Checkouts" });
  await expect(panel.getByText(`${lint.new} new`, { exact: true })).toBeVisible();
  await expect(panel.getByText(`${lint.findings.length - lint.new} inherited`, { exact: true })).toBeVisible();
  await expect(panel.getByText(`${lint.advisories.length} may conflict`, { exact: true })).toBeVisible();
});

/**
 * # Test contract
 *
 * ```json
 * {"test": "the-panel-lists-each-checkout-with-its-ticket-and-link", "checks": ["web/src/App#panel-checkout-line", "web/src/App#panel-checkout-links"]}
 * ```
 */
test("the pill's panel shows each checkout's folder and ticket, the others linking to their own page", async ({ page }) => {
  const status = await tool<{ repository: string; grimoire: string }>(page, "status");
  const mine = status.repository.replace(/[/\\]+$/, "").split(/[/\\]/).pop()!;
  await page.route("**/api/status", async (route) => {
    const fetched = await route.fetch().catch(() => null);
    if (!fetched) return;
    const answer = await fetched.json();
    const others = [
      { name: answer.grimoire, repository: "/srv/other", server: null, current: { id: "0001", title: "Another ticket" } },
      { name: answer.grimoire, repository: "/srv/idle", server: null, current: null },
    ];
    const library = [...answer.library.filter((g: { name: string }) => g.name !== answer.grimoire), { name: answer.grimoire, repository: answer.repository, server: null, current: null }, ...others];
    await route.fulfill({ json: { ...answer, current: { id: "0003", title: "On screen ticket" }, library } }).catch(() => {});
  });
  await page.goto("/g/repo/cauldron");
  await pillOf(page).click();
  const items = page.getByRole("dialog", { name: "Checkouts" }).locator("li");
  await expect(items).toHaveCount(3);
  // Each line: the checkout's folder, then its current ticket's id and title, or no ticket.
  const here = items.filter({ has: page.locator("[aria-current]") });
  await expect(here).toHaveText(`${mine} 0003 On screen ticket`);
  await expect(items.filter({ hasText: "other" })).toHaveText("other 0001 Another ticket");
  await expect(items.filter({ hasText: "idle" })).toHaveText("idle no ticket");
  // The one on screen is no link; each other is, to the grimoire's page for that checkout, as a page of its own.
  await expect(here.locator("a")).toHaveCount(0);
  const link = (folder: string) => items.filter({ hasText: folder }).locator("a");
  await expect(link("other")).toHaveAttribute("href", `/g/${status.grimoire}/?checkout=${encodeURIComponent("/srv/other")}`);
  await expect(link("idle")).toHaveAttribute("href", `/g/${status.grimoire}/?checkout=${encodeURIComponent("/srv/idle")}`);
  await expect(link("other")).not.toHaveAttribute("target", /./);
  await page.evaluate(() => ((window as unknown as { __kept: boolean }).__kept = true));
  await link("other").click();
  await expect(page).toHaveURL((url) => url.pathname.startsWith(`/g/${status.grimoire}`) && url.searchParams.get("checkout") === "/srv/other");
  expect(await page.evaluate(() => (window as unknown as { __kept?: boolean }).__kept), "a new page, not a route inside the old").toBeUndefined();
  await page.unrouteAll({ behavior: "ignoreErrors" });
});

/**
 * # Test contract
 *
 * ```json
 * {"test": "the-panel-closes-on-a-second-press", "checks": ["web/src/App#panel-closes-on-second-press"]}
 * ```
 */
test("pressing the pill again closes its panel", async ({ page }) => {
  await page.goto("/g/repo/cauldron");
  const panel = page.getByRole("dialog", { name: "Checkouts" });
  await pillOf(page).click();
  await expect(panel).toBeVisible();
  await expect(pillOf(page)).toHaveAttribute("aria-expanded", "true");
  await pillOf(page).click();
  await expect(panel).toHaveCount(0);
  await expect(pillOf(page)).toHaveAttribute("aria-expanded", "false");
});

/**
 * # Test contract
 *
 * ```json
 * {"test": "the-panel-closes-on-an-outside-click", "checks": ["web/src/App#panel-closes-on-outside-click"]}
 * ```
 */
test("a click outside the pill's panel closes it, and one inside does not", async ({ page }) => {
  await page.goto("/g/repo/cauldron");
  const panel = page.getByRole("dialog", { name: "Checkouts" });
  await pillOf(page).click();
  await expect(panel).toBeVisible();
  // Inside the panel, on text that is no link: it stays.
  await panel.getByRole("heading", { name: "Checkouts of this project" }).click();
  await expect(panel).toBeVisible();
  // Outside it, in the header's own margin.
  await page.locator("header").click({ position: { x: 3, y: 3 } });
  await expect(panel).toHaveCount(0);
});

/**
 * # Test contract
 *
 * ```json
 * {"test": "a-stream-error-is-away-until-it-opens", "checks": ["web/src/api#away-on-stream-error", "web/src/api#away-ends-on-opened-stream"]}
 * ```
 */
test("an error on the event stream is away until the stream is opened", async ({ page }) => {
  test.setTimeout(60_000);
  const status = await tool<{ repository: string }>(page, "status");
  let open = false;
  let held = 0;
  // The stream is refused as a proxy does while the daemon restarts; every call is held, so that only the stream can end it.
  await page.route("**/api/**", async (route) => {
    if (new URL(route.request().url()).pathname.endsWith("/api/events")) {
      return open ? route.continue() : route.fulfill({ status: 500, contentType: "text/plain", body: "" });
    }
    held++;
    await new Promise(() => {});
  });
  await page.goto(`/g/repo/cauldron?checkout=${encodeURIComponent(status.repository)}`);
  const header = page.locator("header");
  await expect(header).toContainText("daemon not answering · retrying");
  // The page asks again on its retry, and nothing answers: still away.
  const asked = held;
  await expect.poll(() => held, { timeout: 15_000 }).toBeGreaterThan(asked);
  await expect(header).toContainText("daemon not answering · retrying");
  // The stream is opened again: away ends, though no call has been answered.
  open = true;
  await expect(header).not.toContainText("not answering", { timeout: 30_000 });
  await page.unrouteAll({ behavior: "ignoreErrors" });
});

/**
 * # Test contract
 *
 * ```json
 * {"test": "bars-and-counts-are-the-outlines-own", "checks": ["web/src/views/Home#bars-and-counts-are-the-outlines"]}
 * ```
 */
test("a page's bars and finding counts are those its outline answers", async ({ page }) => {
  // The outline's answer with numbers of its own, so nothing else can be where they come from.
  await page.route("**/api/outline", async (route) => {
    const answer = (await (await route.fetch()).json()) as Outline;
    const root = answer.roots[0];
    Object.assign(root.rollup, { modules: 7, contracted: 3, criteria: 11, tested: 5, requirements: 9, held: 4, described: 2, decided: 1 });
    root.findings = 6;
    root.baselined = 2;
    await route.fulfill({ json: answer }).catch(() => {});
  });
  await page.goto("/g/repo/");
  const head = page.locator("main header").first();
  const bars = head.locator("dl");
  for (const [label, n, of] of [["Contracted", 3, 7], ["Criteria tested", 5, 11], ["Requirements held", 4, 9], ["Requirements decided", 1, 9]] as const) {
    const meter = bars.getByRole("meter", { name: label });
    await expect(meter).toHaveAttribute("aria-valuenow", String(n));
    await expect(meter).toHaveAttribute("aria-valuemax", String(of));
    await expect(bars.getByText(`${n} of ${of}`, { exact: true })).toBeVisible();
  }
  // Six findings, two of them inherited: four new.
  await expect(head.getByText("4 new", { exact: true })).toBeVisible();
  await expect(head.getByText("2 inherited", { exact: true })).toBeVisible();
  await page.unrouteAll({ behavior: "ignoreErrors" });
});

/**
 * # Test contract
 *
 * ```json
 * {"test": "the-rail-ends-with-unplaced-only-when-some", "checks": ["web/src/views/Home#rail-ends-with-unplaced-only-when-some"]}
 * ```
 */
test("the rail has Unplaced last when modules are unplaced, and not otherwise", async ({ page }) => {
  await page.route("**/api/outline", async (route) => {
    const answer = (await (await route.fetch()).json()) as Outline;
    answer.unplaced = [];
    await route.fulfill({ json: answer }).catch(() => {});
  });
  await page.goto("/g/repo/");
  await expect(page.locator(rail).locator("li a").first()).toBeVisible();
  await expect(page.locator(rail).getByRole("link", { name: /Unplaced/ })).toHaveCount(0);
  await expect(page.locator(rail).getByText("Unplaced")).toHaveCount(0);
  // The outline's own unplaced modules: the entry comes last in the rail, after every page.
  await page.unrouteAll({ behavior: "ignoreErrors" });
  await page.reload();
  const last = page.locator(`${rail} > :last-child`);
  await expect(last).toHaveText(/Unplaced/);
  await expect(last).toHaveAttribute("href", "/g/repo/unplaced");
});

/**
 * # Test contract
 *
 * ```json
 * {"test": "the-library-stands-spines-in-cloth-and-offset", "checks": ["web/src/views/Library#spine-band-colour-by-name", "web/src/views/Library#spine-offset-sideways"]}
 * ```
 */
test("each spine's bands and offset follow from its name alone, on every visit and in any order", async ({ page }) => {
  // Several more names on this machine, so that a stack has spines enough to differ.
  const extra = ["quill", "vellum", "lantern", "sextant", "orrery", "bestiary", "herbal"].map((name) => ({ name, repository: `/srv/${name}`, server: null }));
  const reordered = async (reverse: boolean) => {
    await page.unrouteAll({ behavior: "ignoreErrors" });
    await page.route("**/api/status", async (route) => {
      const fetched = await route.fetch().catch(() => null);
      if (!fetched) return;
      const answer = await fetched.json();
      const library = [...answer.library, ...extra.map((e) => ({ ...e, current: null }))];
      await route.fulfill({ json: { ...answer, library: reverse ? library.reverse() : library } }).catch(() => {});
    });
  };
  const measure = () =>
    page.locator("[data-spine]").evaluateAll((spines) =>
      spines.map((a) => {
        const own = a.getBoundingClientRect();
        const slot = (a.parentElement as HTMLElement).getBoundingClientRect();
        return {
          name: a.getAttribute("title") ?? "",
          stack: [...(a.closest("[data-stack]")?.querySelectorAll("[data-spine]") ?? [])].indexOf(a) + "@" + [...document.querySelectorAll("[data-stack]")].indexOf(a.closest("[data-stack]")!),
          bands: [...a.querySelectorAll(":scope > span[aria-hidden]")].map((b) => getComputedStyle(b).backgroundColor),
          left: getComputedStyle(a).left,
          shifted: Math.round(own.left - slot.left),
        };
      }),
    );
  const byName = (spines: Awaited<ReturnType<typeof measure>>) => new Map(spines.map((s) => [s.name, { bands: s.bands, left: s.left, shifted: s.shifted }]));
  await reordered(false);
  await page.goto("/");
  await expect(page.locator("[data-spine]", { hasText: "herbal" })).toBeVisible();
  const first = await measure();
  expect(first.length).toBeGreaterThan(extra.length);
  // Cloths are the theme's own colours, each spine's four bands one cloth.
  const cloths = new Set<string>();
  for (const token of CLOTH_TOKENS) cloths.add(await tokenColour(page, token));
  for (const spine of first) {
    expect(spine.bands, spine.name).toHaveLength(4);
    expect(new Set(spine.bands).size, spine.name).toBe(1);
    expect(cloths.has(spine.bands[0]), `${spine.name}'s cloth is a token colour`).toBe(true);
    // Sideways from the spine beneath it: a shift its own offset names.
    expect(spine.shifted, spine.name).not.toBe(0);
    expect(spine.left, spine.name).toBe(`${spine.shifted}px`);
  }
  // Names differ, so the spines do: more than one cloth and offset among them, and neighbours in a stack not all alike.
  expect(new Set(first.map((s) => s.bands[0])).size).toBeGreaterThan(1);
  expect(new Set(first.map((s) => s.left)).size).toBeGreaterThan(1);
  const stackOf = (s: (typeof first)[number]) => s.stack.split("@")[1];
  expect(first.some((s, i) => i > 0 && stackOf(first[i - 1]) === stackOf(s) && first[i - 1].left !== s.left), "adjacent spines offset from each other").toBe(true);
  // The same on another visit.
  await page.reload();
  await expect(page.locator("[data-spine]", { hasText: "herbal" })).toBeVisible();
  expect(byName(await measure())).toEqual(byName(first));
  // And with the library listed the other way round, each name keeps its cloth and offset.
  await reordered(true);
  await page.reload();
  await expect(page.locator("[data-spine]", { hasText: "herbal" })).toBeVisible();
  const reversed = await measure();
  expect(reversed.map((s) => s.name).sort()).toEqual(first.map((s) => s.name).sort());
  expect(byName(reversed)).toEqual(byName(first));
  await page.unrouteAll({ behavior: "ignoreErrors" });
});

/**
 * # Test contract
 *
 * ```json
 * {"test": "the-rail-book-is-offset-like-its-spine", "checks": ["web/src/views/Library#rail-book-offset-like-spine"]}
 * ```
 */
test("each rail book is offset by the same hash as the spine of that name", async ({ page }) => {
  await page.goto("/");
  await expect(page.locator("[data-spine]").first()).toBeVisible();
  const spines = await page.locator("[data-spine]").evaluateAll((as) => as.map((a) => [a.getAttribute("title") ?? "", getComputedStyle(a).left]));
  await page.goto("/g/repo/");
  await expect(page.locator("[data-rail-book]").first()).toBeVisible();
  const books = new Map(await page.locator("[data-rail-book]").evaluateAll((as) => as.map((a): [string, string] => [a.getAttribute("title") ?? "", getComputedStyle(a).left])));
  expect(books.size).toBe(spines.length);
  for (const [name, left] of spines) {
    const i = SPINE_OFFSETS.indexOf(parseInt(left, 10));
    expect(i, `${name}'s spine offset`).toBeGreaterThanOrEqual(0);
    expect(i).toBe((hashOf(name) >>> 14) % SPINE_OFFSETS.length);
    expect(books.get(name), name).toBe(`${RAIL_OFFSETS[i]}px`);
  }
});

/**
 * # Test contract
 *
 * ```json
 * {"test": "the-rail-mark-is-three-books", "checks": ["web/src/views/Library#rail-mark-is-three-books"]}
 * ```
 */
test("the rail's mark is three flat books in the band colours", async ({ page }) => {
  await page.goto("/g/repo/");
  const mark = page.locator("[data-shelf-rail] a[title='Library']");
  await expect(mark).toBeVisible();
  const books = mark.locator("> span > span");
  await expect(books).toHaveCount(3);
  const shown = await books.evaluateAll((els) =>
    els.map((b) => {
      const box = b.getBoundingClientRect();
      return { w: Math.round(box.width), h: Math.round(box.height), top: box.top, bands: [...b.children].map((c) => getComputedStyle(c).backgroundColor) };
    }),
  );
  // Flat: wider than tall; stacked top to bottom; the bands of the keyword, type and string cloths.
  expect(shown.map((b) => [b.w, b.h])).toEqual([[20, 7], [26, 7], [23, 7]]);
  expect(shown[0].top).toBeLessThan(shown[1].top);
  expect(shown[1].top).toBeLessThan(shown[2].top);
  for (const [i, token] of ["--color-syn-keyword", "--color-syn-type", "--color-syn-string"].entries()) {
    const c = await tokenColour(page, token);
    expect(shown[i].bands, token).toEqual([c, c]);
  }
});

/**
 * # Test contract
 *
 * ```json
 * {"test": "a-rail-shelf-is-flat-books-on-a-plank", "checks": ["web/src/views/Library#rail-shelf-is-books-on-a-plank"]}
 * ```
 */
test("a rail shelf is a stack of small flat books on a plank beneath them", async ({ page }) => {
  await page.goto("/g/repo/");
  const shelves = page.locator("[data-shelf-rail] [data-shelf]");
  await expect(shelves.first()).toBeVisible();
  const plank = await tokenColour(page, "--color-bg-3");
  for (const shelf of await shelves.all()) {
    const books = await shelf.locator("[data-rail-book]").evaluateAll((as) => as.map((a) => { const b = a.getBoundingClientRect(); return { w: b.width, h: b.height, top: b.top, bottom: b.bottom }; }));
    expect(books.length).toBeGreaterThan(0);
    for (const b of books) {
      expect(b.h, "flat").toBe(18);
      expect(b.w, "small").toBeGreaterThanOrEqual(28);
      expect(b.w).toBeLessThanOrEqual(36);
    }
    // Stacked one above the next.
    for (let i = 1; i < books.length; i++) expect(books[i].top).toBeGreaterThanOrEqual(books[i - 1].bottom);
    const board = await shelf.locator(":scope > div").evaluate((d) => { const b = d.getBoundingClientRect(); return { top: b.top, w: b.width, h: b.height, bg: getComputedStyle(d).backgroundColor }; });
    expect(board.top, "the plank under the last book").toBeGreaterThanOrEqual(books[books.length - 1].bottom);
    expect([board.w, board.h, board.bg]).toEqual([42, 4, plank]);
  }
});

/**
 * # Test contract
 *
 * ```json
 * {"test": "the-library-stacks-stand-side-by-side", "checks": ["web/src/views/Library#stacks-side-by-side"]}
 * ```
 */
test("the Library's stacks stand beside each other on one shelf", async ({ page }) => {
  await withLibrary(page, [{ name: "tower", repository: "/srv/tower", server: "http://10.0.0.7:4242" }], true);
  await page.goto("/");
  const stacks = page.locator("[data-stack]");
  await expect(stacks).toHaveCount(2);
  const [a, b] = await stacks.evaluateAll((els) => els.map((e) => { const r = e.getBoundingClientRect(); const plank = e.querySelector(":scope > div")!.getBoundingClientRect(); return { left: r.left, right: r.right, plank: plank.bottom }; }));
  expect(b.left, "side by side, not above").toBeGreaterThanOrEqual(a.right);
  expect(Math.abs(a.plank - b.plank), "on one shelf").toBeLessThan(3);
  await page.unrouteAll({ behavior: "ignoreErrors" });
});

/**
 * # Test contract
 *
 * ```json
 * {"test": "a-stack-label-sets-the-port-apart", "checks": ["web/src/views/Library#stack-label-sets-port-apart"]}
 * ```
 */
test("a server's port in a stack's label is set apart, in the same width whatever it is", async ({ page }) => {
  await withLibrary(page, [
    { name: "tower", repository: "/srv/tower", server: "http://10.0.0.7:4242" },
    { name: "vault", repository: "/srv/vault", server: "http://10.0.0.8:80" },
  ]);
  await page.goto("/");
  const captions = page.locator("[data-stack] figcaption");
  await expect(captions.filter({ hasText: "10.0.0.8" })).toHaveCount(1);
  const long = captions.filter({ hasText: "10.0.0.7" });
  const short = captions.filter({ hasText: "10.0.0.8" });
  await expect(long.locator("[data-port]")).toHaveText(":4242");
  await expect(short.locator("[data-port]")).toHaveText(":80");
  await expect(long).toHaveText("10.0.0.7:4242");
  // The host stands in the same place whatever the port: the port's slot is as wide for either.
  const width = (c: Locator) => c.locator("[data-port]").evaluate((p) => p.getBoundingClientRect().width);
  expect(await width(long)).toBe(await width(short));
  // This machine's stack has no port to set apart.
  await expect(captions.filter({ hasText: "this machine" }).locator("[data-port]")).toHaveCount(0);
  await page.unrouteAll({ behavior: "ignoreErrors" });
});

/**
 * # Test contract
 *
 * ```json
 * {"test": "a-proposal-marks-deletions-and-leaves-out-equal-records", "checks": ["web/src/views/Node#proposal-marks-deletions", "web/src/views/Node#proposal-leaves-out-equal-records"]}
 * ```
 */
test("Proposed marks deletions and omits records equal to truth", async ({ page }) => {
  // Last in the file, since the draft's proposal is read by earlier specs: a deletion, and a page proposed just as it is.
  const goal = (await tool<{ node: { slug: string; title: string; body: string; links: string[] } }>(page, "get", { ref: "page:goal" })).node;
  await tool(page, "put_page", { slug: goal.slug, title: goal.title, body: goal.body, links: goal.links, ticket: "0001" });
  await tool(page, "delete_node", { ref: "page:runbook", ticket: "0001" });
  await page.goto("/g/repo/n/ticket%3A0001");
  const rows = page.locator("main h2:has-text('Proposed') + ul > li");
  const deleted = rows.filter({ has: page.locator("a[href$='/n/page%3Arunbook']") });
  await expect(deleted).toHaveCount(1);
  await expect(deleted.getByText("to be deleted", { exact: true })).toBeVisible();
  // The three records it proposes are not marked as deletions, and the record equal to the truth's is not listed.
  await expect(rows.getByText("to be deleted")).toHaveCount(1);
  await expect(rows).toHaveCount(4);
  await expect(rows.filter({ has: page.locator("a[href$='/n/page%3Agoal']") })).toHaveCount(0);
});
