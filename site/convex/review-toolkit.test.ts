/// <reference types="vite/client" />
import { convexTest } from "convex-test";
import { it, expect } from "vitest";
import schema from "./schema";
import { api } from "./_generated/api";
const modules = import.meta.glob(["./**/*.ts", "!./**/*.test.ts"]);
async function setup() {
  const t = convexTest(schema, modules);
  const owner = t.withIdentity({
    subject: "owner",
    email: "owner@example.com",
    emailVerified: true,
    name: "Owner",
  });
  const reviewer = t.withIdentity({
    subject: "reviewer",
    email: "reviewer@example.com",
    emailVerified: true,
    name: "Reviewer",
  });
  const projectId = await owner.mutation(api.projects.create, {
    title: "Toolkit",
  });
  const snapshotId = await t.run(async (ctx) => {
    await ctx.db.insert("cloudMembers", {
      projectId,
      userId: "reviewer",
      email: "reviewer@example.com",
      name: "Reviewer",
      role: "reviewer",
    });
    const storageId = await ctx.storage.store(
      new Blob(["image"], { type: "image/png" }),
    );
    return ctx.db.insert("cloudFiles", {
      projectId,
      storageId,
      uploader: "owner",
      name: "flat.png",
      kind: "snapshot",
      size: 5,
      contentType: "image/png",
      created: Date.now(),
      version: 1,
      width: 1200,
      height: 800,
    });
  });
  return { t, owner, reviewer, projectId, snapshotId };
}
it("persists all five reactions and both drawing tools with threads on their original snapshot", async () => {
  const { t, owner, reviewer, projectId, snapshotId } = await setup();
  for (const stamp of ["check", "x", "heart", "question", "exclaim"] as const)
    await reviewer.mutation(api.review.annotate, {
      snapshotId,
      x: 0.2,
      y: 0.3,
      shape: "stamp",
      stamp,
      body: "",
    });
  for (const shape of ["highlight", "brush"] as const)
    await reviewer.mutation(api.review.annotate, {
      snapshotId,
      x: 0.1,
      y: 0.1,
      shape,
      points: [
        { x: 0.1, y: 0.1 },
        { x: 0.9, y: 0.7 },
      ],
      strokeWidth: 0.01,
      color: "#b784ff",
      opacity: shape === "brush" ? 1 : 0.3,
      body: "",
    });
  await reviewer.mutation(api.review.annotate, {
    snapshotId,
    x: 0.5,
    y: 0.5,
    shape: "pin",
    body: "Old pin",
  });
  await reviewer.mutation(api.review.annotate, {
    snapshotId,
    x: 0.8,
    y: 0.7,
    endX: 0.2,
    endY: 0.1,
    shape: "rectangle",
    body: "Reverse rectangle",
  });
  const rows = await owner.query(api.review.list, { projectId, snapshotId });
  expect(rows).toHaveLength(9);
  await reviewer.mutation(api.review.reply, {
    id: rows[0]._id,
    body: "Looks good",
  });
  await owner.mutation(api.review.resolve, { id: rows[0]._id, resolved: true });
  const next = await t.run(async (ctx) => {
    const old = (await ctx.db.get(snapshotId))!;
    const { _id, _creationTime, ...file } = old;
    return ctx.db.insert("cloudFiles", { ...file, version: 2 });
  });
  expect(
    await owner.query(api.review.list, { projectId, snapshotId: next }),
  ).toEqual([]);
  const reloaded = await owner.query(api.review.list, {
    projectId,
    snapshotId,
  });
  expect(reloaded[0].replies[0].body).toBe("Looks good");
  expect(reloaded[0].resolved).toBe(true);
  expect(reloaded[5].points).toEqual([
    { x: 0.1, y: 0.1 },
    { x: 0.9, y: 0.7 },
  ]);
  await owner.mutation(api.review.resolve, {
    id: rows[0]._id,
    resolved: false,
  });
  await reviewer.mutation(api.review.remove, { id: rows[0]._id });
  expect(await t.run((ctx) => ctx.db.query("cloudReplies").collect())).toEqual(
    [],
  );
});
it("rejects out-of-bounds, malformed, oversized, and non-finite geometry", async () => {
  const { owner, snapshotId } = await setup();
  const base = {
    snapshotId,
    x: 0.1,
    y: 0.1,
    shape: "brush" as const,
    points: [
      { x: 0.1, y: 0.1 },
      { x: 0.2, y: 0.2 },
    ],
    strokeWidth: 0.01,
    color: "#ff0000",
    opacity: 1,
    body: "",
  };
  const invalid = [
    { x: -0.1 },
    { y: 1.1 },
    { strokeWidth: 0 },
    { opacity: NaN },
    { opacity: 1.1 },
    { color: "url(secret)" },
    { points: [{ x: 0.1, y: 0.1 }] },
    { points: Array(513).fill({ x: 0.1, y: 0.1 }) },
    {
      points: [
        { x: 0.1, y: 0.1 },
        { x: Infinity, y: 0 },
      ],
    },
    {
      points: [
        { x: 0.1, y: 0.1 },
        { x: 0.1, y: 0.1 },
      ],
    },
    { stamp: "heart" as const },
    { endX: 0.2 },
    { shape: "pin" as const },
  ];
  for (const bad of invalid)
    await expect(
      owner.mutation(api.review.annotate, { ...base, ...bad }),
    ).rejects.toThrow();
  await expect(
    owner.mutation(api.review.annotate, {
      snapshotId,
      x: 0.1,
      y: 0.1,
      shape: "stamp",
      body: "",
    }),
  ).rejects.toThrow();
  await expect(
    owner.mutation(api.review.annotate, {
      snapshotId,
      x: 0.1,
      y: 0.1,
      shape: "rectangle",
      body: "Rectangle",
    }),
  ).rejects.toThrow();
  const points = Array.from({ length: 512 }, (_, i) => ({
    x: 0.1 + i / 1000,
    y: 0.1,
  }));
  await expect(
    owner.mutation(api.review.annotate, { ...base, points }),
  ).resolves.toBeTruthy();
});
it("enforces thread ownership, project isolation, anonymity, and immediate revocation", async () => {
  const { t, owner, reviewer, projectId, snapshotId } = await setup();
  const payload = {
    snapshotId,
    x: 0.2,
    y: 0.3,
    shape: "stamp" as const,
    stamp: "heart" as const,
    body: "",
  };
  const id = await owner.mutation(api.review.annotate, payload);
  await expect(reviewer.mutation(api.review.remove, { id })).rejects.toThrow(
    "Only the author",
  );
  await expect(
    reviewer.mutation(api.review.resolve, { id, resolved: true }),
  ).rejects.toThrow("Only the author");
  await expect(t.query(api.review.list, { projectId })).rejects.toThrow();
  await expect(t.mutation(api.review.annotate, payload)).rejects.toThrow();
  const other = await owner.mutation(api.projects.create, { title: "Other" });
  await expect(
    owner.query(api.review.list, { projectId: other, snapshotId }),
  ).rejects.toThrow();
  await owner.mutation(api.projects.changeMember, {
    projectId,
    userId: "reviewer",
    role: "remove",
  });
  await expect(
    reviewer.mutation(api.review.annotate, payload),
  ).rejects.toThrow();
  await expect(
    reviewer.mutation(api.review.reply, { id, body: "Late" }),
  ).rejects.toThrow();
  await expect(
    reviewer.query(api.review.list, { projectId }),
  ).rejects.toThrow();
});
