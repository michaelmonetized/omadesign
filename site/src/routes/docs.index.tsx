import { createFileRoute } from "@tanstack/react-router";

export const Route = createFileRoute("/docs/")({
  component: () => (
    <>
      <h1>Docs</h1>
      <p>
        omadesign is a native Linux studio. Start with the{" "}
        <a href="/docs/manual">user manual</a>, then the{" "}
        <a href="/docs/contributing">contrib guide</a> if you want to change it.
      </p>
      <pre>
        <code>
          curl -fsSL
          https://raw.githubusercontent.com/michaelmonetized/omadesign/master/scripts/install-remote.sh
          | sh
        </code>
      </pre>
    </>
  ),
});
