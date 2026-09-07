# Project status

omadesign is an alpha native Linux design studio. The current tools cover vector design, raster painting, photo adjustments, and motion in a shared document.

The [manual](MANUAL.md) describes the available workflows. Known issues and planned work live in [GitHub Issues](https://github.com/michaelmonetized/omadesign/issues).

The [v0.0.3-alpha release](https://github.com/michaelmonetized/omadesign/releases/tag/v0.0.3-alpha) includes layered interchange, RAW photo development, batch adjustments and shareable presets in both ARM64 and x86_64 Linux downloads.

Import supported layers from PSD/PSB, PDF, PDF-compatible AI, SVG and OpenRaster, with an optional [Affinity bridge](affinity-import.md). Photo decodes supported camera RAW files at full resolution, saves adjustments in `.omaphoto` files and exports developed 16-bit PNG/TIFF or JPEG. See [file formats](format-support.md) for the import/export matrix and specific limits.

Current limitations include advanced publishing and text layout, reusable symbols, collaborative editing, PDF/X and CMYK print workflows. Affinity interoperability is partial, with the new unified `.af` format awaiting verification against a real document. Native AI/Affinity writing is not available. RAW support varies by camera and compression; proprietary camera looks and automatic lens corrections are not reproduced, and placement in Design uses 8-bit pixels.
