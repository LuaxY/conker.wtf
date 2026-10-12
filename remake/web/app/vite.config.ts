import tailwindcss from "@tailwindcss/vite";
import react from "@vitejs/plugin-react";
import { defineConfig } from "vite";

// The landing builds into web/ itself (index.html + landing/), next to play.html, dist/ and
// assets/, so one static server over web/ serves the whole site.
export default defineConfig({
  plugins: [react(), tailwindcss()],
  base: "./",
  build: { outDir: "..", emptyOutDir: false, assetsDir: "landing" },
  // `bun run dev`: the game page, the client, the asset pack and the server list come from the dev
  // web server (`bun run serve`, :8080).
  server: { proxy: { "^/(play\\.(html|js)|dist/|assets/|api/)": "http://localhost:8080" } },
});
