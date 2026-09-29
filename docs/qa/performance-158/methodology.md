# Native interaction performance: method and scope

This follows [issue #158](https://github.com/michaelmonetized/omadesign/issues/158) and the [2026-09-27 fleet baseline](../fleet-benchmark-2026-09-27/README.md). It measures the current feature stack on m1pro16 before and after the changes. Historical fleet timings are not pooled with this comparison.

## Compared source

- Before: `caeda65307073e3c83dde2b4a0976d6e04555834`.
- After: `54d7051c5c878c741ea5f5a5bb5bb030e0f4a6a6`.
- Intermediate renderer/brush/sidebar pass: `cb1210910ce1fcd3abe657806dc724f742154402`; one complex-middle and one authoring run provide a separate checkpoint before the extra mask-cache pass. These samples are not pooled into the final comparison.
- Both use identical native QA harnesses from the after commit, built against their respective production code with the repository's normal release profile. No production source was changed for the baseline.
- The authoring harness now sends a hover frame before clicks and explicitly focuses the canvas before its Text shortcut. The original harness could not begin typing against the current feature stack. This harness correction is applied equally to both sides; the supplied document and typed content are unchanged.
- [provenance.json](provenance.json) records source identities, fixture/font SHA-256 values and CPU inventory. Its executable groups distinguish `baseline/bin`, intermediate `optimized/bin` and final `final/bin` hashes. Each run manifest also records executable and input hashes.

## Native workloads

All cases open a real 1440 × 900 eframe/WGPU window, use the production `Studio::ui`, inject synthetic pointer and keyboard events through eframe's `raw_input_hook`, and save a screenshot and editable document. Each side runs two serial repetitions of the following six cases in a fresh profile with networking disabled by `unshare -Urn`. The windows run on a separate GPU-backed headless Sway output at 1600 × 1000, scale 1, 60 Hz, so workspace changes on the user's desktop cannot pause their frame callbacks. This exercises native window rendering and egui event handling, not physical display presentation, operating-system input delivery or manual drawing.

| Case | Composition and checks |
| --- | --- |
| Simple | One movable rectangle above a placed raster. |
| Complex | Movable rectangle above 1,200 vector objects, including 30 object masks. |
| Complex middle | 600 objects below the rectangle and 600 above. Foreground objects are moved away from the four press positions so hit-testing selects the actual target, while overlapping its path. |
| Complex same layer | Rectangle and 1,200 other objects share an expanded vector layer, exercising the real Layers sidebar. |
| Many tabs | Simple active document plus 23 inactive compositions, each with 320 vectors, object masks and a placed raster. |
| Authoring | Original editable 1200 × 900 infographic: type 83 characters across five objects, draw three Pen paths and three Brush strokes, pan, zoom, undo/redo, save, reopen and export. |

Each object-drag process contains four real drags, 90 pointer moves per drag, and 12 position checks covering movement, Undo and Redo. It verifies actual canvas updates, unchanged inactive documents, restored history and selection after tab switching, and exact save/reopen. The two repetitions yield 720 drag-move samples per scenario. Authoring contributes 166 typing, 180 brush-drag, 80 pan and 120 zoom samples. Save has only two samples, so its tail is not statistically meaningful.

The primary measurement is CPU-side UI elapsed time around `Studio::ui`, measured with `Instant`; it includes any descheduling inside that call and is not thread CPU-time accounting. Input-to-UI runs from synthetic event injection in `raw_input_hook` through the end of `Studio::ui`. Frame intervals are also retained. These are not GPU presentation latency, physical mouse-to-photon measurements or an FPS claim. The harness advances one event step per frame; burst input correctness is covered separately by regression tests.

Pooled report medians use `statistics.median`; p95 uses the nearest-rank definition on the raw samples. Individual-run JSON is retained alongside pooled results. The embedded authoring phase summaries retain their older percentile formulas; the report recomputes statistics from raw samples instead of using those summaries. The summarizer rejects failed or timed-out runs and requires matching scenarios, repetitions, workload phases and per-run sample counts. Coordination phases (`startup`, `check`, `setup` and `preview-wait`) can differ in frame count because they include initialization or asynchronous waits. Startup, screenshot capture and document verification are excluded from the drag/brush phase samples but included in whole-process resource measurements.

No benchmark ran concurrently with our compilers or tests. The existing desktop applications and host power policy were preserved. Each build was measured after its compilation finished, with all before runs followed by all after runs and the final candidate build between them; the order was not randomized and thermals were not controlled. This is a local as-used comparison. Whole-process peak RSS, CPU totals and major faults are recorded, alongside host memory, load, CPU frequency policy and pressure before/after each run. Other fleet hosts and offline AI inference were not rerun.

Exploratory runs encountered a full `/tmp` tmpfs, hidden-window frame suspension on the desktop, and a pixman headless compositor that exposed software-rendering adapters. Those results are excluded. All reported before/after runs use the same disk-backed output filesystem and GPU-backed native display; the runner checks free space and enforces an external per-process timeout. The compositor's GLES renderer identifies Asahi/Apple M1 Pro, and a running QA client had five Asahi render-node descriptors with no llvmpipe worker threads. The preserved display receipts document this check; kernel fdinfo did not expose GPU engine timing counters.

## Changes on the interaction path

- The compositor reuses the exact already-composited prefix beneath a moving object or paint layer. Everything above that prefix is drawn in its original order, preserving 8-bit blending. Grouped scenes and linked/area text retain the full renderer when dependencies could cross the split.
- A bounded effect cache reuses native filtered pixels only for compatible appearance stacks. Backdrop-dependent appearance retains its existing compositing path. Conservative culling happens before expensive object-mask/effect allocation, and isolated unfiltered layers use bounded temporary surfaces.
- A second measured pass caches exact transformed object masks for unchanged foreground artwork. Every hit checks the actual premultiplied source bytes, source/viewport dimensions and complete transform. Parent intersections are applied independently, so changing a parent cannot alter the cached own mask. Large sources bypass caching, and misses use the original full-viewport sampling arithmetic.
- Brush and retouch operations clip, feather and publish the affected rectangle. Pixel publication patches both RGBA and the cached premultiplied surface. Stroke setup, selection changes and unsupported cases retain full-update paths. Clone and Smudge avoid duplicate original-image buffers; transformed selections reuse one bounded mapping.
- Brush input consumes all queued pointer samples and the release endpoint. Held-but-idle strokes do not invalidate the canvas. Canvas ownership prevents inspector clicks from starting strokes or clearing selection.
- The Layers sidebar snapshots only visible lightweight row metadata instead of cloning every shape, text layout and mask. Drag payloads are created when dragging starts. Offscreen renames remain in the focus lifecycle.
- The document-tab sidebar renders visible rows; its cache is moved between frames and pruned in one pass. Thumbnail and recovery snapshot preparation waits until after pointer release. Object-mask thumbnail proxies avoid copying full-resolution masks.
- Canvas texture conversion uses the renderer's premultiplied pixels directly. Vector dependency reflow happens after the current drag sample; raster painting no longer triggers unrelated text reflow.

## Correctness and remaining costs

Regression coverage includes byte-for-byte renderer comparisons against full compositing, masks, blends, effects, transforms, overlapping same-layer selections, dependent text, cache eviction, retouch tools, soft selections, queued brush corners/release, cancelled strokes, undo/redo, tab lifecycle, recovery scheduling, sidebar scrolling, renaming and drag/drop. Native workload assertions and final screenshots supplement those tests.

The interaction backdrop is capped at 64 MiB and belongs only to the active Studio; the mapped-selection cache is one entry capped at 64 MiB. Effect storage and transformed-mask storage each have separate 32 MiB/256-entry payload budgets per render thread. Mask source snapshots count toward that budget. These are separate caches, not a single process-wide memory cap; small allocation overhead, transient render buffers, source documents, history and GPU textures are additional. Large viewports can exceed the useful cache working set and incur eviction/recomputation.

Full viewport copying/texture upload remains on changed frames. Foreground above an early moving object still redraws. Groups, dependent text, motion and resizing retain full-scene rendering. Raster stroke setup/commit and undo still keep full before/after image data, and inactive image-heavy documents retain their pixels/history. First selection resampling and new effects have cold costs. These changes do not establish a universal smoothness guarantee for arbitrarily large documents or tab sets.

## Reproduction

Build `omadesign`, `authoring_qa`, `interaction_qa` and `canvas_profile` locally with `cargo build --release -j 2 --bin omadesign --bin authoring_qa --bin interaction_qa --bin canvas_profile`. Preserve each side's executables in a separate directory. For the baseline, use an isolated checkout at the before commit and copy only `src/bin/authoring_qa.rs` and `src/bin/interaction_qa.rs` from the after commit before building.

Use the original fleet `inputs/infographic-seed.oma`, `inputs/type-tasks.json` and accompanying `.omabrand` font bank, matching [provenance.json](provenance.json). The interaction fixture is generated deterministically by its binary and needs no external assets. Use the preserved headless Sway configuration with `WLR_BACKENDS=headless`, `WLR_RENDERER=gles2` and `WLR_RENDER_DRM_DEVICE=/dev/dri/renderD128` on this host. Point client `XDG_RUNTIME_DIR`, `WAYLAND_DISPLAY` and `SWAYSOCK` at that separate runtime, set `WINIT_UNIX_BACKEND=wayland`, and unset `DISPLAY`; do not pass the compositor's `WLR_*` variables to clients. Verify the GPU renderer on other hardware instead of assuming the same render node. Each output directory must be new or empty; the runner rejects existing profiles/results, duplicate scenarios, nonpositive rounds and nonfinite or nonpositive timeouts. Run one side at a time:

```sh
python docs/qa/performance-158/run.py \
  --bin-dir /path/to/before/bin --output /path/to/before/results \
  --seed /path/to/inputs/infographic-seed.oma \
  --tasks /path/to/inputs/type-tasks.json --label caeda653
python docs/qa/performance-158/run.py \
  --bin-dir /path/to/after/bin --output /path/to/after/results \
  --seed /path/to/inputs/infographic-seed.oma \
  --tasks /path/to/inputs/type-tasks.json --label 54d7051c
python docs/qa/performance-158/summarize.py \
  /path/to/before/results /path/to/after/results /path/to/summary.json
```

The committed evidence retains raw measurements, process resources, validation logs and representative native screenshots. Generated profiles and executable files remain local. The installed release and accepted 0.5.0 human QA evidence are unchanged.
