import { defineConfig } from "vite";
import react from "@vitejs/plugin-react";

// The web view's sources live in ui/. Tauri expects a fixed dev port and
// rebuilds the Rust side itself, so Vite ignores it.
export default defineConfig({
  root: "ui",
  plugins: [react()],
  clearScreen: false,
  server: {
    port: 1420,
    strictPort: true,
    watch: { ignored: ["**/src/**/*.rs", "**/target/**"] },
  },
  build: { target: "safari16", outDir: "dist", emptyOutDir: true },
});
