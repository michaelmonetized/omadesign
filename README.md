# omadesign

A native Linux studio for **design, paint, and photograph**. One document, one
layer stack, vector and pixels together. Built so a designer coming from macOS
(Affinity, Photoshop, Illustrator) can sit down and start working.

Pure Rust. No GTK app, no Electron. `cargo` is the toolchain.

Binaries are built **on this machine** and uploaded to GitHub Releases. There is
no GitHub Actions bill. They are linked against **glibc 2.35** (Ubuntu 22.04
era) so they run on Asahi Omarchy, current Arch ARM, and anything newer — not
against the bleeding-edge glibc on the build box.

## Download (Asahi / Omarchy / Linux)

Pick the tarball for your CPU from
[Releases](https://github.com/michaelmonetized/omadesign/releases):

| Machine | File |
|---|---|
| Apple Silicon Asahi, aarch64 Linux | `omadesign-*-aarch64-unknown-linux-gnu.tar.gz` |
| x86_64 Linux | `omadesign-*-x86_64-unknown-linux-gnu.tar.gz` |

```sh
tar xf omadesign-*-$(uname -m)-unknown-linux-gnu.tar.gz
cd omadesign-*-$(uname -m)-unknown-linux-gnu
./install.sh
omadesign
```

`uname -m` is `aarch64` on an M1/M2 Asahi box and `x86_64` on Intel/AMD.

## Build from source

```sh
cargo run --release
cargo test
./target/release/omadesign --export-demo
```

Release tarballs (run on the build machine, both host and aarch64 cross):

```sh
cargo build --release
cargo build --release --target aarch64-unknown-linux-gnu
./scripts/release.sh
```

## Personas

Three rooms, one house. Switch from the top bar.

| Persona | You are… | First tools |
|---|---|---|
| **Design** | drawing a logo, a poster, a mark | Move `V`, Pen `P`, Rectangle `R`, Type `T` |
| **Pixel**  | painting or retouching | Brush `B`, Eraser `E`, Clone `J`, Wand `W` |
| **Photo**  | grading a photograph | Crop `C`, develop sliders, Place in Design |

Keys match what you already have in your fingers (Affinity / Adobe). Press **F1**
any time for the full list.

## Design

- Move, scale (8 handles), rotate (the handle above the box)
- Node tool: drag points and Bézier handles, Alt-click converts corner/smooth, click a segment to insert
- Pen: click a corner, click-drag a smooth point, Enter finishes, click the first point to close
- Pencil, rectangle (with corner radius), ellipse, polygon, star, line
- Type: click to place, type on the canvas (caret, Enter for a new line, Esc finishes). Character studio for font, size, tracking, leading, and OpenType (kerning, ligatures, tabular figures, small caps)
- Fill: solid, linear, radial, none. Stroke: width, cap, join, dash
- Colour studio: HSV, hex, swatches, recent. `X` swaps fill/stroke, `D` restores defaults
- Boolean union / subtract / intersect / XOR
- Align and distribute
- Snap to grid, guides, object edges, canvas centre
- Zoom: drag a box to fill the view with that area; click zooms in, Alt-click zooms out. Scroll / Ctrl+scroll always work
- Rulers, optional grid
- Layers: vector or pixel, opacity, blend modes, lock, hide, reorder, masks

## Pixel

- Brush with size, hardness, opacity, flow. `[` `]` size, `Shift+[` `]` hardness
- Eraser, smudge, clone (Alt-click sets source), flood fill
- Marquee, elliptical marquee, lasso, wand (tolerance in the Brush studio)
- Paint lives on pixel layers and undoes as a stroke

## Photo

- Open a folder, drop files, or load built-in samples
- Develop: exposure, contrast, highlights/shadows/whites/blacks, temp/tint
- Presence: clarity, dehaze, vibrance, saturation, global hue
- Tone curve (five points), split tone, HSL per colour band
- Grain, vignette, rotate, crop
- Histogram, before/after, auto tone
- Export the developed JPEG, or **Place in Design** as a pixel layer

## Files

- Project: `.oma` (JSON, rasters PNG-packed)
- Export: PNG (1×/2×/3×), JPEG, SVG
- Native file dialogs. Drag a photo onto the canvas to place it.

## Architecture

Geometry is defined once and drawn twice — live canvas and PNG/SVG export share
the same contours.

```
src/
  geom.rs         points, bounds, Bézier, hit testing     (no UI)
  document.rs     layers, shapes, command history
  compositor.rs   tiny-skia renderer + PNG/JPEG export
  paint.rs        brush, erase, smudge, clone, fill, wand
  photo.rs        develop pipeline + histograms
  boolean.rs      union / subtract / intersect / xor
  text.rs         rustybuzz OpenType shaping + glyph outlines
  tools.rs        personas, tools, shortcut table
  app.rs          studio state, commands, keys
  ui/             chrome, canvas, studios, photo, welcome
```

Mutations go through `Cmd` + `History`. Tests cover geometry, boolean, paint,
develop, project round-trip, SVG, and export.

## Why Linux first

Coming from macOS you should not have to relearn the room. The well is on the
left, colour and layers on the right, the canvas in the middle, Space pans,
Ctrl+scroll zooms, V/A/P/R/B/E are where you left them.

## License

MIT
