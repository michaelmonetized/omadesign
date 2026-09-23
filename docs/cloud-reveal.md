# Interactive cloud announcement

The homepage opens with a living WebGL dreamscape. It is a normal document
section, with native scrolling into the existing Omadesign homepage. There is
no dialog, playback bar, finite ambient timeline, or scroll lock.

## Local review

```sh
npm --prefix site run dev:cloud-dreamscape
```

Open <http://localhost:5174/cloud-dreamscape.html>. This mounts the same
`CloudIntro` used by the homepage, plus a page below it to demonstrate the
scroll transition. Its project and guide links lead to the actual website.
The old `cloud-film.html` URL remains an alias for this experience.

The local review service can be stopped with
`systemctl --user stop omadesign-cloud-preview`.

```sh
npm --prefix site run build:cloud-dreamscape
```

This produces the standalone review page under `artifacts/cloud-reveal/`.
The normal `npm --prefix site run build` includes the announcement on the
homepage. Public deployment is separate.

## Choreography

- The camera rises through rounded, moonlit cloud banks. The clouds descend
  into their resting position along the lower edge of the viewport.
- The moon rises into view. The exact O maze fills along a circular drawing
  mask, followed by the nine exact wordmark letters drawn in sequence across
  its center.
- The moon disappears. The logo settles to a smaller size and remains visible.
- After the roughly nine-second entrance, clouds continue drifting and warm
  fireflies continue floating and pulsing. Pointer movement gently parts nearby
  clouds and draws nearby fireflies toward the pointer.
- Native document scrolling lifts and parts the clouds with depth as the
  existing homepage comes into view. Returning preserves the settled scene.

## Performance and accessibility

Cloud lighting is calculated once into a small sprite atlas. Each live frame
draws layered cloud billboards, two flowing mist layers and a single firefly point batch, rather than
running a long volume ray march across every display pixel. The atmospheric
canvas caps its pixel budget; the exact SVG logo stays at native resolution.

The logo entrance, original content reveal and ongoing ambient clocks are separate.
The five original features reveal from bottom to top, beginning at 3 seconds
with 2.7 seconds between items. The original invitation appears at 24.541667
seconds (the previous background clip duration). The content keeps its original
text, two-column layout, typography, responsive rules and easing. The frame
loop stops while the section is offscreen or the document is hidden. It
resumes without replaying the entrance. Pointer and scroll inputs are smoothed
without React state updates on each frame. Layout is measured on resize, not
inside the rendering loop.

Reduced motion shows the settled scene without a continuous animation loop.
The announcement has no extra brand label, Explore link, motion toggle or
skip control. Touch scrolling is never intercepted. A CSS cloud/firefly fallback retains the announcement
if WebGL is unavailable.

Clouds use fractal density erosion, volume shadowing and thin-edge scattering.
Their surfaces evolve gently during playback. The sky has multiple star depths
and galactic dust; two independently advected mist layers flow in front of and
behind the cloud banks. The moon uses a locally bundled NASA LROC color map
projected onto a lit sphere, with soft atmospheric bloom. Source credit and
checksum are in `site/src/components/cloud-reveal/textures/SOURCES.md`.

The standalone preview imports the actual site stylesheet. A browser comparison
against the original content CSS confirms matching positions, dimensions, fonts
and transitions at desktop and portrait sizes. On short screens, native scrolling
keeps the original content reachable before the atmosphere exits.

## Source map

- `site/src/components/cloud-waitlist.tsx` and `.css`: homepage layout, native
  scroll runway and announcement links.
- `site/src/components/cloud-reveal/index.tsx`: entrance choreography,
  persistent lifecycle, pointer smoothing and exact SVG artwork.
- `site/src/components/cloud-reveal/renderer.ts`: clouds, moon, stars,
  fireflies and their pointer/scroll response.
- `site/src/components/cloud-reveal/brand.ts` and `letters.ts`: original
  geometry and exact letter grouping. The completed wordmark renders its
  original compound path to preserve its rasterization.
- `site/public/media/cloud/BRAND-SOURCES.md`: source provenance and hashes.

The superseded film exporter, score and audio controller were archived under
`artifacts/cloud-reveal-exports/original-film-source/`. They are not part of
the homepage runtime. Previous movie exports remain available in that parent
directory as historical artifacts, not as this experience's implementation.

## Full-site branch preview

The `preview/cloud-dreamscape` branch replaces the homepage cloud section while
retaining the actual site navigation, product sections, screenshots, recordings,
installation content, updates and documentation. It is not the standalone review
page.

The preview is built with `VITE_PUBLIC_SITE_PREVIEW=1` because the Vercel project
has authentication credentials only in its Production environment. This explicit
build mode omits the global authentication provider and middleware and redirects
cloud, account, project, showcase, competition and API routes to the live site.
It never creates a second authenticated backend or copies production secrets.
Normal builds retain their existing authentication behavior.

Build from the repository root after `vercel pull --environment=preview`:

```sh
VITE_PUBLIC_SITE_PREVIEW=1 mise exec bun@1.4.2 -- vercel build --yes
vercel deploy --prebuilt --archive=tgz --yes
```

This creates a preview deployment only.
