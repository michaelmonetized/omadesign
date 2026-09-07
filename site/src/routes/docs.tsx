import { createFileRoute, Link, Outlet } from "@tanstack/react-router";
import { RELEASE_TAG } from "../release";

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
              <a href="https://github.com/michaelmonetized/omadesign/tree/master/examples/fieldwork">
                Fieldwork brand kit ↗
              </a>
            </li>
            <li>
              <a href="https://github.com/michaelmonetized/omadesign/blob/master/docs/template-drops.md">
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
            These guides include the development preview. Layered interchange,
            RAW and saved photo settings require a{" "}
            <Link to="/docs" hash="development-preview">preview source build</Link>.
            {" "}Published downloads remain {RELEASE_TAG}.
          </p>
        </aside>
        <Outlet />
      </article>
    </main>
  );
}
