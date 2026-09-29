// Generated from the Rust answer types by crates/core/src/answer.rs.
// Do not edit: `cargo test -p grimoire-core --test typescript -- --ignored` rewrites it.

export type Adopted = { baseline: BaselineSummary, replaced: number | null, };

export type Affected = { ref: Ref, title: string, relation: Relation, };

export type Annotation = { line: number, ref: Ref, kind: AnnotationKind, title: string, 
/**
 * For a contract, its criteria; for a test block, what it checks.
 */
links: Array<Linked>, };

export type AnnotationKind = "contract" | "test" | "decl";

export type BaselineSummary = { date: string, reason: string, count: number, };

export type Board = { tickets: Array<BoardTicket>, unattached: Array<Summary>, closed: Array<BoardTicket>, };

export type BoardTicket = { waits_on?: Array<Summary>, followed_by?: Array<Summary>, ref: Ref, title: string, kind?: TicketKind, priority?: Priority, status?: Status, };

export type Closed = { node: NodeView, findings: Array<Finding>, baselined: number, untouched: Array<Ref> | null, };

export type Configured = { config: StoreConfig, new: number, };

export type CoverageAnswer = { requirements: Array<Summary>, tests: Array<Summary>, } | { criteria: Array<CriterionCoverage>, } | { criteria: Array<CoveringCriterion>, };

export type CoveringCriterion = { ref: Ref, statement: string | null, module: ModuleSummary, };

export type Criterion = { module: string, id: string, statement: string, };

export type CriterionCoverage = { ref: Ref, statement: string, requirements: Array<Summary>, tests: Array<Summary>, };

export type Decision = { date: string, text: string, };

export type Decl = { path: string, kind: string, line: number, };

export type Deleted = { deleted: Ref, warnings: Array<Finding>, baselined: number, };

export type EdgeView = { ref: Ref, relation: Relation, title: string | null, };

export type FieldError = { field: string, reason: string, };

export type File = { path: string, module: string, language: string, decls: Array<Decl>, 
/**
 * The fingerprint of the file's text as read.
 */
digest: string, };

export type Finding = { rule: Rule, 
/**
 * Which test within the rule produced it, stable across wording
 * changes: one node can fail two checks of the same rule.
 */
check: string, 
/**
 * The node the finding is about. For a malformed store file, a
 * reference of the kind the file would have held, with the file's slug.
 */
ref: Ref, edge?: FindingEdge, reason: string, 
/**
 * Whether the store's baseline holds this finding: inherited, not new.
 */
baselined: boolean, };

export type FindingEdge = { from: Ref, to: Ref, relation: Relation, };

export type Linked = { ref: Ref, title: string | null, };

export type LintAnswer = { findings: Array<Finding>, counts: { [key in string]: number }, new: number, };

export type Module = { id: string, files: Array<string>, purpose: string | null, criteria: Array<Criterion>, imports: Array<string>, 
/**
 * Whether the adapter expects this module to carry a contract.
 */
expects_contract: boolean, 
/**
 * Why the contract is missing or malformed, when `purpose` is `None`
 * and one was expected.
 */
contract_error: string | null, 
/**
 * The file and line of the `# Module contract` heading.
 */
contract_at: [string, number] | null, };

export type ModuleSummary = { ref: Ref, purpose: string | null, };

export type NodeAnswer = { node: NodeView, };

export type NodeView = { ref: Ref, in: Array<EdgeView>, out: Array<EdgeView>, } & (Page | Requirement | Ticket | Module | Criterion | File | Decl | Test);

export type NodesAnswer = { nodes: Array<Summary>, };

export type Order = { next?: Array<string>, high?: Array<string>, later?: Array<string>, };

export type Outline = { roots: Array<Tree>, unplaced: Array<Tree>, };

export type Page = { slug: string, title: string, body: string, links: Array<Ref>, };

export type Priority = "next" | "high" | "later";

export type Ref = string;

export type Relation = "links" | "covered_by" | "touches" | "relies_on" | "contains" | "imports" | "belongs_to" | "checks";

export type Renamed = { node: NodeView, still_named_in: Array<StillNamed>, };

export type Reordered = { priority: Priority, order: Array<Ref>, };

export type Requirement = { slug: string, title: string, statement: string, scenarios: Array<Scenario>, covered_by: Array<Ref>, decisions: Array<Decision>, };

export type RestsOn = { ref: Ref, title: string, via: Array<Ref>, decisions: Array<Decision>, };

export type RestsOnAnswer = { nodes: Array<RestsOn>, };

export type RestsOnMe = { nodes: Array<Affected>, tickets: Array<Summary>, };

export type Rollup = { modules: number, contracted: number, criteria: number, tested: number, requirements: number, held: number, };

export type Rule = "dangling" | "uncovered" | "malformed" | "unspecified";

export type Scenario = { title: string, given: string, when: string, then: string, };

export type Scope = { include: Array<string>, exclude: Array<string>, };

export type SourceAnswer = { file: Ref, language: string, text: string, focus: number, annotations: Array<Annotation>, };

export type SourceLocation = { file: string, line: number, contract_line?: number, };

export type Started = { node: NodeView, rests_on: Array<RestsOn>, related: Array<Summary>, };

export type Status = "open" | "active" | "closed";

export type StatusAnswer = { repository: string, roots: Array<string>, languages: Array<string>, version: string, supported_languages: Array<string>, 
/**
 * How many nodes of each kind.
 */
nodes: { [key in string]: number }, edges: number, unresolved_imports: number, malformed: number, scope: Scope, baseline: BaselineSummary | null, };

export type StillNamed = { test: Ref, file: string, line: number, };

export type StoreConfig = { version: number, repository: string, roots: Array<string>, languages: Array<string>, scope?: Scope, order?: Order, };

export type Summary = { ref: Ref, title: string, kind?: TicketKind, priority?: Priority, status?: Status, };

export type Test = { file: string, name: string, line: number, checks: Array<string>, };

export type Ticket = { slug: string, title: string, kind: TicketKind, priority: Priority, status: Status, touches: Array<Ref>, relies_on: Array<Ref>, body: string, decision: Decision | null, };

export type TicketKind = "change" | "gap" | "question";

export type TicketsAnswer = { tickets: Array<Summary>, };

export type ToolError = { "error": "not_found", message: string, } | { "error": "invalid", message: string, details: Array<FieldError>, } | { "error": "conflict", message: string, } | { "error": "store_unavailable", message: string, };

export type Tree = { ref: Ref, title: string, 
/**
 * For a page, the fraction of criteria under it some requirement names;
 * for a requirement, its criterion count; null for anything else.
 */
coverage: number | null, findings: number, 
/**
 * How many of `findings` are in the baseline: inherited, not new.
 */
baselined: number, 
/**
 * The checks of the findings on this node itself, sorted and distinct,
 * so a view can say what is wrong and not only how much.
 */
checks: Array<string>, 
/**
 * How many tests check this node: a criterion's evidence, or what holds
 * a constraint that names no criterion.
 */
tests: number, rollup: Rollup, children: Array<Tree>, };

export type Written = { node: NodeView, warnings: Array<Finding>, baselined: number, };

/** Each tool's answer, by tool name. */
export type ToolAnswers = {
  find: NodesAnswer;
  get: NodeAnswer;
  record_decision: NodeAnswer;
  outline: Outline;
  rests_on: RestsOnAnswer;
  rests_on_me: RestsOnMe;
  coverage: CoverageAnswer;
  board: Board;
  related_tickets: TicketsAnswer;
  lint: LintAnswer;
  module_source: SourceLocation;
  source: SourceAnswer;
  status: StatusAnswer;
  put_page: Written;
  patch_page: Written;
  put_requirement: Written;
  file_ticket: Written;
  update_ticket: Written;
  start_ticket: Started;
  close_ticket: Closed;
  rename_node: Renamed;
  reorder: Reordered;
  configure: Configured;
  adopt: Adopted;
  delete_node: Deleted;
};
