"""Linux-safe policy/stream tests and synthetic owned-process lifecycle checks.

No AppKit, real user input, capture producer, or product binary is executed.
"""
from copy import deepcopy
import json
from pathlib import Path
import signal
import subprocess
import sys
import tempfile
import time
import unittest
from unittest.mock import patch

from guard_policy import (AGE_TOLERANCE_NS, FOREGROUND_LIMIT_NS, BIND_LIMIT_NS,
                          READY_LIMIT_NS, MAX_SAMPLE_GAP_NS, TYPES, STATES, CLOCK_DOMAIN,
                          GuardPolicy, source_changes, validate_sample)
import supervise
from supervise import (FileTail, ObservationStream, interrupt_and_join, stop_monitor,
                       strict_json, discover_native, post_join_barrier, MANIFEST_SHA, BINARY_SHA, system_uptime_ns, ClockUnavailableError)

BASE_NS = 100_000_000_000


def sample(seq=1, begin=None, last_event_ns=10_000_000_000, **changes):
    begin = begin if begin is not None else BASE_NS + (seq - 1) * 50_000_000
    rows = {}
    for state in STATES:
        rows[state] = {}
        for index, name in enumerate((*TYPES, 'anyInput')):
            at = begin + index * 10_000
            row = {'begin_ns': at, 'age_begin_ns': at + 1000,
                   'age_end_ns': at + 2000, 'end_ns': at + 3000,
                   'age_seconds': (at + 1500 - last_event_ns) / 1e9}
            if name != 'anyInput':
                row.update(count_before=10, count_after=10)
            rows[state][name] = row
    value = {'event': 'sample', 'clock_domain': CLOCK_DOMAIN, 'seq': seq, 'begin_ns': begin, 'end_ns': begin + 200_000,
             'thread_main': True, 'runner_pid': 0, 'owned_pid': 0,
             'identity_verified': False, 'owned_alive': False, 'owned_ready': False,
             'front_owned': False, 'capture_complete': False, 'sources': rows}
    value.update(changes)
    return value


def owned_sample(seq, **changes):
    defaults = dict(runner_pid=42, owned_pid=43, identity_verified=True, owned_alive=True)
    defaults.update(changes)
    return sample(seq, **defaults)


def set_event(value, state, name, event_ns):
    row = value['sources'][state][name]
    row['age_seconds'] = ((row['age_begin_ns'] + row['age_end_ns']) / 2 - event_ns) / 1e9


def armed_policy():
    policy = GuardPolicy()
    assert policy.observe(sample()).kind == 'continue'
    assert policy.observe(sample(2)).kind == 'armed'
    return policy


class PolicyTests(unittest.TestCase):
    def test_age_only_any_input(self):
        value = sample()
        self.assertIs(validate_sample(value), value)
        value['sources']['hid']['anyInput']['count_after'] = 1
        self.assertEqual(GuardPolicy().observe(value).reason, 'INVALID_OBSERVATION')

    def test_two_live_samples_before_armed(self):
        policy = GuardPolicy()
        self.assertEqual(policy.observe(sample()).kind, 'continue')
        self.assertEqual(policy.phase, 'ARMING')
        self.assertEqual(policy.observe(sample(2)).kind, 'armed')

    def test_start_during_arming_fails(self):
        policy = GuardPolicy()
        policy.observe(sample())
        self.assertEqual(policy.observe(sample(2, runner_pid=42)).reason, 'RUNNER_STARTED_BEFORE_ARMING')

    def test_no_rebaseline_at_pid_binding(self):
        policy = armed_policy()
        policy.observe(sample(3, runner_pid=42))
        value = owned_sample(4)
        value['sources']['hid']['keyDown']['count_before'] += 1
        value['sources']['hid']['keyDown']['count_after'] += 1
        self.assertEqual(policy.observe(value).reason, 'HID_COUNTER_CHANGE')

    def test_intrasample_counter_fails_at_first_baseline(self):
        value = sample()
        value['sources']['combined']['keyDown']['count_after'] += 1
        self.assertEqual(GuardPolicy().observe(value).reason, 'INPUT_DURING_ARMING')

    def test_all_counter_types_and_states_halt(self):
        for state in STATES:
            for name in TYPES:
                with self.subTest(state=state, name=name):
                    policy = armed_policy()
                    value = sample(3)
                    value['sources'][state][name]['count_before'] += 1
                    value['sources'][state][name]['count_after'] += 1
                    result = policy.observe(value)
                    self.assertEqual(result.reason, 'HID_COUNTER_CHANGE' if state == 'hid' else 'UNKNOWN_SESSION_INPUT')
                    self.assertFalse(result.details['human_attribution'])

    def test_counter_wrap_halts(self):
        policy = GuardPolicy()
        first = sample()
        first['sources']['hid']['keyDown'].update(count_before=0xffffffff, count_after=0xffffffff)
        policy.observe(first)
        self.assertEqual(policy.observe(sample(2)).reason, 'HID_COUNTER_CHANGE')

    def test_exact_20ms_bracket_boundary_not_a_new_event(self):
        old, new = sample(), sample(2)
        row = old['sources']['hid']['anyInput']
        old_latest = row['age_end_ns'] - row['age_seconds'] * 1e9
        new_row = new['sources']['hid']['anyInput']
        new_row['age_seconds'] = (new_row['age_begin_ns'] - old_latest - AGE_TOLERANCE_NS) / 1e9
        self.assertFalse(source_changes(old, new, 'hid')['new_event_time_types'])

    def test_above_20ms_bracket_boundary_halts_repeat(self):
        policy = armed_policy()
        value = sample(3)
        set_event(value, 'hid', 'anyInput', 10_000_000_000 + AGE_TOLERANCE_NS + 1_000_000)
        self.assertEqual(policy.observe(value).reason, 'HID_NEW_EVENT_TIME')

    def test_combined_age_only_change_is_unknown(self):
        policy = armed_policy()
        value = sample(3)
        set_event(value, 'combined', 'anyInput', 11_000_000_000)
        self.assertEqual(policy.observe(value).reason, 'UNKNOWN_SESSION_INPUT')

    def test_inflated_bracket_rejects_measurement_offset(self):
        old, new = sample(), sample(2)
        old['sources']['hid']['anyInput']['age_end_ns'] += 100_000
        # A 20.05ms center shift is not >20ms once the true old bracket is included.
        set_event(new, 'hid', 'anyInput', 10_020_050_000)
        self.assertFalse(source_changes(old, new, 'hid')['new_event_time_types'])

    def test_unseen_typed_event_may_have_missing_age(self):
        first, second = sample(), sample(2)
        for value in (first, second):
            value['sources']['hid']['tabletPointer'].update(count_before=0, count_after=0, age_seconds=None)
        policy = GuardPolicy()
        policy.observe(first)
        self.assertEqual(policy.observe(second).kind, 'armed')

    def test_any_input_must_have_age_even_without_counters(self):
        value = sample()
        value['sources']['hid']['anyInput']['age_seconds'] = None
        self.assertEqual(GuardPolicy().observe(value).reason, 'INVALID_OBSERVATION')

    def test_missing_age_with_counter_fails(self):
        value = sample()
        value['sources']['hid']['keyUp']['age_seconds'] = None
        self.assertEqual(GuardPolicy().observe(value).reason, 'INVALID_OBSERVATION')

    def test_source_time_backwards_halts(self):
        policy = armed_policy()
        value = sample(3)
        set_event(value, 'hid', 'keyUp', 9_000_000_000)
        self.assertEqual(policy.observe(value).reason, 'SENSOR_CLOCK_DISCONTINUITY')

    def test_bad_schema_fields_fail_closed(self):
        mutations = [lambda x: x.update(seq=True), lambda x: x.update(thread_main=False),
                     lambda x: x.update(owned_pid=-1), lambda x: x.update(end_ns=1),
                     lambda x: x['sources'].pop('hid'),
                     lambda x: x['sources']['hid']['keyUp'].update(count_before=-1),
                     lambda x: x['sources']['hid']['keyUp'].update(age_seconds=float('nan')),
                     lambda x: x['sources']['hid']['keyUp'].update(age_seconds=float('inf')),
                     lambda x: x['sources']['hid']['keyUp'].update(age_begin_ns=1)]
        for mutation in mutations:
            value = sample()
            mutation(value)
            self.assertEqual(GuardPolicy().observe(value).reason, 'INVALID_OBSERVATION')

    def test_sequence_gap_fails(self):
        policy = armed_policy()
        self.assertEqual(policy.observe(sample(4)).reason, 'OBSERVATION_GAP_OR_CLOCK_ORDER')

    def test_gap_clock_backwards_and_too_long(self):
        for begin in (BASE_NS, BASE_NS + 2 * MAX_SAMPLE_GAP_NS):
            policy = armed_policy()
            self.assertEqual(policy.observe(sample(3, begin=begin)).reason, 'OBSERVATION_GAP_OR_CLOCK_ORDER')

    def test_initial_nonowned_foreground_does_not_halt(self):
        policy = armed_policy()
        self.assertEqual(policy.observe(owned_sample(3)).kind, 'continue')
        self.assertEqual(policy.phase, 'OWNED_STARTED')

    def test_first_foreground_before_window_ready_latches_loss(self):
        policy = armed_policy()
        policy.observe(owned_sample(3, front_owned=True))
        self.assertEqual(policy.observe(owned_sample(4)).reason, 'FOCUS_LOST_AFTER_FIRST_FOREGROUND')

    def test_passive_wait_for_product_focus_then_loss_halts(self):
        policy = armed_policy()
        self.assertEqual(policy.observe(owned_sample(3, owned_ready=True)).kind, 'continue')
        self.assertEqual(policy.phase, 'OWNED_READY_AWAIT_FOREGROUND')
        self.assertIsNone(policy.first_foreground_ns)
        self.assertEqual(policy.observe(owned_sample(4, owned_ready=True)).kind, 'continue')
        self.assertEqual(policy.observe(owned_sample(5, owned_ready=True, front_owned=True)).kind, 'continue')
        self.assertEqual(policy.phase, 'ACTIVE')
        self.assertEqual(policy.observe(owned_sample(6, owned_ready=True)).reason, 'FOCUS_LOST_AFTER_FIRST_FOREGROUND')

    def test_already_foreground_requires_no_activation(self):
        policy = armed_policy()
        self.assertEqual(policy.observe(owned_sample(3, owned_ready=True, front_owned=True)).kind, 'continue')
        self.assertIsNone(policy.foreground_wait_started_ns)

    def test_external_activation_is_absent(self):
        swift = Path(__file__).with_name('interference.swift').read_text()
        supervisor = Path(__file__).with_name('supervise.py').read_text()
        self.assertNotIn('.activate(', swift)
        self.assertNotIn('activateApprovedSeq', swift + supervisor)
        self.assertNotIn('activation_command', supervisor)
        self.assertFalse(hasattr(GuardPolicy(), 'activation_command'))

    def test_passive_foreground_deadline_without_focus_grab(self):
        policy = armed_policy()
        policy.observe(owned_sample(3, owned_ready=True))
        seq = 4
        while (seq - 3) * 50_000_000 <= FOREGROUND_LIMIT_NS:
            self.assertEqual(policy.observe(owned_sample(seq, owned_ready=True)).kind, 'continue')
            seq += 1
        self.assertEqual(policy.observe(owned_sample(seq, owned_ready=True)).reason, 'OWNED_FOREGROUND_DEADLINE')

    def test_bind_and_ready_deadlines(self):
        for owned, duration, expected in [(False, BIND_LIMIT_NS, 'OWNED_BIND_DEADLINE'),
                                          (True, READY_LIMIT_NS, 'OWNED_READY_DEADLINE')]:
            policy = armed_policy()
            seq = 3
            while True:
                value = owned_sample(seq) if owned else sample(seq, runner_pid=42)
                result = policy.observe(value)
                if result.kind == 'halt':
                    self.assertEqual(result.reason, expected)
                    break
                seq += 1
                self.assertLess(seq, 1000)

    def test_input_at_each_phase_halts(self):
        phases = [dict(runner_pid=42), dict(), dict(owned_ready=True),
                  dict(owned_ready=True, front_owned=True),
                  dict(capture_complete=True, front_owned=False, owned_alive=False)]
        for flags in phases:
            policy = armed_policy()
            policy.observe(owned_sample(3, owned_ready=True, front_owned=True))
            value = owned_sample(4, **flags)
            value['sources']['hid']['flagsChanged']['count_before'] += 1
            value['sources']['hid']['flagsChanged']['count_after'] += 1
            self.assertEqual(policy.observe(value).reason, 'HID_COUNTER_CHANGE')

    def test_capture_boundary_allows_expected_release_but_not_input(self):
        policy = armed_policy()
        policy.observe(owned_sample(3, owned_ready=True, front_owned=True))
        self.assertEqual(policy.observe(owned_sample(4, capture_complete=True, owned_alive=False)).kind, 'continue')
        self.assertEqual(policy.phase, 'CAPTURE_FINALIZED_CLEANUP')
        value = owned_sample(5, capture_complete=True, owned_alive=False)
        value['sources']['combined']['mouseMoved']['count_before'] += 1
        value['sources']['combined']['mouseMoved']['count_after'] += 1
        self.assertEqual(policy.observe(value).reason, 'UNKNOWN_SESSION_INPUT')

    def test_completion_before_foreground_and_early_exit_halt(self):
        policy = armed_policy()
        self.assertEqual(policy.observe(owned_sample(3, capture_complete=True)).reason, 'CAPTURE_COMPLETE_BEFORE_FOREGROUND')
        policy = armed_policy()
        self.assertEqual(policy.observe(owned_sample(3, owned_alive=False)).reason, 'OWNED_APP_EXITED_BEFORE_CAPTURE_COMPLETE')

    def test_changed_or_removed_binding_halts(self):
        for changes, reason in [({'owned_pid': 99}, 'OWNED_IDENTITY_CHANGED'),
                                ({'runner_pid': 99}, 'RUNNER_IDENTITY_CHANGED')]:
            policy = armed_policy()
            policy.observe(owned_sample(3))
            self.assertEqual(policy.observe(owned_sample(4, **changes)).reason, reason)
        policy = armed_policy()
        policy.observe(owned_sample(3))
        self.assertEqual(policy.observe(sample(4, runner_pid=42)).reason, 'OWNED_BINDING_REMOVED')

    def test_first_cause_is_immutable(self):
        policy = GuardPolicy()
        first = policy.stop('FIRST')
        self.assertIs(policy.stop('SECOND'), first)
        self.assertIs(policy.observe({}), first)


class FakeMonitor:
    pid = 17
    returncode = None
    def poll(self):
        return self.returncode


class StreamTests(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory()
        self.addCleanup(self.temp.cleanup)
        self.path = Path(self.temp.name) / 'observations'
        self.path.touch()
        self.tail = FileTail(self.path)
        self.addCleanup(self.tail.close)
        self.monitor = FakeMonitor()
        self.now = BASE_NS + 1_000_000
        self.stream = ObservationStream(self.tail, self.monitor, 'test', clock=lambda: self.now)

    def append(self, value):
        value = dict(value, protocol_version=3, nonce='test', clock_domain=CLOCK_DOMAIN)
        with self.path.open('ab') as handle:
            handle.write(json.dumps(value).encode() + b'\n')

    def start(self):
        self.append(dict(event='ready', monitor_pid=17, thread_main=True, at_ns=self.now))
        self.append(sample())
        self.assertIsNone(self.stream.pump())
        self.now = BASE_NS + 51_000_000
        self.append(sample(2))
        self.assertIsNone(self.stream.pump())
        self.assertEqual(self.stream.policy.phase, 'ARMED')

    def test_live_file_tail_nonblocking_and_split_line(self):
        start = time.monotonic()
        self.assertEqual(self.tail.read(), [])
        self.assertLess(time.monotonic() - start, .1)
        with self.path.open('ab') as handle:
            handle.write(b'{"a":')
        self.assertEqual(self.tail.read(), [])
        with self.path.open('ab') as handle:
            handle.write(b'1}\n')
        self.assertEqual(self.tail.read(), [{'a': 1}])

    def test_ready_then_two_samples(self):
        self.start()

    def test_stream_exit_stall_malformed_fail_closed(self):
        self.start()
        self.monitor.returncode = 7
        self.assertEqual(self.stream.pump().reason, 'MONITOR_EXITED')
        self.monitor.returncode = None
        # Fresh independent policy for each fault.
        self.stream.policy = GuardPolicy()
        self.now += MAX_SAMPLE_GAP_NS + 1
        self.assertEqual(self.stream.pump().reason, 'MONITOR_STALLED')

    def test_malformed_line_and_unknown_record(self):
        with self.path.open('ab') as handle:
            handle.write(b'not-json\n')
        self.assertEqual(self.stream.pump().reason, 'INVALID_MONITOR_STREAM')

    def test_duplicate_nonfinite_json_rejected(self):
        for value in (b'{"a":1,"a":2}', b'{"a":NaN}', b'{"a":Infinity}'):
            with self.assertRaises(ValueError):
                strict_json(value)

    def test_unterminated_oversize_fails_without_block(self):
        with self.path.open('ab') as handle:
            handle.write(b'x' * 65537)
        self.assertEqual(self.stream.pump().reason, 'INVALID_MONITOR_STREAM')

    def test_wrong_nonce_and_sample_before_ready_fail(self):
        self.append(sample())
        self.assertEqual(self.stream.pump().reason, 'INVALID_MONITOR_STREAM')

    def test_fresh_stale_future_samples(self):
        self.start()
        self.now += MAX_SAMPLE_GAP_NS * 2
        self.append(sample(3))
        self.assertEqual(self.stream.pump().reason, 'INVALID_MONITOR_STREAM')

    def test_post_join_barrier_rejects_old_queued_sample(self):
        self.start()
        self.now = BASE_NS + 151_000_000
        joined_at = BASE_NS + 120_000_000
        self.append(sample(3))  # begin=BASE+100ms, queued from before join.
        slept = []
        def sleep(seconds):
            slept.append(seconds)
            self.append(sample(4))  # begin=BASE+150ms, genuinely after join.
        self.assertTrue(post_join_barrier(self.stream, joined_at,
                                         clock=lambda: self.now, sleep=sleep))
        self.assertEqual(len(slept), 1)
        self.assertGreaterEqual(self.stream.last_valid_sample['begin_ns'], joined_at)
        self.assertEqual(self.stream.last_valid_sample['seq'], 4)

    def test_post_join_missing_fresh_sample_fails_bounded(self):
        self.start()
        joined_at = self.now
        def sleep(seconds):
            self.now += round(seconds * 1e9)
        self.assertFalse(post_join_barrier(self.stream, joined_at,
                                          clock=lambda: self.now, sleep=sleep))
        self.assertLessEqual(self.now - joined_at, MAX_SAMPLE_GAP_NS + 10_000_000)
        self.assertFalse(self.stream.integrity)

    def test_owned_credential_identity_and_parent(self):
        self.start()
        self.stream.expected_runner_pid = 42
        self.stream.expected_owned_pid = 43
        self.append(dict(event='runner_bound', credential=dict(pid=42, parent_pid=1, start_seconds=10, start_micros=5)))
        self.append(dict(event='owned_bound', credential=dict(pid=43, parent_pid=42, start_seconds=10, start_micros=6)))
        self.now = BASE_NS + 101_000_000
        self.append(owned_sample(3))
        self.assertIsNone(self.stream.pump())
        self.assertEqual(self.stream.policy.owned_pid, 43)

    def test_reused_pid_wrong_parent_or_old_start_fails(self):
        for parent, start in [(99, 11), (42, 9)]:
            with self.subTest(parent=parent, start=start):
                self.stream = ObservationStream(self.tail, self.monitor, 'test', clock=lambda: self.now)
                self.stream.expected_runner_pid = 42
                self.stream.expected_owned_pid = 43
                self.append(dict(event='ready', monitor_pid=17, thread_main=True))
                self.append(dict(event='runner_bound', credential=dict(pid=42, parent_pid=1, start_seconds=10, start_micros=0)))
                self.append(dict(event='owned_bound', credential=dict(pid=43, parent_pid=parent, start_seconds=start, start_micros=0)))
                self.assertEqual(self.stream.pump().reason, 'INVALID_MONITOR_STREAM')

    def test_unbound_owned_sample_fails(self):
        self.start()
        self.now = BASE_NS + 101_000_000
        self.append(owned_sample(3))
        self.assertEqual(self.stream.pump().reason, 'INVALID_MONITOR_STREAM')

    def test_synthetic_monitor_liveness_exit_and_own_cleanup(self):
        with self.path.open('ab') as output:
            monitor = subprocess.Popen([sys.executable, '-c', 'import time; time.sleep(.08)'], stdout=output)
            self.addCleanup(lambda: stop_monitor(monitor))
            stream = ObservationStream(self.tail, monitor, 'test', clock=lambda: self.now)
            self.assertIsNone(stream.pump())
            monitor.wait(timeout=2)
            self.assertEqual(stream.pump().reason, 'MONITOR_EXITED')


class OwnedLifecycleTests(unittest.TestCase):
    def test_sigint_only_owned_runner_then_join(self):
        with tempfile.TemporaryDirectory() as directory:
            base = Path(directory)
            code = ('import signal,time,pathlib\n'
                    'signal.signal(signal.SIGINT,lambda a,b:exit(0))\n'
                    'pathlib.Path("ready").write_text("ready")\n'
                    'while True:time.sleep(.01)\n')
            runner = subprocess.Popen([sys.executable, '-c', code], cwd=base)
            self.addCleanup(lambda: runner.kill() if runner.poll() is None else None)
            deadline = time.monotonic() + 2
            while not (base / 'ready').exists():
                self.assertLess(time.monotonic(), deadline)
                time.sleep(.01)
            unrelated = subprocess.Popen([sys.executable, '-c', 'import time;time.sleep(10)'])
            self.addCleanup(lambda: unrelated.terminate() if unrelated.poll() is None else None)
            self.assertTrue(interrupt_and_join(runner, 'TEST', base, system_uptime_ns() + 6_000_000_000))
            self.assertEqual(runner.returncode, 0)
            self.assertIsNone(unrelated.poll())
            receipt = json.loads((base / 'controlled-interrupt.json').read_text())
            self.assertEqual(receipt['runner_pid'], runner.pid)
            self.assertEqual(receipt['signal'], signal.SIGINT)
            unrelated.terminate()
            unrelated.wait(timeout=2)

    def test_cleanup_timeout_does_not_kill_runner_or_others(self):
        class FakeRunner:
            pid = 123
            returncode = None
            signals = []
            def poll(self): return None
            def send_signal(self, value): self.signals.append(value)
            def wait(self, timeout): raise subprocess.TimeoutExpired('owned', timeout)
        with tempfile.TemporaryDirectory() as directory:
            runner = FakeRunner()
            self.assertFalse(interrupt_and_join(runner, 'TEST', Path(directory), 40_000_000_000, clock=lambda: 1_000_000_000))
            self.assertEqual(runner.signals, [signal.SIGINT])

    def test_clock_loss_after_prior_interrupt_cannot_restart_cleanup_budget(self):
        class Runner:
            pid = 123
            returncode = None
            signals = []
            timeouts = []
            def poll(self): return None
            def send_signal(self, value): self.signals.append(value)
            def wait(self, timeout):
                self.timeouts.append(timeout)
                raise subprocess.TimeoutExpired('owned', timeout)
        def lost_clock():
            raise ClockUnavailableError('synthetic native uptime lost')
        with tempfile.TemporaryDirectory() as directory:
            runner = Runner()
            errors = []
            self.assertFalse(interrupt_and_join(runner, 'TEST', Path(directory),
                                               deadline_ns=BASE_NS, clock=lost_clock,
                                               signal_already_sent=True, clock_errors=errors))
            self.assertEqual(runner.signals, [])
            self.assertEqual(runner.timeouts, [0])
            self.assertEqual(len(errors), 1)

    def test_candidate_marker_through_symlinked_fixture_root(self):
        with tempfile.TemporaryDirectory() as outer:
            parent = Path(outer).resolve(strict=True)
            target = parent / 'real'
            target.mkdir()
            alias = parent / 'alias'
            alias.symlink_to(target, target_is_directory=True)
            with tempfile.TemporaryDirectory(dir=alias) as directory:
                root = Path(directory).resolve(strict=True)
                native = root / 'rcam-pmix-test'
                native.mkdir()
                binary = root / 'binary'
                binary.write_text('synthetic')
                request = dict(schema_version=2, mode='workflow-reopen', round=1,
                               source_manifest_sha256=MANIFEST_SHA, display_policy='preserve',
                               display_mode_change_authorized=False,
                               run_id='00000000-0000-4000-8000-000000000001')
                owned = dict(pid=43, command=[str(binary)], binary_sha256=BINARY_SHA)
                (native / 'request.json').write_text(json.dumps(request))
                (native / 'owned-process.json').write_text(json.dumps(owned))
                with patch.object(Path, 'glob', return_value=[native]):
                    found = discover_native(set(), binary)
                self.assertEqual(found[0], native)
                self.assertEqual(found[1], owned)

    def test_candidate_uses_exact_original_marker_fields(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory).resolve(strict=True)
            native = root / 'rcam-pmix-test'
            native.mkdir()
            binary = root / 'binary'
            binary.write_text('synthetic')
            request = dict(schema_version=2, mode='workflow-reopen', round=1,
                           source_manifest_sha256=MANIFEST_SHA, display_policy='preserve',
                           display_mode_change_authorized=False,
                           run_id='00000000-0000-4000-8000-000000000001')
            owned = dict(pid=43, command=[str(binary)], binary_sha256=BINARY_SHA)
            (native / 'request.json').write_text(json.dumps(request))
            (native / 'owned-process.json').write_text(json.dumps(owned))
            with patch.object(Path, 'glob', return_value=[native]):
                found = discover_native(set(), binary)
            self.assertEqual(found[1], owned)
            self.assertEqual(found[2]['run_id'], request['run_id'])
            self.assertNotIn('launchDate', found[1])


class InjectedSupervisorTests(unittest.TestCase):
    """Exercise the complete wrapper with deterministic owned-only fake actors."""
    def exercise(self, fault=None, temporary_parent=None, clock_failure=None):
        temporary = tempfile.TemporaryDirectory(dir=temporary_parent)
        self.addCleanup(temporary.cleanup)
        root = Path(temporary.name).resolve(strict=True)
        base = root / 'evidence'
        base.mkdir()
        product = root / 'product'
        (product / 'scripts').mkdir(parents=True)
        (product / 'MANIFEST.sha256').write_text('synthetic')
        (product / 'scripts/run_pmix_native.py').write_text('synthetic')
        gates = root / 'background-gates-09'
        binary = gates / 'bin/editor-app-release-internal'
        producer = gates / 'capture-writer-preflight/capture-producer'
        binary.parent.mkdir(parents=True)
        producer.parent.mkdir(parents=True)
        binary.write_text('synthetic')
        producer.write_text('synthetic')
        native = root / 'rcam-pmix-synthetic'
        native.mkdir()
        request = dict(run_id='00000000-0000-4000-8000-000000000001')
        owned = dict(pid=43)
        now = [BASE_NS]
        actors = []
        trace = []
        self.clock_fault_trace = trace
        class Actor:
            def __init__(self, pid):
                self.pid, self.returncode = pid, None
                self.signals = []
                self.wait_timeouts = []
            def poll(self):
                return self.returncode
            def send_signal(self, value):
                self.signals.append(value)
                trace.append(("signal", self.pid, int(value)))
                self.returncode = -int(value)
            def wait(self, timeout):
                self.wait_timeouts.append(timeout)
                return self.returncode
            def terminate(self):
                self.returncode = -int(signal.SIGTERM)
            def kill(self):
                raise AssertionError('synthetic actor must not need kill')
        class Tail:
            def __init__(self, path):
                self.seq = 0
                self.last_clock = None
                self.runner_bound = self.owned_bound = False
            def close(self):
                pass
            def read(self):
                if self.last_clock == now[0]:
                    return []
                self.last_clock = now[0]
                control = json.loads((base / 'monitor-control.json').read_text())
                envelope = dict(protocol_version=3, nonce=control['nonce'], clock_domain=CLOCK_DOMAIN)
                records = []
                if self.seq == 0:
                    records.append(dict(event='ready', monitor_pid=17, thread_main=True, **envelope))
                if fault == 'monitor_exit_before_arm':
                    actors[0].returncode = 2
                    return records
                if control['runnerPID'] and not self.runner_bound:
                    records.append(dict(event='runner_bound', credential=dict(pid=42, parent_pid=1,
                                         start_seconds=10, start_micros=0), **envelope))
                    self.runner_bound = True
                if control['appPID'] and not self.owned_bound:
                    records.append(dict(event='owned_bound', credential=dict(pid=43, parent_pid=42,
                                         start_seconds=10, start_micros=1), **envelope))
                    self.owned_bound = True
                self.seq += 1
                value = sample(self.seq, begin=now[0] - 1_000_000,
                               runner_pid=control['runnerPID'])
                if control['appPID']:
                    value.update(owned_pid=43, identity_verified=True, owned_alive=True,
                                 owned_ready=True, front_owned=True)
                if fault == 'input_after_binding' and control['appPID']:
                    value['sources']['combined']['keyDown']['count_before'] += 1
                    value['sources']['combined']['keyDown']['count_after'] += 1
                if self.seq >= 6 and control['appPID']:
                    value.update(capture_complete=True, owned_alive=False, front_owned=False)
                    actors[1].returncode = 0
                records.append(dict(value, **envelope))
                return records
        def launch(command, **kwargs):
            actor = Actor(17 if 'interference-monitor' in command[0] else 42)
            actors.append(actor)
            return actor
        def fake_sha(path):
            if path == binary: return supervise.BINARY_SHA
            if path == producer: return supervise.PRODUCER_SHA
            if path.name == 'MANIFEST.sha256': return supervise.MANIFEST_SHA
            return supervise.RUNNER_SHA
        def sleep(seconds):
            now[0] += round(seconds * 1e9)
        def native_clock(clock_id):
            broken = ((clock_failure == 'after_runner_launch' and len(actors) >= 2) or
                      (clock_failure == 'runner_completed' and len(actors) >= 2 and actors[1].returncode == 0))
            if broken:
                trace.append(('clock-failure', clock_failure))
                raise OSError('synthetic native uptime lost')
            return now[0]
        with patch.object(supervise.sys, 'platform', 'darwin'), \
             patch.object(supervise, 'ROOT', product), \
             patch.object(supervise, 'sha', side_effect=fake_sha), \
             patch.object(supervise.subprocess, 'run', return_value=type('Compiled', (), {'returncode': 0})()) as compile_call, \
             patch.object(supervise.subprocess, 'Popen', side_effect=launch), \
             patch.object(supervise, 'FileTail', Tail), \
             patch.object(supervise, 'discover_native', return_value=(native, owned, request)), \
             patch.object(supervise.time, 'CLOCK_UPTIME_RAW', 8, create=True), \
             patch.object(supervise.time, 'clock_gettime_ns', side_effect=native_clock), \
             patch.object(supervise.time, 'monotonic', return_value=.036), \
             patch.object(supervise.time, 'monotonic_ns', return_value=36_000_000), \
             patch.object(supervise.time, 'sleep', side_effect=sleep), \
             patch('builtins.print'):
            result = supervise.run(base)
        compile_call.assert_called_once()
        self.last_compile_command = compile_call.call_args.args[0]
        receipt = json.loads((base / 'SUPERVISOR_RESULT.json').read_text())
        return result, receipt, actors

    def test_missing_local_checkout_config_stops_before_process_launch(self):
        with patch.object(supervise.sys, 'platform', 'darwin'), \
             patch.object(supervise, 'ROOT', None), \
             patch.object(supervise.subprocess, 'Popen') as launch:
            with self.assertRaisesRegex(RuntimeError, 'RCAM_GUARD_ROOT'):
                supervise.run()
            launch.assert_not_called()

    def test_symlinked_temporary_root_keeps_strict_hash_and_owned_identity(self):
        # Model macOS /var -> /private/var without changing TMPDIR or production.
        with tempfile.TemporaryDirectory() as outer:
            root = Path(outer).resolve(strict=True)
            target = root / 'real-temporary-parent'
            target.mkdir()
            alias = root / 'symlinked-temporary-parent'
            alias.symlink_to(target, target_is_directory=True)
            code, receipt, actors = self.exercise(temporary_parent=alias)
            self.assertEqual(code, 0)
            self.assertTrue(receipt['success'])
            self.assertTrue(Path(receipt['native_directory']).is_relative_to(target))
            self.assertEqual(actors[1].signals, [])

    def test_native_clock_loss_after_owned_launch_still_interrupts_and_receipts(self):
        code, receipt, actors = self.exercise(clock_failure='after_runner_launch')
        self.assertEqual(code, 2)
        self.assertFalse(receipt['success'])
        self.assertTrue(receipt['clock_unavailable'])
        self.assertIsNone(receipt['duration_seconds'])
        self.assertIsNone(receipt['runner_joined_at_ns'])
        self.assertFalse(receipt['post_join_barrier_satisfied'])
        self.assertTrue(receipt['joined'])
        self.assertTrue(receipt['monitor_joined'])
        self.assertEqual(actors[1].signals, [signal.SIGINT])
        self.assertEqual(actors[1].wait_timeouts, [35.75])
        first_fault = next(i for i, row in enumerate(self.clock_fault_trace) if row[0] == 'clock-failure')
        self.assertEqual(self.clock_fault_trace[first_fault + 1], ('signal', 42, int(signal.SIGINT)))

    def test_native_clock_loss_at_runner_completion_stops_monitor_and_receipts(self):
        code, receipt, actors = self.exercise(clock_failure='runner_completed')
        self.assertEqual(code, 2)
        self.assertFalse(receipt['success'])
        self.assertTrue(receipt['clock_unavailable'])
        self.assertIsNone(receipt['duration_seconds'])
        self.assertFalse(receipt['post_join_barrier_satisfied'])
        self.assertTrue(receipt['joined'])
        self.assertTrue(receipt['monitor_joined'])
        self.assertEqual(actors[1].returncode, 0)
        self.assertEqual(actors[1].signals, [])
        self.assertEqual(actors[0].returncode, -int(signal.SIGTERM))

    def test_python39_process_epoch_does_not_enter_guard_timestamps(self):
        # exercise uses Darwin uptime=BASE_NS and Python monotonic=36ms,
        # and the unmodified/default ObservationStream clock factory.
        code, receipt, actors = self.exercise()
        self.assertEqual(code, 0)
        self.assertTrue(receipt['success'])
        self.assertEqual(receipt['clock_domain'], CLOCK_DOMAIN)
        self.assertGreaterEqual(receipt['runner_joined_at_ns'], BASE_NS)
        self.assertGreaterEqual(receipt['post_join_sample_begin_ns'], receipt['runner_joined_at_ns'])
        self.assertLess(receipt['duration_seconds'], 1)
        self.assertEqual(receipt['execution_budget_seconds'], 190)
        self.assertEqual(receipt['cleanup_budget_seconds'], 40)

    def test_strict_swift6_compile_contract(self):
        self.exercise()
        command = self.last_compile_command
        self.assertEqual(command[0], '/usr/bin/swiftc')
        self.assertIn('-parse-as-library', command)
        self.assertEqual(command[command.index('-swift-version') + 1], '6')
        self.assertIn('-strict-concurrency=complete', command)
        self.assertIn('-warnings-as-errors', command)
        self.assertNotIn('-suppress-warnings', command)
        self.assertNotIn('-strict-concurrency=minimal', command)

    def test_complete_wrapper_success_with_owned_finalized_cleanup(self):
        code, receipt, actors = self.exercise()
        self.assertEqual(code, 0)
        self.assertTrue(receipt['success'])
        self.assertTrue(receipt['joined'])
        self.assertTrue(receipt['monitor_joined'])
        self.assertTrue(receipt['post_join_barrier_satisfied'])
        self.assertGreaterEqual(receipt['post_join_sample_begin_ns'], receipt['runner_joined_at_ns'])
        self.assertFalse(receipt['external_activation'])
        self.assertIsNone(receipt['interrupted'])
        self.assertEqual(actors[1].signals, [])
        self.assertIsNotNone(receipt['first_foreground_ns'])
        self.assertIsNotNone(receipt['capture_completed_ns'])

    def test_monitor_failure_prevents_any_runner_launch(self):
        code, receipt, actors = self.exercise('monitor_exit_before_arm')
        self.assertEqual(code, 2)
        self.assertEqual(len(actors), 1)
        self.assertIsNone(receipt['runner_pid'])
        self.assertFalse(receipt['success'])

    def test_unknown_input_interrupts_only_owned_runner(self):
        code, receipt, actors = self.exercise('input_after_binding')
        self.assertEqual(code, 2)
        self.assertEqual(receipt['interrupted'], 'UNKNOWN_SESSION_INPUT')
        self.assertEqual(actors[1].signals, [signal.SIGINT])
        self.assertTrue(receipt['joined'])
        self.assertTrue(receipt['monitor_joined'])
        self.assertFalse(receipt['human_input_attributed'])
        self.assertTrue(receipt['post_join_barrier_satisfied'])
        self.assertGreaterEqual(receipt['post_join_sample_begin_ns'], receipt['runner_joined_at_ns'])


class PostJoinRegressionTests(unittest.TestCase):
    def test_v2_detects_cleanup_input_after_last_sample_before_join(self):
        with tempfile.TemporaryDirectory(prefix='rcam-review-fixture-') as d:
            root = Path(d).resolve(strict=True)
            base = root / 'evidence'; base.mkdir()
            product = root / 'product'; (product / 'scripts').mkdir(parents=True)
            (product / 'MANIFEST.sha256').write_text('synthetic')
            (product / 'scripts/run_pmix_native.py').write_text('synthetic')
            gates = root / 'background-gates-09'
            binary = gates / 'bin/editor-app-release-internal'; binary.parent.mkdir(parents=True)
            producer = gates / 'capture-writer-preflight/capture-producer'; producer.parent.mkdir(parents=True)
            binary.write_text('synthetic'); producer.write_text('synthetic')
            native = root / 'rcam-pmix-synthetic'; native.mkdir()
            now = [100_000_000_000]
            actors = []
            facts = {'capture_seen': False, 'physical_counter': 10, 'input_at_ns': None,
                     'runner_join_observed_at_ns': None, 'last_sample_end_ns': None}
            class Actor:
                def __init__(self, pid): self.pid, self.returncode = pid, None
                def poll(self):
                    if self.pid == 42 and facts['capture_seen'] and self.returncode is None:
                        now[0] += 2_000_000
                        facts['input_at_ns'] = now[0] - 1_000_000
                        facts['physical_counter'] = 11
                        facts['runner_join_observed_at_ns'] = now[0]
                        facts['last_sample_end_before_join_ns'] = facts['last_sample_end_ns']
                        self.returncode = 0
                    return self.returncode
                def send_signal(self, value): raise AssertionError('unexpected interrupt')
                def wait(self, timeout): return self.returncode
                def terminate(self): self.returncode = -int(signal.SIGTERM)
                def kill(self): raise AssertionError('unexpected kill')
            class Tail:
                def __init__(self, path):
                    self.seq = 0; self.last_at = None; self.rb = self.ob = False
                def close(self): pass
                def read(self):
                    if self.last_at is not None and now[0] - self.last_at < 50_000_000: return []
                    self.last_at = now[0]
                    ctl = json.loads((base / 'monitor-control.json').read_text())
                    env = dict(protocol_version=3, nonce=ctl['nonce'], clock_domain=CLOCK_DOMAIN)
                    rows = []
                    if self.seq == 0: rows.append(dict(event='ready', monitor_pid=17, thread_main=True, **env))
                    if ctl['runnerPID'] and not self.rb:
                        rows.append(dict(event='runner_bound', credential=dict(pid=42, parent_pid=1, start_seconds=10, start_micros=0), **env)); self.rb = True
                    if ctl['appPID'] and not self.ob:
                        rows.append(dict(event='owned_bound', credential=dict(pid=43, parent_pid=42, start_seconds=10, start_micros=1), **env)); self.ob = True
                    self.seq += 1
                    s = sample(self.seq, begin=now[0]-1_000_000, runner_pid=ctl['runnerPID'])
                    if ctl['appPID']:
                        s.update(owned_pid=43, identity_verified=True, owned_alive=True, owned_ready=True, front_owned=True)
                    if self.seq >= 5 and ctl['appPID']:
                        s['capture_complete'] = True
                        facts['capture_seen'] = True
                    for state in STATES:
                        s['sources'][state]['keyDown'].update(count_before=facts['physical_counter'], count_after=facts['physical_counter'])
                    facts['last_sample_end_ns'] = s['end_ns']
                    rows.append(dict(s, **env))
                    return rows
            def launch(command, **kwargs):
                actor = Actor(17 if 'interference-monitor' in command[0] else 42)
                actors.append(actor); return actor
            def hash_of(path):
                if path == binary: return supervise.BINARY_SHA
                if path == producer: return supervise.PRODUCER_SHA
                return supervise.MANIFEST_SHA if path.name == 'MANIFEST.sha256' else supervise.RUNNER_SHA
            def sleep(seconds): now[0] += round(seconds * 1e9)
            real_stream = supervise.ObservationStream
            with patch.object(supervise.sys, 'platform', 'darwin'), patch.object(supervise, 'ROOT', product), \
                 patch.object(supervise, 'sha', side_effect=hash_of), \
                 patch.object(supervise.subprocess, 'run', return_value=type('Compile', (), {'returncode':0})()), \
                 patch.object(supervise.subprocess, 'Popen', side_effect=launch), patch.object(supervise, 'FileTail', Tail), \
                 patch.object(supervise, 'discover_native', return_value=(native, {'pid':43}, {'run_id':'00000000-0000-4000-8000-000000000001'})), \
                 patch.object(supervise.time, 'CLOCK_UPTIME_RAW', 8, create=True), \
                 patch.object(supervise.time, 'clock_gettime_ns', side_effect=lambda clock_id:now[0]), \
                 patch.object(supervise.time, 'monotonic', return_value=.036), \
                 patch.object(supervise.time, 'monotonic_ns', return_value=36_000_000), patch.object(supervise.time, 'sleep', side_effect=sleep), \
                 patch('builtins.print'):
                code = supervise.run(base)
            result = json.loads((base / 'SUPERVISOR_RESULT.json').read_text())
            self.assertEqual(code, 2)
            self.assertFalse(result['success'])
            self.assertEqual(result['interrupted'], 'HID_COUNTER_CHANGE')
            self.assertTrue(result['post_join_barrier_satisfied'])
            self.assertGreaterEqual(result['post_join_sample_begin_ns'], result['runner_joined_at_ns'])
            self.assertLess(facts['last_sample_end_before_join_ns'], facts['input_at_ns'])
            self.assertLess(facts['input_at_ns'], facts['runner_join_observed_at_ns'])
            print(json.dumps({'fixed_regression':'cleanup input detected before any success', 'receipt':result, 'synthetic_facts':facts}, indent=2))


class SwiftActorBoundaryTests(unittest.TestCase):
    def setUp(self):
        self.swift = Path(__file__).with_name('interference.swift').read_text()
        start = self.swift.index('@MainActor\nprivate final class InterferenceObserver')
        end = self.swift.index('@main\nprivate struct InterferenceObserverMain')
        self.observer = self.swift[start:end]
        self.outside = self.swift[:start] + self.swift[end:]

    def test_mutable_state_and_json_remain_actor_owned(self):
        for name in ('current', 'lastCommand', 'runnerCredential', 'ownedCredential',
                     'ownedApplication', 'sequence'):
            self.assertIn('private var ' + name, self.observer)
            self.assertNotIn('var ' + name, self.outside)
        self.assertNotIn('[String: Any]', self.outside)
        for method in ('emit', 'fatal', 'identityAlive', 'applyControl', 'readyMarker', 'sample'):
            self.assertIn('private func ' + method, self.observer)
            self.assertNotIn('func ' + method, self.outside)

    def test_explicit_mainactor_entry_and_timer_boundary(self):
        self.assertIn('@main\nprivate struct InterferenceObserverMain {\n    @MainActor\n    static func main()', self.swift)
        self.assertIn('observer.run()', self.outside)
        self.assertIn('MainActor.assumeIsolated {', self.observer)
        self.assertIn('self.sample()', self.observer)
        self.assertIn('RunLoop.main.add(timer, forMode: .common)', self.observer)
        self.assertIn('MONITOR_NOT_MAIN_THREAD', self.observer)
        self.assertIn('MONITOR_MAIN_RUNLOOP_RETURNED', self.observer)

    def test_no_unsafe_concurrency_opt_out_or_external_activation(self):
        for opt_out in ('@unchecked Sendable', 'nonisolated(unsafe)', '@preconcurrency',
                        '.activate(', 'activateApprovedSeq'):
            self.assertNotIn(opt_out, self.swift)
        for value in ('Control', 'Credential', 'ReadyIdentity'):
            declaration = next(line for line in self.swift.splitlines() if line.startswith('struct ' + value + ':'))
            self.assertIn('Sendable', declaration)


class DarwinClockDomainTests(unittest.TestCase):
    def test_darwin_uses_native_uptime_even_with_python39_process_epoch(self):
        with patch.object(supervise.sys, 'platform', 'darwin'), \
             patch.object(supervise.time, 'CLOCK_UPTIME_RAW', 8, create=True), \
             patch.object(supervise.time, 'clock_gettime_ns', return_value=BASE_NS) as raw, \
             patch.object(supervise.time, 'monotonic_ns', side_effect=AssertionError('process epoch forbidden')):
            self.assertEqual(system_uptime_ns(), BASE_NS)
            raw.assert_called_once_with(8)

    def test_missing_uptime_constant_never_falls_back(self):
        with patch.object(supervise.sys, 'platform', 'darwin'), \
             patch.object(supervise.time, 'CLOCK_UPTIME_RAW', None, create=True), \
             patch.object(supervise.time, 'monotonic_ns', side_effect=AssertionError('fallback forbidden')):
            with self.assertRaisesRegex(RuntimeError, 'CLOCK_UPTIME_RAW unavailable'):
                system_uptime_ns()

    def test_native_clock_errors_and_noninteger_values_fail_closed(self):
        with patch.object(supervise.sys, 'platform', 'darwin'), \
             patch.object(supervise.time, 'CLOCK_UPTIME_RAW', 8, create=True), \
             patch.object(supervise.time, 'clock_gettime_ns', side_effect=OSError('native clock unavailable')):
            with self.assertRaises(ClockUnavailableError):
                system_uptime_ns()
        for value in (None, True, 0, -1, 1.5):
            with patch.object(supervise.sys, 'platform', 'darwin'), \
                 patch.object(supervise.time, 'CLOCK_UPTIME_RAW', 8, create=True), \
                 patch.object(supervise.time, 'clock_gettime_ns', return_value=value):
                with self.assertRaisesRegex(RuntimeError, 'invalid system uptime clock'):
                    system_uptime_ns()

    def test_default_stream_factory_accepts_same_uptime_sample(self):
        class Rows:
            def read(self):
                return [dict(event='ready', monitor_pid=17, thread_main=True,
                             protocol_version=3, nonce='domain', clock_domain=CLOCK_DOMAIN),
                        dict(sample(), protocol_version=3, nonce='domain')]
        with patch.object(supervise.sys, 'platform', 'darwin'), \
             patch.object(supervise.time, 'CLOCK_UPTIME_RAW', 8, create=True), \
             patch.object(supervise.time, 'clock_gettime_ns', return_value=BASE_NS + 1_000_000), \
             patch.object(supervise.time, 'monotonic_ns', return_value=36_000_000):
            stream = ObservationStream(Rows(), FakeMonitor(), 'domain')
            self.assertIsNone(stream.pump())
            self.assertEqual(stream.last_sample_ns, sample()['end_ns'])

    def test_domain_labels_are_required_by_policy_and_stream(self):
        value = sample()
        value['clock_domain'] = 'process_relative'
        self.assertEqual(GuardPolicy().observe(value).reason, 'INVALID_OBSERVATION')
        class Rows:
            def read(self):
                return [dict(event='ready', monitor_pid=17, thread_main=True,
                             protocol_version=3, nonce='domain', clock_domain='process_relative')]
        stream = ObservationStream(Rows(), FakeMonitor(), 'domain', clock=lambda: BASE_NS)
        self.assertEqual(stream.pump().reason, 'INVALID_MONITOR_STREAM')

    def test_uptime_domain_keeps_exact_freshness_bound(self):
        for age, expected in [(MAX_SAMPLE_GAP_NS, None),
                              (MAX_SAMPLE_GAP_NS + 1, 'INVALID_MONITOR_STREAM')]:
            value = sample()
            class Rows:
                def read(self):
                    return [dict(event='ready', monitor_pid=17, thread_main=True,
                                 protocol_version=3, nonce='domain', clock_domain=CLOCK_DOMAIN),
                            dict(value, protocol_version=3, nonce='domain')]
            with patch.object(supervise.sys, 'platform', 'darwin'), \
                 patch.object(supervise.time, 'CLOCK_UPTIME_RAW', 8, create=True), \
                 patch.object(supervise.time, 'clock_gettime_ns', return_value=value['end_ns'] + age), \
                 patch.object(supervise.time, 'monotonic_ns', return_value=36_000_000):
                stream = ObservationStream(Rows(), FakeMonitor(), 'domain')
                action = stream.pump()
                self.assertEqual(action.reason if action else None, expected)

    def test_postjoin_default_factory_uses_uptime_without_epoch_offset(self):
        class Stream:
            integrity = True
            last_valid_sample = None
            policy = GuardPolicy()
            def pump(self):
                self.last_valid_sample = dict(begin_ns=BASE_NS + 2_000_000)
        with patch.object(supervise.sys, 'platform', 'darwin'), \
             patch.object(supervise.time, 'CLOCK_UPTIME_RAW', 8, create=True), \
             patch.object(supervise.time, 'clock_gettime_ns', return_value=BASE_NS + 3_000_000), \
             patch.object(supervise.time, 'monotonic_ns', side_effect=AssertionError('process epoch forbidden')):
            self.assertTrue(post_join_barrier(Stream(), BASE_NS + 1_000_000))

    def test_no_darwin_monotonic_binding_outside_explicit_test_fallback(self):
        source = Path(__file__).with_name('supervise.py').read_text()
        self.assertEqual(source.count('time.monotonic_ns()'), 1)
        self.assertNotIn('time.monotonic()', source)
        self.assertNotIn('clock=time.monotonic', source)
        self.assertIn("value = reader(clock_id)", source)
        swift = Path(__file__).with_name('interference.swift').read_text()
        self.assertIn('DispatchTime.now().uptimeNanoseconds', swift)
        self.assertIn('envelope["clock_domain"] = "darwin_uptime_raw_ns"', swift)


if __name__ == '__main__':
    unittest.main(verbosity=2)
