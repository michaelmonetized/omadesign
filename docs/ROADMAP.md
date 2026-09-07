# Project status

omadesign is an alpha native Linux design studio. The current tools cover vector design, raster painting, photo adjustments, and motion in a shared document.

The [manual](MANUAL.md) describes the current development workflows. Known issues and planned work live in [GitHub Issues](https://github.com/michaelmonetized/omadesign/issues).

The published [v0.0.1-alpha download](https://github.com/michaelmonetized/omadesign/releases/tag/v0.0.1-alpha) predates the [layered interchange](https://github.com/michaelmonetized/omadesign/pull/40) and [RAW photo development](https://github.com/michaelmonetized/omadesign/pull/41) work. These features are available in the development preview; see the [preview build instructions](https://michaelmonetized.github.io/omadesign/docs/#development-preview).

The preview imports supported layers from PSD/PSB, PDF, PDF-compatible AI, SVG and OpenRaster, with an optional [Affinity bridge](affinity-import.md). Photo decodes supported camera RAW files at full resolution, saves adjustments in `.omaphoto` files and exports developed 16-bit PNG/TIFF or JPEG. See [file formats](format-support.md) for the import/export matrix and specific limits.

Current limitations include advanced publishing and text layout, reusable symbols, collaborative editing, PDF/X and CMYK print workflows. Affinity interoperability is partial, with the new unified `.af` format awaiting verification against a real document. Native AI/Affinity writing is not available. RAW support varies by camera and compression; proprietary camera looks and automatic lens corrections are not reproduced, and placement in Design uses 8-bit pixels.
