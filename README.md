# omadesign

A native Linux studio for **design, paint, photograph, and motion**. One
document, one layer stack. Built so a designer coming from macOS can sit down
and start working.

Pure Rust. No GTK app, no Electron. `cargo` is the toolchain.

UI chrome follows **your** desktop: Omarchy theme colours and the fontconfig /
`omarchy font current` face. Icons are **Phosphor Light**. There is no baked-in
orange.

Binaries are built **on this machine** and uploaded to GitHub Releases. There is
no GitHub Actions bill. They are linked against **glibc 2.35** so they run on
Asahi Omarchy, current Arch ARM, and anything newer.

## Install

```sh
curl -fsSL https://raw.githubusercontent.com/michaelmonetized/omadesign/master/scripts/install-remote.sh | sh
```

That is the whole line. It picks aarch64 or x86_64, downloads the latest
release, and puts `omadesign` on `~/.local/bin`.

Tarball by hand:

| Machine | File |
|---|---|
| Apple Silicon Asahi, aarch64 Linux | `omadesign-*-aarch64-unknown-linux-gnu.tar.gz` |
| x86_64 Linux | `omadesign-*-x86_64-unknown-linux-gnu.tar.gz` |

## Personas

| Persona | You are… | First tools |
|---|---|---|
| **Design** | drawing a logo, a poster, a mark | Move `V`, Pen `P`, Rectangle `R`, Type `T` |
| **Pixel**  | painting or retouching | Brush `B`, Eraser `E`, Clone `J`, Wand `W` |
| **Photo**  | grading a photograph | Crop `C`, develop sliders, Place in Design |
| **Motion** | a mark that moves | Space plays, `K` keys, File → Lottie |

Press **F1** for the full key list.

## Design

- Move, scale (8 handles), rotate (the handle above the box)
- Node tool: drag points and Bézier handles
- Pen: click a corner, click-drag a smooth point, Enter finishes, click the first point to close
- Type: click, type on the canvas, Character studio (font, size, tracking, leading, OpenType)
- Zoom: drag a box to that area; click zooms in, Alt-click out
- Fill / stroke, boolean, align, snap, layers, copy/paste, z-order

## Pixel / Photo

Brush, eraser, clone, fill, marquees, wand. Photo: develop sliders, histogram, crop, Place in Design.

## Motion

Timeline under the canvas. Rest pose stays in Design. Keys are X, Y, rotation, scale, opacity. Export animated SVG or Lottie JSON. Import a shape-layer Lottie.

## Docs

- [User manual](docs/MANUAL.md)
- [Contributing](docs/CONTRIBUTING.md)
- [Roadmap](docs/ROADMAP.md)
- [Go to market](docs/gtm/LAUNCH.md)

## Build from source

```sh
cargo run --release
cargo test
./scripts/release.sh
```

## Architecture

Geometry is defined once and drawn twice — live canvas and PNG/SVG export share
the same contours. Mutations go through `Cmd` + `History`.
