import { ClerkProvider, useAuth } from "@clerk/tanstack-react-start";
import { ConvexReactClient } from "convex/react";
import { ConvexProviderWithClerk } from "convex/react-clerk";
import { useState, type ReactNode } from "react";
export function CloudProvider({ children }: { children: ReactNode }) {
  if (import.meta.env.VITE_PUBLIC_SITE_PREVIEW === "1") return <>{children}</>;
  return <AuthenticatedCloudProvider>{children}</AuthenticatedCloudProvider>;
}

function AuthenticatedCloudProvider({ children }: { children: ReactNode }) {
  const [client] = useState(
    () => new ConvexReactClient(import.meta.env.VITE_CONVEX_URL),
  );
  return (
    <ClerkProvider publishableKey={import.meta.env.VITE_CLERK_PUBLISHABLE_KEY}>
      <ConvexProviderWithClerk client={client} useAuth={useAuth}>
        {children}
      </ConvexProviderWithClerk>
    </ClerkProvider>
  );
}
