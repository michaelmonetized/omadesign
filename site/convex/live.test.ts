/// <reference types="vite/client" />
import { convexTest } from "convex-test";
import { describe, expect, it, vi } from "vitest";
import schema from "./schema";
import { api, internal } from "./_generated/api";
const modules = import.meta.glob(["./**/*.ts", "!./**/*.test.ts"]);
const alice = "a".repeat(64),
  bob = "b".repeat(64);
const write = (actor: string, seq: number, name: string) => ({
  id: { actor, seq },
  clock: seq,
  seen: {},
  edit: {
    type: "write" as const,
    writes: [{ path: "/name", value: { Value: name } }],
  },
});

async function setup() {
  const t = convexTest(schema, modules);
  const owner = t.withIdentity({
    subject: "owner",
    email: "owner@example.com",
    emailVerified: true,
    name: "Alice",
  });
  const editor = t.withIdentity({
    subject: "editor",
    email: "editor@example.com",
    emailVerified: true,
    name: "Bob",
  });
  const reviewer = t.withIdentity({
    subject: "reviewer",
    email: "reviewer@example.com",
    emailVerified: true,
    name: "Reviewer",
  });
  const projectId = await owner.mutation(api.projects.create, {
    title: "Live",
  });
  await t.run(async (ctx) => {
    for (const [userId, role] of [
      ["editor", "editor"],
      ["reviewer", "reviewer"],
    ] as const)
      await ctx.db.insert("cloudMembers", {
        projectId,
        userId,
        role,
        email: `${userId}@example.com`,
        name: userId,
      });
  });
  const session = await owner.mutation(api.live.create, {
    projectId,
    seed: '{"":"Object","/name":{"Value":"Live"}}',
  });
  const left = { session, actor: alice },
    right = { session, actor: bob };
  await owner.mutation(api.live.join, left);
  await editor.mutation(api.live.join, right);
  const exchange = { after: 0, changes: [], cursor: null, selection: [] };
  return {
    t,
    owner,
    editor,
    reviewer,
    projectId,
    session,
    left,
    right,
    exchange,
  };
}

describe("Authenticated live sessions", () => {
  it("keeps two independent clients' offline edits, retries and author-owned undo in a durable ordered log", async () => {
    const { t, owner, editor, session, left, right, exchange } = await setup();
    const a = write(alice, 1, "Alice"),
      b = write(bob, 1, "Bob");
    const first = await owner.mutation(api.live.exchange, {
      ...left,
      ...exchange,
      changes: [a],
      cursor: { x: 10, y: 20 },
      selection: ["123"],
    });
    expect(first.acknowledged).toEqual([a.id]);
    await editor.mutation(api.live.exchange, {
      ...right,
      ...exchange,
      changes: [b],
    });
    const retry = await owner.mutation(api.live.exchange, {
      ...left,
      ...exchange,
      changes: [a],
    });
    expect(retry.changes).toEqual([a]);
    expect(retry.more).toBe(true);
    const reconnect = await owner.mutation(api.live.join, left);
    expect(JSON.parse(reconnect.seed)[""]).toBe("Object");
    const next = await owner.mutation(api.live.exchange, {
      ...left,
      ...exchange,
      after: retry.cursor,
    });
    expect(next.changes).toEqual([b]);
    const undo = {
      id: { actor: bob, seq: 2 },
      clock: 2,
      seen: { [bob]: 1 },
      edit: { type: "toggle" as const, target: b.id, enabled: false },
    };
    await editor.mutation(api.live.exchange, {
      ...right,
      ...exchange,
      after: 2,
      changes: [undo],
    });
    const log = await t.run((ctx) =>
      ctx.db
        .query("cloudLiveChanges")
        .withIndex("by_session_revision", (q) => q.eq("session", session))
        .collect(),
    );
    expect(log.map((row) => JSON.parse(row.payload))).toEqual([a, b, undo]);
    expect(
      next.presence.some((peer) => peer.actor === bob && peer.name === "Bob"),
    ).toBe(true);
  });

  it("denies reviewer reads/writes, actor impersonation, forged undo, and revoked or archived access", async () => {
    const {
      t,
      owner,
      editor,
      reviewer,
      projectId,
      session,
      left,
      right,
      exchange,
    } = await setup();
    await expect(
      reviewer.mutation(api.live.join, { session, actor: "c".repeat(64) }),
    ).rejects.toThrow("access");
    await expect(
      reviewer.mutation(api.live.exchange, { ...left, ...exchange }),
    ).rejects.toThrow("access");
    await expect(editor.mutation(api.live.join, left)).rejects.toThrow(
      "another account",
    );
    await expect(
      editor.mutation(api.live.exchange, {
        ...right,
        ...exchange,
        changes: [write(alice, 1, "forged")],
      }),
    ).rejects.toThrow("another client's");
    const a = write(alice, 1, "Alice");
    await owner.mutation(api.live.exchange, {
      ...left,
      ...exchange,
      changes: [a],
    });
    const forged = {
      id: { actor: bob, seq: 1 },
      clock: 2,
      seen: {},
      edit: { type: "toggle" as const, target: a.id, enabled: false },
    };
    await expect(
      editor.mutation(api.live.exchange, {
        ...right,
        ...exchange,
        changes: [forged],
      }),
    ).rejects.toThrow("Cannot undo");
    await t.run(async (ctx) => {
      const member = await ctx.db
        .query("cloudMembers")
        .withIndex("by_project_user", (q) =>
          q.eq("projectId", projectId).eq("userId", "editor"),
        )
        .unique();
      await ctx.db.delete(member!._id);
    });
    await expect(
      editor.mutation(api.live.exchange, { ...right, ...exchange }),
    ).rejects.toThrow("access");
    await t.run((ctx) => ctx.db.patch(projectId, { archived: true }));
    await expect(
      owner.mutation(api.live.exchange, { ...left, ...exchange }),
    ).rejects.toThrow("access");
  });

  it("rejects changed operation IDs, skipped sequences, oversized payloads and metadata writes atomically", async () => {
    const { t, owner, session, left, exchange } = await setup();
    await expect(
      owner.mutation(api.live.exchange, {
        ...left,
        ...exchange,
        changes: [write(alice, 2, "skip")],
      }),
    ).rejects.toThrow("in order");
    const a = write(alice, 1, "A");
    await owner.mutation(api.live.exchange, {
      ...left,
      ...exchange,
      changes: [a],
    });
    await expect(
      owner.mutation(api.live.exchange, {
        ...left,
        ...exchange,
        changes: [write(alice, 1, "changed")],
      }),
    ).rejects.toThrow("reused");
    const impossibleClock = write(alice, 2, "Impossible clock");
    impossibleClock.clock = Number.MAX_SAFE_INTEGER;
    await expect(
      owner.mutation(api.live.exchange, {
        ...left,
        ...exchange,
        changes: [impossibleClock],
      }),
    ).rejects.toThrow("logical clock");
    const meta = write(alice, 2, "token");
    meta.edit.writes[0].path = "/collaboration/actor";
    await expect(
      owner.mutation(api.live.exchange, {
        ...left,
        ...exchange,
        changes: [meta],
      }),
    ).rejects.toThrow("Invalid live property");
    await expect(
      owner.mutation(api.live.exchange, {
        ...left,
        ...exchange,
        changes: [write(alice, 2, "x".repeat(210_000))],
      }),
    ).rejects.toThrow("too large");
    expect(
      await t.run((ctx) =>
        ctx.db
          .query("cloudLiveChanges")
          .withIndex("by_session_revision", (q) => q.eq("session", session))
          .collect(),
      ),
    ).toHaveLength(1);
  });

  it("expires presence and supports an explicit disconnect and reconnect", async () => {
    const { owner, editor, left, right, exchange } = await setup();
    expect(
      (await owner.mutation(api.live.exchange, { ...left, ...exchange }))
        .presence,
    ).toHaveLength(1);
    await editor.mutation(api.live.leave, right);
    expect(
      (await owner.mutation(api.live.exchange, { ...left, ...exchange }))
        .presence,
    ).toEqual([]);
    await editor.mutation(api.live.join, right);
    vi.useFakeTimers();
    try {
      vi.setSystemTime(Date.now() + 31_000);
      expect(
        (await owner.mutation(api.live.exchange, { ...left, ...exchange }))
          .presence,
      ).toEqual([]);
    } finally {
      vi.useRealTimers();
    }
  });

  it("uses approved desktop tokens and immediately rejects a revoked token", async () => {
    const { t, owner, session, exchange } = await setup();
    const token = "d".repeat(64);
    const actor = "e".repeat(64);
    await t.mutation(api.devices.begin, {
      token,
      code: "DDDD1234DDDD",
      label: "Live test",
    });
    await owner.mutation(api.devices.approve, { code: "DDDD1234DDDD" });
    await t.mutation(api.live.join, { session, actor, deviceToken: token });
    await t.mutation(api.live.exchange, {
      session,
      actor,
      deviceToken: token,
      ...exchange,
    });
    await t.mutation(api.devices.disconnect, { deviceToken: token });
    await expect(
      t.mutation(api.live.exchange, {
        session,
        actor,
        deviceToken: token,
        ...exchange,
      }),
    ).rejects.toThrow("Sign in");
  });
  it("creates near-limit seeds using bounded quota metadata and rejects session101", async () => {
    const { owner } = await setup();
    const projectId = await owner.mutation(api.projects.create, {
      title: "Quota read bound",
    });
    const seed = JSON.stringify({
      "": "Object",
      "/payload": { Value: "x".repeat(499900) },
    });
    expect(new TextEncoder().encode(seed).length).toBeLessThan(500000);
    for (let index = 0; index < 34; index++)
      await owner.mutation(api.live.create, { projectId, seed });
    const measured = await owner.mutation(async (ctx) => {
      const session = await ctx.runMutation(
        api.live.create,
        { projectId, seed },
        { transactionLimits: { bytesRead: 8192 } },
      );
      return { session, metrics: await ctx.meta.getTransactionMetrics() };
    });
    expect(measured.metrics.bytesRead.used).toBeLessThan(8192);
    expect(measured.metrics.documentsRead.used).toBeLessThanOrEqual(6);
    for (let index = 35; index < 100; index++)
      await owner.mutation(api.live.create, {
        projectId,
        seed: '{"":"Object"}',
      });
    await expect(
      owner.mutation(api.live.create, { projectId, seed }),
    ).rejects.toThrow("session limit");
    const quota = await owner.query(async (ctx) =>
      ctx.db
        .query("cloudLiveQuotas")
        .withIndex("by_project", (q) => q.eq("projectId", projectId))
        .unique(),
    );
    expect(quota?.count).toBe(100);
  }, 30000);

  it("initializes existing session quotas one bounded page at a time without bypassing the limit", async () => {
    const { t, owner, reviewer } = await setup();
    const projectId = await owner.mutation(api.projects.create, {
      title: "Legacy sessions",
    });
    const seed = JSON.stringify({
      "": "Object",
      "/payload": { Value: "x".repeat(499900) },
    });
    const ids = await t.run(async (ctx) => {
      const ids = [];
      for (let index = 0; index < 3; index++)
        ids.push(
          await ctx.db.insert("cloudLiveSessions", {
            projectId,
            seed,
            revision: 0,
          }),
        );
      return ids;
    });
    await expect(
      owner.mutation(api.live.create, { projectId, seed }),
    ).rejects.toThrow("Initialize");
    const first = await owner.mutation(internal.live.initializeQuotaPage, {
      projectId,
    });
    expect(first.ready).toBe(false);
    expect(first.bytesRead).toBeLessThan(600000);
    await expect(
      owner.mutation(api.live.create, { projectId, seed }),
    ).rejects.toThrow("initializing");
    await owner.action(api.live.initializeQuota, { projectId });
    await owner.mutation(api.live.create, { projectId, seed });
    const joined = await owner.mutation(api.live.join, {
      session: ids[0],
      actor: "f".repeat(64),
    });
    expect(joined.seed).toBe(seed);
    const quota = await owner.query(async (ctx) =>
      ctx.db
        .query("cloudLiveQuotas")
        .withIndex("by_project", (q) => q.eq("projectId", projectId))
        .unique(),
    );
    expect(quota?.count).toBe(4);
    expect(quota?.ready).toBe(true);
    await expect(
      reviewer.action(api.live.initializeQuota, { projectId }),
    ).rejects.toThrow("access");
  });
});
