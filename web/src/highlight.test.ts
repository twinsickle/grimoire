// The source view's highlighting, without a browser.

import { wasmBinary } from "shiki/wasm";
import { expect, test, vi } from "vitest";
import { highlight, indentWidth, indented, plain, type Token } from "./highlight";
import highlightSource from "./highlight.ts?raw";

// The browser fetches Shiki's WASM from the build's assets; here the same bytes come from Shiki's inlined copy.
vi.stubGlobal("fetch", async () => new Response(wasmBinary, { headers: { "content-type": "application/wasm" } }));

const text = (line: Token[]) => line.map((t) => t.text).join("");
const toneAt = (line: Token[], word: string) => line.find((t) => t.text.includes(word))?.tone;

/**
 * # Test contract
 *
 * ```json
 * {"test": "plain-gives-untoned-lines", "checks": ["web/src/highlight#plain"]}
 * ```
 */
test("plain gives one untoned line per line, a contract's JSON left as written", () => {
  const rs = '//! # Module contract\r\n//! ```json\r\n//! {"purpose":"P.","criteria":[]}\r\n//! ```\r\nfn a() {}\rstill a\n';
  const lines = plain(rs);
  expect(lines.map(text)).toEqual(["//! # Module contract", "//! ```json", '//! {"purpose":"P.","criteria":[]}', "//! ```", "fn a() {}\rstill a"]);
  // A final line feed ends the last line and starts none, as the daemon counts; a CR before it is dropped too.
  expect(plain("a\r\nb\r\n").map(text)).toEqual(["a", "b"]);
  expect(plain("a\n\n").map(text)).toEqual(["a", ""]);
  expect(plain("")).toEqual([[]]);
  expect(lines.flat().every((t) => t.tone === null)).toBe(true);
});

/**
 * # Test contract
 *
 * ```json
 * {"test": "highlight-gives-lines-of-tokens", "checks": ["web/src/highlight#grammar-imported-on-first-use", "web/src/highlight#one-line-of-tokens-per-line", "web/src/highlight#only-a-line-feed-ends-a-line", "web/src/highlight#token-tone-is-innermost-scope", "web/src/highlight#tone-kept-across-lines", "web/src/highlight#contract-json-highlighted-as-json", "web/src/highlight#one-line-contract-json-laid-out", "web/src/highlight#unclosed-fence-stays-comment", "web/src/highlight#tsx-read-with-tsx-grammar", "web/src/highlight#no-grammar-comes-back-plain", "web/src/highlight#tokens-are-read-no-html-made"]}
 * ```
 */
test("highlighting gives one line of tokens per line, tones carried across line breaks", async () => {
  const ts = '/**\n * # Module contract\n */\nexport const a: string = "x" + 1; // <b>&amp;</b>\n';
  const lines = await highlight(ts, "typescript");
  expect(lines.map(text)).toEqual(ts.split("\n"));
  // The block comment is a comment on every line it spans, not only where its span opens.
  for (const line of lines.slice(0, 3)) expect(line.every((t) => t.tone === "comment")).toBe(true);
  expect(toneAt(lines[3], "export")).toBe("keyword");
  expect(toneAt(lines[3], '"x"')).toBe("string");
  expect(toneAt(lines[3], "1")).toBe("number");
  // Markup in the code is text, decoded once: never HTML, never double-escaped.
  expect(toneAt(lines[3], "<b>&amp;</b>")).toBe("comment");

  const rs = 'fn ready() -> bool {\r\n    let s = r#"a\r\nb"#;\r\n}\r\n';
  const rust = await highlight(rs, "rust");
  expect(rust.map(text)).toEqual(rs.split(/\r?\n/));
  expect(toneAt(rust[0], "fn")).toBe("keyword");
  expect(toneAt(rust[0], "ready")).toBe("function");
  // A raw string over two lines is a string on both.
  expect(toneAt(rust[1], '"a')).toBe("string");
  expect(toneAt(rust[2], 'b"')).toBe("string");

  // JSX markup in a .tsx file, which is extracted as TypeScript: names, attributes and values toned.
  const tsx = 'const a = (\n  <div className="x">\n    hi\n  </div>\n);\n';
  const jsx = await highlight(tsx, "typescript", "src/a.tsx");
  expect(jsx.map(text)).toEqual(tsx.split("\n"));
  expect(toneAt(jsx[1], "div")).toBe("type");
  expect(toneAt(jsx[1], "className")).toBe("meta");
  expect(toneAt(jsx[1], '"x"')).toBe("string");
  expect(toneAt(jsx[3], "div")).toBe("type");
  // TypeScript inside a JSX expression is TypeScript, and a template's interpolation is code, not string.
  const expr = (await highlight('<p onClick={() => go(1, `n${k}`)} />\n', "typescript", "src/a.tsx"))[0];
  expect(toneAt(expr, "go")).toBe("function");
  expect(toneAt(expr, "1")).toBe("number");
  expect(toneAt(expr, "`n")).toBe("string");
  expect(toneAt(expr, "${k")).toBeNull();
  // A .ts file is not read as TSX: a type assertion is not markup.
  const cast = (await highlight("const a = <string>b;\n", "typescript", "src/a.ts"))[0];
  expect(toneAt(cast, "string")).toBe("type");

  // A contract's fenced JSON is JSON, its comment markers comment, in either language.
  const rsContract = '//! # Module contract\n//!\n//! ```json\n//! {"purpose":"P","n":1}\n//! ```\nfn a() {}\n';
  const rc = await highlight(rsContract, "rust");
  // Every line as written but the one-line JSON, which is laid out indented under its marker.
  const laidOut = rsContract.split("\n");
  laidOut[3] = '//! {\n      "purpose": "P",\n      "n": 1\n    }';
  expect(rc.map(text)).toEqual(laidOut);
  expect(rc[3][0]).toEqual({ text: "//! ", tone: "comment" });
  expect(toneAt(rc[3], "purpose")).toBe("meta");
  expect(toneAt(rc[3], '"P"')).toBe("string");
  expect(toneAt(rc[3], "1")).toBe("number");
  expect(rc[2].every((t) => t.tone === "comment")).toBe(true);
  expect(rc[4].every((t) => t.tone === "comment")).toBe(true);
  const tsContract = '/**\n * ```json\n * {"test": "t",\n *  "checks": []}\n * ```\n */\n';
  const tc = await highlight(tsContract, "typescript");
  expect(tc.map(text)).toEqual(tsContract.split("\n"));
  expect(tc[2][0]).toEqual({ text: " * ", tone: "comment" });
  expect(toneAt(tc[2], "test")).toBe("meta");
  // JSON over two lines is still JSON on the second.
  expect(toneAt(tc[3], "checks")).toBe("meta");
  // A fence never closed stays comment; a fence in a string is not a comment's.
  const open = await highlight("//! ```json\n//! {\"a\": 1}\n", "rust");
  expect(open.flat().every((t) => t.tone === "comment")).toBe(true);
  const inString = await highlight('let s = "\n```json\n{1: 2}\n```\n";\n', "rust");
  expect(inString.slice(1, 4).flat().every((t) => t.tone === "string")).toBe(true);

  // JSON that does not parse on its line is highlighted as written.
  const broken = await highlight('//! ```json\n//! {"a": \n//! ```\n', "rust");
  expect(text(broken[1])).toBe('//! {"a": ');

  // Only a line feed ends a line: a lone CR is text, wherever a span boundary falls.
  const cr = "let a = 1;\rfn b() {}\r\nlet c = 2;\n";
  expect((await highlight(cr, "rust")).map(text)).toEqual(["let a = 1;\rfn b() {}", "let c = 2;", ""]);

  // A lone CR at the very end is text too.
  expect((await highlight("let a = 1;\r", "rust")).map(text)).toEqual(["let a = 1;\r"]);

  // Laying JSON out changes only whitespace: numbers, escapes and repeated keys as written.
  expect(indented('{"n":9007199254740993,"a":1,"a":2,"x":"\\u0061, {[\\"]}","e":[],"o":{ }}')).toBe(
    '{\n  "n": 9007199254740993,\n  "a": 1,\n  "a": 2,\n  "x": "\\u0061, {[\\"]}",\n  "e": [],\n  "o": {}\n}',
  );
  expect(indented('"just a string"')).toBeNull();
  expect(indented("{nope")).toBeNull();

  // JSON punctuation right after a marker is JSON's, not the marker's.
  const tight = await highlight('//! ```json\n//!{"a":\n//!1}\n//! ```\n', "rust");
  expect(tight[2][0]).toEqual({ text: "//!", tone: "comment" });
  expect(toneAt(tight[2], "}")).not.toBe("comment");
  expect(toneAt(tight[2], "1")).toBe("number");

  // A tab reaches the next tab stop.
  expect(indentWidth("\tx")).toBe(4);
  expect(indentWidth("  \t  x")).toBe(6);
  expect(indentWidth("x\t")).toBe(0);

  // A class's parent is a type, not a function.
  expect(toneAt((await highlight("class A extends Base {}\n", "typescript"))[0], "Base")).toBe("type");

  // Nothing of Shiki is imported up front by this module, so none of it is in the first page load through it: only types and the WASM file's address.
  const upFront = highlightSource.split("\n").filter((l) => /^import\s/.test(l) && !/^import type\s/.test(l));
  expect(upFront).toEqual(['import onigWasm from "shiki/onig.wasm?url";']);

  // No grammar: plain lines, the text unchanged.
  const plain = await highlight("a <b>\n\nc", "cobol");
  expect(plain.map(text)).toEqual(["a <b>", "", "c"]);
  expect(plain.flat().every((t) => t.tone === null)).toBe(true);
});

/**
 * # Test contract
 *
 * ```json
 * {"test": "each-language-has-its-grammar", "checks": ["web/src/highlight#one-line-of-tokens-per-line", "web/src/highlight#token-tone-is-innermost-scope", "web/src/highlight#htl-read-with-html-grammar", "web/src/highlight#languages-with-a-grammar", "web/src/highlight#contract-json-highlighted-as-json"]}
 * ```
 */
test("each language the extractor reads is toned by its own grammar", async () => {
  const one = async (code: string, language: string, path = "") => {
    const lines = await highlight(code, language, path);
    expect(lines.map(text)).toEqual(code.split("\n"));
    return lines;
  };

  // JavaScript, its markup included, in a .js file and a .jsx one alike.
  for (const path of ["src/a.js", "src/a.jsx", "src/a.mjs", "src/a.cjs"]) {
    const [js] = await one('export function go(x) { return <div className="y">{x + 1}</div>; }', "javascript", path);
    expect(toneAt(js, "export")).toBe("keyword");
    expect(toneAt(js, "go")).toBe("function");
    expect(toneAt(js, "div")).toBe("type");
    expect(toneAt(js, "className")).toBe("meta");
    expect(toneAt(js, '"y"')).toBe("string");
    expect(toneAt(js, "1")).toBe("number");
  }

  // Java: annotations are meta, as Rust's attributes are; its types are types, primitives among them; an import's name is plain.
  const java = await one('import java.util.List;\nclass A extends B {\n@Override\npublic List<? extends String> names(int n) { return of("a", 1); } /** doc */\nString[] xs;\n}', "java");
  expect(toneAt(java[0], "import")).toBe("keyword");
  expect(toneAt(java[0], "java.util")).toBeNull();
  expect(toneAt(java[1], "class")).toBe("keyword");
  expect(toneAt(java[1], "A")).toBe("type");
  expect(toneAt(java[1], "B")).toBe("type");
  expect(java[2].map((t) => t.tone)).toEqual(["meta"]);
  expect(toneAt(java[3], "public")).toBe("keyword");
  expect(toneAt(java[3], "List")).toBe("type");
  expect(toneAt(java[3], "String")).toBe("type");
  expect(toneAt(java[3], "?")).toBe("type");
  expect(toneAt(java[3], "names")).toBe("function");
  expect(toneAt(java[3], "int")).toBe("type");
  expect(toneAt(java[3], '"a"')).toBe("string");
  expect(toneAt(java[3], "/** doc */")).toBe("comment");
  expect(toneAt(java[4], "String")).toBe("type");

  // HTML, its script and style read as JavaScript and CSS; HTL by the same grammar.
  for (const language of ["html", "htl"]) {
    const html = await one('<!-- note -->\n<div class="a" data-sly-test="${x}"><script>let n = 1;</script><style>p { color: red; }</style></div>', language, "a.html");
    expect(html[0].map((t) => t.tone)).toEqual(["comment"]);
    expect(toneAt(html[1], "div")).toBe("type");
    expect(toneAt(html[1], "class")).toBe("meta");
    expect(toneAt(html[1], '"${x}"')).toBe("string");
    expect(toneAt(html[1], "let")).toBe("keyword");
    expect(toneAt(html[1], "1")).toBe("number");
    expect(toneAt(html[1], "color")).toBe("meta");
  }

  // XML.
  const [xml] = await one('<?xml version="1.0"?><!-- c --><a:b scope="1">t</a:b>', "xml");
  expect(toneAt(xml, "<!-- c -->")).toBe("comment");
  expect(toneAt(xml, "a:b")).toBe("type");
  expect(toneAt(xml, "scope")).toBe("meta");
  expect(toneAt(xml, '"1"')).toBe("string");

  // CSS and SCSS: selectors are types, properties meta, a number keeps its unit and a hex colour is a literal.
  const [css] = await one("/* c */ @media screen { .b, a[href]:hover { font-size: 12px; color: #fff; } }", "css");
  expect(toneAt(css, "/* c */")).toBe("comment");
  expect(toneAt(css, "@media")).toBe("keyword");
  expect(toneAt(css, ".b")).toBe("type");
  expect(toneAt(css, ":hover")).toBe("type");
  expect(toneAt(css, "font-size")).toBe("meta");
  expect(css.find((t) => t.text.includes("12"))).toEqual({ text: "12px", tone: "number" });
  expect(toneAt(css, "#fff")).toBe("number");
  const scss = await one('// c\n@use "x"; .b { &-d { margin: 2px; } @include mix(1); } %ph { }', "scss");
  expect(scss[0].map((t) => t.tone)).toEqual(["comment"]);
  expect(toneAt(scss[1], "@use")).toBe("keyword");
  expect(toneAt(scss[1], ".b")).toBe("type");
  expect(toneAt(scss[1], "margin")).toBe("meta");
  expect(toneAt(scss[1], "mix")).toBe("function");
  expect(toneAt(scss[1], "%ph")).toBe("type");

  // A contract in a markup comment is JSON, as in code, laid out under its line as a one-line block is.
  const contract = await highlight('<!--\n# Module contract\n```json\n{"purpose": "P", "n": 1}\n```\n-->\n<p></p>', "html");
  expect(toneAt(contract[3], "purpose")).toBe("meta");
  expect(toneAt(contract[3], "1")).toBe("number");
});
