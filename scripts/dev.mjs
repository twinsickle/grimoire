#!/usr/bin/env node
// This repository's dev loop in one console: the daemon under bacon, rebuilt
// on every Rust change, and the Vite dev server, the UI with hot reload.
// Their output is merged, each line prefixed with where it came from; Ctrl+C,
// or either one stopping, stops both.
//
//   node scripts/dev.mjs        (or `npm run dev:all` in web/)
//
// Stopping is forceful, as bacon's own restarts are: the daemon does not get
// to remove its serving marker, which every command already tolerates, since
// they check that a daemon answers before deferring to it.

import { spawn, spawnSync } from "node:child_process";
import { constants } from "node:os";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";

const root = join(dirname(fileURLToPath(import.meta.url)), "..");
const windows = process.platform === "win32";

const processes = [
  { name: "daemon", command: "bacon", args: ["serve", "--headless"], cwd: root, colour: 36 },
  // Vite's own entry point, run with this Node: no shell, which on Windows
  // would be needed for npm's .cmd and would break paths with spaces.
  { name: "ui", command: process.execPath, args: [join(root, "web", "node_modules", "vite", "bin", "vite.js")], cwd: join(root, "web"), colour: 35 },
];
const width = Math.max(...processes.map((p) => p.name.length));

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
let open = processes.length;

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

const children = processes.map((p) => {
  // Off Windows each child leads its own process group, so the whole group
  // can be stopped at once; on Windows taskkill stops the tree.
  const child = spawn(p.command, p.args, { cwd: p.cwd, detached: !windows, stdio: ["ignore", "pipe", "pipe"] });
  prefix(p, child.stdout, process.stdout);
  prefix(p, child.stderr, process.stderr);
  child.on("error", (e) => {
    write(p, process.stderr, `could not start \`${p.command}\`: ${e.message}`);
    stopAll(1);
  });
  child.on("exit", (code, signal) => {
    if (reason !== null) return;
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
});

// A signal to the loop wins over a child's own ending: on Windows Ctrl+C
// reaches the children too, and whichever is noticed first must not decide.
for (const signal of ["SIGINT", "SIGTERM", "SIGHUP"]) {
  process.on(signal, () => {
    if (reason === null) stopAll(bySignal(signal));
    else process.exitCode = reason = bySignal(signal);
  });
}
