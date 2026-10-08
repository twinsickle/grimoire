/**
 * # Module contract
 *
 * ```json
 * {"purpose":"The TypeScript shapes of every node, summary and tool answer, generated from the Rust answer types, with the node aliases the views use and the reference helpers.","criteria":[{"id":"mirrors-tools","statement":"Every tool answer, node and error shape the UI uses comes from web/src/generated/tools.ts, generated from the Rust answer types; nothing here restates one by hand."},{"id":"reference-helpers","statement":"kindOf and idOf split a kind:id reference without any other parsing, and localId gives what follows the last # of a criterion, test or ingredient reference, which never holds one; nothing recovers a declaration's file from its reference."},{"id":"gone-references","statement":"goneOf names a closed ticket's references whose edge get gave no title, the nodes the graph no longer holds, and names none for a ticket that is not closed or a node of any other kind."},{"id":"server-away","statement":"serverAway gives the header's warning for a machine whose intent is a copy of a server that did not answer, with when the copy was fetched when that is known, or that refused the machine's token, saying to sign in; and nothing when there is no server or it answered."},{"id":"checked-out-words","statement":"checkedOutWords says who has a ticket checked out as `checked out by alex · since 14:10`, each holder in turn joined by and, the time local and only the hour and minute when it is today, the date before it otherwise, and the text as given when it is not a time; with nobody listed, `checked out by someone unknown`."},{"id":"missing-root","statement":"missingRoot says, of a root the config names, that it has no page yet and that a page of that slug makes it; with no root named, that configure names one."}]}
 * ```
 */

import type * as G from "./generated/tools";

// Every shape a tool answers with, generated from the Rust answer types.
export type * from "./generated/tools";

/** Who has a ticket checked out, in words: `checked out by alex · since 14:10`, the time local, with its date unless it is today (req:ticket-checked-out). */
export function checkedOutWords(list: G.CheckedOut[], now: Date = new Date()): string {
  if (list.length === 0) return "checked out by someone unknown";
  const two = (n: number) => String(n).padStart(2, "0");
  const when = (since: string) => {
    const t = new Date(since);
    if (Number.isNaN(t.getTime())) return since;
    const hour = `${two(t.getHours())}:${two(t.getMinutes())}`;
    const today = t.getFullYear() === now.getFullYear() && t.getMonth() === now.getMonth() && t.getDate() === now.getDate();
    return today ? hour : `${t.getFullYear()}-${two(t.getMonth() + 1)}-${two(t.getDate())} ${hour}`;
  };
  return `checked out by ${list.map((c) => `${c.by} · since ${when(c.since)}`).join(" and ")}`;
}

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

/** The last part of a checkout's path, which names it in the header. */
export function folderOf(path: string): string {
  return path.replace(/[\\/]+$/, "").split(/[\\/]/).pop() || path;
}

/** A checkout of the grimoire on screen, as the header lists it. */
export type Checkout = { key: string; folder: string; href: string; here: boolean; current: G.CurrentTicket | null };

/**
 * The checkouts the header lists: those status lists of the grimoire on screen and
 * of no other, each named by its folder and opening at /g/<name>/?checkout=<checkout>,
 * the one on screen marked and given its current ticket as status answers it; with
 * no library, the one checkout on screen.
 */
export function checkoutsOf(status: Pick<G.StatusAnswer, "grimoire" | "repository" | "current" | "library">): Checkout[] {
  const { grimoire, repository, current } = status;
  const mine = grimoire ? status.library.filter((g) => g.name === grimoire) : [];
  if (!mine.some((g) => g.repository === repository)) return [{ key: repository, folder: folderOf(repository), href: "", here: true, current }, ...checkouts(mine, repository)];
  return checkouts(mine, repository).map((c) => (c.here ? { ...c, current } : c));
}

function checkouts(library: G.LibraryEntry[], repository: string): Checkout[] {
  return library.map((g) => ({ key: g.repository, folder: folderOf(g.repository), href: `/g/${g.name}/?checkout=${encodeURIComponent(g.repository)}`, here: g.repository === repository, current: g.current }));
}

/** A grimoire on the Library's shelf: its name, where it opens, and how many checkouts serve it. */
export type Book = { name: string; href: string; checkouts: number };

/** A stack of books on the Library's shelf: the grimoires of one place, this machine's stores or one team's server. */
export type Stack = { key: string; server: string | null; label: string; books: Book[] };

/**
 * The Library's stacks: the grimoires status lists, one book per grimoire however many
 * checkouts serve it, in name order, stacked by where each comes from, this machine first,
 * then each server in address order, labelled by its address without the scheme unless
 * another differs from it only by that.
 */
export function libraryStacks(status: Pick<G.StatusAnswer, "library">): Stack[] {
  const places = [...new Set(status.library.map((g) => g.server))].sort((a, b) => (a === null ? -1 : b === null ? 1 : a.localeCompare(b)));
  // Two servers that differ only by scheme keep it, so no two labels read alike.
  const label = (place: string) => (places.some((p) => p !== null && p !== place && bareAddress(p) === bareAddress(place)) ? place : bareAddress(place));
  return places.map((place) => {
    const here = status.library.filter((g) => g.server === place);
    const names = [...new Set(here.map((g) => g.name))].sort((a, b) => a.localeCompare(b));
    return {
      key: place ?? "",
      server: place,
      label: place === null ? "this machine" : label(place),
      books: names.map((name) => ({ name, href: `/g/${name}/`, checkouts: here.filter((g) => g.name === name).length })),
    };
  });
}

/** A server's address without its scheme, as a stack's label shows it. */
function bareAddress(address: string): string {
  return address.replace(/^[a-z][a-z0-9+.-]*:\/\//i, "");
}

/**
 * Where the grimoire on screen is kept, in words, and whether that is trouble: the
 * server's warning when it did not answer, that it answers when it did, its
 * address left to the line's title, and this machine for a store of its own.
 */
export function serverState(server: G.ServerCopy | null | undefined): { words: string; bad: boolean } {
  const away = serverAway(server);
  if (away) return { words: away, bad: true };
  if (server) return { words: "team server answering", bad: false };
  return { words: "this machine", bad: false };
}

/**
 * What the grimoire view says when it has no grimoire root to show: the root
 * the config names, when one is, has no page yet; else no root is named.
 */
export function missingRoot(named: string | null | undefined): string {
  if (named) return `The grimoire root \`${named}\` has no page yet: put a page \`${named}\` under a ticket.`;
  return "No grimoire root is named. The check says how to name one with configure.";
}

/** The UI route for a node's page. */
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
