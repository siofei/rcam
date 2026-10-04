"""Owned-only native C runner; preserves all data and does not query processes."""
import argparse,datetime,errno,hashlib,json,os,shutil,subprocess,sys,tempfile,time,uuid
from pathlib import Path
p=argparse.ArgumentParser();p.add_argument('--source',required=True,type=Path);p.add_argument('--binary',required=True,type=Path);p.add_argument('--out',required=True,type=Path);p.add_argument('--smoke',action='store_true');a=p.parse_args();source=a.source.resolve();out=a.out.resolve();out.mkdir(parents=True,exist_ok=False);scratch=out.parent;native=Path(tempfile.mkdtemp(prefix='rcam-i2-c-native-',dir=scratch)).resolve();ui=Path(tempfile.mkdtemp(prefix='rcam-i2-c-ui-',dir=scratch)).resolve();binary=a.binary.resolve(strict=True)
sys.path.insert(0,str(source/'scripts'));from run_s5m1_native import WINDOW_SWIFT
from run_i2_b_native import DISPLAY_SWIFT
sha=lambda p:hashlib.sha256(p.read_bytes()).hexdigest()
def write(path,value):path.write_text(json.dumps(value,ensure_ascii=False,indent=2)+'\n')
from i2_c_protocol import steps as c_steps
steps=c_steps(a.smoke)
request={'schema_version':2,'stage':'S5-I2-C','run_id':str(uuid.uuid4()),'start_utc':datetime.datetime.now(datetime.timezone.utc).isoformat(),'steps':steps,'role':'functional','evidence_version':3,'binary_sha256':sha(binary),'source_manifest_sha256':sha(source/'MANIFEST.sha256'),'fixtures':{n:sha(source/'fixtures/synthetic/s5i2b'/n) for n in ('layer_a.gbr','layer_b.gbr','layer_c.gbr')},'C_fixtures':{'material.gbr':sha(source/'fixtures/synthetic/s5i2c/material.gbr')},'input_scope':'synthetic actual egui menu/widget/canvas input; preferences setup/reload, discard synthetic session/import/select/modal/camera explicitly declared helpers; not physical OS input','whole_I2_acceptance':'NOT_ACCEPTED','user_flicker_report':'OPEN','source':str(source),'UI_ROI':str(ui),'smoke':a.smoke};assert len(steps)<=400;write(native/'request.json',request)
window_swift=native/'window.swift';window_swift.write_text(WINDOW_SWIFT);window=native/'window-helper';subprocess.run(['/usr/bin/swiftc',str(window_swift),'-o',str(window)],check=True)
display_swift=native/'display.swift';display_swift.write_text(DISPLAY_SWIFT);display=native/'display-helper';subprocess.run(['/usr/bin/swiftc',str(display_swift),'-o',str(display)],check=True);request['display']=json.loads(subprocess.check_output([str(display)],text=True));write(native/'request.json',request)
env={k:v for k,v in os.environ.items() if not k.startswith('RCAM_')};env.update(RCAM_I2_C_NATIVE_DIR=str(native),RCAM_I2_C_NATIVE_ROOT=str(scratch),RCAM_UI_ROI_DIR=str(ui),RCAM_UI_ROI_ROOT=str(scratch));code=None;error=None;video=None;start=time.monotonic();captured=False
try:
 with (native/'stderr.log').open('w') as log:
  proc=subprocess.Popen([str(binary)],env=env,stdout=log,stderr=subprocess.STDOUT);write(native/'owned-process.json',{'pid':proc.pid,'command':[str(binary)],'binary_sha256':sha(binary)})
  while proc.poll() is None:
   elapsed=time.monotonic()-start
   if elapsed>210:proc.terminate();error='runner timeout';break
   if elapsed>3 and not captured:
    windows=json.loads(subprocess.check_output([str(window),str(proc.pid)],text=True));write(native/'window.json',windows)
    if windows:
     video=subprocess.Popen(['/usr/sbin/screencapture','-x','-v','-V35','-l',str(windows[0]['window_id']),str(native/'native-interaction.mov')],stdout=subprocess.PIPE,stderr=subprocess.PIPE);captured=True
   time.sleep(.25)
  code=proc.wait(timeout=10)
finally:
 if video is not None:
  if video.poll() is None:video.terminate()
  stdout,stderr=video.communicate(timeout=10);write(native/'video-command.json',{'exit_code':video.returncode,'stdout':stdout.decode(errors='replace'),'stderr':stderr.decode(errors='replace'),'pass_claim':False,'file_bytes':(native/'native-interaction.mov').stat().st_size if (native/'native-interaction.mov').exists() else None})
 write(native/'runner.json',{'exit_code':code,'error':error,'elapsed_s':time.monotonic()-start,'binary_sha256':sha(binary),'command':[str(binary)],'os':subprocess.check_output(['sw_vers'],text=True)})
 def member(src,dst):
  try:os.link(src,dst)
  except OSError as e:
   if e.errno!=errno.EXDEV:raise
   shutil.copy2(src,dst)
 shutil.copytree(native,out/'raw',copy_function=member);shutil.copytree(ui,out/'ui-roi',copy_function=member)
print(json.dumps({'directory':str(out),'exit_code':code,'error':error,'steps':len(steps)},ensure_ascii=False),flush=True);raise SystemExit(int(code!=0 or error is not None))
