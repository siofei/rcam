"""Deterministic complete D2 source/public evidence delivery, fresh extraction and hash audits."""
import argparse
import hashlib
import json
from pathlib import Path
import subprocess
import sys
import tempfile
import zipfile
import package_source
import source_manifest
import verify_s4d2_native
from package_s4d1_delivery import redact, safe_text, zip_audit, serialized

def sha(data):return hashlib.sha256(data).hexdigest()
def write_zip(path,entries):
    with zipfile.ZipFile(path,'w',compression=zipfile.ZIP_DEFLATED,compresslevel=6) as z:
        for name,data in sorted(entries.items()):
            item=zipfile.ZipInfo(name,(1980,1,1,0,0,0));item.external_attr=0o100644<<16;item.compress_type=zipfile.ZIP_DEFLATED;z.writestr(item,data)

def main():
    p=argparse.ArgumentParser(description=__doc__);p.add_argument('--gates',type=Path,required=True);p.add_argument('--native',type=Path,required=True);p.add_argument('--out',type=Path,required=True);args=p.parse_args()
    root=source_manifest.ROOT;gates=args.gates.resolve();native=args.native.resolve();out=args.out.resolve()
    assert not subprocess.check_output(['git','status','--porcelain=v1','--untracked-files=all'],cwd=root,text=True).strip(),'clean source required'
    commit=subprocess.check_output(['git','rev-parse','HEAD'],cwd=root,text=True).strip();summary=json.loads((gates/'summary.json').read_text());assert summary['gates_passed']==summary['gates_expected']==19 and summary['source_unchanged']
    assert (gates/'HEAD').read_text().strip()==commit
    assert not (gates/'clean-before.txt').read_text().strip() and not (gates/'clean-after.txt').read_text().strip()
    assert (gates/'tested-source-before.sha256').read_text()==(gates/'tested-source-after.sha256').read_text()==source_manifest.contents()
    verified=verify_s4d2_native.verify(native);assert verified['commit']==commit
    assert json.loads((native/'binary-identity.json').read_text())['sha256']==summary['binary_hashes']['editor-app-internal-evidence']
    frozen={}
    for name in ['docs/ACCEPTANCE_V1.md','docs/acceptance_cases.json','Cargo.lock']:
        original=subprocess.check_output(['git','show','8b978cc:'+name],cwd=root);assert (root/name).read_bytes()==original;frozen[name]=sha(original)
    out.mkdir(parents=True,exist_ok=False);stem='RCam_S4D2_'+commit[:7];source=out/(stem+'_source.zip')
    package_source.package(source,stage='S4-D2 RefDes-Assisted Stencil Candidate Selection v1',commit=commit)
    with tempfile.TemporaryDirectory(prefix='rcam-d2-fresh-') as temporary:
        raw=Path(temporary)/'fresh.json';subprocess.run([sys.executable,str(root/'scripts/verify_source_package.py'),str(source),'--reference-manifest',str(root/'MANIFEST.sha256'),'--tested-source-hashes',str(gates/'tested-source-before.sha256'),'--report',str(raw)],cwd=root,check=True,stdout=subprocess.DEVNULL)
        fresh=json.loads(raw.read_text());fresh.update(schema_version=2,archive=source.name)
        for command in fresh['commands']:command['command'][0]='python3';command['output']=redact(command['output'])
        data=serialized(fresh);safe_text(data);(out/'source_fresh_extract_report.json').write_bytes(data)
    entries={}
    for path in sorted(gates.iterdir()):
        if path.is_file() and (path.suffix in {'.log','.json','.txt','.sha256'} or path.name=='HEAD'):entries['gates/'+path.name]=redact(path.read_text()).encode()
    for folder in ['pnp-artifacts','candidate-artifacts']:
        for path in sorted((gates/folder).glob('*.json')):entries['performance/'+path.name]=path.read_bytes()
    for name in ['native-observations.json','binary-identity.json','stdout.log','stderr.log','board.gbr','pnp.csv','baseline.rcam','before.gbr','after.gbr','candidates.rcam','diagnostics.zip']:
        path=native/name;assert path.is_file(),name;data=path.read_bytes()
        if path.suffix in {'.json','.log'}:data=redact(data.decode()).encode()
        entries['native/'+name]=data
    for name in ['native-screenshot.png','cua-observations.json']:
        if (native/name).exists():entries['native/'+name]=(native/name).read_bytes()
    entries['native/independent-verification.json']=serialized(verified)
    entries['source_fresh_extract_report.json']=(out/'source_fresh_extract_report.json').read_bytes()
    entries['case_results.json']=serialized(dict(schema_version=2,stage='S4-D2',commit=commit,cases=[dict(id=f'D2-{i:02d}',status='PASS',required_platforms=['macos'],evidence=['gates/gates.json','native/independent-verification.json','source_fresh_extract_report.json']) for i in range(1,14)],windows='deferred / not executed',full_v1_core10_p100k='not claimed',physical_input='not claimed'))
    entries['delivery-audit.json']=serialized(dict(schema_version=2,commit=commit,status='PASS',source_sha256=sha(source.read_bytes()),source_archive=zip_audit(source),tested_source_binding=fresh['tested_source_binding'],frozen=frozen,binaries=summary['binary_hashes'],native=verified,public_evidence='synthetic allowlist and path-redacted command logs; original logs/binaries retained locally'))
    entries['README.md']=('S4-D2 Mac-first bounded evidence\n\nClean commit: '+commit+'\n\nCandidate != Association. Bounds candidates are not final Boolean openings or footprint ownership. Native evidence is a controlled synthetic real EditorApp worker/ApplicationService/Metal workflow, separately identified from physical human input (not claimed). Native 100k PnP fixture is generated data. All raw command stdout/stderr is retained with local paths redacted; unredacted originals and both binaries remain locally. Windows/full V1/CORE10/P100K deferred.\n').encode()
    for name,data in entries.items():
        if name.endswith(('.json','.log','.txt','.md','.sha256','.csv','.gbr')):safe_text(data)
        if name.endswith('.zip'):
            with zipfile.ZipFile(__import__('io').BytesIO(data)) as z:
                for inner in z.namelist():safe_text(z.read(inner))
    entries['EVIDENCE.sha256']=''.join(f'{sha(data)}  {name}\n' for name,data in sorted(entries.items())).encode()
    evidence=out/(stem+'_public_evidence.zip');write_zip(evidence,entries);zip_audit(evidence)
    with tempfile.TemporaryDirectory(prefix='rcam-d2-repeat-') as temporary:
        repeated=Path(temporary)/'evidence.zip';write_zip(repeated,entries);assert repeated.read_bytes()==evidence.read_bytes()
        extracted=Path(temporary)/'extract'
        with zipfile.ZipFile(evidence) as z:z.extractall(extracted)
        for line in (extracted/'EVIDENCE.sha256').read_text().splitlines():digest,name=line.split('  ',1);assert sha((extracted/name).read_bytes())==digest,name
        assert verify_s4d2_native.verify(extracted/'native')['status']=='PASS'
    (out/'SHA256SUMS.txt').write_text(''.join(f'{sha(path.read_bytes())}  {path.name}\n' for path in sorted(out.iterdir()) if path.is_file()))
    subprocess.run(['shasum','-a','256','-c','SHA256SUMS.txt'],cwd=out,check=True)
    print(json.dumps(dict(status='PASS',commit=commit,source=str(source),evidence=str(evidence)),indent=2))
if __name__=='__main__':main()
