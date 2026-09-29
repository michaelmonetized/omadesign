# Paired fleet performance: issue #158 baseline and 0.6.1 candidate

The selected 108 serial workload processes passed their recorded functional checks: 18 cases/repetitions per version on each of three hosts. Earlier failed and superseded attempts remain separately preserved; this is not an all-attempts-pass claim. The candidate source is `8ee01b4ea08e0dac9891694b82dd3f4f377a5cf2`; the retained issue #158 production baseline is `221834bdd8af5aee8554a0e835629cbe2f467558`. The baseline is **not the shipped v0.6.0 tag**. These tables do not support a blanket “0.6.1 is N× faster than 0.6.0” claim.
This is the intended 0.6.1 candidate; its package version still reports 0.6.0 pending release. Source and executable hashes identify the tested code, not that unchanged package label. Both sides use the original fleet release profile with LTO disabled; these [benchmark builds](build-validation/README.md) are not the final packaged 0.6.1 release binaries.

For brush dragging on these three tested hosts, median UI duration had baseline/candidate ratios of 2.87–3.03×; p95 ratios were 3.81–4.56×. This task-specific comparison is against the retained issue #158 baseline. The other authoring phases are reported separately, including unchanged or slower measurements; it is not an across-the-board authoring speedup.

HP native upscaling regressed: baseline runs 69.636/68.197 s, candidate 78.226/82.500 s; the whole-workflow median rose from 68.916 to 80.363 s (16.6% longer; 0.86× old/new). This observed workflow regression is retained without assigning a cause. The same runs reported lower aggregate UI p95 (89.78/78.60 → 5.34/5.81 ms), but sampled more UI calls (2678/2717 → 4350/4553); these differing distributions do not establish an isolated interaction-speed ratio. Core AI inference source is unchanged. The recorded inference phase includes preview/apply readiness, rather than an isolated model kernel. Resource observations do not establish whether the longer workflow reflects external load or self-contention.

Other observed increases include HP pen commit: median 40.658 → 49.377 ms and p95 44.189 → 100.087 ms, from only 6 calls per version; Intel 1024-pixel 2× CLI upscaling: 22.084 → 29.615 s median (34.1% longer, two observations per version). Portable reopen/capture has one observation per version: hpeliteclient 2.612 → 3.117 s; intelpro 1.404 → 2.006 s; m1pro16 1.417 → 1.805 s. These sparse observations are retained without assigning a cause; the full tables include smaller increases and individual runs.

Fresh same-host pairs alternate version order for successive case/repetition pairs on private GPU-backed Sway displays. Both use exact 1440×900 outer windows; the final recorded canvas is baseline 1020 × 770 and candidate 1014.71875 × 771 on all three hosts. The candidate inspector’s minimum width produces a 0.389% smaller final canvas area. Geometry is recorded only at the end; the inspector can expand during text actions after initial fit, so per-phase geometry and equal zoom are not asserted. Timings are not normalized by area. All authoring exports remain 1200×900, and common measured action counts match; the candidate adds hover/setup coordination calls. Artwork equality is checked from actual outputs. Inputs, project fonts, models and architecture-matched ORT 1.28 CPU libraries are hash-verified. The two x86 hosts use identical executable/runtime bytes. Hardware power policies and other applications remain as used; clocks, pressure and thermals are not controlled.

UI timings measure `Studio::ui` CPU wall duration, including descheduling. UI-completion intervals and input-to-UI measurements are retained; they are not physical input latency, GPU presentation time or displayed FPS. Recomputed medians use the conventional middle value and recomputed p95 uses nearest rank; native-AI p95 is separately labeled with its helper’s retained indexing rule. Every old/new ratio below divides the baseline duration by the candidate duration: above 1 means less candidate time, below 1 means more candidate time. Raw samples, maxima and individual repetitions remain available; no outliers are removed.

The original [HP/Intel runner](runner-versions/run_paired-fdinfo.py) and original [M1 runner](run_paired.py) differ only in GPU evidence collection. Recovered cases on all hosts use the latter frozen source; retained receipts preserve their original collector identities. The audit pins both source hashes and verifies identical parsed code outside `gpu_handles`. M1’s Asahi kernel lacks DRM fdinfo, so its collector additionally resolves sysfs driver identity from an actual open render-node character descriptor. Native GPU evidence remains required; workload commands, timing, scheduling and input actions are unchanged.

[Environment provenance and excluded preflight attempts](environment/README.md) retain the display configuration, GPU evidence, collector variants and calibration failures separately from the completed timing matrix.

Completed authoring and CLI measurements are retained byte-for-byte. All native AI cases were rerun on both versions and all hosts with identical QA helper source under `hover-atomic-buttons-held-sliders-escape-cancel-v2`: semantic buttons use hover plus atomic clicks, sliders retain separate held press/release frames, and cancellation uses the real Escape key. The Cancel button is not covered by this protocol. Background-removal runs assert initial Radius 12 and report a changed final value above 12. Native executable/build/source overrides and every retained, replaced or failed attempt are pinned in the continuation lineage; these native timings are a separate recovered QA cohort.

The native baseline rebuild uses checkout `587433037d1df9be5e66b36cc77bc6f44fcd7071`. Its 825 tracked production inputs match the retained `221834b` baseline exactly after excluding only documentation and the two standalone authoring/profiler files; the audit recomputes that source proof. Rebuilt native executable bytes are separately identified, rather than described as the original retained binaries. Authoring, CPU-render, CLI and portable-app measurements retain the original executable identities.

## Fresh paired authoring: main-phase medians

| Task: baseline → candidate (old/new) | hpeliteclient | intelpro | m1pro16 |
| --- | ---: | ---: | ---: |
| Brush drag | 62.344 → 21.193 ms (2.94×) | 45.255 → 15.783 ms (2.87×) | 27.785 → 9.165 ms (3.03×) |
| Typing | 41.986 → 41.373 ms (1.01×) | 30.603 → 30.325 ms (1.01×) | 18.805 → 18.551 ms (1.01×) |
| Pan | 54.778 → 53.776 ms (1.02×) | 40.313 → 40.625 ms (0.99×) | 24.419 → 23.815 ms (1.03×) |
| Zoom | 60.180 → 60.033 ms (1.00×) | 44.781 → 44.564 ms (1.00×) | 26.698 → 25.991 ms (1.03×) |

The ratios describe these tasks and hosts. Use their p95 and run variation below before drawing a public claim; a pooled median alone does not establish universal smoothness or statistical significance.

## Fresh paired measurements: hpeliteclient

Architecture `x86_64`, kernel `7.2.3-arch1-3`, DRM driver `amdgpu`. Selected measurements span `2026-09-29T03:28:12.376852+00:00` to `2026-09-29T04:23:12.063331+00:00`. At the original suite start: Linux-visible RAM 5.65 GiB; available RAM 2.35 GiB; occupied swap 1.80 GiB. Power-profile query: `balanced`. Occupied swap does not prove active paging; per-process faults, CPU policies and pressure are retained.

| Final recorded authoring canvas | Baseline | Candidate |
| --- | ---: | ---: |
| Run 1 | 1020.0 × 770.0 | 1014.71875 × 771.0 |
| Run 2 | 1020.0 × 770.0 | 1014.71875 × 771.0 |

| UI CPU ms | Baseline median/p95 | Candidate median/p95 | Median old/new | P95 old/new | Samples old/new |
| --- | ---: | ---: | ---: | ---: | ---: |
| Idle | 0.775 / 1.451 | 0.820 / 1.131 | 0.94× | 1.28× | 120 / 120 |
| Pen preview | 1.091 / 36.919 | 1.101 / 1.732 | 0.99× | 21.31× | 96 / 96 |
| Pen commit | 40.658 / 44.189 | 49.377 / 100.087 | 0.82× | 0.44× | 6 / 6 |
| Typing | 41.986 / 85.199 | 41.373 / 52.468 | 1.01× | 1.62× | 166 / 166 |
| Brush drag | 62.344 / 98.328 | 21.193 / 25.804 | 2.94× | 3.81× | 180 / 180 |
| Pan | 54.778 / 92.060 | 53.776 / 62.298 | 1.02× | 1.48× | 80 / 80 |
| Zoom | 60.180 / 99.305 | 60.033 / 71.235 | 1.00× | 1.39× | 120 / 120 |
| Save UI update | 161.086 / 167.330 | 153.154 / 173.272 | 1.05× | 0.97× | 2 / 2 |

| UI variation (ms) | Baseline run 1 / run 2 median | Candidate run 1 / run 2 median | Baseline max | Candidate max |
| --- | ---: | ---: | ---: | ---: |
| Idle | 0.744 / 0.791 | 0.822 / 0.816 | 43.332 | 1.734 |
| Pen preview | 1.073 / 1.102 | 0.992 / 1.262 | 41.977 | 1.943 |
| Pen commit | 41.167 / 40.148 | 43.152 / 52.642 | 44.189 | 100.087 |
| Typing | 40.551 / 43.545 | 41.463 / 41.127 | 111.672 | 78.409 |
| Brush drag | 61.906 / 63.438 | 21.094 / 21.370 | 115.787 | 30.448 |
| Pan | 54.390 / 55.620 | 53.602 / 53.884 | 103.336 | 65.910 |
| Zoom | 60.917 / 59.652 | 59.645 / 60.679 | 121.929 | 75.677 |
| Save UI update | 167.330 / 154.843 | 173.272 / 133.036 | 167.330 | 173.272 |

| Native interval ms (not presented FPS) | Baseline median/p95 | Candidate median/p95 | Median old/new |
| --- | ---: | ---: | ---: |
| Typing: UI-completion interval | 44.365 / 88.103 | 43.717 / 55.145 | 1.01× |
| Typing: Input-to-UI | 42.038 / 85.257 | 41.435 / 52.519 | 1.01× |
| Brush drag: UI-completion interval | 64.724 / 100.543 | 23.619 / 28.245 | 2.74× |
| Brush drag: Input-to-UI | 62.394 / 98.373 | 21.256 / 25.850 | 2.94× |

| Whole process: runs 1 / 2 | Baseline | Candidate |
| --- | ---: | ---: |
| Authoring whole-process seconds | 24.943 / 25.246 | 21.245 / 20.543 |
| Authoring peak RSS MiB | 237.734 / 238.672 | 240.008 / 239.008 |
| Authoring major page faults | 14.000 / 23.000 | 123.000 / 39.000 |

| CPU-only 1456 × 900 render | Baseline ms | Candidate ms | Old/new |
| --- | ---: | ---: | ---: |
| Warm median (three calls, one process) | 59.809 | 58.069 | 1.03× |
| First full-scene call (after layer attribution) | 75.330 | 67.844 | 1.11× |

The CPU profiler renders every individual layer four times before full-scene rounds 0–3. Its first full-scene call is not a cold-process/cache measurement; the warm median uses full-scene rounds 1–3.

| Whole-process wall seconds | Baseline observations | Candidate observations | Median old→new | Median old/new | Mechanical p95 old→new |
| --- | ---: | ---: | ---: | ---: | ---: |
| AI 2×: 320×212 → 640×424 | 5.315 / 5.423 | 5.625 / 5.727 | 5.369 → 5.676 | 0.95× | 5.423 → 5.727 |
| AI 4×: 320×212 → 1280×848 | 5.517 / 5.634 | 5.526 / 5.417 | 5.575 → 5.472 | 1.02× | 5.634 → 5.526 |
| AI 2×: 1024×678 → 2048×1356 | 71.582 / 72.111 | 71.256 / 73.099 | 71.847 → 72.177 | 1.00× | 72.111 → 73.099 |
| Background removal 320×212 | 1.511 / 1.204 | 1.609 / 1.005 | 1.358 → 1.307 | 1.04× | 1.511 → 1.609 |
| Background removal 1024×678 | 10.238 / 9.541 | 10.642 / 10.044 | 9.889 → 10.343 | 0.96× | 10.238 → 10.642 |
| Native background-removal workflow | 9.141 / 9.334 | 11.502 / 7.630 | 9.237 → 9.566 | 0.97× | 9.334 → 11.502 |
| Native upscaling workflow | 69.636 / 68.197 | 78.226 / 82.500 | 68.916 → 80.363 | 0.86× | 69.636 → 82.500 |
| Portable app reopen/capture | 2.612 | 3.117 | 2.612 → 3.117 | 0.84× | 2.612 → 3.117 |

| Peak whole-process RSS MiB | Baseline observations | Candidate observations |
| --- | ---: | ---: |
| AI 2×: 320×212 → 640×424 | 100.195 / 100.746 | 100.602 / 100.648 |
| AI 4×: 320×212 → 1280×848 | 101.195 / 101.176 | 101.480 / 101.449 |
| AI 2×: 1024×678 → 2048×1356 | 136.645 / 136.613 | 136.910 / 137.340 |
| Background removal 320×212 | 316.195 / 316.406 | 316.270 / 316.719 |
| Background removal 1024×678 | 528.020 / 527.930 | 528.309 / 528.176 |
| Native background-removal workflow | 439.859 / 441.508 | 439.184 / 440.133 |
| Native upscaling workflow | 653.789 / 649.059 | 652.039 / 643.016 |
| Portable app reopen/capture | 148.070 | 152.016 |

| Native AI UI, harness-reported ms | Baseline p95/max | Candidate p95/max | Calls old/new |
| --- | ---: | ---: | ---: |
| Native background-removal workflow run 1 | 34.331 / 146.544 | 6.651 / 77.300 | 250 / 308 |
| Native background-removal workflow run 2 | 38.725 / 118.520 | 4.067 / 61.440 | 244 / 269 |
| Native upscaling workflow run 1 | 89.782 / 232.393 | 5.339 / 78.663 | 2678 / 4350 |
| Native upscaling workflow run 2 | 78.601 / 231.555 | 5.807 / 75.185 | 2717 / 4553 |

CLI/native AI whole-process timing includes startup, model loading, workflow waits and encoding; background removal also writes comparison variants. Process polling granularity is 100 ms. AI cases have two observations, so process p95 equals the larger observation; it is not a stable tail estimate. Portable reopen has only one observation per side, making its median/p95/max the same observation. Native-AI UI raw frame samples are unavailable. Its helper selects zero-based sorted index floor(0.95×n); the per-run p95/max above are neither pooled nor presented as recomputed nearest-rank distributions. Models remain Real-ESRGAN General x4v3 and U²-NetP on ORT 1.28 CPU. Timing differences are as-used pipeline measurements, not proof of an inference-algorithm speedup.
The pinned core background-removal, upscaling and ML source files are identical between these revisions. These AI timings validate the unchanged pipeline and UI workflow; no AI inference-algorithm optimization is claimed.

## Fresh paired measurements: intelpro

Architecture `x86_64`, kernel `7.2.6-arch2-Watanare-T2-4-t2`, DRM driver `i915`. Selected measurements span `2026-09-29T03:28:12.918699+00:00` to `2026-09-29T04:19:15.774225+00:00`. At the original suite start: Linux-visible RAM 7.59 GiB; available RAM 2.94 GiB; occupied swap 1.48 GiB. Power-profile query: `performance`. Occupied swap does not prove active paging; per-process faults, CPU policies and pressure are retained.

| Final recorded authoring canvas | Baseline | Candidate |
| --- | ---: | ---: |
| Run 1 | 1020.0 × 770.0 | 1014.71875 × 771.0 |
| Run 2 | 1020.0 × 770.0 | 1014.71875 × 771.0 |

| UI CPU ms | Baseline median/p95 | Candidate median/p95 | Median old/new | P95 old/new | Samples old/new |
| --- | ---: | ---: | ---: | ---: | ---: |
| Idle | 0.577 / 0.980 | 0.590 / 0.771 | 0.98× | 1.27× | 120 / 120 |
| Pen preview | 0.794 / 1.001 | 0.735 / 0.886 | 1.08× | 1.13× | 96 / 96 |
| Pen commit | 34.122 / 37.575 | 32.750 / 34.840 | 1.04× | 1.08× | 6 / 6 |
| Typing | 30.603 / 49.958 | 30.325 / 32.674 | 1.01× | 1.53× | 166 / 166 |
| Brush drag | 45.255 / 66.236 | 15.783 / 16.589 | 2.87× | 3.99× | 180 / 180 |
| Pan | 40.313 / 60.318 | 40.625 / 50.567 | 0.99× | 1.19× | 80 / 80 |
| Zoom | 44.781 / 64.097 | 44.564 / 54.312 | 1.00× | 1.18× | 120 / 120 |
| Save UI update | 211.936 / 218.498 | 195.891 / 204.085 | 1.08× | 1.07× | 2 / 2 |

| UI variation (ms) | Baseline run 1 / run 2 median | Candidate run 1 / run 2 median | Baseline max | Candidate max |
| --- | ---: | ---: | ---: | ---: |
| Idle | 0.574 / 0.589 | 0.593 / 0.589 | 20.656 | 1.535 |
| Pen preview | 0.804 / 0.781 | 0.727 / 0.744 | 21.189 | 1.092 |
| Pen commit | 37.439 / 33.710 | 32.703 / 32.796 | 37.575 | 34.840 |
| Typing | 30.639 / 30.542 | 30.119 / 30.455 | 53.999 | 34.851 |
| Brush drag | 45.239 / 45.379 | 15.804 / 15.749 | 68.939 | 16.961 |
| Pan | 40.184 / 40.328 | 40.953 / 40.324 | 61.956 | 75.511 |
| Zoom | 44.920 / 44.637 | 43.980 / 45.306 | 67.065 | 72.263 |
| Save UI update | 205.374 / 218.498 | 187.697 / 204.085 | 218.498 | 204.085 |

| Native interval ms (not presented FPS) | Baseline median/p95 | Candidate median/p95 | Median old/new |
| --- | ---: | ---: | ---: |
| Typing: UI-completion interval | 32.398 / 51.858 | 32.149 / 34.520 | 1.01× |
| Typing: Input-to-UI | 30.638 / 49.990 | 30.362 / 32.704 | 1.01× |
| Brush drag: UI-completion interval | 47.096 / 67.957 | 17.553 / 18.312 | 2.68× |
| Brush drag: Input-to-UI | 45.286 / 66.267 | 15.811 / 16.623 | 2.86× |

| Whole process: runs 1 / 2 | Baseline | Candidate |
| --- | ---: | ---: |
| Authoring whole-process seconds | 19.323 / 19.324 | 15.719 / 15.820 |
| Authoring peak RSS MiB | 232.148 / 232.691 | 241.082 / 238.020 |
| Authoring major page faults | 22.000 / 65.000 | 14.000 / 8.000 |

| CPU-only 1456 × 900 render | Baseline ms | Candidate ms | Old/new |
| --- | ---: | ---: | ---: |
| Warm median (three calls, one process) | 46.373 | 46.281 | 1.00× |
| First full-scene call (after layer attribution) | 61.484 | 53.818 | 1.14× |

The CPU profiler renders every individual layer four times before full-scene rounds 0–3. Its first full-scene call is not a cold-process/cache measurement; the warm median uses full-scene rounds 1–3.

| Whole-process wall seconds | Baseline observations | Candidate observations | Median old→new | Median old/new | Mechanical p95 old→new |
| --- | ---: | ---: | ---: | ---: | ---: |
| AI 2×: 320×212 → 640×424 | 1.402 / 1.402 | 1.402 / 1.402 | 1.402 → 1.402 | 1.00× | 1.402 → 1.402 |
| AI 4×: 320×212 → 1280×848 | 1.302 / 1.302 | 1.302 / 1.302 | 1.302 → 1.302 | 1.00× | 1.302 → 1.302 |
| AI 2×: 1024×678 → 2048×1356 | 22.234 / 21.934 | 21.345 / 37.885 | 22.084 → 29.615 | 0.75× | 22.234 → 37.885 |
| Background removal 320×212 | 1.506 / 0.401 | 1.303 / 0.501 | 0.954 → 0.902 | 1.06× | 1.506 → 1.303 |
| Background removal 1024×678 | 6.414 / 4.510 | 4.409 / 4.409 | 5.462 → 4.409 | 1.24× | 6.414 → 4.409 |
| Native background-removal workflow | 6.010 / 6.110 | 6.313 / 5.311 | 6.060 → 5.812 | 1.04× | 6.110 → 6.313 |
| Native upscaling workflow | 19.224 / 21.533 | 18.623 / 20.937 | 20.378 → 19.780 | 1.03× | 21.533 → 20.937 |
| Portable app reopen/capture | 1.404 | 2.006 | 1.404 → 2.006 | 0.70× | 1.404 → 2.006 |

| Peak whole-process RSS MiB | Baseline observations | Candidate observations |
| --- | ---: | ---: |
| AI 2×: 320×212 → 640×424 | 100.191 / 100.633 | 99.852 / 100.586 |
| AI 4×: 320×212 → 1280×848 | 101.145 / 101.145 | 101.336 / 101.441 |
| AI 2×: 1024×678 → 2048×1356 | 136.684 / 136.516 | 136.785 / 137.035 |
| Background removal 320×212 | 315.902 / 316.625 | 316.102 / 316.199 |
| Background removal 1024×678 | 528.289 / 528.199 | 528.445 / 528.312 |
| Native background-removal workflow | 444.664 / 443.305 | 424.586 / 444.676 |
| Native upscaling workflow | 658.160 / 657.703 | 651.613 / 653.844 |
| Portable app reopen/capture | 149.953 | 151.652 |

| Native AI UI, harness-reported ms | Baseline p95/max | Candidate p95/max | Calls old/new |
| --- | ---: | ---: | ---: |
| Native background-removal workflow run 1 | 19.142 / 53.882 | 1.821 / 44.107 | 202 / 203 |
| Native background-removal workflow run 2 | 19.427 / 53.015 | 1.721 / 42.918 | 203 / 203 |
| Native upscaling workflow run 1 | 11.680 / 55.331 | 1.191 / 37.883 | 1029 / 1025 |
| Native upscaling workflow run 2 | 16.148 / 204.308 | 4.755 / 34.949 | 1081 / 1159 |

CLI/native AI whole-process timing includes startup, model loading, workflow waits and encoding; background removal also writes comparison variants. Process polling granularity is 100 ms. AI cases have two observations, so process p95 equals the larger observation; it is not a stable tail estimate. Portable reopen has only one observation per side, making its median/p95/max the same observation. Native-AI UI raw frame samples are unavailable. Its helper selects zero-based sorted index floor(0.95×n); the per-run p95/max above are neither pooled nor presented as recomputed nearest-rank distributions. Models remain Real-ESRGAN General x4v3 and U²-NetP on ORT 1.28 CPU. Timing differences are as-used pipeline measurements, not proof of an inference-algorithm speedup.
The pinned core background-removal, upscaling and ML source files are identical between these revisions. These AI timings validate the unchanged pipeline and UI workflow; no AI inference-algorithm optimization is claimed.

## Fresh paired measurements: m1pro16

Architecture `aarch64`, kernel `7.1.13-3-2-ARCH`, DRM driver `asahi`. Selected measurements span `2026-09-29T03:32:34.785699+00:00` to `2026-09-29T04:18:20.236932+00:00`. At the original suite start: Linux-visible RAM 15.03 GiB; available RAM 4.20 GiB; occupied swap 14.65 GiB. Power-profile query: `balanced`. Occupied swap does not prove active paging; per-process faults, CPU policies and pressure are retained.

| Final recorded authoring canvas | Baseline | Candidate |
| --- | ---: | ---: |
| Run 1 | 1020.0 × 770.0 | 1014.71875 × 771.0 |
| Run 2 | 1020.0 × 770.0 | 1014.71875 × 771.0 |

| UI CPU ms | Baseline median/p95 | Candidate median/p95 | Median old/new | P95 old/new | Samples old/new |
| --- | ---: | ---: | ---: | ---: | ---: |
| Idle | 0.255 / 0.792 | 0.298 / 0.421 | 0.86× | 1.88× | 120 / 120 |
| Pen preview | 0.341 / 0.749 | 0.347 / 0.577 | 0.98× | 1.30× | 96 / 96 |
| Pen commit | 20.142 / 21.197 | 19.575 / 20.511 | 1.03× | 1.03× | 6 / 6 |
| Typing | 18.805 / 32.409 | 18.551 / 19.514 | 1.01× | 1.66× | 166 / 166 |
| Brush drag | 27.785 / 43.052 | 9.165 / 9.443 | 3.03× | 4.56× | 180 / 180 |
| Pan | 24.419 / 25.349 | 23.815 / 24.151 | 1.03× | 1.05× | 80 / 80 |
| Zoom | 26.698 / 40.558 | 25.991 / 27.438 | 1.03× | 1.48× | 120 / 120 |
| Save UI update | 87.434 / 107.510 | 86.571 / 101.899 | 1.01× | 1.06× | 2 / 2 |

| UI variation (ms) | Baseline run 1 / run 2 median | Candidate run 1 / run 2 median | Baseline max | Candidate max |
| --- | ---: | ---: | ---: | ---: |
| Idle | 0.266 / 0.248 | 0.240 / 0.328 | 17.922 | 1.192 |
| Pen preview | 0.335 / 0.356 | 0.294 / 0.384 | 25.592 | 0.687 |
| Pen commit | 19.984 / 20.300 | 19.611 / 19.539 | 21.197 | 20.511 |
| Typing | 18.817 / 18.757 | 18.268 / 18.872 | 46.819 | 21.275 |
| Brush drag | 27.715 / 27.867 | 9.168 / 9.163 | 52.793 | 9.836 |
| Pan | 24.393 / 24.451 | 23.759 / 23.847 | 41.900 | 24.443 |
| Zoom | 26.740 / 26.607 | 26.004 / 25.991 | 47.099 | 29.013 |
| Save UI update | 107.510 / 67.359 | 71.242 / 101.899 | 107.510 | 101.899 |

| Native interval ms (not presented FPS) | Baseline median/p95 | Candidate median/p95 | Median old/new |
| --- | ---: | ---: | ---: |
| Typing: UI-completion interval | 19.592 / 33.174 | 19.386 / 20.354 | 1.01× |
| Typing: Input-to-UI | 18.825 / 32.424 | 18.566 / 19.532 | 1.01× |
| Brush drag: UI-completion interval | 28.670 / 43.810 | 16.117 / 16.382 | 1.78× |
| Brush drag: Input-to-UI | 27.805 / 43.067 | 9.185 / 9.497 | 3.03× |

| Whole process: runs 1 / 2 | Baseline | Candidate |
| --- | ---: | ---: |
| Authoring whole-process seconds | 16.264 / 12.329 | 11.230 / 11.023 |
| Authoring peak RSS MiB | 222.906 / 221.734 | 227.500 / 228.141 |
| Authoring major page faults | 468.000 / 12.000 | 17.000 / 16.000 |

| CPU-only 1456 × 900 render | Baseline ms | Candidate ms | Old/new |
| --- | ---: | ---: | ---: |
| Warm median (three calls, one process) | 28.226 | 27.936 | 1.01× |
| First full-scene call (after layer attribution) | 34.358 | 30.311 | 1.13× |

The CPU profiler renders every individual layer four times before full-scene rounds 0–3. Its first full-scene call is not a cold-process/cache measurement; the warm median uses full-scene rounds 1–3.

| Whole-process wall seconds | Baseline observations | Candidate observations | Median old→new | Median old/new | Mechanical p95 old→new |
| --- | ---: | ---: | ---: | ---: | ---: |
| AI 2×: 320×212 → 640×424 | 1.203 / 1.104 | 1.204 / 1.105 | 1.153 → 1.154 | 1.00× | 1.203 → 1.204 |
| AI 4×: 320×212 → 1280×848 | 1.109 / 1.104 | 1.105 / 1.104 | 1.106 → 1.104 | 1.00× | 1.109 → 1.105 |
| AI 2×: 1024×678 → 2048×1356 | 12.218 / 12.118 | 12.213 / 12.215 | 12.168 → 12.214 | 1.00× | 12.218 → 12.215 |
| Background removal 320×212 | 0.403 / 0.304 | 0.303 / 0.306 | 0.354 → 0.304 | 1.16× | 0.403 → 0.306 |
| Background removal 1024×678 | 2.003 / 2.006 | 2.004 / 2.005 | 2.004 → 2.005 | 1.00× | 2.006 → 2.005 |
| Native background-removal workflow | 8.244 / 5.324 | 9.670 / 4.217 | 6.784 → 6.943 | 0.98× | 8.244 → 9.670 |
| Native upscaling workflow | 13.337 / 15.553 | 13.026 / 13.338 | 14.445 → 13.182 | 1.10× | 15.553 → 13.338 |
| Portable app reopen/capture | 1.417 | 1.805 | 1.417 → 1.805 | 0.79× | 1.417 → 1.805 |

| Peak whole-process RSS MiB | Baseline observations | Candidate observations |
| --- | ---: | ---: |
| AI 2×: 320×212 → 640×424 | 77.766 / 76.266 | 76.719 / 76.531 |
| AI 4×: 320×212 → 1280×848 | 81.297 / 78.750 | 79.500 / 79.469 |
| AI 2×: 1024×678 → 2048×1356 | 113.156 / 112.766 | 111.641 / 112.750 |
| Background removal 320×212 | 237.344 / 238.250 | 237.922 / 238.797 |
| Background removal 1024×678 | 443.859 / 443.828 | 443.875 / 443.375 |
| Native background-removal workflow | 334.922 / 335.328 | 335.734 / 337.859 |
| Native upscaling workflow | 544.844 / 543.703 | 544.281 / 546.219 |
| Portable app reopen/capture | 145.594 | 148.312 |

| Native AI UI, harness-reported ms | Baseline p95/max | Candidate p95/max | Calls old/new |
| --- | ---: | ---: | ---: |
| Native background-removal workflow run 1 | 15.193 / 78.258 | 1.084 / 58.923 | 184 / 192 |
| Native background-removal workflow run 2 | 14.370 / 36.414 | 1.065 / 28.437 | 184 / 184 |
| Native upscaling workflow run 1 | 7.454 / 51.801 | 0.734 / 34.450 | 744 / 741 |
| Native upscaling workflow run 2 | 7.465 / 77.859 | 0.753 / 32.208 | 740 / 748 |

CLI/native AI whole-process timing includes startup, model loading, workflow waits and encoding; background removal also writes comparison variants. Process polling granularity is 100 ms. AI cases have two observations, so process p95 equals the larger observation; it is not a stable tail estimate. Portable reopen has only one observation per side, making its median/p95/max the same observation. Native-AI UI raw frame samples are unavailable. Its helper selects zero-based sorted index floor(0.95×n); the per-run p95/max above are neither pooled nor presented as recomputed nearest-rank distributions. Models remain Real-ESRGAN General x4v3 and U²-NetP on ORT 1.28 CPU. Timing differences are as-used pipeline measurements, not proof of an inference-algorithm speedup.
The pinned core background-removal, upscaling and ML source files are identical between these revisions. These AI timings validate the unchanged pipeline and UI workflow; no AI inference-algorithm optimization is claimed.

## Correctness and observed output differences

All 108 selected process checks passed, including authoring/history/save-reopen and the recovered native AI Escape-cancel/preview/apply/history/persistence/export checks. This excludes rather than erases failed/superseded attempts, and does not cover the native Cancel button. Separately, 380 decoded-RGBA image pairs and 60 saved-document pairs were examined for repeatability, same-host before/after and same-version cross-host equivalence. 230 image pairs were exact; 15 document pairs matched bytes; 11 additional document pairs passed the strict fresh-ID-only proof. Functional checks do not make nonidentical outputs pixel-identical.

| Comparison | Exact image pairs | Dimension differences | Largest channel delta/255 | Exact document bytes |
| --- | ---: | ---: | ---: | ---: |
| repeatability | 114/114 | 0 | 0 | 15/18 |
| before_after | 108/114 | 0 | 15 | 0/18 |
| cross_host | 8/152 | 0 | 2 | 0/24 |

The JSON preserves every image difference and raw document byte/ID-only result. A separate additive current-schema proof resolves 18 further document pairs by initializing only verified missing defaults before checking IDs and all remaining data. The [output acceptance note](OUTPUT-ACCEPTANCE.md) records the preserved authored state, nine changed authoring pixels and pre-existing cross-architecture AI deltas. No unknown field is discarded and no output is relabeled pixel-identical. Native editor screenshots are dimension-checked, not compared as artwork.

## Historical original table, retained separately

These are the original 2026-09-27 user-desktop measurements from the published issue #158 evidence. They are not fresh pairs, use the earlier surrounding desktop conditions, and are never pooled with the new private-Sway measurements or used to calculate the speedups above.

| Historical UI median/p95 ms | hpeliteclient | intelpro | m1pro16 |
| --- | ---: | ---: | ---: |
| Idle | 2.591 / 67.289 | 1.175 / 2.696 | 0.288 / 0.427 |
| Pen preview | 2.198 / 4.694 | 1.418 / 1.846 | 0.300 / 0.593 |
| Pen commit | 97.534 / 141.298 | 34.679 / 53.393 | 20.256 / 33.150 |
| Typing | 82.047 / 560.552 | 31.480 / 51.164 | 18.827 / 19.637 |
| Brush drag | 97.139 / 184.490 | 45.998 / 66.877 | 27.673 / 41.451 |
| Pan | 79.499 / 144.310 | 41.336 / 61.443 | 24.365 / 37.584 |
| Zoom | 147.583 / 442.663 | 46.664 / 67.285 | 26.655 / 38.795 |
| Save UI update | 466.732 / 699.183 | 204.449 / 205.369 | 84.347 / 89.414 |

| Historical single process wall seconds | hpeliteclient | intelpro | m1pro16 |
| --- | ---: | ---: | ---: |
| AI 2×: 320×212 → 640×424 | 9.555 | 1.202 | 1.102 |
| AI 4×: 320×212 → 1280×848 | 16.193 | 1.202 | 1.104 |
| AI 2×: 1024×678 → 2048×1356 | 166.348 | 15.123 | 12.118 |
| Background removal 320×212 | 5.254 | 0.401 | 0.302 |
| Background removal 1024×678 | 32.014 | 2.704 | 2.004 |
| Native background-removal workflow | 24.863 | 6.507 | 3.404 |
| Native upscaling workflow | 153.081 | 19.920 | 11.012 |
| Portable app reopen/capture | 11.717 | 3.104 | 1.302 |

| Historical warm CPU render median ms | hpeliteclient | intelpro | m1pro16 |
| --- | ---: | ---: | ---: |
| 1456×900 full scene | 104.758 | 46.053 | 28.150 |

[Original complete report and hardware conditions](../fleet-benchmark-2026-09-27/README.md) remain authoritative for that earlier table.

## Reproducibility and claim boundaries

[Full raw summary and comparisons](summary.json) SHA-256: `99a8d21fdf7ace87ac15115f55219fb47f8683fdfc2ad5e96c670d513d7cf187`. [Runner](run_paired.py), [continuation tool](continue_paired.py), [lineage audit](audit_continuation.py), [aggregator](summarize_paired.py), [report generator](make_report.py), and [fresh-ID proof](document_id_equivalence.py) are retained. The JSON pins the completed candidate build receipt and each host’s immutable build snapshot, original input/font/model/runtime identities, source result hashes, native recovery overrides, exact binary identities and all selected per-process resource samples. Architecture readiness snapshots retain their own identity; every captured successful step and artifact must match the completed global build exactly. Continuation lineage preserves prior successful, superseded and failed attempts separately.

The `claim_candidates` array lists each original-table authoring task on each host, including unchanged/slower measurements, run-median ranges, p95 support, sample counts and exact final canvas dimensions. Its wording explicitly names the issue #158 baseline and metric. These are review candidates, not a blanket release claim. Two runs, differing canvas/view geometry and uncontrolled system pressure limit inference. The 0.389% smaller candidate canvas describes only its final recorded area; no analytical timing adjustment is applied. Performance ratios do not imply presented FPS or universal hardware guidance. Keep the original baseline distinct from the shipped v0.6.0 tag and preserve all observed regressions.
