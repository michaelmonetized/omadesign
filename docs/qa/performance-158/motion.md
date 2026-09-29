# Motion playback: results and method

This follow-up to issue #158 investigates the two real documents reported as slow during Motion playback. It is separate from the earlier 1440 × 900 drag/brush comparison. The native workload measures production playback with 24 documents open; a second workload compares the CPU compositor at identical timestamps. Neither changes the original artwork or the user's running editor.

Both comparisons use identical harness source against the respective production implementations. The baseline harnesses link the preserved release library using `opt-level=3`, thin LTO and one codegen unit; their exact `rustc` arguments are retained in `motion/compile-native-args.json` and `motion/compile-profile-args.json`. The baseline production source is not modified to add instrumentation.

## Final results

The final production source is `35c39093b8e8361a7970909bede8d5114e26022c`, compared with the earlier interaction-optimized build `54d7051c5c878c741ea5f5a5bb5bb030e0f4a6a6`. These are additional improvements beyond the original brush/drag report. Each native result pools two 30-second playback runs with 23 background documents. Static warmup is excluded; the first playing update is included.

| Document | Median UI work, before → after | P95 UI work | Maximum UI work | Native input callbacks/second |
| --- | ---: | ---: | ---: | ---: |
| Announcement | 514.60 → 58.96 ms | 525.85 → 73.04 ms | 539.92 → 139.37 ms | 1.96 → 16.17 |
| Live Stream | 184.72 → 36.42 ms | 206.07 → 52.28 ms | 1,877.35 → 57.42 ms | 5.02 → 25.14 |

Callback rates are not displayed FPS. The announcement still exceeds the 33.3 ms budget for 30 fps, and Live Stream exceeds it in many updates. These measurements demonstrate substantially less work and shorter callback intervals, not locked 30/60 fps or final human acceptance.

Live Stream's first playing UI calls fell from 1,877.35/1,874.21 ms to 21.53/21.91 ms across the two runs. Its formerly transparent 5,719-anchor object exposed quadratic path construction; computing the same rotation center once removes that cold hitch. In the separate CPU-only comparison, its first nonzero fixed pose fell from 1,861.09 to 51.39 ms. Announcement's first playing calls were 69.03/69.16 ms. All 16 fixed-time before/after frames have identical dimensions and decoded RGBA pixels.

Warm CPU-only render medians across rounds one and two fell from 508.16 to 64.93 ms for announcement and 176.49 to 40.26 ms for Live Stream. These fixed poses and viewport differ from native playback and must not be combined with the native timings. All cold samples remain in the [raw motion results](motion-results.json), alongside clock anchors, input/font hashes, per-input aggregates and resource receipts. The [artifact auditor](audit_motion.py) independently checks normal playhead progression, unchanged documents, both repetitions and fixed-frame equivalence.

The native occlusion reproduction used the same final motion executable. In its fixed hidden interior (12–15.5 seconds after process start), baseline consumed 99.17% of one CPU core and approximately 719,956 read calls/second; candidate recorded no CPU ticks or read calls over 3.26 sampled seconds. The complete hidden interval, including its boundary, averaged 2.73% CPU. Playback resumed automatically with the normal elapsed-time clock. Visible playback continued to consume approximately one core; this is a fix for stalled redraw spinning, not a claim of lower visible energy use.

Whole-process peak RSS rose from 303.50–307.44 to 371.89–373.50 MiB for announcement and from 218.84–218.88 to 242.73–245.23 MiB for Live Stream. Whole-process CPU time was lower, but process wall time did not uniformly improve. These receipts include startup, warmup and verification; there is no startup-speed claim. The bounded cache storage and remaining transient memory are described below.

### Brushing and dragging retained

Two native repetitions of each earlier interaction workload compare the same `54d7051c` baseline with the final `35c39093` binaries at 1440 × 900. The table pools the main drag or brush phase; whole-run and every phase's raw samples, maxima and first calls remain in the [interaction results](interaction-validation/interaction-results.json) and [independent review](interaction-validation/interaction-review.json).

| Workload | Median UI work, before → after | P95 UI work | Maximum UI work |
| --- | ---: | ---: | ---: |
| Simple drag | 0.575 → 0.598 ms | 1.219 → 0.778 ms | 29.992 → 1.981 ms |
| Drag above 1,200 objects | 0.619 → 0.623 ms | 1.302 → 0.769 ms | 41.129 → 1.838 ms |
| Drag beneath 600 foreground objects | 8.526 → 8.761 ms | 9.322 → 9.228 ms | 28.145 → 11.510 ms |
| Drag in a dense expanded layer | 0.594 → 0.652 ms | 1.027 → 0.778 ms | 21.744 → 2.106 ms |
| Simple canvas with 24 open tabs | 0.565 → 0.620 ms | 1.200 → 0.791 ms | 25.774 → 1.765 ms |
| Infographic brushing | 9.128 → 9.168 ms | 9.605 → 9.419 ms | 34.348 → 9.601 ms |

The medians increased slightly in this sequential local comparison, while every main interaction phase's pooled p95 and maximum improved. Moving periodic theme/font lookups off the UI thread removes recurring long calls even when no canvas render is needed. Results do not establish that every individual phase or scheduling tail improved.

For example, same-layer drag release p95 rose from 21.206 to 22.452 ms (eight samples per side); complex redo rose 0.912 ms and middle-object release rose 0.896 ms. Middle-object drag median rose 0.235 ms. The [interaction review summary](interaction-validation/README.md) retains these increases alongside the improvements.

All 24 baseline/candidate processes passed their native behavior assertions. Twenty-two saved documents are byte-identical to their scenario's reference output. The two candidate authoring outputs differ only in seven newly allocated entity IDs; a strict bijection protects all 586 seed IDs, validates 597 total entities and leaves every other value, type and array position exact. These fixtures contain no references to remapped IDs. The raw byte mismatch remains recorded rather than being labeled byte-identical. All four authoring exports have identical decoded RGBA pixels. The [publication manifest](interaction-validation/publication-manifest.json) records exact binary/source and evidence hashes.

### Build and validation identity

The final all-target check and release build passed, and library tests finished with **902 passed, 0 failed, 7 ignored**. [Validation evidence](motion-validation/) includes the final build receipt and sanitized logs. The production executable SHA-256 is `4babe72b0a919508d4cf8bc53bbb71c167eabd6704217ce6a153fdc43a0522eb`; native motion harness SHA-256 is `007bd97b9c16d530f8adb0d5c51e150fc62ff19b5adeff827c6a03607105a636`. Later documentation commits do not alter these binaries.

The four redraw-guard unit tests and separate real native timer/worker wakeup probe were run against `b6bad21b`, whose guard source is unchanged in `35c39093`. Their separate receipts retain that identity. With no external input, the native timer probe delivered UI work 16.12 ms after its deadline and the worker probe 0.082 ms after requesting repaint, following approximately 683/700 ms gaps without UI callbacks. These are single wakeup observations, not a latency distribution. Final-source native hidden/resume and all visible workloads were rerun above.

The installed release and accepted 0.5.0 human QA evidence remain intact. Fresh copies of the 23 background documents, seven earlier fixtures and both real motion documents are used for the separate manual handoff; automated results do not replace judgement of brushing, dragging and playback feel.

## Fixed inputs

The originals are the two user-supplied native documents. Fixed copies and their accompanying brand assets are recorded in the local `motion/inputs.json` receipt under `~/.local/state/omadesign/qa/performance-158/`. Client artwork, font bytes and private source paths remain local.

| Document | Bytes | SHA-256 |
| --- | ---: | --- |
| `0.6.0-announcement-thumbnail.oma` | 6,101,202 | `d50a4b83b58c2af683dbac4c52ab6952acc3c027252d6322719ecb875a8c39ce` |
| `Live Stream Working Document.oma` | 3,058,808 | `70c742386345f00b4b34d2c9607476dff4e18c2ad41a4426db4a6502081aee05` |

Both canvases are 1920 × 1080 with 30 fps looping clips. The announcement contains 35 layers, 105 vector objects, 228 tracks and 373 keys over 3.6 seconds. Live Stream contains 28 layers, 111 vector objects, 167 tracks and 531 keys over approximately 2.582 seconds. Their saved duration, frame-rate and looping settings are preserved.

The 23 background documents are `Background 01.oma` through `Background 23.oma` from the earlier manual QA fixture set. Each contains 320 editable vectors, eight 128 × 128 object masks and a 320 × 200 placed raster on a 1600 × 1000 canvas. The native harness copies and hashes every input. It also validates referenced fonts without substitution, preserves font-byte snapshots, and copies portable `omatype:` dependencies when present. These two inputs reference the installed Noto Sans Mono Black/Regular and Max55 font files; their exact hashes appear in each native result.

## Native playback measurement

[`motion_qa.rs`](../../../src/bin/motion_qa.rs) opens a fixed 1650 × 2131 eframe/WGPU window at one pixel per point on the isolated 1800 × 2300, scale-1, 60 Hz Sway output. The renderer receipt identifies the compositor's GLES renderer as Asahi/Apple M1 Pro. The desktop session remains separate. Each process gets new XDG configuration, data, cache and state directories.

Each active document is measured with `--background-tabs 23`, all 23 explicit background paths, `--frames 100000 --seconds 30 --timeout 180`. Playback begins after at least 30 UI calls, at least two seconds, and the normal scene/preview readiness checks. The harness sets `playing` once and leaves egui timestamps unchanged; production `Studio::tick_motion` advances the playhead. The 30-second limit is checked after a UI call, so actual measured duration may exceed it by that call. The high UI-call limit is a secondary stopping condition. Raw samples and the stopping reason are retained.

`Instant` measures elapsed time around `Studio::ui`, including descheduling during the call. Input-to-UI runs from the native `raw_input_hook` callback to UI completion. The hook removes user input events while retaining screenshot delivery; this is automated playback, not operating-system input latency. Multiple egui UI calls can share one native input callback. Results therefore retain both individual UI calls and per-input aggregates: summed UI work, final input-to-UI duration, raw-input interval and successive UI-completion interval. Callback rates are explicitly named `observed_ui_calls_per_second` and `observed_native_input_callbacks_per_second`; neither is presented FPS or physical display latency.

Medians use the conventional middle value, averaging the middle pair for an even sample count. P95 uses nearest rank: sorted sample `ceil(0.95 × n)`. Compare matching documents and run configurations; time-bounded playback naturally produces different sample counts as throughput changes. Do not pool native callback timings with CPU-only render timings. This is an as-used local comparison, without controlled thermals, randomized execution order or a fleet-wide claim.

Process startup, static-preview warmup, file/font copying, verification and screenshot encoding are excluded from playback samples. The first playing UI call is included, including geometry/effect work for objects that become visible only after time zero. Whole-process resource receipts include the excluded setup costs. After measurement, the harness checks elapsed-time playhead progression, successful canvas render-key updates, unchanged encoded contents of every tab, unchanged input/font bytes, and receipt of a native screenshot. Render-key updates alone do not establish pixel equivalence or presentation cadence.

## Fixed-time rendering comparison

The separate `motion/motion_profile.rs` harness calls production `render_view_posed` directly into a 1240 × 1800 CPU pixmap. Its view uses offset `(20, 20)` and scale `min((1240 − 40) / document.width, (1800 − 40) / document.height)`. It renders timestamps `0.0, 0.1, 0.4, 0.8, 1.2, 1.8, 2.4, 3.2` seconds in that order for three rounds, retaining all 24 timings. The timestamps are passed directly, including 3.2 seconds for the shorter Live Stream clip; they are not advanced through the playback clock.

Only `render_view_posed` is timed. Loading and PNG encoding are outside each sample; the first draw includes cache population, while subsequent draws may reuse applicable entries. Round zero saves all eight frames. Compare corresponding before/after PNGs by decoded RGBA pixels and dimensions at every timestamp, rather than comparing native screenshots taken at different real-time playheads. This checks fixed-frame compositing independently of native pacing. Its viewport deliberately differs from the native editor's canvas region, and its timings exclude UI, texture upload and GPU presentation.

## Changes and memory tradeoffs

- Blur traverses adjacent RGBA pixels and contiguous rows, retaining transparent edge padding and integer rounding at each pass while removing redundant full-size copies. Sparse RGBA surfaces use the exact combined finite support of both kernels, clipped to the original image edge. Effects that immediately replace RGB with a tint blur only alpha. Integer offsets copy clipped rows.
- Native object-effect reuse compares the exact local raster transform, dimensions, path, paint, image, filters and reveal parameters. Destination placement and opacity remain current. Aging frequency admission retains a useful subset when a sequential scene exceeds the cache budget.
- Complex objects that already require an isolated opacity/blend surface reuse it only when their exact screen-space transform, dimensions and rendering inputs match. Direct opaque object rendering keeps its original path and rounding.
- Legacy layer/group filters reuse output only when dimensions, complete source pixels and filter parameters match. Completed isolated layer/group surfaces restrict final blits to nonzero RGBA bounds while retaining original sampling coordinates and filter domains. Admission is checked before copying an unfiltered source that cannot remain cached.
- Independent appearance reuses source-derived effect planes after exact source-byte, dimension and effect-list validation. Every backdrop blend, outer/content/inner draw, mask, Fill opacity and final object/group opacity operation remains current. Hashes narrow lookup and never establish equivalence alone.
- Tab activation and committed edits reset cache admission history without dropping valid cached pixels. Motion ticks and gesture samples retain that history.
- Motion playback retains existing tab thumbnails and starts no new preview snapshot/render jobs. Stale or cold previews resume after pausing. The timeline resolves names only for visible rows. Recovery remains once per edited revision and is not disabled during long playback.
- Cubic path construction computes the original geometry's rotation center once, retaining the exact per-point transform arithmetic. Previously it recalculated bounds by sampling every contour for each cubic control point. This made first construction quadratic in path complexity; Live Stream's first nonzero pose exposed that cost in a previously transparent 5,719-anchor object.
- Theme polling retains its 400 ms gate but runs the system-font command, theme-file lookup and changed-font loading in one background job. The UI consumes completed results without waiting, keeps the last valid palette after a bad file, and applies changed fonts on the UI thread. Changed worker results wake the editor, and an early repaint re-arms the next poll. Unchanged snapshots do not replace font definitions. Explicit startup/preferences application retains its existing synchronous behavior.

Object effects and complex isolated objects share a 64 MiB/256-entry budget per render thread, increased from the previous 32 MiB object-effect budget. Filtered layer/group storage has a separate 64 MiB/256-entry budget, including retained unfiltered pixels and results. Independent appearance has a separate 16 MiB/128-entry budget including source snapshots and effect planes; oversized sets bypass hashing and retention. The aging admission sketch adds about 2 KiB to each cache. Existing transformed-mask storage has its own 32 MiB budget; interaction backgrounds and mapped pixel selections each have separate 64 MiB limits, and the gradient texture cache also has a separate 64 MiB pixel budget.

These are retained-storage limits, not a total-process or peak-allocation cap. Documents, undo history, GPU textures, transient render/filter buffers, allocator overhead and temporarily held shared pixels are additional. Entries can survive closing a tab until subsequent eviction. Motion still traverses and composites the scene and uploads changed canvas pixels; cache misses, changing filtered content and large viewports retain substantial work. The tradeoff is more bounded retained memory for less repeated effect computation.

## Redraw scheduling

A separate native reproduction found that an occluded Wayland window could consume one CPU core while receiving no UI callbacks. The compositor had withheld the queued surface redraw. The first attempted fix made every Wayland redraw event-driven, but repeated native canaries increased visible simple-drag and brush UI times, including phases with no canvas rendering. That version was rejected for handoff.

The final [eframe patch](../../../vendor/eframe/OMADESIGN_PATCH.md) preserves upstream polling while redraws progress. It switches an existing `Poll` to `Wait` only when **all** outstanding redraws are on Wayland and have remained undelivered for at least 100 ms. Repeated requests cannot refresh their first-request timestamp. Delivery clears that timestamp before UI work, so expensive rendering does not consume the next redraw's allowance. One progressing window retains polling; explicit waits and scheduled timer deadlines retain their existing behavior. Input, worker events and compositor callbacks can wake a waiting event loop. No sleep, fixed frame delay or motion-clock change is introduced.

Visible polling retains its CPU cost. The comparison does not establish why the unconditional-wait version increased measured UI time; CPU scheduling/frequency is a possibility, not a demonstrated cause. Native UI measurements include descheduling. The guard removes prolonged spinning when callbacks stop, rather than claiming general visible-window energy savings.

The vendored crate is the published eframe 0.36.1 package with the exact source diff, archive checksum and licenses recorded in [provenance](../../../vendor/eframe/OMADESIGN_PROVENANCE.json). Cargo's package-test command rejects this dependency because it has dev-dependencies and is not a workspace member. The four guard tests were therefore compiled from the exact, unmodified implementation and test module in `src/native/run.rs`, using the real `winit::WindowId`, `ControlFlow` and `ahash` types. All four passed. Production integration is checked by the normal all-target build and native visible, hidden/resume, timer and worker-repaint workloads; the standalone unit tests do not replace those checks.
