# Omadesign Wayland redraw scheduling patch

This directory contains the published eframe 0.36.1 crate, with one local source
change in `src/native/run.rs`. The normalized published `Cargo.toml` is unchanged.
`OMADESIGN_PROVENANCE.json` records the crates.io archive checksum, upstream
commit, license sources and hashes, and exact local patch. Both upstream
licenses are included. Registry cache bookkeeping and the package-local lockfile
are omitted; Omadesign's root lockfile determines the dependency graph.

## Change

When requesting a redraw on Wayland, wait for the event loop rather than forcing
continuous polling. Winit's Wayland `Window::request_redraw` queues the redraw and
wakes the event loop. When the compositor withholds frame callbacks for an
occluded surface, winit retains that queued request until the surface can draw.
The previous forced `Poll` loop kept consuming a CPU core without delivering UI
callbacks. Other window backends keep their existing `Poll` behavior, and future
scheduled redraws retain the existing `WaitUntil` deadline handling.

## Reproduction and validation

The unmodified native WGPU motion harness was shown, moved to an inactive
workspace in a separate headless Sway compositor for five seconds, and shown
again. During the strict hidden interval, the process used 99.17% of one CPU
core and made about 720,000 read calls per second with zero bytes read or written.
Its native input callbacks had a 5.22-second gap. Documents, source files, fonts
and normal elapsed-time playback checks passed after the window returned.
This reproduces the occluded-window spin; the particular syscall FD was not
traced. The eventfd explanation follows the local eframe/winit/polling sources.

Before considering this patch validated, repeat that identical hidden/visible
workload and verify that hidden CPU/read activity stops and playback resumes
without additional input. Also verify visible motion, brush/drag behavior,
delayed repaints and worker-triggered repaints. Do not impose sleeps or alter
motion timestamps to hide the problem.

The patch is kept separately as `omadesign-wayland-wait.patch` for review and an
upstream submission. It has not been claimed as an upstream fix.
