import path from "node:path";
import { execSync } from "node:child_process";
import process from "node:process";
import tailwindcss from "@tailwindcss/vite";
import react from "@vitejs/plugin-react";
import type { Plugin } from "vite";
import { defineConfig } from "vitest/config";

const PORT = 1420;

/** Before the dev server binds, name the process already holding the port. */
function portInUseHint(): Plugin {
  return {
    name: "port-in-use-hint",
    apply: "serve",
    configureServer() {
      // Vitest starts a server of its own that never binds the port: let tests run beside the app.
      if (process.env.VITEST) return;
      let pids = "";
      try {
        pids = execSync(`lsof -nP -iTCP:${PORT} -sTCP:LISTEN -t`, { encoding: "utf8" }).trim();
      } catch {
        return; // lsof exits non-zero when nothing is listening.
      }
      if (!pids) return;
      const list = pids.split("\n");
      const details = list
        .map((pid) => {
          try {
            return `  ${pid}  ${execSync(`ps -o command= -p ${pid}`, { encoding: "utf8" }).trim()}`;
          } catch {
            return `  ${pid}`;
          }
        })
        .join("\n");
      console.error(
        `\nPort ${PORT} is already in use by:\n${details}\n\nStop it with: kill ${list.join(" ")}\n`,
      );
      process.exit(1);
    },
  };
}

const host = process.env.TAURI_DEV_HOST;

export default defineConfig({
  plugins: [portInUseHint(), react(), tailwindcss()],
  resolve: { alias: { "@": path.resolve(import.meta.dirname, "src") } },
  clearScreen: false,
  server: {
    port: PORT,
    strictPort: true,
    host: host || false,
    hmr: host ? { protocol: "ws", host, port: 1421 } : undefined,
    watch: { ignored: ["**/src-tauri/**"] },
  },
  build: { target: "safari16", sourcemap: !!process.env.TAURI_ENV_DEBUG },
  test: {
    environment: "jsdom",
    globals: true,
    setupFiles: ["./src/test/setup.ts"],
    css: false,
    passWithNoTests: true,
  },
});
