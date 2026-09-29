# Issue 155 native acceptance

[Full native E2E recording (57 seconds)](issue-155.mp4)

Recorded on 2026-09-27 in the actual native WGPU application. Two triangles and a rectangle across two artboards and the surrounding document are seeded; every recorded operation uses pointer/keyboard input. The 570 viewport frames are 1600×1000, encoded at 10 fps. Capture took 155.63 seconds; this deterministic replay is not a performance benchmark.

The recording covers all six single-object alignments and exact undo; ordinary multi-object alignment; Shift-click across two artboards with document-bounds fallback; both distributions; group alignment as a unit; selecting a vector layer and moving all contents together; and Ctrl+S with exact reopening. Assertions and saved editable artwork are included.

Local automated checks: four geometry/history tests cover all six alignments, non-origin boards, document fallback, canvas/layer group selection, vector and placed pixel layers, disabled locked/hidden/empty layers, independent artboards, and single-step undo/redo. Three real egui UI/input tests cover all twelve former Arrange controls, single/layer/Shift-click behavior, full and compact menu removal, and all four order shortcuts across Design, Layout, Pixel and Motion. All passed.

Reproduce with `cargo build --locked --offline --bin inspector_qa`, then `target/debug/inspector_qa 155 /tmp/issue-155-qa` in a graphical session. The harness uses isolated XDG profiles. Existing installed builds and accepted 0.5.0 human-QA evidence are unchanged.

As specified in the issue, raster alignment uses the raster frame, including transparent borders. Close the agent panel to access the inspector's Align/Distribute controls.

Additional native acceptance: [parent followup](parent-review/README.md), 10 seconds, six passing assertions, plus focused regression coverage.
