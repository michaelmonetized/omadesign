# Selection handles, Bézier lasso, and provider discovery — 2026-09-27

## Source

Implemented in the authoritative `/home/michael/Projects/omadesign` checkout,
branch `feat/live-design-agent-selection`, on 0.6.0. A fresh fetch confirms
`origin/master` is `a4d5b2faa2df976bfad164658a3732fd058d78d6`; this branch contains
that complete base. The bottom toolbar asset pickers and upstream zoom placement
remain present. Unrelated artwork, site files, and other worktrees were preserved.

This follow-up supersedes the transform-dialog behavior documented in
[the initial harness report](agent-harness-2026-09-27.md).

## Selection interaction

- **Select → Move / resize selection**, or **Ctrl+T** in Pixel mode, gives four
  corner handles. Drag inside to move; drag corners to resize; Shift preserves
  proportions. Only selection coverage changes, not image pixels.
- **Select → Reshape** offers the same modes as Object → Reshape: Distort, Skew,
  Perspective, and Warp mesh. The first three expose four corners; mesh exposes
  its nine control points.
- Grow, Shrink, and Feather retain their live-preview dialogs.
- **Bézier lasso (Shift+Q)** draws a closed selection from editable cubic curves.
  Click to add sharp nodes; drag while adding a node to give it tangents; click
  the first node or press Enter to close. The completed selection has marching
  ants around its real boundary, including holes.
- Shift-click an edge inserts a node without changing the curve. Alt-click a
  node deletes it, retaining at least three nodes. Ctrl-click toggles sharp and
  curved nodes. Drag nodes or tangent handles; Alt-drag a tangent breaks its
  mirrored counterpart. Ctrl+Z / Ctrl+Shift+Z traverse local selection edits.
- Enter finishes; Escape cancels. Select → Edit Bézier selection reopens stored
  nodes, while New Bézier selection starts another path. Completed nodes survive
  tab switches and restart snapshots. A new selection replaces the stored path.
- Mask rendering and contour tracing run off the UI thread. Stale results cannot
  replace a newer selection or a different document's selection.

## Agent picker

The composer now has a provider/model picker and adjacent effort control. The
picker has a provider rail, search, favorites, and agent-supplied model groups.
Models, effort levels, modes, and other settings come from ACP session metadata;
model catalogs and effort lists are not hardcoded.

Discovery checks local executables and cached ACP adapters, then initializes
short-lived ACP sessions without generating a response. It refreshes every five
minutes while the panel is open and supports manual refresh. Missing executables,
missing adapters, sign-in requests, probe failures, ready sessions, and the active
connection have separate UI states. Discovery does not download an adapter or
read credential files; adapter setup is an explicit connection action.

Saved model/effort choices are scoped to the provider. Configuration requests are
serialized and acknowledged before a queued prompt starts. Full ACP configuration
updates replace dependent controls. Favorites and settings persist alongside
conversation storage, including protection against older background saves.

A successful ACP session proves the harness can initialize, not that every model
in its catalog has paid quota or usable third-party credentials. Provider-specific
errors remain visible. OpenCode can advertise models for multiple services.

Protocol references: [session configuration options](https://agentclientprotocol.com/protocol/v1/session-config-options)
and [authentication](https://agentclientprotocol.com/protocol/v1/authentication).

## Verification

Evidence root: `target/qa/selection-handles-providers-2026-09-27/`.

- Full Rust library suite: **679 passed, 0 failed, 5 ignored**; all targets check
  offline and the application plus both native QA executables build locally.
- Pointer-driven tests cover closing the lasso, inserting/deleting/converting
  nodes, dragging nodes, undo, cancellation, selection holes, all reshape modes,
  native pixels remaining unchanged, tab switching, and stale worker rejection.
- Picker tests cover compact-window visibility, vertically arranged choices,
  advertised effort values, and keeping the popup open while filtering providers.
- The ACP wire test deliberately delays configuration acknowledgements and rejects
  overlapping setting changes or a prompt sent before both settings are accepted.
- Native WGPU/Wayland selection capture: **480 frames, 0 unresolved targets**.
  State snapshots confirm four nodes → five after insertion → moved node at
  `(150, 340)` → four after deletion. The recording exercises ellipse move/resize,
  Distort, Feather, and Bézier drawing/node editing through actual UI events.
- Live discovery: **Codex, Claude, and OpenCode ready; Gemini sign-in required**.
  A real Codex session accepted its advertised `gpt-6-astra` model and a change
  of `reasoning_effort` to `low`. Other providers were capability-probed only.

- Native provider picker: **300 frames, 0 unresolved targets**. The recording
  opens the picker, filters Claude and Codex, selects **6 Sol**, connects the
  session, and changes effort to **High**. `picker-verified/picker.png` shows
  the connected model list; the MP4 records the actual interactions.
- A fresh live **6 Sol / High** design turn completed **15 native edits** and
  produced **12 editable objects**, with no errors, in 154 seconds. Sixteen
  screenshots, the editable `.oma`, PNG export, transcript, settings, and result
  JSON are in `live-design/`. The agent inspected snapshots and refined its
  typography through native tools.

The selected-model live-design run used the same harness and setting queue as
this delivery; its picker predated the final keep-open behavior fix. The final
picker behavior was separately checked in `picker-verified/`. Earlier failed
picker captures are diagnostic artifacts, not acceptance evidence.

See `build-manifest.json` for local artifact hashes and validation logs beside it.
The final debug application SHA-256 is
`67cf0f1409acc30eb7e8b8f85317da303031f32dfdd7d0ef6404d9e9fd3a66c3`.

The installed app and public release have not been replaced. The accepted 0.5.0
human-QA evidence in `AGENTS.md` remains unchanged.
