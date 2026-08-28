# omadesign user manual

A native Linux studio. Design, paint, photograph. One document, one layer stack.

Keys match Affinity / Adobe. Press **F1** any time.

## Install

```sh
curl -fsSL https://raw.githubusercontent.com/michaelmonetized/omadesign/master/scripts/install-remote.sh | sh
```

That installs `~/.local/bin/omadesign` and a desktop entry. Binaries are glibc 2.35, so they run on Asahi Omarchy, Ubuntu 22.04+, and current Arch.

## First five minutes

1. Launch **omadesign**.
2. Pick a document size, or open the demo.
3. **Design** is the default persona. `R` a rectangle, `P` the pen, `T` type.
4. **Pixel** (`B`) paints on a raster layer.
5. **Photo** opens a folder of pictures and grades them.

Chrome follows your desktop: Omarchy theme colours and the font from `omarchy font current` / fontconfig. Icons are Phosphor Light.

## Personas

| Persona | You are… | First tools |
|---|---|---|
| **Design** | a mark, a poster, a layout | Move `V`, Pen `P`, Rectangle `R`, Type `T` |
| **Pixel**  | painting or retouching | Brush `B`, Eraser `E`, Clone `J`, Wand `W` |
| **Photo**  | grading a photograph | Crop `C`, develop sliders, Place in Design |

## Design

- **Move** `V` — click to select, drag to move, eight handles scale, the handle above the box rotates. Shift-click adds to the selection.
- **Node** `A` — drag points and Bézier handles. Alt-click converts corner/smooth. Click a segment to insert.
- **Pen** `P` — click a corner, click-drag a smooth point. Enter or double-click finishes. Click the first point to close.
- **Pencil** `N` — freehand curve.
- **Rectangle** `R` / **Ellipse** `O` / **Polygon** `Y` / **Star** `S` / **Line** `L` — drag. Shift constrains. Corner radius, sides, and inner radius live in Transform.
- **Type** `T` — click to place, type on the canvas. First keystroke replaces the “Type” placeholder. Enter is a new line. Esc or click away finishes. Double-click existing type to edit. Character studio: font, size, tracking, leading, OpenType (kerning, ligatures, tabular figures, small caps).
- **Gradient** `G` — drag across a selected shape.
- **Eyedropper** `I` — sample fill.
- **Zoom** `Z` — drag a box to zoom to that area. Click zooms in, Alt-click zooms out. Ctrl+scroll always works.
- **Hand** `H` / Space — pan.

Colour studio: HSV, hex, swatches, recent. `X` swaps fill/stroke. `D` restores defaults.

Boolean (Object menu): union, subtract, intersect, XOR. Align and distribute. Snap to grid, guides, objects, canvas centre.

## Pixel

Paint lives on a **pixel layer**. Add one from the Layers studio if the document is vector-only.

- Brush `B` — size `[` `]`, hardness `Shift+[` `]`.
- Eraser `E`, Fill `K`, Clone `J` (Alt-click sets source), Smudge `M`.
- Marquee, elliptical marquee, lasso, wand. Tolerance is in Brush.

## Photo

Open a folder, drop files, or load samples. Develop: exposure, contrast, highlights/shadows/whites/blacks, temp/tint, presence, tone curve, split tone, HSL, grain, vignette, rotate, crop. Histogram and before/after. **Place in Design** drops the developed image as a pixel layer.

## Files

- Project: `.oma` (JSON, rasters PNG-packed)
- Export: PNG (1×/2×/3×), JPEG, SVG
- Native file dialogs. Drag a photo onto the canvas to place it.

## Keys

```
Move V · Node A · Pen P · Pencil N
Rectangle R · Ellipse O · Polygon Y · Star S · Line L
Type T · Gradient G · Eyedropper I · Brush B · Eraser E
Fill K · Clone J · Smudge M · Crop C · Wand W · Hand H · Zoom Z
Undo Ctrl+Z · Redo Ctrl+Shift+Z · Duplicate Ctrl+D
Save Ctrl+S · Open Ctrl+O · New Ctrl+N · Export Ctrl+E
Fit Ctrl+0 · 100% Ctrl+1 · Pan Space · Zoom Ctrl+scroll
```

## Theme and font

omadesign does not ship a brand palette. On launch it reads:

1. `~/.local/state/omarchy/current/theme/colors.toml`
2. `~/.config/omarchy/themes/<current>/colors.toml`
3. stock Omarchy Catppuccin if nothing else is there

UI type is `omarchy font current`, then fontconfig `sans-serif`. Override with `OMADESIGN_FONT=/path/to/font.ttf`.
