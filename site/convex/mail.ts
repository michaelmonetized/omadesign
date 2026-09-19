import { internalMutation } from "./_generated/server";
import { components } from "./_generated/api";
import { Resend } from "@convex-dev/resend";
const resend = new Resend(components.resend, { testMode: false });
export const deliveryProbe = internalMutation({
  args: {},
  handler: ctx => resend.sendEmail(ctx, { from: "Omadesign <collab@mail.omadesign.app>", to: "delivered@resend.dev", subject: "Omadesign delivery verification", text: "Service delivery check for Omadesign cloud collaboration." }),
});
