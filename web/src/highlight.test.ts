// The source view's highlighting, without a browser.

import { expect, test } from "vitest";
import { highlight, indentWidth, indented, type Token } from "./highlight";

const text = (line: Token[]) => line.map((t) => t.text).join("");
const toneAt = (line: Token[], word: string) => line.find((t) => t.text.includes(word))?.tone;

/**
 * # Test contract
 *
 * ```json
 * {"test": "highlight-gives-lines-of-tokens", "checks": ["web/src/highlight#lines-of-tokens"]}
 * ```
 */
test("highlighting gives one line of tokens per line, tones carried across line breaks", () => {
  const ts = '/**\n * # Module contract\n */\nexport const a: string = "x" + 1; // <b>&amp;</b>\n';
  const lines = highlight(ts, "typescript");
  expect(lines.map(text)).toEqual(ts.split("\n"));
  // The block comment is a comment on every line it spans, not only where its span opens.
  for (const line of lines.slice(0, 3)) expect(line.every((t) => t.tone === "comment")).toBe(true);
  expect(toneAt(lines[3], "export")).toBe("keyword");
  expect(toneAt(lines[3], '"x"')).toBe("string");
  expect(toneAt(lines[3], "1")).toBe("number");
  // Markup in the code is text, decoded once: never HTML, never double-escaped.
  expect(toneAt(lines[3], "<b>&amp;</b>")).toBe("comment");

  const rs = 'fn ready() -> bool {\r\n    let s = r#"a\r\nb"#;\r\n}\r\n';
  const rust = highlight(rs, "rust");
  expect(rust.map(text)).toEqual(rs.split(/\r?\n/));
  expect(toneAt(rust[0], "fn")).toBe("keyword");
  expect(toneAt(rust[0], "ready")).toBe("function");
  // A raw string over two lines is a string on both.
  expect(toneAt(rust[1], '"a')).toBe("string");
  expect(toneAt(rust[2], 'b"')).toBe("string");

  // JSX markup in a .tsx file, which is extracted as TypeScript: names, attributes and values toned.
  const tsx = 'const a = (\n  <div className="x">\n    hi\n  </div>\n);\n';
  const jsx = highlight(tsx, "typescript");
  expect(jsx.map(text)).toEqual(tsx.split("\n"));
  expect(toneAt(jsx[1], "div")).toBe("type");
  expect(toneAt(jsx[1], "className")).toBe("meta");
  expect(toneAt(jsx[1], '"x"')).toBe("string");
  expect(toneAt(jsx[3], "div")).toBe("type");

  // A contract's fenced JSON is JSON, its comment markers comment, in either language.
  const rsContract = '//! # Module contract\n//!\n//! ```json\n//! {"purpose":"P","n":1}\n//! ```\nfn a() {}\n';
  const rc = highlight(rsContract, "rust");
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
  const tc = highlight(tsContract, "typescript");
  expect(tc.map(text)).toEqual(tsContract.split("\n"));
  expect(tc[2][0]).toEqual({ text: " * ", tone: "comment" });
  expect(toneAt(tc[2], "test")).toBe("meta");
  // JSON over two lines is still JSON on the second.
  expect(toneAt(tc[3], "checks")).toBe("meta");
  // A fence never closed stays comment; a fence in a string is not a comment's.
  const open = highlight("//! ```json\n//! {\"a\": 1}\n", "rust");
  expect(open.flat().every((t) => t.tone === "comment")).toBe(true);
  const inString = highlight('let s = "\n```json\n{1: 2}\n```\n";\n', "rust");
  expect(inString.slice(1, 4).flat().every((t) => t.tone === "string")).toBe(true);

  // JSON that does not parse on its line is highlighted as written.
  const broken = highlight('//! ```json\n//! {"a": \n//! ```\n', "rust");
  expect(text(broken[1])).toBe('//! {"a": ');

  // Only a line feed ends a line: a lone CR is text, wherever a span boundary falls.
  const cr = "let a = 1;\rfn b() {}\r\nlet c = 2;\n";
  expect(highlight(cr, "rust").map(text)).toEqual(["let a = 1;\rfn b() {}", "let c = 2;", ""]);

  // A lone CR at the very end is text too.
  expect(highlight("let a = 1;\r", "rust").map(text)).toEqual(["let a = 1;\r"]);

  // Laying JSON out changes only whitespace: numbers, escapes and repeated keys as written.
  expect(indented('{"n":9007199254740993,"a":1,"a":2,"x":"\\u0061, {[\\"]}","e":[],"o":{ }}')).toBe(
    '{\n  "n": 9007199254740993,\n  "a": 1,\n  "a": 2,\n  "x": "\\u0061, {[\\"]}",\n  "e": [],\n  "o": {}\n}',
  );
  expect(indented('"just a string"')).toBeNull();
  expect(indented("{nope")).toBeNull();

  // JSON punctuation right after a marker is JSON's, not the marker's.
  const tight = highlight('//! ```json\n//!{"a":\n//!1}\n//! ```\n', "rust");
  expect(tight[2][0]).toEqual({ text: "//!", tone: "comment" });
  expect(toneAt(tight[2], "}")).not.toBe("comment");
  expect(toneAt(tight[2], "1")).toBe("number");

  // A tab reaches the next tab stop.
  expect(indentWidth("\tx")).toBe(4);
  expect(indentWidth("  \t  x")).toBe(6);
  expect(indentWidth("x\t")).toBe(0);

  // A class's parent is a type, not a function.
  expect(toneAt(highlight("class A extends Base {}\n", "typescript")[0], "Base")).toBe("type");

  // No grammar: plain lines, the text unchanged.
  const plain = highlight("a <b>\n\nc", "cobol");
  expect(plain.map(text)).toEqual(["a <b>", "", "c"]);
  expect(plain.flat().every((t) => t.tone === null)).toBe(true);
});
