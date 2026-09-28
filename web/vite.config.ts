import { defineConfig, loadEnv, type ProxyOptions } from "vite";
import react from "@vitejs/plugin-react";
import tailwindcss from "@tailwindcss/vite";

// The dev server proxies the daemon so the UI hot-reloads while the daemon
// restarts under bacon. See "Development loop" in README.md.
export default defineConfig(({ mode }) => {
  const daemon = loadEnv(mode, process.cwd(), "GRIMOIRE_").GRIMOIRE_DAEMON ?? "http://127.0.0.1:4242";
  return config(daemon);
});

const sameOrigin: ProxyOptions["configure"] = (proxy) =>
  proxy.on("proxyReq", (proxyReq, req) => {
    const origin = req.headers.origin;
    if (origin && origin === `http://${req.headers.host}`) proxyReq.removeHeader("origin");
  });

const config = (daemon: string) => ({
  plugins: [react(), tailwindcss()],
  server: {
    port: 5173,
    proxy: {
      // The daemon refuses any Origin but its own. A page on this dev server
      // has this server's origin, so the proxy drops that one; any other is
      // passed on for the daemon to refuse.
      "/api": { target: daemon, changeOrigin: true, configure: sameOrigin },
      "/mcp": { target: daemon, changeOrigin: true, configure: sameOrigin },
    },
  },
  build: { outDir: "dist", emptyOutDir: true, sourcemap: true },
});
