import { createFileRoute } from "@tanstack/react-router";
import { downloadStats } from "../../server/download-stats";
export const Route = createFileRoute("/api/stats")({ server: { handlers: { GET: ({ request }) => downloadStats(request) } } });
