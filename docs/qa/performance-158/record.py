#!/usr/bin/env python3
"""Record separate native QA proof; this run is excluded from all benchmarks.

Run only after timing, in the externally prepared 1600x1000 Wayland session
with a blank background and the 1440x900 QA window centered at (80, 50).
The inherited environment selects that session. This helper neither opens a
browser nor changes the desktop, and captures only the QA window rectangle.
"""
import argparse
import hashlib
import json
import os
from pathlib import Path
import shutil
import signal
import subprocess
import time


def artifact(path):
    digest = hashlib.sha256()
    with path.open('rb') as stream:
        for chunk in iter(lambda: stream.read(1024 * 1024), b''):
            digest.update(chunk)
    return {'bytes': path.stat().st_size, 'sha256': digest.hexdigest()}


def stop(process, requested_signal, timeout):
    if process is None:
        return None
    if process.poll() is None:
        try:
            os.killpg(process.pid, requested_signal)
        except ProcessLookupError:
            pass
        try:
            process.wait(timeout=timeout)
        except subprocess.TimeoutExpired:
            try:
                os.killpg(process.pid, signal.SIGKILL)
            except ProcessLookupError:
                pass
            process.wait()
    return process.returncode


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--bin-dir', type=Path, required=True)
    parser.add_argument('--output', type=Path, required=True)
    parser.add_argument('--scenario', choices=('complex-middle', 'authoring'),
                        default='complex-middle')
    parser.add_argument('--seed', type=Path, help='editable .oma input for authoring')
    parser.add_argument('--tasks', type=Path, help='typed text tasks JSON for authoring')
    parser.add_argument('--timeout', type=float, default=180.0,
                        help='external native workload wall-time limit in seconds')
    args = parser.parse_args()
    executable = 'authoring_qa' if args.scenario == 'authoring' else 'interaction_qa'
    binary = (args.bin_dir / executable).resolve()
    output = args.output.resolve()
    if not binary.is_file() or not os.access(binary, os.X_OK):
        parser.error(f'no executable {executable} at {binary}')
    inputs = {}
    typed_objects = None
    if args.scenario == 'authoring':
        for name in ('seed', 'tasks'):
            path = getattr(args, name)
            if path is None or not path.is_file():
                parser.error(f'--{name} must name an existing file for authoring')
            path = path.resolve()
            setattr(args, name, path)
            inputs[name] = {'path': str(path), **artifact(path)}
        try:
            tasks = json.loads(args.tasks.read_text())
            if not isinstance(tasks, list):
                raise ValueError('expected a JSON array of text tasks')
            typed_objects = len(tasks)
        except (OSError, ValueError) as error:
            parser.error(f'invalid --tasks input: {error}')
    elif args.seed is not None or args.tasks is not None:
        parser.error('--seed and --tasks apply only to --scenario authoring')
    if not os.environ.get('WAYLAND_DISPLAY'):
        parser.error('source the prepared Wayland session environment first')
    for tool in ('wf-recorder', 'ffprobe', 'unshare'):
        if not shutil.which(tool):
            parser.error(f'{tool} is required')
    if args.timeout <= 0:
        parser.error('--timeout must be positive')
    if output.exists() and any(output.iterdir()):
        parser.error('use a new or empty output directory to preserve prior evidence')
    output.mkdir(parents=True, exist_ok=True)
    if shutil.disk_usage(output).free < 512 * 1024 * 1024:
        parser.error('output filesystem needs at least 512 MiB free; use disk-backed storage')

    video = output / f'{args.scenario}-native.mp4'
    geometry = '80,50 1440x900'
    recorder_command = [
        'wf-recorder', '--no-dmabuf', '--no-damage', '-g', geometry,
        '-c', 'libx264', '-x', 'yuv420p', '-r', '60',
        '-p', 'crf=20', '-p', 'preset=ultrafast', '-p', 'threads=2',
        '-f', str(video),
    ]
    workload_command = ['unshare', '-Urn', str(binary)]
    if args.scenario == 'authoring':
        workload_command += [str(args.seed), str(args.tasks), str(output / 'workload')]
        required = ['workload/authoring-result.json', 'workload/infographic.oma',
                    'workload/infographic.png', 'workload/native-editor.png']
    else:
        workload_command += [str(output / 'workload'), args.scenario]
        required = ['workload/interaction-result.json', 'workload/interaction.oma',
                    'workload/native-editor.png']
    receipt = {
        'purpose': 'Separate recorded native behavior proof, not a timing run',
        'excluded_from_benchmark': True,
        'scenario': args.scenario,
        'started_utc': time.strftime('%Y-%m-%dT%H:%M:%SZ', time.gmtime()),
        'geometry': geometry,
        'wayland_display': os.environ['WAYLAND_DISPLAY'],
        'recording_command': recorder_command,
        'workload_command': workload_command,
        'executable': {'path': str(binary), **artifact(binary)},
        'inputs': inputs,
    }
    recorder = workload = None
    errors = []
    start = time.monotonic()
    with (output / 'recorder.log').open('w') as recorder_log, \
            (output / 'workload.log').open('w') as workload_log:
        try:
            print('Starting separate recorded QA; its samples are excluded from benchmarks.', flush=True)
            recorder = subprocess.Popen(recorder_command, stdout=recorder_log,
                                        stderr=subprocess.STDOUT, start_new_session=True)
            # Let encoder/screencopy initialization begin before the native app.
            time.sleep(0.3)
            if recorder.poll() is not None:
                raise RuntimeError('wf-recorder exited before the workload; see recorder.log')
            workload = subprocess.Popen(workload_command, stdout=workload_log,
                                        stderr=subprocess.STDOUT, start_new_session=True)
            workload_start = time.monotonic()
            while workload.poll() is None:
                if recorder.poll() is not None:
                    raise RuntimeError('wf-recorder exited during the workload')
                if time.monotonic() - workload_start > args.timeout:
                    raise RuntimeError('native workload exceeded the external timeout')
                time.sleep(0.1)
            if workload.returncode != 0:
                errors.append(f'native workload exited {workload.returncode}; see workload.log')
        except (OSError, RuntimeError, KeyboardInterrupt) as error:
            errors.append(str(error) or type(error).__name__)
        finally:
            receipt['workload_exit_code'] = stop(workload, signal.SIGTERM, 5)
            # SIGINT lets wf-recorder flush the encoder and finalize its MP4.
            receipt['recorder_exit_code'] = stop(recorder, signal.SIGINT, 20)
    receipt['wall_seconds'] = time.monotonic() - start
    if receipt['recorder_exit_code'] != 0:
        errors.append(f"wf-recorder exited {receipt['recorder_exit_code']}; see recorder.log")

    probe_command = ['ffprobe', '-v', 'error', '-show_format', '-show_streams',
                     '-of', 'json', str(video)]
    receipt['probe_command'] = probe_command
    try:
        probe = subprocess.run(probe_command, check=True, capture_output=True,
                               text=True, timeout=30)
        metadata = json.loads(probe.stdout)
        (output / 'ffprobe.json').write_text(json.dumps(metadata, indent=2) + '\n')
        streams = [stream for stream in metadata.get('streams', [])
                   if stream.get('codec_type') == 'video']
        if len(streams) != 1:
            raise ValueError('expected exactly one video stream')
        stream = streams[0]
        expected = {'codec_name': 'h264', 'width': 1440, 'height': 900}
        if any(stream.get(key) != value for key, value in expected.items()) \
                or stream.get('pix_fmt') not in ('yuv420p', 'yuvj420p'):
            raise ValueError(f'unexpected video format: {stream}')
        if float(metadata['format']['duration']) <= 0 or int(stream.get('nb_frames', 0)) < 2:
            raise ValueError('recording contains no usable sequence of frames')
        receipt['video_verified'] = True
        receipt['video_format'] = {key: stream.get(key) for key in (
            'codec_name', 'width', 'height', 'pix_fmt', 'color_range')}
    except (OSError, subprocess.SubprocessError, ValueError, KeyError) as error:
        errors.append(f'video verification failed: {error}')
        receipt['video_verified'] = False

    for name in required:
        if not (output / name).is_file():
            errors.append(f'missing native QA artifact: {name}')
    if (output / required[0]).is_file():
        try:
            result = json.loads((output / required[0]).read_text())
            if args.scenario == 'authoring':
                checks = {key: result.get(key) for key in (
                    'typed_objects', 'pen_paths', 'brush_strokes', 'undo_redo', 'save_reopen_exact')}
                if checks['typed_objects'] != typed_objects or checks['pen_paths'] != 3 \
                        or checks['brush_strokes'] != 3 or checks['undo_redo'] is not True \
                        or checks['save_reopen_exact'] is not True:
                    raise ValueError('native authoring assertions were not all reported successful')
                updated_frames = {
                    phase: sum(sample.get('phase') == phase and sample.get('canvas_changed') is True
                               for sample in result['samples'])
                    for phase in ('typing', 'brush-drag', 'pan', 'zoom')
                }
                if any(updated_frames[phase] < minimum for phase, minimum in (
                        ('typing', 83), ('brush-drag', 85), ('pan', 35), ('zoom', 55))):
                    raise ValueError('native authoring did not continuously update the canvas')
                checks['canvas_updated_frames'] = updated_frames
            else:
                checks = result['checks']
                if result['scenario'] != args.scenario or checks.get('moves_and_undo_redo') != 12 \
                        or not all(checks.get(key) is True for key in (
                            'continuous_object_motion', 'continuous_canvas_updates',
                            'inactive_documents_unchanged', 'tab_selection_and_history', 'save_reopen_exact')):
                    raise ValueError('native interaction assertions were not all reported successful')
            receipt['native_checks'] = checks
        except (ValueError, KeyError) as error:
            errors.append(f'native result verification failed: {error}')
    names = required + [video.name, 'ffprobe.json', 'recorder.log', 'workload.log']
    receipt['artifacts'] = {name: artifact(output / name) for name in names if (output / name).is_file()}
    receipt['errors'] = errors
    receipt['success'] = not errors
    (output / 'recording-receipt.json').write_text(json.dumps(receipt, indent=2) + '\n')
    if errors:
        raise SystemExit('\n'.join(errors))
    print(f'Recorded native QA verified: {video}', flush=True)


if __name__ == '__main__':
    main()
