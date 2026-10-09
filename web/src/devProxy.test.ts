import { describe, expect, test } from "vitest";
import { closeWithDaemon } from "../devProxy";

/** Just enough of an emitter for the dev server's proxy hooks. */
function emitter() {
  const listeners = new Map<string, ((...args: never[]) => void)[]>();
  return {
    on(event: string, listener: (...args: never[]) => void) {
      listeners.set(event, [...(listeners.get(event) ?? []), listener]);
    },
    emit(event: string, ...args: unknown[]) {
      for (const l of listeners.get(event) ?? []) (l as (...a: unknown[]) => void)(...args);
    },
  };
}

/** The browser's side of a proxied answer: whether the proxy ended it, and whether it was cut. */
function browserSide(ended: boolean) {
  return { writableEnded: ended, destroyed: false, destroy() { this.destroyed = true; } };
}

describe("the dev server's proxy", () => {
  /**
   * # Test contract
   *
   * ```json
   * {"test": "the-dev-server-closes-a-stream-the-daemon-dropped", "checks": ["req:web-one-stream"]}
   * ```
   */
  test("the-dev-server-closes-a-stream-the-daemon-dropped", () => {
    const proxy = emitter();
    closeWithDaemon(proxy);
    // The daemon restarts mid-stream: its side closes while the browser's is still open, so the proxy cuts it.
    const daemonSide = emitter();
    const open = browserSide(false);
    proxy.emit("proxyRes", daemonSide, {}, open);
    expect(open.destroyed).toBe(false);
    daemonSide.emit("close");
    expect(open.destroyed).toBe(true);
    // An answer the proxy already ended, as every ordinary call is, is left alone.
    const call = emitter();
    const ended = browserSide(true);
    proxy.emit("proxyRes", call, {}, ended);
    call.emit("close");
    expect(ended.destroyed).toBe(false);
  });
});
