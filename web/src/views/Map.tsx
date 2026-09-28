/**
 * # Module contract
 *
 * ```json
 * {"purpose":"The map: the outline rendered as nested regions with coverage as fill and lint findings as marks.","criteria":[{"id":"outline-driven","statement":"The map is drawn from the outline tool alone at a chosen depth; it derives no structure of its own."},{"id":"marks","statement":"A region shows its coverage percentage and its finding count; a criterion no requirement names is marked uncovered and one no test checks is marked untested, from the checks outline reports; a requirement shows its criteria, or the tests holding it when it names none, and warns only when it has neither."}]}
 * ```
 */

import { useState } from "react";
import { Link } from "react-router-dom";
import { api, useQuery } from "../api";
import { Badge, ErrorBox, KindTag, empty, pressed } from "../components";
import { kindOf, nodePath, type Tree } from "../types";

/**
 * The grimoire as nested regions: pages hold the pages they link to, those hold
 * requirements and modules, modules hold criteria. Coverage is the fill
 * behind a region's head; lint findings are the mark on the right.
 */
export function MapView() {
  const [depth, setDepth] = useState(4);
  const { data, error } = useQuery(() => api.outline(depth), [depth]);
  return (
    <div className="flex flex-col gap-2.5">
      <div className="flex gap-2 items-center text-fg-dim">
        <span>Depth</span>
        {[2, 3, 4, 6].map((d) => (
          <button key={d} className={d === depth ? pressed : ""} onClick={() => setDepth(d)}>
            {d}
          </button>
        ))}
        <span className="text-fg-faint">Fill is criteria covered by a requirement; the badge counts lint findings on or below.</span>
      </div>
      {error && <ErrorBox error={error} />}
      {data?.roots.length === 0 && <p className={empty}>No root page. The store's `roots` names none that exist.</p>}
      {data?.roots.map((t) => <Region key={t.ref} tree={t} depth={0} />)}
    </div>
  );
}

/** Alternating backgrounds so nested regions read as levels. */
const DEPTH_BG = ["bg-bg-1", "bg-bg-2", "bg-bg-1", "bg-bg-2"];

function Region({ tree, depth }: { tree: Tree; depth: number }) {
  const kind = kindOf(tree.ref);
  const isLeaf = kind === "criterion" || kind === "req" || (tree.children.length === 0 && kind !== "page");
  if (isLeaf) return <Leaf tree={tree} />;
  const pct = tree.coverage == null ? null : Math.round(tree.coverage * 100);
  return (
    <div className={`border border-line rounded-md pt-2 px-2.5 pb-2.5 relative overflow-hidden ${DEPTH_BG[depth % 4]}`}>
      {pct != null && <div className="absolute left-0 top-0 bottom-0 bg-module/10 pointer-events-none" style={{ width: `${pct}%` }} />}
      <div className="flex items-center gap-2 relative">
        <KindTag ref={tree.ref} />
        <span className="font-semibold">
          <Link to={nodePath(tree.ref)} className="text-fg">
            {tree.title}
          </Link>
        </span>
        {pct != null && <Badge tone={pct === 100 ? "ok" : pct < 50 ? "warn" : "plain"}>{pct}% covered</Badge>}
        {tree.findings > 0 && (
          <Badge tone="bad">
            {tree.findings} finding{tree.findings === 1 ? "" : "s"}
          </Badge>
        )}
      </div>
      {tree.children.length > 0 && (
        <div className="flex flex-wrap gap-2 mt-2 relative">
          {tree.children.map((c) => (
            <Region key={c.ref} tree={c} depth={depth + 1} />
          ))}
        </div>
      )}
    </div>
  );
}

function Leaf({ tree }: { tree: Tree }) {
  const kind = kindOf(tree.ref);
  const uncovered = kind === "criterion" && tree.checks.includes("uncovered-criterion");
  const untested = kind === "criterion" && tree.checks.includes("untested");
  const count = kind === "req" && typeof tree.coverage === "number" ? tree.coverage : null;
  return (
    <div
      className={`border rounded-md px-2 py-[5px] bg-bg inline-flex gap-2 items-center max-w-full ${kind === "criterion" ? "border-dashed" : ""} ${uncovered || untested ? "border-warn" : "border-line"}`}
      title={tree.title}
    >
      <KindTag ref={tree.ref} />
      <span className="overflow-hidden text-ellipsis whitespace-nowrap max-w-[46ch]">
        <Link to={nodePath(tree.ref)} className="text-fg">
          {tree.title}
        </Link>
      </span>
      {count != null && count > 0 && <Badge tone="plain">{count} crit</Badge>}
      {count === 0 && tree.tests > 0 && <Badge tone="plain">held by {tree.tests} test{tree.tests === 1 ? "" : "s"}</Badge>}
      {count === 0 && tree.tests === 0 && <Badge tone="warn">0 crit</Badge>}
      {tree.findings > 0 && kind !== "criterion" && <Badge tone="bad">{tree.findings}</Badge>}
      {uncovered && <Badge tone="warn">uncovered</Badge>}
      {untested && <Badge tone="warn">untested</Badge>}
    </div>
  );
}
