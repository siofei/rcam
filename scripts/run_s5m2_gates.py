"""Record the A2 gate sequence against one immutable source snapshot.
Uses the caller's pinned Cargo environment. No cleanup, commit or push.
"""
import argparse
import datetime
import hashlib
import json
import os
from pathlib import Path
import shutil
import subprocess
import time
import source_manifest

ROOT = Path(__file__).resolve().parents[1]
COMMANDS = [
 ('fmt',['cargo','fmt','--all','--','--check']),
 ('check',['cargo','check','--workspace','--all-targets','--locked']),
 ('clippy',['cargo','clippy','--workspace','--all-targets','--locked','--','-D','warnings']),
 ('clippy-internal-evidence',['cargo','clippy','--locked','-p','editor-app','--features','internal-evidence','--all-targets','--','-D','warnings']),
 ('workspace-test',['cargo','test','--workspace','--locked']),
 ('automation-contract',['cargo','test','--locked','-p','editor-service','--test','automation_contract']),
 ('headless-workflow',['cargo','test','--locked','-p','editor-service','--test','headless_workflow']),
 ('service-boundary',['cargo','tree','--locked','-p','editor-service','-e','normal']),
 ('verifier-unit',['python3','-B','scripts/test_verify_s5m2_evidence.py']),
 ('package-unit',['python3','-B','scripts/test_package_source.py']),
 ('source-manifest',['python3','-B','scripts/source_manifest.py','--check']),
 ('measure-windows',['cargo','test','--release','--locked','-p','editor-app','a2_measure_cancellation_windows','--','--ignored','--nocapture']),
 ('measure-block',['cargo','test','--release','--locked','-p','editor-app','s4b2_block_release_performance','--','--ignored','--nocapture']),
 ('measure-real-cancel',['cargo','test','--release','--locked','-p','editor-service','--test','task_closeout','a2_real_import_cancel_joins_worker_with_zero_mutation','--','--ignored','--nocapture']),
 ('release',['cargo','build','--release','--locked','-p','editor-app']),
 ('release-internal',['cargo','build','--release','--locked','-p','editor-app','--features','internal-evidence']),
]

def sha(path): return hashlib.sha256(path.read_bytes()).hexdigest()
def write(path, value): path.write_text(json.dumps(value,ensure_ascii=False,indent=2)+'\n')
def git(*args): return subprocess.check_output(['git',*args],cwd=ROOT,text=True).strip()

def main():
    parser=argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--out',required=True,type=Path)
    parser.add_argument('--fixtures',required=True,type=Path,help='JSON array of explicitly authorized local fixture paths; not copied')
    parser.add_argument('--allow-dirty',action='store_true',help='Candidate only; cannot prove clean closeout')
    args=parser.parse_args(); out=args.out.resolve(); out.mkdir(parents=True,exist_ok=False)
    status=git('status','--porcelain=v1','--untracked-files=all')
    if status and not args.allow_dirty: parser.error('clean source required')
    commit=git('rev-parse','HEAD'); identity=source_manifest.contents()
    if (ROOT/'MANIFEST.sha256').read_text() != identity: parser.error('source manifest must match before gates')
    source_sha=sha(ROOT/'MANIFEST.sha256')
    (out/'source-before.sha256').write_text(identity); (out/'status-before.txt').write_text(status+'\n')
    target=Path(os.environ.get('CARGO_TARGET_DIR',ROOT/'target'))
    fixtures = json.loads(args.fixtures.read_text())
    fixture_inventory = [{'index':i,'sha256':sha(Path(p)),'bytes':Path(p).stat().st_size} for i,p in enumerate(fixtures)]
    write(out/'fixture-inventory.json',fixture_inventory)
    env = dict(os.environ); env['RCAM_A2_FIXTURES']=str(args.fixtures.resolve())
    hardware=json.loads(subprocess.check_output(['system_profiler','SPHardwareDataType','SPDisplaysDataType','-json']))
    hw=hardware['SPHardwareDataType'][0]; gpu=hardware['SPDisplaysDataType'][0]
    write(out/'environment.json',{'macos':subprocess.check_output(['sw_vers','-productVersion'],text=True).strip(),
        'cpu':hw['chip_type'],'ram':hw['physical_memory'],'gpu':gpu['sppci_model'],
        'displays':[{'pixels':d.get('_spdisplays_pixels'),'resolution_refresh':d.get('spdisplays_resolution')} for d in gpu['spdisplays_ndrvs']]})
    records=[]; binaries={}
    for name,command in COMMANDS:
        start=time.monotonic(); started_utc=datetime.datetime.now(datetime.timezone.utc).isoformat(); log=out/(name+'.log')
        with log.open('w') as stream:
            result=subprocess.run(command,cwd=ROOT,env=env,stdout=stream,stderr=subprocess.STDOUT)
        code=result.returncode
        if name=='service-boundary' and code==0:
            forbidden=('egui','eframe','wgpu','winit','raw-window-handle')
            violations=[n for n in forbidden if any(line.lstrip(' │├─└').startswith(n+' v') for line in log.read_text().splitlines())]
            write(out/'service-boundary.json',{'forbidden_dependencies':violations})
            if violations: code=1
        if name in ('release','release-internal') and code==0:
            key='release' if name=='release' else 'internal-evidence'
            dest=out/'bin'/('editor-app-'+key); dest.parent.mkdir(exist_ok=True)
            shutil.copy2(target/'release/editor-app',dest)
            data=dest.read_bytes(); expected=commit+('-dirty' if status else '')
            # Inspect actual Mach-O bytes, not a newly assigned package label.
            if expected.encode() not in data: code=1
            if key=='release' and b'RCAM_A2_NATIVE_DIR' in data: code=1
            observed=out/(key+'-identity.json')
            write(observed,{'commit':expected,'sha256':sha(dest),'source':'git-dirty' if status else 'git-clean',
                'method':'actual Mach-O embedded commit bytes plus recorded clean/dirty build source; internal native startup independently reports identity'})
            binaries[key]={'commit':commit,'profile':'release','source_manifest_sha256':source_sha,
                'path':dest.relative_to(out).as_posix(),'sha256':sha(dest),'observed_identity':observed.name}
        records.append({'id':name,'started_utc':started_utc,'profile':'release' if '--release' in command else 'dev/test/tool','command':command,'exit_code':code,'duration_seconds':time.monotonic()-start,
            'commit':commit,'source_manifest_sha256':source_sha,'log':log.name,'sha256':sha(log)})
        write(out/'gates.json',records); write(out/'binaries.json',binaries)
        print(name,code,round(records[-1]['duration_seconds'],2),flush=True)
        if code: break
    after=source_manifest.contents(); (out/'source-after.sha256').write_text(after)
    status_after=git('status','--porcelain=v1','--untracked-files=all'); (out/'status-after.txt').write_text(status_after+'\n')
    okay=identity==after and status==status_after and len(records)==len(COMMANDS) and all(r['exit_code']==0 for r in records)
    write(out/'summary.json',{'stage':'S5-M2-A2','commit':commit,'clean_worktree':not bool(status),
        'source_manifest_sha256':source_sha,'unchanged_source':identity==after,'unchanged_status':status==status_after,
        'gates_expected':len(COMMANDS),'gates_recorded':len(records),'gates_passed':sum(r['exit_code']==0 for r in records),
        'result':'CANDIDATE_GATES_PASS' if okay and status else 'GATES_PASS' if okay else 'FAIL'})
    return 0 if okay else 1

if __name__=='__main__': raise SystemExit(main())
