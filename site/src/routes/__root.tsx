/// <reference types="vite/client" />
import {
  HeadContent,
  Outlet,
  Scripts,
  createRootRoute,
} from "@tanstack/react-router";
import { CloudProvider } from "../cloud/provider";
import { useEffect } from "react";
import { ThemeProvider, useTheme } from "../theme";
import { CloudIcon, DiscordIcon, DownloadIcon, GitHubIcon, SparkleIcon } from "../components/nav-icons";
import { SectionRuler } from "../components/section-ruler";
import { DISCORD, REPO, SITE_ORIGIN, sitePath } from "../site";
import appCss from "../styles.css?url";

export { CURL } from "../site";

export const Route = createRootRoute({
  head: () => ({
    meta: [
      { charSet: "utf-8" },
      { name: "viewport", content: "width=device-width, initial-scale=1" },
      { title: "omadesign — native Linux creative suite" },
      {
        name: "description",
        content:
          "Design, layout, paint, develop and animate in one native Linux studio. Editable gradients, responsive prototypes, photo batches and file workflows for AI agents. Free and open source.",
      },
      { name: "theme-color", content: "#1e1e2e" },
      {
        property: "og:title",
        content: "omadesign — native Linux creative suite",
      },
      {
        property: "og:description",
        content:
          "Omarchy first. Native Rust. Design, Layout, Pixel, Photo, and Motion, with portable colors, assets, and fonts. Free and open source for ARM64 and x86_64 Linux.",
      },
      { property: "og:type", content: "website" },
      { property: "og:url", content: SITE_ORIGIN },
      {
        property: "og:image",
        content: `${SITE_ORIGIN}/media/branding/logo-0.5.8-social.png`,
      },
      { name: "twitter:card", content: "summary_large_image" },
    ],
    links: [
      { rel: "stylesheet", href: appCss },
      {
        rel: "icon",
        type: "image/svg+xml",
        href: sitePath("media/branding/logo-0.5.8.svg"),
      },
      { rel: "preconnect", href: "https://fonts.googleapis.com" },
      {
        rel: "preconnect",
        href: "https://fonts.gstatic.com",
        crossOrigin: "anonymous",
      },
      {
        rel: "stylesheet",
        href: "https://fonts.googleapis.com/css2?family=DM+Sans:wght@400;450;500;550;600;650;700&family=IBM+Plex+Mono:wght@400;500&display=swap",
      },
    ],
  }),
  component: () => (
    <CloudProvider><ThemeProvider>
      <Document />
    </ThemeProvider></CloudProvider>
  ),
});

function Document() {
  const { theme, setTheme } = useTheme();
  useEffect(() => {
    if (window.location.hostname === "michaelmonetized.github.io") {
      const path = window.location.pathname.replace(/^\/omadesign/, "") || "/";
      window.location.replace(
        `${SITE_ORIGIN}${path}${window.location.search}${window.location.hash}`,
      );
    }
  }, []);
  return (
    <html
      lang="en-US"
      id="top"
      className={theme === "latte" ? "latte" : "mocha dark"}
    >
      <head>
        <HeadContent />
      </head>
      <body>
        <a className="skip-link" href="#main">
          Skip to content
        </a>
        <header className="site-header">
          <div className="shell header-inner">
            <a
              href={sitePath()}
              className="wordmark"
              aria-label="omadesign home"
            >
              <img
                src={sitePath("media/branding/logo-0.5.8.svg")}
                alt="omadesign"
                width="160"
                height="90"
              />
            </a>
            <nav className="header-links" aria-label="Main navigation">
              <a href={sitePath("#features")}>Features</a>
              <a href={sitePath("updates")}>Updates</a>
              <a href={sitePath("showcase")}>Showcase</a>
              <a href={sitePath("docs")}>Docs</a>
            </nav>
            <div className="header-icons">
              <a href={DISCORD} aria-label="Discord"><DiscordIcon /></a>
              <a href={REPO} aria-label="GitHub"><GitHubIcon /></a>
              <a href={sitePath("#install")} aria-label="Download"><DownloadIcon /></a>
              <a href={sitePath("cloud")} aria-label="Cloud"><CloudIcon /></a>
              <a href={sitePath("#ai")} aria-label="AI workflows"><SparkleIcon /></a>
            </div>
          </div>
        </header>
        <Outlet />
        <SectionRuler />
        <button
          className="theme-toggle"
          type="button"
          aria-label={`Use ${theme === "mocha" ? "light" : "dark"} theme`}
          onClick={() => setTheme(theme === "mocha" ? "latte" : "mocha")}
        >
          <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="1.5" aria-hidden="true">
            <circle cx="12" cy="12" r="8" />
            <path d="M12 4a8 8 0 0 1 0 16Z" fill="currentColor" />
          </svg>
        </button>
        <footer className="site-footer">
          <div className="shell footer-top">
            <a className="wordmark" href={sitePath()}>
              <img
                src={sitePath("media/branding/logo-0.5.8.svg")}
                alt="omadesign"
                width="160"
                height="90"
              />
            </a>
            <a href="#top">Back to top ↑</a>
          </div>
          <div className="shell footer-bottom">
            <span>
              © {new Date().getFullYear()} omadesign contributors · MIT licensed
            </span>
            <div>
              <a href={sitePath("updates")}>Updates</a>
              <a href={sitePath("compete")}>Compete</a>
              <a href={sitePath("docs/roadmap")}>Roadmap</a>
              <a href={sitePath("docs/contributing")}>Contribute</a>
              <a href={DISCORD}>Join the conversation</a>
              <a href={`${REPO}/issues`}>Report a bug</a>
              <a href={`${REPO}/blob/master/LICENSE`}>License</a>
            </div>
            <span className="mono">Rust · Linux · FOSS</span>
          </div>
        </footer>
        <Scripts />
      </body>
    </html>
  );
}
