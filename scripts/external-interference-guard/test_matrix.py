"""Independent Linux-safe full12 matrix contract tests.

Only synthetic JSON/bytes are created. No Mac/RCam/display/capture is launched.
Place this source file next to the external matrix module under review.
"""
import copy
import hashlib
import json
from pathlib import Path
import signal
import tempfile
import unittest
import uuid
import zipfile
from types import SimpleNamespace
from unittest.mock import patch

import matrix

MANIFEST = 'b' * 64
BINARY = 'c' * 64
PRODUCER = 'd' * 64
RUNNER = 'e' * 64
COMMIT = 'a' * 40
LEDGER = 'f' * 64
EXPECTED_CASES = (
    ('nav', 1), ('nav', 2), ('nav', 3), ('move', 1), ('move', 2),
    ('move', 3), ('points', 1), ('escape', 1), ('new-project', 1),
    ('workflow', 1), ('workflow-reopen', 1), ('workflow-cross-layer', 1),
)
PROJECT_BYTES = b'unit-only synthetic project; never native acceptance\n'


def sha(data):
    return hashlib.sha256(data).hexdigest()


def dump(path, value):
    path.write_text(json.dumps(value, allow_nan=False) + '\n')


def synthetic_display_validators(root):
    """Only adapter wiring; real product diagnostic semantics have their own suite."""
    def receipt(value, target, probe=True):
        if target != 2: raise RuntimeError('unit-only wrong target')
        for moment in ('before', 'after'):
            matrix.guard.display_snapshot(value[moment], with_scale=probe)
    def phases(raw, operation, target, pid, value):
        if raw != 'unit-only diagnostic adapter\n' or target != 2 or type(pid) is not int or pid <= 0:
            raise RuntimeError('unit-only diagnostic adapter rejected')
        return []
    return SimpleNamespace(validate_display_receipt=receipt, validate_display_phases=phases)


def phase_fixture(native, output, full, runner_pid, nonce, bound_at, origin, request,
                  local=None, armed_end=None, seq_before=3):
    """Explicitly synthetic immutable schema4 channel, never native acceptance."""
    binding = dict(schema_version=1, event='RUNNER_BINDING', launch_nonce=nonce,
        runner_pid=runner_pid, runner_path=str(full.root / 'scripts/run_pmix_native.py'), runner_sha256=full.runner_sha,
        native_directory=str(native), output_directory=str(output), run_id=request['run_id'],
        source_manifest_sha256=full.manifest_sha, binary_path=str(full.binary), binary_sha256=full.binary_sha,
        capture_producer_sha256=full.producer_sha, display_id=2, clock_domain=matrix.guard.CLOCK_DOMAIN,
        bound_at_ns=bound_at)
    raw = (json.dumps(binding) + '\n').encode()
    digest = sha(raw)
    request.update(schema_version=4, launch_nonce=nonce, runner_pid=runner_pid,
        native_directory=str(native), output_directory=str(output), runner_binding_sha256=digest,
        runner_clock_domain=matrix.guard.CLOCK_DOMAIN)
    dump(native / 'request.json', request)
    (native / 'runner-binding.json').write_bytes(raw)
    if native != output:
        (output / 'runner-binding.json').write_bytes(raw)
    app = dict(schema_version=1, event='APP_LAUNCH', binding_sha256=digest, launch_nonce=nonce,
        runner_pid=runner_pid, native_directory=str(native), output_directory=str(output), run_id=request['run_id'],
        display_id=2, clock_domain=matrix.guard.CLOCK_DOMAIN, launch_at_ns=origin)
    app_raw = (json.dumps(app) + '\n').encode()
    (native / 'app-launch.json').write_bytes(app_raw)
    if native != output: (output / 'app-launch.json').write_bytes(app_raw)
    for index, (label, operation) in enumerate((('display-before', 'probe'), ('display-active', 'set60'),
                                              ('display-active-probe', 'probe'))):
        value = json.loads((native / (label + '.json')).read_text())
        if operation == 'set60':
            for moment in ('before', 'after'): value[moment].pop('backing_scale', None)
            dump(native / (label + '.json'), value)
        script = 'display-probe.swift' if operation == 'probe' else 'display.swift'
        launch = dict(pid=7000+index, pgid=7000+index, private_session=True,
            command=[*matrix.guard.DISPLAY_SWIFT_PREFIX,str(native/script),operation,'2'], started_monotonic_ns=1_000+index*100,
            timeout_seconds=10)
        process = dict(launch, schema_version=2, exit_code=0, joined=True, owned_group_released=True,
            finished_monotonic_ns=1_050+index*100, timed_out=False, error=None, signals_sent=[], signal=None,
            result='RETURNED', cleanup_grace_seconds=2)
        dump(native/(label+'.subcommand-launch.json'),launch)
        dump(native/(label+'.subcommand-process.json'),process)
        dump(native/(label+'.subcommand.stdout'),value)
        (native/(label+'.subcommand.stderr')).write_text('unit-only diagnostic adapter\n')
    if local is not None:
        (local/'runner-binding.raw.json').write_bytes(raw)
        (local/'runner-request.raw.json').write_bytes((native/'request.json').read_bytes())
        (local/'app-launch.raw.json').write_bytes(app_raw)
        dump(local/'runner-phase-observed.json',dict(binding_sha256=digest,nonce=nonce,armed_end_ns=armed_end,
            observed_at_ns=origin,kernel_credential=dict(pid=runner_pid,parent_pid=1,start_seconds=10,start_micros=0)))
        dump(local/'app-phase-observed.json',dict(binding_sha256=digest,launch_sha256=sha(app_raw),
            observed_at_ns=origin,launch_at_ns=origin,sample_seq_before_binding=seq_before))
    return binding, digest


def background_result(pins, ledger, passed=False):
    """Synthetic adapter output; product raw evidence is never simulated as acceptance."""
    return dict(result='BACKGROUND_QUALIFIED_FOREGROUND_PENDING', stage_PASS_claim=False,
                background_status='GATES_PASS' if passed else 'BLOCKED_CAPTURE_INITIALIZATION',
                background_initialization='INITIALIZATION_ONLY_PASS' if passed else 'BLOCKED',
                background_blocked_reason=None if passed else 'no-eligible-window',
                cargo_gates='PASS', writer_and_refusal_tests='PASS',
                foreground_initialization='PENDING_REAL_OWNED_NATIVE', gates=31,
                source_manifest_sha256=pins.manifest_sha, build_commit=pins.commit,
                binary_sha256=pins.binary_sha, capture_producer_sha256=pins.producer_sha,
                gate_ledger_sha256=ledger)


def seal_synthetic_seed(bundle):
    manifest = bundle / 'Evidence_MANIFEST.sha256'
    manifest.write_text(''.join(matrix.guard.sha(path) + '  ' + path.relative_to(bundle).as_posix() + '\n'
                               for path in sorted(bundle.rglob('*')) if path.is_file() and path != manifest))


def owned_fixture(directory, request, receipt):
    """Complete explicitly synthetic owned observer transcript, no native work."""
    from test_guard import sample, BASE_NS
    from guard_policy import CLOCK_DOMAIN
    nonce = str(uuid.uuid4())
    binary = str(directory / 'unit-only-synthetic-app')
    envelope = dict(protocol_version=3, nonce=nonce, clock_domain=CLOCK_DOMAIN)
    launch_ns = BASE_NS + 55_000_000
    joined_ns = BASE_NS + 255_000_000
    rows = [dict(event='ready', monitor_pid=17, thread_main=True, at_ns=BASE_NS - 1_000_000, **envelope)]
    rows += [dict(sample(index), **envelope) for index in (1, 2)]
    rows.append(dict(event='runner_bound', at_ns=BASE_NS + 60_000_000,
                     credential=dict(pid=42, parent_pid=1, start_seconds=10, start_micros=0), **envelope))
    rows.append(dict(sample(3, runner_pid=42), **envelope))
    rows.append(dict(event='owned_bound', at_ns=BASE_NS + 110_000_000,
                     credential=dict(pid=43, parent_pid=42, start_seconds=10, start_micros=1), **envelope))
    for index in (4, 5, 6, 7):
        row = sample(index, runner_pid=42, owned_pid=43, identity_verified=True,
                     owned_alive=index < 6, owned_ready=index < 6,
                     front_owned=index < 6, capture_complete=index >= 6)
        rows.append(dict(row, **envelope))
    receipt.update(runner_joined_at_ns=joined_ns, post_join_sample_begin_ns=BASE_NS + 300_000_000,
                   first_foreground_ns=BASE_NS + 150_200_000, capture_completed_ns=BASE_NS + 250_200_000,
                   cleanup_started=False, cleanup_deadline_unknown=False)
    origin = BASE_NS + 105_000_000
    full = matrix.guard.FullRun(directory / 'unit-only-product', Path(binary), directory/'unit-only-producer', directory,
        request['mode'],request['round'],2,MANIFEST,BINARY,PRODUCER,RUNNER)
    _, binding_sha = phase_fixture(directory,directory,full,42,nonce,launch_ns,origin,request,
        local=directory,armed_end=BASE_NS+50_200_000)
    receipt.update(app_launch_opportunity_ns=origin,runner_binding_sha256=binding_sha)
    command = ['python3', '-B', 'scripts/run_pmix_native.py', '--binary', binary,
               '--capture-producer', str(directory / 'unit-only-producer'), '--output', str(directory),
               '--mode', request['mode'], '--round', str(request['round']), '--video',
               '--display-id', '2', '--display-policy', 'frozen-60hz', '--allow-display-mode-change']
    if request['mode'] == 'workflow-reopen':
        command += ['--fixture', str(directory.parent / 'workflow1/workflow-output.rcam')]
    dump(directory / 'monitor-control.json', dict(protocolVersion=3, nonce=nonce, commandID=2,
         runnerPID=42, appPID=43, binaryPath=binary, native=str(directory), runID=request['run_id']))
    dump(directory / 'runner-launch.json', dict(pid=42, command=command, nonce=nonce,
         clock_domain=CLOCK_DOMAIN, started_uptime_ns=launch_ns,root=str(full.root),armed_end_ns=BASE_NS+50_200_000,
         product_pins=dict(source_manifest_sha256=MANIFEST,binary_sha256=BINARY,capture_producer_sha256=PRODUCER,runner_sha256=RUNNER)))
    dump(directory / 'native-binding.json', dict(native=str(directory), pid=43,
         run_id=request['run_id'], binary_path=binary, binary_sha256=BINARY))
    dump(directory / 'owned-process.json', dict(pid=43, command=[binary], binary_sha256=BINARY,
        runner_pid=42,launch_nonce=nonce,clock_domain=CLOCK_DOMAIN,app_started_uptime_ns=origin))
    dump(directory / 'window-ready.json', dict(app_pid=43, run_id=request['run_id']))
    dump(directory / 'capture-complete.json', dict(app_pid=43, run_id=request['run_id'], success=True))
    (directory / 'monitor.stdout').write_text(''.join(json.dumps(row) + '\n' for row in rows))
    return rows

class MatrixContract(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory(prefix='rcam-full12-contract-')
        self.addCleanup(self.temp.cleanup)
        self.root = Path(self.temp.name).resolve(strict=True)
        self.pins = matrix.ProductPins(COMMIT, MANIFEST, BINARY, PRODUCER, RUNNER)

    def fixture(self, case=('nav', 1), name='native-test'):
        directory = self.root / name
        directory.mkdir()
        run_id = str(uuid.uuid4())
        original = dict(display_id=2, mode_id=113, width=1920, height=1080,
                        pixel_width=3840, pixel_height=2160,
                        refresh_hz=144, backing_scale=2, in_mirror_set=False)
        active = dict(original, mode_id=212, refresh_hz=60)
        request = dict(schema_version=4, mode=case[0], round=case[1],
                       display_id=2, display_policy='frozen-60hz',
                       display_mode_change_authorized=True,
                       evidence_scope='full-pmix-native', run_id=run_id,
                       source_manifest_sha256=MANIFEST,
                       fixture=str(directory / 'synthetic-fixture.gbr'),
                       fixture_sha256=sha(PROJECT_BYTES))
        receipt = dict(
            runner_pid=42, actual_exit_code=0, joined=True, interrupted=None,
            cleanup_timeout=False, execution_budget_seconds=190,
            cleanup_budget_seconds=40, duration_seconds=1.0,
            clock_unavailable=False, clock_error=None,
            clock_domain='darwin_uptime_raw_ns',
            native_directory=str(directory), run_id=run_id,
            monitor_pid=17, monitor_exit_code=-int(signal.SIGTERM),
            monitor_joined=True, runner_joined_at_ns=1_000_000_000,
            post_join_barrier_satisfied=True,
            post_join_sample_begin_ns=1_001_000_000,
            external_activation=False, phase='CAPTURE_FINALIZED_CLEANUP',
            first_foreground_ns=100_000_000, capture_completed_ns=900_000_000,
            human_input_attributed=False, display_policy='frozen-60hz',
            retry=False, mode=case[0], round=case[1], display_id=2,
            source_manifest_sha256=MANIFEST, utc='2000-01-01T00:00:00+00:00',
            success=True,
        )
        files = {
            'request.json': request,
            'runner.json': dict(exit_code=0, error=None,
                                source_manifest_sha256=MANIFEST),
            'display-before.json': dict(before=original, after=original),
            'display-active.json': dict(before=original, after=active),
            'display-active-probe.json': dict(before=active, after=active),
            'display-restored.json': dict(before=active, after=original),
            'display-restored-probe.json': dict(before=original, after=original),
        }
        if case[0] == 'workflow':
            (directory / 'workflow-output.rcam').write_bytes(PROJECT_BYTES)
        if case[0] == 'workflow-reopen':
            (directory / 'reopen-input.rcam').write_bytes(PROJECT_BYTES)
            request['fixture'] = str(directory / 'reopen-input.rcam')
        for filename, data in files.items():
            dump(directory / filename, data)
        files['display-restored.json'] = copy.deepcopy(files['display-restored.json'])
        for moment in ('before','after'):
            files['display-restored.json'][moment].pop('backing_scale',None)
        dump(directory/'display-restored.json',files['display-restored.json'])
        owned_fixture(directory, request, receipt)
        for name in files:
            files[name] = json.loads((directory/name).read_text())
        return directory, request, receipt, files

    def check_round(self, directory, case, receipt, seen=None, workflow=None):
        return matrix.validate_round(directory, case, receipt,
                                     set() if seen is None else seen, workflow)

    def rejected(self, fn):
        with self.assertRaises((RuntimeError, ValueError, TypeError, KeyError, OSError)):
            fn()

    def test_01_exact_matrix_and_frozen_bound_pins(self):
        self.assertIsInstance(matrix.CASES, tuple)
        self.assertEqual(matrix.CASES, EXPECTED_CASES)
        self.assertEqual(matrix.PRODUCT_PINS, matrix.ProductPins(
            commit='32800302b697b6996fb3bdae852cddee4cab0357',
            manifest_sha='32422693e592b70ac155fdadd5ac0d915cd1352d0ef3770cd148572c30a84cea',
            binary_sha='295201cdd962649aee166076389e4b95206aa133d5c67850cc74c59eef3da149',
            producer_sha='1d22b1b99d5621adf2882d1153609a81a9bf7ff62d93bc9c008b9b186456e9a1',
            runner_sha='20dba8c15a2ed547c98c00ba7d994ffac0dbc610c3f2f0178c046dd6b390a74c',
        ))
        self.assertEqual(self.pins.commit, COMMIT)
        self.assertEqual(self.pins.manifest_sha, MANIFEST)
        self.assertEqual(self.pins.binary_sha, BINARY)
        self.assertEqual(self.pins.producer_sha, PRODUCER)
        self.assertEqual(self.pins.runner_sha, RUNNER)
        evidence = self.root / 'never-created-evidence'
        before = sorted(p.relative_to(self.root).as_posix() for p in self.root.rglob('*'))
        with patch.object(matrix, 'PRODUCT_PINS', None), \
             patch.object(matrix.sys, 'platform', 'darwin'), \
             patch.object(matrix.subprocess, 'Popen', side_effect=AssertionError('Popen before pins')) as popen, \
             patch.object(matrix.subprocess, 'run', side_effect=AssertionError('subprocess before pins')) as process, \
             patch.object(matrix, 'compile_monitor', side_effect=AssertionError('compiler before pins')) as compiler, \
             patch.object(Path, 'mkdir', side_effect=AssertionError('mkdir before pins')) as mkdir:
            with self.assertRaisesRegex(RuntimeError, 'unbound'):
                matrix.run_matrix(root=self.root / 'not-present-source',
                                  binary=self.root / 'not-present-binary',
                                  producer=self.root / 'not-present-producer',
                                  bundle=self.root / 'not-present-bundle',
                                  evidence=evidence, gate_ledger=LEDGER)
            with self.assertRaisesRegex(RuntimeError, 'unbound'):
                matrix.validate_completed(root=self.root / 'not-present-source',
                                          bundle=self.root / 'not-present-bundle',
                                          evidence=evidence, gate_ledger=LEDGER,
                                          attacks=self.root / 'never-created-attacks')
            popen.assert_not_called()
            process.assert_not_called()
            compiler.assert_not_called()
            mkdir.assert_not_called()
        self.assertFalse(evidence.exists())
        after = sorted(p.relative_to(self.root).as_posix() for p in self.root.rglob('*'))
        self.assertEqual(after, before)

    def test_02_synthetic_all_twelve_rounds_validate(self):
        seen = set()
        workflow_sha = sha(PROJECT_BYTES)
        for number, case in enumerate(EXPECTED_CASES):
            with self.subTest(case=case):
                directory, request, receipt, _ = self.fixture(case, 'case-%02d' % number)
                returned_sha = self.check_round(directory, case, receipt, seen, workflow_sha)
                self.assertEqual(returned_sha, workflow_sha)
                # Whether validate_round records IDs itself or returns them for
                # the coordinator, feed the same independently accumulated set.
                seen.add(request['run_id'])
        self.assertEqual(len(seen), 12)

    def test_03_request_schema_round_display_types_are_strict(self):
        directory, request, receipt, _ = self.fixture()
        attacks = [('schema_version', 2), ('schema_version', 3.0),
                   ('schema_version', True), ('round', 1.0), ('round', True),
                   ('display_id', 2.0), ('display_id', True),
                   ('display_mode_change_authorized', 1)]
        for key, value in attacks:
            with self.subTest(key=key, value=value):
                changed = dict(request, **{key: value})
                dump(directory / 'request.json', changed)
                self.rejected(lambda: self.check_round(directory, ('nav', 1), receipt))
        dump(directory / 'request.json', request)
        self.check_round(directory, ('nav', 1), receipt)

    def test_04_request_case_policy_target_scope_mismatches_rejected(self):
        directory, request, receipt, _ = self.fixture()
        for key, value in [('mode', 'move'), ('round', 2), ('display_id', 1),
                           ('display_policy', 'preserve'),
                           ('display_mode_change_authorized', False),
                           ('evidence_scope', 'capture-precheck-only'),
                           ('source_manifest_sha256', '0' * 64)]:
            with self.subTest(key=key):
                dump(directory / 'request.json', dict(request, **{key: value}))
                self.rejected(lambda: self.check_round(directory, ('nav', 1), receipt))
        dump(directory / 'request.json', request)
        self.check_round(directory, ('nav', 1), receipt)

    def test_05_uuid_reuse_invalid_and_receipt_mismatch_rejected(self):
        directory, request, receipt, _ = self.fixture()
        self.rejected(lambda: self.check_round(directory, ('nav', 1), receipt,
                                               {request['run_id']}))
        changed = dict(receipt, run_id=str(uuid.uuid4()))
        self.rejected(lambda: self.check_round(directory, ('nav', 1), changed))
        for invalid in ['unit-not-uuid', '', '10000000-ABCD-4000-8000-000000000001']:
            with self.subTest(run_id=invalid):
                changed_request = dict(request, run_id=invalid)
                changed_receipt = dict(receipt, run_id=invalid)
                dump(directory / 'request.json', changed_request)
                self.rejected(lambda: self.check_round(directory, ('nav', 1), changed_receipt))
        dump(directory / 'request.json', request)

    def test_06_guard_failure_cannot_be_sealed_as_success(self):
        directory, _, receipt, _ = self.fixture()
        attacks = [('success', False), ('actual_exit_code', 2), ('joined', False),
                   ('monitor_joined', False), ('interrupted', 'UNKNOWN_SESSION_INPUT'),
                   ('cleanup_timeout', True), ('clock_unavailable', True),
                   ('post_join_barrier_satisfied', False),
                   ('capture_completed_ns', None),
                   ('clock_domain', 'process-local-monotonic'),
                   ('external_activation', True), ('retry', True),
                   ('success', 1), ('joined', 1), ('actual_exit_code', False),
                   ('monitor_joined', 1), ('monitor_exit_code', 0),
                   ('post_join_barrier_satisfied', 1),
                   ('round', 1.0), ('round', True),
                   ('display_id', 2.0), ('display_id', True),
                   ('source_manifest_sha256', '0' * 64)]
        for key, value in attacks:
            with self.subTest(key=key):
                changed = dict(receipt, **{key: value})
                self.rejected(lambda: self.check_round(directory, ('nav', 1), changed))

    def test_07_exact_budgets_and_real_postjoin_order_required(self):
        directory, _, receipt, _ = self.fixture()
        for key, value in [('execution_budget_seconds', 191),
                           ('execution_budget_seconds', 190.0),
                           ('cleanup_budget_seconds', 41),
                           ('cleanup_budget_seconds', 40.0),
                           ('runner_joined_at_ns', None),
                           ('runner_joined_at_ns', True),
                           ('post_join_sample_begin_ns', None),
                           ('post_join_sample_begin_ns', 999_999_999),
                           ('post_join_sample_begin_ns', 1_250_000_001),
                           ('post_join_sample_begin_ns', 1_001_000_000.0)]:
            with self.subTest(key=key, value=value):
                changed = dict(receipt, **{key: value})
                self.rejected(lambda: self.check_round(directory, ('nav', 1), changed))

    def test_08_display_original_active_restore_and_same_target_required(self):
        directory, _, receipt, files = self.fixture()
        attacks = [
            ('display-before.json', 'after', 'display_id', 1),
            ('display-before.json', 'after', 'display_id', 2.0),
            ('display-before.json', 'after', 'mode_id', 113.0),
            ('display-before.json', 'after', 'mode_id', 114),
            ('display-before.json', 'after', 'refresh_hz', 60),
            ('display-before.json', 'after', 'width', 3840),
            ('display-active.json', 'after', 'display_id', 3),
            ('display-active.json', 'after', 'refresh_hz', 144),
            ('display-active.json', 'after', 'pixel_width', 1920),
            ('display-active.json', 'after', 'backing_scale', 1),
            ('display-active.json', 'before', 'refresh_hz', 60),
            ('display-restored.json', 'before', 'refresh_hz', 144),
            ('display-restored.json', 'after', 'mode_id', 212),
            ('display-restored.json', 'after', 'refresh_hz', 60),
            ('display-active-probe.json', 'after', 'refresh_hz', 144),
            ('display-restored-probe.json', 'after', 'refresh_hz', 60),
            ('display-active.json', 'after', 'in_mirror_set', True),
            ('display-before.json', 'after', 'in_mirror_set', 0),
        ]
        for filename, section, key, value in attacks:
            with self.subTest(filename=filename, key=key):
                changed = copy.deepcopy(files[filename])
                changed[section][key] = value
                dump(directory / filename, changed)
                self.rejected(lambda: self.check_round(directory, ('nav', 1), receipt))
                dump(directory / filename, files[filename])
        self.check_round(directory, ('nav', 1), receipt)

    def test_09_reopen_must_use_current_workflow_output_bytes(self):
        directory, request, receipt, _ = self.fixture(('workflow-reopen', 1))
        self.check_round(directory, ('workflow-reopen', 1), receipt,
                         workflow=sha(PROJECT_BYTES))
        self.rejected(lambda: self.check_round(directory, ('workflow-reopen', 1), receipt,
                                               workflow=None))
        self.rejected(lambda: self.check_round(directory, ('workflow-reopen', 1), receipt,
                                               workflow='0' * 64))
        (directory / 'reopen-input.rcam').write_bytes(PROJECT_BYTES + b'changed')
        changed = dict(request, fixture_sha256=sha(PROJECT_BYTES + b'changed'))
        dump(directory / 'request.json', changed)
        self.rejected(lambda: self.check_round(directory, ('workflow-reopen', 1), receipt,
                                               workflow=sha(PROJECT_BYTES)))

    def test_10_workflow_output_and_reopen_copy_must_exist(self):
        directory, _, receipt, _ = self.fixture(('workflow', 1), 'workflow')
        (directory / 'workflow-output.rcam').unlink()
        self.rejected(lambda: self.check_round(directory, ('workflow', 1), receipt))
        directory, _, receipt, _ = self.fixture(('workflow-reopen', 1), 'reopen')
        (directory / 'reopen-input.rcam').unlink()
        self.rejected(lambda: self.check_round(directory, ('workflow-reopen', 1), receipt,
                                               workflow=sha(PROJECT_BYTES)))

    def test_11_native_verifier_command_exact_no_precheck_relaxation(self):
        source_root = self.root / 'source'
        directory = self.root / 'native'
        actual = matrix.command_native_verify(source_root, directory, self.pins)
        expected = [
            'python3', '-B', 'scripts/verify_pmix_native.py',
            str(directory), '--source-manifest', MANIFEST, '--commit', COMMIT,
            '--binary-sha256', BINARY, '--capture-producer-sha256', PRODUCER,
        ]
        self.assertEqual(actual, expected)
        self.assertNotIn('--allow-capture-precheck', actual)

    def test_12_aggregate_and_actual48_commands_keep_external_ledger(self):
        source_root = self.root / 'source'
        bundle = self.root / 'bundle'
        attacks = self.root / 'attacks'
        expected_aggregate = [
            'python3', '-B', 'scripts/verify_pmix_evidence.py',
            str(bundle), '--source-manifest', MANIFEST, '--commit', COMMIT,
            '--binary-sha256', BINARY, '--gate-ledger-sha256', LEDGER,
        ]
        expected_attacks = [
            'python3', '-B', 'scripts/run_pmix_bundle_attacks.py',
            '--source-root', str(source_root), '--source-manifest', MANIFEST,
            '--commit', COMMIT, '--binary-sha256', BINARY,
            '--gate-ledger-sha256', LEDGER, '--baseline', str(bundle),
            '--out', str(attacks),
        ]
        self.assertEqual(matrix.command_aggregate(source_root, bundle, self.pins, LEDGER),
                         expected_aggregate)
        actual = matrix.command_attacks(source_root, bundle, self.pins, LEDGER, attacks)
        self.assertEqual(actual, expected_attacks)
        self.assertNotIn('--plan-only', actual)
        self.assertNotIn('--allow-capture-precheck', actual)



class FormalOwnedWrapperTests(unittest.TestCase):
    """Real supervisor state machine, synthetic owned actors and raw observations."""
    def wrapper(self, case, callback=None):
        from test_guard import InjectedSupervisorTests
        helper = InjectedSupervisorTests('runTest')
        self.addCleanup(helper.doCleanups)
        code, receipt, actors = helper.exercise(formal_case=case, continuous_guard=callback)
        return helper, code, receipt, actors

    def test_all_formal_modes_use_real60_display_argument_and_unchanged_budget(self):
        for case in EXPECTED_CASES:
            with self.subTest(case=case):
                helper, code, receipt, actors = self.wrapper(case)
                command = helper.last_runner_command
                self.assertEqual(code, 0)
                self.assertEqual(command[command.index('--mode') + 1], case[0])
                self.assertEqual(command[command.index('--round') + 1], str(case[1]))
                self.assertEqual(command[command.index('--display-id') + 1], '2')
                self.assertEqual(command[command.index('--display-policy') + 1], 'frozen-60hz')
                self.assertIn('--allow-display-mode-change', command)
                self.assertEqual('--fixture' in command, case[0] == 'workflow-reopen')
                self.assertEqual(receipt['execution_budget_seconds'], 190)
                self.assertEqual(receipt['cleanup_budget_seconds'], 40)
                self.assertTrue(receipt['post_join_barrier_satisfied'])
                self.assertEqual(actors[1].signals, [])

    def test_continuous_input_halt_interrupts_only_owned_runner(self):
        from guard_policy import Action
        calls = [0]
        def continuous():
            calls[0] += 1
            return Action('halt', 'UNKNOWN_SESSION_INPUT') if calls[0] >= 3 else None
        _, code, receipt, actors = self.wrapper(('nav', 1), continuous)
        self.assertEqual(code, 2)
        self.assertEqual(receipt['interrupted'], 'CONTINUOUS_INPUT_GUARD_FAILURE')
        self.assertEqual(actors[1].signals, [signal.SIGINT])
        self.assertTrue(receipt['joined'])
        self.assertTrue(receipt['monitor_joined'])

    def test_continuous_input_before_arm_prevents_runner(self):
        from guard_policy import Action
        _, code, receipt, actors = self.wrapper(('nav', 1), lambda: Action('halt', 'HID_COUNTER_CHANGE'))
        self.assertEqual(code, 2)
        self.assertEqual(len(actors), 1)
        self.assertIsNone(receipt['runner_pid'])

    def test_preapp_timeout_natural21second_restore_join_keeps_primary_failure_without_app_sigint(self):
        from test_guard import InjectedSupervisorTests
        helper=InjectedSupervisorTests('runTest')
        self.addCleanup(helper.doCleanups)
        code,receipt,actors=helper.exercise(fault='preapp_failed',formal_case=('nav',1))
        self.assertEqual(code,2)
        self.assertEqual(receipt['actual_exit_code'],1)
        self.assertEqual(receipt['interrupted'],'RUNNER_NONZERO_EXIT')
        self.assertEqual(actors[1].signals,[])
        self.assertIsNone(receipt['app_launch_opportunity_ns'])
        self.assertEqual(receipt['phase'],'RUNNER_PREPARING_APP')
        self.assertTrue(receipt['joined'])
        self.assertTrue(receipt['post_join_barrier_satisfied'])


class SyntheticTwelveCaseExecution(unittest.TestCase):
    """Complete coordinator and both real policy streams, no actual RCam/GUI."""
    def exercise(self, fault=None):
        import matrix
        import supervise
        from test_guard import sample, BASE_NS
        temporary = tempfile.TemporaryDirectory(prefix='rcam-full12-owned-')
        self.addCleanup(temporary.cleanup)
        parent = Path(temporary.name).resolve(strict=True)
        root = parent / 'source'
        scripts = root / 'scripts'
        scripts.mkdir(parents=True)
        for name in ('run_pmix_native.py', 'verify_pmix_native.py', 'verify_pmix_evidence.py', 'run_pmix_bundle_attacks.py'):
            (scripts / name).write_text('unit-only synthetic source, never executed\n')
        rows = ''.join(supervise.sha(path) + '  scripts/' + path.name + '\n' for path in sorted(scripts.iterdir()))
        (root / 'MANIFEST.sha256').write_text(rows)
        binary = parent / 'editor-app-release-internal'
        producer = parent / 'capture-producer'
        binary.write_text('unit-only synthetic app, never executed\n')
        producer.write_text('unit-only synthetic producer, never executed\n')
        pins = matrix.ProductPins('a' * 40, supervise.sha(root / 'MANIFEST.sha256'),
                                 supervise.sha(binary), supervise.sha(producer), supervise.sha(scripts / 'run_pmix_native.py'))
        bundle = parent / 'bundle'
        gates = bundle / 'gates'
        gates.mkdir(parents=True)
        (gates / 'gates.json').write_text('[]\n')
        ledger = supervise.sha(gates / 'gates.json')
        with zipfile.ZipFile(bundle / 'Source.zip', 'w') as archive:
            archive.write(root / 'MANIFEST.sha256', 'MANIFEST.sha256')
        review = dict(schema_version=2, stage='S5-M2-C', build_commit=pins.commit,
                      source_manifest_sha256=pins.manifest_sha, native=[],
                      review_state='CLEAN_FINAL_PENDING_INDEPENDENT_REVIEW',
                      user_flicker_report='OPEN', Windows='DEFERRED',
                      K1_native_remaining='DEFERRED', whole_I2='NOT_ALL_PASS')
        dump(bundle / 'REVIEW.json', review)
        seal_synthetic_seed(bundle)
        evidence = parent / 'external-guard'
        now = [BASE_NS]
        monitors = {}
        runners = []
        commands = []
        active = [None]
        active_check = [None]
        class Actor:
            def __init__(self, pid, command, code=None):
                self.pid, self.command, self.returncode = pid, command, code
                self.signals = []
            def poll(self):
                if getattr(self, 'complete_at_ns', None) is not None and now[0] >= self.complete_at_ns:
                    self.returncode = 0
                return self.returncode
            def wait(self, timeout):
                if fault == 'keyboard_cleanup' and len(self.command) > 2 and self.command[2] == 'scripts/run_pmix_native.py' and self.signals:
                    self.first_wait_timeout = timeout
                    now[0] += 35_000_000_000
                    self.returncode = 1
                return self.returncode
            def send_signal(self, sig):
                self.signals.append(sig)
                self.first_sigint_ns = now[0]
                if fault != 'keyboard_cleanup' or len(self.command) < 3 or self.command[2] != 'scripts/run_pmix_native.py':
                    self.returncode = -int(sig)
            def terminate(self): self.returncode = -int(signal.SIGTERM)
            def kill(self): raise AssertionError('no synthetic actor should need kill')
        def completed(command, **kwargs):
            if command[2:3] == ['scripts/verify_pmix_evidence.py']:
                self.assertEqual(command[-1], '--background-only')
                kwargs['stdout'].write((json.dumps(background_result(pins, ledger)) + '\n').encode())
                return type('Qualified', (), {'returncode': 0})()
            if command[0] != '/usr/bin/swiftc':
                raise AssertionError('offline aggregate is a separate operation')
            Path(command[-1]).write_text('unit-only synthetic observer executable\n')
            return type('Compiled', (), {'returncode': 0})()
        def launch(command, **kwargs):
            commands.append(command)
            pid = 100 + len(commands)
            if Path(command[0]).name == 'interference-monitor':
                actor = Actor(pid, command)
                monitors[Path(kwargs['stdout'].name)] = actor
                return actor
            if command[2] == 'scripts/verify_pmix_native.py':
                return Actor(pid, command, 0)
            if len(command) > 3 and command[3] == 'check-round':
                control = Path(command[command.index('--round-check-input') + 1])
                actor = Actor(pid, command, matrix.check_round(control))
                if fault in ('slow_round_check', 'input_round_check') and actor.returncode == 0:
                    actor.returncode = None
                    actor.complete_at_ns = now[0] + 400_000_000
                    active_check[0] = actor
                return actor
            self.assertEqual(command[2], 'scripts/run_pmix_native.py')
            mode = command[command.index('--mode') + 1]
            round_number = int(command[command.index('--round') + 1])
            output = Path(command[command.index('--output') + 1])
            output.mkdir()
            run_id = str(uuid.UUID(int=len(runners) + 1))
            request = dict(schema_version=4, mode=mode, round=round_number, display_id=2,
                           display_policy='frozen-60hz', display_mode_change_authorized=True,
                           evidence_scope='full-pmix-native', run_id=run_id,
                           source_manifest_sha256=pins.manifest_sha, fixture_sha256=sha(PROJECT_BYTES))
            original = dict(display_id=2, mode_id=113, width=1920, height=1080,
                            pixel_width=3840, pixel_height=2160, refresh_hz=144, backing_scale=2, in_mirror_set=False)
            current = dict(original, mode_id=212, refresh_hz=60)
            dump(output / 'request.json', request)
            dump(output / 'display-before.json', dict(before=original, after=original))
            dump(output / 'display-active.json', dict(before=original, after=current))
            dump(output / 'display-active-probe.json', dict(before=current, after=current))
            restored = current if fault == 'restore' and len(runners) == 2 else original
            dump(output / 'display-restored.json', dict(before=current, after=restored))
            dump(output / 'display-restored-probe.json', dict(before=restored, after=restored))
            fixture_path = Path(command[command.index('--fixture')+1]) if '--fixture' in command else None
            full = matrix.guard.FullRun(root,binary,producer,output,mode,round_number,2,
                pins.manifest_sha,pins.binary_sha,pins.producer_sha,pins.runner_sha,fixture_path)
            nonce = kwargs['env']['RCAM_PMIX_LAUNCH_NONCE']
            phase_fixture(output,output,full,pid,nonce,now[0],now[0],request)
            restored_value = json.loads((output/'display-restored.json').read_text())
            for moment in ('before','after'): restored_value[moment].pop('backing_scale',None)
            dump(output/'display-restored.json',restored_value)
            if mode == 'workflow': (output / 'workflow-output.rcam').write_bytes(PROJECT_BYTES)
            if mode == 'workflow-reopen':
                fixture = Path(command[command.index('--fixture') + 1])
                self.assertEqual(fixture, bundle / 'native/workflow1/workflow-output.rcam')
                (output / 'reopen-input.rcam').write_bytes(fixture.read_bytes())
            actor = Actor(pid, command)
            actor.output, actor.request = output, request
            actor.launched_at_ns = now[0]
            actor.app_pid = pid + 1000
            dump(output / 'owned-process.json', dict(pid=actor.app_pid, command=[str(binary)], binary_sha256=pins.binary_sha,
                runner_pid=pid,launch_nonce=nonce,clock_domain=matrix.guard.CLOCK_DOMAIN,app_started_uptime_ns=now[0]))
            dump(output / 'window-ready.json', dict(app_pid=actor.app_pid, run_id=run_id))
            dump(output / 'capture-complete.json', dict(app_pid=actor.app_pid, run_id=run_id, success=True))
            runners.append(actor)
            active[0] = actor
            return actor
        class Tail:
            def __init__(self, path):
                self.actor = monitors[path]
                self.path = path
                self.control = Path(self.actor.command[1])
                self.seq = 0
                self.last_clock = None
                self.partial = b''
                self.runner_bound = self.owned_bound = False
            def close(self): pass
            def read(self):
                if self.last_clock == now[0]: return []
                # Synthetic API acquisition advances the shared uptime; never
                # invent a binding/sample before its actual Popen timestamp.
                now[0] += 2_000_000
                self.last_clock = now[0]
                control = json.loads(self.control.read_text())
                envelope = dict(protocol_version=3, nonce=control['nonce'], clock_domain=supervise.CLOCK_DOMAIN)
                records = []
                if self.seq == 0:
                    records.append(dict(event='ready', monitor_pid=self.actor.pid, thread_main=True, at_ns=now[0] - 2_000_000, **envelope))
                if control['runnerPID'] and not self.runner_bound:
                    records.append(dict(event='runner_bound', at_ns=now[0] - 1_100_000, credential=dict(pid=control['runnerPID'], parent_pid=1,
                                        start_seconds=10, start_micros=0), **envelope))
                    self.runner_bound = True
                if control['appPID'] and not self.owned_bound:
                    records.append(dict(event='owned_bound', at_ns=now[0] - 1_050_000, credential=dict(pid=control['appPID'], parent_pid=control['runnerPID'],
                                        start_seconds=10, start_micros=1), **envelope))
                    self.owned_bound = True
                self.seq += 1
                value = sample(self.seq, begin=now[0] - 1_000_000, runner_pid=control['runnerPID'])
                if control['appPID']:
                    value.update(owned_pid=control['appPID'], identity_verified=True, owned_alive=True,
                                 owned_ready=True, front_owned=True)
                    complete = (now[0] >= active[0].launched_at_ns + 190_001_000_000 if fault == 'late' else self.seq >= 6)
                    if complete:
                        value.update(capture_complete=True, owned_alive=False, front_owned=False)
                        active[0].returncode = 2 if fault == 'nonzero' and len(runners) == 3 else 0
                if fault == 'input_journal' and control['appPID']:
                    row = value['sources']['combined']['keyDown']
                    row['count_before'] += 1
                    row['count_after'] += 1
                if self.control.name == 'continuous-control.json' and fault == 'input' and len(runners) == 3 and active[0].returncode is not None:
                    row = value['sources']['combined']['keyDown']
                    row['count_before'] += 1
                    row['count_after'] += 1
                if self.control.name == 'continuous-control.json' and fault == 'input_round_check' and active_check[0] is not None:
                    row = value['sources']['hid']['keyDown']
                    row['count_before'] += 1
                    row['count_after'] += 1
                records.append(dict(value, **envelope))
                with self.path.open('a') as log:
                    for record in records: log.write(json.dumps(record) + '\n')
                return records
        def discover(previous, candidate_binary, expected_run_id=None, full=None, known_native=None):
            actor = active[0]
            self.assertIsNotNone(full)
            return actor.output, json.loads((actor.output/'owned-process.json').read_text()), actor.request
        interrupted = [False]
        def sleep(seconds):
            if fault == 'keyboard_cleanup' and not interrupted[0] and (evidence / 'nav1/native-binding.json').is_file():
                interrupted[0] = True
                raise KeyboardInterrupt('unit-only exception after owned binding')
            now[0] += 200_000_000 if fault == 'late' else round(seconds * 1e9)
        original_write = supervise.write
        def journal_write(path, value):
            if fault == 'input_journal' and path.name == 'controlled-interrupt.json':
                now[0] += 30_000_000_000
            original_write(path, value)
        with patch.object(supervise, 'write', side_effect=journal_write), \
             patch.object(supervise, 'product_display_validators', side_effect=synthetic_display_validators), \
             patch.object(matrix, 'PRODUCT_PINS', pins), \
             patch.object(matrix.sys, 'platform', 'darwin'), \
             patch.object(supervise.time, 'CLOCK_UPTIME_RAW', 8, create=True), \
             patch.object(supervise.time, 'clock_gettime_ns', side_effect=lambda clock_id: now[0]), \
             patch.object(supervise.time, 'sleep', side_effect=sleep), \
             patch.object(supervise.subprocess, 'run', side_effect=completed), \
             patch.object(supervise.subprocess, 'Popen', side_effect=launch), \
             patch.object(supervise, 'FileTail', Tail), \
             patch.object(supervise, 'discover_native', side_effect=discover), \
             patch('builtins.print'):
            code = matrix.run_matrix(root=root, binary=binary, producer=producer, bundle=bundle,
                                     evidence=evidence, gate_ledger=ledger)
        result = json.loads((evidence / 'MATRIX_RESULT.json').read_text())
        continuous = json.loads((evidence / 'CONTINUOUS_RESULT.json').read_text())
        return code, result, continuous, runners, commands, bundle, evidence

    def test_complete_twelve_cases_real_guard_streams_and_fresh_ids(self):
        code, result, continuous, runners, commands, bundle, evidence = self.exercise()
        self.assertEqual(code, 0)
        self.assertEqual(result['completed_cases'], 12)
        self.assertEqual([(row['mode'], row['round']) for row in result['runs']], list(EXPECTED_CASES))
        self.assertEqual(len({row['run_id'] for row in result['runs']}), 12)
        self.assertEqual(len(runners), 12)
        self.assertEqual(len({actor.pid for actor in runners}), 12)
        self.assertTrue(continuous['success'])
        self.assertEqual(continuous['baseline_resets'], 0)
        self.assertTrue((bundle / 'Evidence_MANIFEST.sha256').is_file())
        self.assertEqual(result['native_aggregate'], 'NOT_RUN')
        self.assertEqual(result['full48'], 'NOT_RUN')
        self.assertEqual(len([c for c in commands if len(c) > 2 and c[2] == 'scripts/verify_pmix_native.py']), 12)

    def test_global_input_during_cleanup_stops_without_fourth_case(self):
        code, result, continuous, runners, commands, bundle, _ = self.exercise('input')
        self.assertEqual(code, 2)
        self.assertEqual(len(runners), 3)
        self.assertLess(result['completed_cases'], 3)
        self.assertEqual(continuous['failure'], 'UNKNOWN_SESSION_INPUT')
        self.assertEqual((bundle / 'Evidence_MANIFEST.sha256').read_bytes(),
                         (bundle.parent / 'external-guard/background-seed-manifest.sha256').read_bytes())

    def test_slow_offline_round_check_keeps_live_stream_consumption_and_full_twelve(self):
        code, result, continuous, runners, commands, _, evidence = self.exercise('slow_round_check')
        self.assertEqual(code, 0)
        self.assertEqual(result['completed_cases'], 12)
        self.assertEqual(len(runners), 12)
        self.assertTrue(continuous['success'])
        self.assertEqual(continuous['baseline_resets'], 0)
        checks = [command for command in commands if len(command) > 3 and command[3] == 'check-round']
        self.assertEqual(len(checks), 12)
        for case in matrix.CASES:
            receipt = json.loads((evidence / matrix.case_name(case) / 'round-evidence-check.json').read_text())
            self.assertTrue(receipt['joined'])
            self.assertTrue(receipt['post_join_barrier_satisfied'])
            self.assertTrue(receipt['success'])

    def test_live_hid_input_during_offline_round_check_stops_before_second_runner(self):
        code, result, continuous, runners, _, bundle, evidence = self.exercise('input_round_check')
        self.assertEqual(code, 2)
        self.assertEqual(result['completed_cases'], 0)
        self.assertEqual(len(runners), 1)
        self.assertEqual(continuous['failure'], 'HID_COUNTER_CHANGE')
        self.assertFalse(json.loads((evidence / 'nav1/round-evidence-check.json').read_text())['success'])
        self.assertEqual((bundle / 'Evidence_MANIFEST.sha256').read_bytes(),
                         (evidence / 'background-seed-manifest.sha256').read_bytes())

    def test_native_failure_stops_exactly_once_and_never_retries(self):
        code, result, _, runners, _, bundle, _ = self.exercise('nonzero')
        self.assertEqual(code, 2)
        self.assertEqual(len(runners), 3)
        self.assertEqual(result['completed_cases'], 2)
        self.assertFalse(result['retry'])
        self.assertEqual((bundle / 'Evidence_MANIFEST.sha256').read_bytes(),
                         (bundle.parent / 'external-guard/background-seed-manifest.sha256').read_bytes())

    def test_target_restoration_failure_stops_before_next_round(self):
        code, result, _, runners, _, bundle, _ = self.exercise('restore')
        self.assertEqual(code, 2)
        self.assertEqual(len(runners), 3)
        self.assertEqual(result['completed_cases'], 2)
        self.assertIn('restoration', result['failure'])
        self.assertEqual((bundle / 'Evidence_MANIFEST.sha256').read_bytes(),
                         (bundle.parent / 'external-guard/background-seed-manifest.sha256').read_bytes())

    def test_late_zero_exit_fails_execution_deadline_even_when_first_poll_sees_done(self):
        code, result, _, runners, _, _, evidence = self.exercise('late')
        receipt = json.loads((evidence / 'nav1/SUPERVISOR_RESULT.json').read_text())
        self.assertEqual(code, 2)
        self.assertEqual(len(runners), 1)
        self.assertEqual(receipt['actual_exit_code'], 0)
        self.assertTrue(receipt['joined'])
        self.assertTrue(receipt['execution_deadline_exceeded'])
        self.assertFalse(receipt['success'])
        self.assertEqual(receipt['interrupted'], 'MICROCHECK_EXECUTION_DEADLINE')

    def test_exception_wait_persists_first40second_cleanup_deadline(self):
        code, _, _, runners, _, _, evidence = self.exercise('keyboard_cleanup')
        receipt = json.loads((evidence / 'nav1/SUPERVISOR_RESULT.json').read_text())
        self.assertEqual(code, 2)
        self.assertEqual(len(runners), 1)
        self.assertTrue(receipt['cleanup_started'])
        self.assertFalse(receipt['cleanup_deadline_unknown'])
        self.assertEqual(receipt['cleanup_deadline_ns'], runners[0].first_sigint_ns + 40_000_000_000)
        self.assertEqual(runners[0].first_wait_timeout, 35.75)
        self.assertLess(receipt['runner_joined_at_ns'], receipt['cleanup_deadline_ns'])
        self.assertFalse(receipt['success'])

    def test_normal_halt_anchors_before30second_journal_io(self):
        code, _, _, runners, _, _, evidence = self.exercise('input_journal')
        receipt = json.loads((evidence / 'nav1/SUPERVISOR_RESULT.json').read_text())
        self.assertEqual(code, 2)
        self.assertEqual(len(runners), 1)
        self.assertEqual(receipt['cleanup_deadline_ns'], runners[0].first_sigint_ns + 40_000_000_000)
        self.assertTrue(receipt['cleanup_started'])
        self.assertFalse(receipt['success'])

class RecoveryAndContinuousEvidenceTests(unittest.TestCase):
    """Synthetic restoration and raw replay checks; no display APIs are called."""
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory(prefix='rcam-recovery-unit-')
        self.addCleanup(self.temp.cleanup)
        self.base = Path(self.temp.name).resolve(strict=True)
        self.native = self.base / 'native'
        self.native.mkdir()
        self.original = dict(display_id=2, mode_id=113, refresh_hz=144, width=1920, height=1080,
                             pixel_width=3840, pixel_height=2160, backing_scale=2, in_mirror_set=False)

    def child(self, label, released=True):
        operation = 'set60' if label == 'display-active' else ('restore' if label == 'display-restored' else 'probe')
        script = 'display-probe.swift' if operation == 'probe' else 'display.swift'
        command = [*matrix.guard.DISPLAY_SWIFT_PREFIX, str(self.native / script), operation, '2']
        if operation == 'restore': command += ['113']
        launch = dict(pid=100, pgid=100, private_session=True, command=command,
                      started_monotonic_ns=1_000_000, timeout_seconds=10)
        process = dict(launch, schema_version=2, joined=True, owned_group_released=released,
                       exit_code=0, finished_monotonic_ns=2_000_000)
        dump(self.native / (label + '.subcommand-launch.json'), launch)
        dump(self.native / (label + '.subcommand-process.json'), process)

    def test_original_target_snapshot_rejects_wrong_target_mirror_and_geometry(self):
        self.assertEqual(matrix.original_snapshot(self.original), self.original)
        for key, value in [('display_id', 1), ('display_id', 2.0), ('mode_id', 212),
                           ('refresh_hz', 60), ('in_mirror_set', True), ('in_mirror_set', 0), ('pixel_width', 1920)]:
            with self.subTest(key=key):
                with self.assertRaises(RuntimeError):
                    matrix.original_snapshot(dict(self.original, **{key: value}))

    def test_normal_finished_finally_accepts_only_paired_joined_private_children(self):
        for label in matrix.DISPLAY_LABELS:
            self.child(label)
        self.assertTrue(matrix.prove_subcommands_joined(self.native, 1))
        self.child('display-active', released=False)
        with self.assertRaisesRegex(RuntimeError, 'join/group'):
            matrix.prove_subcommands_joined(self.native, 1)
        self.child('display-active')
        (self.native / 'display-active.subcommand-process.json').unlink()
        with self.assertRaisesRegex(RuntimeError, 'paired'):
            matrix.prove_subcommands_joined(self.native, 1)

    def test_sigkill_requires_all_five_completed_display_stages(self):
        self.child('display-before')
        with self.assertRaisesRegex(RuntimeError, 'incomplete display journal'):
            matrix.prove_subcommands_joined(self.native, -int(signal.SIGKILL))
        for label in matrix.DISPLAY_LABELS:
            self.child(label)
        self.assertTrue(matrix.prove_subcommands_joined(self.native, -int(signal.SIGKILL)))

    def test_display_receipt_wrong_target_argv_rejects_before_recovery(self):
        for label in matrix.DISPLAY_LABELS:
            self.child(label)
        path = self.native / 'display-active.subcommand-launch.json'
        row = json.loads(path.read_text())
        row['command'][-1] = '3'
        dump(path, row)
        process = self.native / 'display-active.subcommand-process.json'
        value = json.loads(process.read_text())
        value['command'] = row['command']
        dump(process, value)
        with self.assertRaisesRegex(RuntimeError, 'same-target'):
            matrix.prove_subcommands_joined(self.native, 1)

    def recovery_fixture(self, signal_exit=False, final_hz=144):
        local = self.base / 'external'
        local.mkdir()
        root = self.base / 'source'
        (root / 'scripts').mkdir(parents=True)
        source = '// unit-only display helper; not compiled or run\n'
        (root / 'scripts/pmix_display_swift.py').write_text('MUTATOR_SOURCE = ' + repr(source) + '\nPROBE_SOURCE = ' + repr(source))
        (self.native / 'display.swift').write_text(source)
        (self.native / 'display-probe.swift').write_text(source)
        run_id = '00000000-0000-4000-8000-000000000001'
        dump(self.native / 'request.json', dict(run_id=run_id, display_id=2, source_manifest_sha256='b' * 64))
        dump(local / 'SUPERVISOR_RESULT.json', dict(joined=True, actual_exit_code=-9 if signal_exit else 1,
             native_directory=str(self.native), run_id=run_id, source_manifest_sha256='b' * 64,
             runner_joined_at_ns=1_000_000_000, cleanup_deadline_ns=41_000_000_000))
        dump(local / 'native-binding.json', dict(native=str(self.native), run_id=run_id,
                                               initial_display_snapshot=self.original))
        if signal_exit:
            self.child('display-before')
        else:
            for label in matrix.DISPLAY_LABELS:
                self.child(label)
        final = dict(self.original, refresh_hz=final_hz, mode_id=113 if final_hz == 144 else 212)
        dump(self.native / 'display-restored-probe.json', dict(before=final, after=final))
        return local, root

    def test_already_restored_original_probe_never_launches_emergency_child(self):
        local, root = self.recovery_fixture()
        with patch.object(matrix, 'watched_command') as watched:
            result = matrix.assess_and_recover(local, root, None)
        watched.assert_not_called()
        self.assertEqual(result['result'], 'ALREADY_RESTORED_VERIFIED')
        self.assertTrue(result['restoration_verified'])

    def test_unproven_sigkill_child_join_blocks_all_restore_actions(self):
        local, root = self.recovery_fixture(signal_exit=True, final_hz=60)
        with patch.object(matrix, 'watched_command') as watched:
            result = matrix.assess_and_recover(local, root, None)
        watched.assert_not_called()
        self.assertEqual(result['result'], 'BLOCKED')
        self.assertTrue(result['possible_remaining60Hz'])

    def test_joined_failed_restore_can_only_invoke_fixed_original_recovery(self):
        local, root = self.recovery_fixture(final_hz=60)
        calls = []
        def watched(command, checkout, recovery, label, sentinel, **kwargs):
            calls.append((command, kwargs))
            dump(recovery / 'RESTORATION_RESULT.json', dict(success=True, snapshot=self.original))
        with patch.object(matrix, 'watched_command', side_effect=watched), \
             patch.object(matrix.guard, 'system_uptime_ns', return_value=2_000_000_000):
            result = matrix.assess_and_recover(local, root, None)
        self.assertEqual(result['result'], 'EMERGENCY_RESTORED_VERIFIED')
        self.assertEqual(calls[0][0][:4], ['python3', '-B', str(Path(matrix.__file__).resolve()), 'recover'])
        self.assertTrue(calls[0][1]['restoration_after_halt'])
        self.assertEqual(calls[0][1]['seconds'], 40)

    def test_recovery_after_halt_flag_cannot_launch_arbitrary_commands(self):
        with patch.object(matrix.subprocess, 'Popen') as launch:
            with self.assertRaisesRegex(RuntimeError, 'only fixed-target'):
                matrix.watched_command(['python3', 'arbitrary.py'], self.base, self.base,
                                       'bad', None, restoration_after_halt=True)
        launch.assert_not_called()

    def replay_fixture(self, changed=False):
        from test_guard import sample, BASE_NS
        from guard_policy import CLOCK_DOMAIN
        evidence = self.base / 'continuous'
        evidence.mkdir()
        nonce = '00000000-0000-4000-8000-000000000001'
        envelope = dict(protocol_version=3, nonce=nonce, clock_domain=CLOCK_DOMAIN)
        rows = [dict(event='ready', monitor_pid=17, thread_main=True, **envelope)]
        for index in range(1, 5):
            value = sample(index)
            if changed and index == 3:
                value['sources']['hid']['keyDown']['count_before'] += 1
                value['sources']['hid']['keyDown']['count_after'] += 1
            rows.append(dict(value, **envelope))
        (evidence / 'continuous.stdout').write_text(''.join(json.dumps(row) + '\n' for row in rows))
        last = rows[-1]
        dump(evidence / 'continuous-armed.json', dict(monitor_pid=17, nonce=nonce, baseline_seq=2))
        dump(evidence / 'CONTINUOUS_RESULT.json', dict(success=True, monitor_joined=True,
             post_interval_barrier_satisfied=True, baseline_resets=0, initial_baseline_seq=2,
             failure=None, clock_domain=CLOCK_DOMAIN, monitor_pid=17, last_sample_seq=4,
             final_boundary_ns=last['begin_ns'] - 1_000_000,
             post_interval_sample_begin_ns=last['begin_ns']))
        return evidence, BASE_NS

    def test_offline_replay_preserves_one_baseline_and_covers_launch_join(self):
        evidence, base_ns = self.replay_fixture()
        self.assertTrue(matrix.verify_continuous_evidence(evidence, [base_ns + 60_000_000], [base_ns + 80_000_000]))
        with self.assertRaisesRegex(RuntimeError, 'covers every'):
            matrix.verify_continuous_evidence(evidence, [base_ns - 1], [base_ns + 80_000_000])

    def test_resealed_success_receipt_cannot_hide_raw_input_or_rebaseline(self):
        evidence, base_ns = self.replay_fixture(changed=True)
        with self.assertRaisesRegex(RuntimeError, 'raw continuous'):
            matrix.verify_continuous_evidence(evidence, [base_ns + 60_000_000], [base_ns + 80_000_000])

    def test_offline_replay_rejects_partial_final_record(self):
        evidence, base_ns = self.replay_fixture()
        with (evidence / 'continuous.stdout').open('ab') as handle: handle.write(b'{')
        with self.assertRaisesRegex(RuntimeError, 'complete raw'):
            matrix.verify_continuous_evidence(evidence, [base_ns + 60_000_000], [base_ns + 80_000_000])

class Full31BlockingRegressionTests(unittest.TestCase):
    """Safe expectations for the independent v3.0 failures; synthetic only."""
    def fixture(self):
        case = MatrixContract('runTest')
        case.setUp()
        self.addCleanup(case.doCleanups)
        directory, request, receipt, files = case.fixture()
        return directory, request, receipt, files

    def rows(self, directory):
        return [json.loads(row) for row in (directory / 'monitor.stdout').read_text().splitlines()]

    def rewrite(self, directory, rows):
        (directory / 'monitor.stdout').write_text(''.join(json.dumps(row) + '\n' for row in rows))

    def rejected(self, directory, receipt):
        with self.assertRaises((RuntimeError, ValueError, TypeError, KeyError, OSError)):
            matrix.validate_round(directory, ('nav', 1), receipt, set(), None)

    def test_clean_receipt_cannot_hide_real_owned_foreground_loss(self):
        directory, _, receipt, _ = self.fixture()
        rows = self.rows(directory)
        next(row for row in rows if row.get('seq') == 5)['front_owned'] = False
        self.rewrite(directory, rows)
        self.rejected(directory, receipt)

    def test_owned_missing_first_sample_or_binding_cannot_rebaseline(self):
        directory, _, receipt, _ = self.fixture()
        original = self.rows(directory)
        for excluded in ('first', 'runner-bound', 'owned-bound'):
            with self.subTest(excluded=excluded):
                rows = [row for row in original if not (
                    (excluded == 'first' and row.get('seq') == 1) or
                    (excluded == 'runner-bound' and row.get('event') == 'runner_bound') or
                    (excluded == 'owned-bound' and row.get('event') == 'owned_bound'))]
                self.rewrite(directory, rows)
                self.rejected(directory, receipt)

    def test_owned_launch_must_follow_actual_armed_end(self):
        directory, _, receipt, _ = self.fixture()
        path = directory / 'runner-launch.json'
        launch = json.loads(path.read_text())
        from test_guard import BASE_NS
        launch['started_uptime_ns'] = BASE_NS + 10_000_000
        dump(path, launch)
        self.rejected(directory, receipt)

    def test_owned_wrong_parent_nonce_path_and_runid_fail(self):
        directory, _, receipt, _ = self.fixture()
        original = self.rows(directory)
        for field in ('parent', 'nonce', 'control-path', 'run-id'):
            with self.subTest(field=field):
                rows = copy.deepcopy(original)
                control = json.loads((directory / 'monitor-control.json').read_text())
                saved_control = copy.deepcopy(control)
                if field == 'parent':
                    next(row for row in rows if row.get('event') == 'owned_bound')['credential']['parent_pid'] = 999
                elif field == 'nonce': rows[-1]['nonce'] = str(uuid.uuid4())
                elif field == 'control-path': control['binaryPath'] += '-changed'
                else: control['runID'] = str(uuid.uuid4())
                dump(directory / 'monitor-control.json', control)
                self.rewrite(directory, rows)
                self.rejected(directory, receipt)
                dump(directory / 'monitor-control.json', saved_control)

    def test_partial_owned_tail_and_fake_capture_timestamp_rejected(self):
        directory, _, receipt, _ = self.fixture()
        with (directory / 'monitor.stdout').open('ab') as handle: handle.write(b'{')
        self.rejected(directory, receipt)
        directory, _, receipt, _ = self.fixture()
        receipt['capture_completed_ns'] += 1
        self.rejected(directory, receipt)

    def test_legitimate_first_capture_observed_by_postjoin_sample_still_passes(self):
        directory, _, receipt, _ = self.fixture()
        rows = self.rows(directory)
        before = next(row for row in rows if row.get('seq') == 6)
        before.update(capture_complete=False, owned_alive=True, owned_ready=True, front_owned=True)
        post = next(row for row in rows if row.get('seq') == 7)
        receipt['capture_completed_ns'] = post['end_ns']
        self.assertGreater(post['end_ns'], receipt['runner_joined_at_ns'])
        self.rewrite(directory, rows)
        self.assertIsNone(matrix.validate_round(directory, ('nav', 1), receipt, set(), None))

    def test_continuous_missing_seq1_and_before_armed_launch_are_rejected(self):
        fixture = RecoveryAndContinuousEvidenceTests('runTest')
        fixture.setUp()
        self.addCleanup(fixture.doCleanups)
        evidence, base = fixture.replay_fixture()
        path = evidence / 'continuous.stdout'
        rows = [json.loads(row) for row in path.read_text().splitlines()]
        path.write_text(''.join(json.dumps(row) + '\n' for row in rows if row.get('seq') != 1))
        with self.assertRaisesRegex(RuntimeError, 'sample1'):
            matrix.verify_continuous_evidence(evidence, [base + 60_000_000], [base + 80_000_000])
        path.write_text(''.join(json.dumps(row) + '\n' for row in rows))
        with self.assertRaisesRegex(RuntimeError, 'covers every'):
            matrix.verify_continuous_evidence(evidence, [base + 10_000_000], [base + 80_000_000])

    def waiting_actor(self):
        import supervise
        class Actor:
            pid = 42
            returncode = None
            def __init__(self): self.signals, self.timeouts = [], []
            def poll(self): return self.returncode
            def send_signal(self, value): self.signals.append(value)
            def wait(self, timeout):
                self.timeouts.append(timeout)
                raise supervise.subprocess.TimeoutExpired('unit-only owned child', timeout)
        return Actor()

    def test_first_exception_cleanup_clock_persists_one_absolute_origin(self):
        import supervise
        from test_guard import BASE_NS
        actor = self.waiting_actor()
        state = dict(started=False, deadline_ns=None, deadline_unknown=False)
        temporary = tempfile.TemporaryDirectory()
        self.addCleanup(temporary.cleanup)
        self.assertFalse(supervise.interrupt_and_join(actor, 'synthetic exception', Path(temporary.name),
                         clock=lambda: BASE_NS, cleanup_state=state))
        self.assertEqual(state['deadline_ns'], BASE_NS + 40_000_000_000)
        self.assertTrue(state['started'])
        self.assertEqual(actor.timeouts, [35.75])
        self.assertFalse(supervise.interrupt_and_join(actor, 'same cleanup', Path(temporary.name),
                         clock=lambda: BASE_NS + 35_000_000_000, cleanup_state=state, signal_already_sent=True))
        self.assertEqual(actor.timeouts, [35.75, .75])
        self.assertEqual(actor.signals, [signal.SIGINT])
        self.assertEqual(state['deadline_ns'], BASE_NS + 40_000_000_000)

    def test_unknown_first_cleanup_clock_cannot_reopen_when_clock_recovers(self):
        import supervise
        from test_guard import BASE_NS
        actor = self.waiting_actor()
        state = dict(started=False, deadline_ns=None, deadline_unknown=False)
        temporary = tempfile.TemporaryDirectory()
        self.addCleanup(temporary.cleanup)
        def unavailable(): raise supervise.ClockUnavailableError('unit-only native clock failure')
        self.assertFalse(supervise.interrupt_and_join(actor, 'synthetic fault', Path(temporary.name),
                         clock=unavailable, cleanup_state=state))
        self.assertTrue(state['deadline_unknown'])
        self.assertIsNone(state['deadline_ns'])
        self.assertFalse(supervise.interrupt_and_join(actor, 'same cleanup', Path(temporary.name),
                         clock=lambda: BASE_NS, cleanup_state=state, signal_already_sent=True))
        self.assertEqual(actor.timeouts, [35.75, 0])
        self.assertIsNone(state['deadline_ns'])

    def test_missing_started_cleanup_deadline_never_derives_join_plus40(self):
        fixture = RecoveryAndContinuousEvidenceTests('runTest')
        fixture.setUp()
        self.addCleanup(fixture.doCleanups)
        local, root = fixture.recovery_fixture(final_hz=60)
        receipt = json.loads((local / 'SUPERVISOR_RESULT.json').read_text())
        receipt.update(cleanup_started=True, cleanup_deadline_ns=None, cleanup_deadline_unknown=True)
        dump(local / 'SUPERVISOR_RESULT.json', receipt)
        with patch.object(matrix, 'watched_command') as watched:
            result = matrix.assess_and_recover(local, root, None)
        watched.assert_not_called()
        self.assertEqual(result['result'], 'BLOCKED')
        self.assertIn('cannot restart', result['reason'])

class WatchedDeadlineRegressionTests(unittest.TestCase):
    """Real ObservationStream at200ms cadence; injected already-exited actors."""
    def exercise(self, already_exited=False):
        import supervise
        from test_guard import sample, BASE_NS
        temporary = tempfile.TemporaryDirectory(prefix='rcam-watch-deadline-')
        self.addCleanup(temporary.cleanup)
        base = Path(temporary.name).resolve(strict=True)
        now = [BASE_NS]
        class Actor:
            pid = 23
            signals = []
            returncode = 0 if already_exited else None
            def poll(self):
                if now[0] >= deadline + 1_000_000: self.returncode = 0
                return self.returncode
            def wait(self, timeout): return self.returncode
            def send_signal(self, value): self.signals.append(value); self.returncode = -int(value)
        monitor = type('Monitor', (), {'pid': 17, 'returncode': None, 'poll': lambda self: None})()
        class Tail:
            seq = 0
            partial = b''
            def read(self):
                now[0] += 200_000_000
                self.seq += 1
                envelope = dict(protocol_version=3, nonce='unit-only', clock_domain=supervise.CLOCK_DOMAIN)
                rows = [dict(event='ready', monitor_pid=17, thread_main=True, **envelope)] if self.seq == 1 else []
                return rows + [dict(sample(self.seq, begin=now[0] - 1_000_000), **envelope)]
        stream = supervise.ObservationStream(Tail(), monitor, 'unit-only', clock=lambda: now[0])
        sentinel = type('Sentinel', (), {'stream': stream, 'pump': stream.pump})()
        sentinel.pump(); sentinel.pump()
        deadline = now[0] + (300_000_000 if already_exited else 500_000_000)
        actor = Actor()
        command = ['python3', '-B', str(Path(matrix.__file__).resolve()), 'recover']
        with patch.object(matrix.subprocess, 'Popen', return_value=actor), \
             patch.object(supervise, 'system_uptime_ns', side_effect=lambda: now[0]), \
             patch.object(matrix.time, 'sleep', side_effect=lambda value: None):
            with self.assertRaisesRegex(RuntimeError, 'owned verification failed'):
                matrix.watched_command(command, base, base, 'owned-recovery', sentinel,
                                       restoration_after_halt=True, deadline_ns=deadline)
        receipt = json.loads((base / 'owned-recovery.json').read_text())
        self.assertTrue(receipt['joined'])
        self.assertEqual(receipt['exit_code'], 0)
        self.assertTrue(receipt['post_join_barrier_satisfied'])
        self.assertTrue(receipt['deadline_exceeded'])
        self.assertGreater(receipt['joined_at_ns'], deadline)
        self.assertFalse(receipt['success'])
        self.assertEqual(actor.signals, [])

    def test_late100ms_recovery_exit_is_never_success(self): self.exercise()
    def test_first_poll_already_exited_still_checks_real_deadline(self): self.exercise(already_exited=True)


class JournalOriginRegressionTests(unittest.TestCase):
    """Thirty-second synthetic filesystem delay may consume, never move, budget."""
    def exercise(self, first_clock_fails=False, second_clock_fails=False):
        import supervise
        temporary = tempfile.TemporaryDirectory(prefix='rcam-journal-origin-')
        self.addCleanup(temporary.cleanup)
        now = [100_000_000_000]
        state = dict(started=False, deadline_ns=None, deadline_unknown=False)
        trace = []
        class Runner:
            pid = 42
            returncode = None
            def poll(self): return self.returncode
            def send_signal(self, value): trace.append(('SIGINT', now[0]))
            def wait(self, timeout):
                trace.append(('wait', timeout, now[0]))
                self.returncode = 1
                return 1
        reads = [0]
        def clock():
            reads[0] += 1
            trace.append(('clock', now[0]))
            if (first_clock_fails and reads[0] == 1) or (second_clock_fails and reads[0] == 2):
                raise supervise.ClockUnavailableError('unit-only native clock failure')
            return now[0]
        def journal(*args):
            now[0] += 30_000_000_000
            trace.append(('journal', now[0]))
        errors = []
        with patch.object(supervise, 'write', side_effect=journal):
            self.assertTrue(supervise.interrupt_and_join(Runner(), 'unit-only IO delay', Path(temporary.name),
                            clock=clock, cleanup_state=state, clock_errors=errors))
        return state, trace, errors

    def test_journal_delay_does_not_move_origin_or_grant_second_reserve(self):
        state, trace, errors = self.exercise()
        self.assertEqual(state['deadline_ns'], 140_000_000_000)
        self.assertEqual(trace, [('SIGINT', 100_000_000_000), ('clock', 100_000_000_000),
                        ('journal', 130_000_000_000), ('clock', 130_000_000_000), ('wait', 5.75, 130_000_000_000)])
        self.assertEqual(errors, [])

    def test_first_clock_failure_uses_relative_wait_before_any_journal_io(self):
        state, trace, errors = self.exercise(first_clock_fails=True)
        self.assertEqual(trace, [('SIGINT', 100_000_000_000), ('clock', 100_000_000_000),
                        ('wait', 35.75, 100_000_000_000), ('journal', 130_000_000_000)])
        self.assertTrue(state['deadline_unknown'])
        self.assertIsNone(state['deadline_ns'])
        self.assertEqual(len(errors), 1)

    def test_clock_loss_after_journal_never_grants_fresh_relative_reserve(self):
        state, trace, errors = self.exercise(second_clock_fails=True)
        self.assertEqual(state['deadline_ns'], 140_000_000_000)
        self.assertEqual(trace[-1], ('wait', 0, 130_000_000_000))
        self.assertEqual(len(errors), 1)


class AllExitDisplayJournalRegressionTests(unittest.TestCase):
    """Nonnegative failure cannot prove an entirely omitted child was joined."""
    def test_whole_display_pair_missing_rejects_zero_positive_and_signal_exits(self):
        fixture = RecoveryAndContinuousEvidenceTests('runTest')
        fixture.setUp()
        self.addCleanup(fixture.doCleanups)
        for label in matrix.DISPLAY_LABELS:
            fixture.child(label)
        (fixture.native / 'display-active.subcommand-launch.json').unlink()
        (fixture.native / 'display-active.subcommand-process.json').unlink()
        for exit_code in (0, 1, 2, -int(signal.SIGKILL)):
            with self.subTest(exit_code=exit_code):
                with self.assertRaisesRegex(RuntimeError, 'incomplete display journal'):
                    matrix.prove_subcommands_joined(fixture.native, exit_code)

    def test_missing_entire_setter_pair_blocks_even_clean_final_probe_claim(self):
        fixture = RecoveryAndContinuousEvidenceTests('runTest')
        fixture.setUp()
        self.addCleanup(fixture.doCleanups)
        local, root = fixture.recovery_fixture(final_hz=144)
        (fixture.native / 'display-active.subcommand-launch.json').unlink()
        (fixture.native / 'display-active.subcommand-process.json').unlink()
        with patch.object(matrix, 'watched_command') as watched:
            assessment = matrix.assess_and_recover(local, root, None)
        watched.assert_not_called()
        self.assertEqual(assessment['result'], 'BLOCKED')
        self.assertFalse(assessment['restoration_verified'])
        self.assertTrue(assessment['possible_remaining60Hz'])


class BackgroundQualificationContractTests(unittest.TestCase):
    """Synthetic CLI adapter and preservation only, never a real background PASS."""
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory(prefix='rcam-background-adapter-')
        self.addCleanup(self.temp.cleanup)
        self.parent = Path(self.temp.name).resolve(strict=True)
        self.root = self.parent / 'source'
        scripts = self.root / 'scripts'
        scripts.mkdir(parents=True)
        for name in ('run_pmix_native.py', 'verify_pmix_evidence.py'):
            (scripts / name).write_text('unit-only synthetic product source; never executed\n')
        (self.root / 'MANIFEST.sha256').write_text(''.join(
            matrix.guard.sha(path) + '  scripts/' + path.name + '\n' for path in sorted(scripts.iterdir())))
        self.binary = self.parent / 'synthetic-app'
        self.producer = self.parent / 'synthetic-producer'
        self.binary.write_bytes(b'unit-only app bytes\n')
        self.producer.write_bytes(b'unit-only producer bytes\n')
        self.pins = matrix.ProductPins(COMMIT, matrix.guard.sha(self.root / 'MANIFEST.sha256'),
            matrix.guard.sha(self.binary), matrix.guard.sha(self.producer), matrix.guard.sha(scripts / 'run_pmix_native.py'))
        self.bundle = self.parent / 'bundle'
        (self.bundle / 'gates').mkdir(parents=True)
        (self.bundle / 'gates/gates.json').write_bytes(b'[]\n')
        (self.bundle / 'gates/synthetic-background.raw').write_bytes(b'unit-only original sealed raw bytes\n')
        self.ledger = matrix.guard.sha(self.bundle / 'gates/gates.json')
        with zipfile.ZipFile(self.bundle / 'Source.zip', 'w') as archive:
            archive.write(self.root / 'MANIFEST.sha256', 'MANIFEST.sha256')
        self.review = dict(schema_version=2, stage='S5-M2-C', build_commit=self.pins.commit,
            source_manifest_sha256=self.pins.manifest_sha, native=[],
            review_state='CLEAN_FINAL_PENDING_INDEPENDENT_REVIEW', user_flicker_report='OPEN',
            Windows='DEFERRED', K1_native_remaining='DEFERRED', whole_I2='NOT_ALL_PASS')
        dump(self.bundle / 'REVIEW.json', self.review)
        seal_synthetic_seed(self.bundle)
        self.evidence = self.parent / 'external'

    def snapshot(self):
        return {path.relative_to(self.bundle).as_posix(): path.read_bytes()
                for path in self.bundle.rglob('*') if path.is_file()}

    def completed(self, result=None, raw=None, code=0, mutate=None):
        def run(command, **kwargs):
            self.assertEqual(command, matrix.command_background(self.root, self.bundle, self.pins, self.ledger))
            self.assertEqual(kwargs['cwd'], self.root)
            self.assertEqual(kwargs['timeout'], 190)
            self.assertFalse((self.evidence / 'interference-monitor').exists())
            if mutate is not None: mutate()
            value = background_result(self.pins, self.ledger) if result is None else result
            kwargs['stdout'].write(raw if raw is not None else (json.dumps(value, indent=2) + '\n').encode())
            return type('SyntheticCompleted', (), {'returncode': code})()
        return run

    def qualify(self, **kwargs):
        self.evidence.mkdir()
        with patch.object(matrix.subprocess, 'run', side_effect=self.completed(**kwargs)):
            return matrix.qualify_background(self.root, self.bundle, self.evidence, self.pins, self.ledger)

    def test_exact_clean_blocked_and_true_pass_preparation_branches(self):
        for passed in (False, True):
            result = background_result(self.pins, self.ledger, passed)
            self.assertEqual(matrix.validate_background_result(result, self.pins, self.ledger), result)
        self.assertEqual(self.qualify()['background_status'], 'BLOCKED_CAPTURE_INITIALIZATION')
        receipt = json.loads((self.evidence / 'BACKGROUND_QUALIFICATION.json').read_text())
        self.assertTrue(receipt['qualified'])
        self.assertIs(receipt['stage_PASS_claim'], False)
        self.assertEqual((self.evidence / 'background-seed-manifest.sha256').read_bytes(),
                         (self.bundle / 'Evidence_MANIFEST.sha256').read_bytes())

    def test_state_type_identity_and_missing_fields_fail_closed(self):
        original = background_result(self.pins, self.ledger)
        attacks = [('result', 'PASS'), ('stage_PASS_claim', 0), ('stage_PASS_claim', True),
                   ('cargo_gates', 'BLOCKED'), ('writer_and_refusal_tests', 'SKIPPED'),
                   ('foreground_initialization', 'PASS'), ('gates', True), ('gates', 31.0), ('gates', 30),
                   ('background_status', 'CANDIDATE_GATES_PASS'), ('background_initialization', 'PASS'),
                   ('background_blocked_reason', 'permission-denied'), ('success', True)]
        attacks += [(key, '0' * len(original[key])) for key in
                    ('source_manifest_sha256', 'build_commit', 'binary_sha256',
                     'capture_producer_sha256', 'gate_ledger_sha256')]
        for key, value in attacks:
            with self.subTest(key=key, value=value), self.assertRaises(RuntimeError):
                matrix.validate_background_result(dict(original, **{key: value}), self.pins, self.ledger)
        for key in original:
            changed = dict(original)
            del changed[key]
            with self.subTest(missing=key), self.assertRaises(RuntimeError):
                matrix.validate_background_result(changed, self.pins, self.ledger)
        for status, initialization, reason in [('GATES_PASS', 'BLOCKED', None),
                                               ('BLOCKED_CAPTURE_INITIALIZATION', 'BLOCKED', None),
                                               ('GATES_PASS', 'INITIALIZATION_ONLY_PASS', '')]:
            with self.assertRaises(RuntimeError):
                matrix.validate_background_result(dict(original, background_status=status,
                    background_initialization=initialization, background_blocked_reason=reason), self.pins, self.ledger)

    def test_nonzero_malformed_duplicate_and_nonfinite_json_reject(self):
        original = self.snapshot()
        raws = [b'FAIL: no eligible qualification\n', b'{}\n{}\n', b'{"gates":31,"gates":31}',
                b'{"gates":NaN}', b'[]', b'x' * (matrix.guard.MAX_LINE_BYTES + 1)]
        for index, raw in enumerate(raws):
            with self.subTest(index=index):
                self.evidence = self.parent / ('external-' + str(index))
                with self.assertRaises(RuntimeError): self.qualify(raw=raw)
                self.assertFalse(json.loads((self.evidence / 'BACKGROUND_QUALIFICATION.json').read_text())['qualified'])
                self.assertEqual(self.snapshot(), original)
        self.evidence = self.parent / 'external-nonzero'
        with self.assertRaises(RuntimeError): self.qualify(code=1)
        self.assertEqual(self.snapshot(), original)

    def test_qualification_timeout_emits_failed_receipt_before_any_gui(self):
        self.evidence.mkdir()
        before = self.snapshot()
        with patch.object(matrix.subprocess, 'run', side_effect=matrix.subprocess.TimeoutExpired('synthetic', 190)):
            with self.assertRaisesRegex(RuntimeError, 'rejected'):
                matrix.qualify_background(self.root, self.bundle, self.evidence, self.pins, self.ledger)
        self.assertFalse(json.loads((self.evidence / 'BACKGROUND_QUALIFICATION.json').read_text())['qualified'])
        self.assertEqual(self.snapshot(), before)

    def test_real_coordinator_early_rejects_before_compile_clock_observer_runner(self):
        original = self.snapshot()
        bad = dict(background_result(self.pins, self.ledger), background_blocked_reason='permission-denied')
        with patch.object(matrix, 'PRODUCT_PINS', self.pins), patch.object(matrix.sys, 'platform', 'darwin'), \
             patch.object(matrix.subprocess, 'run', side_effect=self.completed(result=bad)) as child, \
             patch.object(matrix.subprocess, 'Popen', side_effect=AssertionError('observer/runner before qualification')) as popen, \
             patch.object(matrix, 'compile_monitor', side_effect=AssertionError('compiler before qualification')) as compiler, \
             patch.object(matrix.guard, 'system_uptime_ns', side_effect=AssertionError('clock before qualification')) as clock, \
             patch.object(matrix.ContinuousSentinel, 'start', side_effect=AssertionError('sentinel before qualification')) as sentinel, \
             patch.object(matrix.guard, 'run', side_effect=AssertionError('runner before qualification')) as runner:
            with self.assertRaisesRegex(RuntimeError, 'background preparation rejected'):
                matrix.run_matrix(root=self.root, binary=self.binary, producer=self.producer,
                    bundle=self.bundle, evidence=self.evidence, gate_ledger=self.ledger)
            self.assertEqual(child.call_count, 1)
            for mock in (popen, compiler, clock, sentinel, runner): mock.assert_not_called()
        self.assertEqual(self.snapshot(), original)
        self.assertFalse((self.bundle / 'native').exists())

    def test_fresh_seed_requires_seal_complete_inventory_and_no_native(self):
        matrix.validate_seed(self.bundle, self.pins, self.ledger)
        (self.bundle / 'native').mkdir()
        with self.assertRaises(RuntimeError): matrix.validate_seed(self.bundle, self.pins, self.ledger)
        (self.bundle / 'native').rmdir()
        manifest = self.bundle / 'Evidence_MANIFEST.sha256'
        old = manifest.read_bytes()
        manifest.unlink()
        with self.assertRaises(RuntimeError): matrix.validate_seed(self.bundle, self.pins, self.ledger)
        manifest.write_bytes(old)
        extra = self.bundle / 'gates/unsealed.raw'
        extra.write_bytes(b'unit-only\n')
        with self.assertRaises(RuntimeError): matrix.validate_seed(self.bundle, self.pins, self.ledger)
        extra.unlink()
        manifest.write_bytes(old + old.splitlines(keepends=True)[0])
        with self.assertRaises(RuntimeError): matrix.validate_seed(self.bundle, self.pins, self.ledger)

    def test_even_a_resealed_background_mutation_during_qualification_rejects(self):
        def mutate():
            (self.bundle / 'gates/synthetic-background.raw').write_bytes(b'unit-only changed raw\n')
            seal_synthetic_seed(self.bundle)
        with self.assertRaisesRegex(RuntimeError, 'rejected'):
            self.qualify(mutate=mutate)
        self.assertFalse(json.loads((self.evidence / 'BACKGROUND_QUALIFICATION.json').read_text())['qualified'])

    def test_real_coordinator_rejects_top_level_seed_symlink_before_resolve(self):
        alias = self.parent / 'seed-alias'
        alias.symlink_to(self.bundle, target_is_directory=True)
        before = self.snapshot()
        with patch.object(matrix, 'PRODUCT_PINS', self.pins), patch.object(matrix.sys, 'platform', 'darwin'), \
             patch.object(matrix.subprocess, 'run') as process, patch.object(matrix, 'compile_monitor') as compiler:
            with self.assertRaisesRegex(RuntimeError, 'symlink'):
                matrix.run_matrix(root=self.root, binary=self.binary, producer=self.producer,
                    bundle=alias, evidence=self.evidence, gate_ledger=self.ledger)
            process.assert_not_called()
            compiler.assert_not_called()
        self.assertEqual(self.snapshot(), before)
        self.assertFalse(self.evidence.exists())

    def rows(self):
        return [dict(mode=case[0], round=case[1], directory='native/' + matrix.case_name(case)) for case in matrix.CASES]

    def test_successful_reseal_keeps_original_background_and_changes_only_native_review(self):
        original = self.snapshot()
        self.qualify()
        rows = self.rows()
        for row in rows:
            directory = self.bundle / row['directory']
            directory.mkdir(parents=True)
            (directory / 'synthetic.raw').write_bytes(b'unit-only native bytes\n')
        matrix.seal_bundle(self.bundle, self.review, rows, self.evidence)
        for name, data in original.items():
            if name not in ('REVIEW.json', 'Evidence_MANIFEST.sha256'):
                self.assertEqual((self.bundle / name).read_bytes(), data)
        self.assertEqual(json.loads((self.bundle / 'REVIEW.json').read_text()), dict(self.review, native=rows))
        self.assertEqual((self.evidence / 'background-seed-manifest.sha256').read_bytes(),
                         original['Evidence_MANIFEST.sha256'])
        self.assertTrue(matrix.verify_background_qualification(self.root, self.bundle, self.evidence,
                                                               self.pins, self.ledger, rows))

    def test_late_background_added_file_or_changed_review_blocks_reseal(self):
        self.qualify()
        old_review = (self.bundle / 'REVIEW.json').read_bytes()
        extra = self.bundle / 'gates/extra.raw'
        extra.write_bytes(b'unit-only late extra\n')
        with self.assertRaisesRegex(RuntimeError, 'inventory'):
            matrix.seal_bundle(self.bundle, self.review, self.rows(), self.evidence)
        self.assertEqual((self.bundle / 'REVIEW.json').read_bytes(), old_review)
        extra.unlink()
        dump(self.bundle / 'REVIEW.json', dict(self.review, whole_I2='ALL_PASS'))
        with self.assertRaises(RuntimeError): matrix.seal_bundle(self.bundle, self.review, self.rows(), self.evidence)

    def test_offline_replay_binds_cli_raw_command_identity_and_preserved_seal(self):
        self.qualify()
        rows = self.rows()
        matrix.seal_bundle(self.bundle, self.review, rows, self.evidence)
        path = self.evidence / 'BACKGROUND_QUALIFICATION.json'
        original = json.loads(path.read_text())
        for key, value in [('qualified', False), ('actual_exit_code', False),
                           ('command', original['command'][:-1]), ('result', dict(original['result'], gates=30)),
                           ('background_seed_manifest_sha256', '0' * 64)]:
            dump(path, dict(original, **{key: value}))
            with self.subTest(key=key), self.assertRaises(RuntimeError):
                matrix.verify_background_qualification(self.root, self.bundle, self.evidence, self.pins, self.ledger, rows)
        dump(path, original)
        self.assertTrue(matrix.verify_background_qualification(self.root, self.bundle, self.evidence,
                                                               self.pins, self.ledger, rows))

class RoundCheckIsolationTests(unittest.TestCase):
    """Pure worker contract; synthetic bytes never grant native acceptance."""
    def setUp(self):
        fixture = MatrixContract('runTest')
        fixture.setUp()
        self.addCleanup(fixture.doCleanups)
        self.local, self.request, self.receipt, _ = fixture.fixture()
        self.pins = fixture.pins
        self.input = self.local / 'round-check-input.json'
        dump(self.local / 'SUPERVISOR_RESULT.json', self.receipt)
        self.control = dict(schema_version=1, mode='nav', round=1, directory=str(self.local),
                            seen_run_ids=[], workflow_sha256=None,
                            supervisor_receipt_sha256=matrix.guard.sha(self.local / 'SUPERVISOR_RESULT.json'))
        self.seen = set()
        self.sentinel = type('SyntheticSentinel', (), {'pump': lambda self: None})()

    def worker(self, control=None):
        dump(self.input, self.control if control is None else control)
        with patch.object(matrix, 'PRODUCT_PINS', self.pins):
            code = matrix.check_round(self.input)
        return code, json.loads((self.local / 'round-check-result.json').read_text())

    def watched(self, mutate=None, fail_join=False):
        def command(command, root, local, label, sentinel):
            self.assertEqual(label, 'round-evidence-check')
            self.assertEqual(command, ['python3', '-B', str(Path(matrix.__file__).resolve()), 'check-round',
                '--root', str(self.local), '--round-check-input', str(self.input)])
            self.assertEqual(matrix.check_round(self.input), 0)
            if mutate is not None:
                path = self.local / 'round-check-result.json'
                result = json.loads(path.read_text())
                result.update(mutate)
                dump(path, result)
            if fail_join:
                raise RuntimeError('synthetic owned child not joined')
        with patch.object(matrix, 'PRODUCT_PINS', self.pins), \
             patch.object(matrix, 'watched_command', side_effect=command):
            return matrix.watched_round_check(self.local, self.local, self.local,
                         ('nav', 1), self.receipt, self.seen, None, self.sentinel)

    def test_complete_raw_worker_and_parent_commit_only_verified_run(self):
        code, result = self.worker()
        self.assertEqual(code, 0)
        self.assertTrue(result['success'])
        self.assertEqual(result['seen_run_ids'], [self.request['run_id']])
        self.assertEqual(result['input_sha256'], matrix.guard.sha(self.input))
        self.assertIsNone(self.watched())
        self.assertEqual(self.seen, {self.request['run_id']})

    def test_input_types_receipt_source_and_duplicate_ids_reject(self):
        attacks = [('round', True), ('workflow_sha256', 'bad'), ('schema_version', True),
                   ('seen_run_ids', [self.request['run_id']]), ('seen_run_ids', ['bad']),
                   ('supervisor_receipt_sha256', '0' * 64)]
        for key, value in attacks:
            with self.subTest(key=key):
                code, result = self.worker(dict(self.control, **{key: value}))
                self.assertEqual(code, 2)
                self.assertFalse(result['success'])
        self.receipt['source_manifest_sha256'] = '0' * 64
        dump(self.local / 'SUPERVISOR_RESULT.json', self.receipt)
        self.control['supervisor_receipt_sha256'] = matrix.guard.sha(self.local / 'SUPERVISOR_RESULT.json')
        code, result = self.worker()
        self.assertEqual(code, 2)
        self.assertIn('pinned product source', result['failure'])

    def test_worker_replays_hidden_focus_loss_and_rejects_partial_tail(self):
        path = self.local / 'monitor.stdout'
        original = path.read_text()
        rows = [json.loads(line) for line in original.splitlines()]
        next(row for row in rows if row.get('seq') == 5)['front_owned'] = False
        path.write_text(''.join(json.dumps(row) + '\n' for row in rows))
        code, result = self.worker()
        self.assertEqual(code, 2)
        self.assertIn('owned raw input/foreground/integrity failure', result['failure'])
        path.write_text(original + '{')
        code, result = self.worker()
        self.assertEqual(code, 2)
        self.assertIn('complete original owned raw line', result['failure'])

    def test_parent_rejects_worker_result_binding_without_advancing_seen_ids(self):
        attacks = [('run_id', str(uuid.UUID(int=1))), ('round', True), ('mode', 'move'),
                   ('seen_run_ids', []), ('input_sha256', '0' * 64),
                   ('supervisor_receipt_sha256', '0' * 64), ('workflow_sha256', '0' * 64),
                   ('success', False), ('failure', 'synthetic failure')]
        for key, value in attacks:
            with self.subTest(key=key), self.assertRaises(RuntimeError):
                self.watched({key: value})
            self.assertEqual(self.seen, set())

    def test_clean_worker_result_cannot_replace_child_join_failure(self):
        with self.assertRaisesRegex(RuntimeError, 'child not joined'):
            self.watched(fail_join=True)
        self.assertEqual(self.seen, set())

    def test_input_changed_after_worker_read_cannot_match_original_parent_bytes(self):
        original = matrix.validate_round
        def delayed(directory, case, receipt, seen, workflow, guard_directory=None):
            changed = json.loads(self.input.read_text())
            changed['directory'] = str(self.local / 'unit-only-nonexistent')
            dump(self.input, changed)
            return original(directory, case, receipt, seen, workflow, guard_directory)
        with patch.object(matrix, 'validate_round', side_effect=delayed):
            with self.assertRaisesRegex(RuntimeError, 'result/input/identity binding'):
                self.watched()
        self.assertEqual(self.seen, set())
        result = json.loads((self.local / 'round-check-result.json').read_text())
        self.assertNotEqual(result['input_sha256'], matrix.guard.sha(self.input))

    def test_worker_canonical_directory_must_match_original_receipt(self):
        other = self.local.parent / 'unit-only-redirect'
        other.mkdir()
        code, result = self.worker(dict(self.control, directory=str(other)))
        self.assertEqual(code, 2)
        self.assertIn('canonical output/owned launch binding', result['failure'])

    def test_control_mutation_during_atomic_write_rejected_before_child_launch(self):
        original = matrix.guard.write
        def changed(path, value):
            original(path, value)
            if path == self.input:
                altered = dict(value, workflow_sha256='0' * 64)
                dump(path, altered)
        with patch.object(matrix.guard, 'write', side_effect=changed), \
             patch.object(matrix, 'watched_command') as child:
            with self.assertRaisesRegex(RuntimeError, 'original round check input bytes'):
                matrix.watched_round_check(self.local, self.local, self.local, ('nav', 1),
                                          self.receipt, self.seen, None, self.sentinel)
            child.assert_not_called()
        self.assertEqual(self.seen, set())

    def test_worker_hashes_and_parses_one_original_receipt_byte_snapshot(self):
        receipt_path = self.local / 'SUPERVISOR_RESULT.json'
        original_read = Path.read_bytes
        reads = []
        def read(path):
            if path == receipt_path:
                reads.append(path)
            return original_read(path)
        with patch.object(Path, 'read_bytes', new=read):
            code, result = self.worker()
        self.assertEqual(code, 0)
        self.assertEqual(len(reads), 1)
        self.assertEqual(result['supervisor_receipt_sha256'], self.control['supervisor_receipt_sha256'])

    def test_parent_requires_passed_receipt_to_match_original_parsed_bytes(self):
        altered = dict(self.receipt, run_id=str(uuid.UUID(int=1)))
        with patch.object(matrix, 'watched_command') as child:
            with self.assertRaisesRegex(RuntimeError, 'original parsed supervisor receipt bytes'):
                matrix.watched_round_check(self.local, self.local, self.local, ('nav', 1),
                                          altered, self.seen, None, self.sentinel)
            child.assert_not_called()
        self.assertEqual(self.seen, set())


class InterruptJournalFailureRegressionTests(unittest.TestCase):
    def exercise(self, recovery=False, first_clock_fails=False, timeout=False):
        temp = tempfile.TemporaryDirectory(prefix='rcam-journal-fault-unit-')
        self.addCleanup(temp.cleanup)
        base = Path(temp.name).resolve(strict=True)
        now = [100_000_000_000]
        class Actor:
            pid = 42
            returncode = None
            def __init__(self): self.signals, self.waits = [], []
            def poll(self): return self.returncode
            def send_signal(self, value): self.signals.append(value)
            def wait(self, timeout):
                self.waits.append(timeout)
                if timeout_fault:
                    raise matrix.subprocess.TimeoutExpired('unit-only child', timeout)
                self.returncode = 0
                return 0
        actor = Actor()
        timeout_fault = timeout
        pumps = [0]
        failed_clock = [False]
        def pump():
            pumps[0] += 1
            if pumps[0] == 2: raise RuntimeError('unit-only watch failure')
            return None
        sentinel = type('SyntheticSentinel', (), {'pump': staticmethod(pump), 'stream': object()})()
        original = matrix.guard.write
        def write(path, value):
            if path.name == 'controlled-interrupt.json':
                now[0] += 30_000_000_000
                raise FileExistsError('unit-only failed durable journal')
            original(path, value)
        def clock():
            if first_clock_fails and actor.signals and not failed_clock[0]:
                failed_clock[0] = True
                raise matrix.guard.ClockUnavailableError('unit-only first cleanup clock loss')
            return now[0]
        command = (['python3', '-B', str(Path(matrix.__file__).resolve()), 'recover'] if recovery else
                   ['python3', '-B', str(Path(matrix.__file__).resolve()), 'check-round'])
        with patch.object(matrix.subprocess, 'Popen', return_value=actor), \
             patch.object(matrix.guard, 'write', side_effect=write), \
             patch.object(matrix.guard, 'system_uptime_ns', side_effect=clock), \
             patch.object(matrix.guard, 'post_join_barrier', return_value=True):
            with self.assertRaises(RuntimeError):
                matrix.watched_command(command, base, base, 'owned-check', sentinel,
                                       restoration_after_halt=recovery,
                                       deadline_ns=140_000_000_000 if recovery else None)
        receipt = json.loads((base / 'owned-check.json').read_text())
        self.assertEqual(actor.signals, [signal.SIGINT])
        self.assertTrue(receipt['interrupt_journal_errors'])
        self.assertFalse(receipt['success'])
        return actor, receipt

    def test_check_child_journal_error_still_joins_using_original_remaining_budget(self):
        actor, receipt = self.exercise()
        self.assertEqual(actor.waits, [5.75])
        self.assertTrue(receipt['joined'])

    def test_recovery_presignal_journal_error_still_joins_same_deadline(self):
        actor, receipt = self.exercise(recovery=True)
        self.assertEqual(actor.waits, [5.75])
        self.assertTrue(receipt['joined'])

    def test_first_clock_unknown_wait_occurs_before_failed_journal_without_new_reserve(self):
        actor, receipt = self.exercise(first_clock_fails=True)
        self.assertEqual(actor.waits, [35.75])
        self.assertTrue(receipt['joined'])
        self.assertTrue(receipt['clock_unavailable'])

    def test_failed_journal_and_legal_wait_timeout_retain_actual_unjoined_failure(self):
        actor, receipt = self.exercise(timeout=True)
        self.assertEqual(actor.waits, [5.75])
        self.assertFalse(receipt['joined'])
        self.assertIsNone(receipt['exit_code'])


class LaunchBindingContractTests(unittest.TestCase):
    def setUp(self):
        fixture=MatrixContract('runTest');fixture.setUp();self.addCleanup(fixture.doCleanups)
        self.directory,self.request,self.receipt,_=fixture.fixture()
        self.raw,self.binding,self.digest=matrix.guard.raw_json(self.directory/'runner-binding.raw.json')
        self.app_raw,self.app,self.app_sha=matrix.guard.raw_json(self.directory/'app-launch.raw.json')
        self.launch=json.loads((self.directory/'runner-launch.json').read_text())
        self.full=matrix.guard.FullRun(Path(self.launch['root']),Path(self.binding['binary_path']),
            self.directory/'unit-only-producer',self.directory,'nav',1,2,MANIFEST,BINARY,PRODUCER,RUNNER)

    def test_exact_binding_nonce_identity_paths_and_pins_reject_mismatches(self):
        g=matrix.guard
        g.validate_runner_binding(self.binding,self.digest,self.request,self.full,self.binding['launch_nonce'],42,self.launch['armed_end_ns'])
        changes={'launch_nonce':str(uuid.uuid4()),'runner_pid':41,'runner_path':'/unit-only-wrong/runner.py',
                 'runner_sha256':'0'*64,'source_manifest_sha256':'0'*64,'binary_path':'/unit-only-wrong/app',
                 'binary_sha256':'0'*64,'capture_producer_sha256':'0'*64,'display_id':1,
                 'output_directory':'/unit-only-wrong/output','clock_domain':'process_relative',
                 'schema_version':True,'bound_at_ns':True}
        for key,value in changes.items():
            with self.subTest(key=key),self.assertRaises((RuntimeError,ValueError,TypeError)):
                g.validate_runner_binding(dict(self.binding,**{key:value}),self.digest,self.request,self.full,
                    self.binding['launch_nonce'],42,self.launch['armed_end_ns'])

    def test_future_and_changed_app_opportunity_do_not_get_accepted(self):
        g=matrix.guard
        origin=g.validate_app_launch(self.app,self.binding,self.digest)
        with self.assertRaises(RuntimeError):g.validate_app_launch(self.app,self.binding,self.digest,origin-1)
        for key,value in [('runner_pid',41),('binding_sha256','0'*64),('launch_nonce',str(uuid.uuid4())),
                          ('native_directory','/unit-only-other-native'),('output_directory','/unit-only-other-output'),
                          ('run_id',str(uuid.uuid4())),('clock_domain','relative'),('launch_at_ns',True)]:
            with self.subTest(key=key),self.assertRaises(RuntimeError):
                g.validate_app_launch(dict(self.app,**{key:value}),self.binding,self.digest)

    def test_hash_and_parse_same_single_bytes_and_marker_symlink_reject(self):
        path=self.directory/'runner-binding.raw.json'
        original=Path.read_bytes;reads=[]
        def read(current):
            if current==path:reads.append(current)
            return original(current)
        with patch.object(Path,'read_bytes',new=read):
            raw,value,digest=matrix.guard.raw_json(path)
        self.assertEqual(len(reads),1);self.assertEqual(sha(raw),digest);self.assertEqual(value,self.binding)
        alias=self.directory/'unit-only-marker-alias.json';alias.symlink_to(path)
        with self.assertRaises(RuntimeError):matrix.guard.raw_json(alias)

    def test_real_distinct_native_and_output_directories_keep_strict_bridge(self):
        import shutil
        native=self.directory.parent/'unit-only-distinct-native';native.mkdir()
        for source in self.directory.iterdir():
            if source.is_file():shutil.copyfile(source,native/source.name)
        request=dict(self.request)
        binding,digest=phase_fixture(native,self.directory,self.full,42,self.binding['launch_nonce'],
            self.binding['bound_at_ns'],self.app['launch_at_ns'],request,local=self.directory,
            armed_end=self.launch['armed_end_ns'])
        shutil.copyfile(native/'request.json',self.directory/'request.json')
        self.receipt.update(native_directory=str(native),runner_binding_sha256=digest)
        dump(self.directory/'SUPERVISOR_RESULT.json',self.receipt)
        control=json.loads((self.directory/'monitor-control.json').read_text());control['native']=str(native)
        dump(self.directory/'monitor-control.json',control)
        body=json.loads((self.directory/'native-binding.json').read_text());body['native']=str(native)
        dump(self.directory/'native-binding.json',body)
        control=dict(schema_version=1,mode='nav',round=1,directory=str(self.directory),seen_run_ids=[],workflow_sha256=None,
                     supervisor_receipt_sha256=matrix.guard.sha(self.directory/'SUPERVISOR_RESULT.json'))
        input_path=self.directory/'round-check-input.json';dump(input_path,control)
        with patch.object(matrix,'PRODUCT_PINS',matrix.ProductPins(COMMIT,MANIFEST,BINARY,PRODUCER,RUNNER)):
            self.assertEqual(matrix.check_round(input_path),0)
        self.assertNotEqual(binding['native_directory'],binding['output_directory'])


class StreamFailureDiagnosticTests(unittest.TestCase):
    def test_first_stale_sample_preserves_consumption_clock_and_not_later_error(self):
        from test_guard import StreamTests, sample
        fixture = StreamTests('runTest')
        fixture.setUp()
        self.addCleanup(fixture.doCleanups)
        fixture.start()
        fixture.now += 400_000_000
        row = sample(3)
        fixture.append(row)
        halt = fixture.stream.pump()
        self.assertEqual(halt.reason, 'INVALID_MONITOR_STREAM')
        details = dict(fixture.stream.first_stream_error)
        self.assertEqual(details['error'], 'stale/future monitor sample')
        self.assertEqual(details['sample_seq'], 3)
        self.assertEqual(details['sample_end_ns'], row['end_ns'])
        self.assertEqual(details['observed_at_ns'], fixture.now)
        with fixture.path.open('ab') as handle:
            handle.write(b'not-json\n')
        fixture.stream.pump()
        self.assertEqual(fixture.stream.first_stream_error, details)
        self.assertEqual(fixture.stream.policy.terminal.details, dict(details, human_attribution=False))
        sentinel = matrix.ContinuousSentinel(fixture.path.parent, fixture.path, 'synthetic')
        sentinel.monitor, sentinel.tail, sentinel.stream = fixture.monitor, fixture.tail, fixture.stream
        def stop(monitor):
            monitor.returncode = -int(signal.SIGTERM)
            return True
        with patch.object(matrix.guard, 'system_uptime_ns', return_value=fixture.now), \
             patch.object(matrix.guard, 'stop_monitor', side_effect=stop):
            self.assertFalse(sentinel.finish())
        receipt = json.loads((fixture.path.parent / 'CONTINUOUS_RESULT.json').read_text())
        self.assertEqual(receipt['first_stream_error'], details)
        self.assertEqual(receipt['failure_details'], dict(details, human_attribution=False))


if __name__ == '__main__':
    unittest.main(verbosity=2)
