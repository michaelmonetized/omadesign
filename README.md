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
| **Pixel**  | painting or retouching | Brush `B`, Eraser `E`, Heal `Shift+J`, Wand `W` |
| **Photo**  | grading a photograph | Crop `C`, develop sliders, Place in Design |
| **Motion** | a mark that moves | Space plays, `K` keys, File → Lottie |

Press **F1** for the full key list.

## Design

- Free transform (`Ctrl+T`): move, scale (8 handles), rotate (the handle above the box)
- Node tool: drag points and Bézier handles
- Pen: click a corner, click-drag a smooth point, Enter finishes, click the first point to close. A twitch under 3px stays a corner.
- Type: click, type on the canvas, Character studio (font, size, tracking, leading, OpenType)
- Zoom: drag a box to that area; click zooms in, Alt-click out
- Fill / stroke, Pathfinder and Divide, stroke outlines, Select Same, align, layers, copy/paste
- Drag-out guides, ruler origins and units, smart alignment and equal-spacing snaps
- Convert artwork to editable object guides; release it back with its original style
- Vector distort, skew, perspective, and a nine-handle warp mesh
- File → Place, drop files on the canvas, Trace (`U`) turns a pixel layer into paths

## Pixel / Photo

Brush, eraser, clone, healing, fill, marquees, wand, and editable layer masks. Photo: develop sliders, histogram, crop, Place in Design.

## Motion

13 editable presets: draw stroke, pop in, slam, shake, fill up, four slide directions,
fly, zoom, buzz, and fade in. Set duration, delay, stagger and intensity, then adjust
the ordinary keys in the timeline. Export animated SVG or Lottie JSON; Lottie
reports unsupported pixels, masks and effects instead of dropping them.

## Templates

**Templates · 52** on the welcome screen, or **File → Template library**. Original
editable vector designs fit all 20 document presets and custom dimensions. Search,
filter, preview, and make one yours. All 52 ship locally; the
[weekly drop plan](docs/template-drops.md) gives each one a suggested adoption idea.

## Docs

- [User manual](docs/MANUAL.md)
- [Contributing](docs/CONTRIBUTING.md)
- [Project status](docs/ROADMAP.md)

## Build from source

```sh
cargo run --release
cargo test
./scripts/release.sh
```

## Architecture

Geometry is defined once and drawn twice — live canvas and PNG/SVG export share
the same contours. Mutations go through `Cmd` + `History`.
