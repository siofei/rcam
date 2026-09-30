"""Complete, deterministic S4-D1 delivery from clean tested source and native evidence.
Raw logs/binaries remain private. Public logs redact local host paths. No networking.
"""
import argparse
import hashlib
import json
from pathlib import Path
import re
import subprocess
import sys
import tempfile
import zipfile
import package_source
import source_manifest
import verify_s4d1_native

ROOT = source_manifest.ROOT

def sha(data):
    return hashlib.sha256(data).hexdigest()

def serialized(value):
    return (json.dumps(value, indent=2, sort_keys=True, ensure_ascii=False)+'\n').encode()

def redact(text):
    # Replace known paths first (including spaces in a volume name), then other
    # host/user/temp paths. Payload files are synthetic, independently verified.
    replacements = [(str(ROOT), '<WORKSPACE>'), (str(ROOT.parents[1]), '<WORKSPACE>'),
                    (str(Path.home()), '<USER>')]
    for value, replacement in sorted(replacements, key=lambda v: -len(v[0])):
        text=text.replace(value,replacement)
    return re.sub(r'(?:/(?:Users|Volumes|private|tmp|var/folders)/[^\n\"\s]*)', '<LOCAL_PATH>', text)

def safe_text(data):
    text=data.decode('utf8')
    assert not re.search(r'/Users/|/Volumes/|/private/|/tmp/|/var/folders/|[A-Za-z]:\\',text), 'public host path leak'
    return text

def zip_audit(path):
    with zipfile.ZipFile(path) as z:
        assert z.testzip() is None
        names=z.namelist()
        assert names==sorted(names) and len(names)==len(set(names))
        for e in z.infolist():
            assert e.date_time==(1980,1,1,0,0,0) and e.external_attr>>16==0o100644
            assert not e.filename.startswith('/') and '..' not in Path(e.filename).parts
            if not e.filename.isascii(): assert e.flag_bits & 0x800
        return dict(entries=len(names),crc='PASS',entry_order='sorted',timestamp='1980-01-01',permissions='100644',unicode_names='UTF-8')

def main():
    p=argparse.ArgumentParser(description=__doc__)
    p.add_argument('--gates',type=Path,required=True)
    p.add_argument('--native',type=Path,required=True)
    p.add_argument('--out',type=Path,required=True)
    args=p.parse_args()
    gates=args.gates.resolve();native=args.native.resolve();out=args.out.resolve()
    status=subprocess.check_output(['git','status','--porcelain=v1','--untracked-files=all'],cwd=ROOT,text=True).strip()
    assert not status,'clean source required'
    commit=subprocess.check_output(['git','rev-parse','HEAD'],cwd=ROOT,text=True).strip()
    assert (gates/'HEAD').read_text().strip()==commit
    summary=json.loads((gates/'summary.json').read_text())
    assert summary['gates_passed']==summary['gates_expected']==16 and summary['source_unchanged']
    assert not (gates/'clean-before.txt').read_text().strip() and not (gates/'clean-after.txt').read_text().strip()
    assert (gates/'tested-source-before.sha256').read_text()==(gates/'tested-source-after.sha256').read_text()==source_manifest.contents()
    native_report=verify_s4d1_native.verify(native)
    assert native_report['commit']==commit
    assert sha((gates/'bin/editor-app-internal-evidence').read_bytes())==summary['binary_hashes']['editor-app-internal-evidence']
    assert json.loads((native/'binary-identity.json').read_text())['sha256']==summary['binary_hashes']['editor-app-internal-evidence']
    frozen={}
    for name in ['docs/ACCEPTANCE_V1.md','docs/acceptance_cases.json','Cargo.lock']:
        baseline=subprocess.check_output(['git','show','484734984dc57122e975d2ce30211e32e95f72d7:'+name],cwd=ROOT)
        assert (ROOT/name).read_bytes()==baseline,name+' changed'
        frozen[name]=sha(baseline)
    out.mkdir(parents=True,exist_ok=False)
    stem='RCam_S4D1_'+commit[:7]
    source=out/(stem+'_source.zip')
    package_source.package(source,stage='S4-D1 PCB / PnP / RefDes Foundation',commit=commit)
    source_audit=zip_audit(source)
    with tempfile.TemporaryDirectory(prefix='rcam-d1-delivery-') as temporary:
        raw_report=Path(temporary)/'fresh.json'
        subprocess.run([sys.executable,str(ROOT/'scripts/verify_source_package.py'),str(source),
                        '--reference-manifest',str(ROOT/'MANIFEST.sha256'),
                        '--tested-source-hashes',str(gates/'tested-source-before.sha256'),
                        '--report',str(raw_report)],cwd=ROOT,check=True,stdout=subprocess.DEVNULL)
        fresh=json.loads(raw_report.read_text())
        fresh['schema_version']=2;fresh['archive']=source.name
        for c in fresh['commands']: c['command'][0]='python3';c['output']=redact(c['output'])
        fresh_bytes=serialized(fresh);safe_text(fresh_bytes)
        (out/'source_fresh_extract_report.json').write_bytes(fresh_bytes)
        entries={}
        for path in sorted(gates.iterdir()):
            if path.is_file() and path.suffix in {'.log','.json','.txt','.sha256'}:
                entries['gates/'+path.name]=redact(path.read_text()).encode()
        entries['gates/HEAD']=(commit+'\n').encode()
        entries['performance.json']=(gates/'pnp-artifacts/performance.json').read_bytes()
        # Only known, independent-verified synthetic outputs; no user documents.
        allowed=['native-observations.json','binary-identity.json','stdout.log','stderr.log',
                 'board.gbr','pnp.csv','pnp-invalid.csv','before.gbr','after.gbr','pnp.rcam','recovery.rcam','diagnostics.zip']
        for name in allowed:
            path=native/name
            assert path.is_file(),name
            data=path.read_bytes()
            if path.suffix in {'.json','.log'}: data=redact(data.decode()).encode()
            entries['native/'+name]=data
        for name in ['cua-observations.json','cua-state.txt','native-screenshot.png']:
            if (native/name).is_file():
                data=(native/name).read_bytes()
                if not name.endswith('.png'):data=redact(data.decode()).encode()
                entries['native/'+name]=data
        entries['native/independent-verification.json']=serialized(native_report)
        entries['source_fresh_extract_report.json']=fresh_bytes
        entries['binary-identity.json']=serialized(dict(commit=commit,builds=summary['binary_hashes'],active_driver='internal-evidence only; default public feature set excludes active input driver'))
        audit=dict(schema_version=2,status='PASS',commit=commit,clean=True,frozen_baseline=frozen,
                   tested_source_binding=fresh['tested_source_binding'],source_sha256=sha(source.read_bytes()),
                   source_archive=source_audit,source_manifest_sha256=sha((ROOT/'MANIFEST.sha256').read_bytes()),
                   source_fresh_extract='full source ZIP; source manifest and package self-test executed in new temporary directory',
                   public_evidence='allowlisted synthetic artifacts and path-redacted logs; binaries and raw local logs excluded',
                   service_boundary=json.loads((gates/'service-boundary.json').read_text()),
                   native=native_report,windows='deferred / not executed',full_v1_core10_p100k='not claimed')
        entries['source_delivery_audit.json']=serialized(audit)
        entries['README.md']=('S4-D1 Mac-first bounded evidence\n\nClean commit: '+commit+'\n\n'
            'All gate commands, exits and raw stdout/stderr are retained in gates/ with local host paths redacted. '
            'Unredacted originals and both binaries remain in private evidence storage. '
            'Native workflow uses the real EditorApp worker/ApplicationService and Apple Metal window through a controlled synthetic-only internal-evidence driver. '
            'Any actual CUA actions are separately recorded; physical human input is not claimed. '
            'Synthetic .rcam/PnP/Gerber outputs are included for independent checks. '
            'Default public build omits the active evidence driver. Windows, full V1, CORE10 and manufacturing P100K are deferred. '
            'Source includes the independent verification scripts.\n').encode()
        for name,data in entries.items():
            if name.endswith(('.json','.md','.txt','.log','.sha256','.csv','.gbr')):safe_text(data)
            if name.endswith('.zip'):
                with zipfile.ZipFile(__import__('io').BytesIO(data)) as z:
                    for inner in z.namelist():safe_text(z.read(inner))
        entries['EVIDENCE.sha256']=''.join(f'{sha(data)}  {name}\n' for name,data in sorted(entries.items())).encode()
        evidence=out/(stem+'_public_evidence.zip')
        package_source.write_deterministic_zip(evidence,entries.items())
        second=Path(temporary)/'repeat.zip';package_source.write_deterministic_zip(second,entries.items())
        assert evidence.read_bytes()==second.read_bytes()
        evidence_audit=zip_audit(evidence)
        with zipfile.ZipFile(evidence) as z:
            for line in z.read('EVIDENCE.sha256').decode().splitlines():
                digest,name=line.split('  ',1);assert sha(z.read(name))==digest
        (out/'SHA256SUMS.txt').write_text(''.join(f'{sha((out/name).read_bytes())}  {name}\n' for name in [source.name,evidence.name,'source_fresh_extract_report.json']))
        subprocess.run(['shasum','-a','256','-c','SHA256SUMS.txt'],cwd=out,check=True)
        # Review lives outside evidence to avoid circular self-hashes. Stage
        # signoff is written separately only after these audits have succeeded.
        review=dict(audit,public_evidence_sha256=sha(evidence.read_bytes()),public_evidence_archive=evidence_audit,
                    deterministic_evidence_repeat='byte-identical',sha256sums='PASS')
        (out/'delivery-audit.json').write_bytes(serialized(review))
        print(json.dumps(dict(status='PASS',commit=commit,delivery=out.name,source_files=fresh['tested_source_binding']['total'],four_files=[source.name,evidence.name,'SHA256SUMS.txt','source_fresh_extract_report.json']),indent=2))

if __name__=='__main__':main()
