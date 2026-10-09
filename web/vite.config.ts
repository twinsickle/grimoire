import { defineConfig, loadEnv, type ProxyOptions } from "vite";
import react from "@vitejs/plugin-react";
import tailwindcss from "@tailwindcss/vite";
import { closeWithDaemon, sameOrigin } from "./devProxy";

// The dev server proxies the daemon so the UI hot-reloads while the daemon
// restarts under bacon. See "Development loop" in README.md.
export default defineConfig(({ mode }) => {
  const daemon = loadEnv(mode, process.cwd(), "GRIMOIRE_").GRIMOIRE_DAEMON ?? "http://127.0.0.1:4242";
  return config(daemon);
});

// The daemon's own origin is dropped, and a stream the daemon drops is closed for the browser too (devProxy.ts).
const proxied: ProxyOptions["configure"] = (proxy) => {
  sameOrigin(proxy);
  closeWithDaemon(proxy);
};

const config = (daemon: string) => ({
  plugins: [react(), tailwindcss()],
  server: {
    port: 5173,
    proxy: {
      // The daemon refuses any Origin but its own. A page on this dev server
      // has this server's origin, so the proxy drops that one; any other is
      // passed on for the daemon to refuse.
      "/api": { target: daemon, changeOrigin: true, configure: proxied },
      "/mcp": { target: daemon, changeOrigin: true, configure: proxied },
      // A grimoire of the daemon's library, at its own address.
      "^/g/[a-z0-9-]+/(api|mcp)": { target: daemon, changeOrigin: true, configure: proxied },
    },
  },
  build: { outDir: "dist", emptyOutDir: true, sourcemap: true },
});
