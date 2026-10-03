"""Portable S5-I1 source/evidence verifier. Default requires a clean commit."""
import argparse,json,re
from pathlib import Path
import source_manifest,package_source
from verify_s5m2_evidence import require,load,safe,digest
import verify_i1_native as native
import verify_i1_diagnostic as diagnostic
from run_i1_gates import COMMANDS

def coverage(root,manifest):
    entries=package_source.checked_manifest(root,manifest)
    actual={p.relative_to(root).as_posix() for p in root.rglob('*') if p.is_file() and p!=root/manifest}
    require(set(entries)==actual,'manifest coverage mismatch: '+manifest)
    return entries

def native_directory(evidence,name):
    return safe(evidence,'native/'+name+'/observations.json').parent

def verify(source,evidence,allow_dirty=False):
    source=source.resolve();evidence=evidence.resolve();source_manifest.ROOT=source
    info=load(source/'PACKAGE_INFO.json');coverage(source,'PACKAGE_MANIFEST.sha256')
    entries=package_source.checked_manifest(source,'MANIFEST.sha256')
    expected={p.relative_to(source).as_posix() for p in source_manifest.source_files()}
    require(set(entries)==expected,'source manifest coverage')
    require(info['source_manifest_sha256']==digest(source/'MANIFEST.sha256'),'package source identity')
    commit=info['git_commit'];require(re.fullmatch('[a-f0-9]{40}',commit) is not None and info['commit']==commit,'commit identity')
    clean=info['clean_worktree'];require(type(clean) is bool and (clean or allow_dirty),'clean source required')
    if clean:package_source.archive_identity(source)
    coverage(evidence,'EVIDENCE_MANIFEST.sha256')
    close=load(safe(evidence,'CLOSEOUT.json'))
    require(close['schema_version']==2 and close['stage']=='S5-I1','stage/schema')
    require(close['commit']==commit and close['clean_worktree']==clean,'commit/clean binding')
    require(close['source_manifest_sha256']==info['source_manifest_sha256'],'source/evidence binding')
    expected_commit=commit+('' if clean else '-dirty')
    commands=dict(COMMANDS);gates=close['gates']
    require([g['id'] for g in gates]==[name for name,_ in COMMANDS],'missing/reordered/duplicate gates')
    for g in gates:
        require(type(g['exit_code']) is int and g['exit_code']==0,'failed gate '+g['id'])
        require(g['command']==commands[g['id']],'gate command mismatch')
        require(g['commit']==commit and g['source_manifest_sha256']==close['source_manifest_sha256'],'gate source identity')
        require(digest(safe(evidence,g['log']))==g['sha256'],'gate raw log hash')
    summary=load(safe(evidence,close['gate_summary']))
    require(summary['unchanged_source'] is True and summary['unchanged_status'] is True and summary['gates_passed']==len(COMMANDS),'mutable/incomplete gates')
    binaries=close['binaries'];require(set(binaries)=={'release','release-internal'},'missing binary')
    for name,b in binaries.items():
        path=safe(evidence,b['path']);data=path.read_bytes()
        require(digest(path)==b['sha256'] and b['commit']==expected_commit and b['profile']=='release','binary identity')
        require(b['source_manifest_sha256']==close['source_manifest_sha256'],'binary/source binding')
        require(expected_commit.encode() in data and (not clean or (commit+'-dirty').encode() not in data),'embedded commit mismatch')
        require((b'RCAM_I1_NATIVE_DIR' in data)==(name=='release-internal'),'public/internal separation')
    rows=load(safe(evidence,close['native_index']))
    require([r['round'] for r in rows]==[1,2,3],'three native rounds required')
    unique=set();starts=[];results=[]
    for row in rows:
        directory=native_directory(evidence,row['directory'])
        report=load(directory/'observations.json');request=load(directory/'request.json')
        require(request.get('stress') is True and request.get('diagnostic') is None,'modifier stress protocol required')
        require(request['run_id'] not in unique,'reused native run');unique.add(request['run_id'])
        require(report['commit']==expected_commit and report['build_source']==('git-clean' if clean else 'git-dirty'),'native build identity')
        require(report['binary_sha256']==binaries['release-internal']['sha256'],'native binary mismatch')
        observed=native.verify(directory,source,safe(evidence,binaries['release-internal']['path']))
        require(row['summary']==observed,'native summary differs from raw evidence')
        results.append(observed);starts.append(request['start_utc'])
    require(starts==sorted(set(starts)),'nonsequential/reused runs')
    diagnostic_rows=load(safe(evidence,close['diagnostic_index']))
    require([r['mode'] for r in diagnostic_rows]==list(diagnostic.MODES),'three causal diagnostic modes required')
    diagnostics=[]
    for row in diagnostic_rows:
        directory=native_directory(evidence,row['directory'])
        report=load(directory/'observations.json');request=load(directory/'request.json')
        require(request['run_id'] not in unique,'reused diagnostic run');unique.add(request['run_id'])
        require(request['diagnostic']==row['mode'],'diagnostic mode binding')
        require(report['commit']==expected_commit and report['build_source']==('git-clean' if clean else 'git-dirty'),'diagnostic build identity')
        observed=diagnostic.verify(directory,source,safe(evidence,binaries['release-internal']['path']))
        require(row['summary']==observed,'diagnostic summary differs from raw evidence');diagnostics.append(observed)

    return {'result':'PASS' if clean else 'CANDIDATE_PASS','commit':commit,'gates':len(gates),'native_runs':len(rows),'native':results,'diagnostics':diagnostics,'historical_shift_cause':'not attributable from retained original inputs'}

def main():
    p=argparse.ArgumentParser(description=__doc__);p.add_argument('--source',required=True,type=Path);p.add_argument('--evidence',required=True,type=Path);p.add_argument('--allow-dirty',action='store_true');a=p.parse_args()
    try:print(json.dumps(verify(a.source,a.evidence,a.allow_dirty),ensure_ascii=False,indent=2));return 0
    except (OSError,ValueError,KeyError,TypeError) as e:print('FAIL:',e);return 1
if __name__=='__main__':raise SystemExit(main())
