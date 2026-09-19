import { mutation, query, internalMutation } from "./_generated/server";
import { v } from "convex/values";
import { actor, access, clean, fail, deviceArg } from "./cloudAuth";
export const list = query({
  args: {},
  handler: async (ctx) =>
    Promise.all(
      (
        await ctx.db
          .query("cloudShowcase")
          .withIndex("by_published", (q) => q.eq("published", true))
          .order("desc")
          .take(100)
      ).map(async (item) => {
        const f = await ctx.db.get(item.snapshotId);
        return {
          ...item,
          image: f ? await ctx.storage.getUrl(f.storageId) : null,
        };
      }),
    ),
});
export const project = query({
  args: { ...deviceArg, projectId: v.id("cloudProjects") },
  handler: async (ctx, a) => {
    await access(ctx, a.projectId, a.deviceToken);
    return ctx.db
      .query("cloudShowcase")
      .withIndex("by_project", (q) => q.eq("projectId", a.projectId))
      .take(100);
  },
});
export const publish = mutation({
  args: {
    ...deviceArg,
    snapshotId: v.id("cloudFiles"),
    title: v.string(),
    description: v.string(),
  },
  handler: async (ctx, a) => {
    const f = await ctx.db.get(a.snapshotId);
    if (!f || f.kind !== "snapshot")
      fail("Only flat exports can be published.");
    const { user } = await access(ctx, f.projectId, a.deviceToken, "owner");
    const rows = await ctx.db
      .query("cloudShowcase")
      .withIndex("by_project", (q) => q.eq("projectId", f.projectId))
      .take(100);
    if (rows.find((r) => r.snapshotId === a.snapshotId && r.published))
      fail("This snapshot is already public.");
    return ctx.db.insert("cloudShowcase", {
      projectId: f.projectId,
      snapshotId: a.snapshotId,
      owner: user.userId,
      author: user.name,
      title: clean(a.title),
      description: clean(a.description, 2000),
      published: true,
      created: Date.now(),
    });
  },
});
export const unpublish = mutation({
  args: { ...deviceArg, id: v.id("cloudShowcase") },
  handler: async (ctx, a) => {
    const item = await ctx.db.get(a.id);
    if (!item) fail("Work unavailable.");
    await access(ctx, item.projectId, a.deviceToken, "owner");
    await ctx.db.patch(a.id, { published: false });
  },
});
export const competitions = query({
  args: {},
  handler: async (ctx) =>
    ctx.db
      .query("cloudCompetitions")
      .withIndex("by_active", (q) => q.eq("active", true))
      .take(50),
});
export const entries = query({
  args: { ...deviceArg, competitionId: v.optional(v.id("cloudCompetitions")) },
  handler: async (ctx, a) => {
    if (!a.competitionId) {
      const u = await actor(ctx, a.deviceToken);
      return ctx.db
        .query("cloudEntries")
        .withIndex("by_owner", (q) => q.eq("owner", u.userId))
        .take(100);
    }
    const rows = await ctx.db
      .query("cloudEntries")
      .withIndex("by_competition", (q) =>
        q.eq("competitionId", a.competitionId!),
      )
      .take(200);
    const result = await Promise.all(
      rows.map(async (e) => {
        const item = await ctx.db.get(e.showcaseId);
        if (!item?.published) return null;
        const f = await ctx.db.get(item.snapshotId);
        return {
          ...e,
          title: item.title,
          author: item.author,
          image: f ? await ctx.storage.getUrl(f.storageId) : null,
        };
      }),
    );
    return result.filter(Boolean);
  },
});
export const enter = mutation({
  args: {
    ...deviceArg,
    competitionId: v.id("cloudCompetitions"),
    showcaseId: v.id("cloudShowcase"),
  },
  handler: async (ctx, a) => {
    const u = await actor(ctx, a.deviceToken);
    const c = await ctx.db.get(a.competitionId);
    const s = await ctx.db.get(a.showcaseId);
    if (!c?.active || c.opens > Date.now() || c.closes < Date.now())
      fail("This competition is not accepting entries.");
    if (!s?.published || s.owner !== u.userId)
      fail("Select your own public showcase work.");
    if (
      await ctx.db
        .query("cloudEntries")
        .withIndex("by_competition_showcase", (q) =>
          q.eq("competitionId", a.competitionId).eq("showcaseId", a.showcaseId),
        )
        .first()
    )
      fail("This work is already entered.");
    return ctx.db.insert("cloudEntries", {
      competitionId: a.competitionId,
      showcaseId: a.showcaseId,
      owner: u.userId,
      created: Date.now(),
    });
  },
});
export const withdraw = mutation({
  args: { ...deviceArg, id: v.id("cloudEntries") },
  handler: async (ctx, a) => {
    const u = await actor(ctx, a.deviceToken);
    const e = await ctx.db.get(a.id);
    if (!e || e.owner !== u.userId) fail("Entry unavailable.");
    await ctx.db.delete(a.id);
  },
});
export const configureCompetition = internalMutation({
  args: {
    id: v.optional(v.id("cloudCompetitions")),
    title: v.string(),
    description: v.string(),
    opens: v.number(),
    closes: v.number(),
    active: v.boolean(),
  },
  handler: async (ctx, { id, ...data }) => {
    if (data.closes <= data.opens)
      fail("Closing date must follow opening date.");
    if (id) {
      await ctx.db.patch(id, data);
      return id;
    }
    return ctx.db.insert("cloudCompetitions", data);
  },
});

export const get = query({
  args: { id: v.id("cloudShowcase") },
  handler: async (ctx, a) => {
    const item = await ctx.db.get(a.id);
    if (!item?.published) return null;
    const file = await ctx.db.get(item.snapshotId);
    return { ...item, image: file ? await ctx.storage.getUrl(file.storageId) : null };
  },
});
