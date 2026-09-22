/// <reference types="vite/client" />
import { convexTest } from "convex-test";
import { beforeEach, afterEach, describe, it, expect, vi } from "vitest";
import schema from "./schema";
import { api, internal } from "./_generated/api";
import { parseBatch } from "../src/shared/telemetry";
import { handleTelemetry, sentryEnvelope } from "../src/server/telemetry";
const modules = import.meta.glob(["./**/*.ts", "!./**/*.test.ts"]);
const now = () => new Date().toISOString().slice(0, 10);
const batch = () => ({ schema: 1 as const, release: "0.5.5-nightly.1", platform: "linux-x86_64" as const, counts: [{ period: now(), metric: "error.import", count: 2 }] });
beforeEach(() => vi.stubEnv("TELEMETRY_SECRET", "test-server-secret"));
afterEach(() => vi.unstubAllEnvs());
describe("Private aggregate telemetry", () => {
  it("rejects personal fields, raw text, fake metrics and inflated activity", () => {
    expect(parseBatch(batch())).not.toBeNull();
    for (const key of ["ip", "user", "installation_id", "path", "email", "stacktrace", "message"]) expect(parseBatch({ ...batch(), [key]: "private" })).toBeNull();
    expect(parseBatch({ ...batch(), counts: [{ period: now(), metric: "tool.my-private-document", count: 1 }] })).toBeNull();
    expect(parseBatch({ ...batch(), counts: [{ period: now(), metric: "active.day", count: 2 }] })).toBeNull();
    expect(parseBatch({ ...batch(), counts: [{ period: "2026-02-31", metric: "active.day", count: 1 }] })).toBeNull();
  });
  it("requires a server secret and stores only summed buckets", async () => {
    const t = convexTest(schema, modules);
    await expect(t.mutation(api.telemetry.ingest, { ...batch(), secret: "wrong" })).rejects.toThrow("Unauthorized");
    const first = await t.mutation(api.telemetry.ingest, { ...batch(), secret: "test-server-secret" });
    const second = await t.mutation(api.telemetry.ingest, { ...batch(), secret: "test-server-secret" });
    expect(first.diagnostics).toEqual(["error.import"]); expect(second.diagnostics).toEqual([]);
    const rows = await t.run(ctx => ctx.db.query("telemetryCounts").collect());
    expect(rows).toHaveLength(1); expect(rows[0].count).toBe(4);
    expect(Object.keys(rows[0]).sort()).toEqual(["_creationTime", "_id", "count", "forwardedHour", "metric", "period", "platform", "release"].sort());
    const report = await t.query(internal.telemetry.summary, {});
    expect(report.counts).toEqual([{ bucket: `${now()} error.import`, count: 4 }]);
  });
  it("sends categorical Sentry envelopes without client identity or context", () => {
    const envelope = sentryEnvelope(batch(), "error.import", "https://abc123@o123.ingest.us.sentry.io/456", true);
    const [header, item, payload] = envelope.body.trim().split("\n").map(line => JSON.parse(line));
    expect(header.event_id).toMatch(/^[a-f0-9]{32}$/);
    expect(item.type).toBe("event");
    expect(payload.user).toEqual({ ip_address: "0.0.0.0" });
    for (const key of ["request", "contexts", "breadcrumbs", "stacktrace", "server_name"]) expect(payload).not.toHaveProperty(key);
    expect(payload.environment).toBe("integration-test");
    expect(envelope.url).toBe("https://o123.ingest.us.sentry.io/api/456/envelope/");
  });
  it("rejects unknown payloads and oversized bodies before accessing the backend", async () => {
    const send = (body: unknown) => handleTelemetry(new Request("https://omadesign.app/api/telemetry", { method: "POST", headers: { "content-type": "application/json", "x-forwarded-for": "private-address", cookie: "account=private" }, body: JSON.stringify(body) }));
    expect((await send({ ...batch(), private: "no" })).status).toBe(400);
    expect((await send({ data: "x".repeat(17000) })).status).toBe(413);
  });
});
