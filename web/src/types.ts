/**
 * # Module contract
 *
 * ```json
 * {"purpose":"The TypeScript shapes of every node, summary and tool answer, generated from the Rust answer types, with the node aliases the views use and the reference helpers.","criteria":[{"id":"mirrors-tools","statement":"Every tool answer, node and error shape the UI uses comes from web/src/generated/tools.ts, generated from the Rust answer types; nothing here restates one by hand."},{"id":"reference-helpers","statement":"kindOf and idOf split a kind:id reference without any other parsing, and localId gives what follows the last # of a criterion, test or ingredient reference, which never holds one; nothing recovers a declaration's file from its reference."},{"id":"gone-references","statement":"goneOf names a closed ticket's references whose edge get gave no title, the nodes the graph no longer holds, and names none for a ticket that is not closed or a node of any other kind."},{"id":"server-away","statement":"serverAway gives the header's warning for a machine whose intent is a copy of a server that did not answer, with when the copy was fetched when that is known, or that refused the machine's token, saying to sign in; and nothing when there is no server or it answered."},{"id":"missing-root","statement":"missingRoot says, of a root the config names, that it has no page yet and that a page of that slug makes it; with no root named, that configure names one."}]}
 * ```
 */

import type * as G from "./generated/tools";

// Every shape a tool answers with, generated from the Rust answer types.
export type * from "./generated/tools";

/** The kinds of node a reference names: the part before its first colon. */
export type Kind = "page" | "req" | "ticket" | "module" | "criterion" | "file" | "decl" | "test" | "recipe";

/** What every node view carries beside its own fields. */
type NodeBase = Pick<G.NodeView, "ref" | "in" | "out">;

/** A node view narrowed to one kind, for the views that know which they hold. */
export type AnyNode = G.NodeView;
export type PageNode = NodeBase & G.Page;
export type ReqNode = NodeBase & G.Requirement;
export type TicketNode = NodeBase & G.Ticket;
export type ModuleNode = NodeBase & G.Module;
export type CriterionNode = NodeBase & G.Criterion;
export type FileNode = NodeBase & G.File;
export type DeclNode = NodeBase & G.Decl;
export type TestNode = NodeBase & G.Test;
export type RecipeNode = NodeBase & G.Recipe;
export type IngredientNode = NodeBase & G.Ingredient;
export type DecisionNode = NodeBase & G.DecisionNode;

export function kindOf(ref: G.Ref): Kind {
  return ref.slice(0, ref.indexOf(":")) as Kind;
}

export function idOf(ref: G.Ref): string {
  return ref.slice(ref.indexOf(":") + 1);
}

/** A criterion's id, a test's name or an ingredient's id: what follows the last `#`, since a module or file name may hold one and they never do. */
export function localId(ref: G.Ref): string {
  const id = idOf(ref);
  return id.slice(id.lastIndexOf("#") + 1);
}

/** The UI route for a node's page. */
/**
 * What the header shows of the daemon's library: nothing when no grimoire is
 * named, the name alone when the library holds one, and a list of them when
 * it holds more.
 */
export function libraryShown(grimoire: string | null, library: { name: string }[]): "none" | "name" | "list" {
  if (!grimoire) return "none";
  return library.length < 2 ? "name" : "list";
}

/**
 * The grimoires a library's switcher offers, one for each checkout: the label names the
 * checkout's folder when the grimoire is served for several, the address opens it as
 * /g/<name>/?checkout=<checkout>, and the one on screen is chosen.
 */
export function libraryChoices(grimoire: string | null, checkout: string | null, library: { name: string; repository: string; server?: string | null }[]): { key: string; label: string; href: string; chosen: boolean; server: string | null }[] {
  const several = (name: string) => library.filter((g) => g.name === name).length > 1;
  const folder = (path: string) => path.replace(/[\\/]+$/, "").split(/[\\/]/).pop() ?? path;
  return library.map((g) => ({
    key: `${g.name}\n${g.repository}`,
    label: several(g.name) ? `${g.name} · ${folder(g.repository)}` : g.name,
    href: `/g/${g.name}/?checkout=${encodeURIComponent(g.repository)}`,
    chosen: g.name === grimoire && (checkout === null || g.repository === checkout),
    server: g.server ?? null,
  }));
}

/**
 * The switcher's choices grouped by where each grimoire comes from: this machine's
 * stores first, then each team's server in address order, keyed by its address and
 * labelled by it without the scheme, unless another differs from it only by that.
 * Choices that all come from one place make one group with no label, since a lone
 * heading says nothing.
 */
export function libraryGroups<C extends { server: string | null }>(choices: C[]): { key: string; label: string | null; choices: C[] }[] {
  const places = [...new Set(choices.map((c) => c.server))].sort((a, b) => (a === null ? -1 : b === null ? 1 : a.localeCompare(b)));
  if (places.length < 2) return [{ key: "", label: null, choices }];
  const bare = (place: string) => place.replace(/^[a-z][a-z0-9+.-]*:\/\//i, "");
  // Two servers that differ only by scheme keep it, so no two headings read alike.
  const label = (place: string) => (places.some((p) => p !== null && p !== place && bare(p) === bare(place)) ? place : bare(place));
  return places.map((place) => ({ key: place ?? "", label: place === null ? "this machine" : label(place), choices: choices.filter((c) => c.server === place) }));
}

/**
 * What the grimoire view says when it has no grimoire root to show: the root
 * the config names, when one is, has no page yet; else no root is named.
 */
export function missingRoot(named: string | null | undefined): string {
  if (named) return `The grimoire root \`${named}\` has no page yet: put a page \`${named}\` under a ticket.`;
  return "No grimoire root is named. The check says how to name one with configure.";
}

export function nodePath(ref: G.Ref): string {
  return `/n/${encodeURIComponent(ref)}`;
}

/**
 * A closed ticket's references that lead nowhere: get gives an edge no title
 * when the graph no longer holds its other end. The check leaves these alone,
 * so where the ticket is read is where a reader is told. Those of a ticket
 * that is not closed are dangling findings, and may be nodes its work has yet
 * to create.
 */
export function goneOf(node: AnyNode): Set<G.Ref> {
  if (!("status" in node) || node.status !== "closed") return new Set();
  return new Set(node.out.filter((e) => e.title === null).map((e) => e.ref));
}

/**
 * What the header says when this machine's intent is a copy of a team's
 * server's and the server did not answer: the copy is read as it was last
 * fetched, and a write waits. Null when there is no server or it answered.
 */
export function serverAway(server: G.ServerCopy | null | undefined): string | null {
  if (!server || server.reachable) return null;
  if (server.refused) return "server refused this machine's token: run grimoire login";
  if (!server.fetched) return "server unreachable";
  return `server unreachable, intent as of ${server.fetched.replace("T", " ").replace(/(:\d\d)?Z$/, " UTC")}`;
}
