/**
 * Every route in behaviour/routes.ts, screenshotted on the behaviour fixture
 * and compared against a committed baseline in visual/baseline/. Run
 * `npm run visual`; after an intended change, look at visual/out/ and run
 * `npm run visual:update`.
 *
 * # Test contract
 *
 * ```json
 * {"test": "every-view-matches-its-baseline", "checks": ["web/src/App#visual-baselines"]}
 * ```
 */
import { expect, test } from "@playwright/test";
import { ROUTES } from "../behaviour/routes";
import { open } from "./settle";

for (const route of ROUTES) {
  test(route.name, async ({ page }) => {
    await open(page, route);
    await expect(page).toHaveScreenshot(`${route.name}.png`, { fullPage: true });
  });
}
