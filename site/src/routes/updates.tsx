import { createFileRoute, Link } from "@tanstack/react-router";
import pixel from "../../../docs/blog/0.5.1-pixel-is-real.md?raw";
import layout from "../../../docs/blog/0.5.0-layout-cloud.md?raw";
import { Markdown } from "../md";
import { CURL } from "../site";
import { latestUpdate, updates } from "../updates";

const bodies: Record<string, { markdown: string; path: string }> = {
  "0.5.1": { markdown: pixel, path: "docs/blog/0.5.1-pixel-is-real.md" },
  "0.5.0": { markdown: layout, path: "docs/blog/0.5.0-layout-cloud.md" },
};

export const Route = createFileRoute("/updates")({
  head: () => ({
    meta: [
      { title: "Updates · omadesign" },
      {
        name: "description",
        content:
          "Studio notes from omadesign. 0.5.1 makes Pixel selections real. 0.5.0 shipped Layout.",
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
