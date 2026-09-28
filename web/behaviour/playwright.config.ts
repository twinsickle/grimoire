import { defineConfig } from "@playwright/test";

// Behaviour tests: what each view does, against a fixture store that
// behaviour/serve.mjs builds in a temporary directory and serves with the
// built UI, so the ticket actions can write without touching a real store.
// Run `npm run behaviour`, which builds the UI first.
const port = process.env.BEHAVIOUR_PORT ?? "4343";

export default defineConfig({
  testDir: ".",
  testMatch: /.*\.spec\.ts/,
  outputDir: "./out",
  fullyParallel: false,
  workers: 1,
  retries: 0,
  reporter: [["list"]],
  use: {
    baseURL: `http://127.0.0.1:${port}`,
    viewport: { width: 1400, height: 900 },
    navigationTimeout: 15_000,
    actionTimeout: 10_000,
  },
  webServer: {
    command: "node serve.mjs",
    cwd: ".",
    url: `http://127.0.0.1:${port}/api/tools`,
    reuseExistingServer: false,
    timeout: 60_000,
    env: { BEHAVIOUR_PORT: port },
  },
});
