import { mutation, query } from "./_generated/server";
import { v } from "convex/values";
import { annotationGeometry } from "./cloudSchema";
import { access, clean, fail, deviceArg } from "./cloudAuth";
const replyFields = {
  annotationId: v.id("cloudAnnotations"),
  author: v.string(),
  authorName: v.string(),
  body: v.string(),
  created: v.number(),
};
const annotationFields = {
  ...annotationGeometry,
  projectId: v.id("cloudProjects"),
  snapshotId: v.id("cloudFiles"),
  body: v.string(),
  author: v.string(),
  authorName: v.string(),
  created: v.number(),
  resolved: v.boolean(),
};
export const list = query({
  returns: v.array(
    v.object({
      ...annotationFields,
      _id: v.id("cloudAnnotations"),
      _creationTime: v.number(),
      replies: v.array(
        v.object({
          ...replyFields,
          _id: v.id("cloudReplies"),
          _creationTime: v.number(),
        }),
      ),
    }),
  ),
  args: {
    ...deviceArg,
    projectId: v.id("cloudProjects"),
    snapshotId: v.optional(v.id("cloudFiles")),
  },
  handler: async (ctx, a) => {
    await access(ctx, a.projectId, a.deviceToken);
    if (a.snapshotId) {
      const f = await ctx.db.get(a.snapshotId);
      if (!f || f.projectId !== a.projectId) fail("Snapshot unavailable.");
    }
    const rows = a.snapshotId
      ? await ctx.db
          .query("cloudAnnotations")
          .withIndex("by_snapshot", (q) => q.eq("snapshotId", a.snapshotId!))
          .take(200)
      : await ctx.db
          .query("cloudAnnotations")
          .withIndex("by_project", (q) => q.eq("projectId", a.projectId))
          .take(200);
    return Promise.all(
      rows.map(async (r) => ({
        ...r,
        replies: await ctx.db
          .query("cloudReplies")
          .withIndex("by_annotation", (q) => q.eq("annotationId", r._id))
          .take(100),
      })),
    );
  },
});
export const annotate = mutation({
  returns: v.id("cloudAnnotations"),
  args: {
    ...deviceArg,
    snapshotId: v.id("cloudFiles"),
    ...annotationGeometry,
    body: v.string(),
  },
  handler: async (ctx, a) => {
    const f = await ctx.db.get(a.snapshotId);
    if (!f || f.kind !== "snapshot") fail("Select a flat export.");
    const { user } = await access(ctx, f.projectId, a.deviceToken);
    for (const n of [a.x, a.y, a.endX, a.endY])
      if (n !== undefined && (!Number.isFinite(n) || n < 0 || n > 1))
        fail("Annotation coordinates must be within the snapshot.");
    if (
      a.shape === "rectangle" &&
      (a.endX === undefined || a.endY === undefined)
    )
      fail("Draw a review rectangle.");
    if (a.shape === "rectangle" && (a.x === a.endX || a.y === a.endY))
      fail("Draw a rectangle with width and height.");
    const drawing = a.shape === "highlight" || a.shape === "brush";
    if (a.shape === "stamp" ? !a.stamp : a.stamp !== undefined)
      fail("Choose a valid stamp for a stamp annotation.");
    if (drawing) {
      if (!a.points || a.points.length < 2 || a.points.length > 512)
        fail("Use between 2 and 512 stroke points.");
      for (const p of a.points)
        if (![p.x, p.y].every((n) => Number.isFinite(n) && n >= 0 && n <= 1))
          fail("Stroke points must be within the snapshot.");
      if (
        !a.points.some((p) => p.x !== a.points![0].x || p.y !== a.points![0].y)
      )
        fail("Draw a stroke before posting.");
      if (a.points[0].x !== a.x || a.points[0].y !== a.y)
        fail("Stroke origin must match its first point.");
      if (
        a.strokeWidth === undefined ||
        !Number.isFinite(a.strokeWidth) ||
        a.strokeWidth < 0.001 ||
        a.strokeWidth > 0.05
      )
        fail("Invalid stroke width.");
      if (!a.color || !/^#[0-9a-f]{6}$/i.test(a.color))
        fail("Use a six-digit stroke color.");
      if (
        a.opacity === undefined ||
        !Number.isFinite(a.opacity) ||
        a.opacity < 0.1 ||
        a.opacity > 1
      )
        fail("Invalid stroke opacity.");
    } else if (
      a.points !== undefined ||
      a.strokeWidth !== undefined ||
      a.color !== undefined ||
      a.opacity !== undefined
    )
      fail("Stroke settings require a drawing annotation.");
    if (
      a.shape !== "rectangle" &&
      (a.endX !== undefined || a.endY !== undefined)
    )
      fail("Rectangle bounds require a rectangle annotation.");
    const body =
      (drawing || a.shape === "stamp") && !a.body.trim()
        ? ""
        : clean(a.body, 4000);
    const count = await ctx.db
      .query("cloudAnnotations")
      .withIndex("by_project", (q) => q.eq("projectId", f.projectId))
      .take(200);
    if (count.length >= 200) fail("Project annotation limit reached.");
    return ctx.db.insert("cloudAnnotations", {
      projectId: f.projectId,
      snapshotId: a.snapshotId,
      x: a.x,
      y: a.y,
      endX: a.endX,
      endY: a.endY,
      shape: a.shape,
      stamp: a.stamp,
      points: a.points,
      strokeWidth: a.strokeWidth,
      color: a.color,
      opacity: a.opacity,
      body,
      author: user.userId,
      authorName: user.name,
      created: Date.now(),
      resolved: false,
    });
  },
});
export const reply = mutation({
  returns: v.id("cloudReplies"),
  args: { ...deviceArg, id: v.id("cloudAnnotations"), body: v.string() },
  handler: async (ctx, a) => {
    const r = await ctx.db.get(a.id);
    if (!r) fail("Annotation unavailable.");
    const { user } = await access(ctx, r.projectId, a.deviceToken);
    const count = await ctx.db
      .query("cloudReplies")
      .withIndex("by_annotation", (q) => q.eq("annotationId", a.id))
      .take(100);
    if (count.length >= 100) fail("Thread limit reached.");
    return ctx.db.insert("cloudReplies", {
      annotationId: a.id,
      author: user.userId,
      authorName: user.name,
      body: clean(a.body, 4000),
      created: Date.now(),
    });
  },
});
export const resolve = mutation({
  returns: v.null(),
  args: { ...deviceArg, id: v.id("cloudAnnotations"), resolved: v.boolean() },
  handler: async (ctx, a) => {
    const r = await ctx.db.get(a.id);
    if (!r) fail("Annotation unavailable.");
    const { user, member } = await access(ctx, r.projectId, a.deviceToken);
    if (member.role === "reviewer" && r.author !== user.userId)
      fail("Only the author or project team can resolve this thread.");
    await ctx.db.patch(a.id, { resolved: a.resolved });
    return null;
  },
});

export const remove = mutation({
  args: { ...deviceArg, id: v.id("cloudAnnotations") },
  returns: v.null(),
  handler: async (ctx, a) => {
    const row = await ctx.db.get(a.id);
    if (!row) fail("Annotation unavailable.");
    const { user, member } = await access(ctx, row.projectId, a.deviceToken);
    if (member.role === "reviewer" && row.author !== user.userId)
      fail("Only the author or project team can delete this thread.");
    const replies = await ctx.db
      .query("cloudReplies")
      .withIndex("by_annotation", (q) => q.eq("annotationId", a.id))
      .take(100);
    for (const reply of replies) await ctx.db.delete(reply._id);
    await ctx.db.delete(a.id);
    return null;
  },
});
