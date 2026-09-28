#!/usr/bin/env python3
"""Run serial native issue-158 workloads with isolated profiles and raw resources."""
import argparse
import hashlib
import json
import os
from pathlib import Path
import platform
import subprocess
import time


def state():
    memory = {}
    for line in Path('/proc/meminfo').read_text().splitlines():
        key, value = line.split(':', 1)
        if key in ('MemTotal', 'MemAvailable', 'SwapTotal', 'SwapFree'):
            memory[key + '_KiB'] = int(value.split()[0])
    return {'memory': memory, 'loadavg': Path('/proc/loadavg').read_text().strip(),
            'pressure': {key: Path('/proc/pressure/' + key).read_text().strip()
                         for key in ('cpu', 'memory', 'io')},
            'cpu_policies': {policy.name: {key: (policy / key).read_text().strip()
                for key in ('scaling_governor', 'scaling_cur_freq', 'scaling_max_freq')
                if (policy / key).exists()}
                for policy in Path('/sys/devices/system/cpu/cpufreq').glob('policy*')}}


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--bin-dir', type=Path, required=True)
    parser.add_argument('--output', type=Path, required=True)
    parser.add_argument('--seed', type=Path, required=True)
    parser.add_argument('--tasks', type=Path, required=True)
    parser.add_argument('--label', required=True)
    parser.add_argument('--rounds', type=int, default=2)
    parser.add_argument('--authoring-bin', default='authoring_qa')
    parser.add_argument('--scenarios', nargs='+', default=['simple', 'complex', 'complex-middle', 'complex-same-layer', 'many-tabs'])
    args = parser.parse_args()
    args.output.mkdir(parents=True, exist_ok=True)
    manifest = {'label': args.label, 'host': platform.node(), 'architecture': platform.machine(),
                'kernel': platform.release(), 'started_utc': time.strftime('%Y-%m-%dT%H:%M:%SZ', time.gmtime()),
                'window': [1440, 900], 'runs': [],
                'artifacts': {str(path): {'bytes': path.stat().st_size,
                    'sha256': hashlib.sha256(path.read_bytes()).hexdigest()}
                    for path in [args.bin_dir / 'interaction_qa', args.bin_dir / args.authoring_bin,
                                 args.seed, args.tasks]}}
    (args.output / 'manifest.json').write_text(json.dumps(manifest, indent=2) + '\n')
    jobs = []
    for repeat in range(1, args.rounds + 1):
        for scenario in args.scenarios:
            name = f'{scenario}-{repeat}'
            jobs.append((name, 'interaction_qa', [str(args.output / name), scenario]))
        name = f'authoring-{repeat}'
        jobs.append((name, args.authoring_bin, [str(args.seed), str(args.tasks), str(args.output / name)]))
    for name, binary, arguments in jobs:
        before = state()
        start = time.monotonic()
        print('START', args.label, name, flush=True)
        with (args.output / (name + '.log')).open('w') as log:
            process = subprocess.Popen(['unshare', '-Urn', str(args.bin_dir / binary), *arguments], stdout=log, stderr=subprocess.STDOUT)
            _, status, usage = os.wait4(process.pid, 0)
            process.returncode = os.waitstatus_to_exitcode(status)
        result = {'name': name, 'exit_code': process.returncode, 'wall_seconds': time.monotonic() - start,
                  'peak_rss_KiB': usage.ru_maxrss, 'major_page_faults': usage.ru_majflt,
                  'user_seconds': usage.ru_utime, 'system_seconds': usage.ru_stime,
                  'before': before, 'after': state(), 'network': 'disabled by unshare -Urn'}
        (args.output / (name + '-resources.json')).write_text(json.dumps(result, indent=2) + '\n')
        manifest['runs'].append(result)
        (args.output / 'manifest.json').write_text(json.dumps(manifest, indent=2) + '\n')
        print('DONE', name, process.returncode, round(result['wall_seconds'], 3), flush=True)
        if process.returncode:
            print((args.output / (name + '.log')).read_text()[-5000:], flush=True)
            raise SystemExit(process.returncode)


if __name__ == '__main__':
    main()
