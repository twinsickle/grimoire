import { Link } from "react-router-dom";
import { idOf, kindOf, type Priority, type Ref, type Status, type Summary } from "./types";

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
