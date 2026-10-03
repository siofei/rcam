"""S5-M2-B immutable-source gate runner. Uses the caller's pinned Cargo environment."""
import argparse,datetime,hashlib,json,os,shutil,subprocess,time
from pathlib import Path
import source_manifest
from run_s5m2_gates import COMMANDS as A2_COMMANDS
ROOT=Path(__file__).resolve().parents[1]
COMMANDS=[(name,cmd) for name,cmd in A2_COMMANDS if name not in ('measure-windows','measure-block','measure-real-cancel','release','release-internal')]
COMMANDS += [
 ('batch-verifier-unit',['python3','-B','scripts/test_verify_batch_drag_native.py']),
 ('batch-release',['cargo','test','--release','--locked','-p','editor-app','batch_drag_tests::','--','--include-ignored','--nocapture','--test-threads=1']),
 ('preview-benchmark',['cargo','test','--release','--locked','-p','editor-app','preview_index_benchmark','--','--ignored','--nocapture','--test-threads=1']),
 ('metal-reference',['cargo','test','--release','--locked','-p','editor-app','native_metal_reference_production_pixel_parity','--','--ignored','--nocapture','--test-threads=1']),
 ('metal-batch',['cargo','test','--release','--locked','-p','editor-app','native_metal_batch_drag_subset_and_outside_world','--','--ignored','--nocapture','--test-threads=1']),
 ('metal-block',['cargo','test','--release','--locked','-p','editor-app','native_metal_block_instance_parity','--','--ignored','--nocapture','--test-threads=1']),
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
            if expected.encode() not in data or ((b'RCAM_BATCH_DRAG_NATIVE_DIR' in data)!=(name=='release-internal')):code=1
            binaries[name]={'path':dest.relative_to(out).as_posix(),'sha256':sha(dest),'commit':expected,'source_manifest_sha256':source,'profile':'release'}
            write(out/'binaries.json',binaries)
        records.append({'id':name,'command':command,'started_utc':utc,'duration_seconds':time.monotonic()-start,'exit_code':code,'commit':commit,'source_manifest_sha256':source,'log':log.name,'sha256':sha(log)})
        write(out/'gates.json',records);print(name,code,round(records[-1]['duration_seconds'],2),flush=True)
        if code:break
    after=source_manifest.contents();(out/'source-after.sha256').write_text(after);status_after=git('status','--porcelain=v1','--untracked-files=all');(out/'status-after.txt').write_text(status_after+'\n')
    okay=before==after and status==status_after and len(records)==len(COMMANDS) and all(r['exit_code']==0 for r in records)
    write(out/'summary.json',{'stage':'S5-M2-B','commit':commit,'clean_worktree':not bool(status),'source_manifest_sha256':source,'unchanged_source':before==after,'unchanged_status':status==status_after,'gates_expected':len(COMMANDS),'gates_passed':sum(r['exit_code']==0 for r in records),'result':('CANDIDATE_GATES_PASS' if status else 'GATES_PASS') if okay else 'FAIL'})
    return 0 if okay else 1
if __name__=='__main__':raise SystemExit(main())
