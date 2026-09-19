import { internalMutation, internalQuery } from "./_generated/server";
import { components } from "./_generated/api";
import { Resend, type EmailId } from "@convex-dev/resend";
import { v } from "convex/values";
const resend = new Resend(components.resend, { testMode: false });
export const deliveryProbe = internalMutation({
  args: {},
  handler: (ctx) =>
    resend.sendEmail(ctx, {
      from: "Omadesign <collab@mail.omadesign.app>",
      to: "delivered@resend.dev",
      subject: "Omadesign delivery verification",
      text: "Service delivery check for Omadesign cloud collaboration.",
    }),
});
export const deliveryStatus = internalQuery({
  args: { id: v.string() },
  handler: (ctx, a) => resend.status(ctx, a.id as EmailId),
});
