// The ticket graph's layout, without a browser. The view is tested in a
// browser by web/behaviour.

import { expect, test } from "vitest";
import { foldKey, layout, OPEN_UP_TO, type GraphFold, type GraphNode } from "./graph";
import type { Ref, RestsOn } from "./types";

const S: Ref = "ticket:s";

/** A walk entry reached along `via`, which starts at the ticket. */
function at(ref: Ref, ...via: Ref[]): RestsOn {
  return { ref, title: ref, via: [S, ...via], decisions: [] };
}

function nodes(l: ReturnType<typeof layout>): Record<string, [number, number]> {
  return Object.fromEntries(l.items.filter((i): i is GraphNode => i.type === "node").map((n) => [n.ref, [n.col, n.row]]));
}

/**
 * # Test contract
 *
 * ```json
 * {"test": "a-walk-lays-out-as-a-tree", "checks": ["web/src/graph#tree-from-via"]}
 * ```
 */
test("each node sits in its distance's column, under the node it was reached from, level with its first child", () => {
  const walk = [at("req:a"), at("module:b"), at("criterion:a#1", "req:a"), at("criterion:a#2", "req:a"), at("module:c", "module:b")];
  const l = layout(S, "S", walk, new Set());
  expect(nodes(l)).toEqual({
    [S]: [0, 0],
    "req:a": [1, 0],
    "criterion:a#1": [2, 0],
    "criterion:a#2": [2, 1],
    "module:b": [1, 2],
    "module:c": [2, 2],
  });
  expect(l.lines).toHaveLength(walk.length);
  expect(l.lines).toContainEqual({ from: { col: 1, row: 0 }, to: { col: 2, row: 1 } });
  expect(l.lines).toContainEqual({ from: { col: 0, row: 0 }, to: { col: 1, row: 2 } });
  expect([l.cols, l.rows, l.folded]).toEqual([3, 3, false]);
  // No two lines cross: every line leaving a column spans rows no other parent's lines enter.
  const spans = new Map<string, [number, number]>();
  for (const { from, to } of l.lines) {
    const k = `${from.col},${from.row}`;
    const [lo, hi] = spans.get(k) ?? [from.row, from.row];
    spans.set(k, [Math.min(lo, to.row), Math.max(hi, to.row)]);
  }
  const byCol = [...spans.entries()].map(([k, s]) => [Number(k.split(",")[0]), ...s]);
  for (const a of byCol) for (const b of byCol) if (a !== b && a[0] === b[0]) expect(a[2] < b[1] || b[2] < a[1]).toBe(true);
});

/**
 * # Test contract
 *
 * ```json
 * {"test": "a-large-walk-folds-by-kind", "checks": ["web/src/graph#folding"]}
 * ```
 */
test("a walk over the limit folds every branch below the ticket's own into counts by kind, which unfold one at a time", () => {
  const reqs = Array.from({ length: OPEN_UP_TO }, (_, i) => at(`req:r${i}`, "page:p"));
  const walk = [at("page:p"), at("module:m", "page:p"), ...reqs, at("criterion:r0#c", "page:p", "req:r0")];
  const folded = layout(S, "S", walk, new Set());
  const folds = folded.items.filter((i): i is GraphFold => i.type === "fold");
  expect(Object.keys(nodes(folded))).toEqual([S, "page:p"]);
  expect(folds.map((f) => [f.kind, f.count, f.col])).toEqual([
    ["req", OPEN_UP_TO, 2],
    ["module", 1, 2],
  ]);
  expect(folded.folded).toBe(true);

  const open = layout(S, "S", walk, new Set([foldKey("page:p", "req")]));
  expect(Object.keys(nodes(open))).toHaveLength(2 + OPEN_UP_TO);
  const below = open.items.filter((i): i is GraphFold => i.type === "fold").map((f) => f.key);
  expect(below).toEqual([foldKey("req:r0", "criterion"), foldKey("page:p", "module")]);
  expect([folded.unfolded, open.unfolded], "only groups opened by the set are counted").toEqual([0, 1]);
  expect(open.items.find((i): i is GraphNode => i.type === "node" && i.ref === "req:r1")?.parentTitle).toBe("page:p");
  // A key for a group that is not folded any more counts nothing.
  expect(layout(S, "S", walk.slice(0, OPEN_UP_TO), new Set([foldKey("page:p", "req")])).unfolded).toBe(0);

  // At the limit, everything is drawn.
  const small = layout(S, "S", walk.slice(0, OPEN_UP_TO), new Set());
  expect(small.folded).toBe(false);
  expect(Object.keys(nodes(small))).toHaveLength(1 + OPEN_UP_TO);
});
