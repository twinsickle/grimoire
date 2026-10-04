import { defineConfig } from "@playwright/test";

// Behaviour tests: what each view does, against a fixture store that
// behaviour/serve.mjs builds in a temporary directory and serves with the
// built UI, so the ticket actions can write without touching a real store.
// Every spec runs twice, as Grimoire is used both ways: `team`, through a
// shared server and the checkout's daemon signed in to it, and `solo`, one
// person's daemon over a store of its own. Each has a fixture of its own,
// so the writes of one never reach the other.
// Run `npm run behaviour`, which builds the UI first.
const solo = process.env.BEHAVIOUR_PORT ?? "4343";
const team = process.env.BEHAVIOUR_TEAM_PORT ?? "4345";

const fixture = (port: string, setup: "solo" | "team") => ({
  command: "node serve.mjs",
  cwd: ".",
  url: `http://127.0.0.1:${port}/api/tools`,
  reuseExistingServer: false,
  timeout: 60_000,
  env: { BEHAVIOUR_PORT: port, BEHAVIOUR_SETUP: setup },
});

export default defineConfig({
  testDir: ".",
  testMatch: /.*\.spec\.ts/,
  outputDir: "./out",
  fullyParallel: false,
  workers: 1,
  retries: 0,
  reporter: [["list"]],
  use: {
    viewport: { width: 1400, height: 900 },
    navigationTimeout: 15_000,
    actionTimeout: 10_000,
  },
  projects: [
    { name: "team", use: { baseURL: `http://127.0.0.1:${team}` } },
    { name: "solo", use: { baseURL: `http://127.0.0.1:${solo}` } },
  ],
  webServer: [fixture(team, "team"), fixture(solo, "solo")],
});
