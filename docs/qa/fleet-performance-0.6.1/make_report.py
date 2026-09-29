#!/usr/bin/env python3
"""Generate complete tables from a completed summarize_paired.py result.

Does not execute measurements, fetch data, invent missing results, discard
regressions, or turn native callback/CPU measurements into presented FPS.
"""
import argparse
import hashlib
import json
from pathlib import Path

HOSTS = ('hpeliteclient', 'intelpro', 'm1pro16')
PHASES = ('idle', 'pen-preview', 'pen-commit', 'typing', 'brush-drag', 'pan', 'zoom', 'save')
AI_CASES = ('upscale-320-2x', 'upscale-320-4x', 'upscale-1024-2x', 'background-320',
            'background-1024', 'background_removal_qa-native', 'upscale_qa-native')
LABELS = {'idle': 'Idle', 'pen-preview': 'Pen preview', 'pen-commit': 'Pen commit',
          'typing': 'Typing', 'brush-drag': 'Brush drag', 'pan': 'Pan', 'zoom': 'Zoom', 'save': 'Save UI update',
          'upscale-320-2x': 'AI 2×: 320×212 → 640×424',
          'upscale-320-4x': 'AI 4×: 320×212 → 1280×848',
          'upscale-1024-2x': 'AI 2×: 1024×678 → 2048×1356',
          'background-320': 'Background removal 320×212',
          'background-1024': 'Background removal 1024×678',
          'background_removal_qa-native': 'Native background-removal workflow',
          'upscale_qa-native': 'Native upscaling workflow', 'package-reopen': 'Portable app reopen/capture'}


def require(value, message):
    if not value:
        raise ValueError(message)


def native_upscale_regression(summary):
    value = summary['hosts']['hpeliteclient']['processes']['upscale_qa-native']['wall_seconds']
    old, new = value['baseline']['pooled']['median'], value['candidate']['pooled']['median']
    if new <= old:
        return None
    raw = {side: '/'.join(f'{run[0]:.3f}' for run in value[side]['raw_runs'])
           for side in ('baseline', 'candidate')}
    native = summary['hosts']['hpeliteclient']['native_ai']
    p95 = {side: '/'.join(f'{run["ui_frame_p95_ms"]:.2f}' for run in native[side]['upscale_qa-native'])
           for side in ('baseline', 'candidate')}
    frames = {side: '/'.join(str(run['frames']) for run in native[side]['upscale_qa-native'])
              for side in ('baseline', 'candidate')}
    return (f'HP native upscaling regressed: baseline runs {raw["baseline"]} s, candidate {raw["candidate"]} s; '
            f'the whole-workflow median rose from {old:.3f} to {new:.3f} s ({(new/old-1)*100:.1f}% longer; '
            f'{old/new:.2f}× old/new). This observed workflow regression is retained without assigning a cause. '
            f'The same runs reported lower aggregate UI p95 ({p95["baseline"]} → {p95["candidate"]} ms), '
            f'but sampled more UI calls ({frames["baseline"]} → {frames["candidate"]}); these differing '
            'distributions do not establish an isolated interaction-speed ratio. Core AI inference source is unchanged. '
            'The recorded inference phase includes preview/apply readiness, rather than an isolated model kernel. '
            'Resource observations do not establish whether the longer workflow reflects external load or self-contention.')


def other_material_regressions(summary):
    pen = summary['hosts']['hpeliteclient']['authoring']['pen-commit']['ui_ms']
    old, new = pen['baseline']['pooled'], pen['candidate']['pooled']
    intel = summary['hosts']['intelpro']['processes']['upscale-1024-2x']['wall_seconds']
    a, b = intel['baseline']['pooled']['median'], intel['candidate']['pooled']['median']
    reopen = []
    for host in HOSTS:
        value = summary['hosts'][host]['processes']['package-reopen']['wall_seconds']
        reopen.append(f'{host} {value["baseline"]["pooled"]["median"]:.3f} → '
                      f'{value["candidate"]["pooled"]["median"]:.3f} s')
    return (f'Other observed increases include HP pen commit: median {old["median"]:.3f} → {new["median"]:.3f} ms '
            f'and p95 {old["p95"]:.3f} → {new["p95"]:.3f} ms, from only {old["n"]} calls per version; '
            f'Intel 1024-pixel 2× CLI upscaling: {a:.3f} → {b:.3f} s median ({(b/a-1)*100:.1f}% longer, '
            'two observations per version). Portable reopen/capture has one observation per version: '
            + '; '.join(reopen) + '. These sparse observations are retained without assigning a cause; '
            'the full tables include smaller increases and individual runs.')


def render_claims(summary):
    ratios = [summary['hosts'][host]['authoring']['brush-drag']['ui_ms']['speedup_old_over_new'] for host in HOSTS]
    median = [row['median'] for row in ratios]
    p95 = [row['p95'] for row in ratios]
    lines = ['# Release claims: measured brushing improvement', '',
             f'Brush-drag UI processing improved by **{min(median):.2f}–{max(median):.2f}× at the median** and **{min(p95):.2f}–{max(p95):.2f}× at p95** across these three tested machines, versus the retained issue #158 baseline.', '',
             '| Host | Median ms, baseline → candidate | Median old/new | P95 ms, baseline → candidate | P95 old/new |',
             '| --- | ---: | ---: | ---: | ---: |']
    for host in HOSTS:
        value = summary['hosts'][host]['authoring']['brush-drag']['ui_ms']
        old, new = value['baseline']['pooled'], value['candidate']['pooled']
        lines.append(f'| {host} | {old["median"]:.3f} → {new["median"]:.3f} | {value["speedup_old_over_new"]["median"]:.2f}× | {old["p95"]:.3f} → {new["p95"]:.3f} | {value["speedup_old_over_new"]["p95"]:.2f}× |')
    others = [summary['hosts'][host]['authoring'][phase]['ui_ms']['speedup_old_over_new']['median']
              for host in HOSTS for phase in ('typing', 'pan', 'zoom')]
    lines += ['', f'Typing, pan and zoom median ratios ranged {min(others):.3f}–{max(others):.3f}× old/new. This is a brushing gain, not a claim that all authoring is 3× faster.', '']
    for host in HOSTS:
        for phase in ('brush-drag', 'typing', 'pan', 'zoom'):
            value = summary['hosts'][host]['authoring'][phase]['ui_ms']
            increases = [(key, value['delta_new_minus_old'][key]) for key in ('median', 'p95', 'max')
                         if value['delta_new_minus_old'][key] > 0]
            if increases:
                lines.append(f'- {host}, {LABELS[phase]}: ' + ', '.join(f'{key} increased {delta:.3f} ms' for key, delta in increases) + '.')
    if native_upscale_regression(summary):
        lines += ['', native_upscale_regression(summary)]
    lines += ['', other_material_regressions(summary)]
    lines += ['', f'Candidate production source: `{summary["candidate_source"]}`. Baseline: `{summary["baseline_source"]}`. The baseline is **not the shipped v0.6.0 tag**, so these results do not support “0.6.1 is 3× faster than 0.6.0.” The tested candidate still reports package 0.6.0 pending release. Both sides use the original fleet release profile with LTO disabled; these [benchmark builds](build-validation/README.md) are not the final packaged 0.6.1 release binaries.', '',
              'Two authoring runs per version/host; ratios use measured `Studio::ui` CPU wall duration, not displayed FPS or physical input latency. The outer window is unchanged; the final recorded candidate canvas area is 0.389% smaller, with no timing normalization or equal-zoom claim. Other applications, thermals and pressure remained as used. Core AI inference code is unchanged; no AI-algorithm speedup is claimed.', '',
              'See [complete measurements and variation](README.md), [raw samples](summary.json) and [output acceptance](OUTPUT-ACCEPTANCE.md). Authoring output has nine explicitly documented changed pixels; do not claim pixel identity.', '']
    return '\n'.join(lines)


def render_acceptance(summary):
    quality = summary['correctness']
    before = [row for row in quality['comparisons'] if row['relation'] == 'before_after']
    authors = [row['images']['authoring'] for row in before]
    authored = [row['documents']['authoring']['current_schema_defaults_proof'] for row in before]
    require(len(authors) == 6 and all(row['same_dimensions'] and row['baseline_size'] == [1200, 900]
            and row['different_pixels'] == 9 and row['max_channel_delta'] == [15, 15, 15, 0] for row in authors),
            'authoring pixel differences differ from the explicitly reviewed nine-pixel result')
    require(all(row['common_authored_state_equal_after_current_defaults_and_id_remap'] is True
            and row['exact_default_fields_added_to_reference_only'] ==
                {'text_wrap_above_only': 1, 'fill_opacity': 596, 'blend_interior': 596} for row in authored),
            'authoring data differs beyond reviewed defaults and fresh IDs')
    same_host_ai = [value for row in before for name, value in row['images'].items() if name != 'authoring']
    repeat = [value for row in quality['comparisons'] if row['relation'] == 'repeatability' for value in row['images'].values()]
    cross = [row for row in quality['comparisons'] if row['relation'] == 'cross_host']
    require(all(value['equal_rgba'] for value in same_host_ai + repeat)
            and all(row['images']['authoring']['equal_rgba'] for row in cross),
            'image output changed beyond the reviewed authoring/cross-architecture categories')
    cross_ai = [value for row in cross for name, value in row['images'].items() if name != 'authoring']
    cross_max = max(max(value['max_channel_delta']) for value in cross_ai)
    common_docs = [value for row in before for value in row['documents'].values()]
    require(all(value['equal_bytes'] or value.get('id_equivalence', {}).get('equal_after_consistent_id_remap')
            or value.get('current_schema_defaults_proof', {}).get(
                'common_authored_state_equal_after_current_defaults_and_id_remap') for value in common_docs),
            'same-host authored document differs beyond current defaults and proven IDs')
    remaining_docs = [(row['relation'], name) for row in quality['comparisons']
                      for name, value in row['documents'].items()
                      if not (value['equal_bytes'] or value.get('id_equivalence', {}).get('equal_after_consistent_id_remap')
                          or value.get('current_schema_defaults_proof', {}).get(
                              'common_authored_state_equal_after_current_defaults_and_id_remap'))]
    require(all(relation == 'cross_host' and name in ('background-native', 'upscale-native')
                for relation, name in remaining_docs), 'unreviewed document difference category')
    return '\n'.join(['# Output acceptance', '',
        f'All {len(common_docs)} same-host before/after saved-document pairs preserve their common authored state under the current schema. Each authoring document adds exactly 596 `fill_opacity:1.0` fields, 596 `blend_interior:false` fields and one `text_wrap_above_only:false` field. The additive proof verifies those declarations against the pinned candidate source, initializes only absent reference-side defaults, and then checks a global fresh-ID bijection and every reference. Seed IDs, other values/types, array order and opaque image bytes remain protected; no unknown field or candidate value is removed.', '',
        '**The authoring images are not pixel-identical.** All six host/repetition comparisons change the same nine isolated pixels of 1200×900 (0.000833%), with maximum RGB delta 15/255 and unchanged alpha. Each version repeats exactly, and its authoring image is identical across hosts. This is a quantified small render difference with preserved authored data, not a claim of bit-identical rendering or universal visual equivalence. Its cause has not been established.', '',
        f'All {len(repeat)} within-version image repeatability pairs are exact. All {len(same_host_ai)} same-host AI/native export comparisons are exact before/after. Cross-architecture AI images have maximum channel delta {cross_max}/255; those differences occur in both versions, while each host’s before/after outputs stay exact. They are pre-existing architecture differences in this comparison, not new candidate-only changes.', '',
        f'The full audit covers {quality["image_pairs"]} image pairs and {quality["document_pairs"]} document pairs. Raw document byte/ID-only failures and every pixel difference remain visible in [summary.json](summary.json); successful current-default proofs do not overwrite those findings. [The additive proof](schema_defaults_equivalence.py) is separate from [strict fresh-ID equivalence](document_id_equivalence.py).', '',
        f'Of the {quality["document_pairs"]} saved-document pairs, {quality["equal_document_byte_pairs"]} match bytes, '
        f'{quality["document_id_only_pairs"]} more pass the strict ID-only proof, and '
        f'{quality["document_default_and_id_pairs"]} more pass the additive-default/ID proof. '
        f'The remaining {len(remaining_docs)} pairs are native AI documents compared across architectures; they are not declared equivalent. '
        'Each host’s baseline/candidate native documents passed the separate common-authored-state proof above.', '',
        'The selected 108 processes passed their checks. Failed/superseded native attempts remain preserved in continuation lineage. The recovered native protocol tests Escape cancellation and does not cover the Cancel button. [Complete measurements](README.md) preserve timing regressions and cohort differences.', ''])


def render(summary, summary_name, summary_sha):
    require(summary['schema_version'] == 1 and summary['completed_processes'] == 108
            and summary['functional_validation_passed'] is True, 'full paired matrix is not complete')
    require(set(summary['hosts']) == set(HOSTS), 'missing fleet host')
    output = []
    add = output.append
    def table(headers, rows):
        add('| ' + ' | '.join(headers) + ' |')
        add('| ' + ' | '.join('---' if i == 0 else '---:' for i in range(len(headers))) + ' |')
        for row in rows:
            add('| ' + ' | '.join(str(v).replace('|', '\\|') for v in row) + ' |')
        add('')
    def f(value):
        return f'{value:.3f}'
    def speed(value):
        return 'undefined (zero duration)' if value is None else f'{value:.2f}×'
    def p50p95(value):
        return f'{f(value["median"])} / {f(value["p95"])}'
    def samples(distribution, divisor=1):
        return ' / '.join(f'{run[0] / divisor:.3f}' for run in distribution['raw_runs'])
    def ranged(distribution):
        return ' / '.join(f(r['median']) for r in distribution['per_run'])

    add('# Paired fleet performance: issue #158 baseline and 0.6.1 candidate')
    add('')
    add(f'The selected 108 serial workload processes passed their recorded functional checks: 18 cases/repetitions per version on each of three hosts. Earlier failed and superseded attempts remain separately preserved; this is not an all-attempts-pass claim. The candidate source is `{summary["candidate_source"]}`; the retained issue #158 production baseline is `{summary["baseline_source"]}`. The baseline is **not the shipped v0.6.0 tag**. These tables do not support a blanket “0.6.1 is N× faster than 0.6.0” claim.')
    add('This is the intended 0.6.1 candidate; its package version still reports 0.6.0 pending release. Source and executable hashes identify the tested code, not that unchanged package label. Both sides use the original fleet release profile with LTO disabled; these [benchmark builds](build-validation/README.md) are not the final packaged 0.6.1 release binaries.')
    add('')
    brush_ratios = [summary['hosts'][host]['authoring']['brush-drag']['ui_ms']['speedup_old_over_new']
                    for host in HOSTS]
    if all(row[key] is not None for row in brush_ratios for key in ('median', 'p95')):
        medians, tails = ([row[key] for row in brush_ratios] for key in ('median', 'p95'))
        add(f'For brush dragging on these three tested hosts, median UI duration had baseline/candidate ratios of {min(medians):.2f}–{max(medians):.2f}×; p95 ratios were {min(tails):.2f}–{max(tails):.2f}×. This task-specific comparison is against the retained issue #158 baseline. The other authoring phases are reported separately, including unchanged or slower measurements; it is not an across-the-board authoring speedup.')
        add('')
    if native_upscale_regression(summary):
        add(native_upscale_regression(summary))
        add('')
    add(other_material_regressions(summary))
    add('')
    add('Fresh same-host pairs alternate version order for successive case/repetition pairs on private GPU-backed Sway displays. Both use exact 1440×900 outer windows; the final recorded canvas is baseline 1020 × 770 and candidate 1014.71875 × 771 on all three hosts. The candidate inspector’s minimum width produces a 0.389% smaller final canvas area. Geometry is recorded only at the end; the inspector can expand during text actions after initial fit, so per-phase geometry and equal zoom are not asserted. Timings are not normalized by area. All authoring exports remain 1200×900, and common measured action counts match; the candidate adds hover/setup coordination calls. Artwork equality is checked from actual outputs. Inputs, project fonts, models and architecture-matched ORT 1.28 CPU libraries are hash-verified. The two x86 hosts use identical executable/runtime bytes. Hardware power policies and other applications remain as used; clocks, pressure and thermals are not controlled.')
    add('')
    add('UI timings measure `Studio::ui` CPU wall duration, including descheduling. UI-completion intervals and input-to-UI measurements are retained; they are not physical input latency, GPU presentation time or displayed FPS. Recomputed medians use the conventional middle value and recomputed p95 uses nearest rank; native-AI p95 is separately labeled with its helper’s retained indexing rule. Every old/new ratio below divides the baseline duration by the candidate duration: above 1 means less candidate time, below 1 means more candidate time. Raw samples, maxima and individual repetitions remain available; no outliers are removed.')
    add('')
    add('The original [HP/Intel runner](runner-versions/run_paired-fdinfo.py) and original [M1 runner](run_paired.py) differ only in GPU evidence collection. Recovered cases on all hosts use the latter frozen source; retained receipts preserve their original collector identities. The audit pins both source hashes and verifies identical parsed code outside `gpu_handles`. M1’s Asahi kernel lacks DRM fdinfo, so its collector additionally resolves sysfs driver identity from an actual open render-node character descriptor. Native GPU evidence remains required; workload commands, timing, scheduling and input actions are unchanged.')
    add('')
    add('[Environment provenance and excluded preflight attempts](environment/README.md) retain the display configuration, GPU evidence, collector variants and calibration failures separately from the completed timing matrix.')
    add('')
    add('Completed authoring and CLI measurements are retained byte-for-byte. All native AI cases were rerun on both versions and all hosts with identical QA helper source under `hover-atomic-buttons-held-sliders-escape-cancel-v2`: semantic buttons use hover plus atomic clicks, sliders retain separate held press/release frames, and cancellation uses the real Escape key. The Cancel button is not covered by this protocol. Background-removal runs assert initial Radius 12 and report a changed final value above 12. Native executable/build/source overrides and every retained, replaced or failed attempt are pinned in the continuation lineage; these native timings are a separate recovered QA cohort.')
    add('')
    add('The native baseline rebuild uses checkout `587433037d1df9be5e66b36cc77bc6f44fcd7071`. Its 825 tracked production inputs match the retained `221834b` baseline exactly after excluding only documentation and the two standalone authoring/profiler files; the audit recomputes that source proof. Rebuilt native executable bytes are separately identified, rather than described as the original retained binaries. Authoring, CPU-render, CLI and portable-app measurements retain the original executable identities.')
    add('')
    add('## Fresh paired authoring: main-phase medians')
    add('')
    rows = []
    for phase in ('brush-drag', 'typing', 'pan', 'zoom'):
        row = [LABELS[phase]]
        for host in HOSTS:
            metric = summary['hosts'][host]['authoring'][phase]['ui_ms']
            row.append(f'{f(metric["baseline"]["pooled"]["median"])} → {f(metric["candidate"]["pooled"]["median"])} ms ({speed(metric["speedup_old_over_new"]["median"])})')
        rows.append(row)
    table(['Task: baseline → candidate (old/new)', *HOSTS], rows)
    add('The ratios describe these tasks and hosts. Use their p95 and run variation below before drawing a public claim; a pooled median alone does not establish universal smoothness or statistical significance.')
    add('')

    for host in HOSTS:
        data = summary['hosts'][host]
        add(f'## Fresh paired measurements: {host}')
        add('')
        memory = data['initial_state']['memory']
        profile = data['power_profile']
        add(f'Architecture `{data["architecture"]}`, kernel `{data["kernel"]}`, DRM driver `{data["expected_drm_driver"]}`. Selected measurements span `{data["started_utc"]}` to `{data["finished_utc"]}`. At the original suite start: Linux-visible RAM {memory["MemTotal_KiB"]/1048576:.2f} GiB; available RAM {memory["MemAvailable_KiB"]/1048576:.2f} GiB; occupied swap {(memory["SwapTotal_KiB"]-memory["SwapFree_KiB"])/1048576:.2f} GiB. Power-profile query: `{profile.get("stdout", "unavailable")}`. Occupied swap does not prove active paging; per-process faults, CPU policies and pressure are retained.')
        add('')
        table(['Final recorded authoring canvas', 'Baseline', 'Candidate'],
              [[f'Run {rep}', *[' × '.join(str(v) for v in next(row['size'] for row in
                  data['canvas_geometry_per_run'][side] if row['repetition'] == rep))
                  for side in ('baseline', 'candidate')]] for rep in (1, 2)])
        rows = []
        for phase in PHASES:
            value = data['authoring'][phase]['ui_ms']
            rows.append([LABELS[phase], p50p95(value['baseline']['pooled']), p50p95(value['candidate']['pooled']),
                         speed(value['speedup_old_over_new']['median']), speed(value['speedup_old_over_new']['p95']),
                         f'{value["baseline"]["pooled"]["n"]} / {value["candidate"]["pooled"]["n"]}'])
        table(['UI CPU ms', 'Baseline median/p95', 'Candidate median/p95', 'Median old/new', 'P95 old/new', 'Samples old/new'], rows)
        rows = []
        for phase in PHASES:
            value = data['authoring'][phase]['ui_ms']
            rows.append([LABELS[phase], ranged(value['baseline']), ranged(value['candidate']),
                         f(value['baseline']['pooled']['max']), f(value['candidate']['pooled']['max'])])
        table(['UI variation (ms)', 'Baseline run 1 / run 2 median', 'Candidate run 1 / run 2 median', 'Baseline max', 'Candidate max'], rows)
        rows = []
        for phase in ('typing', 'brush-drag'):
            for metric, label in [('frame_interval_ms', 'UI-completion interval'), ('input_to_ui_ms', 'Input-to-UI')]:
                value = data['authoring'][phase][metric]
                rows.append([LABELS[phase] + ': ' + label, p50p95(value['baseline']['pooled']),
                             p50p95(value['candidate']['pooled']), speed(value['speedup_old_over_new']['median'])])
        table(['Native interval ms (not presented FPS)', 'Baseline median/p95', 'Candidate median/p95', 'Median old/new'], rows)
        rows = []
        for metric, label, divisor in [('wall_seconds', 'Authoring whole-process seconds', 1),
                                       ('peak_rss_KiB', 'Authoring peak RSS MiB', 1024),
                                       ('major_page_faults', 'Authoring major page faults', 1)]:
            value = data['processes']['authoring'][metric]
            rows.append([label, samples(value['baseline'], divisor), samples(value['candidate'], divisor)])
        table(['Whole process: runs 1 / 2', 'Baseline', 'Candidate'], rows)
        value = data['canvas_full_scene_render_ms']['warm']
        first = data['canvas_full_scene_render_ms']['first_full_scene']
        table(['CPU-only 1456 × 900 render', 'Baseline ms', 'Candidate ms', 'Old/new'], [
            ['Warm median (three calls, one process)', f(value['baseline']['pooled']['median']),
             f(value['candidate']['pooled']['median']), speed(value['speedup_old_over_new']['median'])],
            ['First full-scene call (after layer attribution)', f(first['baseline']['pooled']['median']),
             f(first['candidate']['pooled']['median']), speed(first['speedup_old_over_new']['median'])]])
        add('The CPU profiler renders every individual layer four times before full-scene rounds 0–3. Its first full-scene call is not a cold-process/cache measurement; the warm median uses full-scene rounds 1–3.')
        add('')
        rows = []
        for case in (*AI_CASES, 'package-reopen'):
            value = data['processes'][case]['wall_seconds']
            rows.append([LABELS[case], samples(value['baseline']), samples(value['candidate']),
                         f'{f(value["baseline"]["pooled"]["median"])} → {f(value["candidate"]["pooled"]["median"])}',
                         speed(value['speedup_old_over_new']['median']),
                         f'{f(value["baseline"]["pooled"]["p95"])} → {f(value["candidate"]["pooled"]["p95"])}'])
        table(['Whole-process wall seconds', 'Baseline observations', 'Candidate observations', 'Median old→new', 'Median old/new', 'Mechanical p95 old→new'], rows)
        rows = []
        for case in (*AI_CASES, 'package-reopen'):
            value = data['processes'][case]['peak_rss_KiB']
            rows.append([LABELS[case], samples(value['baseline'], 1024), samples(value['candidate'], 1024)])
        table(['Peak whole-process RSS MiB', 'Baseline observations', 'Candidate observations'], rows)
        rows = []
        for case in ('background_removal_qa-native', 'upscale_qa-native'):
            for repetition in (1, 2):
                sides = {side: next(r for r in data['native_ai'][side][case] if r['repetition'] == repetition)
                         for side in ('baseline', 'candidate')}
                rows.append([f'{LABELS[case]} run {repetition}',
                             f'{f(sides["baseline"]["ui_frame_p95_ms"])} / {f(sides["baseline"]["ui_frame_max_ms"])}',
                             f'{f(sides["candidate"]["ui_frame_p95_ms"])} / {f(sides["candidate"]["ui_frame_max_ms"])}',
                             f'{sides["baseline"]["frames"]} / {sides["candidate"]["frames"]}'])
        table(['Native AI UI, harness-reported ms', 'Baseline p95/max', 'Candidate p95/max', 'Calls old/new'], rows)
        add('CLI/native AI whole-process timing includes startup, model loading, workflow waits and encoding; background removal also writes comparison variants. Process polling granularity is 100 ms. AI cases have two observations, so process p95 equals the larger observation; it is not a stable tail estimate. Portable reopen has only one observation per side, making its median/p95/max the same observation. Native-AI UI raw frame samples are unavailable. Its helper selects zero-based sorted index floor(0.95×n); the per-run p95/max above are neither pooled nor presented as recomputed nearest-rank distributions. Models remain Real-ESRGAN General x4v3 and U²-NetP on ORT 1.28 CPU. Timing differences are as-used pipeline measurements, not proof of an inference-algorithm speedup.')
        if summary['core_ai_source_identity']['identical']:
            add('The pinned core background-removal, upscaling and ML source files are identical between these revisions. These AI timings validate the unchanged pipeline and UI workflow; no AI inference-algorithm optimization is claimed.')
        add('')

    add('## Correctness and observed output differences')
    add('')
    quality = summary['correctness']
    add(f'All 108 selected process checks passed, including authoring/history/save-reopen and the recovered native AI Escape-cancel/preview/apply/history/persistence/export checks. This excludes rather than erases failed/superseded attempts, and does not cover the native Cancel button. Separately, {quality["image_pairs"]} decoded-RGBA image pairs and {quality["document_pairs"]} saved-document pairs were examined for repeatability, same-host before/after and same-version cross-host equivalence. {quality["equal_image_pairs"]} image pairs were exact; {quality["equal_document_byte_pairs"]} document pairs matched bytes; {quality["document_id_only_pairs"]} additional document pairs passed the strict fresh-ID-only proof. Functional checks do not make nonidentical outputs pixel-identical.')
    add('')
    rows = []
    for relation in ('repeatability', 'before_after', 'cross_host'):
        comparisons = [c for c in quality['comparisons'] if c['relation'] == relation]
        images = [value for c in comparisons for value in c['images'].values()]
        docs = [value for c in comparisons for value in c['documents'].values()]
        comparable = [image for image in images if image['same_dimensions']]
        max_delta = max((max(image['max_channel_delta']) for image in comparable), default=0)
        rows.append([relation, f'{sum(i["equal_rgba"] for i in images)}/{len(images)}',
                     sum(not i['same_dimensions'] for i in images), max_delta,
                     f'{sum(d["equal_bytes"] for d in docs)}/{len(docs)}'])
    table(['Comparison', 'Exact image pairs', 'Dimension differences', 'Largest channel delta/255', 'Exact document bytes'], rows)
    add(f'The JSON preserves every image difference and raw document byte/ID-only result. A separate additive current-schema proof resolves {quality["document_default_and_id_pairs"]} further document pairs by initializing only verified missing defaults before checking IDs and all remaining data. The [output acceptance note](OUTPUT-ACCEPTANCE.md) records the preserved authored state, nine changed authoring pixels and pre-existing cross-architecture AI deltas. No unknown field is discarded and no output is relabeled pixel-identical. Native editor screenshots are dimension-checked, not compared as artwork.')
    add('')
    add('## Historical original table, retained separately')
    add('')
    add('These are the original 2026-09-27 user-desktop measurements from the published issue #158 evidence. They are not fresh pairs, use the earlier surrounding desktop conditions, and are never pooled with the new private-Sway measurements or used to calculate the speedups above.')
    add('')
    old = summary['historical_separate']['hosts']
    table(['Historical UI median/p95 ms', *HOSTS], [[LABELS[phase]] +
          [p50p95(old[host]['authoring'][phase]['ui_ms']['pooled']) for host in HOSTS] for phase in PHASES])
    table(['Historical single process wall seconds', *HOSTS], [[LABELS[case]] +
          [f(old[host]['whole_process_resources'][case]['wall_seconds']) for host in HOSTS]
          for case in (*AI_CASES, 'package-reopen')])
    table(['Historical warm CPU render median ms', *HOSTS], [['1456×900 full scene'] +
          [f(old[host]['warm_canvas_render_ms']['median']) for host in HOSTS]])
    add('[Original complete report and hardware conditions](../fleet-benchmark-2026-09-27/README.md) remain authoritative for that earlier table.')
    add('')
    add('## Reproducibility and claim boundaries')
    add('')
    add(f'[Full raw summary and comparisons]({summary_name}) SHA-256: `{summary_sha}`. [Runner](run_paired.py), [continuation tool](continue_paired.py), [lineage audit](audit_continuation.py), [aggregator](summarize_paired.py), [report generator](make_report.py), and [fresh-ID proof](document_id_equivalence.py) are retained. The JSON pins the completed candidate build receipt and each host’s immutable build snapshot, original input/font/model/runtime identities, source result hashes, native recovery overrides, exact binary identities and all selected per-process resource samples. Architecture readiness snapshots retain their own identity; every captured successful step and artifact must match the completed global build exactly. Continuation lineage preserves prior successful, superseded and failed attempts separately.')
    add('')
    add('The `claim_candidates` array lists each original-table authoring task on each host, including unchanged/slower measurements, run-median ranges, p95 support, sample counts and exact final canvas dimensions. Its wording explicitly names the issue #158 baseline and metric. These are review candidates, not a blanket release claim. Two runs, differing canvas/view geometry and uncontrolled system pressure limit inference. The 0.389% smaller candidate canvas describes only its final recorded area; no analytical timing adjustment is applied. Performance ratios do not imply presented FPS or universal hardware guidance. Keep the original baseline distinct from the shipped v0.6.0 tag and preserve all observed regressions.')
    add('')
    return '\n'.join(output)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--summary', type=Path, required=True)
    parser.add_argument('--output', type=Path, required=True)
    parser.add_argument('--claims-output', type=Path)
    parser.add_argument('--acceptance-output', type=Path)
    args = parser.parse_args()
    claims_path = args.claims_output or args.output.with_name('RELEASE-CLAIMS.md')
    acceptance_path = args.acceptance_output or args.output.with_name('OUTPUT-ACCEPTANCE.md')
    require(all(not path.exists() for path in (args.output, claims_path, acceptance_path)),
            'report output exists; preserve it and choose new output paths')
    content = args.summary.read_bytes()
    summary = json.loads(content)
    result = render(summary, args.summary.name, hashlib.sha256(content).hexdigest())
    claims, acceptance = render_claims(summary), render_acceptance(summary)
    args.output.parent.mkdir(parents=True, exist_ok=True)
    with args.output.open('x') as stream:
        stream.write(result)
    for path, value in ((claims_path, claims), (acceptance_path, acceptance)):
        path.parent.mkdir(parents=True, exist_ok=True)
        with path.open('x') as stream:
            stream.write(value)
    print(json.dumps({'report_written': True, 'bytes': len(result.encode()),
                      'claims_written': True, 'acceptance_written': True}))


if __name__ == '__main__':
    main()
