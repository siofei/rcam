"""Real owned-Python process regressions; all native adapters are synthetic, no GUI."""
import contextlib
import io
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

from pmix_owned_command import owned_command, verify_command
import run_pmix_native as runner


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
        # The actual runner executes only owned Python fixture programs. No SDK/display/window/screenshot call runs.
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
                calls=[];display={'after':{'mode_id':113,'refresh_hz':144,'width':1920,'height':1080,'pixel_width':3840,'pixel_height':2160,'backing_scale':2}}
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
                    return owned_command(directory,label,self.command('print('+repr(value)+')'),timeout=1,grace=.2,check=kwargs.get('check',False))
                out=case/'output';argv=['run_pmix_native','--binary',str(binary),'--capture-producer',str(producer),'--output',str(out),'--mode','workflow','--video','--display-policy','preserve']
                start=time.monotonic()
                reader_failure=patch.object(runner.threading.Thread,'start',side_effect=RuntimeError('injected reader start failure')) if failure=='reader-start-failure' else contextlib.nullcontext()
                with patch.object(sys,'argv',argv),patch.object(runner,'owned_command',adapter),reader_failure,contextlib.redirect_stdout(io.StringIO()):
                    self.assertEqual(runner.main(),1)
                self.assertLess(time.monotonic()-start,4)
                self.assertTrue((out/'runner-error.json').exists());self.assertTrue((out/'runner.json').exists());self.assertTrue((out/'file-hashes.json').exists())
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

    def test_runner_has_no_unbounded_run_check_output_or_wait_calls(self):
        import ast
        tree=ast.parse(Path(runner.__file__).read_text())
        for node in ast.walk(tree):
            if isinstance(node,ast.Call) and isinstance(node.func,ast.Attribute):
                self.assertNotIn(node.func.attr,('run','check_output'))
                if node.func.attr=='wait':self.assertTrue(node.args or any(k.arg=='timeout' for k in node.keywords))
                if node.func.attr=='read' and isinstance(node.func.value,ast.Attribute):self.assertNotEqual(node.func.value.attr,'stderr')

if __name__=='__main__':unittest.main()
