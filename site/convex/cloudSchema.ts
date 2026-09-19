import { defineTable } from "convex/server";
import { v } from "convex/values";
export const role = v.union(
  v.literal("owner"),
  v.literal("editor"),
  v.literal("reviewer"),
);
export const fileKind = v.union(
  v.literal("source"),
  v.literal("asset"),
  v.literal("snapshot"),
);
export const cloudTables = {
  cloudProjects: defineTable({
    title: v.string(),
    owner: v.string(),
    created: v.number(),
    updated: v.number(),
    archived: v.boolean(),
  }).index("by_owner", ["owner"]),
  cloudMembers: defineTable({
    projectId: v.id("cloudProjects"),
    userId: v.string(),
    email: v.string(),
    name: v.string(),
    role,
  })
    .index("by_project_user", ["projectId", "userId"])
    .index("by_user", ["userId"])
    .index("by_project", ["projectId"]),
  cloudInvites: defineTable({
    projectId: v.id("cloudProjects"),
    email: v.string(),
    role,
    inviter: v.string(),
    expires: v.number(),
    accepted: v.boolean(),
  })
    .index("by_email", ["email"])
    .index("by_project_email", ["projectId", "email"]),
  cloudFiles: defineTable({
    projectId: v.id("cloudProjects"),
    storageId: v.id("_storage"),
    uploader: v.string(),
    name: v.string(),
    kind: fileKind,
    size: v.number(),
    contentType: v.string(),
    created: v.number(),
    version: v.number(),
    frameId: v.optional(v.string()),
    width: v.optional(v.number()),
    height: v.optional(v.number()),
  })
    .index("by_project", ["projectId"])
    .index("by_project_kind", ["projectId", "kind"])
    .index("by_storage", ["storageId"]),
  cloudUploads: defineTable({
    projectId: v.id("cloudProjects"),
    userId: v.string(),
    created: v.number(),
    kind: fileKind,
  }).index("by_user", ["userId"]),
  cloudAnnotations: defineTable({
    projectId: v.id("cloudProjects"),
    snapshotId: v.id("cloudFiles"),
    x: v.number(),
    y: v.number(),
    endX: v.optional(v.number()),
    endY: v.optional(v.number()),
    shape: v.union(v.literal("pin"), v.literal("rectangle")),
    body: v.string(),
    author: v.string(),
    authorName: v.string(),
    created: v.number(),
    resolved: v.boolean(),
  })
    .index("by_snapshot", ["snapshotId"])
    .index("by_project", ["projectId"]),
  cloudReplies: defineTable({
    annotationId: v.id("cloudAnnotations"),
    author: v.string(),
    authorName: v.string(),
    body: v.string(),
    created: v.number(),
  }).index("by_annotation", ["annotationId"]),
  cloudShowcase: defineTable({
    projectId: v.id("cloudProjects"),
    snapshotId: v.id("cloudFiles"),
    owner: v.string(),
    author: v.string(),
    title: v.string(),
    description: v.string(),
    published: v.boolean(),
    created: v.number(),
  })
    .index("by_published", ["published"])
    .index("by_project", ["projectId"]),
  cloudCompetitions: defineTable({
    title: v.string(),
    description: v.string(),
    opens: v.number(),
    closes: v.number(),
    active: v.boolean(),
  }).index("by_active", ["active"]),
  cloudEntries: defineTable({
    competitionId: v.id("cloudCompetitions"),
    showcaseId: v.id("cloudShowcase"),
    owner: v.string(),
    created: v.number(),
  })
    .index("by_competition", ["competitionId"])
    .index("by_competition_showcase", ["competitionId", "showcaseId"])
    .index("by_owner", ["owner"]),
  cloudDevices: defineTable({
    tokenHash: v.string(),
    code: v.string(),
    label: v.string(),
    created: v.number(),
    expires: v.number(),
    userId: v.optional(v.string()),
    email: v.optional(v.string()),
    name: v.optional(v.string()),
  })
    .index("by_hash", ["tokenHash"])
    .index("by_code", ["code"])
    .index("by_user", ["userId"]),
};
