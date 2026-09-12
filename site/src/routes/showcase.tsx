import { createFileRoute, Link } from "@tanstack/react-router";
import { publishedShowcase, showcaseSeed } from "../cloud/seed";

export const Route = createFileRoute("/showcase")({
  head: () => ({
    meta: [
      { title: "Showcase · omadesign" },
      {
        name: "description",
        content: "Public Layout and Design work, published on purpose. Private by default.",
      },
    ],
  }),
  component: Showcase,
});

function Showcase() {
  const items = publishedShowcase(showcaseSeed);
  return (
    <main id="main" className="section shell gallery-page">
      <div className="section-heading">
        <h1>Showcase</h1>
        <p>
          Publish is opt-in. Nothing leaves your machine until you say so.
          These three are the 0.5.0 starters.
        </p>
      </div>
      <ul className="gallery-grid">
        {items.map((item) => (
          <li key={item.id}>
            <Link to="/showcase/$id" params={{ id: item.id }} className="gallery-card">
              <p className="gallery-kicker">{item.tags.join(" · ")}</p>
              <h2>{item.title}</h2>
              <p>{item.summary}</p>
              <span className="muted">{item.author}</span>
            </Link>
          </li>
        ))}
      </ul>
      <p className="gallery-foot muted">
        Unpublished documents stay private. Sign in on{" "}
        <Link to="/account">Account</Link>, enable cloud sync in the app, then
        File → Publish to showcase.
      </p>
    </main>
  );
}
