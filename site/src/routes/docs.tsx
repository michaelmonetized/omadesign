import { createFileRoute, Link, Outlet } from "@tanstack/react-router";

export const Route = createFileRoute("/docs")({
  component: Docs,
});

function Docs() {
  return (
    <div className="mx-auto grid max-w-6xl gap-10 px-6 py-12 md:grid-cols-[200px_1fr]">
      <aside className="text-sm text-ctp-subtext1">
        <p className="mb-3 font-medium text-ctp-text">Docs</p>
        <ul className="space-y-2">
          <li>
            <Link to="/docs" className="hover:text-ctp-lavender">
              Overview
            </Link>
          </li>
          <li>
            <Link to="/docs/manual" className="hover:text-ctp-lavender">
              User manual
            </Link>
          </li>
          <li>
            <Link to="/docs/contributing" className="hover:text-ctp-lavender">
              Contributing
            </Link>
          </li>
          <li>
            <Link to="/docs/roadmap" className="hover:text-ctp-lavender">
              Roadmap
            </Link>
          </li>
        </ul>
      </aside>
      <article className="prose prose-invert max-w-none prose-headings:text-ctp-text prose-p:text-ctp-subtext0 prose-a:text-ctp-lavender prose-code:text-ctp-green prose-pre:bg-ctp-mantle">
        <Outlet />
      </article>
    </div>
  );
}
