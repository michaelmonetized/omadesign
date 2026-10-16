# Illustrator editing data

Omadesign reads the editable artwork that Illustrator stores separately from its
PDF page. PDF-compatible AI and AI saved without its PDF preview use the same
`PieceInfo /Illustrator /Private` reader. Numbered `AIPrivateData` streams are
joined in numeric order; `AIPDFPrivateData` is accepted when the first form is
absent. Plain streams, PDF Flate compression, Illustrator zlib and Zstandard
editing-data compression are supported. EPS private data supports comment-line
ASCII85, zlib and Zstandard. Legacy Illustrator PostScript is parsed directly;
no PostScript program runs.

Native import reconstructs named layers and nested groups, artboards in their
original layout, editable cubic and compound paths, fill winding, clipping
groups, RGB paint, RGB equivalents of CMYK/spot paint, stroke caps/joins/dashes,
linear/radial gradients, visibility, locks, opacity and blend modes. Point type
from the Illustrator text engine and legacy AI type remain editable. Area type
keeps its text and rectangular frame bounds. Embedded raster samples and placed
picture previews are decoded. Local linked pictures are read only within the
source document's directory; their original bytes are retained once as a source
attachment and their placed layers reference that attachment.

Import notes are saved in the native document. Patterns, symbols, meshes,
unsupported operators, live brushes/effects/multiple appearances, unavailable
fonts, irregular/threaded area type, sheared type and mixed ATE font/paint runs
are labelled when encountered. Live appearance definitions remain in the
retained source. Mixed type currently keeps all editable characters with the
first run's font and paint. CMYK and spot paint uses an RGB equivalent; overprint
and named-ink separation are unsupported. These are fidelity limits, not a claim
of full Illustrator feature support. Source recovery is distinct from exporting
new Illustrator editing data after changing native artwork.

The tokenizer, postfix dictionaries and text-engine key mappings are adapted
from [VectorCraft at b12a52f](https://github.com/storytold/vectorcraft/tree/b12a52f7249e0aa73f030587e05ed9ce0c73ab67/crates/eps/src/import).
Its [MIT notice](../src/formats/illustrator/LICENSE-MIT) is retained.

## Verification

Generated tests exercise compressed and uncompressed private streams, numeric
block ordering, AI and AIPDF stream names, preview-independent editable art,
EPS ASCII85, legacy editable text, group hierarchy, multiple artboards,
compound paths, unsupported-feature notes, malformed input, linked source
deduplication and native save/reopen.

The ignored `illustrator_independent_` tests read a directory given by
`OMADESIGN_ILLUSTRATOR_FIXTURE`. They require the
following independent, real Illustrator files. They are deliberately absent
from this repository:

- `Test-Inkscape-LineIssue.ai` and `.svg`, supplied by their author in
  [Ink/Stitch issue 1623](https://github.com/inkstitch/inkstitch/issues/1623).
  The test compares stroke placement and widths against the author's SVG parsed
  directly by usvg, independently of Omadesign's SVG import conversions.
  The attachment has no explicit redistribution license.
- `font_template.ai` from
  [fontpixel/font-template at 6b613e2](https://github.com/fontpixel/font-template/blob/6b613e2d73d7189b754db04fb5c62e198594d882/font_template.ai),
  under the repository's MIT license, copyright Tom Chen. It contains 208
  artboards and 416 text-engine stories.
- `reuters-dynamicText.ai`, downloaded as `dynamicText.ai` from
  [Reuters ai2svelte at ba5675c](https://github.com/reuters-graphics/ai2svelte/blob/ba5675cc4378acf77902709c08b744ca5796a052/public/examples/dynamicText.ai).
  Its text engine contains 107 stories. The root README names ISC while the
  subdirectory's license is MIT; this fixture is used only for local, read-only
  QA without assuming redistribution rights.
  Its editing data has five artboards (`xs`, `sm`, `md`, `lg`, `xl`); the single
  PDF page has the `xl` crop size but contains Adobe's no-PDF-content notice.
  The test checks all five names and placements, then compares an exact multiset
  of 104 live story texts against independent qpdf/Node ATE extraction. It keeps
  paragraph breaks, blank lines and duplicate text occurrences; three orphan
  stories and 61 undo records are not active artwork. The reusable extractor is
  `node scripts/inspect-ai-live-text.mjs INPUT.ai OUTPUT.json [POPPLER-TEXT.html]`.

The test removes every PDF page's `Contents`, confirms identical editing data,
and compares the resulting native geometry, paint and artboards with the
original import. It requires actual editable text for both text fixtures and
checks native save/reopen. The generated and independent real-file checks are
separate. Passing software checks does not establish a native Illustrator
opening/saving session, human visual acceptance, or a new Illustrator-data
writer roundtrip.

The font fixture checks words from the independent PDF artwork against native
characters; it does not compare per-story multiplicity or font metrics. The
Reuters multiset checks exact characters and repeated bodies, without proving
text placement or painter order. The author's SVG checks stroke bounds and
widths, without proving all colors, dashes, caps, joins or gradients. Real layer
names, parentage and painter order still need an independent source oracle;
the generated hierarchy test covers the controlled layer/group/path case.

Each real fixture also exports its first artboard after OMA reopen. Independent
usvg/resvg and qpdf/Poppler checks require valid dimensions and visible SVG/PDF
artwork in previews capped at 320 pixels. These checks cover portable output
from the reopened native artwork, with source-bound receipts and export notes.
They do not compare every artboard or establish pixel fidelity to Illustrator.

Shared SVG type can replace the fallback only when it preserves the source
characters and paragraph breaks exactly. Otherwise the editable ATE text stays
available and a saved diagnostic identifies the fallback layout.

## Native pixel integration

On the native common stack, embedded byte images and saved previews admit their
storage before allocation. A recognized linked image's decoding or allocation
error stops import instead of choosing its saved preview or PDF/PS fallback.
Unreadable or unsupported link formats still receive the existing preview note.

Reflections use admitted independent storage for shared byte/native buffers and
preserve authored samples, CMYKA, profile and explicit intent. Sheared native
images use the existing shared bilinear sampler in authored precision; CMYK
components are packed into shared pixel views for that sampler and converted
back through the common CMS API. The byte-only shear route retains admitted
surfaces through its fallible conversion. Source UUIDs and original bytes remain
independent of these display/placement buffers. The shear reduction is still
reported in import notes.

New regressions cover native/byte reflection isolation, HDR/profile/CMYKA shear,
16-bit linked-image save/reopen and refusal of a damaged recognized link. They
are source-only until the single combined integration check is authorized and
completed. Original independent AI and strict native Affinity assertions remain
unchanged. See [native common integration](affinity-native.md#native-common-source-integration)
for the exact prerequisite and proof boundaries.

The first combined run at `27791b5db7dd70d19f52d873bb834eb169090fa1`
finished with 24 passed and one Reuters failure at the unchanged exact
preview-independent artwork comparison. The strict 15-file Affinity gate and
all three new pixel regressions passed at that revision. Retained SVG type IDs
were generated from process-wide layer numbers; the adapter now uses source
slot order for stable text/frame IDs across imports. This correction awaits
qualification; the initial run is not a complete integration pass.
The exact shape/style comparison now reports the first differing shape, JSON
field and bounded value context instead of dumping entire documents. It still
compares all raw geometry and style values, including retained SVG identities;
no IDs, fields or thresholds are excluded from equality.
