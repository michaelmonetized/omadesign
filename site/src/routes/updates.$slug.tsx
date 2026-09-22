import { createFileRoute, Link } from "@tanstack/react-router";
import welcome from "../../../docs/blog/0.5.6-your-work-ready.md?raw";
import graphics from "../../../docs/blog/0.5.4-more-control-on-the-canvas.md?raw";
import clipboard from "../../../docs/blog/0.5.3-paste-onto-the-canvas.md?raw";
import pixel from "../../../docs/blog/0.5.1-pixel-is-real.md?raw";
import layout from "../../../docs/blog/0.5.0-layout-cloud.md?raw";
import { Markdown } from "../md";
import { updateBySlug } from "../updates";

const bodies: Record<string, { markdown: string; path: string }> = {
  "0.5.6": { markdown: welcome, path: "docs/blog/0.5.6-your-work-ready.md" },
  "0.5.4": { markdown: graphics, path: "docs/blog/0.5.4-more-control-on-the-canvas.md" },
  "0.5.3": { markdown: clipboard, path: "docs/blog/0.5.3-paste-onto-the-canvas.md" },
  "0.5.1": { markdown: pixel, path: "docs/blog/0.5.1-pixel-is-real.md" },
  "0.5.0": { markdown: layout, path: "docs/blog/0.5.0-layout-cloud.md" },
};

export const Route = createFileRoute("/updates/$slug")({
  head: ({ params }) => {
    const post = updateBySlug(params.slug);
    return {
      meta: [
        {
          title: post
            ? `${post.title} · omadesign ${post.version}`
            : "Update · omadesign",
        },
        {
          name: "description",
          content: post?.dek ?? "A studio note from omadesign.",
        },
      ],
    };
  },
  component: UpdatePost,
});

function UpdatePost() {
  const { slug } = Route.useParams();
  const post = updateBySlug(slug);
  const body = bodies[slug];
  if (!post || !body) {
    return (
      <main id="main" className="section shell">
        <h1>Not in the log</h1>
        <p className="muted">That note is not on this site.</p>
        <p>
          <Link to="/updates">Back to updates</Link>
        </p>
      </main>
    );
  }
  return (
    <main id="main" className="section shell updates-page">
      <p className="gallery-kicker">
        <Link to="/updates">Updates</Link>
        <span aria-hidden="true"> · </span>
        v{post.version}
      </p>
      <p className="update-meta">
        <time dateTime={post.date}>{post.date}</time>
      </p>
      <article className="update-feature prose docs-content">
        <Markdown source={body.markdown} sourcePath={body.path} />
      </article>
    </main>
  );
}
