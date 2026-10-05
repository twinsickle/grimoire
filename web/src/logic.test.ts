// The UI's pure logic and its source rules, without a browser. The views are
// tested in a browser by web/behaviour.

import { afterEach, expect, test, vi } from "vitest";
import { ApiError, call, grimoireBase, Unreachable } from "./api";
import { decisionsInForce, isDescribed } from "./decisions";
import main from "./main.tsx?raw";
import css from "./styles.css?raw";
import { handoff } from "./handoff";
import { goneOf, idOf, kindOf, libraryChoices, libraryShown, localId, missingRoot, serverAway, type AnyNode } from "./types";

/** Every .ts and .tsx file under src, with its text, loaded by Vite as it would any module. */
const sources: Record<string, string> = import.meta.glob(["./**/*.ts", "./**/*.tsx", "!./**/*.test.ts"], { query: "?raw", import: "default", eager: true });

afterEach(() => vi.unstubAllGlobals());

/**
 * # Test contract
 *
 * ```json
 * {"test": "an-address-names-its-grimoire", "checks": ["web/src/api#grimoire-base"]}
 * ```
 */
/**
 * # Test contract
 *
 * ```json
 * {"test": "a-missing-root-says-which-is-missing", "checks": ["web/src/types#missing-root"]}
 * ```
 */
test("a root named but not written is told apart from no root named", () => {
  const unwritten = missingRoot("overview");
  expect(unwritten).toContain("`overview` has no page yet");
  expect(unwritten).toContain("put a page `overview`");
  expect(unwritten).not.toContain("configure");
  for (const none of [null, undefined, ""]) expect(missingRoot(none)).toContain("configure");
});

/**
 * # Test contract
 *
 * ```json
 * {"test": "the-header-names-a-lone-grimoire", "checks": ["web/src/App#library-switcher"]}
 * ```
 */
test("the header names a lone grimoire and lists a library of more", () => {
  expect(libraryShown(null, [])).toBe("none");
  expect(libraryShown("shop", [])).toBe("name");
  expect(libraryShown("shop", [{ name: "shop" }])).toBe("name");
  expect(libraryShown("shop", [{ name: "ledger" }, { name: "shop" }])).toBe("list");
});

test("an address names its grimoire only by a leading /g/ and a slug", () => {
  expect(grimoireBase("/g/ledger/")).toBe("/g/ledger");
  expect(grimoireBase("/g/ledger")).toBe("/g/ledger");
  expect(grimoireBase("/g/billing-service/n/page%3Agoal")).toBe("/g/billing-service");
  // The bare routes are the daemon's default.
  for (const bare of ["/", "/cauldron", "/n/page%3Ag", "/gx/ledger/", "/g/", "/g/Not A Slug/", "/x/g/ledger/"]) expect(grimoireBase(bare)).toBe("");
});

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
  expect(goneOf(ticket("landing")).size).toBe(0);
  // A page linking a requirement that does not exist is a dangling finding, not history.
  expect(goneOf({ ref: "page:readiness", in: [], out } as unknown as AnyNode).size).toBe(0);
});

/**
 * # Test contract
 *
 * ```json
 * {"test": "a-server-that-is-away-is-said-in-the-header", "checks": ["web/src/types#server-away"]}
 * ```
 */
test("a machine whose server did not answer says so, with when its copy was fetched", () => {
  const address = "https://grimoire.example";
  expect(serverAway(null)).toBeNull();
  expect(serverAway(undefined)).toBeNull();
  expect(serverAway({ address, fetched: "2026-10-03T14:02:11Z", reachable: true, refused: false })).toBeNull();
  expect(serverAway({ address, fetched: "2026-10-03T14:02:11Z", reachable: false, refused: false })).toBe("server unreachable, intent as of 2026-10-03 14:02 UTC");
  expect(serverAway({ address, fetched: null, reachable: false, refused: false })).toBe("server unreachable");
  // A token the server refuses is said as that, not as a server that is away.
  expect(serverAway({ address, fetched: "2026-10-03T14:02:11Z", reachable: false, refused: true })).toBe("server refused this machine's token: run grimoire login");
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
 * {"test": "every-answer-is-told-for-what-it-is", "checks": ["web/src/api#errors-typed"]}
 * ```
 */
test("every answer and failure is told for what it is, never as a JSON syntax error", async () => {
  const answer = (status: number, body: string) => vi.fn(async () => new Response(body, { status, headers: { "content-type": "text/plain" } }));
  const tried = async (fetch: () => Promise<Response>) => {
    vi.stubGlobal("fetch", fetch);
    return call("status").then(
      () => null,
      (e: unknown) => e as Error,
    );
  };
  const tool = JSON.stringify({ error: "internal", message: "the store could not be read" });
  const cases: [string, () => Promise<Response>, (e: Error) => void][] = [
    ["no connection", vi.fn(async () => Promise.reject(new TypeError("Failed to fetch"))), (e) => expect(e).toBeInstanceOf(Unreachable)],
    ["the proxy's empty 500", answer(500, ""), (e) => expect(e).toBeInstanceOf(Unreachable)],
    ["a proxy's 502 with text", answer(502, "Bad Gateway"), (e) => expect(e.message).toMatch(/not answering \(502: Bad Gateway\)/)],
    ["a tool error on a 500", answer(500, tool), (e) => expect([e instanceof ApiError, e.message]).toEqual([true, "the store could not be read"])],
    ["a 404 that is no tool's", answer(404, "Not Found"), (e) => expect([e instanceof ApiError, e instanceof Unreachable, e.message]).toEqual([false, false, `status answered 404 with "Not Found", not a tool's answer`])],
    ["a 200 that is not JSON", answer(200, "<!doctype html>"), (e) => expect([e instanceof Unreachable, e.message]).toEqual([false, 'status answered 200 with something that is not JSON: "<!doctype html>"'])],
  ];
  for (const [what, fetch, check] of cases) {
    const e = await tried(fetch);
    expect(e, what).not.toBeNull();
    expect(e?.message, what).not.toMatch(/JSON input|Unexpected token|is not valid JSON/);
    check(e as Error);
  }
});

/** A stand-in EventSource the tests open, fail and close by hand. */
class FakeStream {
  static made: FakeStream[] = [];
  static readonly CONNECTING = 0;
  static readonly OPEN = 1;
  static readonly CLOSED = 2;
  readyState = FakeStream.CONNECTING;
  onopen: (() => void) | null = null;
  onmessage: (() => void) | null = null;
  onerror: (() => void) | null = null;
  constructor(readonly url: string) {
    FakeStream.made.push(this);
  }
  open() {
    this.readyState = FakeStream.OPEN;
    this.onopen?.();
  }
  /** Closed for good, as a browser closes it when a reconnect is answered with no event stream. */
  refuse() {
    this.readyState = FakeStream.CLOSED;
    this.onerror?.();
  }
}

/** A daemon whose status answers for the checkout C:/r and whose every other tool answers nothing. */
function stubStatus() {
  vi.stubGlobal("fetch", vi.fn(async (url: string) => new Response(JSON.stringify(url.endsWith("/api/status") ? { repository: "C:/r" } : {}), { status: 200 })));
}

/** api.ts afresh, its one stream and its away state new, over the fake stream and fake timers. */
async function freshApi() {
  vi.resetModules();
  vi.useFakeTimers();
  FakeStream.made = [];
  vi.stubGlobal("EventSource", FakeStream);
  return import("./api");
}

/**
 * # Test contract
 *
 * ```json
 * {"test": "a-closed-stream-is-opened-again-with-a-backoff", "checks": ["web/src/api#stream-reopens", "web/src/api#one-stream"]}
 * ```
 */
test("a stream closed for good is opened again after one second doubling to ten, one at a time, the wait reset by an opening", async () => {
  const api = await freshApi();
  try {
    stubStatus();
    let told = 0;
    const stop = api.onEvent(() => told++);
    api.onEvent(() => {});
    // The stream is opened once the page's checkout is known, from the first status.
    await vi.advanceTimersByTimeAsync(0);
    expect(FakeStream.made).toHaveLength(1);
    expect(FakeStream.made[0].url).toBe("/api/events?checkout=C%3A%2Fr");
    // Each refusal waits longer before the next attempt: 1, 2, 4, 8, then 10 seconds at most.
    for (const wait of [1000, 2000, 4000, 8000, 10_000, 10_000]) {
      const before = FakeStream.made.length;
      FakeStream.made.at(-1)?.refuse();
      vi.advanceTimersByTime(wait - 1);
      expect(FakeStream.made.length, `not before ${wait}ms`).toBe(before);
      vi.advanceTimersByTime(1);
      expect(FakeStream.made.length, `after ${wait}ms`).toBe(before + 1);
    }
    // An opening counts as an event, and resets the wait.
    const atOpen = told;
    FakeStream.made.at(-1)?.open();
    expect(told).toBe(atOpen + 1);
    const before = FakeStream.made.length;
    FakeStream.made.at(-1)?.refuse();
    vi.advanceTimersByTime(1000);
    expect(FakeStream.made.length).toBe(before + 1);
    // A stream that errs but is reconnecting by itself is left to it: no second stream.
    const live = FakeStream.made.at(-1) as FakeStream;
    live.open();
    live.readyState = FakeStream.CONNECTING;
    live.onerror?.();
    vi.advanceTimersByTime(20_000);
    expect(FakeStream.made.at(-1)).toBe(live);
    stop();
  } finally {
    vi.useRealTimers();
  }
});

/**
 * # Test contract
 *
 * ```json
 * {"test": "the-page-asks-again-while-the-daemon-is-away", "checks": ["web/src/api#retries-while-away", "web/src/api#away-keeps-data"]}
 * ```
 */
test("while the daemon is away every listener is told every two seconds, and no longer once it answers", async () => {
  const api = await freshApi();
  try {
    stubStatus();
    let told = 0;
    api.onEvent(() => told++);
    await vi.advanceTimersByTimeAsync(0);
    vi.advanceTimersByTime(10_000);
    expect(told, "nothing is retried while the daemon answers").toBe(0);
    vi.stubGlobal("fetch", vi.fn(async () => new Response("", { status: 500 })));
    await expect(api.call("status")).rejects.toBeInstanceOf(api.Unreachable);
    vi.advanceTimersByTime(api.RETRY_MS * 3);
    expect(told).toBe(3);
    vi.stubGlobal("fetch", vi.fn(async () => new Response("{}", { status: 200 })));
    await api.call("status");
    vi.advanceTimersByTime(api.RETRY_MS * 3);
    expect(told, "the retries stop once it answers").toBe(3);
  } finally {
    vi.useRealTimers();
  }
});

/**
 * # Test contract
 *
 * ```json
 * {"test": "every-call-names-the-pages-checkout", "checks": ["web/src/api#names-its-checkout"]}
 * ```
 */
test("every call names the checkout the first status answered for, learned before any other call goes", async () => {
  const api = await freshApi();
  try {
    const sent: { tool: string; body: Record<string, unknown> }[] = [];
    vi.stubGlobal(
      "fetch",
      vi.fn(async (url: string, init: { body: string }) => {
        const tool = url.split("/api/")[1];
        sent.push({ tool, body: JSON.parse(init.body) as Record<string, unknown> });
        return new Response(JSON.stringify(tool === "status" ? { repository: "C:/shop/wt" } : {}), { status: 200 });
      }),
    );
    expect(api.currentCheckout()).toBeNull();
    await Promise.all([api.call("lint"), api.call("find", { query: "x" })]);
    // One status first, naming none; every call after names the checkout it answered for.
    expect(sent[0]).toEqual({ tool: "status", body: {} });
    expect(sent.filter((s) => s.tool === "status")).toHaveLength(1);
    expect(sent.slice(1).map((s) => s.body.checkout)).toEqual(["C:/shop/wt", "C:/shop/wt"]);
    expect(sent.find((s) => s.tool === "find")?.body.query).toBe("x");
    await api.call("status");
    expect(sent.at(-1)?.body.checkout).toBe("C:/shop/wt");
    expect(api.currentCheckout()).toBe("C:/shop/wt");
    // The event stream names it too.
    api.onEvent(() => {});
    expect(FakeStream.made.at(-1)?.url).toBe("/api/events?checkout=C%3A%2Fshop%2Fwt");
  } finally {
    vi.useRealTimers();
  }
});

/**
 * # Test contract
 *
 * ```json
 * {"test": "a-checkout-no-longer-served-is-learned-again", "checks": ["web/src/api#names-its-checkout"]}
 * ```
 */
test("a call refused for the checkout it names forgets it, learns it again and is sent once more", async () => {
  const api = await freshApi();
  try {
    // First the daemon serves C:/old; then that checkout is gone, and C:/new answers status.
    let served = "C:/old";
    const sent: Record<string, unknown>[] = [];
    vi.stubGlobal(
      "fetch",
      vi.fn(async (url: string, init: { body: string }) => {
        const body = JSON.parse(init.body) as Record<string, unknown>;
        sent.push(body);
        if (url.endsWith("/api/status") && (body.checkout === undefined || body.checkout === served)) return new Response(JSON.stringify({ repository: served }), { status: 200 });
        if (body.checkout !== served) return new Response(JSON.stringify({ error: "invalid", message: "no checkout", details: [{ field: "checkout", reason: "gone" }] }), { status: 400 });
        return new Response("{}", { status: 200 });
      }),
    );
    await api.call("lint");
    expect(api.currentCheckout()).toBe("C:/old");
    served = "C:/new";
    await api.call("lint");
    expect(api.currentCheckout()).toBe("C:/new");
    expect(sent.at(-1)?.checkout).toBe("C:/new");
  } finally {
    vi.useRealTimers();
  }
});

/**
 * # Test contract
 *
 * ```json
 * {"test": "the-switcher-offers-each-checkout", "checks": ["web/src/App#library-switcher"]}
 * ```
 */
test("the switcher offers each checkout of a grimoire, named by its folder, the one on screen chosen", () => {
  const library = [
    { name: "ledger", repository: "C:/code/ledger" },
    { name: "shop", repository: "C:/code/shop" },
    { name: "shop", repository: "C:/code/shop/.claude/worktrees/wt" },
  ];
  const choices = libraryChoices("shop", "C:/code/shop/.claude/worktrees/wt", library);
  expect(choices.map((c) => c.label)).toEqual(["ledger", "shop · shop", "shop · wt"]);
  expect(choices.map((c) => c.chosen)).toEqual([false, false, true]);
  expect(choices[2].href).toBe("/g/shop/?checkout=C%3A%2Fcode%2Fshop%2F.claude%2Fworktrees%2Fwt");
  // Before the checkout is known, the grimoire on screen is chosen.
  expect(libraryChoices("ledger", null, library).map((c) => c.chosen)).toEqual([true, false, false]);
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

/**
 * # Test contract
 *
 * ```json
 * {"test": "described-until-decided", "checks": ["web/src/decisions#described"]}
 * ```
 */
test("a requirement is described while its decisions in force are all adoptions", () => {
  const d = (text: string, adopted = false, supersedes: number[] = []) => ({ date: "2026-10-04", text, adopted, supersedes });
  expect(isDescribed([])).toBe(false);
  expect(isDescribed([d("read off", true)])).toBe(true);
  expect(isDescribed([d("read off", true), d("chosen")])).toBe(false);
  // A plain adoption restated with the mark, superseding it.
  expect(isDescribed([d("read off, in words"), d("read off", true, [1])])).toBe(true);
  expect(isDescribed([d("read off", true), d("chosen", false, [1])])).toBe(false);
});

/**
 * # Test contract
 *
 * ```json
 * {"test": "a-ticket-hands-its-next-step-to-an-agent", "checks": ["web/src/handoff#handoff"]}
 * ```
 */
test("a ticket's next step is a line for the agent, and an answer waiting or a closed ticket has none", () => {
  const t = (kind: "change" | "gap" | "question", status: "open" | "active" | "landing" | "closed") => handoff({ ref: "ticket:0137", title: "Fix uninstall", kind, status });
  expect(t("change", "open")).toBe('Start ticket 0137 ("Fix uninstall") with the grimoire skill.');
  expect(t("change", "active")).toBe('Continue ticket 0137 ("Fix uninstall") with the grimoire skill.');
  expect(t("change", "landing")).toBe('Ticket 0137 ("Fix uninstall") is waiting to land: land it with the grimoire skill once its change is on the main line.');
  expect(t("question", "open")).toBe('Resolve the question 0137 ("Fix uninstall") with the grimoire skill: lay out the options for me to choose from.');
  expect(t("gap", "active")).toBe('Resolve the gap 0137 ("Fix uninstall") with the grimoire skill: lay out the options for me to choose from.');
  for (const kind of ["change", "gap", "question"] as const) expect(t(kind, "closed")).toBeNull();
  expect(t("question", "landing")).toBeNull();
  expect(t("gap", "landing")).toBeNull();
});
