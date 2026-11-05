import path from "node:path";
import tailwindcss from "@tailwindcss/vite";
import react from "@vitejs/plugin-react";
import { defineConfig } from "vite";

// GitHub Pages serves the site at /Svetra/. Real compiler output is imported from
// ../tests/golden with `?raw`, so the page can't drift from what Svetra prints.
export default defineConfig({
  base: "/Svetra/",
  plugins: [react(), tailwindcss()],
  resolve: { alias: { "@": path.resolve(import.meta.dirname, "src") } },
  server: { fs: { allow: [".."] } },
});
