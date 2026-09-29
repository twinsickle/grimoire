/**
 * # Module contract
 *
 * ```json
 * {"purpose":"Lay out what a ticket rests on as a tree to draw: a column per distance from the ticket, each node level with its first child, and deeper branches folded into counts by kind when the walk is large.","criteria":[{"id":"tree-from-via","statement":"Each node of the walk is placed in the column of its distance, under the last node of its via path, with one line from that node into it; a subtree takes consecutive rows and a node sits on its first child's row, so no two lines cross."},{"id":"folding","statement":"A walk of at most 40 nodes is laid out whole; above that, the start's children are laid out and below every other node its children are folded into one count per kind, in kind order, and a count named in the unfolded set is laid out as those nodes, their own children folded in turn; the layout counts the groups it laid out open because the set named them."}]}
 * ```
 */

import { kindOf, type Kind, type Ref, type RestsOn } from "./types";

/** A walk this size or smaller is drawn whole. */
export const OPEN_UP_TO = 40;

/** The order kinds are listed in under one node: the grimoire's reading order, down to the code. */
const KIND_ORDER: Kind[] = ["page", "req", "ticket", "module", "criterion", "file", "decl", "test"];

/** A node drawn in the graph. */
/** A node drawn in the graph; `parentTitle` names the node it hangs under, null for the start. */
export type GraphNode = { type: "node"; ref: Ref; title: string; parentTitle: string | null; col: number; row: number };
/** Children of one kind under a node, folded into their count. `key` names it in the unfolded set. */
export type GraphFold = { type: "fold"; key: string; parent: Ref; parentTitle: string; kind: Kind; count: number; col: number; row: number };
export type GraphItem = GraphNode | GraphFold;
/** A line from a node to an item one column to its right. */
export type GraphLine = { from: { col: number; row: number }; to: { col: number; row: number } };
/** `unfolded` counts the groups laid out open because the unfolded set named them, not because the walk is small. */
export type GraphLayout = { items: GraphItem[]; lines: GraphLine[]; cols: number; rows: number; folded: boolean; unfolded: number };

/** The key of the children of kind `kind` under `parent`. */
export function foldKey(parent: Ref, kind: Kind): string {
  return `${parent}|${kind}`;
}

/**
 * Lay out the walk from `start`. `walk` is the rests_on answer: each node's
 * `via` begins at the start and ends at the node it was reached from.
 */
export function layout(start: Ref, startTitle: string, walk: RestsOn[], unfolded: ReadonlySet<string>): GraphLayout {
  const children = new Map<Ref, RestsOn[]>();
  for (const n of walk) {
    const parent = n.via[n.via.length - 1] ?? start;
    const list = children.get(parent) ?? [];
    list.push(n);
    children.set(parent, list);
  }
  const open = walk.length <= OPEN_UP_TO;
  const items: GraphItem[] = [];
  const lines: GraphLine[] = [];
  let rows = 0;
  let cols = 1;
  let opened = 0;

  /** Place `ref` and what is shown below it; returns the row it sits on. */
  function place(ref: Ref, title: string, parentTitle: string | null, col: number): number {
    cols = Math.max(cols, col + 1);
    const node: GraphNode = { type: "node", ref, title, parentTitle, col, row: -1 };
    items.push(node);
    const below: { col: number; row: number }[] = [];
    for (const [kind, group] of byKind(children.get(ref) ?? [])) {
      const key = foldKey(ref, kind);
      if (open || col === 0 || unfolded.has(key)) {
        if (!open && col > 0) opened++;
        for (const c of group) below.push({ col: col + 1, row: place(c.ref, c.title, title, col + 1) });
      } else {
        const row = rows++;
        cols = Math.max(cols, col + 2);
        items.push({ type: "fold", key, parent: ref, parentTitle: title, kind, count: group.length, col: col + 1, row });
        below.push({ col: col + 1, row });
      }
    }
    node.row = below.length ? below[0].row : rows++;
    for (const to of below) lines.push({ from: { col, row: node.row }, to });
    return node.row;
  }

  place(start, startTitle, null, 0);
  return { items, lines, cols, rows, folded: items.some((i) => i.type === "fold"), unfolded: opened };
}

/** Children grouped by kind in kind order, each group in the walk's order. */
function byKind(children: RestsOn[]): [Kind, RestsOn[]][] {
  const groups = new Map<Kind, RestsOn[]>();
  for (const c of children) {
    const k = kindOf(c.ref);
    groups.set(k, [...(groups.get(k) ?? []), c]);
  }
  return [...groups.entries()].sort((a, b) => KIND_ORDER.indexOf(a[0]) - KIND_ORDER.indexOf(b[0]));
}
