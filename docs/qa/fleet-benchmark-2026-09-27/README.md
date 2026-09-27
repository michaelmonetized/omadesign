# Fleet performance baseline: ordinary authoring and offline AI

Measured on 2026-09-27 for PR #147 / issue #145, using the same application code and identical fixtures. All three hosts completed every functional check. The HP still has long interaction stalls and slow AI processing; Intelpro and m1pro16 are substantially faster. This is an as-used fleet comparison, with existing applications and power policies preserved, not a controlled hardware laboratory benchmark.

## Hardware and conditions

| Measurement | hpeliteclient | intelpro | m1pro16 |
| --- | ---: | ---: | ---: |
| Machine | HP Elite t655 Thin Client | Apple MacBookPro15,4 | Apple MacBook Pro 16-inch, M1 Pro, 2021 |
| CPU | AMD Ryzen Embedded R2314 | Intel Core i5-8257U | Apple M1 Pro |
| Physical cores / threads | 4 / 4 | 4 / 8 | 10 / 10 (8 performance + 2 efficiency) |
| Architecture | x86_64 | x86_64 | aarch64 |
| RAM visible to Linux | 5.65 GiB | 7.59 GiB | 15.03 GiB |
| RAM available at suite start | 0.73 GiB | 3.16 GiB | 4.42 GiB |
| Swap occupied at suite start | 3.55 GiB | 0.98 GiB | 9.74 GiB |
| One-minute system load at suite start | 1.63 | 0.02 | 1.23 |
| Graphics / active kernel driver | Radeon Vega / amdgpu; 2 GiB reported GPU memory | Iris Plus 645 / i915 | M1 Pro integrated GPU / asahi |
| Mesa / Vulkan driver package | 26.2.2 / vulkan-radeon | 26.2.2 / vulkan-intel | 26.2.3 / vulkan-asahi |
| Power profile | balanced | performance | balanced |
| Configured CPU frequency ceiling | 2.10 GHz (hardware reports up to 3.50 GHz) | 3.90 GHz | 3.036 GHz performance / 2.064 GHz efficiency |
| Kernel | 7.2.3-arch1-3 | 7.2.6-arch2-Watanare-T2-4-t2 | 7.1.13-3-2-ARCH |
| Native authoring window / canvas | 1440 × 900 / 1020 × 770 | 1440 × 900 / 1020 × 770 | 1440 × 900 / 1020 × 770 |
| Runtime / inference provider | ONNX Runtime 1.28 / CPU | ONNX Runtime 1.28 / CPU | ONNX Runtime 1.28 / CPU |
| Per-test limits / networking | No CPU or memory ceiling; network disabled | No CPU or memory ceiling; network disabled | No CPU or memory ceiling; network disabled |

RAM above is OS-usable memory, not a DIMM inventory. The HP GPU reports a separate 2 GiB allocation; do not describe the machine as having a verified 6 GB of installed RAM. Available RAM, swap, CPU frequencies, load and pressure were also sampled before and after every process. Occupied swap alone does not establish active paging; process major-fault counts and pressure samples are retained. No host power or desktop configuration was changed.

## Ordinary authoring: pooled median / p95 UI milliseconds

Two independent runs per host on the same editable 1200 × 900 infographic: 20 layers, 576 final vector shapes including 79 live text objects, and one brush layer. The illustration/card seed was imported as editable SVG; the measured workload uses actual native input to type five headline objects (83 character events), draw three Pen paths and three Brush strokes, pan, zoom, undo/redo and save. This is not a human design-time measurement or an entirely manual reconstruction.

| Measurement | hpeliteclient | intelpro | m1pro16 |
| --- | ---: | ---: | ---: |
| Idle | 2.59 / 67.29 | 1.18 / 2.70 | 0.29 / 0.43 |
| Pen preview | 2.20 / 4.69 | 1.42 / 1.85 | 0.30 / 0.59 |
| Pen commit | 97.53 / 141.30 | 34.68 / 53.39 | 20.26 / 33.15 |
| Typing | 82.05 / 560.55 | 31.48 / 51.16 | 18.83 / 19.64 |
| Brush drag | 97.14 / 184.49 | 46.00 / 66.88 | 27.67 / 41.45 |
| Pan | 79.50 / 144.31 | 41.34 / 61.44 | 24.36 / 37.58 |
| Zoom | 147.58 / 442.66 | 46.66 / 67.28 | 26.66 / 38.80 |
| Save UI update | 466.73 / 699.18 | 204.45 / 205.37 | 84.35 / 89.41 |
| Typing medians, runs 1 / 2 | 61.75 ms / 100.77 ms | 31.56 ms / 31.21 ms | 18.74 ms / 18.89 ms |
| Brush medians, runs 1 / 2 | 110.09 ms / 86.94 ms | 46.08 ms / 45.79 ms | 27.67 ms / 27.68 ms |
| Worst typing update | 1534.73 ms | 73.33 ms | 34.63 ms |
| Typing frame interval median / p95 | 90.06 / 572.11 ms | 35.01 / 54.65 ms | 21.15 / 33.54 ms |
| Brush frame interval median / p95 | 103.70 / 192.26 ms | 49.51 / 70.48 ms | 32.15 / 46.55 ms |
| Authoring whole-process time, runs 1 / 2 | 53.31 s / 69.93 s | 21.53 s / 20.92 s | 14.82 s / 14.01 s |
| Authoring peak process RSS, runs 1 / 2 | 203.8 MiB / 167.1 MiB | 234.1 MiB / 234.0 MiB | 226.4 MiB / 228.7 MiB |
| Authoring major page faults, runs 1 / 2 | 8447 / 5339 | 139 / 167 | 590 / 590 |
| CPU-only full-scene render, 1456 × 900, warm median | 104.76 ms | 46.05 ms | 28.15 ms |

These are CPU durations of the full `Studio::ui` call; input-to-UI and frame intervals are also recorded. They are not end-to-end display presentation latency or a claimed FPS. Tables recompute conventional medians and nearest-rank p95 from the raw samples. The two runs contribute 166 typing, 180 brush-drag, 80 pan and 120 zoom samples per host; Pen commit has only six samples and Save only two. Individual-run distributions remain available, so pooled results do not hide run-to-run variation. CPU-only attribution uses three warm calls after one cold call at the separately stated render size.

## Offline AI: whole-process seconds / peak process RSS

| Measurement | hpeliteclient | intelpro | m1pro16 |
| --- | ---: | ---: | ---: |
| AI 2×: 320 × 212 → 640 × 424 | 9.55 s / 100.6 MiB | 1.20 s / 107.8 MiB | 1.10 s / 78.0 MiB |
| AI 4×: 320 × 212 → 1280 × 848 | 16.19 s / 101.2 MiB | 1.20 s / 108.9 MiB | 1.10 s / 79.7 MiB |
| AI 2×: 1024 × 678 → 2048 × 1356 | 166.35 s / 126.9 MiB | 15.12 s / 144.6 MiB | 12.12 s / 112.3 MiB |
| Background removal: 320 × 212 | 5.25 s / 316.4 MiB | 0.40 s / 324.0 MiB | 0.30 s / 237.3 MiB |
| Background removal: 1024 × 678 | 32.01 s / 525.7 MiB | 2.70 s / 536.4 MiB | 2.00 s / 444.3 MiB |
| Native background-removal workflow | 24.86 s / 440.0 MiB | 6.51 s / 445.7 MiB | 3.40 s / 337.1 MiB |
| Native upscaling workflow | 153.08 s / 604.1 MiB | 19.92 s / 652.8 MiB | 11.01 s / 544.9 MiB |
| Portable app reopen / capture | 11.72 s / 146.8 MiB | 3.10 s / 147.5 MiB | 1.30 s / 146.9 MiB |
| Background-removal native UI p95 / max | 234.69 / 486.65 ms | 21.20 / 53.98 ms | 4.50 / 42.08 ms |
| Upscaling native UI p95 / max | 257.89 / 956.74 ms | 16.93 / 55.19 ms | 0.75 / 35.89 ms |
| Functional assertions / process exits | 11 / 11 processes passed | 11 / 11 processes passed | 11 / 11 processes passed |
| Authoring undo/redo and exact save/reopen | Pass, both runs | Pass, both runs | Pass, both runs |
| AI cancel, preview/apply, undo/redo, persistence, export | Pass | Pass | Pass |
| Reopened infographic export across architectures | Pixel-identical to the other two hosts | Pixel-identical to the other two hosts | Pixel-identical to the other two hosts |
| Three AI output comparisons, ARM versus x86 | At most 1/255 RGB difference; alpha identical | At most 1/255 RGB difference; alpha identical | At most 1/255 RGB difference; alpha identical |

Each CLI/native AI case was run once per host. Whole-process timing includes startup, loading and encoding (the background-removal CLI writes comparison variants), with 100 ms polling granularity. The two native AI suites include many interactions and idle/wait frames, so their low UI p95 values do not predict dense-document authoring speed. Models are bundled Real-ESRGAN General x4v3 and U²-NetP. Every run used a fresh isolated profile and network-disabled namespace. No optional models, GPU inference, affinity tuning or competing benchmark process on the same host was used.

## Earlier HP results retained separately

| Earlier test conditions | Observed result | Relationship to this comparison |
| --- | --- | --- |
| Larger 1876 × 1006 window / 1456 × 876 canvas, uncapped repeat | Median typing 158 ms, brush 104 ms, pan 113 ms, zoom 105 ms; typing maximum 1.1 s | Historical result; not pooled with the normalized window runs above |
| Earlier AI 2× at 320 / 1024 input width | 19.20 s / 165.42 s | Existing desktop load and test conditions differed; not evidence of a software improvement |
| Earlier background removal at 320 / 1024 input width | 2.83 s / 27.33 s | Historical whole-process measurements |
| Earlier 5× exploratory run | Stopped after 117 s | Not a completed benchmark; not counted as a pass |

## Findings for public hardware guidance

| Finding | Evidence / implication |
| --- | --- |
| Low-power thin client remains a poor fit for this dense workflow | HP tails and AI wall times are materially worse; four CPU cores alone are not an adequate minimum specification |
| Intelpro is a useful 8 GB-class entry-level reference | It completes the full workflow, but roughly 31–48 ms typical authoring updates are not a 60 FPS promise |
| m1pro16 is a useful 16 GB-class recommended reference | Roughly 19–28 ms typical authoring updates and much faster AI; brush/navigation p95 still exceeds a 16.7 ms frame budget |
| The comparison does not isolate RAM as the cause | CPU architectures, power profiles, HP frequency ceiling, memory pressure, driver versions and surrounding desktop workloads differ |
| A dedicated GPU is not necessary for the tested workflows | All hosts use integrated graphics; AI inference uses CPU on all three |
| Renderer optimization remains relevant on every host | The synchronous full-scene compositor has measurable CPU cost, even on M1 Pro |
| Keep website requirements provisional | 8 GB minimum / 16 GB recommended is supported as guidance by these reference machines, not a universal smoothness guarantee; available memory and CPU class matter |

Suggested follow-up: reduce full-scene redraws during typing, brush work and navigation; preserve pixel equivalence, masks/blends and undo semantics; rerun this same fixed-size workload before changing performance claims. If doing a controlled hardware comparison later, hold power policy and background load constant and report it separately from these as-used results.

## Provenance and reproducibility

Application code is `221834bdd8af5aee8554a0e835629cbe2f467558`, documented through `587433037d1df9be5e66b36cc77bc6f44fcd7071`. The only benchmark change is fixing the authoring window size; no production renderer or inference code was changed. All binaries were built locally on m1pro16, with the repository Zig toolchain for x86_64. Matching portable executables/runtimes were staged under `~/.local/state/omadesign/qa/fleet-benchmark-20260927/`; normal installed applications were not replaced by this comparison.

SHA-256 verification confirms identical input images, `.oma`, typing tasks and font bank on all hosts, and identical x86_64 executables/runtime on HP and Intel. Architecture-specific executable/runtime hashes, resource samples and host specs are retained in each `host.json`. Correctness also checks actual canvas updates and exact native save/reopen values. All three exported infographic images were compared as RGBA pixels.

| Evidence | Link |
| --- | --- |
| hpeliteclient: specs, all process resources and raw per-frame timings | [hpeliteclient evidence](hpeliteclient/) · [native package capture](hpeliteclient/package-reopened.png) |
| intelpro: specs, all process resources and raw per-frame timings | [intelpro evidence](intelpro/) · [native package capture](intelpro/package-reopened.png) |
| m1pro16: specs, all process resources and raw per-frame timings | [m1pro16 evidence](m1pro16/) · [native package capture](m1pro16/package-reopened.png) |
| Summary and image equivalence | [summary](summary.json) · [verification](verification.json) |
| Reproducible serial runner and aggregation | [runner](run_fleet.py) · [summary script](summarize.py) · [table generator](make_report.py) |
| Existing HP reports and original artwork/fixture details | [AI report](../hpeliteclient-2026-09-27.md) · [authoring report](../authoring-hpeliteclient-2026-09-27.md) |

Prepare `bin/` with `authoring_qa`, `canvas_profile`, `upscale_qa`, `background_removal_qa` and the matching portable `omadesign`; `lib/` with the architecture-matched ORT 1.28 libraries; and `inputs/` with the original 320/1024 cat fixtures, infographic seed, typing tasks and `.omabrand` fonts. Run `run_fleet.py` from the host systemd user manager in a fresh directory so its existing Wayland session is available. One workload runs at a time per host. Native screenshots contain only the test application. The source photograph attribution remains in the linked original AI QA report.
