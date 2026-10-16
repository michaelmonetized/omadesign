import { describe, expect, it, vi } from "vitest";
import { ConvexHttpClient } from "convex/browser";
import { ConvexError } from "convex/values";
import { discoverCloud, handleCloud } from "./cloud";

describe("Native cloud request bounds", () => {
  it("returns safe capability refusals while redacting untrusted provider error arguments", async () => {
    vi.stubEnv("CONVEX_URL", "https://fixture.convex.cloud");
    const mutation = vi.spyOn(ConvexHttpClient.prototype, "mutation");
    const request = () =>
      new Request("http://localhost/api/cloud", {
        method: "POST",
        body: JSON.stringify({
          operation: "live:join",
          args: { session: "fixture", actor: "a".repeat(64), protocol: 2 },
        }),
      });
    try {
      mutation.mockRejectedValueOnce(
        new ConvexError(
          "This live document requires native-preservation protocol 2.",
        ),
      );
      expect((await (await handleCloud(request())).json()).error).toContain(
        "native-preservation",
      );
      mutation.mockRejectedValueOnce(
        new Error("private-device-token and request arguments"),
      );
      expect((await (await handleCloud(request())).json()).error).not.toContain(
        "private-device-token",
      );
    } finally {
      vi.restoreAllMocks();
      vi.unstubAllEnvs();
    }
  });
  it("advertises live capability and rejects unsupported operations before a provider call", async () => {
    expect((await discoverCloud().json()).capabilities.live).toBe(3);
    const response = await handleCloud(
      new Request("http://localhost/api/cloud", {
        method: "POST",
        body: JSON.stringify({ operation: "unapproved:write", args: {} }),
      }),
    );
    expect(response.status).toBe(400);
  });
  it("preflights old advanced creates before quota initialization or any provider call", async () => {
    const action = vi.spyOn(ConvexHttpClient.prototype, "action");
    const mutation = vi.spyOn(ConvexHttpClient.prototype, "mutation");
    try {
      for (const seed of [
        {
          "": "Object",
          "/sources": { Value: [{ id: "retained-original", data: "opaque" }] },
        },
        { "": "Object", "/publishing/facing_pages": { Value: true } },
        {
          "": "Object",
          "/@entities/1/geom/Text/spans/0/font": { Value: "omatype:face" },
        },
        {
          "": "Object",
          "/motion/expressions": { Value: [{ expression: "time" }] },
        },
        { "": "Object", "/@compositions/7/document": "Object" },
        { "": "Object", "/motion/layout28": { Value: {} } },
      ]) {
        const response = await handleCloud(
          new Request("http://localhost/api/cloud", {
            method: "POST",
            body: JSON.stringify({
              operation: "live:create",
              args: {
                projectId: "project",
                seed: JSON.stringify(seed),
                protocol: 2,
              },
            }),
          }),
        );
        expect(response.status).toBe(400);
        expect((await response.json()).error).toMatch(/protocol 3|Unsupported/);
      }
      expect(action).not.toHaveBeenCalled();
      expect(mutation).not.toHaveBeenCalled();
    } finally {
      vi.restoreAllMocks();
    }
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
