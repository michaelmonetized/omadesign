import { createFileRoute } from "@tanstack/react-router";
import src from "../../../docs/format-support.md?raw";
import { Markdown } from "../md";

export const Route = createFileRoute("/docs/formats")({
  head: () => ({ meta: [{ title: "File formats · omadesign" }] }),
  component: () => <Markdown source={src} sourcePath="docs/format-support.md" />,
});
