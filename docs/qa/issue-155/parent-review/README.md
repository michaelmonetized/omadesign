# Issue 155: parent followup

[Complete native recording](issue-155-parent.mp4) — 100 frames / 10 seconds, 1600×1000 at 10 fps, six completed fail-fast assertions.

An unlocked group contains a hidden, object-locked child. Inspector Right and Bottom alignment move both children by the same delta, preserve their offset, undo exactly in one step, and survive Ctrl+S/reopen. The visible triangle and full group selection bounds show the movement; assertions inspect the hidden child too.

Only the initial fixture is seeded. Recorded edits use native egui input handlers; all frames come from the actual WGPU viewport. Playback is deterministic, not a realtime performance benchmark. The full MP4 was decoded without errors. `saved.oma` is the native Ctrl+S artifact. Source, binary and artifact hashes are in `manifest.json`; focused regression output is in `tests.log`.

The additional nested-layer regression selects a plain vector layer within a group. It verifies that visible locked objects move, hidden objects remain unchanged, and the action undoes/redoes exactly. All seven alignment tests pass; see `nested-layer-tests.log`.
