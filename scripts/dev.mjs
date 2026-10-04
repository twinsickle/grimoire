#!/usr/bin/env node
// This repository's dev loop in one console, in the setup a team uses: the
// shared server that holds this project's intent, the daemon under bacon,
// rebuilt on every Rust change, which reads that intent with this checkout's
// code, and the Vite dev server, the UI with hot reload. Their output is
// merged, each line prefixed with where it came from; Ctrl+C, or any one
// stopping, stops them all.
//
//   node scripts/dev.mjs        (or `npm run dev:all` in web/)
//
// The server is the build last installed (`cargo install --path crates/cli`),
// as a host runs a release, and is run from a copy, so installing again while
// the loop runs works: a running program cannot be replaced on Windows. The
// loop watches the installed build, and when an install replaces it, copies
// it again and restarts the server alone, the daemon and the UI running on;
// the store is on disk, so nothing is lost. Its address is the one `.grimoire.toml` names; a project
// file naming a server that is not this machine, or naming a store, starts
// none, and the loop is then the daemon and the UI alone. Whoever runs the
// loop is signed in to the server if the server says they are not, with a
// token of their own made for it; no token is ever revoked here.
//
// Stopping is forceful, as bacon's own restarts are: the daemon does not get
// to remove its serving marker, which every command already tolerates, since
// they check that a daemon answers before deferring to it.

import { spawn, spawnSync } from "node:child_process";
import { copyFileSync, existsSync, mkdirSync, readFileSync, statSync, watchFile } from "node:fs";
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

// The build last installed, or the one GRIMOIRE_BIN names.
const installed = process.env.GRIMOIRE_BIN ?? join(process.env.CARGO_HOME ?? join(homedir(), ".cargo"), "bin", exe);
if (local && !existsSync(installed)) refuse(`${installed} does not exist; install it with \`cargo install --path crates/cli\``);

// The store the server serves: this project's own, where it has always been.
const store = process.env.GRIMOIRE_DEV_STORE ?? "grimoire";

// The server runs a copy of the installed build, so the installed one can be
// replaced while the loop runs.
const serverBin = join(root, "target", "dev-server", exe);
if (local) {
  mkdirSync(dirname(serverBin), { recursive: true });
  copyFileSync(installed, serverBin);
}

const server = { name: "server", command: serverBin, args: ["serve", "--shared", "--store", store, "--port", address?.port || "80"], cwd: root, colour: 33 };
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
const children = [];

/** Record why the loop is stopping, once; later events do not change it. */
function stopAll(code) {
  if (reason !== null) return;
  reason = code;
  process.exitCode = code;
  children.forEach((c) => stopTree(c, "SIGTERM"));
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
  const child = spawn(p.command, p.args, { cwd: p.cwd, detached: !windows, stdio: ["ignore", "pipe", "pipe"] });
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
    await fetch(new URL("/api/status", address), { method: "POST", headers: { "content-type": "application/json" }, body: "{}" });
    return true;
  } catch {
    return false;
  }
}

/** Run the installed build in this checkout and wait for it. */
const grimoire = (args, input) => spawnSync(installed, args, { cwd: root, encoding: "utf8", input });

/**
 * Sign whoever runs the loop in to the server, if the server says they are
 * not: a token of their own, made beside the store and kept outside the
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
  const made = grimoire(["token", "add", name, "--store", store]);
  const login = made.status === 0 ? grimoire(["login"], made.stdout) : made;
  const signed = login.status === 0 ? grimoire(["tool", "status"]) : login;
  if (signed.status === 0) return true;
  write(server, process.stderr, `could not sign in: ${(signed.stderr || signed.stdout).trim()}`);
  return false;
}

// The server first: the daemon reads its intent from it. Then the rest.
if (!local) write(server, process.stdout, address ? `${address.origin} is not this machine: no server is started here` : ".grimoire.toml names a store: no server is started, and the daemon serves that store");
if (local) {
  start(server);
  const deadline = Date.now() + 30_000;
  while (reason === null && !(await answering())) {
    if (Date.now() > deadline) {
      write(server, process.stderr, `no answer at ${address.origin} after 30 seconds; stopping`);
      stopAll(1);
    }
    await new Promise((wait) => setTimeout(wait, 200));
  }
  if (reason === null && !signIn()) stopAll(1);
}
if (reason === null) processes.forEach(start);

/** Wait until the server answers again, or the loop is stopping; false at the deadline. */
async function untilAnswering(seconds) {
  const deadline = Date.now() + seconds * 1000;
  while (reason === null && !(await answering())) {
    if (Date.now() > deadline) return false;
    await new Promise((wait) => setTimeout(wait, 200));
  }
  return reason === null;
}

/**
 * Restart the server alone on the build just installed: stop it, copy the new
 * build over its copy (retried, since an install may still hold the file a
 * moment), start it, and wait for it to answer. Bacon and the UI run on; the
 * daemon reconnects to the server by itself.
 */
async function restartServer(child) {
  server.restarting = true;
  write(server, process.stdout, "a new build was installed: restarting the server on it");
  stopTree(child, "SIGTERM");
  await new Promise((done) => (child.exitCode !== null || child.signalCode !== null ? done() : child.once("exit", done)));
  for (let tries = 0; ; tries++) {
    try {
      copyFileSync(installed, serverBin);
      break;
    } catch (e) {
      if (tries >= 20) {
        write(server, process.stderr, `could not copy the new build: ${e.message}; stopping`);
        stopAll(1);
        return null;
      }
      await new Promise((wait) => setTimeout(wait, 500));
    }
  }
  server.restarting = false;
  if (reason !== null) return null;
  const next = start(server);
  if (await untilAnswering(30)) write(server, process.stdout, `restarted, answering at ${address.origin}`);
  else if (reason === null) {
    write(server, process.stderr, `no answer at ${address.origin} after 30 seconds; stopping`);
    stopAll(1);
  }
  return next;
}

// Watch the installed build: an install replaces it, and the server follows once the file has settled.
if (local && reason === null) {
  let running = children[0];
  let settle = null;
  let restarting = Promise.resolve();
  const stamp = () => {
    try {
      const s = statSync(installed);
      return `${s.mtimeMs}:${s.size}`;
    } catch {
      return null;
    }
  };
  let seen = stamp();
  watchFile(installed, { interval: 1000 }, () => {
    if (reason !== null) return;
    clearTimeout(settle);
    // Two seconds with no further change: the install has finished writing it.
    settle = setTimeout(() => {
      const now = stamp();
      if (now === null || now === seen) return;
      seen = now;
      restarting = restarting.then(async () => {
        const next = await restartServer(running);
        if (next) running = next;
      });
    }, 2000);
  }).unref();
}
