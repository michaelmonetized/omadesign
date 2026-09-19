import { defineSchema, defineTable } from "convex/server";
import { cloudTables } from "./cloudSchema";
import { v } from "convex/values";

export default defineSchema({
  ...cloudTables,
  waitlistSignups: defineTable({
    email: v.string(), name: v.string(),
    list: v.union(v.literal("cloud"), v.literal("competition")),
    created: v.number(), consentVersion: v.string(),
  }).index("by_list_email", ["list", "email"]),
  waitlistRateLimits: defineTable({
    fingerprint: v.string(), count: v.number(), expires: v.number(),
  }).index("by_fingerprint", ["fingerprint"]).index("by_expires", ["expires"]),
  users: defineTable({
    clerkId: v.string(),
    email: v.string(),
    name: v.string(),
  }).index("by_clerk", ["clerkId"]),
  projects: defineTable({
    ownerId: v.id("users"),
    title: v.string(),
    private: v.boolean(),
  }).index("by_owner", ["ownerId"]),
  documents: defineTable({
    projectId: v.id("projects"),
    snapshot: v.string(),
    version: v.number(),
  }).index("by_project", ["projectId"]),
  presence: defineTable({
    projectId: v.id("projects"),
    userId: v.id("users"),
    cursorX: v.number(),
    cursorY: v.number(),
    updated: v.number(),
  }).index("by_project", ["projectId"]),
  comments: defineTable({
    projectId: v.id("projects"),
    pinId: v.string(),
    authorId: v.id("users"),
    body: v.string(),
    created: v.number(),
  }).index("by_pin", ["pinId"]),
  annotations: defineTable({
    projectId: v.id("projects"),
    frameId: v.optional(v.string()),
    x: v.number(),
    y: v.number(),
    resolved: v.boolean(),
    authorId: v.id("users"),
    created: v.number(),
  }).index("by_project", ["projectId"]),
  galleryItems: defineTable({
    projectId: v.id("projects"),
    title: v.string(),
    author: v.string(),
    tags: v.array(v.string()),
    summary: v.string(),
    published: v.boolean(),
  }).index("by_published", ["published"]),
  competitionEntries: defineTable({
    email: v.string(),
    name: v.string(),
    created: v.number(),
    waitlist: v.boolean(),
  }).index("by_email", ["email"]),
});
