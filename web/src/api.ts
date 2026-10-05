/**
 * # Module contract
 *
 * ```json
 * {"purpose":"Reach the daemon: one function per tool over POST /api/<tool>, for the grimoire the page's address names, one shared event stream, and a query hook that re-runs on every event.","criteria":[{"id":"tools-only","statement":"Every request the UI makes is a tool call by name; there is no UI-only endpoint."},{"id":"one-stream","statement":"The page holds one EventSource however many queries are mounted, so re-fetches are never starved by idle streams."},{"id":"errors-typed","statement":"A non-2xx response is thrown as an ApiError carrying the tool's error code and message."},{"id":"grimoire-base","statement":"A page opened at /g/<name>/ makes every tool call and opens its event stream under /g/<name>/api/, that grimoire's in the daemon's library; one opened elsewhere uses the bare /api/, the daemon's default grimoire; grimoireBase reads the name only from a leading /g/ followed by a slug."}]}
 * ```
 */

import { useEffect, useState } from "react";
import type { Priority, Ref, ToolAnswers, ToolError } from "./types";

export class ApiError extends Error {
  constructor(public readonly body: ToolError, public readonly status: number) {
    super(body.message);
  }
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

/** Call a tool by name; its answer's type follows from the name, generated from the Rust answer types. */
export async function call<K extends keyof ToolAnswers>(tool: K, input: unknown = {}): Promise<ToolAnswers[K]> {
  const res = await fetch(`${base}/api/${tool}`, {
    method: "POST",
    headers: { "content-type": "application/json" },
    body: JSON.stringify(input),
  });
  const body = await res.json();
  if (!res.ok) throw new ApiError(body as ToolError, res.status);
  return body as ToolAnswers[K];
}

export const api = {
  status: () => call("status"),
  find: (query: string, kinds?: string[], limit = 20) => call("find", { query, kinds, limit }),
  get: (ref: Ref, ticket?: string) => call("get", ticket ? { ref, ticket } : { ref }),
  outline: (input: { depth?: number; root?: Ref; pages_only?: boolean; ticket?: string }) => call("outline", input),
  restsOn: (ref: Ref, depth?: number) => call("rests_on", { ref, depth }),
  restsOnMe: (ref: Ref) => call("rests_on_me", { ref }),
  coverage: (ref: Ref) => call("coverage", { ref }),
  board: (input: { status?: string[]; kinds?: string[]; priority?: string[]; touching?: Ref; closed?: number }) => call("board", input),
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

function ensureStream() {
  if (stream) return;
  stream = new EventSource(`${base}/api/events`);
  const notify = () => listeners.forEach((l) => l());
  stream.onmessage = notify;
  // On a daemon restart the browser reconnects by itself; the graph may have
  // changed meanwhile, so a reopen counts as an event too.
  stream.onopen = notify;
}

/** A counter that increments on every daemon event, for re-running queries. */
export function useEvents(): number {
  const [tick, setTick] = useState(0);
  useEffect(() => {
    ensureStream();
    const l = () => setTick((t) => t + 1);
    listeners.add(l);
    return () => {
      listeners.delete(l);
    };
  }, []);
  return tick;
}

/** Run a query, again whenever `deps` change or the daemon emits an event. */
export function useQuery<T>(fn: () => Promise<T>, deps: unknown[]): { data: T | undefined; error: ApiError | Error | undefined; loading: boolean } {
  const tick = useEvents();
  // The answer remembers which request it answered; loading is "not this one",
  // derived rather than set, so the previous data stays visible meanwhile.
  const key = JSON.stringify([...deps, tick]);
  const [state, setState] = useState<{ key?: string; data?: T; error?: ApiError | Error }>({});
  useEffect(() => {
    let live = true;
    fn().then(
      (data) => live && setState({ key, data }),
      (error) => live && setState({ key, error }),
    );
    return () => {
      live = false;
    };
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [key]);
  const fresh = state.key === key;
  return { data: state.data, error: fresh ? state.error : undefined, loading: !fresh };
}
