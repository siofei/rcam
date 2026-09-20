"""Validate actual S2-B3.2 native surface results; exit nonzero on schema or gate failure."""
import json, math, sys
from pathlib import Path

def percentile(values, q):
    values=sorted(values)
    return values[max(0,math.ceil(len(values)*q)-1)]

def validate(root):
    def read(name):
        return json.loads((root/name).read_text())
    result=read('native-results.json')
    assert result['schema_version']==2 and result['benchmark']=='s2b32'
    assert result['status']=='PASS' and result['failures']==[], result
    assert result['surface_screenshots']>=4
    pan=read('native-pan-zoom.json');drags=read('native-drag-3x10s.json');release=read('release-latency.json')
    assert pan['schema_version']==drags['schema_version']==release['schema_version']==2
    assert pan['active_duration_seconds']>=30
    assert len(drags['rounds'])==len(release['rounds'])==3
    def frames(rows):
        assert len(rows)>10
        previous=None
        for n,f in enumerate(rows):
            assert f['painted'] and f['focused']
            assert f['canvas_physical']==[1600.,900.]
            assert f['scene_total']==1000
            assert f['candidate_count']==f['object_visits']<=1000
            assert 0<=f['max_candidates_in_view']<=1000
            assert f['cpu_prepare_ms']>=0 and f['previous_surface_gpu_fence_wait_ms']>=0
            if previous is not None: assert f['frame']==previous+1, 'active frame omitted'
            previous=f['frame']
            if n==0:assert f['frame_interval_ms'] is None
            else:assert f['frame_interval_ms']>0
        return [f['frame_interval_ms'] for f in rows[1:]]
    frames(pan['frames'])
    for index,(drag,commit) in enumerate(zip(drags['rounds'],release['rounds'])):
        assert drag['round']==commit['round']==index
        assert drag['active_duration_seconds']>=10
        intervals=frames(drag['frames'])
        assert abs(percentile(intervals,.95)-drag['p95_frame_interval_ms'])<1e-10
        assert drag['p95_frame_interval_ms']<=50
        assert {f['revision'] for f in drag['frames']}=={str(index*2)}
        assert commit['one_undo'] and commit['all_coordinates_correct']
        assert commit['release_to_final_surface_gpu_complete_ms']<=300
    metrics=read('1000-selection-metrics.json')
    assert len(metrics)==1000 and all(m['status']=='exact' for m in metrics)
    assert abs(sum(m['area_mm2'] for m in metrics)-1000*math.pi/16)<1e-8
    assert abs(sum(m['perimeter_mm'] for m in metrics)-500*math.pi)<1e-8
    baseline=(root/'baseline.gbr').read_bytes()
    for name in ['after-navigation.gbr']+[f'after-undo-{n}.gbr' for n in range(3)]:assert (root/name).read_bytes()==baseline
    return {'schema_version':2,'status':'PASS','active_frames':len(pan['frames'])+sum(len(r['frames']) for r in drags['rounds'])}
if __name__=='__main__':
    print(json.dumps(validate(Path(sys.argv[1])),indent=2))
