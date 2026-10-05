"""Portable PMIX Source/Evidence verifier; no subprocess, network or cleanup.

Expected source manifest, base, build commit and binary are independent inputs.
The gate ledger digest must also be frozen outside the resealable bundle.
This check is evidence validation, never independent stage review or V1 PASS.
"""
import argparse
import hashlib
import json
from pathlib import Path, PurePosixPath
import re
import stat
import zipfile

from run_pmix_gates import COMMANDS
from verify_pmix_native import verify as verify_native
from verify_pmix_workflow import parse_json
from pmix_capture_swift import SOURCE as CAPTURE_SOURCE
from run_pmix_capture_preflight import CASES as CAPTURE_CASES, validate_probe, validate_process, validate_toolchain
from verify_pmix_capture import mov_info
from verify_s5m2_evidence import load, require, safe

BASE = '34ccaa42f73e43f87714c954ccb090073e0ec1ed'
MATRIX = [('nav', 1), ('nav', 2), ('nav', 3), ('move', 1), ('move', 2),
          ('move', 3), ('points', 1), ('escape', 1), ('new-project', 1),
          ('workflow', 1), ('workflow-reopen', 1), ('workflow-cross-layer', 1)]


def sha(data):
    return hashlib.sha256(data).hexdigest()


def file_sha(path):
    digest=hashlib.sha256()
    with path.open('rb') as stream:
        for chunk in iter(lambda:stream.read(1024*1024),b''):digest.update(chunk)
    return digest.hexdigest()


def member(name):
    require(type(name) is str and name and '\\' not in name and ':' not in name,
            'unsafe package path')
    require(not PurePosixPath(name).is_absolute()
            and all(p not in ('', '.', '..') for p in name.split('/')),
            'unsafe package member')
    return name


def manifest(data):
    entries = {}
    for line in data.decode('utf-8').splitlines():
        digest, separator, name = line.partition('  ')
        require(separator and re.fullmatch('[0-9a-f]{64}', digest),
                'invalid source manifest row')
        member(name)
        require(name not in entries, 'duplicate source manifest member')
        entries[name] = digest
    require(entries, 'empty source manifest')
    return entries


def evidence_layout(root):
    require(root.is_dir() and not root.is_symlink(), 'evidence root directory')
    required = {'REVIEW.json', 'Source.zip', 'Evidence_MANIFEST.sha256', 'gates'}
    children = {p.name: p for p in root.iterdir()}
    require(required <= set(children) <= required | {'native'}, 'canonical evidence root layout')
    for name, path in children.items():
        require(not path.is_symlink() and (path.is_dir() if name in ('gates', 'native') else path.is_file()),
                'canonical evidence root entry type: '+name)
    for path in root.rglob('*'):
        require(not path.is_symlink() and (path.is_file() or path.is_dir()), 'unsafe evidence entry')


def gate_ledger(path, expected):
    require(type(expected) is str and re.fullmatch('[0-9a-f]{64}', expected), 'external gate ledger digest format')
    require(file_sha(path) == expected, 'external frozen gate ledger digest')
    return load(path)


def verify(root, *, expected_source, expected_commit, expected_binary, expected_gate_ledger,
           expected_base=BASE):
    root = Path(root)
    evidence_layout(root)
    require(re.fullmatch('[0-9a-f]{64}',expected_source) and re.fullmatch('[0-9a-f]{64}',expected_binary)
            and re.fullmatch('[0-9a-f]{40}(?:-dirty)?',expected_commit),'external identity format')
    inventory=manifest(safe(root,'Evidence_MANIFEST.sha256').read_bytes())
    paths={path.relative_to(root).as_posix() for path in root.rglob('*') if path.is_file()}
    require(paths==set(inventory)|{'Evidence_MANIFEST.sha256'},'whole evidence inventory missing/extra')
    for name,digest in inventory.items():require(file_sha(safe(root,name))==digest,'whole evidence digest '+name)
    review = load(root/'REVIEW.json')
    require(review['stage'] == 'S5-M2-C' and type(review['schema_version']) is int and review['schema_version'] == 2,
            'PMIX review schema/stage')
    require(review['accepted_base'] == expected_base == BASE,
            'accepted I1/I2 integration baseline')
    require(review['build_commit'] == expected_commit
            and review['source_manifest_sha256'] == expected_source,
            'external source/commit review binding')
    require(review['user_flicker_report'] == 'OPEN'
            and review['Windows'] == 'DEFERRED'
            and review['K1_native_remaining'] == 'DEFERRED'
            and review['whole_I2'] == 'NOT_ALL_PASS',
            'unearned platform/flicker/I2 closure')
    require(review['review_state'] in ('CANDIDATE_PENDING_INDEPENDENT_REVIEW',
                                     'CLEAN_FINAL_PENDING_INDEPENDENT_REVIEW'),
            'verifier cannot grant independent acceptance')
    source_path = safe(root, 'Source.zip')
    with zipfile.ZipFile(source_path) as source:
        infos = source.infolist()
        names = [i.filename for i in infos]
        require(names and len(names) == len(set(names)), 'duplicate source ZIP member')
        require(len(infos)<=2048 and sum(i.file_size for i in infos)<=128*1024*1024
                and all(i.file_size<=32*1024*1024 for i in infos),'source ZIP size/member budgets')
        for info in infos:
            member(info.filename)
            require(not info.is_dir() and not stat.S_ISLNK(info.external_attr >> 16),
                    'directory/symlink source ZIP member')
        raw_manifest = source.read('MANIFEST.sha256')
        require(sha(raw_manifest) == expected_source, 'external Source manifest')
        entries = manifest(raw_manifest)
        require(set(names) == set(entries) | {'MANIFEST.sha256', 'PACKAGE_INFO.json', 'PACKAGE_MANIFEST.sha256'},
                'source ZIP manifest coverage')
        for name, digest in entries.items():
            require(sha(source.read(name)) == digest, 'Source bytes mismatch: '+name)
        require(source.getinfo('PACKAGE_INFO.json').file_size<=1024*1024,'package metadata size budget')
        package = parse_json(source.read('PACKAGE_INFO.json'))
        require(type(package['schema_version']) is int and package['schema_version']==1
                and type(package['clean_worktree']) is bool and package['supplemental_only'] is False
                and package['stage']=='S5-M2-C','Source package schema/stage')
        require(package['commit'] == package['git_commit'] == expected_commit.removesuffix('-dirty')
                and package['clean_worktree'] == (not expected_commit.endswith('-dirty'))
                and package['source_manifest_sha256'] == expected_source,
                'Source package build identity')
        require(package['included_paths']==sorted(set(entries)|{'MANIFEST.sha256'})
                and type(package['source_file_count']) is int and package['source_file_count']==len(entries)
                and type(package['manifest_count']) is int and package['manifest_count']==len(entries)+1,'Source package count/path coverage')
        packaged_manifest = manifest(source.read('PACKAGE_MANIFEST.sha256'))
        require(set(packaged_manifest) == set(names)-{'PACKAGE_MANIFEST.sha256'},
                'source package manifest coverage')
        for name, digest in packaged_manifest.items():
            require(sha(source.read(name)) == digest, 'package member digest: '+name)
        # Bind executable offline verifier/gates/fixtures to the reviewed Source,
        # not a copied evidence manifest whose bytes can be resealed arbitrarily.
        source_root = Path(__file__).resolve().parents[1]
        for name in entries:
            if name.startswith(('scripts/', 'fixtures/synthetic/s5m2c/')):
                require(sha((source_root/name).read_bytes()) == entries[name],
                        'running source/fixture differs from package: '+name)
    records = gate_ledger(safe(root,'gates/gates.json'), expected_gate_ledger)
    preflight = load(safe(root,'gates/guard-preflight.json'))
    require(preflight['command']==['python3','-B','scripts/test_verify_pmix_guards.py']
            and type(preflight['exit_code']) is int and preflight['exit_code']==0
            and preflight['source_manifest_sha256']==expected_source
            and preflight['log']=='guard-preflight.log'
            and records[0]['guard_preflight_sha256']==preflight['sha256']==file_sha(safe(root,'gates/guard-preflight.log')),
            'mandatory guard preflight/raw log ledger binding')
    lifecycle = load(safe(root,'gates/capture-lifecycle-preflight.json'))
    require(lifecycle['command']==['python3','-B','scripts/test_pmix_capture_lifecycle.py']
            and type(lifecycle['exit_code']) is int and lifecycle['exit_code']==0
            and lifecycle['source_manifest_sha256']==expected_source
            and lifecycle['log']=='capture-lifecycle-preflight.log'
            and records[0]['capture_lifecycle_sha256']==lifecycle['sha256']==file_sha(safe(root,'gates/capture-lifecycle-preflight.log')),
            'mandatory lifecycle preflight/raw log ledger binding')
    writer = safe(root,'gates/capture-writer-preflight/RESULTS.json')
    require(file_sha(writer)==records[0]['capture_writer_results_sha256'], 'capture writer external ledger binding')
    capture = load(writer); capture_root=writer.parent
    require(capture['result']=='PASS' and capture['source_manifest_sha256']==expected_source
            and capture['scope']=='background initialization metadata and synthetic writer only; no stream capture or GUI',
            'capture writer preflight scope/source')
    require({p.name for p in capture_root.iterdir()}==set(capture['files'])|{'RESULTS.json'}, 'capture preflight file inventory')
    for name,digest in capture['files'].items():
        require('/' not in name and file_sha(safe(capture_root,name))==digest, 'capture preflight raw bytes: '+name)
    require((capture_root/'capture.swift').read_text()==CAPTURE_SOURCE
            and file_sha(capture_root/'capture.swift')==capture['producer_source_sha256']
            and file_sha(capture_root/'capture-producer')==capture['producer_sha256'], 'reviewed compiled capture producer identity')
    require(capture['background_writer_and_refusal_tests']=='PASS' and len(capture['probes'])==2, 'actual platform initialization preflight')
    for row,selection,expected_mode in zip(capture['probes'],('real-on-screen','own-window-none'),('--probe-initialization','--probe-no-window')):
        require(row['mode']==expected_mode and type(row['command']) is list and len(row['command'])==2
                and row['command'][1:]==[expected_mode] and Path(row['command'][0]).is_absolute()
                and Path(row['command'][0]).name=='capture-producer', 'initialization probe command')
        state=validate_probe(row['receipt'],selection)
        name=expected_mode[2:]
        validate_process(load(capture_root/(name+'.process.json')),load(capture_root/(name+'.launch.json')),
                         row['command'],row['exit_code'],row['receipt']['producer_pid'],15)
        require([parse_json(line.encode()) for line in (capture_root/(expected_mode[2:]+'.stdout')).read_text().splitlines()]==[row['receipt']]
                and not (capture_root/(expected_mode[2:]+'.stderr')).read_bytes()
                and type(row['exit_code']) is int and row['exit_code']==(0 if state=='INITIALIZATION_ONLY_PASS' else 2), 'initialization actual exit/raw receipt')
    require(capture['probes'][0]['receipt']['result']=='INITIALIZATION_ONLY_PASS'
            and capture['probes'][0]['receipt']['existing_screen_access'] is True
            and capture['probes'][1]['receipt']['result']=='BLOCKED', 'capture initialization constructor/access still blocked')
    compile_receipt=load(capture_root/'compile.json')
    validate_toolchain(capture_root,compile_receipt,capture['probes'][0]['command'][0])
    require([(row['mode'],row['exit_code'],row['samples']) for row in capture['cases']]==CAPTURE_CASES, 'actual writer lifecycle matrix')
    for row in capture['cases']:
        mode=row['mode']; raw=(capture_root/(mode+'.stdout')).read_text()
        validate_process(load(capture_root/(mode+'.process.json')),load(capture_root/(mode+'.launch.json')),
                         row['command'],row['exit_code'],row['producer_pid'],30)
        require(all(event.get('producer_pid',row['producer_pid'])==row['producer_pid'] for event in row['events']), 'actual writer child PID')
        require([parse_json(line.encode()) for line in raw.splitlines()]==row['events']
                and not (capture_root/(mode+'.stderr')).read_bytes(), 'actual writer raw events/stderr: '+mode)
        movie=capture_root/(mode+'.mov')
        if row['samples']:
            require(mov_info(movie)==row['movie'] and file_sha(movie)==row['movie_sha256']
                    and row['movie']['samples']==row['samples']
                    and [event['event'] for event in row['events']]==['ready','finished'], 'actual writer MOV lifecycle: '+mode)
        else:
            require(not movie.exists() and row['movie'] is None and row['movie_sha256'] is None
                    and any(event['event']=='failure' for event in row['events'])
                    and not any(event['event']=='finished' for event in row['events']), 'actual writer failure rejected: '+mode)
    fixtures=load(root/'gates/a2-fixture-inventory.json')
    require(fixtures=={'sha256':['8075367c8ae92db5fed8f7d1f0d0784db2adb84922a766aec404396f4a68b80f','68b9606ba4f52d2a599ac97527985128c7df8215e63c9c19617185d4489baeb9','5a3ca2155d6835592f5c5687bb4da682c6fb22b96c66994ca98b013face5c4fb','87f640be4096344ad749a237982d6f3e26778ffd0139b0dceab7457dde982f81'],'local_paths_copied':False},'original A2 authorized fixture identities/order')
    require([(row['id'], row['command']) for row in records] == COMMANDS,
            'full frozen gate command matrix')
    summary = load(root/'gates/summary.json')
    require(summary['stage'] == 'S5-M2-C' and summary['unchanged_source']
            and summary['unchanged_status'] and summary['gates_expected'] == len(COMMANDS)
            and summary['gates_passed'] == len(COMMANDS), 'incomplete/changed-source gates')
    require(summary['source_manifest_sha256'] == expected_source
            and summary['commit'] == expected_commit.removesuffix('-dirty')
            and summary['clean_worktree'] is (not expected_commit.endswith('-dirty'))
            and summary['result']==('CANDIDATE_GATES_PASS' if expected_commit.endswith('-dirty') else 'GATES_PASS'),
            'gate source/commit identity')
    require((root/'gates/source-before.sha256').read_bytes() == raw_manifest
            and (root/'gates/source-after.sha256').read_bytes() == raw_manifest,
            'gate source changed')
    for row in records:
        require(type(row['exit_code']) is int and row['exit_code'] == 0
                and row['source_manifest_sha256'] == expected_source
                and row['commit'] == expected_commit.removesuffix('-dirty'),
                'failed/wrong-source gate: '+row['id'])
        require(sha(safe(root, 'gates/'+row['log']).read_bytes()) == row['sha256'],
                'gate raw log hash: '+row['id'])
    binaries = load(root/'gates/binaries.json')
    require(set(binaries) == {'release', 'release-internal'}, 'public/internal release matrix')
    for kind, record in binaries.items():
        binary = safe(root, 'gates/'+record['path']).read_bytes()
        require(sha(binary) == record['sha256'] and record['commit'] == expected_commit
                and record['source_manifest_sha256'] == expected_source,
                'packaged binary/source identity')
        require(expected_commit.encode() in binary
                and all((marker in binary)==(kind=='release-internal') for marker in (b'RCAM_I1_NATIVE_DIR',b'RCAM_I2_B_NATIVE_DIR',b'RCAM_I2_C_NATIVE_DIR',b'RCAM_UI_ROI_DIR',b'RCAM_PMIX_NATIVE_DIR')),
                'public/internal PMIX control boundary')
    require(binaries['release-internal']['sha256'] == expected_binary,
            'external native binary identity')
    native_rows = review['native']
    require([(row['mode'], row['round']) for row in native_rows] == MATRIX,
            'three-round/complete native PMIX matrix')
    native_root = root/'native'
    require(native_root.is_dir() and not native_root.is_symlink(), 'native evidence root')
    require(all(p.is_dir() and not p.is_symlink() for p in native_root.iterdir()), 'native root entry type')
    expected_directories = {row['directory'] for row in native_rows}
    require(all(member(name).startswith('native/') and len(name.split('/')) == 2 for name in expected_directories),
            'native matrix direct-child layout')
    require(expected_directories == {p.relative_to(root).as_posix() for p in native_root.iterdir()},
            'native directory inventory missing/extra')
    seen_paths = set()
    seen_ids = set()
    results = []
    workflow = None
    for row in native_rows:
        name = member(row['directory'])
        require(name not in seen_paths, 'reused native directory')
        seen_paths.add(name)
        request_path = safe(root, name+'/request.json')
        directory = request_path.parent
        request = load(request_path)
        require(request['mode'] == row['mode'] and request['round'] == row['round'],
                'native request/matrix binding')
        require(type(request['run_id']) is str and request['run_id']
                and request['run_id'] not in seen_ids, 'reused native run ID')
        seen_ids.add(request['run_id'])
        results.append(verify_native(directory, source_manifest=expected_source,
                                     commit=expected_commit,
                                     binary_sha256=expected_binary,
                                     capture_producer_sha256=capture['producer_sha256']))
        if row['mode'] == 'workflow':
            workflow = sha(safe(directory, 'workflow-output.rcam').read_bytes())
        if row['mode'] == 'workflow-reopen':
            require(workflow == sha(safe(directory, 'reopen-input.rcam').read_bytes()),
                    'fresh process reopened a different project')
    return {'result': 'PMIX_EVIDENCE_VERIFIED_PENDING_INDEPENDENT_REVIEW',
            'gates': len(records), 'native_runs': len(results), 'runs': results,
            'source_manifest_sha256': expected_source, 'build_commit': expected_commit,
            'user_flicker_report': 'OPEN', 'stage_PASS_claim': False}


if __name__ == '__main__':
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('directory', type=Path)
    parser.add_argument('--source-manifest', required=True)
    parser.add_argument('--commit', required=True)
    parser.add_argument('--binary-sha256', required=True)
    parser.add_argument('--gate-ledger-sha256', required=True,
                        help='independently frozen SHA256 of exact gates/gates.json; do not derive from the bundle under test')
    args = parser.parse_args()
    try:
        print(json.dumps(verify(args.directory, expected_source=args.source_manifest,
                                expected_commit=args.commit,
                                expected_binary=args.binary_sha256,
                                expected_gate_ledger=args.gate_ledger_sha256), indent=2))
    except (OSError, ValueError, KeyError, TypeError, zipfile.BadZipFile) as error:
        print('FAIL:', error)
        raise SystemExit(1)
