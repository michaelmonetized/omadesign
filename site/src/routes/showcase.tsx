import {
  createFileRoute,
  Outlet,
  useRouterState,
} from "@tanstack/react-router";
import { useQuery } from "convex/react";
import { api } from "../../convex/_generated/api";
import "../cloud/cloud.css";
export const Route = createFileRoute("/showcase")({ component: Showcase });
function Showcase() {
  const works = useQuery(api.showcase.list, {});
  const pathname = useRouterState({ select: (s) => s.location.pathname });
  if (pathname !== "/showcase" && pathname !== "/showcase/") return <Outlet />;
  return (
    <main id="main" className="cloud-app shell">
      <div className="cloud-app-heading">
        <div>
          <p className="eyebrow">MADE IN OMADESIGN</p>
          <h1>The showcase.</h1>
        </div>
        <a className="text-link" href="/cloud">
          Share your work ↗
        </a>
      </div>
      <p>Finished work, published by its creators.</p>
      <div className="cloud-projects cloud-showcase">
        {works?.map((w) => (
          <article className="cloud-project-card" key={w._id}>
            {w.image && <img src={w.image} alt={w.title} loading="lazy" />}
            <h2>
              <a href={`/showcase/${w._id}`}>{w.title} ↗</a>
            </h2>
            <p>{w.description}</p>
            <p>By {w.author}</p>
          </article>
        ))}
      </div>
      {works?.length === 0 && (
        <p className="cloud-empty">
          The next piece here could be yours. Publish a finished flat export
          from a cloud project.
        </p>
      )}
    </main>
  );
}
