# Native Affinity import

Open `.af`, `.afdesign`, `.afphoto`, `.afpub`, or the document variants through File → Open. The Rust reader reads the native archive and tagged object stream without installing or launching a converter. Supported objects become editable paths, groups, type, paint, image layers, artboards and live masks through the shared artwork importer.

Original bytes are stored once in the OMA project with a SHA-256 and content-derived UUID. Objects reference that UUID. File → Save original source… recovers the exact original container, including fields that Omadesign does not understand. Saving OMA preserves editable artwork and original data; it is not an Affinity writer. Existing OMA files default to no retained sources. The original isolated cut used provisional schema 14. The integrated common stack assigns retained sources to feature 18, reads versions 1–23 and writes version 23; release integration selects schema changes centrally.

If native parsing fails and the separately installed converter exists, import can use it with an explicit diagnostic. No converter is installed by opening a document. Unsupported layer kinds, brush/pressure strokes, color profiles, publishing structures and effects produce conversion notes. Embedded documents and symbols currently use cached images. Exact rich text and affine paint depend on the shared SVG type and paint changes in the Craft stack. Original data retention does not make an unsupported feature editable.

## Qualification

The pinned fixture manifest is `tests/fixtures/affinity-native.json`. Fetch it with:

```sh
node scripts/fetch-affinity-fixtures.mjs /home/michael/Projects/omadesign/.artifacts/affinity-fixtures
```

Only public MIT and CC0 files are listed; files stay ignored. The fixtures are actual Affinity V1/V2/V3 documents (container versions 9, 11 and 12), pinned and SHA-256 verified against the source reader's upstream manifest. The test `native_affinity_real_files_preserve_editable_artwork_and_original_bytes` requires `OMADESIGN_AFFINITY_FIXTURES` to point to that directory. Run Cargo through the program's serialized helper, on a committed revision. It checks editable feature retention, artwork hierarchy, exact original-source bytes, native OMA encode/reopen stability, and independent SVG parsing/reimport. It writes a concise JSON report beside the ignored fixtures.

These checks do not prove reopening SVG/PDF in Affinity, physical display output, installed desktop interaction, or compatibility with every document version/platform. The vendored reader's MIT notice, pinned source and upstream provenance are in `vendor/affinity/`.

The effects cut reconstructs observed `Shad`, `Gaus`, `ColO`, and `OutG` records as editable drop shadow, Gaussian blur, color overlay and outer glow. It retains opacity, color, blend, radius, offset, angle, scaling and knockout where available. Intensity profiles, preserve-alpha Gaussian mode, outlines, bevels and unrecognized effects have explicit diagnostics. Inner-shadow import has no qualified source fixture and is not claimed. Rendered effect semantics still require comparison with the Affinity-saved thumbnail and independent PDF output; a parsed effect is not a visual-fidelity receipt.

Source retention follows layered and frame placement and participates in the same undo step as its artwork. Dropping a second object referencing the same UUID does not duplicate its source data.

## Native common source integration

The adapter integration starts from qualified native common
`2e5709463e196d2a0e6da80aead1bc087cb2a545` and retains its shared source,
text, paint, native pixel, publishing and allocation APIs. The complete original
23 adapter commits are replayed in order; the four original PRs and their
commit-specific receipts remain frozen. The integration itself awaits its
combined Illustrator and strict 15-file Affinity check. Source checks do not
promote the historical receipts to this new revision.

The image bridge borrows the vendored reader's encoded/raw image data, writes
PNG through the common admitted encoder buffer and retains a conservative
base64/XML staging reservation through SVG import. The existing bounded vendor
decoder and original `SourceAttachment` representation are unchanged. Their
metadata, archive and original-byte allocations are outside a complete shared
pool accounting claim; codec scratch estimates are not measured peak memory.

The only added lock package is the local MIT `omadesign-affinity` 0.1.0 reader;
its dependencies already exist in the common lock. The common notice inventory
is retained additively and its lock hash is refreshed by a pure Node inventory.
Both reader MIT notices match the pinned VectorCraft root notice byte for byte
(SHA-256 `8b6025dc4b707e0fb08b9c2054096f1be3ae61529399aa2c348dec676562dc71`).
No registry dependency or application version is changed.
