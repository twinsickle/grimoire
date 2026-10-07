/**
 * # Module contract
 *
 * ```json
 * {"purpose":"Sort a requirement's decisions into those in force and those a later decision superseded, by id.","criteria":[{"id":"in-force","statement":"Each decision's id is the one it carries, its position from 1 where it carries none, and none where it carries null, a decision waiting in a ticket's proposal; those no decision supersedes by id are in force, listed newest first, a waiting one among them, and each superseded one is listed newest first with the id of the latest decision that superseded it, or none when that one is waiting."},{"id":"described","statement":"isDescribed is true when a requirement has decisions in force and every one of them was recorded as an adoption, and false with no decision or with any decision in force that is not one."}]}
 * ```
 */

import type { Decision } from "./types";

/** A decision's id: the one it carries, or its position where it carries none, as a store written before ids holds it; null while it waits in a ticket's proposal. */
export function decisionId(d: Decision, position: number): number | null {
  return d.id === undefined ? position : d.id;
}

/** How a decision's id is said: d3; a decision waiting in a ticket has none yet. */
export function saidId(id: number | null): string {
  return id === null ? "waiting for its ticket to land" : `d${id}`;
}

/** A requirement's decisions by id: those in force newest first, and those a later one superseded, each with the id of what superseded it. */
export function decisionsInForce(decisions: Decision[]): { inForce: { id: number | null; d: Decision }[]; superseded: { id: number; d: Decision; by: number | null }[] } {
  const numbered = decisions.map((d, i) => ({ id: decisionId(d, i + 1), d }));
  // The latest decision naming each id, in the order they landed.
  const by = new Map<number, number | null>();
  for (const { id, d } of numbered) for (const s of d.supersedes ?? []) by.set(s, id);
  const newest = [...numbered].reverse();
  return {
    inForce: newest.filter(({ id }) => id === null || !by.has(id)),
    superseded: newest.flatMap(({ id, d }) => (id !== null && by.has(id) ? [{ id, d, by: by.get(id) ?? null }] : [])),
  };
}

/** Described, not decided: decisions in force, every one recorded as an adoption. */
export function isDescribed(decisions: Decision[]): boolean {
  const { inForce } = decisionsInForce(decisions);
  return inForce.length > 0 && inForce.every(({ d }) => d.adopted === true);
}
