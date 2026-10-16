/// <reference types="vite/client" />
import { convexTest } from "convex-test";
import { describe, expect, it, vi } from "vitest";
import schema from "./schema";
import type { Id } from "./_generated/dataModel";
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
      "/name": { Value: "x".repeat(499900) },
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
      "/name": { Value: "x".repeat(499900) },
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

describe("Native preservation protocol", () => {
  const native = { Pixels: { w: 1, h: 1, data: "display", native: {
    depth: "F32" as const, samples: "samples", cmyk: "inks",
    icc_profile: "icc", icc_bytes: 128, intent: 0,
  } } };
  it("refuses native seeds and old joins before quota/session/actor writes", async () => {
    const {t,owner,projectId} = await setup();
    const seed=JSON.stringify({"":"Object","/@entities/1/mask":native});
    const rows=()=>t.run(async ctx=>({rooms:await ctx.db.query("cloudLiveSessions").take(10),quota:await ctx.db.query("cloudLiveQuotas").take(10),clients:await ctx.db.query("cloudLiveClients").take(10)}));
    const before=await rows();
    await expect(owner.mutation(api.live.create,{projectId,seed})).rejects.toThrow("native-preservation");
    expect(await rows()).toEqual(before);
    const session=await owner.mutation(api.live.create,{projectId,seed,protocol:2});
    await expect(owner.mutation(api.live.join,{session,actor:"c".repeat(64)})).rejects.toThrow("native-preservation");
    expect(await t.run(ctx=>ctx.db.query("cloudLiveClients").withIndex("by_session_actor",q=>q.eq("session",session)).take(10))).toEqual([]);
    const joined=await owner.mutation(api.live.join,{session,actor:"c".repeat(64),protocol:2});
    expect(joined.protocol).toBe(2);expect(joined.supportedProtocol).toBe(2);
    expect(JSON.parse(joined.seed)["/@entities/1/mask"]).toEqual(native);
  });
  it("upgrades neutral rooms atomically and rejects old mutations and empty polls", async()=>{
    const {t,owner,editor,session,left,right,exchange}=await setup();
    const operation={...write(alice,1,"native"),edit:{type:"write" as const,writes:[{path:"/@entities/1/mask",value:native}]}};
    const before=await t.run(ctx=>ctx.db.get(session));
    await expect(owner.mutation(api.live.exchange,{...left,...exchange,changes:[operation]})).rejects.toThrow("native-preservation");
    expect(await t.run(ctx=>ctx.db.get(session))).toEqual(before);
    expect((await editor.mutation(api.live.exchange,{...right,...exchange})).changes).toEqual([]);
    const accepted=await owner.mutation(api.live.exchange,{...left,...exchange,changes:[operation],protocol:2});
    expect(accepted.acknowledged).toEqual([operation.id]);
    const rows=()=>t.run(async ctx=>({room:await ctx.db.get(session),clients:await ctx.db.query("cloudLiveClients").withIndex("by_session_actor",q=>q.eq("session",session)).take(10),changes:await ctx.db.query("cloudLiveChanges").withIndex("by_session_revision",q=>q.eq("session",session)).take(10)}));
    const frozen=await rows();
    for(const changes of [[],[write(bob,1,"lost native")]])
      await expect(editor.mutation(api.live.exchange,{...right,...exchange,changes})).rejects.toThrow("native-preservation");
    await expect(editor.mutation(api.live.join,right)).rejects.toThrow("native-preservation");
    expect(await rows()).toEqual(frozen);
    expect((await editor.mutation(api.live.exchange,{...right,...exchange,protocol:2})).changes).toEqual([operation]);
  });
  it("gates document color separately and rolls back a failed native upgrade",async()=>{
    const {t,owner,projectId,session,left,exchange}=await setup();
    for(const color of [{model:"Cmyk",depth:"U8",icc_profile:[],intent:0},{model:"Rgb",depth:"F32",icc_profile:[],intent:0},{model:"Rgb",depth:"U8",icc_profile:[1,2],intent:0},{model:"Rgb",depth:"U8",icc_profile:[],intent:1}]) {
      await expect(owner.mutation(api.live.create,{projectId,seed:JSON.stringify({"":"Object","/color":{Value:color}})})).rejects.toThrow("native-preservation");
    }
    const bad={...write(alice,2,"bad"),edit:{type:"write" as const,writes:[{path:"/@entities/1/mask",value:native}]}};
    await expect(owner.mutation(api.live.exchange,{...left,...exchange,changes:[bad],protocol:2})).rejects.toThrow("in order");
    expect((await t.run(ctx=>ctx.db.get(session)))?.protocol).toBe(1);
    await owner.mutation(api.live.create,{projectId,seed:JSON.stringify({"":"Object","/color":{Value:{model:"Rgb",depth:"U8",icc_profile:[],intent:0}}})});
  });
});

describe("Motion semantic capability and historical certification",()=>{
  const advanced=(actor:string,seq=1)=>({...write(actor,seq,"motion"),edit:{type:"write" as const,writes:[{path:"/motion/expressions",value:{Value:[{expression:"time"}]}}]}});
  const state=(t:Awaited<ReturnType<typeof setup>>["t"],session:Id<"cloudLiveSessions">)=>t.run(async ctx=>({room:await ctx.db.get(session),clients:await ctx.db.query("cloudLiveClients").withIndex("by_session_actor",q=>q.eq("session",session)).take(100),log:await ctx.db.query("cloudLiveChanges").withIndex("by_session_revision",q=>q.eq("session",session)).take(100),quotas:await ctx.db.query("cloudLiveQuotas").take(100)}));
  it("refuses old advanced create and join before quota or actor mutation, and negotiates the exact offer",async()=>{
    const {t,owner,projectId,session,left}=await setup();
    const seed=JSON.stringify({"":"Object","/motion/expressions":{Value:[{expression:"time"}]}});
    const before=await state(t,session);
    await expect(owner.mutation(api.live.create,{projectId,seed,protocol:2})).rejects.toThrow("protocol 3");
    expect(await state(t,session)).toEqual(before);
    const motion=await owner.mutation(api.live.create,{projectId,seed,protocol:3});
    await expect(owner.mutation(api.live.join,{session:motion,actor:alice,protocol:2})).rejects.toThrow("protocol 3");
    expect((await state(t,motion)).clients).toEqual([]);
    expect((await owner.mutation(api.live.join,{session:motion,actor:alice,protocol:3})).supportedProtocol).toBe(3);
    for(const protocol of [2,3]) expect((await owner.mutation(api.live.join,{...left,protocol})).supportedProtocol).toBe(protocol);
    expect((await t.run(ctx=>ctx.db.get(session)))?.protocol).toBe(1);
  });
  it("refuses old writes and all stale polls after an atomic upgrade, retaining removal and withdrawal",async()=>{
    const {t,owner,editor,session,left,right,exchange}=await setup();
    await editor.mutation(api.live.join,{...right,protocol:2});
    const operation=advanced(alice),before=await state(t,session);
    await expect(owner.mutation(api.live.exchange,{...left,...exchange,protocol:2,changes:[operation]})).rejects.toThrow("protocol 3");
    expect(await state(t,session)).toEqual(before);
    await owner.mutation(api.live.exchange,{...left,...exchange,protocol:3,changes:[operation]});
    const frozen=await state(t,session);
    for(const changes of [[],[write(bob,1,"old")]]) await expect(editor.mutation(api.live.exchange,{...right,...exchange,protocol:2,changes})).rejects.toThrow("protocol 3");
    await expect(editor.mutation(api.live.join,{...right,protocol:2})).rejects.toThrow("protocol 3");
    expect(await state(t,session)).toEqual(frozen);
    const remove={...write(alice,2,"remove"),edit:{type:"write" as const,writes:[{path:"/motion/expressions",value:null}]}};
    const undo={id:{actor:alice,seq:3},clock:3,seen:{[alice]:2},edit:{type:"toggle" as const,target:operation.id,enabled:false}};
    await owner.mutation(api.live.exchange,{...left,...exchange,protocol:3,changes:[remove,undo]});
    expect((await t.run(ctx=>ctx.db.get(session)))?.protocol).toBe(3);
    await expect(editor.mutation(api.live.exchange,{...right,...exchange,protocol:2})).rejects.toThrow("protocol 3");
    expect((await owner.mutation(api.live.exchange,{...left,...exchange,protocol:3})).protocol).toBe(3);
  });
  it("rolls back the whole log, requirement, acknowledgement and presence on a failed 3 operation",async()=>{
    const {t,owner,session,left,exchange}=await setup();
    const before=await state(t,session);
    await expect(owner.mutation(api.live.exchange,{...left,...exchange,protocol:3,cursor:{x:4,y:5},changes:[advanced(alice),write(alice,3,"out of sequence")]})).rejects.toThrow("in order");
    expect(await state(t,session)).toEqual(before);
    await expect(owner.mutation(api.live.exchange,{...left,...exchange,protocol:3,changes:[{...write(alice,1,"future"),edit:{type:"write",writes:[{path:"/motion/layout28",value:{Value:{}}}]}}]})).rejects.toThrow("Unsupported");
    expect(await state(t,session)).toEqual(before);
  });
  it("certifies the actual complete legacy log, refusing seed-only and latest-page inference",async()=>{
    const {t,owner,projectId,exchange}=await setup();
    const session=await t.run(async ctx=>{
      const room=await ctx.db.insert("cloudLiveSessions",{projectId,seed:'{"":"Object"}',revision:18,protocol:1});
      for(let seq=1;seq<=18;seq++) {
        const payload=seq===9?advanced(alice,9):seq===18?{id:{actor:alice,seq},clock:seq,seen:{},edit:{type:"toggle",target:{actor:alice,seq:9},enabled:false}}:write(alice,seq,"neutral");
        await ctx.db.insert("cloudLiveChanges",{session:room,actor:alice,seq,revision:seq,payload:JSON.stringify(payload)});
      }
      return room;
    });
    const before=await state(t,session);
    await expect(owner.mutation(api.live.join,{session,actor:bob,protocol:2})).rejects.toThrow("certification");
    await expect(owner.action(api.live.certify,{session,protocol:2})).rejects.toThrow("protocol 3");
    expect(await state(t,session)).toEqual(before);
    const page=await owner.query(internal.live.certificationPage,{session,protocol:3,cursor:null});
    expect(page.done).toBe(false); expect(page.required).toBe(1); expect(page.bytesRead).toBeLessThan(3_000_000);
    await owner.action(api.live.certify,{session,protocol:3});
    expect((await t.run(ctx=>ctx.db.get(session)))?.protocol).toBe(3);
    await expect(owner.mutation(api.live.join,{session,actor:bob,protocol:2})).rejects.toThrow("protocol 3");
    await owner.mutation(api.live.join,{session,actor:bob,protocol:3});
    expect((await owner.mutation(api.live.exchange,{session,actor:bob,protocol:3,...exchange})).protocol).toBe(3);
  });
  it("never certifies a changed revision or mutates a small incompatible legacy room on refusal",async()=>{
    const {t,owner,projectId}=await setup();
    const session=await t.run(async ctx=>{
      const room=await ctx.db.insert("cloudLiveSessions",{projectId,seed:'{"":"Object"}',revision:1,protocol:1});
      await ctx.db.insert("cloudLiveChanges",{session:room,actor:alice,seq:1,revision:1,payload:JSON.stringify(advanced(alice))}); return room;
    });
    const before=await state(t,session);
    await expect(owner.mutation(api.live.join,{session,actor:bob,protocol:2})).rejects.toThrow("protocol 3");
    await expect(owner.mutation(internal.live.certifyComplete,{session,protocol:3,revision:0,required:3})).rejects.toThrow("changed");
    expect(await state(t,session)).toEqual(before);
  });
});

describe("Complete common reader capability",()=>{
  it("refuses Layout28 before quota, room, log or presence mutation even with offer 3", async()=>{
    const {readFileSync}=await import("node:fs");
    const fixtures=JSON.parse(readFileSync(new URL("../shared/liveProtocol.fixtures.json",import.meta.url),"utf8"));
    const {t,owner,projectId,session,left,exchange}=await setup();
    const rows=()=>t.run(async ctx=>({room:await ctx.db.get(session),quota:await ctx.db.query("cloudLiveQuotas").take(10),clients:await ctx.db.query("cloudLiveClients").withIndex("by_session_actor",q=>q.eq("session",session)).take(10),log:await ctx.db.query("cloudLiveChanges").withIndex("by_session_revision",q=>q.eq("session",session)).take(100)}));
    const before=await rows();
    for(const c of fixtures.filter((c:any)=>c.error && c.name.startsWith("layout28"))) {
      await expect(owner.mutation(api.live.create,{projectId,protocol:3,seed:JSON.stringify({"":"Object",[c.path]:c.cell})})).rejects.toThrow("Unsupported");
      const operation={...write(alice,1,"layout"),edit:{type:"write" as const,writes:[{path:c.path,value:c.cell}]}};
      await expect(owner.mutation(api.live.exchange,{...left,...exchange,protocol:3,changes:[operation]})).rejects.toThrow("Unsupported");
      expect(await rows()).toEqual(before);
    }
  });
  it("rejects known common-only creates and edits before any state change, while 3 retains exact wire cells",async()=>{
    const {readFileSync}=await import("node:fs");
    const fixtures=JSON.parse(readFileSync(new URL("../shared/liveProtocol.fixtures.json",import.meta.url),"utf8"));
    const {t,owner,projectId,session,left,exchange}=await setup();
    const rows=()=>t.run(async ctx=>({room:await ctx.db.get(session),quota:await ctx.db.query("cloudLiveQuotas").take(10),clients:await ctx.db.query("cloudLiveClients").withIndex("by_session_actor",q=>q.eq("session",session)).take(10),log:await ctx.db.query("cloudLiveChanges").withIndex("by_session_revision",q=>q.eq("session",session)).take(100)}));
    const before=await rows();
    const common=fixtures.filter((c:any)=>c.group==="common" && c.required===3);
    for(const c of common) {
      await expect(owner.mutation(api.live.create,{projectId,protocol:2,seed:JSON.stringify({"":"Object",[c.path]:c.cell})})).rejects.toThrow("protocol 3");
      const operation={...write(alice,1,"common"),edit:{type:"write" as const,writes:[{path:c.path,value:c.cell}]}};
      await expect(owner.mutation(api.live.exchange,{...left,...exchange,protocol:2,changes:[operation]})).rejects.toThrow("protocol 3");
      expect(await rows()).toEqual(before);
    }
    const operations=common.map((c:any,index:number)=>({...write(alice,index+1,"common"),edit:{type:"write" as const,writes:[{path:c.path,value:c.cell}]}}));
    await owner.mutation(api.live.exchange,{...left,...exchange,protocol:3,changes:operations});
    const saved=await rows();
    expect(saved.room?.protocol).toBe(3);
    expect(saved.log.map(row=>JSON.parse(row.payload))).toEqual(operations);
    const frozen=await rows();
    await expect(owner.mutation(api.live.exchange,{...left,...exchange,protocol:2})).rejects.toThrow("protocol 3");
    expect(await rows()).toEqual(frozen);
  });
});

describe("Old2 safe controls",()=>{
  it("round trips authoritative native cells with neutral common defaults and no requirement promotion",async()=>{
    const {owner,projectId,exchange}=await setup();
    const native={Pixels:{w:1,h:1,data:"exact-display",native:{depth:"F32" as const,samples:"exact-compressed-floats",icc_profile:"exact-icc",icc_bytes:128,intent:0}}};
    const cells={"":"Object","/sources":{Value:[]},"/publishing":{Value:{facing_pages:false,bleed:[0,0,0,0],styles:[],links:{}}},"/@entities/1/mask":native,"/motion":{Value:{tracks:[],expressions:[],compositions:[],scene3d:null}}};
    const seed=JSON.stringify(cells);
    const session=await owner.mutation(api.live.create,{projectId,seed,protocol:2});
    const args={session,actor:alice,protocol:2};
    const joined=await owner.mutation(api.live.join,args);
    expect(joined.protocol).toBe(2);expect(joined.supportedProtocol).toBe(2);expect(joined.seed).toBe(seed);
    const operation={...write(alice,1,"native"),edit:{type:"write" as const,writes:[{path:"/@entities/1/mask",value:native}]}};
    const reply=await owner.mutation(api.live.exchange,{...args,...exchange,changes:[operation]});
    expect(reply.protocol).toBe(2);expect(reply.supportedProtocol).toBe(2);expect(reply.changes).toEqual([operation]);
    expect((await owner.mutation(api.live.exchange,{...args,...exchange,after:reply.cursor})).protocol).toBe(2);
  });
  it("refuses unknown policy metadata before actor creation",async()=>{
    const {t,owner,projectId}=await setup();
    const session=await t.run(ctx=>ctx.db.insert("cloudLiveSessions",{projectId,seed:'{"":"Object"}',revision:0,protocol:1,protocolPolicy:4}));
    await expect(owner.mutation(api.live.join,{session,actor:alice,protocol:3})).rejects.toThrow("Unsupported");
    expect(await t.run(ctx=>ctx.db.query("cloudLiveClients").withIndex("by_session_actor",q=>q.eq("session",session)).take(10))).toEqual([]);
  });
});

describe("Capability error contract",()=>{
  it("returns structured safe policy errors for the existing HTTP credential-redaction boundary",async()=>{
    const {owner,projectId}=await setup();
    try {
      await owner.mutation(api.live.create,{projectId,seed:JSON.stringify({"":"Object","/sources":{Value:[{id:"original"}]}}),protocol:2});
      throw Error("Expected capability refusal");
    } catch(error) {
      expect((error as {data:unknown}).data).toBe("This live document requires complete common+Motion preservation protocol 3.");
    }
  });
});
