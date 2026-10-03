"""Portable S5-I2-A candidate/clean-final integrity check; never I2/native acceptance.
Usage: python3 verify_i2_a_evidence.py BUNDLE_DIRECTORY
No shell/Cargo execution, network, absolute-path lookup or directory cleanup.
"""
import argparse
import hashlib
import io
import json
from pathlib import Path, PurePosixPath
import re
import stat
import zipfile

EXPECTED_COMMANDS_SHA256 = "32aa7a6aa1008af460ecf7e2b781671cf36ac435a7996ff530b709b1c7fd1177"
GATES = ('fmt', 'check', 'clippy', 'clippy-internal', 'workspace-test',
         'automation-contract', 'headless-workflow', 'verifier-unit', 'service-boundary',
         'i2-core-release', 'i2-budget-unit-release', 'numeric-oracles', 'i2-service-release', 'i2-app-release',
         'i1-release-regression', 'batch-release-regression',
         'metal-reference', 'metal-batch', 'metal-block',
         'release', 'release-internal', 'source-manifest')
REMAINING = ('independent_review', 'B_tool_adapters', 'C_interaction_UX',
             'three_native_I2_rounds', 'menu_flicker_actual_reproduction',
             'stage_commit_and_clean_same_commit_closeout')
FINAL_REMAINING = ('independent_final_A_review', 'B_tool_adapters', 'C_interaction_UX',
                   'three_native_I2_rounds', 'menu_flicker_actual_reproduction', 'Windows_deferred')
PACKAGING_ONLY = {'scripts/verify_i2_a_evidence.py', 'scripts/test_verify_i2_a_evidence.py',
                  'docs/S5_I2_A_CANDIDATE_REVIEW.md', 'docs/S5_I2_A_REVIEW.md'}


def require(condition, message):
    if not condition:
        raise ValueError(message)


def sha(data):
    return hashlib.sha256(data).hexdigest()


def safe(name):
    require(isinstance(name, str) and name and '\\' not in name and ':' not in name,
            'unsafe path')
    p = PurePosixPath(name)
    require(not p.is_absolute() and all(x not in ('.', '..', '') for x in name.split('/')),
            'unsafe path')
    return name


def manifest(data):
    entries = {}
    for line in data.decode('utf-8').splitlines():
        digest, sep, name = line.partition('  ')
        require(sep and re.fullmatch('[0-9a-f]{64}', digest), 'bad digest line')
        safe(name)
        require(name not in entries, 'duplicate manifest path')
        entries[name] = digest
    require(entries, 'empty manifest')
    return entries


def read(root, name):
    p = root / safe(name)
    for ancestor in (p, *p.parents):
        require(not ancestor.is_symlink(), 'symlink evidence path')
        if ancestor == root:
            break
    require(p.is_file() and p.stat().st_size <= 128 * 1024 * 1024, 'missing/oversize evidence')
    return p.read_bytes()


def verify(root, expected_manifest=None, expected_commit=None, expected_precommit_manifest=None):
    root = Path(root).absolute()
    entries = manifest(read(root, 'BUNDLE.sha256'))
    actual = set()
    for p in root.rglob('*'):
        require(not p.is_symlink(), 'symlink bundle')
        if p.is_file() and p != root/'BUNDLE.sha256':
            actual.add(p.relative_to(root).as_posix())
    require(actual == set(entries), 'bundle file set mismatch')
    for name, digest in entries.items():
        require(sha(read(root, name)) == digest, 'bundle digest mismatch: ' + name)
    review = json.loads(read(root, 'REVIEW.json'))
    require(type(review['schema_version']) is int and review['schema_version'] == 2,
            'schema version')
    if expected_manifest is not None:
        require(isinstance(expected_manifest,str) and re.fullmatch('[0-9a-f]{64}',expected_manifest)
                and review['source_manifest_sha256']==expected_manifest, 'external source reference mismatch')
    final=review.get('artifact_mode','candidate')=='clean_final_review'
    require(review.get('artifact_mode','candidate') in ('candidate','clean_final_review'), 'artifact mode')
    acceptance='A_FINAL_REVIEW_PENDING' if final else 'NOT_ACCEPTED'
    require(review['scope'] == 'S5-I2-A' and review['acceptance'] == acceptance,
            'foundation cannot attest whole I2 acceptance')
    remaining=FINAL_REMAINING if final else REMAINING
    require(tuple(review['required_remaining']) == remaining, 'remaining scope omitted')
    require(re.fullmatch('[0-9a-f]{40}', review['base_commit']), 'commit identity')
    if final:
        require(expected_manifest is not None and isinstance(expected_commit,str)
                and re.fullmatch('[0-9a-f]{40}',expected_commit) and expected_commit==review['base_commit'],
                'clean final requires external commit reference')
        require(isinstance(expected_precommit_manifest,str) and re.fullmatch('[0-9a-f]{64}',expected_precommit_manifest),
                'clean final requires external approved source reference')
        approved=review['precommit_A_review']
        require(approved['result']=='PRECOMMIT_A_PASS_MAC_FIRST_BOUNDED'
                and approved['source_manifest_sha256']==expected_precommit_manifest,
                'precommit A approval identity')
        require(sha(read(root,approved['report_path']))==approved['report_sha256'], 'precommit review digest')
        require(review['whole_I2_acceptance']=='NOT_ACCEPTED', 'A cannot attest whole I2')
    source = read(root, 'Source.zip')
    require(sha(source) == review['source_zip_sha256'], 'source ZIP identity')
    with zipfile.ZipFile(io.BytesIO(source)) as z:
        names = z.namelist()
        require(len(names) == len(set(names)), 'duplicate source member')
        require(sum(i.file_size for i in z.infolist()) <= 128 * 1024 * 1024,
                'oversize source ZIP')
        for i in z.infolist():
            safe(i.filename)
            require(not i.is_dir() and not stat.S_ISLNK(i.external_attr >> 16),
                    'source ZIP symlink/directory')
        package = manifest(z.read('PACKAGE_MANIFEST.sha256'))
        require(set(names) == set(package) | {'PACKAGE_MANIFEST.sha256'}, 'source ZIP file set')
        for name, digest in package.items():
            require(sha(z.read(name)) == digest, 'source member hash')
        source_manifest = z.read('MANIFEST.sha256')
        source_hash = sha(source_manifest)
        require(source_hash == review['source_manifest_sha256'], 'source manifest identity')
        files = manifest(source_manifest)
        for name, digest in files.items():
            require(name in package and sha(z.read(name)) == digest, 'source file hash')
        info = json.loads(z.read('PACKAGE_INFO.json'))
        require(info['git_commit'] == review['base_commit'] and info['commit'] == review['base_commit'],
                'package commit')
        require(info['stage'] == ('S5-I2-A clean final' if final else 'S5-I2-A review candidate')
                and info['clean_worktree'] is final,
                'candidate source misrepresented as clean closeout')
        require(type(info['source_file_count']) is int and info['source_file_count'] == len(files),
                'source file count')
        require(info['source_manifest_sha256'] == source_hash, 'package source identity')
    if final:
        approved_raw=read(root,'independent-precommit/source-manifest.sha256')
        require(sha(approved_raw)==expected_precommit_manifest,'approved payload binding')
        approved_files=manifest(approved_raw)
        changed=sorted(n for n in set(files)|set(approved_files) if files.get(n)!=approved_files.get(n))
        require(set(changed)<=PACKAGING_ONLY,'product changed after precommit approval; requires new review')
        impact=json.loads(read(root,'REVIEW_IMPACT.json'))
        require(impact['changed_paths']==changed and impact['product_changes']==[]
                and impact['approved_source_manifest_sha256']==expected_precommit_manifest
                and impact['final_source_manifest_sha256']==source_hash,'review impact binding')
    summary = json.loads(read(root, 'gates/summary.json'))
    require(summary['stage'] == 'S5-I2-A' and summary['result'] == ('GATES_PASS' if final else 'CANDIDATE_GATES_PASS'),
            'gate summary status')
    require(summary['source_manifest_sha256'] == source_hash and summary['commit'] == review['base_commit'],
            'gate summary identity')
    require(summary['clean_worktree'] is final and summary['unchanged_source'] is True
            and summary['unchanged_status'] is True, 'source stability')
    require(type(summary['gates_expected']) is int and summary['gates_expected'] == len(GATES)
            and type(summary['gates_passed']) is int and summary['gates_passed'] == len(GATES),
            'gate count')
    require(read(root, 'gates/source-before.sha256') == source_manifest
            == read(root, 'gates/source-after.sha256'), 'tested source binding')
    if final:
        require(not read(root,'gates/status-before.txt').strip()
                and not read(root,'gates/status-after.txt').strip(),'final worktree dirty')
    gates = json.loads(read(root, 'gates/gates.json'))
    require(sha(json.dumps([(g['id'],g['command']) for g in gates],separators=(',',':')).encode()) == EXPECTED_COMMANDS_SHA256, 'gate command set weakened')
    require(tuple(g['id'] for g in gates) == GATES, 'required gate set/order')
    for g in gates:
        require(type(g['exit_code']) is int and g['exit_code'] == 0, 'failed/mistyped gate')
        require(g['commit'] == review['base_commit'] and g['source_manifest_sha256'] == source_hash,
                'gate identity')
        require(g['log'] == g['id'] + '.log', 'gate log binding')
        log = read(root, 'gates/' + g['log'])
        require((log or g['id'] == 'fmt') and sha(log) == g['sha256'], 'gate log digest')
        if g['id'] in ('i2-core-release', 'i2-budget-unit-release', 'i2-service-release', 'i2-app-release'):
            minimum = {'i2-core-release': 29, 'i2-budget-unit-release':3, 'i2-service-release': 8, 'i2-app-release': 6}[g['id']]
            outcomes = re.findall(rb'test result: ok\. (\d+) passed; 0 failed;', log)
            require(sum(int(n) for n in outcomes) >= minimum, 'missing executed I2 tests')
        if g['id'].startswith('metal-'):
            require(b'1 passed; 0 failed' in log and b'Metal' in log, 'missing native Metal execution')
    numeric=json.loads(read(root,'gates/numeric-oracles/summary.json'))
    require(numeric['source_manifest_sha256']==source_hash and numeric['result']=='NUMERIC_ORACLES_PASS'
            and type(numeric['failures']) is int and numeric['failures']==0, 'numeric oracle status/source')
    require(numeric['circle_cases']==1662 and numeric['original_circle_cases']==678
            and numeric['polygon_cases']==1470 and numeric['original_circle_ready']>=500
            and numeric['polygon_ready']>=1200, 'numeric oracle matrix/admission omitted')
    rows=numeric['original_14_successful_bounds_validated']
    require(len(rows)==14 and len({r['case'] for r in rows})==14
            and all(r['actual'].startswith('READY ') for r in rows), 'independent 14 regressions omitted')
    require(numeric['real_negative_thin_annulus']['actual'].startswith('ERR PrecisionUncertain('),
            'thin annulus false zero negative omitted')
    for kind in ('circle','polygon'):
        payload=read(root,'gates/numeric-oracles/'+kind+'-oracle.json')
        require(sha(payload)==numeric[kind+'_report_sha256'], 'numeric report digest')
        report=json.loads(payload)
        require(report['case_count']==numeric[kind+'_cases'] and report['failures']==[],
                'numeric report failures/count')
    require(numeric['macro_cases']==1512 and numeric['macro_baseline_cases']==900
            and numeric['macro_practical_cases']==numeric['macro_practical_ready']==360,
            'Macro transform/practical matrix omitted')
    payload=read(root,'gates/numeric-oracles/macro-oracle.json')
    require(sha(payload)==numeric['macro_report_sha256'], 'Macro report digest')
    macro=json.loads(payload)
    require(macro['cases']==1512 and macro['baseline_cases']==900 and macro['failures']==[],
            'Macro success error bound failed/matrix omitted')
    rows=numeric['macro_original_32_replayed']
    require(rows==macro['original_32_replayed'] and len(rows)==len({r['input'] for r in rows})==32
            and all(r['actual'].startswith(('READY ','ERR PrecisionUncertain(')) for r in rows),
            'Macro independent 32 regressions omitted')
    payload=read(root,'gates/numeric-oracles/chain-observation.json')
    require(sha(payload)==numeric['chain_report_sha256'], 'chain report digest')
    chain=json.loads(payload)['rows']
    require(chain==numeric['chain_observations'] and len(chain)==2
            and {r['chain10k'] for r in chain}=={'budget','external_cancel'}, 'chain scenarios omitted')
    by_kind={r['chain10k']:r for r in chain}
    budget=by_kind['budget'];cancel=by_kind['external_cancel']
    require(budget['outcome']=='ResourceLimit' and type(budget['polls']) is int
            and 2_000_000<=budget['polls']<2_100_000 and 0<=budget['max_unchecked_ms']<=500,
            'chain budget/checkpoint observation failed')
    require(cancel['outcome']=='Cancelled' and 0<=cancel['cancel_request_to_return_ms']<=2000,
            'chain external controlled return failed')
    boundary = json.loads(read(root, 'gates/service-boundary.json'))
    require(boundary['forbidden_dependencies'] == [], 'service window/GPU dependency')
    binaries = json.loads(read(root, 'gates/binaries.json'))
    require(set(binaries) == {'release', 'release-internal'}, 'release binary set')
    for kind, binary in binaries.items():
        data = read(root, 'gates/' + safe(binary['path']))
        require(sha(data) == binary['sha256'] and binary['source_manifest_sha256'] == source_hash,
                'binary identity')
        expected_binary=review['base_commit']+('' if final else '-dirty')
        require(binary['commit'] == expected_binary and binary['profile'] == 'release',
                'binary profile/commit')
        require(binary['commit'].encode() in data, 'embedded binary commit identity')
        if final:
            require((expected_binary+'-dirty').encode() not in data,'dirty final binary identity')
        require((b'RCAM_I1_NATIVE_DIR' in data) == (kind == 'release-internal'), 'public/internal boundary')
    return {'schema_version': 2, 'result': 'A_CLEAN_FINAL_INTEGRITY_PASS' if final else 'A_CANDIDATE_INTEGRITY_PASS',
            'scope': 'S5-I2-A', 'acceptance': acceptance,
            'source_manifest_sha256': source_hash, 'source_files': len(files),
            'gates': len(gates), 'native_I2_acceptance': 'NOT_EXECUTED',
            'required_remaining': list(remaining)}


if __name__ == '__main__':
    p = argparse.ArgumentParser(description=__doc__)
    p.add_argument('bundle', type=Path)
    p.add_argument('--expected-source-manifest-sha256', required=True)
    p.add_argument('--expected-commit')
    p.add_argument('--expected-precommit-source-manifest-sha256')
    args = p.parse_args()
    try:
        print(json.dumps(verify(args.bundle,args.expected_source_manifest_sha256,args.expected_commit,
                                args.expected_precommit_source_manifest_sha256), ensure_ascii=False, indent=2))
    except (ValueError, KeyError, OSError, zipfile.BadZipFile, json.JSONDecodeError) as error:
        p.exit(1, 'FAIL: ' + str(error) + '\n')
