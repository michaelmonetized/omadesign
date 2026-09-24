import { createFileRoute, Link } from "@tanstack/react-router";
import pixelYouMean from "../../../docs/blog/0.5.9-click-the-pixel.md?raw";
import plugins from "../../../docs/blog/0.5.8-make-it-your-studio.md?raw";
import identity from "../../../docs/blog/0.5.7-a-new-mark.md?raw";
import welcome from "../../../docs/blog/0.5.6-your-work-ready.md?raw";
import graphics from "../../../docs/blog/0.5.4-more-control-on-the-canvas.md?raw";
import clipboard from "../../../docs/blog/0.5.3-paste-onto-the-canvas.md?raw";
import pixel from "../../../docs/blog/0.5.1-pixel-is-real.md?raw";
import layout from "../../../docs/blog/0.5.0-layout-cloud.md?raw";
import { Markdown } from "../md";
import { CURL } from "../site";
import { latestUpdate, updates } from "../updates";

const bodies: Record<string, { markdown: string; path: string }> = {
  "0.5.9": { markdown: pixelYouMean, path: "docs/blog/0.5.9-click-the-pixel.md" },
  "0.5.8": { markdown: plugins, path: "docs/blog/0.5.8-make-it-your-studio.md" },
  "0.5.7": { markdown: identity, path: "docs/blog/0.5.7-a-new-mark.md" },
  "0.5.6": { markdown: welcome, path: "docs/blog/0.5.6-your-work-ready.md" },
  "0.5.4": { markdown: graphics, path: "docs/blog/0.5.4-more-control-on-the-canvas.md" },
  "0.5.3": { markdown: clipboard, path: "docs/blog/0.5.3-paste-onto-the-canvas.md" },
  "0.5.1": { markdown: pixel, path: "docs/blog/0.5.1-pixel-is-real.md" },
  "0.5.0": { markdown: layout, path: "docs/blog/0.5.0-layout-cloud.md" },
};

export const Route = createFileRoute("/updates/")({
  head: () => ({
    meta: [
      { title: "Updates · omadesign" },
      {
        name: "description",
        content:
          "Studio notes from omadesign. 0.5.9 samples any screen pixel, subtracts from a selection, and filters a page of vectors.",
      },
    ],
  }),
  component: UpdatesIndex,
});

function UpdatesIndex() {
  const featured = latestUpdate;
  const body = bodies[featured.slug];
  const earlier = updates.slice(1);
  return (
    <main id="main" className="section shell updates-page">
      <div className="section-heading">
        <div>
          <p className="gallery-kicker">Studio log</p>
          <h1>Updates</h1>
        </div>
        <p>
          What shipped, in the order you can use it. {featured.version} is on
          top. The curl line still installs the latest.
        </p>
      </div>
      <article className="update-feature prose docs-content">
        <p className="update-meta">
          <Link to="/updates/$slug" params={{ slug: featured.slug }}>
            v{featured.version}
          </Link>
          <span aria-hidden="true"> · </span>
          <time dateTime={featured.date}>{featured.date}</time>
        </p>
        {body ? (
          <Markdown source={body.markdown} sourcePath={body.path} />
        ) : null}
      </article>
      {earlier.length > 0 ? (
        <section className="update-archive" aria-labelledby="earlier-notes">
          <h2 id="earlier-notes">Earlier</h2>
          <ul>
            {earlier.map((post) => (
              <li key={post.slug}>
                <Link to="/updates/$slug" params={{ slug: post.slug }}>
                  <p className="gallery-kicker">v{post.version}</p>
                  <h3>{post.title}</h3>
                  <p>{post.dek}</p>
                </Link>
              </li>
            ))}
          </ul>
        </section>
      ) : null}
      <p className="update-install muted">
        Install stays one line.{" "}
        <code>{CURL}</code>
      </p>
    </main>
  );
}
