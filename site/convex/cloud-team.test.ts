/// <reference types="vite/client" />
import { convexTest } from "convex-test";
import { describe, expect, it } from "vitest";
import schema from "./schema";
import { api } from "./_generated/api";
const modules = import.meta.glob(["./**/*.ts", "!./**/*.test.ts"]);

describe("Welcome Team project discovery", () => {
  it("distinguishes private backups, pending invites, and accepted team projects", async () => {
    const t = convexTest(schema, modules);
    const owner = t.withIdentity({ subject: "owner", email: "owner@example.com", emailVerified: true });
    const teammate = t.withIdentity({ subject: "teammate", email: "team@example.com", emailVerified: true });
    const stranger = t.withIdentity({ subject: "stranger", email: "stranger@example.com", emailVerified: true });
    const projectId = await owner.mutation(api.projects.create, { title: "Brand" });
    expect((await owner.query(api.projects.list, {}))[0].shared).toBe(false);
    const invitation = await t.run(ctx => ctx.db.insert("cloudInvites", {
      projectId, email: "team@example.com", role: "reviewer", inviter: "owner",
      expires: Date.now() + 100000, accepted: false,
    }));
    expect((await owner.query(api.projects.list, {}))[0].shared).toBe(false);
    expect(await teammate.query(api.projects.list, {})).toEqual([]);
    await teammate.mutation(api.projects.accept, { id: invitation });
    expect((await owner.query(api.projects.list, {}))[0].shared).toBe(true);
    expect((await teammate.query(api.projects.list, {}))[0].shared).toBe(true);
    expect(await stranger.query(api.projects.list, {})).toEqual([]);
    await owner.mutation(api.projects.changeMember, { projectId, userId: "teammate", role: "remove" });
    expect((await owner.query(api.projects.list, {}))[0].shared).toBe(false);
    expect(await teammate.query(api.projects.list, {})).toEqual([]);
    await owner.mutation(api.projects.archive, { projectId });
    expect(await owner.query(api.projects.list, {})).toEqual([]);
  });

  it("only exposes authorized flat previews and enforces desktop revocation", async () => {
    const t = convexTest(schema, modules);
    const owner = t.withIdentity({ subject: "owner", email: "owner@example.com", emailVerified: true });
    const projectId = await owner.mutation(api.projects.create, { title: "Shared" });
    await t.run(async ctx => {
      await ctx.db.insert("cloudMembers", { projectId, userId: "reviewer", email: "reviewer@example.com", name: "Reviewer", role: "reviewer" });
      const storageId = await ctx.storage.store(new Blob(["file"]));
      const base = { projectId, storageId, uploader: "owner", size: 4, created: Date.now(), version: 1 };
      await ctx.db.insert("cloudFiles", { ...base, name: "private.oma", kind: "source", contentType: "application/octet-stream" });
      await ctx.db.insert("cloudFiles", { ...base, name: "review.png", kind: "snapshot", contentType: "image/png", width: 200, height: 100 });
    });
    const reviewer = t.withIdentity({ subject: "reviewer", email: "reviewer@example.com", emailVerified: true });
    const [project] = await reviewer.query(api.projects.list, {});
    expect(project.previews.map(f => f.name)).toEqual(["review.png"]);
    expect((await reviewer.query(api.projects.get, { projectId })).files.map(f => f.kind)).toEqual(["snapshot"]);
    const token = "c".repeat(64);
    await t.mutation(api.devices.begin, { token, code: "CCCC1234CCCC", label: "Team desktop" });
    await reviewer.mutation(api.devices.approve, { code: "CCCC1234CCCC" });
    expect((await t.query(api.projects.list, { deviceToken: token }))[0].shared).toBe(true);
    await t.mutation(api.devices.disconnect, { deviceToken: token });
    await expect(t.query(api.projects.list, { deviceToken: token })).rejects.toThrow();
  });
});
