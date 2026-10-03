"""Synthetic integrity/tamper fixtures, never execution/acceptance evidence.
Small fixture directories are preserved for inspection; no cleanup performed.
"""
import hashlib
import json
from pathlib import Path
import tempfile
import unittest
import zipfile
from run_i2_a_gates import COMMANDS
from verify_i2_a_evidence import GATES, REMAINING, FINAL_REMAINING, verify


def data(value):
    return (json.dumps(value, sort_keys=True)+'\n').encode()


def sha(value):
    return hashlib.sha256(value).hexdigest()


def put(root, name, value):
    p = root/name
    p.parent.mkdir(parents=True, exist_ok=True)
    p.write_bytes(value)


def seal(root):
    (root/'BUNDLE.sha256').write_text(''.join(
        sha(p.read_bytes())+'  '+p.relative_to(root).as_posix()+'\n'
        for p in sorted(root.rglob('*')) if p.is_file() and p != root/'BUNDLE.sha256'))


def fixture():
    root = Path(tempfile.mkdtemp(prefix='rcam-i2-a-verifier-synthetic-'))
    commit = '1'*40
    source = sha(b'synthetic')+'  README.md\n'
    source_hash = sha(source.encode())
    info = dict(git_commit=commit, commit=commit, stage='S5-I2-A review candidate',
                clean_worktree=False, source_file_count=1, source_manifest_sha256=source_hash)
    members = {'README.md': b'synthetic', 'MANIFEST.sha256': source.encode(),
               'PACKAGE_INFO.json': data(info)}
    members['PACKAGE_MANIFEST.sha256'] = ''.join(sha(v)+'  '+k+'\n' for k,v in sorted(members.items())).encode()
    with zipfile.ZipFile(root/'Source.zip', 'w') as z:
        for k,v in members.items():
            z.writestr(k,v)
    put(root, 'REVIEW.json', data(dict(schema_version=2, scope='S5-I2-A', acceptance='NOT_ACCEPTED',
        base_commit=commit, source_zip_sha256=sha((root/'Source.zip').read_bytes()),
        source_manifest_sha256=source_hash, required_remaining=list(REMAINING))))
    put(root, 'gates/summary.json', data(dict(stage='S5-I2-A', result='CANDIDATE_GATES_PASS',
        source_manifest_sha256=source_hash, commit=commit, clean_worktree=False,
        unchanged_source=True, unchanged_status=True, gates_expected=len(GATES), gates_passed=len(GATES))))
    put(root,'gates/source-before.sha256',source.encode())
    put(root,'gates/source-after.sha256',source.encode())
    gates=[]
    for name,command in COMMANDS:
        n={'i2-core-release':29,'i2-budget-unit-release':3,'i2-service-release':8,'i2-app-release':6}.get(name,1)
        log=b'' if name=='fmt' else f'SYNTHETIC TEST FIXTURE ONLY\nbackend: Metal\ntest result: ok. {n} passed; 0 failed;\n'.encode()
        put(root,'gates/'+name+'.log',log)
        gates.append(dict(id=name, command=command, exit_code=0, commit=commit,
            source_manifest_sha256=source_hash, log=name+'.log', sha256=sha(log)))
    put(root,'gates/gates.json',data(gates))
    put(root,'gates/service-boundary.json',data(dict(forbidden_dependencies=[])))
    numeric=dict(result='NUMERIC_ORACLES_PASS',source_manifest_sha256=source_hash,failures=0,
        circle_cases=1662,original_circle_cases=678,original_circle_ready=517,polygon_cases=1470,polygon_ready=1269,
        original_14_successful_bounds_validated=[dict(case=str(i),actual='READY SYNTHETIC') for i in range(14)],
        real_negative_thin_annulus=dict(actual='ERR PrecisionUncertain(SYNTHETIC)'))
    for kind,count in [('circle',1662),('polygon',1470)]:
        payload=data(dict(case_count=count,failures=[]))
        put(root,'gates/numeric-oracles/'+kind+'-oracle.json',payload)
        numeric[kind+'_report_sha256']=sha(payload)
    rows=[dict(input=str(i),actual='ERR PrecisionUncertain(SYNTHETIC)') for i in range(32)]
    payload=data(dict(cases=1512,baseline_cases=900,failures=[],original_32_replayed=rows))
    put(root,'gates/numeric-oracles/macro-oracle.json',payload)
    numeric.update(macro_cases=1512,macro_baseline_cases=900,macro_practical_cases=360,macro_practical_ready=360,
                   macro_original_32_replayed=rows,macro_report_sha256=sha(payload))
    rows=[dict(chain10k='budget',polls=2010002,max_unchecked_ms=1.,outcome='ResourceLimit'),
          dict(chain10k='external_cancel',cancel_request_to_return_ms=1.,outcome='Cancelled')]
    payload=data(dict(rows=rows));put(root,'gates/numeric-oracles/chain-observation.json',payload)
    numeric.update(chain_observations=rows,chain_report_sha256=sha(payload))
    put(root,'gates/numeric-oracles/summary.json',data(numeric))
    binaries={}
    for kind in ('release','release-internal'):
        content=(commit+'-dirty '+source_hash+(' RCAM_I1_NATIVE_DIR' if kind=='release-internal' else '')).encode()
        path='bin/'+kind
        put(root,'gates/'+path,content)
        binaries[kind]=dict(path=path, sha256=sha(content), source_manifest_sha256=source_hash, commit=commit+'-dirty',profile='release')
    put(root,'gates/binaries.json',data(binaries))
    seal(root)
    return root


def change(root,name,fn):
    p=root/name
    value=json.loads(p.read_bytes())
    fn(value)
    p.write_bytes(data(value))


def final_fixture():
    root=fixture();commit='1'*40
    with zipfile.ZipFile(root/'Source.zip') as z:members={n:z.read(n) for n in z.namelist()}
    source=members['MANIFEST.sha256'];source_hash=sha(source)
    info=json.loads(members['PACKAGE_INFO.json']);info.update(stage='S5-I2-A clean final',clean_worktree=True)
    members['PACKAGE_INFO.json']=data(info)
    members['PACKAGE_MANIFEST.sha256']=''.join(sha(v)+'  '+k+'\n' for k,v in sorted(members.items()) if k!='PACKAGE_MANIFEST.sha256').encode()
    with zipfile.ZipFile(root/'Source.zip','w') as z:
        for k,v in members.items():z.writestr(k,v)
    report=b'SYNTHETIC PRECOMMIT A REVIEW FIXTURE ONLY\n'
    put(root,'independent-precommit/report.md',report);put(root,'independent-precommit/source-manifest.sha256',source)
    change(root,'REVIEW.json',lambda v:v.update(artifact_mode='clean_final_review',acceptance='A_FINAL_REVIEW_PENDING',
        whole_I2_acceptance='NOT_ACCEPTED',required_remaining=list(FINAL_REMAINING),source_zip_sha256=sha((root/'Source.zip').read_bytes()),
        precommit_A_review=dict(result='PRECOMMIT_A_PASS_MAC_FIRST_BOUNDED',source_manifest_sha256=source_hash,
                               report_path='independent-precommit/report.md',report_sha256=sha(report))))
    change(root,'gates/summary.json',lambda v:v.update(result='GATES_PASS',clean_worktree=True))
    put(root,'gates/status-before.txt',b'');put(root,'gates/status-after.txt',b'')
    put(root,'REVIEW_IMPACT.json',data(dict(changed_paths=[],product_changes=[],approved_source_manifest_sha256=source_hash,final_source_manifest_sha256=source_hash)))
    for kind in ('release','release-internal'):
        content=(commit+' '+source_hash+(' RCAM_I1_NATIVE_DIR' if kind=='release-internal' else '')).encode()
        put(root,'gates/bin/'+kind,content)
        change(root,'gates/binaries.json',lambda v:v[kind].update(commit=commit,sha256=sha(content)))
    seal(root)
    return root,(source_hash,commit,source_hash)


class Tamper(unittest.TestCase):
    def test_clean_final_only_attests_integrity_and_keeps_remaining_I2(self):
        root,refs=final_fixture();result=verify(root,*refs)
        self.assertEqual(result['result'],'A_CLEAN_FINAL_INTEGRITY_PASS')
        self.assertEqual(result['acceptance'],'A_FINAL_REVIEW_PENDING')
        self.assertEqual(result['native_I2_acceptance'],'NOT_EXECUTED')

    def test_clean_final_requires_three_external_references(self):
        root,(source,commit,approved)=final_fixture()
        for refs in [(source,None,approved),(source,'2'*40,approved),(source,commit,None),
                     (source,commit,'2'*64),('2'*64,commit,approved)]:
            with self.subTest(refs=refs):
                with self.assertRaises(ValueError):verify(root,*refs)

    def test_clean_final_resealed_metadata_and_dirty_evidence_rejected(self):
        mutations=[
            ('REVIEW.json',lambda v:v.update(whole_I2_acceptance='PASS')),
            ('REVIEW.json',lambda v:v['precommit_A_review'].update(result='FAIL')),
            ('REVIEW.json',lambda v:v.update(artifact_mode='candidate')),
            ('gates/summary.json',lambda v:v.update(clean_worktree=False)),
            ('REVIEW_IMPACT.json',lambda v:v.update(product_changes=['crates/changed.rs'])),
            ('REVIEW_IMPACT.json',lambda v:v.update(changed_paths=['crates/changed.rs'])),
        ]
        for name,fn in mutations:
            root,refs=final_fixture();change(root,name,fn);seal(root)
            with self.subTest(name=name):
                with self.assertRaises(ValueError):verify(root,*refs)
        root,refs=final_fixture();put(root,'gates/status-before.txt',b'M dirty');seal(root)
        with self.assertRaises(ValueError):verify(root,*refs)
        root,refs=final_fixture();content=(refs[1]+'-dirty').encode();put(root,'gates/bin/release',content)
        change(root,'gates/binaries.json',lambda v:v['release'].update(commit=refs[1]+'-dirty',sha256=sha(content)));seal(root)
        with self.assertRaises(ValueError):verify(root,*refs)

    def test_clean_final_unapproved_source_change_rejected(self):
        root,(source,commit,approved)=final_fixture()
        with zipfile.ZipFile(root/'Source.zip') as z:members={n:z.read(n) for n in z.namelist()}
        members['README.md']=b'UNAPPROVED SOURCE CHANGE'
        members['MANIFEST.sha256']=(sha(members['README.md'])+'  README.md\n').encode();new_hash=sha(members['MANIFEST.sha256'])
        info=json.loads(members['PACKAGE_INFO.json']);info['source_manifest_sha256']=new_hash;members['PACKAGE_INFO.json']=data(info)
        members['PACKAGE_MANIFEST.sha256']=''.join(sha(v)+'  '+k+'\n' for k,v in sorted(members.items()) if k!='PACKAGE_MANIFEST.sha256').encode()
        with zipfile.ZipFile(root/'Source.zip','w') as z:
            for k,v in members.items():z.writestr(k,v)
        change(root,'REVIEW.json',lambda v:v.update(source_manifest_sha256=new_hash,source_zip_sha256=sha((root/'Source.zip').read_bytes())))
        change(root,'REVIEW_IMPACT.json',lambda v:v.update(final_source_manifest_sha256=new_hash,changed_paths=['README.md']))
        seal(root)
        with self.assertRaises(ValueError):verify(root,new_hash,commit,approved)

    def test_complete_fixture_only_attests_foundation_integrity(self):
        result=verify(fixture())
        self.assertEqual(result['acceptance'],'NOT_ACCEPTED')
        self.assertEqual(result['native_I2_acceptance'],'NOT_EXECUTED')

    def test_resealed_semantic_tamper_rejected(self):
        mutations=[
            ('REVIEW.json',lambda v:v.update(schema_version=True)),
            ('REVIEW.json',lambda v:v.update(scope='S5-I2')),
            ('REVIEW.json',lambda v:v.update(acceptance='PASS')),
            ('REVIEW.json',lambda v:v.update(required_remaining=[])),
            ('REVIEW.json',lambda v:v.update(base_commit='2'*40)),
            ('gates/summary.json',lambda v:v.update(unchanged_source=False)),
            ('gates/summary.json',lambda v:v.update(clean_worktree=True)),
            ('gates/summary.json',lambda v:v.update(gates_passed=True)),
            ('gates/gates.json',lambda v:v.pop()),
            ('gates/gates.json',lambda v:v[0].update(exit_code=False)),
            ('gates/gates.json',lambda v:v[0].update(exit_code=1)),
            ('gates/gates.json',lambda v:v[0].update(command=['true'])),
            ('gates/gates.json',lambda v:v[0].update(log='../fmt.log')),
            ('gates/service-boundary.json',lambda v:v.update(forbidden_dependencies=['wgpu'])),
            ('gates/binaries.json',lambda v:v['release'].update(profile='debug')),
            ('gates/binaries.json',lambda v:v['release'].update(commit='1'*40)),
            ('gates/numeric-oracles/summary.json',lambda v:v.update(failures=1)),
            ('gates/numeric-oracles/summary.json',lambda v:v.update(failures=False)),
            ('gates/numeric-oracles/summary.json',lambda v:v.update(circle_cases=1)),
            ('gates/numeric-oracles/summary.json',lambda v:v.update(original_14_successful_bounds_validated=[])),
            ('gates/numeric-oracles/summary.json',lambda v:v.update(real_negative_thin_annulus=dict(actual='ZERO'))),
            ('gates/numeric-oracles/summary.json',lambda v:v.update(circle_report_sha256='0'*64)),
            ('gates/numeric-oracles/summary.json',lambda v:v.update(macro_cases=1)),
            ('gates/numeric-oracles/summary.json',lambda v:v.update(macro_original_32_replayed=[])),
            ('gates/numeric-oracles/summary.json',lambda v:v.update(macro_practical_ready=0)),
            ('gates/numeric-oracles/summary.json',lambda v:v.update(macro_report_sha256='0'*64)),
            ('gates/numeric-oracles/summary.json',lambda v:v.update(chain_observations=[])),
            ('gates/numeric-oracles/summary.json',lambda v:v.update(chain_report_sha256='0'*64)),
        ]
        for name,fn in mutations:
            with self.subTest(name=name,mutation=str(fn)):
                root=fixture();change(root,name,fn);seal(root)
                with self.assertRaises(ValueError):verify(root)

    def test_content_and_path_tamper_rejected(self):
        for name,content in [('gates/i2-core-release.log',b'test result: ok. 0 passed; 0 failed;'),
                             ('gates/i2-budget-unit-release.log',b'test result: ok. 0 passed; 0 failed;'),
                             ('gates/metal-reference.log',b'test result: ok. 0 passed; 0 failed;'),
                             ('gates/bin/release',b'wrong'),
                             ('gates/source-after.sha256',b'wrong')]:
            with self.subTest(name=name):
                root=fixture();put(root,name,content)
                if name.endswith('.log'):
                    change(root,'gates/gates.json',lambda v:[g.update(sha256=sha(content)) for g in v if 'gates/'+g['log']==name])
                if name=='gates/bin/release':
                    change(root,'gates/binaries.json',lambda v:v['release'].update(sha256=sha(content)))
                seal(root)
                with self.assertRaises(ValueError):verify(root)
        root=fixture();put(root,'extra',b'unsealed')
        with self.assertRaises(ValueError):verify(root)
        root=fixture();(root/'link').symlink_to('REVIEW.json')
        with self.assertRaises(ValueError):verify(root)
        root=fixture();(root/'BUNDLE.sha256').write_text('0'*64+'  ../outside\n')
        with self.assertRaises(ValueError):verify(root)

    def test_external_reference_and_nested_unsealed_manifest_rejected(self):
        root=fixture()
        with self.assertRaises(ValueError):verify(root,'2'*64)
        result=verify(root,json.loads((root/'REVIEW.json').read_bytes())['source_manifest_sha256'])
        self.assertEqual(result['acceptance'],'NOT_ACCEPTED')
        put(root,'nested/BUNDLE.sha256',b'unsealed reserved-looking leaf')
        with self.assertRaises(ValueError):verify(root)

    def test_resealed_unsafe_source_archive_rejected(self):
        root=fixture()
        with zipfile.ZipFile(root/'Source.zip','a') as z:z.writestr('../outside',b'unsafe')
        change(root,'REVIEW.json',lambda v:v.update(source_zip_sha256=sha((root/'Source.zip').read_bytes())))
        seal(root)
        with self.assertRaises(ValueError):verify(root)


if __name__=='__main__':unittest.main()
