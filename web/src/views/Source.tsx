/**
 * # Module contract
 *
 * ```json
 * {"purpose":"A file's page: its text from the source tool as annotated source, every line numbered and highlighted, and each line the grimoire attaches a node to marked with links to it.","criteria":[{"id":"annotated-source","statement":"A file's page shows its text from the source tool, line by line with numbers, syntax highlighted by web/src/highlight in the palette's syntax tokens (Loading until its lines are highlighted, the same file's last lines kept while a changed text is highlighted, and plain lines if its grammar fails to load), long lines wrapping under their own indent and a contract's one-line JSON laid out indented under its line, each annotated line marked with links to its node and what that node links (a contract's criteria, a test block's checks); opened at #L<n> it scrolls to that line and marks it."},{"id":"plain-source","statement":"A file whose source answer is marked plain is shown with web/src/highlight's plain lines, no tone and no JSON laid out, under a note that it is shown plain because of its length, keeping its numbers, marks, links and #L<n>."}]}
 * ```
 */

import { Fragment, useEffect, useMemo, useState } from "react";
import { Link, useLocation } from "react-router-dom";
import { api, useQuery } from "../api";
import { TAB, highlight, indentWidth, plain, type Token, type Tone } from "../highlight";
import { ErrorBox, RefLink } from "../components";
import { idOf, localId, nodePath, type Annotation, type FileNode } from "../types";

/** A file as its annotated source: every line numbered, the lines the graph attaches nodes to marked and linked. */
export function FileSource({ n }: { n: FileNode }) {
  const ref = `file:${n.path}`;
  const src = useQuery(() => api.source(ref), [ref]);
  const { hash } = useLocation();
  const focus = /^#L\d+$/.test(hash) ? Number(hash.slice(2)) : null;
  const byLine = new Map<number, Annotation[]>();
  for (const a of src.data?.annotations ?? []) byLine.set(a.line, [...(byLine.get(a.line) ?? []), a]);
  const text = src.data?.text;
  // Past the project's plain-file limit the highlighter never runs: it would drag (req:web-annotated-source).
  const asPlain = src.data?.plain ?? false;
  const lines = useLines(text, asPlain, n.language, n.path);
  // Each time lines are drawn, bring the line asked for into view.
  useEffect(() => {
    if (focus && lines) document.getElementById(`L${focus}`)?.scrollIntoView({ block: "center" });
  }, [focus, lines]);
  return (
    <>
      <p className="text-fg-dim">
        {n.language} · in <RefLink ref={`module:${n.module}`} />
      </p>
      {src.error && <ErrorBox error={src.error} />}
      {!lines && !src.error && <p className="text-fg-dim">Loading…</p>}
      {asPlain && (
        <p className="text-fg-dim" id="plain-note">
          Shown plain: at {countLines(text ?? "").toLocaleString("en")} lines this file is longer than the project highlights.
        </p>
      )}
      {lines && (
        <div className="bg-bg-1 border border-line rounded-md overflow-x-auto" role="region" aria-label={`Source of ${n.path}`}>
          <table className="border-collapse w-full font-mono text-[12.5px] leading-[1.55]" style={{ tabSize: TAB }}>
            <caption className="sr-only">Source of {n.path}, each line the grimoire attaches a node to described by its marks</caption>
            <tbody>
              {lines.map((tokens, i) => {
                const line = i + 1;
                const marks = byLine.get(line) ?? [];
                const on = line === focus;
                // An annotated line gets its marks on a row of their own just
                // above it, so they show however long the line is.
                return (
                  <Fragment key={line}>
                    {marks.length > 0 && (
                      <tr className="bg-bg-2" id={`marks-L${line}`}>
                        <td />
                        <td className="pt-1.5 pb-0.5 font-sans text-xs whitespace-normal">
                          {marks.map((a) => (
                            <SourceMark key={a.ref} a={a} />
                          ))}
                        </td>
                      </tr>
                    )}
                    <tr
                      id={`L${line}`}
                      aria-current={on ? "location" : undefined}
                      aria-describedby={marks.length ? `marks-L${line}` : undefined}
                      className={on ? "bg-accent-dim/40" : marks.length ? "bg-bg-2" : ""}
                    >
                      <td className="text-right pr-3 pl-2 text-fg-faint select-none align-top w-[1%] whitespace-nowrap">
                        {/* A router link, so the marked line follows the hash, Back and Forward included. */}
                        <Link to={{ hash: `L${line}` }} aria-label={`Line ${line}`} className="text-fg-faint hover:text-fg">
                          {line}
                        </Link>
                      </td>
                      <td className="pr-4 align-top text-fg">
                        {rowsOf(tokens).map((row, r) => {
                          // Wrapped, the rest of a row hangs under its own indent, one step in.
                          const hang = `${leading(row) + 4}ch`;
                          return (
                            <div key={r} className="whitespace-pre-wrap wrap-anywhere" style={{ paddingLeft: hang, textIndent: `-${hang}` }}>
                              {row.map((t, j) =>
                                t.tone ? (
                                  <span key={j} className={TONE[t.tone]} data-tone={t.tone}>
                                    {t.text}
                                  </span>
                                ) : (
                                  <Fragment key={j}>{t.text}</Fragment>
                                ),
                              )}
                            </div>
                          );
                        })}
                      </td>
                    </tr>
                  </Fragment>
                );
              })}
            </tbody>
          </table>
        </div>
      )}
    </>
  );
}

/**
 * A file's lines as its page shows them: plain past the project's limit, else
 * highlighted, which waits for the file's grammar the first time one in its
 * language is opened; null until there is something to draw. While a changed
 * text is highlighted, the same file's last lines stay shown rather than
 * blinking out; a grammar that fails to load leaves the file plain.
 */
function useLines(text: string | undefined, asPlain: boolean, language: string, path: string): Token[][] | null {
  const plainLines = useMemo(() => (text !== undefined && asPlain ? plain(text) : null), [text, asPlain]);
  const [done, setDone] = useState<{ path: string; lines: Token[][] } | null>(null);
  useEffect(() => {
    if (text === undefined || asPlain) return;
    let live = true;
    highlight(text, language, path).then(
      (lines) => live && setDone({ path, lines }),
      (error: unknown) => {
        console.warn("highlighting failed, the file is shown plain", error);
        if (live) setDone({ path, lines: plain(text) });
      },
    );
    return () => {
      live = false;
    };
  }, [text, asPlain, language, path]);
  if (text === undefined) return null;
  return plainLines ?? (done?.path === path ? done.lines : null);
}

/** How many lines a text has, as the daemon counts them: a final line feed ends the last line and starts none. */
function countLines(text: string): number {
  return text === "" ? 0 : text.split("\n").length - (text.endsWith("\n") ? 1 : 0);
}

/** A line's tokens as the rows it shows on: one, or several where a contract's JSON is laid out. */
function rowsOf(tokens: Token[]): Token[][] {
  const rows: Token[][] = [[]];
  for (const t of tokens) {
    const [first, ...rest] = t.text.split("\n");
    if (first) rows[rows.length - 1].push({ ...t, text: first });
    for (const piece of rest) rows.push(piece ? [{ ...t, text: piece }] : []);
  }
  return rows;
}

/** The width of a row's leading whitespace, in columns. */
function leading(row: Token[]): number {
  return indentWidth(row.map((t) => t.text).join(""));
}

// Written out whole so the stylesheet generator sees each class.
const TONE: Record<Tone, string> = {
  keyword: "text-syn-keyword",
  string: "text-syn-string",
  comment: "text-syn-comment italic",
  number: "text-syn-number",
  type: "text-syn-type",
  function: "text-syn-function",
  meta: "text-syn-meta",
};

const MARK_TONE: Record<Annotation["kind"], string> = { contract: "text-module", test: "text-test", decl: "text-fg-dim" };

/** What a line is: its node, linked, and for a contract its criteria or for a test block what it checks. */
function SourceMark({ a }: { a: Annotation }) {
  return (
    <span className="inline-flex gap-1.5 items-baseline mr-4" data-annotation={a.kind}>
      <span className={`uppercase tracking-[0.05em] text-[10.5px] ${MARK_TONE[a.kind]}`}>{a.kind}</span>
      <Link to={nodePath(a.ref)}>{a.kind === "decl" ? a.title : idOf(a.ref)}</Link>
      {a.links.map((l) => (
        <Link key={l.ref} to={nodePath(l.ref)} title={l.title ?? undefined} className="text-fg-dim">
          {l.ref.includes("#") ? `#${localId(l.ref)}` : idOf(l.ref)}
        </Link>
      ))}
    </span>
  );
}
