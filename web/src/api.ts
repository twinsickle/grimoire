// One function per tool over POST /api/<tool>, plus the event stream. Every
// tool the UI uses is one the agents use; there is no UI-only endpoint.

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

/** A counter that increments on every daemon event, for re-running queries. */
export function useEvents(): number {
  const [tick, setTick] = useState(0);
  useEffect(() => {
    const es = new EventSource("/api/events");
    es.onmessage = () => setTick((t) => t + 1);
    // On a daemon restart the browser reconnects by itself; the graph may
    // have changed meanwhile, so count the reopen as an event too.
    es.onopen = () => setTick((t) => t + 1);
    return () => es.close();
  }, []);
  return tick;
}

/** Run a query, again whenever `deps` change or the daemon emits an event. */
export function useQuery<T>(fn: () => Promise<T>, deps: unknown[]): { data: T | undefined; error: ApiError | Error | undefined; loading: boolean } {
  const tick = useEvents();
  const [state, setState] = useState<{ data?: T; error?: ApiError | Error; loading: boolean }>({ loading: true });
  useEffect(() => {
    let live = true;
    setState((s) => ({ ...s, loading: true }));
    fn().then(
      (data) => live && setState({ data, loading: false }),
      (error) => live && setState({ error, loading: false }),
    );
    return () => {
      live = false;
    };
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [...deps, tick]);
  return { data: state.data, error: state.error, loading: state.loading };
}
