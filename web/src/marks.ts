/**
 * # Module contract
 *
 * ```json
 * {"purpose":"Sort the lint answer into marks on the nodes they are about, so every view reads trouble the same way: new, inherited, recipe due, may conflict and omen.","criteria":[{"id":"marks-of","statement":"marksOf gives each node its marks from a lint answer: a finding with the check due-recipe that is not baselined is due, any other finding new when not baselined and inherited when baselined, each advisory may conflict and each omen an omen; a finding and an advisory mark the node they are about and, when they name an edge, its end as well, once each; an omen marks the node it is on."},{"id":"gravest-first","statement":"A node's marks are kept gravest first, in the order new, due, may conflict, inherited, omen, keeping the lint answer's order within a kind, so the first mark gives a marked node its colour: markBorder gives a box's border for its gravest kind in that kind's colour, the bad colour for new, the recipe colour for due, the accent dashed for may conflict, the faint colour for inherited and the dim one for an omen, and the line colour for a node with none."},{"id":"trouble-only","statement":"troubleOnly keeps of an outline the nodes that are marked or hold a marked node beneath them, each with only such children, in the outline's order; outsideOutline gives, in the order of their gravest mark, the nodes an outline does not hold that a finding, advisory or omen is about, not those only named as an edge's end, which may not exist."}]}
 * ```
 */

import type { Advisory, Finding, LintAnswer, Omen, OtherWork, Ref, Tree } from "./types";

/** The kinds of mark, gravest first. */
export const MARK_KINDS = ["new", "due", "conflict", "inherited", "omen"] as const;
export type MarkKind = (typeof MARK_KINDS)[number];

/** What a mark is called where a person reads it. */
export const MARK_NAME: Record<MarkKind, string> = {
  new: "new",
  due: "recipe due",
  conflict: "may conflict",
  inherited: "inherited",
  omen: "omen",
};

/** One piece of trouble on a node: its kind, the check that found it, why, and for other work, whose. */
export type Mark = { kind: MarkKind; check: string; reason: string; ref: Ref; from?: OtherWork };

/** Each node's marks, gravest first. */
export type Marks = Map<Ref, Mark[]>;

function kindOfFinding(f: Finding): MarkKind {
  if (f.baselined) return "inherited";
  return f.check === "due-recipe" ? "due" : "new";
}

/** The nodes a finding is about: its own, and the end of the edge it names. */
function aboutOf(f: Finding): Ref[] {
  return f.edge && f.edge.to !== f.ref ? [f.ref, f.edge.to] : [f.ref];
}

/** Every node's marks from one lint answer (findings, advisories and omens alone are read). */
export function marksOf(lint: Pick<LintAnswer, "findings" | "advisories" | "omens">): Marks {
  const out: Marks = new Map();
  const add = (on: Ref, m: Mark) => out.set(on, [...(out.get(on) ?? []), m]);
  const finding = (f: Finding, kind: MarkKind, from?: OtherWork) => {
    for (const on of aboutOf(f)) add(on, { kind, check: f.check, reason: f.reason, ref: f.ref, ...(from ? { from } : {}) });
  };
  lint.findings.forEach((f) => finding(f, kindOfFinding(f)));
  lint.advisories.forEach((a: Advisory) => finding(a.finding, "conflict", a.from));
  lint.omens.forEach((o: Omen) => add(o.ref, { kind: "omen", check: o.check, reason: o.reason, ref: o.ref }));
  for (const [on, ms] of out) out.set(on, gravestFirst(ms));
  return out;
}

/** Marks in the order new, due, may conflict, inherited, omen; a stable sort keeps the answer's order within a kind. */
export function gravestFirst(ms: Mark[]): Mark[] {
  return [...ms].sort((a, b) => MARK_KINDS.indexOf(a.kind) - MARK_KINDS.indexOf(b.kind));
}

/** The kinds a node's marks hold, gravest first, each once. */
export function kindsOf(ms: Mark[] | undefined): MarkKind[] {
  return MARK_KINDS.filter((k) => ms?.some((m) => m.kind === k));
}

/** A box's border for its gravest mark, in the mark's colour; written out so Tailwind sees every class. */
const BORDER: Record<MarkKind, string> = {
  new: "border-bad",
  due: "border-recipe",
  conflict: "border-accent border-dashed",
  inherited: "border-fg-faint",
  omen: "border-fg-dim",
};

/** The border of a box whose marks are of these kinds, gravest first; with none, the plain line. */
export function markBorder(kinds: MarkKind[]): string {
  return kinds.length ? BORDER[kinds[0]] : "border-line";
}

/** An outline cut to where there is trouble: marked nodes and the nodes that hold one. */
export function troubleOnly(trees: Tree[], marks: Marks): Tree[] {
  const out: Tree[] = [];
  for (const t of trees) {
    const children = troubleOnly(t.children, marks);
    if (children.length > 0 || (marks.get(t.ref)?.length ?? 0) > 0) out.push({ ...t, children });
  }
  return out;
}

/** The nodes trouble is about that an outline does not hold, such as tickets, files and tests, in the order of their gravest mark. */
export function outsideOutline(trees: Tree[], marks: Marks): Ref[] {
  const held = new Set<Ref>();
  const walk = (ts: Tree[]) =>
    ts.forEach((t) => {
      held.add(t.ref);
      walk(t.children);
    });
  walk(trees);
  // A node only named as an edge's end may be one that does not exist, such as a dangling edge's: it is read on the node the finding is about.
  const rest = [...marks.entries()].filter(([r, ms]) => !held.has(r) && ms.some((m) => m.ref === r)).map(([r]) => r);
  const rank = (r: Ref) => MARK_KINDS.indexOf(marks.get(r)?.[0]?.kind ?? "omen");
  return rest.sort((a, b) => rank(a) - rank(b));
}
