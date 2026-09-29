#!/usr/bin/env python3
"""Run paired issue-158 baseline/candidate workloads on one host, serially.

Requires an already prepared private GPU Wayland display. Does not install,
configure, build or launch a compositor. Results are local evidence, not a
performance acceptance decision. Each CLI/native AI case runs at least twice.
"""
import argparse
from collections import Counter
import datetime
import hashlib
import json
import math
import os
from pathlib import Path
import platform
import re
import signal
import stat
import struct
import subprocess
import time

BINS = ('authoring_qa', 'canvas_profile', 'upscale_qa', 'background_removal_qa', 'omadesign')
BASELINE = '221834bdd8af5aee8554a0e835629cbe2f467558'
MODELS = {'upscale': '74b0bf4bdad2868e256ef722980586b7ff1a8b2c9ba0b8e526498cd28acf5b80',
          'background': '309c8469258dda742793dce0ebea8e6dd393174f89934733ecc8b14c76f4ddd8'}
DISPLAY_KEYS = {'XDG_RUNTIME_DIR', 'WAYLAND_DISPLAY', 'SWAYSOCK', 'WINIT_UNIX_BACKEND'}
CASES = ('authoring', 'canvas-profile', 'upscale-320-2x', 'upscale-320-4x',
         'upscale-1024-2x', 'background-320', 'background-1024',
         'background_removal_qa-native', 'upscale_qa-native', 'package-reopen')
SWAYMSG = 'swaymsg'


def require(value, message):
    if not value:
        raise ValueError(message)


def utc():
    return datetime.datetime.now(datetime.timezone.utc).isoformat()


def read(path):
    return json.loads(Path(path).read_text())


def write(path, value):
    temporary = path.with_suffix(path.suffix + '.tmp')
    temporary.write_text(json.dumps(value, indent=2, allow_nan=False) + '\n')
    temporary.replace(path)


def identity(path):
    path = Path(path)
    h = hashlib.sha256()
    with path.open('rb') as source:
        for block in iter(lambda: source.read(1024 * 1024), b''):
            h.update(block)
    return {'bytes': path.stat().st_size, 'sha256': h.hexdigest()}


def inventory(directory):
    return {str(path.relative_to(directory)): identity(path)
            for path in sorted(directory.rglob('*')) if path.is_file()}


def command(argv, env=None):
    result = subprocess.run(argv, capture_output=True, text=True, timeout=15, env=env)
    return {'exit_code': result.returncode, 'stdout': result.stdout.strip(),
            'stderr': result.stderr.strip()}


def text_file(path):
    try:
        return Path(path).read_text().strip().strip('\0')
    except OSError:
        return None


def state():
    memory = {}
    for line in Path('/proc/meminfo').read_text().splitlines():
        key, value = line.split(':', 1)
        if key in ('MemTotal', 'MemAvailable', 'SwapTotal', 'SwapFree'):
            memory[key + '_KiB'] = int(value.split()[0])
    return {'memory': memory, 'loadavg': text_file('/proc/loadavg'),
            'pressure': {key: text_file('/proc/pressure/' + key) for key in ('cpu', 'memory', 'io')},
            'cpu_policies': {path.name: {key: text_file(path / key) for key in
                             ('scaling_governor', 'scaling_cur_freq', 'scaling_min_freq', 'scaling_max_freq')}
                             for path in Path('/sys/devices/system/cpu/cpufreq').glob('policy*')}}


def sway(env, query):
    result = command([SWAYMSG, '-s', env['SWAYSOCK'], '-r', '-t', query], env)
    require(result['exit_code'] == 0, f'Sway query failed: {query}')
    return json.loads(result['stdout'])


def display_state(env):
    outputs = [{key: item.get(key) for key in ('name', 'active', 'rect', 'scale', 'current_mode')}
               for item in sway(env, 'get_outputs')]
    active = [item for item in outputs if item['active']]
    require(len(active) == 1 and active[0]['scale'] == 1
            and active[0]['rect']['width'] >= 1440 and active[0]['rect']['height'] >= 900,
            'private display must have one scale-1 output fitting 1440x900')
    def clients(node):
        return int(bool(node.get('app_id') or node.get('window'))) + sum(
            clients(child) for key in ('nodes', 'floating_nodes') for child in node.get(key, []))
    # A just-exited client may still await compositor cleanup. This bounded
    # settle happens before the process timer, and never closes a window.
    deadline = time.monotonic() + 2
    while clients(sway(env, 'get_tree')):
        require(time.monotonic() < deadline, 'private display already has a client')
        time.sleep(.05)
    return outputs


def gpu_handles(pid):
    """Supporting process evidence; open DRM handles alone are not GPU timings."""
    observed = []
    try:
        files = list(Path(f'/proc/{pid}/fdinfo').iterdir())
    except OSError:
        return observed
    for path in files:
        content = text_file(path) or ''
        fields = {line.split(':', 1)[0]: line.split(':', 1)[1].strip()
                  for line in content.splitlines() if line.startswith('drm-') and ':' in line}
        if 'drm-driver' in fields:
            observed.append(fields)
    return observed


def png_size(path):
    with path.open('rb') as image:
        header = image.read(24)
    require(header[:8] == b'\x89PNG\r\n\x1a\n' and header[12:16] == b'IHDR', f'invalid PNG: {path}')
    return list(struct.unpack('>II', header[16:24]))


def native_size(path):
    require(png_size(path) == [1440, 900], f'native window dimensions changed: {path}')


def validate(case, directory, log, canvas):
    if case == 'authoring':
        result = read(directory / 'authoring-result.json')
        require(result['canvas_size'] == canvas, 'authoring canvas geometry changed')
        require(result['renderer'] == 'native WGPU' and result['layers'] == 20 and result['vector_shapes'] == 576
                and result['typed_objects'] == 5 and result['pen_paths'] == 3
                and result['brush_strokes'] == 3 and result['undo_redo'] is True
                and result['save_reopen_exact'] is True, 'authoring checks failed')
        counts = Counter(sample['phase'] for sample in result['samples'])
        for phase, count in {'typing': 83, 'brush-drag': 90, 'pan': 40, 'zoom': 60,
                             'pen-commit': 3, 'save': 1}.items():
            require(counts[phase] == count, f'authoring {phase} sample count changed')
        for phase, count in {'typing': 83, 'brush-drag': 85, 'pan': 35, 'zoom': 55}.items():
            require(sum(sample['canvas_changed'] for sample in result['samples']
                        if sample['phase'] == phase) >= count, f'authoring {phase} redraws missing')
        for sample in result['samples']:
            for key in ('ui_ms', 'input_to_ui_ms', 'frame_interval_ms'):
                require(math.isfinite(sample[key]) and sample[key] >= 0, 'invalid authoring timing')
        native_size(directory / 'native-editor.png')
        require(png_size(directory / 'infographic.png') == [1200, 900], 'authoring export geometry')
        return {'passed': True, 'canvas_size': result['canvas_size'], 'phase_counts': dict(counts)}
    if case == 'canvas-profile':
        rows = read(log)
        all_rows = [row for row in rows if row['layer'] == 'ALL']
        require(len(rows) == 84 and [row['round'] for row in all_rows] == [0, 1, 2, 3],
                'incomplete CPU profile')
        require(all(math.isfinite(row[key]) and row[key] >= 0
                    for row in rows for key in ('render_ms', 'convert_ms')), 'invalid CPU timing')
        return {'passed': True, 'render_size': [1456, 900], 'full_scene_samples': 4}
    if case == 'package-reopen':
        native_size(directory / 'package-reopened.png')
        return {'passed': True, 'screenshot_size': [1440, 900]}
    if case.endswith('-native'):
        result = read(directory / 'native-result.json')
        expected = ['cancel_preserves_document', 'undo_redo', 'save_reopen']
        expected += (['pixels_preserved', 'one_undo_step'] if case.startswith('background') else
                     ['cancel_export_no_partial', 'upscale_first', 'upscale_cutout', 'one_undo_step_each',
                      'document_export_4x', 'photo_copy_2x_persisted_and_opened', 'photo_custom_2_5x'])
        require(result['renderer'] == 'native WGPU' and all(result.get(key) is True for key in expected),
                'native AI assertions failed')
        require(result['screenshots'] and result['frames'] > 0, 'native AI evidence incomplete')
        for name in result['screenshots']:
            native_size(directory / (name if str(name).endswith('.png') else f'{name}.png'))
        return {'passed': True, 'checks': expected, 'screenshot_size': [1440, 900]}
    upscale = case.startswith('upscale')
    result = read(directory / ('output.json' if upscale else 'result.json'))
    require(result['runtime'] == 'ONNX Runtime 1.28 CPU'
            and result['sha256'] == MODELS['upscale' if upscale else 'background'], 'model/runtime changed')
    width = int(case.split('-')[1]); height = 212 if width == 320 else 678
    if upscale:
        factor = int(case.rsplit('-', 1)[1][:-1])
        require(result['source'] == [width, height] and result['factor'] == factor
                and result['output'] == [width * factor, height * factor], 'upscale geometry changed')
        require(png_size(directory / 'output.png') == result['output'], 'upscale output missing')
    else:
        require([result['width'], result['height']] == [width, height], 'removal geometry changed')
        for variant in ('global', 'global-guided', 'tiled-guided'):
            for kind in ('mask', 'cutout'):
                require(png_size(directory / f'{variant}-{kind}.png') == [width, height], 'removal output missing')
    return {'passed': True, 'model_sha256': result['sha256'], 'runtime': result['runtime']}


def arguments(case, side, inputs, directory, output):
    if case == 'authoring':
        return 'authoring_qa', [inputs / 'infographic-seed.oma', inputs / 'type-tasks.json', directory]
    document = output / 'runs' / side / 'authoring-2' / 'infographic.oma'
    if case == 'canvas-profile':
        return 'canvas_profile', [document]
    if case == 'package-reopen':
        return 'omadesign', ['--shot-file', document, '--size', '1440x900', '--out', directory / 'package-reopened.png']
    if case.endswith('-native'):
        return case.removesuffix('-native'), [inputs / 'cat-320.png', directory, '--native']
    width = case.split('-')[1]
    if case.startswith('upscale'):
        return 'upscale_qa', [inputs / f'cat-{width}.png', directory / 'output.png', case.rsplit('-', 1)[1][:-1]]
    return 'background_removal_qa', [inputs / f'cat-{width}.png', directory]


def run_one(args, side, case, repetition, env, manifest):
    name = f'{case}-{repetition}'
    directory = args.output / 'runs' / side / name
    directory.mkdir(parents=True)
    native = case in ('authoring', 'package-reopen') or case.endswith('-native')
    if native:
        require(display_state(env) == manifest['display'], 'private display changed between runs')
    binary, parameters = arguments(case, side, args.inputs, directory, args.output)
    executable = getattr(args, f'{side}_bin') / binary
    local_env = env.copy()
    for key, folder in [('XDG_CONFIG_HOME', 'config'), ('XDG_DATA_HOME', 'data'),
                        ('XDG_CACHE_HOME', 'cache'), ('XDG_STATE_HOME', 'state')]:
        path = args.output / 'profiles' / side / name / folder
        path.mkdir(parents=True)
        local_env[key] = str(path)
    local_env['OMADESIGN_ORT_LIBRARY'] = str(args.runtime / 'libonnxruntime.so.1.28.0')
    argv = ['unshare', '-Urn', str(executable), *map(str, parameters)]
    receipt = {'side': side, 'case': case, 'repetition': repetition, 'command': argv,
               'started_utc': utc(), 'before': state(), 'errors': [], 'timed_out': False,
               'network': 'disabled with unshare -Urn', 'gpu_handles': [],
               'limits': 'default user-session limits; no per-test CPU or memory ceiling'}
    process = None; usage = None; interrupted = None; started = time.monotonic(); next_gpu = started
    log_path = directory / 'workload.log'
    print('START', side, name, flush=True)
    with log_path.open('w') as log:
        try:
            process = subprocess.Popen(argv, env=local_env, stdin=subprocess.DEVNULL,
                                       stdout=log, stderr=subprocess.STDOUT, start_new_session=True)
            while True:
                pid, status, usage = os.wait4(process.pid, os.WNOHANG)
                if pid:
                    process.returncode = os.waitstatus_to_exitcode(status); break
                now = time.monotonic()
                if native and not receipt['gpu_handles'] and now >= next_gpu:
                    receipt['gpu_handles'] = gpu_handles(process.pid); next_gpu = now + .5
                if now - started > args.timeout:
                    receipt['timed_out'] = True; raise TimeoutError('workload exceeded deadline')
                time.sleep(.1)
        except BaseException as error:
            interrupted = error; receipt['errors'].append(str(error) or type(error).__name__)
        finally:
            if process is not None and process.returncode is None:
                try: os.killpg(process.pid, signal.SIGTERM)
                except ProcessLookupError: pass
                deadline = time.monotonic() + 10
                while True:
                    pid, status, usage = os.wait4(process.pid, os.WNOHANG)
                    if pid: process.returncode = os.waitstatus_to_exitcode(status); break
                    if time.monotonic() > deadline:
                        try: os.killpg(process.pid, signal.SIGKILL)
                        except ProcessLookupError: pass
                    time.sleep(.1)
            receipt.update(finished_utc=utc(), wall_seconds=time.monotonic() - started,
                           exit_code=process.returncode if process else None, after=state())
            if usage is not None:
                receipt['resources'] = {key: getattr(usage, key) for key in
                    ('ru_utime', 'ru_stime', 'ru_maxrss', 'ru_majflt', 'ru_minflt', 'ru_nvcsw', 'ru_nivcsw')}
    try:
        require(receipt['exit_code'] == 0 and not receipt['timed_out'] and not receipt['errors'], 'process failed')
        if native:
            require(any(item.get('drm-driver') == args.expected_drm_driver for item in receipt['gpu_handles']),
                    'native process lacked the expected hardware DRM handle')
        receipt['validation'] = validate(case, directory, log_path,
                                         [1020, 770] if side == 'baseline' else [1014.71875, 771])
    except (ValueError, KeyError, OSError, TypeError) as error:
        receipt['errors'].append(str(error))
    receipt['passed'] = not receipt['errors']
    write(directory / 'receipt.json', receipt)
    manifest['runs'].append({'side': side, 'case': case, 'repetition': repetition,
                             'receipt': str((directory / 'receipt.json').relative_to(args.output)),
                             'passed': receipt['passed']})
    write(args.output / 'manifest.json', manifest)
    print('DONE', side, name, receipt['passed'], round(receipt['wall_seconds'], 3), flush=True)
    if interrupted is not None: raise interrupted
    require(receipt['passed'], f'{side}/{name} failed; retained receipt describes the failure')


def main():
    global SWAYMSG
    parser = argparse.ArgumentParser(description=__doc__)
    for option in ('baseline-bin', 'candidate-bin', 'inputs', 'runtime', 'baseline-manifest',
                   'candidate-build', 'display-env', 'output'):
        parser.add_argument('--' + option, type=Path, required=True)
    parser.add_argument('--swaymsg', type=Path, default=Path('/usr/bin/swaymsg'))
    parser.add_argument('--candidate-revision', required=True)
    parser.add_argument('--expected-drm-driver', choices=('asahi', 'amdgpu', 'i915', 'xe'), required=True)
    parser.add_argument('--ai-rounds', type=int, default=2)
    parser.add_argument('--timeout', type=float, default=1200)
    args = parser.parse_args()
    require(args.ai_rounds >= 2 and math.isfinite(args.timeout) and args.timeout > 0, 'invalid rounds/timeout')
    require(re.fullmatch('[0-9a-f]{40}', args.candidate_revision), 'candidate revision must be full SHA')
    for key in ('baseline_bin', 'candidate_bin', 'inputs', 'runtime', 'baseline_manifest',
                'candidate_build', 'display_env', 'output', 'swaymsg'):
        setattr(args, key, getattr(args, key).resolve())
    SWAYMSG = str(args.swaymsg)
    require(not args.output.exists(), 'output must be new; retained measurements are never overwritten')
    display = read(args.display_env)
    require(set(display) <= DISPLAY_KEYS and {'SWAYSOCK', 'WAYLAND_DISPLAY', 'XDG_RUNTIME_DIR'} <= set(display),
            'display environment must identify an isolated Sway display')
    private_runtime = Path(display['XDG_RUNTIME_DIR'])
    require(private_runtime == Path('/run/user') / str(os.getuid()) / 'omadesign-fleet-0.6.1'
            and private_runtime.stat().st_uid == os.getuid()
            and stat.S_IMODE(private_runtime.stat().st_mode) == 0o700
            and Path(display['SWAYSOCK']).parent == private_runtime,
            'display must use the task-owned private runtime directory')
    env = os.environ.copy(); env.pop('DISPLAY', None); env.update(display); env['WINIT_UNIX_BACKEND'] = 'wayland'
    # Do not inherit a user's font override or renderer/inference tuning.
    require(not any(key in env for key in ('OMADESIGN_FONT', 'WGPU_BACKEND', 'WGPU_ADAPTER_NAME',
                'LIBGL_ALWAYS_SOFTWARE', 'MESA_LOADER_DRIVER_OVERRIDE', 'ORT_NUM_THREADS', 'OMP_NUM_THREADS',
                'LD_LIBRARY_PATH', 'LD_PRELOAD')),
            'unexpected rendering/font/inference environment override')
    files = {'baseline': inventory(args.baseline_bin), 'candidate': inventory(args.candidate_bin),
             'inputs': inventory(args.inputs), 'runtime': inventory(args.runtime)}
    expected = read(args.baseline_manifest)['artifacts']
    for label, prefix in [('baseline', 'bin'), ('inputs', 'inputs'), ('runtime', 'lib')]:
        require(files[label] == {key.removeprefix(prefix + '/'): value for key, value in expected.items()
                                if key.startswith(prefix + '/')}, f'original {label} artifact drift')
    for side in ('baseline', 'candidate'):
        require(set(files[side]) == set(BINS), f'{side} must contain exactly the five suite executables')
        require(all(os.access(getattr(args, f'{side}_bin') / name, os.X_OK) for name in BINS), 'nonexecutable binary')
    build = read(args.candidate_build)
    architecture = platform.machine()
    require(build['source_commit'] == args.candidate_revision, 'candidate build source mismatch')
    for label, prefix in [('candidate', 'bin'), ('runtime', 'lib')]:
        require(files[label] == {key.removeprefix(f'{architecture}/{prefix}/'): value
                                for key, value in build['artifacts'].items()
                                if key.startswith(f'{architecture}/{prefix}/')},
                f'{label} artifacts do not match the preserved build receipt')
    needed = {'check-all-targets', 'library-tests', architecture + '-portable-release'}
    if architecture == 'aarch64': needed.add('native-qa-release')
    require(needed <= {step['name'] for step in build['steps'] if step['exit_code'] == 0},
            'candidate build/validation steps incomplete for this architecture')
    require(command(['unshare', '-Urn', 'true'])['exit_code'] == 0, 'network namespace unavailable')
    versions = {side: command(['unshare', '-Urn', str(getattr(args, f'{side}_bin') / 'omadesign'),
                               '--version'], env) for side in ('baseline', 'candidate')}
    require(all(value['exit_code'] == 0 and value['stdout'] for value in versions.values()),
            'application version preflight failed')
    manifest = {'schema_version': 1, 'started_utc': utc(), 'host': platform.node(),
                'architecture': platform.machine(), 'kernel': platform.release(),
                'source_revisions': {'baseline': BASELINE, 'candidate': args.candidate_revision},
                'baseline_scope': 'Original issue 158 application baseline; not the shipped v0.6.0 tag.',
                'harness_difference': 'Candidate authoring establishes pointer hover/focus and locks the same 1440x900 outer window. Measured action counts unchanged; canvas baseline 1020x770, candidate 1014.71875x771. Current inspector content expands its default width; candidate canvas is 5.28125px narrower and 1px taller on all three hosts.',
                'historical_scope': 'New same-host paired measurements on private GPU Sway; original user-desktop table retained separately.',
                'artifacts': files, 'display': display_state(env), 'state': state(),
                'power_profile': command(['powerprofilesctl', 'get']),
                'graphics_packages': command(['pacman', '-Q', 'mesa', 'vulkan-radeon', 'vulkan-intel', 'vulkan-asahi']),
                'runner': identity(Path(__file__)), 'expected_drm_driver': args.expected_drm_driver,
                'baseline_manifest': identity(args.baseline_manifest),
                'candidate_build': identity(args.candidate_build), 'swaymsg': identity(args.swaymsg),
                'application_versions': versions,
                'ai_rounds': args.ai_rounds, 'timeout_seconds': args.timeout, 'runs': [], 'errors': [],
                'timing_scope': 'UI CPU wall time and UI-completion intervals, not presented FPS. Process wall time uses 100ms polling. Native-AI p95 is the unchanged harness statistic; raw per-frame AI samples are unavailable.'}
    args.output.mkdir(parents=True)
    (args.output / 'candidate-build.json').write_bytes(args.candidate_build.read_bytes())
    (args.output / 'baseline-manifest.json').write_bytes(args.baseline_manifest.read_bytes())
    write(args.output / 'manifest.json', manifest)
    pair = 0
    try:
        for case in CASES:
            rounds = 2 if case == 'authoring' else 1 if case in ('canvas-profile', 'package-reopen') else args.ai_rounds
            for repetition in range(1, rounds + 1):
                for side in (('baseline', 'candidate') if pair % 2 == 0 else ('candidate', 'baseline')):
                    run_one(args, side, case, repetition, env, manifest)
                pair += 1
        for label, path in [('baseline', args.baseline_bin), ('candidate', args.candidate_bin),
                            ('inputs', args.inputs), ('runtime', args.runtime)]:
            require(inventory(path) == files[label], f'{label} artifacts changed during suite')
    except BaseException as error:
        manifest['errors'].append(str(error) or type(error).__name__)
        raise
    finally:
        manifest['finished_utc'] = utc()
        manifest['success'] = not manifest['errors'] and len(manifest['runs']) == 2 * (4 + 7 * args.ai_rounds)
        write(args.output / 'manifest.json', manifest)


if __name__ == '__main__':
    main()
