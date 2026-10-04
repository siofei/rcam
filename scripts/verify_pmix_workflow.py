"""Independent .rcam DTO and rigid-transform assertions for the fixed PMIX workflow."""
import copy,hashlib,json,math,zipfile
from pathlib import Path
from verify_pmix_export import verify_export

def require(ok,why):
    if not ok:raise ValueError(why)
def parse_json(raw):
    def pairs(items):
        result={}
        for key,value in items:
            require(key not in result,'duplicate JSON key '+key);result[key]=value
        return result
    value=json.loads(raw,object_pairs_hook=pairs,parse_constant=lambda token:(_ for _ in ()).throw(ValueError('invalid JSON constant '+token)))
    pending=[(value,1)]
    while pending:
        item,depth=pending.pop();require(depth<128,'JSON nesting budget')
        if isinstance(item,float):require(math.isfinite(item),'nonfinite JSON number')
        elif type(item) is dict:pending.extend((v,depth+1) for v in item.values())
        elif type(item) is list:pending.extend((v,depth+1) for v in item)
    return value
def decode_project(path):
    with zipfile.ZipFile(path) as z:
        names=z.namelist();require(len(names)==len(set(names)) and len(names)<=100,'project ZIP names/count')
        require(all(not n.startswith('/') and '..' not in n.split('/') and '\\' not in n for n in names),'project ZIP path')
        require(sum(i.file_size for i in z.infolist())<16*1024*1024,'workflow ZIP size')
        values={n:parse_json(z.read(n)) for n in names}
        m=values['manifest.json'];require(m['format']=='rcam' and m['format_version']==1,'project format')
        require({e['path'] for e in m['entries']}==set(names)-{'manifest.json'},'project manifest coverage')
        for e in m['entries']:
            b=z.read(e['path']);require(len(b)==e['uncompressed_size'] and hashlib.sha256(b).hexdigest()==e['sha256'],'project entry integrity')
        return values

def manufacturing(snapshot):return {k:snapshot[k] for k in ('layers','apertures','block_definitions')}
def snapshot_project(snapshot,entries):
    project=entries['project.json'];require(snapshot['layers']==[entries['layers/'+n+'.json']['layer'] for n in reversed(project['layer_order'])],'project stored layers/order vs visible manufacturing')
    require(snapshot['apertures']==project['apertures'],'project apertures')
    require(snapshot['block_definitions']==[entries['blocks/'+n+'.json'] for n in project['block_definition_ids']],'project definitions')
    require([s['layer_id'] for s in snapshot['styles']]==list(reversed(project['layer_order'])),'project style layer order')
    for style in snapshot['styles']:
        workspace=entries['layers/'+style['layer_id']+'.json']['workspace'];stored=workspace['style']
        for key in ('visible','selectable','locked'):require(style[key]==workspace[key],'saved layer '+key)
        for key in ('base_color','color_mode','display_mode'):require(style[key]==stored[key],'saved style '+key)
        require({c['class'] for c in style['classes']}==set(stored['classes']),'saved class coverage')
        for category in style['classes']:
            for key in ('visible','selectable','locked','color_override'):require(category[key]==stored['classes'][category['class']][key],'saved category '+key)

def moved_expected(before,after,rotate=False):
    require(before['apertures']==after['apertures'] and before['block_definitions']==after['block_definitions'],'shared definition mutation')
    count=0
    for a,b in zip(before['layers'],after['layers']):
        require(a['id']==b['id'] and len(a['objects'])==len(b['objects']),'layer/object order/length')
        for x,y in zip(a['objects'],b['objects']):
            expected=copy.deepcopy(x)
            if 'BlockInstance' in x['geometry']:
                count+=1;t=expected['geometry']['BlockInstance']['transform'];observed=y['geometry']['BlockInstance']['transform'];q=t['translation']
                if rotate:
                    radians=math.radians(37);nx=math.cos(radians)*q['x_mm']-math.sin(radians)*q['y_mm'];ny=math.sin(radians)*q['x_mm']+math.cos(radians)*q['y_mm'];angle=(t['rotation_deg']+37)%360
                else:nx=q['x_mm']+2;ny=q['y_mm']-1;angle=t['rotation_deg']
                require(abs(observed['translation']['x_mm']-nx)<1e-9 and abs(observed['translation']['y_mm']-ny)<1e-9,'independent instance transform')
                require(abs(observed['rotation_deg']-angle)<1e-9 and observed['mirror']==t['mirror'],'instance rotation/mirror')
                expected['geometry']['BlockInstance']['transform']=observed
            require(expected==y,'unselected/non-transform field mutation')
    require(count==4,'fixed four block instances')

LABELS=['select-blocks','move','rotate','undo','redo','hide-all','show-all','solo','clear-solo','reselect-blocks','lock','locked-move','unlock','fit-layer','window-all','crossing-all','select-text-group','project-save','project-reopen','export-base','export-upper']
ACTIONS=['block-select','move','rotate','undo','redo','visibility','visibility','solo','solo','block-select','layer-update','move','layer-update','fit-layer','select-rect','select-rect','select','project-save','project-open','export','export']
COUNTS=[4,4,4,4,4,0,0,0,0,4,4,4,4,4,51,51,40,40,0,0,0]

def verify_workflow(root,directory,r,ss,visible_frame):
    indexed={f['id']:f for f in r['frames']};inputs=[e for e in r['events'] if e['label']=='workflow-input'];completions=[e for e in r['events'] if e['label']=='workflow-complete']
    require([e['data']['label'] for e in inputs]==LABELS and [e['data']['label'] for e in completions]==LABELS,'workflow matrix missing/reordered')
    require([e['data']['step'] for e in inputs]==list(range(21)) and [e['data']['step'] for e in completions]==list(range(21)),'workflow step IDs')
    snapshot_project(ss['workflow-opened'],decode_project(root/'fixtures/synthetic/s5m2c/MIX_WORKFLOW.rcam'))
    for i,(start,end,action,count) in enumerate(zip(inputs,completions,ACTIONS,COUNTS)):
        require(start['frame_id']<end['frame_id'] and (i==0 or completions[i-1]['at_ns']<start['at_ns']),'workflow operation order')
        d=end['data'];f=indexed[d['visible_frame_id']];visible_frame(f)
        require(f['id']==end['frame_id']-1 and f['state']==d['state'] and f['input_ns']>=start['at_ns'],'workflow callback/result binding')
        require(d['visible_completed_ns']==f['completed_ns']<=indexed[end['frame_id']]['input_ns']<=end['at_ns'],'workflow completion clock')
        require(abs(d['duration_ms']-(end['at_ns']-start['at_ns'])/1e6)<2,'workflow duration origin')
        workers=[w for w in r['worker'] if w['sequence']==d['worker_sequence']];require(len(workers)==1,'workflow worker binding');w=workers[0]
        require(w['action']==action and start['at_ns']<=w['started_ns']<=w['finished_ns']<=f['observed_ns'],'workflow worker/action/lifetime')
        require(w['error']==d['error'],'workflow error binding')
        if i==11:require(d['error']['code']=='LAYER_LOCKED','locked edit did not refuse')
        else:require(d['error'] is None,'unexpected workflow error')
        for key in w['state']:require(w['state'][key]==d['state'][key],'workflow worker state '+key)
        require(d['state']['selected']==count,'workflow selection/count')
        snap=ss[f'workflow-step-{i:02}'];require(snap['revision']==d['state']['revision'] and snap['document_id']==d['state']['document_id'],'workflow semantic snapshot binding')
        if i>=5 and i<=17:
            require(d['state']['revision']=='4' and d['state']['undo']==2 and d['state']['redo']==0,'view-only/failed operation changed revision/history')
    moved_expected(ss['workflow-step-00'],ss['workflow-step-01'])
    moved_expected(ss['workflow-step-01'],ss['workflow-step-02'],True)
    require(manufacturing(ss['workflow-step-03'])==manufacturing(ss['workflow-step-01']),'workflow Undo exact')
    require(manufacturing(ss['workflow-step-04'])==manufacturing(ss['workflow-step-02']),'workflow Redo exact')
    for i in range(5,21):require(manufacturing(ss[f'workflow-step-{i:02}'])==manufacturing(ss['workflow-step-04']),'workflow view/save/reopen/export mutation')
    require(ss['workflow-step-10']==ss['workflow-step-11'],'locked edit partial mutation')
    require(manufacturing(ss['workflow-navigation'])==manufacturing(ss['workflow-step-13']),'workflow navigation mutation')
    require(ss['workflow-step-17']['styles']==ss['workflow-step-18']['styles'],'project styles lost on reopen')
    saved=decode_project(directory/'workflow-output.rcam');snapshot_project(ss['workflow-step-18'],saved)
    original=decode_project(root/'fixtures/synthetic/s5m2c/MIX_WORKFLOW.rcam')
    require(saved['project.json']['manufacturing']==original['project.json']['manufacturing'],'project precision changed')
    require(completions[17]['data']['state']['project_dirty'] is False and completions[18]['data']['state']['undo']==0,'project save/reopen history baseline')
    exports=[verify_export(directory/f'workflow-{layer}.gbr',ss['workflow-step-18'],layer,saved['project.json']['manufacturing']['precision']['resolution_mm']) for layer in saved['project.json']['layer_order']]
    return {'mode':'workflow','steps':len(inputs),'outputs':['workflow-output.rcam','workflow-layer-1.gbr','workflow-layer-2.gbr'],'independent_export_geometry':exports}

CROSS_LABELS=['cross-layer-select','cross-layer-move','cross-layer-undo','cross-layer-redo','cross-layer-lock','cross-layer-locked-move','cross-layer-unlock','cross-layer-save','cross-layer-reopen','cross-layer-export-base','cross-layer-export-upper']
CROSS_ACTIONS=['select-rect','move','undo','redo','layer-update','move','layer-update','project-save','project-open','export','export']

def translated_geometry(geometry,dx,dy):
    expected=copy.deepcopy(geometry)
    def shift(p):p['x_mm']+=dx;p['y_mm']+=dy
    if 'Flash' in expected:shift(expected['Flash']['center'])
    elif 'Line' in expected:
        shift(expected['Line']['start']);shift(expected['Line']['end'])
    elif 'Arc' in expected:
        arc=expected['Arc']['path']
        for key in ('start','end','center'):shift(arc[key])
    elif 'Region' in expected:
        for contour in expected['Region']['contours']:
            for edge in contour['edges']:
                require(list(edge)==['Line'],'frozen workflow Region edge type')
                shift(edge['Line']['start']);shift(edge['Line']['end'])
    elif 'BlockInstance' in expected:shift(expected['BlockInstance']['transform']['translation'])
    else:raise ValueError('unexpected frozen cross-layer geometry')
    return expected

def verify_cross_layer_workflow(root,directory,r,ss,visible_frame):
    indexed={f['id']:f for f in r['frames']}
    inputs=[e for e in r['events'] if e['label']=='workflow-input']
    completions=[e for e in r['events'] if e['label']=='workflow-complete']
    require([e['data']['label'] for e in inputs]==[e['data']['label'] for e in completions]==CROSS_LABELS,'cross-layer complete/reordered matrix')
    require([e['data']['step'] for e in inputs]==[e['data']['step'] for e in completions]==list(range(11)),'cross-layer step IDs')
    original=ss['workflow-opened'];snapshot_project(original,decode_project(root/'fixtures/synthetic/s5m2c/MIX_WORKFLOW.rcam'))
    require(len(original['layers'])==2 and sum(len(l['objects']) for l in original['layers'])==51,'cross-layer fixed project count')
    for i,(start,end,action) in enumerate(zip(inputs,completions,CROSS_ACTIONS)):
        require(start['frame_id']<end['frame_id'] and (i==0 or completions[i-1]['at_ns']<start['at_ns']),'cross-layer operation order')
        d=end['data'];frame=indexed[d['visible_frame_id']];visible_frame(frame)
        require(frame['id']==end['frame_id']-1 and frame['state']==d['state'] and frame['input_ns']>=start['at_ns'],'cross-layer actual current result/callback')
        require(d['visible_completed_ns']==frame['completed_ns']<=indexed[end['frame_id']]['input_ns']<=end['at_ns'],'cross-layer GPU completion clock')
        require(abs(d['duration_ms']-(end['at_ns']-start['at_ns'])/1e6)<2,'cross-layer duration clock')
        workers=[w for w in r['worker'] if w['sequence']==d['worker_sequence']];require(len(workers)==1,'cross-layer accepted worker')
        worker=workers[0];require(worker['action']==action and start['at_ns']<=worker['started_ns']<=worker['finished_ns']<=frame['observed_ns'],'cross-layer worker/action/lifetime')
        require(worker['error']==d['error'],'cross-layer error producer binding')
        if i==5:require(d['error']['code']=='LAYER_LOCKED','cross-layer lock refusal')
        else:require(d['error'] is None,'unexpected cross-layer error')
        for key,value in worker['state'].items():require(value==d['state'][key],'cross-layer worker result state '+key)
        require(d['state']['selected']==(51 if i<8 else 0),'cross-layer complete text/block selection')
        expected=[('0',0,0),('1',1,0),('2',0,1),('3',1,0),('3',1,0),('3',1,0),('3',1,0),('3',1,0),('0',0,0),('0',0,0),('0',0,0)][i]
        require((d['state']['revision'],d['state']['undo'],d['state']['redo'])==expected,'cross-layer one transaction/history or lock changed history')
    selected=next(s for s in r['snapshots'] if s['label']=='workflow-step-00')['selected_ids']
    frozen_order=decode_project(root/'fixtures/synthetic/s5m2c/MIX_WORKFLOW.rcam')['project.json']['layer_order']
    layers={layer['id']:layer for layer in original['layers']}
    require(selected==[o['object_id'] for layer_id in frozen_order for o in layers[layer_id]['objects']],'cross-layer ordered full selected ID set in frozen panel/storage order')
    before=ss['workflow-step-00'];moved=ss['workflow-step-01']
    require(before['apertures']==moved['apertures'] and before['block_definitions']==moved['block_definitions'],'cross-layer shared definition mutation')
    require(len(before['layers'])==len(moved['layers'])==2,'cross-layer layer count')
    for layer_a,layer_b in zip(before['layers'],moved['layers']):
        require(layer_a['id']==layer_b['id'] and len(layer_a['objects'])==len(layer_b['objects']),'cross-layer layer/order/length')
        for a,b in zip(layer_a['objects'],layer_b['objects']):
            expected=copy.deepcopy(a);expected['geometry']=translated_geometry(a['geometry'],1.25,-.75)
            require(expected==b,'independent cross-layer f64 geometry or non-geometry mutation')
    require(manufacturing(ss['workflow-step-02'])==manufacturing(before),'cross-layer exact Undo')
    require(manufacturing(ss['workflow-step-03'])==manufacturing(moved),'cross-layer exact Redo')
    require(ss['workflow-step-04']==ss['workflow-step-05'],'cross-layer locked edit partial commit')
    for i in range(4,11):require(manufacturing(ss[f'workflow-step-{i:02}'])==manufacturing(moved),'cross-layer view/save/reopen/export manufacturing mutation')
    saved=decode_project(directory/'workflow-cross-layer.rcam');snapshot_project(ss['workflow-step-08'],saved)
    require(ss['workflow-step-07']['styles']==ss['workflow-step-08']['styles'],'cross-layer project style persistence')
    require(saved['project.json']['manufacturing']==decode_project(root/'fixtures/synthetic/s5m2c/MIX_WORKFLOW.rcam')['project.json']['manufacturing'],'cross-layer manufacturing precision changed')
    exports=[verify_export(directory/name,ss['workflow-step-08'],layer,saved['project.json']['manufacturing']['precision']['resolution_mm']) for layer,name in [('layer-1','cross-layer-base.gbr'),('layer-2','cross-layer-upper.gbr')]]
    return {'mode':'workflow-cross-layer','steps':len(inputs),'selected_objects':51,'layers':2,'outputs':['workflow-cross-layer.rcam','cross-layer-base.gbr','cross-layer-upper.gbr'],'independent_export_geometry':exports}
