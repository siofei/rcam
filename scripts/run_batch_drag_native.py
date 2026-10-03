"""Local release native S5-M2-B input/Metal measurements, with isolated settings."""
import argparse,datetime,hashlib,json,os,shutil,subprocess,sys,tempfile,time
from pathlib import Path
from verify_batch_drag_native import verify, PROTOCOL, sha

def write(p,v):p.write_text(json.dumps(v,ensure_ascii=False,indent=2)+'\n')
def main():
    p=argparse.ArgumentParser(description=__doc__)
    p.add_argument('--binary',required=True,type=Path);p.add_argument('--out',required=True,type=Path)
    p.add_argument('--counts',default='100,500,1000,5000');p.add_argument('--rounds',type=int,default=3)
    p.add_argument('--modes',default='move,escape,blur,pointergone,new-project')
    a=p.parse_args();require=sys.platform=='darwin'
    if not require:p.error('native macOS runner only')
    root=Path(__file__).resolve().parents[1];protocol=json.loads(PROTOCOL.read_text());fixture=root/protocol['fixture']
    binary=a.binary.resolve();out=a.out.resolve();out.mkdir(parents=True,exist_ok=False)
    bs=sha(binary);fs=sha(fixture);records=[]
    hardware=json.loads(subprocess.check_output(['system_profiler','SPHardwareDataType','SPDisplaysDataType','-json']))
    hw=hardware['SPHardwareDataType'][0];gpu=hardware['SPDisplaysDataType'][0]
    write(out/'environment.json',{'os':subprocess.check_output(['sw_vers'],text=True),'machine':hw['machine_model'],
        'cpu':hw['chip_type'],'ram':hw['physical_memory'],'gpu':gpu['sppci_model'],
        'displays':[{'pixels':d.get('_spdisplays_pixels'),'resolution_refresh':d.get('spdisplays_resolution')} for d in gpu['spdisplays_ndrvs']],
        'memory_bytes':int(subprocess.check_output(['sysctl','-n','hw.memsize'],text=True)),
        'power':subprocess.check_output(['pmset','-g','custom'],text=True),'scope':'macOS native Metal; power state not changed','protocol':protocol})
    tasks=[(r,n,'move') for r in range(1,a.rounds+1) for n in map(int,a.counts.split(',')) if 'move' in a.modes.split(',')]
    tasks.extend((1,1000,m) for m in a.modes.split(',') if m!='move')
    for round_,n,mode in tasks:
        name=f'round-{round_}-{n}-{mode}';directory=Path(tempfile.mkdtemp(prefix='rcam-batch-drag-',dir='/tmp')).resolve()
        request={'schema_version':2,'selected':n,'mode':mode,'round':round_,'run_id':out.name+'-'+name,'fixture':str(fixture),'fixture_sha256':fs,'protocol_sha256':sha(PROTOCOL),'start_utc':datetime.datetime.now(datetime.timezone.utc).isoformat()}
        write(directory/'request.json',request)
        env={k:v for k,v in os.environ.items() if not k.startswith('RCAM_')};env['RCAM_BATCH_DRAG_NATIVE_DIR']=str(directory)
        start=time.monotonic();code=None;error=None;samples=[]
        with (directory/'stdout.log').open('w') as stdout,(directory/'stderr.log').open('w') as stderr:
            proc=subprocess.Popen([str(binary)],env=env,stdout=stdout,stderr=stderr)
            try:
                while proc.poll() is None:
                    if time.monotonic()-start>135:raise RuntimeError('native timeout')
                    raw=subprocess.run(['ps','-p',str(proc.pid),'-o','rss=,%cpu=,stat='],capture_output=True,text=True)
                    if raw.returncode==0 and raw.stdout.strip():
                        rss,cpu,state=raw.stdout.split();samples.append({'elapsed_seconds':time.monotonic()-start,'rss_bytes':int(rss)*1024,'cpu_percent':float(cpu),'process_state':state,'completed_report_present':(directory/'observations.json').is_file()})
                    elif 'not permitted' in raw.stderr.lower():raise RuntimeError('process metrics permission denied: '+raw.stderr)
                    time.sleep(.25)
                code=proc.returncode
            except (OSError,ValueError,RuntimeError) as e:
                error=str(e);proc.terminate()
                try:code=proc.wait(timeout=10)
                except subprocess.TimeoutExpired:proc.kill();code=proc.wait()
        write(directory/'process-samples.json',samples)
        write(directory/'runner.json',{'exit_code':code,'error':error,'binary_sha256':bs,'fixture_sha256':fs,'elapsed_seconds':time.monotonic()-start})
        try:
            if sha(binary)!=bs or sha(fixture)!=fs:raise RuntimeError('binary/fixture changed')
            summary=verify(directory)
        except (OSError,ValueError,KeyError,TypeError,RuntimeError) as e:error=str(e);summary={'error':error}
        write(directory/'verification.json',summary);shutil.copytree(directory,out/name)
        records.append({'round':round_,'selected':n,'mode':mode,'directory':name,'result':'PASS' if error is None else 'FAIL','summary':summary})
        write(out/'native-index.json',records);print(json.dumps(records[-1],ensure_ascii=False),flush=True)
        if error:return 1
    return 0
if __name__=='__main__':raise SystemExit(main())
