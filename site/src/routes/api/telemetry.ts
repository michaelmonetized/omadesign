import { createFileRoute } from "@tanstack/react-router";
import { handleTelemetry } from "../../server/telemetry";
export const Route = createFileRoute("/api/telemetry")({ server: { handlers: { POST: ({ request }) => handleTelemetry(request) } } });
