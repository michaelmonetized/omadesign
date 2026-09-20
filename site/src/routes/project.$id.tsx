import { createFileRoute, redirect } from "@tanstack/react-router";
export const Route = createFileRoute("/project/$id")({
  beforeLoad: ({ params }) => {
    throw redirect({
      to: "/cloud",
      search: { project: params.id, device: undefined },
    });
  },
});
