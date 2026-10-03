"""S5-I1 immutable-source gate runner. Uses the caller's pinned Cargo environment."""
import argparse,datetime,hashlib,json,os,shutil,subprocess,time
from pathlib import Path
import source_manifest
from run_batch_drag_gates import COMMANDS as B_COMMANDS
ROOT=Path(__file__).resolve().parents[1]
COMMANDS=[(name,cmd) for name,cmd in B_COMMANDS if name not in ('release','release-internal')]
COMMANDS += [
 ('i1-release',['cargo','test','--release','--locked','-p','editor-app','i1_','--','--include-ignored','--nocapture','--test-threads=1']),
 ('i1-verifier-negative',['python3','-B','scripts/test_verify_i1_native.py']),
 ('release',['cargo','build','--release','--locked','-p','editor-app']),
 ('release-internal',['cargo','build','--release','--locked','-p','editor-app','--features','internal-evidence'])]
def sha(p):return hashlib.sha256(p.read_bytes()).hexdigest()
def write(p,v):p.write_text(json.dumps(v,ensure_ascii=False,indent=2)+'\n')
def git(*a):return subprocess.check_output(['git',*a],cwd=ROOT,text=True).strip()
def main():
    p=argparse.ArgumentParser(description=__doc__);p.add_argument('--out',required=True,type=Path);p.add_argument('--allow-dirty',action='store_true');a=p.parse_args()
    out=a.out.resolve();out.mkdir(parents=True,exist_ok=False);status=git('status','--porcelain=v1','--untracked-files=all');commit=git('rev-parse','HEAD')
    if status and not a.allow_dirty:p.error('clean source required')
    before=source_manifest.contents();require=(ROOT/'MANIFEST.sha256').read_text()==before
    if not require:p.error('regenerate source manifest first')
    (out/'source-before.sha256').write_text(before);(out/'status-before.txt').write_text(status+'\n');source=sha(ROOT/'MANIFEST.sha256')
    env=dict(os.environ);target=Path(env.get('CARGO_TARGET_DIR',ROOT/'target'));records=[];binaries={}
    write(out/'environment.json',{'os':subprocess.check_output(['sw_vers'],text=True),'rustc':subprocess.check_output(['rustc','-Vv'],text=True),'cargo':subprocess.check_output(['cargo','-V'],text=True),'target':str(target),'jobs':env.get('CARGO_BUILD_JOBS'),'offline':env.get('CARGO_NET_OFFLINE')})
    for name,command in COMMANDS:
        start=time.monotonic();utc=datetime.datetime.now(datetime.timezone.utc).isoformat();log=out/(name+'.log')
        with log.open('w') as stream:code=subprocess.run(command,cwd=ROOT,env=env,stdout=stream,stderr=subprocess.STDOUT).returncode
        if name=='service-boundary' and code==0:
            forbidden=[n for n in ('egui','eframe','wgpu','winit','raw-window-handle') if any(l.lstrip(' │├─└').startswith(n+' v') for l in log.read_text().splitlines())]
            write(out/'service-boundary.json',{'forbidden_dependencies':forbidden});code=int(bool(forbidden))
        if name in ('release','release-internal') and code==0:
            dest=out/'bin'/('editor-app-'+name);dest.parent.mkdir(exist_ok=True);shutil.copy2(target/'release/editor-app',dest)
            data=dest.read_bytes();expected=commit+('-dirty' if status else '')
            if expected.encode() not in data or ((b'RCAM_I1_NATIVE_DIR' in data)!=(name=='release-internal')):code=1
            binaries[name]={'path':dest.relative_to(out).as_posix(),'sha256':sha(dest),'commit':expected,'source_manifest_sha256':source,'profile':'release'}
            write(out/'binaries.json',binaries)
        records.append({'id':name,'command':command,'started_utc':utc,'duration_seconds':time.monotonic()-start,'exit_code':code,'commit':commit,'source_manifest_sha256':source,'log':log.name,'sha256':sha(log)})
        write(out/'gates.json',records);print(name,code,round(records[-1]['duration_seconds'],2),flush=True)
        if code:break
    after=source_manifest.contents();(out/'source-after.sha256').write_text(after);status_after=git('status','--porcelain=v1','--untracked-files=all');(out/'status-after.txt').write_text(status_after+'\n')
    okay=before==after and status==status_after and len(records)==len(COMMANDS) and all(r['exit_code']==0 for r in records)
    write(out/'summary.json',{'stage':'S5-I1','commit':commit,'clean_worktree':not bool(status),'source_manifest_sha256':source,'unchanged_source':before==after,'unchanged_status':status==status_after,'gates_expected':len(COMMANDS),'gates_passed':sum(r['exit_code']==0 for r in records),'result':('CANDIDATE_GATES_PASS' if status else 'GATES_PASS') if okay else 'FAIL'})
    return 0 if okay else 1
if __name__=='__main__':raise SystemExit(main())
