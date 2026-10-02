"""Synthetic verifier fixtures only. These do not constitute native evidence."""
import copy
import json
from pathlib import Path
import tempfile
import unittest
import verify_s5m2_evidence as v

COMMIT = 'c'*40

def write(path, data):
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_text(json.dumps(data), encoding='utf-8')

def manifest(root, name):
    (root/name).write_text(''.join(v.digest(p)+'  '+p.relative_to(root).as_posix()+'\n'
        for p in sorted(root.rglob('*')) if p.is_file() and p.name != name), encoding='utf-8')

def fixture(root):
    source, evidence = root/'Source', root/'Evidence'
    source.mkdir(); evidence.mkdir()
    for name in v.source_manifest.ROOT_FILES:
        (source/name).write_text('synthetic verifier fixture\n')
    v.source_manifest.ROOT = source
    (source/'MANIFEST.sha256').write_text(v.source_manifest.contents())
    sha = v.digest(source/'MANIFEST.sha256')
    included = sorted(v.source_manifest.ROOT_FILES+['MANIFEST.sha256'])
    write(source/'PACKAGE_INFO.json',{'schema_version':1,'clean_worktree':True,'supplemental_only':False,
        'git_commit':COMMIT,'commit':COMMIT,'source_manifest_sha256':sha,'included_paths':included,
        'source_file_count':len(included)-1,'manifest_count':len(included)})
    manifest(source,'PACKAGE_MANIFEST.sha256')
    report = {'schema_version':2,'stage':'S5-M2-A2','commit':COMMIT,'clean_worktree':True,
        'source_manifest_sha256':sha,'binaries':{},'gates':[],'native_runs':[], 'environment':'environment.json'}
    write(evidence/'environment.json', {'macos':'fixture','cpu':'fixture','ram':'16 GB','gpu':'fixture',
        'displays':[{'pixels':'3840 x 2160','resolution_refresh':'1920 x 1080 @ 144.00Hz'}]})
    for name in ('release','internal-evidence'):
        binary = evidence/(name+'.bin'); binary.write_bytes(b'synthetic verifier fixture '+COMMIT.encode()+(b'RCAM_A2_NATIVE_DIR' if name=='internal-evidence' else b''))
        digest = v.digest(binary)
        write(evidence/(name+'.json'),{'commit':COMMIT,'sha256':digest,'source':'git-clean'})
        report['binaries'][name] = {'commit':COMMIT,'profile':'release','source_manifest_sha256':sha,
            'path':binary.name,'sha256':digest,'observed_identity':name+'.json'}
    for name in sorted(v.GATES):
        log = evidence/(name+'.log'); log.write_text('synthetic verifier fixture')
        report['gates'].append({'id':name,'exit_code':0,'commit':COMMIT,'source_manifest_sha256':sha,
            'command':v.GATE_COMMANDS[name],'log':log.name,'sha256':v.digest(log)})
    binary_sha = report['binaries']['internal-evidence']['sha256']
    for index, (round_number, case) in enumerate((r,c) for r in (1,2,3) for c in ('A','B','C','D','E-undo','E-redo')):
        directory = evidence/f'round-{round_number}-{case}'; directory.mkdir()
        base = (index+1)*1_000_000_000
        before = {'document_id':'a','revision':'0','workspace_revision':'0','generation':2,
            'content_sha256':'0'*64,'dirty':False,'project_dirty':False,'layers':1,'objects':5,
            'undo_entries':1,'redo_entries':1,'selection':['object-1']}
        after = copy.deepcopy(before)
        if case == 'B': after.update(revision='1',workspace_revision='1',undo_entries=2,layers=2,dirty=True,project_dirty=True)
        if case == 'C': after.update(document_id=None,revision=None,workspace_revision=None,generation=3,
            content_sha256=None,dirty=None,project_dirty=None,layers=0,objects=0,selection=[],undo_entries=None,redo_entries=None)
        if case == 'D': after.update(document_id='b',generation=3)
        if case.startswith('E-'):
            delta = -1 if case == 'E-undo' else 1
            after.update(revision='1',workspace_revision='1',undo_entries=1+delta,redo_entries=1-delta)
        version = {'document_id':'a','document_revision':'0','workspace_revision':'0','generation':2,
            'rule_revision':0,'geometry_policy_hash':'0'*64}
        worker_after = after if case=='B' else before
        receipt = {'task_id':7,'input':copy.deepcopy(version),'result_version':v.state_version(worker_after,version),
            'state':'cancelled' if case=='A' else 'completed'}
        baseline_receipt = {'task_id':6,'input':copy.deepcopy(version),'result_version':copy.deepcopy(version),'state':'completed'}
        final_receipt = receipt if case in ('A','B') else {'task_id':8,'input':copy.deepcopy(version),
            'result_version':v.state_version(after,version),'state':'completed'}
        def event(label, delta, data, task=7):
            return {'task_id':task,'event':label,'at_ns':base+delta*1_000_000,'data':copy.deepcopy(data)}
        events = [event('worker_finished',1,{'receipt':baseline_receipt,'after':before,'error':None},6),
            event('reply_received',2,{'installed':True},6),event('worker_begin',10,{'version':version,'before':before}),
            event('worker_finished',30 if case in ('A','B') else 15,{'receipt':receipt,'after':worker_after,'error':'CANCELLED' if case=='A' else None}),
            event('worker_returned',31,{}),event('reply_received',32,{'installed':case in ('A','B')})]
        if case in ('A','B'):
            events.extend([event('native_cancel_input',21,{'phase':case}),event('cancel_button',22,{'outcome':'Requested' if case=='A' else 'TooLate'})])
        else:
            events.extend([event('native_transition_input',21,{'kind':case}),event('delivery_barrier',16,{'injected':True}),
                event('worker_finished',40,{'receipt':final_receipt,'after':after,'error':None},8),event('reply_received',41,{'installed':True},8)])
        if case=='B': events.append(event('commit_barrier',15,{'injected':True}))
        if case!='A': events.append(event('barrier_released',29,{'released':True,'elapsed_ms':10}))
        for delta in (5,50,70):
            image = f'frame-{base+delta*1_000_000}.ppm'
            (directory/image).write_bytes(b'P6\n1 1\n255\n\0\0\0')
            if delta != 5: events.append(event('screenshot_received',delta,{'focused':True,'path':image}))
        request={'schema_version':2,'round':round_number,'case':case,'run_id':directory.name,
            'fixture':'local-fixture.gbr','fixture_sha256':v.LARGE_REAL_SHA256,'start_utc':'synthetic'}
        observation = {'schema_version':2,'stage':'S5-M2-A2','status':'OBSERVED','request':request,
            'commit':COMMIT,'build_source':'git-clean','binary_sha256':binary_sha,'profile':'release','adapter':'fixture Metal',
            'failures':[],'target':7,'events':events,'before':before,'after':after,'input_ns':base+20_000_000,
            'baseline_frame_ns':base+5_000_000,'feedback_upper_bound_ms':30.0,'worker_return_upper_bound_ms':11.0,
            'frames':[{'at_ns':base+3_000_000,'state':before,'receipt':baseline_receipt,'has_last_good':True},
                {'at_ns':base+25_000_000,'state':before,'receipt':baseline_receipt,'has_last_good':True},
                {'at_ns':base+100_000_000,'state':after,'receipt':final_receipt,'has_last_good':case!='C'}]}
        write(directory/'observations.json',observation)
        write(directory/'request.json',request)
        write(directory/'runner.json',{'exit_code':0,'error':None,'binary_sha256':binary_sha,'fixture_sha256':v.LARGE_REAL_SHA256})
        report['native_runs'].append({'round':round_number,'case':case,'observations':directory.name+'/observations.json'})
    write(evidence/'CLOSEOUT.json',report); manifest(evidence,'EVIDENCE_MANIFEST.sha256')
    return source, evidence

class VerifierTests(unittest.TestCase):
    def setUp(self):
        self.tmp = tempfile.TemporaryDirectory(prefix='rcam-a2-verifier-')
        self.addCleanup(self.tmp.cleanup)
        self.source,self.evidence = fixture(Path(self.tmp.name).resolve())
    def check(self): v.verify(self.source,self.evidence)
    def mutate(self, relative, change):
        path=self.evidence/relative; data=v.load(path); change(data); write(path,data)
        manifest(self.evidence,'EVIDENCE_MANIFEST.sha256')
    def reject(self):
        with self.assertRaises((ValueError,KeyError,FileNotFoundError)): self.check()
    def test_valid_relocated_fixture(self): self.check()
    def test_binary_embedded_dirty_identity(self):
        binary = self.evidence/'release.bin'
        binary.write_bytes((COMMIT+'-dirty').encode())
        digest = v.digest(binary)
        identity = self.evidence/'release.json'; data = v.load(identity); data['sha256']=digest; write(identity,data)
        self.mutate('CLOSEOUT.json',lambda d:d['binaries']['release'].update(sha256=digest))
        self.reject()
    def test_runner_timeout_even_with_zero_exit(self):
        self.mutate('round-1-A/runner.json',lambda d:d.update(error='timeout')); self.reject()
    def test_unmanifested_source_file(self):
        (self.source/'unexpected.bin').write_bytes(b'unlisted'); self.reject()
    def test_nested_unmanifested_evidence_manifest(self):
        (self.evidence/'round-1-A/EVIDENCE_MANIFEST.sha256').write_text('unlisted'); self.reject()
    def test_gate_command_substitution(self):
        self.mutate('CLOSEOUT.json',lambda d:d['gates'][0].update(command=['true'])); self.reject()
    def test_empty_selection_baseline(self):
        self.mutate('round-1-A/observations.json',lambda d:d['before'].update(selection=[])); self.reject()
    def test_source_tamper(self):
        (self.source/'README.md').write_text('changed'); self.reject()
    def test_evidence_tamper(self):
        (self.evidence/'fmt.log').write_text('changed'); self.reject()
    def test_missing_gate(self):
        self.mutate('CLOSEOUT.json',lambda d:d['gates'].pop()); self.reject()
    def test_failed_gate(self):
        self.mutate('CLOSEOUT.json',lambda d:d['gates'][0].update(exit_code=1)); self.reject()
    def test_missing_native(self):
        self.mutate('CLOSEOUT.json',lambda d:d['native_runs'].pop()); self.reject()
    def test_duplicate_native(self):
        self.mutate('CLOSEOUT.json',lambda d:d['native_runs'].__setitem__(1,d['native_runs'][0])); self.reject()
    def test_binary_mismatch(self):
        self.mutate('round-1-A/observations.json',lambda d:d.update(binary_sha256='f'*64)); self.reject()
    def test_commit_mismatch(self):
        self.mutate('CLOSEOUT.json',lambda d:d.update(commit='e'*40)); self.reject()
    def test_dirty_binary(self):
        self.mutate('round-1-A/observations.json',lambda d:d.update(build_source='git-dirty')); self.reject()
    def test_timeout(self):
        self.mutate('round-1-B/observations.json',lambda d:d['failures'].append('timeout')); self.reject()
    def test_missing_version_dimension(self):
        self.mutate('round-1-A/observations.json',lambda d:next(e for e in d['events'] if e['event']=='worker_begin')['data']['version'].pop('generation')); self.reject()
    def test_partial_publish(self):
        self.mutate('round-1-A/observations.json',lambda d:d['frames'][1]['state'].update(objects=6)); self.reject()
    def test_late_feedback(self):
        self.mutate('round-1-A/observations.json',lambda d:[e.update(at_ns=e['at_ns']+1_000_000_000) for e in d['events'] if e['event']=='screenshot_received']); self.reject()
    def test_stale_installed(self):
        self.mutate('round-1-C/observations.json',lambda d:[e['data'].update(installed=True) for e in d['events'] if e['event']=='reply_received']); self.reject()
    def test_missing_image(self):
        next(self.evidence.glob('round-1-A/*.ppm')).unlink(); manifest(self.evidence,'EVIDENCE_MANIFEST.sha256'); self.reject()
    def test_traversal(self):
        self.mutate('CLOSEOUT.json',lambda d:d['gates'][0].update(log='../Source/README.md')); self.reject()
    def test_duplicate_json_key(self):
        p=self.evidence/'CLOSEOUT.json'; p.write_text(p.read_text().replace('"schema_version": 2','"schema_version": 2, "schema_version": 2'))
        manifest(self.evidence,'EVIDENCE_MANIFEST.sha256'); self.reject()
    # mutate() refreshes the evidence manifest: these reject contradictions,
    # not an incidental file-hash mismatch.
    def test_partial_commit_frame(self):
        self.mutate('round-1-B/observations.json',lambda d:d['frames'][1]['state'].update(objects=999)); self.reject()
    def test_closed_state_all_fields(self):
        for field,value in [('layers',1),('objects',5),('selection',['object-1']),('content_sha256','0'*64),
                            ('undo_entries',1),('redo_entries',0),('revision','0'),('workspace_revision','0'),
                            ('dirty',False),('project_dirty',False),('generation',2)]:
            with self.subTest(field=field):
                p=self.evidence/'round-1-C/observations.json';original=v.load(p)
                def change(d):
                    d['after'][field]=value;d['frames'][-1]['state'][field]=value
                self.mutate('round-1-C/observations.json',change);self.reject();write(p,original)
                manifest(self.evidence,'EVIDENCE_MANIFEST.sha256')
    def test_result_version_all_dimensions(self):
        for field,value in [('document_id','wrong'),('document_revision','99'),('workspace_revision','99'),
                            ('generation',999),('rule_revision',999),('geometry_policy_hash','f'*64)]:
            with self.subTest(field=field):
                p=self.evidence/'round-1-A/observations.json';original=v.load(p)
                self.mutate('round-1-A/observations.json',lambda d:next(e for e in d['events'] if e['event']=='worker_finished' and e['task_id']==7)['data']['receipt']['result_version'].update({field:value}))
                self.reject();write(p,original);manifest(self.evidence,'EVIDENCE_MANIFEST.sha256')
    def test_input_policy_must_match_accepted_baseline(self):
        def change(d):
            for e in d['events']:
                if e['task_id']==7:
                    if e['event']=='worker_begin':e['data']['version']['geometry_policy_hash']='f'*64
                    if e['event']=='worker_finished':
                        e['data']['receipt']['input']['geometry_policy_hash']='f'*64
                        e['data']['receipt']['result_version']['geometry_policy_hash']='f'*64
        self.mutate('round-1-A/observations.json',change);self.reject()
    def test_runner_fixture_mismatch(self):
        self.mutate('round-1-A/runner.json',lambda d:d.update(fixture_sha256='f'*64));self.reject()
    def test_runner_binary_mismatch(self):
        self.mutate('round-1-A/runner.json',lambda d:d.update(binary_sha256='f'*64));self.reject()
    def test_independent_request_run_mismatch(self):
        self.mutate('round-1-A/request.json',lambda d:d.update(run_id='other-run'));self.reject()
    def test_independent_request_fixture_mismatch(self):
        self.mutate('round-1-A/request.json',lambda d:d.update(fixture_sha256='f'*64));self.reject()
    def test_cancel_button_after_worker_return(self):
        self.mutate('round-1-A/observations.json',lambda d:next(e for e in d['events'] if e['event']=='cancel_button').update(at_ns=d['input_ns']+1_000_000_000));self.reject()
    def test_self_reported_input_cannot_follow_input_event(self):
        def change(d):
            d['input_ns']+=2_000_000
            d['feedback_upper_bound_ms']-=2
            d['worker_return_upper_bound_ms']-=2
        self.mutate('round-1-A/observations.json',change);self.reject()
    def test_commit_barrier_after_button(self):
        self.mutate('round-1-B/observations.json',lambda d:next(e for e in d['events'] if e['event']=='commit_barrier').update(at_ns=d['input_ns']+3_000_000));self.reject()
    def test_release_before_button(self):
        self.mutate('round-1-B/observations.json',lambda d:next(e for e in d['events'] if e['event']=='barrier_released').update(at_ns=d['input_ns']+1_000_000));self.reject()
    def test_transition_before_delivery_barrier(self):
        self.mutate('round-1-C/observations.json',lambda d:next(e for e in d['events'] if e['event']=='delivery_barrier').update(at_ns=d['input_ns']+3_000_000));self.reject()
    def test_transition_input_after_return(self):
        self.mutate('round-1-D/observations.json',lambda d:next(e for e in d['events'] if e['event']=='native_transition_input').update(at_ns=d['input_ns']+40_000_000));self.reject()
    def test_feedback_before_button_with_consistent_report(self):
        def change(d):
            next(e for e in d['events'] if e['event']=='screenshot_received')['at_ns']=d['input_ns']+1_000_000
            d['feedback_upper_bound_ms']=1.0
        self.mutate('round-1-A/observations.json',change);self.reject()
    def test_nonfinite(self):
        for value in ('NaN','Infinity','1e999'):
            p=self.evidence/'invalid.json'; p.write_text('{"value":'+value+'}')
            with self.assertRaises(ValueError): v.load(p)
    def test_symlink(self):
        p=self.evidence/'fmt.log'; p.unlink(); p.symlink_to(self.source/'README.md'); self.reject()

if __name__ == '__main__': unittest.main()
