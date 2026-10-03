// Serve the behaviour fixture: copy its repository to a temporary directory
// (the sources are kept as .rs.txt so Grimoire's own extraction never reads
// them), initialise a store there, fill it from store.json through the tools
// under tickets of its own (its pages under two roots), adopt what it finds as
// the baseline, add the new work, leave one ticket landing and one active with a proposal,
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

// A fixture server stopped by force (Playwright and shots end the process
// tree on Windows, where there are no signals) never reaches its cleanup, so
// each run first removes the directories of runs whose server is gone. The
// directory names the process that made it.
const PREFIX = "grimoire-behaviour-";
function alive(pid) {
  try {
    process.kill(pid, 0);
    return true;
  } catch (e) {
    return e.code === "EPERM";
  }
}
for (const name of readdirSync(tmpdir())) {
  const pid = Number(name.startsWith(PREFIX) ? name.slice(PREFIX.length).split("-")[0] : NaN);
  if (Number.isInteger(pid) && pid > 0 && !alive(pid)) rmSync(join(tmpdir(), name), { recursive: true, force: true });
}

const dir = mkdtempSync(join(tmpdir(), `${PREFIX}${process.pid}-`));
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
const tool = (name, input) => JSON.parse(grimoire("tool", name, JSON.stringify(input)));

// A write that changes intent lands in a ticket's proposal and reaches the
// truth when the ticket closes, so the fixture is seeded under tickets of its
// own, filed after the ten the specs name by id and closed before them, with
// earlier dates, so the board's recently closed stay the fixture's.
function under(title, write) {
  const ref = tool("file_ticket", { title, kind: "change", priority: "later", touches: [], body: "Seeds the fixture." }).node.ref;
  tool("start_ticket", { ref });
  write();
  tool("close_ticket", { ref, decision: { text: "Seeded.", date: "2026-01-10" } });
}
const tickets = records.tickets.map(({ close, land, ...ticket }) => ({ close, land, ref: tool("file_ticket", ticket).node.ref }));
under("Seed the pages and requirements", () => {
  for (const page of records.pages) tool("put_page", page);
  // More than one root, so the views show every root and not only the first.
  tool("configure", { roots: records.roots });
  for (const req of records.requirements) tool("put_requirement", req);
});
for (const { close, ref } of tickets) {
  if (close) {
    tool("start_ticket", { ref });
    // A fixed date, so a view showing a decision renders the same on any day.
    tool("close_ticket", { ref, decision: { text: close, date: "2026-01-15" } });
  }
}

// Everything so far is inherited; what follows is new work, so the views
// have both to show apart.
tool("adopt", { reason: "The fixture's findings are inherited, so the views show them apart from new ones." });
under("Add the new work", () => {
  for (const req of records.after_adoption.requirements) tool("put_requirement", req);
  for (const [ref, add_links] of Object.entries(records.after_adoption.links)) tool("patch_page", { ref, add_links });
});

// One ticket is left landing, its decision written and its change yet to
// land, so the views have one to show. It is started before the draft below,
// which is the one left current.
for (const { land, ref } of tickets) {
  if (land) {
    tool("start_ticket", { ref });
    tool("close_ticket", { ref, decision: { text: land, date: "2026-01-16" }, when: "landed" });
  }
}

// The first ticket stays active with a proposal, so the views have a draft
// to show: a page and a requirement, each read from the truth and changed in
// one field.
const draft = tickets[0].ref;
tool("start_ticket", { ref: draft });
const { slug, title, body, links } = tool("get", { ref: "page:readiness" }).node;
tool("put_page", { slug, title, body: `Proposed: readiness also counts a detached head.

${body}`, links, ticket: draft });
const req = tool("get", { ref: "req:ready-needs-commit" }).node;
tool("put_requirement", { slug: req.slug, title: `${req.title}, detached too`, statement: req.statement, scenarios: req.scenarios, covered_by: req.covered_by, ticket: draft });
// And a page the truth does not have yet.
tool("put_page", { slug: "detached", title: "Detached heads", body: "Proposed by the first ticket: a page the truth does not have yet.", links: [], ticket: draft });

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
