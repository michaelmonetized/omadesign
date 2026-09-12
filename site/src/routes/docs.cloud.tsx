import { createFileRoute } from "@tanstack/react-router";
import src from "../../../docs/cloud.md?raw";
import { Markdown } from "../md";

export const Route = createFileRoute("/docs/cloud")({
  head: () => ({ meta: [{ title: "Cloud · omadesign" }] }),
  component: () => <Markdown source={src} sourcePath="docs/cloud.md" />,
});
