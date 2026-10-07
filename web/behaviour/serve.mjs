// Serve the behaviour fixture: copy its repository to a temporary directory
// (the sources are kept as .rs.txt so Grimoire's own extraction never reads
// them), initialise a store there, fill it from store.json through the tools
// under tickets of its own (its pages under two roots), adopt what it finds as
// the baseline, add the new work, leave one ticket landing and one active with a proposal,
// and serve it with the built UI until Playwright stops it.
//
// Served two ways, as Grimoire is used. BEHAVIOUR_SETUP=solo (the default):
// `grimoire serve` over the store, one person's daemon. BEHAVIOUR_SETUP=team:
// the store is served shared on BEHAVIOUR_SERVER_PORT, the fixture's checkout
// is pointed at that server and signed in with a token of its own, and the
// UI is served by the checkout's daemon, which reads the server's intent and
// sends its writes there. Either way the store is seeded first, through the
// tools on a store of its own, since seeding through a server syncs on every
// write. Nothing is read from or written to the person's own Grimoire data:
// GRIMOIRE_HOME is the run's own.
//
// GRIMOIRE_BIN names the binary; `grimoire` on PATH by default.

import { spawn, spawnSync } from "node:child_process";
import { cpSync, mkdirSync, mkdtempSync, readFileSync, readdirSync, renameSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";

const here = dirname(fileURLToPath(import.meta.url));
const bin = process.env.GRIMOIRE_BIN ?? (process.platform === "win32" ? "grimoire.exe" : "grimoire");
const port = process.env.BEHAVIOUR_PORT ?? "4343";
const setup = process.env.BEHAVIOUR_SETUP ?? "solo";
if (setup !== "solo" && setup !== "team") throw new Error(`BEHAVIOUR_SETUP is solo or team, not ${setup}`);
const serverPort = process.env.BEHAVIOUR_SERVER_PORT ?? String(Number(port) + 50);

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
// The run's own Grimoire data: credentials and a checkout's copy of a server's intent land here.
// A token in the person's environment would be sent in place of the fixture's own, so none is passed on.
const env = { ...process.env, GRIMOIRE_HOME: join(dir, "home") };
delete env.GRIMOIRE_TOKEN;
cpSync(join(here, "fixture", "repo"), repo, { recursive: true });
for (const name of readdirSync(join(repo, "src"))) {
  if (name.endsWith(".txt")) renameSync(join(repo, "src", name), join(repo, "src", name.slice(0, -4)));
}

function grimoire(...args) {
  return run(args);
}
function run(args, input, cwd = repo) {
  const r = spawnSync(bin, args, { cwd, encoding: "utf8", env, input });
  if (r.status !== 0) throw new Error(`grimoire ${args.join(" ")} failed:\n${r.stdout}${r.stderr}`);
  return r.stdout;
}

// Empty, not the skeleton init writes by default: the fixture's own records fill it.
grimoire("init", "--store", store, "--empty");
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
  // More than one root, so the views show every root and not only the first; and the kinds of the fixture's recipes.
  tool("configure", { roots: records.roots, recipe_kinds: records.recipe_kinds });
  // A large-file limit low enough that src/ready.rs, ten lines with its tests aside, is an omen for the views to show.
  tool("configure", { large_file: 10 });
  for (const req of records.requirements) tool("put_requirement", req);
  // One recipe judged against its files, the other left due, so the views have both.
  tool("judge_recipe", { ref: "recipe:ready-check", outcome: "held" });
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
    // Its proposal names a criterion the fixture's code lacks: other work that may conflict, once
    // another ticket is current, shown in the header and on the requirement's page.
    const named = tool("get", { ref: "req:ready-writes-nothing" }).node;
    tool("put_requirement", { slug: named.slug, title: named.title, statement: named.statement, scenarios: named.scenarios, covered_by: [...named.covered_by, "criterion:src/ready#not-written-yet"], ticket: ref });
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

// A second grimoire on the run's machine, a store of its own named `ledger`, so
// the daemon serves a library of two and the views show its switcher.
const ledger = join(dir, "ledger");
mkdirSync(join(ledger, "src"), { recursive: true });
writeFileSync(join(ledger, "README.md"), "The ledger: a second grimoire in the fixture's library.\n");
run(["init", "--store", "ledger", "--name", "ledger", "--empty"], undefined, ledger);

// A team's: the seeded store is served shared, and the fixture's checkout
// becomes one that names that server, signed in, as a teammate's is.
// Every process started from here is stopped with the rest, whatever ends
// the run: Playwright stopping it, a step failing, or one of them exiting.
const running = [];
let stopping = false;
const stop = (code) => {
  if (stopping) return;
  stopping = true;
  for (const p of running) p.kill();
  rmSync(dir, { recursive: true, force: true });
  process.exit(code);
};
const start = (args, cwd, name) => {
  const p = spawn(bin, args, { cwd, stdio: "inherit", env });
  running.push(p);
  p.on("exit", (code) => {
    if (!stopping) console.error(`the fixture's ${name} stopped (${code}); stopping the rest`);
    stop(code ?? 1);
  });
  return p;
};
process.on("SIGTERM", () => stop(0));
process.on("SIGINT", () => stop(0));

try {
  if (setup === "team") {
    start(["serve", "--shared", "--store", store, "--port", serverPort], dir, "server");
    const origin = `http://127.0.0.1:${serverPort}`;
    // The server says it is up by answering its page, which asks no token; each try is bounded, and so is the wait.
    const deadline = Date.now() + 30_000;
    for (;;) {
      const up = await fetch(origin, { signal: AbortSignal.timeout(2_000) }).then((r) => r.ok, () => false);
      if (up) break;
      if (Date.now() > deadline) throw new Error(`the fixture's server did not answer at ${origin} within 30 seconds`);
      await new Promise((wait) => setTimeout(wait, 200));
    }
    const token = run(["token", "add", "behaviour", "--store", store]).trim();
    writeFileSync(join(repo, ".grimoire.toml"), `server = "${origin}"\n`);
    run(["login"], `${token}\n`);
    // The current ticket is a machine's, not the server's: the checkout takes up the draft, as the person working it would.
    run(["tool", "start_ticket", JSON.stringify({ ref: draft })]);
  }
  start(["serve", "--port", port, "--ui-dir", join(here, "..", "dist")], repo, "daemon");
} catch (e) {
  console.error(e instanceof Error ? e.message : e);
  stop(1);
}
