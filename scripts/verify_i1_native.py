"""Portable, fail-closed S5-I1 native truth/history/frame verifier."""
import argparse,hashlib,json,math
from pathlib import Path
from run_i1_native import steps

def require(ok,message):
    if not ok:raise ValueError(message)
def read(p):return json.loads(p.read_text())
def sha(p):return hashlib.sha256(p.read_bytes()).hexdigest()
def content(snapshot):return {key:snapshot[key] for key in ('layers','apertures','block_definitions')}
def objects(snapshot):return {(l['id'],o['object_id']):o for l in snapshot['layers'] for o in l['objects']}
def center(o):return o['geometry']['Flash']['center'] if 'Flash' in o['geometry'] else o['geometry']['center']
def selected(state):return [(o['layer_id'],o['object']['object_id']) for o in state['selected']]
def close(a,b,label):require(abs(a-b)<=1e-8,label+f': {a} != {b}')
def near_point(actual, wanted, label):
    require(isinstance(actual,list) and len(actual)==2 and all(isinstance(v,(int,float)) and math.isfinite(v) for v in actual),label+' invalid point')
    require(math.dist(actual,wanted)<=0.0002,label+' coordinate mismatch')
def screen(state,world):
    x,y,k=state['camera'];a,b,c,d=state['canvas']
    return [(a+c)/2+(world[0]-x)*k,(b+d)/2-(world[1]-y)*k]
def verify_button_context(frame,event,world,modifiers):
    state=frame['state'];point=screen(state,world)
    require(event['button']=='Primary' and event['modifiers']==modifiers and frame['input']['modifiers']==modifiers,'wrong pointer button/modifiers')
    near_point(event['position'],point,'button world/screen');near_point(frame['input']['position'],point,'pointer state')
    a,b,c,d=state['canvas'];require(a<=point[0]<=c and b<=point[1]<=d,'pointer outside canvas')
    require(frame['input']['focused'] and frame['input']['down']==event['pressed'] and frame['input']['released']==(not event['pressed']),'pointer state flags')
    return point
def verify_gesture_focus(frame,allow_gone=False):
    require(frame['input']['focused'],'unfocused gesture')
    require(not any(e['kind']=='focus' and not e['focused'] or e['kind']=='gone' and not allow_gone for e in frame['input']['events']),'gesture input cancelled')
def verify_select_context(detail,state,world,mode):
    require(detail['mode']==mode,'action mode contradicts input')
    near_point(detail['world'],world,'select world');near_point(detail['point'],screen(state,world),'select screen')
    require(detail['camera']==state['camera'] and detail['canvas']==state['canvas'] and detail['ppp']==state['ppp'],'select context changed')
    require(detail['navigation_epoch']==state['navigation_epoch'],'select navigation context changed')
def verify_probe_context(detail,state,world):
    near_point(detail['world'],world,'probe world')
    close(detail['tolerance'],6/(state['camera'][2]*state['ppp']),'probe tolerance')
def verify_capture(report,ledger):
    frames=report['frames'];ids=[f['frame_id'] for f in frames]
    require(ledger['run_id']==report['request']['run_id'],'ledger run identity')
    require(ledger['capture_start']==1 and ledger['capture_end']==ledger['terminal_pending_frame']-1,'capture interval')
    require(ids==list(range(1,ledger['capture_end']+1)),'incomplete continuous frame capture')
    full=list(range(1,ledger['terminal_pending_frame']+1))
    require(ledger['input_ids']==full and ledger['update_ids']==full,'raw input/UI update coverage')
    painted=[f['frame_id'] for f in frames if f['painted']]
    require(ledger['paint_ids']==painted and ledger['paint_count']==len(painted),'independent paint coverage')
    require(all(f['pass_index']==0 for f in frames),'unrecorded additional UI pass')
    require(all(f['completed_ns']>=f['input_ns'] for f in frames),'frame clock reversal')
    require(all(a['completed_ns']<=b['input_ns'] for a,b in zip(frames,frames[1:])),'capture clock order')
    require(all(f['gpu_completed']==f['painted'] for f in frames),'uncompleted painted frame')
    actions=ledger['actions'];require([a['frame_id'] for a in actions]==sorted(a['frame_id'] for a in actions),'action frame order')
    startup=[a for a in actions if a['frame_id']==0]
    require(startup==[{'frame_id':0,'sequence':1,'detail':{'kind':'other','name':'NewWorkspace'}}],'startup action contract')
    require(all(a['frame_id'] in ids or a in startup for a in actions),'action outside capture')
    seq=[a['sequence'] for a in actions];require(seq==sorted(set(seq)),'action sequence order')
    prior=1
    by_frame={}
    for a in actions:
        by_frame.setdefault(a['frame_id'],[]).append(a['sequence'])
    for f in frames:
        require(f['input_before']['sequence']==prior,'input/action sequence discontinuity')
        row=by_frame.get(f['frame_id'],[])
        require(row==list(range(prior+1,prior+1+len(row))),'action sequence skipped within frame')
        prior+=len(row);require(f['sequence']==prior,'UI/action sequence mismatch')


def verify_pointer(expected,step_frames,ledger,completed):
    kind=expected['kind'];events=[(f,e) for f in step_frames for e in f['input']['events']]
    buttons=[(f,e) for f,e in events if e['kind']=='button']
    require(len(buttons)==2 and [e['pressed'] for _,e in buttons]==[True,False],'missing/duplicate native pointer input')
    press,release=buttons[0][0],buttons[1][0]
    require(press['frame_id']<release['frame_id'],'pointer order')
    require(completed is not None and completed['frame_id']>release['frame_id'],'completion predates release')
    mode='Remove' if expected.get('shift') else 'Add' if expected.get('ctrl') else 'Replace'
    wanted_mod={k:bool(expected.get(k)) for k in ('shift','ctrl','alt','mac_cmd','command')}
    for f,e in buttons:
        world=expected['from'] if e['pressed'] or kind=='click' else expected['to']
        verify_button_context(f,e,world,wanted_mod)
    if expected.get('shift') or expected.get('ctrl'):
        require(press['gesture_after'] is not None and not press['gesture_after']['confirmed'],'modifier press did not arm pending gesture')
        require(release['input_before']['gesture'] is not None and release['input_before']['gesture']['confirmed'],'modifier release lost confirmed gesture')
    for f in step_frames:
        if press['frame_id']<=f['frame_id']<=release['frame_id']:
            verify_gesture_focus(f)
            for e in f['input']['events']:
                if e['kind']=='move':
                    world=expected['from'] if kind=='click' or f['frame_id']==press['frame_id'] else expected['to']
                    near_point(e['position'],screen(f['state'],world),'pointer move')
            if kind=='click':near_point(f['input']['position'],screen(f['state'],expected['from']),'held click')
        for g in (f.get('gesture_after'),f.get('input_before',{}).get('gesture')):
            if not g:continue
            require(g['mode']==mode,'gesture mode contradicts input')
            near_point(g['start'],screen(press['state'],expected['from']),'gesture start')
            if kind=='click':
                near_point(g['last'],g['start'],'click gesture last')
                require(not g['moved'],'click crossed drag threshold')
        if expected.get('shift') or expected.get('ctrl'):require(not f['grip_after'] and not f['input_before']['grip'],'modifier entered Grip')
    actions=[a for a in ledger['actions'] if press['frame_id']<=a['frame_id']<=completed['frame_id']]
    probes=[a for a in actions if a['detail']['kind']=='probe']
    require(len(probes)<=1,'duplicate probe')
    for a in probes:verify_probe_context(a['detail'],press['state'],expected['from'])
    choices=[a for a in actions if a['detail']['kind']=='select']
    if kind=='click':
        require(len(choices)==1,'click did not emit exactly one selection action')
        a=choices[0];d=a['detail'];require(release['frame_id']<=a['frame_id']<=completed['frame_id'],'select action timing')
        verify_select_context(d,press['state'],expected['from'],mode)
    else:
        require(not choices,'drag emitted click')
        require(math.dist(screen(press['state'],expected['from']),screen(press['state'],expected['to']))*press['state']['ppp']>=4,'drag below threshold')
        transforms=[a for a in actions if a['detail']['kind'] in ('move','box')]
        require(len(transforms)==(0 if expected.get('escape') else 1),'drag/cancel action count')
        if expected.get('escape'):require(any(e['kind']=='key' and e['key']=='Escape' and e['pressed'] for _,e in events),'missing Escape event')
        for a in transforms:
            require(a['frame_id']>=release['frame_id'],'drag committed before release')
            d=a['detail']
            if d['kind']=='move':near_point(d['delta'],[b-a for a,b in zip(expected['from'],expected['to'])],'drag delta')
            else:require(d['bounds']==[-3.,-3.,9.,9.],'box world bounds')

def verify(directory,source_root=None,binary=None):
    directory=Path(directory);report=read(directory/'observations.json');request=read(directory/'request.json');runner=read(directory/'runner.json')
    require(report['schema_version']==2 and report['stage']=='S5-I1' and report['profile']=='release','identity')
    require(report['error'] is None and runner['error'] is None and runner['exit_code']==0,'native process failed')
    require(report['request']==request and type(request.get('stress')) is bool and request['steps']==steps(request['stress']) and request.get('diagnostic') is None,'fixed input protocol')
    require(set(request['fixtures'])=={'lower.gbr','upper.gbr'},'unexpected fixture path')
    require(report['binary_sha256']==request['binary_sha256']==runner['binary_sha256'],'binary binding')
    if binary:require(sha(Path(binary))==report['binary_sha256'],'external binary hash')
    if source_root:
        source_root=Path(source_root);require(sha(source_root/'MANIFEST.sha256')==request['source_manifest_sha256'],'source manifest binding')
        for name,digest in request['fixtures'].items():require(sha(source_root/'fixtures/synthetic/s5i1'/name)==digest,'fixture hash')
    require('metal' in json.dumps(report['adapter']).lower(),'native Metal required')
    require(len(report['records'])==len(steps(request['stress'])),'missing step records')
    video=read(directory/'video-command.json');require(video['exit_code']==0,'video did not finish successfully')
    require(len(read(directory/'process-samples.json'))>=10,'missing process sampling')
    ledger=read(directory/'capture-ledger.json');verify_capture(report,ledger)
    frames={f['frame_id']:f for f in report['frames']};require(len(frames)==len(report['frames']),'duplicate frame id')
    require(list(frames)==sorted(frames),'frame order')
    previous=None;undo=[];redo=[];cycle=None;verified=[];all_original=set();baseline=None
    for index,(expected,record) in enumerate(zip(steps(request['stress']),report['records'])):
        require(record['step']==index and record['path']==f'step-{index:02}.json','step path/order')
        item=read(directory/record['path']);state=item['state'];snapshot=item['snapshot'];kind=expected['kind'];require(item['input']==expected,'step input mismatch')
        require(not state['busy'] and not state['display_pending'],'unfinished UI state')
        step_frames=[f for f in frames.values() if f['step']==index]
        if kind in ('click','drag'):
            verify_pointer(expected,step_frames,ledger,item['completed_frame'])
        if request['stress'] and index==112:
            press=next(f for f in step_frames if any(e['kind']=='button' and e['pressed'] for e in f['input']['events']))
            require(press['input_before'].get('recovery_collision_due') is True,'missing forced recovery/press collision witness')
            accepted=[a['detail'] for a in ledger['actions'] if a['frame_id']==press['frame_id']]
            require([a['kind'] for a in accepted]==['probe'] and press['gesture_after'] is not None,'recovery consumed canvas press')
        error=state['error'];require((error or {}).get('code')==expected.get('expected_error'),f'unexpected error at step {index}')
        if state['layers']:
            frame=item['completed_frame'];require(frame is not None and frames.get(frame['frame_id'])==frame,'frame not bound to raw record')
            require(frame['state']==state and frame['gpu_completed'] and frame['painted'],'wrong displayed state')
            require(frame['step']==index and frame['phase']==4,'pre-operation frame used')
            require(frame['completed_ns']>=frame['input_ns'],'frame clock reversal')
        image=directory/f'step-{index:02}.ppm';data=image.read_bytes();require(data.startswith(b'P6\n') and len(data)>10000,'missing native surface screenshot')
        if kind in ('import','reimport_exports'):
            mapping={l['layer_id']:l['display_name'] for l in state['layers']};require(len(mapping)==2,'two actual layers')
            require(len(snapshot['layers'])==2 and sum(len(l['objects']) for l in snapshot['layers'])==4,'four imported objects')
            for layer in snapshot['layers']:
                name=mapping[layer['id']];points=[(center(o)['x_mm'],center(o)['y_mm']) for o in layer['objects']]
                require(points==([(0.,0.),(6.,0.)] if 'lower' in name else [(0.,0.),(0.,6.)]),'independent fixture coordinates/order')
                require(all(o['exposure']=='Dark' or o['exposure']=='dark' for o in layer['objects']),'fixture exposure')
            all_original=set(objects(snapshot));require(len(all_original)==4,'unique imported IDs')
            apertures={a['id']:a['shape'] for a in snapshot['apertures']}
            for o in objects(snapshot).values():require(apertures[o['geometry']['Flash']['aperture_id']]=={'Circle':{'diameter_mm':2.0,'hole_diameter_mm':None}},'independent aperture shape')
            if kind=='import':baseline=content(snapshot)
            else:
                require(state['info']['document_id']==previous['state']['info']['document_id'] and not previous['state']['layers'],'reimport did not use new empty workspace')
                for l in state['layers']:
                    name=l['provenance']['original_file_name'];require(name in ('lower.gbr','upper.gbr'),'wrong exported input name')
                    require(l['provenance']['imported_sha256']==sha(directory/name),'reimport did not read exported bytes')
                baseline=content(snapshot)
        elif kind=='new':
            require(not state['layers'] and not selected(state) and state['info']['undo_entries']==0,'new workspace not empty')
            if previous:
                require(state['info']['document_id']!=previous['state']['info']['document_id'],'new workspace retained old document')
                require(content(previous['snapshot'])==baseline,'project before Gerber reimport differs from exact history')
            cycle=None
        elif previous is not None and previous['snapshot'] is not None and snapshot is not None:
            old=previous['snapshot'];old_state=previous['state'];old_objects=objects(old);new_objects=objects(snapshot);old_selected=selected(old_state);old_content=content(old)
            revision=int(state['info']['revision']);old_revision=int(old_state['info']['revision'])
            mutation=False
            if expected.get('expected_error'):
                require(content(snapshot)==old_content and state['info']==old_state['info'],'failed operation mutated document/history')
                require(selected(state)==old_selected,'failed operation changed selection')
            elif kind in ('move','rotate','mirror') or (kind=='drag' and old_selected and not expected.get('escape') and expected['from']==[0,0]):
                mutation=True;require(set(new_objects)==set(old_objects),'transform changed ownership/IDs')
                xs=[center(old_objects[i])['x_mm'] for i in old_selected];pivot=(min(xs)+max(xs))/2
                for ident,a in old_objects.items():
                    b=new_objects[ident]
                    if ident not in old_selected:require(a==b,'unselected transform')
                    else:
                        ca,cb=center(a),center(b);x,y=ca['x_mm'],ca['y_mm']
                        if kind=='rotate':x,y=-y,x
                        elif kind=='mirror':x=2*pivot-x
                        else:
                            dx,dy=(expected['dx'],expected['dy']) if kind=='move' else (expected['to'][0]-expected['from'][0],expected['to'][1]-expected['from'][1]);x+=dx;y+=dy
                        close(cb['x_mm'],x,'manufacturing X');close(cb['y_mm'],y,'manufacturing Y');require(a['exposure']==b['exposure'],'transform exposure')
                require(selected(state)==old_selected,'transform changed selection order')
            elif kind=='duplicate':
                mutation=True;new_ids=[]
                for a_layer,b_layer in zip(old['layers'],snapshot['layers']):
                    require(a_layer['id']==b_layer['id'],'duplicate layer ownership');at=0
                    for a in a_layer['objects']:
                        require(b_layer['objects'][at]==a,'duplicate changed original/exposure position');at+=1
                        if (a_layer['id'],a['object_id']) in old_selected:
                            b=b_layer['objects'][at];at+=1
                            require(b['geometry']==a['geometry'] and b['exposure']==a['exposure'],'copy geometry/exposure')
                            ident=(a_layer['id'],b['object_id']);require(ident not in old_objects,'copy reused ID');new_ids.append(ident)
                    require(at==len(b_layer['objects']),'unexpected duplicate objects')
                require(set(selected(state))==set(new_ids) and len(new_ids)==len(old_selected),'copied selection')
            elif kind=='delete':
                mutation=True
                for a,b in zip(old['layers'],snapshot['layers']):require(a['id']==b['id'] and b['objects']==[o for o in a['objects'] if (a['id'],o['object_id']) not in old_selected],'delete partial/wrong layer')
                require(not selected(state),'delete selection cleanup')
            elif kind=='undo':
                require(undo,'missing undo source');redo.append(old_content);require(content(snapshot)==undo.pop(),'undo not exact')
                require(revision==old_revision+1,'undo revision');require(state['info']['undo_entries']==old_state['info']['undo_entries']-1,'undo count')
            elif kind=='redo':
                require(redo,'missing redo source');undo.append(old_content);require(content(snapshot)==redo.pop(),'redo not exact');require(revision==old_revision+1,'redo revision')
            else:
                require(content(snapshot)==old_content,'nonmanufacturing step changed geometry')
                if kind!='reopen':require(revision==old_revision,'nonmanufacturing revision')
                if kind=='recovery':
                    require(state['info']==old_state['info'] and selected(state)==old_selected,'recovery changed selection/history/dirty')
                    require(state['click_cycle'] is not None and state['click_cycle']==old_state['click_cycle'],'recovery discarded or changed click cycle')
                    actions=[a for a in ledger['actions'] if a['frame_id'] in {f['frame_id'] for f in step_frames}]
                    require(len(actions)==1 and actions[0]['detail']=={'kind':'other','name':'RecoveryWrite'},'missing actual recovery submission')
                    receipt=state['task_receipt'];require(receipt['task_id']==actions[0]['sequence'] and receipt['state']=='completed' and receipt['input']==receipt['result_version'],'recovery TaskVersion/completion')
                    require(receipt['input']['document_id']==state['info']['document_id'] and receipt['input']['document_revision']==state['info']['revision'] and receipt['input']['workspace_revision']==state['info']['workspace_revision'],'recovery task identity changed')
                    path=directory/f'idle-recovery-step-{index}';files=list(path.glob('*.json'));require(len(files)==1,'missing recovery metadata')
                    meta=read(files[0]);require(meta['snapshot_hash']==sha(files[0].with_suffix('.rcam')) and meta['project_id']==state['info']['project_id'] and meta['revision']==state['info']['revision'] and meta['workspace_revision']==state['info']['workspace_revision'],'recovery bytes/metadata binding')
            if mutation:
                undo.append(old_content);redo=[];require(revision==old_revision+1,'edit revision');require(state['info']['undo_entries']==old_state['info']['undo_entries']+1,'edit not one undo')
                require(snapshot['apertures']==old['apertures'] and snapshot['block_definitions']==old['block_definitions'],'shared definitions mutated')
            if kind in ('zoom','pan'):require(state['camera']!=old_state['camera'],'navigation did not change camera')
            if kind=='click':
                candidates=[];pt=expected['from'];tol=6/(state['camera'][2]*state['ppp'])
                for layer in state['layers']:
                    if not (layer['visible'] and layer['effective_visible'] and layer['selectable']):continue
                    source=next(l for l in snapshot['layers'] if l['id']==layer['layer_id'])
                    for o in reversed(source['objects']):
                        c=center(o)
                        if math.hypot(c['x_mm']-pt[0],c['y_mm']-pt[1])<=1+tol:candidates.append((source['id'],o['object_id']))
                require(candidates,'click fixture witness')
                mode='remove' if expected.get('shift') else 'add' if expected.get('ctrl') else 'replace'
                same=cycle is not None and cycle[0]==candidates and cycle[1]==state['camera'] and cycle[2]==state['canvas'] and cycle[3]==state['ppp']
                chosen=((cycle[4]+1)%len(candidates) if mode=='replace' else cycle[4]) if same else 0
                target=candidates[chosen]
                wanted=([target] if mode=='replace' else [i for i in old_selected if i!=target] if mode=='remove' else old_selected+([] if target in old_selected else [target]))
                require(selected(state)==wanted,f'cycle order/modifier wrong at step {index}')
                cycle=(candidates,state['camera'],state['canvas'],state['ppp'],chosen) if mode=='replace' else None
            elif kind=='drag' and expected.get('escape'):require(selected(state)==old_selected,'Esc changed selection')
            elif kind=='drag' and expected['from']==[-3,-3]:
                require(set(selected(state))==set(new_objects) and len(selected(state))==4,'cross-layer marquee wrong')
                require(state['click_cycle'] is None,'marquee retained click cycle')
                cycle=None
            elif kind!='recovery':cycle=None
        previous=item;verified.append(kind)
    require(verified[45]=='reimport_exports' and verified[-1]==('click' if request['stress'] else 'reimport_exports'),'missing native Gerber reimport/stress')
    if request['stress']:require([i for i,k in enumerate(verified) if k=='recovery']==[121,123,127],'missing post-recovery cycling matrix')
    require((directory/'workflow.rcam').is_file(),'missing native project')
    for name in ('lower','upper'):require((directory/f'{name}.gbr').is_file(),'missing native export')
    require((directory/'native-interaction.mov').stat().st_size>10000,'missing native video')
    return {'schema_version':2,'stage':'S5-I1','result':'PASS','steps':len(verified),'frames':len(frames),'binary_sha256':report['binary_sha256'],'source_manifest_sha256':request['source_manifest_sha256'],'scope':'Mac native functional assertions; no PMIX/physical latency claim'}
if __name__=='__main__':
    p=argparse.ArgumentParser(description=__doc__);p.add_argument('directory',type=Path);p.add_argument('--source-root',type=Path);p.add_argument('--binary',type=Path);a=p.parse_args();print(json.dumps(verify(a.directory,a.source_root,a.binary),ensure_ascii=False,indent=2))
