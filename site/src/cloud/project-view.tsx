import { useAuth } from "@clerk/tanstack-react-start";
import { useConvex, useQuery, useMutation } from "convex/react";
import { useEffect, useState, type PointerEvent } from "react";
import { api } from "../../convex/_generated/api";
import type { Doc, Id } from "../../convex/_generated/dataModel";
import { upload, downloadBlob, saveBlob, message } from "./files";
export function ProjectView({
  id,
  onBack,
}: {
  id: Id<"cloudProjects">;
  onBack: () => void;
}) {
  const project = useQuery(api.projects.get, { projectId: id });
  const client = useConvex();
  const { getToken } = useAuth();
  const [busy, setBusy] = useState(false);
  const [notice, setNotice] = useState("");
  const [snapshot, setSnapshot] = useState<Id<"cloudFiles">>();
  const selected =
    project?.files.find((f) => f._id === snapshot && f.kind === "snapshot") ||
    project?.files.find((f) => f.kind === "snapshot");
  if (!project) return <p>Loading project…</p>;
  async function add(file: File, kind: "source" | "asset" | "snapshot") {
    setBusy(true);
    setNotice(`Uploading ${file.name}…`);
    try {
      const id = await upload(client, project!._id, file, kind);
      if (kind === "snapshot") setSnapshot(id);
      setNotice("File saved to the project.");
    } catch (e) {
      setNotice(message(e));
    } finally {
      setBusy(false);
    }
  }
  return (
    <>
      <button className="text-link" onClick={onBack}>
        ← All projects
      </button>
      <div className="cloud-app-toolbar">
        <h2>{project.title}</h2>
        <span className="cloud-badge">{project.role}</span>
      </div>
      {project.role !== "reviewer" && (
        <div className="cloud-upload-row">
          {(["source", "asset", "snapshot"] as const).map((kind) => (
            <label className="cloud-upload" key={kind}>
              <strong>
                {kind === "source"
                  ? "Design file"
                  : kind === "asset"
                    ? "Project asset"
                    : "Flat export for review"}{" "}
                +
              </strong>
              <span>
                {kind === "source"
                  ? ".oma · up to 100 MB"
                  : kind === "snapshot"
                    ? "PNG / JPEG / WebP · up to 20 MB"
                    : "Up to 100 MB"}
              </span>
              <input
                type="file"
                disabled={busy}
                accept={
                  kind === "source"
                    ? ".oma"
                    : kind === "snapshot"
                      ? "image/png,image/jpeg,image/webp"
                      : undefined
                }
                onChange={(e) => {
                  const f = e.target.files?.[0];
                  if (f) void add(f, kind);
                  e.target.value = "";
                }}
              />
            </label>
          ))}
        </div>
      )}
      <p className="cloud-notice" role="status">
        {notice}
      </p>
      <section className="cloud-review">
        <div className="cloud-app-toolbar">
          <h3>Review exports</h3>
          <select
            aria-label="Flat export version"
            value={selected?._id || ""}
            onChange={(e) => setSnapshot(e.target.value as Id<"cloudFiles">)}
          >
            <option value="" disabled>
              Select an export
            </option>
            {project.files
              .filter((f) => f.kind === "snapshot")
              .map((f) => (
                <option key={f._id} value={f._id}>
                  {f.name} · v{f.version}
                </option>
              ))}
          </select>
        </div>
        {selected ? (
          <Review key={selected._id} file={selected} />
        ) : (
          <p className="cloud-empty">
            Add a flat export to start a review. Comments stay attached to that
            version.
          </p>
        )}
      </section>
      {project.role !== "reviewer" && (
        <section>
          <h3>Design files & assets</h3>
          <div className="cloud-file-list">
            {project.files
              .filter((f) => f.kind !== "snapshot")
              .map((f) => (
                <article key={f._id}>
                  <div>
                    <strong>{f.name}</strong>
                    <p>
                      {f.kind} · v{f.version} ·{" "}
                      {(f.size / 1024 / 1024).toFixed(1)} MB
                    </p>
                  </div>
                  <button
                    onClick={async () => {
                      try {
                        saveBlob(
                          await downloadBlob(
                            f._id,
                            await getToken({ template: "convex" }),
                          ),
                          f.name,
                        );
                      } catch (e) {
                        setNotice(message(e));
                      }
                    }}
                  >
                    Download ↓
                  </button>
                </article>
              ))}
          </div>
        </section>
      )}
      {project.role === "owner" && (
        <>
          <Team id={id} />
          <Publish id={id} selected={selected} />
        </>
      )}
    </>
  );
}
function Review({ file }: { file: Doc<"cloudFiles"> }) {
  const { getToken } = useAuth();
  const threads = useQuery(api.review.list, {
    projectId: file.projectId,
    snapshotId: file._id,
  });
  const annotate = useMutation(api.review.annotate);
  const reply = useMutation(api.review.reply);
  const resolve = useMutation(api.review.resolve);
  const [url, setUrl] = useState("");
  const [notice, setNotice] = useState("");
  const [mode, setMode] = useState<"pin" | "rectangle">("pin");
  const [draft, setDraft] = useState<{
    x: number;
    y: number;
    endX?: number;
    endY?: number;
  }>();
  const [body, setBody] = useState("");
  const [selected, setSelected] = useState<Id<"cloudAnnotations">>();
  const [replies, setReplies] = useState<Record<string, string>>({});
  const [busy, setBusy] = useState(false);
  useEffect(() => {
    let active = true;
    let objectUrl = "";
    void getToken({ template: "convex" })
      .then((token) => downloadBlob(file._id, token))
      .then((blob) => {
        if (active) {
          objectUrl = URL.createObjectURL(blob);
          setUrl(objectUrl);
        }
      })
      .catch((e) => {
        if (active) setNotice(message(e));
      });
    return () => {
      active = false;
      if (objectUrl) URL.revokeObjectURL(objectUrl);
    };
  }, [file._id, getToken]);
  function point(e: PointerEvent<HTMLDivElement>) {
    const r = e.currentTarget.getBoundingClientRect();
    return {
      x: Math.min(1, Math.max(0, (e.clientX - r.left) / r.width)),
      y: Math.min(1, Math.max(0, (e.clientY - r.top) / r.height)),
    };
  }
  async function perform(fn: () => Promise<unknown>) {
    setBusy(true);
    setNotice("");
    try {
      await fn();
    } catch (e) {
      setNotice(message(e));
    } finally {
      setBusy(false);
    }
  }
  return (
    <>
      <div className="cloud-review-tools">
        <button
          aria-pressed={mode === "pin"}
          onClick={() => {
            setMode("pin");
            setDraft(undefined);
          }}
        >
          Pin comment
        </button>
        <button
          aria-pressed={mode === "rectangle"}
          onClick={() => {
            setMode("rectangle");
            setDraft(undefined);
          }}
        >
          Draw annotation
        </button>
        <button
          onClick={() =>
            void perform(async () =>
              saveBlob(
                await downloadBlob(
                  file._id,
                  await getToken({ template: "convex" }),
                ),
                file.name,
              ),
            )
          }
        >
          Download export ↓
        </button>
      </div>
      <div className="cloud-review-grid">
        <div>
          <div
            className="cloud-review-image"
            style={{ aspectRatio: `${file.width || 1}/${file.height || 1}` }}
            onPointerDown={(e) => {
              if ((e.target as HTMLElement).closest("button")) return;
              e.currentTarget.setPointerCapture(e.pointerId);
              setDraft(point(e));
            }}
            onPointerUp={(e) => {
              if (mode === "rectangle" && draft) {
                const p = point(e);
                setDraft({ ...draft, endX: p.x, endY: p.y });
              }
            }}
          >
            {url ? (
              <img
                src={url}
                alt={`Review export: ${file.name}`}
                draggable={false}
              />
            ) : (
              <p>Loading export…</p>
            )}
            {threads?.map((t, i) => (
              <button
                key={t._id}
                className={`cloud-pin ${t.resolved ? "resolved" : ""}`}
                style={{
                  left: `${t.x * 100}%`,
                  top: `${t.y * 100}%`,
                  ...(t.shape === "rectangle"
                    ? {
                        width: `${Math.abs((t.endX ?? t.x) - t.x) * 100}%`,
                        height: `${Math.abs((t.endY ?? t.y) - t.y) * 100}%`,
                        left: `${Math.min(t.x, t.endX ?? t.x) * 100}%`,
                        top: `${Math.min(t.y, t.endY ?? t.y) * 100}%`,
                      }
                    : {}),
                }}
                onClick={() => setSelected(t._id)}
                aria-label={`Comment ${i + 1}: ${t.body}`}
              >
                {i + 1}
              </button>
            ))}
            {draft && (
              <span
                className="cloud-pin draft"
                style={{ left: `${draft.x * 100}%`, top: `${draft.y * 100}%` }}
              >
                +
              </span>
            )}
          </div>
          <form
            className="cloud-comment-form"
            onSubmit={(e) => {
              e.preventDefault();
              if (!draft) return;
              void perform(async () => {
                await annotate({
                  snapshotId: file._id,
                  ...draft,
                  shape: mode,
                  body,
                });
                setDraft(undefined);
                setBody("");
              });
            }}
          >
            <label htmlFor="review-comment">
              {draft
                ? "Add your feedback"
                : "Select a point on the export to comment"}
            </label>
            <textarea
              id="review-comment"
              value={body}
              onChange={(e) => setBody(e.target.value)}
              maxLength={4000}
              required
              disabled={!draft}
            />
            <button className="button" disabled={!draft || busy}>
              Post comment
            </button>
            {draft && (
              <button type="button" onClick={() => setDraft(undefined)}>
                Cancel
              </button>
            )}
          </form>
        </div>
        <div className="cloud-threads">
          {threads?.map((t, i) => (
            <article key={t._id} data-selected={selected === t._id}>
              <div className="cloud-thread-heading">
                <strong>
                  {i + 1} · {t.authorName}
                </strong>
                <span>{t.resolved ? "Resolved" : "Open"}</span>
              </div>
              <p>{t.body}</p>
              {t.replies.map((r) => (
                <div className="cloud-reply" key={r._id}>
                  <strong>{r.authorName}</strong>
                  <p>{r.body}</p>
                </div>
              ))}
              <form
                onSubmit={(e) => {
                  e.preventDefault();
                  void perform(async () => {
                    await reply({ id: t._id, body: replies[t._id] || "" });
                    setReplies({ ...replies, [t._id]: "" });
                  });
                }}
              >
                <input
                  aria-label={`Reply to comment ${i + 1}`}
                  placeholder="Reply…"
                  maxLength={4000}
                  required
                  value={replies[t._id] || ""}
                  onChange={(e) =>
                    setReplies({ ...replies, [t._id]: e.target.value })
                  }
                />
                <button disabled={busy}>Reply</button>
              </form>
              <button
                disabled={busy}
                className="text-link"
                onClick={() =>
                  void perform(() =>
                    resolve({ id: t._id, resolved: !t.resolved }),
                  )
                }
              >
                {t.resolved ? "Reopen" : "Resolve"}
              </button>
            </article>
          ))}
          {threads?.length === 0 && (
            <p>
              No comments yet. Select a point or draw a rectangle on the export.
            </p>
          )}
        </div>
      </div>
      <p role="status" className="cloud-notice">
        {notice}
      </p>
    </>
  );
}
function Team({ id }: { id: Id<"cloudProjects"> }) {
  const data = useQuery(api.projects.members, { projectId: id });
  const invite = useMutation(api.projects.invite);
  const change = useMutation(api.projects.changeMember);
  const cancel = useMutation(api.projects.cancelInvite);
  const [email, setEmail] = useState("");
  const [role, setRole] = useState<"editor" | "reviewer">("reviewer");
  const [notice, setNotice] = useState("");
  const [busy, setBusy] = useState(false);
  return (
    <section className="cloud-team">
      <h3>People & access</h3>
      <p>
        Editors can share files. Reviewers see only flat exports and comments.
      </p>
      <form
        className="cloud-create"
        onSubmit={async (e) => {
          e.preventDefault();
          setBusy(true);
          try {
            await invite({ projectId: id, email, role });
            setEmail("");
            setNotice("Invitation sent.");
          } catch (e) {
            setNotice(message(e));
          } finally {
            setBusy(false);
          }
        }}
      >
        <input
          aria-label="Invite email"
          type="email"
          placeholder="teammate@example.com"
          value={email}
          onChange={(e) => setEmail(e.target.value)}
          required
        />
        <select
          aria-label="Invitation role"
          value={role}
          onChange={(e) => setRole(e.target.value as typeof role)}
        >
          <option value="reviewer">Reviewer</option>
          <option value="editor">Editor</option>
        </select>
        <button className="button" disabled={busy}>
          Invite ↗
        </button>
      </form>
      <div className="cloud-file-list">
        {data?.members.map((m) => (
          <article key={m._id}>
            <div>
              <strong>{m.name}</strong>
              <p>
                {m.email} · {m.role}
              </p>
            </div>
            {m.role !== "owner" && (
              <select
                aria-label={`Access for ${m.email}`}
                value={m.role}
                onChange={async (e) => {
                  try {
                    await change({
                      projectId: id,
                      userId: m.userId,
                      role: e.target.value as "editor" | "reviewer" | "remove",
                    });
                  } catch (e) {
                    setNotice(message(e));
                  }
                }}
              >
                <option value="editor">Editor</option>
                <option value="reviewer">Reviewer</option>
                <option value="remove">Remove access</option>
              </select>
            )}
          </article>
        ))}
        {data?.invites
          .filter((i) => !i.accepted && i.expires > Date.now())
          .map((i) => (
            <article key={i._id}>
              <span>
                {i.email} · {i.role} · Invited
              </span>
              <button
                onClick={async () => {
                  try {
                    await cancel({ id: i._id });
                  } catch (e) {
                    setNotice(message(e));
                  }
                }}
              >
                Cancel invitation
              </button>
            </article>
          ))}
      </div>
      <p role="status">{notice}</p>
    </section>
  );
}
function Publish({
  id,
  selected,
}: {
  id: Id<"cloudProjects">;
  selected?: Doc<"cloudFiles">;
}) {
  const items = useQuery(api.showcase.project, { projectId: id });
  const competitions = useQuery(api.showcase.competitions, {});
  const publish = useMutation(api.showcase.publish);
  const unpublish = useMutation(api.showcase.unpublish);
  const enter = useMutation(api.showcase.enter);
  const [title, setTitle] = useState("");
  const [description, setDescription] = useState("");
  const [notice, setNotice] = useState("");
  const [busy, setBusy] = useState(false);
  return (
    <section className="cloud-publish">
      <h3>Share finished work</h3>
      <p>
        Publishing makes the selected flat export public. Your project source
        and assets stay private.
      </p>
      {selected && (
        <form
          className="cloud-publish-form"
          onSubmit={async (e) => {
            e.preventDefault();
            setBusy(true);
            try {
              await publish({ snapshotId: selected._id, title, description });
              setNotice("Your work is public in the showcase.");
              setTitle("");
              setDescription("");
            } catch (e) {
              setNotice(message(e));
            } finally {
              setBusy(false);
            }
          }}
        >
          <p>
            Selected export:{" "}
            <strong>
              {selected.name} · v{selected.version}
            </strong>
          </p>
          <input
            aria-label="Showcase title"
            placeholder="Work title"
            value={title}
            onChange={(e) => setTitle(e.target.value)}
            maxLength={200}
            required
          />
          <textarea
            aria-label="Showcase description"
            placeholder="Tell us about the work"
            value={description}
            onChange={(e) => setDescription(e.target.value)}
            maxLength={2000}
            required
          />
          <button className="button" disabled={busy}>
            Publish selected export ↗
          </button>
        </form>
      )}
      <div className="cloud-file-list">
        {items
          ?.filter((i) => i.published)
          .map((i) => (
            <article key={i._id}>
              <strong>{i.title}</strong>
              <div>
                <button
                  onClick={async () => {
                    try {
                      await unpublish({ id: i._id });
                      setNotice(
                        "Work removed from public showcase and competition galleries.",
                      );
                    } catch (e) {
                      setNotice(message(e));
                    }
                  }}
                >
                  Unpublish
                </button>
                {competitions
                  ?.filter(
                    (c) => c.opens <= Date.now() && c.closes > Date.now(),
                  )
                  .map((c) => (
                    <button
                      key={c._id}
                      onClick={async () => {
                        try {
                          await enter({
                            competitionId: c._id,
                            showcaseId: i._id,
                          });
                          setNotice(`Entered ${c.title}.`);
                        } catch (e) {
                          setNotice(message(e));
                        }
                      }}
                    >
                      Enter {c.title} ↗
                    </button>
                  ))}
              </div>
            </article>
          ))}
      </div>
      <p role="status">{notice}</p>
    </section>
  );
}
