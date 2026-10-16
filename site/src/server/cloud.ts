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
  "live:create",
  "live:join",
  "live:exchange",
  "live:leave",
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

export function discoverCloud() {
  return json({
    version: 1,
    capabilities: { live: 2, native_preservation: 1 },
    convexUrl: process.env.CONVEX_URL,
    convexSiteUrl:
      process.env.CONVEX_SITE_URL || process.env.VITE_CONVEX_SITE_URL,
  });
}

export async function handleCloud(request: Request) {
  const maximum = 1_100_000;
  if (Number(request.headers.get("content-length")) > maximum)
    return json({ error: "Request too large" }, 413);
  let body = "";
  let bytes = 0;
  const reader = request.body?.getReader();
  if (reader) {
    const decoder = new TextDecoder();
    try {
      while (true) {
        const chunk = await reader.read();
        if (chunk.done) break;
        bytes += chunk.value.byteLength;
        if (bytes > maximum) {
          await reader.cancel();
          return json({ error: "Request too large" }, 413);
        }
        body += decoder.decode(chunk.value, { stream: true });
      }
      body += decoder.decode();
    } catch {
      return json({ error: "Invalid request body" }, 400);
    } finally {
      reader.releaseLock();
    }
  }
  try {
    const { operation, args } = JSON.parse(body);
    if (typeof operation !== "string")
      return json({ error: "Invalid operation" }, 400);
    if (!operation.startsWith("live:") && bytes > 16384)
      return json({ error: "Request too large" }, 413);
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
    if (operation === "live:create") {
      await client.action(
        makeFunctionReference<"action">("live:initializeQuota"),
        {
          projectId: args.projectId,
          ...(args.deviceToken === undefined
            ? {}
            : { deviceToken: args.deviceToken }),
        },
      );
    }
    const value = queries.has(operation)
      ? await client.query(makeFunctionReference<"query">(operation), args)
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
}
