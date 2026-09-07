import { createFileRoute } from "@tanstack/react-router";
import src from "../../../docs/affinity-import.md?raw";
import { Markdown } from "../md";

export const Route = createFileRoute("/docs/affinity")({
  head: () => ({ meta: [{ title: "Affinity import · omadesign" }] }),
  component: () => <Markdown source={src} sourcePath="docs/affinity-import.md" />,
});
