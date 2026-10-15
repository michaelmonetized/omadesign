# Oma UI contract 1

Canonical source: michaelmonetized/omadesign, `vendor/oma-ui`.

These small source modules support egui 0.32 and 0.36 without making the
applications share a dependency version, theme, document model or command bus.
Apps check in exact copies; `node scripts/check-shared-ui.mjs [other repo roots]`
checks hashes against the canonical manifest, including every supplied copy.
Changes start in Omadesign and must update all consumers and the manifest.

`fields::number` accepts a stable ID, units, optional reset and mixed/locked/error
state. It returns change/focus/commit/cancel signals; applications decide when to
preview and commit typed commands. A field never owns undo or a document.
`action`, `tab` and `section` return normal egui responses/results.

Workspace panels have application-owned IDs. The canvas is outside the layout
registry: panel commands cannot move, close or obscure it with a dock. Floating
windows are constrained to the current logical viewport on every frame.

All code here is original MIT-licensed Oma code. Pinned craft-libs MIT sources
were read as design references; no Craft source or assets were copied.
