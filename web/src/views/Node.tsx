/**
 * # Module contract
 *
 * ```json
 * {"purpose":"A node page: the node's fields by kind, what it rests on with decisions, what rests on it, its edges and findings, and the ticket actions.","criteria":[{"id":"every-kind","statement":"Every node kind has a rendering: page, requirement, ticket, module, criterion, file and declaration."},{"id":"ticket-actions","statement":"An open ticket can be started and an open or active ticket closed with a decision from its page, through the start_ticket and close_ticket tools."}]}
 * ```
 */

import { useState } from "react";
import { useParams } from "react-router-dom";
import { api, useQuery } from "../api";
import { ErrorBox, KindTag, PriorityTag, RefLink, StatusTag, TicketTable } from "../components";
import {
  idOf,
  kindOf,
  type AnyNode,
  type CriterionNode,
  type DeclNode,
  type FileNode,
  type ModuleNode,
  type PageNode,
  type Ref,
  type ReqNode,
  type TicketNode,
} from "../types";

/** One node: its fields, what it rests on, what rests on it, and its edges. */
export function NodeView() {
  const { ref: raw } = useParams();
  const ref = decodeURIComponent(raw ?? "");
  const { data, error } = useQuery(() => api.get(ref), [ref]);
  if (error) return <ErrorBox error={error} />;
  if (!data) return <p className="dim">Loading…</p>;
  const node = data.node;
  const kind = kindOf(ref);
  return (
    <div className="node">
      <div>
        <div className="head">
          <div className="refline">
            <KindTag ref={ref} />
            <code className="dim">{idOf(ref)}</code>
          </div>
          <h1>{titleOf(node)}</h1>
        </div>
        {kind === "page" && <Page n={node as PageNode} />}
        {kind === "req" && <Req n={node as ReqNode} />}
        {kind === "ticket" && <Ticket n={node as TicketNode} />}
        {kind === "module" && <Module n={node as ModuleNode} />}
        {kind === "criterion" && <Criterion n={node as CriterionNode} />}
        {kind === "file" && <File n={node as FileNode} />}
        {kind === "decl" && <Decl n={node as DeclNode} ref={ref} />}
        <Findings ref={ref} />
      </div>
      <aside>
        <RestsOn ref={ref} />
        <RestsOnMe ref={ref} />
        <Edges title="Links out" edges={node.out} />
        <Edges title="Linked from" edges={node.in} />
      </aside>
    </div>
  );
}

function titleOf(n: AnyNode): string {
  if ("title" in n) return n.title;
  if ("purpose" in n) return n.purpose ?? n.id;
  if ("statement" in n) return n.statement;
  return n.path;
}

function Page({ n }: { n: PageNode }) {
  return (
    <>
      <div className="body-md">{n.body}</div>
      <h2>Links</h2>
      <ul className="plain">
        {n.links.map((l) => (
          <li key={l}>
            <RefLink ref={l} />
          </li>
        ))}
      </ul>
    </>
  );
}

function Req({ n }: { n: ReqNode }) {
  return (
    <>
      <p>{n.statement}</p>
      {n.scenarios.length > 0 && <h2>Scenarios</h2>}
      {n.scenarios.map((s, i) => (
        <div className="scenario" key={i}>
          <div>
            <strong>{s.title}</strong>
          </div>
          <div>
            <b>Given</b> {s.given}
          </div>
          <div>
            <b>When</b> {s.when}
          </div>
          <div>
            <b>Then</b> {s.then}
          </div>
        </div>
      ))}
      <h2>Decisions</h2>
      {n.decisions.length === 0 && <p className="empty">None recorded. `record_decision` adds one.</p>}
      {[...n.decisions].reverse().map((d, i) => (
        <div className="decision" key={i}>
          <span className="date">{d.date}</span>
          {d.text}
        </div>
      ))}
      <h2>Covered by</h2>
      {n.covered_by.length === 0 && <p className="empty">No criterion yet: the code does not satisfy this.</p>}
      <ul className="plain">
        {n.covered_by.map((c) => (
          <li key={c}>
            <RefLink ref={c} />
          </li>
        ))}
      </ul>
    </>
  );
}

function Ticket({ n }: { n: TicketNode }) {
  const ref = `ticket:${n.slug}`;
  const related = useQuery(() => api.relatedTickets(ref), [ref]);
  const [decision, setDecision] = useState("");
  const [busy, setBusy] = useState<string | null>(null);
  async function start() {
    setBusy(null);
    try {
      await api.startTicket(ref);
    } catch (e) {
      setBusy((e as Error).message);
    }
  }
  async function close() {
    setBusy(null);
    try {
      await api.closeTicket(ref, decision);
      setDecision("");
    } catch (e) {
      setBusy((e as Error).message);
    }
  }
  return (
    <>
      <p>
        <PriorityTag p={n.priority} /> · <span className="dim">{n.kind}</span> · <StatusTag s={n.status} />
      </p>
      {n.body && <div className="body-md">{n.body}</div>}
      {n.decision && (
        <div className="decision">
          <span className="date">{n.decision.date}</span>
          {n.decision.text}
        </div>
      )}
      {n.status !== "closed" && (
        <div className="card">
          <h3>Actions</h3>
          <div className="actions">
            {n.status === "open" && <button onClick={start}>Start</button>}
            <textarea placeholder="Decision: what was chosen and why. Closing discards the body." value={decision} onChange={(e) => setDecision(e.target.value)} />
            <button disabled={!decision.trim()} onClick={close}>
              Close
            </button>
          </div>
          {busy && <p className="error">{busy}</p>}
        </div>
      )}
      <h2>Touches</h2>
      <RefList refs={n.touches} empty="Nothing." />
      <h2>Relies on</h2>
      <RefList refs={n.relies_on} empty="Nothing declared." />
      <h2>Related gaps and questions</h2>
      {related.data && <TicketTable tickets={related.data.tickets} empty="None touch what this ticket touches." />}
    </>
  );
}

function RefList({ refs, empty }: { refs: Ref[]; empty: string }) {
  if (refs.length === 0) return <p className="empty">{empty}</p>;
  return (
    <ul className="plain">
      {refs.map((r) => (
        <li key={r}>
          <RefLink ref={r} />
        </li>
      ))}
    </ul>
  );
}

function Module({ n }: { n: ModuleNode }) {
  const ref = `module:${n.id}`;
  const cov = useQuery(() => api.coverage(ref), [ref]);
  return (
    <>
      {n.contract_error && <p className="error">Contract: {n.contract_error}</p>}
      {n.contract_at && (
        <p className="dim mono">
          {n.contract_at[0]}:{n.contract_at[1]}
        </p>
      )}
      <h2>Criteria</h2>
      {n.criteria.length === 0 && <p className="empty">None.</p>}
      <ul className="plain">
        {cov.data?.criteria?.map((c) => (
          <li key={c.ref} style={{ flexDirection: "column", alignItems: "stretch", gap: 2 }}>
            <RefLink ref={c.ref} title={c.statement} />
            <span className="faint" style={{ paddingLeft: 8 }}>
              {c.requirements.length === 0 ? (
                <span className="badge warn">uncovered</span>
              ) : (
                c.requirements.map((r) => (
                  <span key={r.ref} style={{ marginRight: 8 }}>
                    <RefLink ref={r.ref} title={r.title} />
                  </span>
                ))
              )}
            </span>
          </li>
        ))}
      </ul>
      <h2>Files</h2>
      <RefList refs={n.files.map((f) => `file:${f}`)} empty="None." />
      <h2>Imports</h2>
      <RefList refs={n.imports.map((m) => `module:${m}`)} empty="No resolved imports." />
    </>
  );
}

function Criterion({ n }: { n: CriterionNode }) {
  const ref = `criterion:${n.module}#${n.id}`;
  const cov = useQuery(() => api.coverage(ref), [ref]);
  const src = useQuery(() => api.moduleSource(ref), [ref]);
  return (
    <>
      <p>{n.statement}</p>
      <p className="dim">
        In <RefLink ref={`module:${n.module}`} />
        {src.data && (
          <span className="mono">
            {" "}
            · {src.data.file}:{src.data.line}
          </span>
        )}
      </p>
      <h2>Covered by</h2>
      {cov.data?.requirements?.length === 0 && <p className="empty">No requirement names this criterion.</p>}
      <ul className="plain">
        {cov.data?.requirements?.map((r) => (
          <li key={r.ref}>
            <RefLink ref={r.ref} title={r.title} />
          </li>
        ))}
      </ul>
    </>
  );
}

function File({ n }: { n: FileNode }) {
  return (
    <>
      <p className="dim">
        {n.language} · in <RefLink ref={`module:${n.module}`} />
      </p>
      <h2>Declarations</h2>
      {n.decls.length === 0 && <p className="empty">None.</p>}
      <table className="decls">
        <tbody>
          {n.decls.map((d) => (
            <tr key={d.path}>
              <td className="faint">{d.line}</td>
              <td className="dim">{d.kind}</td>
              <td>
                <RefLink ref={`decl:${n.path}#${d.path}`} mono />
              </td>
            </tr>
          ))}
        </tbody>
      </table>
    </>
  );
}

function Decl({ n, ref }: { n: DeclNode; ref: Ref }) {
  const file = idOf(ref).split("#")[0];
  return (
    <p className="dim">
      {n.kind} at <RefLink ref={`file:${file}`} /> line {n.line}
    </p>
  );
}

function RestsOn({ ref }: { ref: Ref }) {
  const { data } = useQuery(() => api.restsOn(ref, 3), [ref]);
  if (!data) return null;
  return (
    <div className="card">
      <h3>Rests on</h3>
      {data.nodes.length === 0 && <p className="empty">Nothing below this node.</p>}
      <ul className="plain">
        {data.nodes.slice(0, 40).map((r) => (
          <li key={r.ref} style={{ flexDirection: "column", alignItems: "stretch", gap: 0 }}>
            <RefLink ref={r.ref} title={r.title} />
            {r.decisions[0] && (
              <div className="decision">
                <span className="date">{r.decisions[0].date}</span>
                {r.decisions[0].text}
              </div>
            )}
          </li>
        ))}
        {data.nodes.length > 40 && <li className="faint">…and {data.nodes.length - 40} more</li>}
      </ul>
    </div>
  );
}

function RestsOnMe({ ref }: { ref: Ref }) {
  const { data } = useQuery(() => api.restsOnMe(ref), [ref]);
  if (!data) return null;
  return (
    <div className="card">
      <h3>Rests on this</h3>
      {data.nodes.length === 0 && <p className="empty">Nothing depends on this node.</p>}
      <ul className="plain">
        {data.nodes.map((a) => (
          <li key={a.ref}>
            <span className="rel">{a.relation}</span>
            <RefLink ref={a.ref} title={a.title} />
          </li>
        ))}
      </ul>
      {data.tickets.length > 0 && (
        <>
          <h3 style={{ marginTop: 10 }}>Open tickets reaching it</h3>
          <TicketTable tickets={data.tickets} />
        </>
      )}
    </div>
  );
}

function Edges({ title, edges }: { title: string; edges: { ref: Ref; relation: string }[] }) {
  if (edges.length === 0) return null;
  return (
    <div className="card">
      <h3>{title}</h3>
      <ul className="plain">
        {edges.map((e, i) => (
          <li key={i}>
            <span className="rel">{e.relation}</span>
            <RefLink ref={e.ref} />
          </li>
        ))}
      </ul>
    </div>
  );
}

function Findings({ ref }: { ref: Ref }) {
  const { data } = useQuery(() => api.lint(), []);
  const mine = data?.findings.filter((f) => f.ref === ref || f.edge?.to === ref) ?? [];
  if (mine.length === 0) return null;
  return (
    <>
      <h2>Lint findings</h2>
      <ul className="plain findings">
        {mine.map((f, i) => (
          <li key={i} className={f.rule}>
            <span className="rel">{f.rule}</span>
            {f.reason}
          </li>
        ))}
      </ul>
    </>
  );
}
