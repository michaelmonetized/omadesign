"""Synthetic orchestration tests. No Omadesign process or display is started."""
import copy
import importlib.util
import json
from pathlib import Path
import platform
from types import SimpleNamespace
import tempfile
import unittest
from unittest import mock

HERE = Path(__file__).resolve().parent
spec = importlib.util.spec_from_file_location('continuation', HERE / 'continue_paired.py')
c = importlib.util.module_from_spec(spec)
spec.loader.exec_module(c)
spec = importlib.util.spec_from_file_location('frozen_runner', HERE / 'run_paired.py')
r = importlib.util.module_from_spec(spec)
spec.loader.exec_module(r)


class FixtureRunner:
    CASES = r.CASES
    DISPLAY_KEYS = r.DISPLAY_KEYS

    def __init__(self):
        self.calls = []

    def validate(self, case, directory, log, canvas):
        if (directory / 'artifact.txt').read_text() != 'fixture output\n':
            raise ValueError('invalid synthetic output')

    def inventory(self, directory):
        return {}

    def display_state(self, env):
        return []

    def command(self, argv):
        return {'exit_code': 0}

    def state(self):
        return {'fixture': True}

    def run_one(self, args, side, case, repetition, env, manifest):
        self.calls.append((side, case, repetition))
        directory = args.output / c.run_path(side, case, repetition)
        directory.mkdir(parents=True)
        for folder in ('config', 'cache', 'data', 'state'):
            (args.output / 'profiles' / side / f'{case}-{repetition}' / folder).mkdir(parents=True)
        (directory / 'artifact.txt').write_text('fixture output\n')
        c.write(directory / 'receipt.json', {'passed': True, 'exit_code': 0, 'timed_out': False, 'errors': [],
                                           'wall_seconds': .5, 'resources': {'ru_utime': .2, 'ru_maxrss': 1024},
                                           'gpu_handles': [{'drm-driver': 'asahi'}]})
        manifest['runs'].append({'side': side, 'case': case, 'repetition': repetition,
                                'receipt': str((directory / 'receipt.json').relative_to(args.output)), 'passed': True})


class ContinuationTests(unittest.TestCase):
    def setUp(self):
        self.temporary = tempfile.TemporaryDirectory()
        self.root = Path(self.temporary.name)
        self.parent = self.root / 'parent'
        self.parent.mkdir()
        self.runner = FixtureRunner()
        self.loader = mock.patch.object(c, 'load_runner', return_value=self.runner)
        self.loader.start()
        self.addCleanup(self.loader.stop)
        self.addCleanup(self.temporary.cleanup)

    def fixture(self, failed=None):
        (self.root / 'swaymsg').write_text('synthetic query tool; never executed\n')
        manifest = {'architecture': platform.machine(), 'ai_rounds': 2, 'display': [],
                    'expected_drm_driver': 'asahi', 'finished_utc': 'fixture end',
                    'success': failed is None, 'errors': [] if failed is None else ['fixture failure'],
                    'source_revisions': {'baseline': 'a' * 40, 'candidate': 'b' * 40},
                    'swaymsg': c.identity(self.root / 'swaymsg'),
                    'artifacts': {name: {} for name in ('baseline', 'candidate', 'inputs', 'runtime')},
                    'runs': []}
        for item in c.schedule(self.runner, 2):
            side, case, repetition = item
            directory = self.parent / c.run_path(*item)
            directory.mkdir(parents=True)
            passed = item != failed
            receipt = {'passed': passed, 'errors': [] if passed else ['fixture failure'],
                       'exit_code': 0 if passed else 101, 'timed_out': False, 'wall_seconds': .5,
                       'resources': {'ru_utime': .2, 'ru_maxrss': 1024},
                       'gpu_handles': [{'drm-driver': 'asahi'}]}
            c.write(directory / 'receipt.json', receipt)
            (directory / 'artifact.txt').write_text('fixture output\n')
            (directory / 'workload.log').write_text('fixture log\n')
            profile = self.parent / 'profiles' / side / f'{case}-{repetition}' / 'config'
            profile.mkdir(parents=True)
            (profile / 'settings').write_text('fixture profile\n')
            manifest['runs'].append({'side': side, 'case': case, 'repetition': repetition,
                                    'receipt': str((directory / 'receipt.json').relative_to(self.parent)),
                                    'passed': passed})
        c.write(self.parent / 'manifest.json', manifest)
        return manifest

    def prepare(self, **kwargs):
        args = SimpleNamespace(parent=self.parent, output=self.root / 'derived',
                               runner=HERE / 'run_paired.py', native_overrides=None,
                               rerun_native=False, **kwargs)
        c.prepare(args)
        return args.output, c.read(args.output / 'manifest.json')

    def test_retained_bytes_and_parent_are_immutable_and_tampering_is_rejected(self):
        failed = ('candidate', 'background_removal_qa-native', 1)
        self.fixture(failed)
        before = c.inventory(self.parent)
        output, derived = self.prepare()
        self.assertEqual(c.inventory(self.parent), before)
        self.assertEqual(len(derived['runs']), 35)
        self.assertEqual(len(derived['continuation']['excluded']), 1)
        excluded = derived['continuation']['excluded'][0]
        original = self.parent / c.run_path(*failed) / 'receipt.json'
        saved = output / excluded['archived_run_path'] / 'receipt.json'
        self.assertEqual(original.read_bytes(), saved.read_bytes())
        self.assertNotEqual(original.stat().st_ino, saved.stat().st_ino)
        retained = output / derived['runs'][0]['origin']['parent_run_path'] / 'artifact.txt'
        retained.write_text('tampered\n')
        with self.assertRaisesRegex(ValueError, 'retained output changed'):
            c.verify_lineage(output, derived)
        self.assertEqual(c.inventory(self.parent), before)

    def test_matched_native_rerun_excludes_all_eight_original_native_runs(self):
        self.fixture()
        args = SimpleNamespace(parent=self.parent, output=self.root / 'derived',
                               runner=HERE / 'run_paired.py', native_overrides=None, rerun_native=True)
        c.prepare(args)
        derived = c.read(args.output / 'manifest.json')
        self.assertEqual(len(derived['runs']), 28)
        self.assertEqual(len(derived['continuation']['excluded']), 8)
        self.assertEqual({entry['case'] for entry in derived['continuation']['excluded']}, set(c.NATIVE))
        self.assertTrue(all(entry['reason'] == 'superseded for matched native protocol'
                            for entry in derived['continuation']['excluded']))
        self.assertTrue(all(entry['case'] not in c.NATIVE for entry in derived['runs']))

    def test_execution_only_recovers_failed_case_and_never_claims_all_attempts_passed(self):
        failed = ('baseline', 'upscale_qa-native', 2)
        self.fixture(failed)
        before = c.inventory(self.parent)
        output, derived = self.prepare()
        architecture = platform.machine()
        steps = ['check-all-targets', 'library-tests', architecture + '-portable-release']
        if architecture == 'aarch64':
            steps.append('native-qa-release')
        build = self.root / 'build.json'
        c.write(build, {'success': True, 'source_commit': 'b' * 40, 'artifacts': {},
                        'steps': [{'name': name, 'exit_code': 0} for name in steps]})
        private = Path('/run/user/43210/omadesign-fleet-0.6.1')
        display = self.root / 'display.json'
        c.write(display, {'XDG_RUNTIME_DIR': str(private), 'SWAYSOCK': str(private / 'sway.sock'),
                          'WAYLAND_DISPLAY': 'wayland-1'})
        args = SimpleNamespace(output=output, baseline_bin=self.root / 'baseline', candidate_bin=self.root / 'candidate',
                               inputs=self.root / 'inputs', runtime=self.root / 'runtime', candidate_build=build,
                               display_env=display, swaymsg=self.root / 'swaymsg',
                               expected_drm_driver='asahi', timeout=1200)
        real_stat = Path.stat
        def path_stat(path, *args, **kwargs):
            if path == private:
                return SimpleNamespace(st_uid=43210, st_mode=0o40700)
            return real_stat(path, *args, **kwargs)
        with mock.patch.object(c.os, 'getuid', return_value=43210), \
                mock.patch.object(Path, 'stat', path_stat), mock.patch.dict(c.os.environ, {}, clear=True):
            c.execute(args)
        final = c.read(output / 'manifest.json')
        self.assertEqual(self.runner.calls, [failed])
        self.assertEqual(len(final['runs']), 36)
        self.assertTrue(final['success'])
        self.assertFalse(final['all_attempts_passed'])
        recovered = final['runs'][-1]
        self.assertEqual(recovered['origin']['attempt_index'], 2)
        self.assertEqual(recovered['origin']['kind'], 'recovered')
        self.assertEqual(c.inventory(self.parent), before)
        c.validate_continuation(output)
        changed = copy.deepcopy(final)
        changed['runs'][-1]['origin']['attempt_index'] = 17
        with self.assertRaisesRegex(ValueError, 'differs from its origin'):
            c.verify_lineage(output, changed)
        changed = copy.deepcopy(final)
        changed['runs'].pop()
        with self.assertRaisesRegex(ValueError, 'differ from retained/recovered lineage'):
            c.verify_lineage(output, changed)
        artifact = output / c.run_path(*failed) / 'artifact.txt'
        original_bytes = artifact.read_bytes()
        artifact.write_text('changed recovered artifact\n')
        with self.assertRaisesRegex(ValueError, 'recovered output changed'):
            c.verify_lineage(output, final)
        artifact.write_bytes(original_bytes)
        # Re-preparation keeps attempt numbers rather than resetting history.
        newer = self.root / 'newer'
        c.prepare(SimpleNamespace(parent=output, output=newer, runner=HERE / 'run_paired.py',
                                  native_overrides=None, rerun_native=False))
        retained = next(entry for entry in c.read(newer / 'manifest.json')['runs'] if c.key(entry) == failed)
        self.assertEqual(retained['origin']['attempt_index'], 2)

    def test_unsealed_parent_and_existing_destination_are_rejected(self):
        self.fixture()
        manifest = c.read(self.parent / 'manifest.json')
        del manifest['finished_utc']
        c.write(self.parent / 'manifest.json', manifest)
        with self.assertRaisesRegex(ValueError, 'sealed'):
            self.prepare()
        (self.root / 'derived').mkdir()
        with self.assertRaisesRegex(ValueError, 'must be new'):
            self.prepare()

    def test_native_override_pins_builds_and_requires_matching_harness_source(self):
        manifest = self.fixture()
        config = {'schema_version': 1, 'protocol': 'synthetic driver protocol', 'sides': {}}
        for side in ('baseline', 'candidate'):
            config['sides'][side] = {}
            for case in c.NATIVE:
                directory = self.root / 'override' / side / case
                directory.mkdir(parents=True)
                executable = directory / 'binary'
                executable.write_text('synthetic fixture; never executed\n' + side)
                executable.chmod(0o700)
                source = directory / 'native.rs'
                source.write_text('synthetic shared driver source\n' + case)
                sources = {'native.rs': {'path': str(source), 'identity': c.identity(source)}}
                build = directory / 'build.json'
                c.write(build, {'success': True, 'production_commit': manifest['source_revisions'][side],
                                'architecture': manifest['architecture'], 'artifacts': {'binary': c.identity(executable)},
                                'steps': [{'name': 'synthetic compile proof', 'exit_code': 0}],
                                'harness_sources': {'native.rs': c.identity(source)}})
                config['sides'][side][case] = {
                    'path': str(executable), 'identity': c.identity(executable),
                    'production_revision': manifest['source_revisions'][side],
                    'build_receipt': {'path': str(build), 'identity': c.identity(build), 'artifact_key': 'binary'},
                    'harness_sources': sources,
                }
        c.check_overrides(config, manifest)
        configuration = self.root / 'overrides.json'
        c.write(configuration, config)
        args = SimpleNamespace(parent=self.parent, output=self.root / 'derived',
                               runner=HERE / 'run_paired.py', native_overrides=configuration, rerun_native=False)
        c.prepare(args)
        prepared = c.read(args.output / 'manifest.json')
        self.assertTrue(prepared['continuation']['rerun_all_native_ai'])
        self.assertEqual(len(prepared['continuation']['excluded']), 8)
        evidence = prepared['continuation']['native_overrides']['evidence']['baseline/' + c.NATIVE[0]]
        copied_source = args.output / evidence['sources'] / 'native.rs'
        copied_source.write_text('changed copied source\n')
        with self.assertRaisesRegex(ValueError, 'copied override source drift'):
            c.verify_lineage(args.output, prepared)
        copied_source.write_bytes(Path(config['sides']['baseline'][c.NATIVE[0]]['harness_sources']['native.rs']['path']).read_bytes())
        changed = copy.deepcopy(config)
        changed['sides']['candidate'][c.NATIVE[0]]['production_revision'] = 'c' * 40
        with self.assertRaisesRegex(ValueError, 'production revision'):
            c.check_overrides(changed, manifest)
        value = config['sides']['candidate'][c.NATIVE[0]]
        source = Path(value['harness_sources']['native.rs']['path'])
        source.write_text('different action protocol\n')
        value['harness_sources']['native.rs']['identity'] = c.identity(source)
        build = Path(value['build_receipt']['path'])
        receipt = c.read(build)
        receipt['harness_sources']['native.rs'] = c.identity(source)
        c.write(build, receipt)
        value['build_receipt']['identity'] = c.identity(build)
        with self.assertRaisesRegex(ValueError, 'driver source differs'):
            c.check_overrides(config, manifest)

    def test_profile_only_partial_attempt_is_archived_and_checked(self):
        manifest = self.fixture()
        item = ('baseline', 'package-reopen', 1)
        import shutil
        shutil.rmtree(self.parent / c.run_path(*item))
        manifest['runs'] = [entry for entry in manifest['runs'] if c.key(entry) != item]
        c.write(self.parent / 'manifest.json', manifest)
        output, derived = self.prepare()
        excluded = derived['continuation']['excluded'][0]
        self.assertIsNone(excluded['archived_run_path'])
        self.assertEqual(excluded['artifact_inventory'], {})
        profile = output / excluded['archived_profile_path'] / 'config/settings'
        self.assertEqual(profile.read_text(), 'fixture profile\n')
        profile.write_text('mutated profile\n')
        with self.assertRaisesRegex(ValueError, 'excluded profile changed'):
            c.verify_lineage(output, derived)

    def test_native_protocol_requires_actual_cancel_and_slider_metadata(self):
        result = {'qa_input_protocol': 'fixture protocol', 'cancel_input': 'Escape key',
                  'cancel_button_covered': False, 'matte_radius_changed': True, 'matte_radius': 20}
        target = self.root / 'native-result.json'
        c.write(target, result)
        c.validate_native_protocol(self.root, 'fixture protocol', c.NATIVE[0])
        with self.assertRaisesRegex(ValueError, 'protocol differs'):
            c.validate_native_protocol(self.root, 'another protocol', c.NATIVE[0])
        for update in ({'cancel_button_covered': True}, {'matte_radius_changed': False}, {'matte_radius': 12}):
            c.write(target, {**result, **update})
            with self.assertRaises(ValueError):
                c.validate_native_protocol(self.root, 'fixture protocol', c.NATIVE[0])


if __name__ == '__main__':
    unittest.main()
