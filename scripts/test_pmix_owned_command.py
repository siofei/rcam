"""Real owned-Python process regressions; all native adapters are synthetic, no GUI."""
import contextlib
import copy
import io
import hashlib
import json
import os
from pathlib import Path
import signal
import shutil
import subprocess
import sys
import tempfile
import time
import unittest
from unittest.mock import patch

from pmix_owned_command import (owned_command, verify_command, verify_display_environment, display_core,
    validate_display_phases, json_bytes, publish_runner_marker, runner_clock_ns, CLOCK_DOMAIN)
import run_pmix_native as runner


NONCE='11111111-1111-4111-8111-111111111111'

def phase_rows(operation,receipt,pid=12345,started=100):
    # Invented diagnostic data. This never executes CoreGraphics or AppKit.
    core=lambda prefix:[prefix+'/'+call+'-'+edge for call in ('online','mode','mirror') for edge in ('enter','return')]
    if operation=='probe':
        full=lambda prefix:core(prefix)+[prefix+'/screen-enter',prefix+'/screen-return']
        names=['runtime-enter','appkit-enter','appkit-return']+full('snapshot-before')+full('snapshot-after')+['receipt-return']
    else:
        names=['runtime-enter']+core('snapshot-before')+['modes-enter','modes-return','mode-selected',
            'begin-enter','begin-return','configure-enter','configure-return','complete-enter','complete-return']+core('snapshot-after')+['receipt-return']
    rows=[];selected=receipt['after']['mode_id'] if operation!='probe' else None
    for index,name in enumerate(names,1):
        value=None
        for prefix,side in (('snapshot-before','before'),('snapshot-after','after')):
            if name==prefix+'/online-return':value=1
            if name==prefix+'/mode-return':value=receipt[side]['mode_id']
            if name==prefix+'/mirror-return':value=int(receipt[side]['in_mirror_set'])
        if name=='appkit-return':value=2
        if name=='modes-return':value=5
        if name=='mode-selected':value=selected
        rows.append({'schema_version':1,'event':'display-phase','producer_pid':pid,'uid':501,'thread_main':True,
                     'clock_domain':CLOCK_DOMAIN,'at_ns':started+index,'sequence':index,'operation':operation,
                     'display_id':2,'phase':name,'cg_error':0 if name in ('begin-return','configure-return','complete-return') else None,
                     'value':value,'selected_mode_id':selected if operation!='probe' and index>=names.index('mode-selected')+1 else None})
    return rows

def phase_text(operation,receipt,pid=12345,started=100):
    return ''.join(json.dumps(row)+'\n' for row in phase_rows(operation,receipt,pid,started))


class OwnedCommands(unittest.TestCase):
    def setUp(self):
        self.temp=tempfile.TemporaryDirectory(prefix='rcam-owned-command-')
        self.root=Path(self.temp.name)
    def tearDown(self):
        retained=os.environ.get('RCAM_OWNED_COMMAND_TEST_OUT')
        if retained:
            destination=Path(retained)/self.id().split('.')[-1];destination.parent.mkdir(parents=True,exist_ok=True)
            shutil.copytree(self.root,destination)
        self.temp.cleanup()
    def command(self,program):return [sys.executable,'-c',program]
    def receipt(self,label):return json.loads((self.root/(label+'.subcommand-process.json')).read_text())
    def absent(self,pid):
        try:os.kill(pid,0)
        except ProcessLookupError:return True
        return False

    def test_actual_normal_raw_and_portable_bindings(self):
        command=self.command('import sys; print("owned-normal",flush=True); print("raw-error",file=sys.stderr,flush=True)')
        result=owned_command(self.root,'window-query',command,timeout=1)
        row=verify_command(self.root,'window-query',command,0,result.stdout,result.stderr)
        self.assertTrue(row['owned_group_released']);self.assertTrue(self.absent(row['pid']))
        self.assertEqual(result.stdout,'owned-normal\n');self.assertEqual(result.stderr,'raw-error\n')
        for key,value in [('timed_out',True),('owned_group_released',False),('pgid',os.getpgrp()),('timeout_seconds',999)]:
            changed=dict(row);changed[key]=value;(self.root/'window-query.subcommand-process.json').write_text(json.dumps(changed))
            with self.assertRaises(ValueError):verify_command(self.root,'window-query',command,0,result.stdout,result.stderr)
        (self.root/'window-query.subcommand-process.json').write_text(json.dumps(row))
        (self.root/'window-query.subcommand.stdout').write_text('resealed')
        with self.assertRaisesRegex(ValueError,'raw output'):verify_command(self.root,'window-query',command,0,result.stdout,result.stderr)

    def test_actual_timeout_term_refusal_kill_and_raw_retention(self):
        command=self.command('import signal,sys; signal.signal(signal.SIGTERM,signal.SIG_IGN); print("before-timeout",flush=True); print("timeout-stderr",file=sys.stderr,flush=True); signal.pause()')
        start=time.monotonic()
        with self.assertRaisesRegex(ValueError,'timeout/failure/cleanup'):
            owned_command(self.root,'image',command,timeout=.25,grace=.2)
        row=self.receipt('image')
        self.assertLess(time.monotonic()-start,1.5);self.assertTrue(row['timed_out'])
        self.assertEqual(row['signals_sent'],[15,9]);self.assertEqual(row['exit_code'],-9)
        self.assertTrue(row['joined']);self.assertTrue(row['owned_group_released']);self.assertTrue(self.absent(row['pid']))
        self.assertEqual((self.root/'image.subcommand.stdout').read_text(),'before-timeout\n')
        self.assertEqual((self.root/'image.subcommand.stderr').read_text(),'timeout-stderr\n')

    def test_actual_descendant_cleanup_leaves_other_owned_sentinel_running(self):
        sentinel=subprocess.Popen(self.command('import signal; signal.pause()'),start_new_session=True)
        try:
            child_code='import signal; signal.pause()'
            program='\n'.join(['import os,signal,subprocess,sys,json',f'child=subprocess.Popen({self.command(child_code)!r})',
                'def stop(sig,frame):',' child.wait(timeout=1)',' sys.exit(0)',
                'signal.signal(signal.SIGTERM,stop)','print(json.dumps({"child":child.pid,"pgid":os.getpgid(child.pid)}),flush=True)','signal.pause()'])
            with self.assertRaisesRegex(ValueError,'timeout/failure/cleanup'):
                owned_command(self.root,'window-query',self.command(program),timeout=.3,grace=1)
            row=self.receipt('window-query');child=json.loads((self.root/'window-query.subcommand.stdout').read_text())
            self.assertEqual(child['pgid'],row['pgid']);self.assertNotEqual(row['pgid'],os.getpgid(sentinel.pid))
            self.assertTrue(row['joined']);self.assertTrue(row['owned_group_released'])
            self.assertTrue(self.absent(child['child']));self.assertIsNone(sentinel.poll())
        finally:
            sentinel.terminate();sentinel.wait(timeout=2)

    def test_nonzero_and_spawn_failure_still_record_stdio_and_join(self):
        command=self.command('import sys; print("before-error"); sys.exit(23)')
        with self.assertRaises(subprocess.CalledProcessError):owned_command(self.root,'environment-os',command,check=True)
        row=self.receipt('environment-os');self.assertEqual(row['exit_code'],23);self.assertTrue(row['joined'])
        self.assertEqual((self.root/'environment-os.subcommand.stdout').read_text(),'before-error\n')
        with self.assertRaises(ValueError):owned_command(self.root,'environment-memory',['/nonexistent/rcam-owned-command'])
        row=self.receipt('environment-memory');self.assertIsNone(row['pid']);self.assertFalse(row['joined']);self.assertEqual(row['result'],'FAIL')
        self.assertTrue((self.root/'environment-memory.subcommand.stdout').exists())

    def test_runner_hung_subcommand_reaches_cleanup_preservation_and_evidence(self):
        # Simulate the Darwin gate; actual adapters execute only owned Python fixtures.
        # No SDK/display/window/screenshot call runs and this is never native proof.
        for failure in ('window-query','image','display-before','environment-os','display-restored','producer-stderr-descendant','reader-start-failure'):
            with self.subTest(failure=failure):
                case=self.root/failure;case.mkdir();binary=case/'synthetic-app';producer=case/'synthetic-producer'
                binary.write_text('#!'+sys.executable+'\n'+'''import json,os,signal
from pathlib import Path
root=Path(os.environ['RCAM_PMIX_NATIVE_DIR']);request=json.loads((root/'request.json').read_text())
(root/'window-ready.json').write_text(json.dumps({'app_pid':os.getpid(),'run_id':request['run_id'],'frame_id':1}))
signal.pause()
''');binary.chmod(0o700);producer.write_bytes(b'never-executed synthetic producer')
                if failure in ('producer-stderr-descendant','reader-start-failure'):
                    program='\n'.join(['import json,os,signal,subprocess,sys',
                        'child=subprocess.Popen('+repr(self.command('import signal; signal.pause()'))+',stdout=subprocess.DEVNULL)',
                        'print(json.dumps({"synthetic_owned_stderr_descendant":child.pid}),file=sys.stderr,flush=True)'])
                    if failure=='reader-start-failure':program+='\nsignal.pause()\n'
                    producer.write_text('#!'+sys.executable+'\n'+program+'\n');producer.chmod(0o700)
                calls=[];snapshot={'display_id':2,'mode_id':113,'refresh_hz':144,'width':1920,'height':1080,'pixel_width':3840,'pixel_height':2160,'backing_scale':2,'in_mirror_set':False};display={'before':dict(snapshot),'after':dict(snapshot)}
                def adapter(directory,label,command,**kwargs):
                    calls.append(label)
                    if label==failure or (failure=='display-restored' and label=='window-query'):
                        return owned_command(directory,label,self.command('import signal; print("synthetic-hung-adapter",flush=True); signal.pause()'),timeout=.2,grace=.2)
                    if label.startswith('display-'):value=json.dumps(display)
                    elif label.startswith('environment-'):value={'environment-os':'synthetic-test-os','environment-machine':'arm64','environment-memory':'17179869184','environment-power':'synthetic-test-power'}[label]
                    elif label=='window-query':value=json.dumps([{'window_id':456,'owner_pid':int(command[2]),'layer':0,'bounds':{'Width':2,'Height':2,'X':0,'Y':0}}])
                    elif label=='image':
                        Path(command[-1]).write_bytes(b'synthetic image, no native pixels');value=''
                    else:raise AssertionError('unexpected real-adapter path: '+label)
                    program='print('+repr(value)+')'
                    if label.startswith('display-'):
                        receipt=json.loads(value);rows=phase_rows(command[5],receipt)
                        program+='\nimport os,json,time,sys\nrows='+repr(rows)+'\nfor row in rows:\n row["producer_pid"]=os.getpid();row["at_ns"]=time.monotonic_ns();print(json.dumps(row),file=sys.stderr,flush=True)'
                    return owned_command(directory,label,self.command(program),timeout=1,grace=.2,check=kwargs.get('check',False))
                out=case/'output';argv=['run_pmix_native','--binary',str(binary),'--capture-producer',str(producer),'--output',str(out),'--mode','workflow','--video','--display-policy','preserve','--display-id','2']
                start=time.monotonic()
                reader_failure=patch.object(runner.threading.Thread,'start',side_effect=RuntimeError('injected reader start failure')) if failure=='reader-start-failure' else contextlib.nullcontext()
                with patch.object(sys,'platform','darwin'),patch.dict(os.environ,{'RCAM_PMIX_LAUNCH_NONCE':NONCE}),patch.object(runner,'runner_clock_ns',side_effect=time.monotonic_ns),patch.object(sys,'argv',argv),patch.object(runner,'owned_command',adapter),reader_failure,contextlib.redirect_stdout(io.StringIO()):
                    self.assertEqual(runner.main(),1)
                self.assertLess(time.monotonic()-start,4)
                self.assertTrue((out/'runner-error.json').exists());self.assertTrue((out/'runner.json').exists());self.assertTrue((out/'file-hashes.json').exists())
                binding_raw=(out/'runner-binding.json').read_bytes();binding=json.loads(binding_raw)
                self.assertEqual((Path(binding['native_directory'])/'runner-binding.json').read_bytes(),binding_raw)
                self.assertEqual(binding['output_directory'],str(out.resolve()))
                self.assertNotEqual(binding['native_directory'],binding['output_directory'])
                if failure in ('display-before','environment-os'):self.assertFalse((out/'app-launch.json').exists())
                cleanup=json.loads((out/'owned-cleanup.json').read_text())
                if cleanup['app_pid'] is not None:self.assertTrue(self.absent(cleanup['app_pid']));self.assertIsNotNone(cleanup['app_exit_code'])
                self.assertFalse((out/'capture-ready.json').exists());self.assertFalse((out/'native-window.mov').exists())
                if failure in ('producer-stderr-descendant','reader-start-failure'):
                    producer_cleanup=json.loads((out/'producer-cleanup.json').read_text())
                    self.assertTrue(producer_cleanup['joined']);self.assertTrue(producer_cleanup['owned_group_released'])
                    self.assertTrue(self.absent(producer_cleanup['pid']))
                    raw=(out/'capture-stderr.log').read_text()
                    if raw:
                        child=json.loads(raw)['synthetic_owned_stderr_descendant'];self.assertTrue(self.absent(child))
                    if failure=='reader-start-failure':
                        self.assertTrue((out/'reader-cleanup-error.json').exists());self.assertFalse(producer_cleanup['reader_joined'])
                    else:self.assertTrue(raw)
                else:self.assertFalse((out/'owned-producer.json').exists())
                if failure=='display-before':
                    self.assertEqual(calls,['display-before']);self.assertTrue((out/'display-restoration-error.json').exists())
                else:self.assertIn('display-restored',calls)
                if failure=='display-restored':self.assertTrue((out/'display-restoration-error.json').exists())
                elif failure!='display-before':self.assertEqual(json.loads((out/'display-restored.json').read_text()),display)

    def display_fixture(self,changed=False):
        """Invented sealed-shaped receipts, never actual Swift/native evidence."""
        initial={'display_id':2,'mode_id':113,'refresh_hz':144,'width':1920,'height':1080,
                 'pixel_width':3840,'pixel_height':2160,'backing_scale':2,'in_mirror_set':False}
        active=dict(initial,mode_id=114,refresh_hz=60) if changed else dict(initial)
        request={'schema_version':4,'display_id':2,'display_policy':'frozen-60hz' if changed else 'preserve',
                 'display_mode_change_authorized':changed}
        receipts={'display-before':{'before':dict(initial),'after':dict(initial)},
                  'display-active':{'before':dict(initial),'after':dict(active)},
                  'display-active-probe':{'before':dict(active),'after':dict(active)},
                  'display-restored':{'before':dict(active),'after':dict(initial)},
                  'display-restored-probe':{'before':dict(initial),'after':dict(initial)}}
        helper=Path('/tmp/synthetic-display-fixture/display.swift');probe=helper.with_name('display-probe.swift')
        commands={'display-before':['/usr/bin/swift','-swift-version','6','-warnings-as-errors',str(probe),'probe','2'],
                  'display-active':['/usr/bin/swift','-swift-version','6','-warnings-as-errors',str(helper if changed else probe),'set60' if changed else 'probe','2'],
                  'display-active-probe':['/usr/bin/swift','-swift-version','6','-warnings-as-errors',str(probe),'probe','2'],
                  'display-restored':['/usr/bin/swift','-swift-version','6','-warnings-as-errors',str(helper if changed else probe),'restore' if changed else 'probe','2']+
                                     (['113'] if changed else []),
                  'display-restored-probe':['/usr/bin/swift','-swift-version','6','-warnings-as-errors',str(probe),'probe','2']}
        if changed:
            for label in ('display-active','display-restored'):
                receipts[label]={side:display_core(snapshot) for side,snapshot in receipts[label].items()}
        return request,receipts,commands

    def check_display_fixture(self,fixture):
        request,receipts,commands=fixture
        (self.root/'display.swift').write_text(runner.DISPLAY_MUTATOR_SWIFT)
        (self.root/'display-probe.swift').write_text(runner.DISPLAY_PROBE_SWIFT)
        (self.root/'window-query.json').write_text(json.dumps({'command':['/usr/bin/swift','/tmp/synthetic-display-fixture/window.swift','123']}))
        def command_files(label,command,stdout):
            started={'display-before':100,'display-active':200,'display-active-probe':300,
                     'display-restored':500,'display-restored-probe':600}.get(label,120)
            row={'schema_version':2,'scope':'owned isolated subcommand; no native acceptance',
                 'command':command,'pid':12345,'pgid':12345,'private_session':True,
                 'timeout_seconds':runner.LIMITS[label],'cleanup_grace_seconds':2,
                 'started_monotonic_ns':started,'finished_monotonic_ns':started+10,'exit_code':0,'signal':None,
                 'timed_out':False,'error':None,'signals_sent':[],'joined':True,
                 'owned_group_released':True,'result':'RETURNED'}
            (self.root/(label+'.subcommand-process.json')).write_text(json.dumps(row))
            (self.root/(label+'.subcommand-launch.json')).write_text(json.dumps({key:row[key] for key in
                ('pid','pgid','private_session','command','started_monotonic_ns','timeout_seconds')}))
            (self.root/(label+'.subcommand.stdout')).write_text(stdout)
            (self.root/(label+'.subcommand.stderr')).write_text(phase_text(command[5],receipts[label],started=started) if label.startswith('display-') else '')
        for label,receipt in receipts.items():
            (self.root/(label+'.json')).write_text(json.dumps(receipt))
            command_files(label,commands[label],json.dumps(receipt)+'\n')
        environment={'os':'synthetic-os\n','machine':'synthetic-arm','memory_bytes':1024,'power':'synthetic-power\n'}
        (self.root/'environment.json').write_text(json.dumps(environment))
        for key,label,command in [('os','environment-os',['/usr/bin/sw_vers']),('machine','environment-machine',['/usr/bin/uname','-m']),
                                  ('memory_bytes','environment-memory',['/usr/sbin/sysctl','-n','hw.memsize']),
                                  ('power','environment-power',['/usr/bin/pmset','-g','custom'])]:
            command_files(label,command,str(environment[key]))
        (self.root/'owned-resource-usage.json').write_text(json.dumps({'started_monotonic_ns':400,'finished_monotonic_ns':450}))
        producer=b'invented display-only fixture capture producer';(self.root/'capture-producer').write_bytes(producer)
        manifest=(runner.ROOT/'MANIFEST.sha256').read_bytes();(self.root/'source-manifest.sha256').write_bytes(manifest)
        source=hashlib.sha256(manifest).hexdigest()
        binding={'schema_version':1,'event':'RUNNER_BINDING','launch_nonce':NONCE,'runner_pid':4321,
                 'runner_path':'/tmp/synthetic-source/scripts/run_pmix_native.py',
                 'runner_sha256':hashlib.sha256(Path(runner.__file__).read_bytes()).hexdigest(),
                 'native_directory':'/tmp/synthetic-display-fixture','output_directory':'/tmp/synthetic-distinct-output',
                 'run_id':'22222222-2222-4222-8222-222222222222','source_manifest_sha256':source,
                 'binary_path':'/tmp/synthetic-binary','binary_sha256':'0'*64,
                 'capture_producer_sha256':hashlib.sha256(producer).hexdigest(),'display_id':2,
                 'clock_domain':CLOCK_DOMAIN,'bound_at_ns':50}
        raw=json_bytes(binding);digest=hashlib.sha256(raw).hexdigest();(self.root/'runner-binding.json').write_bytes(raw)
        request.update(launch_nonce=NONCE,runner_pid=4321,native_directory=binding['native_directory'],
                       output_directory=binding['output_directory'],runner_binding_sha256=digest,
                       runner_clock_domain=CLOCK_DOMAIN,run_id=binding['run_id'],source_manifest_sha256=source)
        launch={'schema_version':1,'event':'APP_LAUNCH','binding_sha256':digest,
                **{key:binding[key] for key in ('launch_nonce','runner_pid','native_directory','output_directory','run_id','display_id','clock_domain')},
                'launch_at_ns':400}
        (self.root/'app-launch.json').write_bytes(json_bytes(launch))
        (self.root/'owned-process.json').write_text(json.dumps({'pid':123,'runner_pid':4321,'launch_nonce':NONCE,
            'clock_domain':CLOCK_DOMAIN,'app_started_uptime_ns':401,'binary_sha256':'0'*64,'command':[binding['binary_path']]}))
        (self.root/'binary-before.json').write_text(json.dumps({'path':binding['binary_path'],'sha256':'0'*64}))
        verify_display_environment(self.root,request,runner.DISPLAY_PROBE_SWIFT,runner.DISPLAY_MUTATOR_SWIFT)

    def reject_display_fixture(self,mutate,changed=True):
        baseline=self.display_fixture(changed);self.check_display_fixture(baseline)
        values=copy.deepcopy(baseline);mutate(*values)
        with self.assertRaises(ValueError):self.check_display_fixture(values)
        self.check_display_fixture(baseline)

    def test_explicit_display_preserve_and_frozen_restore_contract(self):
        self.check_display_fixture(self.display_fixture(False))
        self.check_display_fixture(self.display_fixture(True))
        values=self.display_fixture(True)
        # Current mode may change before cleanup. Restore must still match entry.
        values[1]['display-restored']['before'].update(mode_id=999,refresh_hz=75)
        self.check_display_fixture(values)
        preserved=self.display_fixture(False)
        for receipt in preserved[1].values():
            for snapshot in receipt.values():snapshot['in_mirror_set']=True
        self.check_display_fixture(preserved)
        mirrored=self.display_fixture(True)
        for receipt in mirrored[1].values():
            for snapshot in receipt.values():snapshot['in_mirror_set']=True
        with self.assertRaisesRegex(ValueError,'mirrored'):self.check_display_fixture(mirrored)

    def test_display_request_id_and_schema_are_strict(self):
        self.reject_display_fixture(lambda request,_,__:request.pop('display_id'))
        for value in (False,True,0,-1,2**32,'2'):
            with self.subTest(display_id=value):
                self.reject_display_fixture(lambda request,_,__,value=value:request.update(display_id=value))
        for value in (2,3,True,'4'):
            self.reject_display_fixture(lambda request,_,__,value=value:request.update(schema_version=value))
        self.reject_display_fixture(lambda request,_,__:request.update(display_mode_change_authorized=False))

    def test_display_snapshot_target_types_and_restoration_are_exact(self):
        for label in ('display-before','display-active','display-active-probe','display-restored','display-restored-probe'):
            for side in ('before','after'):
                with self.subTest(label=label,side=side):
                    self.reject_display_fixture(lambda _,receipts,__,label=label,side=side:
                        receipts[label][side].update(display_id=3))
        for key,value in (('mode_id',True),('mode_id',2**32),('width',True),('refresh_hz',False),
                          ('refresh_hz',float('nan')),('backing_scale',0),('pixel_width',-1),('in_mirror_set',0),('in_mirror_set',True)):
            self.reject_display_fixture(lambda _,receipts,__,key=key,value=value:
                receipts['display-restored']['after'].update({key:value}))
        for key,value in (('mode_id',114),('backing_scale',1),('pixel_width',1920)):
            self.reject_display_fixture(lambda _,receipts,__,key=key,value=value:
                receipts['display-restored']['after'].update({key:value}))
        self.reject_display_fixture(lambda _,receipts,__:
            receipts['display-before']['before'].update(mode_id=999))
        self.reject_display_fixture(lambda _,receipts,__:
            receipts['display-active']['before'].update(mode_id=999))
        self.reject_display_fixture(lambda _,receipts,__:
            receipts['display-active']['after'].update(width=960))

    def test_display_command_receipts_bind_target_and_original_mode(self):
        for label in ('display-before','display-active','display-active-probe','display-restored','display-restored-probe'):
            self.reject_display_fixture(lambda _,__,commands,label=label:commands[label].__setitem__(6,'3'))
            self.reject_display_fixture(lambda _,__,commands,label=label:commands[label].pop(6))
        self.reject_display_fixture(lambda _,__,commands:commands['display-restored'].__setitem__(7,'114'))
        self.reject_display_fixture(lambda _,__,commands:commands['display-active'].__setitem__(5,'probe'))
        self.reject_display_fixture(lambda _,receipts,__:receipts['display-active']['after'].update(refresh_hz=144))
        baseline=self.display_fixture(True);self.check_display_fixture(baseline)
        raw=self.root/'display-active.subcommand.stdout'
        raw.write_text(raw.read_text().replace('"display_id": 2','"display_id": 3, "display_id": 2',1))
        with self.assertRaisesRegex(ValueError,'duplicate JSON key'):
            verify_display_environment(self.root,baseline[0],runner.DISPLAY_PROBE_SWIFT,runner.DISPLAY_MUTATOR_SWIFT)
        self.check_display_fixture(baseline)

    def test_display_interpreter_language_and_warning_flags_are_exact(self):
        # Corrupt otherwise consistent raw command receipts, retaining all phases.
        fixture=self.display_fixture(True);self.check_display_fixture(fixture)
        for label in ('display-before','display-active','display-active-probe','display-restored','display-restored-probe'):
            process=self.root/(label+'.subcommand-process.json');launch=self.root/(label+'.subcommand-launch.json')
            original_process=process.read_bytes();original_launch=launch.read_bytes()
            command=json.loads(original_process)['command']
            self.assertEqual(command[:4],['/usr/bin/swift','-swift-version','6','-warnings-as-errors'])
            variants=[command[:1]+command[3:],command[:3]+command[4:],
                      command[:2]+['5']+command[3:],command[:3]+['-suppress-warnings']+command[4:]]
            for variant in variants:
                p=json.loads(original_process);p['command']=variant;process.write_text(json.dumps(p))
                l=json.loads(original_launch);l['command']=variant;launch.write_text(json.dumps(l))
                with self.subTest(label=label,argv=variant),self.assertRaises(ValueError):
                    verify_display_environment(self.root,fixture[0],runner.DISPLAY_PROBE_SWIFT,runner.DISPLAY_MUTATOR_SWIFT)
            process.write_bytes(original_process);launch.write_bytes(original_launch)
        verify_display_environment(self.root,fixture[0],runner.DISPLAY_PROBE_SWIFT,runner.DISPLAY_MUTATOR_SWIFT)

    def test_display_post_exit_probes_and_app_clock_causality(self):
        for label in ('display-active-probe','display-restored-probe'):
            self.reject_display_fixture(lambda _,receipts,__,label=label:
                receipts[label]['after'].update(refresh_hz=75))
            baseline=self.display_fixture(True);self.check_display_fixture(baseline)
            (self.root/(label+'.json')).unlink()
            with self.assertRaisesRegex(ValueError,'missing evidence'):verify_display_environment(self.root,baseline[0],runner.DISPLAY_PROBE_SWIFT,runner.DISPLAY_MUTATOR_SWIFT)
        for label,start in (('display-active-probe',200),('display-active-probe',210),
                            ('display-restored',400),('display-restored-probe',510)):
            baseline=self.display_fixture(True);self.check_display_fixture(baseline)
            path=self.root/(label+'.subcommand-process.json');row=json.loads(path.read_text())
            row.update(started_monotonic_ns=start,finished_monotonic_ns=start+10);path.write_text(json.dumps(row))
            launch=self.root/(label+'.subcommand-launch.json');value=json.loads(launch.read_text())
            value['started_monotonic_ns']=start;launch.write_text(json.dumps(value))
            with self.assertRaisesRegex(ValueError,'causal binding'):
                verify_display_environment(self.root,baseline[0],runner.DISPLAY_PROBE_SWIFT,runner.DISPLAY_MUTATOR_SWIFT)
        for start,finish in ((300,450),(310,450),(False,450),(400,500)):
            baseline=self.display_fixture(True);self.check_display_fixture(baseline)
            (self.root/'owned-resource-usage.json').write_text(json.dumps({'started_monotonic_ns':start,'finished_monotonic_ns':finish}))
            with self.assertRaises(ValueError):verify_display_environment(self.root,baseline[0],runner.DISPLAY_PROBE_SWIFT,runner.DISPLAY_MUTATOR_SWIFT)
        self.check_display_fixture(self.display_fixture(True))

    def test_runner_same_target_cleanup_after_partial_set_abort_and_main_change(self):
        # All external commands are intercepted. No Mac helper or GUI is executed.
        for failure in ('main-change','partial-set','keyboard-interrupt','target-unavailable','invalid-initial',
                        'missing-on-restore','cleanup-receipt-write-failure','cleanup-second-interrupt',
                        'process-lifetime-reversion','restore-process-lifetime-reversion','mirrored-initial','mirrored-after'):
            with self.subTest(failure=failure):
                case=self.root/failure;case.mkdir();binary=case/'never-launched-app';producer=case/'never-launched-producer'
                binary.write_bytes(b'synthetic app');producer.write_bytes(b'synthetic producer')
                initial=self.display_fixture(True)[1]['display-before']['after'];calls=[];synthetic_main={'display_id':2}
                def adapter(directory,label,command,**kwargs):
                    calls.append((label,list(command)))
                    if label=='display-before':
                        if failure=='target-unavailable':raise subprocess.CalledProcessError(2,command,stderr='requested display unavailable; no fallback')
                        if failure=='mirrored-initial':initial['in_mirror_set']=True
                        receipt={'before':dict(initial),'after':dict(initial)}
                        if failure=='invalid-initial':receipt['after']['display_id']=3
                    elif label.startswith('environment-'):
                        value='1024' if label=='environment-memory' else 'synthetic-environment'
                        return subprocess.CompletedProcess(command,0,value,'')
                    elif label=='display-active':
                        self.assertEqual(command[-2:],['set60','2'])
                        if failure in ('partial-set','cleanup-receipt-write-failure','cleanup-second-interrupt'):
                            raise subprocess.CalledProcessError(2,command,stderr='synthetic partial mutation')
                        if failure=='keyboard-interrupt':raise KeyboardInterrupt('synthetic abort')
                        # The new current-main ID differs, but every operation stays on2.
                        synthetic_main['display_id']=3
                        receipt={'before':dict(initial),'after':dict(initial,mode_id=114,refresh_hz=60)}
                        if failure=='mirrored-after':receipt['after']['in_mirror_set']=True
                    elif label=='display-active-probe':
                        self.assertEqual(command[-2:],['probe','2'])
                        current=dict(initial) if failure=='process-lifetime-reversion' else dict(initial,mode_id=114,refresh_hz=60)
                        if failure=='mirrored-after':current['in_mirror_set']=True
                        receipt={'before':dict(current),'after':dict(current)}
                    elif label=='display-restored-probe':
                        self.assertEqual(command[-2:],['probe','2'])
                        current=dict(initial,mode_id=114,refresh_hz=60) if failure=='restore-process-lifetime-reversion' else dict(initial)
                        receipt={'before':dict(current),'after':dict(current)}
                    elif label=='display-restored':
                        if failure=='mirrored-initial':self.assertEqual(command[-2:],['probe','2'])
                        else:self.assertEqual(command[-3:],['restore','2','113'])
                        if failure=='missing-on-restore':raise subprocess.CalledProcessError(2,command,stderr='requested target unavailable; no fallback')
                        receipt={'before':dict(initial) if failure=='mirrored-initial' else dict(initial,mode_id=999,refresh_hz=75),'after':dict(initial)}
                    else:raise AssertionError('unexpected actual command '+label)
                    if command[5]!='probe':receipt={side:display_core(snapshot) for side,snapshot in receipt.items()}
                    # Fake process receipt is owned by this injected adapter, not a Swift execution.
                    (directory/(label+'.subcommand-process.json')).write_text(json.dumps({'pid':12345}))
                    return subprocess.CompletedProcess(command,0,json.dumps(receipt),phase_text(command[5],receipt))
                out=case/'output';argv=['run_pmix_native','--binary',str(binary),'--capture-producer',str(producer),
                    '--output',str(out),'--mode','workflow','--video','--display-policy','frozen-60hz',
                    '--allow-display-mode-change','--display-id','2']
                original_write=runner.write
                original_copy=shutil.copy2
                def copy_except_markers(source,target,*args,**kwargs):
                    self.assertNotIn(Path(source).name,('runner-binding.json','app-launch.json'))
                    return original_copy(source,target,*args,**kwargs)
                def fixture_write(path,value):
                    if path.name=='owned-cleanup.json':
                        if failure=='cleanup-receipt-write-failure':raise OSError('synthetic cleanup receipt write failure')
                        if failure=='cleanup-second-interrupt':raise KeyboardInterrupt('synthetic second abort during cleanup')
                    return original_write(path,value)
                with patch.object(sys,'platform','darwin'),patch.dict(os.environ,{'RCAM_PMIX_LAUNCH_NONCE':NONCE}),patch.object(runner,'runner_clock_ns',side_effect=time.monotonic_ns),patch.object(sys,'argv',argv),patch.object(runner,'owned_command',adapter),patch.object(runner,'write',fixture_write),patch.object(runner.shutil,'copy2',side_effect=copy_except_markers),contextlib.redirect_stdout(io.StringIO()):
                    self.assertEqual(runner.main(),1)
                self.assertFalse((out/'owned-process.json').exists())
                self.assertEqual((out/'app-launch.json').exists(),failure in ('main-change','missing-on-restore','restore-process-lifetime-reversion'))
                binding_raw=(out/'runner-binding.json').read_bytes();binding=json.loads(binding_raw)
                self.assertEqual((Path(binding['native_directory'])/'runner-binding.json').read_bytes(),binding_raw)
                self.assertEqual(binding['output_directory'],str(out.resolve()))
                self.assertTrue((out/'file-hashes.json').exists())
                labels=[label for label,_ in calls]
                if failure in ('target-unavailable','invalid-initial'):
                    self.assertEqual(labels,['display-before'])
                    error=json.loads((out/'display-restoration-error.json').read_text())
                    self.assertFalse(error['mode_change_attempted']);self.assertFalse(error['initial_snapshot_available'])
                else:
                    self.assertIn('display-restored',labels)
                    self.assertEqual(labels[-1],'display-restored' if failure=='missing-on-restore' else 'display-restored-probe')
                    if failure=='main-change':self.assertEqual(synthetic_main['display_id'],3)
                    if failure=='mirrored-initial':
                        self.assertEqual(labels,['display-before','display-restored','display-restored-probe'])
                    if failure in ('missing-on-restore','restore-process-lifetime-reversion'):
                        error=json.loads((out/'display-restoration-error.json').read_text())
                        self.assertTrue(error['mode_change_attempted']);self.assertTrue(error['initial_snapshot_available'])
                        self.assertEqual(error['display_id'],2);self.assertEqual(error['original_mode_id'],113)
                        self.assertEqual((out/'display-restored.json').exists(),failure=='restore-process-lifetime-reversion')
                    else:
                        self.assertEqual(json.loads((out/'display-restored.json').read_text())['after'],initial if failure=='mirrored-initial' else display_core(initial))
                        self.assertFalse((out/'display-restoration-error.json').exists())
                        if failure.startswith('cleanup-'):
                            self.assertFalse((out/'owned-cleanup.json').exists())
                            self.assertIn('owned cleanup/evidence failed',json.loads((out/'runner.json').read_text())['error'])

    def test_pmix_helper_never_resolves_main_display_and_cli_id_is_bounded(self):
        self.assertNotIn('CGMainDisplayID',runner.DISPLAY_MUTATOR_SWIFT)
        self.assertNotIn('NSScreen.main',runner.DISPLAY_MUTATOR_SWIFT)
        self.assertNotIn('CGDisplaySetDisplayMode',runner.DISPLAY_MUTATOR_SWIFT)
        self.assertNotIn('.permanently',runner.DISPLAY_MUTATOR_SWIFT)
        self.assertNotIn('.forAppOnly',runner.DISPLAY_MUTATOR_SWIFT)
        self.assertIn('CGCompleteDisplayConfiguration(configuration, .forSession)',runner.DISPLAY_MUTATOR_SWIFT)
        self.assertIn('CGConfigureDisplayWithDisplayMode(configuration, display, mode, nil)',runner.DISPLAY_MUTATOR_SWIFT)
        self.assertIn('CGDisplayIsInMirrorSet(display)',runner.DISPLAY_MUTATOR_SWIFT)
        for forbidden in ('AppKit','NSApplication','NSScreen'):
            self.assertNotIn(forbidden,runner.DISPLAY_MUTATOR_SWIFT)
        self.assertNotIn('CGCompleteDisplayConfiguration',runner.DISPLAY_PROBE_SWIFT)
        for text in ('0','-1','+2','2.0',' 2','4294967296','True','٢'):
            with self.subTest(text=text):
                with self.assertRaises(runner.argparse.ArgumentTypeError):runner.display_id_argument(text)
        self.assertEqual(runner.display_id_argument('2'),2)
        self.assertEqual(runner.display_id_argument('4294967295'),2**32-1)

    def test_mutator_receipt_never_supplies_or_infers_probe_scale(self):
        from pmix_owned_command import validate_display_receipt
        fixture=self.display_fixture(True)
        core=fixture[1]['display-active'];probe=fixture[1]['display-active-probe']
        validate_display_receipt(core,2,probe=False);validate_display_receipt(probe,2,probe=True)
        with self.assertRaises(ValueError):validate_display_receipt(core,2,probe=True)
        with self.assertRaises(ValueError):validate_display_receipt(probe,2,probe=False)
        self.reject_display_fixture(lambda _,receipts,__:receipts['display-active-probe']['after'].update(backing_scale=1))

    def test_display_raw_phases_require_complete_success_and_exact_identity(self):
        receipt=self.display_fixture(True)[1]['display-active']
        baseline=phase_rows('set60',receipt)
        def check(rows):return validate_display_phases(''.join(json.dumps(row)+'\n' for row in rows),'set60',2,12345,receipt)
        self.assertEqual(len(check(baseline)),len(baseline))
        for key,value in (('producer_pid',999),('producer_pid',True),('display_id',3),('thread_main',False),
                          ('clock_domain','process-relative-monotonic'),('sequence',True),('at_ns',False),('uid',True)):
            rows=copy.deepcopy(baseline);rows[0][key]=value
            with self.subTest(key=key,value=value),self.assertRaises(ValueError):check(rows)
        for phase,key,value in (('complete-return','cg_error',1),('complete-return','cg_error',False),
                                ('complete-return','selected_mode_id',113),('mode-selected','value',113)):
            rows=copy.deepcopy(baseline);next(row for row in rows if row['phase']==phase)[key]=value
            with self.subTest(phase=phase),self.assertRaises(ValueError):check(rows)
        for rows in (baseline[:-1],baseline+[baseline[-1]],baseline[:15]+baseline[16:]):
            with self.assertRaises(ValueError):check(rows)
        raw=phase_text('set60',receipt)
        for corrupt in ('',raw+'unexpected log\n',raw.replace('"display_id": 2','"display_id": 3, "display_id": 2',1)):
            with self.assertRaises(ValueError):validate_display_phases(corrupt,'set60',2,12345,receipt)

    def test_runner_marker_is_complete_once_and_duplicate_publication_fails(self):
        raw=json_bytes({'event':'invented immutable marker','scope':'synthetic parser test'})
        publish_runner_marker(self.root,'runner-binding.json',raw)
        self.assertEqual((self.root/'runner-binding.json').read_bytes(),raw)
        self.assertFalse((self.root/'runner-binding.json.writing').exists())
        with self.assertRaises(FileExistsError):publish_runner_marker(self.root,'runner-binding.json',json_bytes({'other':True}))
        self.assertEqual((self.root/'runner-binding.json').read_bytes(),raw)

    def test_runner_shared_clock_has_no_process_relative_fallback(self):
        import pmix_owned_command as commands
        with patch.object(sys,'platform','darwin'),patch.object(time,'CLOCK_UPTIME_RAW',8,create=True),\
             patch.object(time,'clock_gettime_ns',return_value=123) as reader,\
             patch.object(time,'monotonic_ns',side_effect=AssertionError('must not read relative monotonic')):
            self.assertEqual(runner_clock_ns(),123);reader.assert_called_once_with(8)
            reader.return_value=False
            with self.assertRaises(ValueError):runner_clock_ns()
        with patch.object(sys,'platform','darwin'),patch.object(time,'CLOCK_UPTIME_RAW',None,create=True):
            with self.assertRaisesRegex(ValueError,'shared uptime unavailable'):commands.runner_clock_ns()

    def test_runner_requires_external_launch_nonce_before_any_output_or_helper(self):
        out=self.root/'must-not-be-created'
        argv=['run_pmix_native','--binary','/invented/app','--capture-producer','/invented/producer',
              '--output',str(out),'--mode','nav','--video','--display-id','2']
        for value in ('','not-a-uuid','11111111-1111-4111-8111-11111111111A'):
            with patch.object(sys,'platform','darwin'),patch.object(sys,'argv',argv),\
                 patch.dict(os.environ,{'RCAM_PMIX_LAUNCH_NONCE':value}),\
                 patch.object(runner,'owned_command',side_effect=AssertionError('no helper permitted')),\
                 contextlib.redirect_stderr(io.StringIO()),self.assertRaises(SystemExit) as exit:
                runner.main()
            self.assertEqual(exit.exception.code,2);self.assertFalse(out.exists())

    def test_runner_launch_marker_binding_clock_and_distinct_directories_are_strict(self):
        fixture=self.display_fixture(True);self.check_display_fixture(fixture)
        from pmix_owned_command import verify_runner_markers
        original=(self.root/'app-launch.json').read_bytes()
        for key,value in (('binding_sha256','1'*64),('runner_pid',True),('launch_nonce','33333333-3333-4333-8333-333333333333'),
                          ('native_directory','/tmp/foreign-native'),('output_directory','/tmp/foreign-output'),
                          ('clock_domain','process-relative-monotonic'),('launch_at_ns',50),('launch_at_ns',True),('launch_at_ns',402)):
            launch=json.loads(original);launch[key]=value;(self.root/'app-launch.json').write_bytes(json_bytes(launch))
            with self.subTest(key=key,value=value),self.assertRaises(ValueError):verify_runner_markers(self.root,fixture[0])
        (self.root/'app-launch.json').write_bytes(original)
        binding,launch,_=verify_runner_markers(self.root,fixture[0]);self.assertNotEqual(binding['native_directory'],binding['output_directory'])
        request=copy.deepcopy(fixture[0]);request['runner_pid']=True
        with self.assertRaises(ValueError):verify_runner_markers(self.root,request)

    def test_actual_spawn_shared_clock_and_exact_binary_command_are_bound(self):
        fixture=self.display_fixture(True);self.check_display_fixture(fixture)
        original=(self.root/'owned-process.json').read_bytes()
        for key,value in (('command',['/tmp/foreign-app']),('command',None),
                          ('app_started_uptime_ns',2**64-1),('app_started_uptime_ns',501)):
            owned=json.loads(original);owned[key]=value;(self.root/'owned-process.json').write_bytes(json_bytes(owned))
            with self.subTest(key=key,value=value),self.assertRaises(ValueError):
                verify_display_environment(self.root,fixture[0],runner.DISPLAY_PROBE_SWIFT,runner.DISPLAY_MUTATOR_SWIFT)
        (self.root/'owned-process.json').write_bytes(original)
        verify_display_environment(self.root,fixture[0],runner.DISPLAY_PROBE_SWIFT,runner.DISPLAY_MUTATOR_SWIFT)

    def test_mutator_phase_io_failure_cancels_only_a_still_live_token(self):
        # Static ordering regression on exact production Swift, not SDK execution.
        source=runner.DISPLAY_MUTATOR_SWIFT
        start=source.index('func fail(_ message: String)');end=source.index('func identifier(',start)
        cleanup=source[start:end]
        self.assertIn('if let configuration = pendingConfiguration',cleanup)
        self.assertLess(cleanup.index('pendingConfiguration = nil'),cleanup.index('CGCancelDisplayConfiguration(configuration)'))
        self.assertNotIn('phase(',cleanup)
        self.assertLess(source.index('if began == .success { pendingConfiguration = configuration }'),source.index('phase("begin-return"'))
        for call,returned in (('let cancelled = CGCancelDisplayConfiguration(configuration)','phase("cancel-return"'),
                              ('let completed = CGCompleteDisplayConfiguration(configuration, .forSession)','phase("complete-return"')):
            began=source.index(call);ended=source.index(returned,began)
            self.assertIn('pendingConfiguration = nil',source[began:ended])

    def test_helper_prohibited_policy_uses_current_state_not_switch_bool(self):
        # Static regression over the actual Swift SOURCE, not an AppKit execution.
        # Native compile/probe is required to establish the SDK/runtime behavior.
        source=runner.DISPLAY_PROBE_SWIFT
        start=source.index('let application = NSApplication.shared')
        end=source.index('phase("appkit-return"',start)
        expected='''let application = NSApplication.shared
if application.activationPolicy() != .prohibited {
    _ = application.setActivationPolicy(.prohibited)
}
guard application.activationPolicy() == .prohibited else {
    fail("non-activating helper initialization failed")
}'''
        self.assertEqual(' '.join(source[start:end].split()),' '.join(expected.split()))
        self.assertNotIn('guard application.setActivationPolicy',source)
        for forbidden in ('.accessory','.regular','.activate(','.run(','.finishLaunching('):
            self.assertNotIn(forbidden,source)

    def test_runner_has_no_unbounded_run_check_output_or_wait_calls(self):
        import ast
        tree=ast.parse(Path(runner.__file__).read_text())
        for node in ast.walk(tree):
            if isinstance(node,ast.Call) and isinstance(node.func,ast.Attribute):
                self.assertNotIn(node.func.attr,('run','check_output'))
                if node.func.attr=='wait':self.assertTrue(node.args or any(k.arg=='timeout' for k in node.keywords))
                if node.func.attr=='read' and isinstance(node.func.value,ast.Attribute):self.assertNotEqual(node.func.value.attr,'stderr')

if __name__=='__main__':unittest.main()
