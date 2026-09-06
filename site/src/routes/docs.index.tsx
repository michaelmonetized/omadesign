import { createFileRoute, Link } from "@tanstack/react-router";

export const Route = createFileRoute("/docs/")({
  head: () => ({ meta: [{ title: "Getting started · omadesign" }] }),
  component: DocsOverview,
});

function DocsOverview() {
  return (
    <>
      <header>
        <p className="eyebrow">The studio guide</p>
        <h1>
          A place to start.
          <br />
          Room to find your way.
        </h1>
        <p>
          omadesign brings vector design, painting, photo adjustments and motion
          into one native Linux studio. Start small, learn the tools as you go,
          and keep your artwork in an editable <code>.oma</code> document.
        </p>
      </header>

      <section aria-labelledby="choose-a-persona">
        <h2 id="choose-a-persona">Four ways to make.</h2>
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
              Adjust light and colour, compare edits and crop. Place the result
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
        <h2 id="get-the-app">Get the app.</h2>
        <p>
          The installer chooses the published Linux package for your machine and
          adds a desktop entry. You can also download a package from{" "}
          <a href="https://github.com/michaelmonetized/omadesign/releases">
            GitHub Releases
          </a>
          .
        </p>
        <pre aria-label="Install omadesign">
          <code>
            {
              "curl -fsSL https://raw.githubusercontent.com/michaelmonetized/omadesign/master/scripts/install-remote.sh | sh"
            }
          </code>
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
        <h2 id="make-something">Make something yours.</h2>
        <p>
          Open the template library for an editable starting point, or explore
          the{" "}
          <a href="https://github.com/michaelmonetized/omadesign/tree/master/examples/fieldwork">
            Fieldwork example kit
          </a>{" "}
          to try portable palettes and reusable brand artwork.
        </p>
        <p>
          Curious about what is still taking shape? Read the{" "}
          <Link to="/docs/roadmap">project status</Link>. Found something worth
          fixing? The <Link to="/docs/contributing">contributing guide</Link> is
          a good next step.
        </p>
      </section>
    </>
  );
}
