/**
 * # Module contract
 *
 * ```json
 * {"purpose":"Syntax highlighting for the annotated source: a file's text as lines of tokens, each named by a syntax tone the palette colours.","criteria":[{"id":"lines-of-tokens","statement":"highlight answers a file's text as one line of tokens per line of text, their text joined being the line's text, each token named by the tone of its innermost highlighted scope, only a line feed ending a line and a lone CR kept as text; a construct spanning lines (a block comment, a string) keeps its tone on every line; a fenced json block inside a comment, as a module or test contract is, is highlighted as JSON with each line's comment marker left as comment, a one-line block that parses laid out indented under its line, only whitespace between its tokens changing (the one case where a line's tokens do not join to its text), and a fence never closed stays comment; a language with no grammar comes back as plain lines; the highlighter's HTML is read, never rendered."},{"id":"plain","statement":"plain answers a file's text as one line per line of text with no tone and nothing laid out, its text being the line's, only a line feed ending a line, a final one starting none, and the CR before one dropped, without running the highlighter."}]}
 * ```
 */

import hljs from "highlight.js/lib/core";
import rust from "highlight.js/lib/languages/rust";
import typescript from "highlight.js/lib/languages/typescript";
import json from "highlight.js/lib/languages/json";
import xml from "highlight.js/lib/languages/xml";

// Only the languages Grimoire extracts, by the name a file node carries.
hljs.registerLanguage("rust", rust);
hljs.registerLanguage("typescript", typescript);
// Not a language of its own here: the TypeScript grammar highlights JSX markup through it.
hljs.registerLanguage("xml", xml);
// For the fenced JSON inside comments: module and test contracts.
hljs.registerLanguage("json", json);

/** The syntax tones the palette has a colour for. */
export type Tone = "keyword" | "string" | "comment" | "number" | "type" | "function" | "meta";

/** A run of a line's text, with the tone it is coloured in, or none. */
export type Token = { text: string; tone: Tone | null };

// highlight.js scopes, full (`title.function`) or by their first part, onto tones.
const TONES: Record<string, Tone> = {
  keyword: "keyword",
  literal: "keyword",
  string: "string",
  regexp: "string",
  char: "string",
  comment: "comment",
  doc: "comment",
  quote: "comment",
  number: "number",
  type: "type",
  built_in: "type",
  "title.class": "type",
  name: "type",
  title: "function",
  "title.function": "function",
  meta: "meta",
  attr: "meta",
  attribute: "meta",
  symbol: "meta",
};

const ENTITIES: Record<string, string> = { "&amp;": "&", "&lt;": "<", "&gt;": ">", "&quot;": '"', "&#x27;": "'", "&#39;": "'" };

/**
 * A span's class attribute as a tone: `hljs-title class_ inherited__` is the
 * scope `title.class.inherited`, looked up whole and then by ever shorter
 * prefixes, so it is a `title.class`.
 */
function toneOf(classes: string): Tone | null {
  const parts = classes
    .split(/\s+/)
    .map((c) => c.replace(/^hljs-/, "").replace(/_+$/, ""))
    .filter(Boolean);
  for (let n = parts.length; n > 0; n--) {
    const tone = TONES[parts.slice(0, n).join(".")];
    if (tone) return tone;
  }
  return null;
}

/**
 * A file's text as lines of tokens; plain lines when there is no grammar for
 * its language. A fenced JSON block inside a comment, as a contract is, is
 * highlighted as JSON with its comment markers left as comment.
 */
export function highlight(text: string, language: string): Token[][] {
  return jsonInComments(tokens(text, language));
}

/** A file's text as untoned lines, for one too long to highlight: the highlighter never runs. A final line feed ends the last line and starts none, as the daemon counts. */
export function plain(text: string): Token[][] {
  const pieces = text.split("\n");
  const ended = pieces.length > 1 && pieces[pieces.length - 1] === "";
  if (ended) pieces.pop();
  return pieces.map((line, i, all) => {
    // Only the CR before a line feed is dropped; a lone one is text.
    const own = (i < all.length - 1 || ended) && line.endsWith("\r") ? line.slice(0, -1) : line;
    return own ? [{ text: own, tone: null }] : [];
  });
}

function tokens(text: string, language: string): Token[][] {
  if (!hljs.getLanguage(language)) return text.split(/\r?\n/).map((line) => (line ? [{ text: line, tone: null }] : []));
  const html = hljs.highlight(text, { language, ignoreIllegals: true }).value;
  const lines: Token[][] = [[]];
  // The tones of the spans open here, innermost last; an unmapped span inherits its parent's.
  const open: (Tone | null)[] = [];
  const push = (raw: string) => {
    const decoded = raw.replace(/&(?:amp|lt|gt|quot|#x27|#39);/g, (e) => ENTITIES[e]);
    decoded.split("\n").forEach((piece, i) => {
      if (i > 0) lines.push([]);
      if (piece) lines[lines.length - 1].push({ text: piece, tone: open[open.length - 1] ?? null });
    });
  };
  for (const [, tag, classes, textRun] of html.matchAll(/<(\/?)span(?: class="([^"]*)")?>|([^<]+)/g)) {
    if (textRun !== undefined) push(textRun);
    else if (tag) open.pop();
    else open.push(toneOf(classes ?? "") ?? open[open.length - 1] ?? null);
  }
  // Only a CR before a line feed ends a line, as it does for the extractor: drop
  // that one, wherever a span boundary left it, and keep a lone CR as text.
  const source = text.split("\n");
  lines.forEach((line, i) => {
    const last = line[line.length - 1];
    if (i < source.length - 1 && source[i].endsWith("\r") && last?.text.endsWith("\r")) {
      last.text = last.text.slice(0, -1);
      if (!last.text) line.pop();
    }
  });
  return lines;
}

// A comment line's marker (`//!`, `///`, `//`, ` *`, `#`) and what follows it, so
// JSON punctuation right after a marker stays JSON; a fence opening and closing.
const MARKER = /^(\s*(?:\/\/[/!]?|\*|#)?\s?)(.*)$/;
const OPENS = /^\s*[^\w\s{["]*\s*```json\s*$/;
const CLOSES = /^\s*[^\w\s{["]*\s*```\s*$/;

/** Re-highlights, as JSON, the lines between a comment's ```json fence and its closing one. */
function jsonInComments(lines: Token[][]): Token[][] {
  const text = (line: Token[]) => line.map((t) => t.text).join("");
  // A blank line is outside any comment: a fence never spans one.
  const comment = (line: Token[]) => line.length > 0 && line.every((t) => t.tone === "comment");
  for (let i = 0; i < lines.length; i++) {
    if (!comment(lines[i]) || !OPENS.test(text(lines[i]))) continue;
    let end = i + 1;
    while (end < lines.length && comment(lines[end]) && !CLOSES.test(text(lines[end]))) end++;
    // A fence never closed, or one that leaves the comment, stays comment.
    if (end >= lines.length || !comment(lines[end])) continue;
    const parts = lines.slice(i + 1, end).map((line) => {
      const m = MARKER.exec(text(line));
      return m ? { marker: m[1], body: m[2] } : { marker: "", body: text(line) };
    });
    // A one-line block that parses, as a contract is, is shown indented under
    // its line's number: display only, the file's lines are unchanged.
    const pretty = parts.length === 1 ? indented(parts[0].body) : null;
    if (pretty !== null) {
      const { marker } = parts[0];
      const gap = " ".repeat(marker.length);
      const rows = tokens(pretty, "json").flatMap((row, k) => (k === 0 ? row : [{ text: `\n${gap}`, tone: null }, ...row]));
      lines[i + 1] = [...(marker ? [{ text: marker, tone: "comment" as const }] : []), ...rows];
      i = end;
      continue;
    }
    const body = tokens(parts.map((p) => p.body).join("\n"), "json");
    parts.forEach((p, k) => {
      lines[i + 1 + k] = [...(p.marker ? [{ text: p.marker, tone: "comment" as const }] : []), ...(body[k] ?? [])];
    });
    i = end;
  }
  return lines;
}

/** The source view's tab stop, in characters. */
export const TAB = 4;

/** How many columns a line's leading whitespace fills, a tab reaching the next tab stop. */
export function indentWidth(line: string): number {
  let col = 0;
  for (const c of line) {
    if (c === "\t") col = (Math.floor(col / TAB) + 1) * TAB;
    else if (c === " ") col++;
    else break;
  }
  return col;
}

/**
 * A JSON object or array laid out two spaces to a level, or null when the text
 * is not one. Only whitespace between tokens changes: numbers, escapes and
 * repeated keys are shown exactly as written.
 */
export function indented(body: string): string | null {
  try {
    const value: unknown = JSON.parse(body);
    if (value === null || typeof value !== "object") return null;
  } catch {
    return null;
  }
  let out = "";
  let depth = 0;
  let inString = false;
  const newline = () => `\n${"  ".repeat(depth)}`;
  for (let i = 0; i < body.length; i++) {
    const c = body[i];
    if (inString) {
      out += c;
      if (c === "\\") out += body[++i];
      else if (c === '"') inString = false;
      continue;
    }
    if (/\s/.test(c)) continue;
    if (c === '"') inString = true;
    if (c === "{" || c === "[") {
      // An empty one stays as it is.
      const next = body.slice(i + 1).trimStart()[0];
      if (next === "}" || next === "]") {
        out += c;
        continue;
      }
      depth++;
      out += c + newline();
    } else if (c === "}" || c === "]") {
      if (!/[[{]$/.test(out)) {
        depth--;
        out += newline();
      }
      out += c;
    } else if (c === ",") out += "," + newline();
    else if (c === ":") out += ": ";
    else out += c;
  }
  return out;
}
