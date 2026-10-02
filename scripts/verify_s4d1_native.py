"""Independent, synthetic-only assertions over actual native outputs and observations."""
import argparse
import hashlib
import json
from pathlib import Path
import zipfile

def sha(data): return hashlib.sha256(data).hexdigest()
def project(path):
    with zipfile.ZipFile(path) as z:
        assert z.testzip() is None
        manifest=json.loads(z.read('manifest.json'))
        for e in manifest['entries']:
            data=z.read(e['path']);assert sha(data)==e['sha256'];assert len(data)==e['uncompressed_size']
        return json.loads(z.read('project.json'))
def verify(root):
    observations=json.loads((root/'native-observations.json').read_text())
    assert observations['status']=='PASS'
    assert 'Metal' in observations['adapter']
    identity=json.loads((root/'binary-identity.json').read_text())
    assert observations['commit']==identity['commit']
    assert observations['binary_sha256']==identity['sha256']
    records={r['label']:r for r in observations['records']}
    baseline=records['baseline'];invalid=records['invalid-import-zero-mutation']
    assert baseline['object_count']==4 and baseline['revision']=='1' and baseline['undo']==1
    initial=project(root/'baseline.rcam')
    assert initial['format_version']==1 and initial['board'] is None
    valid=records['valid-mapping-preview']
    assert valid['preview_valid'] and valid['units_confirmed'] and valid['convention_confirmed']
    for k in ['revision','dirty','project_dirty','undo','redo','geometry_sha256','component_count']: assert invalid[k]==baseline[k],k
    assert records['invalid-preview']['preview_valid'] is False
    assert [d['line'] for d in records['invalid-preview']['diagnostics']]==[3,3,4]
    imported=records['import-uncalibrated'];reg=records['registered'];focus=records['search-bottom-focus-overlay']
    assert imported['component_count']==3 and imported['board']['registration'] is None
    assert records['uncalibrated-focus-warning']['focus']['world_position'] is None
    if 'expected-unregistered-candidate-zero-mutation' in records:
        rejected=records['expected-unregistered-candidate-zero-mutation'];prior=records['uncalibrated-focus-warning']
        assert rejected['error_code']=='REGISTRATION_REQUIRED' and rejected['board']['registration'] is None
        assert rejected['focus']['world_position'] is None
        for key in ['revision','dirty','project_dirty','undo','redo','geometry_sha256','component_count']:assert rejected[key]==prior[key],key
    assert records['search-top-two-results']['query_results']==2
    assert imported['dirty']==baseline['dirty'] and imported['project_dirty'] is True
    assert imported['undo']==baseline['undo']+1
    assert reg['undo']==imported['undo']+1
    for r in [imported,reg,focus,records['view-unit-overlay-pure'],records['undo-registration'],records['redo-registration'],records['opened'],records['recovery-restored']]: assert r['geometry_sha256']==baseline['geometry_sha256'] and r['object_count']==baseline['object_count']
    assert reg['board']['registration']['residual_mm']<1e-8
    assert focus['query_results']==1 and focus['focus']['component']['refdes']=='C15' and focus['focus']['component']['side']=='bottom'
    assert focus['focus']['world_position']=={'x_mm':10.,'y_mm':0.} and focus['camera_center']==focus['focus']['world_position']
    for k in ['revision','dirty','project_dirty','undo','redo']: assert focus[k]==records['view-unit-overlay-pure'][k]
    assert records['undo-registration']['board']['registration'] is None
    assert records['redo-registration']['board']==reg['board']
    assert int(records['redo-registration']['revision'])>int(records['undo-registration']['revision'])>int(reg['revision'])
    assert records['opened']['board']==reg['board'] and records['opened']['project_dirty'] is False
    saved=project(root/'pnp.rcam');recovered=project(root/'recovery.rcam')
    assert saved['format_version']==2 and saved['board']==reg['board']
    assert recovered['format_version']==2 and recovered['board']==records['recovery-restored']['board']
    assert recovered['board']['registration']['transform']['reflect_x'] is True
    assert recovered['board']['registration']['transform']['rotation_deg']==37.
    assert records['recovery-restored']['project_dirty'] is True
    before=(root/'before.gbr').read_bytes();after=(root/'after.gbr').read_bytes();assert before==after
    assert sha((root/'pnp.csv').read_bytes())==saved['board']['provenance']['sha256']
    assert {c['side'] for c in saved['board']['components']}=={'top','bottom'}
    with zipfile.ZipFile(root/'diagnostics.zip') as z:
        assert z.testzip() is None
        environment=json.loads(z.read('environment.json'))
        assert environment['os']=='macos' and environment['arch']=='aarch64' and environment['profile']=='release'
        assert 'Metal' in environment['gpu'] and environment['commit']==observations['commit']
        text='\n'.join(z.read(n).decode('utf8',errors='replace') for n in z.namelist())
        for s in ['components.import_pnp','board.set_registration','invalid_number','unknown_side','duplicate_identity']: assert s in text,s
        for forbidden in ['R123','C15','封装','10k','pnp.csv','/Users/','/Volumes/',str(root)]: assert forbidden not in text,forbidden
    return dict(schema_version=2,status='PASS',stage='S4-D1',evidence_kind=observations['evidence_kind'],commit=observations['commit'],binary_sha256=observations['binary_sha256'],records_verified=len(records),component_count=3,sides=['top','bottom'],gerber_sha256=sha(before),source_sha256=sha((root/'pnp.csv').read_bytes()),project_sha256=sha((root/'pnp.rcam').read_bytes()),recovery_sha256=sha((root/'recovery.rcam').read_bytes()),physical_input='not claimed',windows='deferred / not executed')
if __name__=='__main__':
    parser=argparse.ArgumentParser(description=__doc__);parser.add_argument('root',type=Path);parser.add_argument('--out',type=Path,required=True);args=parser.parse_args()
    report=verify(args.root);args.out.write_text(json.dumps(report,indent=2)+'\n');print(json.dumps(report,indent=2))
