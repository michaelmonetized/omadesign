#!/usr/bin/env python3
"""Identical, serial native/CPU workloads; run inside each host's user session."""
import hashlib
import json
import os
import pathlib
import platform
import subprocess
import sys
import time

ROOT = pathlib.Path(__file__).resolve().parent
RESULTS = ROOT / 'results'
RESULTS.mkdir(exist_ok=True)


def command(*args):
    try:
        result = subprocess.run(args, capture_output=True, text=True, timeout=15)
        return {'returncode': result.returncode, 'output': result.stdout.strip()}
    except (OSError, subprocess.TimeoutExpired) as error:
        return {'error': str(error)}


def read(path):
    try:
        return pathlib.Path(path).read_text().strip().strip('\x00')
    except OSError:
        return None


def state():
    memory = {}
    for line in pathlib.Path('/proc/meminfo').read_text().splitlines():
        key, value = line.split(':', 1)
        if key in ['MemTotal', 'MemAvailable', 'SwapTotal', 'SwapFree']:
            memory[key + '_KiB'] = int(value.split()[0])
    return {
        'memory': memory,
        'loadavg': read('/proc/loadavg'),
        'pressure': {key: read('/proc/pressure/' + key) for key in ['cpu', 'memory', 'io']},
        'cpu_policies': {
            p.name: {key: read(p / key) for key in ['affected_cpus', 'scaling_governor', 'scaling_cur_freq', 'scaling_min_freq', 'scaling_max_freq']}
            for p in pathlib.Path('/sys/devices/system/cpu/cpufreq').glob('policy*')
        },
    }


def survey():
    spec = {
        'host': platform.node(), 'architecture': platform.machine(), 'kernel': platform.release(),
        'lscpu': command('lscpu', '-J'), 'cpu_topology': command('lscpu', '-p=CPU,CORE,SOCKET,ONLINE'),
        'model': read('/sys/firmware/devicetree/base/model') or read('/sys/class/dmi/id/product_name'),
        'vendor': read('/sys/class/dmi/id/sys_vendor'), 'os_release': read('/etc/os-release'),
        'graphics': [line for line in command('lspci', '-nnk').get('output', '').splitlines() if any(s in line for s in ['VGA compatible', 'Display controller', '3D controller', 'Kernel driver in use: i915', 'Kernel driver in use: amdgpu', 'Kernel driver in use: radeon'])],
        'graphics_packages': command('pacman', '-Q', 'mesa', 'mesa-asahi-edge', 'vulkan-radeon', 'vulkan-intel', 'vulkan-asahi'),
        'power_profile': command('powerprofilesctl', 'get'),
        'state': state(), 'captured_utc': time.strftime('%Y-%m-%dT%H:%M:%SZ', time.gmtime()),
    }
    # No monitor serial numbers, user window titles, MACs, addresses or process arguments.
    monitors = command('hyprctl', '-j', 'monitors')
    if monitors.get('returncode') == 0:
        spec['monitors'] = [{key: m.get(key) for key in ['name', 'width', 'height', 'refreshRate', 'scale', 'transform']} for m in json.loads(monitors['output'])]
    spec['artifacts'] = {}
    for directory in ['bin', 'lib', 'inputs']:
        for p in sorted((ROOT / directory).rglob('*')):
            if p.is_file():
                spec['artifacts'][str(p.relative_to(ROOT))] = {'bytes': p.stat().st_size, 'sha256': hashlib.sha256(p.read_bytes()).hexdigest()}
    (RESULTS / 'host.json').write_text(json.dumps(spec, indent=2) + '\n')


def run(name, binary, arguments):
    destination = RESULTS / name
    env = os.environ.copy()
    for key, folder in [('XDG_CONFIG_HOME', 'config'), ('XDG_DATA_HOME', 'data'), ('XDG_CACHE_HOME', 'cache'), ('XDG_STATE_HOME', 'state')]:
        path = ROOT / 'profiles' / name / folder
        path.mkdir(parents=True, exist_ok=True)
        env[key] = str(path)
    env['OMADESIGN_ORT_LIBRARY'] = str(ROOT / 'lib/libonnxruntime.so.1.28.0')
    before = state()
    start = time.monotonic()
    timed_out = False
    print('START', name, flush=True)
    with (RESULTS / (name + '.log')).open('w') as log:
        process = subprocess.Popen(['unshare', '-Urn', str(ROOT / 'bin' / binary), *map(str, arguments)], env=env, stdout=log, stderr=subprocess.STDOUT)
        while True:
            pid, status, usage = os.wait4(process.pid, os.WNOHANG)
            if pid:
                process.returncode = os.waitstatus_to_exitcode(status)
                break
            if time.monotonic() - start > 1200 and not timed_out:
                process.terminate()
                timed_out = True
            elif time.monotonic() - start > 1210:
                process.kill()
            time.sleep(0.1)
    report = {
        'name': name, 'returncode': process.returncode, 'timed_out': timed_out,
        'wall_seconds': time.monotonic() - start, 'peak_rss_KiB': usage.ru_maxrss,
        'user_seconds': usage.ru_utime, 'system_seconds': usage.ru_stime,
        'major_page_faults': usage.ru_majflt, 'before': before, 'after': state(),
        'network': 'disabled with unshare -Urn', 'limits': 'default user-session limits, no per-test CPU or memory ceiling',
    }
    (RESULTS / (name + '-resources.json')).write_text(json.dumps(report, indent=2) + '\n')
    print('DONE', name, process.returncode, round(report['wall_seconds'], 3), usage.ru_maxrss, flush=True)
    return process.returncode == 0


survey()
if len(sys.argv) > 1 and sys.argv[1] == 'survey':
    raise SystemExit(0)

ok = True
inputs = ROOT / 'inputs'
for repetition in [1, 2]:
    ok &= run(f'authoring-{repetition}', 'authoring_qa', [inputs / 'infographic-seed.oma', inputs / 'type-tasks.json', RESULTS / f'authoring-{repetition}'])
ok &= run('canvas-profile', 'canvas_profile', [RESULTS / 'authoring-2/infographic.oma'])
for width, factor in [(320, 2), (320, 4), (1024, 2)]:
    name = f'upscale-{width}-{factor}x'
    ok &= run(name, 'upscale_qa', [inputs / f'cat-{width}.png', RESULTS / (name + '.png'), factor])
for width in [320, 1024]:
    name = f'background-{width}'
    ok &= run(name, 'background_removal_qa', [inputs / f'cat-{width}.png', RESULTS / name])
for binary in ['background_removal_qa', 'upscale_qa']:
    name = binary + '-native'
    ok &= run(name, binary, [inputs / 'cat-320.png', RESULTS / name, '--native'])
ok &= run('package-reopen', 'omadesign', ['--shot-file', RESULTS / 'authoring-2/infographic.oma', '--size', '1440x900', '--out', RESULTS / 'package-reopened.png'])
(RESULTS / 'complete.json').write_text(json.dumps({'success': bool(ok), 'state': state()}, indent=2) + '\n')
raise SystemExit(0 if ok else 1)
