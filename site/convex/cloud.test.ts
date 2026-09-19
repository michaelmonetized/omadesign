/// <reference types="vite/client" />
import { convexTest } from "convex-test";
import { describe, it, expect } from "vitest";
import schema from "./schema";
import { api, internal } from "./_generated/api";
const modules = import.meta.glob(["./**/*.ts", "!./**/*.test.ts"]);
function setup() {
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
  return { t, owner, reviewer };
}

describe("Storage ownership and public image removal", () => {
  it("binds upload finalization to its author and rejects expired or reused files", async () => {
    const { t, owner, reviewer } = setup();
    const projectId = await owner.mutation(api.projects.create, { title: "Files" });
    await t.run(ctx => ctx.db.insert("cloudMembers", { projectId, userId: "reviewer", email: "reviewer@example.com", name: "Editor", role: "editor" }));
    const upload = await owner.mutation(api.files.begin, { projectId, kind: "source" });
    const storageId = await t.run(ctx => ctx.storage.store(new Blob(["oma source"], { type: "application/octet-stream" })));
    await expect(reviewer.mutation(api.files.finish, { uploadId: upload.id, storageId, name: "private.oma" })).rejects.toThrow("Upload does not belong to you");
    await owner.mutation(api.files.finish, { uploadId: upload.id, storageId, name: "private.oma" });
    const retry = await owner.mutation(api.files.begin, { projectId, kind: "source" });
    await expect(owner.mutation(api.files.finish, { uploadId: retry.id, storageId, name: "copy.oma" })).rejects.toThrow();
    await t.run(ctx => ctx.db.patch(retry.id, { created: Date.now() - 3600001 }));
    await expect(owner.mutation(api.files.finish, { uploadId: retry.id, storageId, name: "expired.oma" })).rejects.toThrow("Upload expired");
  });
  it("stops serving public image bytes when a work is unpublished", async () => {
    const { t, owner } = setup();
    const projectId = await owner.mutation(api.projects.create, { title: "Public image" });
    const snapshotId = await t.run(async ctx => {
      const storageId = await ctx.storage.store(new Blob(["flat"], { type: "image/png" }));
      return ctx.db.insert("cloudFiles", { projectId, storageId, uploader: "owner", name: "flat.png", kind: "snapshot", size: 4, contentType: "image/png", created: Date.now(), version: 1 });
    });
    const id = await owner.mutation(api.showcase.publish, { snapshotId, title: "Published", description: "Flat only" });
    expect((await t.fetch(`/showcase-image?id=${id}`)).status).toBe(200);
    await owner.mutation(api.showcase.unpublish, { id });
    expect((await t.fetch(`/showcase-image?id=${id}`)).status).toBe(404);
    expect(await t.query(api.showcase.get, { id: "invalid" })).toBeNull();
  });
  it("expires pending device approvals", async () => {
    const { t, owner } = setup();
    await t.mutation(api.devices.begin, { token: "b".repeat(64), code: "BBBB1234BBBB", label: "Expired" });
    await t.run(async ctx => { const d = await ctx.db.query("cloudDevices").first(); await ctx.db.patch(d!._id, { expires: Date.now() - 1 }); });
    await expect(owner.mutation(api.devices.approve, { code: "BBBB1234BBBB" })).rejects.toThrow();
    await t.mutation(internal.cloudMaintenance.expire, {});
    expect(await t.run(ctx => ctx.db.query("cloudDevices").collect())).toEqual([]);
  });
});
describe("Cloud authorization and review", () => {
  it("requires verified identity and isolates projects", async () => {
    const { t, owner, reviewer } = setup();
    await expect(
      t.mutation(api.projects.create, { title: "Private" }),
    ).rejects.toThrow();
    const p = await owner.mutation(api.projects.create, { title: "Private" });
    expect(await reviewer.query(api.projects.list, {})).toEqual([]);
    await expect(
      reviewer.query(api.projects.get, { projectId: p }),
    ).rejects.toThrow();
    await expect(
      t
        .withIdentity({
          subject: "unverified",
          email: "a@b.com",
          emailVerified: false,
        })
        .query(api.projects.list, {}),
    ).rejects.toThrow();
  });
  it("restricts reviewers to flat snapshots, and revokes access immediately", async () => {
    const { t, owner, reviewer } = setup();
    const projectId = await owner.mutation(api.projects.create, {
      title: "Review",
    });
    const { source, snapshot } = await t.run(async (ctx) => {
      await ctx.db.insert("cloudMembers", {
        projectId,
        userId: "reviewer",
        email: "reviewer@example.com",
        name: "Reviewer",
        role: "reviewer",
      });
      const storageId = await ctx.storage.store(
        new Blob(["test"], { type: "image/png" }),
      );
      const base = {
        projectId,
        storageId,
        uploader: "owner",
        size: 4,
        created: Date.now(),
        version: 1,
      };
      return {
        source: await ctx.db.insert("cloudFiles", {
          ...base,
          name: "private.oma",
          kind: "source",
          contentType: "application/octet-stream",
        }),
        snapshot: await ctx.db.insert("cloudFiles", {
          ...base,
          name: "flat.png",
          kind: "snapshot",
          contentType: "image/png",
          width: 100,
          height: 100,
        }),
      };
    });
    expect(
      (await reviewer.query(api.projects.get, { projectId })).files.map(
        (f) => f._id,
      ),
    ).toEqual([snapshot]);
    await expect(
      reviewer.query(api.files.authorizeDownload, { id: source }),
    ).rejects.toThrow();
    await expect(
      reviewer.mutation(api.files.begin, { projectId, kind: "asset" }),
    ).rejects.toThrow();
    const id = await reviewer.mutation(api.review.annotate, {
      snapshotId: snapshot,
      x: 0.2,
      y: 0.3,
      shape: "pin",
      body: "Move this",
    });
    await owner.mutation(api.review.reply, { id, body: "Done" });
    await owner.mutation(api.review.resolve, { id, resolved: true });
    expect(
      (await owner.query(api.review.list, { projectId }))[0].replies,
    ).toHaveLength(1);
    await owner.mutation(api.projects.changeMember, {
      projectId,
      userId: "reviewer",
      role: "remove",
    });
    await expect(
      reviewer.query(api.review.list, { projectId }),
    ).rejects.toThrow();
  });
  it("does not let annotations escape their project or snapshot", async () => {
    const { t, owner } = setup();
    const a = await owner.mutation(api.projects.create, { title: "A" });
    const b = await owner.mutation(api.projects.create, { title: "B" });
    const snapshot = await t.run(async (ctx) => {
      const storageId = await ctx.storage.store(new Blob(["x"]));
      return ctx.db.insert("cloudFiles", {
        projectId: b,
        storageId,
        uploader: "owner",
        name: "flat.png",
        kind: "snapshot",
        size: 1,
        contentType: "image/png",
        created: Date.now(),
        version: 1,
      });
    });
    await expect(
      owner.query(api.review.list, { projectId: a, snapshotId: snapshot }),
    ).rejects.toThrow();
    await expect(
      owner.mutation(api.review.annotate, {
        snapshotId: snapshot,
        x: 1.2,
        y: 0,
        shape: "pin",
        body: "bad",
      }),
    ).rejects.toThrow();
  });
  it("issues browser-approved desktop access and supports revocation", async () => {
    const { t, owner } = setup();
    const token = "a".repeat(64);
    await t.mutation(api.devices.begin, {
      token,
      code: "ABCD1234EFGH",
      label: "Test desktop",
    });
    await expect(
      t.query(api.devices.me, { deviceToken: token }),
    ).rejects.toThrow();
    await owner.mutation(api.devices.approve, { code: "ABCD1234EFGH" });
    expect((await t.query(api.devices.me, { deviceToken: token })).userId).toBe(
      "owner",
    );
    const p = await t.mutation(api.projects.create, {
      title: "Desktop",
      deviceToken: token,
    });
    expect((await owner.query(api.projects.get, { projectId: p })).title).toBe(
      "Desktop",
    );
    const [device] = await owner.query(api.devices.list, {});
    await owner.mutation(api.devices.revoke, { id: device.id });
    await expect(
      t.query(api.devices.me, { deviceToken: token }),
    ).rejects.toThrow();
  });
});

describe("Publishing and invitation boundaries", () => {
  it("only accepts invitations for the verified recipient", async () => {
    const { t, owner, reviewer } = setup();
    const projectId = await owner.mutation(api.projects.create, {
      title: "Invite",
    });
    const id = await t.run((ctx) =>
      ctx.db.insert("cloudInvites", {
        projectId,
        email: "reviewer@example.com",
        role: "reviewer",
        inviter: "owner",
        expires: Date.now() + 100000,
        accepted: false,
      }),
    );
    await expect(owner.mutation(api.projects.accept, { id })).rejects.toThrow();
    await reviewer.mutation(api.projects.accept, { id });
    expect((await reviewer.query(api.projects.get, { projectId })).role).toBe(
      "reviewer",
    );
    await expect(
      reviewer.mutation(api.projects.accept, { id }),
    ).rejects.toThrow();
  });
  it("requires owner publication and hides unpublished competition entries", async () => {
    const { t, owner, reviewer } = setup();
    const projectId = await owner.mutation(api.projects.create, {
      title: "Showcase",
    });
    const { snapshotId, sourceId, competitionId } = await t.run(async (ctx) => {
      await ctx.db.insert("cloudMembers", {
        projectId,
        userId: "reviewer",
        email: "reviewer@example.com",
        name: "Reviewer",
        role: "reviewer",
      });
      const storageId = await ctx.storage.store(new Blob(["image"]));
      const base = {
        projectId,
        storageId,
        uploader: "owner",
        name: "flat.png",
        size: 5,
        contentType: "image/png",
        created: Date.now(),
        version: 1,
      };
      return {
        snapshotId: await ctx.db.insert("cloudFiles", {
          ...base,
          kind: "snapshot",
        }),
        sourceId: await ctx.db.insert("cloudFiles", {
          ...base,
          kind: "source",
        }),
        competitionId: await ctx.db.insert("cloudCompetitions", {
          title: "Test",
          description: "Disposable",
          opens: Date.now() - 1000,
          closes: Date.now() + 100000,
          active: true,
        }),
      };
    });
    expect(await t.query(api.showcase.list, {})).toEqual([]);
    await expect(
      reviewer.mutation(api.showcase.publish, {
        snapshotId,
        title: "Private",
        description: "No",
      }),
    ).rejects.toThrow();
    await expect(
      owner.mutation(api.showcase.publish, {
        snapshotId: sourceId,
        title: "Source",
        description: "No",
      }),
    ).rejects.toThrow();
    const showcaseId = await owner.mutation(api.showcase.publish, {
      snapshotId,
      title: "Public work",
      description: "Only the flat export",
    });
    await owner.mutation(api.showcase.enter, { showcaseId, competitionId });
    await expect(
      owner.mutation(api.showcase.enter, { showcaseId, competitionId }),
    ).rejects.toThrow();
    await owner.mutation(api.showcase.unpublish, { id: showcaseId });
    expect(await t.query(api.showcase.entries, { competitionId })).toEqual([]);
    expect(await t.query(api.showcase.list, {})).toEqual([]);
  });
  it("archives projects and restores only for the owner", async () => {
    const { owner, reviewer } = setup();
    const projectId = await owner.mutation(api.projects.create, {
      title: "Archive",
    });
    await owner.mutation(api.projects.archive, { projectId });
    await expect(
      owner.query(api.projects.get, { projectId }),
    ).rejects.toThrow();
    await expect(
      reviewer.mutation(api.projects.restore, { projectId }),
    ).rejects.toThrow();
    await owner.mutation(api.projects.restore, { projectId });
    expect((await owner.query(api.projects.get, { projectId })).title).toBe(
      "Archive",
    );
  });
});
