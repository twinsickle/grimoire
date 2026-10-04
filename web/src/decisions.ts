/**
 * # Module contract
 *
 * ```json
 * {"purpose":"Sort a requirement's decisions into those in force and those a later decision superseded, by position.","criteria":[{"id":"in-force","statement":"Decisions are numbered by position from 1, the oldest first; those no decision supersedes are in force, listed newest first, and each superseded one is listed newest first with the latest decision that superseded it."},{"id":"described","statement":"isDescribed is true when a requirement has decisions in force and every one of them was recorded as an adoption, and false with no decision or with any decision in force that is not one."}]}
 * ```
 */

import type { Decision } from "./types";

/** A requirement's decisions by position from 1: those in force newest first, and those a later one superseded, each with what superseded it. */
export function decisionsInForce(decisions: Decision[]): { inForce: { n: number; d: Decision }[]; superseded: { n: number; d: Decision; by: number }[] } {
  const by = new Map<number, number>();
  decisions.forEach((d, i) => (d.supersedes ?? []).forEach((p) => by.set(p, Math.max(by.get(p) ?? 0, i + 1))));
  const numbered = decisions.map((d, i) => ({ n: i + 1, d })).reverse();
  return {
    inForce: numbered.filter(({ n }) => !by.has(n)),
    superseded: numbered.filter(({ n }) => by.has(n)).map(({ n, d }) => ({ n, d, by: by.get(n) ?? 0 })),
  };
}

/** Described, not decided: decisions in force, every one recorded as an adoption. */
export function isDescribed(decisions: Decision[]): boolean {
  const { inForce } = decisionsInForce(decisions);
  return inForce.length > 0 && inForce.every(({ d }) => d.adopted === true);
}

