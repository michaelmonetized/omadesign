import { internalMutation } from "./_generated/server";
import type { MutationCtx } from "./_generated/server";
import { fail } from "./cloudAuth";
export async function limit(
  ctx: MutationCtx,
  key: string,
  maximum: number,
  windowMs: number,
) {
  const old = await ctx.db
    .query("cloudLimits")
    .withIndex("by_key", (q) => q.eq("key", key))
    .unique();
  const count = old && old.expires > Date.now() ? old.count : 0;
  if (count >= maximum) fail("Too many requests. Please try again later.");
  const value = {
    key,
    count: count + 1,
    expires:
      old && old.expires > Date.now() ? old.expires : Date.now() + windowMs,
  };
  if (old) await ctx.db.replace(old._id, value);
  else await ctx.db.insert("cloudLimits", value);
}
export const expire = internalMutation({
  args: {},
  handler: async (ctx) => {
    for (const row of await ctx.db
      .query("cloudDevices")
      .withIndex("by_expires", (q) => q.lt("expires", Date.now()))
      .take(500))
      await ctx.db.delete(row._id);
    for (const row of await ctx.db
      .query("cloudUploads")
      .withIndex("by_created", (q) => q.lt("created", Date.now() - 3600000))
      .take(500))
      await ctx.db.delete(row._id);
    for (const row of await ctx.db
      .query("cloudLimits")
      .withIndex("by_expires", (q) => q.lt("expires", Date.now()))
      .take(500))
      await ctx.db.delete(row._id);
  },
});
