# Asset Export

Asset Export collects live artwork references into the document. Collect individually creates one asset per selected object; Collect as one retains their shared scope. The collection does not flatten, move or duplicate artwork. Rename and removal use ordinary native Undo. Deleted targets remain explicit missing references so Undo can restore them without silently changing an asset's identity.

The same pane exports selected assets (all when none are selected), checked artboards or the complete document. Each saved format row has a stable ID, format, scale and filename suffix. Prefix and rows stay with the project. The destination folder is local UI state. Export automatically accepts valid pending settings, captures an immutable source and uses the existing admitted crop/selection and format writers in a cancellable worker. Changes to the source cancel that worker.

All files are staged in the chosen folder before publication. Existing names and duplicate row outputs refuse the batch. Publication uses no-replace hard links; an error removes the operation's published links and staging files while preserving an independently created destination. Filesystems without hard-link support refuse publication. No existing output is intentionally replaced.

Nondefault collection/settings require native project feature 29. Current, embedded and retained values use the shared feature scanner. Empty/default documents preserve the prior wire. Live's existing unknown-document-property policy refuses this metadata; the app blocks metadata edits in Live before mutation. This does not advertise a new Live capability or change a protocol/backend.

The initial row sizes are integer 1×–8× for PNG/JPEG/TIFF; layered/vector rows use 1× and retain their existing writer precision/refusal rules. Stroke paint and ordinary effect padding enter the crop. SVG filter graph overflow regions remain a named asset-bounds refusal; document/artboard export keeps its existing behavior. This is a workflow foundation, not complete Export for Screens parity. Fractional/width/height/PPI rows, PNG8/WebP/GIF, format-specific settings, bleed, presets, subfolders, drag-to-collect, reorder and asset import/export remain open. Existing rendering, format, Motion/native precision and other-platform proof boundaries remain attached.

Source reference: the pinned Vectorcraft `panels/asset_export.rs`, `dialogs/export_for_screens.rs` and engine `cmd/fileio/screens.rs`. Tests and actual native execution are recorded separately from this implementation description.
