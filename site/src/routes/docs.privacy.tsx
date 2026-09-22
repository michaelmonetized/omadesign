import { createFileRoute } from "@tanstack/react-router";
import src from "../../../docs/privacy.md?raw";
import { Markdown } from "../md";
export const Route = createFileRoute("/docs/privacy")({ head: () => ({ meta: [{ title: "Privacy · omadesign" }] }), component: () => <Markdown source={src} sourcePath="docs/privacy.md" /> });
