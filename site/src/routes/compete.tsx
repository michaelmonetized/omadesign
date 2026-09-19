import { createFileRoute } from "@tanstack/react-router";
import { useQuery, useMutation, Authenticated } from "convex/react";
import { api } from "../../convex/_generated/api";
import { useState } from "react";
import "../cloud/cloud.css";
export const Route = createFileRoute("/compete")({ component: Competitions });
function Competitions() {
  const contests = useQuery(api.showcase.competitions, {});
  return (
    <main id="main" className="cloud-app shell">
      <p className="eyebrow">OMADESIGN COMMUNITY</p>
      <h1>Make your mark.</h1>
      <p>Enter your published showcase work from its cloud project.</p>
      <div className="cloud-projects">
        {contests?.map((c) => (
          <article className="cloud-project-card" key={c._id}>
            <h2>{c.title}</h2>
            <p>{c.description}</p>
            <p>
              {new Date(c.opens).toLocaleDateString()} —{" "}
              {new Date(c.closes).toLocaleDateString()}
            </p>
            <p>
              {Date.now() < c.opens
                ? "Opening soon"
                : Date.now() > c.closes
                  ? "Entries closed"
                  : "Open for entries"}
            </p>
            <a className="text-link" href="/cloud">
              Choose a project ↗
            </a>
          </article>
        ))}
      </div>
      {contests?.length === 0 && (
        <p className="cloud-empty">
          No competitions are open yet. Your public showcase work will be ready
          to enter when one opens.
        </p>
      )}
      <Authenticated>
        <MyEntries />
      </Authenticated>
    </main>
  );
}
function MyEntries() {
  const entries = useQuery(api.showcase.entries, {});
  const withdraw = useMutation(api.showcase.withdraw);
  const [notice, setNotice] = useState("");
  return (
    <section>
      <h2>Your entries</h2>
      {entries?.map(
        (e) =>
          e && (
            <article className="cloud-file-list" key={e._id}>
              <span>Entered {new Date(e.created).toLocaleDateString()}</span>
              <button
                onClick={async () => {
                  try {
                    await withdraw({ id: e._id });
                    setNotice("Entry withdrawn.");
                  } catch {
                    setNotice("Could not withdraw entry.");
                  }
                }}
              >
                Withdraw entry
              </button>
            </article>
          ),
      )}
      <p role="status">{notice}</p>
    </section>
  );
}
