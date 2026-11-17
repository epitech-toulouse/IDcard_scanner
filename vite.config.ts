import { defineConfig } from "vitest/config";
import { svelte } from "@sveltejs/vite-plugin-svelte";

// Port fixe attendu par src-tauri/tauri.conf.json (devUrl).
export default defineConfig({
  plugins: [svelte()],
  clearScreen: false,
  server: { port: 1420, strictPort: true, watch: { ignored: ["**/src-tauri/**", "**/target/**"] } },
  build: { target: "es2022" },
  test: { include: ["src/**/*.test.ts"] },
});
