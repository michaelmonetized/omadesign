# Project status

omadesign is an alpha native Linux design studio. The current tools cover vector design, layout frames, raster painting, photo adjustments, and motion in a shared document.

The [manual](MANUAL.md) describes the available workflows. Known issues and planned work live in [GitHub Issues](https://github.com/michaelmonetized/omadesign/issues).

The [v0.5.0 release](https://github.com/michaelmonetized/omadesign/releases/tag/v0.5.0) adds the Layout persona, opt-in cloud comments, and a public showcase at [omadesign.app](https://omadesign.app). Layered interchange, RAW development, batch looks, rotated nodes and flips remain.

Import supported layers from PSD/PSB, GIMP `.xcf`, PDF, PDF-compatible AI, SVG and OpenRaster, with an optional [Affinity bridge](affinity-import.md). Photo decodes supported camera RAW files at full resolution, saves adjustments in `.omaphoto` files and exports developed 16-bit PNG/TIFF or JPEG. See [file formats](format-support.md) for the import/export matrix and specific limits.

Current limitations include advanced publishing and text layout, reusable symbols, CRDT collaborative editing, PDF/X and CMYK print workflows. Layout is frames, stacks and constraints — not Figma import. Cloud sync is last-write-wins and opt-in. Affinity interoperability is partial, with the new unified `.af` format awaiting verification against a real document. Native AI/Affinity writing is not available. RAW support varies by camera and compression; proprietary camera looks and automatic lens corrections are not reproduced, and placement in Design uses 8-bit pixels.
