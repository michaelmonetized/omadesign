import { describe, expect, it } from "vitest";
import { discoverCloud, handleCloud } from "./cloud";

describe("Native cloud request bounds", () => {
  it("advertises live capability and rejects unsupported operations before a provider call", async () => {
    expect((await discoverCloud().json()).capabilities.live).toBe(2);
    const response = await handleCloud(
      new Request("http://localhost/api/cloud", {
        method: "POST",
        body: JSON.stringify({ operation: "unapproved:write", args: {} }),
      }),
    );
    expect(response.status).toBe(400);
  });
  it("bounds a streamed body even without a content length", async () => {
    let cancelled = false;
    const body = new ReadableStream<Uint8Array>({
      pull(controller) {
        controller.enqueue(new Uint8Array(300_000));
      },
      cancel() {
        cancelled = true;
      },
    });
    const response = await handleCloud(
      new Request("http://localhost/api/cloud", {
        method: "POST",
        body,
        duplex: "half",
      } as RequestInit),
    );
    expect(response.status).toBe(413);
    expect(cancelled).toBe(true);
  });
  it("retains the smaller limit for existing operations and validates malformed JSON", async () => {
    const oversized = new Request("http://localhost/api/cloud", {
      method: "POST",
      body: JSON.stringify({
        operation: "projects:create",
        args: { title: "x".repeat(17000) },
      }),
    });
    expect((await handleCloud(oversized)).status).toBe(413);
    expect(
      (
        await handleCloud(
          new Request("http://localhost/api/cloud", {
            method: "POST",
            body: "{invalid",
          }),
        )
      ).status,
    ).toBe(400);
  });
});
