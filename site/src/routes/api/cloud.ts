import { createFileRoute } from "@tanstack/react-router";
import { discoverCloud, handleCloud } from "../../server/cloud";

export const Route = createFileRoute("/api/cloud")({
  server: {
    handlers: {
      GET: async () => discoverCloud(),
      POST: async ({ request }) => handleCloud(request),
    },
  },
});
