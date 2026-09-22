# Landing page refresh — 2026-09-20

The homepage includes native workspace screenshots, an interactive
Design / Pixel / Layout feature overview, six freshly captured native recordings,
and an external-agent workflow section. The original 3D carousel is retained with refreshed native screenshots. The
97-second legacy film is no longer shown on the homepage.

## Media provenance

- Nine screenshots were captured from the local **0.5.4** app with the native
  `--shot SCENE --size 1600x900 --out FILE` path. Their names, dimensions and
  hashes are in `site/public/media/refresh/manifest.json`. WebP conversion only.
- Six continuous native viewport recordings were captured from the current
  **0.5.5-nightly.1** source using `capture_studios`, with actual egui pointer and
  keyboard input. The scenes exercise existing shipped workflows. The capture
  helper's per-frame `last_input` mutation was removed: it invalidated document
  thumbnails on every redraw and prevented the readiness gate from settling.
- The recordings total **131 seconds**, at **1600 × 900 / 30 fps**, with no audio.
  Each replay finished with **zero unresolved input targets**. No HTML UI replica
  or still-image slideshow is presented as a recording.
- Each clip has VP9 WebM and H.264 MP4 encodings, a WebP poster, WebVTT descriptions
  and chapter metadata. WebM is first because some Linux Chromium builds omit
  H.264. Manifest hashes cover both video formats.
- Older source artwork and unused legacy media remain available for existing
  references. They are not presented as refreshed homepage studio footage.

| Clip | Seconds | Chapters |
| --- | ---: | --- |
| graphics / Design | 17 | Gradient placement; gradient direction; group and undo |
| chroma / Pixel | 15 | Compare and sample; apply and undo; reopen preview |
| layout | 15 | Present; draw a frame; undo |
| photo | 24 | Light; color; detail; compare |
| motion | 30 | Presets; keyframes; pop in; slide up; layers |
| brand-kit | 30 | Palettes; brand assets; typography |

The Photo scene uses the original generated landscape documented in
`examples/site-showcase/README.md`. Brand and Motion start from original editable
showcase documents. Chroma key uses a synthetic test subject; it demonstrates
color removal, not automatic subject segmentation.

## Reproduce recordings

Build locally. Use isolated config/data directories and the current graphical
session. For an X11 capture session, `DISPLAY=:0 WINIT_X11_SCALE_FACTOR=1` was used.
The scale override applies only to the capture process.

```sh
cargo build --release --bin capture_studios
# Set XDG_CONFIG_HOME and XDG_DATA_HOME to disposable capture directories.
for scene in graphics chroma layout photo motion brand-kit; do
  target/release/capture_studios "$scene" /tmp/omadesign-capture
done
python3 scripts/publish-recordings.py /tmp/omadesign-capture \
  graphics chroma layout photo motion brand-kit
```

The publisher rejects unresolved replay targets or incorrect MP4 codec, dimensions,
frame count or duration. Review the intermediate native frames before publishing.

## Agent examples

`site/public/media/ai/` contains an original AI-authored campaign in three formats,
with green and violet variations. Every SVG was imported into `.oma`, inspected,
and rendered to PNG by the **0.5.4 native CLI**. The imports have eight editable
vector layers and no import notes. These files, an agent guide, and a ZIP kit are
linked from the homepage. The sources use Nimbus Sans.

The page describes an external coding agent using file and shell access. It does
not advertise an in-app AI assistant, MCP server, or arbitrary prompt-to-edit API.
The examples demonstrate creation, source revisions and batch conversion; painting,
filters, photo adjustment batches and Layout controls remain desktop workflows.

## Validation

- TypeScript and Vercel production build pass.
- Browser checks at 390, 768, 1024 and 1440px: no document-width overflow or uncaught
  page errors; studio and feature selection, responsive Phone preview, AI variant
  switching, clipboard copying and expandable CLI example checked.
- All six WebM clips load and seek in Chromium at their expected durations and
  dimensions. All twelve MP4/WebM files fully decode with FFmpeg.
- Inspected native recording frames and rendered page screenshots. Adjusted the
  wide campaign artwork to keep the title clear of the accent ring.

Existing accepted 0.5.0 human-QA evidence in AGENTS.md remains unchanged. This
website work does not assert human acceptance of a newer desktop build.

## Production result

Published to **https://omadesign.app** using the locally built production artifact.
Deployment: `https://omadesign-3lexrq4rq-hustle-launch.vercel.app` (READY).
Live Chromium verification found no uncaught page errors, successfully sought the
Design WebM player to its seven-second chapter, and exercised both themes. The
ZIP kit, editable poster, SVG source, agent guide and screenshot manifest all
returned HTTP 200. Production HTML includes the new hero and AI section.

## Follow-up correction — 2026-09-21

Restored the original 3D carousel after Michael clarified that it should remain.
Retained the new native screenshots, feature overview, recordings and AI section.
Restored the fullscreen Cloud announcement on arrival and added a replay link.
The original Cloud film now also has a VP9/Opus WebM source for Linux browsers
without H.264 support. Verified automatic playback, dismissal, replay, Escape,
carousel button/keyboard navigation, and mobile width without page errors.

Correction deployed to https://omadesign.app via
`https://omadesign-pyjlrrmc0-hustle-launch.vercel.app` (READY). Repeated the
Cloud autoplay/replay and carousel navigation checks against production: all
passed, with no page errors or mobile document-width overflow.
