// The UI's pure logic and its source rules, without a browser. The views are
// tested in a browser by web/behaviour.

import { afterEach, expect, test, vi } from "vitest";
import { ApiError, call } from "./api";
import main from "./main.tsx?raw";
import css from "./styles.css?raw";
import { idOf, kindOf } from "./types";

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
  const literal = /#[0-9a-fA-F]{3,8}\b|\brgba?\(|\bhsla?\(/;
  expect(Object.keys(sources)).toContain("./App.tsx");
  const offenders = Object.entries(sources).filter(([, text]) => literal.test(text)).map(([path]) => path);
  expect(offenders).toEqual([]);
});
