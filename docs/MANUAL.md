# omadesign user manual

A native Linux studio. Design, paint, retouch and animate. One document, one layer stack.

Familiar shortcuts, with a contextual guide. Press **F1** any time.

## Learn as you draw

The **Shortcut HUD** sits along the bottom of the window. Its upper row follows
the current tool or edit; the lower row shows letter keys. Hold Ctrl, Shift, Alt,
or a combination to see the matching commands and highlighted gestures. Release
the modifier and the normal hints return. With Pen, for example, the strip keeps
angle constraints, handle controls and finishing the path close at hand.

It keeps the same height while modifiers change, so a drag stays anchored to the
same canvas. Hover **+ more** to inspect overflow hints at smaller window sizes.
Hints are informational and do not take keyboard focus from your work. Text
editing and menus get their own context. **Ctrl+/** or **View → Shortcut HUD**
shows or hides the strip; **F1** opens the complete shortcut list.

## Install

```sh
curl -fsSL https://omadesign.app/install | sh
```

That installs `~/.local/bin/omadesign` and a desktop entry under your home directory. Nothing is written to `/usr`. The same line works on immutable systems (Silverblue, Bazzite, NixOS, SteamOS desktop) as well as Omarchy. If `omadesign` is not on PATH, run `~/.local/bin/omadesign` or add `~/.local/bin` to PATH. Binaries are glibc 2.35, so they run on Asahi Omarchy, Ubuntu 22.04+, and current Arch.

## First five minutes

1. Launch **omadesign**.
2. Choose **Vector** or **Layout** for templates, their file icons for blank size setup, or click an existing document thumbnail.
3. **Design** is the default persona. `R` a rectangle, `P` the pen, `T` type.
4. **Pixel** (`B`) paints on a raster layer.
5. **Photo** opens a folder of pictures and grades them.

Chrome follows your desktop: Omarchy theme colors and the font from `omarchy font current` / fontconfig. Icons are Phosphor Light.

## Welcome: your work and projects

The welcome screen is a local file browser with creation actions in the center.
**Your Work → recent** discovers every `.oma` beneath your home directory at any
depth, sorted by last modification, newest first. **Recovered** lists recovery
snapshots separately. Discovery skips hidden directories, Trash and symlinks.
Use **Refresh** after moving or adding work outside the app.

Both file browsers show masonry thumbnails at their natural aspect ratio, with
at most three columns each. Names appear on hover. Click a document to open it.
**Shift-click** the first item to begin selecting several; subsequent clicks
add or remove selections. Choose **Open selected** (or **Recover selected**) to
open them, and **Clear** or Escape to leave selection. The funnel filters by
Vector, Raster, Layout, Photo or Motion; **All modes** clears the filter.

**Projects → recent** finds directories containing `.omabrand` anywhere beneath
home. A project needs no account. Click a folder card to browse its subprojects
first, followed by every descendant `.oma` sorted by modification time. Folder
cards contain stacked asset previews. Enter a subproject to browse deeper;
**← Projects** returns to the project list. **Edit brand…** opens the current
project's brand editor. **Team** appears only while signed into cloud and shared
team projects are available.

| Center action | What opens |
| --- | --- |
| **+ Vector** | 52 editable vector templates |
| Vector file icon | Blank document size chooser |
| **+ Raster** | Blank raster size chooser |
| **+ Layout** | Layout starters with editable frames and prototypes |
| Layout file icon | Blank Layout size chooser |
| **+ Photo** | Photo workspace |
| Photo folder icon | Folder chooser |
| Photo image icon | Image chooser |
| **+ Project** | Project brand editor |

Vector and Raster on this screen lead to the Design and Pixel editing personas.
Motion starts from the editor after there are canvas elements to animate, so it
has no empty-workspace creation button. Startup and remembered-mode preferences
remain in **omadesign → Config**.

### Learn and create with your agent

**Learn with AI** opens a free-form question box. **Create with agent** opens a
brief box and uses the project you are browsing as the working directory when
one is selected. Both hand the prompt to Omarchy's configured default agent.
If none is installed and selected, follow the instructions shown to choose one
in **Omarchy → Setup → Default → Agent**. Omadesign does not choose one for you.

Learning prompts include the [Markdown documentation index](https://omadesign.app/llms.txt)
and instructions to fetch relevant pages. Creation prompts point to the bundled
[Omadesign creation skill](https://omadesign.app/skills/omadesign-create/SKILL.md),
which describes editable documents, project brand assets and native preview
verification. Version-matched help is also available offline:

```sh
omadesign --agent-docs manual
omadesign --agent-docs layout
omadesign --agent-docs formats
omadesign --agent-skill
```

The welcome screen uses the current Omarchy palette and desktop font; it has no
separate app light/dark switch. **Join the conversation** opens the
[Omadesign Discord](https://discord.gg/ejkZS2RBx). **Sign up for cloud** opens
[cloud registration](https://omadesign.app/cloud); local editing and project
browsing remain available without an account. Release notes, docs, contribution
guidance and bug reports are linked from the same column. An available update
opens the existing update details before the explicit install-and-restart action.

## Personas

| Persona | You are… | First tools |
|---|---|---|
| **Design** | a mark, a poster, a layout | Move `V`, Pen `P`, Rectangle `R`, Type `T` |
| **Layout** | a screen, a landing, a dashboard | Frame `F`, Rectangle `R`, Type `T` |
| **Pixel**  | painting or retouching | Brush `B`, Eraser `E`, Clone `J`, Wand `W` |
| **Photo**  | grading a photograph | Crop `C`, develop sliders, Place in Design |
| **Motion** | animating the artboard | Space play, `K` key, File → Lottie |

## Layout

Frames for UI mockups, in the same document as the drawing.

- **Frame** `F` — drag a frame. Draw another frame inside it and it nests. Object → Wrap selection in frame. Image placeholders live in the inspector.
- **Auto-layout** — select a frame, turn on **Stack children**. Vertical or horizontal, with gap, padding and stretch. Children pack in layer order.
- **Constraints** — a child of a frame can pin to min, max, both edges, center, or scale when you resize the parent.
- **Export** — File → Export frame PNG / SVG / HTML for the selected frame.
- **Templates** — choose **+ Layout** on the welcome screen: Fieldwork responsive prototype, mobile screen, landing hero, dashboard and card stack. These also remain under **File → Template library → Layout starters**.
- **Comments** — write a note, pin it on the canvas, resolve it. The inspector shows open counts on the frame.

Cloud is opt-in. File → Sign in opens a secure browser approval. Push project + review export uploads a versioned design and flat snapshot; Cloud projects pulls shared designs into a new document. Review annotations loads cloud feedback. Publishing and competition entry are separate owner actions. See [cloud](cloud.md).

## Design

- **Move** `V` — click to select, drag to move, eight handles scale, the handle above the box rotates. Shift-click adds or removes an object; Shift-drag a selection box to add objects. Dragging a selected object moves the whole selection, and Shift constrains that movement. Alt-drag clones. Corner dots round a rectangle.

Select a layer row to reorder it with **Ctrl+[ / Ctrl+]**; add **Shift** to send it to the back or front of its group. Drag a layer name onto an insertion line to reorder it. Groups move with their children. Clicking an object on the canvas returns these shortcuts to object stacking. Each reorder can be undone in one step.

- **Free transform** `Ctrl+T` — puts the current selection into Move with its scale and rotation handles ready. Also under Object. It keeps live text and shape parameters editable.
- **Node** `A` — drag points and Bézier handles. Shift-click adds nodes. Drag a box around nodes to select them. Drag a segment to move the line. Click a curve to insert. Alt-click converts corner/smooth. Alt-drag a handle breaks symmetry. Delete removes selected points. Object → Break path. Shapes convert to a path the first time you edit them.
- **Flip** — right-click artwork or its object row and choose **Flip horizontal** (left/right) or **Flip vertical** (top/bottom). The same controls live under Object and in the inspector. Flips follow the visible canvas axes even after rotation, and Undo restores them. Dashed rectangles and ellipses become paths so their dash placement mirrors too; Undo restores their shape parameters. For live text, explicitly choose **Object → Convert to path** first; this preserves letter outlines and holes but replaces editable text. Undo restores the text.

Rotated paths keep their visible points and Bézier handles aligned with the artwork. Point, handle and segment edits work at those displayed locations; existing saved rotations remain intact. Selecting a path does not create an undo step.

- **Pen** `P` — click a corner, click-drag a smooth point (a twitch under 3px stays a corner). Shift constrains 45°. Alt-drag breaks handle symmetry. The cubic is drawn as you go. Enter or double-click finishes an **open** path. Esc removes the last point, then cancels. Click the first point to close. Click an open endpoint to continue it, or to join it to the path you're drawing.
- **Artboard** `Shift+O` — draw a new board, drag to move, handles scale, the top handle rotates. Alt-drag clones. Object → Wrap selection in artboard. Click the name in Transform to rename.

Pen and Node gestures:

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
- **Gradient** `G` — drag across a selected shape to place its gradient. The Appearance studio's active **Fill** or **Stroke** row chooses which paint the tool edits; existing stops and gradient type are retained. Endpoints remain visible while the Gradient tool is selected.

Appearance offers **Solid**, **Linear**, **Radial**, **Shape**, and **Conic** paint for both fills and strokes. Click the gradient ramp to insert a stop, drag its handle to move it, and select a stop to edit its color, alpha, and percentage. **+ / −** add and remove stops; **Reverse** flips the stop order, and **Even** distributes them equally. **Angle** rotates linear, radial, and conic gradients. Radial gradients spread outward from the drag start, conic gradients sweep around it, and Shape gradients follow the actual silhouette and holes from interior to boundary. Linear/radial SVG exports keep editable gradient stops; Shape/conic SVG exports use embedded image patterns (up to 4096 pixels per axis). Project files keep all four types editable.
- **Eyedropper** `I` — sample fill.
- **Trace** `U` — raster to vector on the active pixel layer. Threshold, color count, and smoothness live in Trace. Object → Trace to vector does the same without switching tools.
- **Zoom** `Z` — drag a box to that area. Click zooms in, Alt-click zooms out a step. Ctrl-click fits the artboard. Ctrl+Shift-click fits the selection, or every object if nothing is selected. Pinch the trackpad to zoom the canvas. Ctrl++ / Ctrl+- / Ctrl+scroll / Alt-scroll also zoom the canvas, not the chrome. With Z selected, two-finger scroll zooms.
- **Hand** `H` / Space — pan.

Color studio: saturation × brightness, hue, alpha, hex, swatches, and recent colors. Open a color chip to see **Current** and **Previous** side by side; click Previous to restore the color from before this edit. Every picker accepts alpha, including gradient stops, paint, Layout color variables, and Design effect colors. Eight-digit hex uses `#RRGGBBAA`. `X` swaps fill/stroke. `D` restores defaults.

Every vector object and Layout frame has **Opacity** and **Blend** controls in its transform inspector. Placed images use the layer's opacity and blend controls above the Layers tree. A frame's opacity/blend applies to its complete subtree, and an object's opacity applies once to its combined fill, image, and stroke. Layer opacity also applies once to its complete contents. Choose from Normal, Multiply, Screen, Overlay, Darken, Lighten, Color Dodge/Burn, Hard/Soft Light, Difference, Exclusion, Hue, Saturation, Color, and Luminosity. These settings survive project saves and SVG export. Regular layers and Layout frames isolate their contents: child blend modes interact with siblings inside the same container, while the container’s own blend mode interacts with artwork below it. This remains consistent at every opacity. Explicit layer groups can enable **Pass through** for child layers to blend with the outside backdrop.

**Select** has All, None, Invert, Same Fill / Stroke / Effects, and With / Without Fill / Stroke / Effects. Matching compares the complete property, including gradient positions, stroke settings, or the effect stack. Hidden and locked objects stay out of the selection.

**Object → Pathfinder** offers Union, Subtract, Intersect, XOR, and Divide. Select two or more vector objects on the same layer. Operations follow the layer stacking order; Divide makes separate pieces, with holes preserved. Each operation is one undo step. Compound shape `Ctrl+8` and Release compound `Ctrl+Shift+8` remain available. Group `Ctrl+G` creates an editable layer group; Ungroup `Ctrl+Shift+G` releases it without combining paths. Drag objects or layers between sidebar rows to reorder, and onto the centre of a group or Layout frame to nest. Hold Shift while clicking object rows to select several. Dragging a placed image layer onto a Layout frame converts it to an image fill, retaining position, rotation, opacity, blend mode, and effects. An existing layer mask is baked into the image alpha; Undo restores the original image layer and its editable mask.

**Object → Expand stroke to outline** turns the visible stroke into filled geometry, including caps, joins, and dashes. Existing fills stay in place beneath the new outline. Compound outlines retain their holes; use Reshape to move their contours together.

Combine and Release preserve guide state, rotation, stacking and gradient endpoints
in one undo step. Combine and Pathfinder require either artwork or guides, with
no mixture. Shape gradients follow the resulting silhouette. Older two-color radial
fills follow the resulting bounds; editing them in Appearance upgrades them to the
multi-stop format.

### Guides, rulers, and precision

Drag from the top ruler for a horizontal guide or the left ruler for a vertical one. Drag an existing guide to move it. Select a guide and press Delete, drag it outside the canvas, or use its context menu to remove it. View also offers Clear ruler guides. `Ctrl+;` shows or hides ruler and object guides.

**Object → Guides → Convert selection to guides** turns vector artwork into editable, non-printing contours. Curves, compound paths, shapes and live text keep their original data and style. Move, Node and Reshape still edit them; snapping follows the actual curve. **Release guides** restores them as artwork, including any geometry edits. Both actions undo normally, and guides survive project saves while staying out of PNG/JPEG/SVG/Lottie exports. A placed image creates a separate guide around its bounds and retains its pixels. Hidden guides do not capture pointer input or snapping.

Drag the rulers' top-left intersection to set the zero point. Double-click that corner to reset it. Right-click a ruler or use View to choose pixels, millimeters, centimeters, inches, or points. Physical units follow the document DPI; changing units changes the ruler display, not the artwork.

Snapping uses object and artboard edges and centers, guides, the grid, and equal spacing between nearby objects. Alignment lines and gap measurements appear as you move. `Ctrl+Shift+;` toggles snapping; hold Ctrl during the same drag to temporarily reverse that choice, then release it to return. View has individual snapping options.

Hold Shift to constrain pen points and handles, pencil/brush strokes, and object or artboard movement to horizontal, vertical, or 45°. Alt-drag clones an object; combine it with Shift for a constrained copy. During a brush stroke, pressing Shift anchors the constraint at the last free point.

### Reshape

In Design, select vector artwork and choose **Object → Reshape → Distort, Skew, Perspective, or Warp mesh**. Drag the cage handles; the inspector switches modes and finishes the edit. The mesh has nine handles. Shift constrains movement, and Ctrl temporarily reverses snapping. Enter finishes; Esc cancels the current drag, or leaves the mode if no drag is active. Each completed drag is one undo step.

The first moved handle converts live text and parameter-based shapes to paths. Undo restores their original form. Reshape currently supports vector artwork; placed photographs retain the normal move, scale, and rotate tools.

**FX** (right studio): SVG filter effects on the selected object, then the layer underneath — blur, drop/inner shadow, offset, dilate/erode, saturate, hue rotate, brightness, contrast, invert, color matrix, turbulence, displacement. Params are the SVG ones. They rasterize on the canvas and write `<filter>` / `fe*` on SVG export.

Layers expand to show objects. Eye and lock work per object. Click a name to select it on the canvas.

Document tabs sit above the canvas. Ctrl+N is a new tab. Ctrl+O opens another tab. Use the tab's close button or its right-click menu to close it. Unsaved work asks Save / Discard / Cancel.

Idle for a second writes `~/.local/share/omadesign/<id>.oma.swp`. Save deletes it. The splash Recovered tab lists crash leftovers. Recents lists `.oma` files you actually opened.

## Pixel

Paint lives on a **pixel layer**. Add one from the Layers studio if the document is vector-only.

- Brush `B` — size `[` `]`, hardness `Shift+[` `]`.
- Eraser `E`, Fill `K`, Clone `J` (Alt-click sets source), Smudge `M`.
- Healing brush `Shift+J` — Alt-click clean texture on the active image, then paint over a blemish. It blends sampled texture with the destination's local color and preserves transparency. The source stays fixed for the stroke; Undo restores the whole stroke.
- Marquee, elliptical marquee, lasso, wand. Drag to select; the marching ants stay until Esc. Shift adds. Delete clears selected pixels. Paint, fill, clone, heal and smudge stay inside the selection. Wand tolerance is in Brush. Eyedropper `I` shows the sampled color in the sidebar.

### Raster filters and effects

Select an unlocked pixel layer in **Pixel**, then open **Raster studio → Filters** or **Effects**. Choose **Mask** first to process the layer mask instead of the image. A marquee, lasso, or wand selection limits the edit; partial selection coverage and the **Strength** slider blend the result with the source.

The dialog previews the result on a transparency checkerboard. **Original** compares the source. Change effects or settings without committing; **Cancel** leaves the document untouched. **Apply** processes the full-resolution image or mask in the background and creates one Undo/Redo step. Applied pixels and masks persist in `.oma` projects and exports. These are applied raster edits, not a live effect stack; duplicate the layer before applying if you want an independent original. Preview spatial distances scale to the preview resolution; fine detail can differ from full-resolution output.

**Filters** includes:

- **Key & transparency:** Chroma key, Alpha threshold, Feather alpha, Grow alpha, Shrink alpha.
- **Color & tone:** Brightness / contrast, Exposure / gamma, Levels, Hue / saturation, Vibrance, Color balance, Temperature / tint, Grayscale, Sepia, Invert, Threshold, Posterize.

**Effects** includes:

- **Blur:** Gaussian blur, Motion blur with angle.
- **Detail:** Sharpen, Unsharp mask, Find edges, Emboss.
- **Stylize:** Pixelate, Film grain, Vignette, Halftone, Solarize.
- **Distort:** Swirl, Ripple.

For green-screen or blue-screen removal, choose **Chroma key…**, choose Green / Blue or **Sample image**, then click the background in the preview. The color picker also accepts a custom key color. **Similarity** sets how much color is removed; **Falloff** gives partial transparency near the boundary; **Hardness** tightens that transition; **Spill cleanup** removes excess key color near the keyed edges. **Flow** controls how much of the key is applied in this pass by blending the original and keyed result. Other filters call this control **Strength**. The key uses RGB chroma; the key color chip's alpha is not a keying parameter. Existing source transparency is preserved. For local refinement after applying, add or select a layer mask and paint with the brush's **Edge**, **Flow**, and **Opacity** controls.

### Masks

Use the layer context menu's **Mask** submenu, or **Add layer mask** in Pixel, to reveal all, hide all, or start from the current pixel selection. Switch between Pixels/Artwork and Mask in the inspector. Black hides; white reveals. The Eraser hides on a mask, and Fill works on the current paint target.

Masks work on pixel and vector layers and remain editable in the project. Invert flips the mask; Remove reveals the untouched layer. Apply to Pixels bakes the result into a raster layer, with one undo restoring both pixels and mask. Placed image masks follow the image's position, scale, and rotation. Choose Pixels before using the healing or clone brush.

## Photo

Open a photo, browse a folder, drop files, or load samples. Camera RAW files such as DNG, CR2/CR3, NEF, ARW, RAF, ORF and RW2 use the built-in decoder. The Photo library shows camera metadata when available, and imports run in the background. The Develop panel groups adjustments into **Light**, **Color**, and **Detail**. Tone curve, color mixer, and color grading expand when needed. **Before** compares the default development; **Auto light** balances exposure and contrast. RAW exposure and white-balance changes use the 16-bit linear source.

Use **Save settings** to keep development adjustments beside the original photo as a small `.omaphoto` file. Resume through **File → Open**, a drop, or **Photo → Library → ··· → Open photo or settings…**. Choose the original image or its `.omaphoto` file; both restore the original pixels and saved adjustments. Keep the pair together with their matching names, such as `photo.png` and `photo.png.omaphoto`; settings do not contain the image. The original photograph is never rewritten. Opening settings explicitly reports a missing or changed original, or invalid settings, without replacing the current photo. Opening the original with unusable settings shows default development and a note. Failed saves keep your edits available to retry, and edits made while a save finishes remain marked unsaved. Photo edits have their own Undo/Redo history. Quitting offers Save all, Discard or Cancel for unsaved photo settings before the palette and artwork save steps, and waits for writes to finish. Saving a Design `.oma` does not store the RAW source or its settings.

Export JPEG, PNG or TIFF in the background at the full developed resolution, including crop and rotation. RAW PNG/TIFF exports keep 16-bit channels. **Place in Design** adds an 8-bit developed pixel layer with Undo; retain the RAW and settings for later development. The initial display preview has a maximum edge of 1600 pixels. Zoom in for full-resolution detail, prepared in the background and displayed as visible tiles while the preview keeps the view responsive. See [RAW format limits](format-support.md#camera-raw) for supported camera families and color-rendering differences.

### Copy a look across photos

1. Develop the source photo, then choose **Copy adjustments** or press **Ctrl+Shift+C**.
2. Select target photos in the Library. **Ctrl-click** toggles individual photos, **Shift-click** selects a range, and **Ctrl+A** selects all loaded photos. The active photo remains the one shown in the viewer; the selection count tells you how many photos will receive the look.
3. Choose **Paste adjustments** or press **Ctrl+Shift+V**. Select the adjustment categories and apply. Light, color, detail, tone curve, color mixer and color grading can travel independently. **Crop and rotation are off by default**, so each photo keeps its framing.
4. **Ctrl+S** saves the selected photos' settings beside their originals. The batch is one Undo/Redo operation; ordinary slider edits also use the Photo history.

Copying captures the source settings at that moment. Later source edits do not change the copied look. Applying the same values again makes no additional undo entry. Samples and pasted images can receive adjustments but need an original on disk before settings can be saved.

### Presets and whole folders

The Photo preset library saves named looks, filters them by name, and imports or exports **`.omapreset`** files. Presets contain development values and the chosen categories, so they work across unrelated originals. Their library persists between sessions. Imported name conflicts retain both looks with distinct names.

Choose **Library → … → Browse folder…**, open a representative photo from that folder, and copy its adjustments or choose **Presets… → Use preset…**. In **Apply adjustments**, choose **Whole folder**, then **Write settings for N photos**. It processes supported photo filenames directly in that folder, writes each original's **`.omaphoto`** settings in the background, and leaves image pixels untouched. It does not load the whole shoot into memory or recursively scan subfolders. Existing adjustments in excluded categories are retained. Progress and file-specific errors remain visible; cancellation stops remaining work, and Undo restores completed changes. A file changed outside the batch is preserved and reported instead of overwritten during Undo/Redo.

Folder jobs check that undo data fits before writing. The limits are 10,000 photos and 32 MiB of settings history per job; larger jobs need smaller folders. Invalid or mismatched existing settings are reported. Filename recognition does not guarantee that every camera file can be decoded; open a representative RAW photo first. Folder changes are already saved on disk, while pasting to a loaded selection requires Save settings. Export still processes the active photo.

Hold Space or choose Hand to drag the view; middle-drag and two-finger scroll also pan. Pinch, Ctrl+scroll, and Alt+scroll zoom. Ctrl+0 fits the photo; Ctrl+1 shows it at 100%.

## Motion

The artboard you drew is the rest pose. Motion does not rewrite it. Tracks include X, Y, rotation, scale, opacity, stroke reveal and fill reveal.

- Open the **Motion** persona. The timeline sits under the canvas.
- Select vector artwork and choose **Draw stroke, Pop in, Slam, Shake, Fill up, Slide up/down/left/right, Fly, Zoom, Buzz, or Fade in** in the inspector. Draw stroke needs a visible stroke; Fill up needs a closed shape with a fill. Incompatible, locked, hidden and guide objects are skipped.
- Duration sits above the presets. **Timing & energy** opens delay, stagger, intensity and start-at-playhead options. Presets become ordinary keys; each application has its own Undo. They replace only the affected channels inside their time interval and extend the clip if needed. Space previews the result.
- Select a shape. Drag it — that writes keys at the playhead. First key at t > 0 also plants rest at 0, so it animates from where you drew it.
- `K` keys X/Y/rotate/scale for the selection. Diamonds on the row are keys. Drag a diamond to retime. Click a diamond, Delete removes that key. Click the object name on the timeline, or leave the keys unselected, and Delete removes the animation from the selected artwork. The drawing stays. Delete again to remove the object itself. Cycle ease on a selected key.
- Space plays. Home / End jump. Loop is the repeat icon.
- **File → Export animated SVG…** writes animated transforms plus stroke/fill reveals, retaining masks and effects. **Export Lottie…** writes Bodymovin 5.x shape animation with trim paths and fill masks. Pixel layers, layer masks and effects cannot be preserved by this Lottie exporter and produce a clear error; choose animated SVG for those compositions. **Import Lottie…** brings a shape-layer Lottie onto the timeline.

PNG/JPEG/static SVG stay the rest pose. The clip lives in the `.oma`.

Animated SVG outlines text in the exported file so glyph geometry and reveals
match the canvas. The source text stays editable in `.oma`. Lottie import is a
basic shape subset; use `.oma` to retain the complete editable animation.

## Templates

Open **+ Vector** on the welcome screen or **File → Template library** while drawing. **+ Layout** opens its separate frame-based starters; Raster opens a size chooser. Search by name or idea, filter the nine categories, choose any built-in document size or enter custom width, height and DPI. Previews adapt to the chosen proportions. Click a card and **Use this template**, or double-click the card.

Templates open as unsaved documents with editable paper, artwork and copy layers. Existing work stays in its tab. They use locally available fonts and need no network connection. The 52 designs include distinct artwork and layouts for portrait, square and landscape pages; very tiny sizes omit unreadable secondary copy.

All 52 are available immediately. The [weekly drop plan](template-drops.md) proposes a release order and a remix prompt for each week; it does not schedule or publish marketing posts.

## Palettes and brand libraries

The right sidebar has three tabs: **Inspect** for the selected artwork, **Palettes**
for reusable colors, and **Brand** for logos, images, fonts and other assets.

Project libraries live beside your work: `.omacolors` holds the palettes and
`.omatype` names the font roles, and `.omabrand/` holds the assets and font files.
A saved document uses the nearest enclosing folder containing any of these.
If none exists, it starts beside the document. For an
unsaved document, use **Choose a project** to select a folder. The folder button
also lets you switch libraries explicitly.

### Build a palette

1. Open **Palettes** and choose **Personal** for colors available across your work,
   or **Project** for colors stored in the current project folder.
2. Click **+ Palette**, enter a name and click **Rename**. Filter the collection by
   palette name or hex color.
3. Add **+ Current color**, collect fill and stroke colors **From selection**, or
   type a hex value and click **+**. `#RRGGBBAA` includes transparency.
4. Choose **Fill** or **Stroke**, then click a swatch to apply it. Right-clicking a
   swatch applies the stroke directly. Each swatch's **···** menu can replace it
   with the current color, copy its hex value or remove it.
5. Click the palette **Save** button to keep the collection. Palette edits have
   their own save state, separate from saving the artwork.

The collection's **··· → Load palettes…** adds palettes from a file; it keeps
existing colors and gives conflicting names numbered suffixes. Save afterwards
to keep the import. **Export selected palette…** shares one palette;
**Export collection…** shares them all. Duplicate and remove controls are also
available.

Libraries refresh in the background about every three seconds. If the file changes
while you have unsaved palette edits, those edits stay in the panel and Save is
blocked. Export a copy to keep your version, or choose **Reload saved colors** to
discard your palette edits and load the file on disk.

Quitting with unsaved palettes offers **Save all**, **Discard** or **Cancel**.
The app waits for library saves to finish and keeps you in the app if a save
fails or a file conflicts. Any unsaved artwork gets its own save prompt afterwards.

### Build a brand bank

Open **Brand → Load bank…** and choose a project folder or its `.omabrand` folder.
For a new collection, choose a project folder and click **Create bank**. Edit the
brand name and click **Save** to name it.

Use **··· → Add assets…** to copy artwork into the bank; the originals stay in
place. Nested folders are supported. Filter by name, folder path or file type,
or use **Image**, **SVG** and **omadesign** to narrow the tiles. Thumbnails load in
the background, and files added or changed outside the app refresh about every
three seconds. **··· → Refresh now** checks immediately.

Drag a tile onto the canvas to place a copy at the drop point, or double-click it
to place it at the selected artboard's center. With no artboard selected, it uses
the document center. Placement is undoable with **Ctrl+Z**. In Photo, double-click
an asset to place it in Design; drag placement is available on artboards.

Banks accept PNG, JPEG, WebP, TIFF, BMP, GIF, SVG and `.oma` artwork. SVG uses the
app's existing supported import subset; complex SVG features may not carry over.
Use **··· → Save bank copy…** to copy the whole bank, including its name and nested
folders, to another project folder.

### Add brand typography

Open **Brand → Typography** and choose **Add fonts…** to copy TTF or OTF files into
the project. Name the kit and click **Save name**. Select a font row, give its role
a useful name such as Heading, Body or Caption, and click **Save role**. Either save
button keeps both pending name and role edits. Filter by role, family or filename.

Click **Apply** beside a role to use it on selected text, or on the next text you
create. The Character panel's font picker also lists **Project fonts**. Applying a
font to artwork supports Undo; the kit's names and files have their own save controls.
Fonts are available inside omadesign without installing them on your computer.

The Typography **··· → Load kit…** action merges another `.omatype` kit and copies its font files; keep its
`.omabrand/` folder beside the source file. **··· → Save copy…** writes the saved kit and
its fonts into another project folder. Save pending name edits first. Removing a
role keeps its font file for artwork that already uses it.

External font changes refresh in the background. Existing text keeps its applied
face; click **Apply** again to adopt a changed font. If the kit changes while you
are editing a name, the panel preserves your draft and reports the conflict.
Typography **··· → Reload typography** discards the draft and loads the saved kit.

Native `.oma` text remains editable after moving the project, including typing
new characters. Saving artwork into another folder also copies the font faces it
uses into that folder's `.omabrand/fonts/`. SVG export draws project-font text as
vector outlines so its appearance survives sharing; the `.oma` source keeps the
editable text.

### Share a project kit

Copy `.omacolors`, `.omatype` and the complete `.omabrand/` folder with your project
to another machine. **Save bank copy…** copies the assets, typography kit and fonts;
export the palette collection separately as `.omacolors` in the destination folder.
These names
begin with a dot, so enable hidden files in your file manager when copying by hand.
Try the portable [Fieldwork example](../examples/fieldwork).

The `.omacolors` file is readable JSON. One file can hold several named palettes:

```json
{
  "version": 1,
  "palettes": [
    { "name": "Fieldwork", "colors": ["#173F35", "#F5EBDC", "#D97C5B80"] },
    { "name": "Ink", "colors": ["#202420", "#FFFFFF"] }
  ]
}
```

Handwritten files may also contain a single `{ "name": "Ink", "colors": [...] }`
palette or a simple array such as `["#202420", "#FFFFFF"]`. Older saved palettes
with RGBA objects still load and become the portable format when saved.

The optional `.omabrand/brand.json` file sets the bank's display name:

```json
{ "version": 1, "name": "Fieldwork" }
```

Without that file, the project folder supplies the display name. Keep the artwork inside
`.omabrand/`; no absolute paths are needed in the name file.

The optional `.omatype` file names the project's font roles. Paths are relative
to `.omabrand/`, and font files stay inside its `fonts/` folder:

```json
{
  "version": 1,
  "name": "Fieldwork typography",
  "roles": [
    { "name": "Heading", "font": "fonts/Display.ttf" },
    { "name": "Body", "font": "fonts/Reading.otf" }
  ]
}
```

Replace those example filenames with your own fonts. Fonts added through the panel
receive stable filenames automatically. Share fonts only under their license terms.

## Files

- Project: `.oma` (JSON, rasters PNG-packed, motion clip)
- **File → Open** reads layered PSD/PSB, GIMP `.xcf`, every page of PDF and PDF-compatible AI, OpenRaster, SVG/SVGZ, and supported Affinity documents through the optional bridge. Imported documents open in their own tab at their original dimensions. **Save** uses `.oma` and preserves the source file. GIMP text and effects import as pixels; export OpenRaster or PSD back to GIMP.
- Camera RAW files open in Photo for development. **Save settings** creates `.omaphoto` alongside the source; Photo export creates a separate JPEG, PNG or TIFF. The original RAW is not changed.
- **File → Place…** `Ctrl+Shift+P` loads artwork in the background, then lets you click or drag to place it. Nested layers and masks travel together, and Undo removes the placement in one step. Enter places at the center; Esc cancels.
- Drop layered documents on the canvas or welcome screen to open them; ordinary images place, `.oma` opens, and Lottie imports.
- Groups in the layer tree expand, rename, hide, lock and reorder as units. **Pass through** controls whether child blend modes interact with the backdrop. Disable it for isolated group blending.
- **View → Document conversion notes** lists unsupported or converted features. Notes also stay in `.oma` projects. Affinity native features, Photoshop live text/smart objects/effects, and Illustrator private editing data are not universally supported. See the [format support matrix](format-support.md) and [Affinity setup](affinity-import.md).
- Export: PNG (1×/2×/3×), JPEG, SVG, animated SVG, Lottie JSON, layered PSD/PSB, PDF and OpenRaster. Layers unsupported by an export format may become individual pixel layers; conversion notes describe those changes. Native `.af*` and `.ai` writers are not available.
- Copy / cut / paste objects. Objects copied in Omadesign paste at their original positions, including when pasted onto another artboard. Status bar says so. Copy style `Ctrl+Alt+C`, paste style `Ctrl+Alt+V`. Alt-drag clones.
- **Paste** `Ctrl+V` also accepts screenshots, images copied from a browser, copied image files, plain text, and SVG source or files from other applications. External content appears in the center of the visible canvas: images become pixel layers, text becomes an editable text layer, and SVG becomes vector artwork. Command+V works through Omarchy’s universal paste binding; Shift+Insert from the Alt+V clipboard-history picker is also supported. Pasting while editing text inserts into that text instead.
- Native file dialogs. Right-click the canvas for Place, Trace, and the same edits.

## Keys

```
Move V · Node A · Pen P · Pencil N
Rectangle R · Ellipse O · Polygon Y · Star S · Line L
Type T · Gradient G · Eyedropper I · Trace U · Brush B · Eraser E
Fill K · Clone J · Heal Shift+J · Smudge M · Crop C · Wand W · Hand H · Zoom Z
Undo Ctrl+Z · Redo Ctrl+Shift+Z · Duplicate Super+D · Pixel selection: deselect Ctrl+D
Copy Ctrl+C · Paste Ctrl+V · Cut Ctrl+X · Select all Ctrl+A
Save Ctrl+S · Save as Ctrl+Shift+S · Open Ctrl+O · New Ctrl+N · Place Ctrl+Shift+P · Export Ctrl+E
Group Ctrl+G · Ungroup Ctrl+Shift+G · Compound Ctrl+8 · Release Ctrl+Shift+8 · Front Ctrl+Shift+] · Back Ctrl+Shift+[
Free transform Ctrl+T · Guides Ctrl+; · Snapping Ctrl+Shift+; · Hold Ctrl to reverse snapping
Shortcut HUD Ctrl+/ · All shortcuts F1
Fit Ctrl+0 · 100% Ctrl+1 · Zoom in Ctrl++ · Zoom out Ctrl+- · Pan Space · Pinch / Ctrl+scroll zoom
Motion: Space play · K key · Home start · End end · Delete removes a selected key, or the animation if no key is selected
```

## Theme and font

The app chrome follows your desktop theme. On launch it reads:

1. `~/.local/state/omarchy/current/theme/colors.toml`
2. `~/.config/omarchy/themes/<current>/colors.toml`
3. stock Omarchy Catppuccin if nothing else is there

UI type is `omarchy font current`, then fontconfig `sans-serif`. Override with `OMADESIGN_FONT=/path/to/font.ttf`.


## App settings, updates and privacy

Click **omadesign** in the title bar to open **Config | Update | About | Docs**. Config controls the UI font and size, startup mode and welcome tab, rulers, shortcut hints, guide locking, photo provider keys and optional anonymous usage. Preferences are saved under `~/.config/omadesign` (or `XDG_CONFIG_HOME`). Docs opens [omadesign.app/docs](https://omadesign.app/docs/).

About displays the refined logo, branded version and full semantic version. Update checks the current release channel and offers installation when a newer compatible package exists. It runs the official installer, waits for ongoing work, writes a complete local recovery snapshot, and restarts with the same open documents and photo adjustments. Anonymous usage is off by default and has its own toggle; [privacy details](privacy.md) describe the exact data and limits.

Guides start locked. The ruler context menu and **Object → Guides** provide lock, unlock and clear-all actions. Hold **Alt** to draw shapes from their center and **Shift** for equal proportions. Ellipses convert to four smooth anchors. Node corner-radius handles appear with the Node tool, including on paths.

Stroke controls remain expanded in the inspector. Enter widths above 64 px and choose inside, center or outside placement on closed paths. Open paths use centered strokes.

Groups select, move, duplicate and align as units. Double-click an item to edit it individually until another selection or deselection. Duplication stays in place. In Pixel mode, **Ctrl+D** clears an existing pixel selection; **Super+D** duplicates.

In Pixel mode, Ctrl-click an item on the canvas or in the layer list to select its rendered outline. The selection stays in document coordinates when you switch targets. Layer and object context menus include **Mask from item…**: choose it on the source, then click the target in the layer list. **Delete layer/object** removes the chosen item directly in every layer tree.

Documents using object masks or inside/outside strokes save as `.oma` format 6, which prevents older versions from silently removing those features. Other documents keep format 5 compatibility; this build reads formats 1–6.


## Lua plugins and path editing in 0.5.8

Open **Plugins → Manage plugins** to install, enable and run extensions. Studio
starter ships with the app. It includes a custom pixel filter, editable effects,
SVG icons, brushes, a drawing tool, patterns, gradients, palettes and automation.
See [Lua plugin guide](plugins.md) for the full API and batch commands.

With the Node tool, Alt-drag a corner-radius handle to change every corner.
Shift-select corners, then drag a selected corner’s radius handle to update the
selected set. Compound Boolean paths retain their contours and holes when you
convert to paths or double-click with Select/Node. Nodes, handles, and radii stay
editable after repeated operations. **View → Guides** has Lock all and Unlock all.
