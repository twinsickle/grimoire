/**
 * # Module contract
 *
 * ```json
 * {"purpose":"Syntax highlighting for the annotated source: a file's text as lines of tokens, each named by a syntax tone the palette colours.","criteria":[{"id":"lines-of-tokens","statement":"highlight answers a file's text, once Shiki and the grammar of the file's language are loaded (each imported only the first time it is needed, so the first page load holds none of them), as one line of tokens per line of text, their text joined being the line's text, each token named by the tone of its innermost highlighted scope, only a line feed ending a line and a lone CR kept as text; a construct spanning lines (a block comment, a string) keeps its tone on every line; a fenced json block inside a comment, as a module or test contract is, is highlighted as JSON with each line's comment marker left as comment, a one-line block that parses laid out indented under its line, only whitespace between its tokens changing (the one case where a line's tokens do not join to its text), and a fence never closed stays comment; a .tsx file is read with the TSX grammar, so code inside markup is toned as code; a language with no grammar comes back as plain lines; the highlighter's tokens are read and no HTML is ever made."},{"id":"plain","statement":"plain answers a file's text as one line per line of text with no tone and nothing laid out, its text being the line's, only a line feed ending a line, a final one starting none, and the CR before one dropped, without running the highlighter."}]}
 * ```
 */

import type { HighlighterCore, LanguageRegistration, ThemeRegistrationRaw } from "shiki/core";
// Only the file's address: the build emits the WASM as an asset of its own.
import onigWasm from "shiki/onig.wasm?url";

/** The syntax tones the palette has a colour for. */
export type Tone = "keyword" | "string" | "comment" | "number" | "type" | "function" | "meta";

/** A run of a line's text, with the tone it is coloured in, or none. */
export type Token = { text: string; tone: Tone | null };

type Grammar = () => Promise<{ default: LanguageRegistration[] }>;

// Each grammar is imported the first time a file in it is opened, so none of
// them, nor Shiki itself, is part of the first page load. Named by the
// language a file node carries; TypeScript's .tsx files take the TSX grammar.
const GRAMMARS: Record<string, Grammar> = {
  rust: () => import("@shikijs/langs/rust"),
  typescript: () => import("@shikijs/langs/typescript"),
  tsx: () => import("@shikijs/langs/tsx"),
  // For the fenced JSON inside comments: module and test contracts.
  json: () => import("@shikijs/langs/json"),
};

/** The grammar a file is highlighted with: its language's, TSX for a .tsx file. */
function grammarOf(language: string, path = ""): string {
  return language === "typescript" && path.endsWith(".tsx") ? "tsx" : language;
}

// The tones are read off a theme of Shiki's whose colours are not colours but
// the tones' names in disguise: each scope the palette tones gets one, and
// everything else the plain foreground. Shiki's HTML is never made.
// Made up rather than written as literals, since they are not colours: the
// palette's colours are tokens in the stylesheet (req:web-theme-tokens).
const TONES: Tone[] = ["keyword", "string", "comment", "number", "type", "function", "meta"];
const disguise = (n: number) => `#${String(n).padStart(6, "0")}`;
const PLAIN = disguise(0);
const COLOUR = Object.fromEntries(TONES.map((tone, i) => [tone, disguise(i + 1)])) as Record<Tone, string>;
const TONE_OF = new Map<string, Tone>(Object.entries(COLOUR).map(([tone, colour]) => [colour, tone as Tone]));

// TextMate scopes onto tones; the most specific selector wins, so an
// operator is plain though it is a keyword, and a JSON key is meta though it
// is a support type.
const SCOPES: [Tone | null, string[]][] = [
  ["comment", ["comment", "punctuation.definition.comment"]],
  ["string", ["string", "constant.character", "constant.other.character-class.regexp"]],
  ["number", ["constant.numeric"]],
  ["keyword", ["keyword", "storage", "constant.language", "variable.language", "keyword.operator.new", "keyword.operator.expression", "keyword.operator.word"]],
  [null, ["keyword.operator", "storage.type.function.arrow", "meta.template.expression"]],
  ["type", ["entity.name.type", "entity.name.class", "entity.other.inherited-class", "entity.name.tag", "support.type", "support.class"]],
  ["function", ["entity.name.function", "support.function"]],
  ["meta", ["support.type.property-name", "entity.other.attribute-name", "meta.attribute", "meta.decorator", "punctuation.definition.attribute"]],
];

const THEME: ThemeRegistrationRaw = {
  name: "grimoire-tones",
  type: "dark",
  settings: [
    { settings: { foreground: PLAIN, background: PLAIN } },
    ...SCOPES.map(([tone, scope]) => ({ scope, settings: { foreground: tone ? COLOUR[tone] : PLAIN } })),
  ],
};

let core: Promise<HighlighterCore> | null = null;

/**
 * Shiki with the Oniguruma engine, made once and on first use. Its WASM is a
 * file of the build's own, fetched only then, so the first page load carries
 * none of it; the JavaScript regex engine was measured several times slower
 * on TypeScript (question 0246).
 */
function highlighter(): Promise<HighlighterCore> {
  core ??= (async () => {
    const [{ createHighlighterCore }, { createOnigurumaEngine }] = await Promise.all([import("shiki/core"), import("shiki/engine/oniguruma")]);
    return createHighlighterCore({ themes: [THEME], langs: [GRAMMARS.json()], engine: createOnigurumaEngine(() => fetch(onigWasm)) });
  })();
  // A failure to bring Shiki up is not kept, so a chunk that failed to load is tried again with the next file;
  // a WASM that failed to load stays failed, since Shiki keeps its first attempt.
  core.catch(() => {
    core = null;
  });
  return core;
}

/** Shiki with a grammar loaded, or null when there is none for it. */
async function load(grammar: string): Promise<HighlighterCore | null> {
  const grammarFor = GRAMMARS[grammar];
  if (!grammarFor) return null;
  const hl = await highlighter();
  if (!hl.getLoadedLanguages().includes(grammar)) await hl.loadLanguage(grammarFor());
  return hl;
}

/**
 * A file's text as lines of tokens; plain lines when there is no grammar for
 * its language. A fenced JSON block inside a comment, as a contract is, is
 * highlighted as JSON with its comment markers left as comment. The grammar
 * is loaded first, the first time a file in it is asked for.
 */
export async function highlight(text: string, language: string, path = ""): Promise<Token[][]> {
  const grammar = grammarOf(language, path);
  const hl = await load(grammar);
  if (!hl) return text.split(/\r?\n/).map((line) => (line ? [{ text: line, tone: null }] : []));
  return jsonInComments(hl, tokens(hl, text, grammar));
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

/** Shiki's tokens for a text as lines of toned tokens, a run of one tone merged. */
function tokens(hl: HighlighterCore, text: string, grammar: string): Token[][] {
  // Shiki ends a line at a line feed, dropping a CR before it and keeping a lone one as text.
  return hl.codeToTokensBase(text, { lang: grammar, theme: THEME.name }).map((line) => {
    const out: Token[] = [];
    for (const t of line) {
      if (!t.content) continue;
      const tone = TONE_OF.get((t.color ?? PLAIN).toLowerCase()) ?? null;
      const last = out[out.length - 1];
      if (last && last.tone === tone) last.text += t.content;
      else out.push({ text: t.content, tone });
    }
    return out;
  });
}

// A comment line's marker (`//!`, `///`, `//`, ` *`, `#`) and what follows it, so
// JSON punctuation right after a marker stays JSON; a fence opening and closing.
const MARKER = /^(\s*(?:\/\/[/!]?|\*|#)?\s?)(.*)$/;
const OPENS = /^\s*[^\w\s{["]*\s*```json\s*$/;
const CLOSES = /^\s*[^\w\s{["]*\s*```\s*$/;

/** Re-highlights, as JSON, the lines between a comment's ```json fence and its closing one. */
function jsonInComments(hl: HighlighterCore, lines: Token[][]): Token[][] {
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
      const rows = tokens(hl, pretty, "json").flatMap((row, k) => (k === 0 ? row : [{ text: `\n${gap}`, tone: null }, ...row]));
      lines[i + 1] = [...(marker ? [{ text: marker, tone: "comment" as const }] : []), ...rows];
      i = end;
      continue;
    }
    const body = tokens(hl, parts.map((p) => p.body).join("\n"), "json");
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
