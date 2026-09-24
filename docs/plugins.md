# Lua plugins

Omadesign 0.5.8 embeds Lua 5.4 and plugin API **1**. Plugins can make editable
vector artwork, transform geometry, run pixel filters, set native effects and
gradients, supply SVG icons and brush presets, add canvas tools and opt-in
behaviors, install color palettes, and process folders of documents.

## Install and run

Open **Plugins → Manage plugins**. Install a `.lua` file, a folder containing
`main.lua`, or an `.omaplug` ZIP with `main.lua` at its root. Each plugin has an
Enable checkbox. Updates keep a hidden backup of the previous installed folder.
Use **Reload** after editing an installed plugin.

The release installs **Studio starter** on first installation. Its 12 actions
cover all the categories above. Subsequent app updates preserve your installed
copy; fresh source is in `~/.local/share/omadesign/plugin-examples/studio-starter`.
[Download the starter bundle](/plugins/studio-starter-1.0.0.omaplug).

Select an action, enter its parameters, and choose **Run**. A document operation
is one Undo step. Errors and cancellation leave the document unchanged. Editing
or switching documents while a plugin runs causes its result to be discarded.
A brush preset changes the active brush; a palette action saves to your personal
color library. Those settings are separate from document Undo.

For **Ribbon path**, choose **Activate tool**, then drag on the canvas. Escape
exits. The gesture previews as a line; the plugin generates editable output on
release. Choose a Raster document and raster layer before using pixel filters or
brush presets. Select vector objects before running a vector effect or gradient.

Document/selection behaviors are off by default. The manager’s behavior checkbox
opts in to events from enabled plugins and remembers your choice. A behavior’s
own output does not recursively trigger itself.

## Your first plugin

Save this as `main.lua`, then install its folder:

```lua
return {
  api = 1,
  id = "org.example.color-dots",
  name = "Color dots",
  version = "1.0.0",
  description = "A small editable pattern generator.",
  actions = {{
    id = "dots", name = "Create dots", category = "Patterns",
    parameters = {
      {id="count", label="Count", default=8, min=1, max=40},
      {id="color", label="Color", kind="color", default="#89B4FA"},
    },
    run = function(ctx, p)
      for i=0,math.floor(p.count)-1 do
        oma.add_shape{
          kind="ellipse", x=32+i*24, y=ctx.height/2,
          width=12, height=12, fill=p.color, name="Dot",
        }
      end
      oma.message("Dots created. Every circle is editable.")
    end,
  }},
}
```

Use a unique stable `id` (letters, digits, dots, hyphens, underscores; no leading
dot). Actions also need unique IDs. The version is your plugin’s version,
independent of the app. Categories in the manager are **Filters, Effects, Icons,
Brushes, Tools, Behaviors, Batch, Patterns, Gradients, Swatches**. Other category
names are allowed and appear under All categories.

Parameters support `number` (default), `text`, `color`, and `boolean`, each with
`id`, `label`, and a correctly typed `default`. Numbers optionally specify `min`
and `max`. The host validates values for desktop and CLI runs. Colors use hex
`#RRGGBB` or `#RRGGBBAA`. Unknown parameters are rejected.

Each invocation uses a fresh Lua VM. Keep persistent artwork in the document and
preset definitions in your plugin; Lua globals do not persist between runs.
Top-level code should only return the manifest and define functions. `oma` is
available when an action runs, not during manifest discovery.

## Context

`run(ctx, params)` receives:

| Field | Meaning |
| --- | --- |
| `ctx.api` | Host API version, currently 1 |
| `ctx.width`, `ctx.height`, `ctx.name` | Document dimensions and name |
| `ctx.active_layer` | Zero-based active layer index, or nil |
| `ctx.selection` | Array of selected vector objects with `layer`, `id`, `name`, `geom`, `style`, `rotation` |
| `ctx.layers` | Array of `{index, name, locked, visible, raster}`; raster is `{width,height}` or nil |
| `ctx.gesture` | Tool gesture or nil: `{points={{x,y},...}, alt, shift, ctrl}` |

Lua arrays start at **1**. Document layer indexes and pixel coordinates start at
**0**. Shape IDs are integers. Gesture points are in document coordinates.
Geometry and fill tables use the native tagged JSON representation. Inspect an
example with `omadesign --inspect document.oma`, or see the source types in
`src/geom.rs`, `src/document.rs`, and `src/filter.rs`.

## Host API

| Function | Result or behavior |
| --- | --- |
| `oma.add_shape(options)` | Returns `layer, id`; adds a native shape to an editable vector layer, creating a layer when needed |
| `oma.translate(layer, id, dx, dy)` | Moves a selected or identified vector shape |
| `oma.set_geometry(layer, id, geom)` | Replaces native geometry, including editable compound paths |
| `oma.remove(layer, id)` | Removes a vector shape |
| `oma.set_fill(layer, id, fill)` | Hex color, `"none"`, or native fill table |
| `oma.gradient(colors, kind)` | Native multistop fill; kind is `linear`, `radial`, `conic`, or `shape` |
| `oma.set_effects(layer, id, effects)` | Replaces that shape’s native effect stack |
| `oma.color(hex)` | Native `{r,g,b,a}` color, channels 0–255 |
| `oma.brush(options)` | Activates a native Raster brush preset |
| `oma.palette(name, colors)` | Adds a named palette to the personal library |
| `oma.read_asset(relative_path)` | Reads a UTF-8 asset within this plugin’s folder |
| `oma.svg(svg_text, x, y, width)` | Imports SVG paths as editable vector artwork, preserving aspect |
| `oma.pixel(layer, x, y)` | Returns source `r,g,b,a`; out-of-bounds returns transparent black |
| `oma.map_pixels(layer, callback)` | Replaces pixels using `callback(r,g,b,a,x,y) → r,g,b,a`. A non-raster `layer` falls back to the topmost visible raster layer |
| `oma.message(text)` | Shows a completion message in the app status bar |

`add_shape` supports `kind="rect"`, `"ellipse"`, `"line"`, `"path"`, or
`"geometry"`. Common fields: `x`, `y`, `width`, `height`, `fill`, `stroke`,
`stroke_width`, `name`, optional `layer`. Rectangles accept `radius`. Paths use
`points={{x,y},...}` and optional `closed=true`. Geometry uses `geom` containing a
native tagged geometry table. For example:

```lua
oma.add_shape{
  kind="geometry",
  geom={Path={closed=false, anchors={
    {pt={x=20,y=30}, h_in={x=0,y=0}, h_out={x=40,y=-20}, smooth=true, radius=0},
    {pt={x=120,y=60}, h_in={x=-40,y=20}, h_out={x=0,y=0}, smooth=true, radius=0},
  }}},
  fill="none", stroke="#A6E3A1", stroke_width=4,
}
```

Brush options: `size` (1–2048), `hardness`, `opacity`, `flow` (0–1), `spacing`
(0.05–4), and `color`. A pack can expose multiple named actions with different
presets. Palettes take an array of hex colors and persist as ordinary personal
palettes. Gradients remain editable in the native gradient editor.

Effects use tagged native tables, for example:

```lua
for _, s in ipairs(ctx.selection) do
  oma.set_effects(s.layer, s.id, {
    {Shadow={dx=8,dy=8,blur=12,color=oma.color("#00000080")}},
    {Saturate={amount=0.8}},
  })
end
```

Supported effects: `Blur {std}`, `Shadow/InnerShadow {dx,dy,blur,color}`,
`Offset {dx,dy}`, `Morphology {erode,radius}`, `Saturate/Brightness/Contrast/Invert
{amount}`, `HueRotate {degrees}`, `ColorMatrix {values}` (20 numbers),
`Turbulence {fractal,base,octaves,seed}`, `Displacement {scale,x_ch,y_ch}`.
Stacks allow up to 32 effects. Blur is capped at 512, morphology radius at 64,
offsets/displacement at 4096, turbulence at 8 octaves and base frequency ≤1.

Pixel callbacks use straight RGBA channels, 0–255. Return alpha explicitly to
preserve transparency. `oma.pixel` reads the source image throughout a mapping
pass, allowing neighborhood filters without feedback from already written
pixels. Mapping all pixels is one Undo operation. Large or expensive filters
can hit the execution limit; reduce input size or work per document in a batch.

SVG icon bundles should contain paths and presentation attributes. Local fragment
references such as `url(#gradient)` are allowed. External references, stylesheets,
and CSS escapes are rejected. Bundle fonts as outlined paths for icon artwork.

For a tool, set `tool=true` on the action and consume `ctx.gesture.points`. For a
behavior set `event="selection_changed"` or `event="document_opened"`. Normal
actions need neither field.

## Batch processing

Install/list without opening a window:

```sh
omadesign --install-plugin ./my-plugin
omadesign --list-plugins
```

Run one document, saving a new editable result (PNG/SVG exports also work):

```sh
omadesign --plugin ./my-plugin --command dots \
  --input input.oma --output output.oma \
  --params '{"count":12,"color":"#A6E3A1"}'
```

Process every immediate `.oma` file in a folder, in filename order:

```sh
omadesign --plugin ~/.local/share/omadesign/plugins/org.omadesign.studio-starter \
  --command nudge --batch ./input --output-dir ./output \
  --params '{"dx":20,"dy":0}'
```

CLI runs select every visible, unlocked nonguide vector shape and use the first
unlocked layer as active. Inputs are preserved; existing output paths are refused.
A failed document does not stop other files, but the final exit status is nonzero.
Brush/palette actions belong in the desktop manager, and canvas tools require a
gesture; these actions are not document batch commands.

## Runtime limits and distribution

Plugins run on a background worker. They cannot execute programs, access the
network, or read arbitrary files. Lua has table/string/math/utf8 and basic
functions; `io`, `os`, `package`, `debug`, `require`, file loaders, binary chunks,
`pcall`, `xpcall`, and coroutines are unavailable. Use `assert` or `error` to abort.

Limits: 15 seconds per run, 64 MiB Lua heap, 20,000 document edits, 256 MiB queued
edit data, 128 MiB raster input, 2 MiB source, 4 MiB per asset/geometry, 128 actions,
24 parameters per action. Installed bundles allow 512 entries and 64 MiB total.
Archives reject path traversal and symlinks. Plugin file access stays inside the
installed bundle. Hidden/locked shapes and guides cannot be modified.

To distribute, ZIP the **contents** of your folder, including `main.lua`, assets,
README and license, and name it `your-plugin.omaplug`. Users install it in the
manager. There is no remote plugin marketplace in this release.

## Contribute

Fork [Omadesign](https://github.com/michaelmonetized/omadesign), add a uniquely
named folder under `plugins/`, and open a pull request. Include:

- `main.lua`, a README with inputs/output and supported app/API versions, and a license.
- Original or properly licensed assets, with attribution.
- A small `.oma` example or reproducible instructions and an output screenshot.
- Verification that errors/cancellation preserve input, outputs save/reopen, and
  document edits undo in one step. Test batches in a new output folder.

Keep manifest discovery fast and free of side effects. See
`plugins/studio-starter` and `src/plugins/tests.rs` for complete examples and host
regression coverage. Propose host API additions in an issue or PR; do not depend
on unexposed app internals.
