#!/usr/bin/env node
// This repository's dev loop in one console, in the setup a team uses: the
// team's server that holds this project's intent, run as a team runs it, a
// container of the image this checkout builds (compose.yaml); the daemon under
// bacon, rebuilt on every Rust change, which reads that intent with this
// checkout's code; and the Vite dev server, the UI with hot reload. Their
// output is merged, each line prefixed with where it came from; Ctrl+C, or any
// one stopping, stops them all, the server's container included.
//
//   node scripts/dev.mjs                            (or `npm run dev:all` in web/)
//   GRIMOIRE_DEV_SERVER=fast node scripts/dev.mjs
//
// The server is started with `docker compose up --detach --build`, which
// rebuilds only what changed, and its log is followed as its output. With
// GRIMOIRE_DEV_SERVER=fast it runs instead from a debug build made in a Rust
// container (compose.fast.yaml), so a change to the server's side needs no
// image rebuilt. Either way its library is the volume `grimoire_library`, and
// `docker compose up -d` while the loop runs restarts it: the loop follows the
// new container. It also serves the project's site, how it is meant to be
// used, on the port after its own. Its address is the one `.grimoire.toml`
// names, `/g/<name>` naming the grimoire in the server's library; a project
// file naming a server that is not this machine, or naming a store, starts
// none, and the loop is then the daemon and the UI alone. Whoever runs the
// loop is signed in to the server if the server says they are not, with a
// token of their own made inside the container; no token is ever revoked here.
//
// Stopping is forceful, as bacon's own restarts are: the daemon does not get
// to remove its serving marker, which every command already tolerates, since
// they check that a daemon answers before deferring to it. The server's
// container is stopped with `docker compose stop`, which lets it unmark.

import { spawn, spawnSync } from "node:child_process";
import { existsSync, readFileSync } from "node:fs";
import { constants, homedir, hostname } from "node:os";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";

const root = join(dirname(fileURLToPath(import.meta.url)), "..");
const windows = process.platform === "win32";
const exe = windows ? "grimoire.exe" : "grimoire";

/** Stop before anything is started, saying why. */
function refuse(why) {
  process.stderr.write(`dev: ${why}\n`);
  process.exit(1);
}

// The server this project names: the one address, written once, in the file every command reads.
// A project file that names a store, as when going back from the team setup, has no server to start.
const named = readFileSync(join(root, ".grimoire.toml"), "utf8").match(/^\s*server\s*=\s*"([^"]+)"/m);
const address = named ? new URL(named[1]) : null;
const local = address !== null && ["127.0.0.1", "localhost", "[::1]"].includes(address.hostname);
// The grimoire the address names in the server's library, whose store is /data/<name> in the container.
const grimoireName = address?.pathname.match(/\/g\/([a-z0-9-]+)\/?$/)?.[1];
if (local && !grimoireName) refuse(`${address.href} names no grimoire: a team's server serves a library, so its address ends in /g/<name>`);

// The build last installed, or the one GRIMOIRE_BIN names: it signs the person in, as any checkout does.
const installed = process.env.GRIMOIRE_BIN ?? join(process.env.CARGO_HOME ?? join(homedir(), ".cargo"), "bin", exe);
if (local && !existsSync(installed)) refuse(`${installed} does not exist; install it with \`cargo install --path crates/cli\``);

// The server's container: from the image, or on the fast path from a debug build made in a container.
const fast = process.env.GRIMOIRE_DEV_SERVER === "fast";
const composeFiles = ["-f", "compose.yaml", ...(fast ? ["-f", "compose.fast.yaml"] : [])];
const port = address?.port || "80";
// Beside it, the project's site, how it is meant to be used, as someone outside the project reads it: one port up.
const composeEnv = { ...process.env, GRIMOIRE_PORT: port, GRIMOIRE_SITE_PORT: String(Number(port) + 1) };
const compose = (args) => ["compose", ...composeFiles, ...args];

// The server's output is its container's log, followed; `since` keeps a log followed again from repeating itself.
let since = null;
const server = {
  name: "server",
  command: "docker",
  get args() {
    return compose(["logs", "--follow", "--no-log-prefix", ...(since ? ["--since", since] : []), "server"]);
  },
  cwd: root,
  env: composeEnv,
  colour: 33,
};
const processes = [
  { name: "daemon", command: "bacon", args: ["serve", "--headless"], cwd: root, colour: 36 },
  // Vite's own entry point, run with this Node: no shell, which on Windows
  // would be needed for npm's .cmd and would break paths with spaces.
  { name: "ui", command: process.execPath, args: [join(root, "web", "node_modules", "vite", "bin", "vite.js")], cwd: join(root, "web"), colour: 35 },
];
const width = Math.max(...[server, ...processes].map((p) => p.name.length));

// eslint-disable-next-line no-control-regex
const ANSI = /\x1b\[[0-9;?]*[ -/]*[@-~]/g;

/** A line tagged with its process, coloured only where the destination is a terminal. */
function write(p, out, line) {
  const name = p.name.padEnd(width);
  if (out.isTTY) out.write(`\x1b[${p.colour}m${name} |\x1b[0m ${line}\n`);
  else out.write(`${name} | ${line.replace(ANSI, "")}\n`);
}

/**
 * Copy a stream to ours line by line, each tagged with its process. A lone
 * carriage return (a progress line redrawn in place) starts a new tagged
 * line, so it can never overwrite a tag.
 */
function prefix(p, stream, out) {
  let pending = "";
  stream.setEncoding("utf8");
  stream.on("data", (chunk) => {
    const lines = (pending + chunk).split(/\r\n|\n|\r/);
    pending = lines.pop() ?? "";
    for (const line of lines) write(p, out, line);
  });
  stream.on("end", () => {
    if (pending) write(p, out, pending);
  });
}

/** The conventional exit code for a signal: 128 plus its number. */
const bySignal = (signal) => 128 + (constants.signals[signal] ?? 1);

/** Windows reports a process ended by Ctrl+C as STATUS_CONTROL_C_EXIT. */
const CONTROL_C_EXIT = 0xc000013a;

/**
 * Stop a process and everything it started: bacon runs cargo, which runs the
 * daemon, and killing bacon alone would leave the daemon holding its port.
 * Off Windows the saved process group is signalled even when its leader has
 * already exited, since its descendants may not have. On Windows taskkill
 * finds the tree through the leader, so descendants of a leader that exited
 * on its own are out of its reach; only a Job Object, which needs native
 * code, would hold them.
 */
function stopTree(child, signal) {
  if (windows) {
    if (child.exitCode === null && child.signalCode === null) spawnSync("taskkill", ["/pid", String(child.pid), "/T", "/F"], { stdio: "ignore" });
    return;
  }
  try {
    process.kill(-child.pid, signal);
  } catch (e) {
    if (e.code !== "ESRCH") child.kill(signal);
  }
}

let reason = null;
let open = 0;
let serverStarted = false;
const children = [];

/** Record why the loop is stopping, once; later events do not change it. */
function stopAll(code) {
  if (reason !== null) return;
  reason = code;
  process.exitCode = code;
  children.forEach((c) => stopTree(c, "SIGTERM"));
  // The container outlives the log that follows it: it is stopped here, its library staying on the volume.
  if (serverStarted) spawnSync("docker", compose(["stop", "server"]), { cwd: root, env: composeEnv, stdio: "ignore" });
  // Anything still running at the deadline is killed outright, and the loop
  // exits then even if a stream never closes.
  setTimeout(() => {
    children.forEach((c) => stopTree(c, "SIGKILL"));
    setTimeout(() => process.exit(reason), 500).unref();
  }, 5000).unref();
}

function start(p) {
  open += 1;
  // Off Windows each child leads its own process group, so the whole group
  // can be stopped at once; on Windows taskkill stops the tree.
  const child = spawn(p.command, p.args, { cwd: p.cwd, env: p.env ?? process.env, detached: !windows, stdio: ["ignore", "pipe", "pipe"] });
  children.push(child);
  prefix(p, child.stdout, process.stdout);
  prefix(p, child.stderr, process.stderr);
  child.on("error", (e) => {
    write(p, process.stderr, `could not start \`${p.command}\`: ${e.message}`);
    stopAll(1);
  });
  child.on("exit", (code, signal) => {
    // A restart stops the process on purpose: the loop goes on.
    if (reason !== null || p.restarting) return;
    // Only 0 to 255 means anything as an exit status; a process killed outright
    // on Windows reports -1 (4294967295), which the shell would misread.
    const own = code === null ? 1 : code >= 0 && code <= 255 ? code : 1;
    const status = signal ? bySignal(signal) : code === CONTROL_C_EXIT || code === -1073741510 ? 130 : own;
    write(p, process.stdout, `stopped (${signal ?? `exit ${code}`}); stopping the rest`);
    stopAll(status);
  });
  // `close` follows the child's output closing, so nothing it wrote is lost;
  // when every child has closed, the loop ends by itself with the recorded code.
  child.on("close", () => {
    open -= 1;
    if (open === 0) process.exitCode = reason ?? 0;
  });
  return child;
}

// A signal to the loop wins over a child's own ending: on Windows Ctrl+C
// reaches the children too, and whichever is noticed first must not decide.
for (const signal of ["SIGINT", "SIGTERM", "SIGHUP"]) {
  process.on(signal, () => {
    if (reason === null) stopAll(bySignal(signal));
    else process.exitCode = reason = bySignal(signal);
  });
}

/** Whether the server answers at all: any HTTP answer will do, a refusal included. */
async function answering() {
  try {
    await fetch(`${address.href.replace(/\/$/, "")}/api/status`, { method: "POST", headers: { "content-type": "application/json" }, body: "{}" });
    return true;
  } catch {
    return false;
  }
}

/** Run the installed build in this checkout and wait for it. */
const grimoire = (args, input) => spawnSync(installed, args, { cwd: root, encoding: "utf8", input });

/**
 * Sign whoever runs the loop in to the server, if the server says they are
 * not: a token of their own, made in the server's container and kept outside the
 * repository by `login`. A read through the server is what says whether they
 * are; it also fetches this checkout's copy of the intent, so the daemon
 * finds one when it opens. Only the server refusing the sign-in is taken for
 * not being signed in: anything else that fails is said and stops the loop.
 * No token is revoked, so one another checkout uses is never taken from it;
 * a token left over is removed by hand with `grimoire token revoke`.
 */
function signIn() {
  const asked = grimoire(["tool", "status"]);
  if (asked.status === 0) return true;
  const said = (asked.stderr || asked.stdout).trim();
  if (!said.includes("refused this machine")) {
    write(server, process.stderr, `the server's intent could not be read: ${said}`);
    return false;
  }
  // A slug of its own: who and where, and when, so no two are ever the same.
  const who = `${process.env.USER ?? process.env.USERNAME ?? "someone"}-${hostname()}`.toLowerCase().replace(/[^a-z0-9]+/g, "-").replace(/^-+|-+$/g, "").slice(0, 40);
  const name = `dev-${who}-${Date.now().toString(36)}`;
  write(server, process.stdout, `not signed in to the server: making the token \`${name}\` and signing in`);
  // Made inside the container, by the server's own binary, as a team's host makes one.
  const made = spawnSync("docker", compose(["exec", "-T", "server", "grimoire", "token", "add", name, "--store", `/data/${grimoireName}`]), { cwd: root, env: composeEnv, encoding: "utf8" });
  const login = made.status === 0 ? grimoire(["login"], made.stdout) : made;
  const signed = login.status === 0 ? grimoire(["tool", "status"]) : login;
  if (signed.status === 0) return true;
  write(server, process.stderr, `could not sign in: ${(signed.stderr || signed.stdout).trim()}`);
  return false;
}


/** Run docker compose to its end, its output tagged as the server's; its exit code. */
function composeRun(args) {
  return new Promise((done) => {
    const child = spawn("docker", compose(args), { cwd: root, env: composeEnv, stdio: ["ignore", "pipe", "pipe"] });
    prefix(server, child.stdout, process.stdout);
    prefix(server, child.stderr, process.stderr);
    child.on("error", (e) => {
      write(server, process.stderr, `could not run docker: ${e.message}`);
      done(1);
    });
    child.on("close", (code) => done(code ?? 1));
  });
}

/** Wait until the server answers again, or the loop is stopping; false at the deadline. */
async function untilAnswering(seconds) {
  const deadline = Date.now() + seconds * 1000;
  while (reason === null && !(await answering())) {
    if (Date.now() > deadline) return false;
    await new Promise((wait) => setTimeout(wait, 500));
  }
  return reason === null;
}

/**
 * Follow the server's log for as long as the loop runs. The log ends when its
 * container stops, as when `docker compose up -d` replaces it with a new one:
 * the loop then waits for the server to answer again and follows the new
 * container, the daemon and the UI running on, and stops only when it does not.
 */
function follow(child) {
  server.restarting = true;
  child.on("exit", async () => {
    if (reason !== null) return;
    since = new Date().toISOString();
    write(server, process.stdout, "the server's container stopped: waiting for it to answer again");
    if (await untilAnswering(120)) {
      write(server, process.stdout, `answering again at ${address.href}`);
      follow(start(server));
    } else if (reason === null) {
      write(server, process.stderr, `no answer at ${address.href} after 120 seconds; stopping`);
      stopAll(1);
    }
  });
}

// The server first: the daemon reads its intent from it. Then the rest.
if (!local) write(server, process.stdout, address ? `${address.origin} is not this machine: no server is started here` : ".grimoire.toml names a store: no server is started, and the daemon serves that store");
if (local) {
  write(server, process.stdout, fast ? "starting from a debug build made in a container (compose.fast.yaml)" : "starting from the image this checkout builds (compose.yaml)");
  serverStarted = true;
  if ((await composeRun(["up", "--detach", "--build", "server"])) !== 0) {
    write(server, process.stderr, "the server's container did not start; stopping");
    stopAll(1);
  }
  if (reason === null) follow(start(server));
  if (reason === null && !(await untilAnswering(60))) {
    if (reason === null) write(server, process.stderr, `no answer at ${address.href} after 60 seconds; stopping`);
    stopAll(1);
  }
  if (reason === null && !signIn()) stopAll(1);
}
if (reason === null) processes.forEach(start);
