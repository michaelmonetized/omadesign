# Interactive cloud dreamscape verification — 2026-09-23

The initial interactive revision replaces the film concept with a persistent homepage
section. This records engineering and visual inspection, not human creative
approval or a public deployment. Desktop application QA evidence is unchanged.

The later texture and layout revision is recorded below; it supersedes the
centered content layout and measurements in this initial pass.

## Initial revision behavior

- Camera rises through rounded clouds; the moon rises, the exact lime O fills
  in, and nine exact wordmark letters draw across its center in sequence.
- The moon disappears while the logo settles to a smaller responsive size.
- Clouds and warm fireflies remain visible and animated beyond 25 seconds.
  Pointer input changes the atmosphere; native scrolling lifts and parts the
  cloud banks and exposes the following content. No modal or body scroll lock.
- Offscreen animation and its frame loop stop, then resume on return without
  replaying the entrance. Motion off freezes the loop; Motion on resumes it.
- Reduced motion begins settled with no continuous frame loop. Keyboard focus
  reaches every control and link. Skip entrance reveals the announcement.
- Fresh final WebGL screenshots show rounded soft clouds and visible fireflies.
  The earlier cloud tile seams and scroll shading boundary are resolved.
- Original brand path geometry is retained. The completed wordmark uses its
  original compound path, avoiding changes from per-letter rasterization.

## Measurements and scope

Chromium on this Linux machine used ANGLE / Mesa AGX G13/G14 hardware WebGL.
Final 1440 × 900 sample: 60.002 fps, mean frame interval 16.666 ms, p95 16.8 ms,
with no frames above 25 ms during the two-second measurement. An earlier
five-second desktop run also averaged 60 fps with no frames above 25 ms.

An emulated 390 × 844 viewport at DPR 3 averaged 57.4 fps over five seconds
(p95 16.8 ms, three frames above 25 ms, maximum 116.7 ms). The atmosphere
canvas was capped at 780 × 1688; the SVG logo retained native resolution.
This is desktop GPU testing at mobile dimensions, not physical-phone testing.

1440 × 900, 390 × 844, 320 × 568 and 844 × 390 layouts were inspected, including
a fixture with a 106 px sticky site header. No horizontal overflow, overlapping
actions or unreachable controls was found. Fresh final desktop and portrait
pages reported no browser errors and confirmed the WebGL renderer.

Site TypeScript, standalone production build and full Vite/Nitro production
build passed. The full local SSR site requires Clerk credentials not present in
the local environment. Homepage integration was checked using the actual
`CloudIntro` client component, header fixtures and the full production build.
Authenticated cloud behavior and public deployment were outside this change.

## Evidence

- `artifacts/cloud-reveal-exports/dreamscape-qa.json`
- `artifacts/cloud-reveal-exports/dreamscape-desktop.png`
- `artifacts/cloud-reveal-exports/dreamscape-portrait.png`
- `artifacts/cloud-reveal-exports/dreamscape-scroll.png`
- `/tmp/omadesign-dreamscape-independent-qa/landscape-header106.png`
- `/tmp/omadesign-dreamscape-independent-qa/narrow-header106.png`

Local review: <http://localhost:5174/cloud-dreamscape.html>.
Implementation and commands: [`../cloud-reveal.md`](../cloud-reveal.md).


## Texture and original-layout revision

The original cloud announcement content and CSS were restored from Git HEAD
`cec7ed4`, retaining all five feature descriptions, the right-hand invitation,
responsive rules, typography and original entrance transitions. The one-time
content reveal preserves the original start at 3 seconds, 2.7-second stagger
and invitation at 24.541667 seconds. Ambient motion remains indefinite.

Browser comparison against that original stylesheet found zero position or
size differences across eleven content selectors at 2440 × 1456, 1440 × 900
and 390 × 844. Font size, weight, line height and transitions also matched.
The comparison uses the actual site global stylesheet in both cases.

Clouds now have multiscale density detail, volume shadows and edge scattering.
The layered sky and continuously advected mist use shared texture noise, while
cloud lighting is still baked once. NASA SVS lunar albedo is bundled locally;
its source and checksum are documented alongside the texture. Exact logo paths
remain unchanged. On portrait screens the settled logo fits the existing gap
above the content; cramped screens use a small, subdued background mark.

Final standalone production bundle was served through an isolated local HTTP
server and checked with Chromium / ANGLE / Mesa AGX hardware WebGL:

- 1920 × 1160 desktop: 60.002 fps, p95 16.8 ms, zero frames above 25 ms
  during a 240-frame sample. Atmospheric buffer 1726 × 1043.
- Emulated 390 × 844 at DPR 3: 60.003 fps, p95 16.8 ms, zero frames above
  25 ms during a 240-frame sample. This is not a physical-phone benchmark.
- No JavaScript/console errors or failing HTTP responses, including the lunar
  texture and generated production assets.
- Pointer response, animated scroll departure, motion pause/resume and offscreen
  pause/resume passed. Reduced motion starts settled and remains static.
- Portrait, 320 × 568, 844 × 390, and portrait with a 106 px sticky header fixture
  had no horizontal overflow. Both project links remained reachable. On short
  screens the existing content scrolls into view before the departure begins.
- TypeScript, standalone production build, full site production build and
  `git diff --check` passed. Full local SSR still requires absent Clerk credentials;
  no public deployment or authenticated cloud workflow was performed.

Latest evidence is in `artifacts/cloud-reveal-exports/texture-pass/`:
`production-qa.json`, `layout-comparison.json`, `production-desktop.png`,
`production-moon.png`, `production-mist-later.png`, `production-scroll.png`,
`production-portrait.png`, `production-narrow.png`, `production-landscape.png`
and `production-portrait-header.png`.


## Remote preview deployment

On 2026-09-23, the reviewed static dreamscape bundle was deployed to the existing
Omadesign Vercel project as preview deployment `dpl_7Wgwvapt9Pg1ghJE6THM59u65sqB`.
URL: <https://omadesign-fm5o1qtss-hustle-launch.vercel.app>. Production was not changed.
A temporary share URL was issued for phone review without Vercel login.
The deployment record and source asset hashes are in
`artifacts/cloud-reveal-exports/texture-pass/deployment.json`.
