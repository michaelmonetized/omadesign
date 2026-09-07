# Changelog

## 2026-09-06 · Pass 14 — Develop your originals.

Camera RAW files now open directly in Photo with a bundled LibRaw decoder.
DNG, Canon CR2/CR3, Nikon NEF, Sony ARW, Fujifilm RAF and other supported camera
families keep their full-resolution 16-bit linear pixels. Exposure adjustments
happen before display conversion, including DNG baseline exposure. PNG and TIFF
exports retain 16-bit channels; JPEG remains available for sharing.

Loading, folder scans, full-resolution detail and export work run in background
workers. Zooming in requests detail from the full image. Save settings writes an
adjacent `.omaphoto` file, leaving the original untouched; reopening restores the
adjustments. Undo and Redo belong to the selected photo. Quitting waits for writes
and offers to save every edited photograph before handling Design documents.

Place in Design develops a full-resolution pixel layer in the background and
preserves the destination artwork. A new document takes the photo's actual size.
Cropping an already cropped or rotated photo now uses the visible image correctly,
and cropped exports no longer acquire a transparent edge. File-menu exports and
keyboard shortcuts act on Photo while that studio is active.

Validation: **329 tests pass**, including asynchronous imports, settings and quit
safety, full-resolution placement, crop/rotation and zoom clipping. Real ProRAW
DNG, Canon CR3 and Fujifilm X-Trans RAF decode and PNG16 exports match independent
references exactly. A saved exposure/crop/rotation edit also matches an independent
TIFF16 reference; source checksums remain unchanged. ARM64 and x86_64 packages
pass static-dependency and glibc 2.35 checks, including x86 RAW decoding against
Ubuntu 22.04's runtime. Clippy completes with existing repository warnings.

Known limits: camera/compression support follows the bundled decoder. Camera
looks, full Adobe profiles, automatic lens corrections and some DNG processing
instructions are not reproduced. Later edits cannot recover sensor/channel
clipping. Design placement becomes 8-bit pixels; the original and its settings
remain separate. RAW writing and preservation of all export metadata are not
provided. See [format support](docs/format-support.md#camera-raw).

## 2026-09-06 · Pass 13 — Bring your layers.

Open Photoshop PSD/PSB, PDF-compatible Illustrator files, multi-page PDFs,
OpenRaster, SVG/SVGZ, and supported Affinity documents as native layered work.
Groups now expand in the Layers studio, inherit visibility and locks, and move
or delete with their children in one Undo. Masks retain their placement; isolated
and pass-through groups composite separately. File → Place keeps the imported
hierarchy and scales it into the rectangle you draw.

Large imports and layered exports run away from the frame loop. Imported source
files open as unsaved working documents, so Save asks for an `.oma` destination.
The file manager can offer Open With for native and Affinity documents without
changing existing default apps. Conversion notes travel with the working file and explain substitutions and
unsupported objects. Headless `--inspect` and `--convert` use the same codecs as
the desktop. PSD/PSB, PDF and OpenRaster join SVG and image export.

Real files copied from a Mac's iCloud exposed bugs that synthetic documents did
not: a Photoshop color overlay, Affinity Photo float channel tiles, Illustrator
embedded CFF fonts, clipped gradients and transparency masks. The PSD now keeps
its original layer pixels and reproduces its saved composite exactly. The
four-page Illustrator document keeps all its artboards; a second Illustrator file
contains blank PDF-compatible pages and receives an explicit diagnostic. The
Affinity bridge is separately installed, pinned and GPL-licensed; the native
application remains MIT.

Validation: **309 tests pass**, plus four optional PDF interoperability/sample
checks, independent Photoshop and OpenRaster readers, native studio captures of
Designer/Photo/Illustrator documents, formatting and desktop-entry validation.
The PDF page fallback reproduces native pixels exactly with independent Cairo
rendering. Clippy completes with existing repository warnings; strict warning-free
Clippy is not claimed. The release build is installed for local testing.

Known limits: this is partial interoperability, not complete Affinity or Adobe
application parity. Native Affinity and Illustrator writers are unavailable;
new `.af` and Publisher documents lack real-file verification. PSD text and smart
objects use saved pixel layers; complex PDF/SVG text may become outlines.
High-depth pixels become 8-bit and custom profiles are not fully applied. See
[the format matrix](docs/format-support.md) for exact import/export limits.

## 2026-09-06 · Pass 12 — The download caught up.

The website and the download now tell the same story. **v0.0.1-alpha** packages
the current studio for **ARM64 and x86_64 Linux**, including the 52 templates,
guides and snapping, deformation tools, animation presets, shortcut HUD, and
portable palette, brand-asset, and typography libraries showcased on the site.

The hero's two binaries and source archive point to this tagged release. The
FAQ now confirms that these features are included, and Getting started links
the same release notes. The app's alpha status and current limits stay clear.

Installation checks the published SHA-256 before opening the archive. A missing,
malformed, or mismatched checksum stops installation. Local packaging builds only
the application, checks both binaries' required glibc versions against the 2.35
baseline, and includes the Phosphor icon license. `omadesign --version` (or `-V`)
now reports the installed version without opening a window.

Validation: **272 tests pass**, plus formatting, shell checks, TypeScript, and the
website build. Both final archives pass checksum, version, and ELF checks. The
ARM64 package renders native Pen/HUD and Brand windows and exports PNG; the x86_64
package renders native Pen/HUD and Motion windows on a second Linux machine.
Both binaries start against Ubuntu 22.04's glibc 2.35; x86_64 also exports PNG on
that baseline runtime. Installer failure checks confirm that invalid downloads
stop before extraction.

## 2026-09-06 · Pass 11 — Now it moves.

The front door needed more than a polite fade. Headlines now arrive through bold
Catppuccin color wipes, lines follow in sequence, and the template title opens
through an expanding ellipse. Sections rise into view and leave with the scroll.
The artwork gallery has room to breathe: four floating planes orbit through real
3D space, with depth, camera movement, drag momentum, and a pause button.

### The studio is the demo

The simulated panel controls are gone. Design, Pixel, Photo, and Motion now have
separate native screen recordings with named chapters. Click a chapter to jump
straight to the actual tool or panel. The shared brand recording covers palettes,
assets, and typography, including their project dotfiles. These are captured WGPU
frames from the Rust app, with real pointer and keyboard events driving the edits.
The capture utility, chapter manifest, captions, and regeneration notes are included.

The gallery and recording tabs stay in sync. Videos load when requested, pause
after leaving the viewport, and stop when changing studios. Native controls,
downloads, captions, keyboard navigation, and a no-JavaScript fallback keep the
tour usable. Reduced motion switches the orbit to a static artwork grid and
removes the traveling reveals. Keyboard focus reveals controls immediately; a
pointer press holds an arriving panel in place until the click has finished.

### The maze is back

The header and footer restore the complete approved logo: blurred OMA maze,
open-S deSiGN lettering, and original proportions. Both Catppuccin themes keep it
legible. The hero now reads **Professional Grade Graphic Design Tool Suite**,
followed by **Developed for Omarchy BTW.** and **Illustrate / Paint / Refine /
Animate**. The larger **get started in under 1m** line leads into the installer
command, with all three download buttons directly below it.

### A recording found a real wobble

Opening keyframes, effects, or Trace could force the native inspector wider and
shift its controls. Labels and editable values now sit above tracks that fit the
available width. The sidebar stays at the width you chose, including its narrowest
setting. A focused native layout regression opens the real panels and exercises
changing values and every effect across narrow, default, and wide layouts.
Visible color labels in the recorded native panels also use US spelling.

Validation: **272 native tests pass**, including the inspector geometry check.
The release build is reinstalled; installed and built hashes match, native capture
works, and the desktop entry validates. TypeScript, formatting, and the server
build pass. Browser checks cover visible text reveals and exits, mouse momentum,
real touch input, keyboard controls, reduced motion, no JavaScript, and layouts
from 320 to 1735 pixels. All five recordings decode and play; early and rapid
chapter seeks, source changes, offscreen pause, captions, and clicks during an
entrance animation pass. The **156 seconds of native footage total 4.67 MB**.

## 2026-09-05 · Pass 10 — Less pitch. More studio.

The green paint has left the building. So has the pretend logo. The website now
wears Catppuccin Mocha's base, mantle and crust, with a matching Latte option and
the approved geometric deSiGN paths in the header, footer and favicon.

### Take the studios for a spin

Design, Pixel, Photo and Motion now sit in an interactive 3D artwork carousel.
Click a side card, swipe, use the arrows, or navigate with the keyboard. Below
it, **28 panel choices** expose working preview controls: change a fill, reshape
an object, adjust the light, explore masks, or play an animation preset. Settings
stay with their studio. Playback starts only when requested; reduced-motion and
no-JavaScript alternatives are included.

The artwork has a paper trail. Design and Motion use editable native vector
projects. Original generated iris and coastal images supply the Pixel and Photo
examples, with native import, Photo development and export. Sources, prompts and
regeneration instructions live in `examples/site-showcase`. The web controls are
clearly labeled as simplified previews.

### Downloads at the front. Fluff at the exit.

The hero contains **Omarchy**, **Omarchy MX Mac** and **Download src**, the
requested “get started in under 1m” footnote, and the copyable installer command.
The ARM64 button targets Linux on Apple Silicon; the FAQ explains the current
published-alpha/source gap.

Numbered kickers, repeated subtitles and the bottom sales pitch are gone. Copy
uses US spelling. The old “slower” artwork is removed, and the template library
has a fresh native capture. The corrected 97-second film remains available.

The brand kit now demonstrates real interactions: Catppuccin palette swatches
apply to fill or stroke, asset search and category filters compose, and font roles
update an editable type sample. JSON examples remain available when wanted.

### Motion with manners

Sections and their contents enter, drift gently while in view and leave as you
scroll. Hover, focus and preview controls add feedback. One passive scroll
listener updates visible surfaces; layout measurements exclude animated positions
so controls stay put when clicked. Reduced-motion preferences disable the travel.
Obsolete screenshots, the old studio switcher and unused style rules are removed.

Validation: **271 native tests pass**; release build, atomic reinstall, matching
binary hashes, native capture and desktop-entry checks pass. TypeScript, formatting,
server build and static prerender pass. Browser checks cover all 28 panels at both
320 and 1440 pixels, touch and keyboard navigation, brand/HUD/search controls,
clipboard fallback, theme persistence, documentation, video playback and motion
preferences. The final GitHub Pages artifact is reviewed under `/omadesign/`.

## 2026-09-05 · Pass 9 — The front door finally fits the house.

The studio had grown. Its website was still waving from three releases ago.
Now the first thing visitors meet is the reason this app exists: a creative
home on Linux, built for Omarchy, with Rust underneath and ordinary files at
its heart. PRs **32–38 are merged into master**, including the corrected logo
and finished film.

### Your Linux. Your creative suite.

A new charcoal-and-green homepage leads with Omarchy theme and font integration,
native responsiveness, ARM64 and x86_64 Linux packages, and the MIT licence.
Current native screenshots introduce Design, Pixel, Photo and Motion; switching
studios changes the artwork and the tool story together. Light mode brings a
softer paper-and-ink treatment, and remembers the choice when browser storage
is available.

The portable brand kit gets a place of its own. Explore `.omacolors`,
`.omabrand/` and `.omatype`, with readable examples and the actual native panels.
Try the keyboard HUD with tool buttons or a held modifier. Browse **51 features
in nine groups**, search for a particular tool, and open the details that matter.
The 52 editable templates and the corrected 97-second film round out the tour.
The film loads on demand, with native playback controls, scene captions and a
download link.

### A guide that reads like a guide

The manual now has real paragraphs, lists, tables and linkable headings. The
four-studio introduction, project-kit links and documentation navigation work
under the site's GitHub Pages path. The source-build command explicitly chooses
the main application binary. Unsafe Markdown links and raw HTML stay disabled.

### One website, one source

The stale hand-maintained root homepage and superseded media have retired.
TanStack Start prerenders the homepage and documentation from the same source;
a local publisher checks routes, assets and the reviewed revision before
updating a generated `gh-pages` branch. No GitHub Actions workflow or new hosting
account is required. Focused components keep the interactive sections separate,
and lossless WebP screenshots keep the image payload small.

The copy names the actual strengths and the alpha's current boundaries. It does
not claim to be Linux's first professional creative application or invent a
competitor speed ranking. Published September 2 packages predate the new feature
stack; the site clearly points to current-source builds for everything shown.

Validation: **271 native tests pass**, formatting and the release build pass,
and the installed executable matches that build byte for byte. An isolated
native Brand capture and desktop-entry validation pass. Browser QA covers all
studio and dotfile previews, real HUD modifier events, feature search/categories,
FAQ disclosures, clipboard success/fallback, theme persistence/blocked storage,
97-second video playback, and layouts from 320 to 1440 pixels. Documentation,
static direct links and the final published artifact are checked before shipping.

## 2026-09-05 · Pass 8 — S, as in design.

The extra downstroke on the S closed its upper opening and made it read as a g.
Removing it restores the square S, with matching stroke weight and spacing.
The editable logo, SVG, PNG and native construction replay use the correction.

The film now opens with that same complete logo and closes with its animation.
The separate introductory wordmark has retired. Timing stays at 97 seconds,
including the 30-second construction scene and five-second ending.

All **271 tests pass**, including the shortcut suite and native logo construction.
Formatting and Clippy pass with the existing 34 library advisories (46 including
tests). Visual review confirms the lettering reads deSiGN. The release build is
reinstalled. The [corrected film](https://github.com/user-attachments/assets/52fb79d2-373d-48cc-966b-3cb65b766937)
decodes all 2,910 frames; opening and ending frames both match the corrected logo.

## 2026-09-05 · Pass 7 — A mark of our own.

The maze found its letters. The Pen found its manners. Your fonts have a place
to travel with the project, too.

### A straight answer from Shift

Pen previews now use the same constrained position as the point you place.
Holding Shift visibly locks the next segment to 0°, 45° or 90°. Quick modifier
releases keep the click's own state, and handle drags include the final release
position. Shift, Alt and Ctrl changes also work when the pointer is still.

### The type belongs with the brand

**`.omatype`** describes a named set of font roles. Their files live in
**`.omabrand/fonts/`**. The Brand panel can load, name, save, filter and share the
collection, and Character offers the project's fonts alongside ordinary local
faces. Applying a role changes live, editable type with Undo.

Portable font identities let a copied project find the same face on another
machine. Loading a project resolves its kit before shaping text; background
refresh picks up new files without rewriting already-applied font choices.
Save As, native brand assets and recovery snapshots carry the faces their text
uses. Failed writes preserve existing artwork. SVG exports use glyph outlines
for project fonts, while the original document keeps its editable text.

The Brand panel scrolls at compact heights, keeping every role action reachable.
Secondary typography actions share a small menu beside Add fonts.

### The maze becomes a wordmark

The new logo construction scene rebuilds the supplied `logo-4.oma` idea on a
1920×1080 black artboard. Six editable Pen paths share a consistent stroke,
square corners, aligned baselines and balanced spacing. The Omarchy maze keeps
its green glow; the wordmark gets a shadow and individual animation presets.

The 30-second native editor replay replaces the old star-clone scene. The
finished mark returns as a full-frame animated ending. The remaining demo
chapters stay in the film, with a newly extended original soundtrack.

The [finished film](https://github.com/user-attachments/assets/fc19dbe5-5313-4872-b8dc-d433f500b7f1)
is 97 seconds at 1080p / 30 fps. The new scene runs from 0:13 to 0:43, with the
logo ending at 1:32. Editable [artwork](media/logo-4-refined.oma),
[SVG](media/logo-4-refined.svg) and [PNG](media/logo-4-refined.png) are included.

Pass 7 verification: **271 tests passed**, including the shortcut suite, real
Pen modifier events, fresh-process font moves and recovery, failed-write rollback,
missing dependencies, archive capacity, SVG glyph export and compact sidebar
scrolling. The sidebar remains 288 px wide at both QA sizes. Native recordings
cover construction, styling and animation; the final film decodes all 2,910 frames.

Formatting and Clippy passed without new advisories or suppressions. The existing
34 library advisories (46 including tests) remain. The release build is installed,
and the installed executable matches the build byte for byte. Physical keyboard
and native file-dialog interaction remain part of human QA.

## 2026-09-05 · Pass 6 — Good taste travels.

Your brand can live beside your work now. Copy the folder. Take the colours.
Bring the little leaf logo. It all travels as ordinary files.

### A proper home for your colours

The right sidebar now has **Inspect / Palettes / Brand** tabs. Palettes has named
Personal and Project collections, a name-or-hex filter, transparent hex colours,
Fill and Stroke targets, and colour collection from selected artwork. Rename,
duplicate, remove, import, and export are all there; the old tucked-away palette
editor has retired.

Project colours live in **`.omacolors`**, a readable JSON file containing named
palettes and hex values. Share one palette or the whole collection. Imports keep
existing colours and resolve name collisions. Older personal palettes still load.

Edits stay in their own draft until Save. Background refresh notices changes on
disk without overwriting your work, and quitting checks for unsaved collections
before continuing to the artwork save prompt.

### The brand cupboard, without the cupboard doors

**`.omabrand/`** is a filterable bank of logos, images and reusable `.oma` artwork.
Nested folders become searchable categories. A small optional `brand.json` gives
the bank its name. Create or load a bank, add files, rename it, and save a portable
copy from the sidebar. Originals stay where they were; duplicate filenames get
new names.

Drag a tile onto an artboard, or double-click to place it at the centre. Each
placement gets one Undo. Native artwork keeps its editable shapes, text, masks
and motion tracks, with fresh object IDs. Photo offers double-click placement
into Design. Escape cancels a pending load; switching documents cannot drop a
late arrival into the wrong tab.

Discovery, scans and image decoding run in the background. The grid lays out
visible rows, loads only visible previews and bounds its texture cache. Libraries
follow the active document's nearest project folder and notice files added on disk.

### A kit to get your hands on

[Fieldwork](examples/fieldwork) ships with three curated palettes and six original,
editable SVG marks, illustrations and patterns. Copy its hidden sidecars into a
project, or point the sidebar at the example folder. Two native screenshot scenes
show the kit in context.

SVG assets use the existing native import subset; advanced SVG features may not
carry over. Palette collection saves are separate from document saves.

Pass 6 verification: **259 tests passed**, including real sidebar typing, colour
application, filtering and disk saves; native tile drag/drop and Undo; delayed
worker cancellation; external-edit conflicts; document switching; and safe quit
ordering. Repeated layout checks keep both populated panels near 288 pixels wide
at compact and large sizes, including unusually long names.

Formatting and Clippy completed without suppressions. Clippy reports the existing
34 library advisories (46 including tests). All six sample assets passed native
import and preview rendering. Release-build and installed-app screenshots cover
both panels at 960×640 and 1600×1000. Physical keyboard and OS file-dialog use
remain part of human QA.

## 2026-09-05 · Pass 5 — Little keys. Big confidence.

The studio has a quiet co-pilot now. It knows what is in your hand.

### Your next move, in plain sight

- A bottom **Shortcut HUD** shows the active tool's gestures above a strip of
  letter-key hints. Pick Pen and the path controls follow. Hold Ctrl, Shift, Alt
  or a combination and the command row changes with your hand.
- Held gestures light up. Command keycaps stay compact: when Ctrl is held,
  **S · Save** appears under the Ctrl heading. Let go and the tool keys return.
- The HUD uses the same shortcut routing as the app. Text edits, focused fields,
  menus and drawing gestures get appropriate hints. Reading the strip does not
  steal focus or trigger a command.
- The strip stays the same height through modifier changes, keeping the canvas
  steady during a drag. Smaller windows show **+ more** with the overflow hints.
  **Ctrl+/** and **View → Shortcut HUD** toggle it; **F1** remains the full key list.

### Two small gaps closed

Equal-spacing suggestions now reach tool points as well as moving objects and
artboards. Pen placement can find repeated and balanced gaps while retaining
edge/centre/guide priority and Shift constraints.

Photo crop now honours Enter to apply and Esc to cancel during a drag. A held
mouse button cannot silently restart the crop after cancellation.

The earlier passes are included: editable guides and rulers, constrained moves,
smart snapping, Free Transform and four reshape modes, stroke outlines, Select
Same, Pathfinder, masks/healing, 13 motion presets and 52 editable templates.

Pass 5 verification: **239 tests passed**, including the full shortcut suite and
real modifier-only frames during Pen drawing. Checks cover angle constraints,
broken handles, snapping override, text-field/modal ownership, focus loss,
Ctrl+/ toggling, and command delivery with the HUD present.

Layout checks cover all four personas at 960×640 and 1600×1000, with hints shown
and hidden, and prove the canvas stays put as modifiers change. Tool-point gap
snapping and the complete Photo crop Enter/Esc/release lifecycle also pass.
Native previews cover bare, Ctrl, Shift, Alt and combined-modifier states, plus
the compact Motion layout. The release build passed. Clippy completed with
34 library advisories (47 including tests), without suppressions. Physical
keyboard and OS dialog interaction remain part of human QA.

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
