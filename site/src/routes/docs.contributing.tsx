import { createFileRoute } from "@tanstack/react-router";
import src from "../../../docs/CONTRIBUTING.md?raw";
import { Markdown } from "../md";

export const Route = createFileRoute("/docs/contributing")({
  head: () => ({ meta: [{ title: "Contributing · omadesign" }] }),
  component: () => <Markdown source={src} sourcePath="docs/CONTRIBUTING.md" />,
});
