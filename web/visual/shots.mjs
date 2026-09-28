// Quick captures for looking at, not comparing: one PNG per view into
// visual/out/. `node visual/shots.mjs [baseUrl]`.
import { chromium } from "@playwright/test";
import { mkdirSync } from "node:fs";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";

const base = process.argv[2] ?? process.env.GRIMOIRE_UI ?? "http://localhost:5173";
const out = join(dirname(fileURLToPath(import.meta.url)), "out");
mkdirSync(out, { recursive: true });

const views = [
  ["summary", "/", "#contracts"],
  ["map", "/map", "text=Depth"],
  ["board", "/board", "table"],
  ["node-page", "/n/page%3Aarchitecture", "h1"],
  ["node-requirement", "/n/req%3Atool-coverage", "h1"],
  ["node-module", "/n/module%3Acrates%2Fcore%2Fsrc%2Fquery", "h1"],
  ["node-ticket", "/n/ticket%3A0001", "h1"],
];

const browser = await chromium.launch();
const page = await browser.newPage({ viewport: { width: 1400, height: 900 }, colorScheme: "dark" });
for (const [name, path, content] of views) {
  await page.goto(base + path, { waitUntil: "commit" });
  await page.locator("header").getByText(/\d+ pages ·/).waitFor({ timeout: 15000 });
  await page.locator(content).first().waitFor({ timeout: 15000 });
  await page.waitForTimeout(300);
  const file = join(out, `${name}.png`);
  await page.screenshot({ path: file, fullPage: true });
  console.log(`${name}: ${file}`);
}
await browser.close();
