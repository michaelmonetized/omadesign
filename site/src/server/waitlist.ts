import { ConvexHttpClient } from "convex/browser";
import { api } from "../../convex/_generated/api";

function json(data: unknown, status = 200) {
  return Response.json(data, { status, headers: { "Cache-Control": "no-store" } });
}

export async function handleWaitlist(request: Request, legacyBody?: unknown) {
  const origin = request.headers.get("origin");
  const allowed = new Set([new URL(request.url).origin, "https://omadesign.app", "https://www.omadesign.app", "https://michaelmonetized.github.io"]);
  if (origin && !allowed.has(origin)) return json({ error: "Please join from omadesign.app." }, 403);
  if (!request.headers.get("content-type")?.includes("application/json")) return json({ error: "Expected a JSON signup." }, 415);
  if (Number(request.headers.get("content-length")) > 4096) return json({ error: "Signup is too large." }, 413);
  let body: Record<string, unknown>;
  try {
    const raw = legacyBody === undefined ? await request.text() : JSON.stringify(legacyBody);
    if (raw.length > 4096) return json({ error: "Signup is too large." }, 413);
    const parsed = JSON.parse(raw);
    if (!parsed || typeof parsed !== "object" || Array.isArray(parsed)) throw new Error();
    body = parsed;
  } catch { return json({ error: "Please check your signup and try again." }, 400); }
  if (typeof body.website === "string" && body.website.trim()) return json({ error: "Unable to accept this signup." }, 400);
  const email = typeof body.email === "string" ? body.email.trim().toLowerCase() : "";
  const name = typeof body.name === "string" ? body.name.trim() : "";
  const list = body.list ?? (body.action === "waitlist" ? "competition" : undefined);
  if (email.length > 254 || !/^[^\s@]+@[^\s@]+\.[^\s@]+$/.test(email) || name.length > 100 || (list !== "cloud" && list !== "competition")) {
    return json({ error: "Please enter a valid email address." }, 400);
  }
  const url = process.env.CONVEX_URL;
  const secret = process.env.WAITLIST_SECRET;
  if (!url || !secret) return json({ error: "Signups are temporarily unavailable. Please try again shortly." }, 503);
  // Vercel overwrites this header at the edge. Never store raw visitor IP addresses.
  const address = request.headers.get("x-vercel-forwarded-for")?.split(",")[0]?.trim() || "local";
  const hash = await crypto.subtle.digest("SHA-256", new TextEncoder().encode(`${secret}:${address}`));
  const fingerprint = Array.from(new Uint8Array(hash), b => b.toString(16).padStart(2, "0")).join("");
  try {
    const client = new ConvexHttpClient(url);
    const result = await client.mutation(api.waitlist.join, { email, name, list, secret, fingerprint });
    if (!result.ok) return json({ error: result.reason === "rate" ? "Too many attempts. Please try again in a minute." : "Please enter a valid email address." }, result.reason === "rate" ? 429 : 400);
    return json({ ok: true });
  } catch {
    // Do not log email addresses or credentials.
    console.error("Waitlist storage unavailable");
    return json({ error: "We couldn’t save your signup. Please try again shortly." }, 503);
  }
}
