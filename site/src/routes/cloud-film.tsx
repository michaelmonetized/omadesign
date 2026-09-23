import { createFileRoute } from "@tanstack/react-router";
import { CloudDreamscapePreview } from "../components/cloud-reveal/preview";

// Keep the original preview URL working while the experience itself stays scrollable.
export const Route = createFileRoute("/cloud-film")({
  head: () => ({ meta: [{ title: "Omadesign · Cloud dreamscape" }, { name: "robots", content: "noindex" }] }),
  component: CloudDreamscapePreview,
});
