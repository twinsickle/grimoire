/**
 * # Module contract
 *
 * ```json
 * {"purpose":"The board: one view of what can be worked on, from the board tool. Changes in its order by priority, each ready or waiting on the gaps and questions it relies on, then the gaps and questions no change relies on, then on request the recently closed; any row opens into the whole ticket, and changes are reordered by dragging.","criteria":[{"id":"first-screen","statement":"Open, active and landing changes are listed in the board tool's order under their priority, each marked ready or as waiting on its open gaps and questions with a count of those answered, a landing one marked as waiting to land, counted as neither and shown whatever the filters, followed by the unattached gaps and questions; filters show or hide ready changes, changes needing refinement, unattached gaps and questions, and the most recently closed tickets, which are hidden at first and capped; among those closed, a gap or question no change relies on is marked as having nothing built on it."},{"id":"order-from-tool","statement":"Rows appear in the order the board tool returns them and the view never re-sorts; dragging a change within its band calls reorder, dropping it in another band calls update_ticket for its priority and then reorder, and Alt+Up or Alt+Down on its handle moves it one place, into the neighbouring band past an edge, announcing the move; moves run one at a time, each from the order the last one wrote, and a drop on the change's own row does nothing."},{"id":"ticket-in-place","statement":"Any row expands in place into the whole ticket read with get: what it waits on with each one's status, its body as folded Markdown, what it touches and relies on, a closed ticket's reference to a node that no longer exists marked so and not a link, the start and close actions, and a link to its own page."}]}
 * ```
 */

import { useEffect, useRef, useState, type DragEvent, type KeyboardEvent } from "react";
import { Link } from "react-router-dom";
import { api, useQuery } from "../api";
import { ErrorBox, Folded, GoneRef, Markdown, RefLink, StatusTag, TicketActions, empty, errorBox, mono } from "../components";
import { goneOf, idOf, nodePath, type Board, type BoardTicket, type Priority, type Ref, type Summary, type TicketKind, type TicketNode } from "../types";

const BANDS: Priority[] = ["next", "high", "later"];
const BAND_COLOR: Record<Priority, string> = { next: "text-bad", high: "text-warn", later: "text-fg-dim" };
const KIND_CHIP: Record<TicketKind, string> = {
  change: "text-accent border-accent-dim",
  gap: "text-warn border-warn/40",
  question: "text-req border-req/40",
};
const CLOSED_FIRST = 5;
const CLOSED_MORE = 10;

type Show = { ready: boolean; refine: boolean; unattached: boolean; closed: boolean };
type Bands = Record<Priority, Ref[]>;

/** Each band's changes, in the tool's order. */
function bandsOf(board: Board | null): Bands {
  const of = (p: Priority) => (board?.tickets ?? []).filter((t) => t.priority === p).map((t) => t.ref);
  return { next: of("next"), high: of("high"), later: of("later") };
}

/** How long an order this view wrote stands in for the tool's, until the refetch after it arrives. */
const WRITTEN_FRESH_MS = 1500;

/** The time a move was written or read; called from moves, never while rendering. */
function stamp(): number {
  return Date.now();
}

/** The gaps and questions a change still waits on. */
function openWaits(t: BoardTicket): Summary[] {
  return (t.waits_on ?? []).filter((w) => w.status !== "closed");
}

/** "2 questions · 1 gap · 1 answered": what a change relies on, at a glance. */
function waitCount(t: BoardTicket): string {
  const open = openWaits(t);
  const questions = open.filter((w) => w.kind === "question").length;
  const gaps = open.length - questions;
  const answered = (t.waits_on?.length ?? 0) - open.length;
  const parts: string[] = [];
  if (questions) parts.push(`${questions} question${questions === 1 ? "" : "s"}`);
  if (gaps) parts.push(`${gaps} gap${gaps === 1 ? "" : "s"}`);
  if (answered) parts.push(`${answered} answered`);
  return parts.join(" · ");
}

/** What can be worked on: changes by priority with their readiness, the unattached gaps and questions, and the recently closed. */
export function BoardView() {
  const [show, setShow] = useState<Show>({ ready: true, refine: true, unattached: true, closed: false });
  const [closedCount, setClosedCount] = useState(CLOSED_FIRST);
  const [open, setOpen] = useState<Set<Ref>>(new Set());
  const [dragging, setDragging] = useState<Ref | null>(null);
  const [over, setOver] = useState<{ band: Priority; before: Ref | null } | null>(null);
  const [said, setSaid] = useState("");
  const [failed, setFailed] = useState<string | null>(null);
  // Drag events arrive faster than renders: the handlers read these, the rows draw the state.
  const draggingRef = useRef<Ref | null>(null);
  const overRef = useRef<{ band: Priority; before: Ref | null } | null>(null);
  // Moves run one at a time, each from the order the one before it wrote, so a
  // second quick move never works from an answer that predates the first.
  const queueRef = useRef<Promise<void>>(Promise.resolve());
  const writtenRef = useRef<{ bands: Bands; at: number } | null>(null);
  const { data, error } = useQuery(() => api.board({ closed: show.closed ? closedCount : 0 }), [show.closed, closedCount]);
  const dataRef = useRef<Board | null>(null);
  useEffect(() => {
    dataRef.current = data ?? null;
  }, [data]);
  if (error) return <ErrorBox error={error} />;
  if (!data) return <p className="text-fg-dim">Loading…</p>;

  const changes = data.tickets;
  const band = (p: Priority) => changes.filter((t) => t.priority === p);
  // A landing change is done being worked: neither ready to start nor in need of refinement.
  const landing = changes.filter((t) => t.status === "landing").length;
  const ready = changes.filter((t) => t.status !== "landing" && openWaits(t).length === 0).length;
  const refine = changes.length - ready - landing;
  const toggle = (ref: Ref) => setOpen((s) => new Set(s.has(ref) ? [...s].filter((r) => r !== ref) : [...s, ref]));

  const titleOf = (ref: Ref) => dataRef.current?.tickets.find((c) => c.ref === ref)?.title ?? idOf(ref);

  /** Run a move after the ones before it, on the latest order: the one just written, or the tool's. */
  function enqueue(job: (bands: Bands) => Promise<void>) {
    queueRef.current = queueRef.current.then(async () => {
      const written = writtenRef.current;
      const bands = written && stamp() - written.at < WRITTEN_FRESH_MS ? written.bands : bandsOf(dataRef.current);
      setFailed(null);
      try {
        await job(bands);
      } catch (e) {
        setFailed((e as Error).message);
      }
    });
  }

  /** Put `ref` in `target` before `before` (or last): the priority first when it changes, then the band's order. */
  async function place(bands: Bands, ref: Ref, target: Priority, before: Ref | null) {
    const from = BANDS.find((p) => bands[p].includes(ref));
    if (!from) return;
    const next: Bands = { next: bands.next.filter((r) => r !== ref), high: bands.high.filter((r) => r !== ref), later: bands.later.filter((r) => r !== ref) };
    const order = next[target];
    const at = before ? order.indexOf(before) : -1;
    order.splice(at < 0 ? order.length : at, 0, ref);
    if (from !== target) await api.updateTicket(ref, { priority: target });
    await api.reorder(target, order);
    writtenRef.current = { bands: next, at: stamp() };
    setSaid(`${titleOf(ref)} moved to position ${order.indexOf(ref) + 1} of ${order.length} in ${target}.`);
  }

  /** One place up or down; past the edge of a band, into the end of the band above or the start of the band below. */
  async function step(bands: Bands, ref: Ref, up: boolean) {
    const p = BANDS.find((b) => bands[b].includes(ref));
    if (!p) return;
    const list = bands[p];
    const i = list.indexOf(ref);
    const neighbour = BANDS[BANDS.indexOf(p) + (up ? -1 : 1)];
    if (up && i > 0) return place(bands, ref, p, list[i - 1]);
    if (!up && i < list.length - 1) return place(bands, ref, p, list[i + 2] ?? null);
    if (!neighbour) {
      setSaid(`${titleOf(ref)} is already ${up ? "first" : "last"}.`);
      return;
    }
    return place(bands, ref, neighbour, up ? null : (bands[neighbour][0] ?? null));
  }

  function onKey(e: KeyboardEvent, t: BoardTicket) {
    if (!e.altKey || (e.key !== "ArrowUp" && e.key !== "ArrowDown")) return;
    e.preventDefault();
    const up = e.key === "ArrowUp";
    enqueue((bands) => step(bands, t.ref, up));
  }

  const drag = {
    start: (e: DragEvent, ref: Ref) => {
      e.dataTransfer.effectAllowed = "move";
      e.dataTransfer.setData("text/plain", ref);
      draggingRef.current = ref;
      // Drawing the drop targets changes the layout (an empty band appears),
      // and a browser abandons a drag whose layout changes inside dragstart;
      // so they are drawn just after it.
      setTimeout(() => {
        if (draggingRef.current === ref) setDragging(ref);
      }, 0);
    },
    over: (e: DragEvent, target: Priority, rowRef: Ref | null) => {
      const dragged = draggingRef.current;
      if (!dragged) return;
      e.preventDefault();
      // Over itself: dropping there changes nothing.
      if (rowRef === dragged) {
        if (overRef.current) {
          overRef.current = null;
          setOver(null);
        }
        return;
      }
      let before = rowRef;
      if (rowRef) {
        // The lower half of a row drops after it.
        const box = (e.currentTarget as HTMLElement).getBoundingClientRect();
        if (e.clientY > box.top + box.height / 2) {
          const refs = band(target)
            .map((c) => c.ref)
            .filter((r) => r !== dragged);
          before = refs[refs.indexOf(rowRef) + 1] ?? null;
        }
      }
      const was = overRef.current;
      if (was?.band !== target || was.before !== before) {
        overRef.current = { band: target, before };
        setOver(overRef.current);
      }
    },
    drop: (e: DragEvent) => {
      e.preventDefault();
      const dragged = draggingRef.current;
      const at = overRef.current;
      if (dragged && at) enqueue((bands) => place(bands, dragged, at.band, at.before));
      drag.end();
    },
    end: () => {
      draggingRef.current = null;
      overRef.current = null;
      setDragging(null);
      setOver(null);
    },
  };

  const chip = (key: keyof Show, label: string, n: number | null, dot: string) => (
    <button
      key={key}
      aria-pressed={show[key]}
      onClick={() => setShow((s) => ({ ...s, [key]: !s[key] }))}
      className={`inline-flex items-center gap-2 rounded-2xl px-3 py-1 text-[13px] ${show[key] ? "border-accent bg-bg-2" : "bg-transparent text-fg-dim"}`}
    >
      <span className={`inline-block w-2 h-2 rounded-full ${dot}`} />
      {label}
      {n != null && <span className="text-fg-dim">{n}</span>}
    </button>
  );

  return (
    <div className="flex flex-col gap-4 max-w-[140ch] mx-auto">
      <div className="flex items-center gap-2.5 flex-wrap">
        <h1 className="m-0 mr-3">Board</h1>
        {chip("ready", "Ready", ready, "bg-ok")}
        {chip("refine", "Needs refinement", refine, "bg-warn")}
        {chip("unattached", "Not yet attached", data.unattached.length, "bg-req")}
        {chip("closed", "Closed", null, "bg-fg-dim")}
      </div>
      <p aria-live="polite" className="sr-only">
        {said}
      </p>
      {failed && <p className={errorBox}>{failed}</p>}
      {BANDS.map((p) => {
        const rows = band(p).filter((t) => t.status === "landing" || (openWaits(t).length ? show.refine : show.ready));
        // An empty band shows only while dragging, as somewhere to drop.
        if (rows.length === 0 && !dragging) return null;
        return (
          <section key={p} id={`band-${p}`} aria-label={`${p} priority`} className="flex flex-col gap-1">
            <h2 className={`m-0 text-[11.5px] uppercase tracking-[0.06em] font-semibold ${BAND_COLOR[p]}`}>{p}</h2>
            <ul className="list-none m-0 p-0 flex flex-col gap-1" onDragOver={(e) => drag.over(e, p, null)} onDrop={drag.drop}>
              {rows.map((t) => (
                <Row
                  key={t.ref}
                  t={t}
                  open={open.has(t.ref)}
                  onToggle={() => toggle(t.ref)}
                  change={{
                    dragging: dragging === t.ref,
                    dropBefore: over?.band === p && over.before === t.ref && dragging !== t.ref,
                    onDragStart: (e) => drag.start(e, t.ref),
                    onDragOver: (e) => {
                      e.stopPropagation();
                      drag.over(e, p, t.ref);
                    },
                    onDrop: drag.drop,
                    onDragEnd: drag.end,
                    onKey: (e) => onKey(e, t),
                  }}
                />
              ))}
              {dragging && over?.band === p && over.before === null && <li aria-hidden className="h-0.5 bg-accent rounded" />}
              {rows.length === 0 && <li className="border border-dashed border-line rounded-md px-3 py-2 text-fg-dim text-[13px]">Drop here to make it {p}</li>}
            </ul>
          </section>
        );
      })}
      {show.unattached && (
        <section id="unattached" aria-label="Not yet attached" className="flex flex-col gap-1">
          <h2 className="m-0 text-[11.5px] uppercase tracking-[0.06em] font-semibold text-req">
            Not yet attached <span className="normal-case tracking-normal font-normal text-fg-dim">· gaps and questions no change relies on</span>
          </h2>
          {data.unattached.length === 0 && <p className={empty}>Every open gap and question is attached to a change.</p>}
          <ul className="list-none m-0 p-0 flex flex-col gap-1">
            {data.unattached.map((t) => (
              <Row key={t.ref} t={t} open={open.has(t.ref)} onToggle={() => toggle(t.ref)} />
            ))}
          </ul>
        </section>
      )}
      {show.closed && (
        <section id="closed" aria-label="Recently closed" className="flex flex-col gap-1">
          <h2 className="m-0 text-[11.5px] uppercase tracking-[0.06em] font-semibold text-fg-dim">
            Recently closed <span className="normal-case tracking-normal font-normal">· newest first</span>
          </h2>
          <ul className="list-none m-0 p-0 flex flex-col gap-1">
            {data.closed.map((t) => (
              <Row key={t.ref} t={t} open={open.has(t.ref)} onToggle={() => toggle(t.ref)} />
            ))}
          </ul>
          {data.closed.length === closedCount && (
            <button className="self-start" onClick={() => setClosedCount((n) => n + CLOSED_MORE)}>
              Show {CLOSED_MORE} more
            </button>
          )}
        </section>
      )}
    </div>
  );
}

type ChangeRow = {
  dragging: boolean;
  dropBefore: boolean;
  onDragStart: (e: DragEvent) => void;
  onDragOver: (e: DragEvent) => void;
  onDrop: (e: DragEvent) => void;
  onDragEnd: () => void;
  onKey: (e: KeyboardEvent) => void;
};

/** One ticket: a line to scan, opening into the whole ticket. A change also carries its readiness and a drag handle. */
function Row({ t, open, onToggle, change }: { t: BoardTicket; open: boolean; onToggle: () => void; change?: ChangeRow }) {
  const waiting = openWaits(t).length;
  const isChange = t.kind === "change";
  return (
    <li
      data-ticket={t.ref}
      onDragOver={change?.onDragOver}
      onDrop={change?.onDrop}
      className={`bg-bg-1 border rounded-md ${open ? "border-accent-dim" : waiting ? "border-warn/40" : "border-line"} ${change?.dragging ? "opacity-45" : ""} ${
        change?.dropBefore ? "shadow-[0_-2px_0_var(--color-accent)]" : ""
      }`}
    >
      <div className="grid grid-cols-[20px_28px_76px_minmax(120px,22ch)_minmax(0,1fr)_auto_56px] gap-x-2.5 items-center px-2.5 py-1.5">
        {change ? (
          <button
            draggable
            onDragStart={change.onDragStart}
            onDragEnd={change.onDragEnd}
            onKeyDown={change.onKey}
            aria-label={`Move ${t.title}: drag, or Alt+Up and Alt+Down, which cross into the next priority at an edge`}
            className="p-0 w-5 h-7 bg-transparent border-none text-fg-dim cursor-grab flex items-center justify-center hover:bg-transparent"
          >
            <svg width="10" height="16" viewBox="0 0 10 16" fill="currentColor" aria-hidden="true">
              {[3, 8, 13].flatMap((y) => [<circle key={`a${y}`} cx="2.5" cy={y} r="1.3" />, <circle key={`b${y}`} cx="7.5" cy={y} r="1.3" />])}
            </svg>
          </button>
        ) : (
          <span />
        )}
        <button
          aria-label={open ? `Hide ${t.title}` : `Show ${t.title}`}
          aria-expanded={open}
          onClick={onToggle}
          className="p-0 w-7 h-7 bg-transparent border-none text-fg flex items-center justify-center hover:bg-bg-2"
        >
          <svg width="14" height="14" viewBox="0 0 14 14" fill="none" stroke="currentColor" strokeWidth="1.6" aria-hidden="true" className={open ? "rotate-90" : ""}>
            <path d="M5 3l4 4-4 4" />
          </svg>
        </button>
        <span className={`justify-self-start font-mono text-[11px] border rounded px-1.5 ${KIND_CHIP[t.kind ?? "change"]}`}>{t.kind}</span>
        <Link to={nodePath(t.ref)} className={`${mono} text-fg-dim overflow-hidden text-ellipsis whitespace-nowrap`}>
          {idOf(t.ref)}
        </Link>
        <Link to={nodePath(t.ref)} className="text-fg overflow-hidden text-ellipsis whitespace-nowrap">
          {t.title}
        </Link>
        <span className="flex items-center gap-2 justify-end text-xs whitespace-nowrap">
          {isChange && t.status === "landing" && <span className="border rounded-[10px] px-2 text-warn border-warn/40">waiting to land</span>}
          {isChange && t.status !== "closed" && t.status !== "landing" && (
            <span className={`border rounded-[10px] px-2 ${waiting ? "text-warn border-warn/40" : "text-ok border-ok/40"}`}>{waiting ? `waits on ${waiting}` : "ready"}</span>
          )}
          {t.followed_by?.length === 0 && (
            <span title="No change relies on this answer: it needed no work, or its work was never filed." className="border rounded-[10px] px-2 text-warn border-warn/40">
              nothing built on it
              <span className="sr-only">: no change relies on this answer; it needed no work, or its work was never filed.</span>
            </span>
          )}
          {isChange ? <span className="text-fg-dim">{waitCount(t)}</span> : <span className={BAND_COLOR[t.priority ?? "later"]}>{t.priority}</span>}
        </span>
        <span className="justify-self-end">{t.status && <StatusTag s={t.status} />}</span>
      </div>
      {open && <Ticket t={t} />}
    </li>
  );
}

/** The whole ticket, read when its row opens. */
function Ticket({ t }: { t: BoardTicket }) {
  const { data, error } = useQuery(() => api.get(t.ref), [t.ref]);
  if (error) return <ErrorBox error={error} />;
  if (!data) return <p className="text-fg-dim px-12 pb-3">Loading…</p>;
  const n = data.node as TicketNode;
  const titles: Record<string, string> = {};
  for (const e of n.out) if (e.title) titles[e.ref] = e.title;
  const gone = goneOf(n);
  const waits = t.waits_on ?? [];
  const relies = n.relies_on.filter((r) => !waits.some((w) => w.ref === r));
  const label = "m-0 mb-1 text-[11.5px] uppercase tracking-[0.05em] text-fg-dim font-semibold";
  return (
    <div className="border-t border-line grid grid-cols-[minmax(0,1fr)_320px] gap-7 px-4 pt-3.5 pb-4 pl-12">
      <div className="flex flex-col gap-3 min-w-0">
        {waits.length > 0 && (
          <div>
            <h3 className={label}>Waits on</h3>
            <ul className="list-none m-0 p-0 flex flex-col gap-1">
              {waits.map((w) => (
                <li key={w.ref} className="flex items-center gap-2.5 text-[13.5px]">
                  <span className={`font-mono text-[11px] border rounded px-1.5 ${KIND_CHIP[w.kind ?? "gap"]}`}>{w.kind}</span>
                  <Link to={nodePath(w.ref)} className="min-w-0 overflow-hidden text-ellipsis whitespace-nowrap">
                    {w.title}
                  </Link>
                  <span className={`${mono} text-fg-dim text-xs`}>{idOf(w.ref)}</span>
                  <span className={`ml-auto text-xs ${w.status === "closed" ? "text-ok" : "text-warn"}`}>{w.status === "closed" ? "answered" : "open"}</span>
                </li>
              ))}
            </ul>
          </div>
        )}
        {n.decision ? (
          <div>
            <h3 className={label}>
              Decision · {n.decision.date}
              {n.status === "landing" && " · written, waiting for its change to land"}
            </h3>
            <Markdown text={n.decision.text} />
          </div>
        ) : n.body ? (
          <Folded key={t.ref} text={n.body} />
        ) : (
          <p className={empty}>No body.</p>
        )}
      </div>
      <aside className="flex flex-col gap-3 min-w-0">
        {n.status !== "closed" && <TicketActions ticket={t.ref} status={n.status} />}
        <div>
          <h3 className={label}>Touches</h3>
          {n.touches.length === 0 && <p className={empty}>Nothing.</p>}
          <ul className="list-none m-0 p-0 flex flex-col gap-0.5 text-[13px]">
            {n.touches.map((r) => (
              <li key={r} className="min-w-0 overflow-hidden text-ellipsis whitespace-nowrap">
                {gone.has(r) ? <GoneRef ref={r} /> : <RefLink ref={r} title={titles[r]} />}
              </li>
            ))}
          </ul>
        </div>
        {relies.length > 0 && (
          <div>
            <h3 className={label}>Relies on</h3>
            <ul className="list-none m-0 p-0 flex flex-col gap-0.5 text-[13px]">
              {relies.map((r) => (
                <li key={r} className="min-w-0 overflow-hidden text-ellipsis whitespace-nowrap">
                  {gone.has(r) ? <GoneRef ref={r} /> : <RefLink ref={r} title={titles[r]} />}
                </li>
              ))}
            </ul>
          </div>
        )}
        <Link to={nodePath(t.ref)} className="text-[13px]">
          Open the ticket's page
        </Link>
      </aside>
    </div>
  );
}
