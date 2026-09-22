let cached: { expires: number; body: { downloads: number; source: string; includesRepeatDownloads: boolean; updated: string } } | undefined;
export async function downloadStats() {
  try {
    if (!cached || cached.expires < Date.now()) {
      let downloads = 0;
      for (let page = 1; page <= 20; page++) {
        const response = await fetch(`https://api.github.com/repos/michaelmonetized/omadesign/releases?per_page=100&page=${page}`, { headers: { Accept: "application/vnd.github+json", "User-Agent": "Omadesign release statistics" }, signal: AbortSignal.timeout(8000) });
        if (!response.ok) throw new Error("Unavailable");
        const releases = await response.json() as { draft: boolean; assets: { name: string; download_count: number }[] }[];
        for (const release of releases) if (!release.draft) for (const asset of release.assets) {
          if (/^omadesign-.+-(?:aarch64|x86_64)-unknown-linux-gnu\.tar\.gz$/.test(asset.name)) downloads += asset.download_count;
        }
        if (releases.length < 100) break;
        if (page === 20) throw new Error("Incomplete statistics");
      }
      cached = { expires: Date.now() + 3600000, body: { downloads, source: "GitHub release package downloads", includesRepeatDownloads: true, updated: new Date().toISOString() } };
    }
    return Response.json(cached.body, { headers: { "Cache-Control": "public, max-age=3600" } });
  } catch { return Response.json({ error: "Download statistics temporarily unavailable" }, { status: 503, headers: { "Cache-Control": "no-store" } }); }
}
