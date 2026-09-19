import { createFileRoute } from "@tanstack/react-router";
import { ConvexHttpClient } from "convex/browser";
import { makeFunctionReference } from "convex/server";
import { ConvexError } from "convex/values";
const queries = new Set([
  "devices:me",
  "devices:list",
  "projects:list",
  "projects:get",
  "projects:members",
  "projects:invitations",
  "projects:archived",
  "review:list",
  "showcase:project",
  "showcase:competitions",
  "showcase:entries",
]);
const mutations = new Set([
  "devices:begin",
  "devices:approve",
  "devices:revoke",
  "devices:disconnect",
  "projects:create",
  "projects:archive",
  "projects:restore",
  "projects:invite",
  "projects:accept",
  "projects:changeMember",
  "projects:cancelInvite",
  "files:begin",
  "files:finish",
  "review:annotate",
  "review:reply",
  "review:resolve",
  "showcase:publish",
  "showcase:unpublish",
  "showcase:enter",
  "showcase:withdraw",
]);
const json = (data: unknown, status = 200) =>
  Response.json(data, { status, headers: { "Cache-Control": "no-store" } });
export const Route = createFileRoute("/api/cloud")({
  server: {
    handlers: {
      GET: async () =>
        json({
          version: 1,
          convexUrl: process.env.CONVEX_URL,
          convexSiteUrl:
            process.env.CONVEX_SITE_URL || process.env.VITE_CONVEX_SITE_URL,
        }),
      POST: async ({ request }) => {
        if (Number(request.headers.get("content-length")) > 16384)
          return json({ error: "Request too large" }, 413);
        const body = await request.text();
        if (body.length > 16384)
          return json({ error: "Request too large" }, 413);
        try {
          const { operation, args } = JSON.parse(body);
          if (!queries.has(operation) && !mutations.has(operation))
            return json(
              { error: "Unsupported cloud operation. Update Omadesign." },
              400,
            );
          if (!args || typeof args !== "object" || Array.isArray(args))
            return json({ error: "Invalid arguments" }, 400);
          const client = new ConvexHttpClient(process.env.CONVEX_URL!);
          const bearer = request.headers.get("Authorization");
          if (bearer?.startsWith("Bearer ")) client.setAuth(bearer.slice(7));
          const value = queries.has(operation)
            ? await client.query(
                makeFunctionReference<"query">(operation),
                args,
              )
            : await client.mutation(
                makeFunctionReference<"mutation">(operation),
                args,
              );
          return json({ ok: true, value });
        } catch (error) {
          // Validation failures may include request arguments, including device credentials.
          const detail =
            error instanceof ConvexError && typeof error.data === "string"
              ? error.data
              : "Cloud request failed. Check access or update Omadesign and retry.";
          return json({ error: detail.slice(0, 500) }, 400);
        }
      },
    },
  },
});
