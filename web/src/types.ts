/**
 * # Module contract
 *
 * ```json
 * {"purpose":"The TypeScript shapes of every node, summary and tool answer, plus the reference helpers.","criteria":[{"id":"mirrors-tools","statement":"Each type matches the shape the corresponding tool returns; hand-written until generated from the Rust model."},{"id":"reference-helpers","statement":"kindOf and idOf split a kind:id reference without any other parsing."}]}
 * ```
 */

export type Kind = "page" | "req" | "ticket" | "module" | "criterion" | "file" | "decl";
export type Ref = string; // `${Kind}:${id}`

export type Relation = "links" | "covered_by" | "touches" | "relies_on" | "contains" | "imports" | "belongs_to";

export interface EdgeView {
  ref: Ref;
  relation: Relation;
  /** The other end's one-line name, or null when it is dangling. */
  title: string | null;
}

export interface Decision {
  date: string;
  text: string;
}

export interface Scenario {
  title: string;
  given: string;
  when: string;
  then: string;
}

export type TicketKind = "change" | "gap" | "question";
export type Priority = "next" | "high" | "later";
export type Status = "open" | "active" | "closed";

interface NodeBase {
  ref: Ref;
  in: EdgeView[];
  out: EdgeView[];
}

export interface PageNode extends NodeBase {
  slug: string;
  title: string;
  body: string;
  links: Ref[];
}

export interface ReqNode extends NodeBase {
  slug: string;
  title: string;
  statement: string;
  scenarios: Scenario[];
  covered_by: Ref[];
  decisions: Decision[];
}

export interface TicketNode extends NodeBase {
  slug: string;
  title: string;
  kind: TicketKind;
  priority: Priority;
  status: Status;
  touches: Ref[];
  relies_on: Ref[];
  body: string;
  decision: Decision | null;
}

export interface Criterion {
  module: string;
  id: string;
  statement: string;
}

export interface ModuleNode extends NodeBase {
  id: string;
  files: string[];
  purpose: string | null;
  criteria: Criterion[];
  imports: string[];
  expects_contract: boolean;
  contract_error: string | null;
  contract_at: [string, number] | null;
}

export interface CriterionNode extends NodeBase, Criterion {}

export interface Decl {
  path: string;
  kind: string;
  line: number;
}

export interface FileNode extends NodeBase {
  path: string;
  module: string;
  language: string;
  decls: Decl[];
}

export interface DeclNode extends NodeBase, Decl {}

export type AnyNode = PageNode | ReqNode | TicketNode | ModuleNode | CriterionNode | FileNode | DeclNode;

export interface Summary {
  ref: Ref;
  title: string;
  kind?: TicketKind;
  priority?: Priority;
  status?: Status;
}

export interface Tree {
  ref: Ref;
  title: string;
  coverage: number | null;
  findings: number;
  children: Tree[];
}

export interface RestsOn {
  ref: Ref;
  title: string;
  via: Ref[];
  decisions: Decision[];
}

export interface Affected {
  ref: Ref;
  title: string;
  relation: Relation;
}

export interface RestsOnMe {
  nodes: Affected[];
  tickets: Summary[];
}

export type Rule = "dangling" | "uncovered" | "malformed" | "unspecified";

export interface Finding {
  rule: Rule;
  ref: Ref;
  edge?: { from: Ref; to: Ref; relation: Relation };
  reason: string;
}

export interface LintResult {
  findings: Finding[];
  counts: Record<Rule, number>;
}

export interface SourceLocation {
  file: string;
  line: number;
  contract_line?: number;
}

export interface CriterionCoverage {
  ref: Ref;
  statement: string;
  requirements: Summary[];
}

export interface StatusResult {
  repository: string;
  roots: string[];
  languages: string[];
  /** The running build's crate version. */
  version: string;
  /** The languages the running build can extract; compare with `languages`. */
  supported_languages: string[];
  nodes: Record<Kind, number>;
  edges: number;
  unresolved_imports: number;
  malformed: number;
}

export interface ToolError {
  error: "not_found" | "invalid" | "conflict" | "store_unavailable";
  message: string;
  details?: unknown;
}

export function kindOf(ref: Ref): Kind {
  return ref.slice(0, ref.indexOf(":")) as Kind;
}

export function idOf(ref: Ref): string {
  return ref.slice(ref.indexOf(":") + 1);
}

/** The UI route for a node's page. */
export function nodePath(ref: Ref): string {
  return `/n/${encodeURIComponent(ref)}`;
}
