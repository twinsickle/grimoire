// The UI's pure logic and its source rules, without a browser. The views are
// tested in a browser by web/behaviour.

import { afterEach, expect, test, vi } from "vitest";
import { ApiError, call } from "./api";
import { decisionsInForce } from "./decisions";
import main from "./main.tsx?raw";
import css from "./styles.css?raw";
import { goneOf, idOf, kindOf, localId, type AnyNode } from "./types";

/** Every .ts and .tsx file under src, with its text, loaded by Vite as it would any module. */
const sources: Record<string, string> = import.meta.glob(["./**/*.ts", "./**/*.tsx", "!./**/*.test.ts"], { query: "?raw", import: "default", eager: true });

afterEach(() => vi.unstubAllGlobals());

/**
 * # Test contract
 *
 * ```json
 * {"test": "references-split-at-the-first-colon", "checks": ["web/src/types#reference-helpers"]}
 * ```
 */
test("a reference splits at the first colon and nowhere else", () => {
  expect(kindOf("criterion:crates/core/src/lint#stable-order")).toBe("criterion");
  expect(idOf("criterion:crates/core/src/lint#stable-order")).toBe("crates/core/src/lint#stable-order");
  expect(idOf("decl:src/a.rs#Repo::open")).toBe("src/a.rs#Repo::open");
  // A criterion or test id after the last #, though the module or file name holds one.
  expect(localId("criterion:src/a#b#kept")).toBe("kept");
  expect(localId("test:src/a#b.ts#holds")).toBe("holds");
});

/**
 * # Test contract
 *
 * ```json
 * {"test": "only-a-closed-tickets-references-are-gone", "checks": ["web/src/types#gone-references"]}
 * ```
 */
test("a closed ticket's untitled references are gone; an open ticket's and another node's are not", () => {
  const out = [
    { ref: "module:src/ready", relation: "touches", title: "Decide whether a workspace is ready." },
    { ref: "req:ready-old-name", relation: "touches", title: null },
  ];
  const ticket = (status: string) => ({ ref: "ticket:0004", status, in: [], out }) as unknown as AnyNode;
  expect([...goneOf(ticket("closed"))]).toEqual(["req:ready-old-name"]);
  expect(goneOf(ticket("open")).size).toBe(0);
  expect(goneOf(ticket("active")).size).toBe(0);
  // A page linking a requirement that does not exist is a dangling finding, not history.
  expect(goneOf({ ref: "page:readiness", in: [], out } as unknown as AnyNode).size).toBe(0);
});

/**
 * # Test contract
 *
 * ```json
 * {"test": "tool-errors-keep-their-code", "checks": ["web/src/api#errors-typed"]}
 * ```
 */
test("a tool error is thrown with its code and message", async () => {
  const body = { error: "not_found", message: "req:nope does not exist" };
  vi.stubGlobal("fetch", vi.fn(async () => new Response(JSON.stringify(body), { status: 404 })));
  const e = await call("get", { ref: "req:nope" }).catch((err: unknown) => err);
  expect(e).toBeInstanceOf(ApiError);
  expect((e as ApiError).body.error).toBe("not_found");
  expect((e as ApiError).message).toBe("req:nope does not exist");
  expect((e as ApiError).status).toBe(404);
});

/**
 * # Test contract
 *
 * ```json
 * {"test": "the-app-mounts-under-strict-mode", "checks": ["req:web-strict-mode"]}
 * ```
 */
test("the app mounts under StrictMode", () => {
  expect(main).toMatch(/<StrictMode>\s*<App \/>\s*<\/StrictMode>/);
});

/**
 * # Test contract
 *
 * ```json
 * {"test": "colours-are-tokens", "checks": ["req:web-theme-tokens"]}
 * ```
 */
test("colours are named by token, never written as literals", () => {
  for (const kind of ["page", "req", "module", "criterion", "ticket", "file", "decl", "test"]) {
    expect(css).toContain(`--color-${kind}:`);
  }
  // And one per syntax tone, which the source view names as a utility of the same name.
  for (const tone of ["keyword", "string", "comment", "number", "type", "function", "meta"]) {
    expect(css).toContain(`--color-syn-${tone}:`);
    expect(sources["./views/Node.tsx"]).toContain(`${tone}: "text-syn-${tone}`);
  }
  const literal = /#[0-9a-fA-F]{3,8}\b|\brgba?\(|\bhsla?\(/;
  expect(Object.keys(sources)).toContain("./App.tsx");
  const offenders = Object.entries(sources).filter(([, text]) => literal.test(text)).map(([path]) => path);
  expect(offenders).toEqual([]);
});

/**
 * # Test contract
 *
 * ```json
 * {"test": "decisions-sort-into-in-force-and-superseded", "checks": ["web/src/decisions#in-force"]}
 * ```
 */
test("decisions sort into those in force, newest first, and those superseded, with what superseded them", () => {
  const d = (text: string, supersedes?: number[]) => ({ date: "2026-09-29", text, supersedes });
  const { inForce, superseded } = decisionsInForce([d("one"), d("two"), d("three", [1]), d("four", [1, 3])]);
  expect(inForce.map((x) => [x.n, x.d.text])).toEqual([
    [4, "four"],
    [2, "two"],
  ]);
  expect(superseded.map((x) => [x.n, x.by])).toEqual([
    [3, 4],
    [1, 4],
  ]);
  expect(decisionsInForce([]).inForce).toEqual([]);
});
