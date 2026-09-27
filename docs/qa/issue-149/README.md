# Issue 149 native acceptance

[Full native E2E recording (60 seconds)](issue-149.mp4)

Recorded on 2026-09-27 from the actual stacked PR source through the character-spacing changes. The circle and open guide are seeded; all recorded text creation and edits use the production native UI. The recording contains 600 continuous 1600×900 WGPU viewport frames, encoded at 10 fps using deterministic input time.

| Time | Native workflow |
| --- | --- |
| 0–12 s | Click the ellipse with the Text tool, type along it, flip, drag the start bracket around the closed path, undo and redo. |
| 13–22 s | Type on an open Bézier curve, edit its end node, undo and redo the live reflow. |
| 23–35 s | Release and undo; native Ctrl+C followed by Edit → Paste for text alone, then text plus both guides; check released text and remapped links. |
| 38–46 s | Baseline shift, curve spacing, Baseline / Center / Top / Bottom alignment, end offset and overflow. |
| 47–55 s | Release, select text plus guide, attach, delete the guide while retaining its text, and undo the exact guide/link state. |
| 56–60 s | Ctrl+S, reopen the saved project in the application, and verify both editable path links. |

`result.json` contains the native assertions, `saved.oma` is the Ctrl+S document, and `native.png` is the reopened native viewport. `export.svg` preserves the placed glyph outlines; `export.png` is its independently rasterized view for comparison with the canvas. The native clipboard payload is produced by the operating-system copy event and read by the application's Edit → Paste action.

Regression coverage includes straight and circular arc placement, reversed tangents, every supported primitive and the first compound contour, guide edits/deletion/undo, linked versus standalone copies, range tracking plus curve spacing, paragraph alignment, and save/load/export contour equality. The final combined geometry suite passed 26 tests; all 15 clipboard tests passed, including the path payload regression.

Reproduce with `cargo build --locked --offline --bin capture_studios`, then `target/debug/capture_studios path-type /tmp/issue-149-qa --fps 10` in a graphical session. The harness uses isolated XDG profile directories. `SHA256SUMS` records the capture binary and evidence hashes. Existing accepted 0.5.0 human QA evidence is preserved.
