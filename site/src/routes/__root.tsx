/// <reference types="vite/client" />
import { HeadContent, Outlet, Scripts, createRootRoute } from "@tanstack/react-router";
import type { ReactNode } from "react";
import { ThemeProvider, useFlavour } from "../theme";
import appCss from "../styles.css?url";

const CURL =
  "curl -fsSL https://raw.githubusercontent.com/michaelmonetized/omadesign/master/scripts/install-remote.sh | sh";

export const Route = createRootRoute({
  head: () => ({
    meta: [
      { charSet: "utf-8" },
      { name: "viewport", content: "width=device-width, initial-scale=1" },
      { title: "omadesign — your Linux, for making things" },
      {
        name: "description",
        content:
          "Native Linux studio for design, paint, and photograph. No Electron. Type you can type into. Theme from ~/.config.",
      },
      { name: "theme-color", content: "#1e1e2e" },
      { property: "og:title", content: "omadesign" },
      {
        property: "og:description",
        content: "A native Linux studio. Design, paint, photograph.",
      },
      { property: "og:image", content: "/media/design.jpg" },
    ],
    links: [
      { rel: "stylesheet", href: appCss },
      {
        rel: "stylesheet",
        href: "https://fonts.googleapis.com/css2?family=IBM+Plex+Sans:wght@400;500;600;700&family=JetBrains+Mono:wght@400;500&display=swap",
      },
    ],
  }),
  component: Root,
});

function Root() {
  return (
    <ThemeProvider>
      <RootDocument>
        <Outlet />
      </RootDocument>
    </ThemeProvider>
  );
}

function RootDocument({ children }: { children: ReactNode }) {
  const { flavour, setFlavour } = useFlavour();
  return (
    <html lang="en" className={flavour === "latte" ? "latte" : "mocha dark"}>
      <head>
        <HeadContent />
      </head>
      <body className="bg-ctp-base text-ctp-text antialiased">
        <header className="sticky top-0 z-30 border-b border-ctp-surface0/80 bg-ctp-base/80 backdrop-blur-md">
          <div className="mx-auto flex max-w-6xl items-center justify-between px-6 py-4">
            <a href="/" className="font-semibold tracking-tight text-ctp-text">
              omadesign
            </a>
            <nav className="flex items-center gap-5 text-sm text-ctp-subtext0">
              <a href="/docs" className="hover:text-ctp-text">
                Docs
              </a>
              <a href="/docs/roadmap" className="hover:text-ctp-text">
                Roadmap
              </a>
              <a
                href="https://github.com/michaelmonetized/omadesign"
                className="hover:text-ctp-text"
              >
                GitHub
              </a>
              <button
                type="button"
                onClick={() => setFlavour(flavour === "mocha" ? "latte" : "mocha")}
                className="rounded-full border border-ctp-overlay0 px-3 py-1 text-xs text-ctp-subtext1 hover:border-ctp-lavender"
              >
                {flavour === "mocha" ? "Light" : "Dark"}
              </button>
            </nav>
          </div>
        </header>
        {children}
        <footer className="border-t border-ctp-surface0">
          <div className="mx-auto flex max-w-6xl flex-col gap-3 px-6 py-10 text-sm text-ctp-overlay1 md:flex-row md:items-center md:justify-between">
            <p>MIT · native Linux · no GitHub Actions bill</p>
            <code className="rounded-md bg-ctp-mantle px-3 py-1 font-mono text-xs text-ctp-subtext0">
              {CURL}
            </code>
          </div>
        </footer>
        <Scripts />
      </body>
    </html>
  );
}

export { CURL };
