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

- **Move** `V` — click to select, drag to move, eight handles scale, the handle above the box rotates. Shift-click adds to the selection. Alt-drag clones. Corner dots round a rectangle.
- **Node** `A` — drag points and Bézier handles. Shift-click adds nodes. Drag a box around nodes to select them. Drag a segment to move the line. Click a curve to insert. Alt-click converts corner/smooth. Alt-drag a handle breaks symmetry. Delete removes selected points. Object → Break path. Shapes convert to a path the first time you edit them.
- **Pen** `P` — click a corner, click-drag a smooth point (a twitch under 3px stays a corner). Shift constrains 45°. Alt-drag breaks handle symmetry. The cubic is drawn as you go. Enter or double-click finishes an **open** path. Esc removes the last point, then cancels. Click the first point to close. Click an open endpoint to continue it, or to join it to the path you're drawing.
- **Artboard** `Shift+O` — draw a new board, drag to move, handles scale, the top handle rotates. Alt-drag clones. Object → Wrap selection in artboard. Click the name in Transform to rename.

Pen / Node parity with Affinity, Illustrator, and Inkscape:

| Gesture | Pen | Node |
|---|---|---|
| Click | Corner | Select (Shift adds) |
| Click-drag | Smooth | Move selected points |
| Alt-drag | Break handle | Break handle |
| Shift | 45° | 45° on handles |
| Esc | Drop last point, then cancel | — |
| Enter / double-click | Finish open | — |
| Click first point | Close | — |
| Click open end | Continue / join | — |
| Box | — | Select those nodes |
| Drag a segment | — | Move the line |
| Click a curve | — | Insert |
| Alt-click a point | — | Corner ↔ smooth |
| Delete | — | Remove selected points |

- **Pencil** `N` — freehand curve.
- **Rectangle** `R` / **Ellipse** `O` / **Polygon** `Y` / **Star** `S` / **Line** `L` — drag. Shift constrains. Corner radius, sides, and inner radius live in Transform.
- **Type** `T` — click to place, type on the canvas. First keystroke replaces the “Type” placeholder. Enter is a new line. Esc or click away finishes. Double-click existing type to edit. Character studio: font, size, tracking, leading, OpenType (kerning, ligatures, tabular figures, small caps).
- **Gradient** `G` — drag across a selected shape.
- **Eyedropper** `I` — sample fill.
- **Trace** `U` — raster to vector on the active pixel layer. Threshold, colour count, and smoothness live in Trace. Object → Trace to vector does the same without switching tools.
- **Zoom** `Z` — drag a box to that area. Click zooms in, Alt-click zooms out a step. Ctrl-click fits the artboard. Ctrl+Shift-click fits the selection, or every object if nothing is selected. Pinch the trackpad to zoom the canvas. Ctrl++ / Ctrl+- / Ctrl+scroll / Alt-scroll also zoom the canvas, not the chrome. With Z selected, two-finger scroll zooms.
- **Hand** `H` / Space — pan.

Colour studio: HSV, hex, swatches, recent. `X` swaps fill/stroke. `D` restores defaults.

**Select** has All, None, Invert, Same Fill / Stroke / Effects, and With / Without Fill / Stroke / Effects. Matching compares the complete property, including gradient positions, stroke settings, or the effect stack. Hidden and locked objects stay out of the selection.

**Object → Pathfinder** offers Union, Subtract, Intersect, XOR, and Divide. Select two or more vector objects on the same layer. Operations follow the layer stacking order; Divide makes separate pieces, with holes preserved. Each operation is one undo step. Combine `Ctrl+G` and Release `Ctrl+Shift+G` remain available.

**Object → Expand stroke to outline** turns the visible stroke into filled geometry, including caps, joins, and dashes. Existing fills stay in place beneath the new outline. Compound outlines retain their holes; use Reshape to move their contours together.

### Guides, rulers, and precision

Drag from the top ruler for a horizontal guide or the left ruler for a vertical one. Drag an existing guide to move it. Select a guide and press Delete, drag it outside the canvas, or use its context menu to remove it. View also offers Clear Guides. `Ctrl+;` shows or hides guides.

Drag the rulers' top-left intersection to set the zero point. Double-click that corner to reset it. Right-click a ruler or use View to choose pixels, millimetres, centimetres, inches, or points. Physical units follow the document DPI; changing units changes the ruler display, not the artwork.

Snapping uses object and artboard edges and centres, guides, the grid, and equal spacing between nearby objects. Alignment lines and gap measurements appear as you move. `Ctrl+Shift+;` toggles snapping; hold Ctrl during the same drag to temporarily reverse that choice, then release it to return. View has individual snapping options.

Hold Shift to constrain pen points and handles, pencil/brush strokes, and object or artboard movement to horizontal, vertical, or 45°. Alt-drag clones an object; combine it with Shift for a constrained copy. During a brush stroke, pressing Shift anchors the constraint at the last free point.

### Reshape

In Design, select vector artwork and choose **Object → Reshape → Distort, Skew, Perspective, or Warp mesh**. Drag the cage handles; the inspector switches modes and finishes the edit. The mesh has nine handles. Shift constrains movement, and Ctrl temporarily reverses snapping. Enter finishes; Esc cancels the current drag, or leaves the mode if no drag is active. Each completed drag is one undo step.

The first moved handle converts live text and parameter-based shapes to paths. Undo restores their original form. Reshape currently supports vector artwork; placed photographs retain the normal move, scale, and rotate tools.

**FX** (right studio): SVG filter effects on the selected object, then the layer underneath — blur, drop/inner shadow, offset, dilate/erode, saturate, hue rotate, brightness, contrast, invert, color matrix, turbulence, displacement. Params are the SVG ones. They rasterise on the canvas and write `<filter>` / `fe*` on SVG export.

Layers expand to show objects. Eye and lock work per object. Click a name to select it on the canvas.

Document tabs sit above the canvas. Ctrl+N is a new tab. Ctrl+O opens another tab. Use the tab's close button or its right-click menu to close it. Unsaved work asks Save / Discard / Cancel.

Idle for a second writes `~/.local/share/omadesign/<id>.oma.swp`. Save deletes it. The splash Recovered tab lists crash leftovers. Recents lists `.oma` files you actually opened.

## Pixel

Paint lives on a **pixel layer**. Add one from the Layers studio if the document is vector-only.

- Brush `B` — size `[` `]`, hardness `Shift+[` `]`.
- Eraser `E`, Fill `K`, Clone `J` (Alt-click sets source), Smudge `M`.
- Healing brush `Shift+J` — Alt-click clean texture on the active image, then paint over a blemish. It blends sampled texture with the destination's local colour and preserves transparency. The source stays fixed for the stroke; Undo restores the whole stroke.
- Marquee, elliptical marquee, lasso, wand. Tolerance is in Brush.

### Masks

Use the layer context menu's **Mask** submenu, or **Add layer mask** in Pixel, to reveal all, hide all, or start from the current pixel selection. Switch between Pixels/Artwork and Mask in the inspector. Black hides; white reveals. The Eraser hides on a mask, and Fill works on the current paint target.

Masks work on pixel and vector layers and remain editable in the project. Invert flips the mask; Remove reveals the untouched layer. Apply to Pixels bakes the result into a raster layer, with one undo restoring both pixels and mask. Placed image masks follow the image's position, scale, and rotation. Choose Pixels before using the healing or clone brush.

## Photo

Open a folder, drop files, or load samples. The Develop panel groups adjustments into **Light**, **Colour**, and **Detail**. Tone curve, colour mixer, and colour grading expand when needed. **Before** compares the original; **Auto light** balances exposure and contrast. Export JPEG runs in the background. **Place in Design** drops the developed image as a pixel layer.

Hold Space or choose Hand to drag the view; middle-drag and two-finger scroll also pan. Pinch, Ctrl+scroll, and Alt+scroll zoom. Ctrl+0 fits the photo; Ctrl+1 shows it at 100%.

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
- **File → Place…** `Ctrl+Shift+P` — PNG/JPEG/WebP/TIFF/GIF/BMP, SVG, PDF, AI, EPS, PSD onto the current artboard. Click to drop at native size, drag to size it. Enter places at the centre. Esc cancels. Placed rasters are selectable objects: move, scale, rotate.
- Drop a file on the canvas or the welcome screen: images and SVG place, `.oma` opens, Lottie imports. Affinity `.afdesign` is not readable — export SVG or PDF from Affinity first.
- Open/Place format matrix: native `.oma` / SVG / PNG / JPEG / WebP / GIF / BMP / TIFF. Converted with poppler / Ghostscript / ImageMagick / Inkscape: PDF, AI (PDF-based), EPS, PSD. Unsupported with a clear error: Affinity packages.
- Export: PNG (1×/2×/3×), JPEG, SVG (cubics stay cubics, rasters keep their box, type writes `<text>`), animated SVG, Lottie JSON
- Copy / cut / paste objects. Status bar says so. Copy style `Ctrl+Alt+C`, paste style `Ctrl+Alt+V`. Alt-drag clones.
- Native file dialogs. Right-click the canvas for Place, Trace, and the same edits.

## Keys

```
Move V · Node A · Pen P · Pencil N
Rectangle R · Ellipse O · Polygon Y · Star S · Line L
Type T · Gradient G · Eyedropper I · Trace U · Brush B · Eraser E
Fill K · Clone J · Heal Shift+J · Smudge M · Crop C · Wand W · Hand H · Zoom Z
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
