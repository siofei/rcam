"""Package local I1 evidence and source; never commits, uploads or rewrites history."""
import argparse,json,hashlib,shutil,zipfile
from pathlib import Path
from package_source import package
from verify_i1_evidence import verify
ROOT=Path(__file__).resolve().parents[1]
def sha(p):return hashlib.sha256(p.read_bytes()).hexdigest()
def write(p,v):p.write_text(json.dumps(v,ensure_ascii=False,indent=2)+'\n')
def main():
    p=argparse.ArgumentParser(description=__doc__);p.add_argument('--gates',required=True,type=Path);p.add_argument('--native',required=True,type=Path);p.add_argument('--out',required=True,type=Path);p.add_argument('--allow-dirty',action='store_true');a=p.parse_args()
    out=a.out.resolve();out.mkdir(parents=True,exist_ok=False)
    sourcezip=out/'RCam-S5-I1-Source.zip';package(sourcezip,stage='S5-I1')
    source=out/'Source';source.mkdir()
    with zipfile.ZipFile(sourcezip) as z:z.extractall(source)
    evidence=out/'Evidence';evidence.mkdir();shutil.copytree(a.gates,evidence/'gates');shutil.copytree(a.native,evidence/'native')
    info=json.loads((source/'PACKAGE_INFO.json').read_text());summary=json.loads((evidence/'gates/summary.json').read_text())
    gates=json.loads((evidence/'gates/gates.json').read_text());binaries=json.loads((evidence/'gates/binaries.json').read_text())
    for g in gates:g['log']='gates/'+g['log']
    for b in binaries.values():b['path']='gates/'+b['path']
    close={'schema_version':2,'stage':'S5-I1','commit':info['git_commit'],'clean_worktree':info['clean_worktree'],'source_manifest_sha256':info['source_manifest_sha256'],'gates':gates,'binaries':binaries,'gate_summary':'gates/summary.json','native_index':'native/native-index.json','diagnostic_index':'native/diagnostic-index.json'}
    write(evidence/'CLOSEOUT.json',close)
    (evidence/'EVIDENCE_MANIFEST.sha256').write_text(''.join(f'{sha(f)}  {f.relative_to(evidence).as_posix()}\n' for f in sorted(evidence.rglob('*')) if f.is_file()))
    result=verify(source,evidence,a.allow_dirty);write(out/'VERIFICATION.json',result)
    evidencezip=out/'RCam-S5-I1-Evidence.zip'
    # Bound memory when the complete native frame/screenshot corpus exceeds RAM.
    with zipfile.ZipFile(evidencezip,'x',compression=zipfile.ZIP_DEFLATED,compresslevel=9) as archive:
        for f in sorted(p for p in evidence.rglob('*') if p.is_file()):
            member=zipfile.ZipInfo(f.relative_to(evidence).as_posix(),date_time=(1980,1,1,0,0,0))
            member.create_system=3;member.external_attr=0o100644<<16;member.compress_type=zipfile.ZIP_DEFLATED
            with f.open('rb') as stream,archive.open(member,'w',force_zip64=True) as target:
                shutil.copyfileobj(stream,target,length=1024*1024)
    review=out/'REVIEW.md';review.write_text(f'''# S5-I1 {'candidate' if not info['clean_worktree'] else 'clean package'}

Commit/base: {info['git_commit']}; clean: {info['clean_worktree']}.
Source manifest SHA256: {info['source_manifest_sha256']}.

{len(gates)} gates and {result['native_runs']} native runs verified from raw records.
Read VERIFICATION.json and Evidence/CLOSEOUT.json for individual results.
Mac-first bounded. Synthetic egui input and GPU completion upper bounds;
no physical-input/scanout, Windows, PMIX, full V1/CORE10/P100K claim.

Independent source/package review is pending. This package never implies that
unperformed review or clean rebuild has passed. No push or Library upload.
''')
    (out/'SHA256SUMS.txt').write_text(''.join(f'{sha(f)}  {f.name}\n' for f in (sourcezip,evidencezip,review)))
    print(out)
if __name__=='__main__':main()
