import { defineConfig } from "@playwright/test";

// Visual tests: every route in behaviour/routes.ts, screenshotted against
// the behaviour fixture store, which behaviour/serve.mjs builds in a
// temporary directory and serves with the built UI, as a team serves it:
// through a shared server and the checkout's daemon signed in to it. A fixed store means a
// failing screenshot is a rendering change, not a ticket closed or a page
// edited. Run `npm run visual`, which builds the UI first.
const port = process.env.VISUAL_PORT ?? "4344";

export default defineConfig({
  testDir: "./visual",
  testMatch: /.*\.spec\.ts/,
  outputDir: "./visual/out/test-results",
  snapshotPathTemplate: "{testDir}/baseline/{arg}{ext}",
  fullyParallel: false,
  workers: 1,
  retries: 0,
  reporter: [["list"]],
  use: {
    baseURL: `http://127.0.0.1:${port}`,
    viewport: { width: 1400, height: 900 },
    colorScheme: "dark",
    // The event stream keeps a connection open forever, so never wait for
    // the network to go idle; the spec waits for content instead.
    navigationTimeout: 15_000,
    actionTimeout: 10_000,
  },
  expect: {
    // Any pixel more than one colour level off fails. The run-to-run noise is
    // anti-aliasing at most one level off on each channel (a YIQ distance of
    // 0.0038), which 0.005 ignores; two levels is 0.0076, and one changed glyph
    // moves 3 or 4 pixels at full contrast (measured 2026-10-01). Pixels the
    // comparator classes as anti-aliasing are left out whatever their colour.
    toHaveScreenshot: { threshold: 0.005, maxDiffPixels: 0, animations: "disabled", caret: "hide" },
  },
  webServer: {
    command: "node serve.mjs",
    cwd: "./behaviour",
    url: `http://127.0.0.1:${port}/api/tools`,
    reuseExistingServer: false,
    timeout: 60_000,
    env: { BEHAVIOUR_PORT: port, BEHAVIOUR_SETUP: "team" },
  },
});
