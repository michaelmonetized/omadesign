import { createFileRoute } from "@tanstack/react-router";
import src from "../../../docs/MANUAL.md?raw";
import { Markdown } from "../md";

export const Route = createFileRoute("/docs/manual")({
  component: () => <Markdown source={src} />,
});
