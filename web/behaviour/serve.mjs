// Serve the behaviour fixture: copy its repository to a temporary directory
// (the sources are kept as .rs.txt so Grimoire's own extraction never reads
// them), initialise a store there, fill it from store.json through the tools,
// and run `grimoire serve` with the built UI until Playwright stops it.
//
// GRIMOIRE_BIN names the binary; `grimoire` on PATH by default.

import { spawn, spawnSync } from "node:child_process";
import { cpSync, mkdtempSync, readFileSync, readdirSync, renameSync, rmSync } from "node:fs";
import { tmpdir } from "node:os";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";

const here = dirname(fileURLToPath(import.meta.url));
const bin = process.env.GRIMOIRE_BIN ?? (process.platform === "win32" ? "grimoire.exe" : "grimoire");
const port = process.env.BEHAVIOUR_PORT ?? "4343";

const dir = mkdtempSync(join(tmpdir(), "grimoire-behaviour-"));
const repo = join(dir, "repo");
const store = join(dir, "store");
cpSync(join(here, "fixture", "repo"), repo, { recursive: true });
for (const name of readdirSync(join(repo, "src"))) {
  if (name.endsWith(".txt")) renameSync(join(repo, "src", name), join(repo, "src", name.slice(0, -4)));
}

function grimoire(...args) {
  const r = spawnSync(bin, args, { cwd: repo, encoding: "utf8" });
  if (r.status !== 0) throw new Error(`grimoire ${args.join(" ")} failed:\n${r.stdout}${r.stderr}`);
  return r.stdout;
}

grimoire("init", "--store", store);
const records = JSON.parse(readFileSync(join(here, "fixture", "store.json"), "utf8"));
for (const page of records.pages) grimoire("tool", "put_page", JSON.stringify(page));
for (const req of records.requirements) grimoire("tool", "put_requirement", JSON.stringify(req));
for (const { close, ...ticket } of records.tickets) {
  const ref = JSON.parse(grimoire("tool", "file_ticket", JSON.stringify(ticket))).node.ref;
  if (close) {
    grimoire("tool", "start_ticket", JSON.stringify({ ref }));
    grimoire("tool", "close_ticket", JSON.stringify({ ref, decision: { text: close } }));
  }
}

const ui = join(here, "..", "dist");
const daemon = spawn(bin, ["serve", "--port", port, "--ui-dir", ui], { cwd: repo, stdio: "inherit" });
const stop = () => {
  daemon.kill();
  rmSync(dir, { recursive: true, force: true });
  process.exit(0);
};
process.on("SIGTERM", stop);
process.on("SIGINT", stop);
daemon.on("exit", (code) => process.exit(code ?? 0));
