/**
 * Public counts for the homepage, README, and Product Hunt row.
 * Package downloads include repeat downloads and updates. Users are Product Hunt followers.
 * Upvotes are the summed points of every omadesign launch. Stars are GitHub stars.
 */

const REPO = "https://github.com/michaelmonetized/omadesign";
const PRODUCT_HUNT = "https://www.producthunt.com/products/omadesign";
const PRODUCT_HUNT_CAPTURED = { upvotes: 63, users: 28, at: "2026-09-24" };
const PRODUCT_ID = "1313169";
const PACKAGE = /^omadesign-.+-(?:aarch64|x86_64)-unknown-linux-gnu\.tar\.gz$/;
const GH = { Accept: "application/vnd.github+json", "User-Agent": "Omadesign release statistics" };
const number = new Intl.NumberFormat("en-US");

type Release = { draft?: boolean; assets?: { name: string; download_count: number }[] };
export type Chip = {
  key: "downloads" | "users" | "upvotes" | "stars";
  label: string;
  value: number | null;
  display: string | null;
  href: string;
  title: string;
  color: string;
};
export type PublicMetrics = {
  downloads: number;
  users: number | null;
  upvotes: number | null;
  stars: number | null;
  source: string;
  usersSource: string;
  upvotesSource: string;
  starsSource: string;
  includesRepeatDownloads: true;
  updated: string;
  chips: Chip[];
};

let cached: { expires: number; body: PublicMetrics } | undefined;

/**
 * Sum release-package downloads.
 * Counts Linux tarballs on published releases. Checksums and drafts are left out.
 * @param releases GitHub release payloads.
 * @returns The package download total.
 */
export function packageDownloads(releases: Release[]) {
  let downloads = 0;
  for (const release of releases) {
    if (release.draft) continue;
    for (const asset of release.assets ?? []) if (PACKAGE.test(asset.name)) downloads += asset.download_count;
  }
  return downloads;
}

/**
 * Read omadesign's Product Hunt points and followers.
 * Adds each launch's current score once. Followers are the people count on the product.
 * @param html The public product page.
 * @returns Upvotes to date and follower count.
 */
export function productHuntCounts(html: string) {
  const scores = new Map<string, number>();
  for (const match of html.matchAll(/"__typename":"PostEdge","node":\{"__typename":"Post","id":"(\d+)"([\s\S]*?)"latestScore":(\d+)/g)) {
    if (!match[2].includes(`"id":"${PRODUCT_ID}"`)) continue;
    scores.set(match[1], Number(match[3]));
  }
  if (scores.size === 0) throw new Error("No Product Hunt launches");
  let users: number | null = null;
  for (const match of html.matchAll(/followersCount":(\d+)/g)) {
    const before = html.slice(Math.max(0, (match.index ?? 0) - 800), match.index ?? 0);
    const names = [...before.matchAll(/"name":"([^"]+)"/g)];
    if (names.at(-1)?.[1] === "omadesign") users = Number(match[1]);
  }
  if (users === null) throw new Error("No Product Hunt followers");
  let upvotes = 0;
  for (const score of scores.values()) upvotes += score;
  return { upvotes, users };
}

function chip(key: Chip["key"], label: string, value: number | null, href: string, title: string, color: string): Chip {
  return { key, label, value, display: value === null ? null : number.format(value), href, title, color };
}

function body(downloads: number, users: number | null, upvotes: number | null, stars: number | null, liveHunt: boolean): PublicMetrics {
  const captured = liveHunt ? "" : `, captured ${PRODUCT_HUNT_CAPTURED.at}`;
  return {
    downloads,
    users,
    upvotes,
    stars,
    source: "GitHub release package downloads",
    usersSource: `Product Hunt followers${captured}`,
    upvotesSource: `Product Hunt launch points${captured}`,
    starsSource: "GitHub stars",
    includesRepeatDownloads: true,
    updated: new Date().toISOString(),
    chips: [
      chip("downloads", "downloads", downloads, `${REPO}/releases`, "GitHub release packages, including updates", "007ec6"),
      chip("users", "Users", users, PRODUCT_HUNT, "Product Hunt followers", "2ea44f"),
      chip("upvotes", "upvotes", upvotes, PRODUCT_HUNT, "Product Hunt points across launches", "e05d44"),
      chip("stars", "stars", stars, `${REPO}/stargazers`, "GitHub stars", "dfb317"),
    ],
  };
}

async function githubJson<T>(url: string): Promise<T> {
  const response = await fetch(url, { headers: GH, signal: AbortSignal.timeout(8000) });
  if (!response.ok) throw new Error("GitHub unavailable");
  return await response.json() as T;
}

async function load(): Promise<PublicMetrics> {
  const [releases, repo, hunt] = await Promise.all([
    (async () => {
      const releases: Release[] = [];
      for (let page = 1; page <= 20; page++) {
        const batch = await githubJson<Release[]>(`https://api.github.com/repos/michaelmonetized/omadesign/releases?per_page=100&page=${page}`);
        releases.push(...batch);
        if (batch.length < 100) return releases;
      }
      throw new Error("Incomplete statistics");
    })(),
    githubJson<{ stargazers_count?: number }>("https://api.github.com/repos/michaelmonetized/omadesign").catch(() => null),
    fetch(PRODUCT_HUNT, { headers: { "User-Agent": "Mozilla/5.0" }, signal: AbortSignal.timeout(8000) }).then(async response => response.ok ? productHuntCounts(await response.text()) : null).catch(() => null),
  ]);
  const stars = typeof repo?.stargazers_count === "number" ? repo.stargazers_count : null;
  return body(
    packageDownloads(releases),
    hunt?.users ?? PRODUCT_HUNT_CAPTURED.users,
    hunt?.upvotes ?? PRODUCT_HUNT_CAPTURED.upvotes,
    stars,
    hunt !== null,
  );
}

/**
 * Read the public counts.
 * Uses a one-hour memory cache. A failed GitHub download count returns null instead of a zero.
 * @returns The counts, or null when package downloads cannot be read.
 */
export async function readMetrics() {
  if (typeof window !== "undefined") {
    try {
      const response = await fetch("/api/stats");
      if (!response.ok) return null;
      return await response.json() as PublicMetrics;
    } catch {
      return null;
    }
  }
  try {
    if (!cached || cached.expires < Date.now()) cached = { expires: Date.now() + 3600000, body: await load() };
    return cached.body;
  } catch {
    return null;
  }
}

/**
 * Answer `/api/stats`.
 * The full payload keeps the original download fields. `?badge=` returns one Shields endpoint badge.
 * @param request The incoming request.
 * @returns JSON for the counts, or a badge, or 503 when downloads are unavailable.
 */
export async function metricsResponse(request?: Request) {
  const metrics = await readMetrics();
  const headers = { "Cache-Control": metrics ? "public, max-age=3600" : "no-store", "Access-Control-Allow-Origin": "*" };
  if (!metrics) return Response.json({ error: "Download statistics temporarily unavailable" }, { status: 503, headers });
  const badge = request ? new URL(request.url).searchParams.get("badge") : null;
  const chip = badge ? metrics.chips.find(item => item.key === badge) : undefined;
  if (badge) {
    if (!chip?.display) return Response.json({ error: "Count unavailable" }, { status: 503, headers: { "Cache-Control": "no-store", "Access-Control-Allow-Origin": "*" } });
    return Response.json({ schemaVersion: 1, label: chip.label, message: chip.display, color: chip.color, labelColor: "555555" }, { headers });
  }
  return Response.json(metrics, { headers });
}
