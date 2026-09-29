# Final interaction regression comparison

This checks the motion/cold-path/theme follow-up against the already optimized
interaction implementation at `54d7051c5c878c741ea5f5a5bb5bb030e0f4a6a6`. The final
candidate is `35c39093b8e8361a7970909bede8d5114e26022c`. It is separate from the
original issue #158 before/after comparison.

Both versions completed two rounds of all six native WGPU scenarios in the
same 1440 × 900 isolated Sway window. Each drag scenario supplies 360 move
samples per run; authoring supplies 90 brush samples. Timing includes all
samples, with no outlier removal. Medians average the middle pair when needed;
p95 uses nearest rank. Results are an as-used sequential local comparison,
without controlled clocks, thermals or randomized order.

`ui_ms` measures `Studio::ui` CPU wall duration. `input_to_ui_ms` begins at native
input injection. `frame_interval_ms` measures successive native UI-call
completions; these harnesses do not distinguish multiple UI calls within one
native callback. None measures presented FPS or physical input/display latency.

| Interaction | UI median, before → final (ms) | UI p95 (ms) | UI maximum (ms) |
| --- | ---: | ---: | ---: |
| Simple drag | 0.575 → 0.598 | 1.219 → 0.778 | 29.992 → 1.981 |
| Complex drag, top object | 0.619 → 0.623 | 1.302 → 0.769 | 41.129 → 1.838 |
| Complex drag, middle object | 8.526 → 8.761 | 9.322 → 9.228 | 28.145 → 11.510 |
| Complex drag, same layer | 0.594 → 0.652 | 1.027 → 0.778 | 21.744 → 2.106 |
| Drag with 24 open tabs | 0.565 → 0.620 | 1.200 → 0.791 | 25.774 → 1.765 |
| Brush | 9.128 → 9.168 | 9.605 → 9.419 | 34.348 → 9.601 |
| Typing | 18.042 → 18.009 | 33.861 → 18.732 | 37.643 → 19.245 |
| Pan | 23.630 → 23.697 | 38.219 → 24.312 | 58.300 → 28.055 |
| Zoom | 25.889 → 25.879 | 40.516 → 27.195 | 60.468 → 27.749 |

All five drag scenarios and brush improved pooled p95 and maximum UI times,
as well as p95 and maximum UI-completion intervals. Every individual run's
whole-run UI maximum decreased. The first UI call decreased in all twelve
runs. These observations include the original startup samples; they do not
measure total application launch time.

Some measurements increased. Middle-object drag median rose 0.235 ms (2.76%);
the other drag medians rose between 0.004 and 0.058 ms, and brush median rose
0.040 ms. First move/brush calls were not uniformly faster: the largest paired
increase was 0.441 ms for middle-object drag. Among non-coordination phases,
the largest UI p95/maximum increase was same-layer drag release, 21.206 →
22.452 ms (+1.246 ms; eight samples per side). Complex redo p95 increased
0.912 ms and middle-object release p95 increased 0.896 ms. All increases,
including small-sample setup/history phases, remain in the JSON reports. No
automatic performance acceptance threshold or significance claim is applied.

All 24 runs passed their behavioral checks, including measured canvas/object
updates, tool operations, history and save/reopen. Twenty-two of the 24 saved
documents match the baseline reference byte-for-byte. Both final authoring
documents differ only in the same seven newly allocated entity IDs. The
fail-closed proof preserves 586 seed IDs, validates 597 unique entities and
one consistent bijection, and checks every other value/type/array position.
This fixture has no persisted reference edges or unclassified changed-ID
occurrences. The raw byte mismatches and complete ID maps remain reported.
All four authoring exports match by decoded RGBA pixels (1200 × 900), with
zero differing pixels. Native editor screenshots are checked for dimensions,
not compared at arbitrary UI states.

- [Raw measurements and integrity proofs](interaction-results.json) include
  every timing sample and per-run/pooled statistics.
- [First samples and maximum-cost review](interaction-review.json) recomputes
  the statistics from those raw measurements and identifies maximum sample
  indices and phases.
- [Publication manifest](publication-manifest.json) pins binary, source,
  exporter and result hashes. Final binary identities were checked against
  the successful release receipt before export.

The public files contain no document contents, client artwork, screenshots,
font bytes, private paths, commands or process environments. Raw timings allow
the published statistics to be recomputed without the private fixtures. The
pixel and saved-document proofs were checked against retained local artifacts;
repeating those checks requires those original fixtures.
