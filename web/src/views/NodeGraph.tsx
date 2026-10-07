/**
 * # Module contract
 *
 * ```json
 * {"purpose":"Draw a node's graph on its page, centred on it: what rests on it to the left and what it rests on to the right, as boxes that link to each node's page over lines between them, long groups folded into counts that unfold.","criteria":[{"id":"centred","statement":"A node's page draws the node in the middle, what it rests on to its right as the layout of its rests_on walk from web/src/graph, and what rests on it to its left: the nodes whose edges lead to it, but not its own criteria or files, a module's also the requirements its criteria's coverage names, and of tickets only those the board lists as being worked or waited on; read through a ticket's proposal when the page is, its walk and coverage asked through that ticket; each node a link to its page showing its kind and title and naming how it connects, what it hangs under or what it rests on, a reference the page names as no longer existing drawn dimmed and linking nowhere; on the right a walk larger than 40 is folded as graph's folding says, and on the left a group of one kind larger than six is a folded count that unfolds, while any count is unfolded a button folding them all again; with nothing on either side no graph is drawn."}]}
 * ```
 */

import { useState } from "react";
import { Link } from "react-router-dom";
import { api, useQuery } from "../api";
import { KindTag } from "../components";
import { foldKey, layout, type GraphItem } from "../graph";
import { idOf, kindOf, nodePath, type Kind, type Ref } from "../types";

const COL = 224;
const BOX = 190;
const ROW = 34;
const BOX_H = 26;

/** A group of one kind on the left past this many is folded into its count. */
const LEFT_OPEN_UP_TO = 6;

/** The order kinds are drawn in on the left, the grimoire's reading order. */
const LEFT_ORDER: Kind[] = ["page", "req", "ticket", "module", "criterion", "file", "decl", "test"];

const PLURAL: Record<Kind, string> = {
  page: "pages",
  req: "requirements",
  ticket: "tickets",
  module: "modules",
  criterion: "criteria",
  file: "files",
  decl: "declarations",
  test: "tests",
};

/** An item placed to draw; `up` marks one on the left, which rests on the node. */
type Placed = GraphItem & { x: number; y: number; up: boolean };
type Line = { x1: number; y1: number; x2: number; y2: number };

/** What rests on a node, one step up: its incoming edges but its own parts, and for a module the requirements behind its criteria. */
function useDependents(ref: Ref, incoming: { ref: Ref; relation: string; title: string | null }[], ticket?: string) {
  const kind = kindOf(ref);
  const cov = useQuery(() => (kind === "module" ? api.coverage(ref, ticket) : Promise.resolve(null)), [ref, ticket]);
  // The tickets still being worked, from the board: one cheap call, not the walk of all that rests on the node.
  const board = useQuery(() => api.board({}), []);
  if (!board.data || (kind === "module" && cov.data === undefined)) return null;
  const live = new Set<Ref>();
  for (const t of [...board.data.tickets, ...board.data.unattached]) live.add(t.ref);
  for (const t of board.data.tickets) for (const w of t.waits_on ?? []) live.add(w.ref);
  const seen = new Map<Ref, string>();
  for (const e of incoming) {
    // A node's own criteria and files are what it holds, not what rests on it.
    if (e.relation === "belongs_to" || e.relation === "contains") continue;
    // A ticket no longer being worked is history, not context.
    if (kindOf(e.ref) === "ticket" && !live.has(e.ref)) continue;
    seen.set(e.ref, e.title ?? idOf(e.ref));
  }
  const a = cov.data;
  if (a && "criteria" in a) {
    for (const c of a.criteria) if ("requirements" in c) for (const r of c.requirements) seen.set(r.ref, r.title);
  }
  return [...seen.entries()].map(([r, title]) => ({ ref: r, title }));
}

/** A node's graph, centred on it, read through `ticket`'s proposal when one is given; `gone` names the references that no longer exist. */
export function NodeGraph({
  node,
  title,
  incoming,
  gone,
  ticket,
}: {
  node: Ref;
  title: string;
  incoming: { ref: Ref; relation: string; title: string | null }[];
  gone: Set<Ref>;
  ticket?: string;
}) {
  const [unfolded, setUnfolded] = useState<Set<string>>(new Set());
  const down = useQuery(() => api.restsOn(node, undefined, ticket), [node, ticket]);
  const up = useDependents(node, incoming, ticket);
  if (!down.data || !up) return null;
  if (down.data.nodes.length === 0 && up.length === 0) return null;

  // The right: what it rests on, laid out from the node at column 0.
  const right = layout(node, title, down.data.nodes, unfolded);

  // The left: what rests on it, one column, grouped by kind, a long group folded.
  type LeftItem = GraphItem;
  const left: LeftItem[] = [];
  const groups = new Map<Kind, { ref: Ref; title: string }[]>();
  for (const d of up) groups.set(kindOf(d.ref), [...(groups.get(kindOf(d.ref)) ?? []), d]);
  let lrow = 0;
  for (const kind of LEFT_ORDER) {
    const group = groups.get(kind);
    if (!group) continue;
    const key = `up|${foldKey(node, kind)}`;
    if (group.length > LEFT_OPEN_UP_TO && !unfolded.has(key)) {
      left.push({ type: "fold", key, parent: node, parentTitle: title, kind, count: group.length, col: 0, row: lrow++ });
    } else {
      for (const g of group) left.push({ type: "node", ref: g.ref, title: g.title, parentTitle: title, col: 0, row: lrow++ });
    }
  }

  // Line the node up with the middle of the left column and the top of its right tree.
  const startRow = right.items[0]?.row ?? 0;
  const leftMid = left.length ? Math.floor((left.length - 1) / 2) : 0;
  const centre = Math.max(startRow, leftMid);
  const rightShift = centre - startRow;
  const leftShift = centre - leftMid;
  const leftW = left.length ? COL : 0;
  const rows = Math.max(right.rows + rightShift, lrow + leftShift, 1);
  const width = leftW + (right.cols - 1) * COL + BOX;

  const placed: Placed[] = [
    ...left.map((i) => ({ ...i, x: 0, y: (i.row + leftShift) * ROW, up: true })),
    ...right.items.map((i) => ({ ...i, x: leftW + i.col * COL, y: (i.row + rightShift) * ROW, up: false })),
  ];
  const mid = BOX_H / 2;
  const lines: Line[] = [
    ...left.map((i) => ({ x1: BOX, y1: (i.row + leftShift) * ROW + mid, x2: leftW, y2: centre * ROW + mid })),
    ...right.lines.map((l) => ({
      x1: leftW + l.from.col * COL + BOX,
      y1: (l.from.row + rightShift) * ROW + mid,
      x2: leftW + l.to.col * COL,
      y2: (l.to.row + rightShift) * ROW + mid,
    })),
  ];
  const unfold = (key: string) => setUnfolded((s) => new Set([...s, key]));
  const anyUnfolded = unfolded.size > 0;
  return (
    <section aria-label="Graph" className="my-4">
      <div className="flex items-baseline gap-3">
        <h2>Graph</h2>
        <span className="text-fg-dim text-[13px]">
          {up.length > 0 && `${up.length} rest${up.length === 1 ? "s" : ""} on this`}
          {up.length > 0 && down.data.nodes.length > 0 && " · "}
          {down.data.nodes.length > 0 && `rests on ${down.data.nodes.length}`}
          {right.folded && ", deeper branches folded"}
        </span>
        {anyUnfolded && (
          <button className="ml-auto" onClick={() => setUnfolded(new Set())}>
            Fold all
          </button>
        )}
      </div>
      <div className="overflow-x-auto border border-line rounded-md bg-bg-1 p-3">
        <div className="relative" style={{ width, height: rows * ROW }}>
          <svg aria-hidden="true" className="absolute inset-0 overflow-visible" width={width} height={rows * ROW}>
            {lines.map((l, i) => {
              const m = l.x1 + (l.x2 - l.x1) / 2;
              return <path key={i} d={`M ${l.x1} ${l.y1} H ${m} V ${l.y2} H ${l.x2}`} className="fill-none stroke-line" strokeWidth={1.5} />;
            })}
          </svg>
          {placed.map((i) => (
            <Box
              key={`${i.x}:${i.type === "node" ? i.ref : i.key}`}
              item={i}
              centre={i.type === "node" && i.ref === node}
              gone={i.type === "node" && gone.has(i.ref)}
              onUnfold={unfold}
            />
          ))}
        </div>
      </div>
    </section>
  );
}

function Box({ item, centre, gone, onUnfold }: { item: Placed; centre: boolean; gone: boolean; onUnfold: (key: string) => void }) {
  const style = { left: item.x, top: item.y, width: BOX, height: BOX_H };
  if (item.type === "fold") {
    const what = `${item.count} ${item.count === 1 ? item.kind : PLURAL[item.kind]}`;
    return (
      <button
        style={style}
        onClick={() => onUnfold(item.key)}
        aria-label={item.up ? `Show the ${what} resting on ${item.parentTitle}` : `Show the ${what} under ${item.parentTitle}`}
        className="absolute flex items-center gap-2 px-2 text-[12.5px] text-fg-dim border border-dashed border-line rounded-md bg-bg-2"
      >
        <span className="shrink-0">
          <KindTag ref={`${item.kind}:`} />
        </span>
        <span>+{what}</span>
      </button>
    );
  }
  if (gone) {
    return (
      <span
        style={style}
        title="no longer exists"
        aria-label={`${kindOf(item.ref)} ${idOf(item.ref)}, no longer exists${connection(item)}`}
        data-graph-node={item.ref}
        data-gone={item.ref}
        className="absolute flex items-center gap-2 px-2 border border-dashed border-line rounded-md bg-bg-1"
      >
        <span className="shrink-0">
          <KindTag ref={item.ref} />
        </span>
        <span className="text-fg-faint text-[12.5px] line-through overflow-hidden text-ellipsis whitespace-nowrap">{idOf(item.ref)}</span>
      </span>
    );
  }
  if (centre) {
    return (
      <span style={style} title={item.title} data-graph-node={item.ref} className="absolute flex items-center gap-2 px-2 border border-accent rounded-md bg-bg-3">
        <span className="shrink-0">
          <KindTag ref={item.ref} />
        </span>
        <span className="text-fg text-[12.5px] font-semibold overflow-hidden text-ellipsis whitespace-nowrap">{item.title}</span>
      </span>
    );
  }
  return (
    <Link
      to={nodePath(item.ref)}
      style={style}
      title={item.title}
      aria-label={`${kindOf(item.ref)} ${item.title}${connection(item)}`}
      data-graph-node={item.ref}
      className="absolute flex items-center gap-2 px-2 border border-line rounded-md bg-bg-2 no-underline hover:border-accent-dim"
    >
      <span className="shrink-0">
        <KindTag ref={item.ref} />
      </span>
      <span className="text-fg text-[12.5px] overflow-hidden text-ellipsis whitespace-nowrap">{item.title}</span>
    </Link>
  );
}

/** How a box connects to the graph, read out: what it hangs under on the right, what it rests on on the left. */
function connection(item: Placed): string {
  if (item.type !== "node" || !item.parentTitle) return "";
  return item.up ? `, rests on ${item.parentTitle}` : `, under ${item.parentTitle}`;
}
