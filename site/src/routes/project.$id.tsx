import { createFileRoute, Link } from "@tanstack/react-router";
import { useState, type FormEvent } from "react";

export const Route = createFileRoute("/project/$id")({
  head: () => ({ meta: [{ title: "Project · omadesign" }] }),
  component: ProjectCollab,
});

type Pin = {
  id: string;
  author: string;
  body: string;
  resolved: boolean;
};

function ProjectCollab() {
  const { id } = Route.useParams();
  const [pins, setPins] = useState<Pin[]>([]);
  const [body, setBody] = useState("");
  const add = (event: FormEvent) => {
    event.preventDefault();
    if (!body.trim()) return;
    setPins((current) => [
      ...current,
      {
        id: String(current.length + 1),
        author: "You",
        body: body.trim(),
        resolved: false,
      },
    ]);
    setBody("");
  };
  return (
    <main id="main" className="section shell gallery-page">
      <p className="gallery-kicker">Collaborator view</p>
      <h1>Project {id}</h1>
      <p>
        Presence is best-effort. Pin a note, reply, resolve. The owner can
        delete. Report abuse to the owner first.
      </p>
      <form className="waitlist" onSubmit={add}>
        <label>
          Comment
          <input
            value={body}
            onChange={(e) => setBody(e.target.value)}
            placeholder="What should move?"
          />
        </label>
        <button className="button" type="submit">
          Add thread
        </button>
      </form>
      <ul className="comment-list">
        {pins.map((pin) => (
          <li key={pin.id} className={pin.resolved ? "resolved" : undefined}>
            <strong>{pin.author}</strong>
            <p>{pin.body}</p>
            <button
              type="button"
              onClick={() =>
                setPins((current) =>
                  current.map((item) =>
                    item.id === pin.id ? { ...item, resolved: !item.resolved } : item,
                  ),
                )
              }
            >
              {pin.resolved ? "Reopen" : "Resolve"}
            </button>
          </li>
        ))}
      </ul>
      <p className="muted">
        Desktop shows the unresolved count on Layout frames.{" "}
        <Link to="/account">Sign in</Link> so the pin has your name.
      </p>
    </main>
  );
}
