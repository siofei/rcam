"""Versioned external copy from product 42db040; only bind_frames stall policy is parameterized.

Strict behavior remains the default. Used by report_pmix_v1 after product binding.
"""
import argparse
import hashlib
import json
import math
from pathlib import Path
import struct
from verify_s5m2_evidence import load as strict_load, safe as strict_safe

ROOT=Path(__file__).resolve().parents[1]
PROTOCOL=ROOT/'fixtures/synthetic/s5m2b/protocol.json'
def sha(p): return hashlib.sha256(p.read_bytes()).hexdigest()
def require(ok, why):
    if not ok: raise ValueError(why)
def percentile(values,q):
    require(bool(values),'missing samples')
    return sorted(values)[math.ceil(len(values)*q)-1]
def relative(root,name):
    return strict_safe(root,name)

def f32(x):
    return struct.unpack('<f',struct.pack('<f',x))[0]
def close(a,b,tolerance,why):
    require(isinstance(a,(int,float)) and isinstance(b,(int,float)) and math.isfinite(a) and math.isfinite(b) and abs(a-b)<=tolerance,why)
def draw_complete(f):
    require(f['painted'] is True and f.get('gpu_completed') is True,'frame not drawn/GPU complete')
    require(f['input_ns']<=f['observed_ns']<=f['completed_ns'],'frame causal times')
    close(f['input_gpu_complete_ms'],(f['completed_ns']-f['input_ns'])/1e6,1,'GPU completion clock mismatch')
# Frozen gpu::Uniforms: seven [f32/u32; 4] fields, each 16 bytes.
UNIFORM_BYTES = 7 * 4 * 4

def bind_callbacks(r):
    draw = uniform = 0
    painted_count = 0
    for f in r['frames']:
        require(type(f['painted']) is bool,'invalid paint observation')
        now_draw=f['counters'].get('draw',0);now_uniform=f['counters'].get('uniform-upload',0)
        require(type(now_draw) is int and type(now_uniform) is int,'invalid callback counter')
        callbacks=int(f['painted'])
        require(now_draw-draw==callbacks,'draw callback/sample coverage mismatch')
        require(now_uniform-uniform==UNIFORM_BYTES*callbacks,'uniform callback/sample coverage mismatch')
        painted_count+=callbacks
        require(now_draw==painted_count and now_uniform==UNIFORM_BYTES*painted_count,'cumulative callback/sample mismatch')
        draw,uniform=now_draw,now_uniform
    require(r['counters'].get('draw',0)==draw and r['counters'].get('uniform-upload',0)==uniform,'final callback/sample coverage mismatch')

def worker_elapsed_ms(w):
    b=w['batch'];derived=(b['finished_ns']-b['started_ns'])/1e6
    legacy=w['elapsed_ms']
    require(type(legacy) in (int,float) and math.isfinite(legacy) and legacy>0,'invalid worker elapsed sample')
    require(-1e-6<=derived-legacy<=1.,'worker elapsed/shared-clock mismatch')
    return derived

def verify_c_centers(w, preceding):
    receipt=w['batch']['receipt'];version=receipt['input'];state=w['batch']['state']
    require(w['action']=='selection-centers' and type(receipt['task_id']) is int
            and receipt['task_id']==w['sequence'] and receipt['state']=='completed'
            and receipt['input']==receipt['result_version'],'C read-only full task receipt')
    require(set(version)=={'document_id','document_revision','workspace_revision','generation','rule_revision','geometry_policy_hash'}
            and version['document_id']==state['document_id'] and version['document_revision']==state['revision']
            and version['workspace_revision']==state['workspace_revision'],'C read-only TaskVersion/state binding')
    require(type(version['generation']) is int and version['generation']==2
            and type(version['rule_revision']) is int and version['rule_revision']==0
            and version['geometry_policy_hash']==hashlib.sha256(b'{"resolution_mm":0.0001}').hexdigest(),
            'C fixed-scene generation/rule/manufacturing precision fencing')
    require(preceding is not None and state==preceding['batch']['state'],
            'C read-only query changed manufacturing/selection/scene/history')

C_WORKER_PATH = ['new-workspace','open','viewport','select-rect','selection-centers',
                 'probe-drag','drag-move','selection-centers','undo','selection-centers',
                 'redo','selection-centers']

def verify_c_worker_path(workers, release_input):
    # Classify setup too: an initial query relabelled "other" must not escape
    # the same complete TaskVersion/read-only-state checks as later queries.
    require([w['action'] for w in workers]==C_WORKER_PATH,
            'C complete typed setup/manufacturing/read-only worker sequence')
    require([w['action'] for w in workers if w['batch']['started_ns']>=release_input]
            ==C_WORKER_PATH[6:], 'C exact post-release worker boundary')

def verify_c_usage(runner, owned):
    usage=runner['owned_child_rusage']
    require(usage['method']=='macOS wait4 exact owned application child; ru_maxrss bytes; CPU seconds'
            and type(usage['pid']) is int and usage['pid']>0 and usage['pid']==owned['pid'],
            'C exact owned child completion/resource accounting')
    rss=usage['rss_peak_bytes'];elapsed=usage['child_elapsed_s']
    require(type(rss) is int and rss>0 and type(elapsed) in (int,float)
            and math.isfinite(elapsed) and elapsed>0,'C kernel peak RSS/elapsed invalid')
    times=[usage['user_cpu_s'],usage['system_cpu_s']]
    require(all(type(v) in (int,float) and math.isfinite(v) and v>=0 for v in times)
            and sum(times)>0,'C actual child CPU accounting invalid')
    return rss,{'cpu_total_s':sum(times),'cpu_percent_average':sum(times)/elapsed*100,
                'rss_method':usage['method'],'resource_scope':'exact owned child lifetime; no periodic CPU maximum claim'}

def bind_frames(r,p,*,performance=None):
    require(r['observation_version']==2,'missing input/completion observations')
    all_frames=r['frames'];require(bool(all_frames),'missing frames')
    require(len(all_frames)==r['last_observed_frame_id'] and all(f['id']==i for i,f in enumerate(all_frames,1)),'missing/duplicate frame ID coverage')
    bind_callbacks(r)
    indexed={f['id']:f for f in all_frames};events={e['label']:e for e in r['events']}
    for a,b in zip(all_frames,all_frames[1:]):
        require(a['input_ns']<b['input_ns'],'frame input clock order')
        close(b['frame_interval_ms'],(b['input_ns']-a['input_ns'])/1e6,1e-4,'frame interval mismatch')
    for label in ('baseline','press','confirmed','release','undo-input','redo-input','commit-complete','undo-complete','redo-complete','cancel-complete'):
        if label not in events:continue
        e=events[label];f=indexed[e['frame_id']]
        require(f['input_ns']<=e['at_ns']<indexed[f['id']+1]['input_ns'],'event/input frame binding: '+label)
    confirmed=events['confirmed'];release=events['release'];c=confirmed['frame_id'];z=release['frame_id']
    require(c<z and indexed[c]['phase']==5 and indexed[z]['phase']==7,'confirmed/release phase binding')
    require([f['id'] for f in all_frames if f['phase']==6]==list(range(c+1,z)),'preview phase coverage')
    frames=[indexed[i] for i in range(c+1,z)];require(len(frames)>=10,'missing preview frames')
    times=[indexed[i]['frame_interval_ms'] for i in range(c+1,z+1)]
    edges=[(frames[0]['input_ns']-confirmed['at_ns'])/1e6,(release['at_ns']-frames[-1]['input_ns'])/1e6]
    if performance is None:
        require(all(0<t<=p['frame_stall_ms_max'] for t in times+edges),'preview interval/boundary stall')
    else:
        require(all(type(t) in (int,float) and math.isfinite(t) and t>0 for t in times+edges),
                'finite positive preview interval/boundary clock')
        performance.samples('preview_intervals_including_release', times)
        performance.samples('preview_boundary_intervals', edges)
        performance.compare('preview_interval_boundary_max_ms', max(times+edges), p['frame_stall_ms_max'])
    origin=confirmed['data']['trajectory_origin_ns']
    require(indexed[c]['observed_ns']<=origin<=confirmed['at_ns'],'trajectory origin/confirmation clock')
    require(confirmed['data']['state']==indexed[c]['state'],'confirmed state mismatch')
    mode=r['request']['mode'];seconds=p['drag_seconds'] if mode=='move' else p['interruption_drag_seconds']
    require(frames[-1]['input_ns']-origin<seconds*1e9<=indexed[z]['input_ns']-origin,'trajectory endpoint coverage')
    view=indexed[events['press']['frame_id']]['injected']['view']
    require(view['center_mm']==p['camera']['center_mm'] and view['scale']==p['camera']['logical_scale'],'frozen camera mismatch')
    require(view['grid_snap'] is False and view['object_snap'] is False and view['threshold_physical_px']==4,'snap/threshold changed')
    rect=view['rect'];ppp=view['ppp'];require(len(rect)==4 and math.isfinite(ppp) and ppp>0,'view shape')
    for axis in (0,1):close((rect[axis+2]-rect[axis])*ppp,p['canvas_physical'][axis],.5,'physical canvas mismatch')
    center=[f32(f32(rect[i]+rect[i+2])*.5) for i in (0,1)];scale=view['scale'];wc=view['center_mm']
    press=[f32(center[0]+f32((p['drag_press_mm'][0]-wc[0])*scale)),f32(center[1]-f32((p['drag_press_mm'][1]-wc[1])*scale))]
    press_frame=indexed[events['press']['frame_id']]
    require(press_frame['injected']['pressed'] is True and press_frame['view']==view,'missing press button/view')
    require(press_frame['injected']['pointer']==press,'actual press differs from frozen press')
    for a,b in zip(events['press']['data']['position'],press):close(a,b,1e-4,'frozen press coordinate')
    def world(pos):return [wc[0]+f32(pos[0]-center[0])/scale,wc[1]-f32(pos[1]-center[1])/scale]
    start=world(press);dragging=False
    for f in frames+[indexed[z]]:
        inp=f['injected'];require(inp['view']==view,'input camera changed')
        require(inp['trajectory_origin_ns']==origin,'input trajectory clock mismatch')
        t=f32(min((f['input_ns']-origin)/(seconds*1e9),1.))
        expected=[f32(press[0]+f32(36*t)),f32(press[1]+f32(f32(-18*t)+f32(8*f32(math.sin(f32(f32(math.tau)*t))))))]
        if f['id']==z:expected=[f32(press[0]+36),f32(press[1]-18)]
        pos=inp['pointer'];require(isinstance(pos,list) and len(pos)==2,'missing injected pointer')
        for a,b in zip(pos,expected):close(a,b,1e-4,'frozen pointer trajectory mismatch')
        require(inp['pressed'] is False and inp['released']==(f['id']==z),'pointer button phase mismatch')
        if f['id']==z:
            require(inp['escape']==(mode=='escape') and inp['pointer_gone']==(mode=='pointergone') and inp['focused']==(mode!='blur'),'release/cancel input mismatch')
            for a,b in zip(release['data']['position'],expected):close(a,b,1e-4,'release input position mismatch')
            for i,k in enumerate(('x_mm','y_mm')):close(release['data']['delta_mm'][k],world(pos)[i]-start[i],1e-9,'release manufacture delta mismatch')
            continue
        require(f['view']==view and inp['focused'] and not inp['escape'] and not inp['pointer_gone'],'preview input/view changed')
        distance=f32(math.sqrt(f32(f32(pos[0]-press[0])**2)+f32(f32(pos[1]-press[1])**2)))
        dragging=dragging or f32(distance*ppp)>=4
        gesture=f['gesture'];require(gesture is not None and gesture['confirmed'] is True and gesture['dragging']==dragging and gesture['error'] is None,'gesture confirmation/threshold mismatch')
        require(gesture['last']==pos,'processed pointer mismatch')
        require(f['state']['delta'] is not None,'missing preview delta')
        for i,k in enumerate(('x_mm','y_mm')):
            expected_delta=world(pos)[i]-start[i] if dragging else 0
            close(f['state']['delta'][k],expected_delta,1e-9,'trajectory/manufacture preview mismatch')
            close(f['paint_delta'][i],expected_delta,1e-4,'trajectory/paint preview mismatch')
    require(dragging,'trajectory never crossed drag threshold')
    return frames,times,indexed

def bind_completion(r,events,indexed,label,origin,action):
    e=events[label];d=e['data'];f=indexed[d['visible_frame_id']]
    require(f['id']==e['frame_id']-1,'completion does not link preceding callback')
    draw_complete(f)
    require(d['visible_completed_ns']==f['completed_ns']<=indexed[e['frame_id']]['input_ns']<=e['at_ns'],'completion precedes GPU/clock mismatch')
    require(f['state']==d['state'] and f['state']['busy'] is False and f['state']['display_pending'] is False,'visible completion state mismatch')
    require(f['state']['delta'] is None and f['paint_delta']==[0.,0.,0.,0.],'completion retained preview')
    trigger=indexed[events[origin]['frame_id']]['input_ns']
    require(f['input_ns']>=trigger,'completion frame predates operation')
    close(d['duration_ms'],(e['at_ns']-trigger)/1e6,2,'completion duration origin mismatch')
    if action is None:
        require(d['worker_sequence'] is None,'cancel references mutation worker');return
    matches=[w for w in r['worker'] if w['sequence']==d['worker_sequence']]
    require(len(matches)==1,'missing/duplicate completion worker sequence');w=matches[0]
    require(w['action']==action and w['error'] is None,'worker action/error contradicts completion')
    b=w['batch'];require(b is not None,'missing worker observations')
    require(trigger<=b['started_ns']<=b['finished_ns']<=f['observed_ns'],'worker/result/completion causal order')
    require(set(b['state'])=={'document_id','revision','workspace_revision','dirty','project_dirty','undo','redo','selected','scene_serial','scene_objects'},'worker result state fields missing')
    for k,v in b['state'].items():require(v==f['state'][k],'worker/result state mismatch: '+k)
    require(w['selected_count']==f['state']['selected'] and w['scene_count']==f['state']['scene_objects'],'worker result counts mismatch')

def verify(directory, *, c_investigation=False):
    p=strict_load(PROTOCOL); r=strict_load(directory/'observations.json')
    request=strict_load(directory/'request.json'); runner=strict_load(directory/'runner.json')
    require(type(r['schema_version']) is int and r['schema_version']==2 and r['stage']=='S5-M2-B','stage/schema')
    require(r['profile']=='release' and 'Metal' in r['adapter'],'release/Metal required')
    require(not r['failures'],'native failures: '+repr(r['failures']))
    require(r['request']==request,'request mismatch')
    require(runner['exit_code']==0 and runner['error'] is None,'runner failed')
    require(r['binary_sha256']==runner['binary_sha256'],'binary mismatch')
    require(r['fixture_sha256']==request['fixture_sha256']==runner['fixture_sha256']==p['fixture_sha256'],'fixture mismatch')
    require(r['protocol_sha256']==sha(PROTOCOL)==request['protocol_sha256'],'protocol mismatch')
    n=request['selected'];mode=request['mode'];require(n in p['selected_counts'],'selection size')
    events=r['events']; by={e['label']:e for e in events}
    for label in ('warmup-begin','baseline','press','confirmed','release'):
        require(sum(e['label']==label for e in events)==1,'missing/duplicate '+label)
    require(by['baseline']['at_ns']<by['press']['at_ns']<by['confirmed']['at_ns']<by['release']['at_ns'],'input/confirm/release ordering')
    require(by['baseline']['at_ns']-by['warmup-begin']['at_ns']>=p['warmup_seconds']*1e9,'warmup too short')
    baseline=by['baseline']['data']['state']; require(baseline['selected']==n and baseline['scene_objects']==100000,'baseline workload')
    frames,times,indexed=bind_frames(r,p)
    sequences=[w['sequence'] for w in r['worker']]
    require(sequences==sorted(set(sequences)),'worker sequence uniqueness/order')
    require(all(w['error'] is None for w in r['worker']),'worker error')
    for w in r['worker']:
        require(w.get('batch') is not None,'missing worker clock/result observation')
        b=w['batch'];require(0<=b['started_ns']<=b['finished_ns']<=r['frames'][-1]['input_ns'],'worker lifetime outside recorded run')
        worker_elapsed_ms(w)
    for a,b in zip(r['worker'],r['worker'][1:]):require(a['batch']['finished_ns']<=b['batch']['started_ns'],'overlapping/out-of-order worker results')
    release_input=indexed[by['release']['frame_id']]['input_ns']
    after=[w['action'] for w in r['worker'] if w.get('batch') and w['batch']['started_ns']>=release_input]
    if c_investigation:
        require(request['stage']=='S5-I2-C-menu-investigation' and request['alt_bypass'] is True
                and n==5000 and mode=='move','C investigation frozen scene/Alt declaration')
        verify_c_worker_path(r['worker'],release_input)
        for frame in r['frames']:
            inp=frame['injected']
            if 5<=frame['phase']<=7 and inp['pointer'] is not None:
                require(inp['modifiers']=={'alt':True,'ctrl':False,'shift':False,'command':False}
                        and all(inp['button_alt']),'C actual Alt bypass input omitted')
        preceding=None
        for w in r['worker']:
            if w['action']=='selection-centers':
                verify_c_centers(w,preceding)
            preceding=w
    else:
        require(after==(['drag-move','undo','redo'] if mode=='move' else (['new-project'] if mode=='new-project' else [])),'unexpected post-release worker path')
    for a,b in zip(r['frames'],r['frames'][1:]):
        require(a['id']<b['id'] and a['input_ns']<b['input_ns'],'frame order')
        for k in ('scene-allocation','index-allocation','geometry-storage-init-upload','index-storage-init-upload','selection-upload','uniform-upload'):
            require(b['counters'].get(k,0)>=a['counters'].get(k,0),'nonmonotonic GPU counter')
    for f in frames:
        require(f['painted'] and f.get('gpu_completed') is True,'preview not drawn/completed')
        require(f['focused'] is True,'preview lost focus')
        require(f['input_ns']<=f['observed_ns']<=f['completed_ns'],'frame causal times')
        require(abs(f['input_gpu_complete_ms']-(f['completed_ns']-f['input_ns'])/1e6)<1,'GPU completion clock mismatch')
        require(f['snapshot_identity_unchanged'] and f['scene_identity_unchanged'] and f['preview_index_identity_unchanged'],'preview mutated snapshot/scene/index')
        for k in ('document_id','revision','workspace_revision','dirty','project_dirty','undo','redo','selected','scene_serial','scene_objects'):
            require(f['state'][k]==baseline[k],'partial preview '+k)
        require(all(abs(a-b)<.5 for a,b in zip(f['state']['canvas_physical'],p['canvas_physical'])),'canvas changed')
        require(f['prepare']['preview_index_ms']==0,'preview rebuilt index')
        require(f['ui_allocation_count_bytes'] is not None,'missing allocator measurement')
        if f['state']['delta'] is not None:
            for k,key in enumerate(('x_mm','y_mm')):require(abs(f['paint_delta'][k]-f['state']['delta'][key])<1e-4,'paint/model preview delta mismatch')
    # First confirmed frame may install unchanged selection flags from ProbeDrag.
    # Every subsequent preview upload is measured between the actual callbacks.
    first,last=frames[0]['counters'],frames[-1]['counters']
    for k in ('scene-allocation','index-allocation','geometry-storage-init-upload','index-storage-init-upload','selection-upload','geometry-patch-attempt','geometry-full-build-attempt'):
        require(last.get(k,0)==first.get(k,0),'steady preview work: '+k)
    require(last.get('uniform-upload',0)>first.get('uniform-upload',0),'missing uniform uploads')
    if c_investigation:
        rss,resource_summary=verify_c_usage(runner,strict_load(directory/'owned-process.json'))
        samples=None
    else:
        samples=strict_load(directory/'process-samples.json');require(len(samples)>10,'missing RSS/CPU')
    # macOS ps(1): Z is a zombie; the E modifier means trying to exit.
    # Keep these samples, but require the completed native report to have existed
    # at sampling time. A running/unfinished process must always have positive RSS.
    def valid_sample(v):
        state=v.get('process_state','')
        exiting=state.startswith('Z') or 'E' in state[1:]
        terminal_zero=v['rss_bytes']==0 and exiting and v.get('completed_report_present') is True
        return (v['rss_bytes']>0 or terminal_zero) and math.isfinite(v['cpu_percent'])
    if not c_investigation:
        require(all(valid_sample(v) for v in samples),'invalid process metrics')
        require(sum(v['rss_bytes']>0 for v in samples)>10,'missing live process metrics')
        rss=max(v['rss_bytes'] for v in samples)
        resource_summary={'cpu_percent_max':max(v['cpu_percent'] for v in samples)}
    require(rss<=p['rss_peak_bytes_max'],'RSS budget')
    require(r['counters']['custom-buffer-largest-observed-bytes']<=p['custom_gpu_bytes_max'],'GPU budget')
    require(mode=='move' or mode in p['interruptions'],'unsupported mode')
    snapshot_events={'before':'baseline'}
    if mode=='move':snapshot_events.update(moved='commit-complete',undo='undo-complete',redo='redo-complete')
    else:snapshot_events['cancelled']='cancel-complete'
    require([s['label'] for s in r['snapshots']]==list(snapshot_events),'missing/duplicate/reordered snapshot')
    snapshots={s['label']:s for s in r['snapshots']}
    for label,event in snapshot_events.items():
        require(sum(e['label']==event for e in events)==1,'missing/duplicate snapshot event '+event)
        require(snapshots[label]['state']==by[event]['data']['state'],'snapshot/event state mismatch: '+label)
    def coords(label):
        s=snapshots[label];file=relative(directory,s['path']);require(sha(file)==s['sha256'],'snapshot hash')
        data=file.read_bytes();require(len(data)==s['count']*16,'snapshot size');return list(struct.iter_unpack('<dd',data))
    before=coords('before');require(len(before)==100000,'baseline count')
    require(all(x==i%1000+1 and y==i//1000+1 for i,(x,y) in enumerate(before)),'fixture manufacturing truth')
    summary={'selected':n,'mode':mode,'frames':len(frames),'rss_peak_bytes':rss,
             **resource_summary,'ui_allocation_count_max':max(f['ui_allocation_count_bytes'][0] for f in frames)}
    if mode=='move':
        require((by['release']['at_ns']-by['confirmed']['at_ns'])/1e9>=p['drag_seconds'],'drag too short')
        require(len(frames)>=100,'insufficient performance samples')
        # Includes confirmed→first and last→release input intervals.
        # Derive intervals independently from the raw input clock.
        for a,b in zip(frames,frames[1:]):require(abs(b['frame_interval_ms']-(b['input_ns']-a['input_ns'])/1e6)<1e-4,'frame interval mismatch')
        metrics={'frame_p95_ms':percentile(times,.95),'frame_p99_ms':percentile(times,.99),'frame_max_ms':max(times),'input_gpu_p95_ms':percentile([f['input_gpu_complete_ms'] for f in frames],.95),'cpu_update_p95_ms':percentile([f['cpu_update_ms'] for f in frames],.95),'cpu_prepare_p95_ms':percentile([f['prepare']['cpu_prepare_ms'] for f in frames],.95)}
        summary.update(metrics)
        for k,limit in [('frame_p95_ms','frame_p95_ms_max'),('frame_p99_ms','frame_p99_ms_max'),('frame_max_ms','frame_stall_ms_max'),('input_gpu_p95_ms','input_gpu_complete_p95_ms_max')]:require(metrics[k]<=p[limit],f'{k} {metrics[k]} > {p[limit]}')
        require(by['release']['at_ns']<by['commit-complete']['at_ns']<by['undo-input']['at_ns']<by['undo-complete']['at_ns']<by['redo-input']['at_ns']<by['redo-complete']['at_ns'],'transaction input/completion order')
        for label,origin,step,undo,redo in [('commit-complete','release',1,baseline['undo']+1,0),('undo-complete','undo-input',2,baseline['undo'],1),('redo-complete','redo-input',3,baseline['undo']+1,0)]:
            bind_completion(r,by,indexed,label,origin,{'commit-complete':'drag-move','undo-complete':'undo','redo-complete':'redo'}[label])
            e=by[label];d=e['data'];s=d['state'];require(s['document_id']==baseline['document_id'],'transaction document')
            require(int(s['revision'])==int(baseline['revision'])+step and s['undo']==undo and s['redo']==redo,'transaction revision/history')
            measured=(e['at_ns']-indexed[by[origin]['frame_id']]['input_ns'])/1e6
            require(abs(measured-d['duration_ms'])<2,'transaction clock mismatch')
            require(d['duration_ms']<=p['commit_undo_redo_ms_max'][str(n)],label+' duration '+str(d['duration_ms']))
            summary[label+'_ms']=d['duration_ms']
        moved=coords('moved');undone=coords('undo');redone=coords('redo')
        require(before==undone and moved==redone,'Undo/Redo exact restoration')
        require(snapshots['before']['content_sha256']==snapshots['undo']['content_sha256'] and snapshots['moved']['content_sha256']==snapshots['redo']['content_sha256'],'full semantic restoration')
        require(len(moved)==len(before),'partial commit length')
        delta=by['release']['data']['delta_mm']
        for i,((x,y),(a,b)) in enumerate(zip(before,moved)):
            require(abs(a-x-(delta['x_mm'] if i<n else 0))<1e-9 and abs(b-y-(delta['y_mm'] if i<n else 0))<1e-9,'moved coordinate or unselected corruption')
        actions=[w['action'] for w in r['worker']]
        require(actions.count('drag-move')==actions.count('undo')==actions.count('redo')==1,'not one transaction path')
    else:
        require('cancel-complete' in by,'cancel incomplete')
        bind_completion(r,by,indexed,'cancel-complete','release','new-project' if mode=='new-project' else None)
        s=by['cancel-complete']['data']['state']
        require(by['cancel-complete']['at_ns']>by['release']['at_ns'],'cancel causal order')
        if mode=='new-project':
            require(coords('cancelled')==[],'new project snapshot is not empty')
            require(s['document_id']!=baseline['document_id'] and s['selected']==s['undo']==s['redo']==0 and s['scene_objects']==0,'new project retained drag/content')
        else:
            require(coords('cancelled')==before,'cancel changed positions')
            require(snapshots['cancelled']['content_sha256']==snapshots['before']['content_sha256'],'cancel semantic mutation')
            for k in ('document_id','revision','dirty','project_dirty','undo','redo','selected'):require(s[k]==baseline[k],'cancel changed '+k)
        require(not any(w['action']=='drag-move' for w in r['worker']),'cancel queued move')
    shots=[e for e in events if e['label']=='screenshot'];require(bool(shots),'missing surface readback')
    requests=[e for e in events if e['label']=='screenshot-request'];require(bool(requests),'missing screenshot request')
    expected_labels={'batch-baseline','batch-preview','batch-redo' if mode=='move' else 'batch-cancel'}
    if mode!='move':expected_labels.remove('batch-preview')  # interrupted after 1s; normal preview screenshot is at 2s
    require(expected_labels.issubset({e['data']['request']['label'] for e in shots}),'missing required screenshot response')
    for e in shots:
        d=e['data'];matching=[q for q in requests if q['data']==d['request']]
        require(len(matching)==1,'screenshot request/response binding');q=matching[0]
        require(q['frame_id']==d['request']['frame_id'] and q['at_ns']<e['at_ns'],'screenshot causal binding')
        frame=indexed[q['frame_id']];draw_complete(frame)
        require(e['at_ns']>=frame['completed_ns'] and e['frame_id']>=q['frame_id'],'screenshot response precedes completed frame')
        if d['request']['label']=='batch-preview':require(frame['phase']==6 and frame['gesture']['dragging'],'screenshot is not moving preview')
        if d['request']['label'] in ('batch-redo','batch-cancel'):
            completed=by['redo-complete' if mode=='move' else 'cancel-complete']
            require(q['frame_id']==completed['frame_id'] and frame['state']==completed['data']['state'],'screenshot predates visible completion')
        data=relative(directory,d['path']).read_bytes();header=f"P6\n{d['width']} {d['height']}\n255\n".encode()
        require(data.startswith(header) and len(data)==len(header)+d['width']*d['height']*3,'invalid screenshot pixels/dimensions')
        require(d['width']>=p['canvas_physical'][0] and d['height']>=p['canvas_physical'][1] and hashlib.sha256(data).hexdigest()==d['sha256'],'screenshot hash/extent mismatch')
    return summary

def main():
    p=argparse.ArgumentParser(description=__doc__);p.add_argument('directory',type=Path);a=p.parse_args()
    try: print(json.dumps(verify(a.directory),indent=2));return 0
    except (OSError,ValueError,KeyError,TypeError) as e:print('FAIL:',e);return 1
if __name__=='__main__':raise SystemExit('Use current_refresh.py report with the authenticated product root.')
