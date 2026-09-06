import { createFileRoute } from "@tanstack/react-router";
import src from "../../../docs/ROADMAP.md?raw";
import { Markdown } from "../md";

export const Route = createFileRoute("/docs/roadmap")({
  head: () => ({ meta: [{ title: "Project status · omadesign" }] }),
  component: () => <Markdown source={src} sourcePath="docs/ROADMAP.md" />,
});
