/**
 * # Module contract
 *
 * ```json
 * {"purpose":"Name the browser tab after what a view shows, then Grimoire.","criteria":[{"id":"tab-title-names-what-is-open","statement":"tabTitle names what is open followed by ` · Grimoire`."},{"id":"tab-title-grimoire-alone-when-nothing-named","statement":"tabTitle gives `Grimoire` alone when nothing is named."},{"id":"node-name-is-title-or-id","statement":"nodeName is a node's title when its kind has one and its id otherwise."},{"id":"use-tab-title-sets-document-title","statement":"useTabTitle sets the document's title to tabTitle of what the view shows."},{"id":"root-view-has-no-name","statement":"The root view calls useTabTitle with nothing."},{"id":"unplaced-view-named-unplaced","statement":"The unplaced modules view calls useTabTitle with `Unplaced`."},{"id":"library-view-named-library","statement":"The Library calls useTabTitle with `Library`."},{"id":"page-view-named-by-its-title-as-read","statement":"A page calls useTabTitle with its title as the ticket it is read through would leave it."},{"id":"node-view-named-by-node-name","statement":"Any other node calls useTabTitle with nodeName."},{"id":"cauldron-view-named-cauldron","statement":"The cauldron calls useTabTitle with `Cauldron`."},{"id":"findings-view-named-findings","statement":"The findings view calls useTabTitle with `Findings`."}]}
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
