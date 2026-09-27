/**
 * # Module contract
 *
 * ```json
 * {"purpose":"The small shared pieces: a reference rendered as a kind tag plus link, priority and status tags, the ticket table, the error box, and the Markdown renderer for page bodies.","criteria":[{"id":"ref-is-a-link","statement":"Every reference shown anywhere is a link to that node's page, tagged with its kind."},{"id":"markdown-bodies","statement":"A page body is rendered as Markdown wherever it is shown, since the schema allows Markdown in bodies and nowhere else."}]}
 * ```
 */

import { marked } from "marked";
import { Link } from "react-router-dom";
import { idOf, kindOf, type Priority, type Ref, type Status, type Summary } from "./types";

/** A page body rendered as Markdown. Bodies come from the store, which only the daemon writes. */
export function Markdown({ text }: { text: string }) {
  const html = marked.parse(text, { async: false, gfm: true, breaks: false }) as string;
  return <div className="md" dangerouslySetInnerHTML={{ __html: html }} />;
}

export function nodePath(ref: Ref): string {
  return `/n/${encodeURIComponent(ref)}`;
}

export function KindTag({ ref }: { ref: Ref }) {
  const k = kindOf(ref);
  return <span className={`kind kind-${k}`}>{k}</span>;
}

/** A reference as a link with its kind tag and, when known, its title. */
export function RefLink({ ref, title, mono = false }: { ref: Ref; title?: string; mono?: boolean }) {
  return (
    <span className="ref">
      <KindTag ref={ref} />
      <Link to={nodePath(ref)} className={mono || !title ? "mono" : undefined}>
        {title ?? idOf(ref)}
      </Link>
      {title && <span className="faint mono">{idOf(ref)}</span>}
    </span>
  );
}

export function PriorityTag({ p }: { p: Priority }) {
  return <span className={`prio prio-${p}`}>{p}</span>;
}

export function StatusTag({ s }: { s: Status }) {
  return <span className={`status-${s}`}>{s}</span>;
}

export function TicketRow({ t }: { t: Summary }) {
  return (
    <tr>
      <td>{t.priority && <PriorityTag p={t.priority} />}</td>
      <td>
        <Link to={nodePath(t.ref)}>{t.title}</Link>
        <div className="faint mono">{idOf(t.ref)}</div>
      </td>
      <td className="dim">{t.kind}</td>
      <td>{t.status && <StatusTag s={t.status} />}</td>
    </tr>
  );
}

export function TicketTable({ tickets, empty = "No tickets." }: { tickets: Summary[]; empty?: string }) {
  if (tickets.length === 0) return <p className="empty">{empty}</p>;
  return (
    <table className="tickets">
      <thead>
        <tr>
          <th>Priority</th>
          <th>Ticket</th>
          <th>Kind</th>
          <th>Status</th>
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

export function ErrorBox({ error }: { error: Error }) {
  return <div className="error">{error.message}</div>;
}
