import { createFileRoute, Link } from "@tanstack/react-router";
import { showcaseSeed } from "../cloud/seed";

export const Route = createFileRoute("/showcase/$id")({
  head: ({ params }) => {
    const item = showcaseSeed.find((entry) => entry.id === params.id);
    return {
      meta: [{ title: `${item?.title ?? "Project"} · omadesign` }],
    };
  },
  component: ShowcaseItem,
});

function ShowcaseItem() {
  const { id } = Route.useParams();
  const item = showcaseSeed.find((entry) => entry.id === id && entry.published);
  if (!item) {
    return (
      <main id="main" className="section shell">
        <h1>Not public</h1>
        <p className="muted">That project is private, or it never existed.</p>
        <p>
          <Link to="/showcase">Back to the showcase</Link>
        </p>
      </main>
    );
  }
  return (
    <main id="main" className="section shell gallery-page">
      <p className="gallery-kicker">{item.tags.join(" · ")}</p>
      <h1>{item.title}</h1>
      <p className="hero-intro">{item.summary}</p>
      <p className="muted">by {item.author}</p>
      <div className="gallery-detail">
        <p>
          Open it in omadesign, switch to Layout, and the frames stay nested.
          Auto-layout and constraints travel with the file.
        </p>
        <p>
          Comments live on the frame. Resolve them in the inspector. Nothing
          here was published by accident.
        </p>
      </div>
      <p>
        <Link to="/showcase">All public work</Link>
      </p>
    </main>
  );
}
