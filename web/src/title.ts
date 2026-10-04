/**
 * # Module contract
 *
 * ```json
 * {"purpose":"Name the browser tab after what a view shows, then Grimoire.","criteria":[{"id":"tab-title","statement":"tabTitle names what is open followed by ` · Grimoire`, and gives `Grimoire` alone when nothing is named; nodeName is a node's title when its kind has one and its id otherwise; useTabTitle sets the document's title to tabTitle of what the view shows, and each view calls it: the root with nothing, the unplaced modules with `Unplaced`, a page with its title as the ticket it is read through would leave it, any other node with nodeName, the cauldron with `Cauldron`."}]}
 * ```
 */

import { useEffect } from "react";
import { idOf, type AnyNode } from "./types";

/** The browser tab's title: what is open, then Grimoire; Grimoire alone when nothing is named, as at the root. */
export function tabTitle(subject?: string | null): string {
  return subject ? `${subject} · Grimoire` : "Grimoire";
}

/** What a node is called in a tab: its title when its kind has one, else its id. */
export function nodeName(node: AnyNode): string {
  return "title" in node && node.title ? node.title : idOf(node.ref);
}

/** Name the browser tab after what this view shows, for as long as it shows it. */
export function useTabTitle(subject?: string | null): void {
  useEffect(() => {
    document.title = tabTitle(subject);
  }, [subject]);
}
