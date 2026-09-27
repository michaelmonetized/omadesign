# Issue 154 native acceptance

[Full native E2E recording (37 seconds)](issue-154.mp4)

Recorded on 2026-09-27 using the real WGPU application UI and pointer/keyboard events. The initial two-shape document is seeded; recorded edits use the production UI. All 370 frames are actual 1600×1000 native viewport captures, encoded at 10 fps. This deterministic input replay is not a wall-clock performance benchmark (112.19 seconds capture time).

The recording checks the single inspector flip pair, multi-selection shared-center reflection, one-step undo and exact redo, Object/Canvas/Layers Transform menus, Shift+H/V in Design/Layout/Motion, ordinary H/V text entry, and Ctrl+S followed by exact project reopening. `result.json` lists the assertions. `result.oma` is the saved editable document.

Local regression validation: `cargo test --lib --locked --offline flip -- --nocapture` — 9 passed, 0 failed. This also covers locked/hidden/ancestor-locked objects, text exclusion, rotated paths, dashed geometry, gradients and corner radii. A prior canvas-menu test now opens the Transform submenu before selecting Flip.

Reproduce: `cargo build --locked --offline --bin inspector_qa`, then `target/debug/inspector_qa 154 /tmp/issue-154-qa` in a graphical session. The harness isolates all XDG profile directories and leaves the installed application intact. `SHA256SUMS` records the capture executable and MP4.

The accepted 0.5.0 human QA evidence remains unchanged. This is automated native acceptance of this PR, not a new human release signoff.

Additional native acceptance: [motion followup](motion-review/README.md), 12 seconds, six passing assertions, plus focused regression coverage.
