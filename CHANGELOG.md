# Changelog

## 2026-09-05 · Pass 2 — Ctrl means control

The keyboard gets its keys back, and the inspector gets room to breathe.

### The little things that were very big things

- Copy, cut, and paste now understand the clipboard events Linux actually sends.
  Quick modifier chords survive releasing Ctrl before the next frame is drawn.
- Editing a colour, number, layer name, or text field keeps its own clipboard,
  selection, and undo keys. Canvas type editing no longer steals inspector focus.
- Shift variants get their turn: save as, place, redo, release, front/back,
  brush hardness, and the two pixel marquees. Copy/paste style works through
  the native clipboard too.
- Alt-scroll keeps zooming through its final glide. Photo scrolling stays in
  the view under your pointer; Space, Hand, and middle-drag actually pan.
  Fit and 100% now address the photo when Photo is active.

### Less dashboard. More drawing.

- The right inspector starts with the thing you selected: its name, shape,
  geometry, and actual appearance. Fill and stroke are tidy rows with a colour
  chip, editable hex value, and opacity. Colour collections and stroke details
  open when you need them.
- Geometry fields line up. Type controls appear with type. Layers have readable
  hierarchy and object icons, with visibility and locking close at hand.
- Photo development is organised into **Light**, **Colour**, and **Detail**.
  Advanced curves and colour work unfold on demand. The histogram and Before
  comparison stay within reach, and export has a clear home at the bottom.
- Photo JPEG export runs in the background, skips an unnecessary PNG round trip,
  and reports errors instead of claiming success when a write fails.

### Same build, from code to your launcher

The installer now works from the source checkout and release packages. It swaps
the binary atomically, respects the desktop application directory, and passes
opened files to the app. Existing windows can finish safely; relaunch before QA
to use the new build.

Native screenshot captures accept `--size WIDTHxHEIGHT` and ignore desktop input,
so typing elsewhere cannot quietly edit the scene being reviewed.

Pass 2 verification: **156 tests passed**, including the command-chord matrix,
native clipboard events, modifier release, text-field focus, canvas gestures,
Photo panning/zooming, and inspector sizing. A separate isolated Ctrl+S check
saved and reopened live canvas text, then saved again with an inspector field
focused. Native layouts were reviewed at 1600×1000 and 960×640. The release build,
installer shell check, and desktop-entry validation passed.

Native Open, Place, Save As, and Export dialogs have shortcut dispatch coverage;
their OS interaction remains part of human QA. Physical keyboard injection was
not used. Clippy still reports advisory structural/style warnings; no blanket
lint suppression was added.

## 2026-09-05 · Pass 1 — The pudding reduction

A lighter studio starts with less work between your hand and the canvas.

### Weight off the frame

- Document tabs transfer ownership instead of copying documents, undo history,
  and raster buffers every frame. Recovery compresses changed snapshots in the
  background and rejects stale results before they replace a saved state.
- Asset search, icon downloads, and font requests leave the drawing loop.
  Font and icon catalogues are reused; shape paths and pixel previews keep
  caches that follow their actual content.
- Rulers, grids, and checkerboards draw the visible area. Photo auto-tone uses
  a histogram, and unused clarity work stays out of the pipeline.

On this ARM machine, a synthetic scene with twelve 1024×1024 raster layers went
from roughly **6.1–6.8 ms to 0.11 ms** per unchanged egui CPU frame. A 1920×1080
JPEG export went from **31–32 ms to 18.5 ms**. These are local workload measurements,
not a GPU frame-rate or general performance guarantee.

### Fewer ghosts in the machine

- Fixed portrait-image blur crashes, stale path caches after text/geometry edits,
  alpha conversion, flood-fill comparisons, transparent PNG export, and gradient
  opacity. JPEG transparency is composited onto white.
- Fixed dirty-state and recovery edge cases around saving, undo, live type,
  and closing inactive tabs.
- Removed dead helpers, weak or environment-dependent tests, stale launch and
  QA notes, and tracked generated website output — about 3 MB of obsolete files.
  Retained useful behavioural coverage and added regressions for actual failures.
- Separated document tabs, recovery, keyboard handling, and photo state from the
  main app module. Removed fake asset providers and silent random substitutions.

### A quieter place to start

The welcome screen, document tabs, toolbar, status bar, and motion timeline use
clearer spacing and calmer surfaces. Compact windows get a persona picker.
Phosphor icons render from their own font, and the app still follows your desktop
theme and font.

Pass 1 verification: 140 tests passed, the native release build succeeded, and
eight native scenes plus compact layouts were visually checked. The website
build also passed. This remains an alpha; local QA builds are separate from the
portable, dual-architecture release process.
