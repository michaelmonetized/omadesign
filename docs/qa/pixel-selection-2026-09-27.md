# Pixel selection dialogs — 2026-09-27

Implemented in the primary `/home/michael/Projects/omadesign` checkout.

Pixel mode's Select menu now offers All pixels, Deselect, Invert pixel selection,
Move, Resize, Grow, Shrink, Feather, and Reshape. The six editing actions open
dialogs for completed marquee, ellipse, and lasso selections. Reshape exposes
rotation and horizontal/vertical skew. Resize supports keeping proportions.

Dialogs render asynchronous previews from the original selection. Apply commits
coverage; Cancel, Escape, and closing the window discard it. Background editor
input is disabled during the dialog. Changed documents or selection generations
invalidate pending results. Distances are native image pixels, and selection
coverage clips at image bounds.

Feathered coverage now blends brush, fill, eraser, clone, heal, and smudge results
with the original pixels. Accumulated stroke buffers remain unfeathered to avoid
repeated attenuation. Raster filters map selection coverage into their target
layer's coordinates.

Validation:

- `cargo test --lib --locked --offline`: **614 passed, 0 failed, 5 ignored**.
- Tests cover all three selection shapes, transformed image coordinates,
  growing/shrinking and image boundaries, zero/large radii, partial coverage,
  invalid transforms, menu/dialog interactions, Apply/Cancel, stale sessions,
  and feathered paint/fill/eraser behavior with pixel undo.
- `cargo check --all-targets --locked --offline`: passed.
- `cargo fmt --all -- --check` and `git diff --check`: passed.
- `cargo build --bin capture_studios --bin omadesign --locked --offline`: passed.
- Local debug app SHA-256:
  `87b06e69cd13f83fdfb396ed3213cca4caead063c88efb059ab91c1c1a9b8fab`.
- Native Wayland/WGPU capture: **660 frames, 22 seconds, 0 unresolved input
  targets**. Created an ellipse selection through the canvas, opened all six
  dialogs through Select, applied five, and canceled Reshape. Inspected native
  screenshots of Move, Resize, Grow, Shrink, Feather, and Reshape controls.
  Evidence, recording, screenshots, and validation logs are retained in
  `target/qa/pixel-selection-2026-09-27/`.

No installed application or release was replaced. The accepted 0.5.0 human-QA
record in `AGENTS.md` remains unchanged; these automated checks are not a new
human-QA signoff.
