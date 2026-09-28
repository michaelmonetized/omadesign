#!/usr/bin/env python3
"""Compute pooled medians and nearest-rank p95 from native raw frame samples."""
import argparse
import json
import math
from pathlib import Path
import statistics


def stats(values):
    values = sorted(values)
    return {'samples': len(values), 'median_ms': statistics.median(values),
            'p95_ms': values[math.ceil(len(values) * .95) - 1], 'max_ms': values[-1]}


def collect(directory):
    manifest = json.loads((directory / 'manifest.json').read_text())
    groups = {}
    for run in manifest['runs']:
        name = run['name']
        result = directory / name / ('authoring-result.json' if name.startswith('authoring-') else 'interaction-result.json')
        data = json.loads(result.read_text())
        scenario = data.get('scenario', 'authoring')
        for sample in data['samples']:
            key = scenario + '/' + sample['phase']
            row = groups.setdefault(key, {'ui_ms': [], 'input_to_ui_ms': [], 'frame_interval_ms': []})
            for measure in row:
                row[measure].append(sample[measure])
    return {phase: {measure: stats(values) for measure, values in row.items()}
            for phase, row in groups.items()}


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('before', type=Path)
    parser.add_argument('after', type=Path)
    parser.add_argument('output', type=Path)
    args = parser.parse_args()
    before, after = collect(args.before), collect(args.after)
    compared = {phase: {'before': before[phase], 'after': after[phase],
                       'median_speedup': before[phase]['ui_ms']['median_ms'] / after[phase]['ui_ms']['median_ms']}
                for phase in sorted(before.keys() & after.keys())}
    args.output.write_text(json.dumps(compared, indent=2) + '\n')
    print('| Workload | Before median / p95 CPU ms | After median / p95 CPU ms | Median speedup |')
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
