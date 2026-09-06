/// <reference types="vite/client" />
import {
  HeadContent,
  Outlet,
  Scripts,
  createRootRoute,
} from "@tanstack/react-router";
import { ThemeProvider, useTheme } from "../theme";
import { REPO, sitePath } from "../site";
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
          "An Omarchy-first creative suite for Linux. Native Rust. Design, paint, retouch and animate, with portable palettes, brand assets and fonts. Free and open source.",
      },
      { name: "theme-color", content: "#1e1e2e" },
      {
        property: "og:title",
        content: "omadesign — native Linux creative suite",
      },
      {
        property: "og:description",
        content:
          "Omarchy first. Native Rust. Design, Pixel, Photo, and Motion, with portable colors, assets, and fonts. Free and open source for ARM64 and x86_64 Linux.",
      },
      { property: "og:type", content: "website" },
      {
        property: "og:image",
        content:
          "https://michaelmonetized.github.io/omadesign/media/studio/omadesign-logo.webp",
      },
      { name: "twitter:card", content: "summary_large_image" },
    ],
    links: [
      { rel: "stylesheet", href: appCss },
      {
        rel: "icon",
        type: "image/svg+xml",
        href: sitePath("media/showcase/favicon.svg"),
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
    <ThemeProvider>
      <Document />
    </ThemeProvider>
  ),
});

function Document() {
  const { theme, setTheme } = useTheme();
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
                src={sitePath(
                  `media/showcase/wordmark-on-${theme === "mocha" ? "dark" : "light"}.svg`,
                )}
                alt="omadesign"
                width="138"
                height="40"
              />
            </a>
            <nav aria-label="Main navigation">
              <a className="nav-features" href={sitePath("#features")}>
                Features
              </a>
              <a href={sitePath("docs")}>Docs</a>
              <a href={REPO}>
                GitHub <span aria-hidden="true">↗</span>
              </a>
            </nav>
            <div className="header-actions">
              <button
                className="theme-toggle"
                type="button"
                aria-label={`Use ${theme === "mocha" ? "light" : "dark"} theme`}
                onClick={() => setTheme(theme === "mocha" ? "latte" : "mocha")}
              >
                <svg
                  viewBox="0 0 24 24"
                  fill="none"
                  stroke="currentColor"
                  strokeWidth="1.5"
                  aria-hidden="true"
                >
                  <circle cx="12" cy="12" r="8" />
                  <path d="M12 4a8 8 0 0 1 0 16Z" fill="currentColor" />
                </svg>
              </button>
              <a className="button button-small" href={sitePath("#install")}>
                Get omadesign <span aria-hidden="true">↗</span>
              </a>
            </div>
          </div>
        </header>
        <Outlet />
        <footer className="site-footer">
          <div className="shell footer-top">
            <a className="wordmark" href={sitePath()}>
              <img
                src={sitePath(
                  `media/showcase/wordmark-on-${theme === "mocha" ? "dark" : "light"}.svg`,
                )}
                alt="omadesign"
                width="117"
                height="34"
              />
            </a>
            <a href="#top">Back to top ↑</a>
          </div>
          <div className="shell footer-bottom">
            <span>
              © {new Date().getFullYear()} omadesign contributors · MIT licensed
            </span>
            <div>
              <a href={sitePath("docs/roadmap")}>Roadmap</a>
              <a href={sitePath("docs/contributing")}>Contribute</a>
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
