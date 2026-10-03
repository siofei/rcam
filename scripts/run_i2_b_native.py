"""Owned Mac release window: synthetic egui widget/canvas functional evidence.
Setup helpers are explicitly recorded. No physical OS-input latency claim.
No process enumeration or directory removal. All temporary runs are preserved.
"""
import argparse,datetime,errno,hashlib,json,math,os,shutil,subprocess,tempfile,time,uuid
from pathlib import Path
from run_s5m1_native import WINDOW_SWIFT
def sha(p):return hashlib.sha256(p.read_bytes()).hexdigest()
def write(p,v):p.write_text(json.dumps(v,ensure_ascii=False,indent=2)+'\n')
def steps(copy=False):
    out=[{'kind':k} for k in ('new','b_import','b_camera','b_select_all')]
    conflicts=[('enter','escape',False),('enter','escape',True),('button','escape',False),('button','blur',False),('button','gone',False),('button','ime',False)]
    widget=lambda n:{'kind':'widget','name':n}
    text=lambda v:{'kind':'text','value':v}
    field=lambda n,v:[widget(n),text(v)]
    out += [{'kind':'b_modal','tool':'rotate'},widget('B-area'),widget('transform-apply'),{'kind':'undo'},{'kind':'redo'},{'kind':'undo'}]
    out += [{'kind':'b_modal','tool':'mirror'},widget('B-bounds'),widget('transform-apply'),{'kind':'undo'}]
    out += [{'kind':'b_modal','tool':'move'}]+field('B-X','3')+field('B-Y','-2')+[widget('transform-apply'),{'kind':'undo'}]
    out += [{'kind':'b_modal','tool':'mirror'},widget('transform-vertical'),widget('transform-apply'),{'kind':'undo'}]
    out += [{'kind':'b_modal','tool':'move'},widget('B-pick'),{'kind':'point_click','from':[2.,1.]},widget('T-pick'),{'kind':'zoom','from':[2.,1.],'factor':1.1},{'kind':'pan'},{'kind':'point_click','from':[30.,10.],'outside':True},{'kind':'point_click','from':[15.,5.]}]
    if copy:out.append(widget('transform-copy'))
    out += field('T-X','8')+field('T-Y','-3')+[widget('transform-apply'),{'kind':'undo'},{'kind':'b_select_all'}]
    out += [{'kind':'b_modal','tool':'move'},widget('B-pick'),{'kind':'escape'},{'kind':'escape'}]
    out += [{'kind':'b_modal','tool':'move'}]+field('T-X','NaN')+[{'kind':'escape'}]
    for tool,x,y in [('measure','3','4'),('measure','0','0'),('text','5','6'),('array_base','2','3'),('array_target','-2','5'),('board','7','8')]:
        out += [{'kind':'b_adapter','tool':tool}]+field('adapter-X',x)+field('adapter-Y',y)+[widget('adapter-apply')]
    out += [{'kind':k} for k in ('b_tool_select','b_active_last','b_select_one')]
    out += [{'kind':'b_adapter','tool':'grip'}]+field('adapter-X','22')+field('adapter-Y','4')+[widget('adapter-apply'),{'kind':'undo'}]
    # The real rectangle must still exist: Block Create replaces its source
    # span with an instance, which does not support this fixed-edge Grip.
    for via,cancel,repeat in conflicts:
        out += [{'kind':'b_adapter','tool':'grip'}]+field('adapter-X','22')+field('adapter-Y','4')+[{'kind':'cancel_conflict','name':'adapter-apply','via':via,'cancel':cancel,'repeat':repeat}]
        if cancel=='ime':out += [{'kind':'ime_end'},{'kind':'escape'}]
    out += [{'kind':'b_block_create'},widget('block-origin'),widget('adapter-bounds'),widget('adapter-apply'),widget('block-create'),{'kind':'b_block_place'}]
    out += [widget('block-local'),widget('adapter-bounds'),widget('adapter-pick'),{'kind':'point_click','from':[0.5,0.]},widget('adapter-apply')]
    out += [widget('block-target')]+field('adapter-X','15')+field('adapter-Y','5')+[widget('adapter-apply')]
    # Frozen cancellation matrix. Setup is declared; confirmations are real egui input.
    for tool in ('move','copy','rotate','mirror','vertical'):
        for via,cancel,repeat in conflicts:
            out += [{'kind':'b_modal','tool':tool},{'kind':'cancel_conflict','name':'transform-apply','via':via,'cancel':cancel,'repeat':repeat}]
            if cancel=='ime':out += [{'kind':'ime_end'},{'kind':'escape'}]
    for via,cancel,repeat in conflicts:
        out += [{'kind':'b_block_place'},widget('block-target'),{'kind':'cancel_conflict','name':'adapter-apply','via':via,'cancel':cancel,'repeat':repeat}]
        if cancel=='ime':out += [{'kind':'ime_end'},{'kind':'escape'}]
        out += [{'kind':'b_tool_select'}]
    return out

def feedback_steps():
    angle=.53
    out=[{'kind':k} for k in ('new','b_feedback_import','b_feedback_camera','b_feedback_select')]+[{'kind':'b_modal','tool':'move'},{'kind':'widget','name':'B-pick'}]
    # First10 predeclared warmups and all30 measured samples; no discarded tail.
    for n in range(40):
        x=7+n%8;radius=.25+1/(40*2)
        out.append({'kind':'feedback_move','sample':n,'warmup':n<10,'from':[x+radius*math.cos(angle),1+radius*math.sin(angle)]})
    out += [{'kind':'escape'},{'kind':'escape'}]
    return out

DISPLAY_SWIFT = r'''
import AppKit
import CoreGraphics
import Foundation
let d=CGMainDisplayID()
let mode=CGDisplayCopyDisplayMode(d)
let rows=NSScreen.screens.map { s -> [String:Any] in ["frame":[s.frame.minX,s.frame.minY,s.frame.width,s.frame.height],"backingScaleFactor":s.backingScaleFactor,"maximumFramesPerSecond":s.maximumFramesPerSecond] }
let value:[String:Any] = ["mainDisplayID":d,"pixels":[CGDisplayPixelsWide(d),CGDisplayPixelsHigh(d)],"modeRefreshHz":mode?.refreshRate ?? 0,"screens":rows]
print(String(data:try! JSONSerialization.data(withJSONObject:value,options:[.sortedKeys]),encoding:.utf8)!)
'''
def main():
    p=argparse.ArgumentParser(description=__doc__);p.add_argument('--binary',required=True,type=Path);p.add_argument('--out',required=True,type=Path);p.add_argument('--scratch-root',type=Path,default=Path('/tmp'));p.add_argument('--copy',action='store_true');p.add_argument('--feedback',action='store_true');a=p.parse_args()
    root=Path(__file__).resolve().parents[1];out=a.out.resolve();out.mkdir(parents=True,exist_ok=False)
    native=Path(tempfile.mkdtemp(prefix='rcam-i2-b-native-',dir=a.scratch_root.resolve(strict=True))).resolve();binary=a.binary.resolve(strict=True)
    write(native/'request.json',{'schema_version':2,'stage':'S5-I2-B','run_id':str(uuid.uuid4()),'start_utc':datetime.datetime.now(datetime.timezone.utc).isoformat(),'steps':feedback_steps() if a.feedback else steps(a.copy),'role':'feedback' if a.feedback else 'functional','copy':a.copy,'evidence_version':3,'binary_sha256':sha(binary),'source_manifest_sha256':sha(root/'MANIFEST.sha256'),'fixtures':{n:sha(root/'fixtures/synthetic/s5i2b'/n) for n in ('layer_a.gbr','layer_b.gbr','layer_c.gbr')},'input_scope':'synthetic egui widget/canvas input; new/import/select-all/modal/adapter/camera are setup helpers; not physical OS input','whole_I2_acceptance':'NOT_ACCEPTED','C_native_rounds':'PENDING','performance_fixture_sha256':sha(root/'fixtures/synthetic/s2b3_2/P100K_CIRCLES.gbr') if a.feedback else None,'feedback_protocol':{'clock':'one process monotonic Instant; T0 immediately before egui dispatch; T1 actual post-paint screenshot callback before encoding','camera':[10.,1.,40.],'dpi':2.,'warmups':10,'measured':30,'p95_limit_ms':100,'scope':'controlled synthetic RawInput→production canvas Snap marker→actual surface readback; not OS scanout latency'} if a.feedback else None})
    env={k:v for k,v in os.environ.items() if not k.startswith('RCAM_')};env['RCAM_I2_B_NATIVE_DIR']=str(native);env['RCAM_I2_B_NATIVE_ROOT']=str(a.scratch_root.resolve(strict=True))
    swift=native/'window.swift';swift.write_text(WINDOW_SWIFT);window=native/'window-helper'
    subprocess.run(['/usr/bin/swiftc',str(swift),'-o',str(window)],check=True)
    ds=native/'display.swift';ds.write_text(DISPLAY_SWIFT);display=native/'display-helper';subprocess.run(['/usr/bin/swiftc',str(ds),'-o',str(display)],check=True)
    request=json.loads((native/'request.json').read_text());request['display']=json.loads(subprocess.check_output([str(display)],text=True));write(native/'request.json',request)
    code=None;error=None;start=time.monotonic();video=None;captured=False
    try:
        with (native/'stderr.log').open('w') as log:
            proc=subprocess.Popen([str(binary)],env=env,stdout=log,stderr=subprocess.STDOUT)
            write(native/'owned-process.json',{'pid':proc.pid,'command':[str(binary)],'binary_sha256':sha(binary)})
            while proc.poll() is None:
                elapsed=time.monotonic()-start
                if elapsed>210:proc.terminate();error='runner timeout';break
                if elapsed>2 and not captured:
                    windows=json.loads(subprocess.check_output([str(window),str(proc.pid)],text=True));write(native/'window.json',windows)
                    if windows:
                        video=subprocess.Popen(['/usr/sbin/screencapture','-x','-v','-V180','-l',str(windows[0]['window_id']),str(native/'native-interaction.mov')],stdout=subprocess.PIPE,stderr=subprocess.PIPE);captured=True
                time.sleep(.25)
            code=proc.wait(timeout=10)
    finally:
        if video is not None:
            if video.poll() is None:video.terminate()
            stdout,stderr=video.communicate(timeout=10);write(native/'video-command.json',{'exit_code':video.returncode,'stdout':stdout.decode(errors='replace'),'stderr':stderr.decode(errors='replace')})
        write(native/'runner.json',{'exit_code':code,'error':error,'elapsed_s':time.monotonic()-start,'binary_sha256':sha(binary),'command':[str(binary)],'os':subprocess.check_output(['sw_vers'],text=True)})
        # Retain the original run. Immutable regular-file hardlinks on the same
        # volume avoid duplicating multi-GB frame evidence; EXDEV uses a copy.
        def member(src,dst):
            try:os.link(src,dst)
            except OSError as e:
                if e.errno!=errno.EXDEV:raise
                shutil.copy2(src,dst)
        shutil.copytree(native,out/'raw',copy_function=member)
    print(json.dumps({'directory':str(out/'raw'),'exit_code':code,'error':error}),flush=True)
    return int(code!=0 or error is not None)
if __name__=='__main__':raise SystemExit(main())
