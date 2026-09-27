# Issue 154: motion followup

[Complete native recording](issue-154-motion.mp4) — 120 frames / 12 seconds, 1600×1000 at 10 fps, six completed fail-fast assertions.

Shift+H and Shift+V reflect two animated objects about their visible selection center at playhead 0.70 s. Native assertions compare the transformed outlines at three keyframe times, check one-step undo, and verify Ctrl+S/reopen. X/Y offsets, rotation and gradient-angle keys reflect with the base artwork; key times/easing and scale channels are preserved.

Only the initial fixture is seeded. Recorded edits use native egui input handlers; all frames come from the actual WGPU viewport. Playback is deterministic, not a realtime performance benchmark. The full MP4 was decoded without errors. `saved.oma` is the native Ctrl+S artifact. Source, binary and artifact hashes are in `manifest.json`; focused regression output is in `tests.log`.
