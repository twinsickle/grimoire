/**
 * # Module contract
 *
 * ```json
 * {"purpose":"Draw a ticket's graph on its page: the layout of its whole rests_on walk from web/src/graph, as boxes that link to each node's page over lines between them, with folded counts that unfold.","criteria":[{"id":"on-ticket-page","statement":"A ticket's page shows the graph of its whole rests_on walk above its touches, each node a link to its page showing its kind and title, each folded count a button naming how many of which kind under which node it unfolds, and while any count is unfolded a button that folds them all again; each node link names the node it hangs under, so the edges are read out as well as drawn; a ticket that rests on nothing draws no graph, its page saying so in what it rests on."}]}
 * ```
 */

import { useState } from "react";
import { Link } from "react-router-dom";
import { api, useQuery } from "../api";
import { KindTag } from "../components";
import { layout, type GraphItem, type GraphLine } from "../graph";
import { kindOf, nodePath, type Kind, type Ref } from "../types";

const COL = 224;
const BOX = 190;
const ROW = 34;
const BOX_H = 26;

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

/** What a ticket rests on, drawn as a tree from the ticket at the left. */
export function TicketGraph({ ticket, title }: { ticket: Ref; title: string }) {
  const [unfolded, setUnfolded] = useState<Set<string>>(new Set());
  const { data } = useQuery(() => api.restsOn(ticket), [ticket]);
  // A ticket resting on nothing would draw one lone box; the page already says it rests on nothing.
  if (!data || data.nodes.length === 0) return null;
  const g = layout(ticket, title, data.nodes, unfolded);
  const unfold = (key: string) => setUnfolded((s) => new Set([...s, key]));
  return (
    <section aria-label="What this ticket rests on" className="mb-4">
      <div className="flex items-baseline gap-3">
        <h2>Graph</h2>
        <span className="text-fg-dim text-[13px]">
          {data.nodes.length} node{data.nodes.length === 1 ? "" : "s"} it rests on{g.folded ? ", deeper branches folded" : ""}
        </span>
        {g.unfolded > 0 && (
          <button className="ml-auto" onClick={() => setUnfolded(new Set())}>
            Fold all
          </button>
        )}
      </div>
      <div className="overflow-x-auto border border-line rounded-md bg-bg-1 p-3">
        <div className="relative" style={{ width: (g.cols - 1) * COL + BOX, height: g.rows * ROW }}>
          <svg aria-hidden="true" className="absolute inset-0 overflow-visible" width={(g.cols - 1) * COL + BOX} height={g.rows * ROW}>
            {g.lines.map((l) => (
              <path key={`${l.from.col},${l.from.row}-${l.to.col},${l.to.row}`} d={elbow(l)} className="fill-none stroke-line" strokeWidth={1.5} />
            ))}
          </svg>
          {g.items.map((i) => (
            <Item key={i.type === "node" ? `n:${i.ref}` : `f:${i.key}`} item={i} onUnfold={unfold} />
          ))}
        </div>
      </div>
    </section>
  );
}

/** A line from the right edge of a box to the left edge of one a column over, turning halfway across the gap. */
function elbow(l: GraphLine): string {
  const x1 = l.from.col * COL + BOX;
  const x2 = l.to.col * COL;
  const y1 = l.from.row * ROW + BOX_H / 2;
  const y2 = l.to.row * ROW + BOX_H / 2;
  const mid = x1 + (x2 - x1) / 2;
  return `M ${x1} ${y1} H ${mid} V ${y2} H ${x2}`;
}

function Item({ item, onUnfold }: { item: GraphItem; onUnfold: (key: string) => void }) {
  const style = { left: item.col * COL, top: item.row * ROW, width: BOX, height: BOX_H };
  if (item.type === "fold") {
    const what = `${item.count} ${item.count === 1 ? item.kind : PLURAL[item.kind]}`;
    return (
      <button
        style={style}
        onClick={() => onUnfold(item.key)}
        aria-label={`Show the ${what} under ${item.parentTitle}`}
        className="absolute flex items-center gap-2 px-2 text-[12.5px] text-fg-dim border border-dashed border-line rounded-md bg-bg-2"
      >
        <span className="shrink-0">
          <KindTag ref={`${item.kind}:`} />
        </span>
        <span>+{what}</span>
      </button>
    );
  }
  return (
    <Link
      to={nodePath(item.ref)}
      style={style}
      title={item.title}
      aria-label={`${kindOf(item.ref)} ${item.title}${item.parentTitle ? `, under ${item.parentTitle}` : ""}`}
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
