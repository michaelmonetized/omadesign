# Cloud reveal verification — 2026-09-23

Historical record of the superseded film concept. See the
[archive inventory](../../artifacts/README.md) for the preserved film and later
dreamscape exports. Paths and deployment checks below refer to the original
September 23 environment; the archive does not deploy or change the live site.

This records engineering checks for the new 20-second SVG/WebGL composition.
It is not a human creative approval or a public deployment record. The accepted
0.5.0 desktop QA evidence in `AGENTS.md` is unchanged.

## Builds and geometry

- Site TypeScript check passed.
- Site Vite/Nitro production build passed.
- Standalone composition production build passed; portable player is under
  `artifacts/cloud-reveal/`.
- Exact native 0.6.0 monogram contours and SVG wordmark path were verified
  against the source files available during the original verification.
- No typeface substitution or geometry simplification. White wordmark and lime
  presentation color follow the requested composition.

## Browser rendering and interaction

Chromium on Linux used hardware WebGL: ANGLE / Mesa AGX G13/G14. Desktop,
portrait at device pixel ratio 3, and a 3840 × 2160 full-resolution canvas were
inspected. The final built standalone bundle returned no JavaScript errors,
console errors, or failing HTTP responses.

- Pause freezes time, scrubbing lands at the requested time, restart/replay
  returns to the beginning, and the final frame stops at 20 seconds.
- Reduced motion starts at a static final plate.
- Deliberately unavailable WebGL leaves SVG artwork and the CSS fallback.
- Monogram and wordmark retreat as one group; gap relative to icon diameter
  was 0.150367147 at 12s and 0.150366984 at 16s.
- During concurrent video export, five-second playback samples averaged
  36.28 fps at desktop and 37.75 fps at portrait. This is a test on this
  machine, not a physical-phone performance claim. Time tracked wall time.
- Adaptive atmosphere buffers were 1633 × 919 and 833 × 1802; SVG stays at
  native display resolution. Full-quality capture disables that pixel budget.

Actual `CloudIntro` was mounted with the site's real stylesheet in a temporary
client fixture at 1440 × 900, 390 × 844, and 320 × 568, plus reduced motion.
Skip reveals the actual `/cloud` and `/docs/cloud` links; replay hides and
inerts the copy. Escape and Explore close the dialog, dispose graphics, restore
focus and body scrolling. `#cloud` reopens with sound muted. Narrow-screen
content scrolls vertically, links remain reachable, and no horizontal overflow
was found. Temporary fixtures were removed.

The full local SSR site still requires Clerk credentials absent from the local
environment. Integration was checked through the actual client component and
the production build; authenticated cloud behavior was outside this change.

## Evidence

- `artifacts/cloud-reveal-exports/browser-qa.json`
- `artifacts/cloud-reveal-exports/hero-4k.png`
- `artifacts/cloud-reveal-exports/portrait.png`
- `/tmp/omadesign-composition-qa/dialog/report.json`
- `/tmp/omadesign-composition-qa/audit/report.json`

The original synthesized score is 20 seconds, stereo AAC at 48 kHz. Offline
analysis found no clipping (peak −4.8 dBFS). The browser fetches it only after
sound is requested. Score provenance and regeneration are documented in
[`SCORE.md`](../../artifacts/cloud-reveal-exports/original-film-source/SCORE.md).

## Delivered movie

`artifacts/cloud-reveal-exports/omadesign-cloud-1080p.mp4` was rendered from the
frozen standalone build: 1920 × 1080 H.264, 24 fps, 480 frames, 20.000 seconds,
stereo AAC at 48 kHz. FFmpeg decoded the entire file with no errors. Size:
9,872,729 bytes. SHA-256:
`9fbd0174bea98ee701d2f858a4638a85b13318361bee7aa1cb83a260b332c8a0`.

The portable player is also packaged as
`artifacts/cloud-reveal-exports/omadesign-cloud-player.tar.gz`.
