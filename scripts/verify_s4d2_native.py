"""Independent assertions over controlled synthetic D2 native evidence; no physical-input claim."""
import argparse
import hashlib
import json
from pathlib import Path
import zipfile
from verify_s4d1_native import project

def verify(root):
    report=json.loads((root/'native-observations.json').read_text())
    assert report['status']=='PASS' and 'Metal' in report['adapter']
    identity=json.loads((root/'binary-identity.json').read_text())
    assert identity['commit']==report['commit'] and identity['sha256']==report['binary_sha256']
    assert report['frame_ticks']>50 and report['max_frame_gap_ms']>0
    rows={r['label']:r for r in report['records']}
    search_before=rows['before-refdes-search'];search_after=rows['refdes-search-list-only']
    for k in ['revision','dirty','project_dirty','undo','redo','geometry_sha256','camera_center','camera_scale','selected_count','focused_side']:assert search_before[k]==search_after[k],k
    before=rows['unregistered-before'];rejected=rows['unregistered-rejected']
    assert rejected['error_code']=='REGISTRATION_REQUIRED' and rejected['candidate_page'] is None
    for k in ['revision','dirty','project_dirty','undo','redo','geometry_sha256','camera_center','camera_scale','selected_count']:assert before[k]==rejected[k],k
    top=rows['registered-top-candidates'];bottom=rows['registered-bottom-candidates'];selected=rows['replace-selection'];added=rows['add-selection']
    assert top['component_count']==bottom['component_count']==100000
    assert top['focused_side']=='top' and bottom['focused_side']=='bottom'
    assert top['world_rotation_deg']==bottom['world_rotation_deg']==74
    assert top['candidate_page']['window']==bottom['candidate_page']['window']
    assert top['candidate_page']['items']==bottom['candidate_page']['items']
    assert top['candidate_page']['total']==4 and selected['selected_count']==added['selected_count']==4
    assert any(c['exposure']=='clear' for c in top['candidate_page']['items'])
    for a,b in [(top,selected),(bottom,added),(added,rows['units-zoom-overlay'])]:
        for k in ['revision','dirty','project_dirty','undo','redo','geometry_sha256']:assert a[k]==b[k],k
    moved=rows['move-after-selection'];undo=rows['undo-move']
    assert moved['geometry_sha256']!=top['geometry_sha256'] and moved['dirty']
    assert moved['candidate_page'] is None and undo['candidate_page'] is None
    assert undo['geometry_sha256']==top['geometry_sha256'] and not undo['dirty']
    assert moved['undo']==selected['undo']+1 and int(undo['revision'])>int(moved['revision'])
    assert rows['registration-undo-cache-cleared']['registration'] is None
    assert rows['registration-redo-cache-cleared']['candidate_page'] is None
    reflect=rows['reflected-bottom-candidates'];assert reflect['registration']['transform']['reflect_x']
    assert abs(reflect['world_rotation_deg']-180)<1e-8
    opened=rows['project-open-no-candidate-state'];assert opened['candidate_page'] is None and opened['selected_count']==0
    assert rows['requery-after-open']['candidate_page']['total']==4
    saved=project(root/'candidates.rcam');assert saved['format_version']==2 and len(saved['board']['components'])==100000
    text=json.dumps(saved);assert 'candidate_page' not in text and 'nearby_manufacturing' not in text
    with zipfile.ZipFile(root/'diagnostics.zip') as z:
        assert z.testzip() is None
        environment=json.loads(z.read('environment.json'));assert environment['profile']=='release' and environment['commit']==report['commit'] and 'Metal' in environment['gpu']
        events='\n'.join(z.read(n).decode('utf8',errors='replace') for n in z.namelist())
        for item in ['components.nearby_manufacturing','components.nearby.counts','candidate_count','components.candidates.select','requested_count','selected_count','registration']:assert item in events,item
        for item in ['C15','C16','pnp.csv','/Users/','/Volumes/',str(root)]:assert item not in events,item
    assert (root/'before.gbr').read_bytes()==(root/'after.gbr').read_bytes()
    assert (root/'after.gbr').read_bytes().count(b'D03*')==4
    return dict(schema_version=2,status='PASS',stage='S4-D2',commit=report['commit'],observations=len(rows),native='controlled synthetic native worker/service/Metal',physical_input='not claimed',windows='deferred / not executed')
if __name__=='__main__':
    parser=argparse.ArgumentParser(description=__doc__);parser.add_argument('root',type=Path);parser.add_argument('--out',type=Path);args=parser.parse_args();result=verify(args.root)
    output=json.dumps(result,indent=2)+'\n'
    if args.out:args.out.write_text(output)
    print(output)
