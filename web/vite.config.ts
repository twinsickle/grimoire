import { defineConfig, loadEnv } from "vite";
import react from "@vitejs/plugin-react";
import tailwindcss from "@tailwindcss/vite";

// The dev server proxies the daemon so the UI hot-reloads while the daemon
// restarts under bacon. See "Development loop" in README.md.
export default defineConfig(({ mode }) => {
  const daemon = loadEnv(mode, process.cwd(), "GRIMOIRE_").GRIMOIRE_DAEMON ?? "http://127.0.0.1:4242";
  return config(daemon);
});

const config = (daemon: string) => ({
  plugins: [react(), tailwindcss()],
  server: {
    port: 5173,
    proxy: {
      "/api": { target: daemon, changeOrigin: true },
      "/mcp": { target: daemon, changeOrigin: true },
    },
  },
  build: { outDir: "dist", emptyOutDir: true, sourcemap: true },
});
