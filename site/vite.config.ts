import tailwindcss from "@tailwindcss/vite";
import { tanstackStart } from "@tanstack/react-start/plugin/vite";
import viteReact from "@vitejs/plugin-react";
import { nitro } from "nitro/vite";
import { defineConfig, loadEnv } from "vite";

export default defineConfig(({ mode }) => {
  const pages = loadEnv(mode, ".", "GITHUB_PAGES").GITHUB_PAGES === "1";
  return {
    base: pages ? "/omadesign/" : "/",
    server: {
      // The website imports the shared manuals without exposing the rest of the repo.
      fs: { allow: [".", "../docs"] },
    },
    build: pages ? { outDir: ".pages-build" } : undefined,
    plugins: [
      tailwindcss(),
      tanstackStart({
        router: { basepath: pages ? "/omadesign" : "/" },
        prerender: pages
          ? {
              enabled: true,
              autoSubfolderIndex: true,
              autoStaticPathsDiscovery: true,
              crawlLinks: false,
              failOnError: true,
            }
          : undefined,
      }),
      viteReact(),
      // Start prerenders its own static client output; Pages has no server runtime.
      ...(pages ? [] : [nitro({ preset: "vercel" })]),
    ],
  };
});
