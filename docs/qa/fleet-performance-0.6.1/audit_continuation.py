"""Read-only provenance checks for selected runs after native QA recovery."""
import hashlib
import json
from pathlib import Path
import subprocess

import continue_paired as continuation
from document_id_equivalence import strict_load

NATIVE = ('background_removal_qa-native', 'upscale_qa-native')
PROTOCOL = 'hover-atomic-buttons-held-sliders-escape-cancel-v2'
CONTINUATION_SHA256 = '6f598be2b8aebf17032432d68b432f286f904e01b199d81b286f899590c5c2ba'
BASELINE = '221834bdd8af5aee8554a0e835629cbe2f467558'
REBUILD_CHECKOUT = '587433037d1df9be5e66b36cc77bc6f44fcd7071'
TREE_SHA256 = '7590d7cc3c119079ef0f29ee320e07ace467aa3f22ee282236238f74a8d30dfd'


def require(value, message):
    if not value:
        raise ValueError(message)


def read(path):
    return strict_load(path.read_bytes())


def identity(path):
    digest = hashlib.sha256()
    with path.open('rb') as stream:
        for block in iter(lambda: stream.read(1024 * 1024), b''):
            digest.update(block)
    return {'bytes': path.stat().st_size, 'sha256': digest.hexdigest()}


def snapshot(directory, proof):
    path = directory / continuation.relative(proof['path'])
    require(identity(path) == proof['identity'], 'continuation snapshot identity changed')
    return read(path)


def audit_baseline_source_equivalence(proof, repository):
    excluded = ['docs/', 'src/bin/authoring_qa.rs', 'src/bin/canvas_profile.rs']
    require(proof['baseline_production_revision'] == BASELINE
            and proof['rebuild_checkout_revision'] == REBUILD_CHECKOUT
            and proof['tracked_source_tree_equal'] is True
            and proof['rebuild_checkout_clean_at_verification'] is True
            and proof['excluded_paths'] == excluded, 'baseline rebuild source proof scope differs')
    def omitted(path):
        return path.startswith('docs/') or path in excluded[1:]
    def tree(revision):
        data = subprocess.check_output(['git', 'ls-tree', '-rz', '--full-tree', revision], cwd=repository)
        rows = []
        for row in data.split(b'\0'):
            if row:
                meta, raw_path = row.split(b'\t', 1)
                path = raw_path.decode()
                if not omitted(path):
                    rows.append((path, meta.decode()))
        return rows
    original, rebuild = tree(BASELINE), tree(REBUILD_CHECKOUT)
    digest = hashlib.sha256(json.dumps(original, separators=(',', ':')).encode()).hexdigest()
    require(original == rebuild and len(original) == proof['tracked_entries_compared'] == 825
            and digest == proof['canonical_tracked_tree_sha256'] == TREE_SHA256,
            'baseline rebuild production input tree differs')
    changes = subprocess.check_output(['git', 'diff', '--name-only', BASELINE, REBUILD_CHECKOUT],
                                      cwd=repository).decode().splitlines()
    require(changes == proof['commit_diff_paths'] and all(omitted(path) for path in changes),
            'baseline rebuild changed paths differ from the explicit exclusions')
    return {'verified_against_repository': True, **proof}


def audit(directory, manifest, completed_build, expected_runner, expected_parent_runner, repository):
    lineage = manifest['continuation']
    require(manifest.get('status') == 'complete' and lineage['schema_version'] == 1
            and lineage['rerun_all_native_ai'] is True, 'matched native continuation incomplete')
    require(lineage['execution_runner']['identity'] == expected_runner,
            'continuation execution runner differs from its assigned frozen source')
    continuation_identity = identity(Path(continuation.__file__))
    require(continuation_identity['sha256'] == CONTINUATION_SHA256
            and lineage['continuation_tool']['identity'] == continuation_identity,
            'continuation tool differs from the published audited source')
    # Recomputes the original parent, retained, excluded and recovered evidence.
    parent = continuation.verify_lineage(directory, manifest)
    require(parent['runner'] == expected_parent_runner, 'original parent runner differs from its assigned frozen source')
    require(snapshot(directory, lineage['execution_build']) == completed_build,
            'continuation did not use the completed candidate build receipt')
    require(type(manifest['all_attempts_passed']) is bool, 'missing attempt-history status')
    if not parent['success'] or parent.get('all_attempts_passed') is False:
        require(manifest['all_attempts_passed'] is False, 'failed parent history was relabeled passing')

    override_proof = lineage['native_overrides']
    require(override_proof and override_proof['protocol'] == PROTOCOL, 'unknown native input protocol')
    config = snapshot(directory, override_proof)
    require(config['schema_version'] == 1 and config['protocol'] == PROTOCOL
            and set(config['sides']) == {'baseline', 'candidate'}, 'invalid native override configuration')
    public_overrides = {}
    for side in ('baseline', 'candidate'):
        require(set(config['sides'][side]) == set(NATIVE), 'native case override missing')
        public_overrides[side] = {}
        for case in NATIVE:
            spec = config['sides'][side][case]
            evidence = override_proof['evidence'][f'{side}/{case}']
            build_path = directory / continuation.relative(evidence['build_receipt'])
            require(identity(build_path) == spec['build_receipt']['identity'], 'native build receipt changed')
            build = read(build_path)
            require(spec['production_revision'] == manifest['source_revisions'][side]
                    and build['production_commit'] == spec['production_revision']
                    and build['architecture'] == manifest['architecture'] and build.get('success') is True,
                    'native override production/architecture/build mismatch')
            require(build['steps'] and all(step['exit_code'] == 0 for step in build['steps']),
                    'native override build steps failed')
            require(build['artifacts'][spec['build_receipt']['artifact_key']] == spec['identity']
                    == evidence['executable_identity'], 'native executable is not the recorded build artifact')
            source_ids = {name: value['identity'] for name, value in spec['harness_sources'].items()}
            require(source_ids and source_ids == build['harness_sources'], 'native source/build mismatch')
            require(continuation.inventory(directory / continuation.relative(evidence['sources'])) == source_ids,
                    'native source snapshot changed')
            receipt_fields = ['raw_relink_receipt', 'source_snapshot_receipt']
            if side == 'baseline':
                receipt_fields += ['baseline_source_equivalence', 'baseline_library_build']
            for field in receipt_fields:
                data = (json.dumps(build[field], indent=2, allow_nan=False) + '\n').encode()
                require({'bytes': len(data), 'sha256': hashlib.sha256(data).hexdigest()} == build[field + '_identity'],
                        'embedded native provenance differs from its original file identity')
            source_snapshot = build['source_snapshot_receipt']
            require(source_snapshot['protocol'] == PROTOCOL
                    and {name: {'bytes': value['bytes'], 'sha256': value['corrected_sha256']}
                         for name, value in source_snapshot['files'].items()} == source_ids,
                    'native frozen source receipt differs from compiled sources')
            public_overrides[side][case] = {
                'production_revision': spec['production_revision'], 'executable_identity': spec['identity'],
                'build_receipt_identity': spec['build_receipt']['identity'], 'harness_source_identities': source_ids,
                'successful_build_steps': [step['name'] for step in build['steps']],
                'compiler': {key: build[key] for key in ('rustc', 'cargo') if key in build},
                'embedded_provenance_identities': {field: build[field + '_identity'] for field in receipt_fields},
                'linked_library_identities': {name: {key: value[key] for key in ('bytes', 'sha256')}
                                               for name, value in build['externs_independently_verified'].items()},
                'source_snapshot': source_snapshot,
            }
            if side == 'baseline':
                source_proof = audit_baseline_source_equivalence(build['baseline_source_equivalence'], repository)
                library = build['baseline_library_build']
                require(library['product_source'] == BASELINE and library['checkout'] == REBUILD_CHECKOUT
                        and library.get('success') is True and library['steps']
                        and all(step['exit_code'] == 0 for step in library['steps'])
                        and manifest['architecture'] in {step['architecture'] for step in library['steps']},
                        'baseline library rebuild receipt does not prove this production/architecture')
                require(library['rustc'] == build['rustc'], 'baseline library/relink compiler differs')
                public_overrides[side][case]['baseline_source_equivalence'] = source_proof
                public_overrides[side][case]['baseline_library_build'] = {
                    'product_source': library['product_source'], 'checkout': library['checkout'],
                    'rustc': library['rustc'],
                    'successful_architectures': [step['architecture'] for step in library['steps']],
                }
    for case in NATIVE:
        require(public_overrides['baseline'][case]['harness_source_identities']
                == public_overrides['candidate'][case]['harness_source_identities'],
                'baseline/candidate native harness source differs')

    retained = {continuation.key(row): row for row in lineage['retained']}
    recovered = {continuation.key(row): row for row in lineage['new_attempts']}
    require(len(retained) == len(lineage['retained']) and len(recovered) == len(lineage['new_attempts'])
            and not retained.keys() & recovered.keys(), 'duplicate/overlapping selected origins')
    selected = {continuation.key(row): row for row in manifest['runs']}
    require(len(selected) == len(manifest['runs']) and selected.keys() == retained.keys() | recovered.keys(),
            'selected cases do not exactly cover retained and recovered origins')
    public_runs = []
    for key, row in selected.items():
        side, case, repetition = key
        origin = row['origin']
        expected = retained[key] if key in retained else recovered[key]
        expected_origin = {name: value for name, value in expected.items()
                           if name not in ('side', 'case', 'repetition', 'receipt', 'passed')}
        require(origin == expected_origin and origin['parent_manifest_sha256']
                == lineage['parent_manifest']['identity']['sha256'], 'selected origin metadata differs')
        path = directory / continuation.run_path(*key)
        require(row['receipt'] == str(continuation.run_path(*key) / 'receipt.json')
                and identity(path / 'receipt.json') == origin['receipt_identity']
                and continuation.inventory(path) == origin['artifact_inventory'],
                'selected receipt/output inventory changed')
        receipt = read(path / 'receipt.json')
        require(row['passed'] is True and receipt['passed'] is True and receipt['exit_code'] == 0
                and receipt['timed_out'] is False and not receipt['errors'], 'selected attempt did not pass')
        if case in NATIVE:
            require(origin['kind'] == 'recovered' and origin['native_protocol'] == PROTOCOL
                    and origin['native_override'] == config['sides'][side][case], 'unmatched native protocol cohort')
            require(receipt['command'][:2] == ['unshare', '-Urn']
                    and Path(receipt['command'][2]) == Path(origin['native_override']['path']),
                    'native receipt did not execute the explicit override')
            result = read(path / 'native-result.json')
            require(result.get('qa_input_protocol') == PROTOCOL and result.get('cancel_input') == 'Escape key'
                    and result.get('cancel_button_covered') is False, 'native cancel/input coverage metadata differs')
            if case == 'background_removal_qa-native':
                radius = result.get('matte_radius')
                require(result.get('matte_radius_changed') is True and type(radius) is int
                        and radius > 12, 'native matte Radius did not change from initial12')
        elif origin['kind'] == 'recovered':
            require(origin['native_override'] is None and origin['native_protocol'] is None,
                    'native override applied to a different workload')
        if origin['kind'] == 'recovered':
            require(origin['execution_runner_sha256'] == expected_runner['sha256'], 'recovered runner differs')
        public_runs.append({'side': side, 'case': case, 'repetition': repetition,
                            'origin': origin['kind'], 'attempt_index': origin['attempt_index'],
                            'receipt_identity': origin['receipt_identity'],
                            'artifact_inventory': origin['artifact_inventory']})
    require(sum(row['case'] in NATIVE for row in public_runs) == 8, 'native selected matrix incomplete')
    return {'scope': 'Selected successful processes only; retained measurements are byte-identical to their parent. Failed/superseded attempts remain separate and are not relabeled passing.',
            'all_recorded_attempts_passed': manifest['all_attempts_passed'],
            'parent_manifest_identity': lineage['parent_manifest']['identity'],
            'parent_inventory_identity': lineage['parent_inventory']['identity'],
            'parent_success': parent['success'], 'parent_error_count': len(parent['errors']),
            'original_parent_runner_identity': parent['runner'],
            'execution_runner_identity': lineage['execution_runner']['identity'],
            'continuation_tool_identity': lineage['continuation_tool']['identity'],
            'completed_execution_build_identity': lineage['execution_build']['identity'],
            'native_protocol': PROTOCOL, 'cancel_input': 'Escape key', 'cancel_button_covered': False,
            'native_override_config_identity': override_proof['identity'], 'native_overrides': public_overrides,
            'retained_count': len(retained), 'recovered_count': len(recovered), 'selected_origins': public_runs,
            'excluded_attempts': [{key: row[key] for key in ('side', 'case', 'repetition', 'reason',
                                                            'receipt_identity', 'artifact_inventory')}
                                  for row in lineage['excluded']]}
