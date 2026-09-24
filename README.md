# omadesign

[![downloads](https://img.shields.io/endpoint?url=https%3A%2F%2Fomadesign.app%2Fapi%2Fstats%3Fbadge%3Ddownloads&style=flat-square)](https://github.com/michaelmonetized/omadesign/releases)
[![Users](https://img.shields.io/endpoint?url=https%3A%2F%2Fomadesign.app%2Fapi%2Fstats%3Fbadge%3Dusers&style=flat-square)](https://www.producthunt.com/products/omadesign)
[![upvotes](https://img.shields.io/endpoint?url=https%3A%2F%2Fomadesign.app%2Fapi%2Fstats%3Fbadge%3Dupvotes&style=flat-square)](https://www.producthunt.com/products/omadesign)
[![stars](https://img.shields.io/endpoint?url=https%3A%2F%2Fomadesign.app%2Fapi%2Fstats%3Fbadge%3Dstars&style=flat-square)](https://github.com/michaelmonetized/omadesign/stargazers)

[Download v0.5.8](https://github.com/michaelmonetized/omadesign/releases/tag/v0.5.8) · [Explore the studio](https://omadesign.app/) · [Updates](https://omadesign.app/updates) · [User manual](https://omadesign.app/docs/manual/) · [Showcase](https://omadesign.app/showcase) · [Discord](https://discord.gg/ejkZS2RBx) · [Build from source](docs/CONTRIBUTING.md)

A native Linux studio for **design, layout, paint, photograph, and motion**. One
document, one layer stack. Built so a designer coming from macOS can sit down
and start working.

Native Rust UI and editing, with bundled LibRaw for camera decoding. No GTK app,
no Electron. `cargo` is the toolchain.

UI chrome follows **your** desktop: Omarchy theme colors and the fontconfig /
`omarchy font current` face. Icons are **Phosphor Light**. There is no baked-in
orange.

Binaries are built **on this machine** and uploaded to GitHub Releases. There is
no GitHub Actions bill. They are linked against **glibc 2.35** so they run on
Asahi Omarchy, current Arch ARM, and anything newer.

## Install

```sh
curl -fsSL https://omadesign.app/install | sh
```

That is the whole line. It picks aarch64 or x86_64, downloads the latest
release, checks its SHA-256, and puts the app in `~/.local`. Nothing is
written to `/usr`. That is the install path on immutable systems
(Silverblue, Bazzite, NixOS, SteamOS desktop) as well as Omarchy.
Run `omadesign --version` to confirm. If the short command is missing,
run `~/.local/bin/omadesign` or add `~/.local/bin` to PATH.

Tarball by hand:

| Machine | File |
|---|---|
| Apple Silicon Asahi, aarch64 Linux | `omadesign-*-aarch64-unknown-linux-gnu.tar.gz` |
| x86_64 Linux | `omadesign-*-x86_64-unknown-linux-gnu.tar.gz` |

## Your work and projects

The welcome screen finds `.oma` documents anywhere under your home directory,
newest modified first, and shows natural-aspect thumbnails. Hidden directories,
Trash and symlinks are excluded. Projects are directories containing `.omabrand`;
open one to browse its subprojects and all descendant documents. Shift-click
starts a multi-selection. Local projects need no cloud account.

Choose **Vector** or **Layout** for templates, or their file icons for a blank
size chooser. **Raster** opens the size chooser. **Photo** enters the workspace,
opens a folder, or opens one image. **Project** opens the brand editor. Motion
is available once there are elements to animate.

**Learn with AI** sends your question and documentation guidance to Omarchy's
default agent. **Create with agent** adds the bundled creation skill and uses
the selected project's directory. Choose an agent in Omarchy's default settings
if none is configured. Version-matched docs and the skill are available offline:

```sh
omadesign --agent-docs manual
omadesign --agent-skill
```

## Personas

| Persona | You are… | First tools |
|---|---|---|
| **Design** | drawing a logo, a poster, a mark | Move `V`, Pen `P`, Rectangle `R`, Type `T` |
| **Layout** | a screen, a landing, a dashboard | Frame `F`, Rectangle `R`, Type `T` |
| **Pixel**  | painting or retouching | Brush `B`, Eraser `E`, Heal `Shift+J`, Wand `W` |
| **Photo**  | grading a photograph | Crop `C`, develop sliders, Place in Design |
| **Motion** | a mark that moves | Space plays, `K` keys, File → Lottie |

Press **F1** for the full key list.

The bottom **Shortcut HUD** follows your tool and held modifiers: letter keys
when idle, command keys with Ctrl/Shift/Alt, and contextual drawing gestures.
**Ctrl+/** toggles it. Hover **+ more** for hints that do not fit the window.

## Design

- Free transform (`Ctrl+T`): move, scale (8 handles), rotate (the handle above the box)
- Node tool: drag points and Bézier handles
- Pen: click a corner, click-drag a smooth point, Enter finishes, click the first point to close. A twitch under 3px stays a corner.
- Type: click, type on the canvas, Character studio (font, size, tracking, leading, OpenType)
- Zoom: drag a box to that area; click zooms in, Alt-click out
- Multi-stop linear, radial, shape and conic gradients for fills and strokes, with angle and alpha controls
- Current/Previous color chips and alpha in every picker, including Design effects
- Object opacity and blend modes, Pathfinder and Divide, stroke outlines, Select Same, align, layers, copy/paste
- Drag sidebar rows to sort/nest; Ctrl+G groups, Ctrl+Shift+G ungroups, Ctrl+8 compounds, Ctrl+Shift+8 releases
- Drag-out guides, ruler origins and units, smart alignment and equal-spacing snaps
- Convert artwork to editable object guides; release it back with its original style
- Vector distort, skew, perspective, and a nine-handle warp mesh
- File → Place, drop files on the canvas, Trace (`U`) turns a pixel layer into paths
- Paste screenshots, copied image files, browser images, text, and SVG with `Ctrl+V`. External content appears in the visible canvas center; copied Omadesign objects keep their original positions.

## Layout

Stack, Wrap and Grid frames, Fixed/Hug/Fill sizing, constraints and breakpoint
overrides. Reuse document-local components, instances and variants; bind color
and spacing variables. Place embedded image fills, then link screens, overlays
and variant changes in Present. Export a frame subtree to PNG, SVG or standalone
responsive HTML. Open the Fieldwork starter to explore. See the
[Layout guide](docs/layout.md) for controls and limits.

## Pixel / Photo

Brush, eraser, clone, healing, fill, marquees, wand and editable pixel layer
masks. Raster studio has **17 filters and 13 effects**, including chroma key with
sampling, similarity, falloff, hardness, spill cleanup and Flow. Compare the
preview, Apply at full resolution and Undo as one edit. Raster treatments are
applied pixel edits; live raster stacks and vector-object masks are not included.
Photo: develop sliders, histogram, crop, Place in Design.

Open camera RAW files including DNG, CR2/CR3, NEF, ARW and RAF with the built-in
LibRaw decoder. Photo keeps the 16-bit linear source for exposure and color edits;
export full-resolution 16-bit PNG/TIFF or JPEG. **Save settings** stores an adjacent
`.omaphoto` file and preserves the camera original. Copy adjustments with
**Ctrl+Shift+C**, select photos with Ctrl/Shift-click or **Ctrl+A**, and paste with
**Ctrl+Shift+V**. Choose the categories to apply; crop and rotation are opt-in.
Save named looks as shareable `.omapreset` files, or apply a look to a whole
folder in the background without decoding every RAW. Camera and compression support,
color rendering and Design placement have
[documented limits](docs/format-support.md#camera-raw).

## Motion

13 editable presets: draw stroke, pop in, slam, shake, fill up, four slide directions,
fly, zoom, buzz, and fade in. Set duration, delay, stagger and intensity, then adjust
the ordinary keys in the timeline. Export animated SVG or Lottie JSON; Lottie
reports unsupported pixels, masks and effects instead of dropping them.

## Templates

**Vector** / **Layout** on the welcome screen, or **File → Template library**. Original
editable vector designs fit all 20 document presets and custom dimensions. Search,
filter, preview, and make one yours. All 52 ship locally; the
[weekly drop plan](docs/template-drops.md) gives each one a suggested adoption idea.

## Palettes and brand libraries

The right sidebar's **Inspect / Palettes / Brand** tabs keep reusable colors and
artwork close to the canvas. Create named **Personal** or **Project** palettes,
collect colors from your selection, search names or hex values, and apply a
swatch to Fill or Stroke. Save, import, or export one palette or a whole collection;
imports preserve existing palettes and transparency.

Load or create a **Brand** bank, add logos and imagery, then drag a tile onto the
canvas or double-click to center it on the selected artboard. Searchable folders,
background previews and automatic refresh keep the collection current. Placement
supports Undo.

**Brand → Typography** keeps TTF and OTF fonts with the project. Add fonts, name
roles such as Heading and Body, then Apply to selected text or choose a role in
Character's **Project fonts** list. Font files are copied locally; no system font
installation is needed. Text stays editable when the project moves.

Project kits travel as `.omacolors`, `.omatype` and the `.omabrand/` folder. Start with the
[Fieldwork example](examples/fieldwork), or follow the
[palette and brand library guide](docs/MANUAL.md#palettes-and-brand-libraries).

## Layered files

Open and edit supported PSD/PSB layers, GIMP `.xcf`, all PDF pages, PDF-compatible Illustrator
artwork, OpenRaster, and SVG/SVGZ. Nested groups keep their order, visibility,
opacity, blend settings and editable masks. Export layered PSD, PSB, PDF and
OpenRaster from File. Imported documents save as `.oma`; source files are preserved.

Affinity `.af`, `.afdesign`, `.afphoto`, `.afpub`, `.aftemplate` and `.afpackage`
use the optional [Affinity bridge](docs/affinity-import.md). Proprietary features
have limits: review **View → Document conversion notes** and the
[format support matrix](docs/format-support.md). Native Affinity and Illustrator
export are not available.

```sh
omadesign --inspect artwork.psd
omadesign --convert artwork.afdesign --output artwork.oma
omadesign --convert artwork.oma --output artwork.pdf
```

## Docs

- [User manual](docs/MANUAL.md)
- [Contributing](docs/CONTRIBUTING.md)
- [Project status](docs/ROADMAP.md)

## Build from source

```sh
cargo run --release --bin omadesign
cargo test
./scripts/release.sh
```

## Architecture

Geometry is defined once and drawn twice — live canvas and PNG/SVG export share
the same contours. Mutations go through `Cmd` + `History`.

## Lua plugins

Open **Plugins → Manage plugins** for the bundled filters, effects, icons, brushes,
tools, patterns, gradients and automation examples. Install `.lua`, folders or
`.omaplug` bundles. See the [Lua API and contribution guide](docs/plugins.md) for
editable output, one-step Undo, canvas gestures and document batch processing.
