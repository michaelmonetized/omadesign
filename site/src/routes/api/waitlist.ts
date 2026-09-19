import { createFileRoute } from "@tanstack/react-router";
import { handleWaitlist } from "../../server/waitlist";

export const Route = createFileRoute("/api/waitlist")({
  server: { handlers: { POST: ({ request }) => handleWaitlist(request) } },
});
