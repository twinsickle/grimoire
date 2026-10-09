/**
 * # Module contract
 *
 * ```json
 * {"purpose":"Say what a person hands their agent to take a ticket's next step, since the UI starts and closes nothing itself.","criteria":[{"id":"brews-inscribed-change","statement":"handoff gives the line for an inscribed change's next step, to brew it, naming it by id and quoted title and the grimoire skill."},{"id":"continues-brewing-change","statement":"handoff gives the line for a brewing change's next step, to continue it, naming it by id and quoted title and the grimoire skill."},{"id":"bottles-simmering-change","statement":"handoff gives the line for a simmering change's next step, to bottle it once its change is on the main line, naming it by id and quoted title and the grimoire skill."},{"id":"resolves-gap-or-question","statement":"handoff gives the line for an inscribed or brewing gap or question's next step, to resolve it by laying out the options for the person to choose from, naming it by id and quoted title and its kind and the grimoire skill."},{"id":"nothing-for-answer-waiting","statement":"handoff gives nothing for a gap or question simmering, which is bottled when its change is."},{"id":"nothing-for-bottled","statement":"handoff gives nothing for a bottled ticket."}]}
 * ```
 */

import { idOf, type Ref, type Status, type TicketKind } from "./types";

/** The line a person hands their agent to take this ticket's next step, or null when it has none of its own. */
export function handoff(t: { ref: Ref; title: string; kind: TicketKind; status: Status }): string | null {
  const named = `ticket ${idOf(t.ref)} ("${t.title}")`;
  if (t.status === "bottled") return null;
  if (t.kind !== "change") {
    // An answer waiting for its change is bottled when that change is: nothing is asked of anyone.
    if (t.status === "simmering") return null;
    return `Resolve the ${t.kind} ${idOf(t.ref)} ("${t.title}") with the grimoire skill: lay out the options for me to choose from.`;
  }
  if (t.status === "inscribed") return `Brew ${named} with the grimoire skill.`;
  if (t.status === "brewing") return `Continue ${named} with the grimoire skill.`;
  return `${named[0].toUpperCase()}${named.slice(1)} is simmering: bottle it with the grimoire skill once its change is on the main line.`;
}
