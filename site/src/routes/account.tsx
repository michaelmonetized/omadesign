import { createFileRoute, Link } from "@tanstack/react-router";
import { useState, type FormEvent } from "react";

export const Route = createFileRoute("/account")({
  head: () => ({ meta: [{ title: "Account · omadesign" }] }),
  component: Account,
});

function Account() {
  const clerk = import.meta.env.VITE_CLERK_PUBLISHABLE_KEY as string | undefined;
  const [email, setEmail] = useState("");
  const [name, setName] = useState("");
  const [saved, setSaved] = useState("");
  const save = (event: FormEvent) => {
    event.preventDefault();
    localStorage.setItem(
      "omadesign-identity",
      JSON.stringify({ email, name, savedAt: Date.now() }),
    );
    setSaved("Saved in this browser. Match it in the app under File → Sign in.");
  };
  return (
    <main id="main" className="section shell gallery-page">
      <h1>Account</h1>
      <p className="hero-intro">
        Cloud is opt-in. Sign in here, then enable sync on a document. Private
        stays private.
      </p>
      {clerk ? (
        <p className="muted">
          Clerk is configured for this deploy. Use the hosted sign-in on the
          live domain.
        </p>
      ) : (
        <form className="waitlist" onSubmit={save}>
          <p className="muted">
            Clerk keys are not on this build yet. This identity is enough for
            desktop sync and comments until they are.
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
            Save identity
          </button>
          {saved ? <p className="muted">{saved}</p> : null}
        </form>
      )}
      <ul className="account-links">
        <li>
          <Link to="/showcase">Public showcase</Link>
        </li>
        <li>
          <Link to="/compete">Competition waitlist</Link>
        </li>
        <li>
          Invite a collaborator from File in the app. They need this same
          identity email.
        </li>
      </ul>
    </main>
  );
}
