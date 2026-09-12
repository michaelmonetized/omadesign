import { createFileRoute } from "@tanstack/react-router";
import { publishedShowcase, showcaseSeed, type ShowcaseItem } from "../../cloud/seed";

type WaitlistEntry = { email: string; name: string; created: number };
type Store = { gallery: ShowcaseItem[]; waitlist: WaitlistEntry[] };

const memory: Store = {
  gallery: [...showcaseSeed],
  waitlist: [],
};

function json(data: unknown, status = 200) {
  return new Response(JSON.stringify(data), {
    status,
    headers: {
      "Content-Type": "application/json",
      "Access-Control-Allow-Origin": "*",
    },
  });
}

export const Route = createFileRoute("/api/cloud")({
  component: () => null,
  server: {
    handlers: {
      GET: async () => json({ gallery: publishedShowcase(memory.gallery) }),
      OPTIONS: async () =>
        new Response(null, {
          status: 204,
          headers: {
            "Access-Control-Allow-Origin": "*",
            "Access-Control-Allow-Methods": "GET,POST,OPTIONS",
            "Access-Control-Allow-Headers": "Content-Type,Authorization",
          },
        }),
      POST: async ({ request }) => {
        const body = (await request.json().catch(() => ({}))) as {
          action?: string;
          email?: string;
          name?: string;
          item?: ShowcaseItem;
          snapshot?: string;
        };
        if (body.action === "waitlist") {
          const email = (body.email ?? "").trim();
          if (!email.includes("@")) return json({ error: "email" }, 400);
          if (!memory.waitlist.some((entry) => entry.email === email)) {
            memory.waitlist.push({
              email,
              name: (body.name ?? "").trim(),
              created: Date.now(),
            });
          }
          return json({ ok: true });
        }
        if (body.action === "publish" && body.item) {
          if (!body.item.published) return json({ error: "private" }, 400);
          memory.gallery.push(body.item);
          return json({ ok: true, id: body.item.id });
        }
        if (body.action === "sync") {
          return json({ ok: true });
        }
        return json({ error: "unknown" }, 400);
      },
    },
  },
});
