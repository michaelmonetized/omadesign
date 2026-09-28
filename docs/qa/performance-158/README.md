# Brush and canvas interaction performance — issue #158

The native M1 Pro comparison reduces median brush UI work from **28.70 to 9.29 ms**, dragging beneath foreground artwork from **45.21 to 8.61 ms**, and moving on a simple canvas with 24 open tabs from **5.72 to 0.67 ms**. All 24 benchmark processes passed their behavior checks, and all four exported infographics have identical RGBA pixels.

Compared production source: `caeda653` → `54d7051c`, above the current feature stack. Both sides use identical QA harnesses, original authoring inputs, a fixed 1440 × 900 native WGPU window, and a separate GPU-backed headless Wayland display. This is a local as-used comparison on m1pro16, with synthetic egui pointer and keyboard events; it does not measure operating-system input delivery, physical mouse-to-photon latency or performance on the other fleet hosts. [Methodology, source scope and reproduction](methodology.md).

## Interaction results

CPU-side elapsed time around production `Studio::ui`, recomputed from raw samples using conventional medians and nearest-rank p95. Each drag row pools 720 updates; brush pools 180. The speedup column measures this UI work, not displayed FPS.

| Workload | Before median / p95 (ms) | After median / p95 (ms) | Median speedup |
| --- | ---: | ---: | ---: |
| Simple canvas | 5.66 / 6.48 | 0.64 / 0.99 | 8.90× |
| 1,200 objects, target above | 44.87 / 61.47 | 0.66 / 0.98 | 68.02× |
| 1,200 objects, target in middle | 45.21 / 61.59 | 8.61 / 9.82 | 5.25× |
| 1,200 objects, same expanded layer | 49.37 / 65.80 | 0.67 / 1.39 | 73.66× |
| Simple canvas, 24 open tabs | 5.72 / 6.57 | 0.67 / 1.00 | 8.57× |
| Infographic brush | 28.70 / 43.13 | 9.29 / 9.92 | 3.09× |
| Infographic typing | 19.34 / 34.13 | 18.73 / 33.58 | 1.03× |
| Infographic pan | 25.23 / 39.19 | 24.04 / 39.19 | 1.05× |
| Infographic zoom | 26.88 / 40.87 | 26.17 / 40.69 | 1.03× |
| Save (two samples) | 81.15 / 92.29 | 76.95 / 78.12 | 1.05× |

The first optimized checkpoint (`cb121091`) still spent 19.95 ms per middle-of-stack drag in its single measured repetition. Profiling that remaining work led to exact transformed-mask reuse; the final two repetitions have a pooled 8.61 ms median. The [intermediate raw evidence](intermediate/manifest.json) is separate from the before/after comparison.

Observed native frame intervals provide a second view of responsiveness:

| Workload | Before median / p95 (ms) | After median / p95 (ms) |
| --- | ---: | ---: |
| Simple canvas | 16.11 / 17.13 | 16.14 / 16.91 |
| 1,200 objects, target above | 45.68 / 62.20 | 16.14 / 16.75 |
| 1,200 objects, target in middle | 45.97 / 62.34 | 16.12 / 17.58 |
| 1,200 objects, same expanded layer | 50.30 / 66.68 | 16.15 / 16.86 |
| Simple canvas, 24 open tabs | 16.11 / 17.25 | 16.15 / 18.03 |
| Infographic brush | 29.48 / 43.95 | 16.12 / 16.78 |

Dense drag and brush intervals now approach the display's frame cadence. Simple/many-tab intervals were already near 16 ms despite spending substantially more time inside the UI. Many-tab p95 intervals increased slightly (17.25 → 18.03 ms). Some simple/many-tab release intervals also increased (~16 → 20.8 ms, eight samples each), while release UI and input-to-UI times decreased. The interval includes preceding-frame completion and scheduling, so it is not an isolated release-latency measurement. Typing, pan and zoom improved modestly and remain much more expensive than cached movement.

Occasional stalls remain. Maximum observed UI samples increased for many-tab dragging (29.65 → 39.46 ms), pan (39.74 → 60.26 ms) and zoom (46.21 → 57.91 ms), despite their lower medians. These are observations from two repetitions, not estimated worst-case bounds; the comparison does not establish that every frame became faster.

## What changed

- Reuse the exact composited background beneath changing objects or paint layers, preserving original foreground and blend order.
- Cache compatible effects and transformed object masks with explicit size limits; cull offscreen work before expensive allocations.
- Update only affected brush/retouch rectangles and premultiplied pixels; retain queued pointer samples, corners and the release endpoint.
- Render visible lightweight Layers/sidebar rows and tab thumbnails without repeatedly cloning full compositions or masks. Defer recovery/thumbnail snapshots until after pointer release.
- Avoid unrelated text reflow during raster strokes and redundant canvas pixel conversion.

Groups, dependent text, motion and resizing retain their full-scene paths where reuse could change behavior. Exact-render and tool-state tests cover those boundaries. [Detailed changes, cache limits and remaining costs](methodology.md#changes-on-the-interaction-path).

## Memory and resource tradeoffs

| Whole-process peak RSS (median of two runs) | Before MiB | After MiB |
| --- | ---: | ---: |
| simple | 102.16 | 105.57 |
| complex | 114.70 | 145.59 |
| complex-middle | 115.38 | 146.34 |
| complex-same-layer | 122.14 | 146.41 |
| many-tabs | 142.15 | 145.46 |
| authoring | 227.79 | 230.55 |

Dense fixtures use roughly 24–31 MiB more peak memory; simple, many-tab and authoring fixtures use about 3 MiB more. Cache payload limits are separate: active interaction background 64 MiB, one transformed selection 64 MiB, and per-render-thread effect and object-mask caches 32 MiB each. These are not a global process-memory cap; document pixels, undo, textures, transient buffers and allocation overhead are additional.

Median whole-process user CPU decreased in every workload, while median system CPU increased. Whole-process duration and major page faults were not uniformly better: simple median wall time was 9.36 → 10.92 s, and first-run faults varied substantially. Startup, shader caches, screenshot/export/save work and host scheduling are included in these totals. All run resources, host pressure and observed increases are retained in [audit.json](audit.json); no general startup, energy or memory-efficiency claim is made.

## Correctness and native evidence

- [Full library suite](validation/full-library-final.log): **863 passed, 0 failed, 7 ignored**. Tests cover exact compositing, mask/effect cache invalidation and eviction, brush/retouch/soft selections, queued input, undo/redo, tabs, recovery, sidebar scrolling, rename focus and drag/drop.
- [All-target check](validation/check-all-targets.log), [release build](validation/release-final.log), and [benchmark script validation](validation/script-validation.log) passed.
- [Independent audit](audit.json): 12 successful native processes per side, matching workload sample counts, all native assertions, and zero differing pixels across four 1200 × 900 authoring exports. Every drag process verifies continuous object/canvas updates, 12 movement/undo/redo checks, unchanged inactive documents, tab state and exact save/reopen.
- [Production executable smoke](production-smoke/receipt.json): the built `omadesign` binary opened the authored document and produced a [native screenshot](production-smoke/native-editor.png), exit 0.
- Separate recorded QA runs: [dense movement beneath foreground artwork](recordings/complex-middle/complex-middle-native.mp4) and [authoring, including three brush strokes](recordings/authoring/authoring-native.mp4). Their receipts verify native assertions and H.264 1440 × 900 video. Recording samples are excluded from benchmark timing.
- Representative screenshots: [dense middle](screenshots/complex-middle.png), [same expanded layer](screenshots/complex-same-layer.png), [24 tabs](screenshots/many-tabs.png), [authoring](screenshots/authoring.png), and [unchanged final export](screenshots/infographic-export.png).

Raw measurements are in [before](before/manifest.json), [after](after/manifest.json), and [summary.json](summary.json). [Provenance](provenance.json) records source, executable and fixture hashes. Native recordings use a task-only blank headless display; the user's desktop is not captured. Generated profiles, executables and full saved workload documents remain in the local QA artifact directory.

The installed release and accepted 0.5.0 human QA evidence are unchanged. The work is verified locally and prepared for review; it is not a fleet-wide rerun or a new installed-release acceptance.
