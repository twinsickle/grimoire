// Quick captures for looking at, not comparing: one PNG per route in
// behaviour/routes.ts into visual/out/, on the behaviour fixture. Run
// `npm run shots`, which builds the UI and serves the fixture as the visual
// test does.
import { test } from "@playwright/test";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";
import { ROUTES } from "../behaviour/routes";
import { open } from "./settle";

const out = join(dirname(fileURLToPath(import.meta.url)), "out");

for (const route of ROUTES) {
  test(route.name, async ({ page }) => {
    await open(page, route);
    await page.screenshot({ path: join(out, `${route.name}.png`), fullPage: true });
  });
}
