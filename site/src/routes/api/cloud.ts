import { handleWaitlist } from "../../server/waitlist";
import { createFileRoute } from "@tanstack/react-router";
import { publishedShowcase, showcaseSeed, type ShowcaseItem } from "../../cloud/seed";

type Store = { gallery: ShowcaseItem[] };

const memory: Store = {
  gallery: [...showcaseSeed],
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
          return handleWaitlist(request, body);
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
