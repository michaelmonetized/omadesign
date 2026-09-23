# Cloud reveal brand geometry

The reveal uses the new 0.6.0 geometric O and custom Omadesign wordmark. The artwork is geometry, never a font or an AI reconstruction.

## Monogram

- Source: `/home/michael/Clients/HurleyUS/Ventures/Omadesign/sources/oma/logo-0.6.0-2000-3page.oma`
- Source SHA-256: `877dfa89154aacd3fafd470cc96194b958989a1ac5314230c945b1a12b811bfa`
- Native shape: `68381` (`Union`), Artboard 2. Three closed polygon contours, 6,045 vertices, even-odd fill.
- The same complete shape was verified equal in `/home/michael/Projects/omadesign/media/logo-0.6.0.oma`.
- Repository document SHA-256 at extraction: `93ff0a65686e0192847652f0e7bc71047035eb5a36fe961c71eae72b020fa346`
- Tight viewBox: `-1064.0 447.99997 1024.0 1024.00003`. Native coordinate precision is retained verbatim; no resampling or simplification.
- Native green: `#A6E3A1` (Catppuccin green). Glow is a compositing effect and does not change the source geometry.

## Wordmark

- Source: `/home/michael/Clients/HurleyUS/Ventures/Omadesign/sources/svg/omadesign-wordmark-source-2000.svg`
- Source SHA-256: `defb3dc7a73c75940334ada5d0a81b6355f36dfe14e3f9ba592c8cdc434685ac`
- Exported shape `oma-83069`, ten contours and 10,820 vertices, even-odd fill.
- Original source canvas: 2000 × 2000. Tight viewBox: `64.186 894.500 1871.628 179.000`.
- The entire SVG `d` attribute is copied byte for byte, with its existing export precision.
- Original source fill is `#BAC2DE`; the cloud reveal intentionally uses white (`#FFFFFF`), as requested.

## Source selection

`omadesign-icon-source-2000.svg` is a combined lockup despite its filename. It has a ring cut around the wordmark plus duplicate wordmark paths. It is byte-identical to `icon-0.5.8.svg`, so filenames alone cannot identify the standalone symbol. Using that cut ring by itself would leave incorrect gaps. The reveal instead uses the complete native monogram from Artboard 2.

Combined SVG inspected: `/home/michael/Clients/HurleyUS/Ventures/Omadesign/sources/svg/omadesign-icon-source-2000.svg`. SHA-256: `3cdff0cd48cbe4ac15081314e4e6a57a7b704f7d31a788b438ac693068822a09`.

The source exports, the combined lockup, and the complete native monogram were rasterized with librsvg and visually inspected. Public SVG path data and the TypeScript path data were compared for exact equality.

## Generated assets

| Asset | SHA-256 |
| --- | --- |
| `site/public/media/cloud/brand-icon.svg` | `c29a380767527d7750422563973f3a47364ddd53b3a6056449450986f6bc5c16` |
| `site/public/media/cloud/brand-wordmark.svg` | `6f0b5c7fba1a2f348d2f35124bcaa8ee670484304dfb4ef55a1ef9927a4ff212` |
| `site/src/components/cloud-reveal/brand.ts` | `30b74d96032a714c442d0ce1d417124d32506f0312ed0acc23512f6c9da234e6` |
