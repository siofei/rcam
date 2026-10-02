"""Read-only A2 closeout verifier. Run from Source/scripts; paths are explicit.
Hashes bind evidence; native truth still requires independent inspection.
Exit: 0 verified, 1 mismatch/incomplete evidence, 2 command configuration error.
"""
import argparse
import hashlib
import json
import math
from pathlib import Path
import re
import sys
import package_source
import source_manifest

from run_s5m2_gates import COMMANDS
GATE_COMMANDS = dict(COMMANDS)
GATES = set(GATE_COMMANDS)
CASES = {'A', 'B', 'C', 'D', 'E-undo', 'E-redo'}
LARGE_REAL_SHA256 = '87f640be4096344ad749a237982d6f3e26778ffd0139b0dceab7457dde982f81'
VERSION = {'document_id', 'document_revision', 'workspace_revision', 'generation',
           'rule_revision', 'geometry_policy_hash'}

def require(condition, message):
    if not condition:
        raise ValueError(message)

def digest(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()

def load(path):
    def pairs(items):
        out = {}
        for k, v in items:
            require(k not in out, 'duplicate JSON key: '+k)
            out[k] = v
        return out
    value = json.loads(path.read_text(encoding='utf-8'), object_pairs_hook=pairs,
                       parse_constant=lambda x: (_ for _ in ()).throw(ValueError('invalid JSON number '+x)))
    def finite(v):
        if isinstance(v, float):
            require(math.isfinite(v), 'nonfinite numeric evidence')
        elif isinstance(v, dict):
            for child in v.values(): finite(child)
        elif isinstance(v, list):
            for child in v: finite(child)
    finite(value)
    return value

def safe(root, name):
    require(type(name) is str and name and not Path(name).is_absolute(), 'invalid evidence path')
    require('\\' not in name and all(p not in ('', '.', '..') for p in name.split('/')), 'unsafe evidence path')
    path = root
    for part in name.split('/'):
        path = path/part
        require(not path.is_symlink(), 'symlink evidence path')
    require(path.is_file() and path.resolve().is_relative_to(root.resolve()), 'missing evidence: '+name)
    return path

def integer(value, label):
    require(type(value) is int and value >= 0, 'invalid timestamp/count '+label)
    return value

def state_version(state, reference):
    # A2 operations never change rules or manufacturing precision. Bind the
    # four state dimensions and carry the other two from an accepted baseline.
    require(set(reference) == VERSION, 'incomplete reference TaskVersion')
    integer(reference['rule_revision'], 'rule revision')
    require(type(reference['geometry_policy_hash']) is str, 'invalid policy hash')
    return dict(reference, document_id=state['document_id'],
                document_revision=state['revision'],
                workspace_revision=state['workspace_revision'],
                generation=integer(state['generation'], 'generation'),
                geometry_policy_hash=reference['geometry_policy_hash'] if state['document_id'] is not None else '')


def verify_native(evidence, rows, commit, binary_sha, build_source='git-clean'):
    require(len(rows) == 18, 'three complete native rounds required')
    expected = [(r, c) for r in (1, 2, 3) for c in ('A', 'B', 'C', 'D', 'E-undo', 'E-redo')]
    require([(r['round'], r['case']) for r in rows] == expected, 'native order/matrix mismatch')
    previous_end = -1
    names = set()
    ids = set()
    for row in rows:
        name = row['observations']
        require(name not in names, 'reused native evidence'); names.add(name)
        path = safe(evidence, name)
        native = load(path)
        request = native['request']
        native_root = path.parent.relative_to(evidence).as_posix()
        prefix = native_root+'/' if native_root != '.' else ''
        require(load(safe(evidence, prefix+'request.json')) == request, 'independent request mismatch')
        require(request['round'] == row['round'] and request['case'] == row['case'], 'native request mismatch')
        require(request['run_id'] not in ids, 'duplicate native run'); ids.add(request['run_id'])
        require(native['schema_version'] == 2 and native['stage'] == 'S5-M2-A2' and native['status'] == 'OBSERVED', 'native schema mismatch')
        require(native['commit'] == commit and native['build_source'] == build_source and native['binary_sha256'] == binary_sha, 'native source/binary mismatch')
        require(native['profile'] == 'release' and 'Metal' in native['adapter'], 'native release/Metal required')
        require(native['failures'] == [], 'native failed or timed out')
        require(request['fixture_sha256'] == LARGE_REAL_SHA256, 'frozen REAL021 fixture mismatch')
        runner = load(path.parent/'runner.json')
        require(type(runner['exit_code']) is int and runner['exit_code'] == 0 and runner['binary_sha256'] == binary_sha and runner.get('error') is None, 'native process failed/identity mismatch')
        require(runner['fixture_sha256'] == request['fixture_sha256'], 'runner fixture mismatch')
        target = integer(native['target'], 'task')
        events = native['events']; frames = native['frames']
        require(bool(events) and bool(frames), 'missing raw events/frames')
        # Concurrent worker/UI log writers may interleave timestamps. Compare
        # causal events rather than requiring their collection order to be time order.
        def one(label):
            found = [e for e in events if e['task_id'] == target and e['event'] == label]
            require(len(found) == 1, 'missing/duplicate native '+label)
            integer(found[0]['at_ns'], label)
            return found[0]
        begin, finish, returned, reply = [one(k) for k in ('worker_begin', 'worker_finished', 'worker_returned', 'reply_received')]
        version = begin['data']['version']; receipt = finish['data']['receipt']
        require(set(version) == VERSION and set(receipt['result_version']) == VERSION, 'incomplete TaskVersion')
        require(receipt['input'] == version and receipt['task_id'] == target, 'worker receipt mismatch')
        before, after = native['before'], native['after']
        require(begin['data']['before'] == before, 'worker/UI baseline mismatch')
        require(version == state_version(before, version), 'input version/state mismatch')
        require(receipt['result_version'] == state_version(finish['data']['after'], version), 'result version/state mismatch')
        t0 = integer(native['input_ns'], 'input')
        baseline = integer(native['baseline_frame_ns'], 'baseline frame')
        require(previous_end < baseline < begin['at_ns'] <= t0 <= returned['at_ns'] <= reply['at_ns'], 'native causal clock order mismatch')
        require(begin['at_ns'] <= finish['at_ns'] <= returned['at_ns'], 'worker terminal order mismatch')
        previous_end = max(integer(f['at_ns'], 'frame') for f in frames)
        require(previous_end >= reply['at_ns'], 'final frame missing')
        def accepted_receipt(frame):
            accepted = frame['receipt']
            completed = [e for e in events if e['task_id'] == accepted['task_id'] and e['event'] == 'worker_finished']
            installed = [e for e in events if e['task_id'] == accepted['task_id'] and e['event'] == 'reply_received']
            require(len(completed) == len(installed) == 1 and completed[0]['data']['receipt'] == accepted
                    and installed[0]['data']['installed'] is True, 'frame receipt not independently accepted')
            require(completed[0]['at_ns'] <= installed[0]['at_ns'] <= frame['at_ns'], 'accepted frame receipt clock mismatch')
            return accepted
        baseline_frames = [f for f in frames if f['at_ns'] <= baseline]
        require(bool(baseline_frames), 'baseline state frame missing')
        baseline_state = max(baseline_frames, key=lambda f:f['at_ns'])
        accepted = accepted_receipt(baseline_state)
        require(baseline_state['state'] == before and accepted['result_version'] == version,
                'input version differs from accepted baseline')
        final_frame = max(frames, key=lambda f:f['at_ns'])
        final_receipt = accepted_receipt(final_frame)
        require(final_frame['state'] == after and final_receipt['result_version'] == state_version(after, version),
                'final accepted version/state mismatch')
        baseline_path = prefix+f'frame-{baseline}.ppm'
        safe(evidence, baseline_path)
        shots = [e for e in events if e['task_id'] == target and e['event'] == 'screenshot_received']
        require(len(shots) == 2, 'feedback/final surface readbacks missing or duplicate')
        shot = min(shots, key=lambda e:e['at_ns'])
        require(abs(native['feedback_upper_bound_ms']-(shot['at_ns']-t0)/1e6) < 1e-6, 'feedback clock/report mismatch')
        require(abs(native['worker_return_upper_bound_ms']-(returned['at_ns']-t0)/1e6) < 1e-6, 'worker clock/report mismatch')
        for screenshot in shots:
            require(screenshot['data']['focused'] is True, 'native window unfocused')
            image_path = safe(evidence, prefix+screenshot['data']['path'])
            require(image_path.read_bytes().startswith(b'P6\n'), 'invalid native surface readback')
        active = [f for f in frames if begin['at_ns'] <= f['at_ns'] <= previous_end]
        require(bool(active), 'no active UI observations')
        require(all(f['busy'] for f in active if f.get('pending') and f['pending']['id'] > target), 'stale reply cleared newer busy')
        case = row['case']
        require(all(f['state'] in (before, after) for f in active), 'partial UI publication')
        require(all(f['state'] == before for f in active if f['at_ns'] < reply['at_ns']), 'UI state published before reply')
        if case in ('A', 'B'):
            input_event = one('native_cancel_input')
            button = one('cancel_button')
            require(input_event['data']['phase'] == case, 'cancel input phase mismatch')
            require(t0 <= input_event['at_ns'] <= button['at_ns'] <= finish['at_ns'], 'cancel input/button/terminal order mismatch')
            require(reply['data']['installed'] is True and final_receipt == receipt, 'cancel/commit receipt not installed')
            feedback_after = button['at_ns']
        else:
            input_event = one('native_transition_input')
            require(input_event['data']['kind'] == case, 'transition input kind mismatch')
            require(t0 <= input_event['at_ns'], 'transition input clock mismatch')
            require(final_receipt['task_id'] > target, 'replacement receipt missing')
            feedback_after = input_event['at_ns']
        require(all(feedback_after <= integer(e['at_ns'], 'screenshot') <= previous_end for e in shots),
                'screenshot/input/final frame order mismatch')
        require(max(e['at_ns'] for e in shots) >= reply['at_ns'], 'final readback precedes reply')
        if case == 'A':
            require(len(before['selection']) > 0, 'nonempty selection baseline required')
            require(one('cancel_button')['data']['outcome'] == 'Requested', 'running cancel not observed')
            require(receipt['state'] == 'cancelled' and finish['data']['error'] == 'CANCELLED', 'wrong cancel terminal')
            require(before == after == finish['data']['after'], 'cancel changed content/history/selection')
            require(all(f['state'] == before for f in active), 'partial cancelled UI publication')
            require(all(f['has_last_good'] for f in active), 'old scene lost during cancellation')
            require(not any(e['event'].endswith('_barrier') and e['task_id'] == target for e in events), 'artificial wait cannot prove cancel SLA')
            require(shot['at_ns']-t0 <= 500_000_000 and returned['at_ns']-t0 <= 2_000_000_000, 'cancel SLA exceeded')
        elif case == 'B':
            require(one('commit_barrier')['at_ns'] <= t0, 'no actual Committing observation')
            require(one('cancel_button')['data']['outcome'] == 'TooLate', 'TooLate not observed')
            require(receipt['state'] == 'completed' and finish['data']['error'] is None, 'commit misreported/failed')
            require(after['undo_entries'] == before['undo_entries']+1 and after['layers'] == before['layers']+1 and after['objects'] == before['objects'], 'atomic commit/Undo mismatch')
            require(after == finish['data']['after'], 'commit UI result mismatch')
        else:
            require(reply['data']['installed'] is False, 'stale reply installed')
            require(finish['data']['after'] == before, 'old read changed manufacturing/selection')
            if case == 'C':
                closed = dict(before, document_id=None, revision=None, workspace_revision=None,
                              generation=before['generation']+1, content_sha256=None, dirty=None,
                              project_dirty=None, layers=0, objects=0, selection=[], undo_entries=None, redo_entries=None)
                require(after == closed, 'close retained document content/history/selection')
            if case == 'D': require(after['document_id'] != before['document_id'] and after['content_sha256'] == before['content_sha256'], 'project switch incorrect')
            if case.startswith('E-'):
                require(int(after['revision']) > int(before['revision']), 'history revision did not advance')
                delta = -1 if case == 'E-undo' else 1
                require(after['undo_entries'] == before['undo_entries']+delta and after['redo_entries'] == before['redo_entries']-delta, 'Undo/Redo incorrect')
            # A serial owner completes replacement after rejecting the old reply.
            # Every observed state must be the complete old or complete new state.
            require(all(f['state'] in (before, after) for f in active), 'partial/stale UI publication')
        if case != 'A':
            release = one('barrier_released')
            require(release['data']['released'] is True and 0 <= release['data']['elapsed_ms'] < 5000, 'test synchronization timed out')
            if case == 'B':
                barrier = one('commit_barrier')
                require(begin['at_ns'] <= barrier['at_ns'] <= t0 <= input_event['at_ns'] <= button['at_ns']
                        <= release['at_ns'] <= finish['at_ns'], 'commit barrier causal order mismatch')
            else:
                barrier = one('delivery_barrier')
                require(finish['at_ns'] <= barrier['at_ns'] <= t0 <= input_event['at_ns']
                        <= release['at_ns'] <= returned['at_ns'], 'delivery barrier causal order mismatch')

def verify(source, evidence):
    source_manifest.ROOT = source
    load(source/'PACKAGE_INFO.json')  # reject duplicate keys/nonfinite values first
    package = package_source.archive_identity(source)
    source_entries = package_source.checked_manifest(source, 'PACKAGE_MANIFEST.sha256')
    source_actual = {p.relative_to(source).as_posix() for p in source.rglob('*') if p.is_file() and p != source/'PACKAGE_MANIFEST.sha256'}
    require(set(source_entries) == source_actual, 'source package has unmanifested files')
    commit = package['git_commit']
    entries = package_source.checked_manifest(evidence, 'EVIDENCE_MANIFEST.sha256')
    actual = {p.relative_to(evidence).as_posix() for p in evidence.rglob('*') if p.is_file() and p != evidence/'EVIDENCE_MANIFEST.sha256'}
    require(set(entries) == actual, 'evidence manifest coverage mismatch')
    report = load(safe(evidence, 'CLOSEOUT.json'))
    require(report['schema_version'] == 2 and report['stage'] == 'S5-M2-A2', 'wrong schema/stage')
    require(report['commit'] == commit and report['clean_worktree'] is True, 'commit/clean identity mismatch')
    require(report['source_manifest_sha256'] == digest(source/'MANIFEST.sha256'), 'source identity mismatch')
    environment = load(safe(evidence, report['environment']))
    require(all(environment.get(k) for k in ('macos', 'cpu', 'ram', 'gpu', 'displays')), 'native environment missing')
    require(all(d.get('pixels') and d.get('resolution_refresh') for d in environment['displays']), 'native display information missing')
    binaries = report['binaries']
    require(set(binaries) == {'release', 'internal-evidence'}, 'binary inventory incomplete')
    for kind, build in binaries.items():
        require(build['commit'] == commit and build['profile'] == 'release', 'binary build identity mismatch')
        require(build['source_manifest_sha256'] == report['source_manifest_sha256'], 'binary source mismatch')
        binary_path = safe(evidence, build['path'])
        require(digest(binary_path) == build['sha256'], 'release binary hash mismatch')
        binary_bytes = binary_path.read_bytes()
        require((b'RCAM_A2_NATIVE_DIR' in binary_bytes) == (kind == 'internal-evidence'), 'public/internal driver boundary mismatch')
        require(commit.encode() in binary_bytes and (commit+'-dirty').encode() not in binary_bytes, 'embedded binary commit mismatch/dirty')
        identity = load(safe(evidence, build['observed_identity']))
        require(identity['commit'] == commit and identity['sha256'] == build['sha256'] and identity['source'] == 'git-clean', 'observed binary identity mismatch')
    gates = report['gates']
    require(len(gates) == len(GATES) and {g['id'] for g in gates} == GATES, 'missing/duplicate gates')
    for gate in gates:
        require(gate['exit_code'] == 0 and type(gate['exit_code']) is int, 'gate failed')
        require(gate['commit'] == commit and gate['source_manifest_sha256'] == report['source_manifest_sha256'], 'gate/source mismatch')
        require(gate['command'] == GATE_COMMANDS[gate['id']], 'gate command mismatch')
        require(digest(safe(evidence, gate['log'])) == gate['sha256'], 'gate log mismatch')
    verify_native(evidence, report['native_runs'], commit, binaries['internal-evidence']['sha256'])
    return ['Source manifest', 'Evidence manifest', 'Commit identity', 'Test logs', 'Release identity', 'Native matrix']

def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--source', required=True, type=Path)
    parser.add_argument('--evidence', required=True, type=Path)
    args = parser.parse_args()
    if not args.source.is_dir() or not args.evidence.is_dir(): parser.error('source and evidence must exist')
    try:
        for label in verify(args.source.resolve(), args.evidence.resolve()): print(label+': PASS')
    except (OSError, ValueError, KeyError, TypeError, OverflowError) as error:
        print('FAIL: '+str(error), file=sys.stderr); return 1
    return 0
if __name__ == '__main__': sys.exit(main())
