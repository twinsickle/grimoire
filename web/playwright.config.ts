import { defineConfig } from "@playwright/test";

// Visual tests against a running UI: the Vite dev server (5173) by default,
// or the daemon's built copy with GRIMOIRE_UI=http://127.0.0.1:4242.
export default defineConfig({
  testDir: "./visual",
  testMatch: /.*\.spec\.ts/,
  outputDir: "./visual/out/test-results",
  snapshotPathTemplate: "{testDir}/baseline/{arg}{ext}",
  fullyParallel: false,
  retries: 0,
  reporter: [["list"]],
  use: {
    baseURL: process.env.GRIMOIRE_UI ?? "http://localhost:5173",
    viewport: { width: 1400, height: 900 },
    colorScheme: "dark",
    // The event stream keeps a connection open forever, so never wait for
    // the network to go idle; the spec waits for content instead.
    navigationTimeout: 15_000,
    actionTimeout: 10_000,
  },
  expect: {
    toHaveScreenshot: { maxDiffPixelRatio: 0.005, animations: "disabled", caret: "hide" },
  },
});
