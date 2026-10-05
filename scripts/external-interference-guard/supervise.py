"""External RCam microcheck supervisor; imports are safe and side-effect free.

Start only from a fresh evidence directory alongside background-gates-09.
No product sources, manifest, display modes, or third-party processes are changed.
"""
import datetime
import hashlib
import json
import os
from pathlib import Path
import signal
import subprocess
import sys
import time
import uuid
from dataclasses import dataclass
import re

from guard_policy import GuardPolicy, MAX_SAMPLE_GAP_NS, validate_sample, CLOCK_DOMAIN

# Configure the checkout locally; never publish an operator's filesystem path.
ROOT = Path(os.environ['RCAM_GUARD_ROOT']) if os.environ.get('RCAM_GUARD_ROOT') else None
MANIFEST_SHA = 'e0ae3aed5175ed735a5725d321293b574c93672ebd7dce8b23127b91a4cbc1ae'
BINARY_SHA = '13cd6e5cc985963197f2eb4288fce1458c37a57eb2deb9855a6581acf9afd69c'
PRODUCER_SHA = 'ef7984f492b3dcee31166bfdff432ec47589dbf0e24d03dc86c69f7535711a1c'
RUNNER_SHA = '5251d99e4e6313590cde31aae17523d28039f07649de4168d6e992a1cd25eda7'
EXECUTION_SECONDS = 190
CLEANUP_SECONDS = 40
MONITOR_STOP_SECONDS = 4
MAX_LINE_BYTES = 65536
MAX_BATCH_BYTES = 1048576
POST_JOIN_BARRIER_SECONDS = MAX_SAMPLE_GAP_NS / 1e9
MONITOR_ARM_LIMIT_NS = 5_000_000_000


@dataclass(frozen=True)
class FullRun:
    """One immutable formal case. New product pins must be supplied independently."""
    root: Path
    binary: Path
    producer: Path
    output: Path
    mode: str
    round: int
    display_id: int
    manifest_sha: str
    binary_sha: str
    producer_sha: str
    runner_sha: str
    fixture: object = None

    def validate(self):
        require(self.mode in ('nav', 'move', 'points', 'escape', 'new-project',
                             'workflow', 'workflow-reopen', 'workflow-cross-layer'), 'formal mode')
        require(type(self.round) is int and self.round in ((1, 2, 3) if self.mode in ('nav', 'move') else (1,)),
                'formal round')
        require(type(self.display_id) is int and self.display_id == 2, 'authorized target display')
        require((self.mode == 'workflow-reopen') == (self.fixture is not None), 'formal reopen fixture')
        require(all(type(value) is str and re.fullmatch('[0-9a-f]{64}', value) for value in
                    (self.manifest_sha, self.binary_sha, self.producer_sha, self.runner_sha)),
                'formal product pins unbound')


def require(condition, message):
    if not condition:
        raise RuntimeError(message)


class ClockUnavailableError(RuntimeError):
    """Required uptime cannot be read; never substitute another absolute clock."""


def system_uptime_ns():
    """Match Swift Dispatch uptime; Darwin Python3.9 monotonic is process-local."""
    try:
        if sys.platform == 'darwin':
            clock_id = getattr(time, 'CLOCK_UPTIME_RAW', None)
            if type(clock_id) is not int:
                raise ClockUnavailableError('Darwin CLOCK_UPTIME_RAW unavailable')
            reader = getattr(time, 'clock_gettime_ns', None)
            if not callable(reader):
                raise ClockUnavailableError('Darwin clock_gettime_ns unavailable')
            value = reader(clock_id)
        else:
            value = time.monotonic_ns()  # Synthetic/background tests only.
    except ClockUnavailableError:
        raise
    except (OSError, ValueError, TypeError, OverflowError) as error:
        raise ClockUnavailableError('system uptime unavailable: ' + str(error)) from error
    if type(value) is not int or value <= 0:
        raise ClockUnavailableError('invalid system uptime clock')
    return value


def utc():
    return datetime.datetime.now(datetime.timezone.utc).isoformat()


def sha(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()


def write(path, value):
    """Atomic external control/receipt writes; never replace product files."""
    temporary = path.with_name(path.name + '.new')
    with temporary.open('x', encoding='utf-8') as handle:
        json.dump(value, handle, indent=2, allow_nan=False)
        handle.write('\n')
        handle.flush()
    os.replace(temporary, path)


def strict_json(data):
    def reject_constant(value):
        raise ValueError('nonfinite JSON constant: ' + value)
    def pairs(values):
        result = {}
        for key, value in values:
            if key in result:
                raise ValueError('duplicate JSON key: ' + key)
            result[key] = value
        return result
    return json.loads(data, parse_constant=reject_constant, object_pairs_hook=pairs)


class FileTail:
    """Read available bytes only. A regular file never blocks like a PIPE."""
    def __init__(self, path):
        self.handle = path.open('rb')
        self.partial = b''

    def read(self):
        data = self.handle.read(MAX_BATCH_BYTES)
        self.partial += data
        chunks = self.partial.split(b'\n')
        self.partial = chunks.pop()
        require(len(self.partial) <= MAX_LINE_BYTES, 'unterminated monitor line too large')
        require(all(0 < len(row) <= MAX_LINE_BYTES for row in chunks), 'invalid monitor line size')
        return [strict_json(row) for row in chunks]

    def close(self):
        self.handle.close()


class ObservationStream:
    """Inject a clock and an owned monitor handle for deterministic tests."""
    def __init__(self, tail, monitor, nonce, policy=None, clock=None, continuous_guard=None):
        self.tail, self.monitor, self.nonce = tail, monitor, nonce
        self.policy = policy or GuardPolicy()
        self.clock = clock or system_uptime_ns
        self.continuous_guard = continuous_guard
        self.created_ns = self.clock()
        self.ready = False
        self.last_sample_ns = None
        self.records = []
        self.actions = []
        self.runner_bound = self.owned_bound = False
        self.expected_runner_pid = self.expected_owned_pid = 0
        self.runner_credential = None
        self.last_valid_sample = None
        self.integrity = True

    def pump(self, allow_owned_stop=False):
        self.actions = []
        try:
            if self.continuous_guard is not None:
                external = self.continuous_guard()
                if external:
                    self.policy.stop('CONTINUOUS_INPUT_GUARD_FAILURE', {'reason': external.reason})
            for row in self.tail.read():
                require(type(row) is dict and type(row.get('protocol_version')) is int and row.get('protocol_version') == 3 and
                        row.get('nonce') == self.nonce and row.get('clock_domain') == CLOCK_DOMAIN, 'monitor envelope')
                event = row.get('event')
                if event == 'ready':
                    require(not self.ready and row.get('monitor_pid') == self.monitor.pid and
                            row.get('thread_main') is True, 'monitor ready identity')
                    self.ready = True
                elif event == 'sample':
                    require(self.ready, 'sample before monitor ready')
                    require(row.get('runner_pid') == 0 or (self.runner_bound and
                            row.get('runner_pid') == self.expected_runner_pid), 'unverified runner observation')
                    require(row.get('owned_pid') == 0 or (self.owned_bound and
                            row.get('owned_pid') == self.expected_owned_pid), 'unverified owned observation')
                    validate_sample(row)
                    if self.last_valid_sample is not None:
                        require(row['seq'] == self.last_valid_sample['seq'] + 1 and
                                self.last_valid_sample['end_ns'] <= row['begin_ns'] and
                                row['begin_ns'] - self.last_valid_sample['end_ns'] <= MAX_SAMPLE_GAP_NS,
                                'raw observation gap/order')
                    else:
                        require(row['seq'] == 1, 'raw stream must begin at sample 1')
                    now_ns = self.clock()
                    require(row['end_ns'] <= now_ns and
                            now_ns - row['end_ns'] <= MAX_SAMPLE_GAP_NS,
                            'stale/future monitor sample')
                    self.last_valid_sample = row
                    self.last_sample_ns = row['end_ns']
                    self.actions.append(self.policy.observe(row))
                elif event in ('runner_bound', 'owned_bound'):
                    attribute = event
                    require(not getattr(self, attribute), 'repeated process binding')
                    credential = row.get('credential')
                    require(type(credential) is dict and all(type(credential.get(key)) is int and
                            credential[key] >= 0 for key in
                            ('pid', 'parent_pid', 'start_seconds', 'start_micros')),
                            'launch credential')
                    require(credential['pid'] > 0 and credential['start_micros'] < 1_000_000,
                            'launch credential range')
                    if event == 'runner_bound':
                        require(credential['pid'] == self.expected_runner_pid, 'runner binding target')
                        self.runner_credential = credential
                    else:
                        require(self.runner_bound and credential['pid'] == self.expected_owned_pid and
                                credential['parent_pid'] == self.expected_runner_pid,
                                'owned launch target/parent')
                        start = (credential['start_seconds'], credential['start_micros'])
                        runner_start = (self.runner_credential['start_seconds'], self.runner_credential['start_micros'])
                        require(start >= runner_start, 'owned launch predates runner')
                    setattr(self, attribute, True)
                elif event == 'fatal':
                    self.integrity = False
                    self.actions.append(self.policy.stop('MONITOR_FATAL', {'reason': row.get('reason')}))
                else:
                    raise RuntimeError('unknown monitor record')
                if event != 'sample':
                    self.records.append(row)
            code = self.monitor.poll()
            if code is not None and not (allow_owned_stop and code == -int(signal.SIGTERM)):
                self.integrity = False
                return self.policy.stop('MONITOR_EXITED', {'exit_code': self.monitor.returncode})
            anchor = self.last_sample_ns if self.last_sample_ns is not None else self.created_ns
            limit = MAX_SAMPLE_GAP_NS if self.last_sample_ns is not None else MONITOR_ARM_LIMIT_NS
            if self.clock() - anchor > limit:
                self.integrity = False
                return self.policy.stop('MONITOR_STALLED')
            return self.policy.terminal
        except (ValueError, TypeError, KeyError, RuntimeError, OSError) as error:
            self.integrity = False
            return self.policy.stop('INVALID_MONITOR_STREAM', {'error': str(error)})


def discover_native(previous, binary, expected_run_id=None, full=None):
    manifest_sha = full.manifest_sha if full else MANIFEST_SHA
    binary_sha = full.binary_sha if full else BINARY_SHA
    mode = full.mode if full else 'workflow-reopen'
    matches = []
    for path in set(Path('/tmp').glob('rcam-pmix-*')) - previous:
        if path.is_symlink() or not path.is_dir():
            continue
        try:
            request = strict_json((path / 'request.json').read_bytes())
            owned = strict_json((path / 'owned-process.json').read_bytes())
        except (FileNotFoundError, json.JSONDecodeError, ValueError, OSError):
            continue  # Product marker writes can be in progress; do not bind them.
        if not (type(request) is dict and request.get('source_manifest_sha256') == manifest_sha and
                request.get('mode') == mode and type(owned) is dict and
                owned.get('binary_sha256') == binary_sha):
            continue
        if full:
            require(type(request.get('schema_version')) is int and request['schema_version'] == 3 and
                    type(request.get('round')) is int and request['round'] == full.round and
                    type(request.get('display_id')) is int and request['display_id'] == full.display_id and
                    request.get('display_policy') == 'frozen-60hz' and
                    request.get('display_mode_change_authorized') is True and
                    request.get('evidence_scope') == 'full-pmix-native', 'formal request policy mismatch')
        else:
            require(request.get('schema_version') == 2 and request.get('round') == 1 and
                    request.get('display_policy') == 'preserve' and
                    request.get('display_mode_change_authorized') is False,
                    'request policy mismatch')
        run_id = request.get('run_id')
        require(type(run_id) is str and str(uuid.UUID(run_id)) == run_id, 'request run_id')
        require(type(owned.get('pid')) is int and owned['pid'] > 0, 'owned PID')
        require(owned.get('command') == [str(binary.resolve(strict=True))], 'owned binary command')
        if expected_run_id is not None:
            require(run_id == expected_run_id, 'bound run_id changed')
        matches.append((path.resolve(strict=True), owned, request))
    require(len(matches) <= 1, 'ambiguous new owned native directories')
    return matches[0] if matches else None


def interrupt_and_join(runner, reason, base, deadline_ns=None, clock=None,
                       signal_already_sent=False, clock_errors=None, cleanup_state=None):
    """Anchor immediately after first SIGINT, before all durable journal I/O.

    First clock failure stays unknown. Its original relative reserve may be used
    only immediately, before journal I/O. A later clock failure/unknown prior
    interrupt permits only a zero-time join check, never a fresh reserve.
    """
    clock = clock or system_uptime_ns
    if runner.poll() is not None:
        runner.wait(timeout=0)
        return True
    reserve = CLEANUP_SECONDS - MONITOR_STOP_SECONDS - POST_JOIN_BARRIER_SECONDS
    if deadline_ns is None and cleanup_state is not None:
        deadline_ns = cleanup_state['deadline_ns']
    unknown = (signal_already_sent and deadline_ns is None) or bool(
        cleanup_state is not None and cleanup_state['deadline_unknown'])
    fresh_unknown_clock = False
    if not signal_already_sent:
        runner.send_signal(signal.SIGINT)
        if cleanup_state is not None:
            cleanup_state['started'] = True
        # No write, print, filesystem read or wait may precede this first anchor.
        if deadline_ns is None and not unknown:
            try:
                deadline_ns = clock() + CLEANUP_SECONDS * 1_000_000_000
                if cleanup_state is not None:
                    cleanup_state['deadline_ns'] = deadline_ns
            except ClockUnavailableError as error:
                unknown = fresh_unknown_clock = True
                if cleanup_state is not None:
                    cleanup_state['deadline_unknown'] = True
                if clock_errors is not None:
                    clock_errors.append(str(error))
        journal = {'runner_pid': runner.pid, 'signal': int(signal.SIGINT), 'reason': reason,
                   'scope': 'only this launched runner; its finally owns app/producer cleanup'}
        if fresh_unknown_clock:
            # Keep the pre-existing bounded relative fault path, but do it now:
            # logging must not move the reserve's start later than the signal.
            try:
                runner.wait(timeout=reserve)
                joined = True
            except subprocess.TimeoutExpired:
                joined = False
            write(base / 'controlled-interrupt.json', journal)
            return joined
        write(base / 'controlled-interrupt.json', journal)
    # Journal I/O may consume the original deadline. Re-read only the remaining
    # budget; this clock never establishes or refreshes the first origin.
    try:
        now_ns = clock()
        timeout = (0 if unknown else max(0, (deadline_ns - now_ns) / 1e9 -
                                        MONITOR_STOP_SECONDS - POST_JOIN_BARRIER_SECONDS))
    except ClockUnavailableError as error:
        if clock_errors is not None:
            clock_errors.append(str(error))
        timeout = 0
    try:
        runner.wait(timeout=timeout)
        return True
    except subprocess.TimeoutExpired:
        return False  # No killall, group signal, or forced app/runner kill.


def post_join_barrier(stream, joined_at_ns, clock=None, sleep=time.sleep):
    """Cover the entire owned runner interval with a genuinely post-join sample."""
    clock = clock or system_uptime_ns
    deadline_ns = joined_at_ns + MAX_SAMPLE_GAP_NS
    while True:
        stream.pump()
        if not stream.integrity:
            return False
        last = stream.last_valid_sample
        now_ns = clock()
        if now_ns <= deadline_ns and last is not None and last['begin_ns'] >= joined_at_ns:
            return True
        if now_ns >= deadline_ns:
            stream.integrity = False
            stream.policy.stop('POST_JOIN_OBSERVATION_DEADLINE')
            return False
        sleep(.01)


def stop_monitor(monitor):
    if monitor.poll() is None:
        monitor.terminate()
        try:
            monitor.wait(timeout=2)
        except subprocess.TimeoutExpired:
            monitor.kill()
            try:
                monitor.wait(timeout=2)
            except subprocess.TimeoutExpired:
                return False
    return monitor.returncode is not None


def run(base=None, *, full=None, continuous_guard=None, compiled_monitor=None):
    require(sys.platform == 'darwin', 'this external supervisor runs only on macOS')
    if full:
        full.validate()
    root = full.root if full else ROOT
    require(root is not None, 'set RCAM_GUARD_ROOT to the locked product checkout')
    base = (base or Path(__file__).resolve().parent).resolve(strict=True)
    gates = base.parent / 'background-gates-09'
    binary = (full.binary if full else gates / 'bin/editor-app-release-internal').resolve(strict=True)
    producer = (full.producer if full else gates / 'capture-writer-preflight/capture-producer').resolve(strict=True)
    manifest_sha = full.manifest_sha if full else MANIFEST_SHA
    binary_sha = full.binary_sha if full else BINARY_SHA
    producer_sha = full.producer_sha if full else PRODUCER_SHA
    runner_sha = full.runner_sha if full else RUNNER_SHA
    output = full.output if full else base / 'workflow-reopen1'
    require(not output.exists(), 'fresh native output required')
    require(sha(binary) == binary_sha, 'locked binary hash')
    require(sha(producer) == producer_sha, 'locked producer hash')
    require(sha(root / 'MANIFEST.sha256') == manifest_sha, 'locked product manifest')
    require(sha(root / 'scripts/run_pmix_native.py') == runner_sha, 'locked runner source')
    command = ['python3', '-B', 'scripts/run_pmix_native.py', '--binary', str(binary),
               '--capture-producer', str(producer), '--output', str(output),
               '--mode', full.mode if full else 'workflow-reopen', '--round', str(full.round if full else 1),
               '--video']
    if full:
        command += ['--display-id', str(full.display_id), '--display-policy', 'frozen-60hz',
                    '--allow-display-mode-change']
        if full.fixture is not None:
            command += ['--fixture', str(full.fixture.resolve(strict=True))]
    else:
        command += ['--fixture', str(root / 'fixtures/synthetic/s5m2c/MIX_WORKFLOW.rcam'),
                    '--display-policy', 'preserve']
    for name in ('monitor-control.json', 'interference-monitor', 'monitor-compile.stdout',
                 'monitor-compile.stderr', 'monitor-compile.json', 'runner.stdout', 'runner.stderr',
                 'monitor.stdout', 'monitor.stderr', 'SUPERVISOR_RESULT.json', 'workflow-reopen1'):
        require(not (base / name).exists(), 'fresh external evidence directory required: ' + name)
    system_uptime_ns()  # Fail before compiler/monitor/runner if the required clock is unavailable.
    if compiled_monitor is None:
        compile_command = ['/usr/bin/swiftc', '-parse-as-library', '-swift-version', '6',
                           '-strict-concurrency=complete', '-warnings-as-errors',
                           str(base / 'interference.swift'), '-o', str(base / 'interference-monitor')]
        with (base / 'monitor-compile.stdout').open('xb') as out, (base / 'monitor-compile.stderr').open('xb') as err:
            compiled = subprocess.run(compile_command, stdout=out, stderr=err, timeout=30)
        write(base / 'monitor-compile.json', {'command': compile_command, 'exit_code': compiled.returncode})
        require(compiled.returncode == 0, 'monitor compilation failed')
        monitor_binary = base / 'interference-monitor'
    else:
        monitor_binary, monitor_sha = compiled_monitor
        require(monitor_binary.is_file() and not monitor_binary.is_symlink() and sha(monitor_binary) == monitor_sha,
                'compiled monitor identity')
    nonce = str(uuid.uuid4())
    control = {'protocolVersion': 3, 'nonce': nonce, 'commandID': 0,
               'runnerPID': 0, 'appPID': 0, 'binaryPath': None, 'native': None,
               'runID': None}
    control_path = base / 'monitor-control.json'
    require(not control_path.exists(), 'fresh external evidence directory required')
    write(control_path, control)
    runner = monitor = tail = None
    native = request = owned = None
    interrupted = None
    execution_started_ns = None
    joined = monitor_joined = barrier_satisfied = False
    joined_at_ns = None
    stream = None
    launched_at_ns = None
    cleanup_deadline_ns = None
    cleanup_state = {'started': False, 'deadline_ns': None, 'deadline_unknown': False}
    interrupt_sent = False
    clock_errors = []
    try:
        launched_at_ns = system_uptime_ns()
        with (base / 'runner.stdout').open('xb') as out, (base / 'runner.stderr').open('xb') as err, \
             (base / 'monitor.stdout').open('xb') as mout, (base / 'monitor.stderr').open('xb') as merr:
            monitor = subprocess.Popen([str(monitor_binary), str(control_path), nonce],
                                       stdout=mout, stderr=merr)
            tail = FileTail(base / 'monitor.stdout')
            stream = ObservationStream(tail, monitor, nonce, continuous_guard=continuous_guard)
            # Establish ready, liveness, and two consecutive raw samples before any runner.
            while stream.policy.phase != 'ARMED':
                halt = stream.pump()
                if halt:
                    interrupted = halt.reason
                    break
                time.sleep(.01)
            if interrupted is None:
                previous = set(Path('/tmp').glob('rcam-pmix-*'))
                runner = subprocess.Popen(command, cwd=root, stdout=out, stderr=err)
                execution_started_ns = system_uptime_ns()
                stream.expected_runner_pid = runner.pid
                control.update(commandID=control['commandID'] + 1, runnerPID=runner.pid)
                write(control_path, control)
                write(base / 'runner-launch.json', {
                    'pid': runner.pid, 'command': command, 'utc': utc(),
                    'started_uptime_ns': execution_started_ns,
                    'nonce': nonce, 'clock_domain': CLOCK_DOMAIN, 'armed_baseline_seq': stream.policy.previous['seq'],
                    'soft_deadline_seconds': EXECUTION_SECONDS,
                    'cleanup_reserve_seconds': CLEANUP_SECONDS,
                    'scope': ('formal single matrix case; frozen60; no retry' if full else
                              'single own-window capture microcheck; preserve; no retry')})
                while True:
                    halt = stream.pump()
                    if halt:
                        interrupted = halt.reason
                        break
                    code = runner.poll()
                    if code is not None:
                        joined = True
                        joined_at_ns = system_uptime_ns()
                        if code != 0:
                            interrupted = 'RUNNER_NONZERO_EXIT'
                        if joined_at_ns - execution_started_ns > EXECUTION_SECONDS * 1_000_000_000:
                            interrupted = interrupted or 'MICROCHECK_EXECUTION_DEADLINE'
                        break
                    if native is None:
                        match = discover_native(previous, binary, full=full)
                        if match:
                            native, owned, request = match
                            initial_display = None
                            if full:
                                initial = strict_json((native / 'display-before.json').read_bytes())
                                initial_display = initial['after']
                                require(initial['before'] == initial_display and
                                        type(initial_display.get('display_id')) is int and
                                        initial_display['display_id'] == full.display_id and
                                        type(initial_display.get('mode_id')) is int and
                                        initial_display['mode_id'] == 113 and
                                        initial_display.get('refresh_hz') == 144 and
                                        initial_display.get('in_mirror_set') is False,
                                        'formal initial display snapshot')
                            stream.expected_owned_pid = owned['pid']
                            control.update(commandID=control['commandID'] + 1, appPID=owned['pid'],
                                           binaryPath=str(binary), native=str(native), runID=request['run_id'])
                            write(control_path, control)
                            write(base / 'native-binding.json', {
                                'native': str(native), 'pid': owned['pid'], 'run_id': request['run_id'],
                                'binary_path': str(binary), 'binary_sha256': binary_sha,
                                'initial_display_snapshot': initial_display,
                                'scope': 'candidate only; monitor must verify live parent/start/executable'})
                    if system_uptime_ns() - execution_started_ns > EXECUTION_SECONDS * 1_000_000_000:
                        interrupted = 'MICROCHECK_EXECUTION_DEADLINE'
                        break
                    time.sleep(.01)
                if interrupted is not None and runner.poll() is None:
                    # Keep monitoring during SIGINT/wait; do not block on wait(40).
                    runner.send_signal(signal.SIGINT)
                    interrupt_sent = True
                    cleanup_state['started'] = True
                    try:
                        cleanup_deadline_ns = system_uptime_ns() + CLEANUP_SECONDS * 1_000_000_000
                        cleanup_state['deadline_ns'] = cleanup_deadline_ns
                    except ClockUnavailableError:
                        cleanup_state['deadline_unknown'] = True
                        raise
                    write(base / 'controlled-interrupt.json', {
                        'runner_pid': runner.pid, 'signal': int(signal.SIGINT), 'reason': interrupted,
                        'scope': 'only this launched runner; its finally owns app/producer cleanup'})
                    while runner.poll() is None and system_uptime_ns() < cleanup_deadline_ns - int((MONITOR_STOP_SECONDS + POST_JOIN_BARRIER_SECONDS) * 1e9):
                        stream.pump()  # Terminal policy retains the original cause; raw records continue.
                        time.sleep(.01)
                    joined = runner.poll() is not None
                    if joined:
                        joined_at_ns = system_uptime_ns()
                elif runner is not None:
                    joined = runner.poll() is not None
                    if joined and joined_at_ns is None:
                        joined_at_ns = system_uptime_ns()
    except BaseException as error:
        interrupted = interrupted or 'SUPERVISOR_EXCEPTION: ' + type(error).__name__ + ': ' + str(error)
        if isinstance(error, ClockUnavailableError):
            clock_errors.append(str(error))
            if stream is not None:
                stream.integrity = False
                stream.policy.stop('CLOCK_UNAVAILABLE', {'error': str(error)})
        if runner is not None:
            if runner.poll() is None:
                previous_signal = interrupt_sent
                interrupt_sent = True
                joined = interrupt_and_join(runner, interrupted, base, cleanup_deadline_ns,
                                            signal_already_sent=previous_signal,
                                            clock_errors=clock_errors, cleanup_state=cleanup_state)
                cleanup_deadline_ns = cleanup_state['deadline_ns'] or cleanup_deadline_ns
            else:
                joined = True
            if joined:
                try:
                    joined_at_ns = system_uptime_ns()
                except ClockUnavailableError as clock_error:
                    clock_errors.append(str(clock_error))
                    joined_at_ns = None
        if isinstance(error, (SystemExit, KeyboardInterrupt)):
            # Still write the result after owned cleanup; never let an interrupt skip it.
            pass
    finally:
        cleanup_deadline_ns = cleanup_state['deadline_ns'] or cleanup_deadline_ns
        execution_exceeded = bool(joined_at_ns is not None and execution_started_ns is not None and
                                  not interrupt_sent and joined_at_ns > execution_started_ns + EXECUTION_SECONDS * 1_000_000_000)
        cleanup_exceeded = bool(joined_at_ns is not None and cleanup_deadline_ns is not None and
                                joined_at_ns > cleanup_deadline_ns)
        if execution_exceeded or cleanup_exceeded:
            interrupted = interrupted or ('MICROCHECK_EXECUTION_DEADLINE' if execution_exceeded else 'CLEANUP_DEADLINE')
        if runner is not None and joined and stream is not None:
            if joined_at_ns is None:
                interrupted = interrupted or 'JOIN_CLOCK_UNAVAILABLE'
            else:
                try:
                    barrier_satisfied = post_join_barrier(stream, joined_at_ns,
                                                         clock=system_uptime_ns, sleep=time.sleep)
                except ClockUnavailableError as clock_error:
                    clock_errors.append(str(clock_error))
                    stream.integrity = False
                    stream.policy.stop('CLOCK_UNAVAILABLE', {'error': str(clock_error)})
            if stream.policy.terminal:
                interrupted = interrupted or stream.policy.terminal.reason
            if not barrier_satisfied:
                interrupted = interrupted or 'POST_JOIN_OBSERVATION_DEADLINE'
            if interrupted is None and stream.policy.capture_completed_ns is None:
                interrupted = 'RUNNER_EXIT_WITHOUT_OBSERVED_CAPTURE_COMPLETE'
        if monitor is not None:
            monitor_joined = stop_monitor(monitor)
            if monitor.returncode != -int(signal.SIGTERM):
                interrupted = interrupted or 'MONITOR_UNEXPECTED_FINAL_EXIT'
        if tail is not None:
            tail.close()
        duration_seconds = None
        if launched_at_ns is not None:
            try:
                duration_seconds = (system_uptime_ns() - launched_at_ns) / 1e9
            except ClockUnavailableError as clock_error:
                clock_errors.append(str(clock_error))
        if clock_errors:
            interrupted = interrupted or 'CLOCK_UNAVAILABLE'
        receipt = {
            'runner_pid': runner.pid if runner else None,
            'actual_exit_code': runner.returncode if runner else None,
            'joined': joined, 'interrupted': interrupted,
            'cleanup_timeout': bool(runner and not joined and runner.returncode is None),
            'execution_budget_seconds': EXECUTION_SECONDS, 'cleanup_budget_seconds': CLEANUP_SECONDS,
            'duration_seconds': duration_seconds,
            'clock_unavailable': bool(clock_errors),
            'clock_error': clock_errors[0] if clock_errors else None,
            'clock_domain': CLOCK_DOMAIN,
            'native_directory': str(native) if native else None,
            'run_id': request['run_id'] if request else None,
            'monitor_pid': monitor.pid if monitor else None,
            'monitor_exit_code': monitor.returncode if monitor else None,
            'monitor_joined': monitor_joined,
            'runner_joined_at_ns': joined_at_ns,
            'cleanup_deadline_ns': cleanup_deadline_ns,
            'cleanup_started': cleanup_state['started'] or interrupt_sent,
            'cleanup_deadline_unknown': cleanup_state['deadline_unknown'],
            'execution_deadline_exceeded': execution_exceeded,
            'cleanup_deadline_exceeded': cleanup_exceeded,
            'post_join_barrier_satisfied': barrier_satisfied,
            'post_join_sample_begin_ns': (stream.last_valid_sample['begin_ns']
                                          if barrier_satisfied else None),
            'external_activation': False,
            'phase': stream.policy.phase if stream else None,
            'first_foreground_ns': stream.policy.first_foreground_ns if stream else None,
            'capture_completed_ns': stream.policy.capture_completed_ns if stream else None,
            'human_input_attributed': False,
            'display_policy': 'frozen-60hz' if full else 'preserve', 'retry': False,
            'mode': full.mode if full else 'workflow-reopen', 'round': full.round if full else 1,
            'display_id': full.display_id if full else None,
            'source_manifest_sha256': manifest_sha, 'utc': utc(),
            'success': bool(runner and joined and runner.returncode == 0 and interrupted is None and
                            monitor_joined and barrier_satisfied and stream and stream.policy.capture_completed_ns is not None)}
        write(base / 'SUPERVISOR_RESULT.json', receipt)
        if stream and stream.policy.terminal:
            write(base / 'interference-halt.json', {
                'reason': stream.policy.terminal.reason, 'details': stream.policy.terminal.details,
                'scope': 'raw HID/session state and owned foreground only; no human attribution'})
        print(json.dumps(receipt, allow_nan=False), flush=True)
    return 0 if receipt['success'] else 2


if __name__ == '__main__':
    raise SystemExit('Use the pinned matrix.py coordinator; legacy microcheck CLI is disabled.')
