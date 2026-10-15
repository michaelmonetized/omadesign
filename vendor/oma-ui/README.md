# Oma UI contract 1

Canonical source: michaelmonetized/omadesign, `vendor/oma-ui`.

These small source modules support egui 0.32 and 0.36 without making the
applications share a dependency version, theme, document model or command bus.
Apps check in exact copies; `node scripts/check-shared-ui.mjs [other repo roots]`
checks hashes against the canonical manifest, including every supplied copy.
Changes start in Omadesign and must update all consumers and the manifest.

Revision 2 restores hidden floating tabs in their saved relative order regardless
of restoration order. Earlier cuts retain revision 1 and its exact test receipts.
Check matching historical copies with
`node scripts/check-shared-ui.mjs --at CANONICAL_ROOT FULL_SHA CONSUMER_ROOT FULL_SHA`.
The default command compares the current top copies; pinned mode compares only
the explicitly selected canonical revision and consumers.

Revision 3 adds context-owned, one-use native focus and floating size requests.
Call request_focus(ctx, workspace_id, panel) after normalizing the known registry;
it returns focus to the native tab when the logical panel has not changed.
Delivery waits until both the tab and its native layer accept interaction, so
a closing modal cannot consume a request before the background tab can focus.
request_floating_size accepts a visible floating panel and finite positive size.
show revalidates requests and uses Window.fixed_size for the requested frame,
then returns to ordinary resizing. Window IDs and saved Group.rect remain stable.
Book.toolbar_for delivers explicit requests; toolbar remains compatible.
Requests never enter serialized preferences or artwork history.

Saved floating rectangles are outer window footprints. egui 0.36 takes that size
directly; an egui 0.32 app enables the oma-ui-egui032 compile feature to subtract
its public frame/title margins. Omadesign and EditBay leave that feature disabled.
All consumers retain identical canonical source and manifest bytes.

`fields::native_scope(ui, absolute_id, body)` preserves native control identity
when earlier conditional rows change. It uses egui 0.36's absolute child scope
and egui 0.32's public Ui constructor with inherited presentation and allocation.
It is for native controls; the application retains their precision and behavior.
Fields and panel tabs share this small adapter in `native_scope.rs`.
Error associations use the public native ID value on both versions. Numeric
focus uses each version's supported focus lock; the explicit pending-direction
reset is enabled only on egui 0.36, which exposes that public API.

`fields::number` accepts a stable ID, units, optional reset and mixed/locked/error
state. It returns change/focus/commit/cancel signals; applications decide when to
preview and commit typed commands. A field never owns undo or a document.
The native parser uses egui's whitespace/minus grammar and rejects nonfinite
drafts before conversion. Invalid drafts and application errors have one visible
associated message and polite live semantics. Names, mixed state and focus use
the actual widget. FieldResponse.changed is authoritative after rejection or
cancellation; the native response remains available for app-owned history.
`number_is_valid(&field)` lets an existing dialog disable Apply for a rejected
native draft even when egui retains the previous numeric value. It also observes
the supplied application error; it never substitutes for `changed` when editing.
`action`, `tab` and `section` return normal egui responses/results.

Workspace panels have application-owned IDs. The canvas is outside the layout
registry: panel commands cannot move, close or obscure it with a dock. Floating
windows are constrained to the current logical viewport on every frame.

All code here is original MIT-licensed Oma code. Pinned craft-libs MIT sources
were read as design references; no Craft source or assets were copied.
