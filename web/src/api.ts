/**
 * # Module contract
 *
 * ```json
 * {"purpose":"Reach the daemon: one function per tool over POST /api/<tool>, one shared event stream, and a query hook that re-runs on every event.","criteria":[{"id":"tools-only","statement":"Every request the UI makes is a tool call by name; there is no UI-only endpoint."},{"id":"one-stream","statement":"The page holds one EventSource however many queries are mounted, so re-fetches are never starved by idle streams."},{"id":"errors-typed","statement":"A non-2xx response is thrown as an ApiError carrying the tool's error code and message."}]}
 * ```
 */

import { useEffect, useState } from "react";
import type {
  AnyNode,
  CriterionCoverage,
  Finding,
  LintResult,
  Ref,
  RestsOn,
  RestsOnMe,
  SourceLocation,
  StatusResult,
  Summary,
  ToolError,
  Tree,
} from "./types";

export class ApiError extends Error {
  constructor(public readonly body: ToolError, public readonly status: number) {
    super(body.message);
  }
}

export async function call<T>(tool: string, input: unknown = {}): Promise<T> {
  const res = await fetch(`/api/${tool}`, {
    method: "POST",
    headers: { "content-type": "application/json" },
    body: JSON.stringify(input),
  });
  const body = await res.json();
  if (!res.ok) throw new ApiError(body as ToolError, res.status);
  return body as T;
}

export const api = {
  status: () => call<StatusResult>("status"),
  find: (query: string, kinds?: string[], limit = 20) => call<{ nodes: Summary[] }>("find", { query, kinds, limit }),
  get: (ref: Ref) => call<{ node: AnyNode }>("get", { ref }),
  outline: (depth: number, root?: Ref) => call<{ roots: Tree[] }>("outline", { depth, root }),
  restsOn: (ref: Ref, depth?: number) => call<{ nodes: RestsOn[] }>("rests_on", { ref, depth }),
  restsOnMe: (ref: Ref) => call<RestsOnMe>("rests_on_me", { ref }),
  coverage: (ref: Ref) => call<{ criteria?: CriterionCoverage[]; requirements?: Summary[] }>("coverage", { ref }),
  board: (input: { status?: string[]; kinds?: string[]; priority?: string[]; touching?: Ref }) => call<{ tickets: Summary[] }>("board", input),
  relatedTickets: (ref: Ref) => call<{ tickets: Summary[] }>("related_tickets", { ref }),
  lint: () => call<LintResult>("lint"),
  moduleSource: (ref: Ref) => call<SourceLocation>("module_source", { ref }),
  startTicket: (ref: Ref) => call<{ node: AnyNode; rests_on: RestsOn[]; related: Summary[] }>("start_ticket", { ref }),
  closeTicket: (ref: Ref, text: string) => call<{ node: AnyNode; findings: Finding[] }>("close_ticket", { ref, decision: { text } }),
};

// One event stream for the whole page. Browsers allow about six connections
// per origin over HTTP/1.1, so one EventSource per query would starve the
// very fetches the events are meant to trigger.
const listeners = new Set<() => void>();
let stream: EventSource | null = null;

function ensureStream() {
  if (stream) return;
  stream = new EventSource("/api/events");
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
