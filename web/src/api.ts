/**
 * # Module contract
 *
 * ```json
 * {"purpose":"Reach the daemon: one function per tool over POST /api/<tool>, for the grimoire the page's address names, one shared event stream, and a query hook that re-runs on every event.","criteria":[{"id":"tools-only","statement":"Every request the UI makes is a tool call by name; there is no UI-only endpoint."},{"id":"one-stream","statement":"The page holds one EventSource however many queries are mounted, so re-fetches are never starved by idle streams."},{"id":"tool-error-is-api-error","statement":"A non-2xx response whose body is a tool's error is thrown as an ApiError carrying its code and message."},{"id":"unreachable-when-daemon-away","statement":"A request that fails to connect, or a server error whose body is no tool's error, as a proxy answers while the daemon is away, is thrown as Unreachable, saying the daemon is not answering."},{"id":"non-json-answer-names-status","statement":"Any other answer that is not JSON is thrown as an error naming its status and the start of its body."},{"id":"no-answer-is-json-syntax-error","statement":"No answer is thrown as a JSON syntax error."},{"id":"checkout-from-the-address","statement":"Every call names a checkout: the one the page's address names as ?checkout=, kept for the tab."},{"id":"checkout-from-the-first-status","statement":"Every call names a checkout: when the page's address names none, the one the first status answers for."},{"id":"event-stream-names-it","statement":"The event stream names a checkout as every call does."},{"id":"learned-before-any-other-call","statement":"The checkout the first status answers for is learned before any other call is sent or the stream is opened."},{"id":"status-names-it-once-known","statement":"status itself names the checkout once it is known."},{"id":"refused-checkout-learned-again-and-resent","statement":"A call refused for the checkout it names forgets that checkout, learns it again and is sent once more."},{"id":"stream-reopens","statement":"When the browser closes the event stream for good, as it does when a reconnect is answered with anything but an event stream, it is opened again after a backoff of one second doubling to ten, reset on each opening; every opening counts as an event."},{"id":"away-on-unreachable-call","statement":"useAway is true from a call thrown as Unreachable."},{"id":"away-on-stream-error","statement":"useAway is true from an error on the event stream."},{"id":"away-ends-on-newer-answer","statement":"useAway ends on an answer from the daemon to a request started after the last Unreachable call or stream error."},{"id":"away-ends-on-opened-stream","statement":"useAway ends on the event stream being opened."},{"id":"late-answer-leaves-away","statement":"An answer to a request already on its way when the daemon went away leaves useAway true."},{"id":"away-keeps-last-data","statement":"While useAway is true, a query whose deps are unchanged keeps its last data and reports no error."},{"id":"away-reports-unreachable-without-data","statement":"While useAway is true, a query with no data for its deps reports the Unreachable error."},{"id":"retries-while-away","statement":"While useAway is true, every listener of onEvent, and so every query, is told every two seconds, so the page asks again until the daemon answers whether or not an event comes; once it answers, the retries stop."},{"id":"grimoire-base","statement":"A page opened at /g/<name>/ makes every tool call and opens its event stream under /g/<name>/api/, that grimoire's in the daemon's library; one opened elsewhere uses the bare /api/, the daemon's default grimoire; grimoireBase reads the name only from a leading /g/ followed by a slug."}]}
 * ```
 */

import { useEffect, useState, useSyncExternalStore } from "react";
import type { NodeAnswer, Priority, Ref, ToolAnswers, ToolError } from "./types";

export class ApiError extends Error {
  constructor(public readonly body: ToolError, public readonly status: number) {
    super(body.message);
  }
}

/** The daemon did not answer: no connection, or a proxy's server error while it is away. */
export class Unreachable extends Error {
  constructor(why: string) {
    super(`The daemon is not answering (${why}); this page retries until it does.`);
  }
}

/** Whether a parsed body is a tool's error, as the daemon answers one. */
function isToolError(body: unknown): body is ToolError {
  return typeof body === "object" && body !== null && typeof (body as ToolError).error === "string" && typeof (body as ToolError).message === "string";
}

// Whether the daemon is answering, for the whole page: set by every call and by the event stream.
let away = false;
const awayListeners = new Set<() => void>();
// While it is away, every query is asked again on this timer, so the page recovers even when no event comes.
let retrying: ReturnType<typeof setInterval> | null = null;
export const RETRY_MS = 2000;
// Each request is numbered as it starts. A failure remembers the newest number started by then, and only a request
// started after it is taken as the daemon answering again: a slow one already on its way when it went away is not.
let started = 0;
let failedAt = 0;

/** The daemon did not answer: away, and every request started up to now is from before. */
function failed() {
  failedAt = started;
  setAway(true);
}

/** The daemon answered request `n`: back, unless that request was already on its way when it went away. */
function answered(n: number) {
  if (n > failedAt) setAway(false);
}

function setAway(now: boolean) {
  if (away === now) return;
  away = now;
  if (now && retrying === null) retrying = setInterval(notify, RETRY_MS);
  if (!now && retrying !== null) {
    clearInterval(retrying);
    retrying = null;
  }
  awayListeners.forEach((l) => l());
}

function subscribeAway(l: () => void): () => void {
  awayListeners.add(l);
  return () => {
    awayListeners.delete(l);
  };
}

/** True while the daemon is not answering, from any request or the event stream. */
export function useAway(): boolean {
  return useSyncExternalStore(subscribeAway, () => away);
}

/**
 * The grimoire an address names in the daemon's library: `/g/<name>` when the
 * path starts with one, else "" for the daemon's default on the bare routes.
 */
export function grimoireBase(pathname: string): string {
  const m = pathname.match(/^\/g\/([a-z0-9]+(?:-[a-z0-9]+)*)(?=\/|$)/);
  return m ? `/g/${m[1]}` : "";
}

/** The grimoire this page reads, from the address it was opened at; switching grimoires opens a page of its own. */
export const base: string = typeof window === "undefined" ? "" : grimoireBase(window.location.pathname);

const CHECKOUT_KEY = `grimoire.checkout:${base}`;

/** The checkout the page's address names, else the one this tab chose before, else none yet. */
function checkoutAsked(): string | null {
  if (typeof window === "undefined") return null;
  const named = new URLSearchParams(window.location.search).get("checkout");
  try {
    if (named) sessionStorage.setItem(CHECKOUT_KEY, named);
    return named ?? sessionStorage.getItem(CHECKOUT_KEY);
  } catch {
    return named;
  }
}

// The checkout this page reads (req:machine-checkouts): a daemon may serve several of one grimoire.
let checkout: string | null = checkoutAsked();
let learning: Promise<boolean> | null = null;

/** The checkout this page names in its calls, once it is known. */
export function currentCheckout(): string | null {
  return checkout;
}

/** Learn the checkout from the first status, which is answered without one, before any other call goes; whether it was. */
function learnCheckout(): Promise<boolean> {
  learning ??= call("status").then(
    (s) => {
      checkout ??= s.repository;
      return true;
    },
    () => {
      // Not learned: the next call tries again.
      learning = null;
      return false;
    },
  );
  return learning;
}

/** Call a tool by name; its answer's type follows from the name, generated from the Rust answer types. */
export async function call<K extends keyof ToolAnswers>(tool: K, input: unknown = {}): Promise<ToolAnswers[K]> {
  try {
    return await callOnce(tool, input);
  } catch (e) {
    // The checkout this page named is no longer served, or was never one: forget it, learn it again, ask once more.
    if (e instanceof ApiError && checkout !== null && e.body.error === "invalid" && e.body.details.some((d) => d.field === "checkout")) {
      forgetCheckout();
      return callOnce(tool, input);
    }
    throw e;
  }
}

function forgetCheckout() {
  checkout = null;
  learning = null;
  try {
    sessionStorage.removeItem(CHECKOUT_KEY);
  } catch {
    // Nothing kept, nothing to forget.
  }
}

async function callOnce<K extends keyof ToolAnswers>(tool: K, input: unknown): Promise<ToolAnswers[K]> {
  if (checkout === null && tool !== "status") await learnCheckout();
  const named = checkout !== null && typeof input === "object" && input !== null && !Array.isArray(input) ? { ...input, checkout } : input;
  const n = ++started;
  let res: Response;
  let text: string;
  try {
    res = await fetch(`${base}/api/${tool}`, {
      method: "POST",
      headers: { "content-type": "application/json" },
      body: JSON.stringify(named),
    });
    text = await res.text();
  } catch (e) {
    failed();
    throw new Unreachable(e instanceof Error ? e.message : String(e));
  }
  let body: unknown;
  let parsed = true;
  try {
    body = JSON.parse(text);
  } catch {
    parsed = false;
  }
  // A server error that is no tool's is the proxy in front of a daemon that is away.
  if (!res.ok && !(parsed && isToolError(body))) {
    if (res.status >= 500) {
      failed();
      throw new Unreachable(`${res.status}${text.trim() ? `: ${text.trim().slice(0, 80)}` : ""}`);
    }
    answered(n);
    throw new Error(`${tool} answered ${res.status} with ${text.trim() ? `"${text.trim().slice(0, 120)}"` : "nothing"}, not a tool's answer`);
  }
  answered(n);
  if (!parsed) throw new Error(`${tool} answered ${res.status} with something that is not JSON: "${text.trim().slice(0, 120)}"`);
  if (!res.ok) throw new ApiError(body as ToolError, res.status);
  return body as ToolAnswers[K];
}

export const api = {
  status: () => call("status"),
  find: (query: string, kinds?: string[], limit = 20) => call("find", { query, kinds, limit }),
  // Given one `ref`, get answers one node; `{refs}` would answer several (req:tool-get).
  get: (ref: Ref, ticket?: string) => call("get", ticket ? { ref, ticket } : { ref }) as Promise<NodeAnswer>,
  outline: (input: { depth?: number; root?: Ref; pages_only?: boolean; ticket?: string }) => call("outline", input),
  restsOn: (ref: Ref, depth?: number, ticket?: string) => call("rests_on", ticket ? { ref, depth, ticket } : { ref, depth }),
  restsOnMe: (ref: Ref) => call("rests_on_me", { ref }),
  coverage: (ref: Ref, ticket?: string) => call("coverage", ticket ? { ref, ticket } : { ref }),
  board: (input: { status?: string[]; kinds?: string[]; priority?: string[]; touching?: Ref; bottled?: number }) => call("board", input),
  updateTicket: (ref: Ref, fields: { priority?: Priority }) => call("update_ticket", { ref, ...fields }),
  reorder: (priority: Priority, order: Ref[]) => call("reorder", { priority, order }),
  relatedTickets: (ref: Ref) => call("related_tickets", { ref }),
  lint: () => call("lint"),
  moduleSource: (ref: Ref) => call("module_source", { ref }),
  source: (ref: Ref) => call("source", { ref }),
};

// One event stream for the whole page. Browsers allow about six connections
// per origin over HTTP/1.1, so one EventSource per query would starve the
// very fetches the events are meant to trigger.
const listeners = new Set<() => void>();
let stream: EventSource | null = null;
let backoff = 1000;

/** Tell every listener, so every query re-runs. */
function notify() {
  listeners.forEach((l) => l());
}

let streamWaits = false;

function ensureStream() {
  if (stream || streamWaits) return;
  // The stream names the page's checkout, so it waits until the checkout is known.
  if (checkout === null) {
    streamWaits = true;
    void learnCheckout().then((learned) => {
      streamWaits = false;
      if (learned) return ensureStream();
      // The daemon did not answer: try again later, as a closed stream is.
      const wait = backoff;
      backoff = Math.min(backoff * 2, 10_000);
      setTimeout(ensureStream, wait);
    });
    return;
  }
  const opened = new EventSource(`${base}/api/events${checkout ? `?checkout=${encodeURIComponent(checkout)}` : ""}`);
  stream = opened;
  opened.onmessage = notify;
  // On a daemon restart the browser reconnects by itself; the graph may have
  // changed meanwhile, so a reopen counts as an event too.
  opened.onopen = () => {
    backoff = 1000;
    setAway(false);
    notify();
  };
  // A reconnect answered with anything but an event stream, as a dev proxy
  // answers while the daemon restarts, closes the stream for good: open it
  // again, a little later each time, until the daemon is back.
  opened.onerror = () => {
    failed();
    if (opened.readyState !== EventSource.CLOSED) return;
    stream = null;
    const wait = backoff;
    backoff = Math.min(backoff * 2, 10_000);
    setTimeout(ensureStream, wait);
  };
}

/** Be told of every daemon event, and of every retry while it is away; the page's one stream is opened on the first. */
export function onEvent(l: () => void): () => void {
  ensureStream();
  listeners.add(l);
  return () => {
    listeners.delete(l);
  };
}

/** A counter that increments on every daemon event, for re-running queries. */
export function useEvents(): number {
  const [tick, setTick] = useState(0);
  useEffect(() => onEvent(() => setTick((t) => t + 1)), []);
  return tick;
}

/** Run a query, again whenever `deps` change or the daemon emits an event. */
export function useQuery<T>(fn: () => Promise<T>, deps: unknown[]): { data: T | undefined; error: ApiError | Error | undefined; loading: boolean } {
  const tick = useEvents();
  // The answer remembers which request it answered; loading is "not this one",
  // derived rather than set, so the previous data stays visible meanwhile.
  const asked = JSON.stringify(deps);
  const key = JSON.stringify([...deps, tick]);
  const [state, setState] = useState<{ key?: string; asked?: string; data?: T; error?: ApiError | Error }>({});
  useEffect(() => {
    let live = true;
    fn().then(
      (data) => live && setState({ key, asked, data }),
      // While the daemon is away, the last answer to the same question stays shown.
      (error) => live && setState((was) => (error instanceof Unreachable && was.asked === asked && was.data !== undefined ? { key, asked, data: was.data } : { key, asked, error })),
    );
    return () => {
      live = false;
    };
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [key]);
  const fresh = state.key === key;
  return { data: state.data, error: fresh ? state.error : undefined, loading: !fresh };
}
