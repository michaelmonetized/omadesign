import { createFileRoute, Link, Outlet } from "@tanstack/react-router";
import { RELEASE_TAG, RELEASE_SOURCE_URL } from "../release";

export const Route = createFileRoute("/docs")({ component: Docs });

function Docs() {
  return (
    <main id="main" className="docs-layout">
      <aside className="docs-nav">
        <nav aria-label="Documentation">
          <p className="docs-nav-title">A little guidance.</p>
          <ul>
            <li>
              <Link to="/docs" activeOptions={{ exact: true }}>
                Overview
              </Link>
            </li>
            <li>
              <Link to="/docs/manual">User manual</Link>
            </li>
            <li>
              <Link to="/docs/formats">File formats</Link>
            </li>
            <li>
              <Link to="/docs/affinity">Affinity setup</Link>
            </li>
            <li>
              <Link to="/docs/contributing">Contributing</Link>
            </li>
            <li>
              <Link to="/docs/roadmap">Project status</Link>
            </li>
          </ul>
        </nav>
        <nav aria-label="Project resources">
          <p className="docs-nav-title">Keep exploring</p>
          <ul>
            <li>
              <a href={`${RELEASE_SOURCE_URL}/examples/fieldwork`}>
                Fieldwork brand kit ↗
              </a>
            </li>
            <li>
              <a href={`${RELEASE_SOURCE_URL}/docs/template-drops.md`}>
                Template collection notes ↗
              </a>
            </li>
            <li>
              <a href="https://github.com/michaelmonetized/omadesign/issues">
                Issues & ideas ↗
              </a>
            </li>
          </ul>
        </nav>
      </aside>
      <article
        className="docs-content prose"
        aria-label="Documentation article"
      >
        <aside className="docs-note docs-availability" aria-label="Documentation version">
          <p>
            These guides cover {RELEASE_TAG}, including layered interchange,
            RAW development and saved photo settings. Get the{" "}
            <Link to="/docs" hash="get-the-app">Linux release</Link>.
            {" "}Affinity import needs the optional{" "}
            <Link to="/docs/affinity">bridge setup</Link>.
          </p>
        </aside>
        <Outlet />
      </article>
    </main>
  );
}
