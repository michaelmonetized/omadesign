# Craft comparison: Omadesign, Omatainer and EditBay

Reviewed 2026-10-15. All 27 repositories whose names contain craft in the 64-repository Storytold org snapshot; 9 relevant editors plus shared UI reviewed at targeted source seams. This is a broad first comparison, not an exhaustive line-by-line or runtime audit.

Findings distinguish verified local fixes, source-confirmed gaps, strengths and depth checks. No unsupported feature percentages, estimated parity scores or borrowed readiness claims.

Omadesign focused tests and native file qualification are recorded separately. No Craft application was built/run here. Omatainer hardware and EditBay native tests were not run during this comparison.

## Starting points

- omadesign: 56dacd2b1cfea7b0780cb4070f302ec43e5540e2
- omatainer: 79bfbd6f235b2f2d9bf6a14d10ff406ee2470edd
- editbay: 9df41866ce4b7f466b90543df46dccf98be59638

These are the modern feature/runtime checkouts. Older root checkouts were not used as evidence that a capability was missing. Storytold features below have source evidence unless stated otherwise. No Craft app was run here.

## Results and work order

The private native artwork is delivered with 85 vector objects and zero pixel layers, with its source unchanged and native save/reopen verified. Eighteen SVG tests and 82 other focused tests passed. Visible activity and persistent error text were verified in the headless egui path. This does not certify universal SVG/Affinity fidelity or physical/native interaction across all three apps.

1. Remove avoidable rasterization and make file state visible: first fixes implemented in PRs [213](https://github.com/michaelmonetized/omadesign/pull/213), [215](https://github.com/michaelmonetized/omadesign/pull/215), [218](https://github.com/michaelmonetized/omadesign/pull/218).
2. Compact property fields and movable/saved panel workspaces.
3. Deeper editable text, paint, color and file models.
4. Product-specific photo, audio and video workflow depth, with independent qualification.

Trackers: [omadesign](https://github.com/michaelmonetized/omadesign/issues/216), [omatainer](https://github.com/michaelmonetized/omatainer/issues/517), [editbay](https://github.com/michaelmonetized/editbay/issues/70)

## Findings

### UI-01: Movable, tabbed, saved panels

All three · Workspace · P1 · Gap

**Current:** Three separate frontends; Omadesign has a fixed inspector/hierarchy arrangement, Omatainer separate editor windows, EditBay an authored studio layout.

**Reference:** Shared split/tab/accordion/floating model is wired into multiple Craft frontends.

**Acceptance:** Move, tab, float, close, restore and reset panels. Save per-task layouts. Reopen after changing display scale without losing a panel; keep the canvas protected.

**Evidence:** [Shared docking model: crates/ui/src/docking/model.rs](https://github.com/storytold/craft-libs/blob/8f5a2831614e632f91ad367f7af2a6e5b61b9529/crates/ui/src/docking/model.rs), [VectorCraft panel wiring: crates/ui-egui/src/panel_docking.rs](https://github.com/storytold/vectorcraft/blob/b12a52f7249e0aa73f030587e05ed9ce0c73ab67/crates/ui-egui/src/panel_docking.rs), [omadesign: src/ui/studios.rs](https://github.com/michaelmonetized/omadesign/blob/56dacd2b1cfea7b0780cb4070f302ec43e5540e2/src/ui/studios.rs), [omatainer: src/ui.rs](https://github.com/michaelmonetized/omatainer/blob/79bfbd6f235b2f2d9bf6a14d10ff406ee2470edd/src/ui.rs), [editbay: crates/editbay-app/src/studio.rs](https://github.com/michaelmonetized/editbay/blob/9df41866ce4b7f466b90543df46dccf98be59638/crates/editbay-app/src/studio.rs) · [Tracked work](https://github.com/michaelmonetized/omatainer/issues/128)

### UI-02: A consistent property-field system

All three · Inspector · P1 · Gap

**Current:** Property controls are constructed separately; EditBay exposes a long wrapped row of clip controls and an Apply button. Omadesign mixes property sections in one scroll area.

**Reference:** VectorCraft uses paired field widths, compact labeled controls and selection-aware sections.

**Acceptance:** Use shared field sizing, units, mixed values, focus, reset and error states. Qualify empty/single/mixed/locked selections at 100%, 150%, 200% scale; no clipped labels.

**Evidence:** [vectorcraft: crates/ui-egui/src/panels/mod.rs](https://github.com/storytold/vectorcraft/blob/b12a52f7249e0aa73f030587e05ed9ce0c73ab67/crates/ui-egui/src/panels/mod.rs), [vectorcraft: crates/ui-egui/src/panels/transform.rs](https://github.com/storytold/vectorcraft/blob/b12a52f7249e0aa73f030587e05ed9ce0c73ab67/crates/ui-egui/src/panels/transform.rs), [omadesign: src/ui/studios.rs](https://github.com/michaelmonetized/omadesign/blob/56dacd2b1cfea7b0780cb4070f302ec43e5540e2/src/ui/studios.rs), [editbay: crates/editbay-app/src/timeline_ui.rs](https://github.com/michaelmonetized/editbay/blob/9df41866ce4b7f466b90543df46dccf98be59638/crates/editbay-app/src/timeline_ui.rs) · [Tracked work](https://github.com/michaelmonetized/omadesign/issues/216)

### UI-03: One gesture, one undo step

All three · Editing · P1 · Depth check

**Current:** Omatainer explicitly groups creative slider gestures; Omadesign has typed commands; EditBay uses prepared revision-bound actions. Do not discard these protections during UI work.

**Reference:** VectorCraft panels route through app.run and session interaction previews.

**Acceptance:** Drag a numeric control, type a value, cancel with Escape and Undo/Redo. Each committed gesture creates one named undo entry; stale workers cannot mutate another document.

**Evidence:** [vectorcraft: crates/ui-egui/src/panels/mod.rs](https://github.com/storytold/vectorcraft/blob/b12a52f7249e0aa73f030587e05ed9ce0c73ab67/crates/ui-egui/src/panels/mod.rs), [omatainer: docs/manual.md](https://github.com/michaelmonetized/omatainer/blob/79bfbd6f235b2f2d9bf6a14d10ff406ee2470edd/docs/manual.md), [omadesign: src/ui/object_appearance.rs](https://github.com/michaelmonetized/omadesign/blob/56dacd2b1cfea7b0780cb4070f302ec43e5540e2/src/ui/object_appearance.rs), [editbay: crates/editbay-app/src/workspace.rs](https://github.com/michaelmonetized/editbay/blob/9df41866ce4b7f466b90543df46dccf98be59638/crates/editbay-app/src/workspace.rs) · [Tracked work](https://github.com/michaelmonetized/omadesign/issues/216)

### UI-04: Visible conversion progress and durable failures

Omadesign · Import feedback · P0 · Fixed

**Current:** File work now shows filename, reader/converter stage and elapsed time. Failures remain in a visible window, with Retry for opens/recoveries. A malformed SVG UI regression checks the rendered text and preserves the current document.

**Reference:** Craft frontends expose task-specific activity/status models; the useful requirement is explicit state, not a decorative percentage.

**Acceptance:** Show filename and actual stage while converting and parsing; retain readable failures with Retry and source preserved. Exercise a slow converter, nonzero exit, malformed SVG and timeout.

**Evidence:** [omadesign: src/app/file_io.rs](https://github.com/michaelmonetized/omadesign/blob/56dacd2b1cfea7b0780cb4070f302ec43e5540e2/src/app/file_io.rs), [omadesign: src/formats/affinity.rs](https://github.com/michaelmonetized/omadesign/blob/56dacd2b1cfea7b0780cb4070f302ec43e5540e2/src/formats/affinity.rs), [lightcraft: crates/ui-egui/src/panels/activity.rs](https://github.com/storytold/lightcraft/blob/0da998f24dfc03069d49867f8877af2bf7c51f94/crates/ui-egui/src/panels/activity.rs) · [Tracked work](https://github.com/michaelmonetized/omadesign/pull/218)

### UI-05: Keyboard and accessible property controls

All three · Accessibility · P1 · Depth check

**Current:** Omatainer has accessibility and translated safety flows; coverage must follow the new controls in every app.

**Reference:** craft-ui centralizes button/tab/docking interactions; application labels remain application-owned.

**Acceptance:** Keyboard-only panel moves and numeric editing; stable accessible names, visible focus and announced errors at each state. Record native reader proof separately from source inspection.

**Evidence:** [craft-libs: crates/ui/src/lib.rs](https://github.com/storytold/craft-libs/blob/8f5a2831614e632f91ad367f7af2a6e5b61b9529/crates/ui/src/lib.rs), [omatainer: src/ui/accessibility.rs](https://github.com/michaelmonetized/omatainer/blob/79bfbd6f235b2f2d9bf6a14d10ff406ee2470edd/src/ui/accessibility.rs), [omadesign: src/ui/character_spacing.rs](https://github.com/michaelmonetized/omadesign/blob/56dacd2b1cfea7b0780cb4070f302ec43e5540e2/src/ui/character_spacing.rs), [editbay: crates/editbay-app/src/timeline_ui.rs](https://github.com/michaelmonetized/editbay/blob/9df41866ce4b7f466b90543df46dccf98be59638/crates/editbay-app/src/timeline_ui.rs) · [Tracked work](https://github.com/michaelmonetized/omadesign/issues/216)

### SVG-01: Large vector pages without a bitmap canvas

Omadesign · SVG · P0 · Fixed

**Current:** PR 213 creates a vector document without allocating a page-sized background bitmap. A 100,000 × 80,000 page with 2,000 objects is tested.

**Reference:** VectorCraft constructs a vector document from the SVG dimensions.

**Acceptance:** Native save/reopen, fit/zoom and a viewport-sized render must work. Pixel resource budgets apply to actual decoded raster content.

**Evidence:** [omadesign: src/formats/svg.rs](https://github.com/michaelmonetized/omadesign/blob/56dacd2b1cfea7b0780cb4070f302ec43e5540e2/src/formats/svg.rs), [vectorcraft: crates/svg/src/import.rs](https://github.com/storytold/vectorcraft/blob/b12a52f7249e0aa73f030587e05ed9ce0c73ab67/crates/svg/src/import.rs) · [Tracked work](https://github.com/michaelmonetized/omadesign/pull/213)

### SVG-02: Custom strokes stay vector

Omadesign · SVG · P0 · Fixed

**Current:** Custom miters, skewed/stretched strokes and extended dash arrays now become editable filled Bézier outlines; stroke-first order stays vector.

**Reference:** VectorCraft retains a richer stroke/appearance model.

**Acceptance:** Independent renderer comparison, alpha-aware error bounds, no raster layers, native save/reopen. Outline conversion must be disclosed because width/dash controls are no longer live.

**Evidence:** [omadesign: src/formats/svg.rs](https://github.com/michaelmonetized/omadesign/blob/56dacd2b1cfea7b0780cb4070f302ec43e5540e2/src/formats/svg.rs), [vectorcraft: crates/doc/src/node.rs](https://github.com/storytold/vectorcraft/blob/b12a52f7249e0aa73f030587e05ed9ce0c73ab67/crates/doc/src/node.rs) · [Tracked work](https://github.com/michaelmonetized/omadesign/pull/215)

### SVG-03: Circular radial gradients stay editable

Omadesign · SVG · P0 · Fixed

**Current:** Centered circular padded radial gradients now map to native stops and radius; prior conversion used pixels.

**Reference:** VectorCraft separates gradient definition from full paint placement geometry.

**Acceptance:** Compare multiple stops and opacity with an independent renderer. Preserve focal/elliptical/repeating cases explicitly until the model can represent them.

**Evidence:** [omadesign: src/formats/svg.rs](https://github.com/michaelmonetized/omadesign/blob/56dacd2b1cfea7b0780cb4070f302ec43e5540e2/src/formats/svg.rs), [vectorcraft: crates/color/src/gradient.rs](https://github.com/storytold/vectorcraft/blob/b12a52f7249e0aa73f030587e05ed9ce0c73ab67/crates/color/src/gradient.rs) · [Tracked work](https://github.com/michaelmonetized/omadesign/pull/215)

### SVG-04: Compound paths retain curves and fill rules

Omadesign · SVG · P0 · Fixed

**Current:** Importer now stores compound Bézier contours rather than flattening them to points; single nonzero winding and open-path fills retain their appearance.

**Reference:** VectorCraft keeps path data and fill rules in the document.

**Acceptance:** Exercise holes, self-overlaps, open filled paths with open strokes, transformed curves, save and reopen; compare independent render output.

**Evidence:** [omadesign: src/formats/svg.rs](https://github.com/michaelmonetized/omadesign/blob/56dacd2b1cfea7b0780cb4070f302ec43e5540e2/src/formats/svg.rs), [omadesign: src/geom.rs](https://github.com/michaelmonetized/omadesign/blob/56dacd2b1cfea7b0780cb4070f302ec43e5540e2/src/geom.rs), [vectorcraft: crates/svg/src/import.rs](https://github.com/storytold/vectorcraft/blob/b12a52f7249e0aa73f030587e05ed9ce0c73ab67/crates/svg/src/import.rs) · [Tracked work](https://github.com/michaelmonetized/omadesign/pull/215)

### SVG-05: Stretched, equivalent text spans stay type

Omadesign · SVG type · P0 · Fixed

**Current:** Positive axis-aligned text stretches and matching adjacent spans now import as native type within the native typography model.

**Reference:** VectorCraft retains text transforms and richer per-character style.

**Acceptance:** Retain actual text, font, scale, origin and native roundtrip; compare visible rendering and reject unsupported styling to outlines without losing its appearance.

**Evidence:** [omadesign: src/formats/svg.rs](https://github.com/michaelmonetized/omadesign/blob/56dacd2b1cfea7b0780cb4070f302ec43e5540e2/src/formats/svg.rs), [VectorCraft SVG type: crates/svg/src/import/text.rs](https://github.com/storytold/vectorcraft/blob/b12a52f7249e0aa73f030587e05ed9ce0c73ab67/crates/svg/src/import/text.rs) · [Tracked work](https://github.com/michaelmonetized/omadesign/pull/215)

### SVG-06: Full affine and mixed-style SVG text

Omadesign · SVG type · P1 · Gap

**Current:** Rotation/skew/reflection, multiple positioned chunks, styled spans and textPath still exceed this importer. CharSpan lacks font, size and fill overrides. Nondefault variable-font axes are also not native type yet; inactive axes on static fonts no longer force outlines.

**Reference:** VectorCraft has a raw-XML text prepass with transforms, character positioning, styles, paths and flowed text.

**Acceptance:** Retain editable content, per-character style, positioning and full matrix for rotated, Arabic/RTL, vertical, flowed and path text; check real font substitution and independent appearance.

**Evidence:** [omadesign: src/formats/svg.rs](https://github.com/michaelmonetized/omadesign/blob/56dacd2b1cfea7b0780cb4070f302ec43e5540e2/src/formats/svg.rs), [omadesign: src/geom.rs](https://github.com/michaelmonetized/omadesign/blob/56dacd2b1cfea7b0780cb4070f302ec43e5540e2/src/geom.rs), [VectorCraft SVG type: crates/svg/src/import/text.rs](https://github.com/storytold/vectorcraft/blob/b12a52f7249e0aa73f030587e05ed9ce0c73ab67/crates/svg/src/import/text.rs) · [Tracked work](https://github.com/michaelmonetized/omadesign/issues/216)

### SVG-07: Affine, focal, repeating and patterned paint

Omadesign · SVG paint · P1 · Gap

**Current:** Native Gradient stores endpoints/stops; unsupported radial geometry, repeat/reflect and SVG patterns still rasterize.

**Reference:** VectorCraft has gradient placement geometry, freeform data and reusable pattern definitions.

**Acceptance:** Extend paint representation through render/edit/save/export. Test skewed elliptical/focal radial gradients, nonzero inner radius, repeat/reflect, pattern transforms and nested symbols with no raster fallback.

**Evidence:** [omadesign: src/formats/svg.rs](https://github.com/michaelmonetized/omadesign/blob/56dacd2b1cfea7b0780cb4070f302ec43e5540e2/src/formats/svg.rs), [omadesign: src/gradient.rs](https://github.com/michaelmonetized/omadesign/blob/56dacd2b1cfea7b0780cb4070f302ec43e5540e2/src/gradient.rs), [vectorcraft: crates/color/src/gradient.rs](https://github.com/storytold/vectorcraft/blob/b12a52f7249e0aa73f030587e05ed9ce0c73ab67/crates/color/src/gradient.rs), [vectorcraft: crates/svg/src/import.rs](https://github.com/storytold/vectorcraft/blob/b12a52f7249e0aa73f030587e05ed9ce0c73ab67/crates/svg/src/import.rs) · [Tracked work](https://github.com/michaelmonetized/omadesign/issues/216)

### SVG-08: Complex filter graphs on large vector art

Omadesign · SVG effects · P1 · Gap

**Current:** Only the supported simple blur maps to native filters; other SVG filter groups become bounded pixel layers.

**Reference:** VectorCraft has a dedicated effect import path; it also documents unsupported cases.

**Acceptance:** Preserve a source effect graph or equivalent native operations and render at view/export scale. Test large vector bounds without a mandatory page-sized raster and report every reduced operation.

**Evidence:** [omadesign: src/formats/svg.rs](https://github.com/michaelmonetized/omadesign/blob/56dacd2b1cfea7b0780cb4070f302ec43e5540e2/src/formats/svg.rs), [vectorcraft: crates/svg/src/import/fx.rs](https://github.com/storytold/vectorcraft/blob/b12a52f7249e0aa73f030587e05ed9ce0c73ab67/crates/svg/src/import/fx.rs) · [Tracked work](https://github.com/michaelmonetized/omadesign/issues/216)

### FMT-01: Native Affinity import and effects

Omadesign · Affinity · P0 · Gap

**Current:** Current bridge relies on a separately installed external SVG converter. Its unsupported inner/drop shadow and text-flow data can be lost before our SVG reader sees the document.

**Reference:** VectorCraft has a native Rust container/object/text/paint reader, explicitly a supported subset rather than universal compatibility.

**Acceptance:** Qualify a direct reader against user-authorized real V1/V2/V3 files: paths, native text, masks, artboards, shadows, gradients and placements; retain unknown data and actionable diagnostics.

**Evidence:** [omadesign: src/formats/affinity.rs](https://github.com/michaelmonetized/omadesign/blob/56dacd2b1cfea7b0780cb4070f302ec43e5540e2/src/formats/affinity.rs), [vectorcraft: crates/affinity/src/lib.rs](https://github.com/storytold/vectorcraft/blob/b12a52f7249e0aa73f030587e05ed9ce0c73ab67/crates/affinity/src/lib.rs), [vectorcraft: crates/affinity/src/text.rs](https://github.com/storytold/vectorcraft/blob/b12a52f7249e0aa73f030587e05ed9ce0c73ab67/crates/affinity/src/text.rs) · [Tracked work](https://github.com/michaelmonetized/omadesign/issues/216)

### FMT-02: Illustrator editing data

Omadesign · Illustrator · P1 · Gap

**Current:** AI support currently uses PDF-compatible artwork or a PostScript conversion; private live editing structures are not reconstructed.

**Reference:** VectorCraft has its own AI/editing-data import work, while its roadmap still reports wrong structures in real files and no editing-data writer.

**Acceptance:** Compare real AI fixtures with and without PDF previews; preserve artboards, hierarchy, type and paint. Label unsupported live features and qualify roundtrip independently.

**Evidence:** [omadesign: docs/format-support.md](https://github.com/michaelmonetized/omadesign/blob/56dacd2b1cfea7b0780cb4070f302ec43e5540e2/docs/format-support.md), [vectorcraft: ROADMAP.md](https://github.com/storytold/vectorcraft/blob/b12a52f7249e0aa73f030587e05ed9ce0c73ab67/ROADMAP.md) · [Tracked work](https://github.com/michaelmonetized/omadesign/issues/216)

### FMT-03: Editable PSD text and smart objects

Omadesign · Photoshop · P1 · Gap

**Current:** PSD text and smart objects arrive as saved layer pixels; native vectors/type export as rendered layer pixels.

**Reference:** PhotoCraft has separate text/vector/smart-object document and UI models plus format-level PSD blocks. Presence of the reader alone is not an editable import proof.

**Acceptance:** Independent PSD fixtures must retain editable text, placed source, transforms and smart filters through import, save/reopen and export, with explicit fallback notes.

**Evidence:** [omadesign: docs/format-support.md](https://github.com/michaelmonetized/omadesign/blob/56dacd2b1cfea7b0780cb4070f302ec43e5540e2/docs/format-support.md), [photocraft: crates/doc/src/text.rs](https://github.com/storytold/photocraft/blob/4e7a17cd5d5723313f459753663d0d155f047e19/crates/doc/src/text.rs), [photocraft: crates/ui-egui/src/smart_ui.rs](https://github.com/storytold/photocraft/blob/4e7a17cd5d5723313f459753663d0d155f047e19/crates/ui-egui/src/smart_ui.rs), [photocraft: crates/psd/src/lib.rs](https://github.com/storytold/photocraft/blob/4e7a17cd5d5723313f459753663d0d155f047e19/crates/psd/src/lib.rs) · [Tracked work](https://github.com/michaelmonetized/omadesign/issues/216)

### FMT-04: Native high-depth, profile-aware document pixels

Omadesign · Color and depth · P1 · Gap

**Current:** General document Pixels are RGBA8; high-depth/CMYK PSD becomes RGBA8, with approximate CMYK conversion. Photo development is a distinct higher-precision path.

**Reference:** PhotoCraft has document color modes and CMS; VectorCraft retains authored CMYK/Lab color models.

**Acceptance:** Preserve 16-bit/float and ICC/CMYK authoring through masks, blending, edits and export. Qualify color transforms and display separately; no silent 8-bit reduction.

**Evidence:** [omadesign: docs/format-support.md](https://github.com/michaelmonetized/omadesign/blob/56dacd2b1cfea7b0780cb4070f302ec43e5540e2/docs/format-support.md), [omadesign: src/document.rs](https://github.com/michaelmonetized/omadesign/blob/56dacd2b1cfea7b0780cb4070f302ec43e5540e2/src/document.rs), [photocraft: crates/doc/src/mode.rs](https://github.com/storytold/photocraft/blob/4e7a17cd5d5723313f459753663d0d155f047e19/crates/doc/src/mode.rs), [vectorcraft: crates/color/src/lib.rs](https://github.com/storytold/vectorcraft/blob/b12a52f7249e0aa73f030587e05ed9ce0c73ab67/crates/color/src/lib.rs) · [Tracked work](https://github.com/michaelmonetized/omadesign/issues/216)

### FMT-05: Editable PDF gradients and masks on export

Omadesign · PDF · P1 · Gap

**Current:** Current PDF interchange may turn gradient/masked/effected groups into pixels; exported text is outlined.

**Reference:** VectorCraft and PdfCraft have dedicated vector PDF/document stacks; exact fidelity needs a paired corpus.

**Acceptance:** Preserve vector paint/mask/clip operations and embedded editable text where possible. Render exported files independently and inspect object types, layers and fonts.

**Evidence:** [omadesign: docs/format-support.md](https://github.com/michaelmonetized/omadesign/blob/56dacd2b1cfea7b0780cb4070f302ec43e5540e2/docs/format-support.md), [vectorcraft: crates/pdf/src/lib.rs](https://github.com/storytold/vectorcraft/blob/b12a52f7249e0aa73f030587e05ed9ce0c73ab67/crates/pdf/src/lib.rs), [pdfcraft: crates/content/src/lib.rs](https://github.com/storytold/pdfcraft/blob/afa8e1843cfed613f3d403119f6c2649bce824c9/crates/content/src/lib.rs) · [Tracked work](https://github.com/michaelmonetized/omadesign/issues/216)

### PHOTO-01: Local RAW adjustments and optical corrections

Omadesign · Photo · P1 · Gap

**Current:** DevelopParams has global controls, curve/HSL/crop/rotation; no local adjustment mask or lens profile model. Format documentation calls out lens/profile gaps.

**Reference:** LightCraft pipeline has lens/CA/vignetting, perspective, masks and cached stages.

**Acceptance:** Keep source linear data; store editable local masks and optical settings in sidecars; preview/export agreement at multiple sizes with camera/lens fixtures.

**Evidence:** [omadesign: src/photo.rs](https://github.com/michaelmonetized/omadesign/blob/56dacd2b1cfea7b0780cb4070f302ec43e5540e2/src/photo.rs), [omadesign: docs/format-support.md](https://github.com/michaelmonetized/omadesign/blob/56dacd2b1cfea7b0780cb4070f302ec43e5540e2/docs/format-support.md), [lightcraft: crates/pipeline/src/lib.rs](https://github.com/storytold/lightcraft/blob/0da998f24dfc03069d49867f8877af2bf7c51f94/crates/pipeline/src/lib.rs) · [Tracked work](https://github.com/michaelmonetized/omadesign/issues/216)

### PHOTO-02: A durable photo catalog

Omadesign · Photo library · P2 · Gap

**Current:** Current Photo library is a bounded folder gallery plus source-bound sidecars.

**Reference:** LightCraft catalog has journaled operations, keywords, rules, albums/stacks and versioned persistence.

**Acceptance:** Import a large library, rate/keyword/filter/stack, restart, recover a damaged log and relink moved originals without losing edits. Keep folder-only use simple.

**Evidence:** [omadesign: src/app/photo_session/gallery.rs](https://github.com/michaelmonetized/omadesign/blob/56dacd2b1cfea7b0780cb4070f302ec43e5540e2/src/app/photo_session/gallery.rs), [omadesign: src/photo/edits.rs](https://github.com/michaelmonetized/omadesign/blob/56dacd2b1cfea7b0780cb4070f302ec43e5540e2/src/photo/edits.rs), [lightcraft: crates/catalog/src/lib.rs](https://github.com/storytold/lightcraft/blob/0da998f24dfc03069d49867f8877af2bf7c51f94/crates/catalog/src/lib.rs) · [Tracked work](https://github.com/michaelmonetized/omadesign/issues/216)

### PHOTO-03: Camera support measured by real files

Omadesign · Camera support · P1 · Depth check

**Current:** Built-in LibRaw and verified DNG/CR3/RAF paths are documented; that does not prove every camera/compression mode.

**Reference:** LightCraft has native RAW modules and its own open camera-coverage work. Neither extension lists nor crate counts prove parity.

**Acceptance:** Maintain per-camera/compression/orientation/profile fixtures, independent decode checks, preview/export precision and memory receipts.

**Evidence:** [omadesign: docs/format-support.md](https://github.com/michaelmonetized/omadesign/blob/56dacd2b1cfea7b0780cb4070f302ec43e5540e2/docs/format-support.md), [lightcraft: ROADMAP.md](https://github.com/storytold/lightcraft/blob/0da998f24dfc03069d49867f8877af2bf7c51f94/ROADMAP.md), [lightcraft: crates/pipeline/src/lib.rs](https://github.com/storytold/lightcraft/blob/0da998f24dfc03069d49867f8877af2bf7c51f94/crates/pipeline/src/lib.rs) · [Tracked work](https://github.com/michaelmonetized/omadesign/issues/216)

### LAYOUT-01: IDML interchange

Omadesign · Publishing · P2 · Gap

**Current:** No IDML reader/writer in the supported format routes. Existing layout frames and text threading are useful foundations.

**Reference:** DesignCraft IDML covers styles, pages, master spreads, stories, threads and placed graphics, with explicit unsupported structures.

**Acceptance:** Import/export a multipage styled and threaded layout, preserve links/bleed/masters, inspect the ZIP/XML independently, and report unsupported tables/notes rather than claiming universal InDesign parity.

**Evidence:** [omadesign: docs/format-support.md](https://github.com/michaelmonetized/omadesign/blob/56dacd2b1cfea7b0780cb4070f302ec43e5540e2/docs/format-support.md), [omadesign: src/text_geometry.rs](https://github.com/michaelmonetized/omadesign/blob/56dacd2b1cfea7b0780cb4070f302ec43e5540e2/src/text_geometry.rs), [designcraft: crates/idml/src/lib.rs](https://github.com/storytold/designcraft/blob/a930276fd7462f3d9abd2fbb38544c94c0c911c8/crates/idml/src/lib.rs) · [Tracked work](https://github.com/michaelmonetized/omadesign/issues/216)

### LAYOUT-02: Components, variants, auto layout and prototypes

Omadesign · Layout · P2 · Depth check

**Current:** These already exist: stable component/instance IDs and overrides, solver, token bindings, responsive export and prototype transitions.

**Reference:** FigCraft has auto-layout sizing/constraints plus a broad Figma-like document.

**Acceptance:** Compare nested component overrides, variant swaps, min/max/hug/fill/grid sizing, prototype back/overlays and responsive export through save/reopen; add only measured gaps.

**Evidence:** [omadesign: src/layout_components.rs](https://github.com/michaelmonetized/omadesign/blob/56dacd2b1cfea7b0780cb4070f302ec43e5540e2/src/layout_components.rs), [omadesign: src/layout_prototype.rs](https://github.com/michaelmonetized/omadesign/blob/56dacd2b1cfea7b0780cb4070f302ec43e5540e2/src/layout_prototype.rs), [omadesign: src/layout/solver.rs](https://github.com/michaelmonetized/omadesign/blob/56dacd2b1cfea7b0780cb4070f302ec43e5540e2/src/layout/solver.rs), [figcraft: crates/layout/src/lib.rs](https://github.com/storytold/figcraft/blob/102404e8c3d23d86c3490c1a6beff3202bb73e25/crates/layout/src/lib.rs) · [Tracked work](https://github.com/michaelmonetized/omadesign/issues/216)

### LAYOUT-03: Live merge and presence

Omadesign · Collaboration · P2 · Gap

**Current:** Our cloud model is sync, publishing and comment pins; it is not evidence of simultaneous document merge.

**Reference:** FigCraft has replicated changes and a WebSocket session transport with reconnect/presence code.

**Acceptance:** Two independent clients edit, disconnect, reconnect, undo their own changes and converge without lost objects; qualify permissions, conflicts and live service behavior separately.

**Evidence:** [omadesign: src/cloud.rs](https://github.com/michaelmonetized/omadesign/blob/56dacd2b1cfea7b0780cb4070f302ec43e5540e2/src/cloud.rs), [omadesign: src/app/cloud.rs](https://github.com/michaelmonetized/omadesign/blob/56dacd2b1cfea7b0780cb4070f302ec43e5540e2/src/app/cloud.rs), [figcraft: crates/collab-net/src/lib.rs](https://github.com/storytold/figcraft/blob/102404e8c3d23d86c3490c1a6beff3202bb73e25/crates/collab-net/src/lib.rs), [figcraft: crates/collab/src/merge.rs](https://github.com/storytold/figcraft/blob/102404e8c3d23d86c3490c1a6beff3202bb73e25/crates/collab/src/merge.rs) · [Tracked work](https://github.com/michaelmonetized/omadesign/issues/216)

### MOTION-01: Expressions and composition depth

Omadesign · Motion · P2 · Gap

**Current:** Current motion is object property tracks, easing, presets and a supported Lottie shape subset.

**Reference:** EffectCraft has expression evaluation, composition interchange and property/keyframe subsystems.

**Acceptance:** Expressions must evaluate deterministically with bounded work, named errors and cancellable rendering; preserve project data, nested comps, curves and save/reopen.

**Evidence:** [omadesign: src/motion.rs](https://github.com/michaelmonetized/omadesign/blob/56dacd2b1cfea7b0780cb4070f302ec43e5540e2/src/motion.rs), [effectcraft: crates/expr/src/lib.rs](https://github.com/storytold/effectcraft/blob/0311bdc34d0dc10c2d18ff7875b4f52d483e5e27/crates/expr/src/lib.rs), [effectcraft: crates/interchange/src/lib.rs](https://github.com/storytold/effectcraft/blob/0311bdc34d0dc10c2d18ff7875b4f52d483e5e27/crates/interchange/src/lib.rs) · [Tracked work](https://github.com/michaelmonetized/omadesign/issues/216)

### MOTION-02: 3D scene and model composition

Omadesign · Motion · P2 · Gap

**Current:** Our inspected motion model is 2D object tracks, not a 3D camera/model scene.

**Reference:** EffectCraft model crate includes glTF/OBJ/STL/FBX, materials, skins and extrusion.

**Acceptance:** Define intended 3D scope first; if accepted, qualify camera/light/mesh import, transforms, alpha/color, animation and independently inspected output. A UI toggle is not feature completion.

**Evidence:** [omadesign: src/motion.rs](https://github.com/michaelmonetized/omadesign/blob/56dacd2b1cfea7b0780cb4070f302ec43e5540e2/src/motion.rs), [effectcraft: crates/model/src/lib.rs](https://github.com/storytold/effectcraft/blob/0311bdc34d0dc10c2d18ff7875b4f52d483e5e27/crates/model/src/lib.rs) · [Tracked work](https://github.com/michaelmonetized/omadesign/issues/216)

### AUDIO-01: Saved production and performance workspaces

Omatainer · Workspace · P1 · Gap

**Current:** Compact human UI is present; separate editor windows do not yet establish a general movable panel workspace.

**Reference:** SoundCraft docks tracks, clips, MIDI, memory locations and history, with separate Edit and Mix layouts.

**Acceptance:** Implement the existing workspace issue with library/decks/session/arrangement/MIDI/mixer presets; preserve live load locks, warnings and keyboard/controller focus.

**Evidence:** [omatainer: src/ui.rs](https://github.com/michaelmonetized/omatainer/blob/79bfbd6f235b2f2d9bf6a14d10ff406ee2470edd/src/ui.rs), [soundcraft: crates/ui-egui/src/docking.rs](https://github.com/storytold/soundcraft/blob/3a28a657692c1b98b28c276d8f61df9471074f0f/crates/ui-egui/src/docking.rs) · [Tracked work](https://github.com/michaelmonetized/omatainer/issues/128)

### AUDIO-02: CLAP and broader native plugin choices

Omatainer · Plugins · P1 · Gap

**Current:** Native isolated Linux VST3 work is implemented; CLAP/LV2/AU/VST2 and other-platform proof remain open in the capability matrix.

**Reference:** SoundCraft includes CLAP, VST3 and platform AU host crates. These do not prove every plugin works.

**Acceptance:** Finish each existing format issue separately; qualify scanning, crash recovery, state, automation, MIDI, buses, latency and editor lifecycle using real native plugins.

**Evidence:** [omatainer: src/plugin_host/mod.rs](https://github.com/michaelmonetized/omatainer/blob/79bfbd6f235b2f2d9bf6a14d10ff406ee2470edd/src/plugin_host/mod.rs), [omatainer: docs/capability-matrix.md](https://github.com/michaelmonetized/omatainer/blob/79bfbd6f235b2f2d9bf6a14d10ff406ee2470edd/docs/capability-matrix.md), [soundcraft: crates/mix/src/lib.rs](https://github.com/storytold/soundcraft/blob/3a28a657692c1b98b28c276d8f61df9471074f0f/crates/mix/src/lib.rs) · [Tracked work](https://github.com/michaelmonetized/omatainer/issues/198)

### AUDIO-03: Routing, sends, latency and multichannel depth

Omatainer · Mixer · P1 · Depth check

**Current:** Omatainer already has prepared routing, sidechain/output maps, PDC and source-owned capture. Existing send/bus issues need measured depth checks.

**Reference:** SoundCraft mix models pre/post sends, buses and multichannel main formats up to 16 channels.

**Acceptance:** Paired sessions must check routing, solo/mute, automation, feedback rejection, compensation and offline/live agreement. Add surround only if it belongs in product scope.

**Evidence:** [omatainer: src/engine/audio/routing.rs](https://github.com/michaelmonetized/omatainer/blob/79bfbd6f235b2f2d9bf6a14d10ff406ee2470edd/src/engine/audio/routing.rs), [omatainer: docs/capability-matrix.md](https://github.com/michaelmonetized/omatainer/blob/79bfbd6f235b2f2d9bf6a14d10ff406ee2470edd/docs/capability-matrix.md), [soundcraft: crates/mix/src/lib.rs](https://github.com/storytold/soundcraft/blob/3a28a657692c1b98b28c276d8f61df9471074f0f/crates/mix/src/lib.rs) · [Tracked work](https://github.com/michaelmonetized/omatainer/issues/321)

### AUDIO-04: Professional audio file and stem delivery

Omatainer · Delivery · P2 · Depth check

**Current:** Offline/master/stem work already has owned issues and implementation; do not count those issue titles as proof they are missing.

**Reference:** SoundCraft audio IO includes streamed WAV/RF64/BWF, AIFF and FLAC writing with dither.

**Acceptance:** Inspect delivered headers, dither, metadata, alignment and interrupted export behavior; compare long-file memory and real listening separately.

**Evidence:** [omatainer: src/audio_delivery.rs](https://github.com/michaelmonetized/omatainer/blob/79bfbd6f235b2f2d9bf6a14d10ff406ee2470edd/src/audio_delivery.rs), [omatainer: docs/capability-matrix.md](https://github.com/michaelmonetized/omatainer/blob/79bfbd6f235b2f2d9bf6a14d10ff406ee2470edd/docs/capability-matrix.md), [soundcraft: crates/audio-io/src/lib.rs](https://github.com/storytold/soundcraft/blob/3a28a657692c1b98b28c276d8f61df9471074f0f/crates/audio-io/src/lib.rs) · [Tracked work](https://github.com/michaelmonetized/omatainer/issues/179)

### AUDIO-05: Live MIDI, controllers and device routing

Omatainer · Hardware · P1 · Strength

**Current:** Device routing, MIDI learn/feedback, profiles, controller paths and native qualification already exist in our source. Physical acceptance remains device-specific.

**Reference:** SoundCraft roadmap explicitly says it has no MIDI device layer and only default audio devices with a fixed block size.

**Acceptance:** Preserve our implemented live paths while improving presentation. Report raw MIDI, native routing, physical controller and audible playback proof separately.

**Evidence:** [omatainer: src/engine/midi/learn.rs](https://github.com/michaelmonetized/omatainer/blob/79bfbd6f235b2f2d9bf6a14d10ff406ee2470edd/src/engine/midi/learn.rs), [omatainer: src/engine/audio/routing.rs](https://github.com/michaelmonetized/omatainer/blob/79bfbd6f235b2f2d9bf6a14d10ff406ee2470edd/src/engine/audio/routing.rs), [soundcraft: ROADMAP.md](https://github.com/storytold/soundcraft/blob/3a28a657692c1b98b28c276d8f61df9471074f0f/ROADMAP.md) · [Tracked work](https://github.com/michaelmonetized/omatainer/issues/517)

### AUDIO-06: Session, DJ and arrangement integration

Omatainer · Performance · P1 · Strength

**Current:** Omatainer has clip launching, deck load locks, musical session history and an editable audio/MIDI arrangement.

**Reference:** SoundCraft is centered on a Pro Tools-like edit/mix workflow. Visual similarity is not a reason to remove performance semantics.

**Acceptance:** Keep quantized launch, held-note safety, live load protection and shared history working across workspace changes; stage acceptance requires the actual rig.

**Evidence:** [omatainer: src/ui/arrangement.rs](https://github.com/michaelmonetized/omatainer/blob/79bfbd6f235b2f2d9bf6a14d10ff406ee2470edd/src/ui/arrangement.rs), [omatainer: src/ui/deck_load_lock.rs](https://github.com/michaelmonetized/omatainer/blob/79bfbd6f235b2f2d9bf6a14d10ff406ee2470edd/src/ui/deck_load_lock.rs), [omatainer: src/engine/mod.rs](https://github.com/michaelmonetized/omatainer/blob/79bfbd6f235b2f2d9bf6a14d10ff406ee2470edd/src/engine/mod.rs), [soundcraft: crates/ui-egui/src/docking.rs](https://github.com/storytold/soundcraft/blob/3a28a657692c1b98b28c276d8f61df9471074f0f/crates/ui-egui/src/docking.rs) · [Tracked work](https://github.com/michaelmonetized/omatainer/issues/517)

### VIDEO-01: Task-focused movable video panels

EditBay · Workspace · P1 · Gap

**Current:** RC9 studio has real media, preview, timeline and delivery paths but no shared movable workspace model in the inspected UI.

**Reference:** FilmCraft has docked source/program/timeline/bin panels and saved layouts.

**Acceptance:** Create Edit/Color/Audio/Delivery workspaces with hide/restore/reset, keyboard focus and stable monitor sizing; preserve immutable job ownership.

**Evidence:** [editbay: crates/editbay-app/src/studio.rs](https://github.com/michaelmonetized/editbay/blob/9df41866ce4b7f466b90543df46dccf98be59638/crates/editbay-app/src/studio.rs), [filmcraft: crates/ui-egui/src/dock.rs](https://github.com/storytold/filmcraft/blob/8309f21a6c3f34ddf190af7fd9bfd913053a1fd5/crates/ui-egui/src/dock.rs), [Shared docking model: crates/ui/src/docking/model.rs](https://github.com/storytold/craft-libs/blob/8f5a2831614e632f91ad367f7af2a6e5b61b9529/crates/ui/src/docking/model.rs) · [Tracked work](https://github.com/michaelmonetized/editbay/issues/70)

### VIDEO-02: Direct clip and effect controls

EditBay · Timeline · P1 · Gap

**Current:** Core supports real trim/split/slip/roll/slide, tracks and controls. UI exposes many values in wrapped rows with explicit Apply.

**Reference:** FilmCraft has specialized timeline hit/automation panels and operation-specific editing.

**Acceptance:** Selection-aware compact inspector and direct timeline handles must drive existing typed actions, with live previews, commit/cancel and one-step undo; qualify snapping and keyboard routes.

**Evidence:** [editbay: crates/editbay-app/src/timeline_ui.rs](https://github.com/michaelmonetized/editbay/blob/9df41866ce4b7f466b90543df46dccf98be59638/crates/editbay-app/src/timeline_ui.rs), [editbay: crates/editbay-core/src/timeline.rs](https://github.com/michaelmonetized/editbay/blob/9df41866ce4b7f466b90543df46dccf98be59638/crates/editbay-core/src/timeline.rs), [filmcraft: crates/ui-egui/src/panels/timeline.rs](https://github.com/storytold/filmcraft/blob/8309f21a6c3f34ddf190af7fd9bfd913053a1fd5/crates/ui-egui/src/panels/timeline.rs), [filmcraft: crates/ui-egui/src/panels/timeline_hit.rs](https://github.com/storytold/filmcraft/blob/8309f21a6c3f34ddf190af7fd9bfd913053a1fd5/crates/ui-egui/src/panels/timeline_hit.rs) · [Tracked work](https://github.com/michaelmonetized/editbay/issues/70)

### VIDEO-03: Editorial interchange formats

EditBay · Interchange · P1 · Gap

**Current:** Current shipping core/project routes do not expose an EDL/FCPXML/OTIO/AAF/OMF exchange layer.

**Reference:** FilmCraft interchange reads/writes those timeline formats and reports unmapped features; it does not imply native Premiere parity.

**Acceptance:** Use exact rational timing; import/export linked A/V, gaps, speed/reverse, transitions and relinked media; inspect outputs independently and retain loss reports.

**Evidence:** [editbay: crates/editbay-core/src/lib.rs](https://github.com/michaelmonetized/editbay/blob/9df41866ce4b7f466b90543df46dccf98be59638/crates/editbay-core/src/lib.rs), [filmcraft: crates/interchange/src/lib.rs](https://github.com/storytold/filmcraft/blob/8309f21a6c3f34ddf190af7fd9bfd913053a1fd5/crates/interchange/src/lib.rs) · [Tracked work](https://github.com/michaelmonetized/editbay/issues/70)

### VIDEO-04: Editable caption tracks

EditBay · Captions · P1 · Gap

**Current:** Inspected timeline actions/model have titles but no dedicated caption authoring track/import/export path.

**Reference:** FilmCraft has a separate caption timeline panel and caption subsystem.

**Acceptance:** Import/edit/split/retime/export SRT/VTT with exact cue timing, Unicode, safe-area preview and burn-in/sidecar delivery choices.

**Evidence:** [editbay: crates/editbay-core/src/timeline.rs](https://github.com/michaelmonetized/editbay/blob/9df41866ce4b7f466b90543df46dccf98be59638/crates/editbay-core/src/timeline.rs), [filmcraft: crates/ui-egui/src/panels/timeline_captions.rs](https://github.com/storytold/filmcraft/blob/8309f21a6c3f34ddf190af7fd9bfd913053a1fd5/crates/ui-egui/src/panels/timeline_captions.rs) · [Tracked work](https://github.com/michaelmonetized/editbay/issues/70)

### VIDEO-05: Scopes and a dedicated color workspace

EditBay · Color · P1 · Gap

**Current:** RC9 core has color configuration and exposure/contrast/saturation controls, but that is not a scopes workflow.

**Reference:** FilmCraft scopes panel has waveform, parade, vectorscope and HDR display controls.

**Acceptance:** Drive scopes from the actual program-frame color space; qualify known ramps, HDR units, clipping, paused/playback updates and independent numeric results.

**Evidence:** [editbay: crates/editbay-core/src/composition.rs](https://github.com/michaelmonetized/editbay/blob/9df41866ce4b7f466b90543df46dccf98be59638/crates/editbay-core/src/composition.rs), [editbay: crates/editbay-app/src/timeline_ui.rs](https://github.com/michaelmonetized/editbay/blob/9df41866ce4b7f466b90543df46dccf98be59638/crates/editbay-app/src/timeline_ui.rs), [filmcraft: crates/ui-egui/src/panels/scopes.rs](https://github.com/storytold/filmcraft/blob/8309f21a6c3f34ddf190af7fd9bfd913053a1fd5/crates/ui-egui/src/panels/scopes.rs) · [Tracked work](https://github.com/michaelmonetized/editbay/issues/70)

### VIDEO-06: Multicam sync and cutting

EditBay · Multicam · P2 · Gap

**Current:** Generic tracks and reusable compositions exist; no dedicated multicam angle/synchronization actions in the inspected timeline model.

**Reference:** FilmCraft has angle grids, live cuts, synchronization and camera editing UI.

**Acceptance:** Sync independent camera clips by timecode/markers/audio, switch/cut angles with linked audio policy, reopen and deliver the exact edit.

**Evidence:** [editbay: crates/editbay-core/src/timeline.rs](https://github.com/michaelmonetized/editbay/blob/9df41866ce4b7f466b90543df46dccf98be59638/crates/editbay-core/src/timeline.rs), [filmcraft: crates/ui-egui/src/panels/multicam.rs](https://github.com/storytold/filmcraft/blob/8309f21a6c3f34ddf190af7fd9bfd913053a1fd5/crates/ui-egui/src/panels/multicam.rs) · [Tracked work](https://github.com/michaelmonetized/editbay/issues/70)

### VIDEO-07: Revision-owned jobs and recoverable work

EditBay · Reliability · P1 · Strength

**Current:** RC9 has document owner generations, immutable delivery snapshots, stale-result rejection and durable recovery. Existing saved native QA is evidence from its recorded run, not a new test here.

**Reference:** Craft model/worker boundaries are valuable references, not grounds to discard our existing ownership protections.

**Acceptance:** Retain ownership checks through new panels and effects; switching tabs, editing, closing and cancelling must never attach old output to new work.

**Evidence:** [editbay: crates/editbay-app/src/workspace.rs](https://github.com/michaelmonetized/editbay/blob/9df41866ce4b7f466b90543df46dccf98be59638/crates/editbay-app/src/workspace.rs), [editbay: crates/editbay-app/src/delivery_ui.rs](https://github.com/michaelmonetized/editbay/blob/9df41866ce4b7f466b90543df46dccf98be59638/crates/editbay-app/src/delivery_ui.rs), [filmcraft: crates/ui-egui/src/panels/multicam.rs](https://github.com/storytold/filmcraft/blob/8309f21a6c3f34ddf190af7fd9bfd913053a1fd5/crates/ui-egui/src/panels/multicam.rs) · [Tracked work](https://github.com/michaelmonetized/editbay/issues/70)

### VIDEO-08: Codec breadth versus native playback proof

EditBay · Codec and playback · P1 · Depth check

**Current:** RC9 has actual preview/audio/delivery worker implementations and prior native qualification. They were not rerun during this read-only comparison.

**Reference:** FilmCraft contains a much wider codec/module surface; its roadmap still lists camera, acceleration and delivery gaps.

**Acceptance:** Build a stream/codec/pixel-format matrix, inspect metadata and rendered output, then measure native audio/display timing separately. Module presence is source evidence only.

**Evidence:** [editbay: crates/editbay-media/src/delivery.rs](https://github.com/michaelmonetized/editbay/blob/9df41866ce4b7f466b90543df46dccf98be59638/crates/editbay-media/src/delivery.rs), [editbay: crates/editbay-delivery/src/lib.rs](https://github.com/michaelmonetized/editbay/blob/9df41866ce4b7f466b90543df46dccf98be59638/crates/editbay-delivery/src/lib.rs), [filmcraft: ROADMAP.md](https://github.com/storytold/filmcraft/blob/8309f21a6c3f34ddf190af7fd9bfd913053a1fd5/ROADMAP.md) · [Tracked work](https://github.com/michaelmonetized/editbay/issues/70)

### SUITE-01: Share small UI primitives across products

All three · Shared foundations · P1 · Gap

**Current:** The three app frontends maintain separate theme, control and layout code.

**Reference:** craft-libs provides focused egui buttons, tabs, icons, themes and docking; apps retain document semantics.

**Acceptance:** Extract field/button/tab/panel primitives with application-owned themes and commands. Avoid a new framework; migrate one real inspector in each app before expanding.

**Evidence:** [craft-libs: crates/ui/src/lib.rs](https://github.com/storytold/craft-libs/blob/8f5a2831614e632f91ad367f7af2a6e5b61b9529/crates/ui/src/lib.rs), [omadesign: src/ui/theme.rs](https://github.com/michaelmonetized/omadesign/blob/56dacd2b1cfea7b0780cb4070f302ec43e5540e2/src/ui/theme.rs), [omatainer: src/theme.rs](https://github.com/michaelmonetized/omatainer/blob/79bfbd6f235b2f2d9bf6a14d10ff406ee2470edd/src/theme.rs), [editbay: crates/editbay-app/src/theme.rs](https://github.com/michaelmonetized/editbay/blob/9df41866ce4b7f466b90543df46dccf98be59638/crates/editbay-app/src/theme.rs) · [Tracked work](https://github.com/michaelmonetized/omadesign/issues/216)

### SUITE-02: Shared real-file and visual regression corpus

All three · Qualification · P1 · Depth check

**Current:** Omadesign has real-file/native receipts and synthetic regressions; coverage needs to follow every format loss. EditBay was source-reviewed only during this comparison.

**Reference:** photocraft-corpus organizes real-file test oracles; Craft UI sources also contain interaction and docking tests.

**Acceptance:** Record fixture provenance and licenses, expected editable structure, independent render, save/reopen, memory and failure behavior. Human and physical proof must have separate receipts.

**Evidence:** [photocraft-corpus: README.md](https://github.com/storytold/photocraft-corpus/blob/b2e1dcba8954efa9ad0e7944c258cc0e9a5efc1e/README.md), [vectorcraft: crates/ui-egui/src/panel_docking_tests.rs](https://github.com/storytold/vectorcraft/blob/b12a52f7249e0aa73f030587e05ed9ce0c73ab67/crates/ui-egui/src/panel_docking_tests.rs), [omadesign: docs/format-support.md](https://github.com/michaelmonetized/omadesign/blob/56dacd2b1cfea7b0780cb4070f302ec43e5540e2/docs/format-support.md), [editbay: crates/editbay-app/src/workspace.rs](https://github.com/michaelmonetized/editbay/blob/9df41866ce4b7f466b90543df46dccf98be59638/crates/editbay-app/src/workspace.rs) · [Tracked work](https://github.com/michaelmonetized/omadesign/issues/216)

## Repository coverage

All 27 craft-named repositories in the 64-repository org snapshot are inventoried. Primary comparisons use selected implementation/model/UI seams. Adjacent apps and infrastructure have inventory/entry-point review only. This is a broad first comparison, not an exhaustive code or runtime audit.

| Repository | Pinned revision | Scope | Coverage |
| --- | --- | --- | --- |
| [AniCraft](https://github.com/storytold/AniCraft) | No commits | Empty | No source at the pinned inventory; animation feature comparison unavailable. |
| [cadcraft](https://github.com/storytold/cadcraft) | [fc4e98315a](https://github.com/storytold/cadcraft/tree/fc4e98315a8d771b5ab832f0ade0b5e7b106df7f) | Adjacent | CAD/drafting is outside the three current product scopes; inventoried, not counted as a missing feature. |
| [artcraft](https://github.com/storytold/artcraft) | [d4e4ac69bb](https://github.com/storytold/artcraft/tree/d4e4ac69bbd93d237eaf5d0260bc6a5ff3a5e373) | Infrastructure/AI | AI desktop/media creation product; root architecture inventoried, provider/runtime behavior not qualified. |
| [artcraftx](https://github.com/storytold/artcraftx) | [98c2a48940](https://github.com/storytold/artcraftx/tree/98c2a48940aa0a54604637f665ee539c122fa707) | Infrastructure/AI | AI creation client; no license inferred where metadata is absent. |
| [artcraft-services](https://github.com/storytold/artcraft-services) | [24cb90e16b](https://github.com/storytold/artcraft-services/tree/24cb90e16b5794797c6228040495d087c724dfdc) | Infrastructure/AI | Hosted backend/provider/auth/billing code; no live provider test or permission to copy unknown-license code. |
| [craft-launcher](https://github.com/storytold/craft-launcher) | [3fd2d56999](https://github.com/storytold/craft-launcher/tree/3fd2d56999fa00a7b90f159cb686a5fc64347e7e) | Infrastructure | Suite install/update UX reference; no new installer was implemented by this comparison. |
| [craft-contrib](https://github.com/storytold/craft-contrib) | [5cc4f3a064](https://github.com/storytold/craft-contrib/tree/5cc4f3a064f2e6bbef6b156937ddc06cbc754155) | Infrastructure | Contributor guidelines, not a product capability. |
| [craft-libs](https://github.com/storytold/craft-libs) | [8f5a283161](https://github.com/storytold/craft-libs/tree/8f5a2831614e632f91ad367f7af2a6e5b61b9529) | Primary source comparison | Targeted implementation and model reading; no Craft app was built or run in this turn. |
| [craft-common-actions](https://github.com/storytold/craft-common-actions) | [f0808a5ccb](https://github.com/storytold/craft-common-actions/tree/f0808a5ccbc32869248193ffab0de98d733de451) | Infrastructure | Shared release workflows; a workflow file is not a successful release on every target. |
| [craft-fonts](https://github.com/storytold/craft-fonts) | [3d9ab3a132](https://github.com/storytold/craft-fonts/tree/3d9ab3a132ed0eff457e424226a435a901da30d3) | Infrastructure | Shared fonts and font code; asset licenses require their own review. |
| [craft-launcher-assets](https://github.com/storytold/craft-launcher-assets) | [550fe6ba32](https://github.com/storytold/craft-launcher-assets/tree/550fe6ba32501302156965242bd09798b8400120) | Infrastructure | Pinned screenshots and attribution; reference images are not native test receipts. |
| [designcraft](https://github.com/storytold/designcraft) | [a930276fd7](https://github.com/storytold/designcraft/tree/a930276fd7462f3d9abd2fbb38544c94c0c911c8) | Primary source comparison | Targeted implementation and model reading; no Craft app was built or run in this turn. |
| [effectcraft](https://github.com/storytold/effectcraft) | [0311bdc34d](https://github.com/storytold/effectcraft/tree/0311bdc34d0dc10c2d18ff7875b4f52d483e5e27) | Primary source comparison | Targeted implementation and model reading; no Craft app was built or run in this turn. |
| [figcraft](https://github.com/storytold/figcraft) | [102404e8c3](https://github.com/storytold/figcraft/tree/102404e8c3d23d86c3490c1a6beff3202bb73e25) | Primary source comparison | Targeted implementation and model reading; no Craft app was built or run in this turn. |
| [filmcraft](https://github.com/storytold/filmcraft) | [8309f21a6c](https://github.com/storytold/filmcraft/tree/8309f21a6c3f34ddf190af7fd9bfd913053a1fd5) | Primary source comparison | Targeted implementation and model reading; no Craft app was built or run in this turn. |
| [crafthub](https://github.com/storytold/crafthub) | No commits | Empty | No source at the pinned inventory. |
| [lightcraft](https://github.com/storytold/lightcraft) | [0da998f24d](https://github.com/storytold/lightcraft/tree/0da998f24dfc03069d49867f8877af2bf7c51f94) | Primary source comparison | Targeted implementation and model reading; no Craft app was built or run in this turn. |
| [pdfcraft](https://github.com/storytold/pdfcraft) | [afa8e1843c](https://github.com/storytold/pdfcraft/tree/afa8e1843cfed613f3d403119f6c2649bce824c9) | Primary source comparison | Targeted implementation and model reading; no Craft app was built or run in this turn. |
| [photocraft](https://github.com/storytold/photocraft) | [4e7a17cd5d](https://github.com/storytold/photocraft/tree/4e7a17cd5d5723313f459753663d0d155f047e19) | Primary source comparison | Targeted implementation and model reading; no Craft app was built or run in this turn. |
| [craft-repo-status-app](https://github.com/storytold/craft-repo-status-app) | [0022e899bc](https://github.com/storytold/craft-repo-status-app/tree/0022e899bcfeb41efe9645f86c6513903edc0b79) | Infrastructure | Repository health dashboard, not an editor feature. |
| [deckcraft](https://github.com/storytold/deckcraft) | [9859da3925](https://github.com/storytold/deckcraft/tree/9859da39255c0262cd592bb57101d7f2fa961f72) | Adjacent | Presentation app; useful future publishing reference, not a current product parity target. |
| [soundcraft](https://github.com/storytold/soundcraft) | [3a28a65769](https://github.com/storytold/soundcraft/tree/3a28a657692c1b98b28c276d8f61df9471074f0f) | Primary source comparison | Targeted implementation and model reading; no Craft app was built or run in this turn. |
| [vectorcraft](https://github.com/storytold/vectorcraft) | [b12a52f724](https://github.com/storytold/vectorcraft/tree/b12a52f7249e0aa73f030587e05ed9ce0c73ab67) | Primary source comparison | Targeted implementation and model reading; no Craft app was built or run in this turn. |
| [gridcraft](https://github.com/storytold/gridcraft) | [e6b6b34970](https://github.com/storytold/gridcraft/tree/e6b6b349708ab4978cb3f061f6bd6ecbc25667cb) | Adjacent | Spreadsheet domain; not a missing feature in a design, DJ or video editor. |
| [photocraft-corpus](https://github.com/storytold/photocraft-corpus) | [b2e1dcba89](https://github.com/storytold/photocraft-corpus/tree/b2e1dcba8954efa9ad0e7944c258cc0e9a5efc1e) | Qualification | Real-file oracle corpus; inspected organization, not executed in this turn. |
| [wordcraft](https://github.com/storytold/wordcraft) | [fd0bb2fb1e](https://github.com/storytold/wordcraft/tree/fd0bb2fb1ec081e3eddc4d01917637fef0cafdfe) | Adjacent | Writing/publishing domain; text interchange merits a separate scoped comparison. |
| [placeholder-artcraft](https://github.com/storytold/placeholder-artcraft) | [0daf83dba9](https://github.com/storytold/placeholder-artcraft/tree/0daf83dba9b36e2d9a870114cb12ec873913bd58) | Archived | Historical snapshot, excluded from current capability claims. |

## Boundaries

- Empty AniCraft/crafthub and archived placeholder-artcraft are explicitly accounted for.
- Source presence does not prove native runtime, interoperability, performance, physical audio or display behavior.
- Storytold documents important limits of its own: VectorCraft AI exchange/interaction validation, SoundCraft device/MIDI input, FilmCraft native Premiere interchange, and DesignCraft IDML subsets. Their readiness percentages are not adopted here.
- Review licenses and required attribution before code/asset reuse; unknown-license repositories are not assumed reusable.
- No private client artwork or screenshots are included in the public report.
