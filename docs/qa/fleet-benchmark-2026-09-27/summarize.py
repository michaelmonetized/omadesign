"""Summarize the three completed suites without mixing historical test conditions."""
import json
import math
import pathlib
import statistics
import sys

root = pathlib.Path(sys.argv[1])
summary = {}
expected_inputs = None
expected_x86 = None
phases = ['idle', 'pen-preview', 'pen-commit', 'typing', 'brush-drag', 'pan', 'zoom', 'save']


def percentiles(values):
    v = sorted(values)
    return {'n': len(v), 'p50': statistics.median(v), 'p95': v[math.ceil(len(v) * 0.95) - 1], 'max': v[-1]}


for host in ['hpeliteclient', 'intelpro', 'm1pro16']:
    directory = root / host / 'results'
    load = lambda p: json.loads((directory / p).read_text())
    assert load('complete.json')['success'], f'{host} incomplete'
    info = load('host.json')
    inputs = {p: v['sha256'] for p, v in info['artifacts'].items() if p.startswith('inputs/')}
    if expected_inputs is None:
        expected_inputs = inputs
    assert inputs == expected_inputs, f'{host} input drift'
    if info['architecture'] == 'x86_64':
        executables = {p: v['sha256'] for p, v in info['artifacts'].items() if p.startswith(('bin/', 'lib/'))}
        if expected_x86 is None:
            expected_x86 = executables
        assert executables == expected_x86, 'x86 binary/runtime drift'
    runs = [load(f'authoring-{i}/authoring-result.json') for i in [1, 2]]
    for run in runs:
        assert run['canvas_size'] == [1020., 770.], f'{host} canvas mismatch'
        assert run['save_reopen_exact'] and run['undo_redo']
        assert run['layers'] == 20 and run['vector_shapes'] == 576
    samples = [s for run in runs for s in run['samples']]
    timing = {}
    for phase in phases:
        subset = [s for s in samples if s['phase'] == phase]
        timing[phase] = {
            'ui_ms': percentiles([s['ui_ms'] for s in subset]),
            'interval_ms': percentiles([s['frame_interval_ms'] for s in subset]),
            'redraws': sum(s['canvas_changed'] for s in subset),
            'per_run': [
                {'frames': p['n'], 'p50_ms': p['p50'], 'p95_ms': p['p95'], 'max_ms': p['max']}
                for p in [percentiles([s['ui_ms'] for s in run['samples'] if s['phase'] == phase]) for run in runs]
            ],
        }
    resources = {}
    for path in sorted(directory.glob('*-resources.json')):
        value = json.loads(path.read_text())
        assert value['returncode'] == 0 and not value['timed_out'], str(path)
        resources[value['name']] = {key: value[key] for key in [
            'name', 'returncode', 'timed_out', 'wall_seconds', 'peak_rss_KiB',
            'user_seconds', 'system_seconds', 'major_page_faults', 'network', 'limits',
        ]}
    assert len(resources) == 11, (host, len(resources))
    profile = load('canvas-profile.log')
    warm = [r['render_ms'] for r in profile if r['layer'] == 'ALL' and r['round'] > 0]
    summary[host] = {
        'spec': info, 'graphics': load('graphics.json'), 'authoring': timing,
        'resources': resources, 'warm_canvas_median_ms': statistics.median(warm),
        'native_background': load('background_removal_qa-native/native-result.json'),
        'native_upscale': load('upscale_qa-native/native-result.json'),
        'input_hashes_match': True, 'canvas_size': [1020, 770],
    }
(root / 'summary.json').write_text(json.dumps(summary, indent=2) + '\n')
print('Verified 33 completed processes, identical fixtures, identical x86 binaries, and equal canvas sizes.')
for host, value in summary.items():
    print(host, {phase: {k: round(v, 2) if isinstance(v, float) else v for k, v in value['authoring'][phase]['ui_ms'].items()} for phase in ['typing', 'brush-drag', 'pan', 'zoom']})
