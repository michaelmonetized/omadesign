#!/usr/bin/env python3
"""Measure real app pipelines serially on the target, with isolated QA files."""
import json, os, pathlib, subprocess, sys, time

root = pathlib.Path(__file__).resolve().parent
results = root / 'results'
results.mkdir(exist_ok=True)
env = os.environ.copy()
for key, name in [('XDG_DATA_HOME','data'), ('XDG_CONFIG_HOME','config'),
                  ('XDG_CACHE_HOME','cache'), ('XDG_STATE_HOME','state')]:
    p = root / 'profile' / name
    p.mkdir(parents=True, exist_ok=True)
    env[key] = str(p)
env['OMADESIGN_ORT_LIBRARY'] = str(pathlib.Path.home() / '.local/share/omadesign/lib/libonnxruntime.so.1')

def pressure():
    return {n: pathlib.Path('/proc/pressure/' + n).read_text().strip() for n in ['memory', 'cpu', 'io']}

def meminfo():
    values = {}
    for line in pathlib.Path('/proc/meminfo').read_text().splitlines():
        key, value = line.split(':', 1)
        if key in ['MemTotal', 'MemAvailable', 'SwapTotal', 'SwapFree']:
            values[key + '_KiB'] = int(value.strip().split()[0])
    return values

def run(name, executable, args, cpus=None):
    dest = results / name
    log = results / (name + '.log')
    before = {'memory': meminfo(), 'pressure': pressure()}
    cmd = ['/usr/bin/unshare', '-Urn', str(root / 'bin' / executable), *map(str, args)]
    if cpus is not None:
        cmd = ['/usr/bin/taskset', '-c', cpus, *cmd]
    start = time.monotonic()
    print('START', name, flush=True)
    with log.open('w') as output:
        process = subprocess.Popen(cmd, stdout=output, stderr=subprocess.STDOUT, env=env)
        while True:
            pid, status, usage = os.wait4(process.pid, os.WNOHANG)
            if pid:
                process.returncode = os.waitstatus_to_exitcode(status)
                break
            if time.monotonic() - start > 1800:
                process.terminate()
            time.sleep(0.2)
    report = {'name':name, 'returncode':process.returncode, 'wall_seconds':time.monotonic()-start,
              'peak_rss_KiB':usage.ru_maxrss, 'user_seconds':usage.ru_utime,
              'system_seconds':usage.ru_stime, 'major_page_faults':usage.ru_majflt,
              'before':before, 'after':{'memory':meminfo(), 'pressure':pressure()},
              'network':'disabled with unshare -Urn', 'provider':'CPU', 'log':log.name}
    (results / (name + '-resources.json')).write_text(json.dumps(report, indent=2)+'\n')
    print('DONE', json.dumps({k:report[k] for k in ['name','returncode','wall_seconds','peak_rss_KiB','major_page_faults']}), flush=True)
    if process.returncode:
        print(log.read_text()[-5000:], flush=True)
        raise SystemExit(process.returncode)

def up(width, factor):
    name=f'upscale-{width}-{factor}x'
    run(name, 'upscale_qa', [root/'inputs'/f'cat-{width}.png', results/(name+'.png'), factor])

mode = sys.argv[1] if len(sys.argv)>1 else 'small'
if mode == 'small':
    for factor in [2,4,5]: up(320,factor)
    for width in [320,1024]:
        name=f'background-{width}'
        run(name, 'background_removal_qa', [root/'inputs'/f'cat-{width}.png', results/name])
    for factor in [2,4]: up(1024,factor)
elif mode == 'native':
    for tool in ['background_removal_qa','upscale_qa']:
        run(tool+'-native', tool, [root/'inputs'/'cat-320.png', results/(tool+'-native'), '--native'])
elif mode == 'large':
    up(2048,2)
elif mode == 'stress':
    up(6000,2)
elif mode == 'background':
    for width in [320,1024]:
        name=f'background-{width}'
        run(name, 'background_removal_qa', [root/'inputs'/f'cat-{width}.png', results/name])
elif mode == 'single':
    run('single-thread-upscale-320-2x', 'upscale_qa', [root/'inputs'/'cat-320.png',results/'single-thread-upscale-320-2x.png',2], '3')
elif mode == 'two':
    run('two-thread-upscale-320-2x', 'upscale_qa', [root/'inputs'/'cat-320.png',results/'two-thread-upscale-320-2x.png',2], '2,3')
elif mode == 'typical':
    up(1024,2)
elif mode == 'baseline':
    run('normal-priority-upscale-320-2x', 'upscale_qa', [root/'inputs'/'cat-320.png',results/'normal-priority-upscale-320-2x.png',2])
elif mode == 'installed':
    run('installed-reopen', str(pathlib.Path.home()/'.local/bin/omadesign'), ['--shot-file',results/'upscale_qa-native'/'upscaled-cutout.oma','--size','1440x900','--out',results/'installed-reopened.png'])
else:
    raise SystemExit('Unknown mode')
