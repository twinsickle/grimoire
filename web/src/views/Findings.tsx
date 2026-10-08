/**
 * # Module contract
 *
 * ```json
 * {"purpose":"The findings view: every finding, recipe due, advisory and omen of the lint answer in one place, read as a list or as the grimoire's tree marked where trouble is.","criteria":[{"id":"list","statement":"As a list, the view groups the lint answer under New, the findings marksOf calls new, which fail the check; Recipes due; and Other work may conflict, the advisories; each heading with its mark and count, and each group with none saying so in words; beneath them, folded and each with its count, the inherited findings and the omens. Each row is a link to the page of the node it is about, giving its mark's MarkDot and MarkChip, the node's kind and id, the check, the reason and, for an advisory, the work it comes from."},{"id":"tree","statement":"As a tree, the view draws the outline answer's roots, asked as deep as the grimoire view's rail asks, as nested rows, each with a MarkDot per kind of its node's marks, its kind and title; by default only the nodes troubleOnly keeps, with a pressed Only where there is trouble button that, released, shows the whole outline; then, under Not in the outline, each node outsideOutline gives. Pressing a row selects its node, kept in the address as ?at=<ref>, and the selected node is shown beside the tree with its kind and title, its marks through MarkWords and a link to its page; with none selected, it says to choose one."},{"id":"switch-remembered","statement":"A List | Tree switch, its pressed button the one shown, chooses how the view reads; the view opens on the list unless the browser's storage holds the last choice, which each press writes there; reading or writing the storage failing leaves the list and the switch working as before."}]}
 * ```
 */

import { useState } from "react";
import { Link, useSearchParams } from "react-router-dom";
import { api, useQuery } from "../api";
import { ErrorBox, KindTag, empty, mono, pressed, rel } from "../components";
import { MARK_NAME, kindsOf, marksOf, outsideOutline, troubleOnly, type Mark, type MarkKind, type Marks } from "../marks";
import { useTabTitle } from "../title";
import { idOf, nodePath, type LintAnswer, type Ref, type Tree } from "../types";
import { MarkChip, MarkDot, MarkWords } from "./Marks";
import { ALL } from "./Home";

type As = "list" | "tree";
const AS_KEY = "grimoire.findings.as";

/** The last choice of list or tree this browser kept; the list when it kept none or will not say. */
function rememberedAs(): As {
  try {
    return localStorage.getItem(AS_KEY) === "tree" ? "tree" : "list";
  } catch {
    return "list";
  }
}

function rememberAs(as: As) {
  try {
    localStorage.setItem(AS_KEY, as);
  } catch {
    // Storage refused: the choice holds for this page only.
  }
}

export function FindingsView() {
  useTabTitle("Findings");
  const lint = useQuery(() => api.lint(), []);
  const [as, setAs] = useState<As>(rememberedAs);
  const choose = (next: As) => {
    setAs(next);
    rememberAs(next);
  };
  if (lint.error) return <ErrorBox error={lint.error} />;
  return (
    <div className="max-w-[150ch] mx-auto">
      <div className="flex flex-wrap items-baseline gap-x-4 gap-y-2 mb-3">
        <h1 className="m-0">Findings</h1>
        {lint.data && <Counts lint={lint.data} />}
        <div role="group" aria-label="Read as" className="ml-auto inline-flex">
          {(["list", "tree"] as const).map((a, i) => (
            <button
              key={a}
              type="button"
              aria-pressed={as === a}
              onClick={() => choose(a)}
              className={`text-[13px] ${i === 0 ? "rounded-r-none" : "rounded-l-none -ml-px"} ${as === a ? pressed : ""}`}
            >
              {a === "list" ? "List" : "Tree"}
            </button>
          ))}
        </div>
      </div>
      {!lint.data ? <p className="text-fg-dim">Loading…</p> : as === "list" ? <List lint={lint.data} /> : <TreeView marks={marksOf(lint.data)} />}
    </div>
  );
}

/** How much of each there is, the new in the bad colour when there are any. */
function Counts({ lint }: { lint: LintAnswer }) {
  const rows = rowsOf(lint);
  const n = (k: MarkKind) => rows.filter((r) => r.kind === k).length;
  return (
    <p className="m-0 text-[13px] text-fg-dim">
      <span className={n("new") > 0 ? "text-bad font-semibold" : ""}>{n("new")} new</span> · {n("due")} {n("due") === 1 ? "recipe" : "recipes"} due · {n("conflict")} may conflict · {n("inherited")} inherited · {n("omen")} {n("omen") === 1 ? "omen" : "omens"}
    </p>
  );
}

/** One row of the list: a mark on the node it is about, each finding once. */
function rowsOf(lint: LintAnswer): Mark[] {
  const all = marksOf(lint);
  // marksOf marks an edge's end as well; the list says each once, on the node it is about.
  return [...all.entries()].flatMap(([on, ms]) => ms.filter((m) => m.ref === on));
}

const NONE: Record<MarkKind, string> = {
  new: "Nothing fails the check.",
  due: "No recipe is due to be judged.",
  conflict: "No other work may conflict with this checkout.",
  inherited: "Nothing inherited.",
  omen: "No omens.",
};

const HEADING: Record<MarkKind, string> = {
  new: "New, failing the check",
  due: "Recipes due",
  conflict: "Other work may conflict",
  inherited: "inherited",
  omen: "omens",
};

function List({ lint }: { lint: LintAnswer }) {
  const rows = rowsOf(lint);
  const of = (k: MarkKind) => rows.filter((r) => r.kind === k);
  return (
    <>
      {(["new", "due", "conflict"] as const).map((k) => (
        <section key={k} aria-label={HEADING[k]} data-group={k}>
          <h2 className="flex items-center gap-2">
            <MarkDot kind={k} />
            {HEADING[k]} <span className="text-fg-faint font-normal">{of(k).length}</span>
          </h2>
          {of(k).length === 0 ? <p className={empty}>{NONE[k]}</p> : <Rows rows={of(k)} />}
        </section>
      ))}
      {(["inherited", "omen"] as const).map((k) => (
        <details key={k} data-group={k} className="mt-4">
          <summary className="cursor-pointer text-fg-dim inline-flex items-center gap-2">
            <MarkDot kind={k} />
            {of(k).length} {k === "omen" && of(k).length === 1 ? "omen" : HEADING[k]}
          </summary>
          {of(k).length === 0 ? <p className={`${empty} mt-1.5`}>{NONE[k]}</p> : <Rows rows={of(k)} />}
        </details>
      ))}
    </>
  );
}

function Rows({ rows }: { rows: Mark[] }) {
  return (
    <ul className="list-none m-0 mt-1.5 p-0 flex flex-col">
      {rows.map((m, i) => (
        <li key={`${m.ref} ${m.check} ${i}`}>
          <Link
            to={nodePath(m.ref)}
            data-mark-row={m.kind}
            className={`grid grid-cols-[7.5em_minmax(12em,22em)_9em_1fr] gap-3 items-baseline px-2 py-1 -mx-2 rounded no-underline hover:no-underline hover:bg-bg-2 ${m.kind === "new" ? "text-fg" : "text-fg-dim"}`}
          >
            <span className="inline-flex items-center gap-1.5">
              <MarkDot kind={m.kind} />
              <MarkChip kind={m.kind} />
            </span>
            <span className="inline-flex items-center gap-1.5 min-w-0">
              <KindTag ref={m.ref} />
              <span className={`${mono} truncate text-fg`}>{idOf(m.ref)}</span>
            </span>
            <span className={rel}>{m.check}</span>
            <span className={m.kind === "omen" ? "italic" : ""}>
              {m.reason}
              {m.from && <span className="text-fg-faint text-[12.5px]"> · {m.from.kind === "ticket" ? `ticket ${m.from.ticket}, ${m.from.title}, would bring this when it lands` : "landed on the main line since this branch's base: merge the main line"}</span>}
            </span>
          </Link>
        </li>
      ))}
    </ul>
  );
}

function TreeView({ marks }: { marks: Marks }) {
  const outline = useQuery(() => api.outline({ depth: ALL }), []);
  const [onlyTrouble, setOnlyTrouble] = useState(true);
  const [params, setParams] = useSearchParams();
  const at = params.get("at");
  if (outline.error) return <ErrorBox error={outline.error} />;
  if (!outline.data) return <p className="text-fg-dim">Loading…</p>;
  const all = [...outline.data.roots, ...outline.data.unplaced];
  const trees = onlyTrouble ? troubleOnly(all, marks) : all;
  const outside = outsideOutline(all, marks);
  const select = (ref: Ref) => setParams({ at: ref }, { replace: true });
  const title = at ? (titleIn(all, at) ?? idOf(at)) : null;
  return (
    <div className="grid grid-cols-[minmax(0,1fr)_minmax(16rem,24rem)] gap-5 items-start">
      <div className="min-w-0">
        <button type="button" aria-pressed={onlyTrouble} onClick={() => setOnlyTrouble((o) => !o)} className={`text-[13px] mb-2 ${onlyTrouble ? pressed : ""}`}>
          Only where there is trouble
        </button>
        <nav aria-label="Marked outline" className="border border-line rounded-md bg-bg-1 py-1.5">
          {trees.length === 0 ? <p className={`${empty} px-3 m-0`}>Nothing in the outline is marked.</p> : <Branch trees={trees} depth={0} marks={marks} at={at} onSelect={select} />}
        </nav>
        {outside.length > 0 && (
          <>
            <h2>Not in the outline</h2>
            <ul className="list-none m-0 p-0 border border-line rounded-md bg-bg-1 py-1.5" aria-label="Not in the outline">
              {outside.map((r) => (
                <TreeRow key={r} ref_={r} title={idOf(r)} depth={0} marks={marks} at={at} onSelect={select} />
              ))}
            </ul>
          </>
        )}
      </div>
      <aside aria-label="Selected" className="sticky top-0 border border-line rounded-md bg-bg-1 px-3.5 py-3">
        {at && title !== null ? (
          <>
            <div className="flex items-center gap-2 mb-1.5">
              <KindTag ref={at} />
              <code className="text-fg-dim">{idOf(at)}</code>
            </div>
            <h2 className="mt-0">{title}</h2>
            {(marks.get(at)?.length ?? 0) > 0 ? <MarkWords on={at} marks={marks.get(at) ?? []} /> : <p className={empty}>No trouble on it; it holds some beneath it.</p>}
            <Link to={nodePath(at)}>Open its page</Link>
          </>
        ) : (
          <p className={`${empty} m-0`}>Choose a node to read its trouble here.</p>
        )}
      </aside>
    </div>
  );
}

function titleIn(trees: Tree[], ref: Ref): string | undefined {
  for (const t of trees) {
    if (t.ref === ref) return t.title;
    const below = titleIn(t.children, ref);
    if (below !== undefined) return below;
  }
  return undefined;
}

function Branch({ trees, depth, marks, at, onSelect }: { trees: Tree[]; depth: number; marks: Marks; at: string | null; onSelect: (ref: Ref) => void }) {
  return (
    <ul className="list-none m-0 p-0">
      {trees.map((t, i) => (
        <TreeRow key={`${t.ref} ${i}`} ref_={t.ref} title={t.title} depth={depth} marks={marks} at={at} onSelect={onSelect}>
          {t.children.length > 0 && <Branch trees={t.children} depth={depth + 1} marks={marks} at={at} onSelect={onSelect} />}
        </TreeRow>
      ))}
    </ul>
  );
}

function TreeRow({
  ref_: ref,
  title,
  depth,
  marks,
  at,
  onSelect,
  children,
}: {
  ref_: Ref;
  title: string;
  depth: number;
  marks: Marks;
  at: string | null;
  onSelect: (ref: Ref) => void;
  children?: React.ReactNode;
}) {
  const mine = marks.get(ref) ?? [];
  const kinds = kindsOf(mine);
  const selected = at === ref;
  return (
    <li>
      <button
        type="button"
        onClick={() => onSelect(ref)}
        aria-current={selected || undefined}
        aria-label={`${title}${kinds.length ? `, marked ${kinds.map((k) => MARK_NAME[k]).join(", ")}` : ""}`}
        data-tree-node={ref}
        data-marks={kinds.join(" ") || undefined}
        style={{ paddingLeft: 10 + depth * 18 }}
        className={`w-full flex items-center gap-2 text-left border-0 rounded-none pr-3 py-[3px] ${selected ? "bg-bg-3" : "bg-transparent hover:bg-bg-2"}`}
      >
        <span className="inline-flex gap-[3px] w-[30px] shrink-0 justify-end">
          {kinds.map((k) => (
            <MarkDot key={k} kind={k} reasons={mine.filter((m) => m.kind === k).map((m) => m.reason)} />
          ))}
        </span>
        <KindTag ref={ref} />
        <span className={`truncate text-[13px] ${kinds.length ? "text-fg" : "text-fg-dim"}`}>{title}</span>
      </button>
      {children}
    </li>
  );
}
