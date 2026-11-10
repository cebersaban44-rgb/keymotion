import { defineConfig } from "vite";
import react from "@vitejs/plugin-react";
import tailwindcss from "@tailwindcss/vite";

// Tauri expects a fixed port and must see Rust errors in the terminal.
export default defineConfig({
  plugins: [react(), tailwindcss()],
  clearScreen: false,
  server: { host: "127.0.0.1", port: 1420, strictPort: true },
  envPrefix: ["VITE_", "TAURI_ENV_"],
  build: { target: "chrome120", sourcemap: false, chunkSizeWarningLimit: 900 },
});
