import { describe, expect, test } from "vitest";
import { packageDownloads, productHuntCounts } from "./public-metrics";

describe("Public counts", () => {
  test("counts published Linux packages and skips checksums and drafts", () => {
    expect(packageDownloads([
      { draft: true, assets: [{ name: "omadesign-9.0.0-x86_64-unknown-linux-gnu.tar.gz", download_count: 50 }] },
      { assets: [
        { name: "omadesign-0.5.8-aarch64-unknown-linux-gnu.tar.gz", download_count: 12 },
        { name: "omadesign-0.5.8-x86_64-unknown-linux-gnu.tar.gz", download_count: 10 },
        { name: "omadesign-0.5.8-aarch64-unknown-linux-gnu.tar.gz.sha256", download_count: 9 },
        { name: "omadesign-0.5.8-source.zip", download_count: 4 },
      ] },
    ])).toBe(22);
  });

  test("adds both Product Hunt launches once and reads followers", () => {
    const html = [
      '"__typename":"PostEdge","node":{"__typename":"Post","id":"1251829","product":{"__typename":"Product","id":"1313169","slug":"omadesign"},"latestScore":60',
      '"__typename":"PostEdge","node":{"__typename":"Post","id":"1245212","product":{"__typename":"Product","id":"1313169","slug":"omadesign"},"latestScore":3',
      '"__typename":"PostEdge","node":{"__typename":"Post","id":"1251829","product":{"__typename":"Product","id":"1313169"},"latestScore":60',
      '"__typename":"PostEdge","node":{"__typename":"Post","id":"9","product":{"__typename":"Product","id":"1319427","slug":"other"},"latestScore":700',
      '"name":"omadesign","reviewsCount":0,"followersCount":28',
      '"name":"Lunacy","followersCount":900',
    ].join("");
    expect(productHuntCounts(html)).toEqual({ upvotes: 63, users: 28 });
  });
});
