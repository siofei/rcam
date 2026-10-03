"""Fail-closed raw-data checks; synthetic unit data is never acceptance evidence."""
import json,struct,tempfile,unittest
from pathlib import Path
from verify_batch_drag_native import verify,sha,PROTOCOL

def write(p,v):p.write_text(json.dumps(v))
class Verify(unittest.TestCase):
    def setUp(self):
        self.tmp=tempfile.TemporaryDirectory();self.d=Path(self.tmp.name);self.fixture('move');self.check()
    def fixture(self,mode):
        from copy import deepcopy
        from math import sin,tau,hypot
        from verify_batch_drag_native import f32
        self.base={'document_id':'d','revision':'0','workspace_revision':'0','dirty':False,'project_dirty':True,'undo':0,'redo':0,'selected':100,'scene_serial':1,'scene_objects':100000,'canvas_physical':[1600,900],'busy':False,'display_pending':False,'delta':None}
        request={'selected':100,'mode':mode,'fixture_sha256':json.loads(PROTOCOL.read_text())['fixture_sha256'],'protocol_sha256':sha(PROTOCOL),'run_id':'unit-only'}
        view={'center_mm':[500.5,50.5],'scale':.74,'rect':[0.,0.,800.,450.],'ppp':2.,'grid_snap':False,'object_snap':False,'threshold_physical_px':4.}
        press=[f32(400+f32((1-500.5)*.74)),f32(225-f32((1-50.5)*.74))]
        def world(pos):return [500.5+f32(pos[0]-400)/.74,50.5-f32(pos[1]-225)/.74]
        start=world(press);origin=10_052_000_000;frames=[];events=[];workers=[];snapshots=[]
        def frame(at,phase,state,position=None,pressed=False,released=False,dragging=False):
            id_=len(frames)+1
            inp={'view':deepcopy(view),'pointer':position,'pressed':pressed,'released':released,'escape':released and mode=='escape','pointer_gone':released and mode=='pointergone','focused':not(released and mode=='blur'),'trajectory_origin_ns':origin}
            delta=state['delta'];paint=[delta['x_mm'],delta['y_mm'],0.,0.] if delta else [0.,0.,0.,0.]
            f={'id':id_,'phase':phase,'injected':inp,'view':deepcopy(view),'gesture':None if delta is None else {'last':position or press,'confirmed':True,'dragging':dragging,'error':None},'focused':True,'input_ns':at,'observed_ns':at+1_000_000,'completed_ns':at+25_000_000,'painted':True,'gpu_completed':True,'snapshot_identity_unchanged':True,'scene_identity_unchanged':True,'preview_index_identity_unchanged':True,'state':deepcopy(state),'paint_delta':paint,'prepare':{'preview_index_ms':0,'cpu_prepare_ms':1},'ui_allocation_count_bytes':[2,10],'cpu_update_ms':2,'frame_interval_ms':(at-frames[-1]['input_ns'])/1e6 if frames else None,'input_gpu_complete_ms':25,'counters':{'draw':id_,'uniform-upload':112*id_}}
            frames.append(f);return f
        def event(label,f,data,offset=2_000_000):
            e={'label':label,'frame_id':f['id'],'at_ns':f['input_ns']+offset,'data':deepcopy(data)};events.append(e);return e
        baseline=frame(10_000_000_000,3,self.base);event('baseline',baseline,{'state':self.base})
        event('warmup-begin',baseline,{},-10_000_000_000)
        armed=dict(self.base,delta={'x_mm':0.,'y_mm':0.})
        down=frame(10_025_000_000,5,armed,press,pressed=True);event('press',down,{'position':press})
        confirm=frame(10_050_000_000,5,armed);event('confirmed',confirm,{'state':armed,'trajectory_origin_ns':origin},3_000_000)
        seconds=10 if mode=='move' else 1;at=origin+33_000_000;dragging=False
        while at-origin<seconds*1e9:
            t=f32((at-origin)/(seconds*1e9));pos=[f32(press[0]+f32(36*t)),f32(press[1]+f32(f32(-18*t)+f32(8*f32(sin(f32(f32(tau)*t))))))]
            dragging=dragging or hypot(pos[0]-press[0],pos[1]-press[1])*2>=4
            delta={k:world(pos)[i]-start[i] if dragging else 0. for i,k in enumerate(('x_mm','y_mm'))}
            frame(at,6,dict(self.base,delta=delta),pos,dragging=dragging);at+=33_000_000
        endpoint=[f32(press[0]+36),f32(press[1]-18)];final_delta={k:world(endpoint)[i]-start[i] for i,k in enumerate(('x_mm','y_mm'))}
        release=frame(at,7,self.base,endpoint,released=True);event('release',release,{'position':endpoint,'delta_mm':final_delta,'mode':mode})
        def snapshot(label,state,moved=False,empty=False):
            data=b''.join(struct.pack('<dd',i%1000+1+(final_delta['x_mm'] if moved and i<100 else 0),i//1000+1+(final_delta['y_mm'] if moved and i<100 else 0)) for i in range(0 if empty else 100000))
            path=self.d/(label+'.f64le');path.write_bytes(data)
            snapshots.append({'label':label,'path':path.name,'sha256':sha(path),'count':0 if empty else 100000,'content_sha256':'empty' if empty else ('moved' if moved else 'before'),'state':deepcopy(state)})
        snapshot('before',self.base)
        def completed(label,trigger,state,action,phase):
            nonlocal at
            at+=33_000_000;visible=frame(at,phase,state)
            seq=None
            if action:
                seq=len(workers)+1;keys=('document_id','revision','workspace_revision','dirty','project_dirty','undo','redo','selected','scene_serial','scene_objects')
                workers.append({'sequence':seq,'action':action,'elapsed_ms':9.,'error':None,'selected_count':state['selected'],'scene_count':state['scene_objects'],'batch':{'started_ns':trigger['input_ns']+1_000_000,'finished_ns':trigger['input_ns']+10_000_000,'state':{k:state[k] for k in keys}}})
            at+=33_000_000;f=frame(at,phase,state);event(label,f,{'state':state,'duration_ms':(at+2_000_000-trigger['input_ns'])/1e6,'visible_frame_id':visible['id'],'visible_completed_ns':visible['completed_ns'],'worker_sequence':seq});return f
        if mode=='move':
            moved=dict(self.base,revision='1',undo=1,dirty=True,scene_serial=2)
            completed('commit-complete',release,moved,'drag-move',7);snapshot('moved',moved,True)
            at+=33_000_000;undo_input=frame(at,9,moved);event('undo-input',undo_input,{})
            undone=dict(self.base,revision='2',undo=0,redo=1,scene_serial=3)
            completed('undo-complete',undo_input,undone,'undo',9);snapshot('undo',undone)
            at+=33_000_000;redo_input=frame(at,11,undone);event('redo-input',redo_input,{})
            final=dict(moved,revision='3',scene_serial=4)
            last=completed('redo-complete',redo_input,final,'redo',11);snapshot('redo',final,True)
        else:
            final=dict(self.base)
            if mode=='new-project':final.update(document_id='new',selected=0,scene_objects=0,scene_serial=2,project_dirty=False)
            last=completed('cancel-complete',release,final,'new-project' if mode=='new-project' else None,7);snapshot('cancelled',final,empty=mode=='new-project')
        for _ in range(3):at+=33_000_000;frame(at,12,final)
        pixels=b'P6\n1600 900\n255\n'+bytes(1600*900*3);(self.d/'surface.ppm').write_bytes(pixels)
        def screenshot(label,f):
            request={'label':label,'frame_id':f['id']};event('screenshot-request',f,request)
            event('screenshot',f,{'request':request,'path':'surface.ppm','width':1600,'height':900,'sha256':sha(self.d/'surface.ppm')},30_000_000)
        screenshot('batch-baseline',baseline)
        if mode=='move':screenshot('batch-preview',frames[90])
        screenshot('batch-redo' if mode=='move' else 'batch-cancel',last)
        self.report={'schema_version':2,'stage':'S5-M2-B','observation_version':2,'last_observed_frame_id':len(frames),'profile':'release','adapter':'Metal test','failures':[],'request':request,'binary_sha256':'binary','fixture_sha256':request['fixture_sha256'],'protocol_sha256':request['protocol_sha256'],'frames':frames,'worker':workers,'counters':{'custom-buffer-largest-observed-bytes':10000,'draw':len(frames),'uniform-upload':112*len(frames)},'snapshots':snapshots,'events':events}
        write(self.d/'request.json',request);write(self.d/'runner.json',{'exit_code':0,'error':None,'binary_sha256':'binary','fixture_sha256':request['fixture_sha256']});write(self.d/'process-samples.json',[{'rss_bytes':1000000,'cpu_percent':10}]*20)
    def tearDown(self):self.tmp.cleanup()
    def check(self):write(self.d/'observations.json',self.report);return verify(self.d)
    def rejected(self):
        with self.assertRaises((ValueError,KeyError)):self.check()
    def cancelled(self,new_project=False):self.fixture('new-project' if new_project else 'escape')
    def test_cancel_valid(self):self.cancelled();self.check()
    def test_new_project_valid(self):self.cancelled(True);self.check()
    def test_cancel_snapshot_history_disagrees(self):self.cancelled();self.report['snapshots'][1]['state']['undo']=1;self.rejected()
    def test_new_project_nonempty_snapshot(self):
        self.cancelled(True);snap=self.report['snapshots'][1];path=self.d/snap['path'];path.write_bytes(struct.pack('<dd',1,1));snap['count']=1;snap['sha256']=sha(path);self.rejected()
    def test_valid(self):self.assertEqual(self.check()['selected'],100)
    def test_snapshot_history_disagrees(self):self.report['snapshots'][1]['state']['undo']=2;self.rejected()
    def test_snapshot_document_disagrees(self):self.report['snapshots'][2]['state']['document_id']='other';self.rejected()
    def test_snapshot_revision_disagrees(self):self.report['snapshots'][3]['state']['revision']='99';self.rejected()
    def test_duplicate_snapshot(self):self.report['snapshots'].append(self.report['snapshots'][1]);self.rejected()
    def test_duplicate_completion_event(self):self.report['events'].append(next(e for e in self.report['events'] if e['label']=='commit-complete'));self.rejected()
    def test_no_release(self):self.report['profile']='debug';self.rejected()
    def test_duplicate_input(self):self.report['events'].append(next(e for e in self.report['events'] if e['label']=='press'));self.rejected()
    def test_gpu_clock_mismatch(self):self.report['frames'][20]['input_gpu_complete_ms']=0;self.rejected()
    def test_nonmonotonic_counter(self):self.report['frames'][20]['counters']['uniform-upload']=0;self.rejected()
    def test_zero_zombie_rss_preserved(self):
        write(self.d/'process-samples.json',[{'rss_bytes':1000000,'cpu_percent':10}]*20+[{'rss_bytes':0,'cpu_percent':0,'process_state':'Z','completed_report_present':True}]);self.check()
    def test_zero_exiting_rss_after_report(self):
        write(self.d/'process-samples.json',[{'rss_bytes':1000000,'cpu_percent':10}]*20+[{'rss_bytes':0,'cpu_percent':0,'process_state':'?E','completed_report_present':True}]);self.check()
    def test_zero_exiting_rss_before_report_rejected(self):
        write(self.d/'process-samples.json',[{'rss_bytes':1000000,'cpu_percent':10}]*20+[{'rss_bytes':0,'cpu_percent':0,'process_state':'?E','completed_report_present':False}]);self.rejected()
    def test_zero_live_rss_after_report_rejected(self):
        write(self.d/'process-samples.json',[{'rss_bytes':1000000,'cpu_percent':10}]*20+[{'rss_bytes':0,'cpu_percent':0,'process_state':'R','completed_report_present':True}]);self.rejected()
    def test_zero_live_rss_rejected(self):
        write(self.d/'process-samples.json',[{'rss_bytes':1000000,'cpu_percent':10}]*20+[{'rss_bytes':0,'cpu_percent':0,'process_state':'R'}]);self.rejected()
    def test_missing_frames(self):self.report['frames']=[];self.rejected()
    def test_partial_history(self):self.report['frames'][30]['state']['undo']=1;self.rejected()
    def test_snapshot_changed(self):self.report['frames'][30]['snapshot_identity_unchanged']=False;self.rejected()
    def test_undrawn_frame(self):self.report['frames'][30]['painted']=False;self.rejected()
    def test_index_upload(self):[f for f in self.report['frames'] if f['phase']==6][-1]['counters']['index-storage-init-upload']=4;self.rejected()
    def test_wrong_request(self):self.report['request']=dict(self.report['request'],selected=500);self.rejected()
    def test_protocol_changed(self):self.report['protocol_sha256']='wrong';self.rejected()
    def test_fixture_changed(self):self.report['fixture_sha256']='wrong';self.rejected()
    def test_binary_changed(self):self.report['binary_sha256']='wrong';self.rejected()
    def test_missing_coords(self):
        (self.d/'moved.f64le').unlink()
        with self.assertRaises(ValueError):self.check()
    def test_short_drag(self):next(e for e in self.report['events'] if e['label']=='release')['at_ns']=4_000_000_000;self.rejected()
    def test_duplicate_operation(self):self.report['worker'].append({'action':'drag-move'});self.rejected()
    def test_wrong_preview(self):self.report['frames'][30]['paint_delta'][0]=100;self.rejected()
    def test_corrupt_unselected_with_valid_hash(self):
        s=next(s for s in self.report['snapshots'] if s['label']=='moved');p=self.d/s['path'];data=bytearray(p.read_bytes());data[16*90000:16*90000+8]=struct.pack('<d',999);p.write_bytes(data);s['sha256']=sha(p);self.rejected()
    def test_cpu_memory_missing(self):write(self.d/'process-samples.json',[]);self.rejected()
    def test_missing_screenshot(self):self.report['events']=[e for e in self.report['events'] if e['label']!='screenshot'];self.rejected()
    def test_causal_clock(self):self.report['frames'][30]['completed_ns']=0;self.rejected()
    def preview(self):return [f for f in self.report['frames'] if f['phase']==6]
    def completion(self,label='commit-complete'):return next(e for e in self.report['events'] if e['label']==label)
    def test_truncated_six_seconds(self):
        removed={f['id'] for f in self.preview()[101:]};self.report['frames']=[f for f in self.report['frames'] if f['id'] not in removed];self.rejected()
    def test_missing_first_preview(self):self.report['frames'].remove(self.preview()[0]);self.rejected()
    def test_missing_last_preview(self):self.report['frames'].remove(self.preview()[-1]);self.rejected()
    def test_missing_middle_preview(self):self.report['frames'].remove(self.preview()[100]);self.rejected()
    def test_rephase_middle_preview(self):self.preview()[100]['phase']=7;self.rejected()
    def test_fake_final_frame_count(self):self.report['last_observed_frame_id']-=1;self.rejected()
    def test_zero_preview_and_paint(self):
        for f in self.preview():f['state']['delta']={'x_mm':0.,'y_mm':0.};f['paint_delta']=[0.,0.,0.,0.]
        self.rejected()
    def test_missing_preview_delta(self):self.preview()[100]['state']['delta']=None;self.rejected()
    def test_pointer_state_paint_all_stationary(self):
        press=next(e for e in self.report['events'] if e['label']=='press')['data']['position']
        for f in self.preview():f['injected']['pointer']=press;f['gesture']['last']=press;f['state']['delta']={'x_mm':0.,'y_mm':0.};f['paint_delta']=[0.,0.,0.,0.]
        self.rejected()
    def test_frozen_camera_changed_everywhere(self):
        for f in self.report['frames']:f['injected']['view']['scale']=1.;f['view']['scale']=1.
        self.rejected()
    def test_threshold_not_crossed(self):self.preview()[100]['gesture']['dragging']=False;self.rejected()
    def test_unpainted_completions(self):
        for f in self.report['frames']:
            if f['state']['revision'] in ('1','2','3'):f['painted']=False;f['gpu_completed']=False
        self.rejected()
    def test_completion_gpu_failure(self):self.report['frames'][self.completion()['data']['visible_frame_id']-1]['gpu_completed']=False;self.rejected()
    def test_completion_clock_before_gpu(self):self.completion()['data']['visible_completed_ns']=0;self.rejected()
    def test_worker_error(self):self.report['worker'][0]['error']='RESOURCE_LIMIT';self.rejected()
    def test_worker_result_wrong_revision(self):self.report['worker'][0]['batch']['state']['revision']='99';self.rejected()
    def test_worker_empty_result_state(self):self.report['worker'][0]['batch']['state']={};self.rejected()
    def test_worker_wrong_count(self):self.report['worker'][0]['selected_count']=1;self.rejected()
    def test_worker_completes_after_visible_frame(self):self.report['worker'][0]['batch']['finished_ns']=10**15;self.rejected()
    def test_completion_wrong_worker(self):self.completion()['data']['worker_sequence']=999;self.rejected()
    def test_new_project_model_only_completion(self):
        self.cancelled(True);e=self.completion('cancel-complete');e['data']['visible_frame_id']=next(e for e in self.report['events'] if e['label']=='release')['frame_id'];self.rejected()
    def test_cancel_unpainted_frame(self):
        self.cancelled();e=self.completion('cancel-complete');self.report['frames'][e['data']['visible_frame_id']-1]['painted']=False;self.rejected()
    def test_screenshot_tiny_p6(self):(self.d/'surface.ppm').write_bytes(b'P6\n1 1\n255\n000');self.rejected()
    def test_wrong_actual_press(self):self.report['frames'][1]['injected']['pointer']=[0.,0.];self.rejected()
    def test_unexpected_worker_after_cancel(self):
        self.cancelled();self.report['worker']=[{'sequence':1,'action':'undo','error':None,'batch':{'started_ns':10**15}}];self.rejected()
    def test_screenshot_ack_before_gpu(self):
        next(e for e in self.report['events'] if e['label']=='screenshot')['at_ns']=0;self.rejected()
    def test_missing_worker_observation(self):self.report['worker'][0]['batch']=None;self.rejected()
    def renumbered_omission(self,which):
        from test_batch_drag_package_negatives import omit,renumber
        omit(self.report,which);renumber(self.report);self.rejected()
    def test_first_preview_deleted_and_renumbered(self):self.renumbered_omission('first')
    def test_middle_preview_deleted_and_renumbered(self):self.renumbered_omission('middle')
    def test_last_preview_deleted_and_renumbered(self):self.renumbered_omission('last')
    def test_short_chunk_deleted_and_renumbered(self):self.renumbered_omission('chunk')
    def test_extra_callback_same_input_frame(self):self.preview()[30]['counters']['draw']+=1;self.rejected()
    def test_unmatched_uniform_callback(self):self.preview()[30]['counters']['uniform-upload']+=112;self.rejected()
    def test_final_draw_counter_missing_sample(self):self.report['counters']['draw']+=1;self.rejected()
    def test_final_uniform_counter_missing_sample(self):self.report['counters']['uniform-upload']+=112;self.rejected()
    def test_worker_zero_elapsed(self):self.report['worker'][0]['elapsed_ms']=0.;self.rejected()
    def test_worker_elapsed_disagrees(self):self.report['worker'][0]['elapsed_ms']=1.;self.rejected()
    def test_worker_elapsed_later_than_finish(self):self.report['worker'][0]['elapsed_ms']=10.;self.rejected()
    def test_worker_elapsed_observer_gap(self):self.report['worker'][0]['elapsed_ms']=8.5;self.check()
    def test_path_escape(self):self.report['snapshots'][0]['path']='../outside';self.rejected()
if __name__=='__main__':unittest.main()
