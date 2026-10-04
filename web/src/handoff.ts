/**
 * # Module contract
 *
 * ```json
 * {"purpose":"Say what a person hands their agent to take a ticket's next step, since the UI starts and closes nothing itself.","criteria":[{"id":"handoff","statement":"handoff gives the line for a ticket's next step, naming it by id and quoted title and the grimoire skill: to start an open change, to continue an active one, to land a landing change once its change is on the main line, and to resolve an open or active gap or question by laying out the options for the person to choose from, naming its kind; and nothing for a gap or question landing, which closes when its change lands, nor for a closed ticket."}]}
 * ```
 */

import { idOf, type Ref, type Status, type TicketKind } from "./types";

/** The line a person hands their agent to take this ticket's next step, or null when it has none of its own. */
export function handoff(t: { ref: Ref; title: string; kind: TicketKind; status: Status }): string | null {
  const named = `ticket ${idOf(t.ref)} ("${t.title}")`;
  if (t.status === "closed") return null;
  if (t.kind !== "change") {
    // An answer waiting for its change closes when that change lands: nothing is asked of anyone.
    if (t.status === "landing") return null;
    return `Resolve the ${t.kind} ${idOf(t.ref)} ("${t.title}") with the grimoire skill: lay out the options for me to choose from.`;
  }
  if (t.status === "open") return `Start ${named} with the grimoire skill.`;
  if (t.status === "active") return `Continue ${named} with the grimoire skill.`;
  return `${named[0].toUpperCase()}${named.slice(1)} is waiting to land: land it with the grimoire skill once its change is on the main line.`;
}
