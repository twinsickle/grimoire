/**
 * # Module contract
 *
 * ```json
 * {"purpose":"The map: the outline rendered as nested regions with coverage as fill and lint findings as marks.","criteria":[{"id":"outline-driven","statement":"The map is drawn from the outline tool alone at a chosen depth; it derives no structure of its own."},{"id":"marks","statement":"A region shows its coverage percentage and its finding count; an uncovered criterion is marked as such."}]}
 * ```
 */

import { useState } from "react";
import { Link } from "react-router-dom";
import { api, useQuery } from "../api";
import { ErrorBox, KindTag, nodePath } from "../components";
import { kindOf, type Tree } from "../types";

/**
 * The manual as nested regions: pages hold the pages they link to, those hold
 * requirements and modules, modules hold criteria. Coverage is the fill
 * behind a region's head; lint findings are the mark on the right.
 */
export function MapView() {
  const [depth, setDepth] = useState(4);
  const { data, error } = useQuery(() => api.outline(depth), [depth]);
  return (
    <div className="map">
      <div className="map-controls">
        <span>Depth</span>
        {[2, 3, 4, 6].map((d) => (
          <button key={d} className={d === depth ? "on" : ""} onClick={() => setDepth(d)}>
            {d}
          </button>
        ))}
        <span className="faint">Fill is criteria covered by a requirement; the badge counts lint findings on or below.</span>
      </div>
      {error && <ErrorBox error={error} />}
      {data?.roots.length === 0 && <p className="empty">No root page. The store's `roots` names none that exist.</p>}
      {data?.roots.map((t) => <Region key={t.ref} tree={t} depth={0} />)}
    </div>
  );
}

function Region({ tree, depth }: { tree: Tree; depth: number }) {
  const kind = kindOf(tree.ref);
  const isLeaf = kind === "criterion" || kind === "req" || (tree.children.length === 0 && kind !== "page");
  if (isLeaf) return <Leaf tree={tree} />;
  const pct = tree.coverage == null ? null : Math.round(tree.coverage * 100);
  return (
    <div className={`region depth-${depth % 4}`}>
      {pct != null && <div className="fill" style={{ width: `${pct}%` }} />}
      <div className="head">
        <KindTag ref={tree.ref} />
        <span className="title">
          <Link to={nodePath(tree.ref)}>{tree.title}</Link>
        </span>
        {pct != null && <span className={`badge ${pct === 100 ? "ok" : pct < 50 ? "warn" : ""}`}>{pct}% covered</span>}
        {tree.findings > 0 && <span className="badge bad">{tree.findings} finding{tree.findings === 1 ? "" : "s"}</span>}
      </div>
      {tree.children.length > 0 && (
        <div className="body">
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
  const uncovered = kind === "criterion" && tree.findings > 0;
  const count = kind === "req" && typeof tree.coverage === "number" ? tree.coverage : null;
  return (
    <div className={`leaf ${kind} ${uncovered ? "uncovered" : ""}`} title={tree.title}>
      <KindTag ref={tree.ref} />
      <span className="title">
        <Link to={nodePath(tree.ref)}>{tree.title}</Link>
      </span>
      {count != null && <span className={`badge ${count === 0 ? "warn" : ""}`}>{count} crit</span>}
      {tree.findings > 0 && kind !== "criterion" && <span className="badge bad">{tree.findings}</span>}
      {uncovered && <span className="badge warn">uncovered</span>}
    </div>
  );
}
