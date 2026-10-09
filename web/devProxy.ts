// The dev server's proxy hooks, kept apart from vite.config.ts so a unit test
// can reach them without Node's types: only what the hooks use is named here.

type Emits = { on(event: string, listener: (...args: never[]) => void): unknown };
type Request = { headers: { origin?: string; host?: string } };
type ProxyRequest = { removeHeader(name: string): void };
type BrowserSide = { writableEnded: boolean; destroy(): void };

/** The daemon refuses any Origin but its own. A page on this dev server has
 *  this server's origin, so the proxy drops that one; any other is passed on
 *  for the daemon to refuse. */
export function sameOrigin(proxy: Emits) {
  proxy.on("proxyReq", (proxyReq: ProxyRequest, req: Request) => {
    const origin = req.headers.origin;
    if (origin && origin === `http://${req.headers.host}`) proxyReq.removeHeader("origin");
  });
}

/** A daemon restarting drops its event stream; the proxy would hold the
 *  browser's side open, silent, so the page never reconnects. Close it too
 *  (req:web-one-stream). */
export function closeWithDaemon(proxy: Emits) {
  proxy.on("proxyRes", (daemonSide: Emits, _req: unknown, browserSide: BrowserSide) => {
    daemonSide.on("close", () => {
      if (!browserSide.writableEnded) browserSide.destroy();
    });
  });
}
