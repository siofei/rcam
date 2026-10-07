"""Synthetic preserve/report regressions. Requires an exact frozen product checkout.

No Mac process, display API or native acquisition is executed by this suite.
"""
import ast
from collections import Counter
import copy
from contextlib import nullcontext
from dataclasses import replace
import hashlib
import json
import math
import os
from pathlib import Path
import tempfile
import subprocess
import shutil
from types import ModuleType, SimpleNamespace
import unittest
from unittest.mock import patch

import current_refresh
import matrix
import supervise as guard
import test_matrix as old_matrix_tests
from performance_v1 import PerformanceReport, distribution

PRODUCT = Path(os.environ['RCAM_REPORT_TEST_ROOT']).resolve(strict=True)
matrix.verify_source(PRODUCT, matrix.require_pins())
import sys
sys.path.insert(0, str(PRODUCT / 'scripts'))
import report_pmix_v1 as pmix
import report_batch_v1 as batch
import test_verify_batch_drag_native as old_batch_tests
import test_pmix_owned_command as owned_tests
import test_pmix_bootstrap as bootstrap_tests
import test_pmix_frame_coverage as coverage_tests
import test_pmix_input_modifiers as modifier_tests


def dump(path, value):
    path.write_text(json.dumps(value, allow_nan=False) + '\n')


class Preserve(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory()
        self.addCleanup(self.temp.cleanup)
        self.native = Path(self.temp.name).resolve(strict=True)
        self.full = guard.FullRun(PRODUCT, self.native/'app', self.native/'producer', self.native/'output',
                                  'nav', 1, 2, *[getattr(matrix.PRODUCT_PINS, key) for key in
                                  ('manifest_sha', 'binary_sha', 'producer_sha', 'runner_sha')], display_policy='preserve')

    def preparation(self):
        initial = dict(display_id=2, mode_id=113, refresh_hz=144, width=1920, height=1080,
                       pixel_width=3840, pixel_height=2160, backing_scale=2, in_mirror_set=False)
        for index, label in enumerate(('display-before', 'display-active', 'display-active-probe')):
            receipt = dict(before=initial, after=initial)
            started = 1000 + index*100
            process = dict(schema_version=2, pid=12345, pgid=12345, private_session=True,
                           command=[*guard.DISPLAY_SWIFT_PREFIX, str(self.native/'display-probe.swift'), 'probe', '2'],
                           started_monotonic_ns=started, finished_monotonic_ns=started+50,
                           timeout_seconds=10, cleanup_grace_seconds=2, exit_code=0, joined=True,
                           owned_group_released=True, timed_out=False, error=None, signals_sent=[], signal=None, result='RETURNED')
            dump(self.native/(label+'.json'), receipt)
            dump(self.native/(label+'.subcommand.stdout'), receipt)
            dump(self.native/(label+'.subcommand-process.json'), process)
            dump(self.native/(label+'.subcommand-launch.json'), {key: process[key] for key in
                 ('pid','pgid','private_session','command','started_monotonic_ns','timeout_seconds')})
            (self.native/(label+'.subcommand.stderr')).write_text(owned_tests.phase_text('probe', receipt, started=started))
        return initial

    def test_preserve_nav_move_argv_and_strict_default(self):
        for mode in ('nav','move'):
            full = replace(self.full, mode=mode)
            command = guard.runner_command(PRODUCT, full.binary, full.producer, full.output, full)
            self.assertEqual(command[-4:], ['--display-id','2','--display-policy','preserve'])
            self.assertNotIn('--allow-display-mode-change', command)
            self.assertNotIn('--fixture', command)
        strict = replace(self.full, display_policy='frozen-60hz')
        self.assertEqual(guard.runner_command(PRODUCT, strict.binary, strict.producer, strict.output, strict)[-3:],
                         ['--display-policy','frozen-60hz','--allow-display-mode-change'])
        for fields in (dict(mode='points'), dict(round=2), dict(display_policy='unknown'), dict(display_id=True)):
            with self.subTest(fields=fields), self.assertRaises(RuntimeError): replace(self.full, **fields).validate()

    def test_real_product_phase_validators_accept_only_unchanged_probe_preparation(self):
        initial = self.preparation()
        self.assertEqual(guard.validate_app_preparation(self.native, PRODUCT, 'preserve'), initial)
        with self.assertRaises(RuntimeError): guard.validate_app_preparation(self.native, PRODUCT)

    def test_preparation_wrong_snapshot_auth_operation_and_phase_are_rejected(self):
        self.preparation()
        for key, value in (('backing_scale',1),('refresh_hz',60),('mode_id',114),('display_id',3),('in_mirror_set',True)):
            self.preparation()
            file = self.native/'display-active.json'
            receipt = json.loads(file.read_text()); receipt['after'][key] = value
            dump(file, receipt); dump(self.native/'display-active.subcommand.stdout', receipt)
            with self.subTest(key=key), self.assertRaises((RuntimeError, ValueError)):
                guard.validate_app_preparation(self.native, PRODUCT, 'preserve')
        self.preparation()
        for name in ('display-active.subcommand-launch.json','display-active.subcommand-process.json'):
            file = self.native/name; row=json.loads(file.read_text()); row['command'][-2]='set60'; dump(file,row)
        with self.assertRaises(RuntimeError): guard.validate_app_preparation(self.native, PRODUCT, 'preserve')
        self.preparation(); (self.native/'display-active.subcommand.stderr').write_text('')
        with self.assertRaises(ValueError): guard.validate_app_preparation(self.native, PRODUCT, 'preserve')

    def test_binding_auth_schema_nonce_scope_and_identity_reject(self):
        request = dict(mode='nav', round=1, display_id=2, display_policy='preserve',
                       display_mode_change_authorized=False, evidence_scope='capture-precheck-only',
                       run_id='22222222-2222-4222-8222-222222222222',source_manifest_sha256=self.full.manifest_sha)
        nonce='11111111-1111-4111-8111-111111111111'
        # Build complete early channels via the retained old synthetic fixture helper.
        for name in ('display-before','display-active','display-active-probe'):
            initial=dict(display_id=2,mode_id=113,refresh_hz=144,width=1920,height=1080,pixel_width=3840,pixel_height=2160,backing_scale=2,in_mirror_set=False)
            dump(self.native/(name+'.json'),dict(before=initial,after=initial))
        binding,digest=old_matrix_tests.phase_fixture(self.native,self.native,self.full,42,nonce,100,500,request)
        full=replace(self.full,output=self.native)
        guard.validate_runner_binding(binding,digest,request,full,nonce,42,50)
        for key,value in (('schema_version',2),('display_mode_change_authorized',True),('display_mode_change_authorized',0),
                          ('launch_nonce','33333333-3333-4333-8333-333333333333'),('evidence_scope','full-pmix-native'),
                          ('runner_binding_sha256','f'*64),('runner_pid',43),('round',True)):
            changed=dict(request);changed[key]=value
            with self.subTest(key=key,value=value),self.assertRaises(RuntimeError):
                guard.validate_runner_binding(binding,digest,changed,full,nonce,42,50)
        for key in ('launch_nonce','binary_sha256','source_manifest_sha256','runner_sha256'):
            changed=dict(binding);changed[key]='f'*64
            with self.subTest(binding=key),self.assertRaises(RuntimeError):
                guard.validate_runner_binding(changed,digest,request,full,nonce,42,50)

    def test_owned_phase_accepts_preserve_without_setting_and_binds_original_launch_clock(self):
        full=replace(self.full,output=self.native)
        nonce='11111111-1111-4111-8111-111111111111'
        request=dict(mode='nav',round=1,display_id=2,display_policy='preserve',display_mode_change_authorized=False,
                     evidence_scope='capture-precheck-only',run_id='22222222-2222-4222-8222-222222222222',
                     source_manifest_sha256=full.manifest_sha)
        self.preparation()
        old_matrix_tests.phase_fixture(self.native,self.native,full,42,nonce,100,2000,request)
        self.preparation()  # Replace old synthetic set60 journal with real-validator probe data.
        local=self.native/'external';local.mkdir()
        phase=guard.OwnedLaunchPhase(local,full,nonce,42,50)
        stream=SimpleNamespace(runner_bound=True,runner_credential=dict(pid=42),last_valid_sample=dict(seq=3),
                               policy=guard.GuardPolicy(require_app_launch=True))
        with patch.object(guard,'system_uptime_ns',return_value=3000):phase.poll(stream)
        self.assertEqual(stream.policy.app_launch_ns,2000)
        with patch.object(guard,'system_uptime_ns',return_value=4000):phase.poll(stream)
        self.assertEqual(stream.policy.app_launch_ns,2000)

    def test_discovery_schema4_preserve_auth_false_and_exact_target(self):
        self.full.binary.write_bytes(b'synthetic-only-app')
        request=dict(schema_version=4,mode='nav',round=1,display_id=2,display_policy='preserve',
                     display_mode_change_authorized=False,evidence_scope='capture-precheck-only',
                     source_manifest_sha256=self.full.manifest_sha,run_id='22222222-2222-4222-8222-222222222222')
        dump(self.native/'request.json',request)
        dump(self.native/'owned-process.json',dict(pid=43,binary_sha256=self.full.binary_sha,command=[str(self.full.binary)]))
        match=guard.discover_native(set(),self.full.binary,full=self.full,known_native=self.native)
        self.assertEqual(match[0],self.native)
        request['display_mode_change_authorized']=True;dump(self.native/'request.json',request)
        with self.assertRaises(RuntimeError):guard.discover_native(set(),self.full.binary,full=self.full,known_native=self.native)


class PreserveRawReplay(unittest.TestCase):
    def fixture(self, mode='nav'):
        old = old_matrix_tests.MatrixContract(); old.setUp(); self.addCleanup(old.temp.cleanup)
        directory, request, receipt, _ = old.fixture((mode,1))
        original = json.loads((directory/'display-before.json').read_text())['after']
        for name in ('display-before','display-active','display-active-probe','display-restored','display-restored-probe'):
            dump(directory/(name+'.json'),dict(before=original,after=original))
        request.update(display_policy='preserve',display_mode_change_authorized=False,evidence_scope='capture-precheck-only')
        dump(directory/'request.json',request)
        (directory/'runner-request.raw.json').write_bytes((directory/'request.json').read_bytes())
        launch=json.loads((directory/'runner-launch.json').read_text())
        launch['command'].remove('--allow-display-mode-change'); launch['command'][-1]='preserve'
        dump(directory/'runner-launch.json',launch)
        receipt['display_policy']='preserve'
        return directory,request,receipt

    def test_actual_raw_replay_for_both_preserve_modes(self):
        for mode in ('nav','move'):
            directory,_,receipt=self.fixture(mode)
            seen=set();matrix.validate_round(directory,(mode,1),receipt,seen,None,display_policy='preserve')
            self.assertEqual(seen,{receipt['run_id']})
            with self.assertRaises(RuntimeError):matrix.validate_round(directory,(mode,1),receipt,set(),None)

    def test_replay_rejects_extra_authorization_and_nine_field_snapshot_changes(self):
        directory,request,receipt=self.fixture()
        for name in ('display-active','display-restored'):
            file=directory/(name+'.json'); original=file.read_bytes()
            changed=json.loads(original);changed['after'].pop('backing_scale');dump(file,changed)
            with self.subTest(name=name),self.assertRaises(RuntimeError):
                matrix.validate_round(directory,('nav',1),receipt,set(),None,display_policy='preserve')
            file.write_bytes(original)
        launch=json.loads((directory/'runner-launch.json').read_text());launch['command'].append('--allow-display-mode-change')
        dump(directory/'runner-launch.json',launch)
        with self.assertRaises(RuntimeError):matrix.validate_round(directory,('nav',1),receipt,set(),None,display_policy='preserve')

    def test_replay_nonce_and_raw_hid_change_fail(self):
        for fault in ('nonce','hid'):
            directory,_,receipt=self.fixture()
            file=directory/'monitor.stdout';rows=[json.loads(line) for line in file.read_text().splitlines()]
            if fault=='nonce':rows[-1]['nonce']='ffffffff-ffff-4fff-8fff-ffffffffffff'
            else:
                row=rows[-1]['sources']['hid']['keyDown'];row['count_before']+=1;row['count_after']+=1
            file.write_text(''.join(json.dumps(row)+'\n' for row in rows))
            with self.subTest(fault=fault),self.assertRaises(RuntimeError):
                matrix.validate_round(directory,('nav',1),receipt,set(),None,display_policy='preserve')


class CopiedFunctionalPredicates(unittest.TestCase):
    def test_extra_product_adapter_shadows_cannot_override_external_copies(self):
        with tempfile.TemporaryDirectory() as temporary:
            root=Path(temporary).resolve(strict=True)
            shutil.copytree(PRODUCT/'scripts',root/'scripts')
            for name in ('report_pmix_v1','report_batch_v1','performance_v1'):
                (root/'scripts'/(name+'.py')).write_text("raise RuntimeError('unreviewed shadow executed')\n")
            external=Path(__file__).parent.resolve()
            code="import sys; from pathlib import Path; sys.path.insert(0,sys.argv[1]); import current_refresh; verify=current_refresh.report_verifier(Path(sys.argv[2])); assert verify.__module__=='report_pmix_v1'"
            result=subprocess.run([sys.executable,'-B','-c',code,str(external),str(root)],capture_output=True,text=True)
            self.assertEqual(result.returncode,0,result.stderr)

    def test_product_adapter_fixture_accepts_real_directory_alias_after_canonicalization(self):
        with tempfile.TemporaryDirectory() as temporary:
            root=Path(temporary).resolve(strict=True)
            physical=root/'physical-product';physical.mkdir()
            alias=root/'product-alias';alias.symlink_to(physical,target_is_directory=True)
            self.assertNotEqual(alias,alias.resolve(strict=True))
            with patch.object(tempfile,'TemporaryDirectory',return_value=nullcontext(str(alias))):
                self.test_extra_product_adapter_shadows_cannot_override_external_copies()

    def test_real_alias_does_not_allow_wrong_source_shadow_to_execute(self):
        with tempfile.TemporaryDirectory() as temporary:
            root=Path(temporary).resolve(strict=True)
            scripts=root/'product/scripts';scripts.mkdir(parents=True)
            (scripts/'verify_s5m2_evidence.py').write_text('')
            product_alias=root/'product-alias';product_alias.symlink_to(root/'product',target_is_directory=True)
            shadow=root/'shadow';shadow.mkdir()
            (shadow/'verify_s5m2_evidence.py').write_text("raise RuntimeError('unreviewed shadow executed')\n")
            shadow_alias=root/'shadow-alias';shadow_alias.symlink_to(shadow,target_is_directory=True)
            external=Path(__file__).parent.resolve(strict=True)
            code="""import sys
from pathlib import Path
sys.path.insert(0,sys.argv[1])
import current_refresh
sys.path.insert(0,sys.argv[2])
expected=(Path(sys.argv[3])/'scripts/verify_s5m2_evidence.py').resolve(strict=True)
try:
    current_refresh.bound_import('verify_s5m2_evidence',expected)
except RuntimeError as error:
    assert str(error)=='shadow module: verify_s5m2_evidence',str(error)
else:
    raise AssertionError('wrong source shadow accepted')
"""
            result=subprocess.run([sys.executable,'-B','-c',code,str(external),str(shadow_alias),str(product_alias)],
                                  capture_output=True,text=True)
            self.assertEqual(result.returncode,0,result.stderr)

    def test_cached_wrong_external_module_origin_rejected_before_execution(self):
        with tempfile.TemporaryDirectory() as temporary:
            file=Path(temporary)/'report_pmix_v1.py';file.write_text('')
            fake=ModuleType('report_pmix_v1');fake.__file__=str(file)
            with patch.dict(sys.modules,report_pmix_v1=fake),self.assertRaisesRegex(RuntimeError,'cached module'):
                current_refresh.report_verifier(PRODUCT)

    def test_exact_frozen_provenance_and_only_five_explicit_scope_timing_assertions_replaced(self):
        def calls(path):
            return Counter(ast.dump(node,include_attributes=False) for node in ast.walk(ast.parse(path.read_text()))
                           if isinstance(node,ast.Call) and isinstance(node.func,ast.Name) and node.func.id=='require')
        root=Path(__file__).parent
        expected={'verify_pmix_native.py':'c379e18249a2d7969b26142388340f79d2aa32875f1bc6e79417857d03fad06b',
                  'verify_batch_drag_native.py':'c872e0eb84a572a4f94dff4507fd397a1d271f1b44fb2df3396b13d1fa11f69e'}
        for name,digest in expected.items():self.assertEqual(guard.sha(PRODUCT/'scripts'/name),digest)
        original=ast.parse((PRODUCT/'scripts/verify_pmix_native.py').read_text())
        allowed=Counter()
        for node in ast.walk(original):
            if isinstance(node,ast.Call) and isinstance(node.func,ast.Name) and node.func.id=='require' and node.lineno in (487,489,567,601,605):
                allowed[ast.dump(node,include_attributes=False)]+=1
        self.assertEqual(sum(allowed.values()),5)
        self.assertEqual(calls(PRODUCT/'scripts/verify_pmix_native.py')-calls(root/'report_pmix_v1.py'),allowed)
        self.assertFalse(calls(PRODUCT/'scripts/verify_batch_drag_native.py')-calls(root/'report_batch_v1.py'))
        original_funcs={node.name:node for node in original.body if isinstance(node,ast.FunctionDef)}
        adapted_funcs={node.name:node for node in ast.parse((root/'report_pmix_v1.py').read_text()).body if isinstance(node,ast.FunctionDef)}
        for name in original_funcs.keys()-{'verify','bootstrap_source_contract','bind_worker_requests'}:
            self.assertEqual(ast.dump(original_funcs[name]),ast.dump(adapted_funcs[name]),name)

    def test_geometry_finite_and_1e9_tolerance_kept(self):
        pmix.point(dict(x_mm=1.,y_mm=2.),1.,2.)
        for value in (1.000000002,float('nan'),float('inf')):
            with self.assertRaises(ValueError):pmix.point(dict(x_mm=value,y_mm=2.),1.,2.)

    def test_adapted_frame_coverage_and_paint_binding_reject_corruption(self):
        values=coverage_tests.fixture();pmix.bind_frame_producers(*values)
        values[0]['terminal_frame']['counters']['draw']+=1
        with self.assertRaises(ValueError):pmix.bind_frame_producers(*values)

    def test_adapted_constructor_nonce_clock_and_full_worker_versions_kept(self):
        values=bootstrap_tests.fixture()
        pmix.bind_worker_requests(*values,PRODUCT/'MANIFEST.sha256',PRODUCT)
        values[0]['worker'][0]['receipt']['input']['generation']=1
        with self.assertRaises(ValueError):pmix.bind_worker_requests(*values,PRODUCT/'MANIFEST.sha256',PRODUCT)

    def test_actual_drag_modifier_contract_rejects_changed_processed_input(self):
        fixture=modifier_tests.InputModifiers();fixture.setUp()
        report,frames=fixture.fixture('move');pmix.verify_input_modifiers(report,frames,fixture.policy)
        frames[3]['processed_modifiers']['alt']=False
        with self.assertRaises(ValueError):pmix.verify_input_modifiers(report,frames,fixture.policy)


class TimingReport(unittest.TestCase):
    def test_budget_failure_does_not_hide_later_single_commit_assertion(self):
        # Execute the exact unchanged commit/history predicate after report-only
        # comparisons, extracted structurally from the reviewed external copy.
        tree=ast.parse(Path(pmix.__file__).read_text())
        verify=next(node for node in tree.body if isinstance(node,ast.FunctionDef) and node.name=='verify')
        nodes=[node for node in ast.walk(verify) if isinstance(node,ast.For) and isinstance(node.target,ast.Tuple)
               and [getattr(part,'id',None) for part in node.target.elts]==['label','trigger','action','step','u','z']]
        self.assertEqual(len(nodes),1)
        # This block calls the original completion binding first, then checks all
        # three transaction/revision/history steps, before manufacturing checks.
        compiled=compile(ast.fix_missing_locations(ast.Module(body=nodes,type_ignores=[])),pmix.__file__,'exec')
        baseline=dict(document_id='d',revision='0',undo=0)
        labels={'commit-complete':dict(document_id='d',revision='1',undo=1,redo=0),
                'undo-complete':dict(document_id='d',revision='2',undo=0,redo=1),
                'redo-complete':dict(document_id='d',revision='3',undo=1,redo=0)}
        performance=PerformanceReport();performance.compare('frame_max_ms',601,200)
        def environment():
            return dict(completion=lambda label,trigger,action:dict(state=labels[label],visible_frame_id=1,duration_ms=601),
                        baseline=baseline,require=pmix.require,indexed={1:dict(completed_ns=601_000_000,input_ns=1)},
                        event=lambda label:dict(frame_id=1),performance=performance,summary={})
        values=environment();exec(compiled,values)
        self.assertEqual(len(performance.comparisons),4)
        self.assertTrue(all(row['result']=='EXCEEDED' for row in performance.comparisons))
        for label in labels:
            original=dict(labels[label]);labels[label]['undo']+=1
            with self.subTest(label=label),self.assertRaisesRegex(ValueError,'one transaction/revision history'):
                exec(compiled,environment())
            labels[label]=original

    def test_distribution_and_explicit_exceeded_without_performance_pass(self):
        report=PerformanceReport();report.samples('interval',[10,20,51,201,567])
        report.compare('frame_max_ms',567,200)
        result=report.result();values=result['distributions']['interval']
        self.assertEqual([values[key] for key in ('p50_ms','p95_ms','p99_ms','max_ms')],[51,567,567,567])
        self.assertEqual(values['over_50ms_count'],3);self.assertEqual(values['over_200ms_count'],2)
        self.assertEqual(result['historical_budget_comparisons'][0]['result'],'EXCEEDED')
        self.assertEqual(result['smoothness'],'UNVERIFIED');self.assertEqual(result['flicker'],'UNVERIFIED')

    def test_nonfinite_zero_negative_and_bool_timing_reject(self):
        for value in (0,-1,float('nan'),float('inf'),True,'10'):
            with self.subTest(value=value),self.assertRaises(ValueError):distribution([value])
        self.assertEqual(distribution([0,1],positive=False)['max_ms'],1)

    def test_preview_over_200ms_continues_functional_checks_while_strict_rejects(self):
        fixture=old_batch_tests.Verify();fixture.setUp();self.addCleanup(fixture.tearDown)
        report=fixture.report
        frames=report['frames'];removed=set(range(50,57));mapping={}
        kept=[frame for frame in frames if frame['id'] not in removed]
        for number,frame in enumerate(kept,1):
            mapping[frame['id']]=number;frame['id']=number
            frame['counters'].update(draw=number,**{'uniform-upload':112*number})
            frame['frame_interval_ms']=(frame['input_ns']-kept[number-2]['input_ns'])/1e6 if number>1 else None
        for event in report['events']:event['frame_id']=mapping[event['frame_id']]
        report.update(frames=kept,last_observed_frame_id=len(kept));report['counters'].update(draw=len(kept),**{'uniform-upload':112*len(kept)})
        protocol=json.loads(old_batch_tests.PROTOCOL.read_text())
        performance=PerformanceReport();batch.bind_frames(report,protocol,performance=performance)
        self.assertEqual(performance.comparisons[0]['result'],'EXCEEDED')
        with self.assertRaisesRegex(ValueError,'stall'):batch.bind_frames(report,protocol)
        kept[60]['state']['delta']['x_mm']+=.000000002
        with self.assertRaises(ValueError):batch.bind_frames(report,protocol,performance=PerformanceReport())


class ThinDriver(unittest.TestCase):
    def exercise(self, fault=None, case=None):
        temp=tempfile.TemporaryDirectory();self.addCleanup(temp.cleanup)
        home=Path(temp.name).resolve();root=home/'product';root.mkdir()
        binary=home/'app';binary.write_bytes(b'unit-only-app')
        producer=home/'producer';producer.write_bytes(b'unit-only-producer')
        pins=matrix.ProductPins('a'*40,'b'*64,guard.sha(binary),guard.sha(producer),'c'*64)
        evidence=home/'evidence';calls=[];phases=[]
        class Sentinel:
            def __init__(self,*args):pass
            def start(self):phases.append('start')
            def pump(self):return None
            def finish(self):phases.append('finish');return fault!='tail'
        sentinel=Sentinel()
        def owned_run(local,*,full,continuous_guard,compiled_monitor):
            self.assertEqual(full.display_policy,'preserve');self.assertEqual(full.round,1)
            self.assertEqual(continuous_guard,sentinel.pump)
            self.assertEqual(compiled_monitor,(home/'unit-monitor','d'*64))
            calls.append(full.mode);full.output.mkdir()
            receipt=dict(success=fault!='guard',runner_joined_at_ns=200,
                         run_id='11111111-1111-4111-8111-'+('111111111111' if full.mode=='nav' else '222222222222'))
            guard.write(local/'SUPERVISOR_RESULT.json',receipt)
            guard.write(local/'runner-launch.json',dict(started_uptime_ns=100))
            return 2 if fault=='guard' else 0
        def watched(command,root,local,label,watched_sentinel):
            self.assertIs(watched_sentinel,sentinel)
            self.assertEqual(command[3],'report');self.assertEqual(label,'functional-report')
            inputs=guard.strict_json((local/'report-input.json').read_bytes())
            receipt=guard.strict_json((local/'SUPERVISOR_RESULT.json').read_bytes())
            report=dict(scope=current_refresh.SCOPE,functional_result='FAIL' if fault=='functional' else 'PASS',
                        input_sha256=guard.sha(local/'report-input.json'),supervisor_receipt_sha256=inputs['receipt_sha256'],
                        directory=inputs['directory'],run_id=receipt['run_id'],performance=dict(status='MEASURED_REPORT_ONLY',
                        historical_budget_comparisons=[dict(result='EXCEEDED',actual_ms=601,historical_limit_ms=200)]))
            guard.write(local/'REPORT.json',report)
            if fault=='functional':raise RuntimeError('owned verification failed: functional-report')
        with patch.object(matrix,'PRODUCT_PINS',pins),patch.object(matrix,'verify_source',return_value={}), \
             patch.object(guard.sys,'platform','darwin'),patch.object(guard,'system_uptime_ns',return_value=100), \
             patch.object(matrix,'compile_monitor',return_value=(home/'unit-monitor','d'*64)), \
             patch.object(matrix,'ContinuousSentinel',return_value=sentinel),patch.object(guard,'run',side_effect=owned_run), \
             patch.object(matrix,'watched_command',side_effect=watched), \
             patch.object(matrix,'verify_continuous_evidence',side_effect=RuntimeError('raw corruption') if fault=='replay' else None,return_value=True), \
             patch.object(matrix,'assess_and_recover',side_effect=AssertionError('preserve must never restore')), \
             patch.object(matrix,'command_aggregate',side_effect=AssertionError('no aggregate')):
            selection = {} if case is None else {'case': case}
            code=current_refresh.run(root=root,binary=binary,producer=producer,evidence=evidence,**selection)
        result=guard.strict_json((evidence/'CURRENT_REFRESH_RESULT.json').read_bytes())
        return code,result,calls,phases,evidence

    def test_nav_then_move_each_fresh_and_exceeded_budgets_remain_report_only(self):
        code,result,calls,phases,_=self.exercise()
        self.assertEqual(code,0);self.assertEqual(calls,['nav','move']);self.assertEqual(phases,['start','finish'])
        self.assertEqual(result['functional_result'],'PASS');self.assertFalse(result['overall_PASS_claim'])
        self.assertEqual(result['performance'],'REPORT_ONLY');self.assertEqual(result['flicker'],'UNVERIFIED')
        self.assertFalse(result['display_mode_change_authorized'])
        self.assertEqual(result['case_selection'],'all')
        self.assertEqual(result['requested_cases'],['nav1','move1'])
        self.assertEqual(result['completed_cases'],['nav1','move1'])

    def test_explicit_single_case_completes_only_requested_case(self):
        for case,mode in (('move1','move'),('nav1','nav')):
            with self.subTest(case=case):
                code,result,calls,phases,evidence=self.exercise(case=case)
                self.assertEqual(code,0);self.assertEqual(calls,[mode]);self.assertEqual(phases,['start','finish'])
                self.assertEqual(result['execution_result'],'COMPLETE')
                self.assertEqual(result['case_selection'],case)
                self.assertEqual(result['requested_cases'],[case]);self.assertEqual(result['completed_cases'],[case])
                self.assertEqual(len(result['runs']),1)
                self.assertTrue(result['continuous_input_guard']);self.assertTrue(result['continuous_raw_replay'])
                self.assertFalse(result['overall_PASS_claim']);self.assertFalse(result['stage_PASS_claim'])
                self.assertFalse((evidence/('nav1' if mode=='move' else 'move1')).exists())

    def test_single_case_guard_report_tail_and_replay_failure_remain_session_fail(self):
        for case in ('move1','nav1'):
            for fault in ('guard','functional','tail','replay'):
                with self.subTest(case=case,fault=fault):
                    code,result,calls,phases,_=self.exercise(fault,case=case)
                    self.assertEqual(code,2);self.assertEqual(calls,[case[:-1]]);self.assertEqual(phases,['start','finish'])
                    self.assertEqual(result['execution_result'],'FAIL');self.assertEqual(result['functional_result'],'FAIL')
                    self.assertEqual(result['requested_cases'],[case])
                    self.assertEqual(result['completed_cases'],[] if fault in ('guard','functional') else [case])
                    self.assertFalse(result['overall_PASS_claim'])

    def test_explicit_all_keeps_original_nav_then_move_behavior(self):
        code,result,calls,phases,_=self.exercise(case='all')
        self.assertEqual(code,0);self.assertEqual(calls,['nav','move']);self.assertEqual(phases,['start','finish'])
        self.assertEqual(result['requested_cases'],['nav1','move1'])
        self.assertEqual(result['completed_cases'],['nav1','move1'])

    def test_invalid_case_rejects_before_filesystem_compiler_or_guard_changes(self):
        for case in ('move2','nav2','move','',None,True,1,('move1',)):
            with self.subTest(case=case),patch.object(matrix,'verify_source') as verify_source, \
                 patch.object(matrix,'compile_monitor') as compiler,patch.object(matrix,'ContinuousSentinel') as sentinel, \
                 self.assertRaisesRegex(RuntimeError,'case selection'):
                current_refresh.run(root=PRODUCT,binary=PRODUCT/'never-app',producer=PRODUCT/'never-producer',
                                    evidence=PRODUCT/'never-evidence',case=case)
            verify_source.assert_not_called();compiler.assert_not_called();sentinel.assert_not_called()

    def test_cli_accepts_only_named_cases_and_defaults_to_all(self):
        base=['current_refresh.py','run','--root','product','--binary','app','--producer','producer','--evidence','fresh']
        for case in (None,'all','nav1','move1'):
            argv=base if case is None else base+['--case',case]
            with self.subTest(case=case),patch.object(sys,'argv',argv),patch.object(current_refresh,'run',return_value=0) as run:
                self.assertEqual(current_refresh.main(),0)
                self.assertEqual(run.call_args.kwargs['case'],'all' if case is None else case)
        for case in ('nav','move2',''):
            with self.subTest(case=case),patch.object(sys,'argv',base+['--case',case]), \
                 patch.object(current_refresh,'run') as run,patch.object(sys,'stderr'),self.assertRaises(SystemExit) as exit:
                current_refresh.main()
            self.assertEqual(exit.exception.code,2);run.assert_not_called()

    def test_guard_and_function_failure_stop_before_move_with_original_failure_kept(self):
        for fault in ('guard','functional'):
            code,result,calls,phases,evidence=self.exercise(fault)
            self.assertEqual(code,2);self.assertEqual(calls,['nav']);self.assertEqual(phases,['start','finish'])
            self.assertEqual(result['functional_result'],'FAIL')
            if fault=='functional':
                report=guard.strict_json((evidence/'nav1/REPORT.json').read_bytes())
                self.assertEqual(report['functional_result'],'FAIL')
                self.assertEqual(report['performance']['historical_budget_comparisons'][0]['result'],'EXCEEDED')

    def test_final_continuous_failure_cannot_grant_functional_or_overall_pass(self):
        code,result,calls,_,_=self.exercise('tail')
        self.assertEqual(code,2);self.assertEqual(calls,['nav','move'])
        self.assertFalse(result['continuous_input_guard']);self.assertEqual(result['execution_result'],'FAIL')

    def test_final_raw_continuous_replay_corruption_fails_completion(self):
        code,result,calls,_,_=self.exercise('replay')
        self.assertEqual(code,2);self.assertEqual(calls,['nav','move'])
        self.assertFalse(result['continuous_raw_replay']);self.assertIn('raw corruption',result['failure'])

    def test_report_maps_functional_assertion_to_explicit_fail_retaining_exceedance(self):
        with tempfile.TemporaryDirectory() as temporary:
            home=Path(temporary);directory=home/'native';directory.mkdir();local=home/'guard';local.mkdir()
            binary=home/'app';binary.write_bytes(b'unit-app');producer=home/'producer';producer.write_bytes(b'unit-producer')
            pins=matrix.ProductPins('a'*40,'b'*64,guard.sha(binary),guard.sha(producer),'c'*64)
            dump(directory/'request.json',dict(mode='move',round=1))
            dump(local/'runner-launch.json',dict(root=str(PRODUCT),command=['--binary',str(binary),'--capture-producer',str(producer)],
                product_pins=dict(source_manifest_sha256=pins.manifest_sha,binary_sha256=pins.binary_sha,
                                  capture_producer_sha256=pins.producer_sha,runner_sha256=pins.runner_sha)))
            def failed_verify(*args,**kwargs):
                kwargs['performance'].compare('frame_max_ms',601,200)
                raise ValueError('one transaction/revision history')
            with patch.object(matrix,'PRODUCT_PINS',pins),patch.object(matrix,'verify_source'), \
                 patch.object(matrix,'validate_round'),patch.object(current_refresh,'report_verifier',return_value=failed_verify):
                result=current_refresh.report(PRODUCT,directory,local,{},[])
            self.assertEqual(result['functional_result'],'FAIL')
            self.assertIn('one transaction',result['functional_error'])
            self.assertEqual(result['performance']['historical_budget_comparisons'][0]['result'],'EXCEEDED')
            self.assertFalse(result['overall_PASS_claim'])


if __name__=='__main__':
    unittest.main()
