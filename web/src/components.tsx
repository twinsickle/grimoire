/**
 * # Module contract
 *
 * ```json
 * {"purpose":"The small shared pieces: a reference rendered as a kind tag plus link, priority, status and count badges, the ticket table, a ticket's line to hand to an agent, the error box, the Markdown renderer for bodies and its five-line fold, and the utility class sets the views share.","criteria":[{"id":"ref-is-a-link","statement":"Every reference shown anywhere is tagged with its kind and is a link to that node's page, except a closed ticket's reference to a node the graph no longer holds, which is shown as its id in plain text marked no longer exists, since a link would lead nowhere."},{"id":"withdrawn-tag","statement":"StatusTag shows a ticket given as withdrawn as withdrawn, dimmed, in place of closed, so a withdrawn change reads apart from one that landed wherever a ticket's status is shown: the ticket table, the cauldron's rows and the ticket's page, which also labels its decision as withdrawn, nothing of it applied."},{"id":"markdown-bodies","statement":"A page body is rendered as Markdown wherever it is shown, since the schema allows Markdown in bodies and nowhere else."},{"id":"markdown-inert","statement":"Rendered Markdown is sanitised before it reaches the page: HTML only, no SVG or MathML; no event, style or srcset attributes; no script, style, iframe, object, embed or form elements; and every URL-bearing attribute holds an http, https, mailto or relative URL, data URLs included in what is refused."},{"id":"handoff-copy","statement":"Handoff shows a ticket's line from handoff, selectable, with a Copy button that puts the line on the clipboard and then reads Copied and says so aloud, until the line changes; where the clipboard is missing or refuses, it says aloud to select the line and copy it by hand; a gap or question waiting for its change gets no line and says it closes when that change lands, a closed ticket nothing; it calls no tool."}]}
 * ```
 */

import DOMPurify from "dompurify";
import { marked } from "marked";
import { useLayoutEffect, useRef, useState, type ReactNode } from "react";
import { handoff } from "./handoff";
import { Link } from "react-router-dom";
import { idOf, kindOf, nodePath, type Kind, type Priority, type Ref, type Status, type Summary, type TicketKind } from "./types";

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

/** Where a link or image in a body may point: the web, mail, or somewhere relative. */
const SAFE_URL = /^(?:(?:https?|mailto):|[^a-z]|[a-z+.-]+(?:[^a-z+.\-:]|$))/i;

/** Attributes that hold a URL. DOMPurify lets data URLs through on images; this holds every one to SAFE_URL. */
const URL_ATTRIBUTES = new Set(["href", "src", "action", "formaction", "poster", "background", "cite", "longdesc", "xlink:href"]);
DOMPurify.addHook("uponSanitizeAttribute", (_node, data) => {
  if (URL_ATTRIBUTES.has(data.attrName) && !SAFE_URL.test(data.attrValue.trim())) data.keepAttr = false;
});

/**
 * A body rendered as Markdown, made inert. A body is text an agent or a person
 * wrote through the tools, and this origin can call every tool, so nothing in
 * it may run.
 */
export function Markdown({ text }: { text: string }) {
  const raw = marked.parse(text, { async: false, gfm: true, breaks: false }) as string;
  const html = DOMPurify.sanitize(raw, {
    USE_PROFILES: { html: true },
    FORBID_TAGS: ["style", "form", "iframe", "object", "embed"],
    FORBID_ATTR: ["style", "srcset"],
    ALLOWED_URI_REGEXP: SAFE_URL,
  });
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

/** A reference whose node no longer exists: its kind and id as text, marked, since a link would lead nowhere. */
export function GoneRef({ ref }: { ref: Ref }) {
  return (
    <span className="inline-flex items-center gap-1.5" data-gone={ref}>
      <KindTag ref={ref} />
      <span className={`${mono} text-fg-dim`}>{idOf(ref)}</span>
      <Badge tone="warn">no longer exists</Badge>
    </span>
  );
}

const BADGE_TONE = {
  plain: "",
  ok: "text-ok border-ok",
  warn: "text-warn border-warn",
  bad: "text-bad border-bad",
} as const;

/** A file and line, linking to the file's annotated source scrolled to that line. */
export function SourceLink({ file, line }: { file: string; line: number }) {
  return (
    <Link to={`${nodePath(`file:${file}`)}#L${line}`} className={mono}>
      {file}:{line}
    </Link>
  );
}

/** A small pill: a count, a percentage, or a mark such as "uncovered". */
export function Badge({ tone = "plain", children }: { tone?: keyof typeof BADGE_TONE; children: ReactNode }) {
  return <span className={`inline-block text-[11px] px-1.5 py-px rounded-[10px] bg-bg-3 text-fg-dim border border-line ${BADGE_TONE[tone]}`}>{children}</span>;
}

const PRIORITY_COLOR: Record<Priority, string> = { next: "text-bad", high: "text-warn", later: "text-fg-dim" };

export function PriorityTag({ p }: { p: Priority }) {
  return <span className={`font-mono font-semibold text-[11px] uppercase tracking-[0.05em] ${PRIORITY_COLOR[p]}`}>{p}</span>;
}

const STATUS_COLOR: Record<Status, string> = { open: "text-fg-dim", active: "text-accent", landing: "text-warn", closed: "text-fg-faint" };

/** A ticket's status; a closed change that was withdrawn reads as withdrawn, nothing of it applied (req:tool-withdraw-ticket). */
export function StatusTag({ s, withdrawn }: { s: Status; withdrawn?: boolean }) {
  if (s === "closed" && withdrawn) return <span className="text-fg-faint italic">withdrawn</span>;
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
      <td className={cell}>{t.status && <StatusTag s={t.status} withdrawn={t.withdrawn} />}</td>
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
 * A ticket's next step, handed to the person's agent: the line to give it, and a Copy button. The UI starts and closes
 * nothing; an answer waiting for its change says when it closes, and a closed ticket shows nothing.
 */
export function Handoff({ ticket }: { ticket: { ref: Ref; title: string; kind: TicketKind; status: Status } }) {
  // What the last press did, to the line it copied: a line that has since changed is not the one copied.
  const [copied, setCopied] = useState<{ line: string; ok: boolean } | null>(null);
  const line = handoff(ticket);
  if (ticket.status === "closed") return null;
  if (!line) return <p className="m-0 text-fg-dim text-[13px]">Answered: closes when the change applying it lands.</p>;
  const said = copied?.line === line ? copied : null;
  async function copy(text: string) {
    try {
      await navigator.clipboard.writeText(text);
      setCopied({ line: text, ok: true });
    } catch {
      setCopied({ line: text, ok: false });
    }
  }
  return (
    <div className="flex flex-col gap-1.5">
      <p className="m-0 text-fg-dim text-[13px]">Hand this to your agent:</p>
      <div className="flex gap-2 items-start">
        <code data-handoff className="flex-1 min-w-0 whitespace-pre-wrap break-words bg-bg-2 border border-line rounded px-2.5 py-1.5 text-[13px] text-fg">
          {line}
        </code>
        <button onClick={() => copy(line)} aria-label={`Copy: ${line}`}>
          {said?.ok ? "Copied" : "Copy"}
        </button>
      </div>
      {/* Said aloud as well as shown: the button's own name stays what it does. */}
      <p role="status" className="sr-only">
        {said?.ok ? "Copied to the clipboard." : ""}
      </p>
      {said && !said.ok && (
        <p role="alert" className={errorBox}>
          The browser would not copy it: select the line and copy it by hand.
        </p>
      )}
    </div>
  );
}
