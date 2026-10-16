import {
  action,
  internalMutation,
  internalQuery,
  mutation,
} from "./_generated/server";
import { internal } from "./_generated/api";
import { v } from "convex/values";
import { access, deviceArg, fail } from "./cloudAuth";
import type { QueryCtx, MutationCtx } from "./_generated/server";
import * as policy from "../shared/liveProtocol";
import type { Doc, Id } from "./_generated/dataModel";

const { POLICY_VERSION } = policy;
function checked<T>(operation: () => T): T {
  try {
    return operation();
  } catch (error) {
    if (error instanceof policy.LiveProtocolError) fail(error.message);
    throw error;
  }
}
const requireProtocol: typeof policy.requireProtocol = (...args) =>
  checked(() => policy.requireProtocol(...args));
const seedProtocol: typeof policy.seedProtocol = (...args) =>
  checked(() => policy.seedProtocol(...args));
const changesProtocol: typeof policy.changesProtocol = (...args) =>
  checked(() => policy.changesProtocol(...args));
const preflightCreate: typeof policy.preflightCreate = (...args) =>
  checked(() => policy.preflightCreate(...args));

const presence = v.object({
  actor: v.string(),
  name: v.string(),
  cursor: v.union(v.null(), v.object({ x: v.number(), y: v.number() })),
  selection: v.array(v.string()),
});
const id = v.object({ actor: v.string(), seq: v.number() });
const change = v.object({
  id,
  clock: v.number(),
  seen: v.record(v.string(), v.number()),
  edit: v.union(
    v.object({
      type: v.literal("write"),
      writes: v.array(
        v.object({
          path: v.string(),
          value: v.union(
            v.null(),
            v.literal("Object"),
            v.literal("List"),
            v.object({
              Pixels: v.object({
                w: v.number(),
                h: v.number(),
                data: v.string(),
                native: v.optional(
                  v.object({
                    depth: v.union(
                      v.literal("U8"),
                      v.literal("U16"),
                      v.literal("F32"),
                    ),
                    samples: v.string(),
                    cmyk: v.optional(v.string()),
                    icc_profile: v.string(),
                    icc_bytes: v.number(),
                    intent: v.number(),
                  }),
                ),
              }),
            }),
            v.object({ Variant: v.string() }),
            v.object({ Value: v.any() }),
          ),
        }),
      ),
    }),
    v.object({ type: v.literal("toggle"), target: id, enabled: v.boolean() }),
  ),
});
const args = {
  ...deviceArg,
  session: v.id("cloudLiveSessions"),
  actor: v.string(),
  protocol: v.optional(v.number()),
};
const cursor = v.union(v.null(), v.object({ x: v.number(), y: v.number() }));

async function authorized(
  ctx: QueryCtx | MutationCtx,
  session: Id<"cloudLiveSessions">,
  token?: string,
) {
  const room = await ctx.db.get(session);
  if (!room) fail("Live session not found.");
  const { user } = await access(ctx, room.projectId, token, "write");
  return { room, user };
}
function actorId(actor: string) {
  if (!/^[a-f0-9]{64}$/.test(actor)) fail("Invalid live client ID.");
}
function integer(n: number) {
  if (!Number.isSafeInteger(n) || n < 1) fail("Invalid live sequence.");
}

async function certifiedProtocol(
  ctx: QueryCtx | MutationCtx,
  room: Doc<"cloudLiveSessions">,
) {
  if (
    room.protocolPolicy !== undefined &&
    room.protocolPolicy !== POLICY_VERSION
  )
    fail("Unsupported live capability policy.");
  if (room.protocolPolicy === POLICY_VERSION) {
    requireProtocol(3, room.protocol ?? 1);
    return room.protocol ?? 1;
  }
  const page = await ctx.db
    .query("cloudLiveChanges")
    .withIndex("by_session_revision", (q) => q.eq("session", room._id))
    .paginate({ cursor: null, numItems: 8, maximumBytesRead: 2_000_000 });
  if (!page.isDone)
    fail("Live history requires complete capability certification before use.");
  return Math.max(
    room.protocol ?? 1,
    seedProtocol(room.seed),
    changesProtocol(page.page.map((row) => JSON.parse(row.payload))),
  );
}

/** Certify the actual seed and every stored write with bounded read-only pages.
 * Args: authenticated room, offer and scan cursor. Returns a capability summary without mutation.
 */
export const certificationPage = internalQuery({
  args: {
    ...deviceArg,
    session: v.id("cloudLiveSessions"),
    protocol: v.optional(v.number()),
    cursor: v.union(v.null(), v.string()),
  },
  returns: v.object({
    revision: v.number(),
    required: v.number(),
    cursor: v.union(v.null(), v.string()),
    done: v.boolean(),
    bytesRead: v.number(),
  }),
  handler: async (ctx, a) => {
    const { room } = await authorized(ctx, a.session, a.deviceToken);
    if (
      room.protocolPolicy !== undefined &&
      room.protocolPolicy !== POLICY_VERSION
    )
      fail("Unsupported live capability policy.");
    requireProtocol(a.protocol, room.protocol ?? 1);
    const page = await ctx.db
      .query("cloudLiveChanges")
      .withIndex("by_session_revision", (q) => q.eq("session", a.session))
      .paginate({ cursor: a.cursor, numItems: 8, maximumBytesRead: 2_000_000 });
    const required = Math.max(
      room.protocol ?? 1,
      seedProtocol(room.seed),
      changesProtocol(page.page.map((row) => JSON.parse(row.payload))),
    );
    requireProtocol(a.protocol, required);
    return {
      revision: room.revision,
      required,
      cursor: page.isDone ? null : page.continueCursor,
      done: page.isDone,
      bytesRead: (await ctx.meta.getTransactionMetrics()).bytesRead.used,
    };
  },
});
export const certifyComplete = internalMutation({
  args: {
    ...deviceArg,
    session: v.id("cloudLiveSessions"),
    protocol: v.optional(v.number()),
    revision: v.number(),
    required: v.number(),
  },
  returns: v.null(),
  handler: async (ctx, a) => {
    const { room } = await authorized(ctx, a.session, a.deviceToken);
    if (
      room.protocolPolicy !== undefined &&
      room.protocolPolicy !== POLICY_VERSION
    )
      fail("Unsupported live capability policy.");
    if (room.revision !== a.revision)
      fail("Live history changed during certification. Retry.");
    const required = Math.max(
      room.protocol ?? 1,
      a.required,
      seedProtocol(room.seed),
    );
    requireProtocol(a.protocol, required);
    await ctx.db.patch(a.session, {
      protocol: required,
      protocolPolicy: POLICY_VERSION,
    });
    return null;
  },
});
export const certify = action({
  args: {
    ...deviceArg,
    session: v.id("cloudLiveSessions"),
    protocol: v.optional(v.number()),
  },
  returns: v.null(),
  handler: async (ctx, a): Promise<null> => {
    let cursor: string | null = null,
      revision: number | undefined,
      required = 1;
    for (let count = 0; count < 256; count++) {
      const page: {
        revision: number;
        required: number;
        cursor: string | null;
        done: boolean;
        bytesRead: number;
      } = await ctx.runQuery(internal.live.certificationPage, { ...a, cursor });
      if (revision !== undefined && revision !== page.revision)
        fail("Live history changed during certification. Retry.");
      revision = page.revision;
      required = Math.max(required, page.required);
      if (page.done)
        return await ctx.runMutation(internal.live.certifyComplete, {
          ...a,
          revision,
          required,
        });
      cursor = page.cursor;
    }
    fail(
      "Live history exceeds the bounded certification window; keep the room closed.",
    );
  },
});

export const create = mutation({
  args: {
    ...deviceArg,
    projectId: v.id("cloudProjects"),
    seed: v.string(),
    protocol: v.optional(v.number()),
  },
  returns: v.id("cloudLiveSessions"),
  handler: async (ctx, a) => {
    await access(ctx, a.projectId, a.deviceToken, "write");
    const protocol = preflightCreate(a.seed, a.protocol);
    let quota = await ctx.db
      .query("cloudLiveQuotas")
      .withIndex("by_project", (q) => q.eq("projectId", a.projectId))
      .unique();
    if (!quota) {
      const legacy = await ctx.db
        .query("cloudLiveSessions")
        .withIndex("by_project", (q) => q.eq("projectId", a.projectId))
        .take(1);
      if (legacy.length)
        fail(
          "Initialize the live session quota before creating another session.",
        );
      const quotaId = await ctx.db.insert("cloudLiveQuotas", {
        projectId: a.projectId,
        count: 0,
        ready: true,
        cursor: null,
      });
      quota = (await ctx.db.get(quotaId))!;
    }
    if (!quota.ready)
      fail("Live session quota is initializing. Retry shortly.");
    if (quota.count >= 100) fail("Live session limit reached.");
    const session = await ctx.db.insert("cloudLiveSessions", {
      projectId: a.projectId,
      seed: a.seed,
      revision: 0,
      protocol,
      protocolPolicy: POLICY_VERSION,
    });
    await ctx.db.patch(quota._id, { count: quota.count + 1 });
    return session;
  },
});

export const initializeQuotaPage = internalMutation({
  args: {
    ...deviceArg,
    projectId: v.id("cloudProjects"),
  },
  returns: v.object({ ready: v.boolean(), bytesRead: v.number() }),
  handler: async (ctx, a) => {
    await access(ctx, a.projectId, a.deviceToken, "write");
    const quota = await ctx.db
      .query("cloudLiveQuotas")
      .withIndex("by_project", (q) => q.eq("projectId", a.projectId))
      .unique();
    if (quota?.ready)
      return {
        ready: true,
        bytesRead: (await ctx.meta.getTransactionMetrics()).bytesRead.used,
      };
    const count = quota?.count ?? 0;
    const cursor = quota?.cursor ?? null;
    const page = await ctx.db
      .query("cloudLiveSessions")
      .withIndex("by_project", (q) => q.eq("projectId", a.projectId))
      .paginate({ cursor, numItems: 1, maximumBytesRead: 600_000 });
    const total = count + page.page.length;
    const ready = page.isDone || total >= 100;
    const value = {
      projectId: a.projectId,
      count: total,
      ready,
      cursor: ready ? null : page.continueCursor,
    };
    if (quota) await ctx.db.replace(quota._id, value);
    else await ctx.db.insert("cloudLiveQuotas", value);
    return {
      ready,
      bytesRead: (await ctx.meta.getTransactionMetrics()).bytesRead.used,
    };
  },
});

export const initializeQuota = action({
  args: { ...deviceArg, projectId: v.id("cloudProjects") },
  returns: v.null(),
  handler: async (ctx, a) => {
    for (let page = 0; page <= 100; page++) {
      const progress = await ctx.runMutation(
        internal.live.initializeQuotaPage,
        a,
      );
      if (progress.ready) return null;
    }
    fail("Live session quota initialization needs a retry.");
  },
});

export const join = mutation({
  args,
  returns: v.object({
    seed: v.string(),
    projectId: v.id("cloudProjects"),
    protocol: v.number(),
    supportedProtocol: v.number(),
  }),
  handler: async (ctx, a) => {
    actorId(a.actor);
    const { room, user } = await authorized(ctx, a.session, a.deviceToken);
    const protocol = await certifiedProtocol(ctx, room);
    const supportedProtocol = requireProtocol(a.protocol, protocol);
    const old = await ctx.db
      .query("cloudLiveClients")
      .withIndex("by_session_actor", (q) =>
        q.eq("session", a.session).eq("actor", a.actor),
      )
      .unique();
    if (old && old.userId !== user.userId)
      fail("This live client belongs to another account.");
    if (old)
      await ctx.db.patch(old._id, { updated: Date.now(), name: user.name });
    else {
      const clients = await ctx.db
        .query("cloudLiveClients")
        .withIndex("by_session_updated", (q) => q.eq("session", a.session))
        .take(1000);
      if (clients.length >= 1000) fail("Live client limit reached.");
      await ctx.db.insert("cloudLiveClients", {
        session: a.session,
        actor: a.actor,
        userId: user.userId,
        name: user.name,
        updated: Date.now(),
        seq: 0,
        cursor: null,
        selection: [],
      });
    }
    if (room.protocolPolicy !== POLICY_VERSION)
      await ctx.db.patch(a.session, {
        protocol,
        protocolPolicy: POLICY_VERSION,
      });
    return {
      seed: room.seed,
      projectId: room.projectId,
      protocol,
      supportedProtocol: Math.max(2, supportedProtocol),
    };
  },
});

export const exchange = mutation({
  args: {
    ...args,
    changes: v.array(change),
    cursor,
    selection: v.array(v.string()),
    after: v.number(),
  },
  returns: v.object({
    acknowledged: v.array(id),
    changes: v.array(change),
    cursor: v.number(),
    more: v.boolean(),
    presence: v.array(presence),
    protocol: v.number(),
    supportedProtocol: v.number(),
  }),
  handler: async (ctx, a) => {
    actorId(a.actor);
    const { room, user } = await authorized(ctx, a.session, a.deviceToken);
    const protocol = Math.max(
      await certifiedProtocol(ctx, room),
      changesProtocol(a.changes),
    );
    const supportedProtocol = requireProtocol(a.protocol, protocol);
    const client = await ctx.db
      .query("cloudLiveClients")
      .withIndex("by_session_actor", (q) =>
        q.eq("session", a.session).eq("actor", a.actor),
      )
      .unique();
    if (!client || client.userId !== user.userId)
      fail("Join this live session first.");
    if (
      !Number.isSafeInteger(a.after) ||
      a.after < 0 ||
      a.after > room.revision
    )
      fail("Invalid live cursor.");
    if (
      a.changes.length > 64 ||
      JSON.stringify(a.changes).length > 200_000 ||
      a.selection.length > 100 ||
      a.selection.some((s) => s.length > 30)
    )
      fail("Live batch too large.");
    if (
      a.cursor &&
      (!Number.isFinite(a.cursor.x) || !Number.isFinite(a.cursor.y))
    )
      fail("Invalid live cursor.");
    let seq = client.seq;
    let revision = room.revision;
    for (const operation of a.changes) {
      if (operation.id.actor !== a.actor)
        fail("Cannot submit another client's operations.");
      integer(operation.id.seq);
      integer(operation.clock);
      if (
        Object.keys(operation.seen).length > 1000 ||
        Object.entries(operation.seen).some(
          ([actor, n]) =>
            !/^[a-f0-9]{64}$/.test(actor) || !Number.isSafeInteger(n) || n < 0,
        )
      )
        fail("Invalid live causal history.");
      const payload = JSON.stringify(operation);
      if (operation.id.seq <= seq) {
        const old = await ctx.db
          .query("cloudLiveChanges")
          .withIndex("by_session_actor_seq", (q) =>
            q
              .eq("session", a.session)
              .eq("actor", a.actor)
              .eq("seq", operation.id.seq),
          )
          .unique();
        if (!old || old.payload !== payload)
          fail("Live operation ID reused with different data.");
        continue;
      }
      if (operation.id.seq !== seq + 1)
        fail("Live operations must be submitted in order.");
      if (operation.clock > revision + 1) fail("Invalid live logical clock.");
      if (operation.edit.type === "toggle") {
        if (operation.edit.target.actor !== a.actor)
          fail("Cannot undo another client's changes.");
        const target = await ctx.db
          .query("cloudLiveChanges")
          .withIndex("by_session_actor_seq", (q) =>
            q
              .eq("session", a.session)
              .eq("actor", a.actor)
              .eq(
                "seq",
                operation.edit.type === "toggle"
                  ? operation.edit.target.seq
                  : 0,
              ),
          )
          .unique();
        if (!target || JSON.parse(target.payload).edit.type !== "write")
          fail("Undo target is not an authored edit.");
      } else if (
        !operation.edit.writes.length ||
        operation.edit.writes.length > 20_000 ||
        operation.edit.writes.some(
          (w) =>
            w.path.length > 4096 ||
            (w.path !== "" && !w.path.startsWith("/")) ||
            /^\/(cloud|collaboration|workspace)(\/|$)/.test(w.path),
        )
      )
        fail("Invalid live property edits.");
      seq = operation.id.seq;
      await ctx.db.insert("cloudLiveChanges", {
        session: a.session,
        actor: a.actor,
        seq,
        revision: ++revision,
        payload,
      });
    }
    if (
      revision !== room.revision ||
      protocol !== (room.protocol ?? 1) ||
      room.protocolPolicy !== POLICY_VERSION
    )
      await ctx.db.patch(a.session, {
        revision,
        protocol,
        protocolPolicy: POLICY_VERSION,
      });
    await ctx.db.patch(client._id, {
      seq,
      updated: Date.now(),
      cursor: a.cursor,
      selection: a.selection,
      name: user.name,
    });
    const rows = await ctx.db
      .query("cloudLiveChanges")
      .withIndex("by_session_revision", (q) =>
        q.eq("session", a.session).gt("revision", a.after),
      )
      .take(2);
    const page = rows.slice(0, 1);
    const peers = await ctx.db
      .query("cloudLiveClients")
      .withIndex("by_session_updated", (q) =>
        q.eq("session", a.session).gt("updated", Date.now() - 30_000),
      )
      .take(100);
    return {
      protocol,
      supportedProtocol: Math.max(2, supportedProtocol),
      acknowledged: a.changes.map((c) => c.id),
      changes: page.map((r) => JSON.parse(r.payload)),
      cursor: page.at(-1)?.revision ?? a.after,
      more: rows.length > 1,
      presence: peers
        .filter((p) => p.actor !== a.actor)
        .map((p) => ({
          actor: p.actor,
          name: p.name,
          cursor: p.cursor,
          selection: p.selection,
        })),
    };
  },
});

export const leave = mutation({
  args,
  returns: v.null(),
  handler: async (ctx, a) => {
    const { user } = await authorized(ctx, a.session, a.deviceToken);
    const client = await ctx.db
      .query("cloudLiveClients")
      .withIndex("by_session_actor", (q) =>
        q.eq("session", a.session).eq("actor", a.actor),
      )
      .unique();
    if (client?.userId === user.userId)
      await ctx.db.patch(client._id, {
        updated: 0,
        cursor: null,
        selection: [],
      });
    return null;
  },
});
