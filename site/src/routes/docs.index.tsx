import { createFileRoute, Link } from "@tanstack/react-router";
import { CURL } from "../site";

export const Route = createFileRoute("/docs/")({
  head: () => ({ meta: [{ title: "Getting started · omadesign" }] }),
  component: DocsOverview,
});

function DocsOverview() {
  return (
    <>
      <header>
        <h1>Getting started</h1>
        <p>
          omadesign brings vector design, painting, photo adjustments and motion
          into one native Linux studio. Save editable artwork in an{" "}
          <code>.oma</code> document.
        </p>
      </header>

      <section aria-labelledby="choose-a-persona">
        <h2 id="choose-a-persona">Studios</h2>
        <dl className="docs-personas">
          <div>
            <dt>Design</dt>
            <dd>
              Draw precise paths, edit type, arrange artboards and reshape
              vector artwork.
            </dd>
          </div>
          <div>
            <dt>Pixel</dt>
            <dd>
              Paint, select, clone and heal pixels. Use layer masks to hide and
              reveal your work.
            </dd>
          </div>
          <div>
            <dt>Photo</dt>
            <dd>
              Adjust light and color, compare edits and crop. Place the result
              back into Design.
            </dd>
          </div>
          <div>
            <dt>Motion</dt>
            <dd>
              Start with a preset, edit its keys and animate your artwork on the
              timeline.
            </dd>
          </div>
        </dl>
        <p>
          The <Link to="/docs/manual">user manual</Link> walks through each
          persona. Press <kbd>F1</kbd> in the app for shortcuts; the bottom hint
          strip follows your current tool and the modifiers you hold.
        </p>
      </section>

      <section aria-labelledby="get-the-app">
        <h2 id="get-the-app">Install</h2>
        <p>
          The installer chooses the published Linux package for your machine and
          adds a desktop entry. You can also download a package from{" "}
          <a href="https://github.com/michaelmonetized/omadesign/releases">
            GitHub Releases
          </a>
          .
        </p>
        <pre aria-label="Install omadesign">
          <code>{CURL}</code>
        </pre>
        <aside className="docs-note" aria-label="Version information">
          <p>
            This guide describes the current source. The published installer
            packages are older and may not yet include every tool shown here.
            See the{" "}
            <Link to="/docs/contributing">source build instructions</Link> for
            the latest work, and check the release notes for your installed
            version.
          </p>
        </aside>
      </section>

      <section aria-labelledby="make-something">
        <h2 id="make-something">Templates and resources</h2>
        <p>
          Open the{" "}
          <Link to="/docs/manual" hash="templates">
            template library
          </Link>{" "}
          for an editable starting point, or explore the{" "}
          <a href="https://github.com/michaelmonetized/omadesign/tree/master/examples/fieldwork">
            Fieldwork example kit
          </a>{" "}
          to try portable palettes and reusable brand artwork.
        </p>
        <p>
          Read the <Link to="/docs/roadmap">project status</Link> for current
          limits, or the <Link to="/docs/contributing">contributing guide</Link>{" "}
          to help with development.
        </p>
      </section>
    </>
  );
}
