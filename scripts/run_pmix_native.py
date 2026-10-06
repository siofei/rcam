"""Local PMIX native runner. Preserve display by default; join valid movie before app release."""
import argparse,datetime,hashlib,json,os,queue,shutil,subprocess,sys,tempfile,threading,time,uuid
from pathlib import Path
from pmix_display_swift import PROBE_SOURCE as DISPLAY_PROBE_SWIFT, MUTATOR_SOURCE as DISPLAY_MUTATOR_SWIFT
from pmix_capture_swift import SOURCE as CAPTURE_SWIFT
from pmix_capture_lifecycle import Lifecycle
from pmix_owned_command import (owned_command, LIMITS, drain_owned_group, group_present, validate_display_receipt,
    display_core, validate_display_phases, runner_clock_ns, json_bytes, publish_runner_marker, CLOCK_DOMAIN, MARKER_NAMES,
    DISPLAY_SWIFT_PREFIX)

WINDOW_SWIFT = r'''
import CoreGraphics
import Foundation
let pid = Int(CommandLine.arguments[1])!
let windows = CGWindowListCopyWindowInfo([.optionOnScreenOnly,.excludeDesktopElements], kCGNullWindowID) as! [[String:Any]]
let matching = windows.filter { ($0[kCGWindowOwnerPID as String] as? Int)==pid && ($0[kCGWindowLayer as String] as? Int)==0 }
print(String(data:try! JSONSerialization.data(withJSONObject:matching.map { ["window_id":$0[kCGWindowNumber as String]!,"owner_pid":$0[kCGWindowOwnerPID as String]!,"layer":$0[kCGWindowLayer as String]!,"bounds":$0[kCGWindowBounds as String]!] },options:[.sortedKeys]),encoding:.utf8)!)
'''
ROOT=Path(__file__).resolve().parents[1]
def sha(p):return hashlib.sha256(p.read_bytes()).hexdigest()
def write(p,v):
    tmp=p.with_name(p.name+'.writing');tmp.write_text(json.dumps(v,ensure_ascii=False,indent=2)+'\n');os.replace(tmp,p)
def optional(p):
    if not p.is_file():return None
    try:return json.loads(p.read_text())
    except json.JSONDecodeError:return None

def display_policy_operation(policy,authorized):
    if policy=='preserve':return 'probe'
    if policy!='frozen-60hz' or not authorized:raise ValueError('explicit display-change authorization required for frozen60Hz protocol')
    return 'set60'

def display_id_argument(text):
    if not text.isascii() or not text.isdigit() or not 0<int(text)<2**32:
        raise argparse.ArgumentTypeError('display ID must be an integer from 1 through UInt32.max')
    return int(text)

def main():
    p=argparse.ArgumentParser(description=__doc__)
    p.add_argument('--binary',type=Path,required=True);p.add_argument('--capture-producer',type=Path,required=True);p.add_argument('--output',type=Path,required=True)
    p.add_argument('--mode',choices=['nav','move','points','escape','new-project','workflow','workflow-reopen','workflow-cross-layer'],required=True)
    p.add_argument('--round',type=int,default=1);p.add_argument('--video',action='store_true');p.add_argument('--fixture',type=Path)
    p.add_argument('--display-policy',choices=['preserve','frozen-60hz'],default='preserve')
    p.add_argument('--display-id',type=display_id_argument,required=True,help='explicit target display for all probe/set/restore operations')
    p.add_argument('--allow-display-mode-change',action='store_true',help='only after explicit user coordination of the setting change')
    a=p.parse_args()
    if not a.video:p.error('--video required')
    if (a.mode=='workflow-reopen')!=(a.fixture is not None):p.error('--fixture only and always for workflow-reopen')
    if sys.platform!='darwin':p.error('native macOS only')
    nonce=os.environ.get('RCAM_PMIX_LAUNCH_NONCE')
    try:
        if type(nonce) is not str or str(uuid.UUID(nonce))!=nonce:raise ValueError('canonical launch UUID required')
    except (ValueError,AttributeError):p.error('RCAM_PMIX_LAUNCH_NONCE must contain the external launch nonce')
    try:operation=display_policy_operation(a.display_policy,a.allow_display_mode_change)
    except ValueError as e:p.error(str(e))
    a.output.mkdir(parents=True,exist_ok=False);out=a.output.resolve();native=Path(tempfile.mkdtemp(prefix='rcam-pmix-',dir='/tmp')).resolve()
    binary=a.binary.resolve(strict=True);producer=a.capture_producer.resolve(strict=True)
    fixture=a.fixture.resolve(strict=True) if a.fixture else ROOT/'fixtures/synthetic/s5m2c'/('MIX_WORKFLOW.rcam' if a.mode.startswith('workflow') else 'PMIX.gbr')
    if a.mode=='workflow-reopen':shutil.copy2(fixture,native/'reopen-input.rcam');fixture=native/'reopen-input.rcam'
    protocol=ROOT/'fixtures/synthetic/s5m2c/protocol.json'
    for name,data in [('display.swift',DISPLAY_MUTATOR_SWIFT),('display-probe.swift',DISPLAY_PROBE_SWIFT),('window.swift',WINDOW_SWIFT),('capture.swift',CAPTURE_SWIFT)]: (native/name).write_text(data)
    shutil.copy2(producer,native/'capture-producer');os.chmod(native/'capture-producer',0o700)
    request={'schema_version':4,'mode':a.mode,'selected':1000,'fixture':str(fixture),'fixture_sha256':sha(fixture),'protocol_sha256':sha(protocol),'native_inputs_sha256':sha(protocol.with_name('native-inputs.json')),'round':a.round,'run_id':str(uuid.uuid4()),'start_utc':datetime.datetime.now(datetime.timezone.utc).isoformat(),'source_manifest_sha256':sha(ROOT/'MANIFEST.sha256'),'display_id':a.display_id,'display_policy':a.display_policy,'display_mode_change_authorized':a.allow_display_mode_change,'evidence_scope':'full-pmix-native' if operation=='set60' else 'capture-precheck-only'}
    before=None;original=None;process=video=reader=None;life=None;code=1;error=None;exit_code=None;rss=None;usage_receipt=None;restore=None;video_record=None;changed=False
    ui=Path(tempfile.mkdtemp(prefix='rcam-i2-c-ui-',dir=native));messages=queue.Queue();events=[];stdout_rows=[];producer_stderr=None;producer_err_file=None;reader_started=False;reader_cleanup_error=None;producer_signals=[];producer_group_released=False
    write(native/'binary-before.json',{'path':str(binary),'sha256':sha(binary),'bytes':binary.stat().st_size});shutil.copy2(ROOT/'MANIFEST.sha256',native/'source-manifest.sha256')
    def publish_marker(name,value):
        raw=json_bytes(value);publish_runner_marker(native,name,raw);publish_runner_marker(out,name,raw)
        return hashlib.sha256(raw).hexdigest()
    def display_command(label,op,mode=None):
        helper='display-probe.swift' if op=='probe' else 'display.swift'
        command=[*DISPLAY_SWIFT_PREFIX,str(native/helper),op,str(a.display_id)]+([str(mode)] if mode is not None else [])
        returned=owned_command(native,label,command,check=True)
        from verify_pmix_workflow import parse_json
        receipt=parse_json(returned.stdout.encode());validate_display_receipt(receipt,a.display_id,probe=op=='probe')
        pid=json.loads((native/(label+'.subcommand-process.json')).read_text())['pid']
        validate_display_phases(returned.stderr,op,a.display_id,pid,receipt)
        write(native/(label+'.json'),receipt)
        return receipt
    try:
        script=Path(__file__).resolve()
        binding={'schema_version':1,'event':'RUNNER_BINDING','launch_nonce':nonce,'runner_pid':os.getpid(),
                 'runner_path':str(script),'runner_sha256':sha(script),'native_directory':str(native),'output_directory':str(out),
                 'run_id':request['run_id'],'source_manifest_sha256':request['source_manifest_sha256'],
                 'binary_path':str(binary),'binary_sha256':sha(binary),'capture_producer_sha256':sha(native/'capture-producer'),
                 'display_id':a.display_id,'clock_domain':CLOCK_DOMAIN,'bound_at_ns':runner_clock_ns()}
        binding_raw=json_bytes(binding);binding_sha=hashlib.sha256(binding_raw).hexdigest()
        request.update(launch_nonce=nonce,runner_pid=binding['runner_pid'],native_directory=str(native),output_directory=str(out),
                       runner_binding_sha256=binding_sha,runner_clock_domain=CLOCK_DOMAIN)
        publish_runner_marker(native,'runner-binding.json',binding_raw);write(native/'request.json',request)
        publish_runner_marker(out,'runner-binding.json',binding_raw)
        initial=display_command('display-before','probe')
        if initial['before']!=initial['after']:raise RuntimeError('initial target probe changed')
        before=initial
        write(native/'display-before.json',before);original=before['after']['mode_id']
        if operation=='set60' and before['after']['in_mirror_set']:
            raise RuntimeError('target display is mirrored; refusing linked mode changes')
        environment={'timing_scope':'egui raw input and production Metal completion bound; no scanout claim'}
        for key,label,command in [('os','environment-os',['/usr/bin/sw_vers']),('machine','environment-machine',['/usr/bin/uname','-m']),
                                  ('memory_bytes','environment-memory',['/usr/sbin/sysctl','-n','hw.memsize']),('power','environment-power',['/usr/bin/pmset','-g','custom'])]:
            value=owned_command(native,label,command,check=True).stdout
            environment[key]=int(value) if key=='memory_bytes' else (value.strip() if key=='machine' else value)
        write(native/'environment.json',environment)
        changed=operation=='set60';active=display_command('display-active',operation)
        if display_core(active['before'])!=display_core(before['after']):raise RuntimeError('active original target snapshot mismatch')
        if changed and abs(active['after']['refresh_hz']-60)>.01:raise RuntimeError('actual60Hz unavailable')
        if operation=='probe' and active['after']!=before['after']:raise RuntimeError('display changed before capture')
        for key in ('width','height','pixel_width','pixel_height'):
            if active['after'][key]!=before['after'][key]:raise RuntimeError('display geometry changed')
        active_probe=display_command('display-active-probe','probe')
        if active_probe['before']!=active_probe['after'] or display_core(active_probe['after'])!=display_core(active['after']):
            raise RuntimeError('active target mode did not survive setter process exit')
        if active_probe['after']['backing_scale']!=before['after']['backing_scale']:raise RuntimeError('actual target scale changed')
        if changed and active_probe['after']['in_mirror_set']:
            raise RuntimeError('target became mirrored; refusing app launch after linked mode change')
        env={k:v for k,v in os.environ.items() if not k.startswith('RCAM_')};env.update(RCAM_PMIX_NATIVE_DIR=str(native),RCAM_UI_ROI_DIR=str(ui),RCAM_UI_ROI_ROOT=str(native))
        with (native/'stdout.log').open('wb') as app_out,(native/'stderr.log').open('wb') as app_err:
            launch={'schema_version':1,'event':'APP_LAUNCH','binding_sha256':binding_sha,
                    **{key:binding[key] for key in ('launch_nonce','runner_pid','native_directory','output_directory','run_id','display_id','clock_domain')},
                    'launch_at_ns':runner_clock_ns()}
            publish_marker('app-launch.json',launch)
            app_started=time.monotonic_ns();process=subprocess.Popen([str(binary)],env=env,stdout=app_out,stderr=app_err)
            write(native/'owned-process.json',{'pid':process.pid,'command':[str(binary)],'binary_sha256':sha(binary),
                  'runner_pid':binding['runner_pid'],'launch_nonce':nonce,'clock_domain':CLOCK_DOMAIN,
                  'app_started_uptime_ns':runner_clock_ns()});start=time.monotonic();video_started=ready_at=stop_at=None
            def receive():
                try:
                    for line in video.stdout:stdout_rows.append(line);messages.put(json.loads(line))
                except BaseException as e:messages.put({'event':'failure','error':'producer stdout: '+str(e)})
            while True:
                waited,status,usage=os.wait4(process.pid,os.WNOHANG)
                if waited:
                    exit_code=os.waitstatus_to_exitcode(status);process.returncode=exit_code;rss=int(usage.ru_maxrss)
                    usage_receipt={'pid':waited,'exit_code':exit_code,'peak_rss_bytes':rss,'user_cpu_seconds':usage.ru_utime,'system_cpu_seconds':usage.ru_stime,'elapsed_seconds':time.monotonic()-start,'started_monotonic_ns':app_started,'finished_monotonic_ns':time.monotonic_ns(),'clock':'owned os.wait4 child completion; macOS ru_maxrss bytes','pass_claim':False}
                    write(native/'owned-resource-usage.json',usage_receipt)
                    if life is None:raise RuntimeError('app exited before capture producer')
                    life.app_exited(exit_code);break
                if time.monotonic()-start>255:raise RuntimeError('native process timeout')
                marker=optional(native/'window-ready.json')
                if video is None and marker:
                    if marker['app_pid']!=process.pid or marker['run_id']!=request['run_id']:raise RuntimeError('window-ready identity')
                    cmd=['/usr/bin/swift',str(native/'window.swift'),str(process.pid)];q_start=time.monotonic_ns();query=owned_command(native,'window-query',cmd,timeout=max(.001,min(LIMITS['window-query'],start+255-time.monotonic())));q_end=time.monotonic_ns()
                    windows=json.loads(query.stdout) if query.returncode==0 else []
                    write(native/'window-query.json',{'pid':process.pid,'command':cmd,'exit_code':query.returncode,'started_monotonic_ns':q_start,'finished_monotonic_ns':q_end,'stdout':query.stdout,'stderr':query.stderr,'windows':windows});write(native/'window.json',windows)
                    if query.returncode or not windows:raise RuntimeError('ready app has no owned window')
                    window=max(windows,key=lambda w:w['bounds']['Width']*w['bounds']['Height']);wid=window['window_id']
                    image_cmd=['/usr/sbin/screencapture','-x','-o','-l',str(wid),str(native/'native-window.png')];i_start=time.monotonic_ns();image=owned_command(native,'image',image_cmd,timeout=max(.001,min(LIMITS['image'],start+255-time.monotonic())))
                    write(native/'image-command.json',{'app_pid':process.pid,'window_id':wid,'command':image_cmd,'exit_code':image.returncode,'started_monotonic_ns':i_start,'finished_monotonic_ns':time.monotonic_ns(),'stdout':image.stdout,'stderr':image.stderr})
                    if image.returncode:raise RuntimeError('owned image capture failed')
                    scale=active_probe['after']['backing_scale'];width=round(window['bounds']['Width']*scale);height=round(window['bounds']['Height']*scale)
                    video_cmd=[str(native/'capture-producer'),'--owned-window',str(process.pid),str(wid),str(native/'native-window.mov'),request['run_id'],'H264-MOV',str(width),str(height)]
                    video_started=time.monotonic_ns();producer_err_file=(native/'capture-stderr.log').open('xb');video=subprocess.Popen(video_cmd,stdin=subprocess.PIPE,stdout=subprocess.PIPE,stderr=producer_err_file,text=True,start_new_session=True)
                    write(native/'owned-producer.json',{'pid':video.pid,'pgid':video.pid,'private_session':True,'app_pid':process.pid,'window_id':wid,'command':video_cmd,'producer_sha256':sha(native/'capture-producer'),'started_monotonic_ns':video_started})
                    life=Lifecycle(process.pid,wid,request['run_id'],video.pid);reader=threading.Thread(target=receive,daemon=True);reader.start();reader_started=True
                if video is not None:
                    try:row=messages.get(timeout=.05)
                    except queue.Empty:row=None
                    if row:
                        events.append(row);write(native/'capture-events.json',events)
                        if row['event']=='failure':raise RuntimeError('capture producer failure: '+row['error'])
                        if row['event']=='ready':
                            life.first_frame(row);ready_at=time.monotonic_ns();write(native/'capture-ready.json',{'app_pid':process.pid,'run_id':request['run_id'],'video_pid':video.pid,'video_started_monotonic_ns':video_started,'ready_monotonic_ns':ready_at,'first_frame':row})
                        elif row['event']=='finished':
                            if life.state!='STOP_REQUESTED':raise RuntimeError('producer finished before protocol stop')
                            video_record=row
                        else:raise RuntimeError('unknown capture event')
                    if ready_at is None and time.monotonic_ns()-video_started>20e9:raise RuntimeError('no accepted first frame before readiness deadline')
                    done=optional(native/'protocol-done.json')
                    if done and life.state=='READY':
                        life.protocol_end(done);life.stop_requested();stop_at=time.monotonic_ns();video.stdin.write('STOP\n');video.stdin.flush()
                    if stop_at is not None and life.state=='STOP_REQUESTED' and time.monotonic_ns()-stop_at>20e9:raise RuntimeError('capture finish/join deadline')
                    if video.poll() is not None and life.state!='RELEASED':
                        video.wait(timeout=1);reader.join(timeout=1)
                        if reader.is_alive():raise RuntimeError('producer reader failed to join')
                        while not messages.empty():
                            extra=messages.get_nowait();events.append(extra)
                            if extra['event']=='failure':raise RuntimeError('capture producer failure: '+extra['error'])
                            if extra['event']=='finished':video_record=extra
                            else:raise RuntimeError('extra/out-of-order producer event')
                        stderr=(native/'capture-stderr.log').read_bytes().decode('utf-8');producer_stderr=stderr;(native/'capture-stdout.log').write_text(''.join(stdout_rows))
                        write(native/'capture-events.json',events)
                        if video_record is None or stderr:raise RuntimeError('missing final writer event or producer stderr')
                        # Import lazily to avoid the runner/verifier source-constant cycle.
                        from verify_pmix_capture import mov_info
                        movie=mov_info(native/'native-window.mov');life.producer_joined(video_record,video.returncode,movie)
                        if group_present(video.pid):raise RuntimeError('producer descendants remain; refusing app release')
                        joined=time.monotonic_ns();receipt={'pid':video.pid,'app_pid':process.pid,'window_id':wid,'command':video_cmd,'producer_sha256':sha(native/'capture-producer'),'started_monotonic_ns':video_started,'ready_monotonic_ns':ready_at,'stop_requested_monotonic_ns':stop_at,'finished_monotonic_ns':joined,'exit_code':video.returncode,'joined_before_app_release':True,'finalization':'SCStream.stopCapture + drain + AVAssetWriter.finishWriting + process.join','first_frame':life.ready,'finished_event':video_record,'stdout':''.join(stdout_rows),'stderr':stderr,'movie':movie}
                        write(native/'video-command.json',receipt);life.release_app();write(native/'capture-complete.json',{'app_pid':process.pid,'run_id':request['run_id'],'video_pid':video.pid,'success':True,'producer_finished_monotonic_ns':joined,'release_monotonic_ns':time.monotonic_ns(),'movie_sha256':sha(native/'native-window.mov')})
                else:threading.Event().wait(.05)
            observations=optional(native/'observations.json')
            if not observations or observations['failures']:raise RuntimeError('missing/failed app observations')
            if observations['binary_sha256']!=sha(binary) or observations['fixture_sha256']!=sha(fixture):raise RuntimeError('binary/fixture identity changed')
            code=0
    except BaseException as e:
        error=str(e);write(native/'runner-error.json',{'error':error,'type':type(e).__name__})
    finally:
        try:
            # Owned-process cleanup is bounded; evidence write failures stay fatal.
            if video is not None:
                try:
                    if video.poll() is None and reader_started:
                        try:video.stdin.write('STOP\n');video.stdin.flush();video.wait(timeout=5)
                        except (BrokenPipeError,OSError,subprocess.TimeoutExpired):pass
                    if video.poll() is None or group_present(video.pid):
                        producer_group_released=drain_owned_group(video,producer_signals,2)
                    else:producer_group_released=True
                    if not producer_group_released:raise RuntimeError('owned producer group not released')
                except BaseException as e:error='owned producer cleanup failed: '+str(e);code=1
            if reader is not None:
                try:
                    reader.join(timeout=2)
                    if reader.is_alive():raise RuntimeError('owned reader still running')
                except BaseException as e:
                    reader_cleanup_error=repr(e);error='owned reader cleanup failed: '+str(e);code=1
                    write(native/'reader-cleanup-error.json',{'error':reader_cleanup_error,'started':reader_started})
            if producer_err_file is not None:
                try:producer_err_file.close()
                except BaseException as e:error='producer stderr file close failed: '+str(e);code=1
            if video is not None:
                for label,pipe in (('stdin',video.stdin),('stdout',video.stdout)):
                    if label=='stdout' and reader is not None and reader.is_alive():continue
                    try:pipe.close()
                    except BaseException as e:
                        error='owned producer pipe close failed: '+str(e);code=1
                        write(native/('producer-'+label+'-cleanup-error.json'),{'error':repr(e)})
                (native/'capture-stdout.log').write_text(''.join(stdout_rows))
                try:write(native/'capture-events.json',[json.loads(line) for line in stdout_rows])
                except json.JSONDecodeError:pass
                write(native/'producer-cleanup.json',{'pid':video.pid,'pgid':video.pid,'private_session':True,
                      'exit_code':video.returncode,'signal':-video.returncode if video.returncode is not None and video.returncode<0 else None,
                      'joined':video.returncode is not None,'owned_group_released':producer_group_released,'signals_sent':producer_signals,
                      'reader_joined':reader_started and not reader.is_alive() and reader_cleanup_error is None,
                      'finished_monotonic_ns':time.monotonic_ns(),'control_state':life.state if life else None,
                      'scope':'owned producer actual cleanup; never native success'})
            if process is not None and process.returncode is None:
                try:
                    process.terminate()
                    try:process.wait(timeout=5)
                    except subprocess.TimeoutExpired:process.kill();process.wait(timeout=2)
                except BaseException as e:error='owned app cleanup failed: '+str(e);code=1
            write(native/'owned-cleanup.json',{'app_pid':process.pid if process else None,'app_exit_code':process.returncode if process else None,'producer_pid':video.pid if video else None,'producer_exit_code':video.returncode if video else None,'producer_reader_joined':reader_started and reader is not None and not reader.is_alive() and reader_cleanup_error is None,'finished_monotonic_ns':time.monotonic_ns(),'scope':'cleanup status; resource accounting only from owned wait4'})
        except BaseException as e:
            error='owned cleanup/evidence failed: '+str(e);code=1
        finally:
            # A cleanup or receipt error must not skip same-target restoration.
            try:
                if before is None:raise RuntimeError('no initial display snapshot; no display mode was set; restore query skipped')
                op='restore' if changed else 'probe'
                restore=display_command('display-restored',op,original if changed else None)
                if display_core(restore['after'])!=display_core(before['after']):error='display restoration/preservation mismatch';code=1
                restored_probe=display_command('display-restored-probe','probe')
                if restored_probe['before']!=restored_probe['after'] or restored_probe['after']!=before['after']:
                    raise RuntimeError('original target mode did not survive restore helper process exit')
            except BaseException as e:
                error='display restoration/preservation failed: '+str(e);code=1
                write(native/'display-restoration-error.json',{'error':error,'display_id':a.display_id,'original_mode_id':original,'mode_change_attempted':changed,'initial_snapshot_available':before is not None})
        write(native/'runner.json',{'exit_code':exit_code,'error':error,'peak_child_rss_bytes':rss,'scope':'owned-child wait4 resource usage; no process enumeration; movie valid/joined before app release','resource_usage':usage_receipt,'ui_roi_directory':ui.name,'source_manifest_sha256':request['source_manifest_sha256'],'capture_state':life.state if life else None})
        for item in native.iterdir():
            if item.name in MARKER_NAMES:
                if not (out/item.name).is_file() or (out/item.name).read_bytes()!=item.read_bytes():
                    raise RuntimeError('immutable output runner marker missing/changed: '+item.name)
            elif item.is_file():shutil.copy2(item,out/item.name)
            elif item.is_dir():shutil.copytree(item,out/item.name)
        write(out/'file-hashes.json',{f.relative_to(out).as_posix():sha(f) for f in sorted(out.rglob('*')) if f.is_file()})
        print(json.dumps({'output':str(out),'native_directory':str(native),'exit_code':code,'error':error,'display_restored':restore},ensure_ascii=False),flush=True)
    return code
if __name__=='__main__':raise SystemExit(main())
