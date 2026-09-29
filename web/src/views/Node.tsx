/**
 * # Module contract
 *
 * ```json
 * {"purpose":"A node page for every kind but a page: the node's fields by kind, what it rests on with decisions, what rests on it, its edges and findings, and the ticket actions.","criteria":[{"id":"every-kind","statement":"Every node kind but a page has a rendering: requirement, ticket, module, criterion, file (as its annotated source), declaration and test; a page is rendered by the grimoire view, which is its node page."},{"id":"ticket-actions","statement":"An open ticket can be started and an open or active ticket closed with a decision from its page, through the start_ticket and close_ticket tools."},{"id":"annotated-source","statement":"A file's page shows its text from the source tool, line by line with numbers, each annotated line marked with links to its node and what that node links (a contract's criteria, a test block's checks); opened at #L<n> it scrolls to that line and marks it; the pages of a module, criterion, declaration and test link to their line there."}]}
 * ```
 */

import { Fragment, useEffect } from "react";
import { Link, useLocation, useParams } from "react-router-dom";
import { api, useQuery } from "../api";
import {
  Badge,
  ErrorBox,
  KindTag,
  Markdown,
  PriorityTag,
  RefLink,
  SourceLink,
  StatusTag,
  TicketActions,
  TicketTable,
  card,
  decision as decisionBox,
  decisionDate,
  empty,
  errorBox,
  item,
  list,
  rel,
} from "../components";
import {
  idOf,
  nodePath,
  kindOf,
  type AnyNode,
  type Annotation,
  type CoverageAnswer,
  type CriterionCoverage,
  type CriterionNode,
  type DeclNode,
  type TestNode,
  type FileNode,
  type ModuleNode,
  type Ref,
  type ReqNode,
  type Summary,
  type TicketNode,
} from "../types";

/** One node: its fields, what it rests on, what rests on it, and its edges. */
export function NodeView() {
  const { ref: raw } = useParams();
  const ref = decodeURIComponent(raw ?? "");
  const { data, error } = useQuery(() => api.get(ref), [ref]);
  if (error) return <ErrorBox error={error} />;
  if (!data) return <p className="text-fg-dim">Loading…</p>;
  const node = data.node;
  const kind = kindOf(ref);
  return (
    <div className={`grid grid-cols-1 gap-6 items-start max-w-[150ch] mx-auto ${kind === "file" ? "" : "lg:grid-cols-[minmax(0,1fr)_360px]"}`}>
      <div>
        <div className="flex flex-col gap-1.5 mb-3">
          <div className="flex gap-2.5 items-center">
            <KindTag ref={ref} />
            <code className="text-fg-dim">{idOf(ref)}</code>
          </div>
          <h1>{titleOf(node)}</h1>
        </div>
        {kind === "req" && <Req n={node as ReqNode} />}
        {kind === "ticket" && <Ticket n={node as TicketNode} />}
        {kind === "module" && <Module n={node as ModuleNode} />}
        {kind === "criterion" && <Criterion n={node as CriterionNode} />}
        {kind === "file" && <File n={node as FileNode} />}
        {kind === "decl" && <Decl n={node as DeclNode} ref={ref} />}
        {kind === "test" && <TestView n={node as TestNode} />}
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
  if ("name" in n) return n.name;
  return n.path;
}

function Decision({ date, text }: { date: string; text: string }) {
  return (
    <div className={decisionBox}>
      <span className={decisionDate}>{date}</span>
      {text}
    </div>
  );
}

function Req({ n }: { n: ReqNode }) {
  const label = "text-fg-dim font-semibold text-xs uppercase tracking-[0.04em] mr-1.5";
  return (
    <>
      <p>{n.statement}</p>
      {n.scenarios.length > 0 && <h2>Scenarios</h2>}
      {n.scenarios.map((s, i) => (
        <div className="bg-bg border border-line rounded-md px-2.5 py-2 my-1.5" key={i}>
          <div>
            <strong>{s.title}</strong>
          </div>
          <div>
            <b className={label}>Given</b> {s.given}
          </div>
          <div>
            <b className={label}>When</b> {s.when}
          </div>
          <div>
            <b className={label}>Then</b> {s.then}
          </div>
        </div>
      ))}
      <h2>Decisions</h2>
      {n.decisions.length === 0 && <p className={empty}>None recorded. `record_decision` adds one.</p>}
      {[...n.decisions].reverse().map((d, i) => <Decision key={i} date={d.date} text={d.text} />)}
      <h2>Covered by</h2>
      {n.covered_by.length === 0 && <p className={empty}>No criterion yet: the code does not satisfy this.</p>}
      <ul className={list}>
        {n.covered_by.map((c) => (
          <li key={c} className={item}>
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
  return (
    <>
      <p>
        <PriorityTag p={n.priority} /> · <span className="text-fg-dim">{n.kind}</span> · <StatusTag s={n.status} />
      </p>
      {n.body && <Markdown text={n.body} />}
      {n.decision && <Decision date={n.decision.date} text={n.decision.text} />}
      {n.status !== "closed" && (
        <div className={card}>
          <h3 className="mb-2">Actions</h3>
          <TicketActions ticket={ref} status={n.status} />
        </div>
      )}
      <h2>Touches</h2>
      <RefList refs={n.touches} empty="Nothing." titles={titlesOf(n.out)} />
      <h2>Relies on</h2>
      <RefList refs={n.relies_on} empty="Nothing declared." titles={titlesOf(n.out)} />
      <h2>Related gaps and questions</h2>
      {related.data && <TicketTable tickets={related.data.tickets} empty="None touch what this ticket touches." />}
    </>
  );
}

/** Titles for the other ends of a node's edges, so lists of references read as names. */
function titlesOf(edges: { ref: Ref; title: string | null }[]): Record<string, string> {
  const m: Record<string, string> = {};
  for (const e of edges) if (e.title) m[e.ref] = e.title;
  return m;
}

function RefList({ refs, empty: emptyText, titles = {} }: { refs: Ref[]; empty: string; titles?: Record<string, string> }) {
  if (refs.length === 0) return <p className={empty}>{emptyText}</p>;
  return (
    <ul className={list}>
      {refs.map((r) => (
        <li key={r} className={item}>
          <RefLink ref={r} title={titles[r]} />
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
      {n.contract_error && <p className={errorBox}>Contract: {n.contract_error}</p>}
      {n.contract_at && (
        <p className="text-fg-dim">
          Contract at <SourceLink file={n.contract_at[0]} line={n.contract_at[1]} />
        </p>
      )}
      <h2>Criteria</h2>
      {n.criteria.length === 0 && <p className={empty}>None.</p>}
      <ul className={list}>
        {moduleCriteria(cov.data).map((c) => (
          <li key={c.ref} className="flex flex-col items-stretch gap-0.5">
            <RefLink ref={c.ref} title={c.statement} />
            <span className="text-fg-faint pl-2">
              {c.requirements.length === 0 ? (
                <Badge tone="warn">uncovered</Badge>
              ) : (
                c.requirements.map((r) => (
                  <span key={r.ref} className="mr-2">
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
      <RefList refs={n.imports.map((m) => `module:${m}`)} empty="No resolved imports." titles={titlesOf(n.out)} />
    </>
  );
}

/** A module's criteria from its coverage answer, each with what names and checks it. */
function moduleCriteria(a: CoverageAnswer | undefined): CriterionCoverage[] {
  if (!a || !("criteria" in a)) return [];
  return a.criteria.filter((c): c is CriterionCoverage => "requirements" in c);
}

/** The requirements naming a criterion, from its coverage answer. */
function coveredBy(a: CoverageAnswer | undefined): Summary[] | undefined {
  return a && "requirements" in a ? a.requirements : undefined;
}

function Criterion({ n }: { n: CriterionNode }) {
  const ref = `criterion:${n.module}#${n.id}`;
  const cov = useQuery(() => api.coverage(ref), [ref]);
  const src = useQuery(() => api.moduleSource(ref), [ref]);
  return (
    <>
      <p>{n.statement}</p>
      <p className="text-fg-dim">
        In <RefLink ref={`module:${n.module}`} />
        {src.data && (
          <>
            {" "}
            · <SourceLink file={src.data.file} line={src.data.line} />
          </>
        )}
      </p>
      <h2>Covered by</h2>
      {coveredBy(cov.data)?.length === 0 && <p className={empty}>No requirement names this criterion.</p>}
      <ul className={list}>
        {coveredBy(cov.data)?.map((r) => (
          <li key={r.ref} className={item}>
            <RefLink ref={r.ref} title={r.title} />
          </li>
        ))}
      </ul>
    </>
  );
}

/** A file as its annotated source: every line numbered, the lines the graph attaches nodes to marked and linked. */
function File({ n }: { n: FileNode }) {
  const ref = `file:${n.path}`;
  const src = useQuery(() => api.source(ref), [ref]);
  const { hash } = useLocation();
  const focus = /^#L\d+$/.test(hash) ? Number(hash.slice(2)) : null;
  // Once the text is in, bring the line asked for into view.
  useEffect(() => {
    if (focus && src.data) document.getElementById(`L${focus}`)?.scrollIntoView({ block: "center" });
  }, [focus, src.data]);
  const byLine = new Map<number, Annotation[]>();
  for (const a of src.data?.annotations ?? []) byLine.set(a.line, [...(byLine.get(a.line) ?? []), a]);
  const lines = src.data ? src.data.text.split(/\r?\n/) : [];
  return (
    <>
      <p className="text-fg-dim">
        {n.language} · in <RefLink ref={`module:${n.module}`} />
      </p>
      {src.error && <ErrorBox error={src.error} />}
      {!src.data && !src.error && <p className="text-fg-dim">Loading…</p>}
      {src.data && (
        <div className="bg-bg-1 border border-line rounded-md overflow-x-auto" role="region" aria-label={`Source of ${n.path}`}>
          <table className="border-collapse w-full font-mono text-[12.5px] leading-[1.55]">
            <caption className="sr-only">Source of {n.path}, each line the grimoire attaches a node to described by its marks</caption>
            <tbody>
              {lines.map((text, i) => {
                const line = i + 1;
                const marks = byLine.get(line) ?? [];
                const on = line === focus;
                // An annotated line gets its marks on a row of their own just
                // above it, so they show however long the line is.
                return (
                  <Fragment key={line}>
                    {marks.length > 0 && (
                      <tr className="bg-bg-2" id={`marks-L${line}`}>
                        <td />
                        <td className="pt-1.5 pb-0.5 font-sans text-xs whitespace-normal">
                          {marks.map((a) => (
                            <SourceMark key={a.ref} a={a} />
                          ))}
                        </td>
                      </tr>
                    )}
                    <tr
                      id={`L${line}`}
                      aria-current={on ? "location" : undefined}
                      aria-describedby={marks.length ? `marks-L${line}` : undefined}
                      className={on ? "bg-accent-dim/40" : marks.length ? "bg-bg-2" : ""}
                    >
                      <td className="text-right pr-3 pl-2 text-fg-faint select-none align-top w-[1%] whitespace-nowrap">
                        {/* A router link, so the marked line follows the hash, Back and Forward included. */}
                        <Link to={{ hash: `L${line}` }} aria-label={`Line ${line}`} className="text-fg-faint hover:text-fg">
                          {line}
                        </Link>
                      </td>
                      <td className="pr-4 align-top whitespace-pre text-fg">{text}</td>
                    </tr>
                  </Fragment>
                );
              })}
            </tbody>
          </table>
        </div>
      )}
    </>
  );
}

const MARK_TONE: Record<Annotation["kind"], string> = { contract: "text-module", test: "text-test", decl: "text-fg-dim" };

/** What a line is: its node, linked, and for a contract its criteria or for a test block what it checks. */
function SourceMark({ a }: { a: Annotation }) {
  return (
    <span className="inline-flex gap-1.5 items-baseline mr-4" data-annotation={a.kind}>
      <span className={`uppercase tracking-[0.05em] text-[10.5px] ${MARK_TONE[a.kind]}`}>{a.kind}</span>
      <Link to={nodePath(a.ref)}>{a.kind === "decl" ? a.title : idOf(a.ref)}</Link>
      {a.links.map((l) => (
        <Link key={l.ref} to={nodePath(l.ref)} title={l.title ?? undefined} className="text-fg-dim">
          {l.ref.includes("#") ? `#${l.ref.split("#")[1]}` : idOf(l.ref)}
        </Link>
      ))}
    </span>
  );
}

function Decl({ n, ref }: { n: DeclNode; ref: Ref }) {
  const file = idOf(ref).split("#")[0];
  return (
    <p className="text-fg-dim">
      {n.kind} at <SourceLink file={file} line={n.line} />
    </p>
  );
}

function TestView({ n }: { n: TestNode }) {
  return (
    <>
      <p className="text-fg-dim">
        Test block at <SourceLink file={n.file} line={n.line} />
      </p>
      <h2>Checks</h2>
      <RefList refs={n.checks.map((c) => (c.startsWith("req:") ? c : `criterion:${c}`))} empty="Nothing." titles={titlesOf(n.out)} />
    </>
  );
}

function RestsOn({ ref }: { ref: Ref }) {
  const { data } = useQuery(() => api.restsOn(ref, 3), [ref]);
  if (!data) return null;
  return (
    <div className={card}>
      <h3 className="mb-2">Rests on</h3>
      {data.nodes.length === 0 && <p className={empty}>Nothing below this node.</p>}
      <ul className={list}>
        {data.nodes.slice(0, 40).map((r) => (
          <li key={r.ref} className="flex flex-col items-stretch">
            <RefLink ref={r.ref} title={r.title} />
            {r.decisions[0] && <Decision date={r.decisions[0].date} text={r.decisions[0].text} />}
          </li>
        ))}
        {data.nodes.length > 40 && <li className="text-fg-faint">…and {data.nodes.length - 40} more</li>}
      </ul>
    </div>
  );
}

function RestsOnMe({ ref }: { ref: Ref }) {
  const { data } = useQuery(() => api.restsOnMe(ref), [ref]);
  if (!data) return null;
  return (
    <div className={card}>
      <h3 className="mb-2">Rests on this</h3>
      {data.nodes.length === 0 && <p className={empty}>Nothing depends on this node.</p>}
      <ul className={list}>
        {data.nodes.map((a) => (
          <li key={a.ref} className={item}>
            <span className={rel}>{a.relation}</span>
            <RefLink ref={a.ref} title={a.title} />
          </li>
        ))}
      </ul>
      {data.tickets.length > 0 && (
        <>
          <h3 className="mt-2.5 mb-2">Open tickets reaching it</h3>
          <TicketTable tickets={data.tickets} />
        </>
      )}
    </div>
  );
}

function Edges({ title, edges }: { title: string; edges: { ref: Ref; relation: string; title: string | null }[] }) {
  if (edges.length === 0) return null;
  return (
    <div className={card}>
      <h3 className="mb-2">{title}</h3>
      <ul className={list}>
        {edges.map((e, i) => (
          <li key={i} className={item}>
            <span className={rel}>{e.relation}</span>
            <RefLink ref={e.ref} title={e.title ?? undefined} />
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
      <ul className={list}>
        {mine.map((f, i) => (
          <li key={i} className={`${item} ${f.rule === "dangling" ? "text-bad" : "text-warn"}`}>
            <span className={rel}>{f.rule}</span>
            {f.reason}
          </li>
        ))}
      </ul>
    </>
  );
}
