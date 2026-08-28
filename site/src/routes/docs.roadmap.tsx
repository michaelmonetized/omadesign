import { createFileRoute } from "@tanstack/react-router";
import src from "../../../docs/ROADMAP.md?raw";
import { Markdown } from "../md";

export const Route = createFileRoute("/docs/roadmap")({
  component: () => <Markdown source={src} />,
});
