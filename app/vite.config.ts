import { defineConfig } from "vite";
import react from "@vitejs/plugin-react";
import tailwindcss from "@tailwindcss/vite";

// Tauri expects a fixed port and must see Rust errors in the terminal.
export default defineConfig({
  plugins: [react(), tailwindcss()],
  clearScreen: false,
  server: { host: "127.0.0.1", port: 1420, strictPort: true },
  envPrefix: ["VITE_", "TAURI_ENV_"],
  // One chunk on purpose: the app loads from disk, and all four languages'
  // texts (about 450 kB of the 1.45 MB) are in it so the language changes at
  // once. The limit still catches an accidental large dependency.
  build: { target: "chrome120", sourcemap: false, chunkSizeWarningLimit: 1500 },
});
