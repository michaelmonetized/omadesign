import tailwindcss from "@tailwindcss/vite";
import { tanstackStart } from "@tanstack/react-start/plugin/vite";
import viteReact from "@vitejs/plugin-react";
import { nitro } from "nitro/vite";
import { defineConfig, loadEnv } from "vite";
import { syncAgentDocs } from "./scripts/sync-agent-docs";

export default defineConfig(({ mode }) => {
  syncAgentDocs();
  const pages = loadEnv(mode, ".", "GITHUB_PAGES").GITHUB_PAGES === "1";
  const publicPreview = process.env.VITE_PUBLIC_SITE_PREVIEW === "1";
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
              crawlLinks: true,
              failOnError: true,
            }
          : undefined,
      }),
      viteReact(),
      // Start prerenders its own static client output; Pages has no server runtime.
      ...(pages
        ? []
        : [
            nitro({
              preset: "vercel",
              routeRules: {
                ...(publicPreview ? Object.fromEntries(
                  ["cloud", "account", "project", "showcase", "compete", "api"].flatMap((path) => [
                    [`/${path}`, { redirect: { to: `https://omadesign.app/${path}`, statusCode: 307 } }],
                    [`/${path}/**`, { redirect: { to: `https://omadesign.app/${path}/**`, statusCode: 307 } }],
                  ]),
                ) : {}),
                "/install": {
                  headers: {
                    "content-type": "text/plain; charset=utf-8",
                    "cache-control": "public, max-age=300",
                  },
                },
                "/install/": { redirect: "/install" },
              },
            }),
          ]),
    ],
  };
});
