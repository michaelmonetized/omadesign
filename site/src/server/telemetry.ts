import { ConvexHttpClient } from "convex/browser";
import { api } from "../../convex/_generated/api";
import { parseBatch, type Batch } from "../shared/telemetry";
const json = (body: unknown, status = 200) => Response.json(body, { status, headers: { "Cache-Control": "no-store" } });

export function sentryEnvelope(batch: Batch, metric: string, dsn: string, diagnostic = false) {
  const parsed = new URL(dsn);
  if (parsed.protocol !== "https:" || !parsed.hostname.endsWith(".ingest.us.sentry.io") || !/^\/[0-9]+$/.test(parsed.pathname) || !/^[a-f0-9]+$/.test(parsed.username)) throw new Error("Invalid Sentry destination");
  const eventId = crypto.randomUUID().replaceAll("-", ""); // Per event, never stored on a device or reused.
  const category = metric.replaceAll(".", " ");
  const event = {
    event_id: eventId,
    platform: "other", // No browser SDK integration or automatic request/user context.
    level: metric.startsWith("crash.") ? "fatal" : "error",
    release: `omadesign@${batch.release}`,
    environment: diagnostic ? "integration-test" : "desktop",
    message: `Omadesign ${category}`,
    fingerprint: ["omadesign", metric],
    user: { ip_address: "0.0.0.0" }, // Explicit non-user address; never infer the incoming client's address.
    tags: { category: metric, platform: batch.platform, privacy: "aggregate-v1" },
    extra: { aggregate_count: batch.counts.filter(c => c.metric === metric).reduce((n, c) => n + c.count, 0) },
  };
  const payload = JSON.stringify(event);
  return {
    url: `${parsed.origin}/api${parsed.pathname}/envelope/`,
    body: `${JSON.stringify({ event_id: eventId, dsn })}\n${JSON.stringify({ type: "event", length: new TextEncoder().encode(payload).length, content_type: "application/json" })}\n${payload}\n`,
    eventId,
  };
}

export async function handleTelemetry(request: Request) {
  if (!request.headers.get("content-type")?.startsWith("application/json")) return json({ error: "Expected JSON" }, 415);
  if (Number(request.headers.get("content-length")) > 16384) return json({ error: "Too large" }, 413);
  // Bound streamed bodies too. Never read or forward cookies, Authorization,
  // forwarding headers, user-agent, referrer or an address from the Request.
  let text = "";
  try {
    const reader = request.body?.getReader();
    if (!reader) return json({ error: "Empty body" }, 400);
    const chunks: Uint8Array[] = []; let size = 0;
    while (true) {
      const { done, value } = await reader.read(); if (done) break;
      size += value.length;
      if (size > 16384) { await reader.cancel(); return json({ error: "Too large" }, 413); }
      chunks.push(value);
    }
    const bytes = new Uint8Array(size); let offset = 0;
    for (const chunk of chunks) { bytes.set(chunk, offset); offset += chunk.length; }
    text = new TextDecoder().decode(bytes);
  } catch { return json({ error: "Invalid body" }, 400); }
  let batch: Batch | null;
  try { batch = parseBatch(JSON.parse(text)); } catch { batch = null; }
  if (!batch) return json({ error: "Invalid aggregate schema" }, 400);
  if (!process.env.CONVEX_URL || !process.env.TELEMETRY_SECRET) return json({ error: "Unavailable" }, 503);
  try {
    const result = await new ConvexHttpClient(process.env.CONVEX_URL).mutation(api.telemetry.ingest, { ...batch, secret: process.env.TELEMETRY_SECRET });
    if (!result.accepted) return json({ error: "Try later" }, 429);
    // Aggregate storage remains available if diagnostic delivery is temporarily down.
    if (process.env.SENTRY_DSN) {
      await Promise.all(result.diagnostics.map(async metric => {
        try {
          const envelope = sentryEnvelope(batch, metric, process.env.SENTRY_DSN!);
          await fetch(envelope.url, { method: "POST", headers: { "Content-Type": "application/x-sentry-envelope" }, body: envelope.body, signal: AbortSignal.timeout(4000) });
        } catch { /* No payloads or request details are logged. */ }
      }));
    }
    return json({ ok: true });
  } catch { return json({ error: "Unavailable" }, 503); }
}
