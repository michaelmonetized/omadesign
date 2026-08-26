# Atelier

A free, open-source, all-in-one design tool for Linux — a native alternative to
Canva's Affinity — built in Rust. One document, one layer stack, **vector shapes
and raster paint living together**.

Runs on **x86_64 and aarch64** Linux. Pure Rust: no GTK, no Qt, no C toolchain
dependencies — `cargo` is the only requirement on both architectures.

![stack](https://img.shields.io/badge/arch-x86__64%20%7C%20aarch64-blue)

## Build & run

```sh
cargo build --release
./target/release/atelier
```

Headless export (no display needed):

```sh
./target/release/atelier --export-demo   # writes atelier-demo-export.png
```

Tests:

```sh
cargo test
```

## Features (current)

- **Hybrid document model** — vector layers and pixel layers share one stack,
  reorderable, per-layer visibility / lock / opacity
- **Vector tools**
  - Rectangle `R`, ellipse `O`, pen (polyline paths) `P`
  - Text `T` — real glyph outlines from any system TTF (flattened béziers),
    editable content/size in Properties
  - Select `V` — click, move, **corner-handle resize**, delete
- **Raster brush** `B` — flow-based strokes onto pixel layers, committed as a
  single undoable op per stroke
- **Undo/redo** `Ctrl+Z / Ctrl+Shift+Z / Ctrl+Y`
- **Duplicate** `Ctrl+D`, nudge with arrow keys (`Shift` = 10px)
- **Zoom** scroll = pan, `Ctrl+scroll` = zoom to cursor, `+/-`, `0` fit, `1` = 100%,
  middle-drag or `Space`+drag to pan
- **Layers panel** — add vector/pixel, duplicate, delete, reorder, rename target,
  active-layer opacity + lock
- **Save/Open** `Ctrl+S / Ctrl+O` — `.atelier` JSON project format (raster layers
  stored PNG-compressed, base64)
- **PNG export** at 1x/2x/3x via tiny-skia (WYSIWYG with the canvas — same
  geometry feeds both renderers)

## Architecture

```
src/
  document.rs   hybrid layer stack, geometry (rect/ellipse/polyline/text),
                hit-testing, command pattern + history
  render.rs     view transform, egui renderer, ear-clipping triangulator,
                tiny-skia compositor (export), checkerboard, selection UI
  brush.rs      stroke buffer + stamping + alpha compositing
  text.rs       ab_glyph font loading, bézier flattening to contours
  project.rs    .atelier project file (serde DTO, PNG-packed rasters)
  ui.rs         panels: tools, layers, properties, top/status bars
  main_app.rs   tool state machine, input handling, shortcuts
```

Design principle: geometry is defined once and rendered twice — egui for the
live canvas, tiny-skia for export — so exports match the screen.

## Roadmap toward fuller Affinity parity

- Bézier pen (curve segments, anchor/handle editing)
- Boolean ops (union/subtract/intersect/xor) via polygon clipping
- Even-odd fill for text holes on canvas (export already winding-correct)
- Layer groups, masks, blend modes, layer effects
- Gradients, stroke dash styles, arrowheads
- Multi-select, alignment tools, snapping, artboards
- Adjustment layers, CMYK export, PDF/SVG export
- File dialogs (`rfd`), autosave, crash recovery
- GPU raster engine (currently CPU tiny-skia; fine up to ~4k docs)

## Why both architectures work

Every dependency is pure Rust or runtime-dlopen:
`eframe/egui` (UI), `winit` (Wayland/X11), `tiny-skia` (CPU raster),
`ab_glyph` (fonts), `serde` (files). Verified with
`cargo check --target aarch64-unknown-linux-gnu` locally and a CI matrix that
builds and tests on native x86_64 and aarch64 runners (`.github/workflows/build.yml`).

## License

MIT
