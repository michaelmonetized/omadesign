#!/usr/bin/env python3
"""Real release archive smoke in a fresh prefix and task-only GPU display.

The supplementary retained QA executables exercise the installed ORT/model
pipeline. They are explicitly distinct from the shipped production executable.
No workload is run until --archive and its expected SHA256 are supplied.
"""
import argparse
import hashlib
import json
import os
from pathlib import Path
import platform
import shutil
import signal
import stat
import subprocess
import tarfile
import time
import traceback


def identity(path):
    data = path.read_bytes()
    return {'bytes': len(data), 'sha256': hashlib.sha256(data).hexdigest()}


def write(path, value):
    with path.open('x') as out:
        json.dump(value, out, indent=2, allow_nan=False)
        out.write('\n')


def require(value, message):
    if not value:
        raise RuntimeError(message)


def gpu_handles(pid):
    found = []
    directory = Path('/proc') / str(pid)
    for fd in (directory / 'fd').glob('*'):
        try:
            info = (directory / 'fdinfo' / fd.name).read_text()
            driver = next((line.split(':', 1)[1].strip() for line in info.splitlines()
                           if line.startswith('drm-driver:')), None)
            target = os.readlink(fd)
            if not target.startswith('/dev/dri/renderD'):
                continue
            mode = fd.stat()
            if not stat.S_ISCHR(mode.st_mode):
                continue
            device = f'{os.major(mode.st_rdev)}:{os.minor(mode.st_rdev)}'
            source = 'fdinfo'
            if not driver:
                driver = (Path('/sys/dev/char') / device / 'device/driver').resolve().name
                source = 'open descriptor sysfs driver'
            item = {'node': target, 'device': device, 'driver': driver, 'source': source}
            if item not in found:
                found.append(item)
        except (OSError, ValueError):
            pass
    return found


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--archive', type=Path, required=True)
    parser.add_argument('--archive-sha256', required=True)
    parser.add_argument('--output', type=Path, required=True)
    parser.add_argument('--version', default='0.6.1')
    parser.add_argument('--expected-drm-driver', choices=('amdgpu', 'i915'), required=True)
    parser.add_argument('--fleet-root', type=Path, default=Path.home() / '.local/state/omadesign/qa/fleet-performance-0.6.1')
    parser.add_argument('--fixtures', type=Path, default=Path.home() / '.local/state/omadesign/qa/fleet-benchmark-20260927/inputs')
    args = parser.parse_args()
    archive, output = args.archive.resolve(), args.output.resolve()
    require(not output.exists(), 'Output must be a fresh task directory')
    require(identity(archive)['sha256'] == args.archive_sha256, 'Archive digest differs')
    require(platform.machine() == 'x86_64', 'This helper expects x86_64 archive QA')
    output.mkdir(parents=True)
    receipt = {'schema_version': 1, 'host': platform.node(), 'architecture': platform.machine(),
               'archive': identity(archive), 'script': identity(Path(__file__)),
               'started_utc': time.strftime('%Y-%m-%dT%H:%M:%SZ', time.gmtime()),
               'steps': [], 'success': False, 'supplemental_helpers_are_shipped_binary': False}
    existing = [Path.home() / '.local/bin/omadesign', Path.home() / '.local/share/applications/omadesign.desktop']
    preserved = {str(path): identity(path) if path.is_file() else None for path in existing}
    receipt['existing_install_before'] = preserved
    sway_process = None
    runtime = None
    sway_env = None
    swaymsg = None

    def run(name, command, env, timeout=90, expect_gpu=False, expected_runtime=None):
        print('START', name, flush=True)
        started = time.monotonic()
        mapped = set()
        devices = []
        executables = set()
        with (output / f'{name}.stdout').open('x') as stdout, (output / f'{name}.stderr').open('x') as stderr:
            process = subprocess.Popen(command, env=env, cwd=output, stdin=subprocess.DEVNULL,
                                       stdout=stdout, stderr=stderr, start_new_session=True)
            timed_out = False
            while process.poll() is None:
                try:
                    executables.add(os.readlink(f'/proc/{process.pid}/exe'))
                    for line in Path(f'/proc/{process.pid}/maps').read_text().splitlines():
                        if 'libonnxruntime' in line and '/' in line:
                            mapped.add('/' + line.split('/', 1)[1])
                except OSError:
                    pass
                if expect_gpu:
                    for item in gpu_handles(process.pid):
                        if item not in devices:
                            devices.append(item)
                if time.monotonic() - started > timeout:
                    timed_out = True
                    os.killpg(process.pid, signal.SIGKILL)
                    break
                time.sleep(0.025)
            code = process.wait()
        step = {'name': name, 'command': list(map(str, command)), 'exit_code': code,
                'timed_out': timed_out, 'wall_seconds': time.monotonic() - started,
                'observed_executables': sorted(executables), 'gpu_handles': devices,
                'observed_ort_mappings': sorted(mapped)}
        receipt['steps'].append(step)
        write(output / f'{name}.receipt.json', step)
        require(code == 0 and not timed_out, f'{name} failed; see retained output')
        if expect_gpu:
            require(any(row['driver'] == args.expected_drm_driver for row in devices), 'Expected hardware DRM handle not observed')
        if expected_runtime:
            require(mapped and {Path(path).resolve() for path in mapped if 'providers_shared' not in path}
                    == {expected_runtime.resolve()}, 'Supplemental helper did not load only the installed ONNX Runtime')
        return (output / f'{name}.stdout').read_text()

    try:
        unpacked = output / 'archive'
        unpacked.mkdir()
        with tarfile.open(archive) as packed:
            require(all(not Path(item.name).is_absolute() and '..' not in Path(item.name).parts
                        for item in packed.getmembers()), 'Archive has unsafe paths')
            packed.extractall(unpacked, filter='data')
        package = unpacked / f'omadesign-{args.version}-x86_64-unknown-linux-gnu'
        require(package.is_dir() and len(list(unpacked.iterdir())) == 1, 'Unexpected archive root')
        receipt['archive_binary'] = identity(package / 'omadesign')
        prefix = output / 'prefix'
        profile = output / 'profile'
        env = os.environ.copy()
        for key in ('DISPLAY', 'WAYLAND_DISPLAY', 'SWAYSOCK', 'LD_LIBRARY_PATH', 'LD_PRELOAD', 'OMADESIGN_ORT_LIBRARY', 'ORT_DYLIB_PATH'):
            env.pop(key, None)
        for variable, child in [('XDG_CONFIG_HOME', 'config'), ('XDG_DATA_HOME', 'data'),
                                ('XDG_CACHE_HOME', 'cache'), ('XDG_STATE_HOME', 'state'), ('TMPDIR', 'tmp')]:
            directory = profile / child
            directory.mkdir(parents=True)
            env[variable] = str(directory)
        env['PYTHONDONTWRITEBYTECODE'] = '1'
        run('install', ['sh', str(package / 'install.sh'), '--prefix', str(prefix)], env)
        binary = prefix / 'bin/omadesign'
        require(identity(binary) == receipt['archive_binary'], 'Installed binary bytes differ')
        receipt['installed_binary'] = identity(binary)
        for library in (package / 'lib').glob('*.so*'):
            require(identity(library) == identity(prefix / 'share/omadesign/lib' / library.name), 'Installed runtime bytes differ')
        receipt['installed_runtime'] = {p.name: identity(p) for p in (prefix / 'share/omadesign/lib').glob('*.so*')}
        inputs = output / 'inputs'
        shutil.copytree(args.fixtures, inputs)
        receipt['fixtures'] = {str(p.relative_to(inputs)): identity(p) for p in inputs.rglob('*') if p.is_file()}
        before_inputs = receipt['fixtures'].copy()
        offline = ['/usr/bin/unshare', '-Urn', str(binary)]
        version = run('version', offline + ['--version'], env).strip()
        require(version == f'omadesign {args.version}', f'Wrong release version: {version}')
        seed = inputs / 'infographic-seed.oma'
        info = json.loads(run('inspect-seed', offline + ['--inspect', str(seed)], env))
        require((info['width'], info['height']) == (1200, 900) and info['layers'], 'Unexpected seed dimensions/layers')
        run('convert-png', offline + ['--convert', str(seed), '--output', str(output / 'converted.png')], env)
        info_png = json.loads(run('inspect-png', offline + ['--inspect', str(output / 'converted.png')], env))
        require((info_png['width'], info_png['height']) == (1200, 900), 'Converted PNG did not decode correctly')
        run('convert-oma', offline + ['--convert', str(seed), '--output', str(output / 'roundtrip.oma')], env)
        roundtrip = json.loads(run('inspect-roundtrip', offline + ['--inspect', str(output / 'roundtrip.oma')], env))
        require(roundtrip == info, 'Native save/reopen inspect data differs')
        sway_prefix = args.fleet_root / 'sway-prefix'
        sway = sway_prefix / 'usr/bin/sway'
        swaymsg = sway.with_name('swaymsg')
        require(identity(sway)['sha256'] == '7ba78b7531a3b3cb1852b3b1e464b4458cce1749c6c0fe3c6fb4b16ece9d5b9c', 'Private Sway differs from verified package')
        runtime = Path('/run/user') / str(os.getuid()) / f'oma061-smoke-{os.getpid()}'
        runtime.mkdir(mode=0o700)
        config = output / 'sway-headless.conf'
        shutil.copyfile(args.fleet_root / 'sway-headless.conf', config)
        require(identity(config)['sha256'] == 'a0bf3be5168104ef713d34b913a09c01c0ed16338dc83706550f31d82f95ddc0', 'Private display config changed')
        sway_env = env.copy()
        sway_env.update(XDG_RUNTIME_DIR=str(runtime), WLR_BACKENDS='headless', WLR_RENDERER='gles2',
                        WLR_RENDER_DRM_DEVICE='/dev/dri/renderD128', LD_LIBRARY_PATH=str(sway_prefix / 'usr/lib'))
        with (output / 'sway-headless.log').open('x') as log:
            sway_process = subprocess.Popen([str(sway), '-d', '-c', str(config)], env=sway_env,
                                            stdin=subprocess.DEVNULL, stdout=log, stderr=subprocess.STDOUT,
                                            start_new_session=True)
        for _ in range(100):
            require(sway_process.poll() is None, 'Private Sway exited')
            sockets = list(runtime.glob('sway-ipc.*.sock'))
            displays = [p for p in runtime.glob('wayland-*') if not p.name.endswith('.lock')]
            if sockets and displays:
                break
            time.sleep(0.1)
        else:
            raise RuntimeError('Private display did not become ready')
        env.update(XDG_RUNTIME_DIR=str(runtime), WAYLAND_DISPLAY=displays[0].name,
                   SWAYSOCK=str(sockets[0]), WINIT_UNIX_BACKEND='wayland')
        outputs = json.loads(subprocess.check_output([str(swaymsg), '-s', str(sockets[0]), '-r', '-t', 'get_outputs'], env=sway_env))
        require(any(p['active'] and p['current_mode']['width'] == 1600 and p['current_mode']['height'] == 1000 for p in outputs), 'Wrong headless display size')
        renderer = (output / 'sway-headless.log').read_text()
        require('GL renderer:' in renderer and not any(p in renderer.lower() for p in ('llvmpipe', 'softpipe', 'pixman')), 'Hardware GLES renderer not proven')
        receipt['display'] = {'pid': sway_process.pid, 'sway': identity(sway), 'config': identity(config),
                              'outputs': outputs, 'env': {k: env[k] for k in ('XDG_RUNTIME_DIR', 'WAYLAND_DISPLAY', 'SWAYSOCK')},
                              'renderer_lines': [line for line in renderer.splitlines() if 'GL renderer:' in line or 'GL vendor:' in line]}
        run('shot-file', offline + ['--shot-file', str(output / 'roundtrip.oma'), '--size', '1440x900', '--out', str(output / 'native-editor.png')], env, timeout=120, expect_gpu=True)
        screenshot = json.loads(run('inspect-screenshot', offline + ['--inspect', str(output / 'native-editor.png')], env))
        require((screenshot['width'], screenshot['height']) == (1440, 900), 'Wrong native screenshot size')
        installed_runtime = prefix / 'share/omadesign/lib/libonnxruntime.so.1'
        receipt['supplemental_helper_scope'] = 'Retained benchmark helper source8ee01b4; real-model inference verifies exact installed runtime auto-discovery, not current shipped UI code. ORT overrides and LD_LIBRARY_PATH are unset.'
        helpers = {}
        expected_helpers = {
            'background_removal_qa': 'eacadccece83c2f8c44433aa17346064d2f907da727f6d8c553fe579ef71d75d',
            'upscale_qa': '2baf23c724f685580e165e21073d36b20aec9e7ab10b8d230a724acbcc65f176',
        }
        for name in ('background_removal_qa', 'upscale_qa'):
            original = args.fleet_root / 'candidate/x86_64/bin' / name
            require(identity(original)['sha256'] == expected_helpers[name], 'Supplemental helper differs from pinned benchmark executable')
            helper = prefix / 'bin' / f'{name}-supplemental'
            shutil.copy2(original, helper)
            helpers[name] = identity(helper)
        receipt['supplemental_helpers'] = helpers
        run('runtime-background', ['/usr/bin/unshare', '-Urn', str(prefix / 'bin/background_removal_qa-supplemental'), str(inputs / 'cat-320.png'), str(output / 'background')], env, timeout=120, expected_runtime=installed_runtime)
        run('runtime-upscale', ['/usr/bin/unshare', '-Urn', str(prefix / 'bin/upscale_qa-supplemental'), str(inputs / 'cat-320.png'), str(output / 'upscaled.png'), '2'], env, timeout=120, expected_runtime=installed_runtime)
        background = json.loads((output / 'background/result.json').read_text())
        upscale = json.loads((output / 'upscaled.json').read_text())
        require(background['sha256'] == '309c8469258dda742793dce0ebea8e6dd393174f89934733ecc8b14c76f4ddd8' and background['inference_calls'] > 0, 'Unexpected background model or no inference')
        require(upscale['sha256'] == '74b0bf4bdad2868e256ef722980586b7ff1a8b2c9ba0b8e526498cd28acf5b80' and upscale['output'] == [640, 424] and upscale['tiles'] > 0, 'Unexpected upscale model/output')
        receipt['inference_results'] = {'background': background, 'upscale': upscale}
        for name, path, size in [('inspect-background', output / 'background/tiled-guided-cutout.png', (320, 212)), ('inspect-upscaled', output / 'upscaled.png', (640, 424))]:
            result = json.loads(run(name, offline + ['--inspect', str(path)], env))
            require((result['width'], result['height']) == size, 'Inference output did not decode')
        require(before_inputs == {str(p.relative_to(inputs)): identity(p) for p in inputs.rglob('*') if p.is_file()}, 'Input copies changed')
        require(preserved == {str(p): identity(p) if p.is_file() else None for p in existing}, 'Existing installation changed')
        receipt['outputs'] = {str(p.relative_to(output)): identity(p) for p in [output / 'converted.png', output / 'roundtrip.oma', output / 'native-editor.png', output / 'upscaled.png', *(output / 'background').glob('*.png')]}
        receipt['existing_install_unchanged'] = True
        receipt['success'] = True
    except Exception as error:
        receipt['error'] = str(error)
        receipt['traceback'] = traceback.format_exc()
        raise
    finally:
        if sway_process is not None and sway_process.poll() is None:
            socket = next(runtime.glob('sway-ipc.*.sock'), None)
            try:
                if socket:
                    subprocess.run([str(swaymsg), '-s', str(socket), 'exit'], env=sway_env, timeout=5, check=True, capture_output=True)
                sway_process.wait(timeout=8)
            except Exception as cleanup_error:
                receipt['compositor_cleanup_fallback'] = str(cleanup_error)
                try:
                    os.killpg(sway_process.pid, signal.SIGTERM)
                    sway_process.wait(timeout=5)
                except (ProcessLookupError, subprocess.TimeoutExpired):
                    if sway_process.poll() is None:
                        os.killpg(sway_process.pid, signal.SIGKILL)
                        sway_process.wait()
            receipt['private_compositor_exit'] = sway_process.returncode
        receipt['finished_utc'] = time.strftime('%Y-%m-%dT%H:%M:%SZ', time.gmtime())
        write(output / 'receipt.json', receipt)
        print(json.dumps({'success': receipt['success'], 'output': str(output), 'steps': len(receipt['steps'])}), flush=True)


if __name__ == '__main__':
    main()
