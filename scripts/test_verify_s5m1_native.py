"""Verifier must reject incomplete/tampered native evidence, not mirror rendering."""
import copy,unittest
from verify_s5m1_native import percentile,verify_navigation,verify_selection,verify_edit_counters,purity
BASE={'revision':'0','workspace_revision':'0','dirty':False,'project_dirty':True,'undo':0,'redo':0}
def nav():
    origin=1_000_000_000
    state=dict(BASE,document_id='doc-1',scene_serial=1)
    frames=[dict(state,phase=3,frame_id=i,input_monotonic_ns=origin+i*15_000_000,observed_monotonic_ns=origin+i*15_000_000+1_000_000,acknowledged_monotonic_ns=origin+(i+1)*15_000_000,elapsed_seconds=i*.015,focused=True,canvas_physical=[1600.,900.],scene_count=100000,manufacturing_count=100000,painted=True,busy=False,display_pending=False,blocked=None,display_error=None,error=None,ui_error=None,frame_interval_ms=15.) for i in range(1,4001)]
    return {'frames':frames,'records':[{'label':'navigation-start','frame_id':0,'input_monotonic_ns':origin,'monotonic_ns':origin,'elapsed_seconds':1.,'phase_origin_ns':origin,'state':dict(state),'counters':{'geometry-full-build-call':5}},{'label':'navigation-end','frame_id':4000,'input_monotonic_ns':origin+60_000_000_000,'monotonic_ns':origin+60_002_000_000,'elapsed_seconds':61.002,'phase_origin_ns':origin,'state':dict(state),'counters':{'geometry-full-build-call':5}}]}
def highlight_state(primary,count=1,scene=50):
    return dict(BASE,document_id='doc-1',scene_serial=1,scene_count=scene,manufacturing_count=100000,selected_count=count,selected_primary=primary,busy=False,display_pending=False,blocked=None,display_error=None,error=None,ui_error=None,canvas_physical=[1600.,900.])
def highlight_frame(state):return dict(state,painted=True,gpu_completed=True,focused=True,selected_primary_in_scene=True,counters={})
def selection():
    rows=[{'label':'complete-operable-load','state':highlight_state(None)}]
    for i in range(200):
        ordinal=(i*37%100)*1000+(i*499%1000)+1
        rows.append(dict(label='point-highlight',point=i,ordinal=ordinal,actual_id=f'object-{ordinal}',state=highlight_state(f'object-{ordinal}'),before={'state':highlight_state(None),'counters':{}},frame=highlight_frame(highlight_state(f'object-{ordinal}')),input_to_gpu_complete_upper_bound_ms=20.))
    rows.append(dict(label='marquee-start',state=highlight_state('object-1',1,100000)))
    rows.append(dict(label='marquee-highlight',selected_count=100000,ordered=True,release_to_gpu_complete_upper_bound_ms=200.,state=highlight_state('object-100000',100000,100000),frame=highlight_frame(highlight_state('object-100000',100000,100000))))
    rows.extend({'label':label} for label in ['bounded-move-before','bounded-move-after','bounded-undo','bounded-redo','bounded-export','bounded-export-reopen','single-move-before','single-move-after','single-move-undo'])
    for row in rows[-9:]:
        row['state']=dict(BASE,document_id='doc-1',scene_serial=2,selected_count=1,selected_primary='object-1')
        row['counters']={}
        if row['label'] in ['single-move-after','single-move-undo']:
            row['counters']={'geometry-patch-attempt':1,'scene-allocation':1,'geometry-storage-init-upload':1000}
        row['completed_frame']=dict(row['state'],counters=dict(row['counters']))
    return {'records':rows,'exact_hit_cpu_ms':[2.]*401}
class Verification(unittest.TestCase):
    def test_nearest_rank_keeps_slow_tail(self):
        self.assertEqual(percentile([1.]*94+[70.]*6,.95),70.)
        self.assertEqual(percentile([1.]*98+[90.]*2,.99),90.)
    def test_real_frame_count_and_thresholds(self):self.assertEqual(verify_navigation(nav())['frames'],4000)
    def test_dropped_frame_is_rejected(self):
        r=nav();r['frames'].pop(200)
        with self.assertRaisesRegex(ValueError,'population|dropped'):verify_navigation(r)
    def test_partial_coverage_is_rejected(self):
        r=nav();r['frames'][100]['scene_count']=99999
        with self.assertRaisesRegex(ValueError,'partial'):verify_navigation(r)
    def test_background_busy_keeps_valid_navigation_and_pending_is_rejected(self):
        r=nav();r['frames'][100]['busy']=True
        self.assertEqual(verify_navigation(r)['frames'],4000)
        r['frames'][100]['display_pending']=True
        with self.assertRaisesRegex(ValueError,'partial'):verify_navigation(r)
    def test_navigation_rebuild_is_rejected(self):
        r=nav();r['records'][1]['counters']['geometry-full-build-call']=6
        with self.assertRaisesRegex(ValueError,'rebuilt'):verify_navigation(r)
    def test_tail_pause_is_rejected(self):
        r=nav();r['frames'][-1]['frame_interval_ms']=201.
        with self.assertRaisesRegex(ValueError,'interval'):verify_navigation(r)
    def test_navigation_boundary_and_time_mutations(self):
        def trial(change):
            r=nav();change(r)
            with self.assertRaises((ValueError,KeyError)):verify_navigation(r)
        for change in [lambda r:r.update(frames=r['frames'][-2:]),lambda r:r.update(frames=r['frames'][1:]),lambda r:r['frames'].pop(),lambda r:r['frames'][0].update(elapsed_seconds=59.9),lambda r:r['frames'][200].update(frame_interval_ms=1.),lambda r:r['frames'][200].update(acknowledged_monotonic_ns=0),lambda r:r['records'][1].update(input_monotonic_ns=0),lambda r:r['records'][0].pop('frame_id'),lambda r:r['frames'][0].update(frame_interval_ms=None),lambda r:r['frames'][0].update(frame_interval_ms=float('nan'))]:
            with self.subTest(change=change):trial(change)
    def test_selection_frame_context_mutations(self):
        ids=[f'object-{i}' for i in range(1,100001)]
        for label in ['point-highlight','marquee-highlight']:
            for key,value in [('document_id','wrong'),('revision','stale'),('scene_serial',9),('scene_count',0),('manufacturing_count',0),('selected_count',0),('selected_primary',None),('focused',False),('painted',False),('gpu_completed',False),('selected_primary_in_scene',False),('busy',True),('display_pending',True),('blocked','LIMIT'),('display_error','ERROR'),('error','ERROR'),('ui_error','ERROR')]:
                with self.subTest(label=label,key=key):
                    r=selection();next(row for row in r['records'] if row['label']==label)['frame'][key]=value
                    with self.assertRaises(ValueError):verify_selection(r,ids)
    def test_marquee_partial_scene_even_when_state_agrees(self):
        r=selection();row=next(row for row in r['records'] if row['label']=='marquee-highlight')
        row['state']['scene_count']=row['frame']['scene_count']=99999
        with self.assertRaisesRegex(ValueError,'partial full'):verify_selection(r,[f'object-{i}' for i in range(1,100001)])
    def test_real_pause_not_hidden_by_interval_stats(self):
        r=nav();r['records'][0]['input_monotonic_ns']-=186_000_000;r['frames'][0]['frame_interval_ms']=201.
        with self.assertRaisesRegex(ValueError,'threshold'):verify_navigation(r)
    def test_full_200_point_and_100k_id_evidence(self):
        r=selection();ids=[f'object-{i}' for i in range(1,100001)]
        self.assertEqual(verify_selection(r,ids)['marquee_ids'],100000)
        for bad in [ids[:-1],list(reversed(ids)),ids[:-1]+[ids[0]]]:
            with self.subTest(case='truncated/reordered/duplicated'):
                with self.assertRaisesRegex(ValueError,'marquee IDs'):verify_selection(r,bad)
    def test_fixed_point_mismatch_is_rejected(self):
        r=selection();r['records'][6]['actual_id']='object-99999'
        with self.assertRaisesRegex(ValueError,'wrong identity'):verify_selection(r,[])
    def test_prior_object_highlight_is_rejected(self):
        r=selection();r['records'][2]['frame']['selected_primary']='object-1'
        with self.assertRaisesRegex(ValueError,'bind current'):
            verify_selection(r,[f'object-{i}' for i in range(1,100001)])
    def test_edit_pre_callback_counters_are_rejected(self):
        r=selection();r['records'][-2]['counters']={}
        with self.assertRaisesRegex(ValueError,'before callback'):verify_edit_counters(r)
    def test_edit_previous_revision_frame_is_rejected(self):
        r=selection();r['records'][-2]['completed_frame']['revision']='stale'
        with self.assertRaisesRegex(ValueError,'current document'):verify_edit_counters(r)
    def test_missing_actual_gpu_upload_is_rejected(self):
        r=selection();row=r['records'][-2];row['counters']['geometry-storage-init-upload']=0
        row['completed_frame']['counters']=dict(row['counters'])
        with self.assertRaisesRegex(ValueError,'GPU allocation/upload'):verify_edit_counters(r)
    def test_revision_dirty_and_history_mutation_is_rejected(self):
        for key in BASE:
            after=copy.deepcopy(BASE);after[key]=not after[key] if isinstance(after[key],bool) else str(after[key])+'changed'
            with self.subTest(key=key):
                with self.assertRaisesRegex(ValueError,'read-only'):purity(BASE,after)
if __name__=='__main__':unittest.main()
