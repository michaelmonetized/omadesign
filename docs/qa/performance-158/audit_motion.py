#!/usr/bin/env python3
"""Audit completed motion artifacts; never launch a workload.

Usage: audit_motion.py /path/to/performance-158/motion --output audit.json
Requires Pillow and the complete local inputs, font snapshots, PNGs and receipts.
Native sample counts may differ: both sides measure 30 seconds of real playback.
"""
import argparse
from collections import defaultdict
from functools import lru_cache
import hashlib
import json
import math
from pathlib import Path
import statistics
import struct

from PIL import Image, ImageChops


DOCS = {'announcement': '0.6.0-announcement-thumbnail.oma',
        'live-stream': 'Live Stream Working Document.oma'}
TIMES = (0., .1, .4, .8, 1.2, 1.8, 2.4, 3.2)
CHECKS = ('normal_elapsed_time_playback', 'documents_unchanged',
          'original_and_copied_files_unchanged', 'font_files_and_snapshots_unchanged',
          'native_screenshot_received')
RAW_METRICS = ('ui_ms', 'input_to_ui_ms', 'input_interval_ms', 'frame_interval_ms')
INPUT_METRICS = ('ui_ms', 'input_to_ui_ms', 'input_interval_ms', 'ui_completion_interval_ms')


def require(condition, message):
    if not condition:
        raise ValueError(message)


def read(path):
    return json.loads(path.read_text())


def process_receipt(path):
    receipt = read(path)
    require(receipt['exit_code'] == 0, f'{path}: failed process')
    require(not receipt.get('timed_out') and not receipt.get('errors')
            and receipt.get('success', True) is True, f'{path}: incomplete process')
    return receipt


@lru_cache(maxsize=None)
def digest(path):
    checksum = hashlib.sha256()
    with Path(path).open('rb') as stream:
        for block in iter(lambda: stream.read(1024 * 1024), b''):
            checksum.update(block)
    return checksum.hexdigest()


def stats(values):
    values = sorted(values)
    require(values and all(math.isfinite(v) and v >= 0 for v in values), 'invalid samples')
    return {'samples': len(values), 'median': statistics.median(values),
            'p95': values[math.ceil(.95 * len(values)) - 1], 'maximum': values[-1]}


def compare(old, new):
    return {'baseline': stats(old), 'candidate': stats(new)}


def f32(value):
    return struct.unpack('f', struct.pack('f', value))[0]


def check_input(entry):
    copy = Path(entry['copy'])
    require(copy.stat().st_size == entry['bytes'] and digest(copy) == entry['sha256'],
            f'copied input mismatch: {copy}')
    require(digest(entry['source']) == entry['sha256'], f'source mismatch: {entry["source"]}')
    fonts = []
    for font in entry['fonts']:
        for field in ('loaded_from', 'snapshot'):
            path = Path(font[field])
            require(path.stat().st_size == font['bytes'] and digest(path) == font['sha256'],
                    f'font mismatch: {path}')
        fonts.append((font['reference'], font['bytes'], font['sha256']))
    return (entry['bytes'], entry['sha256'], sorted(fonts))


def native(path, expected_tabs=24, timed_matrix=True):
    report = read(path)
    require(report['renderer'] == 'native WGPU', f'{path}: renderer')
    require(report['requested_window'] == [1650, 2131], f'{path}: window')
    require(report['open_documents'] == len(report['inputs']) == expected_tabs, f'{path}: tabs')
    for key in CHECKS:
        require(report['checks'].get(key) is True, f'{path}: failed {key}')
    inputs = [check_input(entry) for entry in report['inputs']]
    phases = [sample['phase'] for sample in report['samples']]
    phase_blocks = [phase for index, phase in enumerate(phases)
                    if index == 0 or phase != phases[index - 1]]
    require(phase_blocks == ['warmup', 'playback', 'screenshot'],
            f'{path}: incomplete or reordered phases')
    samples = [s for s in report['samples'] if s['phase'] == 'playback']
    require(len(samples) >= 2, f'{path}: insufficient playback samples')
    require(report['checks']['playhead_advanced_ui_calls'] == sum(
        s['playhead_before'] != s['playhead_after'] for s in samples), f'{path}: advanced count')
    require(report['checks']['canvas_render_key_updated_ui_calls'] == sum(
        s['canvas_changed'] for s in samples), f'{path}: render count')
    require(report['checks']['playhead_advanced_ui_calls'] > 0
            and report['checks']['canvas_render_key_updated_ui_calls'] > 0, f'{path}: no motion')
    duration = f32(max(report['motion']['duration'], .05))
    for previous, sample in zip(report['samples'], report['samples'][1:]):
        if sample['phase'] != 'playback':
            continue
        require(sample['egui_time'] >= previous['egui_time'] and sample['playing_after'] is True,
                f'{path}: playback clock stopped or reversed')
        require(sample['playhead_before'] == previous['playhead_after'],
                f'{path}: discontinuous playback position')
        elapsed = f32(max(sample['egui_time'] - previous['egui_time'], 0.))
        expected = f32(sample['playhead_before'] + elapsed)
        if expected > duration:
            expected = f32(math.fmod(expected, duration)) if report['motion']['looped'] else duration
        require(sample['playhead_after'] == expected, f'{path}: elapsed-time clock mismatch')
    grouped = []
    for sample in samples:
        for metric in RAW_METRICS:
            require(math.isfinite(sample[metric]) and sample[metric] >= 0, f'{path}: {metric}')
        if grouped and grouped[-1]['input_sequence'] == sample['input_sequence']:
            item = grouped[-1]
            item['ui_calls'] += 1
            item['ui_ms'] += sample['ui_ms']
            item['input_to_ui_ms'] = sample['input_to_ui_ms']
            item['ui_completion_interval_ms'] += sample['frame_interval_ms']
        else:
            require(not grouped or sample['input_sequence'] > grouped[-1]['input_sequence'],
                    f'{path}: unordered native callbacks')
            grouped.append({key: sample[key] for key in ('input_sequence', 'ui_ms',
                            'input_to_ui_ms', 'input_interval_ms')} | {
                                'ui_calls': 1, 'ui_completion_interval_ms': sample['frame_interval_ms']})
    require(grouped == report['native_input_samples'], f'{path}: per-input aggregates differ')
    require(len(samples) == report['playback']['ui_calls'] and len(grouped) ==
            report['playback']['native_input_callbacks'], f'{path}: callback counts differ')
    seconds = report['playback_seconds']
    require(math.isfinite(seconds) and seconds > 0, f'{path}: elapsed duration')
    for field, count in [('observed_ui_calls_per_second', len(samples)),
                         ('observed_native_input_callbacks_per_second', len(grouped))]:
        require(math.isclose(report[field], count / seconds, rel_tol=1e-12), f'{path}: {field}')
    require(report['measurement_limit_seconds'] == 30 and seconds >= 30
            and report['stop_reason'] == 'measurement_time_limit', f'{path}: stopping condition')
    require(report['requested_ui_calls'] == 100000 and report['warmup_min_seconds'] == 2
            and report['warmup_ui_calls'] >= (30 if timed_matrix else 2),
            f'{path}: changed native protocol')
    with Image.open(path.parent / 'native-editor.png') as image:
        require(list(image.size) == report['screenshot_size'] == [1650, 2131], f'{path}: screenshot')
    return report, inputs, samples, grouped


def pixels(before, after):
    with Image.open(before) as image:
        old = image.convert('RGBA')
    with Image.open(after) as image:
        new = image.convert('RGBA')
    require(old.size == new.size == (1240, 1800), f'fixed-frame dimensions: {before}, {after}')
    old_bytes, new_bytes = old.tobytes(), new.tobytes()
    difference = ImageChops.difference(old, new)
    channels = difference.split()
    combined = channels[0]
    for channel in channels[1:]:
        combined = ImageChops.lighter(combined, channel)
    return {'baseline': str(before), 'candidate': str(after), 'size': list(old.size),
            'equal_rgba': old_bytes == new_bytes,
            'different_pixels': old.width * old.height - combined.histogram()[0],
            'maximum_channel_delta': max(high for _, high in difference.getextrema()),
            'baseline_rgba_sha256': hashlib.sha256(old_bytes).hexdigest(),
            'candidate_rgba_sha256': hashlib.sha256(new_bytes).hexdigest()}


def cpu_profiles(root, document):
    sides = [root / f'{side}-{document}' for side in ('baseline', 'candidate')]
    reports = [read(side / 'profile.json') for side in sides]
    old, new = reports
    require(old['window'] == new['window'] == [1240, 1800] and old['view'] == new['view'],
            f'{document}: CPU view mismatch')
    require(old['view'] == {'scale': .625, 'offset': [20, 20]}, f'{document}: unexpected CPU view')
    for report in reports:
        require(len(report['samples']) == 24, f'{document}: CPU sample count')
        for index, sample in enumerate(report['samples']):
            require(sample['round'] == index // 8 and math.isclose(
                sample['time'], TIMES[index % 8], abs_tol=1e-6), f'{document}: CPU timestamps')
    frames = [pixels(sides[0] / f'frame-{index}.png', sides[1] / f'frame-{index}.png')
              for index in range(8)]
    timing = compare([s['render_ms'] for s in old['samples']], [s['render_ms'] for s in new['samples']])
    rounds = {str(r): compare([s['render_ms'] for s in old['samples'] if s['round'] == r],
                             [s['render_ms'] for s in new['samples'] if s['round'] == r])
              for r in range(3)}
    return {'render_ms': timing, 'rounds': rounds, 'frames': frames,
            'profiles': {side: {'result_sha256': digest(directory / 'profile.json')}
                         for side, directory in zip(('baseline', 'candidate'), sides)}}


def occlusion(root):
    results = {}
    for side in ('baseline', 'candidate'):
        paths = sorted(root.glob(f'{side}-occlusion*/occlusion-receipt.json'))
        require(len(paths) == 1, f'{side}: expected exactly one retained occlusion run')
        for path in paths:
            receipt = process_receipt(path)
            events = receipt['events']
            require([event['action'] for event in events] == ['hide', 'show'], f'{path}: events')
            require(all(event['result'] and all(r.get('success') is True for r in event['result'])
                        for event in events), f'{path}: visibility command failed')
            # Match the runner and original retained baseline exactly. Deriving
            # bounds from late workspace commands changes which samples count.
            interior = [s for s in receipt['samples'] if 12. <= s['elapsed'] <= 15.5]
            require(len(interior) >= 2, f'{path}: insufficient hidden-interior observations')
            first, last = interior[0], interior[-1]
            require(all(s['phase'] == 'hidden' for s in interior)
                    and events[0]['elapsed'] <= first['elapsed'] < last['elapsed'] <= events[1]['elapsed'],
                    f'{path}: strict hidden interval was not hidden')
            seconds = last['elapsed'] - first['elapsed']
            require(seconds > 0, f'{path}: hidden duration')
            user = last['process_user_seconds'] - first['process_user_seconds']
            system = last['process_system_seconds'] - first['process_system_seconds']
            calls = last['io']['syscr'] - first['io']['syscr']
            require(min(user, system, calls) >= 0, f'{path}: nonmonotonic counters')
            report, _, _, _ = native(path.parent / 'workload/motion-result.json', 1, False)
            results[path.parent.name] = {'receipt_sha256': digest(path), 'seconds': seconds,
                'result_sha256': digest(path.parent / 'workload/motion-result.json'),
                'process_cpu_percent': (user + system) / seconds * 100,
                'read_calls_per_second': calls / seconds, 'native_checks': report['checks'],
                'excluded_from_playback_matrix': True}
    return results


def audit(root):
    pinned = read(root / 'inputs.json')
    native_results, cpu = {}, {}
    for document, filename in DOCS.items():
        snapshot = root / 'inputs' / filename
        require(digest(snapshot) == pinned[filename]['sha256'] and snapshot.stat().st_size ==
                pinned[filename]['bytes'], f'fixed input mismatch: {filename}')
        paths = {}
        for side in ('baseline', 'candidate'):
            discovered = set(root.glob(f'{side}-native-{document}-*/motion-result.json'))
            expected = {repetition: root / f'{side}-native-{document}-{repetition}/motion-result.json'
                        for repetition in (1, 2)}
            require(discovered == set(expected.values()),
                    f'{document}: expected exactly two paired native runs')
            paths[side] = expected
        pool = {side: defaultdict(list) for side in paths}
        runs = []
        for repetition in sorted(paths['baseline']):
            loaded = [native(paths[side][repetition]) for side in paths]
            require(loaded[0][1] == loaded[1][1], f'{document}-{repetition}: input/font mismatch')
            require(loaded[0][1][0][1] == pinned[filename]['sha256'], f'{document}: wrong active input')
            for field in ('canvas_size', 'active_shapes', 'motion', 'requested_ui_calls',
                          'measurement_limit_seconds', 'warmup_min_seconds'):
                require(loaded[0][0][field] == loaded[1][0][field], f'{document}: changed {field}')
            paired = {'repetition': repetition}
            for side, (report, _, samples, grouped) in zip(paths, loaded):
                receipt_path = root / f'{side}-native-{document}-{repetition}-receipt.json'
                receipt = process_receipt(receipt_path)
                for metric in RAW_METRICS:
                    pool[side][f'ui_calls/{metric}'].extend(s[metric] for s in samples)
                for metric in INPUT_METRICS:
                    pool[side][f'native_inputs/{metric}'].extend(s[metric] for s in grouped)
                paired[side] = {'ui_calls': len(samples), 'native_input_callbacks': len(grouped),
                    'playback_seconds': report['playback_seconds'], 'checks': report['checks'],
                    'ui_calls_per_second': report['observed_ui_calls_per_second'],
                    'native_input_callbacks_per_second': report['observed_native_input_callbacks_per_second'],
                    'process_resource': receipt.get('resource'), 'wall_seconds': receipt.get('wall_seconds'),
                    'result_sha256': digest(paths[side][repetition]), 'receipt_sha256': digest(receipt_path)}
            runs.append(paired)
        native_results[document] = {'runs': runs, 'pooled_ms': {
            metric: compare(pool['baseline'][metric], pool['candidate'][metric])
            for metric in pool['baseline']}}
        cpu[document] = cpu_profiles(root, document)
    equal = all(frame['equal_rgba'] for result in cpu.values() for frame in result['frames'])
    return {'passed': equal, 'artifact_root': str(root),
            'method': 'Conventional median; nearest-rank p95. Rates are callbacks, not presented FPS.',
            'native': native_results, 'cpu': cpu, 'equal_fixed_frames': equal,
            'fixed_frame_pairs': sum(len(result['frames']) for result in cpu.values()),
            'occlusion': occlusion(root)}


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('root', type=Path)
    parser.add_argument('--output', type=Path, required=True)
    args = parser.parse_args()
    try:
        result = audit(args.root.resolve())
    except (ValueError, KeyError, OSError, TypeError) as error:
        result = {'passed': False, 'error': str(error)}
    args.output.parent.mkdir(parents=True, exist_ok=True)
    args.output.write_text(json.dumps(result, indent=2, allow_nan=False) + '\n')
    print(json.dumps({'passed': result['passed'], 'output': str(args.output)}))
    raise SystemExit(0 if result['passed'] else 1)


if __name__ == '__main__':
    main()
