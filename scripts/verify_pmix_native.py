"""Read-only, portable S5-M2-C raw-frame and manufacturing evidence checks."""
import argparse,copy,hashlib,json,math,sys
from pathlib import Path
ROOT=Path(__file__).resolve().parents[1]
sys.path.insert(0,str(ROOT/'scripts'))
from verify_batch_drag_native import require,close,percentile,bind_callbacks,draw_complete,f32,bind_frames
from verify_s5m2_evidence import load,safe
from verify_pmix_workflow import verify_workflow,verify_cross_layer_workflow,decode_project,snapshot_project
from verify_i2_c_ui_roi import analyze as analyze_roi, rows as roi_rows
from verify_pmix_capture import capture_receipts
from pmix_owned_command import verify_display_environment
from pmix_display_swift import PROBE_SOURCE as DISPLAY_PROBE_SWIFT, MUTATOR_SOURCE as DISPLAY_MUTATOR_SWIFT

def sha(p):return hashlib.sha256(p.read_bytes()).hexdigest()
def directory_path(root,name):
    require(type(name) is str and name and not Path(name).is_absolute() and "\\" not in name and all(p not in ('','..','.') for p in name.split('/')),'unsafe ROI directory')
    path=root
    for part in name.split('/'):
        path=path/part
        require(not path.is_symlink(),'symlink ROI directory')
    require(path.is_dir() and path.resolve().is_relative_to(root.resolve()),'missing/escaped ROI directory')
    return path

DRAG_INPUT_POLICY={'version':1,'modes':['move','escape','new-project'],'incoming_phases':[4,5,6,7],
                   'bypass':'alt-only','other_frames':'none-except-explicit-command-shortcuts',
                   'production_contour':'unchanged'}
MODIFIER_KEYS={'alt','ctrl','shift','command','mac_cmd'}
# tick records its phase before changing control state. These are only the
# raw-input hook's transitions; conditional hooks can retain their input phase.
COMMON_INPUT_PHASES={0,1,12,13,14}
MODE_INPUT_PHASES={
    'move':COMMON_INPUT_PHASES|set(range(2,12)),
    'escape':COMMON_INPUT_PHASES|set(range(2,8)),
    'new-project':COMMON_INPUT_PHASES|set(range(2,8)),
    'nav':COMMON_INPUT_PHASES|{2,3,20,21},
    'points':COMMON_INPUT_PHASES|{2,3}|set(range(30,39)),
    'workflow':COMMON_INPUT_PHASES|{70,71,75,76},
    'workflow-reopen':COMMON_INPUT_PHASES,
    'workflow-cross-layer':COMMON_INPUT_PHASES|{70,71},
}
INPUT_PHASE_TRANSITIONS={4:{5},6:{6,7},8:{9},10:{11},20:{20,21},
                         32:{32,33},35:{35,36},36:{36,37},37:{37,38},75:{75,76}}


def verify_input_modifiers(report,frames,native_inputs):
    """Bind the free trajectory to actual injected and egui-processed modifiers."""
    require(type(native_inputs['version']) is int and native_inputs['version']==2
            and native_inputs['drag_input_policy']==DRAG_INPUT_POLICY
            and type(native_inputs['drag_input_policy']['version']) is int,
            'PMIX declared free-trajectory input policy')
    mode=report['request']['mode'];require(mode in MODE_INPUT_PHASES,'PMIX input policy mode')
    drag=mode in DRAG_INPUT_POLICY['modes']
    events=report['events'];anchors={}
    for label in ('press','release','undo-input','redo-input'):
        matches=[event for event in events if event['label']==label]
        require(len(matches)<=1,'duplicate modifier input anchor '+label)
        if matches:anchors[label]=matches[0]['frame_id']
    expected_anchors=({'press','release','undo-input','redo-input'} if mode=='move'
                      else {'press','release'} if drag else set())
    require(set(anchors)==expected_anchors
            and all(type(number) is int and any(frame['id']==number for frame in frames)
                    for number in anchors.values()),'PMIX modifier input anchors/mode')
    def modifiers(value,expected):
        require(type(value) is dict and set(value)==MODIFIER_KEYS
                and all(type(value[key]) is bool for key in MODIFIER_KEYS)
                and value==expected,'actual PMIX modifier fields/bypass')
    for frame in frames:
        injected=frame['injected'];incoming=injected.get('input_phase');observed=frame['phase']
        require(type(injected.get('input_policy_version')) is int and injected['input_policy_version']==1
                and type(incoming) is int and 0<=incoming<=76,'PMIX actual input policy/version/phase')
        require(type(observed) is int and incoming in MODE_INPUT_PHASES[mode]
                and observed in MODE_INPUT_PHASES[mode]
                and observed in INPUT_PHASE_TRANSITIONS.get(incoming,{incoming}),
                'PMIX actual input/observed phase transition')
        if drag and observed in (5,6,7):
            require(incoming in ({5:{4,5},6:{6},7:{6,7}}[observed]),'PMIX drag incoming/observed phase')
        else:require(not drag or incoming not in (4,5,6,7),'PMIX bypass phase outside drag')
        alt=drag and incoming in (4,5,6,7)
        expected=dict(alt=alt,ctrl=False,shift=False,command=False,mac_cmd=False)
        key=None
        if frame['id']==anchors.get('undo-input'):key='Z';expected.update(command=True,mac_cmd=True)
        if frame['id']==anchors.get('redo-input'):key='Z';expected.update(command=True,mac_cmd=True,shift=True)
        if mode=='escape' and frame['id']==anchors.get('release'):key='Escape'
        if frame['id']==anchors.get('undo-input'):
            require(incoming==8 and observed==9,'PMIX actual Undo input phase')
        if frame['id']==anchors.get('redo-input'):
            require(incoming==10 and observed==11,'PMIX actual Redo input phase')
        modifiers(injected.get('modifiers'),expected);modifiers(frame.get('processed_modifiers'),expected)
        require(type(injected['pressed']) is bool and type(injected['released']) is bool,
                'PMIX actual pointer phase booleans')
        if drag:
            require(injected['pressed']==(frame['id']==anchors['press'])
                    and injected['released']==(frame['id']==anchors['release']),
                    'PMIX actual drag button anchors')
            require((incoming==4)==(frame['id']==anchors['press'])
                    and (incoming==6 and observed==7)==(frame['id']==anchors['release']),
                    'PMIX actual press/release transition anchors')
            if mode=='move':
                require((incoming==8)==(frame['id']==anchors['undo-input'])
                        and (incoming==10)==(frame['id']==anchors['redo-input']),
                        'PMIX actual history transition anchors')
        elif mode=='points':
            pair=(incoming,observed)
            require(injected['pressed']==(pair in ((32,33),(35,36)))
                    and injected['released']==(pair in ((32,33),(37,38))),
                    'PMIX actual point/box button transitions')
        else:
            require(not injected['pressed'] and not injected['released'],'PMIX extra non-point button')
        expected_buttons=[]
        for name,pressed in (('pressed',True),('released',False)):
            if injected[name]:expected_buttons.append(pressed)
        for field in ('buttons','keys'):
            raw=injected.get(field);processed=frame.get('processed_'+field)
            require(type(raw) is list and type(processed) is list and processed==raw
                    and json.dumps(processed,sort_keys=True)==json.dumps(raw,sort_keys=True),
                    'PMIX delivered button/key records: '+field)
        buttons=injected['buttons']
        require(len(buttons)==len(expected_buttons),'PMIX actual button count')
        for button,pressed in zip(buttons,expected_buttons):
            require(type(button) is dict and set(button)=={'button','pressed','position','modifiers'}
                    and button['button']=='Primary' and type(button['pressed']) is bool and button['pressed'] is pressed
                    and type(button['position']) is list and len(button['position'])==2
                    and all(type(v) in (int,float) and math.isfinite(v) for v in button['position'])
                    and button['position']==injected['pointer'],'PMIX actual button identity/position')
            modifiers(button['modifiers'],expected)
        keys=injected['keys'];require(len(keys)==(2 if key else 0),'PMIX actual key count')
        for event,pressed in zip(keys,(True,False)):
            require(type(event) is dict and set(event)=={'key','physical_key','pressed','repeat','modifiers'}
                    and event['key']==event['physical_key']==key and type(event['pressed']) is bool
                    and event['pressed'] is pressed and event['repeat'] is False,'PMIX actual key identity/phase')
            modifiers(event['modifiers'],expected)
        if drag and frame['id']==anchors.get('press'):
            require(incoming==4 and injected['pressed'] and not injected['released'] and alt,'PMIX actual Alt press')
        if drag and frame['id']==anchors.get('release'):
            require(incoming==6 and injected['released'] and not injected['pressed'] and alt,'PMIX actual Alt release/cancel')
    return {'policy':'free-trajectory-alt-v1','frames':len(frames),'production_contour':'unchanged'}


def bind_frame_producers(report, ui, paints, roi_requests, roi_samples, roi_finalization):
    """Exact complete coverage, including the real on-exit callback/fence.

    The existing UI = ordinary PMIX frames + 1 relation stays exact. Its one
    terminal record is mandatory and participates in every callback counter.
    """
    require(report.get('observation_version') == 3, 'PMIX terminal observation version')
    frames = report.get('frames')
    terminal = report.get('terminal_frame')
    require(type(frames) is list and frames and type(terminal) is dict,
            'PMIX missing ordinary/terminal frame')
    require(type(report.get('last_observed_frame_id')) is int
            and report['last_observed_frame_id'] == len(frames), 'PMIX ordinary frame coverage')
    require(len(ui) == len(frames) + 1, 'PMIX/UI frame producer coverage')
    all_frames = frames + [terminal]
    require(all(type(f.get('id')) is int and f['id'] == number
                for number, f in enumerate(all_frames, 1)), 'PMIX complete frame IDs')
    require(all(type(f.get('frame')) is int and f['frame'] == number
                for number, f in enumerate(ui, 1)), 'PMIX/UI complete frame IDs')
    previous_ui_input = -1
    for frame, ui_frame in zip(all_frames, ui):
        require(type(frame.get('pass_index')) is int
                and type(ui_frame.get('pass_index')) is int
                and frame['pass_index'] == ui_frame['pass_index'] == 0,
                'PMIX unmodelled additional update pass')
        ui_input = ui_frame['input']['t_ns']
        require(type(ui_input) is int and ui_input > previous_ui_input,
                'PMIX/UI reused raw input')
        previous_ui_input = ui_input
        state = frame['state']
        require(ui_frame['version'] == state['version']
                and ui_frame['scene_serial'] == state['scene_serial']
                and ui_frame['selected'] == state['selected']
                and ui_frame['busy'] == state['busy']
                and ui_frame['display_pending'] == state['display_pending'],
                'PMIX/UI actual scene/version/frame binding')
        require(ui_frame['canvas'] == frame['view']['rect']
                and ui_frame['ppp'] == frame['view']['ppp'], 'PMIX/UI physical canvas binding')
        require(frame['input_ns'] <= frame['observed_ns'], 'PMIX terminal input/update clock')
        if frame['painted']:
            draw_complete(frame)
        else:
            require('gpu_completed' not in frame and 'completed_ns' not in frame
                    and 'input_gpu_complete_ms' not in frame, 'unpainted PMIX fabricated fence')
    for first, second in zip(all_frames, all_frames[1:]):
        require(first['input_ns'] < second['input_ns'], 'PMIX complete input clock order')
        close(second['frame_interval_ms'],
              (second['input_ns'] - first['input_ns']) / 1e6, 1e-4,
              'PMIX terminal raw frame interval')
    require([p['frame'] for p in paints] == [f['id'] for f in all_frames if f['painted']],
            'PMIX/UI complete paint coverage')
    bind_callbacks(dict(report, frames=all_frames))
    exit_record = report.get('exit')
    require(type(exit_record) is dict, 'PMIX missing on-exit receipt')
    close_id = exit_record.get('close_requested_frame_id')
    close_ns = exit_record.get('close_requested_ns')
    exited_ns = exit_record.get('exited_ns')
    require(type(close_id) is int and 1 <= close_id <= len(all_frames)
            and type(close_ns) is int and type(exited_ns) is int, 'PMIX Close/exit identity')
    close_frame = all_frames[close_id - 1]
    close_end = (all_frames[close_id]['input_ns'] if close_id < len(all_frames) else exited_ns)
    require(close_frame['phase'] == terminal['phase'] == 14
            and close_frame['observed_ns'] <= close_ns < close_end,
            'PMIX actual Close update binding')
    if close_frame['painted']:
        require(close_ns <= close_frame['completed_ns'], 'PMIX Close after completed callback')
    require(exited_ns >= terminal.get('completed_ns', terminal['observed_ns'])
            and exited_ns > close_ns, 'PMIX final fence/on-exit clock')
    require(exit_record.get('full_surface_requests_drained') is True,
            'PMIX undrained full-surface requests')
    require(type(roi_finalization) is dict
            and exit_record.get('roi_finalization') == roi_finalization,
            'PMIX ROI finalization/exit binding')
    require(type(roi_finalization.get('schema_version')) is int
            and roi_finalization['schema_version'] == 1
            and type(roi_finalization.get('frames')) is int
            and roi_finalization.get('frames') == len(all_frames)
            and roi_finalization.get('quiesced') is True
            and roi_finalization.get('pending') is False
            and roi_finalization.get('writer_joined') is True, 'PMIX ROI drain/writer finalization')
    require(type(roi_finalization.get('requests')) is int
            and type(roi_finalization.get('samples')) is int
            and roi_requests and len(roi_requests) == len(roi_samples)
            == roi_finalization.get('requests') == roi_finalization.get('samples'),
            'PMIX ROI every request delivered')
    seen_requests = set()
    for number, (request, sample) in enumerate(zip(roi_requests, roi_samples)):
        key = (request.get('frame'), request.get('request_ns'))
        require(key not in seen_requests and type(sample.get('sample')) is int
                and sample['sample'] == number
                and sample.get('request') == request, 'PMIX ROI request/readback identity')
        seen_requests.add(key)
        request_id = request.get('frame')
        require(type(request_id) is int and 1 <= request_id <= close_id,
                'PMIX ROI request after Close')
        snapshot = {k: v for k, v in request.items() if k != 'request_ns'}
        require(snapshot == ui[request_id - 1], 'PMIX ROI actual request update binding')
        require(type(request.get('request_ns')) is int
                and request['request_ns'] >= ui[request_id - 1]['t_ns'], 'PMIX ROI request clock')
    return all_frames

CONVEX=[(320,0),(296,122),(226,226),(122,296),(0,320),(-122,296),(-226,226),(-296,122),(-320,0),(-296,-122),(-226,-226),(-122,-296),(0,-320),(122,-296),(226,-226),(296,-122)]
CONCAVE=[(-300,-300),(-100,-300),(-100,-200),(100,-200),(100,-300),(300,-300),(300,300),(100,300),(100,200),(-100,200),(-100,300),(-300,300),(-300,100),(-200,100),(-200,-100),(-300,-100)]

def point(p,x,y):
    require(set(p)=={'x_mm','y_mm'},'point shape');close(p['x_mm'],x,1e-9,'manufacturing X');close(p['y_mm'],y,1e-9,'manufacturing Y')
def manufacture(s,delta=(0.,0.)):
    require(len(s['layers'])==1 and not s['block_definitions'],'main workload layer/definitions')
    shapes=[{'Circle':{'diameter_mm':.5,'hole_diameter_mm':None}},{'Rectangle':{'width_mm':.6,'height_mm':.4,'hole_diameter_mm':None}},{'Obround':{'width_mm':.6,'height_mm':.3,'hole_diameter_mm':None}},{'Polygon':{'diameter_mm':.6,'vertices':6,'rotation_deg':0.,'hole_diameter_mm':None}}]
    require([a['shape'] for a in s['apertures'][:4]]==shapes,'frozen apertures')
    require(len(s['apertures'])==5 and s['apertures'][4]['shape']=={'Circle':{'diameter_mm':.1,'hole_diameter_mm':None}},'frozen stroke aperture')
    objects=s['layers'][0]['objects'];require(len(objects)==100000,'100k workload count')
    for i,o in enumerate(objects):
        require(o['object_id']==f'object-{i+1}' and o['exposure']=='dark','object ID/order/exposure')
        g=o['geometry'];require(len(g)==1,'geometry union');k=i%10;x=i%400+1.;y=i//400+1.
        if i//400<50 and i%400<20:x+=delta[0];y+=delta[1]
        if k<4:
            require('Flash' in g,'Flash count/type');a=g['Flash'];point(a['center'],x,y)
            require(a['aperture_id']==f'aperture-{10+k}' and a['transform']=={'mirror':'None','rotation_deg':0.,'scale':1.},'Flash reference/transform')
        elif k<7:
            require('Line' in g,'Line count/type');a=g['Line'];dx,dy=[(.3,0.),(0.,.3),(.25,.2)][k-4]
            point(a['start'],x-dx,y-dy);point(a['end'],x+dx,y+dy);require(a['width_mm']==.1,'Line width')
        elif k<9:
            require('Arc' in g,'Arc count/type');a=g['Arc'];v=a['path'];point(v['start'],x+.3,y);point(v['end'],x,y+.3);point(v['center'],x,y)
            require(a['width_mm']==.1 and v['direction']==('CounterClockwise' if k==7 else 'Clockwise') and v['full_circle'] is False,'Arc parameters')
            require(v['source']=={'resolution_mm':1e-6,'single_quadrant':False},'Arc source')
        else:
            require('Region' in g,'Region count/type');c=g['Region']['contours'];require(len(c)==1 and c[0]['role']=='Solid' and len(c[0]['edges'])==16,'Region contour/edges')
            template=CONVEX if i//10%2==0 else CONCAVE
            for j,e in enumerate(c[0]['edges']):
                require(list(e)==['Line'],'Region edge type');a,b=template[j];d,f=template[(j+1)%16]
                point(e['Line']['start'],x+a/1000,y+b/1000);point(e['Line']['end'],x+d/1000,y+f/1000)

def state_equal(a,b):
    for k in ('document_id','revision','workspace_revision','version','dirty','project_dirty','undo','redo','selected'):
        require(a[k]==b[k],'unexpected manufacturing/view mutation '+k)
def task_version(version):
    keys={'document_id','document_revision','workspace_revision','generation','rule_revision','geometry_policy_hash'}
    require(type(version) is dict and set(version)==keys,'complete typed TaskVersion fields')
    for key in ('generation','rule_revision'):require(type(version[key]) is int and 0<=version[key]<2**64,'TaskVersion unsigned '+key)
    require(version['rule_revision']==0,'frozen PMIX rule revision')
    if version['document_id'] is None:
        require(version['document_revision'] is None and version['workspace_revision'] is None and version['geometry_policy_hash']=='','empty document full version')
    else:
        require(type(version['document_id']) is str and version['document_id'],'document ID type')
        for key in ('document_revision','workspace_revision'):require(type(version[key]) is str and version[key].isascii() and version[key].isdigit() and int(version[key])<2**64,'TaskVersion revision type/range')
        require(version['geometry_policy_hash']==hashlib.sha256(b'{"resolution_mm":0.0001}').hexdigest(),'independent frozen manufacturing policy hash')

BOOTSTRAP_SOURCE_PATHS = (
    'crates/editor-app/src/main.rs',
    'crates/editor-app/src/native_pmix.rs',
    'crates/editor-app/src/native_s5m1.rs',
    'crates/editor-app/src/state.rs',
    'crates/editor-service/src/task.rs',
)

def bootstrap_source_contract(source_manifest_path):
    """Bind the reviewed constructor path to the actual producer's source.

    The caller has already authenticated this manifest's external digest. These
    production modules must match the reviewed ROOT bytes, not a permanently
    pinned whole checkout or the new verifier's own changed source manifest.
    A production-path change requires another bootstrap-contract review.
    """
    manifest = Path(source_manifest_path)
    require(manifest.is_file(), 'missing bootstrap producer source manifest')
    rows = manifest.read_text().splitlines()
    require(0 < len(rows) <= 2048, 'bootstrap source manifest bounds')
    declared = {}
    for row in rows:
        parts = row.split('  ', 1)
        require(len(parts) == 2 and len(parts[0]) == 64
                and all(c in '0123456789abcdef' for c in parts[0])
                and parts[1] and parts[1] not in declared,
                'bootstrap source manifest entry/duplicate')
        declared[parts[1]] = parts[0]
    proof = {}
    for path in BOOTSTRAP_SOURCE_PATHS:
        digest = sha(ROOT / path)
        require(declared.get(path) == digest, 'bootstrap reviewed production source binding ' + path)
        proof[path] = digest
    return proof

def bind_worker_requests(report, all_frames, source_manifest_path):
    """Keep the constructor enqueue and every ordinary UI enqueue observable.

    There is no UI frame zero. The only pre-input request is derived from the
    authenticated source's unique constructor send(NewWorkspace), and its raw
    action remains 'other'. No bootstrap event or semantic snapshot is invented.
    """
    source_proof = bootstrap_source_contract(source_manifest_path)
    requests = report['requests']
    workers = report['worker']
    require(type(requests) is list and type(workers) is list and len(requests) >= 2 and workers,
            'missing constructor request/worker coverage')
    require(all(type(w['sequence']) is int and 0 < w['sequence'] < 2**64 for w in workers)
            and [w['sequence'] for w in workers] == sorted({w['sequence'] for w in workers}),
            'worker sequence order/duplicate')
    require(all(type(q['sequence']) is int and 0 < q['sequence'] < 2**64 for q in requests)
            and [q['sequence'] for q in requests] == [w['sequence'] for w in workers],
            'accepted request/worker coverage')
    require(all(type(q['frame_id']) is int and q['frame_id'] >= 0 for q in requests)
            and [i for i, q in enumerate(requests) if q['frame_id'] == 0] == [0],
            'unique initial constructor frame-zero request')
    require(all_frames and all(type(f['id']) is int and f['id'] == i
                              for i, f in enumerate(all_frames, 1)), 'real UI frame IDs for requests')
    for frame in all_frames:
        task_version(frame['state']['version'])
        require(type(frame['input_ns']) is int and type(frame['observed_ns']) is int
                and 0 <= frame['input_ns'] <= frame['observed_ns'] < 2**64,
                'request UI unsigned causal clocks')
    indexed = {f['id']: f for f in all_frames}
    mode = report['request']['mode']
    worker_end = report['frames'][-1]['input_ns']
    for worker in workers:
        require(all(type(worker[k]) is int and 0 <= worker[k] < 2**64
                    for k in ('started_ns', 'finished_ns')), 'worker unsigned clock types')
        require((worker['error'] is None or
                 (mode in ('workflow', 'workflow-cross-layer') and worker['action'] == 'move'
                  and worker['error']['code'] == 'LAYER_LOCKED'))
                and worker['blocked'] is None
                and 0 <= worker['started_ns'] <= worker['finished_ns'] <= worker_end,
                'worker error/time')
    empty = dict(document_id=None, document_revision=None, workspace_revision=None,
                 generation=0, rule_revision=0, geometry_policy_hash='')
    first = requests[0]
    bootstrap = workers[0]
    require(first['sequence'] == bootstrap['sequence'] == 1
            and first['action'] == bootstrap['action'] == 'other', 'constructor source-derived action/sequence')
    require(first['input'] == first['view_version'] == empty, 'constructor empty initial full version')
    installed = None
    for number, (request_row, worker) in enumerate(zip(requests, workers)):
        require(type(request_row['at_ns']) is int and 0 <= request_row['at_ns'] < 2**64,
                'accepted enqueue unsigned clock')
        task_version(request_row['input']); task_version(request_row['view_version'])
        require(request_row['input'] == request_row['view_version'] == worker['receipt']['input']
                and request_row['action'] == worker['action'],
                'worker input bound to actual accepted enqueue full version/action')
        if number == 0:
            require(request_row['at_ns'] < all_frames[0]['input_ns']
                    and request_row['at_ns'] <= worker['started_ns'], 'constructor pre-input enqueue/worker clock')
            receipt = worker['receipt']; result = receipt['result_version']; state = worker['state']
            task_version(result); task_version(state['version'])
            require(type(receipt['task_id']) is int and receipt['task_id'] == 1
                    and receipt['state'] == 'completed' and worker['error'] is None
                    and worker['blocked'] is None, 'constructor successful typed terminal receipt')
            require(result['document_id'] is not None and result['document_revision'] == '0'
                    and result['workspace_revision'] == '0' and result['generation'] == 1
                    and state['version'] == result, 'constructor new workspace full result version')
            require(all(type(state[k]) is int and state[k] == 0 for k in ('undo', 'redo', 'selected'))
                    and type(state['scene_serial']) is int and state['scene_serial'] >= 0,
                    'constructor empty workspace worker state')
            matches = [f for f in all_frames if f['state']['version'] == result]
            require(matches, 'constructor result never installed in a real UI frame')
            installed = matches[0]
            require(worker['finished_ns'] <= installed['observed_ns'], 'constructor worker/UI installation clock')
            require(all(installed['state'][k] == state[k] for k in state),
                    'constructor actual installed worker state')
            ui_state = installed['state']
            require(all(type(ui_state[k]) is int and ui_state[k] == 0 for k in ('undo', 'redo', 'selected'))
                    and type(ui_state['scene_serial']) is int
                    and type(ui_state['scene_objects']) is int and ui_state['scene_objects'] == 0
                    and ui_state['primary'] is None and ui_state['dirty'] is False
                    and ui_state['project_dirty'] is False, 'constructor actual empty UI workspace')
            for frame in all_frames[:installed['id'] - 1]:
                before = frame['state']
                require(before['version'] == empty
                        and all(before[k] is None for k in ('document_id', 'revision', 'workspace_revision',
                                                           'undo', 'redo', 'dirty', 'project_dirty', 'scene_serial'))
                        and type(before['selected']) is int and before['selected'] == 0
                        and type(before['scene_objects']) is int and before['scene_objects'] == 0
                        and before['primary'] is None, 'constructor pre-installation UI initial state')
        else:
            require(request_row['frame_id'] in indexed, 'accepted enqueue missing real UI frame')
            frame = indexed[request_row['frame_id']]
            require(frame['state']['version'] == request_row['input']
                    and frame['input_ns'] <= request_row['at_ns'] <= worker['started_ns'],
                    'accepted enqueue UI full version/clock binding')
            if frame['id'] < len(all_frames):
                require(request_row['at_ns'] < indexed[frame['id'] + 1]['input_ns'], 'enqueue frame interval')
            if number == 1:
                require(request_row['input'] == bootstrap['receipt']['result_version']
                        and request_row['frame_id'] >= installed['id']
                        and request_row['at_ns'] >= bootstrap['finished_ns'],
                        'constructor next accepted request full continuity')
    previous_version = None
    for worker in workers:
        receipt = worker['receipt']; version = receipt['result_version']; state = worker['state']
        task_version(receipt['input']); task_version(version); task_version(state['version'])
        require(type(receipt['task_id']) is int and receipt['task_id'] == worker['sequence']
                and receipt['state'] == ('failed' if worker['error'] else 'completed'), 'typed terminal worker receipt')
        keys = {'document_id', 'document_revision', 'workspace_revision', 'generation', 'rule_revision', 'geometry_policy_hash'}
        require(set(receipt['input']) == set(version) == keys and version == state['version'],
                'complete TaskVersion receipt/state')
        require(version['document_id'] == state['document_id']
                and version['document_revision'] == state['revision']
                and version['workspace_revision'] == state['workspace_revision'],
                'TaskVersion document/workspace state binding')
        require(receipt['input']['generation'] == (previous_version['generation'] if previous_version is not None else 0),
                'initial/serial TaskVersion generation')
        if previous_version is not None:
            require(receipt['input'] == previous_version, 'serial worker full version continuity')
        require(version['generation'] == receipt['input']['generation']
                + int(version['document_id'] != receipt['input']['document_id']),
                'actual document replacement generation transition')
        previous_version = version
        if worker['action'] == 'selection-centers':
            require(receipt['input'] == version, 'read-only centers changed full version')
    for first_worker, second_worker in zip(workers, workers[1:]):
        require(first_worker['finished_ns'] <= second_worker['started_ns'], 'worker concurrency')
    return {'classification': 'source-derived constructor NewWorkspace', 'observed_action': first['action'],
            'sequence': 1, 'first_installed_ui_frame': installed['id'], 'production_sources': source_proof}

def visible_frame(f):
    draw_complete(f);require(not f['state']['display_pending'],'old-view fallback completion')
    paint=f['paint'];v=f['view'];require(paint['scene_serial']==f['state']['scene_serial'],'stale callback scene')
    rect=v['rect'];ppp=v['ppp']
    # Exact public camera->GPU contract, rounded to f32 only at display boundary.
    for actual,expected in zip(paint['uniform_view'],[rect[0]*ppp,rect[1]*ppp,(rect[2]-rect[0])*ppp,(rect[3]-rect[1])*ppp]):close(actual,f32(expected),1e-4,'paint viewport')
    camera=[v['center_mm'][0]-paint['scene_anchor']['x_mm'],v['center_mm'][1]-paint['scene_anchor']['y_mm'],v['scale']*ppp,1.5*1.0123/(v['scale']*ppp)]
    for actual,expected in zip(paint['uniform_camera'],camera):close(actual,f32(expected),1e-5,'paint camera/scale binding')
    require(paint['uniform_counts'][0]==paint['objects'],'paint scene/count binding')
    return f

def verify(directory, *, source_manifest, commit, binary_sha256, capture_producer_sha256, allow_capture_precheck=False):
    directory=Path(directory);r=load(directory/'observations.json');request=load(directory/'request.json');runner=load(directory/'runner.json');p=load(ROOT/'fixtures/synthetic/s5m2c/protocol.json')
    hashes=load(directory/'file-hashes.json');actual={f.relative_to(directory).as_posix() for f in directory.rglob('*') if f.is_file()}
    require(actual==set(hashes)|{'file-hashes.json'},'file inventory missing/extra')
    for name,digest in hashes.items():require(sha(safe(directory,name))==digest,'file content hash '+name)
    require(r['request']==request and r['stage']=='S5-M2-C' and r['schema_version']==2 and r['observation_version']==3 and r['profile']=='release','identity/schema')
    require(not r['failures'] and runner['exit_code']==0 and runner['error'] is None,'native/runner failure')
    require(r['binary_sha256']==load(directory/'binary-before.json')['sha256']==binary_sha256,'external binary binding')
    require(r['commit']==commit and r['build_source']==('git-dirty' if commit.endswith('-dirty') else 'git-clean'),'external commit/build source binding')
    require(r['source_manifest_sha256']==request['source_manifest_sha256']==source_manifest,'external source manifest binding')
    owned=load(directory/'owned-process.json');usage=load(directory/'owned-resource-usage.json')
    require(type(r['pid']) is int and r['pid']>0 and r['pid']==owned['pid']==usage['pid'],'owned process/resource PID binding')
    require(owned['binary_sha256']==binary_sha256 and usage==runner['resource_usage'] and usage['exit_code']==0 and usage['peak_rss_bytes']==runner['peak_child_rss_bytes'],'owned wait4 identity/RSS')
    require(usage['clock']=='owned os.wait4 child completion; macOS ru_maxrss bytes' and usage['elapsed_seconds']>0 and usage['user_cpu_seconds']>=0 and usage['system_cpu_seconds']>=0,'owned resource clock/CPU')
    mode=request['mode'];workflow=mode.startswith('workflow')
    expected_fixture=(sha(directory/'reopen-input.rcam') if mode=='workflow-reopen' else (load(ROOT/'fixtures/synthetic/s5m2c/workflow-freeze.json')['sha256'] if workflow else p['fixture']['sha256']))
    require(r['fixture_sha256']==request['fixture_sha256']==expected_fixture,'fixture binding')
    require(r['native_inputs_sha256']==request['native_inputs_sha256']==sha(ROOT/'fixtures/synthetic/s5m2c/native-inputs.json'),'native input protocol binding')
    require(r['protocol_sha256']==request['protocol_sha256']==sha(ROOT/'fixtures/synthetic/s5m2c/protocol.json'),'protocol binding')
    require(request['source_manifest_sha256']==runner['source_manifest_sha256']==sha(directory/'source-manifest.sha256'),'source manifest binding')
    before=load(directory/'display-before.json')['after'];display=load(directory/'display-active-probe.json')['after'];restored=load(directory/'display-restored-probe.json')['after']
    verify_display_environment(directory,request,DISPLAY_PROBE_SWIFT,DISPLAY_MUTATOR_SWIFT)
    require(restored==before,'native display restoration/preservation')
    if request['evidence_scope']=='capture-precheck-only':
        require(allow_capture_precheck and request['display_policy']=='preserve' and display==before,'capture-only display scope; formal performance cannot use preserved144Hz as frozen60Hz')
    else:
        require(request['evidence_scope']=='full-pmix-native' and request['display_policy']=='frozen-60hz' and request['display_mode_change_authorized'] is True and display['refresh_hz']==60,'native display Hz/explicit authorization')
    for key in ('width','height','pixel_width','pixel_height','backing_scale'):require(display[key]==before[key],'native display geometry')
    fs=r['frames'];require(fs and len(fs)==r['last_observed_frame_id'] and all(f['id']==i for i,f in enumerate(fs,1)),'frame coverage/IDs')
    roi_root=directory_path(directory,runner['ui_roi_directory'])
    require(load(roi_root/'identity.json')['pid']==r['pid'],'PMIX/UI ROI owned process binding')
    roi=analyze_roi(roi_root,binary_sha256,source_manifest)
    ui=roi_rows(roi_root/'frames.jsonl')
    all_frames=bind_frame_producers(r,ui,roi_rows(roi_root/'paint.jsonl'),
        roi_rows(roi_root/'requests.jsonl'),roi_rows(roi_root/'samples.jsonl'),
        load(safe(roi_root,'capture-finalization.json')))
    input_policy=verify_input_modifiers(r,all_frames,load(ROOT/'fixtures/synthetic/s5m2c/native-inputs.json'))
    require(roi['user_flicker_report']=='OPEN','unearned flicker closure')
    require(not roi['outliers'],'actual PMIX menu ROI outliers require investigation')
    indexed={f['id']:f for f in all_frames}
    for a,b in zip(all_frames,all_frames[1:]):
        require(a['input_ns']<b['input_ns'],'input clock monotonic')
        close(b['frame_interval_ms'],(b['input_ns']-a['input_ns'])/1e6,1e-4,'frame interval raw clock')
    for f in all_frames:
        task_version(f['state']['version'])
        require(f['input_ns']<=f['observed_ns'],'input/update causal clock')
        if f['painted']:draw_complete(f)
    events=r['events'];by={}
    for e in events:
        by.setdefault(e['label'],[]).append(e)
        f=indexed[e['frame_id']];require(f['input_ns']<=e['at_ns'],'event/input binding')
        if f['id']<len(all_frames):require(e['at_ns']<indexed[f['id']+1]['input_ns'],'event/frame end binding')
    surface_requests=[e['data'] for e in events if e['label']=='screenshot-request']
    surface_deliveries=[e['data']['request'] for e in events if e['label']=='screenshot']
    require(sorted(json.dumps(row,sort_keys=True) for row in surface_requests)
            ==sorted(json.dumps(row,sort_keys=True) for row in surface_deliveries),
            'every PMIX full-surface request delivered')
    def event(label):require(len(by.get(label,[]))==1,'missing/duplicate '+label);return by[label][0]
    if not workflow:
        base=event('baseline');warm=event('warmup-begin');require(base['at_ns']-warm['at_ns']>=10e9,'warmup <10s')
        baseline=base['data']['state'];require(baseline['scene_objects']==100000,'baseline workload')
    ss={}
    for record in r['snapshots']:
        label=record['label'];require(label not in ss,'duplicate snapshot');file=safe(directory,record['path']);require(sha(file)==record['sha256'],'snapshot content');ss[label]=load(file)
        require(record['count']==sum(len(l['objects']) for l in ss[label]['layers']),'snapshot count metadata')
        require(record['state']['revision']==ss[label]['revision'] and record['state']['document_id']==ss[label]['document_id'],'snapshot document/revision state')
        frame=indexed[record['frame_id']];require(frame['state']==record['state'] and frame['observed_ns']<=record['at_ns'],'snapshot/frame binding')
    if not workflow:manufacture(ss['before'])
    require(runner['peak_child_rss_bytes']>0 and runner['peak_child_rss_bytes']<=p['budgets']['rss_bytes'],'peak child RSS')
    require(0<r['counters']['custom-buffer-largest-observed-bytes']<=p['budgets']['custom_gpu_bytes'],'GPU buffer budget')
    require('Metal' in r['adapter'],'native Metal adapter')
    bootstrap_binding=bind_worker_requests(r,all_frames,safe(directory,'source-manifest.sha256'))
    for name in ('stdout.log','stderr.log','environment.json','window.json','window-query.json','image-command.json','native-window.png','video-command.json','native-window.mov','capture-ready.json','protocol-done.json','capture-complete.json'):
        safe(directory,name)
    capture=capture_receipts(directory,r,capture_producer_sha256)
    summary={'mode':request['mode'],'evidence_scope':request['evidence_scope'],'display_id':request['display_id'],'display_binding':'explicit-target-runner-schema4','raw_frames':len(fs),'raw_update_frames':len(all_frames),'terminal_frame_id':r['terminal_frame']['id'],'constructor_bootstrap':bootstrap_binding,'input_policy':input_policy,'rss_peak_bytes':runner['peak_child_rss_bytes'],'gpu_peak_bytes':r['counters']['custom-buffer-largest-observed-bytes'],'ui_roi':roi,'capture':capture,'user_flicker_report':'OPEN','stage_PASS_claim':False}
    if request['mode']=='nav':
        start=event('navigation-begin');end=event('navigation-end-input');done=event('navigation-complete');origin=start['data']['origin_ns'];a=start['frame_id'];b=end['frame_id']
        require(a<b<done['frame_id'],'navigation phase sequence')
        require([f['id'] for f in fs if f['phase']==20]==list(range(a+1,b)),'navigation raw phase coverage')
        require(indexed[b]['phase']==21 and indexed[b-1]['input_ns']-origin<60e9<=indexed[b]['input_ns']-origin,'navigation endpoint coverage')
        samples=[indexed[i] for i in range(a+1,b+1)];require(len(samples)>100,'navigation sample count')
        current_lat=[]
        for f in samples:
            state_equal(f['state'],baseline)
            require(f['state']['scene_objects']==100000,'navigation reduced display workload')
            v=f['injected']['view'];t=min((f['input_ns']-origin)/1e9,60.);target=[200.5+80*math.sin(math.tau*t/11),125.5+8*math.sin(math.tau*t/7)];scale=1.656*(1.05+.35*math.sin(math.tau*t/17))
            require(f['injected']['trajectory_origin_ns']==origin,'trajectory origin mismatch')
            require(len(f['injected']['zoom'])==len(f['injected']['wheel'])==1,'navigation raw inputs missing/duplicated')
            close(f['injected']['zoom'][0],f32((scale/v['scale'])**1.25),2e-7,'raw zoom trajectory')
            for axis in (0,1):close(f['injected']['wheel'][0][axis],f32((v['center_mm'][axis]-target[axis])*(1 if axis==0 else -1)*v['scale']),1e-4,'raw pan trajectory')
            rect=v['rect'];ppp=v['ppp'];require(not v['grid_snap'] and not v['object_snap'],'snap setting')
            for axis in (0,1):close((rect[axis+2]-rect[axis])*ppp,p['canvas_physical'][axis],.5,'canvas physical extent')
            processed=f['processed_navigation'];out=f['view'];zoom=processed['zoom'];scroll=processed['scroll']
            close(out['scale'],v['scale']*zoom**.8,1e-9,'processed camera zoom')
            close(out['center_mm'][0],v['center_mm'][0]-scroll[0]/v['scale'],1e-8,'processed camera pan X')
            close(out['center_mm'][1],v['center_mm'][1]+scroll[1]/v['scale'],1e-8,'processed camera pan Y')
            valid=next((g for g in fs[f['id']-1:] if g['painted'] and g.get('gpu_completed') and not g['state']['display_pending']),None)
            require(valid is not None,'input lacks current-view completion');visible_frame(valid);current_lat.append((valid['completed_ns']-f['input_ns'])/1e6)
        d=done['data'];visible=indexed[d['visible_frame_id']];visible_frame(visible)
        require(visible['id']>=b and visible['id']==done['frame_id']-1 and d['visible_completed_ns']==visible['completed_ns'],'endpoint completion binding')
        manufacture(ss['after-navigation']);require(ss['before']['layers']==ss['after-navigation']['layers'] and ss['before']['apertures']==ss['after-navigation']['apertures'],'navigation semantic mutation')
        times=[f['frame_interval_ms'] for f in samples]
        summary.update(frame_p95_ms=percentile(times,.95),frame_p99_ms=percentile(times,.99),frame_max_ms=max(times),input_gpu_p95_ms=percentile(current_lat,.95),input_gpu_max_ms=max(current_lat),navigation_frames=len(samples),pending_frames=sum(f['state']['display_pending'] for f in samples))
        for key in ['frame_p95_ms','frame_p99_ms','frame_max_ms','input_gpu_p95_ms']:require(summary[key]<=p['budgets'][key],f'{key}: {summary[key]} > {p["budgets"][key]}')
    elif request['mode'] in ('move','escape','new-project'):
        mode=request['mode'];p2={'frame_stall_ms_max':200,'drag_seconds':10,'interruption_drag_seconds':1,'camera':p['camera'],'drag_press_mm':[1.,1.],'canvas_physical':p['canvas_physical']}
        # The frozen M2-B parser consumes v2's complete frames list. Adapt only
        # its in-memory envelope after v3's mandatory terminal binding above;
        # never alter the raw producer evidence or the frozen M2-B contract.
        complete_envelope=dict(r,observation_version=2,frames=all_frames,last_observed_frame_id=len(all_frames))
        preview,times,indexed=bind_frames(complete_envelope,p2)
        require(baseline['selected']==1000,'mixed drag selection count')
        selected=next(x['selected_ids'] for x in r['snapshots'] if x['label']=='before')
        require(selected==[f'object-{n}' for n in p['drag']['ordinals']],'mixed drag ordered selected IDs')
        for f in preview:
            draw_complete(f)
            require(f['snapshot_identity_unchanged'] and f['scene_identity_unchanged'] and f['preview_index_identity_unchanged'],'preview mutated model/scene/index')
            for key in ['geometry-full-build-attempt','geometry-full-build-call','geometry-patch-attempt','scene-allocation','index-allocation','geometry-storage-init-upload','index-storage-init-upload','selection-upload']:
                require(f['counters'].get(key,0)==(preview[0]['counters'].get(key,0) if key=='selection-upload' else base['data']['counters'].get(key,0)),'preview rebuilt/uploaded geometry '+key)
        require(preview[0]['counters'].get('selection-upload',0)-base['data']['counters'].get('selection-upload',0) in (0,400000),'unexpected ProbeDrag selection upload')
        def completion(label,trigger,action):
            e=event(label);origin=event(trigger);d=e['data'];f=indexed[d['visible_frame_id']]
            visible_frame(f)
            require(f['id']==e['frame_id']-1 and f['input_ns']>=indexed[origin['frame_id']]['input_ns'],'completion callback/trigger')
            require(f['state']==d['state'] and f['view']==indexed[e['frame_id']]['view'],'completion scene/view state')
            require(d['visible_completed_ns']==f['completed_ns']<=indexed[e['frame_id']]['input_ns']<=e['at_ns'],'completion clock')
            close(d['duration_ms'],(e['at_ns']-indexed[origin['frame_id']]['input_ns'])/1e6,2,'completion duration clock')
            if action:
                workers=[w for w in r['worker'] if w['sequence']==d['worker_sequence']];require(len(workers)==1,'completion worker binding');w=workers[0]
                require(w['action']==action and indexed[origin['frame_id']]['input_ns']<=w['started_ns']<=w['finished_ns']<=f['observed_ns'],'worker lifetime/operation')
                for key in w['state']:require(w['state'][key]==f['state'][key],'worker/current frame state '+key)
            else:require(d['worker_sequence'] is None,'cancel mutation worker')
            return d
        def semantic_equal(a,b):
            for key in ('layers','apertures','block_definitions'):require(a[key]==b[key],'exact semantic restoration '+key)
        if mode=='move':
            summary.update(frame_p95_ms=percentile(times,.95),frame_p99_ms=percentile(times,.99),frame_max_ms=max(times),input_gpu_p95_ms=percentile([f['input_gpu_complete_ms'] for f in preview],.95))
            for key in ['frame_p95_ms','frame_p99_ms','frame_max_ms','input_gpu_p95_ms']:require(summary[key]<=p['budgets'][key],f'{key}: {summary[key]} > {p["budgets"][key]}')
            for label,trigger,action,step,u,z in [('commit-complete','release','drag-move',1,baseline['undo']+1,0),('undo-complete','undo-input','undo',2,baseline['undo'],1),('redo-complete','redo-input','redo',3,baseline['undo']+1,0)]:
                d=completion(label,trigger,action);st=d['state'];require(st['document_id']==baseline['document_id'] and int(st['revision'])==int(baseline['revision'])+step and st['undo']==u and st['redo']==z,'one transaction/revision history')
                visible=indexed[d['visible_frame_id']];ready=(visible['completed_ns']-indexed[event(trigger)['frame_id']]['input_ns'])/1e6
                require(ready<=300,label+' visible completion >300ms');summary[label+'_ms']=ready;summary[label+'_report_event_ms']=d['duration_ms']
            delta=event('release')['data']['delta_mm'];manufacture(ss['moved'],(delta['x_mm'],delta['y_mm']))
            semantic_equal(ss['before'],ss['undo']);semantic_equal(ss['moved'],ss['redo'])
            for i,(a,b) in enumerate(zip(ss['before']['layers'][0]['objects'],ss['moved']['layers'][0]['objects'])):
                c=copy.deepcopy(b)
                if i//400<50 and i%400<20:c['geometry']=a['geometry']
                require(c==a,'unselected or non-geometry field mutation')
            require(ss['before']['apertures']==ss['moved']['apertures'],'shared apertures changed')
            actions=[w['action'] for w in r['worker']];require(actions.count('drag-move')==actions.count('undo')==actions.count('redo')==1,'extra/missing transaction worker')
        else:
            d=completion('cancel-complete','release','new-project' if mode=='new-project' else None)
            require(not any(w['action']=='drag-move' for w in r['worker']),'cancel committed')
            if mode=='escape':semantic_equal(ss['before'],ss['cancelled']);state_equal(d['state'],baseline)
            else:require(d['state']['document_id']!=baseline['document_id'] and d['state']['selected']==d['state']['undo']==d['state']['redo']==d['state']['scene_objects']==0,'project replacement retained state')
    elif request['mode']=='points':
        inputs=by.get('point-input',[]);completions=by.get('point-complete',[])
        require(len(inputs)==len(completions)==200,'missing/duplicate point matrix')
        cpu=[];latency=[]
        for i,(e,done,witness) in enumerate(zip(inputs,completions,p['points'])):
            require(e['data']['index']==i and e['frame_id']<done['frame_id'],'point order')
            f=indexed[e['frame_id']];d=done['data'];v=indexed[d['visible_frame_id']];visible_frame(v)
            require(v['id']==done['frame_id']-1 and v['input_ns']>=f['input_ns'],'point callback/input binding')
            require(v['state']==d['state'] and d['visible_completed_ns']==v['completed_ns']<=done['at_ns'],'point completion binding')
            require(d['state']['primary']==witness['expected_id'] and d['state']['selected']==int(witness['expected_id'] is not None),'point manufacturing truth')
            for key in ('document_id','revision','workspace_revision','dirty','project_dirty','undo','redo'):require(d['state'][key]==baseline[key],'query mutated '+key)
            require(f['view']['scale']==20. and f['view']['center_mm']==witness['position_mm'],'point camera/world')
            rect=f['view']['rect'];require(f['injected']['pointer']==[(rect[0]+rect[2])/2,(rect[1]+rect[3])/2] and f['injected']['pressed'] and f['injected']['released'],'point raw click')
            workers=[w for w in r['worker'] if w['sequence']==d['worker_sequence']];require(len(workers)==1,'point worker missing');w=workers[0]
            require(w['action']=='select' and f['input_ns']<=w['started_ns']<=w['finished_ns']<=v['observed_ns'],'point worker lifetime')
            hits=[h for h in r['hits'] if w['started_ns']<=h['start_ns']<=h['end_ns']<=w['finished_ns']];require(len(hits)==1,'CPU hit sample missing/duplicate')
            h=hits[0];point(h['point'],*witness['position_mm']);close(h['tolerance_mm'],6/(20*f['view']['ppp']),1e-12,'point hit tolerance')
            cpu.append((h['end_ns']-h['start_ns'])/1e6)
            derived=(done['at_ns']-f['input_ns'])/1e6;close(d['duration_ms'],derived,2,'point duration clock');latency.append((v['completed_ns']-f['input_ns'])/1e6)
        done=event('box-complete');release=event('box-release');press=event('box-press');d=done['data'];f=indexed[d['visible_frame_id']];visible_frame(f)
        require(press['frame_id']<release['frame_id']<=f['id']==done['frame_id']-1,'box operation/callback order')
        require(d['state']==f['state'] and d['state']['selected']==100000,'box complete/full count')
        workers=[w for w in r['worker'] if w['sequence']==d['worker_sequence']];require(len(workers)==1 and workers[0]['action']=='select-rect','box is not real marquee query')
        require(indexed[release['frame_id']]['input_ns']<=workers[0]['started_ns']<=workers[0]['finished_ns']<=f['observed_ns'],'box worker clock')
        selected=next(x['selected_ids'] for x in r['snapshots'] if x['label']=='full-box');require(selected==[f'object-{i}' for i in range(1,100001)],'box ordered IDs')
        manufacture(ss['full-box']);require(ss['before']['layers']==ss['full-box']['layers'],'point/box mutation')
        reported=(done['at_ns']-indexed[release['frame_id']]['input_ns'])/1e6;close(d['duration_ms'],reported,2,'box duration clock')
        box=(f['completed_ns']-indexed[release['frame_id']]['input_ns'])/1e6
        summary.update(box_report_event_ms=reported,box_worker_ready_ms=(workers[0]['finished_ns']-indexed[release['frame_id']]['input_ns'])/1e6)
        summary.update(point_cpu_p95_ms=percentile(cpu,.95),point_highlight_p95_ms=percentile(latency,.95),box_ready_ms=box)
        for key in ['point_cpu_p95_ms','point_highlight_p95_ms','box_ready_ms']:require(summary[key]<=p['budgets'][key],f'{key}: {summary[key]} > {p["budgets"][key]}')
    elif mode=='workflow':summary.update(verify_workflow(ROOT,directory,r,ss,visible_frame))
    elif mode=='workflow-cross-layer':summary.update(verify_cross_layer_workflow(ROOT,directory,r,ss,visible_frame))
    elif mode=='workflow-reopen':
        require([x['label'] for x in r['snapshots']]==['workflow-opened'],'fresh reopen snapshots')
        e=event('workflow-opened');f=indexed[e['data']['visible_frame_id']];visible_frame(f)
        require(f['id']==e['frame_id']-1 and f['state']==e['data']['state'],'fresh reopen visible completion')
        snapshot_project(ss['workflow-opened'],decode_project(directory/'reopen-input.rcam'))
        require(f['state']['undo']==f['state']['redo']==f['state']['selected']==0 and not f['state']['project_dirty'],'fresh reopen retained transient state')
    else:raise ValueError('unsupported mode')
    return summary
if __name__=='__main__':
    parser=argparse.ArgumentParser(description=__doc__)
    parser.add_argument('directory',type=Path)
    parser.add_argument('--source-manifest',required=True)
    parser.add_argument('--commit',required=True)
    parser.add_argument('--binary-sha256',required=True)
    parser.add_argument('--capture-producer-sha256',required=True)
    parser.add_argument('--allow-capture-precheck',action='store_true',help='capture-only preserved-display evidence; never a frozen PMIX performance result')
    args=parser.parse_args()
    try:print(json.dumps(verify(args.directory,source_manifest=args.source_manifest,commit=args.commit,binary_sha256=args.binary_sha256,capture_producer_sha256=args.capture_producer_sha256,allow_capture_precheck=args.allow_capture_precheck),indent=2))
    except (OSError,ValueError,KeyError,TypeError,StopIteration) as e:print('FAIL:',e);raise SystemExit(1)
