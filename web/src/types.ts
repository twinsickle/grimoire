/**
 * # Module contract
 *
 * ```json
 * {"purpose":"The TypeScript shapes of every node, summary and tool answer, generated from the Rust answer types, with the node aliases the views use and the reference helpers.","criteria":[{"id":"mirrors-tools","statement":"Every tool answer, node and error shape the UI uses comes from web/src/generated/tools.ts, generated from the Rust answer types; nothing here restates one by hand."},{"id":"reference-helpers","statement":"kindOf and idOf split a kind:id reference without any other parsing, and localId gives what follows the last # of a criterion or test reference, which never holds one; nothing recovers a declaration's file from its reference."},{"id":"gone-references","statement":"goneOf names a closed ticket's references whose edge get gave no title, the nodes the graph no longer holds, and names none for a ticket that is not closed or a node of any other kind."}]}
 * ```
 */

import type * as G from "./generated/tools";

// Every shape a tool answers with, generated from the Rust answer types.
export type * from "./generated/tools";

/** The kinds of node a reference names: the part before its first colon. */
export type Kind = "page" | "req" | "ticket" | "module" | "criterion" | "file" | "decl" | "test";

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

export function kindOf(ref: G.Ref): Kind {
  return ref.slice(0, ref.indexOf(":")) as Kind;
}

export function idOf(ref: G.Ref): string {
  return ref.slice(ref.indexOf(":") + 1);
}

/** A criterion's id or a test's name: what follows the last `#`, since a module or file name may hold one and they never do. */
export function localId(ref: G.Ref): string {
  const id = idOf(ref);
  return id.slice(id.lastIndexOf("#") + 1);
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
