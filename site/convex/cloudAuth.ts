import { ConvexError, v } from "convex/values";
import type { QueryCtx, MutationCtx } from "./_generated/server";
import type { Id } from "./_generated/dataModel";
export const deviceArg = { deviceToken: v.optional(v.string()) };
export async function hash(value:string) {
  return Array.from(new Uint8Array(await crypto.subtle.digest("SHA-256",new TextEncoder().encode(value))),b=>b.toString(16).padStart(2,"0")).join("");
}
export function fail(message:string):never {throw new ConvexError(message);}
export function clean(value:string, max=200) { const text=value.trim(); if(!text || text.length>max) fail(`Enter between 1 and ${max} characters.`);return text; }
export async function actor(ctx:QueryCtx|MutationCtx, token?:string) {
  if(token) {
    if(token.length!==64) fail("Sign in to cloud again.");
    const tokenHash=await hash(token);
    const d=await ctx.db.query("cloudDevices").withIndex("by_hash",q=>q.eq("tokenHash",tokenHash)).unique();
    if(!d?.userId || !d.email || d.expires<Date.now()) fail("Sign in to cloud again.");
    return {userId:d.userId,email:d.email,name:d.name || d.email};
  }
  const id=await ctx.auth.getUserIdentity();
  if(!id || !id.email || id.emailVerified!==true) fail("Sign in with a verified email address.");
  return {userId:id.subject,email:id.email.toLowerCase(),name:id.name || id.email};
}
export async function access(ctx:QueryCtx|MutationCtx,projectId:Id<"cloudProjects">, token?:string, required:"read"|"write"|"owner"="read") {
 const user=await actor(ctx,token);
 const project=await ctx.db.get(projectId);
 const member=await ctx.db.query("cloudMembers").withIndex("by_project_user",q=>q.eq("projectId",projectId).eq("userId",user.userId)).unique();
 if(!project || project.archived || !member || (required==="write" && member.role==="reviewer") || (required==="owner" && member.role!=="owner")) fail("You do not have access to this project.");
 return {user,project,member};
}
