import { defineConfig } from "vite";
import { svelte } from "@sveltejs/vite-plugin-svelte";
import { readFileSync } from "node:fs";

// The app version, from the Rust manifest (used by the browser preview's sample data).
const version = /^version = "(.+)"/m.exec(readFileSync("src-tauri/Cargo.toml", "utf8"))?.[1] ?? "0.0.0";

// Tauri expects a fixed port and does not need the dev server's own reload banner.
export default defineConfig({
  plugins: [svelte()],
  define: { __APP_VERSION__: JSON.stringify(version) },
  clearScreen: false,
  server: { port: 1420, strictPort: true, watch: { ignored: ["**/src-tauri/**"] } },
  build: { target: "safari16", outDir: "dist", assetsInlineLimit: 0, chunkSizeWarningLimit: 200 },
});
