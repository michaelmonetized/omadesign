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
| **Motion** | animating the artboard | Space play, `K` key, File → Lottie |

## Design

- **Move** `V` — click to select, drag to move, eight handles scale, the handle above the box rotates. Shift-click adds to the selection.
- **Node** `A` — drag points and Bézier handles. Click a segment to insert. Alt-click converts corner/smooth. Delete removes the selected point. Object → Break path. Shapes convert to a path the first time you edit them.
- **Pen** `P` — click a corner, click-drag a smooth point. The cubic is drawn as you go. Enter or double-click finishes an **open** path. Click the first point to close. Click an open endpoint to continue it, or to join it to the path you're drawing.
- **Pencil** `N` — freehand curve.
- **Rectangle** `R` / **Ellipse** `O` / **Polygon** `Y` / **Star** `S` / **Line** `L` — drag. Shift constrains. Corner radius, sides, and inner radius live in Transform.
- **Type** `T` — click to place, type on the canvas. First keystroke replaces the “Type” placeholder. Enter is a new line. Esc or click away finishes. Double-click existing type to edit. Character studio: font, size, tracking, leading, OpenType (kerning, ligatures, tabular figures, small caps).
- **Gradient** `G` — drag across a selected shape.
- **Eyedropper** `I` — sample fill.
- **Trace** `U` — raster to vector on the active pixel layer. Threshold, colour count, and smoothness live in Trace. Object → Trace to vector does the same without switching tools.
- **Zoom** `Z` — drag a box to that area. Click zooms in, Alt-click zooms out a step. Ctrl-click fits the artboard. Ctrl+Shift-click fits the selection, or every object if nothing is selected. Pinch the trackpad to zoom the canvas. Ctrl++ / Ctrl+- / Ctrl+scroll / Alt-scroll also zoom the canvas, not the chrome. With Z selected, two-finger scroll zooms.
- **Hand** `H` / Space — pan.

Colour studio: HSV, hex, swatches, recent. `X` swaps fill/stroke. `D` restores defaults.

Boolean (Object menu): union, subtract, intersect, XOR. Combine `Ctrl+G`, release `Ctrl+Shift+G`. Align and distribute. Bring to front / send to back. Snap to grid, guides, objects. Click a ruler to drop a guide.

**FX** (right studio): SVG filter effects on the active layer — blur, drop/inner shadow, offset, dilate/erode, saturate, hue rotate, brightness, contrast, invert, color matrix, turbulence, displacement. Params are the SVG ones. They rasterise on the canvas and write `<filter>` / `fe*` on SVG export.

## Pixel

Paint lives on a **pixel layer**. Add one from the Layers studio if the document is vector-only.

- Brush `B` — size `[` `]`, hardness `Shift+[` `]`.
- Eraser `E`, Fill `K`, Clone `J` (Alt-click sets source), Smudge `M`.
- Marquee, elliptical marquee, lasso, wand. Tolerance is in Brush.

## Photo

Open a folder, drop files, or load samples. Develop: exposure, contrast, highlights/shadows/whites/blacks, temp/tint, presence, tone curve, split tone, HSL, grain, vignette, rotate, crop. Histogram and before/after. **Place in Design** drops the developed image as a pixel layer.

## Motion

The artboard you drew is the rest pose. Motion does not rewrite it. Tracks are offsets: X, Y, rotation, scale, opacity.

- Open the **Motion** persona. The timeline sits under the canvas.
- Select a shape. Drag it — that writes keys at the playhead. First key at t > 0 also plants rest at 0, so it animates from where you drew it.
- `K` keys X/Y/rotate/scale for the selection. Diamonds on the row are keys. Drag a diamond to retime. Click it, Delete removes it. Cycle ease on a selected key.
- Space plays. Home / End jump. Loop is the repeat icon.
- **File → Export animated SVG…** writes CSS `@keyframes`. **Export Lottie…** writes Bodymovin 5.x JSON that lottie-web and dotLottie play. **Import Lottie…** brings a shape-layer Lottie onto the timeline.

PNG/JPEG/static SVG stay the rest pose. The clip lives in the `.oma`.

## Files

- Project: `.oma` (JSON, rasters PNG-packed, motion clip)
- **File → Place…** `Ctrl+Shift+P` — PNG/JPEG/WebP/TIFF/GIF/BMP or SVG onto the current artboard. Click to drop at native size, drag to size it. Enter places at the centre. Esc cancels.
- Drop a file on the canvas or the welcome screen: images and SVG place, `.oma` opens, Lottie imports.
- Export: PNG (1×/2×/3×), JPEG, SVG, animated SVG, Lottie JSON
- Copy / cut / paste shapes. Copy style `Ctrl+Alt+C`, paste style `Ctrl+Alt+V`.
- Native file dialogs. Right-click the canvas for Place, Trace, and the same edits.

## Keys

```
Move V · Node A · Pen P · Pencil N
Rectangle R · Ellipse O · Polygon Y · Star S · Line L
Type T · Gradient G · Eyedropper I · Trace U · Brush B · Eraser E
Fill K · Clone J · Smudge M · Crop C · Wand W · Hand H · Zoom Z
Undo Ctrl+Z · Redo Ctrl+Shift+Z · Duplicate Ctrl+D
Copy Ctrl+C · Paste Ctrl+V · Cut Ctrl+X · Select all Ctrl+A
Save Ctrl+S · Save as Ctrl+Shift+S · Open Ctrl+O · New Ctrl+N · Place Ctrl+Shift+P · Export Ctrl+E
Combine Ctrl+G · Release Ctrl+Shift+G · Front Ctrl+Shift+] · Back Ctrl+Shift+[
Fit Ctrl+0 · 100% Ctrl+1 · Zoom in Ctrl++ · Zoom out Ctrl+- · Pan Space · Pinch / Ctrl+scroll zoom
Motion: Space play · K key · Home start · End end
```

## Theme and font

omadesign does not ship a brand palette. On launch it reads:

1. `~/.local/state/omarchy/current/theme/colors.toml`
2. `~/.config/omarchy/themes/<current>/colors.toml`
3. stock Omarchy Catppuccin if nothing else is there

UI type is `omarchy font current`, then fontconfig `sans-serif`. Override with `OMADESIGN_FONT=/path/to/font.ttf`.
