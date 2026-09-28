/**
 * # Module contract
 *
 * ```json
 * {"purpose":"The small shared pieces: a reference rendered as a kind tag plus link, priority, status and count badges, the ticket table, the ticket actions, the error box, the Markdown renderer for bodies and its five-line fold, and the utility class sets the views share.","criteria":[{"id":"ref-is-a-link","statement":"Every reference shown anywhere is a link to that node's page, tagged with its kind."},{"id":"markdown-bodies","statement":"A page body is rendered as Markdown wherever it is shown, since the schema allows Markdown in bodies and nowhere else."}]}
 * ```
 */

import { marked } from "marked";
import { useLayoutEffect, useRef, useState, type ReactNode } from "react";
import { api } from "./api";
import { Link } from "react-router-dom";
import { idOf, kindOf, nodePath, type Kind, type Priority, type Ref, type Status, type Summary } from "./types";

/** Class sets several views share. Colours are always theme tokens. */
export const card = "bg-bg-1 border border-line rounded-md px-3 py-2.5 mb-3";
export const list = "list-none m-0 p-0 flex flex-col gap-1";
export const item = "flex gap-2 items-baseline";
export const rel = "font-mono text-[11px] text-fg-faint min-w-[7.5em]";
export const decision = "border-l-2 border-req px-2.5 py-0.5 my-1.5";
export const decisionDate = "font-mono text-xs text-fg-dim mr-2";
export const empty = "text-fg-faint italic";
export const mono = "font-mono text-[12.5px]";
/** A pressed toggle button; the resting look is the button element default. */
export const pressed = "bg-accent-dim border-accent";

/** A page body rendered as Markdown. Bodies come from the store, which only the daemon writes. */
export function Markdown({ text }: { text: string }) {
  const html = marked.parse(text, { async: false, gfm: true, breaks: false }) as string;
  return <div className="md" dangerouslySetInnerHTML={{ __html: html }} />;
}

/** One background per kind; written out so Tailwind sees every class it must generate. */
const KIND_BG: Record<Kind, string> = {
  page: "bg-page",
  req: "bg-req",
  module: "bg-module",
  criterion: "bg-criterion",
  ticket: "bg-ticket",
  file: "bg-file",
  decl: "bg-decl",
  test: "bg-test",
};

export function KindTag({ ref }: { ref: Ref }) {
  const k = kindOf(ref);
  return (
    <span
      className={`inline-block font-mono font-semibold text-[10px] leading-none tracking-[0.06em] uppercase px-[5px] py-[3px] rounded-[3px] text-bg min-w-[3.2em] text-center ${KIND_BG[k]}`}
    >
      {k}
    </span>
  );
}

/** A reference as a link with its kind tag and, when known, its title. */
export function RefLink({ ref, title, mono: isMono = false }: { ref: Ref; title?: string; mono?: boolean }) {
  return (
    <span className="inline-flex items-center gap-1.5">
      <KindTag ref={ref} />
      <Link to={nodePath(ref)} className={isMono || !title ? mono : undefined}>
        {title ?? idOf(ref)}
      </Link>
      {title && <span className={`text-fg-faint ${mono}`}>{idOf(ref)}</span>}
    </span>
  );
}

const BADGE_TONE = {
  plain: "",
  ok: "text-ok border-ok",
  warn: "text-warn border-warn",
  bad: "text-bad border-bad",
} as const;

/** A small pill: a count, a percentage, or a mark such as "uncovered". */
export function Badge({ tone = "plain", children }: { tone?: keyof typeof BADGE_TONE; children: ReactNode }) {
  return <span className={`inline-block text-[11px] px-1.5 py-px rounded-[10px] bg-bg-3 text-fg-dim border border-line ${BADGE_TONE[tone]}`}>{children}</span>;
}

const PRIORITY_COLOR: Record<Priority, string> = { next: "text-bad", high: "text-warn", later: "text-fg-dim" };

export function PriorityTag({ p }: { p: Priority }) {
  return <span className={`font-mono font-semibold text-[11px] uppercase tracking-[0.05em] ${PRIORITY_COLOR[p]}`}>{p}</span>;
}

const STATUS_COLOR: Record<Status, string> = { open: "text-fg-dim", active: "text-accent", closed: "text-fg-faint" };

export function StatusTag({ s }: { s: Status }) {
  return <span className={STATUS_COLOR[s]}>{s}</span>;
}

const cell = "px-2.5 py-2 border-b border-line align-top group-hover:bg-bg-1";

export function TicketRow({ t }: { t: Summary }) {
  return (
    <tr className="group">
      <td className={cell}>{t.priority && <PriorityTag p={t.priority} />}</td>
      <td className={cell}>
        <Link to={nodePath(t.ref)}>{t.title}</Link>
        <div className={`text-fg-faint ${mono}`}>{idOf(t.ref)}</div>
      </td>
      <td className={`${cell} text-fg-dim`}>{t.kind}</td>
      <td className={cell}>{t.status && <StatusTag s={t.status} />}</td>
    </tr>
  );
}

export function TicketTable({ tickets, empty: emptyText = "No tickets." }: { tickets: Summary[]; empty?: string }) {
  if (tickets.length === 0) return <p className={empty}>{emptyText}</p>;
  const head = "text-left text-fg-dim font-medium text-xs px-2.5 py-1.5 border-b border-line";
  return (
    <table className="border-collapse w-full">
      <thead>
        <tr>
          <th className={head}>Priority</th>
          <th className={head}>Ticket</th>
          <th className={head}>Kind</th>
          <th className={head}>Status</th>
        </tr>
      </thead>
      <tbody>
        {tickets.map((t) => (
          <TicketRow key={t.ref} t={t} />
        ))}
      </tbody>
    </table>
  );
}

export const errorBox = "text-bad px-3 py-2 border border-bad rounded-md bg-bad/8";

export function ErrorBox({ error }: { error: Error }) {
  return <div className={errorBox}>{error.message}</div>;
}

/** A body of up to five lines shows whole; a longer one shows five and a Show more. */
export function Folded({ text }: { text: string }) {
  const body = useRef<HTMLDivElement>(null);
  const [long, setLong] = useState(false);
  const [open, setOpen] = useState(false);
  useLayoutEffect(() => {
    const el = body.current;
    if (!el) return;
    // Measured while folded, the last block's trailing margin removed: more
    // than half a line beyond the five-line box means there is text to show.
    const measure = () => {
      // The Markdown's own line height, which the five-line box is sized in.
      const line = parseFloat(getComputedStyle(el.querySelector(".md") ?? el).lineHeight) || 20;
      setLong((was) => was || el.scrollHeight - el.clientHeight > line / 2);
    };
    measure();
    const seen = new ResizeObserver(measure);
    seen.observe(el);
    return () => seen.disconnect();
  }, [text]);
  return (
    <div className="flex flex-col gap-1 items-start">
      <div
        ref={body}
        data-folded={(long && !open) || undefined}
        className={`w-full leading-[1.55] [&_.md>:last-child]:mb-0 ${open ? "" : "max-h-[5lh] overflow-hidden"} ${long && !open ? "[mask-image:linear-gradient(to_bottom,black_65%,transparent)]" : ""}`}
      >
        <Markdown text={text} />
      </div>
      {long && (
        <button type="button" aria-expanded={open} onClick={() => setOpen(!open)} className="text-[13px] text-accent bg-transparent border-none p-0 hover:underline">
          {open ? "Show less" : "Show more"}
        </button>
      )}
    </div>
  );
}

/**
 * Start an open ticket, or close an open or active one with a decision,
 * through the start_ticket and close_ticket tools.
 */
export function TicketActions({ ticket, status }: { ticket: Ref; status: Status }) {
  const [decisionText, setDecision] = useState("");
  const [failed, setFailed] = useState<string | null>(null);
  async function run(act: () => Promise<unknown>) {
    setFailed(null);
    try {
      await act();
    } catch (e) {
      setFailed((e as Error).message);
    }
  }
  async function close() {
    await api.closeTicket(ticket, decisionText);
    setDecision("");
  }
  if (status === "closed") return null;
  return (
    <div className="flex flex-col gap-2">
      <div className="flex gap-2 items-center flex-wrap">
        {status === "open" && <button onClick={() => run(() => api.startTicket(ticket))}>Start</button>}
        <textarea
          className="flex-1 min-w-60 min-h-[60px]"
          aria-label="Decision"
          placeholder="Decision: what was chosen and why. Closing discards the body."
          value={decisionText}
          onChange={(e) => setDecision(e.target.value)}
        />
        <button disabled={!decisionText.trim()} onClick={() => run(close)}>
          Close
        </button>
      </div>
      {failed && <p className={errorBox}>{failed}</p>}
    </div>
  );
}
