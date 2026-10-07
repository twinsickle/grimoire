/**
 * # Module contract
 *
 * ```json
 * {"purpose":"A node page for every kind but a page: what the node is, then one graph centred on it, then the rest of what it holds and its findings, and a ticket's next step to hand to an agent.","criteria":[{"id":"every-kind","statement":"Every node kind but a page has a rendering: requirement, ticket, module, criterion, file (as its annotated source), declaration, test, recipe and ingredient; a page is rendered by the grimoire view, which is its node page."},{"id":"content-then-graph","statement":"A node's page shows, under its title, what the node is: a requirement's statement with its retired or described mark, a ticket's priority, kind and status, a criterion's statement and where it is written, a module's contract and where it is, a recipe's kind, where it is written and the paths of its files not found, an ingredient's recipe; then its graph from web/src/views/NodeGraph; then the rest of what it holds: a requirement's scenarios and decisions, a ticket's body, decision, next step and proposal, a module's criteria each by its id and statement, a recipe's text as Markdown, its ingredients each by its id, linking to its page, and what it says, and its files; then its findings, omens and advisories. It shows no panel of what it rests on or what rests on it, and no list of its edges: the graph shows them. A file's page is its annotated source and draws no graph."},{"id":"ticket-actions","statement":"A ticket's page offers no way to start or close it: an open, active or landing ticket shows, under Next step, the line handoff gives with a Copy button that puts it on the clipboard and says Copied, and a gap or question waiting, landing, for the change that applies its answer shows no line and says it closes when that change lands; a landing ticket's decision is shown as written and waiting for its change to land."},{"id":"annotated-source","statement":"A file's page shows its text from the source tool, line by line with numbers, syntax highlighted by web/src/highlight in the palette's syntax tokens, long lines wrapping under their own indent and a contract's one-line JSON laid out indented under its line, each annotated line marked with links to its node and what that node links (a contract's criteria, a test block's checks); opened at #L<n> it scrolls to that line and marks it; the pages of a module, criterion, declaration and test link to their line there."},{"id":"decisions-in-force","statement":"A requirement's page lists its decisions in force newest first, each with its position and, when it was recorded as an adoption, marked so, and folds the superseded ones beneath, each naming the decision that superseded it; a retired requirement is marked retired, and one that is described, its decisions in force all adoptions, is marked described, not decided."},{"id":"draft-shown","statement":"An active or landing ticket's page shows its proposal as a Proposed section: each proposed page and requirement that would change the truth, and each ticket whose references it would change, with whether it is new or which fields differ, each linking to the node read as proposed (the node page with ?ticket=), and each deletion as a mark, a record equal to the truth's not listed; a node page opened with ?ticket= reads the node through that ticket's proposal and says so, and a requirement's or ticket's page without one names each active or landing ticket whose proposal would change it."},{"id":"stale-reference-marked","statement":"A closed ticket's page hands its graph the references of the ticket's whose node the graph no longer holds, so they are drawn marked as no longer existing and link nowhere; the references of a ticket that is not closed are drawn as links as before, since the check reports those."},{"id":"plain-source","statement":"A file whose source answer is marked plain is shown with web/src/highlight's plain lines, no tone and no JSON laid out, under a note that it is shown plain because of its length, keeping its numbers, marks, links and #L<n>."},{"id":"omens-shown","statement":"A node's page lists, under Omens, each omen of the lint answer on that node, and a module's page each omen on one of its files, with its reason, dimmed and never as a warning, apart from its findings; with none, nothing is shown."},{"id":"advisories-shown","statement":"A node's page lists, under Other work may conflict, each advisory of the lint answer on that node or naming it as an edge's end, with the work it comes from, landed since this branch's base or a ticket linked by id, and its reason, apart from its findings and not as a warning; with none, nothing is shown."}]}
 * ```
 */

import { Fragment, useEffect, useMemo } from "react";
import { Link, useLocation, useParams, useSearchParams } from "react-router-dom";
import { api, useQuery } from "../api";
import type { Draft, ProposalView } from "../generated/tools";
import { NodeGraph } from "./NodeGraph";
import { decisionsInForce, isDescribed } from "../decisions";
import { TAB, highlight, indentWidth, plain, type Token, type Tone } from "../highlight";
import {
  Badge,
  ErrorBox,
  KindTag,
  Markdown,
  PriorityTag,
  RefLink,
  SourceLink,
  StatusTag,
  Handoff,
  card,
  decision as decisionBox,
  decisionDate,
  empty,
  errorBox,
  item,
  list,
  rel,
} from "../components";
import { nodeName, useTabTitle } from "../title";
import {
  goneOf,
  idOf,
  localId,
  nodePath,
  kindOf,
  type AnyNode,
  type Annotation,
  type CriterionNode,
  type DeclNode,
  type TestNode,
  type FileNode,
  type IngredientNode,
  type ModuleNode,
  type RecipeNode,
  type Ref,
  type ReqNode,
  type TicketNode,
} from "../types";

/** One node: its fields, what it rests on, what rests on it, and its edges. */
export function NodeView() {
  const { ref: raw } = useParams();
  const ref = decodeURIComponent(raw ?? "");
  // ?ticket= reads the node as that ticket's proposal would leave it.
  const [params] = useSearchParams();
  const ticket = params.get("ticket") ?? undefined;
  const { data, error } = useQuery(() => api.get(ref, ticket), [ref, ticket]);
  useTabTitle(data && data.node.ref === ref ? nodeName(data.node) : idOf(ref));
  if (error) return <ErrorBox error={error} />;
  // Between two node pages the query keeps the last answer while the next loads; a node of another kind is not this page's.
  if (!data || data.node.ref !== ref) return <p className="text-fg-dim">Loading…</p>;
  const node = data.node;
  const kind = kindOf(ref);
  // An ingredient is of the kind recipe, its id the recipe's, #, its own.
  const ingredient = kind === "recipe" && "says" in node;
  const gone = goneOf(node);
  return (
    <div className="max-w-[150ch] mx-auto">
      <div>
        <div className="flex flex-col gap-1.5 mb-3">
          <div className="flex gap-2.5 items-center">
            <KindTag ref={ref} />
            <code className="text-fg-dim">{idOf(ref)}</code>
          </div>
          <h1>{titleOf(node)}</h1>
          <DraftNotice ref={ref} ticket={ticket} drafts={data.drafts} />
        </div>
        {/* What the node is comes first, then its graph, then the rest of what it holds. */}
        {kind === "req" && <Req n={node as ReqNode} part="lead" />}
        {kind === "ticket" && <Ticket n={node as TicketNode} proposal={data.proposal} part="lead" />}
        {kind === "module" && <Module n={node as ModuleNode} part="lead" />}
        {kind === "criterion" && <Criterion n={node as CriterionNode} />}
        {kind === "recipe" && !ingredient && <Recipe n={node as RecipeNode} part="lead" />}
        {ingredient && <Ingredient n={node as IngredientNode} />}
        {kind !== "file" && <NodeGraph key={`${ref}|${ticket ?? ""}`} node={ref} title={titleOf(node)} incoming={node.in} gone={gone} ticket={ticket} />}
        {kind === "req" && <Req n={node as ReqNode} part="rest" />}
        {kind === "ticket" && <Ticket n={node as TicketNode} proposal={data.proposal} part="rest" />}
        {kind === "module" && <Module n={node as ModuleNode} part="rest" />}
        {kind === "recipe" && !ingredient && <Recipe n={node as RecipeNode} part="rest" />}
        {kind === "file" && <File n={node as FileNode} />}
        {kind === "decl" && <Decl n={node as DeclNode} />}
        {kind === "test" && <TestView n={node as TestNode} />}
        <Findings ref={ref} files={kind === "module" ? (node as ModuleNode).files.map((f) => `file:${f}`) : []} />
      </div>
    </div>
  );
}

/** Read through a ticket, say so; read as the truth, name the drafts that would change it. */
export function DraftNotice({ ref, ticket, drafts }: { ref: Ref; ticket?: string; drafts?: Ref[] }) {
  if (ticket) {
    return (
      <p className="m-0 text-[13px]">
        <Badge tone="warn">draft</Badge> As <RefLink ref={`ticket:${ticket}`} /> proposes it; <Link to={nodePath(ref)}>the truth</Link> may differ.
      </p>
    );
  }
  if (!drafts || drafts.length === 0) return null;
  return (
    <p className="m-0 text-[13px] text-fg-dim">
      {drafts.map((t) => (
        <span key={t}>
          <RefLink ref={t} /> proposes a change to this: <Link to={asProposed(ref, t)}>read it as proposed</Link>.{" "}
        </span>
      ))}
    </p>
  );
}

/** The node's page, read through a ticket's proposal. */
function asProposed(ref: Ref, ticket: Ref): string {
  return `${nodePath(ref)}?ticket=${encodeURIComponent(idOf(ticket))}`;
}

function titleOf(n: AnyNode): string {
  if ("title" in n) return n.title;
  if ("purpose" in n) return n.purpose ?? n.id;
  if ("statement" in n) return n.statement;
  if ("says" in n) return n.says;
  if ("name" in n) return n.name;
  return n.path;
}

function Decision({ date, text, label }: { date: string; text: string; label?: string }) {
  return (
    <div className={decisionBox}>
      {label && <span className={decisionDate}>{label}</span>}
      <span className={decisionDate}>{date}</span>
      {text}
    </div>
  );
}

function Decisions({ n }: { n: ReqNode }) {
  const { inForce, superseded } = decisionsInForce(n.decisions);
  return (
    <>
      <h2>Decisions</h2>
      {n.decisions.length === 0 && <p className={empty}>None recorded. `record_decision` adds one.</p>}
      {inForce.map(({ n: at, d }) => (
        <Decision key={at} label={`#${at}${d.adopted ? " · adoption" : ""}`} date={d.date} text={d.text} />
      ))}
      {superseded.length > 0 && (
        <details className="mt-2">
          <summary className="text-fg-dim text-[13px] cursor-pointer">
            {superseded.length} superseded
          </summary>
          {superseded.map(({ n: at, d, by }) => (
            <Decision key={at} label={`#${at}${d.adopted ? " · adoption" : ""} · superseded by #${by}`} date={d.date} text={d.text} />
          ))}
        </details>
      )}
    </>
  );
}

/** Which part of a node's page a renderer draws: what it is, above the graph, or the rest, below it. */
type Part = "lead" | "rest";

function Req({ n, part }: { n: ReqNode; part: Part }) {
  const label = "text-fg-dim font-semibold text-xs uppercase tracking-[0.04em] mr-1.5";
  if (part === "lead") {
    return (
      <>
        {n.retired && (
          <p>
            <Badge tone="warn">retired</Badge> <span className="text-fg-dim">It no longer holds: no page links it and it covers no criterion. Its last decision says why.</span>
          </p>
        )}
        {!n.retired && isDescribed(n.decisions) && (
          <p data-described>
            <Badge tone="warn">described</Badge> <span className="text-fg-dim">Read off the code at adoption: it says what the code did, not what anyone chose, until a decision is recorded on it.</span>
          </p>
        )}
        <p>{n.statement}</p>
      </>
    );
  }
  return (
    <>
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
      <Decisions n={n} />
    </>
  );
}

function Ticket({ n, proposal, part }: { n: TicketNode; proposal?: ProposalView; part: Part }) {
  const ref = `ticket:${n.slug}`;
  if (part === "lead") {
    return (
      <p>
        <PriorityTag p={n.priority} /> · <span className="text-fg-dim">{n.kind}</span> · <StatusTag s={n.status} withdrawn={n.decision?.withdrawn} />
      </p>
    );
  }
  return (
    <>
      {n.body && <Markdown text={n.body} />}
      {/* A landing ticket's decision is written, not in force: it closes when its change lands. */}
      {n.decision && <Decision date={n.decision.date} text={n.decision.text} label={n.status === "landing" ? "written, waiting for its change to land" : n.decision.withdrawn ? "withdrawn: nothing of it was applied" : undefined} />}
      {n.status !== "closed" && (
        <div className={card}>
          <h3 className="mb-2">Next step</h3>
          <Handoff ticket={{ ref, title: n.title, kind: n.kind, status: n.status }} />
        </div>
      )}
      {(n.status === "active" || n.status === "landing") && <Proposed ticket={ref} proposal={proposal} />}
    </>
  );
}

/** An active or landing ticket's proposal: what it would change in the truth, and how, each readable as proposed. */
function Proposed({ ticket, proposal }: { ticket: Ref; proposal?: ProposalView }) {
  const drafts: Draft[] = [...(proposal?.pages ?? []), ...(proposal?.requirements ?? []), ...(proposal?.tickets ?? [])];
  const deleted = proposal?.deleted ?? [];
  return (
    <>
      <h2>Proposed</h2>
      {drafts.length + deleted.length === 0 ? (
        <p className={empty}>Nothing yet. A write that changes intent under this ticket lands here, and reaches the truth when the ticket closes.</p>
      ) : (
        <ul className={list}>
          {drafts.map((d) => (
            <li key={d.ref} className={item}>
              <RefLink ref={d.ref} title={d.title} />{" "}
              <span className="text-fg-dim text-[13px]">{d.new ? "new" : `changes ${d.differs.join(", ")}`}</span>{" "}
              <Link className="text-[13px]" to={asProposed(d.ref, ticket)}>
                as proposed
              </Link>
            </li>
          ))}
          {deleted.map((r) => (
            <li key={r} className={item}>
              <RefLink ref={r} /> <Badge tone="warn">to be deleted</Badge>
            </li>
          ))}
        </ul>
      )}
    </>
  );
}

function Module({ n, part }: { n: ModuleNode; part: Part }) {
  if (part === "lead") {
    return (
      <>
        {n.contract_error && <p className={errorBox}>Contract: {n.contract_error}</p>}
        {n.contract_at && (
          <p className="text-fg-dim">
            Contract at <SourceLink file={n.contract_at[0]} line={n.contract_at[1]} />
          </p>
        )}
      </>
    );
  }
  return (
    <>
      <h2>Criteria</h2>
      {n.criteria.length === 0 && <p className={empty}>None.</p>}
      <ul className={list}>
        {n.criteria.map((c) => (
          <li key={c.id} className={item}>
            <Link to={nodePath(`criterion:${n.id}#${c.id}`)} className="text-fg-dim font-mono text-[12.5px] shrink-0">
              #{c.id}
            </Link>
            <span>{c.statement}</span>
          </li>
        ))}
      </ul>
    </>
  );
}

function Criterion({ n }: { n: CriterionNode }) {
  const ref = `criterion:${n.module}#${n.id}`;
  const src = useQuery(() => api.moduleSource(ref), [ref]);
  // Its title is its statement, so only where it is written is said beneath.
  return (
    <>
      <p className="text-fg-dim">
        In <RefLink ref={`module:${n.module}`} />
        {src.data && (
          <>
            {" "}
            · <SourceLink file={src.data.file} line={src.data.line} />
          </>
        )}
      </p>
    </>
  );
}

/** A recipe: what it is above its graph; its prose, ingredients and files below. */
function Recipe({ n, part }: { n: RecipeNode; part: Part }) {
  if (part === "lead") {
    return (
      <>
        <p className="text-fg-dim">
          A <span className="text-fg">{n.kind}</span> recipe, written in <code>{n.file}</code> at line {n.line}
        </p>
        {n.missing.length > 0 && (
          <p className={errorBox}>
            Not found in the checkout: {n.missing.map((m, i) => (
              <Fragment key={m}>
                {i > 0 && ", "}
                <code>{m}</code>
              </Fragment>
            ))}
          </p>
        )}
      </>
    );
  }
  return (
    <>
      {n.text && <Markdown text={n.text} />}
      <h2>Ingredients</h2>
      {n.ingredients.length === 0 && <p className={empty}>None.</p>}
      <ul className={list}>
        {n.ingredients.map((i) => (
          <li key={i.id} className={item}>
            <Link to={nodePath(`recipe:${n.id}#${i.id}`)} className="text-fg-dim font-mono text-[12.5px] shrink-0">
              #{i.id}
            </Link>
            <span>{i.says}</span>
          </li>
        ))}
      </ul>
      <h2>Files</h2>
      {n.files.length === 0 && <p className={empty}>None named: nothing in the code is represented yet.</p>}
      <ul className={list}>
        {n.files.map((f) => (
          <li key={f} className={item}>
            <code>{f}</code>
            {n.missing.includes(f) && <Badge tone="warn">not found</Badge>}
          </li>
        ))}
      </ul>
    </>
  );
}

function Ingredient({ n }: { n: IngredientNode }) {
  // Its title is what it says, so only whose it is is said beneath.
  return (
    <p className="text-fg-dim">
      In <RefLink ref={`recipe:${n.recipe}`} />
    </p>
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
  const text = src.data?.text;
  // Past the project's plain-file limit the highlighter never runs: it would drag (req:web-annotated-source).
  const asPlain = src.data?.plain ?? false;
  const lines = useMemo(() => (text === undefined ? [] : asPlain ? plain(text) : highlight(text, n.language)), [text, n.language, asPlain]);
  return (
    <>
      <p className="text-fg-dim">
        {n.language} · in <RefLink ref={`module:${n.module}`} />
      </p>
      {src.error && <ErrorBox error={src.error} />}
      {!src.data && !src.error && <p className="text-fg-dim">Loading…</p>}
      {asPlain && (
        <p className="text-fg-dim" id="plain-note">
          Shown plain: at {countLines(text ?? "").toLocaleString("en")} lines this file is longer than the project highlights.
        </p>
      )}
      {src.data && (
        <div className="bg-bg-1 border border-line rounded-md overflow-x-auto" role="region" aria-label={`Source of ${n.path}`}>
          <table className="border-collapse w-full font-mono text-[12.5px] leading-[1.55]" style={{ tabSize: TAB }}>
            <caption className="sr-only">Source of {n.path}, each line the grimoire attaches a node to described by its marks</caption>
            <tbody>
              {lines.map((tokens, i) => {
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
                      <td className="pr-4 align-top text-fg">
                        {rowsOf(tokens).map((row, r) => {
                          // Wrapped, the rest of a row hangs under its own indent, one step in.
                          const hang = `${leading(row) + 4}ch`;
                          return (
                            <div key={r} className="whitespace-pre-wrap wrap-anywhere" style={{ paddingLeft: hang, textIndent: `-${hang}` }}>
                              {row.map((t, j) =>
                                t.tone ? (
                                  <span key={j} className={TONE[t.tone]} data-tone={t.tone}>
                                    {t.text}
                                  </span>
                                ) : (
                                  <Fragment key={j}>{t.text}</Fragment>
                                ),
                              )}
                            </div>
                          );
                        })}
                      </td>
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

/** How many lines a text has, as the daemon counts them: a final line feed ends the last line and starts none. */
function countLines(text: string): number {
  return text === "" ? 0 : text.split("\n").length - (text.endsWith("\n") ? 1 : 0);
}

/** A line's tokens as the rows it shows on: one, or several where a contract's JSON is laid out. */
function rowsOf(tokens: Token[]): Token[][] {
  const rows: Token[][] = [[]];
  for (const t of tokens) {
    const [first, ...rest] = t.text.split("\n");
    if (first) rows[rows.length - 1].push({ ...t, text: first });
    for (const piece of rest) rows.push(piece ? [{ ...t, text: piece }] : []);
  }
  return rows;
}

/** The width of a row's leading whitespace, in columns. */
function leading(row: Token[]): number {
  return indentWidth(row.map((t) => t.text).join(""));
}

// Written out whole so the stylesheet generator sees each class.
const TONE: Record<Tone, string> = {
  keyword: "text-syn-keyword",
  string: "text-syn-string",
  comment: "text-syn-comment italic",
  number: "text-syn-number",
  type: "text-syn-type",
  function: "text-syn-function",
  meta: "text-syn-meta",
};

const MARK_TONE: Record<Annotation["kind"], string> = { contract: "text-module", test: "text-test", decl: "text-fg-dim" };

/** What a line is: its node, linked, and for a contract its criteria or for a test block what it checks. */
function SourceMark({ a }: { a: Annotation }) {
  return (
    <span className="inline-flex gap-1.5 items-baseline mr-4" data-annotation={a.kind}>
      <span className={`uppercase tracking-[0.05em] text-[10.5px] ${MARK_TONE[a.kind]}`}>{a.kind}</span>
      <Link to={nodePath(a.ref)}>{a.kind === "decl" ? a.title : idOf(a.ref)}</Link>
      {a.links.map((l) => (
        <Link key={l.ref} to={nodePath(l.ref)} title={l.title ?? undefined} className="text-fg-dim">
          {l.ref.includes("#") ? `#${localId(l.ref)}` : idOf(l.ref)}
        </Link>
      ))}
    </span>
  );
}

function Decl({ n }: { n: DeclNode }) {
  return (
    <p className="text-fg-dim">
      {n.kind} at <SourceLink file={n.file} line={n.line} />
    </p>
  );
}

function TestView({ n }: { n: TestNode }) {
  return (
    <>
      <p className="text-fg-dim">
        Test block at <SourceLink file={n.file} line={n.line} />
      </p>
    </>
  );
}

function Findings({ ref, files }: { ref: Ref; files: Ref[] }) {
  const { data } = useQuery(() => api.lint(), []);
  const about = (f: { ref: Ref; edge?: { to: Ref } }) => f.ref === ref || f.edge?.to === ref;
  const mine = data?.findings.filter(about) ?? [];
  // Signs, never failures: on the node, or for a module on one of its files (req:lint-omens).
  const omens = (data?.omens ?? []).filter((o) => o.ref === ref || files.includes(o.ref));
  // Other work's, shown apart and never as a warning (req:check-own-work).
  const others = data?.advisories.filter((a) => about(a.finding)) ?? [];
  if (mine.length === 0 && omens.length === 0 && others.length === 0) return null;
  return (
    <>
      {mine.length > 0 && <FindingList findings={mine} />}
      {omens.length > 0 && (
        <>
          <h2>Omens</h2>
          <ul className={list} id="omens">
            {omens.map((o) => (
              <li key={`${o.ref} ${o.check}`} className={`${item} text-fg-dim`}>
                <span className={rel}>{o.check}</span>
                {o.ref !== ref && <RefLink ref={o.ref} />}
                {o.reason}
              </li>
            ))}
          </ul>
        </>
      )}
      {others.length > 0 && (
        <>
          <h2>Other work may conflict</h2>
          <ul className={list}>
            {others.map((a, i) => (
              <li key={i} className="text-fg-dim">
                <div className="text-xs text-fg-faint">
                  {a.from.kind === "ticket" ? (
                    <>
                      <RefLink ref={`ticket:${a.from.ticket}`} title={a.from.title} /> would bring this when it lands
                    </>
                  ) : (
                    "Landed on the main line since this branch's base: merge the main line"
                  )}
                </div>
                {a.finding.reason}
              </li>
            ))}
          </ul>
        </>
      )}
    </>
  );
}

function FindingList({ findings: mine }: { findings: { rule: string; reason: string }[] }) {
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
