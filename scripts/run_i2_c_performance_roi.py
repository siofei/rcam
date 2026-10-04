"""Owned P100K/selected5000 warmup10s/move10s/commit/Undo/Redo with UI ROI.
Investigation only; no PMIX or global P100K performance acceptance.
"""
import argparse,datetime,errno,hashlib,json,os,shutil,subprocess,sys,tempfile,time
from pathlib import Path
p=argparse.ArgumentParser();p.add_argument('--source',required=True,type=Path);p.add_argument('--binary',required=True,type=Path);p.add_argument('--out',required=True,type=Path);a=p.parse_args();source=a.source.resolve();out=a.out.resolve();out.mkdir(parents=True,exist_ok=False);native=Path(tempfile.mkdtemp(prefix='rcam-batch-drag-',dir='/tmp')).resolve();ui=Path(tempfile.mkdtemp(prefix='rcam-i2-c-ui-',dir=out.parent)).resolve();binary=a.binary.resolve(strict=True)
sys.path.insert(0,str(source/'scripts'));from run_s5m1_native import WINDOW_SWIFT
from run_i2_b_native import DISPLAY_SWIFT
sha=lambda p:hashlib.sha256(p.read_bytes()).hexdigest()
def write(path,value):path.write_text(json.dumps(value,ensure_ascii=False,indent=2)+'\n')
protocol=source/'fixtures/synthetic/s5m2b/protocol.json';fixture=source/'fixtures/synthetic/s2b3_2/P100K_CIRCLES.gbr';request={'schema_version':2,'selected':5000,'mode':'move','round':1,'run_id':out.name,'fixture':str(fixture),'fixture_sha256':sha(fixture),'protocol_sha256':sha(protocol),'start_utc':datetime.datetime.now(datetime.timezone.utc).isoformat(),'stage':'S5-I2-C-menu-investigation','alt_bypass':True,'alt_scope':'actual Alt modifier throughout pointer press/hold/release, existing B contour bypass; global prefs unchanged','source_manifest_sha256':sha(source/'MANIFEST.sha256'),'binary_sha256':sha(binary),'ROI':str(ui),'scope':'fixed native B performance scene; UI recorder overhead present; not PMIX/globalP100K performance acceptance'};write(native/'request.json',request)
for name,text in [('window',WINDOW_SWIFT),('display',DISPLAY_SWIFT)]:
 swift=native/(name+'.swift');swift.write_text(text);subprocess.run(['/usr/bin/swiftc',str(swift),'-o',str(native/(name+'-helper'))],check=True)
request['display']=json.loads(subprocess.check_output([str(native/'display-helper')],text=True));write(native/'request.json',request)
env={k:v for k,v in os.environ.items() if not k.startswith('RCAM_')};env.update(RCAM_BATCH_DRAG_NATIVE_DIR=str(native),RCAM_UI_ROI_DIR=str(ui),RCAM_UI_ROI_ROOT=str(out.parent));code=None;error=None;start=time.monotonic();video=None;captured=False;usage=None;child_elapsed=None
try:
 with (native/'stdout.log').open('w') as stdout,(native/'stderr.log').open('w') as stderr:
  proc=subprocess.Popen([str(binary)],env=env,stdout=stdout,stderr=stderr);write(native/'owned-process.json',{'pid':proc.pid,'command':[str(binary)],'binary_sha256':sha(binary)})
  while code is None:
   child,status,observed=os.wait4(proc.pid,os.WNOHANG)
   if child==proc.pid:
    code=os.waitstatus_to_exitcode(status);proc.returncode=code;usage=observed;child_elapsed=time.monotonic()-start;break
   elapsed=time.monotonic()-start
   if elapsed>135 and error is None:proc.terminate();error='owned native timeout'
   if elapsed>145:raise subprocess.TimeoutExpired([str(binary)],145)
   if elapsed>3 and not captured:
    windows=json.loads(subprocess.check_output([str(native/'window-helper'),str(proc.pid)],text=True));write(native/'window.json',windows)
    if windows:
     video=subprocess.Popen(['/usr/sbin/screencapture','-x','-v','-V25','-l',str(windows[0]['window_id']),str(native/'performance-window.mov')],stdout=subprocess.PIPE,stderr=subprocess.PIPE);captured=True
   time.sleep(.25)
  assert code is not None
finally:
 if video is not None:
  if video.poll() is None:video.terminate()
  stdout,stderr=video.communicate(timeout=10);write(native/'video-command.json',{'exit_code':video.returncode,'stdout':stdout.decode(errors='replace'),'stderr':stderr.decode(errors='replace'),'pass_claim':False,'file_bytes':(native/'performance-window.mov').stat().st_size if (native/'performance-window.mov').exists() else None})
 accounting={'method':'macOS wait4 exact owned application child; ru_maxrss bytes; CPU seconds','pid':proc.pid,'rss_peak_bytes':int(usage.ru_maxrss) if usage is not None else None,'user_cpu_s':usage.ru_utime if usage is not None else None,'system_cpu_s':usage.ru_stime if usage is not None else None,'child_elapsed_s':child_elapsed}
 write(native/'runner.json',{'owned_child_rusage':accounting,'exit_code':code,'error':error,'elapsed_s':time.monotonic()-start,'binary_sha256':sha(binary),'fixture_sha256':request['fixture_sha256'],'command':[str(binary)]})
 def member(src,dst):
  try:os.link(src,dst)
  except OSError as e:
   if e.errno!=errno.EXDEV:raise
   shutil.copy2(src,dst)
 shutil.copytree(native,out/'raw',copy_function=member);shutil.copytree(ui,out/'ui-roi',copy_function=member)
print(json.dumps({'directory':str(out),'exit_code':code,'error':error},ensure_ascii=False),flush=True);raise SystemExit(int(code!=0 or error is not None))
