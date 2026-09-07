# Layered file formats

Omadesign imports supported content into its native layer tree and records conversion notes with the document. A format appearing in Open is not a promise of complete application compatibility. Save imported work as `.oma` to retain Omadesign's editable document and the import notes; retain the source when a conversion reports losses.

| Format | Import | Export | Main limits |
| --- | --- | --- | --- |
| `.oma` | Native document, groups, vectors, text, pixels, masks, effects, pages and conversion notes | Native | Omadesign's editable working format; it cannot recreate unsupported features already lost during import. |
| `.af`, `.affinity` | Supported new Affinity document objects through the optional converter | No native writer | Upstream V3 support is installed; no real unified `.af` fixture has yet been verified in this change. |
| `.afdesign`, `.afphoto`, `.afpub`, `.aftemplate`, `.afpackage` | Supported Affinity vectors, text, pixel layers, groups, visibility, opacity, masks and artboards via SVG | No native writer | Partial Affinity interoperability. Most adjustments/live effects, publishing structures, custom profiles and edit history are not retained. Multi-spread documents can fail. |
| `.psd`, `.psb` | Native layered reader, including groups, names, placement, visibility, opacity, blends, pixel masks and supported Normal color overlays | Native layered RGB/8-bit PSD and PSB | Text and smart objects import as their saved layer pixels. Other effects, fills, adjustments and high-depth/color-management limits produce notes. |
| `.pdf` | Pages/artboards, paths, supported text, images and optional-content layers | PDF with pages, vector artwork and optional-content layers | Complex layer/group appearances export as pixels with notes; backdrop-dependent effects can require rasterized pages. PDF artwork is not a full source document. |
| `.ai` | PDF-compatible Illustrator artwork; older PostScript artwork through conversion | No Illustrator-native writer | Private Illustrator data, live effects, symbols and appearance stacks are not reconstructed. Export PDF or SVG for interchange. |
| `.svg`, `.svgz` | Objects, layer/group hierarchy, names, transforms, text, images, styles, visibility and supported masks | SVG | Complex text can become editable outlines. Clips/masks can become pixel masks; unsupported effects may be rendered into pixel layers. Scripts, animation and foreign content are not imported. |
| `.ora` | OpenRaster pixel layers/groups, offsets, names, visibility, opacity, isolation and supported blends | OpenRaster with layers, stack.xml, merged preview and thumbnail | Vectors/text become pixels per layer. Masked/effected groups become one pixel layer. Extended SVG layer sources and unsupported compositing operations are rejected. |
| `.eps`, `.ps` | Ghostscript conversion to PDF, then native PDF import | No direct writer | Ghostscript must be installed. Conversion can discard original layer metadata or flatten artwork. |
| PNG, JPEG, WebP, GIF, BMP, TIFF | Pixel image | PNG and JPEG through headless conversion; other existing export actions remain separate | This path imports a single image, not layered TIFF or animated GIF structure. |

## Affinity

Run `./scripts/setup-affinity-import.sh` once to install the pinned external converter. Inkscape's Python modules plus Python zstandard, Pillow and NumPy are required. Import does not download software. Full setup, version pin, license information and limitations are in [affinity-import.md](affinity-import.md).

The converter supports the new `.af` container through upstream Affinity V3 work. Legacy Designer fixtures have been converted, and real user `.afdesign` and `.afphoto` files have been exercised. This does not establish general `.afpub`, `.aftemplate`, `.afpackage`, or unified `.af` fidelity.

A real Photo document exposed a missing format-9 raster decoder. The separate GPL compatibility module now reads its floating-point channel tiles, including constant-one alpha tiles. That file produces seven image objects without conversion exceptions, versus four before the fix. Channels become 8-bit RGBA, values outside 0–1 are clipped, and custom profile transforms are not applied; the import report says so. This is not an HDR-preserving conversion.

Affinity resources such as `.afassets`, `.afbrushes`, `.afstyles`, `.afpalette` and `.afmacro` are not layered documents. `.afbook` references chapter documents. An `.afpackage` is a document accompanied by resource folders, not necessarily a ZIP archive. Linked resources still need to be available to the conversion path.

The Rust application remains MIT-licensed. Inkscape, its converter and the files in `scripts/affinity-bridge/` are separately licensed GPL programs. Setup preserves their source and license. A distribution bundling them must preserve the corresponding source and license obligations.

## Photoshop

The reader supports raw, RLE, ZIP and ZIP-predicted channels, grayscale/RGB/CMYK documents, and 8/16/32-bit source channels. Higher depth becomes 8-bit RGBA; CMYK conversion is approximate and does not apply embedded ICC profiles. Layer hierarchy, Unicode names, negative offsets, hidden layers, opacity and the supported blend modes are preserved. Masks retain their placement, default fill and density. Feathering is not supported. Clipping relationships are baked into editable pixel masks, so later edits to the base no longer propagate automatically.

Text and smart objects retain their separate saved pixel layers rather than native type or embedded documents. A supported Normal Color Overlay becomes an editable native color effect. Other adjustments, fills and effects are not recreated, and their notes explain the limitation. Vector masks use a cached pixel mask when available.

Export writes RGB/8-bit PSD or PSB with separate layers, groups, pixel masks, offsets and supported color overlays. Vectors, native text, transformations and other effects are rendered into their individual pixel layers. Opaque canvas paper is exported as a background layer. Application limits include 64 megapixels, 512 MiB decoded working budgets, 8,192 layers and 64 group levels.

Verification includes independent Photoshop samples and a real 37 MB PSD. Independent `psd-tools` inspection confirmed the supported color overlay remained editable after export, the original layer pixels were unchanged, and the saved composite matched pixel for pixel. Nine focused codec tests cover independent PSD/PSB fixtures and mask/group cases.

## PDF and Illustrator

PDF import creates artboards for all pages, editable paths and strokes, nested optional-content layer groups, and pixel layers for embedded images. It reads supported axial gradients, blending, opacity, clipped artwork and alpha/luminosity soft masks; clips and soft masks become cropped pixel masks. Supported straightforward text and embedded OpenType remain editable; embedded CFF and transformed/complex text can become editable outlines. Embedded subset fonts may not contain characters needed for later edits.

Import reports approximations for unsupported drawing operators, radial/mesh/pattern shading, dash phases/patterns, custom miter limits, font substitutions, ICC/spot/pattern colors, overprint and knockout. PDF transparency groups are imported, but nonisolated groups are approximated as isolated groups. Annotations and form fields are not artwork. Encrypted PDFs must be saved as unlocked copies before importing. Illustrator's private editing data is not interpreted: `.ai` support means its PDF-compatible or older PostScript artwork.

PDF export writes pages, paths, raster images with alpha, opacity/blending and optional-content layer metadata, including names, order, visibility and locks. Text becomes vector outlines. Gradients, masks, effects and complex compositing are rendered at document resolution into the affected layer or group, while unaffected vector layers remain editable. A rendered group becomes one named PDF layer; its internal objects and hierarchy remain in the `.oma` master. Hidden layers retain their visibility state. Backdrop-dependent pass-through effects can require rendering entire pages. Every fallback produces a note explaining the reduction in editability; effects are not silently dropped or replaced with a single gradient color. Fallback images are limited to 64 megapixels each and a 512 MiB total pixel budget.

A real four-page kitchen drawing imported with 608 shapes, 48 groups and 142 layers and was visually compared with Poppler rendering. A 28-artboard Illustrator document exposed blank PDF-compatible pages; an independent PDF renderer also showed them as blank. Artwork present only in Illustrator's private data cannot be recovered through the PDF path. Independent export checks cover hidden optional-content layers, raster alpha, masked gradients, shadows and group opacity. The page fallback for backdrop-dependent blending retained the exact native composite pixels and rendered pixel for pixel with Poppler's Cairo backend; a separate Splash-backend check covers layers and alpha.

Legacy PostScript Illustrator, EPS and PS files use an installed Ghostscript executable to create PDF before native import. The process uses a private temporary directory and bounded runtime and output; missing Ghostscript produces setup guidance.

## OpenRaster

OpenRaster import/export uses the public 0.0.6 ZIP/XML specification. PNG layers stay separate, including hidden layers and negative offsets. Groups preserve hierarchy and isolation. Imported pass-through group opacity is applied to descendants with a note to preserve OpenRaster's compositing rules. When exporting a translucent native pass-through group, a note explains that overlapping children may composite differently; isolated groups provide consistent interchange. Layer locks use an Omadesign XML extension that other applications may ignore. Unsupported Porter–Duff compositing and extended non-PNG layer content are rejected instead of silently changed to Normal blending. Sixteen-bit PNG channels become 8-bit RGBA with a note.

Export renders vector artwork separately for each layer; masked/effected groups become one pixel layer. Rendered layers are clipped to the document canvas. The archive includes the required first, uncompressed `mimetype` entry, `stack.xml`, `mergedimage.png` and a thumbnail no larger than 256×256. ZIP entries are read directly without filesystem extraction. Archive, expanded-data and aggregate pixel budgets are 512 MiB; canvas size is limited to 32,768 pixels per side and 64 megapixels.

Verification includes a fixture generated independently with Python's ZIP, PNG and compression primitives, plus independent Python `zipfile`, XML and Pillow inspection of Omadesign's exported archive. These checks cover order, nested groups, hidden layers, exact pixel channels, negative offsets and required previews.

## Headless inspection and conversion

The command line uses the same readers and writers as the desktop. Inspection writes JSON with canvas dimensions, pages, layer metadata and import notes. Conversion prints notes to stderr and writes a separate destination file:

```sh
omadesign --inspect artwork.afdesign
omadesign --inspect artwork.psd
omadesign --convert artwork.afphoto --output artwork.oma
omadesign --convert artwork.oma --output artwork.psd
omadesign --convert artwork.oma --output artwork.pdf
omadesign --convert artwork.oma --output artwork.ora
```

Headless output formats are `.oma`, `.svg`, `.png`, `.jpg`/`.jpeg`, `.psd`, `.psb`, `.pdf` and `.ora`. Same-file conversion is refused to protect the original. General source-file size is limited to 512 MiB; specific readers also enforce object, pixel, decompression or process limits.

Primary references: [Affinity V3 converter source](https://gitlab.com/inkscape/extras/extension-afdesign/-/commit/cd5cf29d5df22e07b1e9209219079ca44015b7fe), [OpenRaster archive specification](https://www.openraster.org/baseline/file-layout-spec.html), [OpenRaster layer-stack specification](https://www.openraster.org/baseline/layer-stack-spec.html).
