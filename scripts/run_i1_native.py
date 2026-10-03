"""Mac release functional evidence through real egui input/worker/Metal callbacks."""
import argparse, datetime, hashlib, json, os, shutil, subprocess, tempfile, time, uuid
from pathlib import Path
from run_s5m1_native import WINDOW_SWIFT

def sha(p): return hashlib.sha256(p.read_bytes()).hexdigest()
def write(p,v): p.write_text(json.dumps(v,ensure_ascii=False,indent=2)+'\n')
def steps(stress=False):
    out=[{'kind':k} for k in ('new','import','camera')]
    click=lambda **kw:{'kind':'click','from':[0,0],**kw}
    box={'kind':'drag','from':[-3,-3],'to':[9,9]}
    out += [click(),click(),click(),click(shift=True),click(ctrl=True),click(),{'kind':'zoom','from':[0,0],'factor':1.2},click(),{'kind':'pan'},click(),{'kind':'camera'},click(),box,{'kind':'drag','from':[0,0],'to':[2,1]}]
    out += [{'kind':k} for k in ('undo','redo','undo','rotate','undo','mirror','undo','duplicate','undo','redo','undo')]
    out += [box,{'kind':'delete'},{'kind':'undo'},box,{'kind':'locked','layer':'upper','value':True},{'kind':'move','dx':2,'dy':1,'expected_error':'LAYER_LOCKED'},{'kind':'locked','layer':'upper','value':False},{'kind':'visible','layer':'upper','value':False},click(),{'kind':'visible','layer':'upper','value':True},box,{'kind':'drag','from':[0,0],'to':[2,1],'escape':True}]
    out += [{'kind':k} for k in ('save','reopen')]+[{'kind':'export','layer':n} for n in ('lower','upper')]
    out += [{"kind":"new"},{"kind":"reimport_exports"}]
    if stress:
        out += [{'kind':'camera'}]
        for n in range(24):
            out += [click(shift=True,hold_frames=n%4),click(ctrl=True,hold_frames=(n+2)%5),click(hold_frames=n%3)]
        out += [box,click(),{'kind':'recovery'},click(),{'kind':'recovery'},click(shift=True),click(),click(),{'kind':'recovery'},click()]
    return out

def main():
    p=argparse.ArgumentParser(description=__doc__);p.add_argument('--binary',type=Path,required=True);p.add_argument('--out',type=Path,required=True);p.add_argument('--stress',action='store_true');p.add_argument('--diagnostic',choices=['foreign_move','pointer_gone','early_release']);a=p.parse_args()
    root=Path(__file__).resolve().parents[1];out=a.out.resolve();out.mkdir(parents=True,exist_ok=False)
    native=Path(tempfile.mkdtemp(prefix='rcam-i1-native-',dir='/tmp')).resolve();binary=a.binary.resolve()
    request={'run_id':str(uuid.uuid4()),'start_utc':datetime.datetime.now(datetime.timezone.utc).isoformat(),'schema_version':2,'stage':'S5-I1','steps':steps(a.stress),'stress':a.stress,'diagnostic':a.diagnostic,'binary_sha256':sha(binary),'source_manifest_sha256':sha(root/'MANIFEST.sha256'),'fixtures':{n:sha(root/'fixtures/synthetic/s5i1'/n) for n in ('lower.gbr','upper.gbr')}};write(native/'request.json',request)
    env={k:v for k,v in os.environ.items() if not k.startswith('RCAM_')};env['RCAM_I1_NATIVE_DIR']=str(native)
    window=native/'window.swift';window.write_text(WINDOW_SWIFT)
    window_binary=native/'window-helper'
    subprocess.run(['/usr/bin/swiftc',str(window),'-o',str(window_binary)],check=True)
    samples=[];video=None;captured=False;start=time.monotonic();error=None
    try:
        with (native/'stderr.log').open('w') as log:
            proc=subprocess.Popen([str(binary)],env=env,stdout=log,stderr=subprocess.STDOUT)
            while proc.poll() is None:
                elapsed=time.monotonic()-start
                if elapsed>210:proc.terminate();error='runner timeout';break
                sample=subprocess.run(['/bin/ps','-o','rss=,%cpu=,etime=','-p',str(proc.pid)],capture_output=True,text=True);samples.append({'elapsed_s':elapsed,'ps':sample.stdout.strip()})
                if elapsed>2 and not captured:
                    windows=json.loads(subprocess.check_output([str(window_binary),str(proc.pid)],text=True));write(native/'window.json',windows)
                    if windows:
                        video=subprocess.Popen(['/usr/sbin/screencapture','-x','-v','-V'+str(90 if a.stress else 70),'-l',str(windows[0]['window_id']),str(native/'native-interaction.mov')],stdout=subprocess.PIPE,stderr=subprocess.PIPE);captured=True
                time.sleep(.25)
            code=proc.wait(timeout=10)
    finally:
        if video is not None:
            if video.poll() is None:video.terminate()
            stdout,stderr=video.communicate(timeout=10);write(native/'video-command.json',{'exit_code':video.returncode,'stdout':stdout.decode(errors='replace'),'stderr':stderr.decode(errors='replace')})
        write(native/'runner.json',{'exit_code':code if 'code' in locals() else None,'error':error,'elapsed_s':time.monotonic()-start,'binary_sha256':sha(binary),'command':[str(binary)],'utc':datetime.datetime.now(datetime.timezone.utc).isoformat(),'os':subprocess.check_output(['sw_vers'],text=True)})
        write(native/'process-samples.json',samples)
        shutil.copytree(native,out/'raw')
    print(json.dumps({'directory':str(out/'raw'),'exit_code':code,'error':error}),flush=True)
    return int(code!=0 or error is not None)
if __name__=='__main__':raise SystemExit(main())
