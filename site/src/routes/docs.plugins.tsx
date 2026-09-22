import { createFileRoute } from "@tanstack/react-router";
import src from "../../../docs/plugins.md?raw";
import { Markdown } from "../md";

export const Route = createFileRoute("/docs/plugins")({
  head: () => ({ meta: [{ title: "Lua plugins · omadesign" }] }),
  component: () => <Markdown source={src} sourcePath="docs/plugins.md" />,
});
