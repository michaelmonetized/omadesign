"""Generate the comparison tables from completed measurements, with fixed interpretation."""
import json
import pathlib
import sys

root = pathlib.Path(sys.argv[1])
data = json.loads((root / 'summary.json').read_text())
hosts = ['hpeliteclient', 'intelpro', 'm1pro16']
out = []
add = out.append


def table(rows):
    add('| Measurement | hpeliteclient | intelpro | m1pro16 |')
    add('| --- | ---: | ---: | ---: |')
    for name, values in rows:
        add('| ' + ' | '.join([name, *map(str, values)]) + ' |')
    add('')


def values(f):
    return [f(data[h]) for h in hosts]


def resource(d, name):
    return d['resources'][name]


add('# Fleet performance baseline: ordinary authoring and offline AI')
add('')
add('Measured on 2026-09-27 for PR #147 / issue #145, using the same application code and identical fixtures. All three hosts completed every functional check. The HP still has long interaction stalls and slow AI processing; Intelpro and m1pro16 are substantially faster. This is an as-used fleet comparison, with existing applications and power policies preserved, not a controlled hardware laboratory benchmark.')
add('')
add('## Hardware and conditions')
add('')
table([
    ('Machine', ['HP Elite t655 Thin Client', 'Apple MacBookPro15,4', 'Apple MacBook Pro 16-inch, M1 Pro, 2021']),
    ('CPU', ['AMD Ryzen Embedded R2314', 'Intel Core i5-8257U', 'Apple M1 Pro']),
    ('Physical cores / threads', ['4 / 4', '4 / 8', '10 / 10 (8 performance + 2 efficiency)']),
    ('Architecture', values(lambda d: d['spec']['architecture'])),
    ('RAM visible to Linux', values(lambda d: f"{d['spec']['state']['memory']['MemTotal_KiB']/1048576:.2f} GiB")),
    ('RAM available at suite start', values(lambda d: f"{d['spec']['state']['memory']['MemAvailable_KiB']/1048576:.2f} GiB")),
    ('Swap occupied at suite start', values(lambda d: f"{(d['spec']['state']['memory']['SwapTotal_KiB']-d['spec']['state']['memory']['SwapFree_KiB'])/1048576:.2f} GiB")),
    ('One-minute system load at suite start', values(lambda d: d['spec']['state']['loadavg'].split()[0])),
    ('Graphics / active kernel driver', ['Radeon Vega / amdgpu; 2 GiB reported GPU memory', 'Iris Plus 645 / i915', 'M1 Pro integrated GPU / asahi']),
    ('Mesa / Vulkan driver package', ['26.2.2 / vulkan-radeon', '26.2.2 / vulkan-intel', '26.2.3 / vulkan-asahi']),
    ('Power profile', values(lambda d: d['spec']['power_profile']['output'])),
    ('Configured CPU frequency ceiling', ['2.10 GHz (hardware reports up to 3.50 GHz)', '3.90 GHz', '3.036 GHz performance / 2.064 GHz efficiency']),
    ('Kernel', values(lambda d: d['spec']['kernel'])),
    ('Native authoring window / canvas', ['1440 × 900 / 1020 × 770'] * 3),
    ('Runtime / inference provider', ['ONNX Runtime 1.28 / CPU'] * 3),
    ('Per-test limits / networking', ['No CPU or memory ceiling; network disabled'] * 3),
])
add('RAM above is OS-usable memory, not a DIMM inventory. The HP GPU reports a separate 2 GiB allocation; do not describe the machine as having a verified 6 GB of installed RAM. Available RAM, swap, CPU frequencies, load and pressure were also sampled before and after every process. Occupied swap alone does not establish active paging; process major-fault counts and pressure samples are retained. No host power or desktop configuration was changed.')
add('')
add('## Ordinary authoring: pooled median / p95 UI milliseconds')
add('')
add('Two independent runs per host on the same editable 1200 × 900 infographic: 20 layers, 576 final vector shapes including 79 live text objects, and one brush layer. The illustration/card seed was imported as editable SVG; the measured workload uses actual native input to type five headline objects (83 character events), draw three Pen paths and three Brush strokes, pan, zoom, undo/redo and save. This is not a human design-time measurement or an entirely manual reconstruction.')
add('')
rows = []
for phase, label in [('idle', 'Idle'), ('pen-preview', 'Pen preview'), ('pen-commit', 'Pen commit'), ('typing', 'Typing'), ('brush-drag', 'Brush drag'), ('pan', 'Pan'), ('zoom', 'Zoom'), ('save', 'Save UI update')]:
    rows.append((label, values(lambda d, p=phase: f"{d['authoring'][p]['ui_ms']['p50']:.2f} / {d['authoring'][p]['ui_ms']['p95']:.2f}")))
rows.extend([
    ('Typing medians, runs 1 / 2', values(lambda d: ' / '.join(f"{r['p50_ms']:.2f} ms" for r in d['authoring']['typing']['per_run']))),
    ('Brush medians, runs 1 / 2', values(lambda d: ' / '.join(f"{r['p50_ms']:.2f} ms" for r in d['authoring']['brush-drag']['per_run']))),
    ('Worst typing update', values(lambda d: f"{d['authoring']['typing']['ui_ms']['max']:.2f} ms")),
    ('Typing frame interval median / p95', values(lambda d: f"{d['authoring']['typing']['interval_ms']['p50']:.2f} / {d['authoring']['typing']['interval_ms']['p95']:.2f} ms")),
    ('Brush frame interval median / p95', values(lambda d: f"{d['authoring']['brush-drag']['interval_ms']['p50']:.2f} / {d['authoring']['brush-drag']['interval_ms']['p95']:.2f} ms")),
    ('Authoring whole-process time, runs 1 / 2', values(lambda d: ' / '.join(f"{resource(d, f'authoring-{i}')['wall_seconds']:.2f} s" for i in [1, 2]))),
    ('Authoring peak process RSS, runs 1 / 2', values(lambda d: ' / '.join(f"{resource(d, f'authoring-{i}')['peak_rss_KiB']/1024:.1f} MiB" for i in [1, 2]))),
    ('Authoring major page faults, runs 1 / 2', values(lambda d: ' / '.join(str(resource(d, f'authoring-{i}')['major_page_faults']) for i in [1, 2]))),
    ('CPU-only full-scene render, 1456 × 900, warm median', values(lambda d: f"{d['warm_canvas_median_ms']:.2f} ms")),
])
table(rows)
add('These are CPU durations of the full `Studio::ui` call; input-to-UI and frame intervals are also recorded. They are not end-to-end display presentation latency or a claimed FPS. Tables recompute conventional medians and nearest-rank p95 from the raw samples. The two runs contribute 166 typing, 180 brush-drag, 80 pan and 120 zoom samples per host; Pen commit has only six samples and Save only two. Individual-run distributions remain available, so pooled results do not hide run-to-run variation. CPU-only attribution uses three warm calls after one cold call at the separately stated render size.')
add('')
add('## Offline AI: whole-process seconds / peak process RSS')
add('')
rows = []
for name, label in [('upscale-320-2x', 'AI 2×: 320 × 212 → 640 × 424'), ('upscale-320-4x', 'AI 4×: 320 × 212 → 1280 × 848'), ('upscale-1024-2x', 'AI 2×: 1024 × 678 → 2048 × 1356'), ('background-320', 'Background removal: 320 × 212'), ('background-1024', 'Background removal: 1024 × 678'), ('background_removal_qa-native', 'Native background-removal workflow'), ('upscale_qa-native', 'Native upscaling workflow'), ('package-reopen', 'Portable app reopen / capture')]:
    rows.append((label, values(lambda d, n=name: f"{resource(d,n)['wall_seconds']:.2f} s / {resource(d,n)['peak_rss_KiB']/1024:.1f} MiB")))
rows.extend([
    ('Background-removal native UI p95 / max', values(lambda d: f"{d['native_background']['ui_frame_p95_ms']:.2f} / {d['native_background']['ui_frame_max_ms']:.2f} ms")),
    ('Upscaling native UI p95 / max', values(lambda d: f"{d['native_upscale']['ui_frame_p95_ms']:.2f} / {d['native_upscale']['ui_frame_max_ms']:.2f} ms")),
    ('Functional assertions / process exits', ['11 / 11 processes passed'] * 3),
    ('Authoring undo/redo and exact save/reopen', ['Pass, both runs'] * 3),
    ('AI cancel, preview/apply, undo/redo, persistence, export', ['Pass'] * 3),
    ('Reopened infographic export across architectures', ['Pixel-identical to the other two hosts'] * 3),
    ('Three AI output comparisons, ARM versus x86', ['At most 1/255 RGB difference; alpha identical'] * 3),
])
table(rows)
add('Each CLI/native AI case was run once per host. Whole-process timing includes startup, loading and encoding (the background-removal CLI writes comparison variants), with 100 ms polling granularity. The two native AI suites include many interactions and idle/wait frames, so their low UI p95 values do not predict dense-document authoring speed. Models are bundled Real-ESRGAN General x4v3 and U²-NetP. Every run used a fresh isolated profile and network-disabled namespace. No optional models, GPU inference, affinity tuning or competing benchmark process on the same host was used.')
add('')
add('## Earlier HP results retained separately')
add('')
add('| Earlier test conditions | Observed result | Relationship to this comparison |')
add('| --- | --- | --- |')
add('| Larger 1876 × 1006 window / 1456 × 876 canvas, uncapped repeat | Median typing 158 ms, brush 104 ms, pan 113 ms, zoom 105 ms; typing maximum 1.1 s | Historical result; not pooled with the normalized window runs above |')
add('| Earlier AI 2× at 320 / 1024 input width | 19.20 s / 165.42 s | Existing desktop load and test conditions differed; not evidence of a software improvement |')
add('| Earlier background removal at 320 / 1024 input width | 2.83 s / 27.33 s | Historical whole-process measurements |')
add('| Earlier 5× exploratory run | Stopped after 117 s | Not a completed benchmark; not counted as a pass |')
add('')
add('## Findings for public hardware guidance')
add('')
add('| Finding | Evidence / implication |')
add('| --- | --- |')
add('| Low-power thin client remains a poor fit for this dense workflow | HP tails and AI wall times are materially worse; four CPU cores alone are not an adequate minimum specification |')
add('| Intelpro is a useful 8 GB-class entry-level reference | It completes the full workflow, but roughly 31–48 ms typical authoring updates are not a 60 FPS promise |')
add('| m1pro16 is a useful 16 GB-class recommended reference | Roughly 19–28 ms typical authoring updates and much faster AI; brush/navigation p95 still exceeds a 16.7 ms frame budget |')
add('| The comparison does not isolate RAM as the cause | CPU architectures, power profiles, HP frequency ceiling, memory pressure, driver versions and surrounding desktop workloads differ |')
add('| A dedicated GPU is not necessary for the tested workflows | All hosts use integrated graphics; AI inference uses CPU on all three |')
add('| Renderer optimization remains relevant on every host | The synchronous full-scene compositor has measurable CPU cost, even on M1 Pro |')
add('| Keep website requirements provisional | 8 GB minimum / 16 GB recommended is supported as guidance by these reference machines, not a universal smoothness guarantee; available memory and CPU class matter |')
add('')
add('Suggested follow-up: reduce full-scene redraws during typing, brush work and navigation; preserve pixel equivalence, masks/blends and undo semantics; rerun this same fixed-size workload before changing performance claims. If doing a controlled hardware comparison later, hold power policy and background load constant and report it separately from these as-used results.')
add('')
add('## Provenance and reproducibility')
add('')
add('Application code is `221834bdd8af5aee8554a0e835629cbe2f467558`, documented through `587433037d1df9be5e66b36cc77bc6f44fcd7071`. The only benchmark change is fixing the authoring window size; no production renderer or inference code was changed. All binaries were built locally on m1pro16, with the repository Zig toolchain for x86_64. Matching portable executables/runtimes were staged under `~/.local/state/omadesign/qa/fleet-benchmark-20260927/`; normal installed applications were not replaced by this comparison.')
add('')
add('SHA-256 verification confirms identical input images, `.oma`, typing tasks and font bank on all hosts, and identical x86_64 executables/runtime on HP and Intel. Architecture-specific executable/runtime hashes, resource samples and host specs are retained in each `host.json`. Correctness also checks actual canvas updates and exact native save/reopen values. All three exported infographic images were compared as RGBA pixels.')
add('')
add('| Evidence | Link |')
add('| --- | --- |')
for host in hosts:
    add(f'| {host}: specs, all process resources and raw per-frame timings | [{host} evidence]({host}/) · [native package capture]({host}/package-reopened.png) |')
add('| Summary and image equivalence | [summary](summary.json) · [verification](verification.json) |')
add('| Reproducible serial runner and aggregation | [runner](run_fleet.py) · [summary script](summarize.py) · [table generator](make_report.py) |')
add('| Existing HP reports and original artwork/fixture details | [AI report](../hpeliteclient-2026-09-27.md) · [authoring report](../authoring-hpeliteclient-2026-09-27.md) |')
add('')
add('Prepare `bin/` with `authoring_qa`, `canvas_profile`, `upscale_qa`, `background_removal_qa` and the matching portable `omadesign`; `lib/` with the architecture-matched ORT 1.28 libraries; and `inputs/` with the original 320/1024 cat fixtures, infographic seed, typing tasks and `.omabrand` fonts. Run `run_fleet.py` from the host systemd user manager in a fresh directory so its existing Wayland session is available. One workload runs at a time per host. Native screenshots contain only the test application. The source photograph attribution remains in the linked original AI QA report.')
(root / 'README.md').write_text('\n'.join(out) + '\n')
print(root / 'README.md')
