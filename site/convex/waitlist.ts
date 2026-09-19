import { mutation, internalMutation } from "./_generated/server";
import { v } from "convex/values";

export const join = mutation({
  args: {
    secret: v.string(), email: v.string(), name: v.string(),
    list: v.union(v.literal("cloud"), v.literal("competition")), fingerprint: v.string(),
  },
  handler: async (ctx, args) => {
    if (!process.env.WAITLIST_SECRET || args.secret !== process.env.WAITLIST_SECRET) {
      throw new Error("Unauthorized");
    }
    const email = args.email.trim().toLowerCase();
    const name = args.name.trim();
    if (email.length > 254 || !/^[^\s@]+@[^\s@]+\.[^\s@]+$/.test(email) || name.length > 100) {
      return { ok: false, reason: "invalid" } as const;
    }
    const now = Date.now();
    const rate = await ctx.db.query("waitlistRateLimits")
      .withIndex("by_fingerprint", q => q.eq("fingerprint", args.fingerprint)).unique();
    if (rate && rate.expires > now && rate.count >= 5) return { ok: false, reason: "rate" } as const;
    if (rate) await ctx.db.patch(rate._id, {
      count: rate.expires > now ? rate.count + 1 : 1,
      expires: rate.expires > now ? rate.expires : now + 60_000,
    });
    else await ctx.db.insert("waitlistRateLimits", { fingerprint: args.fingerprint, count: 1, expires: now + 60_000 });

    const existing = await ctx.db.query("waitlistSignups")
      .withIndex("by_list_email", q => q.eq("list", args.list).eq("email", email)).unique();
    if (!existing) await ctx.db.insert("waitlistSignups", {
      email, name, list: args.list, created: now, consentVersion: "access-updates-v1",
    });
    return { ok: true } as const;
  },
});

// Internal only: scheduled cleanup cannot expose signup records or be called by visitors.
export const cleanRateLimits = internalMutation({
  args: {},
  handler: async ctx => {
    const stale = await ctx.db.query("waitlistRateLimits")
      .withIndex("by_expires", q => q.lt("expires", Date.now() - 86_400_000)).take(1000);
    for (const item of stale) await ctx.db.delete(item._id);
  },
});

// Account operators can honor a removal request through the authenticated CLI.
export const removeSignup = internalMutation({
  args: { email: v.string(), list: v.union(v.literal("cloud"), v.literal("competition")) },
  handler: async (ctx, { email, list }) => {
    const entry = await ctx.db.query("waitlistSignups")
      .withIndex("by_list_email", q => q.eq("list", list).eq("email", email.trim().toLowerCase())).unique();
    if (entry) await ctx.db.delete(entry._id);
    return { removed: Boolean(entry) };
  },
});
