import { defineConfig } from "vite";
import { svelte } from "@sveltejs/vite-plugin-svelte";
import { fileURLToPath } from "node:url";
export default defineConfig({
  plugins: [svelte()],
  resolve: { alias: { "@tauri-apps/api/core": fileURLToPath(new URL("./invoke.ts", import.meta.url)), "$lib": fileURLToPath(new URL("../../src/lib", import.meta.url)) } },
  server: { host: "127.0.0.1", port: 1432, strictPort: true },
});
