import { createFileRoute } from "@tanstack/react-router";
import src from "../../../docs/CONTRIBUTING.md?raw";
import { Markdown } from "../md";

export const Route = createFileRoute("/docs/contributing")({
  component: () => <Markdown source={src} />,
});
