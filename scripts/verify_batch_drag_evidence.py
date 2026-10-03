"""Portable S5-M2-B source/evidence verifier. Default requires a clean commit."""
import argparse,json,re
from pathlib import Path
import source_manifest,package_source
from verify_s5m2_evidence import require,load,safe,digest
import verify_batch_drag_native as native
from run_batch_drag_gates import COMMANDS

def coverage(root,manifest):
    entries=package_source.checked_manifest(root,manifest)
    actual={p.relative_to(root).as_posix() for p in root.rglob('*') if p.is_file() and p!=root/manifest}
    require(set(entries)==actual,'manifest coverage mismatch: '+manifest)
    return entries

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
    require(close['schema_version']==2 and close['stage']=='S5-M2-B','stage/schema')
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
        require((b'RCAM_BATCH_DRAG_NATIVE_DIR' in data)==(name=='release-internal'),'public/internal separation')
    native.PROTOCOL=source/'fixtures/synthetic/s5m2b/protocol.json'
    protocol=load(native.PROTOCOL)
    require(digest(safe(source,protocol['fixture']))==protocol['fixture_sha256'],'packaged fixture mismatch')
    rows=load(safe(evidence,close['native_index']))
    matrix=[(r,n,'move') for r in (1,2,3) for n in (100,500,1000,5000)]+[(1,1000,m) for m in protocol['interruptions']]
    require([(r['round'],r['selected'],r['mode']) for r in rows]==matrix,'native matrix incomplete/reordered')
    unique=set();starts=[];results=[]
    for row in rows:
        name=row['directory'];report_path=safe(evidence,'native/'+name+'/observations.json');report=load(report_path)
        request=load(report_path.parent/'request.json')
        require(request['run_id'] not in unique,'reused native run');unique.add(request['run_id'])
        require(request['round']==row['round'] and request['selected']==row['selected'] and request['mode']==row['mode'],'row/request mismatch')
        require(report['commit']==expected_commit and report['build_source']==('git-clean' if clean else 'git-dirty'),'native build identity')
        require(report['binary_sha256']==binaries['release-internal']['sha256'],'native binary mismatch')
        require('Metal' in report['adapter'],'native Metal required')
        require(row['result']=='PASS','native runner failed')
        observed=native.verify(report_path.parent);require(row['summary']==observed,'native summary differs from raw evidence')
        results.append(observed);starts.append(request['start_utc'])
    require(starts==sorted(set(starts)),'nonsequential/reused runs')
    environment=load(safe(evidence,'native/environment.json'))
    require(environment['protocol']==protocol and environment['memory_bytes']>0 and all(environment.get(k) for k in ('cpu','ram','gpu','displays','power','os')),'environment missing')
    require(all(d.get('pixels') and d.get('resolution_refresh') for d in environment['displays']),'display metadata missing')
    return {'result':'PASS' if clean else 'CANDIDATE_PASS','commit':commit,'gates':len(gates),'native_runs':len(rows),'native':results}

def main():
    p=argparse.ArgumentParser(description=__doc__);p.add_argument('--source',required=True,type=Path);p.add_argument('--evidence',required=True,type=Path);p.add_argument('--allow-dirty',action='store_true');a=p.parse_args()
    try:print(json.dumps(verify(a.source,a.evidence,a.allow_dirty),ensure_ascii=False,indent=2));return 0
    except (OSError,ValueError,KeyError,TypeError) as e:print('FAIL:',e);return 1
if __name__=='__main__':raise SystemExit(main())
