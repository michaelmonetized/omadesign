#!/usr/bin/env python3
"""Copy a sealed fleet attempt, then explicitly execute only missing/replaced runs.

Preparation never launches a workload. Failed and superseded attempts remain in
the derived output, while retained receipts and output files stay byte-identical.
Use `prepare --help` and `execute --help` for the two separate stages.
"""
import argparse
import copy
import datetime
import hashlib
import importlib.util
import json
import math
import os
from pathlib import Path
import platform
import shutil
import stat

NATIVE = ('background_removal_qa-native', 'upscale_qa-native')
RUNNERS = {
    '75a04566353266808a8cb9300becb397ce6bd5840b62af12c9f30bb1485abc8e',
    '2e643fdd816b6b31d38a2c9d6ceb6af488ec65f6b8296097c9b54d95e9280dba',
}


def require(value, message):
    if not value:
        raise ValueError(message)


def read(path):
    return json.loads(Path(path).read_text())


def write(path, value):
    temporary = path.with_suffix(path.suffix + '.tmp')
    temporary.write_text(json.dumps(value, indent=2, allow_nan=False) + '\n')
    temporary.replace(path)


def identity(path):
    h = hashlib.sha256()
    with Path(path).open('rb') as stream:
        for chunk in iter(lambda: stream.read(1024 * 1024), b''):
            h.update(chunk)
    return {'bytes': Path(path).stat().st_size, 'sha256': h.hexdigest()}


def inventory(root):
    result = {}
    for path in sorted(root.rglob('*')):
        require(not path.is_symlink(), f'lineage copy requires ordinary files: {path}')
        if path.is_file():
            result[str(path.relative_to(root))] = identity(path)
    return result


def load_runner(path):
    require(identity(path)['sha256'] in RUNNERS, 'execution runner is not a frozen audited collector')
    spec = importlib.util.spec_from_file_location('fleet_frozen_runner', path)
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


def key(entry):
    return entry['side'], entry['case'], entry['repetition']


def schedule(runner, rounds):
    pair = 0
    for case in runner.CASES:
        count = 2 if case == 'authoring' else 1 if case in ('canvas-profile', 'package-reopen') else rounds
        for repetition in range(1, count + 1):
            for side in (('baseline', 'candidate') if pair % 2 == 0 else ('candidate', 'baseline')):
                yield side, case, repetition
            pair += 1


def run_path(side, case, repetition):
    return Path('runs') / side / f'{case}-{repetition}'


def utc():
    return datetime.datetime.now(datetime.timezone.utc).isoformat()


def relative(path):
    path = Path(path)
    require(not path.is_absolute() and '..' not in path.parts, f'unsafe relative evidence path: {path}')
    return path


def subset(files, prefix):
    return {name.removeprefix(str(prefix) + '/'): value for name, value in files.items()
            if name.startswith(str(prefix) + '/')}


def verified_override(config, manifest, case, side):
    """All identities must come from a completed, explicitly supplied build."""
    value = config['sides'][side][case]
    require(value['production_revision'] == manifest['source_revisions'][side],
            f'{side}/{case}: override changes the application production revision')
    require(identity(value['path']) == value['identity'], f'{side}/{case}: override executable drift')
    require(os.access(value['path'], os.X_OK), 'override is not executable')
    proof = value['build_receipt']
    require(identity(proof['path']) == proof['identity'], 'override build receipt drift')
    build = read(proof['path'])
    verify_override_build(value, build, manifest)
    for name, item in value['harness_sources'].items():
        relative(name)
        require(identity(item['path']) == item['identity'], 'override harness source drift')
    return value


def verify_override_build(value, build, manifest):
    proof = value['build_receipt']
    require(build.get('success') is True
            and build['production_commit'] == value['production_revision']
            and build['architecture'] == manifest['architecture']
            and build['artifacts'][proof['artifact_key']] == value['identity'],
            'override build does not prove the requested production/architecture/artifact')
    require(build['steps'] and all(step['exit_code'] == 0 for step in build['steps']),
            'override build contains missing or failed steps')
    sources = value['harness_sources']
    require(sources and {name: item['identity'] for name, item in sources.items()}
            == build['harness_sources'], 'override harness source inventory differs from its build')


def validate_native_protocol(directory, protocol, case):
    result = read(directory / 'native-result.json')
    require(result.get('qa_input_protocol') == protocol, 'native result protocol differs from approved override')
    require(result.get('cancel_input') == 'Escape key' and result.get('cancel_button_covered') is False,
            'native cancellation coverage metadata differs from approved protocol')
    if case == 'background_removal_qa-native':
        require(result.get('matte_radius_changed') is True and type(result.get('matte_radius')) is int
                and result['matte_radius'] > 12, 'native matte radius did not change through the slider')


def check_overrides(config, manifest):
    require(config['schema_version'] == 1 and isinstance(config['protocol'], str)
            and config['protocol'].strip(), 'missing native override protocol')
    require(set(config['sides']) == {'baseline', 'candidate'}, 'native override requires both sides')
    for side in ('baseline', 'candidate'):
        require(set(config['sides'][side]) == set(NATIVE), 'override both native AI workflows together')
        for case in NATIVE:
            verified_override(config, manifest, case, side)
    for case in NATIVE:
        sources = [{name: item['identity'] for name, item in config['sides'][side][case]['harness_sources'].items()}
                   for side in ('baseline', 'candidate')]
        require(sources[0] == sources[1], f'{case}: baseline/candidate native driver source differs')


def prepare(args):
    parent, output = args.parent.resolve(), args.output.resolve()
    require(not output.exists(), 'derived output must be new')
    require(not output.is_relative_to(parent), 'derived output cannot be inside its parent')
    manifest = read(parent / 'manifest.json')
    require(isinstance(manifest.get('success'), bool) and manifest.get('finished_utc'),
            'parent must be sealed after its runner has stopped')
    runner = load_runner(args.runner)
    require(manifest['ai_rounds'] >= 2, 'incomplete original repetition count')
    expected = list(schedule(runner, manifest['ai_rounds']))
    entries = {key(entry): entry for entry in manifest['runs']}
    require(len(entries) == len(manifest['runs']) and set(entries) <= set(expected),
            'duplicate or unexpected parent cases')
    overrides = read(args.native_overrides) if args.native_overrides else None
    if overrides:
        check_overrides(overrides, manifest)
    rerun_native = args.rerun_native or overrides is not None
    parent_id = identity(parent / 'manifest.json')
    parent_files = inventory(parent)
    # Ordinary copies ensure a recovered workflow cannot mutate parent files.
    shutil.copytree(parent, output, copy_function=shutil.copy2)
    lineage_dir = Path('lineage') / parent_id['sha256']
    saved = output / lineage_dir
    saved.mkdir(parents=True, exist_ok=False)
    shutil.copy2(parent / 'manifest.json', saved / 'parent-manifest.json')
    write(saved / 'parent-files.json', parent_files)
    shutil.copy2(args.runner, saved / 'execution-runner.py')
    shutil.copy2(__file__, saved / 'continuation-tool.py')
    lineage = {
        'schema_version': 1, 'prepared_utc': utc(),
        'parent_manifest': {'path': str(lineage_dir / 'parent-manifest.json'), 'identity': parent_id},
        'parent_inventory': {'path': str(lineage_dir / 'parent-files.json'),
                             'identity': identity(saved / 'parent-files.json')},
        'parent_status': {'success': manifest['success'], 'errors': manifest['errors']},
        'execution_runner': {'path': str(lineage_dir / 'execution-runner.py'), 'identity': identity(args.runner)},
        'continuation_tool': {'path': str(lineage_dir / 'continuation-tool.py'), 'identity': identity(__file__)},
        'rerun_all_native_ai': rerun_native, 'retained': [], 'excluded': [], 'new_attempts': [],
        'native_overrides': None,
    }
    if overrides:
        shutil.copy2(args.native_overrides, saved / 'native-overrides.json')
        lineage['native_overrides'] = {
            'path': str(lineage_dir / 'native-overrides.json'),
            'identity': identity(saved / 'native-overrides.json'), 'protocol': overrides['protocol'],
            'evidence': {},
        }
        for side in ('baseline', 'candidate'):
            for case in NATIVE:
                value = overrides['sides'][side][case]
                place = saved / 'native-builds' / side / case
                place.mkdir(parents=True)
                shutil.copy2(value['build_receipt']['path'], place / 'build-receipt.json')
                for name, source in value['harness_sources'].items():
                    target = place / 'sources' / relative(name)
                    target.parent.mkdir(parents=True, exist_ok=True)
                    shutil.copy2(source['path'], target)
                lineage['native_overrides']['evidence'][f'{side}/{case}'] = {
                    'build_receipt': str((place / 'build-receipt.json').relative_to(output)),
                    'sources': str((place / 'sources').relative_to(output)),
                    'executable_identity': value['identity'],
                }
    retained = []
    for item in expected:
        side, case, repetition = item
        directory = run_path(*item)
        entry = entries.get(item)
        preserve = bool(entry and entry['passed'] and not (rerun_native and case in NATIVE))
        if entry:
            require(relative(entry['receipt']) == directory / 'receipt.json', 'noncanonical parent receipt')
            receipt = read(parent / entry['receipt'])
            require(receipt['passed'] is entry['passed'], 'manifest/receipt pass status mismatch')
        if preserve:
            require(receipt['exit_code'] == 0 and not receipt['errors'] and not receipt['timed_out'],
                    'retained receipt does not represent a successful process')
            require(math.isfinite(receipt['wall_seconds']) and receipt['wall_seconds'] >= 0
                    and receipt['resources']
                    and all(math.isfinite(value) and value >= 0 for value in receipt['resources'].values()),
                    'retained resource/timing values are invalid')
            if case in NATIVE or case in ('authoring', 'package-reopen'):
                require(any(handle.get('drm-driver') == manifest['expected_drm_driver']
                            for handle in receipt['gpu_handles']), 'retained native hardware evidence missing')
            runner.validate(case, parent / directory, parent / directory / 'workload.log',
                            [1020, 770] if side == 'baseline' else [1014.71875, 771])
            retained_entry = copy.deepcopy(entry)
            origin = {'kind': 'retained', 'parent_manifest_sha256': parent_id['sha256'],
                      'attempt_index': entry.get('origin', {}).get('attempt_index', 1),
                      'parent_receipt': entry['receipt'], 'receipt_identity': identity(parent / entry['receipt']),
                      'parent_run_path': str(directory), 'artifact_inventory': subset(parent_files, directory)}
            retained_entry['origin'] = origin
            retained.append(retained_entry)
            lineage['retained'].append({'side': side, 'case': case, 'repetition': repetition, **origin})
        else:
            reason = ('superseded for matched native protocol' if entry and entry['passed']
                      else 'failed parent attempt' if entry else 'unrecorded partial output')
            archive = Path('excluded-attempts') / parent_id['sha256'] / side / f'{case}-{repetition}'
            excluded = {
                'side': side, 'case': case, 'repetition': repetition, 'reason': reason,
                'parent_run_path': str(directory), 'archived_run_path': None,
                'parent_receipt': entry['receipt'] if entry else None,
                'receipt_identity': identity(parent / entry['receipt']) if entry else None,
                'artifact_inventory': subset(parent_files, directory),
                'parent_profile_path': None, 'archived_profile_path': None, 'profile_inventory': {},
            }
            if (output / directory).exists():
                destination = output / archive / 'run'
                destination.parent.mkdir(parents=True, exist_ok=True)
                shutil.move(output / directory, destination)
                excluded['archived_run_path'] = str(archive / 'run')
            profile = Path('profiles') / side / f'{case}-{repetition}'
            if (output / profile).exists():
                (output / archive).mkdir(parents=True, exist_ok=True)
                shutil.move(output / profile, output / archive / 'profile')
                excluded.update(parent_profile_path=str(profile), archived_profile_path=str(archive / 'profile'),
                                profile_inventory=subset(parent_files, profile))
            if excluded['archived_run_path'] or excluded['archived_profile_path']:
                lineage['excluded'].append(excluded)
    derived = copy.deepcopy(manifest)
    derived.update(started_utc=utc(), runs=retained, errors=[], success=False,
                   status='prepared', continuation=lineage,
                   all_attempts_passed=manifest.get('all_attempts_passed', manifest['success']))
    derived.pop('finished_utc', None)
    derived['runner'] = identity(args.runner)
    write(output / 'manifest.json', derived)
    require(inventory(parent) == parent_files, 'parent evidence changed during preparation')
    verify_lineage(output, derived)
    print(json.dumps({'status': 'prepared', 'retained_runs': len(retained),
                      'pending_runs': len(expected) - len(retained),
                      'excluded_attempts': len(lineage['excluded']), 'output': str(output)}))


def verify_lineage(output, manifest):
    """Read-only identity audit; never resolve original host absolute paths."""
    output = Path(output)
    lineage = manifest['continuation']
    for field in ('parent_manifest', 'parent_inventory', 'execution_runner', 'continuation_tool'):
        proof = lineage[field]
        require(identity(output / relative(proof['path'])) == proof['identity'], f'{field} snapshot drift')
    parent = read(output / lineage['parent_manifest']['path'])
    parent_files = read(output / lineage['parent_inventory']['path'])
    require(lineage['parent_manifest']['identity'] == parent_files['manifest.json'], 'parent manifest inventory mismatch')
    require(lineage['parent_status'] == {'success': parent['success'], 'errors': parent['errors']},
            'parent attempt status differs from snapshot')
    for field in ('architecture', 'ai_rounds', 'source_revisions', 'artifacts', 'display', 'expected_drm_driver'):
        require(manifest[field] == parent[field], f'original cohort {field} changed')
    original_entries = {key(entry): entry for entry in parent['runs']}
    selected = {key(entry): entry for entry in manifest['runs']}
    require(len(selected) == len(manifest['runs']), 'duplicate selected case')
    retained_keys = [key(entry) for entry in lineage['retained']]
    recovered_keys = [key(entry) for entry in lineage['new_attempts']]
    require(len(set(retained_keys + recovered_keys)) == len(retained_keys + recovered_keys)
            and set(selected) == set(retained_keys + recovered_keys),
            'selected cases differ from retained/recovered lineage')
    excluded_keys = [key(entry) for entry in lineage['excluded']]
    require(len(set(excluded_keys)) == len(excluded_keys)
            and set(original_entries) - set(retained_keys) <= set(excluded_keys),
            'excluded lineage does not preserve each replaced parent attempt')
    if lineage['rerun_all_native_ai']:
        require(not any(entry['case'] in NATIVE for entry in lineage['retained']),
                'matched native protocol cohort retained an original native run')
    parent_sha = lineage['parent_manifest']['identity']['sha256']
    for retained in lineage['retained']:
        original = original_entries.get(key(retained))
        require(original and original['passed'] and original['receipt'] == retained['parent_receipt']
                and retained['receipt_identity'] == parent_files[original['receipt']],
                'retained case does not match its parent receipt')
        prefix = relative(retained['parent_run_path'])
        require(prefix == run_path(*key(retained)), 'retained output belongs to another case')
        require(retained['artifact_inventory'] == subset(parent_files, prefix), 'retained inventory provenance mismatch')
        require(inventory(output / prefix) == retained['artifact_inventory'], 'retained output changed')
        require(identity(output / retained['parent_receipt']) == retained['receipt_identity'], 'retained receipt changed')
        entry = selected[key(retained)]
        origin = {name: value for name, value in retained.items() if name not in ('side', 'case', 'repetition')}
        require(origin['kind'] == 'retained' and origin['parent_manifest_sha256'] == parent_sha
                and origin['attempt_index'] == original.get('origin', {}).get('attempt_index', 1),
                'retained attempt lineage mismatch')
        expected_entry = {**original, 'origin': origin}
        require(entry == expected_entry, 'selected retained entry differs from its origin')
    for excluded in lineage['excluded']:
        prefix = run_path(*key(excluded))
        archive = Path('excluded-attempts') / parent_sha / excluded['side'] / f'{excluded["case"]}-{excluded["repetition"]}'
        require(relative(excluded['parent_run_path']) == prefix, 'excluded output belongs to another case')
        require(excluded['artifact_inventory'] == subset(parent_files, excluded['parent_run_path']),
                'excluded inventory provenance mismatch')
        if excluded['archived_run_path']:
            require(relative(excluded['archived_run_path']) == archive / 'run', 'noncanonical excluded run archive')
            require(inventory(output / relative(excluded['archived_run_path'])) == excluded['artifact_inventory'],
                    'excluded attempt changed')
        else:
            require(not excluded['artifact_inventory'] and not excluded['parent_receipt'],
                    'excluded run artifacts lack an archive mapping')
        if excluded['parent_receipt']:
            require(relative(excluded['parent_receipt']) == prefix / 'receipt.json'
                    and excluded['receipt_identity'] == parent_files[excluded['parent_receipt']],
                    'excluded receipt provenance mismatch')
        if excluded['archived_profile_path']:
            profile = Path('profiles') / excluded['side'] / f'{excluded["case"]}-{excluded["repetition"]}'
            require(relative(excluded['archived_profile_path']) == archive / 'profile'
                    and relative(excluded['parent_profile_path']) == profile
                    and excluded['profile_inventory'] == subset(parent_files, profile),
                    'excluded profile provenance mismatch')
            require(inventory(output / relative(excluded['archived_profile_path'])) == excluded['profile_inventory'],
                    'excluded profile changed')
        else:
            require(excluded['parent_profile_path'] is None and not excluded['profile_inventory'],
                    'excluded profile artifacts lack an archive mapping')
    overrides = None
    if lineage['native_overrides']:
        proof = lineage['native_overrides']
        require(identity(output / relative(proof['path'])) == proof['identity'], 'override snapshot drift')
        overrides = read(output / proof['path'])
        require(overrides['schema_version'] == 1 and overrides['protocol'] == proof['protocol']
                and bool(proof['protocol']) and set(overrides['sides']) == {'baseline', 'candidate'}
                and lineage['rerun_all_native_ai'] is True, 'override snapshot protocol/scope mismatch')
        require(set(proof['evidence']) == {f'{side}/{case}' for side in ('baseline', 'candidate') for case in NATIVE},
                'override snapshot evidence inventory differs')
        for side in ('baseline', 'candidate'):
            require(set(overrides['sides'][side]) == set(NATIVE), 'override snapshot native case inventory differs')
            for case in NATIVE:
                value = overrides['sides'][side][case]
                require(value['production_revision'] == manifest['source_revisions'][side],
                        'override snapshot production revision differs')
                evidence = proof['evidence'][f'{side}/{case}']
                build_path = output / relative(evidence['build_receipt'])
                require(identity(build_path) == value['build_receipt']['identity'], 'copied override build drift')
                verify_override_build(value, read(build_path), manifest)
                require(evidence['executable_identity'] == value['identity'], 'copied override executable identity differs')
                sources = {name: item['identity'] for name, item in value['harness_sources'].items()}
                require(inventory(output / relative(evidence['sources'])) == sources, 'copied override source drift')
        for case in NATIVE:
            require({name: item['identity'] for name, item in overrides['sides']['baseline'][case]['harness_sources'].items()}
                    == {name: item['identity'] for name, item in overrides['sides']['candidate'][case]['harness_sources'].items()},
                    'copied native driver source differs between sides')
    for recovered in lineage['new_attempts']:
        entry = selected[key(recovered)]
        origin = {name: value for name, value in recovered.items()
                  if name not in ('side', 'case', 'repetition', 'receipt', 'passed')}
        require(entry == {'side': recovered['side'], 'case': recovered['case'], 'repetition': recovered['repetition'],
                          'receipt': recovered['receipt'], 'passed': recovered['passed'], 'origin': origin},
                'selected recovered entry differs from its origin')
        predecessor = original_entries.get(key(recovered))
        attempt_index = predecessor.get('origin', {}).get('attempt_index', 1) + 1 if predecessor else 1
        require(origin['kind'] == 'recovered' and origin['parent_manifest_sha256'] == parent_sha
                and origin['execution_runner_sha256'] == lineage['execution_runner']['identity']['sha256']
                and origin['attempt_index'] == attempt_index, 'recovered attempt lineage mismatch')
        prefix = run_path(*key(recovered))
        require(relative(recovered['receipt']) == prefix / 'receipt.json', 'noncanonical recovered receipt')
        require(identity(output / recovered['receipt']) == recovered['receipt_identity'], 'recovered receipt changed')
        require(inventory(output / prefix) == recovered['artifact_inventory'], 'recovered output changed')
        receipt = read(output / recovered['receipt'])
        require(receipt['passed'] is recovered['passed'], 'recovered receipt pass status mismatch')
        expected_override = overrides['sides'][recovered['side']][recovered['case']] if overrides and recovered['case'] in NATIVE else None
        require(recovered['native_override'] == expected_override
                and recovered['native_protocol'] == (overrides['protocol'] if expected_override else None),
                'recovered native override lineage mismatch')
        if recovered['passed']:
            require(receipt['exit_code'] == 0 and not receipt['timed_out'] and not receipt['errors'],
                    'recovered successful receipt has process errors')
            if expected_override:
                validate_native_protocol(output / prefix, overrides['protocol'], recovered['case'])
    if lineage.get('execution_build'):
        proof = lineage['execution_build']
        require(identity(output / relative(proof['path'])) == proof['identity'], 'execution build snapshot drift')
    return parent


def validate_continuation(output):
    """Public read-only audit entry point for a copied result directory."""
    output = Path(output)
    manifest = read(output / 'manifest.json')
    verify_lineage(output, manifest)
    return manifest


def execute(args):
    output = args.output.resolve()
    manifest = read(output / 'manifest.json')
    require(manifest.get('status') == 'prepared' and not manifest.get('finished_utc'),
            'execute only a newly prepared continuation; derive again after any stopped attempt')
    parent = verify_lineage(output, manifest)
    lineage = manifest['continuation']
    require(identity(__file__) == lineage['continuation_tool']['identity'], 'continuation tool changed after preparation')
    runner = load_runner(output / lineage['execution_runner']['path'])
    runner.SWAYMSG = str(args.swaymsg.resolve())
    require(identity(args.swaymsg) == manifest['swaymsg'], 'Sway query executable changed')
    require(platform.machine() == manifest['architecture'], 'wrong execution architecture')
    require(math.isfinite(args.timeout) and args.timeout > 0, 'invalid timeout')
    for field in ('baseline_bin', 'candidate_bin', 'inputs', 'runtime', 'candidate_build'):
        setattr(args, field, getattr(args, field).resolve())
    args.output = output
    files = {name: runner.inventory(path) for name, path in
             [('baseline', args.baseline_bin), ('candidate', args.candidate_bin),
              ('inputs', args.inputs), ('runtime', args.runtime)]}
    require(files == manifest['artifacts'], 'application/input/runtime artifacts differ from original cohort')
    build = read(args.candidate_build)
    require(build.get('success') is True and build['source_commit'] == manifest['source_revisions']['candidate'],
            'continuation requires the completed candidate build receipt')
    for name, prefix in [('candidate', 'bin'), ('runtime', 'lib')]:
        require(files[name] == {key.removeprefix(f'{manifest["architecture"]}/{prefix}/'): value
                               for key, value in build['artifacts'].items()
                               if key.startswith(f'{manifest["architecture"]}/{prefix}/')}, 'candidate build artifact drift')
    needed = {'check-all-targets', 'library-tests', manifest['architecture'] + '-portable-release'}
    if manifest['architecture'] == 'aarch64':
        needed.add('native-qa-release')
    require(build['steps'] and all(step['exit_code'] == 0 for step in build['steps'])
            and needed <= {step['name'] for step in build['steps']}, 'candidate validation/build incomplete')
    display = read(args.display_env)
    require(set(display) <= runner.DISPLAY_KEYS and {'SWAYSOCK', 'WAYLAND_DISPLAY', 'XDG_RUNTIME_DIR'} <= set(display),
            'invalid private display environment')
    private = Path('/run/user') / str(os.getuid()) / 'omadesign-fleet-0.6.1'
    require(Path(display['XDG_RUNTIME_DIR']) == private and private.stat().st_uid == os.getuid()
            and stat.S_IMODE(private.stat().st_mode) == 0o700 and Path(display['SWAYSOCK']).parent == private,
            'display is not the task-owned private runtime')
    env = os.environ.copy()
    env.pop('DISPLAY', None)
    env.update(display)
    env['WINIT_UNIX_BACKEND'] = 'wayland'
    require(not any(name in env for name in ('OMADESIGN_FONT', 'WGPU_BACKEND', 'WGPU_ADAPTER_NAME',
            'LIBGL_ALWAYS_SOFTWARE', 'MESA_LOADER_DRIVER_OVERRIDE', 'ORT_NUM_THREADS', 'OMP_NUM_THREADS',
            'LD_LIBRARY_PATH', 'LD_PRELOAD')), 'unexpected renderer/font/inference/library environment')
    require(runner.display_state(env) == manifest['display'], 'private display changed')
    require(args.expected_drm_driver == manifest['expected_drm_driver'], 'hardware-driver expectation changed')
    require(runner.command(['unshare', '-Urn', 'true'])['exit_code'] == 0, 'network namespace unavailable')
    overrides = None
    if lineage['native_overrides']:
        proof = lineage['native_overrides']
        require(identity(output / proof['path']) == proof['identity'], 'override manifest drift')
        overrides = read(output / proof['path'])
        check_overrides(overrides, manifest)
        original_arguments = runner.arguments
        def arguments(case, side, inputs, directory, destination):
            binary, parameters = original_arguments(case, side, inputs, directory, destination)
            if case in NATIVE:
                binary = str(Path(overrides['sides'][side][case]['path']).resolve())
            return binary, parameters
        runner.arguments = arguments
        original_validate = runner.validate
        def validate(case, directory, log, canvas):
            result = original_validate(case, directory, log, canvas)
            if case in NATIVE:
                validate_native_protocol(directory, overrides['protocol'], case)
            return result
        runner.validate = validate
    lineage['executed_utc'] = utc()
    lineage['execution_state'] = runner.state()
    build_snapshot = Path(lineage['parent_manifest']['path']).parent / 'execution-build.json'
    shutil.copy2(args.candidate_build, output / build_snapshot)
    lineage['execution_build'] = {'path': str(build_snapshot), 'identity': identity(args.candidate_build)}
    manifest['status'] = 'running'
    write(output / 'manifest.json', manifest)
    present = {key(entry) for entry in manifest['runs']}
    previous = {key(entry): entry for entry in parent['runs']}
    try:
        for item in schedule(runner, manifest['ai_rounds']):
            if item in present:
                continue
            before = len(manifest['runs'])
            try:
                runner.run_one(args, *item, env, manifest)
            finally:
                if len(manifest['runs']) == before + 1:
                    entry = manifest['runs'][-1]
                    predecessor = previous.get(item)
                    origin = {
                        'kind': 'recovered', 'parent_manifest_sha256': lineage['parent_manifest']['identity']['sha256'],
                        'execution_runner_sha256': lineage['execution_runner']['identity']['sha256'],
                        'attempt_index': (predecessor.get('origin', {}).get('attempt_index', 1) + 1) if predecessor else 1,
                        'receipt_identity': identity(output / entry['receipt']),
                        'artifact_inventory': inventory(output / run_path(*item)),
                        'native_override': overrides['sides'][item[0]][item[1]] if overrides and item[1] in NATIVE else None,
                        'native_protocol': overrides['protocol'] if overrides and item[1] in NATIVE else None,
                    }
                    entry['origin'] = origin
                    lineage['new_attempts'].append({'side': item[0], 'case': item[1], 'repetition': item[2],
                                                    'receipt': entry['receipt'], 'passed': entry['passed'], **origin})
                    if not entry['passed']:
                        manifest['all_attempts_passed'] = False
                    write(output / 'manifest.json', manifest)
        verify_lineage(output, manifest)
        require({key(entry) for entry in manifest['runs']} == set(schedule(runner, manifest['ai_rounds']))
                and all(entry['passed'] for entry in manifest['runs']), 'selected matrix incomplete')
        for name, path in [('baseline', args.baseline_bin), ('candidate', args.candidate_bin),
                           ('inputs', args.inputs), ('runtime', args.runtime)]:
            require(runner.inventory(path) == files[name], f'{name} artifacts changed')
        if overrides:
            check_overrides(overrides, manifest)
    except BaseException as error:
        manifest['errors'].append(str(error) or type(error).__name__)
        raise
    finally:
        manifest['finished_utc'] = utc()
        manifest['success'] = not manifest['errors'] and len(manifest['runs']) == len(list(schedule(runner, manifest['ai_rounds'])))
        manifest['status'] = 'complete' if manifest['success'] else 'stopped'
        write(output / 'manifest.json', manifest)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    stages = parser.add_subparsers(dest='stage', required=True)
    setup = stages.add_parser('prepare', help='copy and seal provenance; never launch a workload')
    for name in ('parent', 'output', 'runner'):
        setup.add_argument('--' + name, type=Path, required=True)
    setup.add_argument('--native-overrides', type=Path)
    setup.add_argument('--rerun-native', action='store_true')
    run = stages.add_parser('execute', help='execute only pending cases in a prepared continuation')
    for name in ('output', 'baseline-bin', 'candidate-bin', 'inputs', 'runtime', 'candidate-build', 'display-env'):
        run.add_argument('--' + name, type=Path, required=True)
    run.add_argument('--swaymsg', type=Path, default=Path('/usr/bin/swaymsg'))
    run.add_argument('--expected-drm-driver', choices=('asahi', 'amdgpu', 'i915', 'xe'), required=True)
    run.add_argument('--timeout', type=float, default=1200)
    args = parser.parse_args()
    (prepare if args.stage == 'prepare' else execute)(args)


if __name__ == '__main__':
    main()
