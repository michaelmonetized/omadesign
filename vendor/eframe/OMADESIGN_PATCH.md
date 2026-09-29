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

Four unit tests in `native::run::redraw_poll_guard_tests` cover the exact timeout
boundary, repeated requests, delivery/removal and long-paint reset, one active
window beside a stalled window, explicit waits/deadlines, and other backends.
All four [passed](../../docs/qa/performance-158/motion-validation/guard-tests.log).
Cargo rejects `cargo test -p eframe --lib` because this dependency has
dev-dependencies and is not a workspace member. The exact, unmodified guard and
test module were therefore compiled separately with the real `winit` and
`ahash` types; the [source receipt](../../docs/qa/performance-158/motion-validation/guard-tests-source.json)
records that scope and source hash. This tests the scheduling decision and does
not substitute for native event-loop validation.

A separate native WGPU [wakeup probe](../../docs/qa/performance-158/motion-validation/wakeup-result.json)
linked the exact patched eframe dependency used by application revision
`b6bad21b`. After a 683.39 ms native-callback idle interval, its delayed-repaint
check completed 16.12 ms after the requested deadline. After a separate
700.22 ms idle interval, a worker repaint reached UI in 0.0819 ms. Both checks
passed without synthetic input, changed timestamps or watchdog repaints.
[Provenance](../../docs/qa/performance-158/motion-validation/wakeup-provenance.json)
pins the probe binary and tested dependency; this probe was not rerun against
the final `35c39093` application binary. The guard source is identical there.

The final `35c39093` native Motion comparison repeated visible playback and the
hidden/resume protocol. Its strict hidden observation interval recorded zero
CPU-counter increments and zero read calls, and normal playback/document checks
passed after the window returned. Visible rendering still consumes CPU.
The [Motion report](../../docs/qa/performance-158/motion.md) gives the paired
measurements, interaction results and limits; no sleep or altered motion clock
was introduced to obtain them.

The patch is kept separately as `omadesign-wayland-wait.patch` for review and an
upstream submission. Apply it to the pinned upstream crate with
`git apply --unidiff-zero omadesign-wayland-wait.patch`; its zero-context form
avoids whitespace-only context lines in the checked-in patch. Applying it has
been checked to reproduce the recorded local source hash exactly. It has not
been claimed as an upstream fix.
