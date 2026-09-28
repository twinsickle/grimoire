/**
 * Every view, screenshotted and compared against a committed baseline in
 * visual/baseline/. Run `npm run visual`; after an intended change, look at
 * visual/out/ and run `npm run visual:update`.
 */
import { expect, test, type Page } from "@playwright/test";

/** Wait for the header's status line, which proves the daemon answered, then for the view's own content. */
async function settle(page: Page, content: string) {
  await page.locator("header").getByText(/\d+ pages ·/).waitFor();
  await page.locator(content).first().waitFor();
  // Let the shared event stream deliver anything in flight and fonts settle.
  await page.waitForTimeout(300);
}

const views: { name: string; path: string; content: string }[] = [
  { name: "summary", path: "/", content: "#contracts" },
  { name: "map", path: "/map", content: "text=Depth" },
  { name: "board", path: "/board", content: "table" },
  { name: "node-page", path: "/n/page%3Aarchitecture", content: "h1" },
  { name: "node-requirement", path: "/n/req%3Atool-coverage", content: "h1" },
  { name: "node-module", path: "/n/module%3Acrates%2Fcore%2Fsrc%2Fquery", content: "h1" },
  { name: "node-ticket", path: "/n/ticket%3A0001", content: "h1" },
];

for (const v of views) {
  test(v.name, async ({ page }) => {
    await page.goto(v.path);
    await settle(page, v.content);
    await expect(page).toHaveScreenshot(`${v.name}.png`, { fullPage: true });
  });
}
