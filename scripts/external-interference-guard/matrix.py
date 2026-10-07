"""External frozen PMIX twelve-case coordinator; no product source changes.

Product pins bind the reviewed clean source and fresh Mac runtime identity.
Imports are harmless. GUI execution is Darwin-only. Tests use synthetic data.
"""
import argparse
import ast
from dataclasses import dataclass
import hashlib
import json
from pathlib import Path, PurePosixPath
import re
import signal
import subprocess
import sys
import time
import uuid
import zipfile

import supervise as guard

CASES = (('nav', 1), ('nav', 2), ('nav', 3), ('move', 1), ('move', 2),
         ('move', 3), ('points', 1), ('escape', 1), ('new-project', 1),
         ('workflow', 1), ('workflow-reopen', 1), ('workflow-cross-layer', 1))
DISPLAY_ID = 2
ORIGINAL_MODE = 113
ORIGINAL_HZ = 144
GEOMETRY = {'width': 1920, 'height': 1080, 'pixel_width': 3840,
            'pixel_height': 2160, 'backing_scale': 2}
VERIFICATION_SECONDS = 190
ATTACK_SECONDS = 48 * VERIFICATION_SECONDS


@dataclass(frozen=True)
class ProductPins:
    commit: str
    manifest_sha: str
    binary_sha: str
    producer_sha: str
    runner_sha: str

    def validate(self):
        guard.require(type(self.commit) is str and re.fullmatch('[0-9a-f]{40}', self.commit),
                      'clean product commit pin')
        guard.require(all(type(value) is str and re.fullmatch('[0-9a-f]{64}', value) for value in
                          (self.manifest_sha, self.binary_sha, self.producer_sha, self.runner_sha)),
                      'product SHA pins')


# Reviewed clean product source and independently measured fresh Mac builds.
PRODUCT_PINS = ProductPins(
    commit='0cf2c702bb2d815c51fa5080f8b625600ac5d252',
    manifest_sha='37df7531654cb9a4d70231add83072a51c92fbfdc7dbc112bc492f1e8043caf0',
    binary_sha='7e6fdb651c1e8deddc5163567d737fb5e2f440aae0275faf9dcb6800e4b0be8f',
    producer_sha='1a9a79d94a838ae868f14d62a62ce992fc737b463f5ae8f88b70499bcca8f9b5',
    runner_sha='20dba8c15a2ed547c98c00ba7d994ffac0dbc610c3f2f0178c046dd6b390a74c',
)


def require_pins():
    guard.require(type(PRODUCT_PINS) is ProductPins, 'formal product pins unbound; no process may start')
    PRODUCT_PINS.validate()
    return PRODUCT_PINS


def case_name(case):
    guard.require(case in CASES, 'frozen matrix case')
    return case[0] + str(case[1])


def command_native_verify(root, directory, pins):
    return ['python3', '-B', 'scripts/verify_pmix_native.py', str(directory),
            '--source-manifest', pins.manifest_sha, '--commit', pins.commit,
            '--binary-sha256', pins.binary_sha, '--capture-producer-sha256', pins.producer_sha]


def command_aggregate(root, bundle, pins, ledger):
    return ['python3', '-B', 'scripts/verify_pmix_evidence.py', str(bundle),
            '--source-manifest', pins.manifest_sha, '--commit', pins.commit,
            '--binary-sha256', pins.binary_sha, '--gate-ledger-sha256', ledger]


def command_background(root, bundle, pins, ledger):
    return command_aggregate(root, bundle, pins, ledger) + ['--background-only']


def command_attacks(root, bundle, pins, ledger, out):
    return ['python3', '-B', 'scripts/run_pmix_bundle_attacks.py', '--source-root', str(root),
            '--source-manifest', pins.manifest_sha, '--commit', pins.commit,
            '--binary-sha256', pins.binary_sha, '--gate-ledger-sha256', ledger,
            '--baseline', str(bundle), '--out', str(out)]


def verify_source(root, pins):
    """Check every manifest entry before importing/executing any product script."""
    guard.require(guard.sha(root / 'MANIFEST.sha256') == pins.manifest_sha, 'locked source manifest')
    entries = {}
    for row in (root / 'MANIFEST.sha256').read_text().splitlines():
        digest, separator, name = row.partition('  ')
        parts = PurePosixPath(name)
        guard.require(separator and re.fullmatch('[0-9a-f]{64}', digest) and name and
                      '\\' not in name and ':' not in name and not parts.is_absolute() and
                      all(part not in ('', '.', '..') for part in name.split('/')) and name not in entries,
                      'source manifest path/digest')
        target = root / name
        guard.require(not any((root / Path(*parts.parts[:index])).is_symlink()
                              for index in range(1, len(parts.parts) + 1)) and target.is_file(),
                      'source manifest regular file')
        guard.require(guard.sha(target) == digest, 'locked source bytes: ' + name)
        entries[name] = digest
    guard.require(entries and entries.get('scripts/run_pmix_native.py') == pins.runner_sha,
                  'locked runner manifest entry')
    return entries


def validate_seed(bundle, pins, ledger):
    """A fresh canonical bundle seed is prepared by the Mac owner before GUI work."""
    guard.require(type(ledger) is str and re.fullmatch('[0-9a-f]{64}', ledger), 'external gate ledger pin')
    guard.require(bundle.is_dir() and not bundle.is_symlink() and
                  {path.name for path in bundle.iterdir()} ==
                  {'Source.zip', 'gates', 'REVIEW.json', 'Evidence_MANIFEST.sha256'},
                  'fresh sealed canonical Source/gates/REVIEW seed required')
    guard.require((bundle / 'gates').is_dir() and
                  all((bundle / name).is_file() for name in
                      ('Source.zip', 'REVIEW.json', 'Evidence_MANIFEST.sha256')), 'seed entry types')
    for path in bundle.rglob('*'):
        guard.require(not path.is_symlink() and (path.is_dir() or path.is_file()), 'seed regular entry')
    inventory = background_inventory((bundle / 'Evidence_MANIFEST.sha256').read_bytes())
    guard.require({path.relative_to(bundle).as_posix() for path in bundle.rglob('*') if path.is_file()} ==
                  set(inventory) | {'Evidence_MANIFEST.sha256'}, 'complete sealed background inventory')
    for name, digest in inventory.items():
        guard.require(guard.sha(bundle / name) == digest, 'sealed background bytes: ' + name)
    guard.require(guard.sha(bundle / 'gates/gates.json') == ledger, 'external gate ledger identity')
    review = guard.strict_json((bundle / 'REVIEW.json').read_bytes())
    guard.require(type(review) is dict and type(review.get('schema_version')) is int and
                  review['schema_version'] == 2 and review.get('stage') == 'S5-M2-C' and
                  review.get('build_commit') == pins.commit and
                  review.get('source_manifest_sha256') == pins.manifest_sha and review.get('native') == [],
                  'fresh review identity/native list')
    guard.require(review.get('review_state') in ('CANDIDATE_PENDING_INDEPENDENT_REVIEW',
                                               'CLEAN_FINAL_PENDING_INDEPENDENT_REVIEW') and
                  review.get('user_flicker_report') == 'OPEN' and review.get('Windows') == 'DEFERRED' and
                  review.get('K1_native_remaining') == 'DEFERRED' and review.get('whole_I2') == 'NOT_ALL_PASS',
                  'seed cannot grant stage/platform acceptance')
    with zipfile.ZipFile(bundle / 'Source.zip') as archive:
        guard.require(archive.namelist().count('MANIFEST.sha256') == 1 and
                      archive.getinfo('MANIFEST.sha256').file_size <= 1024 * 1024 and
                      guard.hashlib.sha256(archive.read('MANIFEST.sha256')).hexdigest() == pins.manifest_sha,
                      'seed Source manifest identity')
    return review


def background_inventory(data):
    """Original background seal, retained outside the later resealed bundle."""
    entries = {}
    for row in data.decode('utf-8').splitlines():
        digest, separator, name = row.partition('  ')
        parts = PurePosixPath(name)
        guard.require(separator and re.fullmatch('[0-9a-f]{64}', digest) and name and
                      '\\' not in name and ':' not in name and not parts.is_absolute() and
                      all(part not in ('', '.', '..') for part in name.split('/')) and
                      name not in entries and name != 'Evidence_MANIFEST.sha256' and
                      (name in ('Source.zip', 'REVIEW.json') or name.startswith('gates/')),
                      'original background seal path/digest')
        entries[name] = digest
    guard.require({'Source.zip', 'REVIEW.json', 'gates/gates.json'} <= set(entries),
                  'original background seal required members')
    return entries


def validate_background_result(result, pins, ledger):
    """Admit only the product's two fully verified clean preparation branches."""
    guard.require(type(result) is dict and 'success' not in result and
                  {'background_status', 'background_initialization', 'background_blocked_reason'} <= set(result) and
                  result.get('result') == 'BACKGROUND_QUALIFIED_FOREGROUND_PENDING' and
                  result.get('stage_PASS_claim') is False and
                  result.get('cargo_gates') == result.get('writer_and_refusal_tests') == 'PASS' and
                  result.get('foreground_initialization') == 'PENDING_REAL_OWNED_NATIVE' and
                  type(result.get('gates')) is int and result['gates'] == 31,
                  'strict background preparation qualification')
    guard.require((result.get('background_status'), result.get('background_initialization'),
                   result.get('background_blocked_reason')) in
                  (('BLOCKED_CAPTURE_INITIALIZATION', 'BLOCKED', 'no-eligible-window'),
                   ('GATES_PASS', 'INITIALIZATION_ONLY_PASS', None)),
                  'verified clean background initialization branch')
    guard.require(all(result.get(key) == expected for key, expected in
                      (('source_manifest_sha256', pins.manifest_sha), ('build_commit', pins.commit),
                       ('binary_sha256', pins.binary_sha), ('capture_producer_sha256', pins.producer_sha),
                       ('gate_ledger_sha256', ledger))), 'background qualification exact product identities')
    return result


def verify_background_preserved(bundle, evidence, native_rows=None):
    seal = (evidence / 'background-seed-manifest.sha256').read_bytes()
    review_bytes = (evidence / 'background-seed-review.json').read_bytes()
    inventory = background_inventory(seal)
    guard.require(guard.hashlib.sha256(review_bytes).hexdigest() == inventory['REVIEW.json'],
                  'preserved original background review digest')
    original = guard.strict_json(review_bytes)
    guard.require(type(original) is dict and original.get('native') == [], 'preserved fresh background review')
    for path in bundle.rglob('*'):
        guard.require(not path.is_symlink() and (path.is_file() or path.is_dir()), 'preserved bundle regular entries')
    actual_background = {path.relative_to(bundle).as_posix() for path in bundle.rglob('*')
                         if path.is_file() and path.relative_to(bundle).parts[0] != 'native' and
                         path.relative_to(bundle).as_posix() != 'Evidence_MANIFEST.sha256'}
    guard.require(actual_background == set(inventory), 'unchanged original background inventory')
    for name, digest in inventory.items():
        if name != 'REVIEW.json':
            guard.require(guard.sha(bundle / name) == digest, 'unchanged sealed background bytes: ' + name)
    if native_rows is None:
        guard.require((bundle / 'Evidence_MANIFEST.sha256').read_bytes() == seal and
                      (bundle / 'REVIEW.json').read_bytes() == review_bytes,
                      'original background seal/review unchanged before native execution')
    else:
        guard.require(guard.strict_json((bundle / 'REVIEW.json').read_bytes()) == dict(original, native=native_rows),
                      'final review may only add the frozen native matrix')
    return original


def qualify_background(root, bundle, evidence, pins, ledger):
    """Pure product verification must pass before compiler, observer or GUI launch."""
    for source, target in (('Evidence_MANIFEST.sha256', 'background-seed-manifest.sha256'),
                           ('REVIEW.json', 'background-seed-review.json')):
        with (evidence / target).open('xb') as handle:
            handle.write((bundle / source).read_bytes())
    command = command_background(root, bundle, pins, ledger)
    failure = None
    code = None
    result = None
    try:
        with (evidence / 'background-qualification.stdout').open('xb') as out, \
             (evidence / 'background-qualification.stderr').open('xb') as err:
            completed = subprocess.run(command, cwd=root, stdout=out, stderr=err, timeout=VERIFICATION_SECONDS)
        code = completed.returncode
        guard.require(type(code) is int and code == 0, 'product background qualification exit')
        raw = (evidence / 'background-qualification.stdout').read_bytes()
        guard.require(0 < len(raw) <= guard.MAX_LINE_BYTES, 'one bounded background qualification JSON')
        result = validate_background_result(guard.strict_json(raw), pins, ledger)
        validate_seed(bundle, pins, ledger)
        verify_background_preserved(bundle, evidence)
    except BaseException as error:
        failure = type(error).__name__ + ': ' + str(error)
    guard.write(evidence / 'BACKGROUND_QUALIFICATION.json',
                {'qualified': failure is None, 'failure': failure, 'command': command,
                 'actual_exit_code': code, 'timeout_seconds': VERIFICATION_SECONDS, 'result': result,
                 'background_seed_manifest_sha256': guard.sha(evidence / 'background-seed-manifest.sha256'),
                 'background_seed_review_sha256': guard.sha(evidence / 'background-seed-review.json'),
                 'stage_PASS_claim': False})
    guard.require(failure is None, 'background preparation rejected: ' + str(failure))
    return result


def verify_background_qualification(root, bundle, evidence, pins, ledger, rows):
    receipt = guard.strict_json((evidence / 'BACKGROUND_QUALIFICATION.json').read_bytes())
    raw = (evidence / 'background-qualification.stdout').read_bytes()
    guard.require(0 < len(raw) <= guard.MAX_LINE_BYTES and receipt.get('qualified') is True and
                  receipt.get('failure') is None and type(receipt.get('actual_exit_code')) is int and
                  receipt['actual_exit_code'] == 0 and receipt.get('command') == command_background(root, bundle, pins, ledger) and
                  type(receipt.get('timeout_seconds')) is int and receipt['timeout_seconds'] == VERIFICATION_SECONDS and
                  receipt.get('stage_PASS_claim') is False and
                  receipt.get('result') == guard.strict_json(raw), 'original background qualification receipt/raw')
    validate_background_result(receipt['result'], pins, ledger)
    guard.require(receipt.get('background_seed_manifest_sha256') == guard.sha(evidence / 'background-seed-manifest.sha256') and
                  receipt.get('background_seed_review_sha256') == guard.sha(evidence / 'background-seed-review.json'),
                  'original background qualification seal/review binding')
    verify_background_preserved(bundle, evidence, rows)
    return True


class OwnedReplayTail:
    """One original raw record at a time, using its own acquisition clock."""
    def __init__(self, path):
        self.handle = path.open('rb')
        try:
            self.next = self._line()
            guard.require(type(self.next) is dict and self.next.get('event') == 'ready', 'owned raw begins with ready')
            self.now_ns = self.next.get('at_ns')
            guard.require(type(self.now_ns) is int and self.now_ns > 0, 'owned ready uptime')
        except BaseException:
            self.handle.close()
            raise
        self.current = None
        self.done = False

    def _line(self):
        data = self.handle.readline(guard.MAX_LINE_BYTES + 1)
        if not data:
            return None
        guard.require(data.endswith(b'\n') and len(data) <= guard.MAX_LINE_BYTES, 'complete original owned raw line')
        return guard.strict_json(data)

    def read(self):
        row = self.next
        self.next = None
        if row is None:
            row = self._line()
        if row is None:
            self.done = True
            return []
        guard.require(type(row) is dict, 'owned raw record object')
        stamp = row.get('end_ns') if row.get('event') == 'sample' else row.get('at_ns')
        guard.require(type(stamp) is int and stamp >= self.now_ns, 'owned raw event clock order')
        if row.get('event') == 'sample':
            guard.require(type(row.get('begin_ns')) is int and row['begin_ns'] >= self.now_ns,
                          'owned binding/ready precedes sample acquisition')
        self.now_ns = stamp
        self.current = row
        return [row]

    def close(self):
        self.handle.close()


def verify_launch_phases(local, directory, case, receipt, launch, request, control):
    """Use the accepted actual bytes and original two-directory ownership graph."""
    runner_raw, runner, runner_sha = guard.raw_json(local / 'runner-binding.raw.json')
    source_raw, _, _ = guard.raw_json(directory / 'runner-binding.json')
    saved_request, _, _ = guard.raw_json(local / 'runner-request.raw.json')
    current_request, _, _ = guard.raw_json(directory / 'request.json')
    app_raw, app, app_sha = guard.raw_json(local / 'app-launch.raw.json')
    source_app, _, _ = guard.raw_json(directory / 'app-launch.json')
    observed = guard.strict_json((local / 'runner-phase-observed.json').read_bytes())
    app_observed = guard.strict_json((local / 'app-phase-observed.json').read_bytes())
    command = launch.get('command')
    values = launch.get('product_pins')
    guard.require(type(command) is list and command.count('--output') == 1 and command.count('--capture-producer') == 1 and
                  type(launch.get('root')) is str and Path(launch['root']).is_absolute() and type(values) is dict and
                  set(values) == {'source_manifest_sha256', 'binary_sha256', 'capture_producer_sha256', 'runner_sha256'} and
                  all(type(value) is str and re.fullmatch('[0-9a-f]{64}', value) for value in values.values()) and
                  values['source_manifest_sha256'] == receipt.get('source_manifest_sha256') and
                  runner_raw == source_raw and app_raw == source_app and saved_request == current_request and
                  type(launch.get('armed_end_ns')) is int and launch['armed_end_ns'] > 0 and
                  observed.get('armed_end_ns') == launch['armed_end_ns'] and
                  observed.get('binding_sha256') == runner_sha == receipt.get('runner_binding_sha256') and
                  observed.get('nonce') == control.get('nonce') and
                  app_observed.get('binding_sha256') == runner_sha and app_observed.get('launch_sha256') == app_sha,
                  'original phase bytes/source/owned launch graph')
    full = guard.FullRun(Path(launch['root']), Path(control['binaryPath']),
                        Path(command[command.index('--capture-producer') + 1]),
                        Path(command[command.index('--output') + 1]), case[0], case[1], DISPLAY_ID,
                        values['source_manifest_sha256'], values['binary_sha256'],
                        values['capture_producer_sha256'], values['runner_sha256'])
    guard.validate_runner_binding(runner, runner_sha, request, full, control['nonce'], receipt['runner_pid'],
                                  launch['armed_end_ns'])
    origin = guard.validate_app_launch(app, runner, runner_sha)
    guard.require(type(observed.get('observed_at_ns')) is int and observed['observed_at_ns'] >= runner['bound_at_ns'] and
                  type(app_observed.get('observed_at_ns')) is int and app_observed['observed_at_ns'] >= origin and
                  app_observed.get('launch_at_ns') == receipt.get('app_launch_opportunity_ns') == origin and
                  type(app_observed.get('sample_seq_before_binding')) is int and app_observed['sample_seq_before_binding'] >= 2 and
                  receipt.get('native_directory') == control.get('native') == runner['native_directory'] and
                  str(directory.resolve(strict=True)) == runner['output_directory'],
                  'actual app opportunity/observer/original native and output binding')
    return origin, observed, app_observed


def verify_owned_evidence(local, directory, case, receipt):
    """Replay full owned policy, including actual foreground, without summary substitutions."""
    control = guard.strict_json((local / 'monitor-control.json').read_bytes())
    launch = guard.strict_json((local / 'runner-launch.json').read_bytes())
    binding = guard.strict_json((local / 'native-binding.json').read_bytes())
    owned = guard.strict_json((directory / 'owned-process.json').read_bytes())
    request = guard.strict_json((directory / 'request.json').read_bytes())
    nonce = control.get('nonce')
    guard.require(type(nonce) is str and str(uuid.UUID(nonce)) == nonce and
                  type(control.get('protocolVersion')) is int and control['protocolVersion'] == 3 and
                  type(control.get('commandID')) is int and control['commandID'] == 2 and
                  launch.get('nonce') == nonce and launch.get('clock_domain') == guard.CLOCK_DOMAIN,
                  'owned raw/control launch nonce/domain')
    app_origin, phase_observed, app_observed = verify_launch_phases(local, directory, case, receipt, launch, request, control)
    runner_pid = receipt.get('runner_pid')
    app_pid = binding.get('pid')
    binary = binding.get('binary_path')
    guard.require(type(runner_pid) is int and runner_pid > 0 and type(app_pid) is int and app_pid > 0 and
                  type(control.get('runnerPID')) is type(control.get('appPID')) is type(launch.get('pid')) is int and
                  type(owned.get('pid')) is int and
                  control.get('runnerPID') == launch.get('pid') == runner_pid and
                  control.get('appPID') == owned.get('pid') == app_pid and
                  type(binary) is str and control.get('binaryPath') == binary and
                  owned.get('command') == [binary] and owned.get('binary_sha256') == binding.get('binary_sha256') and
                  type(owned.get('runner_pid')) is int and owned['runner_pid'] == runner_pid and
                  owned.get('launch_nonce') == nonce and owned.get('clock_domain') == guard.CLOCK_DOMAIN and
                  type(owned.get('app_started_uptime_ns')) is int and owned['app_started_uptime_ns'] >= app_origin and
                  type(binding.get('binary_sha256')) is str and re.fullmatch('[0-9a-f]{64}', binding['binary_sha256']) and
                  control.get('native') == binding.get('native') == receipt.get('native_directory') and
                  control.get('runID') == binding.get('run_id') == request.get('run_id') == receipt.get('run_id'),
                  'immutable owned runner/app/path/native/run binding')
    command = launch.get('command')
    guard.require(type(command) is list and all(type(value) is str for value in command) and
                  command[:3] == ['python3', '-B', 'scripts/run_pmix_native.py'] and
                  command.count('--binary') == command.count('--capture-producer') == command.count('--output') ==
                  command.count('--mode') == command.count('--round') ==
                  command.count('--display-id') == command.count('--display-policy') == 1 and
                  command[command.index('--binary') + 1] == binary and
                  command[command.index('--mode') + 1] == case[0] and
                  command[command.index('--round') + 1] == str(case[1]) and
                  command[command.index('--display-id') + 1] == str(DISPLAY_ID) and
                  command[command.index('--display-policy') + 1] == 'frozen-60hz' and
                  command.count('--allow-display-mode-change') == command.count('--video') == 1,
                  'owned launch exact formal case/display')
    expected_command = ['python3', '-B', 'scripts/run_pmix_native.py', '--binary', binary,
                        '--capture-producer', command[command.index('--capture-producer') + 1],
                        '--output', command[command.index('--output') + 1], '--mode', case[0],
                        '--round', str(case[1]), '--video', '--display-id', '2',
                        '--display-policy', 'frozen-60hz', '--allow-display-mode-change']
    guard.require(('--fixture' in command) == (case[0] == 'workflow-reopen'), 'owned launch workflow chain argument')
    if case[0] == 'workflow-reopen':
        previous = Path(command[command.index('--output') + 1]).parent / 'workflow1/workflow-output.rcam'
        guard.require(command.count('--fixture') == 1 and
                      command[command.index('--fixture') + 1] == str(previous),
                      'owned launch preceding workflow output')
        expected_command += ['--fixture', str(previous)]
    guard.require(command == expected_command, 'immutable exact native runner argv')
    started = launch.get('started_uptime_ns')
    joined = receipt.get('runner_joined_at_ns')
    guard.require(type(started) is int and type(joined) is int and 0 < started <= joined and
                  joined - started <= guard.EXECUTION_SECONDS * 1_000_000_000,
                  'owned actual launch/join execution deadline')
    for name in ('window-ready.json', 'capture-complete.json'):
        marker = guard.strict_json((directory / name).read_bytes())
        guard.require(type(marker.get('app_pid')) is int and marker['app_pid'] == app_pid and
                      marker.get('run_id') == request['run_id'], 'owned ' + name + ' identity')
        if name == 'capture-complete.json':
            guard.require(marker.get('success') is True, 'owned successful capture marker')
    monitor_pid = receipt.get('monitor_pid')
    guard.require(type(monitor_pid) is int and monitor_pid > 0, 'owned observer PID')
    monitor = type('ReplayMonitor', (), {'pid': monitor_pid, 'returncode': None,
                                        'poll': lambda self: None})()
    tail = None
    try:
        tail = OwnedReplayTail(local / 'monitor.stdout')
        policy = guard.GuardPolicy(require_app_launch=True)
        stream = guard.ObservationStream(tail, monitor, nonce, policy=policy, clock=lambda: tail.now_ns)
        stream.expected_runner_pid, stream.expected_owned_pid = runner_pid, app_pid
        armed_end = None
        post_join = None
        while not tail.done:
            if tail.current is not None and tail.current.get('seq') == app_observed['sample_seq_before_binding']:
                policy.bind_app_launch_origin(app_origin)
            halt = stream.pump()
            guard.require(halt is None and stream.integrity, 'owned raw input/foreground/integrity failure' +
                          (': ' + str(halt.reason) + ': ' + str((halt.details or {}).get('error')) if halt else ''))
            row = tail.current
            if tail.done:
                break
            if row['event'] == 'sample':
                if any(action.kind == 'armed' for action in stream.actions):
                    armed_end = row['end_ns']
                if row['runner_pid']:
                    guard.require(armed_end is not None and started >= armed_end and row['begin_ns'] >= started,
                                  'owned runner begins only after actual raw ARMED')
                if row['begin_ns'] == receipt['post_join_sample_begin_ns']:
                    guard.require(joined <= row['begin_ns'] <= row['end_ns'] <= joined + guard.MAX_SAMPLE_GAP_NS,
                                  'owned raw genuine post-join barrier')
                    post_join = row
            elif row['event'] in ('runner_bound', 'owned_bound'):
                guard.require(armed_end is not None and started >= armed_end and row['at_ns'] >= started,
                              'owned kernel binding after launch/arm')
        guard.require(stream.runner_bound and stream.owned_bound and armed_end is not None and
                      started >= armed_end and post_join is not None and stream.policy.terminal is None and
                      stream.policy.first_foreground_ns == receipt.get('first_foreground_ns') and
                      stream.policy.capture_completed_ns == receipt.get('capture_completed_ns') and
                      stream.policy.capture_completed_ns is not None and
                      stream.policy.phase == receipt.get('phase') == 'CAPTURE_FINALIZED_CLEANUP',
                      'owned raw actual foreground/capture/bind/postjoin receipt agreement')
        guard.require(phase_observed.get('kernel_credential') == stream.runner_credential and
                      app_origin == stream.policy.app_launch_ns and armed_end == launch['armed_end_ns'],
                      'phase opportunity tied to original raw kernel runner and arming')
    finally:
        if tail:
            tail.close()
    return True

def validate_round(directory, case, receipt, seen_ids, workflow_sha, guard_directory=None):
    """Additional external guards; the original native verifier remains mandatory."""
    guard.require(case in CASES, 'matrix case')
    guard.require(receipt.get('success') is True and receipt.get('joined') is True and
                  type(receipt.get('actual_exit_code')) is int and receipt['actual_exit_code'] == 0 and
                  receipt.get('monitor_joined') is True and receipt.get('post_join_barrier_satisfied') is True and
                  receipt.get('interrupted') is None and receipt.get('clock_unavailable') is False and
                  receipt.get('cleanup_timeout') is False and receipt.get('clock_domain') == guard.CLOCK_DOMAIN and
                  receipt.get('monitor_exit_code') == -int(signal.SIGTERM) and
                  type(receipt.get('capture_completed_ns')) is int and receipt['capture_completed_ns'] > 0 and
                  type(receipt.get('first_foreground_ns')) is int and
                  0 < receipt['first_foreground_ns'] <= receipt['capture_completed_ns'] and
                  type(receipt.get('source_manifest_sha256')) is str and
                  re.fullmatch('[0-9a-f]{64}', receipt['source_manifest_sha256']) and
                  receipt.get('external_activation') is False and receipt.get('retry') is False and
                  type(receipt.get('execution_budget_seconds')) is int and receipt['execution_budget_seconds'] == 190 and
                  type(receipt.get('cleanup_budget_seconds')) is int and receipt['cleanup_budget_seconds'] == 40 and
                  receipt.get('display_policy') == 'frozen-60hz' and
                  type(receipt.get('display_id')) is int and receipt['display_id'] == DISPLAY_ID and
                  type(receipt.get('round')) is int and
                  (receipt.get('mode'), receipt.get('round')) == case, 'formal external owned guard result')
    joined, observed = receipt.get('runner_joined_at_ns'), receipt.get('post_join_sample_begin_ns')
    guard.require(type(joined) is int and type(observed) is int and 0 < joined <= observed and
                  observed <= joined + guard.MAX_SAMPLE_GAP_NS, 'formal post-join barrier')
    request = guard.strict_json((directory / 'request.json').read_bytes())
    guard.require(type(request.get('schema_version')) is int and request['schema_version'] == 4 and
                  type(request.get('round')) is int and (request.get('mode'), request['round']) == case and
                  type(request.get('display_id')) is int and request['display_id'] == DISPLAY_ID and
                  request.get('display_policy') == 'frozen-60hz' and
                  request.get('display_mode_change_authorized') is True and
                  request.get('evidence_scope') == 'full-pmix-native' and
                  request.get('source_manifest_sha256') == receipt.get('source_manifest_sha256'),
                  'formal request/display contract')
    run_id = request.get('run_id')
    guard.require(type(run_id) is str and str(uuid.UUID(run_id)) == run_id and
                  receipt.get('run_id') == run_id and run_id not in seen_ids, 'fresh independent run ID')
    displays = [guard.strict_json((directory / (name + '.json')).read_bytes()) for name in
                ('display-before', 'display-active', 'display-active-probe',
                 'display-restored', 'display-restored-probe')]
    for index, display in enumerate(displays):
        for moment in ('before', 'after'):
            snapshot = display[moment]
            guard.display_snapshot(snapshot, with_scale=index in (0, 2, 4))
    before, active, active_probe, restored, restored_probe = displays
    guard.require(type(before['after'].get('mode_id')) is int and before['after']['mode_id'] == ORIGINAL_MODE and
                  before['after']['refresh_hz'] == ORIGINAL_HZ and before['before'] == before['after'],
                  'original authorized 144Hz mode')
    guard.require(guard.display_snapshot(active['before'], False) == guard.display_snapshot(before['after']) and
                  active['after']['refresh_hz'] == 60 and active_probe['before'] == active_probe['after'] and
                  guard.display_snapshot(active_probe['after']) == guard.display_snapshot(active['after'], False) and
                  guard.display_snapshot(restored['before'], False) == guard.display_snapshot(active_probe['after']) and
                  guard.display_snapshot(restored['after'], False) == guard.display_snapshot(before['after']) and
                  restored_probe['before'] == restored_probe['after'] == before['after'],
                  'per-round real60Hz and exact same-target144Hz restoration')
    if case[0] == 'workflow-reopen':
        copied = guard.sha(directory / 'reopen-input.rcam')
        guard.require(workflow_sha is not None and copied == workflow_sha == request.get('fixture_sha256'),
                      'fresh reopen must consume preceding workflow output bytes')
    verify_owned_evidence(guard_directory or directory, directory, case, receipt)
    seen_ids.add(run_id)
    return guard.sha(directory / 'workflow-output.rcam') if case[0] == 'workflow' else workflow_sha


class ContinuousSentinel:
    """Unbound raw input stream with exactly one baseline across all twelve cases."""
    def __init__(self, base, executable, digest):
        self.base, self.executable, self.digest = base, executable, digest
        self.monitor = self.tail = self.stream = None
        self.nonce = str(uuid.uuid4())
        self.joined = self.barrier = False
        self.failure = None
        self.armed_seq = None
        self.final_boundary_ns = None
        self.stdout = self.stderr = None

    def start(self):
        guard.require(guard.sha(self.executable) == self.digest, 'sentinel executable identity')
        control = {'protocolVersion': 3, 'nonce': self.nonce, 'commandID': 0,
                   'runnerPID': 0, 'appPID': 0, 'binaryPath': None, 'native': None, 'runID': None}
        guard.write(self.base / 'continuous-control.json', control)
        self.stdout = (self.base / 'continuous.stdout').open('xb')
        self.stderr = (self.base / 'continuous.stderr').open('xb')
        self.monitor = subprocess.Popen([str(self.executable), str(self.base / 'continuous-control.json'), self.nonce],
                                        stdout=self.stdout, stderr=self.stderr)
        self.tail = guard.FileTail(self.base / 'continuous.stdout')
        self.stream = guard.ObservationStream(self.tail, self.monitor, self.nonce)
        while self.stream.policy.phase != 'ARMED':
            halt = self.pump()
            guard.require(halt is None, 'continuous monitor failed before any runner: ' + (halt.reason if halt else ''))
            time.sleep(.01)
        self.armed_seq = self.stream.policy.previous['seq']
        guard.write(self.base / 'continuous-armed.json', {'monitor_pid': self.monitor.pid,
                    'baseline_seq': self.armed_seq, 'nonce': self.nonce, 'clock_domain': guard.CLOCK_DOMAIN,
                    'scope': 'unbound input only; one continuous baseline; no identity reset'})

    def pump(self):
        guard.require(self.stream is not None, 'continuous sentinel not started')
        halt = self.stream.pump()
        if halt:
            self.failure = self.failure or halt.reason
        return halt

    def finish(self):
        """Called only after the last owned runner and round observer have joined."""
        error = None
        if self.stream:
            try:
                boundary = guard.system_uptime_ns()
                self.final_boundary_ns = boundary
                self.barrier = guard.post_join_barrier(self.stream, boundary,
                                                      clock=guard.system_uptime_ns, sleep=time.sleep)
                if self.stream.policy.terminal:
                    self.failure = self.failure or self.stream.policy.terminal.reason
            except BaseException as failure:
                error = str(failure)
                self.failure = self.failure or 'CONTINUOUS_FINAL_BARRIER_EXCEPTION'
        if self.monitor:
            try:
                self.joined = guard.stop_monitor(self.monitor)
                if self.monitor.returncode != -int(signal.SIGTERM):
                    self.failure = self.failure or 'CONTINUOUS_UNEXPECTED_FINAL_EXIT'
                elif self.stream:
                    self.stream.pump(allow_owned_stop=True)
                    guard.require(not self.tail.partial, 'partial terminal continuous observation')
                    if self.stream.policy.terminal:
                        self.failure = self.failure or self.stream.policy.terminal.reason
            except BaseException as failure:
                error = str(failure)
                self.failure = self.failure or 'CONTINUOUS_STOP_EXCEPTION'
        if self.tail:
            self.tail.close()
        for handle in (self.stdout, self.stderr):
            if handle:
                handle.close()
        guard.write(self.base / 'CONTINUOUS_RESULT.json', {
            'monitor_pid': self.monitor.pid if self.monitor else None,
            'monitor_joined': self.joined, 'post_interval_barrier_satisfied': self.barrier,
            'initial_baseline_seq': self.armed_seq,
            'final_boundary_ns': self.final_boundary_ns,
            'post_interval_sample_begin_ns': (self.stream.last_valid_sample['begin_ns']
                                              if self.barrier else None),
            'last_sample_seq': self.stream.last_valid_sample['seq'] if self.stream and self.stream.last_valid_sample else None,
            'failure': self.failure, 'error': error, 'clock_domain': guard.CLOCK_DOMAIN,
            'failure_details': self.stream.policy.terminal.details if self.stream and self.stream.policy.terminal else None,
            'first_stream_error': self.stream.first_stream_error if self.stream else None,
            'success': bool(self.joined and self.barrier and self.failure is None),
            'human_input_attributed': False, 'external_activation': False, 'baseline_resets': 0})
        return self.joined and self.barrier and self.failure is None


def watched_command(command, root, base, label, sentinel, seconds=VERIFICATION_SECONDS,
                    restoration_after_halt=False, deadline_ns=None):
    """Run a trusted offline check while continuing the same raw input stream."""
    if restoration_after_halt:
        guard.require(command[:4] == ['python3', '-B', str(Path(__file__).resolve()), 'recover'],
                      'only fixed-target restoration may continue after input halt')
    process = None
    joined = barrier = False
    interrupted = None
    clock_errors = []
    journal_errors = []
    started = joined_at = None
    try:
        halt = sentinel.pump()
        guard.require(restoration_after_halt or halt is None, 'continuous input guard halted before ' + label)
        if deadline_ns is not None:
            guard.require(guard.system_uptime_ns() < deadline_ns, 'existing cleanup budget exhausted')
        with (base / (label + '.stdout')).open('xb') as out, (base / (label + '.stderr')).open('xb') as err:
            process = subprocess.Popen(command, cwd=root, stdout=out, stderr=err)
            started = guard.system_uptime_ns()
            limit = min(started + seconds * 1_000_000_000, deadline_ns) if deadline_ns is not None else started + seconds * 1_000_000_000
            while True:
                halt = sentinel.pump()
                if halt and not restoration_after_halt:
                    raise RuntimeError('continuous input guard: ' + halt.reason)
                if process.poll() is not None:
                    joined = True
                    joined_at = guard.system_uptime_ns()
                    guard.require(joined_at <= limit, 'owned verification deadline exceeded at observed join')
                    break
                guard.require(guard.system_uptime_ns() <= limit,
                              'owned verification deadline')
                time.sleep(.01)
    except BaseException as failure:
        interrupted = type(failure).__name__ + ': ' + str(failure)
        if process:
            prior_signal = False
            if restoration_after_halt and process.poll() is None:
                # The round cleanup budget is already running. Clock loss cannot
                # grant this newly owned recovery child a second fresh reserve.
                process.send_signal(signal.SIGINT)
                prior_signal = True
                try:
                    guard.write(base / 'controlled-interrupt.json', {'runner_pid': process.pid,
                                'signal': int(signal.SIGINT), 'reason': interrupted,
                                'scope': 'only launched recovery child; original round cleanup budget'})
                except BaseException as journal_error:
                    journal_errors.append(type(journal_error).__name__ + ': ' + str(journal_error))
            joined = guard.interrupt_and_join(process, interrupted, base, deadline_ns=deadline_ns,
                                              signal_already_sent=prior_signal, clock_errors=clock_errors,
                                              journal_errors=journal_errors)
            if joined and joined_at is None:
                try:
                    joined_at = guard.system_uptime_ns()
                except guard.ClockUnavailableError as error:
                    clock_errors.append(str(error))
    finally:
        observed_limit = (min(started + seconds * 1_000_000_000, deadline_ns) if deadline_ns is not None else
                          started + seconds * 1_000_000_000) if started is not None else deadline_ns
        budget_exceeded = bool(joined_at is not None and observed_limit is not None and joined_at > observed_limit)
        if budget_exceeded:
            interrupted = interrupted or 'OWNED_VERIFICATION_DEADLINE'
        if joined and joined_at is not None:
            try:
                barrier = guard.post_join_barrier(sentinel.stream, joined_at,
                                                 clock=guard.system_uptime_ns, sleep=time.sleep)
            except guard.ClockUnavailableError as error:
                clock_errors.append(str(error))
        receipt = {'command': command, 'pid': process.pid if process else None,
                   'exit_code': process.returncode if process else None, 'joined': joined,
                   'post_join_barrier_satisfied': barrier, 'failure': interrupted,
                   'clock_unavailable': bool(clock_errors), 'clock_domain': guard.CLOCK_DOMAIN,
                   'interrupt_journal_errors': journal_errors,
                   'joined_at_ns': joined_at, 'deadline_ns': observed_limit, 'deadline_exceeded': budget_exceeded,
                   'success': bool(process and joined and process.returncode == 0 and barrier and
                                   interrupted is None and not clock_errors and not journal_errors and
                                   (sentinel.pump() is None or restoration_after_halt))}
        guard.write(base / (label + '.json'), receipt)
    guard.require(receipt['success'], 'owned verification failed: ' + label)
    return receipt


def check_round(input_path):
    """Pure offline owned-round check, isolated from the live sentinel consumer."""
    pins = require_pins()
    local = input_path.parent.resolve(strict=True)
    result = {'result': 'BLOCKED', 'success': False, 'failure': None}
    try:
        raw = input_path.read_bytes()
        guard.require(0 < len(raw) <= guard.MAX_LINE_BYTES, 'round check input size')
        control = guard.strict_json(raw)
        guard.require(type(control) is dict and set(control) == {
            'schema_version', 'mode', 'round', 'directory', 'seen_run_ids',
            'workflow_sha256', 'supervisor_receipt_sha256'} and
            type(control['schema_version']) is int and control['schema_version'] == 1 and
            type(control['round']) is int and (control['mode'], control['round']) in CASES and
            type(control['directory']) is str and type(control['seen_run_ids']) is list and
            len(control['seen_run_ids']) < len(CASES), 'round check immutable input')
        seen = control['seen_run_ids']
        guard.require(all(type(value) is str and str(uuid.UUID(value)) == value for value in seen) and
                      seen == sorted(set(seen)), 'round check preceding run IDs')
        workflow = control['workflow_sha256']
        guard.require(workflow is None or (type(workflow) is str and re.fullmatch('[0-9a-f]{64}', workflow)),
                      'round check preceding workflow digest')
        directory = Path(control['directory']).resolve(strict=True)
        receipt_path = local / 'SUPERVISOR_RESULT.json'
        receipt_raw = receipt_path.read_bytes()
        receipt_sha = hashlib.sha256(receipt_raw).hexdigest()
        guard.require(type(control['supervisor_receipt_sha256']) is str and
                      control['supervisor_receipt_sha256'] == receipt_sha,
                      'round check original supervisor receipt')
        receipt = guard.strict_json(receipt_raw)
        guard.require(receipt.get('source_manifest_sha256') == pins.manifest_sha,
                      'round check pinned product source')
        original_launch = guard.strict_json((local / 'runner-launch.json').read_bytes())
        original_command = original_launch.get('command')
        guard.require(type(original_command) is list and original_command.count('--output') == 1 and
                      str(directory) == original_command[original_command.index('--output') + 1],
                      'round check canonical output/owned launch binding')
        seen = set(seen)
        workflow = validate_round(directory, (control['mode'], control['round']), receipt,
                                  seen, workflow, guard_directory=local)
        result.update(result='OWNED_ROUND_EVIDENCE_VERIFIED', success=True,
                      mode=control['mode'], round=control['round'], run_id=receipt['run_id'],
                      seen_run_ids=sorted(seen), workflow_sha256=workflow,
                      input_sha256=hashlib.sha256(raw).hexdigest(),
                      supervisor_receipt_sha256=receipt_sha)
    except BaseException as error:
        result['failure'] = type(error).__name__ + ': ' + str(error)
    guard.write(local / 'round-check-result.json', result)
    return 0 if result['success'] else 2


def watched_round_check(root, directory, local, case, receipt, seen_ids, workflow_sha, sentinel):
    """Keep consuming every live sample throughout raw replay and project hashing."""
    input_path = local / 'round-check-input.json'
    receipt_raw = (local / 'SUPERVISOR_RESULT.json').read_bytes()
    receipt_sha = hashlib.sha256(receipt_raw).hexdigest()
    guard.require(guard.strict_json(receipt_raw) == receipt, 'original parsed supervisor receipt bytes')
    directory = directory.resolve(strict=True)
    original_launch = guard.strict_json((local / 'runner-launch.json').read_bytes())
    original_command = original_launch.get('command')
    guard.require(type(original_command) is list and original_command.count('--output') == 1 and
                  str(directory) == original_command[original_command.index('--output') + 1],
                  'original round output/owned launch binding')
    control = {'schema_version': 1, 'mode': case[0], 'round': case[1],
               'directory': str(directory), 'seen_run_ids': sorted(seen_ids),
               'workflow_sha256': workflow_sha, 'supervisor_receipt_sha256': receipt_sha}
    # Bind the exact bytes this parent writes, rather than any later file state.
    expected_input_sha = hashlib.sha256((json.dumps(control, indent=2, allow_nan=False) + '\n').encode('utf-8')).hexdigest()
    guard.write(input_path, control)
    guard.require(guard.sha(input_path) == expected_input_sha, 'original round check input bytes')
    command = ['python3', '-B', str(Path(__file__).resolve()), 'check-round',
               '--root', str(root), '--round-check-input', str(input_path)]
    try:
        watched_command(command, root, local, 'round-evidence-check', sentinel)
    except RuntimeError as error:
        result_path = local / 'round-check-result.json'
        if result_path.is_file():
            raw_failure = result_path.read_bytes()
            guard.require(0 < len(raw_failure) <= guard.MAX_LINE_BYTES, 'failed round check result size')
            failed = guard.strict_json(raw_failure)
            if type(failed) is dict and type(failed.get('failure')) is str:
                raise RuntimeError(str(error) + ': ' + failed['failure']) from error
        raise
    raw = (local / 'round-check-result.json').read_bytes()
    guard.require(0 < len(raw) <= guard.MAX_LINE_BYTES, 'round check result size')
    result = guard.strict_json(raw)
    run_id = receipt.get('run_id')
    guard.require(type(result) is dict and result.get('result') == 'OWNED_ROUND_EVIDENCE_VERIFIED' and
                  result.get('success') is True and result.get('failure') is None and
                  type(result.get('round')) is int and (result.get('mode'), result['round']) == case and
                  type(run_id) is str and str(uuid.UUID(run_id)) == run_id and run_id not in seen_ids and
                  result.get('run_id') == run_id and result.get('seen_run_ids') == sorted(seen_ids | {run_id}) and
                  result.get('input_sha256') == expected_input_sha == guard.sha(input_path) and
                  result.get('supervisor_receipt_sha256') == receipt_sha == guard.sha(local / 'SUPERVISOR_RESULT.json'),
                  'joined round check result/input/identity binding')
    workflow = result.get('workflow_sha256')
    guard.require('workflow_sha256' in result and
                  ((case[0] == 'workflow' and type(workflow) is str and re.fullmatch('[0-9a-f]{64}', workflow)) or
                   (case[0] != 'workflow' and workflow == workflow_sha)), 'joined round check workflow chain')
    guard.require(sentinel.pump() is None, 'continuous input halt after evidence check')
    seen_ids.add(run_id)
    return workflow


DISPLAY_LABELS = ('display-before', 'display-active', 'display-active-probe',
                  'display-restored', 'display-restored-probe')
SUBCOMMAND_LABELS = set(DISPLAY_LABELS) | {'environment-os', 'environment-machine',
                                        'environment-memory', 'environment-power', 'window-query', 'image'}


def original_snapshot(snapshot):
    guard.require(type(snapshot) is dict and type(snapshot.get('display_id')) is int and
                  snapshot['display_id'] == DISPLAY_ID and type(snapshot.get('mode_id')) is int and
                  snapshot['mode_id'] == ORIGINAL_MODE and snapshot.get('refresh_hz') == ORIGINAL_HZ and
                  snapshot.get('in_mirror_set') is False and
                  all(type(snapshot.get(key)) in (int, float) and snapshot[key] == value for key, value in GEOMETRY.items()),
                  'immutable original target144Hz snapshot')
    return snapshot


def helper_source(root, name='MUTATOR_SOURCE'):
    """Extract the reviewed literal, without executing a product module."""
    tree = ast.parse((root / 'scripts/pmix_display_swift.py').read_text())
    values = [node.value for node in tree.body if isinstance(node, ast.Assign) and
              any(isinstance(target, ast.Name) and target.id == name for target in node.targets)]
    guard.require(len(values) == 1, 'single reviewed PMIX display helper literal')
    source = ast.literal_eval(values[0])
    guard.require(type(source) is str and source, 'reviewed PMIX display helper source')
    return source


def prove_subcommands_joined(native, runner_exit):
    """Fail closed when durable records cannot exclude a delayed display child.

    Signal death or compound journal/cleanup failures can omit a whole pair.
    Only all five completed display stages can exclude that unrecorded setter.
    A nonnegative exit alone cannot prove absent stages were never launched.
    """
    launches = {path.name.removesuffix('.subcommand-launch.json'): path
                for path in native.glob('*.subcommand-launch.json')}
    processes = {path.name.removesuffix('.subcommand-process.json'): path
                 for path in native.glob('*.subcommand-process.json')}
    guard.require(launches and set(launches) == set(processes) and set(launches) <= SUBCOMMAND_LABELS,
                  'all owned subcommand launch/process records paired')
    guard.require(type(runner_exit) is int, 'owned runner actual joined exit')
    guard.require(set(DISPLAY_LABELS) <= set(launches),
                  'incomplete display journal cannot prove absence of unrecorded child on any exit')
    for label, path in launches.items():
        guard.require(not path.is_symlink() and not processes[label].is_symlink(), 'regular owned child receipts')
        launch = guard.strict_json(path.read_bytes())
        process = guard.strict_json(processes[label].read_bytes())
        guard.require(type(launch.get('pid')) is int and launch['pid'] > 0 and
                      launch.get('private_session') is True and type(launch.get('pgid')) is int and
                      launch['pgid'] == launch['pid'] and
                      launch == {key: process.get(key) for key in
                                 ('pid', 'pgid', 'private_session', 'command', 'started_monotonic_ns', 'timeout_seconds')} and
                      process.get('joined') is True and process.get('owned_group_released') is True and
                      type(process.get('exit_code')) is int and type(process.get('schema_version')) is int and
                      process['schema_version'] == 2 and
                      type(process.get('started_monotonic_ns')) is int and
                      type(process.get('finished_monotonic_ns')) is int and
                      0 < process['started_monotonic_ns'] <= process['finished_monotonic_ns'],
                      'owned child actual join/group release')
        if label in DISPLAY_LABELS:
            operation = 'set60' if label == 'display-active' else ('restore' if label == 'display-restored' else 'probe')
            script = 'display-probe.swift' if operation == 'probe' else 'display.swift'
            expected = [*guard.DISPLAY_SWIFT_PREFIX, str(native / script), operation, str(DISPLAY_ID)]
            if operation == 'restore': expected += [str(ORIGINAL_MODE)]
            guard.require(launch.get('command') == expected, 'same-target inner display command')
    return True


def assess_and_recover(local, root, sentinel):
    """Restore only the fixed original display, after provable owned joins."""
    assessment = {'result': 'BLOCKED', 'restoration_verified': False, 'reason': None,
                  'possible_remaining60Hz': True, 'display_id': DISPLAY_ID, 'mode_id': ORIGINAL_MODE}
    try:
        receipt = guard.strict_json((local / 'SUPERVISOR_RESULT.json').read_bytes())
        guard.require(receipt.get('joined') is True and type(receipt.get('actual_exit_code')) is int,
                      'inner runner join required before emergency restore')
        deadline = receipt.get('cleanup_deadline_ns')
        if deadline is None:
            guard.require(receipt.get('cleanup_started') is False and
                          receipt.get('cleanup_deadline_unknown') is False,
                          'already-started or unknown cleanup budget cannot restart from join')
            joined_at = receipt.get('runner_joined_at_ns')
            guard.require(type(joined_at) is int and joined_at > 0, 'recovery join clock required')
            deadline = joined_at + guard.CLEANUP_SECONDS * 1_000_000_000
        guard.require(type(deadline) is int, 'existing round cleanup deadline')
        binding = guard.strict_json((local / 'native-binding.json').read_bytes())
        snapshot = original_snapshot(binding.get('initial_display_snapshot'))
        native = Path(binding['native']).resolve(strict=True)
        guard.require(not native.is_symlink() and receipt.get('native_directory') == str(native) and
                      binding.get('run_id') == receipt.get('run_id'), 'immutable owned native binding')
        request = guard.strict_json((native / 'request.json').read_bytes())
        guard.require(request.get('run_id') == receipt['run_id'] and type(request.get('display_id')) is int and
                      request['display_id'] == DISPLAY_ID and request.get('source_manifest_sha256') == receipt.get('source_manifest_sha256'),
                      'owned recovery request target/run/source')
        source = helper_source(root)
        guard.require((native / 'display.swift').read_text() == source, 'reviewed native display helper bytes')
        probe_source = helper_source(root, 'PROBE_SOURCE')
        guard.require((native / 'display-probe.swift').read_text() == probe_source,
                      'reviewed native independent probe bytes')
        prove_subcommands_joined(native, receipt['actual_exit_code'])
        final_path = native / 'display-restored-probe.json'
        if final_path.is_file():
            final = guard.strict_json(final_path.read_bytes())
            if final.get('before') == final.get('after') == snapshot:
                assessment.update(result='ALREADY_RESTORED_VERIFIED', restoration_verified=True,
                                  possible_remaining60Hz=False)
                return assessment
        recovery = local / 'display-recovery'
        # Reserve the unchanged final monitor-stop and fresh-observation budget.
        recovery_deadline = deadline - int((guard.MONITOR_STOP_SECONDS + guard.POST_JOIN_BARRIER_SECONDS) * 1e9)
        guard.require(guard.system_uptime_ns() < recovery_deadline, 'no remaining round cleanup reserve for restoration')
        recovery.mkdir(exist_ok=False)
        expected = recovery / 'expected-original.json'
        guard.write(expected, snapshot)
        helper = recovery / 'display.swift'
        helper.write_text(source)
        probe_helper = recovery / 'display-probe.swift'
        probe_helper.write_text(probe_source)
        command = ['python3', '-B', str(Path(__file__).resolve()), 'recover', '--root', str(root),
                   '--display-helper', str(helper), '--display-probe-helper', str(probe_helper),
                   '--expected-snapshot', str(expected),
                   '--recovery-output', str(recovery)]
        watched_command(command, root, recovery, 'owned-recovery', sentinel, seconds=40,
                        restoration_after_halt=True, deadline_ns=recovery_deadline)
        restored = guard.strict_json((recovery / 'RESTORATION_RESULT.json').read_bytes())
        guard.require(restored.get('success') is True and restored.get('snapshot') == snapshot,
                      'independent emergency post-join144Hz probe')
        assessment.update(result='EMERGENCY_RESTORED_VERIFIED', restoration_verified=True,
                          possible_remaining60Hz=False)
    except BaseException as error:
        assessment['reason'] = type(error).__name__ + ': ' + str(error)
    finally:
        guard.write(local / 'RESTORATION_ASSESSMENT.json', assessment)
    return assessment


def recover_display(*, root, helper, probe_helper, expected, output):
    """Trusted owned restoration child. It may never start an app or set60."""
    pins = require_pins()
    guard.require(sys.platform == 'darwin', 'display restoration only on macOS')
    root = root.resolve(strict=True)
    output = output.resolve(strict=True)
    guard.require(output != root and root not in output.parents and helper.parent.resolve(strict=True) == output and
                  expected.parent.resolve(strict=True) == output and probe_helper.parent.resolve(strict=True) == output,
                  'recovery evidence outside product source')
    verify_source(root, pins)
    snapshot = original_snapshot(guard.strict_json(expected.read_bytes()))
    guard.require(helper.read_text() == helper_source(root), 'locked emergency display helper')
    guard.require(probe_helper.read_text() == helper_source(root, 'PROBE_SOURCE'), 'locked independent emergency probe')
    sys.path.insert(0, str(root / 'scripts'))
    from pmix_owned_command import owned_command, validate_display_receipt, validate_display_phases
    success = False
    failure = None
    try:
        restore = [*guard.DISPLAY_SWIFT_PREFIX, str(helper), 'restore', str(DISPLAY_ID), str(ORIGINAL_MODE)]
        returned = owned_command(output, 'display-restored', restore, check=True)
        restored = guard.strict_json(returned.stdout)
        validate_display_receipt(restored, DISPLAY_ID, probe=False)
        restore_process = guard.strict_json((output / 'display-restored.subcommand-process.json').read_bytes())
        validate_display_phases(returned.stderr, 'restore', DISPLAY_ID, restore_process['pid'], restored)
        guard.require(guard.display_snapshot(restored['after'], False) == guard.display_snapshot(snapshot),
                      'emergency same-target mode restoration')
        # owned_command has already joined leader and exclusive group before probe.
        probe = [*guard.DISPLAY_SWIFT_PREFIX, str(probe_helper), 'probe', str(DISPLAY_ID)]
        returned = owned_command(output, 'display-restored-probe', probe, check=True)
        observed = guard.strict_json(returned.stdout)
        validate_display_receipt(observed, DISPLAY_ID, probe=True)
        probe_process = guard.strict_json((output / 'display-restored-probe.subcommand-process.json').read_bytes())
        validate_display_phases(returned.stderr, 'probe', DISPLAY_ID, probe_process['pid'], observed)
        guard.require(observed.get('before') == observed.get('after') == snapshot,
                      'emergency mode persists after restorer join')
        success = True
    except BaseException as error:
        failure = type(error).__name__ + ': ' + str(error)
    guard.write(output / 'RESTORATION_RESULT.json', {'success': success, 'failure': failure,
                'snapshot': snapshot if success else None, 'display_id': DISPLAY_ID,
                'mode_id': ORIGINAL_MODE, 'stage_PASS_claim': False})
    return 0 if success else 2

def compile_monitor(base, source):
    executable = base / 'interference-monitor'
    command = ['/usr/bin/swiftc', '-parse-as-library', '-swift-version', '6',
               '-strict-concurrency=complete', '-warnings-as-errors', str(source), '-o', str(executable)]
    with (base / 'monitor-compile.stdout').open('xb') as out, (base / 'monitor-compile.stderr').open('xb') as err:
        compiled = subprocess.run(command, stdout=out, stderr=err, timeout=30)
    guard.write(base / 'monitor-compile.json', {'command': command, 'exit_code': compiled.returncode})
    guard.require(compiled.returncode == 0, 'strict Swift6 monitor compilation')
    return executable, guard.sha(executable)


def seal_bundle(bundle, review, rows, evidence):
    guard.require(len(rows) == len(CASES) and [(row['mode'], row['round']) for row in rows] == list(CASES),
                  'complete frozen matrix before sealing')
    original = verify_background_preserved(bundle, evidence)
    guard.require(review == original, 'only the qualified original review may be sealed')
    review = dict(review, native=rows)
    guard.write(bundle / 'REVIEW.json', review)
    manifest = bundle / 'Evidence_MANIFEST.sha256'
    verify_background_preserved(bundle, evidence, rows)
    with manifest.open('w') as handle:
        for path in sorted(bundle.rglob('*')):
            guard.require(not path.is_symlink(), 'bundle symlink')
            if path.is_file() and path != manifest:
                handle.write(guard.sha(path) + '  ' + path.relative_to(bundle).as_posix() + '\n')


def run_matrix(*, root, binary, producer, bundle, evidence, gate_ledger):
    pins = require_pins()  # Before any filesystem changes/compiler/Popen.
    guard.require(sys.platform == 'darwin', 'full GUI matrix runs only on macOS')
    guard.require(not bundle.is_symlink(), 'fresh seed cannot be a symlink')
    root, binary, producer, bundle = [path.resolve(strict=True) for path in (root, binary, producer, bundle)]
    evidence = evidence.resolve(strict=False)
    guard.require(evidence != bundle and bundle not in evidence.parents and evidence not in bundle.parents,
                  'external guard evidence must be outside canonical product bundle')
    guard.require(root != evidence and root not in evidence.parents and root != bundle and root not in bundle.parents,
                  'runtime evidence outside product source')
    verify_source(root, pins)
    guard.require(guard.sha(binary) == pins.binary_sha and guard.sha(producer) == pins.producer_sha,
                  'locked app and producer')
    review = validate_seed(bundle, pins, gate_ledger)
    evidence.mkdir(parents=True, exist_ok=False)
    qualification = qualify_background(root, bundle, evidence, pins, gate_ledger)
    guard.system_uptime_ns()
    monitor = compile_monitor(evidence, Path(__file__).with_name('interference.swift'))
    sentinel = ContinuousSentinel(evidence, *monitor)
    rows, results, seen_ids = [], [], set()
    workflow_sha = None
    failure = None
    continuous_pass = False
    last_local = None
    recovery = None
    try:
        sentinel.start()
        (bundle / 'native').mkdir(exist_ok=False)
        for case in CASES:
            guard.require(sentinel.pump() is None, 'continuous input halt between rounds')
            name = case_name(case)
            local = evidence / name
            local.mkdir(exist_ok=False)
            last_local = local
            output = bundle / 'native' / name
            fixture = bundle / 'native/workflow1/workflow-output.rcam' if case[0] == 'workflow-reopen' else None
            if fixture:
                guard.require(workflow_sha is not None and fixture.is_file(), 'preceding workflow output required')
            full = guard.FullRun(root, binary, producer, output, case[0], case[1], DISPLAY_ID,
                                 pins.manifest_sha, pins.binary_sha, pins.producer_sha, pins.runner_sha, fixture)
            code = guard.run(local, full=full, continuous_guard=sentinel.pump, compiled_monitor=monitor)
            receipt = guard.strict_json((local / 'SUPERVISOR_RESULT.json').read_bytes())
            results.append({'mode': case[0], 'round': case[1], 'guard_exit_code': code,
                            'run_id': receipt.get('run_id'), 'guard_success': receipt.get('success')})
            guard.require(code == 0 and receipt.get('success') is True, 'first failed case stops matrix: ' + name)
            watched_command(command_native_verify(root, output, pins), root, local, 'native-verifier', sentinel)
            workflow_sha = watched_round_check(root, output, local, case, receipt, seen_ids, workflow_sha, sentinel)
            rows.append({'mode': case[0], 'round': case[1], 'directory': 'native/' + name})
    except BaseException as error:
        failure = type(error).__name__ + ': ' + str(error)
    finally:
        if failure is not None and last_local is not None:
            recovery = assess_and_recover(last_local, root, sentinel)
        continuous_pass = sentinel.finish()
        success = failure is None and len(rows) == len(CASES) and continuous_pass
        result = {'result': 'GUARDED_FULL12_EXECUTION_COMPLETE' if success else 'BLOCKED',
                  'success': success, 'failure': failure, 'completed_cases': len(rows), 'expected_cases': 12,
                  'runs': results, 'continuous_input_guard': continuous_pass, 'retry': False,
                  'display_id': DISPLAY_ID, 'display_policy': 'frozen-60hz',
                  'restoration_policy': 'each owned runner finally restores its original target144Hz snapshot',
                  'source_manifest_sha256': pins.manifest_sha, 'build_commit': pins.commit,
                  'gate_ledger_sha256': gate_ledger, 'stage_PASS_claim': False,
                  'background_qualification': qualification['result'],
                  'background_status': qualification['background_status'],
                  'native_aggregate': 'NOT_RUN', 'full48': 'NOT_RUN', 'recovery': recovery, 'utc': guard.utc()}
        guard.write(evidence / 'MATRIX_RESULT.json', result)
    if success:
        # Offline seal/aggregate/attacks happen after the protected GUI interval.
        seal_bundle(bundle, review, rows, evidence)
    return 0 if success else 2


def verify_continuous_evidence(evidence, launches, joined_times):
    """Replay raw unbound policy without resetting the first baseline."""
    result = guard.strict_json((evidence / 'CONTINUOUS_RESULT.json').read_bytes())
    armed = guard.strict_json((evidence / 'continuous-armed.json').read_bytes())
    guard.require(result.get('success') is True and result.get('monitor_joined') is True and
                  result.get('post_interval_barrier_satisfied') is True and result.get('baseline_resets') == 0 and
                  result.get('initial_baseline_seq') == armed.get('baseline_seq') == 2 and
                  result.get('failure') is None and result.get('clock_domain') == guard.CLOCK_DOMAIN and
                  result.get('monitor_pid') == armed.get('monitor_pid'), 'continuous guard receipt')
    policy = guard.GuardPolicy()
    first = last = None
    armed_end = None
    ready = False
    with (evidence / 'continuous.stdout').open('rb') as handle:
        for data in handle:
            guard.require(data.endswith(b'\n') and 0 < len(data) <= guard.MAX_LINE_BYTES, 'complete raw continuous line')
            row = guard.strict_json(data)
            guard.require(type(row) is dict and type(row.get('protocol_version')) is int and
                          row['protocol_version'] == 3 and row.get('nonce') == armed['nonce'] and
                          row.get('clock_domain') == guard.CLOCK_DOMAIN, 'continuous raw envelope')
            if row.get('event') == 'ready':
                guard.require(not ready and first is None and row.get('monitor_pid') == result['monitor_pid'] and
                              row.get('thread_main') is True, 'one continuous ready identity')
                ready = True
            else:
                guard.require(ready and row.get('event') == 'sample' and row.get('runner_pid') == row.get('owned_pid') == 0,
                              'continuous sentinel must remain unbound')
                if first is None:
                    guard.require(row.get('seq') == 1, 'continuous raw must start at sample1')
                action = policy.observe(row)
                guard.require(action.kind != 'halt', 'raw continuous input/integrity failure')
                if action.kind == 'armed':
                    guard.require(row['seq'] == armed['baseline_seq'] == 2, 'actual continuous ARMED sequence')
                    armed_end = row['end_ns']
                first = first or row
                last = row
    guard.require(first is not None and last is not None and policy.phase == 'ARMED' and
                  last['seq'] == result.get('last_sample_seq'), 'complete continuous sample inventory')
    boundary = result.get('final_boundary_ns')
    guard.require(type(boundary) is int and result.get('post_interval_sample_begin_ns') == last['begin_ns'] and
                  boundary <= last['begin_ns'] <= last['end_ns'] <= boundary + guard.MAX_SAMPLE_GAP_NS and
                  armed_end is not None and all(type(start) is int and armed_end <= start for start in launches) and
                  all(type(joined) is int and joined <= boundary for joined in joined_times),
                  'continuous raw interval covers every owned round through final cleanup')
    return True

def validate_completed(*, root, bundle, evidence, gate_ledger, attacks):
    """Offline original aggregation plus 48 actual resealed attacks, no GUI launch."""
    pins = require_pins()
    root, bundle, evidence = [path.resolve(strict=True) for path in (root, bundle, evidence)]
    verify_source(root, pins)
    result = guard.strict_json((evidence / 'MATRIX_RESULT.json').read_bytes())
    guard.require(result.get('success') is True and result.get('completed_cases') == 12 and
                  result.get('source_manifest_sha256') == pins.manifest_sha and
                  result.get('build_commit') == pins.commit and result.get('gate_ledger_sha256') == gate_ledger and
                  result.get('continuous_input_guard') is True, 'completed guarded matrix required')
    guard.require(guard.sha(bundle / 'gates/gates.json') == gate_ledger, 'unchanged external gate ledger')
    guard.require(not attacks.exists(), 'fresh full48 output required')
    rows = [{'mode': case[0], 'round': case[1], 'directory': 'native/' + case_name(case)} for case in CASES]
    verify_background_qualification(root, bundle, evidence, pins, gate_ledger, rows)
    seen_ids, launches, joins = set(), [], []
    workflow = None
    for case in CASES:
        local = evidence / case_name(case)
        receipt = guard.strict_json((local / 'SUPERVISOR_RESULT.json').read_bytes())
        guard.require(receipt.get('source_manifest_sha256') == pins.manifest_sha, 'guard/product source identity')
        workflow = validate_round(bundle / 'native' / case_name(case), case, receipt, seen_ids, workflow, guard_directory=local)
        launch = guard.strict_json((local / 'runner-launch.json').read_bytes())
        guard.require(launch.get('pid') == receipt.get('runner_pid') and launch.get('clock_domain') == guard.CLOCK_DOMAIN,
                      'own launch/join identity')
        launches.append(launch.get('started_uptime_ns'))
        joins.append(receipt['runner_joined_at_ns'])
    guard.require([row.get('run_id') for row in result.get('runs', [])] ==
                  [guard.strict_json((evidence / case_name(case) / 'SUPERVISOR_RESULT.json').read_bytes())['run_id'] for case in CASES],
                  'matrix summary/actual twelve guard run bindings')
    guard.require(all(launches[index] >= joins[index - 1] for index in range(1, len(CASES))),
                  'next owned round may launch only after previous actual join')
    verify_continuous_evidence(evidence, launches, joins)
    tasks = [('aggregate', command_aggregate(root, bundle, pins, gate_ledger), VERIFICATION_SECONDS),
             ('full48', command_attacks(root, bundle, pins, gate_ledger, attacks), ATTACK_SECONDS)]
    failure = None
    records = []
    try:
        for label, command, seconds in tasks:
            with (evidence / (label + '.stdout')).open('xb') as out, (evidence / (label + '.stderr')).open('xb') as err:
                completed = subprocess.run(command, cwd=root, stdout=out, stderr=err, timeout=seconds)
            row = {'command': command, 'actual_exit_code': completed.returncode, 'timeout_seconds': seconds}
            guard.write(evidence / (label + '.json'), row)
            records.append(row)
            guard.require(completed.returncode == 0, 'original product verification failed: ' + label)
        summary = guard.strict_json((attacks / 'SUMMARY.json').read_bytes())
        attacked = guard.strict_json((attacks / 'RESULTS.json').read_bytes())
        tree = ast.parse((root / 'scripts/run_pmix_bundle_attacks.py').read_text())
        inventories = [ast.literal_eval(node.value) for node in tree.body if isinstance(node, ast.Assign) and
                       any(isinstance(target, ast.Name) and target.id == 'CASES' for target in node.targets)]
        guard.require(len(inventories) == 1 and type(inventories[0]) is list and len(inventories[0]) == 48 and
                      all(type(name) is str for name in inventories[0]) and len(set(inventories[0])) == 48,
                      'reviewed original named48 attack inventory')
        guard.require(summary.get('result') == 'ALL_REAL_RESEALED_ATTACKS_REJECTED' and
                      summary.get('cases') == 48 and summary.get('authentic_baseline_reverified_each_time') is True and
                      len(attacked) == 48 and [row.get('id') for row in attacked] == inventories[0] and
                      all(row.get('result') == 'REJECTED' and row.get('resealed') is True for row in attacked),
                      'all48 authentic resealed negative results required')
    except BaseException as error:
        failure = type(error).__name__ + ': ' + str(error)
    receipt = {'result': 'ORIGINAL_AGGREGATE_AND_FULL48_VERIFIED' if failure is None else 'BLOCKED',
               'success': failure is None, 'failure': failure, 'commands': records,
               'stage_PASS_claim': False, 'user_flicker_report': 'OPEN', 'Windows': 'DEFERRED',
               'source_manifest_sha256': pins.manifest_sha, 'build_commit': pins.commit, 'utc': guard.utc()}
    guard.write(evidence / 'VALIDATION_RESULT.json', receipt)
    return 0 if failure is None else 2


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('operation', choices=('run', 'validate', 'recover', 'check-round'))
    parser.add_argument('--root', required=True, type=Path)
    parser.add_argument('--bundle', type=Path)
    parser.add_argument('--guard-evidence', type=Path)
    parser.add_argument('--gate-ledger-sha256')
    parser.add_argument('--binary', type=Path)
    parser.add_argument('--producer', type=Path)
    parser.add_argument('--attacks-output', type=Path)
    parser.add_argument('--display-helper', type=Path)
    parser.add_argument('--display-probe-helper', type=Path)
    parser.add_argument('--expected-snapshot', type=Path)
    parser.add_argument('--recovery-output', type=Path)
    parser.add_argument('--round-check-input', type=Path)
    args = parser.parse_args()
    if args.operation == 'check-round':
        if args.round_check_input is None:
            parser.error('owned round check input required')
        return check_round(args.round_check_input)
    if args.operation == 'recover':
        if args.display_helper is None or args.display_probe_helper is None or args.expected_snapshot is None or args.recovery_output is None:
            parser.error('owned recovery helper/snapshot/output required')
        return recover_display(root=args.root, helper=args.display_helper, probe_helper=args.display_probe_helper,
                               expected=args.expected_snapshot, output=args.recovery_output)
    if args.bundle is None or args.guard_evidence is None or args.gate_ledger_sha256 is None:
        parser.error('bundle/guard-evidence/external gate ledger required')
    if args.operation == 'run':
        if args.binary is None or args.producer is None:
            parser.error('--binary and --producer required for run')
        return run_matrix(root=args.root, binary=args.binary, producer=args.producer, bundle=args.bundle,
                          evidence=args.guard_evidence, gate_ledger=args.gate_ledger_sha256)
    if args.attacks_output is None:
        parser.error('--attacks-output required for validate')
    return validate_completed(root=args.root, bundle=args.bundle, evidence=args.guard_evidence,
                              gate_ledger=args.gate_ledger_sha256, attacks=args.attacks_output)


if __name__ == '__main__':
    raise SystemExit(main())
