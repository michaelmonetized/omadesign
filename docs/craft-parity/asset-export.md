# Asset Export

Asset Export collects live artwork references into the document. Collect individually creates one asset per selected object; Collect as one retains their shared scope. The collection does not flatten, move or duplicate artwork. Rename and removal use ordinary native Undo. Deleted targets remain explicit missing references so Undo can restore them without silently changing an asset's identity.

The same pane exports selected assets (all when none are selected), checked artboards or the complete document. Each saved format row has a stable ID, format, scale and filename suffix. Prefix and rows stay with the project. The destination folder is local UI state. Export automatically accepts valid pending settings, captures an immutable source and uses the existing admitted crop/selection and format writers in a cancellable worker. Changes to the source cancel that worker.

All files are staged in the chosen folder before publication. Existing names and duplicate row outputs refuse the batch. Publication uses no-replace hard links; an error removes the operation's published links and staging files while preserving an independently created destination. Filesystems without hard-link support refuse publication. No existing output is intentionally replaced.

Nondefault collection/settings require native project feature 29. Current, embedded and retained values use the shared feature scanner. Empty/default documents preserve the prior wire. Live's existing unknown-document-property policy refuses this metadata; the app blocks metadata edits in Live before mutation. This does not advertise a new Live capability or change a protocol/backend.

The initial row sizes are integer 1×–8× for PNG/JPEG/TIFF; layered/vector rows use 1× and retain their existing writer precision/refusal rules. Stroke paint and ordinary effect padding enter the crop. SVG filter graph overflow regions remain a named asset-bounds refusal; document/artboard export keeps its existing behavior. This is a workflow foundation, not complete Export for Screens parity. Fractional/width/height/PPI rows, PNG8/WebP/GIF, format-specific settings, bleed, presets, subfolders, drag-to-collect, reorder and asset import/export remain open. Existing rendering, format, Motion/native precision and other-platform proof boundaries remain attached.

Source reference: the pinned Vectorcraft `panels/asset_export.rs`, `dialogs/export_for_screens.rs` and engine `cmd/fileio/screens.rs`. Tests and actual native execution are recorded separately from this implementation description.

## Fractional and physical sizing

Raster rows accept `0.5x`, `1.5x`, `512w`, `512h` and `300ppi` (case-insensitive). Width and height preserve the source aspect ratio with whole-pixel rounding; a positive result smaller than one pixel becomes one pixel. PPI scales document pixels by requested PPI / document PPI, preserving physical size. SVG and layered rows retain their native 1x route.

PPI records actual PNG pHYs, JPEG JFIF and TIFF resolution tags. PNG uses integer pixels/metre, JPEG uses integer PPI, and TIFF uses a four-decimal rational; codec quantization is reported in output notes. Float TIFF, retained CMYKA, ICC and straight-alpha behavior use the existing admitted encoders. No byte-preview resize or alternate compositor is introduced.

Unchanged integer rows keep the exact native feature-29 wire. New sizing metadata requires native feature 30, including embedded documents and retained/deleted/withdrawn collaboration values. Live metadata editing remains unavailable under the existing fail-closed policy. Application versioning is separate and unchanged.

Qualified production is `7cda60b2`: the initial 42 passes at818484ab, three corrected passes ata7ea1e37 and two final gate passes at7cda60b2 remain separate. The original failures are retained. A genuine7c desktop build exercised invalid input, actual clipboard paste, fractional/PPI native export, one full-document Undo/Redo and cold reopen/resave. Independent readers verified the three actual native PNG/SVG files plus seven small density/native-precision outputs. No human acceptance or full-parity claim. Extra formats, per-format compression/quality controls, presets, bleed, subfolders and drag-to-collect remain open.
