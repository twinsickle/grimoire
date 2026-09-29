import { defineConfig } from "@playwright/test";
import visual from "../playwright.config";

// The visual test's setup, running the captures instead of the comparisons.
export default defineConfig({ ...visual, testDir: ".", testMatch: /shots\.capture\.ts/, outputDir: "./out/shots-results", webServer: { ...visual.webServer, cwd: "../behaviour" } } as typeof visual);
