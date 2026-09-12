import { createFileRoute, Link } from "@tanstack/react-router";
import { useState, type FormEvent } from "react";
import { SITE_ORIGIN } from "../site";

export const Route = createFileRoute("/compete")({
  head: () => ({
    meta: [
      { title: "Best design · omadesign" },
      {
        name: "description",
        content: "A public showcase competition for native Linux design work. Waitlist is open.",
      },
    ],
  }),
  component: Compete,
});

function Compete() {
  const [email, setEmail] = useState("");
  const [name, setName] = useState("");
  const [status, setStatus] = useState("");
  const submit = async (event: FormEvent) => {
    event.preventDefault();
    setStatus("Sending…");
    try {
      const response = await fetch(`${SITE_ORIGIN}/api/cloud`, {
        method: "POST",
        headers: { "Content-Type": "application/json" },
        body: JSON.stringify({ action: "waitlist", email, name }),
      });
      if (!response.ok) throw new Error("failed");
      setStatus("You're on the list. We'll write when entries open.");
      setEmail("");
      setName("");
    } catch {
      setStatus("Cloud is still warming up. Email hello@omadesign.app and we'll add you.");
    }
  };
  return (
    <main id="main" className="section shell gallery-page">
      <div className="section-heading">
        <h1>Best design</h1>
        <p>A competition on the public showcase. Not day-one. The door is already here.</p>
      </div>
      <article className="rules">
        <h2>Rules</h2>
        <ol>
          <li>Made in omadesign. Native file, or a Layout frame export.</li>
          <li>You own the work. No stolen marks, no generated stock dumps.</li>
          <li>Publish is opt-in. Private files are not entries.</li>
          <li>Linux is the studio. The audience is public.</li>
          <li>Voting comes later. The waitlist is the first honest step.</li>
        </ol>
      </article>
      <form className="waitlist" onSubmit={submit}>
        <h2>Waitlist</h2>
        <p className="muted">
          No spam. One note when entries open. Clerk identity will bind the
          entry to you when cloud is fully live.
        </p>
        <label>
          Name
          <input value={name} onChange={(e) => setName(e.target.value)} required />
        </label>
        <label>
          Email
          <input
            type="email"
            value={email}
            onChange={(e) => setEmail(e.target.value)}
            required
          />
        </label>
        <button className="button" type="submit">
          Join the waitlist
        </button>
        {status ? <p className="muted">{status}</p> : null}
      </form>
      <p className="muted">
        See the <Link to="/showcase">showcase</Link> while you wait.
      </p>
    </main>
  );
}
