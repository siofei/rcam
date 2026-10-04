"""Synthetic verifier negatives only; never native/acceptance evidence."""
import copy
import hashlib
import json
from pathlib import Path
import tempfile
import unittest
from verify_i2_c_ui_roi import analyze, foreground, ppm
from verify_i2_c_native import metrics
from verify_i2_c_evidence import EXPECTED_COMMANDS_SHA256, COMMANDS
from verify_batch_drag_native import verify_c_centers, verify_c_usage, verify_c_worker_path, C_WORKER_PATH


def put(root, name, value):
    (root/name).write_text(json.dumps(value)+'\n')


def fixture():
    root = Path(tempfile.mkdtemp(prefix='rcam-i2-c-verifier-synthetic-'))
    put(root,'identity.json',dict(schema_version=2,profile='release',user_flicker_report='OPEN',
        sample_interval_ms=100,max_frames=40000,max_samples=1200,
        binary_sha256='1'*64,source_manifest_sha256='2'*64))
    labels = ('file','edit','arrange','insert','tools','layer','view','help')
    pixels = bytes([0,0,0]*12 + [240,240,240]*52)
    data = b'P6\n8 8\n255\n'+pixels
    (root/'glyph.ppm').write_bytes(data)
    frames=[]; paints=[]; samples=[]
    for n in range(12):
        time = 1_000_000 + n*110_000_000
        menus={name:dict(rect=[i*8,0,(i+1)*8,8],enabled=True,hovered=False)
               for i,name in enumerate(labels)}
        q=dict(frame=2*n+1,t_ns=time,input=dict(t_ns=time-1000),
               canvas=[0,20,80,60],ppp=1,menus=menus,popup=False)
        end=dict(q,frame=2*n+2,t_ns=time+500_000)
        frames.extend([q,end])
        paints.extend([dict(frame=f['frame'],t_ns=f['t_ns']+1000,
                            viewport_px=[0,20,80,40],clip_px=[0,20,80,40],
                            scissor_xyxy=[0,20,80,60]) for f in (q,end)])
        crops=[dict(name=name,path='glyph.ppm',sha256=hashlib.sha256(data).hexdigest(),
                    rect_px=menus[name]['rect'],metadata=menus[name]) for name in labels]
        samples.append(dict(sample=n,request=dict(q,request_ns=time+100_000),
                            callback_after_ui_frame=end['frame'],callback_last_ui=end,
                            callback_ns=time+1_000_000,surface_size_px=[80,60],crops=crops))
    for name,values in [('frames.jsonl',frames),('paint.jsonl',paints),('samples.jsonl',samples)]:
        (root/name).write_text(''.join(json.dumps(v)+'\n' for v in values))
    return root


def change(root, name, fn):
    path=root/name; values=[json.loads(line) for line in path.read_text().splitlines()]
    fn(values); path.write_text(''.join(json.dumps(v)+'\n' for v in values))


def crop_pixels(root, sample, name, transform):
    values=[json.loads(line) for line in (root/'samples.jsonl').read_text().splitlines()]
    crop=next(c for c in values[sample]['crops'] if c['name']==name)
    old=(root/crop['path']).read_bytes();width,height,pixels=ppm(old)
    altered=transform(width,height,pixels)
    data=f'P6\n{width} {height}\n255\n'.encode()+altered
    path=f'changed-{sample}-{name}.ppm';(root/path).write_bytes(data)
    crop.update(path=path,sha256=hashlib.sha256(data).hexdigest())
    (root/'samples.jsonl').write_text(''.join(json.dumps(v)+'\n' for v in values))
    return pixels,altered


class Negatives(unittest.TestCase):
    def setUp(self):
        self.root=fixture()  # Retained by design; no cleanup of historical evidence.

    def reject(self):
        with self.assertRaises((ValueError,KeyError,OSError)):
            analyze(self.root)

    def test_valid_surface_still_open(self):
        r=analyze(self.root,'1'*64,'2'*64)
        self.assertEqual(r['user_flicker_report'],'OPEN')
        self.assertEqual(r['result'],'NOT_REPRODUCED_IN_MEASURED_SURFACE_SCOPE')
        self.assertEqual(len(r['static_menu_labels']),8)

    def test_pixels_hash_tamper(self):
        p=self.root/'glyph.ppm';p.write_bytes(p.read_bytes()[:-3]+b'xxx');self.reject()

    def test_resealed_missing_glyph_requires_inspection(self):
        data=b'P6\n8 8\n255\n'+bytes([240]*192)
        (self.root/'glyph.ppm').write_bytes(data)
        change(self.root,'samples.jsonl',lambda xs:[c.update(sha256=hashlib.sha256(data).hexdigest()) for x in xs for c in x['crops']])
        self.assertEqual(analyze(self.root)['result'],'ROI_OUTLIERS_REQUIRE_INSPECTION')

    def test_resealed_black_surface_requires_inspection(self):
        data=b'P6\n8 8\n255\n'+bytes([0]*192)
        (self.root/'glyph.ppm').write_bytes(data)
        change(self.root,'samples.jsonl',lambda xs:[c.update(sha256=hashlib.sha256(data).hexdigest()) for x in xs for c in x['crops']])
        self.assertTrue(analyze(self.root)['outliers'])

    def position_outlier(self):
        report=analyze(self.root)
        outlier=next(e for e in report['outliers'] if e['sample']==0 and e['name']=='file')
        self.assertIn('foreground physical position/shape',outlier['reasons'])
        self.assertGreater(outlier['changed_foreground_pixels'],0)
        return outlier

    def test_resealed_one_physical_pixel_horizontal_translation(self):
        old,new=crop_pixels(self.root,0,'file',lambda w,h,p:b''.join(
            p[y*w*3+(w-1)*3:(y+1)*w*3]+p[y*w*3:y*w*3+(w-1)*3] for y in range(h)))
        self.assertEqual(sum(old),sum(new));self.assertEqual(sum(foreground(old)),sum(foreground(new)))
        outlier=self.position_outlier();self.assertEqual(outlier['dark_pixels'],outlier['median_dark'])

    def test_resealed_one_physical_pixel_vertical_translation(self):
        old,new=crop_pixels(self.root,0,'file',lambda w,h,p:p[-w*3:]+p[:-w*3])
        self.assertEqual(sum(old),sum(new));self.assertEqual(sum(foreground(old)),sum(foreground(new)))
        self.position_outlier()

    def test_resealed_equal_count_shape_change(self):
        def swap(w,h,p):
            values=[p[i:i+3] for i in range(0,len(p),3)]
            values[0],values[-1]=values[-1],values[0]
            return b''.join(values)
        old,new=crop_pixels(self.root,0,'file',swap)
        self.assertEqual(sum(old),sum(new));self.assertEqual(sum(foreground(old)),sum(foreground(new)))
        self.position_outlier()

    def test_balanced_shape_change_preserves_bbox_centroid_row_column_counts(self):
        # Equal row/column projections and centroid are insufficient: the two
        # diagonal ink pixels exchange places without changing these metrics.
        points={(1,1),(2,2),(4,1),(5,1),(6,1),(4,2),(5,2),(6,2),(1,4),(2,4),(3,4),(4,4)}
        pixels=b''.join(bytes([0,0,0] if (x,y) in points else [240,240,240]) for y in range(8) for x in range(8))
        data=b'P6\n8 8\n255\n'+pixels;(self.root/'glyph.ppm').write_bytes(data)
        change(self.root,'samples.jsonl',lambda xs:[c.update(sha256=hashlib.sha256(data).hexdigest()) for s in xs for c in s['crops']])
        before=analyze(self.root)['entries'][0]['foreground_geometry']
        def swap(w,h,p):
            values=[p[i:i+3] for i in range(0,len(p),3)]
            for a,b in ((9,10),(17,18)):values[a],values[b]=values[b],values[a]
            return b''.join(values)
        old,new=crop_pixels(self.root,0,'file',swap);a,b=foreground(old),foreground(new)
        self.assertEqual([sum(a[y*8:y*8+8]) for y in range(8)],[sum(b[y*8:y*8+8]) for y in range(8)])
        self.assertEqual([sum(a[x::8]) for x in range(8)],[sum(b[x::8]) for x in range(8)])
        outlier=self.position_outlier();self.assertEqual(outlier['foreground_geometry'],before)

    def test_rgb_antialias_variation_with_identical_foreground_is_allowed(self):
        old,new=crop_pixels(self.root,0,'file',lambda w,h,p:bytes(v+2 for v in p))
        self.assertNotEqual(old,new);self.assertEqual(foreground(old),foreground(new))
        report=analyze(self.root);self.assertFalse(report['outliers'])
        self.assertEqual(report['static_menu_labels']['file']['distinct_pixel_hashes'],2)

    def test_changed_hover_pixels_are_excluded_during_callback(self):
        crop_pixels(self.root,0,'file',lambda w,h,p:p[-w*3:]+p[:-w*3])
        change(self.root,'frames.jsonl',lambda xs:xs[1]['menus']['file'].update(hovered=True))
        change(self.root,'samples.jsonl',lambda xs:xs[0]['callback_last_ui']['menus']['file'].update(hovered=True))
        report=analyze(self.root);self.assertFalse(report['outliers'])
        self.assertEqual(report['static_menu_labels']['file']['eligible'],11)

    def test_changed_popup_pixels_are_excluded_during_callback(self):
        crop_pixels(self.root,0,'file',lambda w,h,p:p[-w*3:]+p[:-w*3])
        change(self.root,'frames.jsonl',lambda xs:xs[1].update(popup=True))
        change(self.root,'samples.jsonl',lambda xs:xs[0]['callback_last_ui'].update(popup=True))
        self.assertFalse(analyze(self.root)['outliers'])

    def test_changed_disabled_pixels_are_excluded_during_callback(self):
        crop_pixels(self.root,0,'file',lambda w,h,p:p[-w*3:]+p[:-w*3])
        change(self.root,'frames.jsonl',lambda xs:xs[1]['menus']['file'].update(enabled=False))
        change(self.root,'samples.jsonl',lambda xs:xs[0]['callback_last_ui']['menus']['file'].update(enabled=False))
        self.assertFalse(analyze(self.root)['outliers'])

    def test_legitimate_new_physical_rect_is_a_separate_position_group(self):
        crop_pixels(self.root,0,'file',lambda w,h,p:p[-w*3:]+p[:-w*3])
        change(self.root,'frames.jsonl',lambda xs:[f['menus']['file'].update(rect=[1,0,9,8]) for f in xs[:2]])
        def update(xs):
            s=xs[0]
            for f in (s['request'],s['callback_last_ui']):f['menus']['file']['rect']=[1,0,9,8]
            c=next(c for c in s['crops'] if c['name']=='file');c['metadata']['rect']=[1,0,9,8];c['rect_px']=[1,0,9,8]
        change(self.root,'samples.jsonl',update)
        self.assertFalse(analyze(self.root)['outliers'])

    def test_same_physical_rect_with_new_dpi_has_separate_position_group(self):
        crop_pixels(self.root,0,'file',lambda w,h,p:p[-w*3:]+p[:-w*3])
        def state(f):
            f['ppp']=2;f['canvas']=[v/2 for v in f['canvas']]
            for m in f['menus'].values():m['rect']=[v/2 for v in m['rect']]
        change(self.root,'frames.jsonl',lambda xs:[state(f) for f in xs[:2]])
        def update(xs):
            s=xs[0]
            for f in (s['request'],s['callback_last_ui']):state(f)
            for c in s['crops']:c['metadata']['rect']=[v/2 for v in c['metadata']['rect']]
        change(self.root,'samples.jsonl',update)
        self.assertFalse(analyze(self.root)['outliers'])

    def test_tied_glyph_layouts_do_not_silently_ignore_new_shape(self):
        for n in range(6):crop_pixels(self.root,n,'file',lambda w,h,p:p[-w*3:]+p[:-w*3])
        self.assertTrue(analyze(self.root)['outliers'])

    def test_crop_position_changed(self):
        change(self.root,'samples.jsonl',lambda xs:xs[0]['crops'][0].update(rect_px=[1,0,9,8]));self.reject()

    def test_omitted_menu(self):
        change(self.root,'samples.jsonl',lambda xs:xs[0]['crops'].pop());self.reject()

    def test_surface_time_reversed(self):
        change(self.root,'samples.jsonl',lambda xs:xs[0].update(callback_ns=0));self.reject()

    def test_actual_request_interval_weakened(self):
        change(self.root,'samples.jsonl',lambda xs:xs[1]['request'].update(request_ns=50_000_000));self.reject()

    def test_callback_state_forged(self):
        change(self.root,'samples.jsonl',lambda xs:xs[0]['callback_last_ui'].update(popup=True));self.reject()

    def test_legitimate_hover_excluded_across_callback(self):
        def hover(xs):xs[1]['menus']['file']['hovered']=True
        change(self.root,'frames.jsonl',hover)
        change(self.root,'samples.jsonl',lambda xs:xs[0]['callback_last_ui']['menus']['file'].update(hovered=True))
        r=analyze(self.root);self.assertEqual(r['static_menu_labels']['file']['eligible'],11)

    def test_gpu_scissor_tamper(self):
        change(self.root,'paint.jsonl',lambda xs:xs[0].update(scissor_xyxy=[0,0,80,60]));self.reject()

    def test_legitimate_disabled_transition_excluded(self):
        change(self.root,'frames.jsonl',lambda xs:xs[1]['menus']['file'].update(enabled=False))
        change(self.root,'samples.jsonl',lambda xs:xs[0]['callback_last_ui']['menus']['file'].update(enabled=False))
        self.assertEqual(analyze(self.root)['static_menu_labels']['file']['eligible'],11)

    def test_duplicate_jsonl_key(self):
        path=self.root/'frames.jsonl';path.write_text(path.read_text().replace('"frame": 1,','"frame": 1, "frame": 1,',1));self.reject()

    def test_nonfinite_jsonl(self):
        path=self.root/'frames.jsonl';path.write_text(path.read_text().replace('"ppp": 1','"ppp": NaN',1));self.reject()

    def test_boolean_sample_number(self):
        change(self.root,'samples.jsonl',lambda xs:xs[0].update(sample=False));self.reject()

    def test_continuous_ui_frame_omission(self):
        change(self.root,'frames.jsonl',lambda xs:xs.pop(2));self.reject()

    def test_external_binary_mismatch(self):
        with self.assertRaises(ValueError):analyze(self.root,'3'*64,'2'*64)

    def test_external_source_mismatch(self):
        with self.assertRaises(ValueError):analyze(self.root,'1'*64,'3'*64)

    def test_selected_composite_not_object_sum(self):
        good=dict(status=dict(state='ready',tooltip='面积 48.000000000000 mm²\n周长 58.485281374239 mm'))
        metrics(good,48,50+6*2**.5)
        bad=copy.deepcopy(good);bad['status']['tooltip']='面积 64.000000000000 mm²\n周长 70.000000000000 mm'
        with self.assertRaises(ValueError):metrics(bad,48,50+6*2**.5)

    def test_no_pending_stale_numeric_admission(self):
        for state in ('pending','unavailable','empty'):
            with self.assertRaises(ValueError):metrics(dict(status=dict(state=state,tooltip='面积 48 mm²\n周长 58.485281374239 mm')),48,50+6*2**.5)

    def test_frozen_gate_commands(self):
        digest=hashlib.sha256(json.dumps(COMMANDS,separators=(',',':')).encode()).hexdigest()
        self.assertEqual(digest,EXPECTED_COMMANDS_SHA256)
        self.assertEqual(len(COMMANDS),28)

    def query(self):
        version=dict(document_id='doc-2',document_revision='1',workspace_revision='0',generation=2,
                     rule_revision=0,geometry_policy_hash=hashlib.sha256(b'{"resolution_mm":0.0001}').hexdigest())
        state=dict(document_id='doc-2',revision='1',workspace_revision='0',dirty=True,project_dirty=True,
                   undo=1,redo=0,selected=5000,scene_serial=4,scene_objects=100000)
        worker=dict(action='selection-centers',sequence=8,batch=dict(state=state,
                    receipt=dict(task_id=8,state='completed',input=version,result_version=copy.deepcopy(version))))
        return worker,dict(batch=dict(state=copy.deepcopy(state)))

    def test_actual_readonly_query_fence(self):
        verify_c_centers(*self.query())

    def test_query_forged_generation_rule_precision(self):
        for field,value in [('generation',3),('rule_revision',1),('geometry_policy_hash','0'*64)]:
            w,p=self.query();w['batch']['receipt']['input'][field]=value;w['batch']['receipt']['result_version'][field]=value
            with self.assertRaises(ValueError):verify_c_centers(w,p)

    def test_query_stale_result_version(self):
        w,p=self.query();w['batch']['receipt']['result_version']['document_revision']='0'
        with self.assertRaises(ValueError):verify_c_centers(w,p)

    def test_readonly_query_partial_mutation(self):
        w,p=self.query();w['batch']['state']['undo']=2
        with self.assertRaises(ValueError):verify_c_centers(w,p)

    def test_readonly_query_untyped_other_action(self):
        w,p=self.query();w['action']='other'
        with self.assertRaises(ValueError):verify_c_centers(w,p)

    def worker_path(self):
        return [dict(action=action,batch=dict(started_ns=i*1000)) for i,action in enumerate(C_WORKER_PATH)]

    def test_complete_setup_and_post_release_query_path(self):
        verify_c_worker_path(self.worker_path(),6000)

    def test_initial_query_unknown_action_cannot_escape_receipt_checks(self):
        for index in (0,4,7,9,11):
            rows=self.worker_path();rows[index]['action']='other'
            with self.assertRaises(ValueError):verify_c_worker_path(rows,6000)

    def test_setup_query_omission_insertion_and_wrong_release_boundary(self):
        rows=self.worker_path();del rows[4]
        with self.assertRaises(ValueError):verify_c_worker_path(rows,6000)
        rows=self.worker_path();rows.insert(5,dict(action='selection-centers',batch=dict(started_ns=4500)))
        with self.assertRaises(ValueError):verify_c_worker_path(rows,6000)
        with self.assertRaises(ValueError):verify_c_worker_path(self.worker_path(),7000)

    def usage(self):
        return dict(owned_child_rusage=dict(method='macOS wait4 exact owned application child; ru_maxrss bytes; CPU seconds',
                    pid=123,rss_peak_bytes=200*1024*1024,child_elapsed_s=23.,user_cpu_s=5.,system_cpu_s=2.)),dict(pid=123)

    def test_owned_child_exact_resource_accounting(self):
        rss,report=verify_c_usage(*self.usage());self.assertEqual(rss,200*1024*1024)
        self.assertAlmostEqual(report['cpu_percent_average'],7/23*100)

    def test_resource_accounting_unrelated_pid(self):
        r,p=self.usage();r['owned_child_rusage']['pid']=456
        with self.assertRaises(ValueError):verify_c_usage(r,p)

    def test_resource_accounting_missing_or_fabricated(self):
        for key,value in [('rss_peak_bytes',0),('rss_peak_bytes',True),('user_cpu_s',float('nan')),('method','sampled aggregate children')]:
            r,p=self.usage();r['owned_child_rusage'][key]=value
            with self.assertRaises(ValueError):verify_c_usage(r,p)


if __name__=='__main__':
    unittest.main(verbosity=2)
