# Threaded text integrity follow-up

The original PR167 base (`df325889`) failed four focused checks: inherited OpenType defaults changed after merging; a populated target with incompatible font/size/appearance could be merged; an empty source gained an unintended leading paragraph; and a rotated follower click selected character 103 instead of 98. [Before log](before-regressions.txt) and the [test-only patch](before-regressions.patch) make those failures reproducible. Production source was unchanged in that temporary checkout.

The fix keeps existing file versions and range attributes. Compatible populated frames materialize effective character metrics, features and paragraph defaults at the append boundary. Different run-wide font, size or shape appearance is rejected with instructions to match settings or choose an empty frame; rejection leaves the document, active edit and history unchanged. Pointer selection tracks the visible frame separately from the story-head mutation target and accounts for shape and ancestor rotations. Selection overlays use the same coordinate transforms.

[Native recording](native.mp4) · [Checkpoints](result.json) · [Saved artwork](saved.oma) · [Selection](selection.png) · [Rejected merge](rejected.png)

The native recording uses production egui pointer/key input in a real WGPU window at 1600×900, 10 fps. Only the initial three-frame document is seeded.

| Time | Native workflow and assertions |
| --- | --- |
| 0–4 s | Try to link a target with a different size; verify both stories and history are untouched and the actionable message appears. |
| 5–7 s | Link a compatible populated target with different inherited spacing, features and alignment; verify reciprocal links, full source content and resolved styles. |
| 8–13 s | Select five characters in the rotated follower, click again to collapse selection, then drag backwards; verify exact story indices 179–184. |
| 14–20 s | Replace only that selection with `EDITED`, undo, redo and undo; verify the complete resulting story each time. |
| 21–25 s | Ctrl+S, reopen through the app and verify story content, links and styles. Compare saved/reopened PNG rendering exactly. |

The focused source regressions also cover explicit range overrides, actual shaped-glyph parity before/after merging, manual pair offsets, atomic undo/redo, empty targets, source-head validation when linking from a follower, and pointers under combined child/parent rotation. Unsupported mixed-font/size merging is explicitly rejected, not silently approximated. This follow-up does not change the separate linked-guide group-rotation behavior.

Run `capture_studios area-integrity OUTPUT --fps 10` from the checked-out repository. `receipt.json` identifies the exact source and copied binaries. Existing accepted 0.5.0 QA evidence remains untouched.

Validation: five focused integrity regressions and 21 clipboard tests pass. The area suite initially passed 23 tests and exposed one obsolete version-10 expectation; the populated placeholder targets now correctly require version 12 after their inherited metrics are preserved. Test-only commit `1c7237c7` updates that assertion. The parent integration run verifies this correction with the complete stack. No runtime code changed after the recorded `ccb06e80` build.
