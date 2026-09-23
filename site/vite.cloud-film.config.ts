import { fileURLToPath } from "node:url";
import tailwindcss from "@tailwindcss/vite";
import viteReact from "@vitejs/plugin-react";
import { defineConfig } from "vite";

const siteRoot = fileURLToPath(new URL(".", import.meta.url));

export default defineConfig({
  root: siteRoot,
  base: "./",
  publicDir: false,
  plugins: [tailwindcss(), viteReact()],
  server: { host: "0.0.0.0", port: 5174, strictPort: true },
  build: {
    outDir: fileURLToPath(new URL("../artifacts/cloud-reveal", import.meta.url)),
    emptyOutDir: true,
    rollupOptions: {
      input: {
        dreamscape: fileURLToPath(new URL("./cloud-dreamscape.html", import.meta.url)),
        legacy: fileURLToPath(new URL("./cloud-film.html", import.meta.url)),
      },
    },
  },
});
