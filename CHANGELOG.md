# Changelog

## 2026-09-05 · Pass 4 — A year of good starts

The pudding has a sketchbook, a dance floor, and something fresh for every week.

### The lines can change jobs

- **Object → Guides** turns vector artwork into editable, non-printing guides.
  Béziers stay Béziers, text stays text, and compound paths keep their holes.
  Move a guide, edit its nodes, then release it back into artwork with its style
  intact. Undo and project saves preserve the whole arrangement.
- Snapping follows the actual guide curve, including a Shift-constrained drag.
  Hidden guides stay out of the way; exported artwork leaves guides behind.
  Photographs get a separate bounds guide while their pixels remain untouched.
- Combining and separating compound paths preserve guide state, rotation,
  linear gradients and stacking, with one Undo per operation. Mixed guide and
  artwork inputs are rejected clearly. Radial fills stay radial and follow each
  resulting object's bounds; the current format cannot retain a shared radial
  centre across separated contours.
- **Free transform · Ctrl+T** gives the existing move, scale and rotate handles
  a clear entry in Object and F1. Distort, Skew, Perspective and the nine-handle
  Warp mesh remain one menu away in **Object → Reshape**.
  Reshaping rotated artwork now maps its linear gradient from the correct pose.

### Give it a little life

**Draw stroke, Pop in, Slam, Shake, Fill up, Slide up/down/left/right, Fly, Zoom,
Buzz, and Fade in.** Thirteen starting points, all made from ordinary timeline
keys. Duration stays close; delay, stagger and intensity unfold under Timing &
energy. Each application has its own Undo and preserves unrelated animation.

Draw stroke traces the path instead of fading it. Fill up reveals the interior
from the bottom. The native canvas, animated SVG and Lottie use those reveal
channels. Animated SVG retains masks and effects; Lottie reports unsupported
pixels, masks or effects instead of quietly throwing them away. Moving filtered
artwork also stops being clipped to the box it started in.

The Motion inspector now opens with the presets. Appearance and manual key
controls unfold when needed, leaving more room to choose the next move.

### Fifty-two invitations to make something

**Templates · 52** on the welcome screen and **File → Template library** open a
searchable bank of original designs: events, food, culture, community, editorial,
branding, education, wellness and products. These are editable shapes and live
words, with 13 artwork families and 52 distinct compositions.

Choose any of the 20 document sizes or your own dimensions. Portrait, square and
landscape layouts reflow. Preview rendering happens in the background, cached
thumbnails stay small, and the gallery only lays out visible rows. Creating a
template keeps existing work in its own tab and starts a fresh unsaved document.

All 52 ship locally. The [weekly drop plan](docs/template-drops.md) pairs every
design with a practical remix prompt for a year of adoption campaigns. It is an
editorial plan; posts and public releases are not scheduled automatically.

Pass 4 verification: **230 tests passed**, including the full shortcut suite,
native guide dragging, compound geometry/gradient/undo regressions, editable
preset timing and reveal pixels, export fidelity, unsaved-document preservation,
and compact gallery clipping. The template audit built **1,352 documents**:
52 designs across every preset and six custom/minimum/extreme sizes.

The release build passed. Welcome and floating template galleries, object guides
and the Motion inspector were reviewed at 1600×1000 and 960×640. Portrait and
landscape contact sheets cover all 52 templates. Browser renders of animated SVG
and lottie-web matched the native reveal geometry at four animation stages.
Clippy completed with 34 library advisories (46 including tests); none were hidden.
Physical keyboard and OS dialog interaction remain part of human QA.

## 2026-09-05 · Pass 3 — Rulers, rubber, and a little repair

The pudding has learned to park between the lines. It can also bend them.

### A steadier hand

- Pull guides straight out of either ruler. Grab them again to move them,
  delete a selected guide, or drag it off the canvas to send it home.
  Show/hide and clear controls live in View and the ruler context menus.
- Drag the ruler corner to choose a new zero. Double-click to reset it.
  Switch between px, mm, cm, inches, and points without resizing the artwork.
- Smart snaps find object and artboard edges, centres, and equal gaps. Live
  alignment lines and gap labels explain the landing. Motion uses the visible
  animated geometry too. Point targets are cached; moving objects are excluded
  from the frozen target set, so they cannot snap to their own shadow.
- **Ctrl+;** toggles guides. **Ctrl+Shift+;** toggles snapping. Hold **Ctrl**
  during a drag to reverse snapping temporarily. Hold **Shift** for horizontal,
  vertical, or 45° movement, including pen/brush work and Alt-drag copies.
- Moving a selection, or an artboard with its contents, now returns together
  on Undo. Duplicating several objects also forms one undoable action.

### A little bend goes a long way

**Object → Reshape** adds Distort, Skew, Perspective, and a smooth nine-handle
Warp mesh. Pull a grip, switch modes in the inspector, and finish with Enter
or Done. Esc restores the current drag. Every released drag gets its own Undo.
Changing tools, documents, selections, or personas safely leaves the cage.

Reshape works on vector artwork in Design. Moving the first handle converts
live type and parameter-based shapes into paths; Undo brings their original
form back. Stroke widths stay uniform, and radial gradients stay radial.

### Select with intent. Cut without collateral damage.

- A dedicated **Select** menu finds objects with the same fill, stroke, or
  effects, and objects with or without those properties. It also offers All,
  None, and Invert. Locked/hidden artwork and canvas paper stay out of the way.
- **Expand stroke to outline** respects caps, joins, dashes, rotation, existing
  fills, and layer order. Compound outlines keep their holes.
- **Pathfinder** gathers Union, Subtract, Intersect, XOR, and the new Divide.
  Rotated artwork is processed where it actually appears. Empty intersections
  stay empty, holes subtract area correctly, and each operation has one Undo.
  The repeated boolean implementations now share the same geometry path.

### Keep the good parts

- Editable layer masks reveal, hide, start from a pixel selection, invert,
  remove, or apply to pixels. Black hides; white reveals. Placed-image masks
  follow the image's position, scale, and rotation. The original pixels remain
  intact until Apply, which is itself undoable.
- **Healing brush · Shift+J:** Alt-click clean texture, then paint over a
  blemish. The brush blends texture into the destination's local colour and
  shading while preserving transparency. A stroke keeps an immutable source.
- The eraser now erases actual image pixels. Previously it erased an empty
  preview buffer, which looked busy and accomplished nothing.
- Brush, mask, heal, and selection controls show the settings their tool uses.
  Pixel painting targets the chosen visible, unlocked layer instead of quietly
  falling back to the canvas paper. Undo, Esc, tool changes, and tab changes
  handle in-flight pixel strokes without stranding unrecorded edits.
- Masks survive project saves and SVG export. SVG and canvas rendering agree
  on mask luminance, alpha, placement, mirroring, and mask-before-effects order.

The manual and F1 help cover the new controls. Native review scenes for
reshaping, masking, and healing make the next round of human QA repeatable.

Pass 3 verification: **207 tests passed**, including the full native command-chord
matrix, clipboard/focus regressions, ruler pointer gestures, mid-drag modifier
changes, animated snapping, all four reshape modes, outline/Pathfinder stacking
and undo, mask save/apply/cancel, and real eraser/healing input. A healing seam
regression compares the unchanged demo against its pristine image.

The release build passed. Native reshape, masking, and healing layouts were
reviewed at 1600×1000 and 960×640. Independent SVG rendering matched canvas mask
opacity within 1/255 for vector, rotated-image, and mirrored-image fixtures.
Clippy completed with 32 library advisories (44 including tests); existing
structural/style warnings were not hidden. OS file-dialog interaction and the
physical desktop keyboard remain part of human QA.

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
