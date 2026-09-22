import { mutation, internalQuery, internalMutation } from "./_generated/server";
import { v } from "convex/values";
import { parseBatch } from "../src/shared/telemetry";

const count = v.object({ period: v.string(), metric: v.string(), count: v.number() });
export const ingest = mutation({
  args: { secret: v.string(), schema: v.number(), release: v.string(), platform: v.string(), counts: v.array(count) },
  handler: async (ctx, { secret, ...input }) => {
    if (!process.env.TELEMETRY_SECRET || secret !== process.env.TELEMETRY_SECRET) throw new Error("Unauthorized");
    const batch = parseBatch(input);
    if (!batch) throw new Error("Invalid aggregate counts");
    // Global budget, never keyed by an IP, fingerprint, cookie or device ID.
    const hour = Math.floor(Date.now() / 3600000);
    const budget = await ctx.db.query("telemetryBudget").first();
    if (budget?.hour === hour && budget.requests >= 6000) return { accepted: false, diagnostics: [] as string[] };
    if (budget) await ctx.db.patch(budget._id, { hour, requests: budget.hour === hour ? budget.requests + 1 : 1 });
    else await ctx.db.insert("telemetryBudget", { hour, requests: 1 });
    const diagnostics: string[] = [];
    for (const item of batch.counts) {
      const row = await ctx.db.query("telemetryCounts").withIndex("by_bucket", q => q.eq("period", item.period).eq("metric", item.metric).eq("release", batch.release).eq("platform", batch.platform)).unique();
      const diagnostic = /^(error|crash)\./.test(item.metric);
      const forward = diagnostic && row?.forwardedHour !== hour;
      if (forward) diagnostics.push(item.metric);
      if (row) await ctx.db.patch(row._id, { count: row.count + item.count, ...(forward ? { forwardedHour: hour } : {}) });
      else await ctx.db.insert("telemetryCounts", { ...item, release: batch.release, platform: batch.platform, ...(forward ? { forwardedHour: hour } : {}) });
    }
    return { accepted: true, diagnostics };
  },
});

/** Owner-only CLI/dashboard reporting. There is no public reader of detailed usage. */
export const summary = internalQuery({
  args: { since: v.optional(v.string()) },
  handler: async (ctx, args) => {
    const since = args.since ?? new Date(Date.now() - 31 * 86400000).toISOString().slice(0, 10);
    const rows = await ctx.db.query("telemetryCounts").withIndex("by_bucket", q => q.gte("period", since.slice(0, 7))).collect();
    const totals: Record<string, number> = {};
    for (const row of rows) {
      if (row.period.length === 10 && row.period < since) continue;
      const key = `${row.period} ${row.metric}`;
      totals[key] = (totals[key] ?? 0) + row.count;
    }
    return { explanation: "Opt-in active installations, not identifiable people. UTC calendar periods; counts are best effort and may include reinstallations.", counts: Object.entries(totals).sort(([a], [b]) => a.localeCompare(b)).map(([bucket, count]) => ({ bucket, count })) };
  },
});


// Reserved probe release, removable only through authenticated operator tooling.
export const discardIntegrationProbe = internalMutation({
  args: {},
  handler: async ctx => {
    const rows = await ctx.db.query("telemetryCounts").filter(q => q.eq(q.field("release"), "0.0.0-alpha.0")).collect();
    for (const row of rows) await ctx.db.delete(row._id);
    return { removed: rows.length };
  },
});
