import { mutation, query } from "./_generated/server";
import { v } from "convex/values";
import { actor, clean, fail, hash, deviceArg } from "./cloudAuth";
export const begin = mutation({
  args: { token: v.string(), code: v.string(), label: v.string() },
  handler: async (ctx, a) => {
    if (!/^[a-f0-9]{64}$/.test(a.token) || !/^[A-Z0-9]{12}$/.test(a.code))
      fail("Invalid device request.");
    const tokenHash = await hash(a.token);
    if (
      await ctx.db
        .query("cloudDevices")
        .withIndex("by_hash", (q) => q.eq("tokenHash", tokenHash))
        .first()
    )
      fail("Start a new device request.");
    if (
      await ctx.db
        .query("cloudDevices")
        .withIndex("by_code", (q) => q.eq("code", a.code))
        .first()
    )
      fail("Start a new device request.");
    await ctx.db.insert("cloudDevices", {
      tokenHash,
      code: a.code,
      label: clean(a.label, 80),
      created: Date.now(),
      expires: Date.now() + 600000,
    });
    return { code: a.code };
  },
});
export const approve = mutation({
  args: { code: v.string() },
  handler: async (ctx, { code }) => {
    const user = await actor(ctx);
    const d = await ctx.db
      .query("cloudDevices")
      .withIndex("by_code", (q) =>
        q.eq("code", code.toUpperCase().replace(/[-\s]/g, "")),
      )
      .unique();
    if (!d || d.expires < Date.now() || d.userId)
      fail("This device code has expired or was already used.");
    await ctx.db.patch(d._id, { ...user, expires: Date.now() + 30 * 86400000 });
    return { label: d.label };
  },
});
export const me = query({
  args: deviceArg,
  handler: async (ctx, a) => actor(ctx, a.deviceToken),
});
export const list = query({
  args: {},
  handler: async (ctx) => {
    const u = await actor(ctx);
    return (
      await ctx.db
        .query("cloudDevices")
        .withIndex("by_user", (q) => q.eq("userId", u.userId))
        .take(100)
    ).map((d) => ({ id: d._id, label: d.label, expires: d.expires }));
  },
});
export const revoke = mutation({
  args: { id: v.id("cloudDevices") },
  handler: async (ctx, { id }) => {
    const u = await actor(ctx);
    const d = await ctx.db.get(id);
    if (!d || d.userId !== u.userId) fail("Device not found.");
    await ctx.db.delete(id);
  },
});
