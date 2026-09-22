import { mutation, query } from "./_generated/server";
import { v } from "convex/values";
import { actor, access, clean, fail, deviceArg } from "./cloudAuth";
import { Resend } from "@convex-dev/resend";
import { components } from "./_generated/api";
import { limit } from "./cloudMaintenance";
import { role } from "./cloudSchema";
const mail = new Resend(components.resend, { testMode: false });
export const list = query({
  args: deviceArg,
  returns: v.array(v.object({
    _id: v.id("cloudProjects"),
    _creationTime: v.number(),
    title: v.string(), owner: v.string(), created: v.number(),
    updated: v.number(), archived: v.boolean(), role,
    shared: v.boolean(),
    previews: v.array(v.object({
      _id: v.id("cloudFiles"), name: v.string(), kind: v.literal("snapshot"),
      version: v.number(), created: v.number(),
      width: v.optional(v.number()), height: v.optional(v.number()),
    })),
  })),
  handler: async (ctx, a) => {
    const u = await actor(ctx, a.deviceToken);
    const members = await ctx.db
      .query("cloudMembers")
      .withIndex("by_user", (q) => q.eq("userId", u.userId))
      .take(100);
    const projects = await Promise.all(
      members.map(async (m) => {
        const p = await ctx.db.get(m.projectId);
        if (!p || p.archived) return null;
        // An owner's private cloud backup is not a team project. Only accepted
        // memberships count; pending invitations do not expose the Team tab.
        const team = await ctx.db.query("cloudMembers")
          .withIndex("by_project", (q) => q.eq("projectId", p._id)).take(2);
        const shared = team.length > 1;
        const previews = shared ? await ctx.db.query("cloudFiles")
          .withIndex("by_project_kind", (q) => q.eq("projectId", p._id).eq("kind", "snapshot"))
          .order("desc").take(3) : [];
        return { ...p, role: m.role, shared, previews: previews.map((f) => ({
          _id: f._id, name: f.name, kind: "snapshot" as const,
          version: f.version, created: f.created, width: f.width, height: f.height,
        })) };
      }),
    );
    return projects
      .filter((p) => p !== null)
      .sort((a, b) => b.updated - a.updated);
  },
});
export const create = mutation({
  args: { ...deviceArg, title: v.string() },
  handler: async (ctx, a) => {
    const u = await actor(ctx, a.deviceToken);
    const existing = await ctx.db
      .query("cloudMembers")
      .withIndex("by_user", (q) => q.eq("userId", u.userId))
      .take(100);
    if (existing.length >= 100) fail("Project limit reached.");
    const id = await ctx.db.insert("cloudProjects", {
      title: clean(a.title),
      owner: u.userId,
      created: Date.now(),
      updated: Date.now(),
      archived: false,
    });
    await ctx.db.insert("cloudMembers", { projectId: id, ...u, role: "owner" });
    return id;
  },
});
export const get = query({
  args: { ...deviceArg, projectId: v.id("cloudProjects") },
  handler: async (ctx, a) => {
    const { project, member } = await access(ctx, a.projectId, a.deviceToken);
    const files = await ctx.db
      .query("cloudFiles")
      .withIndex("by_project", (q) => q.eq("projectId", a.projectId))
      .order("desc")
      .take(200);
    return {
      ...project,
      role: member.role,
      files: files.filter(
        (f) => member.role !== "reviewer" || f.kind === "snapshot",
      ),
    };
  },
});
export const members = query({
  args: { ...deviceArg, projectId: v.id("cloudProjects") },
  handler: async (ctx, a) => {
    await access(ctx, a.projectId, a.deviceToken, "owner");
    return {
      members: await ctx.db
        .query("cloudMembers")
        .withIndex("by_project", (q) => q.eq("projectId", a.projectId))
        .take(100),
      invites: await ctx.db
        .query("cloudInvites")
        .withIndex("by_project_email", (q) => q.eq("projectId", a.projectId))
        .take(100),
    };
  },
});
export const invite = mutation({
  args: {
    ...deviceArg,
    projectId: v.id("cloudProjects"),
    email: v.string(),
    role: v.union(v.literal("editor"), v.literal("reviewer")),
  },
  handler: async (ctx, a) => {
    const { user, project } = await access(
      ctx,
      a.projectId,
      a.deviceToken,
      "owner",
    );
    const email = clean(a.email, 254).toLowerCase();
    if (!/^[^\s@]+@[^\s@]+\.[^\s@]+$/.test(email)) fail("Enter a valid email.");
    const invites = await ctx.db
      .query("cloudInvites")
      .withIndex("by_project_email", (q) => q.eq("projectId", a.projectId))
      .take(100);
    if (invites.length >= 100) fail("Invitation limit reached.");
    const old = invites.find((i) => i.email === email);
    if (old && !old.accepted && old.expires > Date.now())
      fail("An invitation is already pending.");
    const data = {
      projectId: a.projectId,
      email,
      role: a.role,
      inviter: user.userId,
      expires: Date.now() + 7 * 86400000,
      accepted: false,
    };
    if (old) await ctx.db.patch(old._id, data);
    else await ctx.db.insert("cloudInvites", data);
    await limit(ctx, `invites:${user.userId}`, 20, 3600000);
    await mail.sendEmail(ctx, {
      from: "Omadesign <collab@mail.omadesign.app>",
      to: email,
      subject: "You're invited to an Omadesign project",
      text: `${user.name} invited you to ${project.title} as ${a.role}. Sign in with ${email} to accept: https://omadesign.app/cloud\nThis invitation expires in seven days.`,
    });
    return { ok: true };
  },
});
export const invitations = query({
  args: deviceArg,
  handler: async (ctx, a) => {
    const u = await actor(ctx, a.deviceToken);
    const rows = await ctx.db
      .query("cloudInvites")
      .withIndex("by_email", (q) => q.eq("email", u.email))
      .take(100);
    return Promise.all(
      rows
        .filter((r) => !r.accepted && r.expires > Date.now())
        .map(async (r) => ({
          ...r,
          title: (await ctx.db.get(r.projectId))?.title || "Project",
        })),
    );
  },
});
export const accept = mutation({
  args: { ...deviceArg, id: v.id("cloudInvites") },
  handler: async (ctx, a) => {
    const u = await actor(ctx, a.deviceToken);
    const i = await ctx.db.get(a.id);
    if (!i || i.email !== u.email || i.accepted || i.expires < Date.now())
      fail("Invitation unavailable.");
    const p = await ctx.db.get(i.projectId);
    if (!p || p.archived) fail("Project unavailable.");
    const old = await ctx.db
      .query("cloudMembers")
      .withIndex("by_project_user", (q) =>
        q.eq("projectId", i.projectId).eq("userId", u.userId),
      )
      .unique();
    const membership = await ctx.db
      .query("cloudMembers")
      .withIndex("by_user", (q) => q.eq("userId", u.userId))
      .take(100);
    const team = await ctx.db
      .query("cloudMembers")
      .withIndex("by_project", (q) => q.eq("projectId", i.projectId))
      .take(100);
    if (!old && (membership.length >= 100 || team.length >= 100))
      fail("Project membership limit reached.");
    if (!old)
      await ctx.db.insert("cloudMembers", {
        projectId: i.projectId,
        ...u,
        role: i.role,
      });
    await ctx.db.patch(i._id, { accepted: true });
    return i.projectId;
  },
});
export const changeMember = mutation({
  args: {
    ...deviceArg,
    projectId: v.id("cloudProjects"),
    userId: v.string(),
    role: v.union(
      v.literal("editor"),
      v.literal("reviewer"),
      v.literal("remove"),
    ),
  },
  handler: async (ctx, a) => {
    await access(ctx, a.projectId, a.deviceToken, "owner");
    const m = await ctx.db
      .query("cloudMembers")
      .withIndex("by_project_user", (q) =>
        q.eq("projectId", a.projectId).eq("userId", a.userId),
      )
      .unique();
    if (!m || m.role === "owner") fail("Cannot change project owner.");
    if (a.role === "remove") await ctx.db.delete(m._id);
    else await ctx.db.patch(m._id, { role: a.role });
  },
});
export const cancelInvite = mutation({
  args: { ...deviceArg, id: v.id("cloudInvites") },
  handler: async (ctx, a) => {
    const i = await ctx.db.get(a.id);
    if (!i) fail("Invitation unavailable.");
    await access(ctx, i.projectId, a.deviceToken, "owner");
    await ctx.db.delete(i._id);
  },
});

export const archive = mutation({
  args: { ...deviceArg, projectId: v.id("cloudProjects") },
  handler: async (ctx, a) => {
    await access(ctx, a.projectId, a.deviceToken, "owner");
    const published = await ctx.db
      .query("cloudShowcase")
      .withIndex("by_project", (q) => q.eq("projectId", a.projectId))
      .take(100);
    for (const item of published)
      await ctx.db.patch(item._id, { published: false });
    await ctx.db.patch(a.projectId, { archived: true, updated: Date.now() });
  },
});
export const restore = mutation({
  args: { ...deviceArg, projectId: v.id("cloudProjects") },
  handler: async (ctx, a) => {
    const user = await actor(ctx, a.deviceToken);
    const project = await ctx.db.get(a.projectId);
    if (!project || project.owner !== user.userId) fail("Project unavailable.");
    await ctx.db.patch(a.projectId, { archived: false, updated: Date.now() });
  },
});
export const archived = query({
  args: deviceArg,
  handler: async (ctx, a) => {
    const user = await actor(ctx, a.deviceToken);
    return (
      await ctx.db
        .query("cloudProjects")
        .withIndex("by_owner", (q) => q.eq("owner", user.userId))
        .take(100)
    ).filter((p) => p.archived);
  },
});
