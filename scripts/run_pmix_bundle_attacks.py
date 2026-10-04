"""Actual resealed PMIX bundle attacks. Never mock the trusted verifier.

Plan-only is preparation, not execution. Real execution requires the original
complete authentic bundle to pass before and after every isolated attack. Each
case retains its own files/results. Hardlinks are optional for immutable inputs;
all replacements use a fresh inode, so original evidence cannot be truncated.
"""
import argparse
import copy
import hashlib
import importlib
import json
import os
from pathlib import Path
import shutil
import tempfile
import zipfile

CASES = [
    'inventory-extra', 'inventory-missing-log', 'source-extra-member',
    'source-duplicate-member', 'source-traversal', 'source-symlink',
    'source-schema-float', 'source-clean-int', 'source-count',
    'source-commit', 'source-duplicate-json-key', 'gate-matrix',
    'gate-failed-exit', 'gate-summary-count', 'gate-command',
    'gate-log-resealed', 'fixture-identity', 'binary-resealed',
    'review-wrong-base', 'review-unsupported-pass', 'native-matrix',
    'native-reused-directory', 'native-reused-runid', 'roi-parent-path',
    'typed-generation-bool', 'typed-policy-all-producers',
    'typed-rule-all-producers', 'accepted-enqueue-time',
    'worker-terminal-state', 'worker-error', 'geometry-coordinate',
    'geometry-aperture-hole', 'snapshot-undo-count', 'stale-scene',
    'zero-trajectory', 'frame-first-renumber', 'frame-middle-renumber',
    'frame-last-renumber', 'frame-chunk-renumber', 'missing-stdout',
    'video-failed-exit', 'video-wrong-window', 'video-truncated',
    'export-coordinate', 'export-local-hole', 'project-style-resealed',
    'cross-layer-selected-order', 'roi-foreground-resealed',
]
VERSION_KEYS = {'document_id', 'document_revision', 'workspace_revision',
                'generation', 'rule_revision', 'geometry_policy_hash'}


def sha(path):
    h = hashlib.sha256()
    with path.open('rb') as stream:
        for block in iter(lambda: stream.read(1024*1024), b''): h.update(block)
    return h.hexdigest()


def atomic(path, data):
    fd, name = tempfile.mkstemp(prefix='.attack-replace-', dir=path.parent)
    with os.fdopen(fd, 'wb') as stream: stream.write(data)
    os.replace(name, path)


def read(path): return json.loads(path.read_text())
def write(path, value): atomic(path, (json.dumps(value, ensure_ascii=False, indent=2)+'\n').encode())


def seal(root):
    for path in root.rglob('file-hashes.json'):
        local = path.parent
        write(path, {f.relative_to(local).as_posix(): sha(f)
                     for f in sorted(local.rglob('*')) if f.is_file() and f != path})
    path = root/'Evidence_MANIFEST.sha256'
    atomic(path, ''.join(f'{sha(f)}  {f.relative_to(root).as_posix()}\n'
                        for f in sorted(root.rglob('*')) if f.is_file() and f != path).encode())


def zip_replace(path, entries):
    fd, temp = tempfile.mkstemp(prefix='.attack-zip-', dir=path.parent)
    os.close(fd)
    with zipfile.ZipFile(temp, 'w', compression=zipfile.ZIP_DEFLATED) as z:
        for info, data in entries: z.writestr(info, data)
    os.replace(temp, path)


def rewrite_versions(value, field, replacement):
    if type(value) is dict:
        if set(value) == VERSION_KEYS and (field != 'geometry_policy_hash' or value['document_id'] is not None):
            value[field] = replacement
        for child in value.values(): rewrite_versions(child, field, replacement)
    elif type(value) is list:
        for child in value: rewrite_versions(child, field, replacement)


def jsonl(path): return [json.loads(line) for line in path.read_text().splitlines()]
def write_jsonl(path, rows): atomic(path, ''.join(json.dumps(row)+'\n' for row in rows).encode())


def native(root, mode='nav', round_number=1):
    row = next(row for row in read(root/'REVIEW.json')['native']
               if row['mode'] == mode and row['round'] == round_number)
    return root/row['directory']


def observation_change(root, change, mode='nav', round_number=1):
    directory = native(root, mode, round_number)
    path = directory/'observations.json'; value = read(path)
    change(value, directory)
    write(path, value)


def frame_deletion(value, directory, which):
    roi = directory/read(directory/'runner.json')['ui_roi_directory']
    rows = jsonl(roi/'frames.jsonl'); paints = jsonl(roi/'paint.jsonl'); samples = jsonl(roi/'samples.jsonl')
    protected = {e['frame_id'] for e in value['events']} | {s['frame_id'] for s in value['snapshots']} | {q['frame_id'] for q in value['requests']}
    eligible = [f['id'] for f in value['frames'] if f['phase'] == 20 and f['id'] not in protected]
    if not eligible: raise ValueError('authentic navigation lacks deletable recorded phase20 frame')
    anchor = eligible[0] if which == 'first' else eligible[-1] if which == 'last' else eligible[len(eligible)//2]
    removed = {anchor}
    if which == 'chunk':
        removed = set(n for n in range(anchor, anchor+8) if n in eligible)
        if len(removed) < 3: raise ValueError('authentic navigation lacks eligible recorded chunk')
    def remap(n): return n-sum(k < n for k in removed)
    def mapping(v):
        if type(v) is dict:
            for key in ('frame', 'frame_id', 'visible_frame_id', 'callback_after_ui_frame'):
                if type(v.get(key)) is int: v[key] = remap(v[key])
            for child in v.values(): mapping(child)
        elif type(v) is list:
            for child in v: mapping(child)
    value['frames'] = [f for f in value['frames'] if f['id'] not in removed]
    for f in value['frames']: f['id'] = remap(f['id'])
    for a, b in zip(value['frames'], value['frames'][1:]): b['frame_interval_ms'] = (b['input_ns']-a['input_ns'])/1e6
    for key in ('events', 'snapshots', 'requests'): mapping(value[key])
    value['last_observed_frame_id'] -= len(removed)
    rows = [row for row in rows if row['frame'] not in removed]
    paints = [row for row in paints if row['frame'] not in removed]
    samples = [row for row in samples if row['request']['frame'] not in removed and row['callback_after_ui_frame'] not in removed]
    for dataset in (rows, paints, samples): mapping(dataset)
    for number, sample in enumerate(samples): sample['sample'] = number
    write_jsonl(roi/'frames.jsonl', rows); write_jsonl(roi/'paint.jsonl', paints); write_jsonl(roi/'samples.jsonl', samples)


def mutate(root, case):
    if case.startswith('source-'):
        path = root/'Source.zip'
        with zipfile.ZipFile(path) as z: entries = [(copy.copy(i), z.read(i.filename)) for i in z.infolist()]
        if case in ('source-extra-member', 'source-duplicate-member', 'source-traversal', 'source-symlink'):
            name = {'source-extra-member': 'unexpected.txt', 'source-duplicate-member': 'PACKAGE_INFO.json', 'source-traversal': '../escape.txt', 'source-symlink': 'linked.txt'}[case]
            info = zipfile.ZipInfo(name); info.create_system = 3; info.external_attr = (0o120777 if case == 'source-symlink' else 0o100644) << 16
            entries.append((info, b'../outside'))
        else:
            for n, (info, data) in enumerate(entries):
                if info.filename != 'PACKAGE_INFO.json': continue
                meta = json.loads(data)
                key, replacement = {'source-schema-float': ('schema_version', 1.0), 'source-clean-int': ('clean_worktree', 0), 'source-count': ('source_file_count', meta['source_file_count']-1), 'source-commit': ('commit', '0'*40), 'source-duplicate-json-key': ('schema_version', 1)}[case]
                meta[key] = replacement; raw = (json.dumps(meta)+'\n').encode()
                if case == 'source-duplicate-json-key': raw = raw.replace(b'{', b'{"schema_version":1,', 1)
                entries[n] = (info, raw)
            payloads = {info.filename: data for info, data in entries}
            for n, (info, data) in enumerate(entries):
                if info.filename == 'PACKAGE_MANIFEST.sha256': entries[n] = (info, ''.join(f'{hashlib.sha256(v).hexdigest()}  {k}\n' for k, v in sorted(payloads.items()) if k != info.filename).encode())
        zip_replace(path, entries); return
    if case.startswith('gate-'):
        path = root/'gates/gates.json'; rows = read(path)
        if case == 'gate-matrix': rows[0], rows[1] = rows[1], rows[0]
        elif case == 'gate-failed-exit': rows[0]['exit_code'] = 101
        elif case == 'gate-command': rows[0]['command'].append('--ignored')
        elif case == 'gate-log-resealed':
            atomic(root/'gates'/rows[0]['log'], b'fabricated passing log\n'); rows[0]['sha256'] = sha(root/'gates'/rows[0]['log'])
        elif case == 'gate-summary-count':
            summary = read(root/'gates/summary.json'); summary['gates_passed'] -= 1; write(root/'gates/summary.json', summary)
        write(path, rows); return
    if case == 'fixture-identity':
        path = root/'gates/a2-fixture-inventory.json'; value = read(path); value['sha256'][0] = '0'*64; write(path, value); return
    if case == 'binary-resealed':
        path = root/'gates/binaries.json'; value = read(path); record = value['release-internal']; binary = root/'gates'/record['path']; atomic(binary, binary.read_bytes()+b'changed'); record['sha256'] = sha(binary); write(path, value); return
    if case in ('review-wrong-base', 'review-unsupported-pass', 'native-matrix', 'native-reused-directory'):
        path = root/'REVIEW.json'; value = read(path)
        if case == 'review-wrong-base': value['accepted_base'] = '0'*40
        elif case == 'review-unsupported-pass': value['Windows'] = 'PASS'
        elif case == 'native-matrix': value['native'].pop()
        else: value['native'][1]['directory'] = value['native'][0]['directory']
        write(path, value); return
    if case == 'inventory-extra': atomic(root/'unexpected.txt', b'extra'); return
    if case in ('inventory-missing-log', 'missing-stdout'):
        (native(root)/'stdout.log').unlink(); return
    if case == 'native-reused-runid':
        directory = native(root, round_number=2); value = read(directory/'request.json'); value['run_id'] = read(native(root)/'request.json')['run_id']; write(directory/'request.json', value)
        r = read(directory/'observations.json'); r['request'] = value; write(directory/'observations.json', r); return
    if case == 'roi-parent-path':
        path = native(root)/'runner.json'; value = read(path); value['ui_roi_directory'] = '../outside'; write(path, value); return
    if case in ('typed-policy-all-producers', 'typed-rule-all-producers'):
        directory = native(root); field = 'geometry_policy_hash' if case.startswith('typed-policy') else 'rule_revision'; replacement = '0'*64 if field == 'geometry_policy_hash' else 1
        path = directory/'observations.json'; value = read(path); rewrite_versions(value, field, replacement); write(path, value)
        roi = directory/read(directory/'runner.json')['ui_roi_directory']
        for name in ('frames.jsonl', 'samples.jsonl'):
            dataset = jsonl(roi/name); rewrite_versions(dataset, field, replacement); write_jsonl(roi/name, dataset)
        return
    if case.startswith('video-'):
        directory = native(root)
        if case == 'video-truncated': atomic(directory/'native-window.mov', b'not-video'*32)
        else:
            value = read(directory/'video-command.json')
            if case == 'video-failed-exit': value['exit_code'] = 17
            else: value['command'][value['command'].index('--owned-window')+2] = '99999999'
            write(directory/'video-command.json', value)
        return
    if case.startswith('export-'):
        path = native(root, 'workflow')/'workflow-layer-1.gbr'; raw = path.read_text()
        if case == 'export-local-hole': raw = raw.replace('%ADD17C,3X1*%', '%ADD17C,3X0.8*%', 1)
        else:
            import re
            match = re.search(r'X(-?\d+)Y(-?\d+)D03\*', raw); raw = raw[:match.start()]+f'X{int(match[1])+100}Y{match[2]}D03*'+raw[match.end():]
        atomic(path, raw.encode()); return
    if case == 'project-style-resealed':
        path = native(root, 'workflow')/'workflow-output.rcam'
        with zipfile.ZipFile(path) as z: entries = [(copy.copy(i), z.read(i.filename)) for i in z.infolist()]
        payloads = {info.filename: data for info, data in entries}; layer = json.loads(payloads['layers/layer-1.json']); layer['workspace']['style']['base_color'] = '#000000'; payloads['layers/layer-1.json'] = (json.dumps(layer)+'\n').encode(); meta = json.loads(payloads['manifest.json'])
        for row in meta['entries']: row.update(sha256=hashlib.sha256(payloads[row['path']]).hexdigest(), uncompressed_size=len(payloads[row['path']]))
        payloads['manifest.json'] = (json.dumps(meta)+'\n').encode(); zip_replace(path, [(info, payloads[info.filename]) for info, data in entries]); return
    if case == 'roi-foreground-resealed':
        directory = native(root); roi = directory/read(directory/'runner.json')['ui_roi_directory']; rows = jsonl(roi/'samples.jsonl')
        eligible = next(row for row in rows if row['request']['frame'] > 100); crop = eligible['crops'][0]; path = roi/crop['path']; data = path.read_bytes(); header, dim, maximum, pixels = data.split(b'\n', 3); pixels = bytearray(pixels); pixels[:3] = b'\0\0\0' if min(pixels[:3]) >= 140 else b'\xff\xff\xff'; atomic(path, header+b'\n'+dim+b'\n'+maximum+b'\n'+pixels); crop['sha256'] = sha(path); write_jsonl(roi/'samples.jsonl', rows); return
    def change(value, directory):
        if case == 'typed-generation-bool': value['worker'][0]['receipt']['input']['generation'] = False
        elif case == 'accepted-enqueue-time': value['requests'][0]['at_ns'] = value['worker'][0]['started_ns']+1
        elif case == 'worker-terminal-state': value['worker'][0]['receipt']['state'] = 'cancelled'
        elif case == 'worker-error': value['worker'][0]['error'] = {'code': 'IO_ERROR'}
        elif case == 'snapshot-undo-count': value['snapshots'][0]['state']['undo'] += 1
        elif case == 'stale-scene': next(f for f in value['frames'] if f['painted'])['paint']['scene_serial'] = 'stale'
        elif case == 'zero-trajectory': next(f for f in value['frames'] if f['phase'] == 20)['injected']['view']['scale'] = 0.
        elif case.startswith('frame-'): frame_deletion(value, directory, case.split('-')[1])
        elif case == 'cross-layer-selected-order': next(s for s in value['snapshots'] if s['label'] == 'workflow-step-00')['selected_ids'].reverse()
        elif case in ('geometry-coordinate', 'geometry-aperture-hole'):
            record = value['snapshots'][0]; path = directory/record['path']; snapshot = read(path)
            if case == 'geometry-coordinate': snapshot['layers'][0]['objects'][0]['geometry']['Flash']['center']['x_mm'] += 1.
            else: snapshot['apertures'][0]['shape']['Circle']['hole_diameter_mm'] = .2
            write(path, snapshot); record['sha256'] = sha(path)
        else: raise ValueError('unimplemented attack '+case)
    observation_change(root, change, 'workflow-cross-layer' if case == 'cross-layer-selected-order' else 'nav')


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--source-root', required=True, type=Path)
    parser.add_argument('--source-manifest', required=True)
    parser.add_argument('--commit', required=True)
    parser.add_argument('--binary-sha256', required=True)
    parser.add_argument('--gate-ledger-sha256', required=True, help='frozen digest outside tested bundle')
    parser.add_argument('--baseline', type=Path)
    parser.add_argument('--out', required=True, type=Path)
    parser.add_argument('--plan-only', action='store_true')
    parser.add_argument('--hardlink-immutable', action='store_true')
    args = parser.parse_args()
    if sha(args.source_root/'MANIFEST.sha256') != args.source_manifest: parser.error('trusted source manifest identity')
    args.out.mkdir(parents=True, exist_ok=False)
    if args.plan_only:
        write(args.out/'PLAN.json', {'cases': CASES, 'count': len(CASES), 'status': 'PREPARED_NOT_EXECUTED', 'authentic_full_bundle_required': True, 'external_gate_ledger_sha256': args.gate_ledger_sha256, 'stage_PASS_claim': False})
        return 0
    if args.baseline is None: parser.error('--baseline required for real execution')
    import sys
    sys.path.insert(0, str(args.source_root/'scripts'))
    verifier = importlib.import_module('verify_pmix_evidence').verify
    config = dict(expected_source=args.source_manifest, expected_commit=args.commit, expected_binary=args.binary_sha256, expected_gate_ledger=args.gate_ledger_sha256)
    before = sha(args.baseline/'Evidence_MANIFEST.sha256')
    positive = verifier(args.baseline, **config)
    write(args.out/'BASELINE_POSITIVE.json', positive)
    records = []
    for case in CASES:
        verifier(args.baseline, **config)
        root = args.out/case
        shutil.copytree(args.baseline, root, copy_function=os.link if args.hardlink_immutable else shutil.copy2)
        mutate(root, case); seal(root)
        error = None
        try: verifier(root, **config)
        except (OSError, ValueError, KeyError, TypeError, zipfile.BadZipFile) as failure: error = str(failure)
        record = {'id': case, 'result': 'REJECTED' if error else 'UNEXPECTED_ACCEPT', 'reason': error, 'directory': case, 'resealed': True}
        records.append(record); write(args.out/'RESULTS.json', records)
        verifier(args.baseline, **config)
        if before != sha(args.baseline/'Evidence_MANIFEST.sha256'): raise ValueError('original baseline manifest changed')
        if error is None: raise ValueError('attack accepted; stop and preserve: '+case)
    write(args.out/'SUMMARY.json', {'result': 'ALL_REAL_RESEALED_ATTACKS_REJECTED', 'cases': len(records), 'authentic_baseline_reverified_each_time': True, 'source_manifest_sha256': args.source_manifest, 'build_commit': args.commit, 'stage_PASS_claim': False})
    return 0


if __name__ == '__main__': raise SystemExit(main())
