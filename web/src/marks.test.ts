// The marks of trouble, sorted from a lint answer without a browser. How they are drawn is tested in a browser by
// web/behaviour.

import { expect, test } from "vitest";
import { kindsOf, markBorder, marksOf, outsideOutline, troubleOnly } from "./marks";
import type { Finding, LintAnswer, Rollup, Tree } from "./types";

function finding(ref: string, check: string, baselined = false, to?: string): Finding {
  return { rule: "uncovered", check, ref, reason: `${check} on ${ref}`, baselined, ...(to ? { edge: { from: ref, to, relation: "covered_by" } } : {}) };
}

const lint: Pick<LintAnswer, "findings" | "advisories" | "omens"> = {
  findings: [
    finding("module:a", "untested", true),
    finding("req:r", "dangling-edge", false, "criterion:a#gone"),
    finding("recipe:x", "due-recipe"),
    finding("recipe:y", "due-recipe", true),
    finding("module:a", "no-contract"),
  ],
  advisories: [{ from: { kind: "ticket", ticket: "0009", title: "Elsewhere" }, finding: finding("req:r", "dangling-edge", false, "criterion:a#other") }],
  omens: [{ check: "large-file", ref: "file:a.rs", reason: "big" }],
};

/**
 * # Test contract
 *
 * ```json
 * {"test": "lint-sorts-into-marks", "checks": ["web/src/marks#marks-of"]}
 * ```
 */
test("each finding, advisory and omen marks the node it is about, and an edge's end", () => {
  const m = marksOf(lint);
  expect(m.get("module:a")?.map((x) => x.kind)).toEqual(["new", "inherited"]);
  expect(m.get("recipe:x")?.map((x) => x.kind)).toEqual(["due"]);
  // A due recipe in the baseline is inherited like any finding there.
  expect(m.get("recipe:y")?.map((x) => x.kind)).toEqual(["inherited"]);
  expect(m.get("req:r")?.map((x) => x.kind)).toEqual(["new", "conflict"]);
  expect(m.get("req:r")?.[1].from).toEqual({ kind: "ticket", ticket: "0009", title: "Elsewhere" });
  // The edge's end is marked too, with the finding still saying whose it is.
  expect(m.get("criterion:a#gone")?.map((x) => [x.kind, x.ref])).toEqual([["new", "req:r"]]);
  expect(m.get("criterion:a#other")?.map((x) => x.kind)).toEqual(["conflict"]);
  expect(m.get("file:a.rs")?.map((x) => [x.kind, x.reason])).toEqual([["omen", "big"]]);
  expect(m.has("page:quiet")).toBe(false);
});

/**
 * # Test contract
 *
 * ```json
 * {"test": "marks-gravest-first", "checks": ["web/src/marks#gravest-first"]}
 * ```
 */
test("a node's marks are gravest first, and its border takes the gravest's colour", () => {
  const m = marksOf({
    findings: [finding("req:z", "a", true), finding("req:z", "due-recipe"), finding("req:z", "b"), finding("req:z", "c")],
    advisories: [{ from: { kind: "since_base" }, finding: finding("req:z", "d") }],
    omens: [{ check: "large-file", ref: "req:z", reason: "o" }],
  });
  expect(m.get("req:z")?.map((x) => `${x.kind}:${x.check}`)).toEqual(["new:b", "new:c", "due:due-recipe", "conflict:d", "inherited:a", "omen:large-file"]);
  expect(kindsOf(m.get("req:z"))).toEqual(["new", "due", "conflict", "inherited", "omen"]);
  expect(markBorder(kindsOf(m.get("req:z")))).toBe("border-bad");
  expect(markBorder(["conflict", "omen"])).toBe("border-accent border-dashed");
  expect(markBorder(["inherited"])).toBe("border-fg-faint");
  expect(markBorder([])).toBe("border-line");
  expect(kindsOf(undefined)).toEqual([]);
});

const rollup: Rollup = { modules: 0, contracted: 0, criteria: 0, tested: 0, requirements: 0, held: 0, described: 0, decided: 0 };
function tree(ref: string, ...children: Tree[]): Tree {
  return { ref, title: ref, coverage: null, findings: 0, baselined: 0, omens: 0, checks: [], tests: 0, rollup, rites: false, children };
}

/**
 * # Test contract
 *
 * ```json
 * {"test": "outline-cut-to-trouble", "checks": ["web/src/marks#trouble-only"]}
 * ```
 */
test("the outline is cut to marked nodes and those holding one, and the marked nodes it lacks are named", () => {
  const outline = [tree("page:root", tree("page:a", tree("req:r"), tree("req:fine")), tree("page:quiet", tree("req:also-fine")), tree("module:a"))];
  const m = marksOf(lint);
  const cut = troubleOnly(outline, m);
  const refs = (ts: Tree[]): unknown[] => ts.map((t) => (t.children.length ? [t.ref, refs(t.children)] : t.ref));
  expect(refs(cut)).toEqual([["page:root", [["page:a", ["req:r"]], "module:a"]]]);
  expect(troubleOnly(outline, new Map())).toEqual([]);
  // Not in the outline: the recipes and the file, the gravest first; an edge's end alone is read on the node it is about.
  expect(outsideOutline(outline, m)).toEqual(["recipe:x", "recipe:y", "file:a.rs"]);
});
