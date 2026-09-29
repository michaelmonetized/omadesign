#!/usr/bin/env python3
"""Audit a completed before/after matrix without running any native workloads.

Use the full local result directories, including editable files and PNGs:
  python audit.py /path/to/before /path/to/after /path/to/audit.json
Timing increases are observations from two repetitions, not significance tests.
Only exported infographic pixels are compared; native UI screenshots can change
because thumbnail rendering and canvas pixel conversion were deliberately fixed.
"""
import argparse
from collections import Counter, defaultdict
import hashlib
import json
import math
from pathlib import Path
import statistics

from PIL import Image, ImageChops


SCENARIOS = ('simple', 'complex', 'complex-middle', 'complex-same-layer', 'many-tabs', 'authoring')
EXPECTED_RUNS = {f'{scenario}-{repeat}' for scenario in SCENARIOS for repeat in (1, 2)}
MEASURES = ('ui_ms', 'input_to_ui_ms', 'frame_interval_ms')
RESOURCE_MEASURES = ('peak_rss_KiB', 'wall_seconds', 'user_seconds', 'system_seconds', 'major_page_faults')
INTERACTION_COUNTS = {'drag-move': 360, 'drag-down': 4, 'drag-up': 4, 'undo': 4, 'redo': 4}
AUTHORING_COUNTS = {'typing': 83, 'brush-drag': 90, 'pan': 40, 'zoom': 60, 'save': 1}


def require(condition, message):
    if not condition:
        raise ValueError(message)


def digest(path):
    checksum = hashlib.sha256()
    with path.open('rb') as stream:
        for chunk in iter(lambda: stream.read(1024 * 1024), b''):
            checksum.update(chunk)
    return checksum.hexdigest()


def stats(values):
    ordered = sorted(values)
    return {'samples': len(ordered), 'median': statistics.median(ordered),
            'p95': ordered[math.ceil(len(ordered) * .95) - 1], 'max': ordered[-1]}


def delta(before, after):
    return {'before': before, 'after': after, 'delta': after - before,
            'percent': (after / before - 1) * 100 if before else None}


def load_matrix(directory):
    manifest = json.loads((directory / 'manifest.json').read_text())
    names = [run['name'] for run in manifest['runs']]
    require(len(names) == 12 and set(names) == EXPECTED_RUNS,
            f'{directory}: expected exactly two complete repetitions of all six scenarios')
    require(manifest['window'] == [1440, 900], f'{directory}: unexpected window dimensions')
    reports, resources, artifacts = {}, {}, {}
    pooled = defaultdict(lambda: {measure: [] for measure in MEASURES})
    for resource in manifest['runs']:
        name = resource['name']
        scenario = name.rsplit('-', 1)[0]
        require(resource['exit_code'] == 0 and resource.get('timed_out') is False,
                f'{directory}/{name}: workload failed or timed out')
        require(resource.get('network') == 'disabled by unshare -Urn',
                f'{directory}/{name}: missing network-isolation receipt')
        standalone = json.loads((directory / f'{name}-resources.json').read_text())
        require(standalone == resource, f'{directory}/{name}: resource receipt differs from manifest')
        for measure in RESOURCE_MEASURES:
            require(isinstance(resource.get(measure), (int, float)) and resource[measure] >= 0,
                    f'{directory}/{name}: invalid process resource {measure}')
        result_name = 'authoring-result.json' if scenario == 'authoring' else 'interaction-result.json'
        report = json.loads((directory / name / result_name).read_text())
        require(report['renderer'] == 'native WGPU', f'{directory}/{name}: unexpected renderer')
        counts = Counter(sample['phase'] for sample in report['samples'])
        expected = AUTHORING_COUNTS if scenario == 'authoring' else INTERACTION_COUNTS
        for phase, count in expected.items():
            require(counts[phase] == count, f'{directory}/{name}: {phase} has {counts[phase]}, expected {count}')
        if scenario == 'authoring':
            for key, value in {'typed_objects': 5, 'pen_paths': 3, 'brush_strokes': 3,
                               'undo_redo': True, 'save_reopen_exact': True}.items():
                require(report.get(key) == value, f'{directory}/{name}: failed {key}')
            for phase, minimum in {'typing': 83, 'brush-drag': 85, 'pan': 35, 'zoom': 55}.items():
                changed = sum(sample['canvas_changed'] for sample in report['samples'] if sample['phase'] == phase)
                require(changed >= minimum, f'{directory}/{name}: insufficient actual {phase} updates')
            files = [result_name, 'infographic.oma', 'infographic.png', 'native-editor.png']
        else:
            require(report['scenario'] == scenario, f'{directory}/{name}: wrong scenario')
            checks = report['checks']
            require(checks.get('moves_and_undo_redo') == 12, f'{directory}/{name}: missing position checks')
            for key in ('continuous_object_motion', 'continuous_canvas_updates',
                        'inactive_documents_unchanged', 'tab_selection_and_history', 'save_reopen_exact'):
                require(checks.get(key) is True, f'{directory}/{name}: failed {key}')
            expected_shapes = 1201 if scenario.startswith('complex') else 1
            require(report['active_shapes'] == expected_shapes, f'{directory}/{name}: wrong active fixture')
            require(report['open_documents'] == (24 if scenario == 'many-tabs' else 1),
                    f'{directory}/{name}: wrong tab count')
            require(report['inactive_shapes_each'] == (320 if scenario == 'many-tabs' else 0),
                    f'{directory}/{name}: wrong inactive fixture')
            moving = [sample for sample in report['samples'] if sample['phase'] == 'drag-move']
            for key in ('object_moved', 'canvas_changed'):
                require(sum(sample[key] for sample in moving) >= 352,
                        f'{directory}/{name}: too few actual {key} samples')
            files = [result_name, 'interaction.oma', 'native-editor.png']
        for filename in files:
            path = directory / name / filename
            require(path.is_file() and path.stat().st_size > 0, f'missing artifact: {path}')
            artifacts[f'{name}/{filename}'] = {'bytes': path.stat().st_size, 'sha256': digest(path)}
        with Image.open(directory / name / 'native-editor.png') as image:
            require(image.size == (1440, 900), f'{directory}/{name}: screenshot dimensions changed')
        for sample in report['samples']:
            for measure in MEASURES:
                value = sample[measure]
                require(isinstance(value, (int, float)) and math.isfinite(value) and value >= 0,
                        f'{directory}/{name}: invalid timing sample')
                if sample['phase'] in expected:
                    pooled[f'{scenario}/{sample["phase"]}'][measure].append(value)
        reports[name], resources[name] = report, resource
    return manifest, reports, resources, dict(pooled), artifacts


def compare_export(before, after):
    with Image.open(before) as image:
        old = image.convert('RGBA')
    with Image.open(after) as image:
        new = image.convert('RGBA')
    result = {'before': str(before), 'after': str(after), 'before_size': list(old.size),
              'after_size': list(new.size), 'equal_pixels': False}
    if old.size != new.size:
        return result
    old_bytes, new_bytes = old.tobytes(), new.tobytes()
    result['equal_pixels'] = old_bytes == new_bytes
    difference = ImageChops.difference(old, new)
    # Merge all four channels so alpha-only differences cannot disappear.
    bands = difference.split()
    combined = bands[0]
    for band in bands[1:]:
        combined = ImageChops.lighter(combined, band)
    histogram = combined.histogram()
    result['different_pixels'] = old.width * old.height - histogram[0]
    result['maximum_channel_delta'] = max(high for _, high in difference.getextrema())
    result['difference_bounds'] = combined.getbbox()
    result['before_rgba_sha256'] = hashlib.sha256(old_bytes).hexdigest()
    result['after_rgba_sha256'] = hashlib.sha256(new_bytes).hexdigest()
    return result


def audit(before, after):
    old_manifest, old_reports, old_resources, old_pool, old_artifacts = load_matrix(before)
    new_manifest, new_reports, new_resources, new_pool, new_artifacts = load_matrix(after)
    for field in ('host', 'architecture', 'kernel', 'window'):
        require(old_manifest[field] == new_manifest[field], f'matrix environment differs: {field}')
    for name in EXPECTED_RUNS:
        require(old_reports[name]['canvas_size'] == new_reports[name]['canvas_size'],
                f'{name}: paired native canvas dimensions differ')
        if name.startswith('authoring-'):
            for field in ('layers', 'vector_shapes'):
                require(old_reports[name][field] == new_reports[name][field], f'{name}: output {field} differ')
    require(old_pool.keys() == new_pool.keys(), 'pooled workload phases differ')
    timing, increases = {}, []
    for phase in sorted(old_pool):
        timing[phase] = {}
        for measure in MEASURES:
            old, new = stats(old_pool[phase][measure]), stats(new_pool[phase][measure])
            require(old['samples'] == new['samples'], f'{phase}/{measure}: unequal sample counts')
            comparisons = {key: delta(old[key], new[key]) for key in ('median', 'p95', 'max')}
            timing[phase][measure] = {'samples_each': old['samples'], **comparisons}
            for key in ('median', 'p95'):
                if new[key] > old[key]:
                    increases.append({'phase': phase, 'measure': measure, 'statistic': key,
                                      'samples_each': old['samples'], **comparisons[key]})
    resources = {name: {measure: delta(old_resources[name][measure], new_resources[name][measure])
                        for measure in RESOURCE_MEASURES} for name in sorted(EXPECTED_RUNS)}
    resource_summary, resource_increases = {}, []
    for scenario in SCENARIOS:
        resource_summary[scenario] = {}
        for measure in RESOURCE_MEASURES:
            old = [old_resources[f'{scenario}-{repeat}'][measure] for repeat in (1, 2)]
            new = [new_resources[f'{scenario}-{repeat}'][measure] for repeat in (1, 2)]
            resource_summary[scenario][measure] = {
                'median': delta(statistics.median(old), statistics.median(new)),
                'maximum': delta(max(old), max(new)),
            }
            comparison = resource_summary[scenario][measure]['median']
            if comparison['delta'] > 0:
                resource_increases.append({'scenario': scenario, 'measure': measure,
                                           'statistic': 'median_of_two_runs', **comparison})
    exports = [compare_export(before / 'authoring-1/infographic.png',
                              directory / f'authoring-{repeat}/infographic.png')
               for directory, repeat in ((before, 2), (after, 1), (after, 2))]
    return {
        'success': all(export['equal_pixels'] for export in exports),
        'before_label': old_manifest['label'], 'after_label': new_manifest['label'],
        'successful_runs_each': 12, 'native_assertions_and_sample_parity': True,
        'export_pixel_comparisons': exports, 'timing': timing,
        'observed_timing_increases': increases, 'resources_per_run': resources,
        'resources_by_scenario': resource_summary,
        'observed_resource_increases': resource_increases,
        'manifest_sha256': {'before': digest(before / 'manifest.json'),
                            'after': digest(after / 'manifest.json')},
        'artifacts': {'before': old_artifacts, 'after': new_artifacts},
        'notes': [
            'UI timings are elapsed time around CPU-side Studio::ui; no GPU/presentation latency claim.',
            'Input-to-UI starts at synthetic raw_input_hook injection; intervals end at successive UI completions.',
            'Timing/memory increases are observations, not proof of statistically significant regression.',
            'Each resource median summarizes two whole-process values; RSS is per-process peak resident memory.',
            'Save has two samples per side; its tail is not statistically meaningful.',
            'Full exported infographic RGBA pixels are compared; native UI screenshots are retained and dimension-checked.',
        ],
    }


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('before', type=Path)
    parser.add_argument('after', type=Path)
    parser.add_argument('output', type=Path)
    args = parser.parse_args()
    try:
        report = audit(args.before.resolve(), args.after.resolve())
    except (OSError, ValueError, KeyError, TypeError) as error:
        report = {'success': False, 'error': str(error)}
    args.output.parent.mkdir(parents=True, exist_ok=True)
    args.output.write_text(json.dumps(report, indent=2) + '\n')
    if not report['success']:
        raise SystemExit(f'Audit failed; inspect {args.output}')
    print('PASS: 12 successful runs per side; native assertions/sample parity and all exported pixels match.')
    print('| Scenario | Before / after median peak RSS MiB | RSS change |')
    print('| --- | ---: | ---: |')
    for scenario, metrics in report['resources_by_scenario'].items():
        rss = metrics['peak_rss_KiB']['median']
        print(f"| {scenario} | {rss['before'] / 1024:.2f} / {rss['after'] / 1024:.2f} | {rss['percent']:+.2f}% |")
    print(f"Observed timing increases to review: {len(report['observed_timing_increases'])}; see {args.output}")


if __name__ == '__main__':
    main()
