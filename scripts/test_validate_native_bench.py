"""Exercise acceptance rejection paths without a GPU or permissive result-only checks."""
import copy
import json
import math
from pathlib import Path
import tempfile
import unittest
from validate_native_bench import validate

class NativeSchemaTests(unittest.TestCase):
    def setUp(self):
        self.tmp=tempfile.TemporaryDirectory()
        self.addCleanup(self.tmp.cleanup)
        self.root=Path(self.tmp.name)
        def frames(revision):
            return [dict(frame=n,painted=True,focused=True,canvas_physical=[1600.,900.],scene_total=1000,candidate_count=100,object_visits=100,max_candidates_in_view=10,cpu_prepare_ms=.1,previous_surface_gpu_fence_wait_ms=.2,frame_interval_ms=None if n==0 else 20.,revision=str(revision)) for n in range(12)]
        self.data={
            'native-results.json':dict(schema_version=2,benchmark='s2b32',status='PASS',failures=[],surface_screenshots=4),
            'native-pan-zoom.json':dict(schema_version=2,active_duration_seconds=30,frames=frames(0)),
            'native-drag-3x10s.json':dict(schema_version=2,rounds=[dict(round=n,active_duration_seconds=10,p95_frame_interval_ms=20.,frames=frames(n*2)) for n in range(3)]),
            'release-latency.json':dict(schema_version=2,rounds=[dict(round=n,one_undo=True,all_coordinates_correct=True,release_to_final_surface_gpu_complete_ms=50.) for n in range(3)]),
            '1000-selection-metrics.json':[dict(status='exact',area_mm2=math.pi/16,perimeter_mm=math.pi/2) for _ in range(1000)]}
        for name in ['baseline.gbr','after-navigation.gbr']+[f'after-undo-{n}.gbr' for n in range(3)]:
            (self.root/name).write_bytes(b'unchanged writer snapshot')
    def run_validation(self):
        for name,value in self.data.items():(self.root/name).write_text(json.dumps(value))
        return validate(self.root)
    def test_valid(self):
        self.assertEqual(self.run_validation()['status'],'PASS')
    def test_reject_frame_faults(self):
        for key,value in [('painted',False),('focused',False),('canvas_physical',[1599,900]),('frame',500),('revision','999'),('frame_interval_ms',200.),('object_visits',1000)]:
            with self.subTest(key=key):
                original=copy.deepcopy(self.data)
                self.data['native-drag-3x10s.json']['rounds'][0]['frames'][5][key]=value
                with self.assertRaises(AssertionError):self.run_validation()
                self.data=original
    def test_reject_limits_and_invariants(self):
        for file,path,value in [
            ('native-pan-zoom.json',['active_duration_seconds'],29.),
            ('native-drag-3x10s.json',['rounds',0,'active_duration_seconds'],9.),
            ('release-latency.json',['rounds',0,'release_to_final_surface_gpu_complete_ms'],301.),
            ('release-latency.json',['rounds',0,'one_undo'],False),
            ('1000-selection-metrics.json',[0,'status'],'unsupported')]:
            with self.subTest(path=path):
                original=copy.deepcopy(self.data);item=self.data[file]
                for key in path[:-1]:item=item[key]
                item[path[-1]]=value
                with self.assertRaises(AssertionError):self.run_validation()
                self.data=original
    def test_reject_writer_change(self):
        (self.root/'after-undo-2.gbr').write_bytes(b'changed')
        with self.assertRaises(AssertionError):self.run_validation()

if __name__=='__main__':unittest.main()
