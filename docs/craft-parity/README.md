# Craft workflow parity

The original comparison selected 25 gaps and eight deeper investigations. It was not an exhaustive feature inventory. The rejected nightly and its human QA report remain part of the record. The workspace correction in #332 provides working docks and native editing paths; it does not establish full parity.

Run `node scripts/craft-parity.mjs REFERENCE_REPOS OUTPUT_JSON` against the pinned local reference checkouts to enumerate command declarations and pane source files. Keep generated source evidence under `.artifacts`. Every emitted entry starts unmapped. Runtime-built commands, tools, menus, file formats and complete workflows require inspection beyond that catalog.

## Current implementation layers

| Layer | Working scope | Remaining scope |
| --- | --- | --- |
| Workspace, #332 | Theme-derived chrome, one mode control, actual dock nesting/splits, recoverable sizing, Layers thumbnails/footer, ratio lock, native paste | Full panel and workflow parity |
| Color workflows, #333 | 21 harmony choices, compact variation grid, five-color themes, linked/custom wheel edits, atomic file-backed theme library, guarded swatch saving, explicit recolor mappings | Pattern-internal recoloring, CMYK/spot-color models, color reduction, global swatch linkage and recolor preview remain open |
| Actions, #341 | Actual recording of initial vector command set, durable libraries, atomic playback/rollback and recording recovery | Wider commands, selection/parameter editing, pixel/photo/motion actions and dedicated import/export |
| Styles, #339 | File-backed appearance presets, rendered previews, atomic multi-object apply | Linked document styles, additive appearances, character-color override and dedicated import/export |
| Channels | Bounded composite-channel viewing | Channel selection, editable alpha/spot channels, native channel operations |
| Symbols, #335/#337 | Linked vector creation/place/edit, independent copies, select instances, batch reset/break, rename and source-fitted thumbnails | Hidden library definitions, cross-definition replacement, alternate-artwork redefinition, external libraries and symbolism tools |
| Export, #343/#347/#349 | Document-owned live asset collection, shared fractional/width/height/PPI raster rows and genuine density tags, native Undo/cold reopen, actual PNG/SVG export and collision-safe publication; painted strokes, curve extrema and ordinary effect padding enter the crop | Presets, extra formats/settings, bleed/subfolders, SVG graph overflow bounds and drag collection remain open |
| Other vector panes | Existing type, path, appearance, paint, effects and library bodies | Attributes, CSS properties, document info, flattener/separation previews and remaining reference commands |
| Photo, Layout, Motion | Existing source and exact-source qualified work retained | Complete reference workflows still need item-by-item reconciliation and implementation |

An item closes when its real editing path works, has the necessary Undo and native save/reopen behavior, and has been exercised at the appropriate boundary. A missing reference file, disabled control, unsupported operation or unrun test remains open. Historical passes stay bound to their source; they do not certify a later union.

The current live QA window stays open while implementation continues. New binaries use separate directories so the running QA build and user documents remain intact. Version preparation and public release remain separate from this work.
