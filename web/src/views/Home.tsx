/**
 * # Module contract
 *
 * ```json
 * {"purpose":"The grimoire view: every page as a tree in a rail, and beside it the root, a page or the unplaced modules, with how far adoption has come as bars; a page's view is its node page.","criteria":[{"id":"story-order","statement":"At the root, only the grimoire's own level is shown: the root page drawn as any page is, with the whole grimoire's rollup bars and the pages it links as cards in link order, each with its first paragraph, rollup bars and findings."},{"id":"contracts-grouped","statement":"At a page, its body is rendered as Markdown with its rollup bars, the pages it links as cards, every module it links once with its purpose and its criteria beneath it, a module with no contract marked so, and its requirements; a body longer than five lines shows five and a Show more; files and declarations are not shown."},{"id":"outline-driven","statement":"The rail and every number shown are the outline tool's answer: the rail lists its pages nested and ordered as its tree, ending with Unplaced only when the outline's unplaced list is not empty, and the bars and finding counts are the outline's own."},{"id":"marks","statement":"The rail shows each page's new findings as a warning and its inherited ones dimmed; a criterion is marked uncovered, untested or inherited from the checks and baselined count outline reports; a requirement shows its criteria, or the tests holding it when it names none, and warns only when it has neither."}]}
 * ```
 */

import { Link } from "react-router-dom";
import { api, useQuery } from "../api";
import { Badge, ErrorBox, Folded, KindTag, card, empty, mono } from "../components";
import { idOf, kindOf, localId, nodePath, type PageNode, type Ref, type Rollup, type Tree } from "../types";

/** Deep enough to reach every page, so the rail is the whole outline. */
const ALL = 64;

/** Where the grimoire view stands: the root, one page, or the modules no page links. */
export type At = { root: true } | { page: Ref } | { unplaced: true };

/** Read the grimoire: a rail of every page, and the root, a page or the unplaced modules beside it. */
export function HomeView({ at }: { at: At }) {
  const { data, error } = useQuery(() => api.outline(ALL), []);
  if (error) return <ErrorBox error={error} />;
  if (!data) return <p className="text-fg-dim">Loading…</p>;
  const root = data.roots[0];
  if (!root) return <p className={empty}>No root page. The store's `roots` names none that exist.</p>;
  const selected = "page" in at ? at.page : "unplaced" in at ? "unplaced" : root.ref;
  const path = "page" in at ? pathTo(root, at.page) : null;
  return (
    <div className="grid grid-cols-[250px_minmax(0,1fr)] gap-8 items-start max-w-[150ch] mx-auto">
      <Rail root={root} unplaced={data.unplaced} selected={selected} />
      <div className="min-w-0">
        {"root" in at && <PagePane tree={root} above={[]} />}
        {"unplaced" in at && <UnplacedPane modules={data.unplaced} />}
        {"page" in at && (path ? <PagePane tree={path[path.length - 1]} above={path.slice(0, -1)} /> : <p className={empty}>{at.page} is not in the outline: no page above it links it.</p>)}
      </div>
    </div>
  );
}

/** The trees from the root down to `ref`, first found in link order. */
function pathTo(t: Tree, ref: Ref): Tree[] | null {
  if (t.ref === ref) return [t];
  for (const c of t.children) {
    const p = pathTo(c, ref);
    if (p) return [t, ...p];
  }
  return null;
}

const pages = (t: Tree) => t.children.filter((c) => kindOf(c.ref) === "page");

// ---------------------------------------------------------------------------
// The rail
// ---------------------------------------------------------------------------

function Rail({ root, unplaced, selected }: { root: Tree; unplaced: Tree[]; selected: string }) {
  return (
    <nav aria-label="Pages" className="sticky top-0 max-h-[calc(100vh-6rem)] overflow-auto bg-bg-1 border border-line rounded-md py-2">
      <ul className="list-none m-0 p-0">
        <RailItem tree={root} depth={0} selected={selected} to="/" />
      </ul>
      {unplaced.length > 0 && (
        <Link
          to="/unplaced"
          aria-current={selected === "unplaced" ? "page" : undefined}
          className={`${railRow(selected === "unplaced")} mt-1 border-t border-line pt-2`}
          style={{ paddingLeft: 12 }}
        >
          <span className="flex-1">Unplaced</span>
          <Badge tone="warn">{unplaced.length}</Badge>
        </Link>
      )}
    </nav>
  );
}

function railRow(on: boolean): string {
  return `flex items-center gap-2 pr-2.5 py-1 text-[13.5px] hover:no-underline ${on ? "bg-bg-3 text-fg" : "text-fg-dim hover:text-fg"}`;
}

function RailItem({ tree, depth, selected, to }: { tree: Tree; depth: number; selected: string; to?: string }) {
  const on = tree.ref === selected;
  const fresh = tree.findings - tree.baselined;
  return (
    <li>
      <Link to={to ?? nodePath(tree.ref)} aria-current={on ? "page" : undefined} className={railRow(on)} style={{ paddingLeft: 12 + depth * 14 }}>
        <span className="flex-1 overflow-hidden text-ellipsis whitespace-nowrap">{tree.title}</span>
        {fresh > 0 && <Badge tone="bad">{fresh} new</Badge>}
        {tree.baselined > 0 && (
          <span className="text-fg-faint text-xs" title={`${tree.baselined} inherited`}>
            {tree.baselined}
          </span>
        )}
      </Link>
      {pages(tree).length > 0 && (
        <ul className="list-none m-0 p-0">
          {pages(tree).map((c) => (
            <RailItem key={c.ref} tree={c} depth={depth + 1} selected={selected} />
          ))}
        </ul>
      )}
    </li>
  );
}

// ---------------------------------------------------------------------------
// Bars and counts
// ---------------------------------------------------------------------------

/** The rollup as three bars: modules contracted, criteria tested, requirements held. */
function Bars({ rollup, compact = false }: { rollup: Rollup; compact?: boolean }) {
  const rows: [string, number, number][] = [
    ["Contracted", rollup.contracted, rollup.modules],
    ["Criteria tested", rollup.tested, rollup.criteria],
    ["Requirements held", rollup.held, rollup.requirements],
  ];
  const shown = rows.filter(([, , of]) => of > 0);
  if (shown.length === 0) return null;
  return (
    <dl className={`grid ${compact ? "grid-cols-[120px_minmax(0,1fr)_64px] text-xs" : "grid-cols-[150px_minmax(0,360px)_80px] text-[12.5px]"} gap-x-3 gap-y-1 items-center m-0 text-fg-dim`}>
      {shown.map(([label, n, of]) => (
        <Bar key={label} label={label} n={n} of={of} />
      ))}
    </dl>
  );
}

function Bar({ label, n, of }: { label: string; n: number; of: number }) {
  const whole = n === of;
  return (
    <>
      <dt>{label}</dt>
      <dd className="m-0 h-1 rounded-sm bg-bg-3" role="meter" aria-label={label} aria-valuemin={0} aria-valuemax={of} aria-valuenow={n}>
        <div className={`h-1 rounded-sm ${whole ? "bg-ok" : "bg-warn"}`} style={{ width: `${(100 * n) / of}%` }} />
      </dd>
      <dd className="m-0 text-fg">
        {n} of {of}
      </dd>
    </>
  );
}

/** Findings on or below a node: new as a warning, inherited dimmed. */
function FindingCounts({ tree }: { tree: Tree }) {
  const fresh = tree.findings - tree.baselined;
  return (
    <>
      {fresh > 0 && <Badge tone="bad">{fresh} new</Badge>}
      {tree.baselined > 0 && <span className="text-fg-dim text-[12.5px]">{tree.baselined} inherited</span>}
    </>
  );
}

const sectionHead = "text-[19px] flex items-center gap-2.5 mb-2 pt-2 border-t border-line";

function Cards({ trees }: { trees: Tree[] }) {
  return (
    <div id="pages" className="grid grid-cols-[repeat(auto-fill,minmax(340px,1fr))] gap-2.5 mt-3">
      {trees.map((p) => (
        <AreaCard key={p.ref} tree={p} />
      ))}
    </div>
  );
}

/** A card's line: the first paragraph with a sentence in it, as plain text without Markdown marks. */
function firstParagraph(body: string): string {
  const plain = body.split(/\n\s*\n/).map((p) =>
    p
      .replace(/^\s*(?:[-*+]|\d+\.|#+)\s+/gm, "")
      .replace(/(\*\*|__|`)/g, "")
      .replace(/\[([^\]]*)\]\([^)]*\)/g, "$1")
      .replace(/\s+/g, " ")
      .trim(),
  );
  return plain.find((p) => p.length > 40) ?? plain[0] ?? "";
}

function AreaCard({ tree }: { tree: Tree }) {
  const { data } = useQuery(() => api.get(tree.ref), [tree.ref]);
  const page = data?.node as PageNode | undefined;
  return (
    <Link to={nodePath(tree.ref)} className={`${card} flex flex-col gap-2 text-fg hover:border-accent hover:no-underline m-0`}>
      <div className="flex items-center gap-2">
        <strong className="flex-1">{tree.title}</strong>
        <FindingCounts tree={tree} />
      </div>
      <p className="text-fg-dim m-0 text-[13px] line-clamp-2">{page ? firstParagraph(page.body) : "…"}</p>
      <Bars rollup={tree.rollup} compact />
    </Link>
  );
}

// ---------------------------------------------------------------------------
// A page
// ---------------------------------------------------------------------------

function PagePane({ tree, above }: { tree: Tree; above: Tree[] }) {
  const { data } = useQuery(() => api.get(tree.ref), [tree.ref]);
  const page = data?.node as PageNode | undefined;
  const modules = tree.children.filter((c) => kindOf(c.ref) === "module");
  const reqs = tree.children.filter((c) => kindOf(c.ref) === "req");
  return (
    <div className="flex flex-col gap-6 max-w-[110ch]">
      <header className="flex flex-col gap-2.5">
        {above.length > 0 && (
          <nav aria-label="Breadcrumb" className="text-[13px] text-fg-dim flex gap-1.5">
            {above.map((t, i) => (
              <span key={t.ref}>
                <Link to={i === 0 ? "/" : nodePath(t.ref)}>{t.title}</Link> /
              </span>
            ))}
          </nav>
        )}
        <div className="flex items-center gap-3">
          <h1 className="m-0 text-[24px]">{tree.title}</h1>
          <code className="text-fg-dim">{idOf(tree.ref)}</code>
          <span className="flex-1" />
          <FindingCounts tree={tree} />
        </div>
        <Bars rollup={tree.rollup} />
        {page ? <Folded key={tree.ref} text={page.body} /> : <p className="text-fg-dim">Loading…</p>}
      </header>
      {pages(tree).length > 0 && <Cards trees={pages(tree)} />}
      {modules.length > 0 && (
        <section id="modules">
          <h2 className={sectionHead}>
            <span className="text-module">Modules</span>
            <span className="text-fg-dim text-[13px] font-normal">{modules.length}</span>
          </h2>
          <Modules modules={modules} />
        </section>
      )}
      {reqs.length > 0 && (
        <section id="requirements">
          <h2 className={sectionHead}>
            <span className="text-req">Requirements</span>
            <span className="text-fg-dim text-[13px] font-normal">{reqs.length}</span>
          </h2>
          <RequirementList reqs={reqs} />
        </section>
      )}
    </div>
  );
}

function Modules({ modules }: { modules: Tree[] }) {
  return (
    <ul className="list-none m-0 p-0 flex flex-col gap-2.5">
      {modules.map((m) => (
        <li key={m.ref} className="grid grid-cols-[minmax(180px,30ch)_minmax(0,1fr)] gap-x-4 items-baseline">
          <Link to={nodePath(m.ref)} className={`${mono} text-module overflow-hidden text-ellipsis whitespace-nowrap`}>
            {idOf(m.ref)}
          </Link>
          <div className="flex flex-col gap-1">
            {m.checks.includes("no-contract") ? (
              <span>
                <Badge tone="warn">no contract</Badge>
              </span>
            ) : (
              <span className="text-fg-dim text-[13px]">{m.title}</span>
            )}
            {m.children.length > 0 && (
              <div className="flex flex-wrap gap-1.5">
                {m.children.map((c) => (
                  <CriterionChip key={c.ref} tree={c} />
                ))}
              </div>
            )}
          </div>
        </li>
      ))}
    </ul>
  );
}

/** A criterion by its id, marked with what outline says is wrong with it. */
function CriterionChip({ tree }: { tree: Tree }) {
  const marks = tree.checks.filter((c) => c === "uncovered-criterion" || c === "untested").map((c) => (c === "uncovered-criterion" ? "uncovered" : c));
  const warn = marks.length > 0;
  const inherited = warn && tree.baselined === tree.findings;
  return (
    <Link
      to={nodePath(tree.ref)}
      title={tree.title}
      className={`${mono} text-[11.5px] px-1.5 rounded border hover:no-underline ${warn ? "text-warn border-warn/50" : "text-criterion border-line"}`}
    >
      #{localId(tree.ref)}
      {warn && ` · ${marks.join(", ")}${inherited ? ", inherited" : ""}`}
    </Link>
  );
}

function RequirementList({ reqs }: { reqs: Tree[] }) {
  return (
    <ul className="list-none m-0 mt-2 p-0 grid grid-cols-[repeat(auto-fill,minmax(360px,1fr))] gap-x-6 gap-y-1">
      {reqs.map((r) => {
        const count = typeof r.coverage === "number" ? r.coverage : 0;
        return (
          <li key={r.ref} className="flex gap-2 items-baseline text-[13px] min-w-0">
            <KindTag ref={r.ref} />
            <Link to={nodePath(r.ref)} className="flex-1 overflow-hidden text-ellipsis whitespace-nowrap">
              {r.title}
            </Link>
            {count > 0 && (
              <span className="text-fg-dim text-xs whitespace-nowrap">
                {count} criteri{count === 1 ? "on" : "a"}
              </span>
            )}
            {count === 0 && r.tests > 0 && (
              <span className="text-test text-xs whitespace-nowrap">
                held by {r.tests} test{r.tests === 1 ? "" : "s"}
              </span>
            )}
            {count === 0 && r.tests === 0 && <Badge tone="warn">uncovered</Badge>}
          </li>
        );
      })}
    </ul>
  );
}

// ---------------------------------------------------------------------------
// Unplaced
// ---------------------------------------------------------------------------

function UnplacedPane({ modules }: { modules: Tree[] }) {
  return (
    <div className="flex flex-col gap-4 max-w-[110ch]">
      <h1 className="m-0 text-[24px]">Unplaced</h1>
      <p className="text-fg-dim m-0">Modules of the grimoire that no page links yet. Link each from the page of its area.</p>
      {modules.length > 0 ? <Modules modules={modules} /> : <p className={empty}>Every module is on a page.</p>}
    </div>
  );
}
