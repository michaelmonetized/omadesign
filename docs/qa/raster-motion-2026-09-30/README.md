# Raster motion: keyframes and presets

Pasted images are raster layers selected with a shared `RASTER_ID` sentinel, while the original motion UI, hit testing, presets and renderer assumed vector shape IDs. Raster keys therefore targeted the wrong ID and raster pixels never consumed a motion pose.

The fix maps raster selections to persistent layer IDs throughout authoring, timeline selection/key deletion, canvas manipulation, snapping, rendering and animated SVG export. Images support all 12 applicable presets, including Fill up; Draw stroke remains for vector outlines. Raster masks follow the animation, opacity keys remain absolute, and undo/redo, duplication and layer deletion preserve the correct tracks. Discrete key operations and completed motion drags now have separate undo steps.

The local optimized 0.6.1 build is installed as `/home/michael/.local/bin/omadesign-raster-motion` and was launched with `/home/michael/Documents/Omadesign Raster Motion Demo.oma` in Motion mode. It uses an isolated preview profile. The existing installed `omadesign` binary was not replaced. `manifest.json` proves that the running executable matches the built and installed binary.

- Full local library suite: **918 passed, 2 failed, 7 ignored**. Both failures are untouched text-ellipsis tests and reproduce on the unmodified `b54f80d0` baseline; see `baseline-ellipsis.log` and `full-tests.log`.
- Focused motion suite: **53 passed**, including raster import, all applicable presets, independent IDs, key deletion, masks moving into view, duplication, save/reopen, undo/redo, canvas gestures and timeline interaction.
- `cargo check --all-targets --locked --offline`, optimized application build, native QA harness build and `git diff --check` passed.
- [Native UI recording](native/issue-raster-motion.mp4): **9 passing assertions**, 240 frames at 1600×1000, 24 seconds. It exercises actual egui pointer/keyboard events for selection, K, dragging, presets, undo/redo, playback and Ctrl+S/reopen. Only the two clipboard-image fixtures are seeded. The video decoded without errors. This is deterministic acceptance capture, not a performance benchmark; inactive workspaces paused capture and are included in the wall time.
- [Launched application](launched.png), [editable test document](native/result.oma), [animated SVG](native/animated.svg), and rendered start/middle/end PNGs provide the visual and export receipts.

The screenshot harness was adjusted to capture held gestures and playback while document thumbnails are intentionally deferred. Production rendering was not changed for that harness requirement.

The accepted 0.5.0 human QA evidence is unchanged. This is a local fix and automated acceptance, not a public release or new human QA signoff. Lottie retains its existing explicit restriction on pixel layers; native frame/video exports and animated SVG support this raster motion.
