# Brush packs

Choose **Pixel → Brush → Brush library** to browse packs and see their strokes.
Omadesign includes Essentials and Texture: 13 original tips, including Square, free to use, modify,
and share under CC0. Get the same portable packs at **omadesign.app/brushes**.

Choose **Import** for `.omabrush`, Photoshop `.abr`, Illustrator `.ai`, PNG,
JPEG, WebP, TIFF, SVG, or EPS artwork (EPS needs Ghostscript). Drop ABR and Omadesign packs into the editor to install
them. Original source files are never changed. Imported packs live in
`$XDG_DATA_HOME/omadesign/brushes` (normally `~/.local/share/omadesign/brushes`).

A transparent image uses its alpha as the brush tip. An opaque image uses dark
pixels as ink and white pixels as empty space. Empty borders are cropped.
SVG artwork is rendered to a sampled tip. Paint in your chosen foreground color,
then adjust Size, Spacing, Flow, and Opacity. The cursor shows the selected tip’s visible boundary and angle. Sampled tips include their own edge. Save current settings as a brush,
or export the selected pack to share it.

Paint across and outside artboards, including negative canvas coordinates. Pixel
layers grow with the brush; existing artwork stays in place. Undo restores both
the stroke and the previous layer bounds. Selections and layer masks still limit
where paint appears. Automatic growth supports up to 64 million pixels per layer;
add another pixel layer when working across a larger area.

Spacing ranges from 1% to 10,000% of the tip size and carries between input samples. Angle rotates the tip; **Follow stroke direction** turns it along the stroke. **Scatter and randomness** contains scatter distance and angle, size, and opacity jitter. Each stroke starts a fresh random pattern. Saved presets retain these controls.

Wayland tablet tools and X11 tablets supply contact, release, position, and pressure. In the Brush inspector, enable pressure for Size and Flow independently. Dab spacing follows the pressure-scaled size, including very light strokes. Hover previews retain the chosen tip shape. Tablet pad buttons, tilt, physical tool rotation, and automatic reverse-side eraser switching are not brush controls yet. Third-party licenses still apply.

Photoshop ABR versions 1, 2, 6, 7, and 10 support sampled 8/16-bit tips with raw
or PackBits compression. Names and spacing come from descriptors when available. Modern computed round and elliptical tips also import. Photoshop-specific tool settings, pressure,
scattering, textures, and dual-brush dynamics are not reproduced.

Illustrator art and pattern libraries import from legacy PostScript or PDF-private
artwork, including zlib and Zstandard compression. Unsupported libraries can use
expanded artwork exported as SVG. Art, pattern, and bristle
brush behavior is not reproduced as editable vector strokes.

For batch conversion:

```sh
omadesign --convert-brushes input.abr output.omabrush
```

Each portable pack is versioned JSON containing names, settings, source notes,
license information, dimensions, and base64 alpha samples. Imports validate
counts, lengths, decoded memory, dimensions, and settings before installation.
