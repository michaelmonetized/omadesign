import { createFileRoute } from "@tanstack/react-router";
import src from "../../../docs/MANUAL.md?raw";
import { Markdown } from "../md";

export const Route = createFileRoute("/docs/manual")({
  head: () => ({ meta: [{ title: "User manual · omadesign" }] }),
  component: () => <Markdown source={src} sourcePath="docs/MANUAL.md" />,
});
