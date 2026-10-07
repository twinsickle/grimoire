// Generated from the Rust answer types by crates/core/src/answer.rs.
// Do not edit: `cargo test -p grimoire-core --test typescript -- --ignored` rewrites it.

export type Adopted = { baseline: BaselineSummary, replaced: number | null, };

export type Advisory = { from: OtherWork, finding: Finding, };

export type Affected = { ref: Ref, title: string, relation: Relation, };

export type Annotation = { line: number, ref: Ref, kind: AnnotationKind, title: string, 
/**
 * For a contract, its criteria; for a test block, what it checks.
 */
links: Array<Linked>, };

export type AnnotationKind = "contract" | "test" | "decl";

export type BaselineSummary = { date: string, reason: string, count: number, };

export type Board = { tickets: Array<BoardTicket>, unattached: Array<Summary>, closed: Array<BoardTicket>, };

export type BoardTicket = { waits_on?: Array<Summary>, followed_by?: Array<Summary>, 
/**
 * The tickets whose proposals would change this ticket's references:
 * what it touches and relies on here is the truth's, and an edit waits.
 */
drafts?: Array<Ref>, ref: Ref, title: string, kind?: TicketKind, priority?: Priority, status?: Status, withdrawn?: boolean, };

export type Closed = { node: NodeView, findings: Array<Finding>, baselined: number, untouched: Array<Ref> | null, unnamed: Array<RestsOn> | null, settled: Array<Summary>, due: Array<RecipeState> | null, };

export type Configured = { config: StoreConfig, new: number, };

export type CoverageAnswer = { requirements: Array<Summary>, tests: Array<Summary>, } | { criteria: Array<CriterionCoverage>, } | { criteria: Array<CoveringCriterion>, };

export type CoveringCriterion = { ref: Ref, statement: string | null, module: ModuleSummary, };

export type Criterion = { module: string, id: string, statement: string, };

export type CriterionCoverage = { ref: Ref, statement: string, requirements: Array<Summary>, tests: Array<Summary>, };

export type CurrentTicket = { id: string, title: string, };

export type Decision = { id?: number | null, date: string, text: string, supersedes?: Array<number>, adopted?: boolean, withdrawn?: boolean, };

export type DecisionNode = { requirement: string, id: number, date: string, text: string, supersedes?: Array<number>, adopted?: boolean, };

export type Decl = { path: string, kind: string, line: number, 
/**
 * The file it is declared in, set where the file is known, so no one
 * recovers it from the reference.
 */
file: string, };

export type Deleted = { deleted: Ref, warnings: Array<Finding>, baselined: number, settled?: Array<Summary>, reopened?: Array<Summary>, };

export type Draft = { ref: Ref, title: string, new: boolean, differs: Array<string>, };

export type EdgeView = { ref: Ref, relation: Relation, title: string | null, };

export type FieldError = { field: string, reason: string, };

export type File = { path: string, module: string, language: string, decls: Array<Decl>, 
/**
 * The fingerprint of the file's text as read.
 */
digest: string, expects_contract: boolean, lines: number, test_lines: number, };

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

export type FindingKey = { rule: Rule, check: string, ref: Ref, edge?: FindingEdge, };

export type Ingredient = { recipe: string, id: string, says: string, };

export type Judged = { recipe: Ref, judgement: Judgement, due: boolean | null, };

export type Judgement = { recipe: string, fingerprint: string, date: string, ticket: string, outcome: JudgementOutcome, };

export type JudgementOutcome = "held" | "updated";

export type Landed = { landed: boolean, node: NodeView | null, findings: Array<Finding>, baselined: number, untouched: Array<Ref> | null, unnamed: Array<RestsOn> | null, settled: Array<Summary>, due: Array<RecipeState> | null, waiting: Array<Summary>, note?: string, };

export type LanguageTier = { language: string, tier: number | null, };

export type LibraryEntry = { name: string, repository: string, server: string | null, current: CurrentTicket | null, };

export type Linked = { ref: Ref, title: string | null, };

export type LintAnswer = { findings: Array<Finding>, advisories: Array<Advisory>, mended: Array<Mended>, omens: Array<Omen>, counts: { [key in string]: number }, new: number, without_code: boolean, };

export type Mended = { ticket: string, title: string, finding: FindingKey, };

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

export type NodeAnswer = { node: NodeView, proposal?: ProposalView, drafts?: Array<Ref>, };

export type NodeView = { ref: Ref, in: Array<EdgeView>, out: Array<EdgeView>, } & (Page | Requirement | Ticket | Module | Criterion | File | Decl | Test | Recipe | Ingredient | DecisionNode);

export type NodesAnswer = { nodes: Array<Summary>, };

export type Omen = { check: string, ref: Ref, reason: string, };

export type Order = { next?: Array<string>, high?: Array<string>, later?: Array<string>, };

export type OtherWork = { "kind": "since_base" } | { "kind": "ticket", ticket: string, title: string, };

export type Outline = { roots: Array<Tree>, unplaced: Array<Tree>, without_code: boolean, };

export type Page = { slug: string, title: string, body: string, links: Array<Ref>, };

export type Priority = "next" | "high" | "later";

export type ProposalView = { pages: Array<Draft>, requirements: Array<Draft>, deleted: Array<Ref>, tickets: Array<Draft>, };

export type Recipe = { id: string, kind: string, title: string, text: string, ingredients: Array<Ingredient>, files: Array<string>, uses: Array<string>, file: string, line: number, missing: Array<string>, digest: string, };

export type RecipeKind = { uses: Array<string>, };

export type RecipeState = { ref: Ref, title: string, due: boolean, via: Ref, };

export type Ref = string;

export type Relation = "links" | "covered_by" | "touches" | "relies_on" | "contains" | "imports" | "belongs_to" | "checks" | "uses" | "represents" | "supersedes";

export type Renamed = { node: NodeView, still_named_in: Array<StillNamed>, };

export type Reordered = { priority: Priority, order: Array<Ref>, };

export type Requirement = { slug: string, title: string, statement: string, scenarios: Array<Scenario>, covered_by: Array<Ref>, decisions: Array<Decision>, 
/**
 * Retired: it no longer holds. It stays readable with its decisions, the
 * last saying why, but no page links it and it covers no criterion.
 */
retired?: boolean, };

export type RestsOn = { ref: Ref, title: string, via: Array<Ref>, decisions: Array<Decision>, says?: string, };

export type RestsOnAnswer = { nodes: Array<RestsOn>, };

export type RestsOnMe = { nodes: Array<Affected>, tickets: Array<Summary>, };

export type Rollup = { modules: number, contracted: number, criteria: number, tested: number, requirements: number, held: number, described: number, decided: number, };

export type Roots = { grimoire?: string, rites?: string, };

export type Rule = "dangling" | "uncovered" | "malformed" | "unspecified";

export type Scenario = { title: string, given: string, when: string, then: string, };

export type Scope = { include: Array<string>, exclude: Array<string>, };

export type ServerCopy = { address: string, fetched: string | null, reachable: boolean, refused: boolean, };

export type SourceAnswer = { file: Ref, language: string, text: string, focus: number, plain: boolean, annotations: Array<Annotation>, };

export type SourceLocation = { file: string, line: number, contract_line?: number, };

export type Started = { node: NodeView, rests_on: Array<RestsOn>, related: Array<Summary>, recipes: Array<RecipeState>, waiting: Array<WaitingHere>, };

export type Status = "open" | "active" | "landing" | "closed";

export type StatusAnswer = { repository: string, roots: Roots, languages: Array<string>, version: string, supported_languages: Array<string>, tiers: Array<LanguageTier>, nodes: { [key in string]: number }, edges: number, unresolved_imports: number, malformed: number, scope: Scope, baseline: BaselineSummary | null, shared: boolean, server: ServerCopy | null, current: CurrentTicket | null, grimoire: string | null, library: Array<LibraryEntry>, };

export type StillNamed = { test: Ref, file: string, line: number, };

export type StoreConfig = { version: number, repository: string, roots: Roots, languages: Array<string>, scope?: Scope, order?: Order, site?: Array<string>, large_file?: number, plain_file?: number, recipe_kinds?: { [key in string]: RecipeKind }, };

export type Summary = { ref: Ref, title: string, kind?: TicketKind, priority?: Priority, status?: Status, withdrawn?: boolean, };

export type Test = { file: string, name: string, line: number, checks: Array<string>, };

export type Ticket = { slug: string, title: string, kind: TicketKind, priority: Priority, status: Status, touches: Array<Ref>, relies_on: Array<Ref>, body: string, decision: Decision | null, };

export type TicketKind = "change" | "gap" | "question";

export type TicketsAnswer = { tickets: Array<Summary>, };

export type ToolError = { "error": "not_found", message: string, } | { "error": "invalid", message: string, details: Array<FieldError>, } | { "error": "conflict", message: string, details: Array<FieldError>, } | { "error": "store_unavailable", message: string, };

export type Tree = { ref: Ref, title: string, 
/**
 * For a page, the fraction of criteria under it some requirement names;
 * for a requirement, its criterion count; null for anything else.
 */
coverage: number | null, findings: number, 
/**
 * How many of `findings` are in the baseline: inherited, not new.
 */
baselined: number, omens: number, 
/**
 * The checks of the findings on this node itself, sorted and distinct,
 * so a view can say what is wrong and not only how much.
 */
checks: Array<string>, 
/**
 * How many tests check this node: a criterion's evidence, or what holds
 * a constraint that names no criterion.
 */
tests: number, rollup: Rollup, rites: boolean, children: Array<Tree>, };

export type WaitingHere = { ticket: string, title: string, warning: string, };

export type Withdrawn = { node: NodeView, settled: Array<Summary>, reopened: Array<Summary>, };

export type Written = { node: NodeView, warnings: Array<Finding>, baselined: number, settled?: Array<Summary>, reopened?: Array<Summary>, reprioritised?: Array<Summary>, };

/** Each tool's answer, by tool name. */
export type ToolAnswers = {
  find: NodesAnswer;
  get: NodeAnswer;
  record_decision: NodeAnswer;
  supersede: NodeAnswer;
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
  retire_node: Written;
  start_ticket: Started;
  close_ticket: Closed;
  land_ticket: Landed;
  rename_node: Renamed;
  reorder: Reordered;
  configure: Configured;
  adopt: Adopted;
  delete_node: Deleted;
  withdraw_ticket: Withdrawn;
  judge_recipe: Judged;
};
