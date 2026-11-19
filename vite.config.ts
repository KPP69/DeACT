import { defineConfig } from "vite";
import { svelte } from "@sveltejs/vite-plugin-svelte";

// Tauri expects a fixed dev port and ignores the Rust backend directory.
// https://v2.tauri.app/start/frontend/vite/
export default defineConfig(async () => ({
  plugins: [svelte()],

  // Prevent Vite from clearing the screen so Rust errors stay visible.
  clearScreen: false,
  server: {
    port: 1420,
    strictPort: true,
    watch: {
      ignored: ["**/src-tauri/**"],
    },
  },
}));
