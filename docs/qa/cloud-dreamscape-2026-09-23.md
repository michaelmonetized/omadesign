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

## Full production-site context preview

Branch: `preview/cloud-dreamscape`; deployed website revision: `5482598`.
Vercel preview: <https://omadesign-m9zfc7q76-hustle-launch.vercel.app>
(`dpl_GJ5Feih9wpcHZxjbN963EVoAceff`). This contains the actual homepage and
public documentation rather than the standalone follow-on review page.

Removed the cloud section's plain brand label, Explore link, Motion toggle and
Show cloud links control. Kept the existing feature and invitation choreography.
Excluded the cloud experience from the general homepage surface reveal observer:
its animation had been competing with the canvas's own entrance on the full page.

Remote Chromium checks passed at 1440 × 1000 and emulated 390 × 844 (DPR 3):
HTTP 200, WebGL active, settled logo and continuing ambient motion, full original
product sections present, native scroll departure into the product content,
no removed controls, no horizontal overflow, no JavaScript errors or HTTP failures.
Cloud guide navigation returned the actual documentation. Cloud app routes return
307 to production, intentionally, because this public review build has no preview
authentication credentials. No authenticated backend workflow was tested.
TypeScript, full Vercel build and whitespace checks passed.

Evidence: `artifacts/cloud-reveal-exports/texture-pass/fullsite-qa.json`,
`fullsite-desktop.png`, `fullsite-phone.png`, and corresponding `-product.png`
screenshots. Production remained `dpl_9YmM6ZrTpqFXdijNLUBXUUy45CQa`.

## Smoke, dust and interactive turbulence revision

Cloud volume baking now perturbs the density field and erodes it at a finer
scale, breaking up the former smooth lobe outlines. Live cloud shading folds
multiple noise scales into the lit atlas, with stronger drift, rolling edges
and subtle expansion. Two mist depths use independently advected, warped smoke
fields and exponential density-to-opacity shading. A separate GPU particle
pass adds 160 faint drifting dust motes alongside the existing warm fireflies.

Pointer movement and mouse/touch presses deposit up to eight expanding vortex
wakes. The same world-space field parts mist, deforms cloud edges and displaces
dust; each wake dissipates on the scene clock. Event listeners are passive,
retain native scrolling/link behavior, and clear touch attraction on release
or cancellation. Current element bounds are used after scrolling. Reduced
motion clears all wakes; existing offscreen/hidden-tab suspension is retained.
The original content layout, brand geometry and reveal timing remain intact.

Validation: TypeScript, standalone and full site production builds pass.
A deterministic GPU readback at the same scene time found 13,459 color channels
changed by a gesture; the expired wake produced exactly the baseline pixels.
No WebGL error was reported. Browser screenshots, performance and lifecycle
results are saved in `artifacts/cloud-reveal-exports/shader-pass/`, including
`qa.json` and `interaction.json`. Phone dimensions are desktop emulation,
not physical-device GPU measurements. This revision has not been deployed.

Final static production-bundle checks: 1440 × 900 desktop and emulated
390 × 844 at DPR 3 both averaged 60.002 fps across 180 frames (p95 16.7 ms
and 16.8 ms respectively). Neither showed horizontal overflow, JavaScript
errors or WebGL errors. Offscreen suspension and reduced-motion stillness
passed. A dispatched touch swipe scrolled the document 385 px; changing the
reduced-motion preference while running stopped animation successfully.

### Shader revision: full-site Vercel preview

Deployed the tested local shader changes with the full website on 2026-09-23:
<https://omadesign-mrono83xj-hustle-launch.vercel.app>
(`dpl_21PyUP2SKJVK7dEWWCDKnsaUn4bz`, READY, preview). This is the successor
review deployment to `omadesign-m9zfc7q76-hustle-launch.vercel.app`.
Built using `VITE_PUBLIC_SITE_PREVIEW=1 vercel build` and deployed prebuilt.
The public-preview authentication redirects remain as documented above.

Remote verification at 1440 × 1000 and 390 × 844 confirmed WebGL rendering,
settled artwork, persistent atmospheric motion, pointer response, native scroll
departure, real homepage sections and no horizontal overflow. No JavaScript
errors were reported; the phone WebGL error check returned zero. The cloud guide
returned HTTP 200. The served shader bundle SHA-256 matches the local build:
`40526e79a4b4a4150d2c0247d0020f4cbcd64b2ff98f5cb83d4a3b71fdcb9f91`.
Screenshots and deployment evidence are in the `shader-pass` artifact directory.

Production was checked before and after and remained
`dpl_Gg88J74WUqaDAxo5Vk3Wki4pZT7h`. A temporary authenticated share link was
created for review without a Vercel login; deployment protection remains enabled.

### Earlier invitation and production preparation

Michael authorized production publication after making “share the work” appear
sooner. The invitation now starts at 5 seconds instead of 24.541667 seconds,
with a 0.8-second transition. Feature-list staging remains independent and
continues through 13.8 seconds; reduced motion reveals everything immediately.
Browser checks confirmed the invitation hidden/inert at approximately 4 seconds,
active at approximately 5 seconds with only the first feature revealed, and all
five features visible by 14.65 seconds. Reduced motion showed all content with
the animation paused. TypeScript and whitespace checks passed.

The production robots.txt and sitemap.xml fix from `b087f34` is included to
preserve the newer production site's crawlability. Production must use a fresh
`vercel build --prod` with preview mode disabled, retaining real authentication.

### Production publication

Published revision `a85d51d` to <https://omadesign.app> on 2026-09-23.
Production deployment `dpl_6ShqaypPdbqiT7CKizbyph2ZWCgR` is READY:
<https://omadesign-o7wmxsy7o-hustle-launch.vercel.app>.
The rollback deployment is `dpl_Gg88J74WUqaDAxo5Vk3Wki4pZT7h`.

The live browser confirmed the invitation becomes active at approximately
5 seconds with the first feature visible. Desktop 1440 × 1000 and phone-sized
390 × 844 checks showed WebGL rendering and no horizontal overflow or JavaScript
errors. The real cloud page loads and its Sign in button opens the Clerk sign-in
modal. No account credentials were submitted or authenticated project workflow
tested. The production build has no preview-mode cloud redirects.

The served shader bundle matches the production build SHA-256
`04c57f12f94e9ecbf28b6927caf2e236ca5e835f625808907fe6d9ef2d4f4ee7`.
Cloud, cloud guide, installer, robots.txt and sitemap.xml return HTTP 200; the
installer and crawlability files match the local output. The deployment-scoped
error-log query returned no entries immediately after verification.
Production screenshots, endpoint hashes and deployment receipt are saved in
`artifacts/cloud-reveal-exports/shader-pass/production-*`.
