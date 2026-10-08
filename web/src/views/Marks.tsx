/**
 * # Module contract
 *
 * ```json
 * {"purpose":"Draw the marks of trouble: each kind's mark, the legend of them all, and a node's own trouble in words.","criteria":[{"id":"mark-shapes","statement":"MarkDot draws a mark by its kind: new a dot in the bad colour, inherited a dot in the faint colour, recipe due a dot in the recipe colour ringed in it, may conflict a dashed ring in the accent colour and omen a slanted ring in the dim colour, each from the theme's tokens, with a title saying the mark's name and each reason given it, one line each; MarkChip names a mark as a chip, its text and border in the bad colour for new and dim for every other kind."},{"id":"legend","statement":"Legend names the five marks in the order new, inherited, recipe due, may conflict, omen, each beside its MarkDot, as one line labelled Marks."},{"id":"words","statement":"MarkWords says a node's marks one line each, gravest first: the mark's name as a chip, in the bad colour for new and never as a warning otherwise, then its check and its reason; a may-conflict line names the work it comes from, a ticket linked by id and title or the main line since this branch's base; a mark on another node than the one the page is about, such as an omen on a module's file, names that node, linked; given none, it draws nothing."}]}
 * ```
 */

import { RefLink, mono } from "../components";
import { MARK_NAME, type Mark, type MarkKind } from "../marks";
import type { Ref } from "../types";

/** Each mark's look, written out so Tailwind sees every class it must generate. */
const SHAPE: Record<MarkKind, string> = {
  new: "bg-bad",
  inherited: "bg-fg-faint",
  due: "bg-recipe outline outline-1 outline-recipe outline-offset-1",
  conflict: "border border-dashed border-accent",
  omen: "border border-fg-dim -skew-x-[18deg] w-[7px]",
};

/** One mark, its reasons said when it is pointed at. */
export function MarkDot({ kind, reasons = [] }: { kind: MarkKind; reasons?: string[] }) {
  const title = [MARK_NAME[kind], ...reasons].join("\n");
  return <span title={title} data-mark={kind} className={`inline-block shrink-0 size-2 rounded-full ${SHAPE[kind]}`} />;
}

/** The five marks, each beside its name. */
export function Legend() {
  // Read in the order a person meets them: the two kinds of finding, then what is not one.
  const order: MarkKind[] = ["new", "inherited", "due", "conflict", "omen"];
  return (
    <p aria-label="Marks" className="m-0 flex flex-wrap items-center gap-x-3.5 gap-y-1 text-[12px] text-fg-dim">
      {order.map((k) => (
        <span key={k} className="inline-flex items-center gap-1.5">
          <MarkDot kind={k} />
          {MARK_NAME[k]}
        </span>
      ))}
    </p>
  );
}

/** A mark's name as a chip: in the bad colour for new, which fails the check, and quiet for every other. */
export function MarkChip({ kind }: { kind: MarkKind }) {
  const tone = kind === "new" ? "text-bad border-bad" : "text-fg-dim border-line";
  return <span className={`inline-block not-italic text-[11px] px-1.5 py-px rounded-[10px] bg-bg-3 border ${tone}`}>{MARK_NAME[kind]}</span>;
}

/** A node's own trouble, one line each under its title: what kind, which check and why. */
export function MarkWords({ marks, on }: { marks: Mark[]; on: Ref }) {
  if (marks.length === 0) return null;
  return (
    <ul aria-label="Trouble here" className="list-none m-0 mb-3 p-0 flex flex-col gap-1">
      {marks.map((m, i) => (
        <li key={`${m.kind} ${m.check} ${m.ref} ${i}`} data-mark-line={m.kind} className={`flex gap-2 items-baseline ${m.kind === "new" ? "" : "text-fg-dim"} ${m.kind === "omen" ? "italic" : ""}`}>
          <span className="inline-flex items-center gap-1.5 shrink-0 w-[7.5em] not-italic">
            <MarkDot kind={m.kind} />
            <MarkChip kind={m.kind} />
          </span>
          {/* The check, then why, wrapping as one paragraph so a narrow column never overflows. */}
          <span className="min-w-0 [overflow-wrap:anywhere]">
            <span className={`${mono} text-[11px] text-fg-faint not-italic mr-2`}>{m.check}</span>
            {m.ref !== on && (
              <>
                <RefLink ref={m.ref} />{" "}
              </>
            )}
            {m.reason}
            {m.from && (
              <span className="text-fg-faint text-[12.5px]">
                {" "}
                {m.from.kind === "ticket" ? (
                  <>
                    · <RefLink ref={`ticket:${m.from.ticket}`} title={m.from.title} /> would bring this when it lands
                  </>
                ) : (
                  "· landed on the main line since this branch's base: merge the main line"
                )}
              </span>
            )}
          </span>
        </li>
      ))}
    </ul>
  );
}
