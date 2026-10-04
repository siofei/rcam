"""S5-M2-C resumed PMIX and frozen I1/I2 immutable-source gate runner. Uses the caller's pinned Cargo environment."""
import argparse,datetime,hashlib,json,os,shutil,subprocess,time
from pathlib import Path
import source_manifest
ROOT=Path(__file__).resolve().parents[1]
from run_i2_c_gates import COMMANDS as C_COMMANDS
COMMANDS = C_COMMANDS[:-3] + [
 ('pmix-cpu-release',['cargo','test','--release','--locked','-p','editor-app','pmix_tests::','--','--include-ignored','--nocapture','--test-threads=1']),
 ('pmix-a2-regression',['cargo','test','--release','--locked','-p','editor-app','s5m2_tests::','--','--include-ignored','--nocapture','--test-threads=1']),
 ('pmix-metal-reference',['cargo','test','--release','--locked','-p','editor-app','native_metal_pmix_reference_parity','--','--ignored','--nocapture','--test-threads=1']),
] + C_COMMANDS[-3:]

def sha(p):return hashlib.sha256(p.read_bytes()).hexdigest()
def write(p,v):p.write_text(json.dumps(v,ensure_ascii=False,indent=2)+'\n')
def git(*a):return subprocess.check_output(['git',*a],cwd=ROOT,text=True).strip()
def main():
    p=argparse.ArgumentParser(description=__doc__);p.add_argument('--out',required=True,type=Path);p.add_argument('--allow-dirty',action='store_true');p.add_argument('--a2-fixtures',required=True,type=Path,help='original authorized A2 JSON path list, kept local and not copied');a=p.parse_args()
    out=a.out.resolve();out.mkdir(parents=True,exist_ok=False);status=git('status','--porcelain=v1','--untracked-files=all');commit=git('rev-parse','HEAD')
    if status and not a.allow_dirty:p.error('clean source required')
    before=source_manifest.contents();require=(ROOT/'MANIFEST.sha256').read_text()==before
    if not require:p.error('regenerate source manifest first')
    (out/'source-before.sha256').write_text(before);(out/'status-before.txt').write_text(status+'\n');source=sha(ROOT/'MANIFEST.sha256')
    env=dict(os.environ);env['RCAM_I2_A_GATES_OUT']=str(out);env['RCAM_PMIX_FIXTURE_OUT']=str(ROOT/'fixtures/synthetic/s5m2c/MIX_WORKFLOW.rcam');env['RCAM_A2_FIXTURES']=str(a.a2_fixtures.resolve());target=Path(env.get('CARGO_TARGET_DIR',ROOT/'target'));records=[];binaries={}
    fixture_hashes=[sha(Path(path)) for path in json.loads(a.a2_fixtures.read_text())]
    expected_fixtures=['8075367c8ae92db5fed8f7d1f0d0784db2adb84922a766aec404396f4a68b80f','68b9606ba4f52d2a599ac97527985128c7df8215e63c9c19617185d4489baeb9','5a3ca2155d6835592f5c5687bb4da682c6fb22b96c66994ca98b013face5c4fb','87f640be4096344ad749a237982d6f3e26778ffd0139b0dceab7457dde982f81']
    if fixture_hashes!=expected_fixtures:p.error('original A2 four-fixture identity/order required')
    write(out/'a2-fixture-inventory.json',{'sha256':fixture_hashes,'local_paths_copied':False})
    write(out/'environment.json',{'os':subprocess.check_output(['sw_vers'],text=True),'rustc':subprocess.check_output(['rustc','-Vv'],text=True),'cargo':subprocess.check_output(['cargo','-V'],text=True),'target':str(target),'jobs':env.get('CARGO_BUILD_JOBS'),'offline':env.get('CARGO_NET_OFFLINE')})
    # Mandatory additional preflight; none of the frozen31 commands is removed.
    preflight_log=out/'guard-preflight.log'
    with preflight_log.open('w') as stream:
        preflight_code=subprocess.run(['python3','-B','scripts/test_verify_pmix_guards.py'],cwd=ROOT,env=env,stdout=stream,stderr=subprocess.STDOUT).returncode
    write(out/'guard-preflight.json',{'command':['python3','-B','scripts/test_verify_pmix_guards.py'],'exit_code':preflight_code,'source_manifest_sha256':source,'log':preflight_log.name,'sha256':sha(preflight_log)})
    if preflight_code:
        write(out/'summary.json',{'stage':'S5-M2-C','result':'FAIL','guard_preflight_exit_code':preflight_code,'gates_expected':len(COMMANDS),'gates_passed':0,'source_manifest_sha256':source})
        return 1
    lifecycle_log=out/'capture-lifecycle-preflight.log'
    with lifecycle_log.open('w') as stream:
        lifecycle_code=subprocess.run(['python3','-B','scripts/test_pmix_capture_lifecycle.py'],cwd=ROOT,env=env,stdout=stream,stderr=subprocess.STDOUT).returncode
    write(out/'capture-lifecycle-preflight.json',{'command':['python3','-B','scripts/test_pmix_capture_lifecycle.py'],'exit_code':lifecycle_code,'source_manifest_sha256':source,'log':lifecycle_log.name,'sha256':sha(lifecycle_log)})
    capture_log=out/'capture-writer-preflight.log'
    with capture_log.open('w') as stream:
        capture_code=subprocess.run(['python3','-B','scripts/run_pmix_capture_preflight.py','--out',str(out/'capture-writer-preflight')],cwd=ROOT,env=env,stdout=stream,stderr=subprocess.STDOUT).returncode
    if lifecycle_code or capture_code not in (0,2):
        write(out/'summary.json',{'stage':'S5-M2-C','result':'FAIL','capture_lifecycle_exit_code':lifecycle_code,'capture_writer_exit_code':capture_code,'gates_expected':len(COMMANDS),'gates_passed':0,'source_manifest_sha256':source})
        return 1
    capture_receipt=json.loads((out/'capture-writer-preflight/RESULTS.json').read_text())
    capture_blocked=capture_code==2
    if capture_receipt['result']!=('BLOCKED' if capture_blocked else 'PASS'):raise RuntimeError('unaccounted preflight exit/result')
    for name,command in COMMANDS:
        start=time.monotonic();utc=datetime.datetime.now(datetime.timezone.utc).isoformat();log=out/(name+'.log')
        with log.open('w') as stream:code=subprocess.run(command,cwd=ROOT,env=env,stdout=stream,stderr=subprocess.STDOUT).returncode
        if name=='service-boundary' and code==0:
            forbidden=[n for n in ('egui','eframe','wgpu','winit','raw-window-handle') if any(l.lstrip(' │├─└').startswith(n+' v') for l in log.read_text().splitlines())]
            write(out/'service-boundary.json',{'forbidden_dependencies':forbidden});code=int(bool(forbidden))
        if name in ('release','release-internal') and code==0:
            dest=out/'bin'/('editor-app-'+name);dest.parent.mkdir(exist_ok=True);shutil.copy2(target/'release/editor-app',dest)
            data=dest.read_bytes();expected=commit+('-dirty' if status else '')
            if expected.encode() not in data or any((marker in data)!=(name=='release-internal') for marker in (b'RCAM_I1_NATIVE_DIR',b'RCAM_I2_B_NATIVE_DIR',b'RCAM_I2_C_NATIVE_DIR',b'RCAM_UI_ROI_DIR',b'RCAM_PMIX_NATIVE_DIR')):code=1
            binaries[name]={'path':dest.relative_to(out).as_posix(),'sha256':sha(dest),'commit':expected,'source_manifest_sha256':source,'profile':'release'}
            write(out/'binaries.json',binaries)
        records.append({'id':name,'command':command,'started_utc':utc,'duration_seconds':time.monotonic()-start,'exit_code':code,'commit':commit,'source_manifest_sha256':source,'log':log.name,'sha256':sha(log)})
        if len(records)==1:
            records[0]['guard_preflight_sha256']=sha(preflight_log)
            records[0]['capture_lifecycle_sha256']=sha(lifecycle_log)
            records[0]['capture_writer_results_sha256']=sha(out/'capture-writer-preflight/RESULTS.json')
        write(out/'gates.json',records);print(name,code,round(records[-1]['duration_seconds'],2),flush=True)
        if code:break
    after=source_manifest.contents();(out/'source-after.sha256').write_text(after);status_after=git('status','--porcelain=v1','--untracked-files=all');(out/'status-after.txt').write_text(status_after+'\n')
    okay=before==after and status==status_after and len(records)==len(COMMANDS) and all(r['exit_code']==0 for r in records)
    write(out/'summary.json',{'stage':'S5-M2-C','commit':commit,'clean_worktree':not bool(status),'source_manifest_sha256':source,'unchanged_source':before==after,'unchanged_status':status==status_after,'gates_expected':len(COMMANDS),'gates_passed':sum(r['exit_code']==0 for r in records),'capture_initialization':capture_receipt['result'],'capture_initialization_exit_code':capture_code,'result':('BLOCKED_CAPTURE_INITIALIZATION' if capture_blocked else ('CANDIDATE_GATES_PASS' if status else 'GATES_PASS')) if okay else 'FAIL'})
    if okay:
        # Outside the evidence directory. Reviewers freeze this independent value
        # before testing mutations; a digest read from a mutated bundle is untrusted.
        trusted=out.parent/(out.name+'-trusted-gate-ledger.json')
        if trusted.exists():raise RuntimeError('external ledger receipt already exists; never overwrite')
        write(trusted,{'source_manifest_sha256':source,'build_commit':commit+('-dirty' if status else ''),'gates_json_sha256':sha(out/'gates.json'),'gates_expected':len(COMMANDS),'scope':'local frozen external receipt; pass exact digest to verifier independently of bundle'})
        print('frozen external gate ledger',sha(out/'gates.json'),flush=True)
    return (2 if capture_blocked else 0) if okay else 1
if __name__=='__main__':raise SystemExit(main())
