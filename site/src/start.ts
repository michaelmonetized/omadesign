import { clerkMiddleware } from "@clerk/tanstack-react-start/server";
import { createStart } from "@tanstack/react-start";
export const startInstance = createStart(() => ({
  // The public-site review build delegates cloud/account routes to production.
  requestMiddleware: import.meta.env.VITE_PUBLIC_SITE_PREVIEW === "1" ? [] : [clerkMiddleware()],
}));
