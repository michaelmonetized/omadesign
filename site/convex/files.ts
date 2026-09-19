import { mutation, query } from "./_generated/server";
import { v } from "convex/values";
import { access, clean, fail, deviceArg } from "./cloudAuth";
import { fileKind } from "./cloudSchema";
export const begin = mutation({
  args: { ...deviceArg, projectId: v.id("cloudProjects"), kind: fileKind },
  handler: async (ctx, a) => {
    const { user } = await access(ctx, a.projectId, a.deviceToken, "write");
    const files = await ctx.db
      .query("cloudFiles")
      .withIndex("by_project", (q) => q.eq("projectId", a.projectId))
      .take(200);
    if (files.length >= 200) fail("This project has reached its file limit.");
    const pending = await ctx.db
      .query("cloudUploads")
      .withIndex("by_user", (q) => q.eq("userId", user.userId))
      .take(25);
    if (pending.filter((p) => p.created > Date.now() - 3600000).length >= 20)
      fail("Finish existing uploads before starting more.");
    for (const p of pending)
      if (p.created < Date.now() - 3600000) await ctx.db.delete(p._id);
    const id = await ctx.db.insert("cloudUploads", {
      projectId: a.projectId,
      userId: user.userId,
      kind: a.kind,
      created: Date.now(),
    });
    return { id, url: await ctx.storage.generateUploadUrl() };
  },
});
export const finish = mutation({
  args: {
    ...deviceArg,
    uploadId: v.id("cloudUploads"),
    storageId: v.id("_storage"),
    name: v.string(),
    width: v.optional(v.number()),
    height: v.optional(v.number()),
    frameId: v.optional(v.string()),
  },
  handler: async (ctx, a) => {
    const pending = await ctx.db.get(a.uploadId);
    if (!pending || pending.created < Date.now() - 3600000)
      fail("Upload expired; try again.");
    const { user } = await access(
      ctx,
      pending.projectId,
      a.deviceToken,
      "write",
    );
    if (pending.userId !== user.userId) fail("Upload does not belong to you.");
    const file = await ctx.db.system.get(a.storageId);
    if (!file || file._creationTime < pending.created)
      fail("Upload not found.");
    if (
      await ctx.db
        .query("cloudFiles")
        .withIndex("by_storage", (q) => q.eq("storageId", a.storageId))
        .first()
    )
      fail("File already saved.");
    const type = file.contentType || "application/octet-stream";
    const sizeLimit =
      pending.kind === "snapshot" ? 20 * 1024 * 1024 : 100 * 1024 * 1024;
    if (file.size > sizeLimit || file.size === 0)
      fail("File exceeds the upload limit.");
    const name = clean(a.name, 200).replace(/[\\/\u0000-\u001f]/g, "_");
    if (pending.kind === "source" && !name.toLowerCase().endsWith(".oma"))
      fail("Choose an Omadesign .oma project.");
    if (
      pending.kind === "snapshot" &&
      (!["image/png", "image/jpeg", "image/webp"].includes(type) ||
        !a.width ||
        !a.height ||
        a.width < 1 ||
        a.height < 1 ||
        a.width > 32768 ||
        a.height > 32768)
    )
      fail("Choose a PNG, JPEG, or WebP flat export.");
    const last = await ctx.db
      .query("cloudFiles")
      .withIndex("by_project_kind", (q) =>
        q.eq("projectId", pending.projectId).eq("kind", pending.kind),
      )
      .order("desc")
      .first();
    const id = await ctx.db.insert("cloudFiles", {
      projectId: pending.projectId,
      storageId: a.storageId,
      uploader: user.userId,
      name,
      kind: pending.kind,
      size: file.size,
      contentType: type,
      created: Date.now(),
      version: (last?.version || 0) + 1,
      width: a.width,
      height: a.height,
      frameId: a.frameId,
    });
    await ctx.db.delete(pending._id);
    await ctx.db.patch(pending.projectId, { updated: Date.now() });
    return id;
  },
});
export const authorizeDownload = query({
  args: { ...deviceArg, id: v.id("cloudFiles") },
  handler: async (ctx, a) => {
    const f = await ctx.db.get(a.id);
    if (!f) fail("File unavailable.");
    const { member } = await access(ctx, f.projectId, a.deviceToken);
    if (member.role === "reviewer" && f.kind !== "snapshot")
      fail("Reviewers can only download flat exports.");
    return f;
  },
});
