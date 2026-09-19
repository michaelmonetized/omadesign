import { mutation,query } from "./_generated/server";
import { v } from "convex/values";
import { access,clean,fail,deviceArg } from "./cloudAuth";
export const list=query({args:{...deviceArg,projectId:v.id("cloudProjects"),snapshotId:v.optional(v.id("cloudFiles"))},handler:async(ctx,a)=>{
 await access(ctx,a.projectId,a.deviceToken);
 if(a.snapshotId){const f=await ctx.db.get(a.snapshotId);if(!f||f.projectId!==a.projectId)fail("Snapshot unavailable.");}
 const rows=a.snapshotId?await ctx.db.query("cloudAnnotations").withIndex("by_snapshot",q=>q.eq("snapshotId",a.snapshotId!)).take(200):await ctx.db.query("cloudAnnotations").withIndex("by_project",q=>q.eq("projectId",a.projectId)).take(200);
 return Promise.all(rows.map(async r=>({...r,replies:await ctx.db.query("cloudReplies").withIndex("by_annotation",q=>q.eq("annotationId",r._id)).take(100)})));
}});
export const annotate=mutation({args:{...deviceArg,snapshotId:v.id("cloudFiles"),x:v.number(),y:v.number(),endX:v.optional(v.number()),endY:v.optional(v.number()),shape:v.union(v.literal("pin"),v.literal("rectangle")),body:v.string()},handler:async(ctx,a)=>{
 const f=await ctx.db.get(a.snapshotId);if(!f||f.kind!=="snapshot")fail("Select a flat export.");const {user}=await access(ctx,f.projectId,a.deviceToken);
 for(const n of [a.x,a.y,a.endX,a.endY])if(n!==undefined&&(!Number.isFinite(n)||n<0||n>1))fail("Annotation coordinates must be within the snapshot.");
 if(a.shape==="rectangle"&&(a.endX===undefined||a.endY===undefined))fail("Draw a review rectangle.");
 const count=await ctx.db.query("cloudAnnotations").withIndex("by_project",q=>q.eq("projectId",f.projectId)).take(200);if(count.length>=200)fail("Project annotation limit reached.");
 return ctx.db.insert("cloudAnnotations",{projectId:f.projectId,snapshotId:a.snapshotId,x:a.x,y:a.y,endX:a.endX,endY:a.endY,shape:a.shape,body:clean(a.body,4000),author:user.userId,authorName:user.name,created:Date.now(),resolved:false});
}});
export const reply=mutation({args:{...deviceArg,id:v.id("cloudAnnotations"),body:v.string()},handler:async(ctx,a)=>{const r=await ctx.db.get(a.id);if(!r)fail("Annotation unavailable.");const {user}=await access(ctx,r.projectId,a.deviceToken);const count=await ctx.db.query("cloudReplies").withIndex("by_annotation",q=>q.eq("annotationId",a.id)).take(100);if(count.length>=100)fail("Thread limit reached.");return ctx.db.insert("cloudReplies",{annotationId:a.id,author:user.userId,authorName:user.name,body:clean(a.body,4000),created:Date.now()});}});
export const resolve=mutation({args:{...deviceArg,id:v.id("cloudAnnotations"),resolved:v.boolean()},handler:async(ctx,a)=>{const r=await ctx.db.get(a.id);if(!r)fail("Annotation unavailable.");const {user,member}=await access(ctx,r.projectId,a.deviceToken);if(member.role==="reviewer"&&r.author!==user.userId)fail("Only the author or project team can resolve this thread.");await ctx.db.patch(a.id,{resolved:a.resolved});}});
