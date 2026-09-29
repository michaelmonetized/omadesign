#!/usr/bin/env python3
"""Compute pooled medians and nearest-rank p95 from native raw frame samples."""
import argparse
import json
import math
from pathlib import Path
import statistics

MEASURES = ('ui_ms', 'input_to_ui_ms', 'frame_interval_ms')
SCENARIOS = {'simple', 'complex', 'complex-middle', 'complex-same-layer', 'many-tabs', 'authoring'}
# These phases include asynchronous preview/screenshot waits or initialization.
# Workload phase counts must match; coordination frame counts need not.
COORDINATION_PHASES = {'startup', 'check', 'setup', 'preview-wait'}


def stats(values):
    values = sorted(values)
    return {'samples': len(values), 'median_ms': statistics.median(values),
            'p95_ms': values[math.ceil(len(values) * .95) - 1], 'max_ms': values[-1]}


def collect(directory):
    manifest = json.loads((directory / 'manifest.json').read_text())
    groups = {}
    signatures = {}
    identities = set()
    runs = manifest.get('runs')
    if not isinstance(runs, list) or not runs:
        raise ValueError(f'{directory}: manifest has no completed runs')
    for run in runs:
        name = run['name']
        expected_scenario, separator, repeat = name.rpartition('-')
        if not separator or expected_scenario not in SCENARIOS or not repeat.isdigit() or int(repeat) < 1:
            raise ValueError(f'{directory}: invalid run name {name!r}')
        identity = (expected_scenario, int(repeat))
        if identity in identities:
            raise ValueError(f'{directory}: duplicate run {name}')
        identities.add(identity)
        if run.get('exit_code') != 0 or run.get('timed_out', False):
            raise ValueError(f'{directory}: failed or timed-out run {name}')
        result = directory / name / ('authoring-result.json' if name.startswith('authoring-') else 'interaction-result.json')
        data = json.loads(result.read_text())
        scenario = data.get('scenario', 'authoring')
        if scenario != expected_scenario:
            raise ValueError(f'{result}: scenario {scenario!r} does not match run {name}')
        samples = data.get('samples')
        if not isinstance(samples, list) or not samples:
            raise ValueError(f'{result}: no raw samples')
        phases = {}
        for sample in samples:
            phase = sample['phase']
            if not isinstance(phase, str) or not phase:
                raise ValueError(f'{result}: invalid sample phase')
            if phase not in COORDINATION_PHASES:
                phases[phase] = phases.get(phase, 0) + 1
            key = scenario + '/' + sample['phase']
            row = groups.setdefault(key, {measure: [] for measure in MEASURES})
            for measure in row:
                value = sample[measure]
                if not isinstance(value, (float, int)) or isinstance(value, bool) or not math.isfinite(value) or value < 0:
                    raise ValueError(f'{result}: invalid {measure} sample {value!r}')
                row[measure].append(value)
        if not phases:
            raise ValueError(f'{result}: no workload samples')
        signatures[identity] = phases
    scenarios = {scenario for scenario, _ in identities}
    if 'scenarios' in manifest and scenarios != set(manifest['scenarios']) | {'authoring'}:
        raise ValueError(f'{directory}: completed scenarios do not match the requested manifest scenarios')
    rounds = manifest.get('rounds', max(repeat for _, repeat in identities))
    if not isinstance(rounds, int) or isinstance(rounds, bool) or rounds < 1:
        raise ValueError(f'{directory}: invalid manifest round count')
    if identities != {(scenario, repeat) for scenario in scenarios for repeat in range(1, rounds + 1)}:
        raise ValueError(f'{directory}: missing scenario repetitions')
    return ({phase: {measure: stats(values) for measure, values in row.items()}
             for phase, row in groups.items()}, signatures)


def compare(before_directory, after_directory):
    before, before_runs = collect(before_directory)
    after, after_runs = collect(after_directory)
    if before_runs.keys() != after_runs.keys():
        raise ValueError('before and after must contain the same scenarios and repetitions')
    for run, phases in before_runs.items():
        if phases != after_runs[run]:
            raise ValueError(f'{run[0]}-{run[1]}: workload phases or sample counts differ between before and after')
    compared = {}
    for phase in sorted(before.keys() & after.keys()):
        median = after[phase]['ui_ms']['median_ms']
        if median <= 0:
            raise ValueError(f'{phase}: after median UI duration must be positive')
        compared[phase] = {'before': before[phase], 'after': after[phase],
                           'median_speedup': before[phase]['ui_ms']['median_ms'] / median}
    return compared


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('before', type=Path)
    parser.add_argument('after', type=Path)
    parser.add_argument('output', type=Path)
    args = parser.parse_args()
    try:
        compared = compare(args.before, args.after)
    except (ValueError, KeyError, TypeError, OSError) as error:
        parser.error(str(error))
    args.output.write_text(json.dumps(compared, indent=2) + '\n')
    print('| Workload | Before median / p95 UI ms | After median / p95 UI ms | Median speedup |')
    print('| --- | ---: | ---: | ---: |')
    phases = [name + '/drag-move' for name in ['simple', 'complex', 'complex-middle', 'complex-same-layer', 'many-tabs']]
    phases += ['authoring/' + name for name in ['typing', 'brush-drag', 'pan', 'zoom', 'save']]
    for phase in phases:
        if phase not in compared:
            continue
        row = compared[phase]
        old, new = row['before']['ui_ms'], row['after']['ui_ms']
        print(f"| {phase} | {old['median_ms']:.2f} / {old['p95_ms']:.2f} | {new['median_ms']:.2f} / {new['p95_ms']:.2f} | {row['median_speedup']:.2f}x |")


if __name__ == '__main__':
    main()
