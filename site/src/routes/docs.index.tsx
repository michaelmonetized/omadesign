import { createFileRoute, Link } from "@tanstack/react-router";
import { CURL } from "../site";
import { SOURCE_BUILD, RELEASE_SOURCE_URL, RELEASE_TAG, RELEASE_URL } from "../release";

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
              Grade a photo, copy its look across a shoot, and save reusable
              presets. Export or place the result into Design.
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
          <a href={RELEASE_URL}>{RELEASE_TAG} on GitHub Releases</a>.
        </p>
        <pre aria-label="Install omadesign">
          <code>{CURL}</code>
        </pre>
        <aside className="docs-note" aria-label="Version information">
          <p>
            {RELEASE_TAG} includes layered interchange, RAW development, batch
            photo adjustments and shareable presets. Affinity import also needs the optional{" "}
            <Link to="/docs/affinity">bridge setup</Link>. Read the{" "}
            <a href={RELEASE_URL}>release notes</a> for features and limitations.
          </p>
        </aside>
      </section>

      <section id="development-preview" aria-labelledby="build-from-source">
        <h2 id="build-from-source">Build from source</h2>
        <p>
          Build the same <a href={RELEASE_SOURCE_URL}>{RELEASE_TAG} source</a> used
          for the published packages. On Linux, install
          Git, stable Rust, a C/C++ compiler and pkg-config, then run:
        </p>
        <pre aria-label="Build the release from source"><code>{SOURCE_BUILD}</code></pre>
        <p>
          Open a camera file in Photo, develop it, and use <strong>Save settings</strong>{" "}
          to keep adjustments beside the original. Export PNG or TIFF for 16-bit
          developed pixels, or JPEG for sharing.
        </p>
        <p>
          Check the <Link to="/docs/formats">format support guide</Link> before
          importing layered work. Affinity documents also need the optional{" "}
          <Link to="/docs/affinity">Affinity bridge setup</Link>. Keep your source
          documents: unsupported features can be converted or omitted, with notes.
        </p>
      </section>

      <section aria-labelledby="make-something">
        <h2 id="make-something">Templates and resources</h2>
        <p>
          Open the{" "}
          <Link to="/docs/manual" hash="templates">
            template library
          </Link>{" "}
          for an editable starting point, or explore the{" "}
          <a href={`${RELEASE_SOURCE_URL}/examples/fieldwork`}>
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
