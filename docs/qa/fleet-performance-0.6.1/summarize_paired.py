#!/usr/bin/env python3
"""Audit three completed paired fleets and summarize real retained measurements.

No workload is launched. The fresh comparison and original historical table
remain separate. Pixel/document differences are quantified, never silently
accepted or relabeled identical. Native-AI raw UI samples are unavailable.
"""
import argparse
import ast
from collections import Counter, defaultdict
from datetime import datetime, timezone
import hashlib
import json
import math
from pathlib import Path
import re
import statistics
import subprocess
import sys

from PIL import Image, ImageChops, ImageStat

sys.dont_write_bytecode = True
from document_id_equivalence import prove as prove_ids, strict_load
from run_paired import BASELINE, BINS, CASES, MODELS, validate
from audit_continuation import audit as audit_continuation, PROTOCOL as NATIVE_PROTOCOL
from schema_defaults_equivalence import prove_with_current_defaults, verify_source as verify_schema_defaults

HOSTS = ('hpeliteclient', 'intelpro', 'm1pro16')
SIDES = ('baseline', 'candidate')
METRICS = ('ui_ms', 'input_to_ui_ms', 'frame_interval_ms')
PHASES = ('idle', 'pen-preview', 'pen-commit', 'typing', 'brush-drag', 'pan', 'zoom', 'save')
# The candidate harness adds hover delivery before clicks. Preserve those raw
# samples but do not mistake them for a common old/new authoring workload phase.
COORDINATION = {'startup', 'check', 'setup', 'preview-wait', 'pointer-hover'}
BASELINE_DOCS = Path(__file__).resolve().parents[1] / 'fleet-benchmark-2026-09-27'
CORE_AI = ('src/background_removal.rs', 'src/background_removal',
           'src/upscale.rs', 'src/upscale', 'src/ml.rs', 'src/ml')
RUNTIME_FILES = ('libonnxruntime.so.1.28.0', 'libonnxruntime_providers_shared.so')
CANVAS_SIZES = {'baseline': [1020, 770], 'candidate': [1014.71875, 771]}
RUNNER_SOURCES = {
    'x86_64': {'path': 'runner-versions/run_paired-fdinfo.py',
               'identity': {'bytes': 23048, 'sha256': '75a04566353266808a8cb9300becb397ce6bd5840b62af12c9f30bb1485abc8e'}},
    'aarch64': {'path': 'run_paired.py',
                'identity': {'bytes': 24123, 'sha256': '2e643fdd816b6b31d38a2c9d6ceb6af488ec65f6b8296097c9b54d95e9280dba'}},
}


def require(value, message):
    if not value:
        raise ValueError(message)


def read(path):
    return strict_load(path.read_bytes())


def identity(path):
    h = hashlib.sha256()
    with path.open('rb') as stream:
        for block in iter(lambda: stream.read(1024 * 1024), b''):
            h.update(block)
    return {'sha256': h.hexdigest(), 'bytes': path.stat().st_size}


def inventory(path):
    return {str(p.relative_to(path)): identity(p) for p in sorted(path.rglob('*')) if p.is_file()}


def runner_ast_without_gpu(source):
    tree = ast.parse(source)
    functions = [node for node in tree.body if isinstance(node, ast.FunctionDef) and node.name == 'gpu_handles']
    require(len(functions) == 1, 'runner must contain exactly one top-level gpu_handles function')
    tree.body.remove(functions[0])
    return ast.dump(tree, include_attributes=False)


def verify_runner_sources():
    outside = []
    for source in RUNNER_SOURCES.values():
        path = Path(__file__).parent / source['path']
        require(identity(path) == source['identity'], 'frozen runner source changed: ' + source['path'])
        outside.append(runner_ast_without_gpu(path.read_bytes()))
    require(outside[0] == outside[1], 'runner program differs outside gpu_handles collector')
    return {'frozen_sources': RUNNER_SOURCES,
            'assigned_architectures': {'hpeliteclient': 'x86_64', 'intelpro': 'x86_64', 'm1pro16': 'aarch64'},
            'continuation_execution_runner_identity': RUNNER_SOURCES['aarch64']['identity'],
            'ast_identical_except_gpu_handles': True,
            'program_ast_without_gpu_handles_sha256': hashlib.sha256(outside[0].encode()).hexdigest(),
            'scope': 'Only GPU evidence collection differs. The Asahi collector can resolve the driver from sysfs for an actual open render-node character descriptor when DRM fdinfo is absent. Scheduling, timing, actions, commands, validation and all other program AST are identical.'}


def validate_build_snapshot(snapshot, final, architecture):
    """A measured architecture may finish before the other architecture builds.

    Preserve that immutable readiness receipt. It must contain complete proof
    for this host and be an exact successful subset of the final global build;
    it cannot substitute a different compiler, command, log, or executable.
    """
    require(architecture in ('aarch64', 'x86_64'), 'unsupported build architecture')
    require(final.get('success') is True, 'final global build incomplete')
    for key in ('source_commit', 'rustc', 'cargo', 'profile'):
        require(isinstance(snapshot.get(key), str) and snapshot[key]
                and snapshot[key] == final.get(key), 'build snapshot differs from final ' + key)
    def steps(receipt):
        rows = receipt['steps']
        require(isinstance(rows, list) and rows, 'build steps missing')
        names = [row['name'] for row in rows]
        require(len(names) == len(set(names)), 'duplicate build step')
        require(all(row['exit_code'] == 0 for row in rows), 'unsuccessful recorded build step')
        return {row['name']: row for row in rows}
    old_steps, final_steps = steps(snapshot), steps(final)
    for name, step in old_steps.items():
        require(name in final_steps and step == final_steps[name], 'build step changed after readiness: ' + name)
    needed = {'check-all-targets', 'library-tests', architecture + '-portable-release'}
    if architecture == 'aarch64':
        needed.add('native-qa-release')
    require(needed <= old_steps.keys(), 'architecture readiness steps incomplete')
    for name, artifact in snapshot['artifacts'].items():
        require(type(artifact.get('bytes')) is int and artifact['bytes'] > 0
                and isinstance(artifact.get('sha256'), str)
                and re.fullmatch(r'[0-9a-f]{64}', artifact['sha256']), 'invalid readiness artifact identity')
        require(final['artifacts'].get(name) == artifact, 'artifact changed after readiness: ' + name)
    for directory, expected in [('bin', BINS), ('lib', RUNTIME_FILES)]:
        prefix = f'{architecture}/{directory}/'
        names = {name.removeprefix(prefix) for name in snapshot['artifacts'] if name.startswith(prefix)}
        require(names == set(expected), 'architecture readiness artifacts incomplete: ' + directory)
    return {'architecture': architecture, 'verified_exact_successful_subset_of_final': True,
            'identical_receipt_contents_to_final': snapshot == final,
            'global_build_success_recorded_at_capture': snapshot.get('success') is True,
            'successful_step_names': sorted(old_steps), 'artifact_identities': snapshot['artifacts'],
            'source_commit': snapshot['source_commit'], 'rustc': snapshot['rustc'], 'cargo': snapshot['cargo'],
            'scope': 'Immutable receipt captured when this architecture was ready; all captured steps and artifacts match the completed final global receipt exactly. Other architecture work may still have been pending.'}


def number(value):
    return type(value) in (int, float) and math.isfinite(value) and value >= 0


def stats(values):
    values = sorted(values)
    require(values and all(number(v) for v in values), 'invalid or absent raw timings')
    return {'n': len(values), 'median': statistics.median(values),
            'p95': values[math.ceil(.95 * len(values)) - 1],
            'min': values[0], 'max': values[-1], 'mean': statistics.mean(values)}


def distribution(runs):
    require(runs and all(runs), 'missing repetition')
    per_run = [stats(values) for values in runs]
    return {'pooled': stats([value for values in runs for value in values]),
            'per_run': per_run, 'raw_runs': runs,
            'run_median_range': [min(r['median'] for r in per_run), max(r['median'] for r in per_run)]}


def paired(before, after):
    return {'baseline': before, 'candidate': after,
            'speedup_old_over_new': {key: before['pooled'][key] / after['pooled'][key]
                                    if after['pooled'][key] > 0 else None
                                    for key in ('median', 'p95', 'max')},
            'delta_new_minus_old': {key: after['pooled'][key] - before['pooled'][key]
                                   for key in ('median', 'p95', 'max')}}


def state(value):
    # Never copy commands, environment, process IDs or unrelated desktop data.
    result = {key: value[key] for key in ('memory', 'pressure', 'cpu_policies') if key in value}
    if value.get('loadavg'):
        result['load_average_1_5_15_minutes'] = [float(v) for v in value['loadavg'].split()[:3]]
    return result


def png_difference(left, right):
    with Image.open(left) as image:
        old = image.convert('RGBA')
    with Image.open(right) as image:
        new = image.convert('RGBA')
    result = {'baseline': identity(left), 'compared': identity(right),
              'baseline_size': list(old.size), 'compared_size': list(new.size),
              'same_dimensions': old.size == new.size}
    if old.size != new.size:
        result.update(equal_rgba=False, different_pixels=None, max_channel_delta=None,
                      mean_channel_delta=None)
        return result
    delta = ImageChops.difference(old, new)
    channels = delta.split()
    combined = channels[0]
    for channel in channels[1:]:
        combined = ImageChops.lighter(combined, channel)
    result.update(equal_rgba=old.tobytes() == new.tobytes(),
                  different_pixels=old.width * old.height - combined.histogram()[0],
                  max_channel_delta=[extent[1] for extent in delta.getextrema()],
                  mean_channel_delta=ImageStat.Stat(delta).mean,
                  baseline_rgba_sha256=hashlib.sha256(old.tobytes()).hexdigest(),
                  compared_rgba_sha256=hashlib.sha256(new.tobytes()).hexdigest())
    return result


def json_differences(left, right):
    counts = Counter()
    examples = []
    def changed(path, reason):
        counts[reason] += 1
        if len(examples) < 30:
            examples.append({'path': '/' + '/'.join(map(str, path)), 'kind': reason})
    def visit(a, b, path=()):
        if type(a) is not type(b):
            changed(path, 'type'); return
        if isinstance(a, dict):
            for key in sorted(a.keys() | b.keys()):
                if key not in a:
                    changed(path + (key,), 'added_field')
                elif key not in b:
                    changed(path + (key,), 'removed_field')
                else:
                    visit(a[key], b[key], path + (key,))
        elif isinstance(a, list):
            if len(a) != len(b):
                changed(path, 'array_length')
            for index, (x, y) in enumerate(zip(a, b)):
                visit(x, y, path + (index,))
        elif json.dumps(a, allow_nan=False) != json.dumps(b, allow_nan=False):
            changed(path, 'value')
    visit(left, right)
    return {'counts': dict(counts), 'total': sum(counts.values()), 'first_paths': examples,
            'values_omitted': True}


def document_difference(left, right, seed):
    a, b = left.read_bytes(), right.read_bytes()
    result = {'baseline': identity(left), 'compared': identity(right), 'equal_bytes': a == b}
    if a == b:
        return result
    result['id_equivalence'] = prove_ids(a, b, seed)
    result['json_differences'] = json_differences(strict_load(a), strict_load(b))
    result['current_schema_defaults_proof'] = prove_with_current_defaults(a, b, seed)
    return result


def artifact_paths(directory, side, repetition):
    base = directory / 'runs' / side
    images = {'authoring': base / f'authoring-{repetition}/infographic.png'}
    documents = {'authoring': base / f'authoring-{repetition}/infographic.oma'}
    for case in ('upscale-320-2x', 'upscale-320-4x', 'upscale-1024-2x'):
        images[case] = base / f'{case}-{repetition}/output.png'
    for case in ('background-320', 'background-1024'):
        for variant in ('global', 'global-guided', 'tiled-guided'):
            for kind in ('mask', 'cutout'):
                name = f'{variant}-{kind}'
                images[f'{case}/{name}'] = base / f'{case}-{repetition}/{name}.png'
    for name in ('document-4x', 'photo-copy', 'photo-custom'):
        images[f'upscale-native/{name}'] = base / f'upscale_qa-native-{repetition}/{name}.png'
    documents['upscale-native'] = base / f'upscale_qa-native-{repetition}/upscaled-cutout.oma'
    documents['background-native'] = base / f'background_removal_qa-native-{repetition}/native-cutout.oma'
    return images, documents


def correctness(directories, seed):
    comparisons = []
    def compare(relation, reference, compared):
        a_host, a_side, a_rep = reference
        b_host, b_side, b_rep = compared
        old = artifact_paths(directories[a_host], a_side, a_rep)
        new = artifact_paths(directories[b_host], b_side, b_rep)
        entry = {'relation': relation, 'reference': list(reference), 'compared': list(compared),
                 'images': {}, 'documents': {}}
        for name in old[0]:
            entry['images'][name] = png_difference(old[0][name], new[0][name])
        for name in old[1]:
            entry['documents'][name] = document_difference(old[1][name], new[1][name], seed)
        comparisons.append(entry)
    for host in HOSTS:
        for side in SIDES:
            compare('repeatability', (host, side, 1), (host, side, 2))
        for repetition in (1, 2):
            compare('before_after', (host, 'baseline', repetition), (host, 'candidate', repetition))
    for side in SIDES:
        for host in ('hpeliteclient', 'intelpro'):
            for repetition in (1, 2):
                compare('cross_host', ('m1pro16', side, repetition), (host, side, repetition))
    images = [image for row in comparisons for image in row['images'].values()]
    documents = [doc for row in comparisons for doc in row['documents'].values()]
    return {'comparisons': comparisons, 'image_pairs': len(images), 'document_pairs': len(documents),
            'equal_image_pairs': sum(image['equal_rgba'] for image in images),
            'equal_document_byte_pairs': sum(doc['equal_bytes'] for doc in documents),
            'document_id_only_pairs': sum(not doc['equal_bytes'] and doc.get('id_equivalence', {}).get(
                'equal_after_consistent_id_remap') is True for doc in documents),
            'document_default_and_id_pairs': sum(not doc['equal_bytes']
                and not doc.get('id_equivalence', {}).get('equal_after_consistent_id_remap')
                and doc.get('current_schema_defaults_proof', {}).get(
                    'common_authored_state_equal_after_current_defaults_and_id_remap') is True for doc in documents),
            'all_images_exact': all(image['equal_rgba'] for image in images),
            'all_documents_exact_or_id_equivalent': all(doc['equal_bytes'] or doc.get('id_equivalence', {}).get(
                'equal_after_consistent_id_remap') is True for doc in documents),
            'policy': 'Differences are observations requiring review, not an automatic acceptance threshold. No tolerances or unknown document fields are discarded. Native screenshots are dimension-checked, not compared as artwork.'}


def summarize_host(host, directory, historical, candidate_build, input_identity, runner_proof, repository):
    manifest = read(directory / 'manifest.json')
    require(manifest.get('success') is True and not manifest['errors'], f'{host}: incomplete suite')
    require(manifest['host'] == host and manifest['schema_version'] == 1, f'{host}: manifest identity')
    require(manifest['architecture'] == runner_proof['assigned_architectures'][host], f'{host}: unexpected architecture')
    require(manifest['runner'] == runner_proof['continuation_execution_runner_identity'],
            f'{host}: recovery runner differs from assigned frozen source')
    require(manifest['ai_rounds'] == 2, f'{host}: expected exactly two AI rounds')
    require(manifest['source_revisions'] == {'baseline': BASELINE, 'candidate': candidate_build['source_commit']},
            f'{host}: source revision mismatch')
    require(identity(directory / 'candidate-build.json') == manifest['candidate_build'],
            f'{host}: build receipt snapshot identity drift')
    build_snapshot = validate_build_snapshot(read(directory / 'candidate-build.json'), candidate_build,
                                             manifest['architecture'])
    build_snapshot['receipt_identity'] = manifest['candidate_build']
    require(identity(directory / 'baseline-manifest.json') == manifest['baseline_manifest']
            and read(directory / 'baseline-manifest.json') == read(historical / host / 'host.json'),
            f'{host}: historical baseline snapshot drift')
    arch = manifest['architecture']
    old_artifacts = read(historical / host / 'host.json')['artifacts']
    actual = manifest['artifacts']
    for category, prefix in [('baseline', 'bin/'), ('inputs', 'inputs/'), ('runtime', 'lib/')]:
        require(actual[category] == {name.removeprefix(prefix): value for name, value in old_artifacts.items()
                                     if name.startswith(prefix)}, f'{host}: original {category} identity drift')
    require(actual['inputs'] == input_identity, f'{host}: supplied verification fixtures differ')
    for category, prefix in [('candidate', f'{arch}/bin/'), ('runtime', f'{arch}/lib/')]:
        require(actual[category] == {name.removeprefix(prefix): value for name, value in candidate_build['artifacts'].items()
                                     if name.startswith(prefix)}, f'{host}: final {category} does not match build receipt')
    require(set(actual['baseline']) == set(actual['candidate']) == set(BINS), f'{host}: missing binary')
    expected = {(side, case, rep) for side in SIDES for case in CASES
                for rep in range(1, (1 if case in ('canvas-profile', 'package-reopen') else 2) + 1)}
    listed = [(run['side'], run['case'], run['repetition']) for run in manifest['runs']]
    require(len(listed) == 36 and set(listed) == expected, f'{host}: incomplete or duplicated 36-run matrix')
    continuation_proof = audit_continuation(directory, manifest, candidate_build, manifest['runner'],
        runner_proof['frozen_sources'][manifest['architecture']]['identity'], repository)
    run_origins = {(row['side'], row['case'], row['repetition']): row
                   for row in continuation_proof['selected_origins']}
    data = {side: {'receipts': {}, 'authoring_runs': [], 'native_ai': {}, 'cli_ai': {},
                   'canvas_rows': None} for side in SIDES}
    hashes = {}
    def load(path):
        value = read(path)
        hashes[str(path.relative_to(directory))] = identity(path)
        return value
    for run in manifest['runs']:
        side, case, rep = run['side'], run['case'], run['repetition']
        folder = directory / 'runs' / side / f'{case}-{rep}'
        require(run['receipt'] == str((folder / 'receipt.json').relative_to(directory)), 'unexpected receipt path')
        receipt = load(folder / 'receipt.json')
        require(run['passed'] is True and receipt['passed'] is True and receipt['exit_code'] == 0
                and receipt['timed_out'] is False and not receipt['errors'], f'{host}/{side}/{case}/{rep}: failed process')
        require((receipt['side'], receipt['case'], receipt['repetition']) == (side, case, rep), 'receipt identity mismatch')
        require(receipt['network'] == 'disabled with unshare -Urn', 'network isolation missing')
        require(number(receipt['wall_seconds']) and receipt['wall_seconds'] > 0, 'invalid process time')
        require(all(number(value) for value in receipt['resources'].values()), 'invalid process resource counter')
        native = case in ('authoring', 'package-reopen') or case.endswith('-native')
        if native:
            require(any(item.get('drm-driver') == manifest['expected_drm_driver'] for item in receipt['gpu_handles']),
                    'expected native GPU handle missing')
        validation = validate(case, folder, folder / 'workload.log', CANVAS_SIZES[side])
        require(receipt['validation'] == validation, 'stored functional validation differs from retained evidence')
        data[side]['receipts'][f'{case}-{rep}'] = {
            'case': case, 'repetition': rep, 'wall_seconds': receipt['wall_seconds'],
            'resources': receipt['resources'], 'before': state(receipt['before']), 'after': state(receipt['after']),
            'started_utc': receipt['started_utc'], 'finished_utc': receipt['finished_utc'],
            'functional_validation': validation, 'hardware_drm_observed': native,
            'measurement_origin': run_origins[(side, case, rep)]['origin'],
            'attempt_index': run_origins[(side, case, rep)]['attempt_index'],
            'gpu_evidence': [{key: entry[key] for key in ('drm-driver', 'drm-pdev', 'driver_source',
                                                         'render_node', 'character_device') if key in entry}
                             for entry in receipt['gpu_handles']] if native else []}
        if case == 'authoring':
            result = load(folder / 'authoring-result.json')
            for phase, minimum in [('typing', 83), ('brush-drag', 85), ('pan', 35), ('zoom', 55)]:
                samples = [s for s in result['samples'] if s['phase'] == phase]
                require(all(type(s['canvas_changed']) is bool for s in samples)
                        and sum(s['canvas_changed'] for s in samples) >= minimum, f'{host}: insufficient measured {phase} updates')
            data[side]['authoring_runs'].append((rep, result))
        elif case == 'canvas-profile':
            data[side]['canvas_rows'] = load(folder / 'workload.log')
        elif case.endswith('-native'):
            result = load(folder / 'native-result.json')
            keys = ('frames', 'inference_frames', 'inference_seconds', 'elapsed_seconds',
                    'ui_frame_p95_ms', 'ui_frame_max_ms')
            require(all(number(result[key]) for key in keys), 'invalid native AI statistic')
            require(result['ui_frame_p95_ms'] <= result['ui_frame_max_ms'], 'native AI p95 exceeds maximum')
            data[side]['native_ai'].setdefault(case, []).append({'repetition': rep, **{key: result[key] for key in keys}})
            data[side]['native_ai'][case][-1].update({key: result[key] for key in
                ('qa_input_protocol', 'cancel_input', 'cancel_button_covered')})
            if case == 'background_removal_qa-native':
                data[side]['native_ai'][case][-1].update({key: result[key] for key in
                    ('matte_radius_changed', 'matte_radius')})
        elif case != 'package-reopen':
            result = load(folder / ('output.json' if case.startswith('upscale') else 'result.json'))
            keys = ('seconds',) if case.startswith('upscale') else ('inference_seconds', 'refinement_seconds')
            require(all(number(result[key]) for key in keys), 'invalid AI stage timing')
            data[side]['cli_ai'].setdefault(case, []).append({'repetition': rep, 'model_sha256': result['sha256'],
                                                            **{key: result[key] for key in keys}})
    authoring, processes, canvas = {}, {}, {}
    for side in SIDES:
        data[side]['authoring_runs'].sort(key=lambda pair: pair[0])
    phases = set(s['phase'] for side in SIDES for _, run in data[side]['authoring_runs'] for s in run['samples'])
    for phase in sorted(phases):
        authoring[phase] = {'coordination_phase': phase in COORDINATION}
        for metric in METRICS:
            distributions = {}
            for side in SIDES:
                rows = [[s[metric] for s in run['samples'] if s['phase'] == phase]
                        for _, run in data[side]['authoring_runs']]
                distributions[side] = distribution(rows) if all(rows) else None
            if not all(distributions.values()):
                require(phase in COORDINATION, 'missing deterministic authoring phase')
                authoring[phase][metric] = distributions
            else:
                require(phase in COORDINATION or [r['n'] for r in distributions['baseline']['per_run']] ==
                        [r['n'] for r in distributions['candidate']['per_run']], 'authoring workload sample counts differ')
                authoring[phase][metric] = paired(distributions['baseline'], distributions['candidate'])
    for case in CASES:
        processes[case] = {}
        for metric in ('wall_seconds', 'peak_rss_KiB', 'user_seconds', 'system_seconds', 'major_page_faults'):
            values = {}
            for side in SIDES:
                receipts = sorted((r for r in data[side]['receipts'].values() if r['case'] == case), key=lambda r: r['repetition'])
                resource = {'peak_rss_KiB': 'ru_maxrss', 'user_seconds': 'ru_utime',
                            'system_seconds': 'ru_stime', 'major_page_faults': 'ru_majflt'}.get(metric)
                values[side] = distribution([[r['resources'][resource] if resource else r[metric]] for r in receipts])
            processes[case][metric] = paired(values['baseline'], values['candidate'])
    for label, condition in [('all', lambda r: True), ('warm', lambda r: r['round'] > 0),
                             ('first_full_scene', lambda r: r['round'] == 0)]:
        canvas[label] = paired(*[distribution([[r['render_ms'] for r in data[side]['canvas_rows']
                                               if r['layer'] == 'ALL' and condition(r)]]) for side in SIDES])
    geometry = {side: [{'repetition': rep, 'size': result['canvas_size']}
                       for rep, result in data[side]['authoring_runs']] for side in SIDES}
    canvas_size = {side: geometry[side][0]['size'] for side in SIDES}
    canvas_area_change = (math.prod(canvas_size['candidate']) / math.prod(canvas_size['baseline']) - 1) * 100
    selected_receipts = [receipt for side in SIDES for receipt in data[side]['receipts'].values()]
    first_measurement = min(row['started_utc'] for row in selected_receipts)
    last_measurement = max(row['finished_utc'] for row in selected_receipts)
    public = {'manifest_sha256': identity(directory / 'manifest.json')['sha256'], 'source_result_identities': hashes,
              'candidate_build_snapshot': build_snapshot,
              'continuation': continuation_proof,
              'architecture': arch, 'kernel': manifest['kernel'], 'source_revisions': manifest['source_revisions'],
              'artifacts': actual, 'runner': manifest['runner'], 'started_utc': first_measurement,
              'finished_utc': last_measurement, 'continuation_started_utc': manifest['started_utc'],
              'continuation_finished_utc': manifest['finished_utc'], 'display': manifest['display'],
              'initial_state': state(manifest['state']), 'power_profile': manifest['power_profile'],
              'graphics_packages': manifest['graphics_packages'], 'expected_drm_driver': manifest['expected_drm_driver'],
              'authoring': authoring, 'processes': processes, 'canvas_full_scene_render_ms': canvas,
              'canvas_size': canvas_size, 'canvas_geometry_per_run': geometry,
              'canvas_area_percent_change': canvas_area_change,
              'canvas_geometry_scope': 'The harness records the final canvas rectangle only. Inspector expansion can follow initial fit during text actions; no per-phase geometry or equal-zoom assertion follows. Area change describes only the final rectangle and is not used to normalize timings.',
              'native_ai': {side: data[side]['native_ai'] for side in SIDES},
              'cli_ai_stages': {side: data[side]['cli_ai'] for side in SIDES},
              'raw_process_receipts': {side: data[side]['receipts'] for side in SIDES},
              'raw_authoring_samples': {side: [{'repetition': rep, 'canvas_size': run['canvas_size'],
                                                'columns': ['phase', *METRICS, 'canvas_changed'],
                                                'rows': [[s[key] for key in ('phase', *METRICS, 'canvas_changed')]
                                                         for s in run['samples']]}
                                               for rep, run in data[side]['authoring_runs']] for side in SIDES},
              'raw_cpu_profile_samples': {side: [{'layer_index': index // 4, 'full_scene': row['layer'] == 'ALL',
                                                  **{key: row[key] for key in ('round', 'render_ms', 'convert_ms')}}
                                                 for index, row in enumerate(data[side]['canvas_rows'])] for side in SIDES}}
    return public


def historical_table(root):
    result = {}
    for host in HOSTS:
        directory = root / host
        require(read(directory / 'complete.json')['success'] is True, 'historical suite incomplete')
        authoring = [read(directory / f'authoring-{r}/authoring-result.json') for r in (1, 2)]
        resources = {}
        for path in directory.glob('*-resources.json'):
            receipt = read(path)
            require(receipt['returncode'] == 0 and receipt['timed_out'] is False, 'historical process failed')
            resources[path.name.removesuffix('-resources.json')] = {key: receipt[key] for key in
                ('wall_seconds', 'peak_rss_KiB', 'user_seconds', 'system_seconds', 'major_page_faults')}
        profile = read(directory / 'canvas-profile.log')
        result[host] = {'authoring': {phase: {metric: distribution([[s[metric] for s in run['samples']
                                    if s['phase'] == phase] for run in authoring]) for metric in METRICS} for phase in PHASES},
                        'whole_process_resources': resources,
                        'warm_canvas_render_ms': stats([row['render_ms'] for row in profile if row['layer'] == 'ALL' and row['round'] > 0]),
                        'native_ai_reported': {case: {key: read(directory / case / 'native-result.json')[key]
                                                    for key in ('frames', 'ui_frame_p95_ms', 'ui_frame_max_ms')}
                                               for case in ('background_removal_qa-native', 'upscale_qa-native')},
                        'source_result_identities': {f'authoring-{r}/authoring-result.json': identity(directory / f'authoring-{r}/authoring-result.json')
                                                     for r in (1, 2)}}
    return {'scope': 'Original 2026-09-27 desktop measurements, retained separately. Never pooled with fresh private-Sway measurements and not used for fresh software speedup claims.',
            'hosts': result}


def claims(hosts):
    result = []
    for host, data in hosts.items():
        for phase in ('brush-drag', *(phase for phase in PHASES if phase != 'brush-drag')):
            value = data['authoring'][phase]['ui_ms']
            old, new = value['baseline'], value['candidate']
            factor = value['speedup_old_over_new']['median']
            ratio_text = f'{factor:.2f}× old/new' if factor is not None else 'undefined ratio (zero candidate duration)'
            result.append({'host': host, 'task': phase, 'metric': 'median Studio::ui CPU wall milliseconds',
                           'baseline': old['pooled']['median'], 'candidate': new['pooled']['median'],
                           'speedup_old_over_new': factor, 'direction': ('undefined' if factor is None else
                               'lower' if factor > 1 else 'higher' if factor < 1 else 'unchanged'),
                           'baseline_run_median_range': old['run_median_range'], 'candidate_run_median_range': new['run_median_range'],
                           'baseline_p95': old['pooled']['p95'], 'candidate_p95': new['pooled']['p95'],
                           'p95_speedup_old_over_new': value['speedup_old_over_new']['p95'],
                           'samples_per_side': {side: value[side]['pooled']['n'] for side in SIDES},
                           'claim_text': f'On {host}, {phase} median UI duration measured {old["pooled"]["median"]:.3f} ms with the retained issue #158 baseline and {new["pooled"]["median"]:.3f} ms with the candidate ({ratio_text}).',
                           'canvas_size': data['canvas_size'],
                           'canvas_area_percent_change': data['canvas_area_percent_change'],
                           'qualification': f'Two as-used runs; same 1440×900 outer window. Final recorded canvas baseline {data["canvas_size"]["baseline"][0]}×{data["canvas_size"]["baseline"][1]}, candidate {data["canvas_size"]["candidate"][0]}×{data["canvas_size"]["candidate"][1]} ({data["canvas_area_percent_change"]:+.3f}% final area). Per-phase geometry and equal zoom are not asserted; timings are not normalized by area. Not a shipped-v0.6.0 comparison, FPS claim, or statistical significance claim.'})
    return result


def core_ai_identity(repository, candidate):
    trees = {}
    for side, revision in [('baseline', BASELINE), ('candidate', candidate)]:
        output = subprocess.check_output(['git', 'ls-tree', '-r', revision, '--', *CORE_AI],
                                         cwd=repository, text=True)
        trees[side] = {line.split('\t', 1)[1]: line.split()[2] for line in output.splitlines()}
        require(trees[side], 'core AI source provenance unavailable')
    return {'source_revisions': {'baseline': BASELINE, 'candidate': candidate},
            'git_blob_identities': trees, 'identical': trees['baseline'] == trees['candidate'],
            'scope': 'Core background removal, upscaling and ML source files; UI, harness and dependencies are not covered by this source equality check.'}


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--root', type=Path, required=True, help='contains one run_paired output directory per host')
    parser.add_argument('--candidate-build', type=Path, required=True)
    parser.add_argument('--inputs', type=Path, required=True, help='original retained inputs, verified against every host manifest')
    parser.add_argument('--historical', type=Path, default=BASELINE_DOCS)
    parser.add_argument('--repository', type=Path, default=Path(__file__).resolve().parents[3])
    parser.add_argument('--output', type=Path, required=True)
    args = parser.parse_args()
    require(not args.output.exists(), 'output exists; retain old summaries and choose a new filename')
    build = read(args.candidate_build)
    require(build['success'] is True and all(step['exit_code'] == 0 for step in build['steps']), 'candidate build did not pass')
    runner_proof = verify_runner_sources()
    schema_defaults = verify_schema_defaults(args.repository, build['source_commit'])
    inputs = inventory(args.inputs)
    directories = {host: args.root / host for host in HOSTS}
    host_data = {host: summarize_host(host, directories[host], args.historical, build, inputs, runner_proof,
                                    args.repository) for host in HOSTS}
    for category in ('baseline', 'candidate', 'runtime'):
        require(host_data['hpeliteclient']['artifacts'][category] == host_data['intelpro']['artifacts'][category],
                f'x86 {category} identities differ')
    for case in ('background_removal_qa-native', 'upscale_qa-native'):
        source_maps = [host_data[host]['continuation']['native_overrides'][side][case]['harness_source_identities']
                       for host in HOSTS for side in SIDES]
        require(all(value == source_maps[0] for value in source_maps), 'native QA protocol source differs across hosts/versions')
    seed = (args.inputs / 'infographic-seed.oma').read_bytes()
    quality = correctness(directories, seed)
    summary = {'schema_version': 1, 'created_utc': datetime.now(timezone.utc).isoformat(),
               'completed_processes': 108, 'functional_validation_passed': True,
               'process_scope': 'Selected successful measurements only. Prior failed and superseded attempts remain separately preserved in continuation lineage.',
               'all_recorded_attempts_passed': all(host_data[host]['continuation']['all_recorded_attempts_passed'] for host in HOSTS),
               'baseline_source': BASELINE, 'candidate_source': build['source_commit'],
               'candidate_build_receipt': identity(args.candidate_build), 'input_artifacts': inputs,
               'runner_provenance': runner_proof,
               'current_schema_default_source_proof': schema_defaults,
               'core_ai_source_identity': core_ai_identity(args.repository, build['source_commit']),
               'tool_identities': {name: identity(Path(__file__).with_name(name)) for name in
                                   ('summarize_paired.py', 'run_paired.py', 'continue_paired.py',
                                    'audit_continuation.py', 'document_id_equivalence.py',
                                    'schema_defaults_equivalence.py')},
               'method': {'median': 'conventional', 'p95': 'nearest rank ceil(.95*n)', 'speedup': 'old duration / new duration; values below 1 indicate slower candidate',
                          'order': 'Serial per host, sides alternate order for successive case/repetition pairs',
                          'process_polling_ms': 100, 'ai_process_repetitions': 2,
                          'small_sample_limit': 'A process p95 from two observations is their maximum, not a stable tail estimate. Short AI times are sensitive to 100ms polling granularity.',
                          'native_ai_limit': 'Native-AI UI p95/max are per-run harness-reported statistics under the common recovery protocol. The helper uses sorted[floor(.95*n)] with zero-based indexing, rather than the nearest-rank rule used for raw authoring samples. Raw AI UI samples are unavailable; no pooled percentile or AI-UI speedup is reconstructed.',
                          'native_recovery_protocol': NATIVE_PROTOCOL,
                          'native_cancel_input': 'Escape key', 'native_cancel_button_covered': False,
                          'native_recovery_scope': 'All native AI repetitions are rerun with identical helper source on both versions/all hosts. Semantic buttons use hover plus atomic clicks; coordinate sliders retain held press/release frames. Original native attempts remain separate. Authoring and completed CLI measurements are retained byte-for-byte.',
                          'cpu_profile_order': 'Each individual layer is rendered four times before full-scene rounds0–3. Full-scene round0 is not a cold-process/cache measurement.',
                          'scope': 'As-used same-host paired comparison of retained issue baseline and final candidate. Original historical desktop results remain separate. No presented FPS or shipped-v0.6.0 claim.'},
               'hosts': host_data, 'correctness': quality, 'historical_separate': historical_table(args.historical),
               'claim_candidates': claims(host_data),
               'performance_acceptance': 'Measured ratios, all regressions and per-run variation remain visible. The output review documents verified schema defaults, consistent IDs, quantified authoring pixel differences and pre-existing cross-architecture AI differences without claiming universal or pixel-identical behavior.'}
    payload = json.dumps(summary, indent=2, allow_nan=False) + '\n'
    require(not any(token in payload for token in ('/home/', '/run/user/', 'data:image/', 'base64')),
            'private path or encoded artwork leaked into summary')
    args.output.parent.mkdir(parents=True, exist_ok=True)
    with args.output.open('x') as stream:
        stream.write(payload)
    print(json.dumps({'summarized': True, 'processes': 108, 'image_pairs': quality['image_pairs'],
                      'nonidentical_image_pairs': quality['image_pairs'] - quality['equal_image_pairs'],
                      'all_documents_exact_or_id_equivalent': quality['all_documents_exact_or_id_equivalent'],
                      'summary': identity(args.output)}))


if __name__ == '__main__':
    main()
