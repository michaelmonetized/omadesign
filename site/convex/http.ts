import { httpRouter } from "convex/server";
import { httpAction } from "./_generated/server";
import { api,components } from "./_generated/api";
import type { Id } from "./_generated/dataModel";
import { Resend } from "@convex-dev/resend";
const http=httpRouter();
function headers(req:Request){const o=req.headers.get("Origin")||"";const origin=["https://omadesign.app","http://localhost:5173","http://localhost:5174"].includes(o)?o:"https://omadesign.app";return {"Access-Control-Allow-Origin":origin,"Vary":"Origin","Access-Control-Allow-Headers":"Authorization,Content-Type,X-Omadesign-Device","Access-Control-Allow-Methods":"GET,OPTIONS","Cache-Control":"private, no-store"};}
http.route({path:"/file",method:"OPTIONS",handler:httpAction(async(_ctx,req)=>new Response(null,{status:204,headers:headers(req)}))});
http.route({path:"/file",method:"GET",handler:httpAction(async(ctx,req)=>{
 const h=headers(req);
 try { const id=new URL(req.url).searchParams.get("id") as Id<"cloudFiles">;
 const f=await ctx.runQuery(api.files.authorizeDownload,{id,deviceToken:req.headers.get("X-Omadesign-Device")||undefined});
 const data=await ctx.storage.get(f.storageId);if(!data)return new Response("File unavailable",{status:404,headers:h});
 return new Response(data,{headers:{...h,"Content-Type":f.contentType,"X-Content-Type-Options":"nosniff","Content-Disposition":`attachment; filename*=UTF-8''${encodeURIComponent(f.name)}`}});
 }catch{return new Response("Access denied",{status:403,headers:h});}
})});
const resend=new Resend(components.resend,{});
http.route({path:"/resend-webhook",method:"POST",handler:httpAction((ctx,req)=>resend.handleResendEventWebhook(ctx,req))});
export default http;
