# File formats

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
| `.xcf` | Native GIMP reader: pixel layers, groups, names, visibility, opacity, offsets, supported blends and applied layer masks | No XCF writer; export OpenRaster or PSD | Live text, layer effects, paths and floating selections are not reconstructed. High bit depth becomes 8-bit RGBA. Unmapped blend modes display as Normal. Save `.oma` to keep the imported stack. |
| `.eps`, `.ps` | Ghostscript conversion to PDF, then native PDF import | No direct writer | Ghostscript must be installed. Conversion can discard original layer metadata or flatten artwork. |
| `.omaphoto` | Reopens the matching original in Photo with saved development, crop and rotation | Save settings beside the original | Contains settings, not pixels. Keep the original and sidecar together with matching names; the source size and modification time must still match. |
| `.omapreset` | Photo preset library Import | Photo preset library Export | Source-independent, named adjustment collections in versioned JSON. No image pixels or source identity. Crop and rotation are opt-in. |
| Camera RAW: DNG, CR2/CR3, NEF/NRW, ARW, RAF, ORF, RW2, PEF and others | Full-resolution sensor decoding and Photo development | Developed PNG/TIFF with 16-bit channels, or JPEG; no camera RAW writer | Support depends on the camera and compression mode. Lens corrections, proprietary camera looks and unsupported DNG opcodes/profiles are not recreated. Design placement becomes 8-bit pixels. |
| PNG, JPEG, WebP, GIF, BMP, TIFF | Pixel image | PNG and JPEG through headless conversion; other existing export actions remain separate | This path imports a single image, not layered TIFF or animated GIF structure. |

## Camera RAW

RAW decoding is built into Omadesign using the pinned [LibRaw 0.22.2 source](https://github.com/LibRaw/LibRaw/tree/0.22.2). It does not require an installed converter or a download when opening a photo. The decoder reads sensor data at full resolution, subtracts black levels, demosaics supported Bayer/X-Trans sensors, applies the camera white balance and color matrix, and honors image orientation. It returns 16-bit linear sRGB with automatic brightness disabled. DNG baseline exposure and the Photo exposure/white-balance adjustments are applied before the display transfer function.

Recognized extensions are `.3fr`, `.arw`, `.bay`, `.cap`, `.cr2`, `.cr3`, `.crw`, `.dcr`, `.dcs`, `.dng`, `.drf`, `.erf`, `.fff`, `.iiq`, `.k25`, `.kdc`, `.mdc`, `.mef`, `.mos`, `.mrw`, `.nef`, `.nrw`, `.orf`, `.pef`, `.ptx`, `.pxn`, `.raf`, `.raw`, `.rw2`, `.rwl`, `.rwz`, `.sr2`, `.srf`, `.srw`, `.sti` and `.x3f`. An extension identifies a family; it does not establish support for every camera or compression mode. JPEG XL-compressed DNG, GPR, EIP packages and R3D video are not supported by this build. Only the first image of a multi-image RAW is developed, with a conversion note.

Open a RAW through **File → Open**, the Photo library, a folder, a file-manager Open With action, or a drop. Loading and folder scans run in background workers. Photo keeps the original decoded linear pixels separate from its development settings and its 1600-pixel display preview. Camera/lens information and exposure metadata are shown when present. **Before** shows the default camera-balanced development, not the camera's embedded JPEG. At close zoom, a background worker develops full-resolution detail and the viewer uploads only the visible tiles. The previous preview stays visible while detail is prepared; results from an older photo or adjustment are discarded.

**Save settings** writes a small adjacent file such as `DSC_0001.NEF.omaphoto`. Resume with **File → Open**, a drop, or **Photo → Library → ··· → Open photo or settings…**, choosing either the original or the settings file. Opening `.omaphoto` restores the original source, including RAW precision, and its development settings. The source photograph is never rewritten. Keep the settings file and original together with matching names. Settings are bound to the source size and modification time. An explicit settings open fails without replacing the current photo when the original is missing or changed, or the settings are invalid. Opening the original instead shows default development and a note if its settings cannot be used. Failed saves retain unsaved edits for retry. This metadata check is not a cryptographic content identity. Unsaved changes exist only in the current Photo session. A `.oma` Design document does not contain the RAW source or its development settings.

Photo export develops the full-resolution source, including crop and rotation. RAW PNG/TIFF exports retain 16-bit developed channels; JPEG is an 8-bit delivery image. Exported files use display sRGB values and do not preserve the sensor mosaic, camera edit history, or all source metadata. **Place in Design** and general document conversion produce an 8-bit raster layer; keep the RAW and `.omaphoto` settings for further development. Native RAW writing is not provided.

Rendering is not intended to match Lightroom, Capture One or an in-camera JPEG exactly. Proprietary looks, full Adobe camera profiles, automatic lens correction, some DNG opcodes and multi-frame computational rendering are not implemented. Sensor/channel clipping cannot be recovered by later exposure changes. The decoder rejects images larger than 64 megapixels and inputs larger than 512 MiB, limits LibRaw's sensor unpacking buffers, and requests cancellation through LibRaw's progress callback after 120 seconds. Total working memory also includes decoded pixels and development buffers. The callback is a cooperative limit, not a process deadline.

Real-file verification covers an iPhone 16 Pro Max ProRAW DNG, a compressed Canon EOS R6 CR3 and a compressed Fujifilm X-T30 II X-Trans RAF. These exercise different sensor layouts, compression, camera metadata and orientation. On the validated native build, the decoded 16-bit RGB and full-resolution PNG exports matched independently generated system LibRaw and sRGB-transfer references pixel for pixel. The files cover 3024 × 4032, 3407 × 2271 and 6246 × 4170 pixels. A separate sidecar round trip restored exposure, rotation and crop; its 3024 × 1512 TIFF retained 16-bit pixels and exactly matched an independent development/crop reference, while the JPEG retained the same dimensions. Source SHA-256 checks remained unchanged. These checks do not establish support for every listed camera family or bit-identical output across architectures.

LibRaw is distributed under its CDDL option, with its source and license in `vendor/libraw`. Release packages and the installer retain that source and the native JPEG/zlib notices. Omadesign's own code remains MIT-licensed.

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

## GIMP XCF

The reader follows the [GIMP XCF specification](https://developer.gimp.org/core/standards/xcf/). It does not launch GIMP or write `.xcf`. Pixel layers, groups, names, visibility, opacity, offsets, supported blend modes and applied layer masks become native layers. 32-bit and 64-bit pointers, RLE, uncompressed and zlib tiles are accepted. 8-bit RGB, grayscale and indexed documents are preferred; higher precision is reduced to 8-bit RGBA with a note. Live text, layer effects, paths and floating selections are not reconstructed. Unmapped blend modes display as Normal. Export OpenRaster or PSD for GIMP; keep `.oma` as the editable source.

Verification uses independently encoded fixtures for grouped RLE layers, 64-bit pointers and zlib tiles, plus a bounded oversize rejection.

## Headless inspection and conversion

The command line uses the same readers and writers as the desktop. Inspection writes JSON with canvas dimensions, pages, layer metadata and import notes. Conversion prints notes to stderr and writes a separate destination file:

```sh
omadesign --inspect artwork.afdesign
omadesign --inspect artwork.psd
omadesign --inspect artwork.xcf
omadesign --inspect photograph.NEF
omadesign --inspect photograph.NEF.omaphoto
omadesign --convert photograph.NEF.omaphoto --output developed.tif
omadesign --convert photograph.dng --output photograph.tif
omadesign --convert artwork.afphoto --output artwork.oma
omadesign --convert artwork.oma --output artwork.psd
omadesign --convert artwork.oma --output artwork.pdf
omadesign --convert artwork.oma --output artwork.ora
```

Headless document output formats are `.oma`, `.svg`, `.png`, `.jpg`/`.jpeg`, `.psd`, `.psb`, `.pdf` and `.ora`. RAW inspection includes camera metadata, source dimensions, precision and saved development settings. RAW-to-PNG/TIFF conversion uses the full-resolution 16-bit Photo pipeline; RAW-to-JPEG produces an 8-bit image. Saved `.omaphoto` settings are applied automatically. Converting RAW to a document format renders an 8-bit pixel layer. Same-file conversion is refused to protect the original. General source-file size is limited to 512 MiB; specific readers also enforce object, pixel, decompression or process limits.

Primary references: [Affinity V3 converter source](https://gitlab.com/inkscape/extras/extension-afdesign/-/commit/cd5cf29d5df22e07b1e9209219079ca44015b7fe), [OpenRaster archive specification](https://www.openraster.org/baseline/file-layout-spec.html), [OpenRaster layer-stack specification](https://www.openraster.org/baseline/layer-stack-spec.html).
