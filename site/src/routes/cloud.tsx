import { createFileRoute } from "@tanstack/react-router";
import { SignInButton, UserButton } from "@clerk/tanstack-react-start";
import {
  Authenticated,
  Unauthenticated,
  AuthLoading,
  useMutation,
  useQuery,
} from "convex/react";
import { useState } from "react";
import { api } from "../../convex/_generated/api";
import type { Id } from "../../convex/_generated/dataModel";
import { ProjectView } from "../cloud/project-view";
import { message } from "../cloud/files";
import "../cloud/cloud.css";
export const Route = createFileRoute("/cloud")({
  validateSearch: (s: Record<string, unknown>) => ({
    project: typeof s.project === "string" ? s.project : undefined,
    device: typeof s.device === "string" ? s.device : undefined,
  }),
  head: () => ({ meta: [{ title: "Cloud · omadesign" }] }),
  component: Cloud,
});
function Cloud() {
  return (
    <main id="main" className="cloud-app shell">
      <div className="cloud-app-heading">
        <div>
          <p className="eyebrow">OMADESIGN / CLOUD</p>
          <h1>Your work, shared.</h1>
        </div>
        <UserButton />
      </div>
      <AuthLoading>
        <p role="status">Connecting to your workspace…</p>
      </AuthLoading>
      <Unauthenticated>
        <div className="cloud-empty">
          <h2>A place for projects and feedback.</h2>
          <p>
            Share project files with your team. Review flat exports with
            clients. Publish finished work when you’re ready.
          </p>
          <SignInButton mode="modal">
            <button className="button">Sign in to cloud ↗</button>
          </SignInButton>
        </div>
      </Unauthenticated>
      <Authenticated>
        <Workspace />
      </Authenticated>
    </main>
  );
}
function Workspace() {
  const search = Route.useSearch();
  const navigate = Route.useNavigate();
  const projects = useQuery(api.projects.list, {});
  const invites = useQuery(api.projects.invitations, {});
  const create = useMutation(api.projects.create);
  const accept = useMutation(api.projects.accept);
  const [title, setTitle] = useState("");
  const [notice, setNotice] = useState("");
  const [busy, setBusy] = useState(false);
  if (search.device) return <DeviceApproval code={search.device} />;
  if (search.project)
    return (
      <ProjectView
        id={search.project as Id<"cloudProjects">}
        onBack={() =>
          navigate({ search: { project: undefined, device: undefined } })
        }
      />
    );
  return (
    <>
      <div className="cloud-app-toolbar">
        <h2>Projects</h2>
        <a href="/account">Connected devices ↗</a>
      </div>
      <form
        className="cloud-create"
        onSubmit={async (e) => {
          e.preventDefault();
          setBusy(true);
          setNotice("");
          try {
            const id = await create({ title });
            setTitle("");
            await navigate({ search: { project: id, device: undefined } });
          } catch (e) {
            setNotice(message(e));
          } finally {
            setBusy(false);
          }
        }}
      >
        <label htmlFor="project-title" className="cloud-sr-only">
          Project name
        </label>
        <input
          id="project-title"
          value={title}
          onChange={(e) => setTitle(e.target.value)}
          placeholder="Name a new project"
          maxLength={200}
          required
        />
        <button className="button" disabled={busy}>
          {busy ? "Creating…" : "Create project +"}
        </button>
      </form>
      <p role="status">{notice}</p>
      {!!invites?.length && (
        <section className="cloud-invites">
          <h3>Invitations</h3>
          {invites.map((i) => (
            <article key={i._id}>
              <span>
                <strong>{i.title}</strong> · {i.role}
              </span>
              <button
                onClick={async () => {
                  try {
                    await accept({ id: i._id });
                  } catch (e) {
                    setNotice(message(e));
                  }
                }}
              >
                Accept invitation ↗
              </button>
            </article>
          ))}
        </section>
      )}
      <div className="cloud-projects">
        {projects?.map((p) => (
          <button
            className="cloud-project-card"
            key={p._id}
            onClick={() =>
              navigate({ search: { project: p._id, device: undefined } })
            }
          >
            <span>{p.role}</span>
            <h3>{p.title}</h3>
            <p>Updated {new Date(p.updated).toLocaleDateString()}</p>
            <span>Open project ↗</span>
          </button>
        ))}
      </div>
      {projects?.length === 0 && (
        <p className="cloud-empty">
          Create your first project, then add a design file, assets, and flat
          exports for review.
        </p>
      )}
    </>
  );
}
function DeviceApproval({ code }: { code: string }) {
  const approve = useMutation(api.devices.approve);
  const [status, setStatus] = useState("");
  const [done, setDone] = useState(false);
  return (
    <div className="cloud-empty">
      <h2>Connect your desktop</h2>
      <p>
        Continue only if this code matches the code shown in Omadesign on your
        computer.
      </p>
      <code className="cloud-device-code">{code}</code>
      <button
        className="button"
        disabled={done}
        onClick={async () => {
          try {
            const d = await approve({ code });
            setDone(true);
            setStatus(`${d.label} is connected. Return to Omadesign.`);
          } catch (e) {
            setStatus(message(e));
          }
        }}
      >
        Connect this device
      </button>
      <p role="status">{status}</p>
    </div>
  );
}
