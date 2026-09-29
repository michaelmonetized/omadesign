# Omadesign Wayland redraw scheduling patch

This directory contains the published eframe 0.36.1 crate, with one local source
change in `src/native/run.rs`. The normalized published `Cargo.toml` is unchanged.
`OMADESIGN_PROVENANCE.json` records the crates.io archive checksum, upstream
commit, license sources and hashes, and exact local patch. Both upstream
licenses are included. Registry cache bookkeeping and the package-local lockfile
are omitted; Omadesign's root lockfile determines the dependency graph.

## Change

Keep upstream `Poll` when requesting a redraw. Record each window's first
outstanding request time; repeating a request does not extend it. Only demote an
existing `Poll` to `Wait` when every outstanding request is on Wayland and has
remained undelivered for at least 100 ms. One progressing window keeps polling
even when another window is hidden. Clear delivered requests before running UI,
so a long render does not consume the next request's allowance. Destroyed,
missing and directly painted invisible windows are removed from the guard.

Winit's Wayland `Window::request_redraw` queues the redraw and wakes the event
loop. When the compositor withholds frame callbacks for an occluded surface,
winit retains that request until the surface can draw. Input, worker events and
compositor callbacks still wake `Wait` immediately. The guard adds no sleep or
rendering delay and does not change motion timestamps. Explicit `Wait`, future
`WaitUntil` deadlines and non-Wayland scheduling remain unchanged. The 100 ms
threshold bounds polling without relying on `Occluded`, which this Wayland
backend does not emit. Visible redraw polling retains its CPU cost by design.

## Reproduction and validation

The unmodified native WGPU motion harness was shown, moved to an inactive
workspace in a separate headless Sway compositor for five seconds, and shown
again. During the strict hidden interval, the process used 99.17% of one CPU
core and made about 720,000 read calls per second with zero bytes read or written.
Its native input callbacks had a 5.22-second gap. Documents, source files, fonts
and normal elapsed-time playback checks passed after the window returned.
This reproduces the occluded-window spin; the particular syscall FD was not
traced. The eventfd explanation follows the local eframe/winit/polling sources.

An earlier version changed every Wayland redraw request to `Wait`. Repeat native
canaries showed increased visible UI time for simple drags and brushing; the
current guard restores upstream visible polling instead of applying that broad
change. This is a scheduling observation, not proof of a CPU-frequency cause.

Before considering the current guard validated, repeat that identical hidden/visible
workload and verify that hidden CPU/read activity stops and playback resumes
without additional input. Also verify visible motion, brush/drag behavior,
delayed repaints and worker-triggered repaints. Do not impose sleeps or alter
motion timestamps to hide the problem.

Four unit tests in `native::run::redraw_poll_guard_tests` cover the exact timeout
boundary, repeated requests, delivery/removal and long-paint reset, one active
window beside a stalled window, explicit waits/deadlines, and other backends.
Run them with `cargo test -p eframe --lib redraw_poll_guard_tests`; native QA is
still required to verify actual callback/wakeup behavior.

The patch is kept separately as `omadesign-wayland-wait.patch` for review and an
upstream submission. Apply it to the pinned upstream crate with
`git apply --unidiff-zero omadesign-wayland-wait.patch`; its zero-context form
avoids whitespace-only context lines in the checked-in patch. Applying it has
been checked to reproduce the recorded local source hash exactly. It has not
been claimed as an upstream fix.
