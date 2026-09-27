# Issue 150 native acceptance

[Threading and closed-shape text (47 seconds)](issue-150.mp4) · [Frame, hyphenation and wrap controls (70 seconds)](options.mp4)

Recorded from the stacked area-text implementation in a real native WGPU window. The recordings contain continuous 1600×900 viewport frames at 10 fps with deterministic input time. Text creation and changes use production egui pointer and keyboard events; each scene ends with Ctrl+S and reopening that file in the application. The two scenes use isolated temporary profiles.

| Main recording | Workflow |
| --- | --- |
| 0–8 s | Drag an area frame, type two paragraphs, and check hidden-source overflow. |
| 9–18 s | Thread through three boxes using the out ports, show threads, delete the middle box, and undo. |
| 18–23 s | Move the wrapping circle and resize the first frame without scaling the type. |
| 23–34 s | Set terminal ellipsis, max lines and bottom alignment; replace the whole story from a follower, then undo. |
| 35–42 s | Create editable area text inside the closed circle outline. |
| 43–47 s | Save, reopen, and verify frame, thread, overflow and closed-shape data. |

| Options recording | Workflow |
| --- | --- |
| 0–13 s | Top/Center/Bottom/Justify icon controls and all four frame insets. |
| 14–23 s | Visible, Clip, auto-height, max lines and Ellipsis. |
| 24–28 s | Point/area conversion in both directions and undo. |
| 28–43 s | Enable hyphenation for the first paragraph; edit minimum word length, before/after limits and unlimited consecutive lines. |
| 43–65 s | Bounding-box, actual-shape, inverted, jump and no-wrap modes; offsets, locked-layer and stacking preferences; move and undo. |
| 66–70 s | Save, reopen, and verify paragraph settings. |

The result JSON files contain native assertions. `saved.oma` and `options-saved.oma` are the actual Ctrl+S documents. The native viewport images show the reopened projects; the SVG exports use the same glyph contours, and independent `rsvg-convert` renders are included for visual comparison.

Focused regression coverage includes 27 geometry tests, 15 clipboard tests and the compact-inspector test. Cases cover styled line heights, widow/orphan/keep constraints, consecutive hyphens across frames, grapheme-safe ellipsis, terminal overflow, all vertical alignments, partial versus complete story copying with fresh IDs, head promotion/deletion/conversion, explicit glyph scaling from a follower, nested group and image-alpha obstacles, and closed-shape frames. A word that cannot fit an obstacle-created interval advances to the next usable interval without consuming clipped characters.

Reproduce with `cargo build --locked --offline --bin capture_studios`, then run `target/debug/capture_studios area-type /tmp/issue-150-main --fps 10` and `target/debug/capture_studios area-options /tmp/issue-150-options --fps 10` serially in a graphical session. `validation.json` and `SHA256SUMS` identify the source, binary and evidence. Existing accepted 0.5.0 human QA evidence is preserved.
