"""Portable C native truth oracle. Synthetic input, production Metal, no OS claim."""
import argparse
import gzip
import hashlib
import json
import math
import re
from pathlib import Path
from i2_c_protocol import steps
from verify_s5m2_evidence import load, safe
from verify_i1_native import content, objects, selected, center, verify_capture
from verify_i2_b_native_semantics import verify_input_trace, near, operation_from_points
from verify_i2_c_ui_roi import analyze, need, ppm


def sha(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()


def metrics(state, area, perimeter):
    status = state['status']
    need(status['state'] == 'ready', 'composed status not ready')
    values = re.findall(r'(?:面积|周长) ([0-9.]+) (?:mm²|mm)', status['tooltip'])
    need(len(values) == 2 and near(list(map(float, values)), [area, perimeter], 1e-8),
         'independent selected material area/perimeter mismatch')


def verify(root, source=None, binary=None, manifest=None):
    root = Path(root); raw = root / 'raw'
    request = load(raw / 'request.json'); report = load(raw / 'observations.json')
    runner = load(raw / 'runner.json'); ledger = load(raw / 'capture-ledger.json')
    expected = steps(False)
    need(request['stage'] == report['stage'] == 'S5-I2-C' and request['smoke'] is False
         and request['steps'] == expected and request['evidence_version'] == report['evidence_version'] == 3,
         'C full mandatory trajectory/stage/version')
    need(request['whole_I2_acceptance'] == 'NOT_ACCEPTED' and request['user_flicker_report'] == 'OPEN',
         'unearned I2/flicker acceptance')
    need(runner['exit_code'] == 0 and runner['error'] is None and report['error'] is None
         and report['profile'] == 'release' and 'Metal' in report['adapter'], 'native runner/release/Metal')
    need(request == report['request'] and request['binary_sha256'] == report['binary_sha256'] == runner['binary_sha256'],
         'native request/binary producer mismatch')
    if binary is not None:
        need(sha(Path(binary)) == report['binary_sha256'], 'external binary mismatch')
    if manifest is not None:
        need(request['source_manifest_sha256'] == manifest, 'external source manifest mismatch')
    if source is not None:
        source = Path(source)
        need(sha(source/'MANIFEST.sha256') == request['source_manifest_sha256'], 'source manifest mismatch')
        for group, directory in [('fixtures','s5i2b'),('C_fixtures','s5i2c')]:
            for name, digest in request[group].items():
                need(sha(source/'fixtures/synthetic'/directory/name) == digest, 'fixture identity mismatch')
    owned = load(raw/'owned-process.json')
    windows = load(raw/'window.json')
    need(owned['binary_sha256'] == report['binary_sha256'] and windows,
         'owned native window/binary missing')
    frames = []
    for member in report['frame_files']:
        path = safe(raw, member['path']); need(sha(path) == member['sha256'], 'frame shard hash')
        part = load(path); need(len(part) == member['count'], 'frame shard count'); frames.extend(part)
    need(all(type(f['frame_id']) is int and type(f['painted']) is bool
             and type(f['gpu_completed']) is bool for f in frames), 'native frame field types')
    verify_capture(dict(report, frames=frames), ledger)
    verify_input_trace(expected, request, report, ledger, frames)
    need([r['step'] for r in report['records']] == list(range(len(expected))), 'missing/duplicate C steps')
    records = [load(safe(raw, r['path'])) for r in report['records']]
    by_frame = {f['frame_id']:f for f in frames}
    undo = []; redo = []; conflicts = []; combinations = set(); cursor_styles = set(); cycle = None
    for n, (step, record) in enumerate(zip(expected, records)):
        state = record['state']; complete = record['completed_frame']; snapshot = record['snapshot']
        need(complete == by_frame.get(complete['frame_id']) and complete['state'] == state
             and complete['step'] == n and complete['painted'] and complete['gpu_completed'],
             'step receipt not bound to completed Metal frame')
        need(record['input'] == step, 'step input producer mismatch')
        previous = records[n-1] if n else None
        old_state = previous['state'] if previous else None
        old_snapshot = previous['snapshot'] if previous else None
        kind = step['kind']; step_frames = [f for f in frames if f['step'] == n]
        events = [(f,e) for f in step_frames for e in f['delivered_input']['events']]
        if kind in ('import','reimport_exports'):
            need(len(snapshot['layers']) == 2 and len(objects(snapshot)) == 4,
                 'I1 overlapping two-layer four-object fixture omitted')
            by_name={l['layer_id']:l['provenance']['original_file_name'] for l in state['layers']}
            for layer in snapshot['layers']:
                points=[list(center(o).values()) for o in layer['objects']]
                expected_points=[[0.,0.],[6.,0.]] if 'lower' in by_name[layer['id']] else [[0.,0.],[0.,6.]]
                need(points == expected_points, 'independent I1 import/export/reimport coordinates')
            cycle=None
        if kind == 'click' and len(state['layers']) == 2:
            candidates=[]; point=step['from']; tolerance=6/(state['camera'][2]*state['ppp'])
            for layer in state['layers']:
                if not (layer['visible'] and layer['effective_visible'] and layer['selectable']): continue
                source_layer=next(l for l in snapshot['layers'] if l['id'] == layer['layer_id'])
                for obj in reversed(source_layer['objects']):
                    c=center(obj)
                    if math.hypot(c['x_mm']-point[0],c['y_mm']-point[1]) <= 1+tolerance:
                        candidates.append((source_layer['id'],obj['object_id']))
            need(candidates,'I1 click fixture witness')
            mode='remove' if step.get('shift') else 'add' if step.get('ctrl') else 'replace'
            same=cycle and cycle[:4] == (candidates,state['camera'],state['canvas'],state['ppp'])
            chosen=((cycle[4]+1)%len(candidates) if mode=='replace' else cycle[4]) if same else 0
            target=candidates[chosen]; prior_selected=selected(old_state)
            wanted=([target] if mode=='replace' else [i for i in prior_selected if i!=target] if mode=='remove'
                    else prior_selected+([] if target in prior_selected else [target]))
            need(selected(state) == wanted,f'I1 cycle/modifier order at step {n}')
            cycle=(candidates,state['camera'],state['canvas'],state['ppp'],chosen) if mode=='replace' else None
        elif kind != 'recovery': cycle=None
        if kind == 'c_preferences':
            need(state['interaction'] == step['value'], 'preferences did not apply independently')
        if kind == 'c_reload_preferences':
            need(state['interaction'] == old_state['interaction'], 'persisted preferences reload mismatch')
        if kind == 'c_hover_menu':
            need(state['cursor_shown'] is False, 'cross displayed over menu')
        if kind == 'c_hover':
            cursor_styles.add(state['interaction']['cursor'])
            need(state['cursor_shown'] == (state['interaction']['cursor'] != 'normal'), 'canvas cursor style mismatch')
        status = state['status']
        need(status is not None and not any(token in status['selection'] for token in ('object-', 'revision', 'doc-')),
             'internal identity leaked into status')
        slots = status['slots']
        need(all(r[3]-r[1] == 20 for r in slots), 'status line height changed')
        need(all(a[2] <= b[0] for a,b in zip(slots,slots[1:])), 'status fixed slots overlap')
        if kind == 'c_new':
            need(not state['layers'] and not state['selected'] and not content(snapshot)['layers']
                 and state['info']['undo_entries'] == 0, 'explicit synthetic reset not empty')
            undo = []; redo = []
        if kind == 'b_select_all' and len(state['layers']) == 3:
            metrics(state, 9+1.875*math.pi, 20+5*math.pi)
        if kind == 'c_complex_select':
            need(len(state['selected']) == 5, 'complex selected fixture omitted')
            metrics(state, 48, 50+6*math.sqrt(2))
        if kind == 'widget' and step['name'] == 'block-create':
            definition = snapshot['block_definitions'][-1]
            need(definition['local_origin'] == {'x_mm':7.75,'y_mm':1.3125}
                 and len(definition['objects']) == 5, 'independent complex Block area origin')
            for before, after in zip(old_state['selected'], definition['objects']):
                def shifted(value):
                    if isinstance(value,dict):
                        if set(value) == {'x_mm','y_mm'}:
                            return {'x_mm':value['x_mm']-7.75,'y_mm':value['y_mm']-1.3125}
                        return {k:shifted(v) for k,v in value.items()}
                    if isinstance(value,list): return [shifted(v) for v in value]
                    return value
                need(after['exposure'] == before['object']['exposure']
                     and after['geometry'] == shifted(before['object']['geometry']),
                     'Block local geometry/order/exposure mismatch')
        if kind == 'widget' and step['name'] == 'adapter-apply' and old_state['point_adapter']['target'] == 'BlockTarget':
            adapter = old_state['point_adapter']; instance = state['selected'][0]['object']['geometry']['BlockInstance']
            need(adapter['world'] == [0.,0.] and adapter['block_translation'] == {'x_mm':.75,'y_mm':-.6875}
                 and instance['transform']['translation'] == adapter['block_translation']
                 and snapshot['block_definitions'] == old_snapshot['block_definitions'],
                 'independent Block bounds/reference target placement')
        if kind in ('drag', 'click'):
            buttons = [(f,e) for f,e in events if e['kind'] == 'button' and e['button'] == 'Primary']
            need([e['pressed'] for _,e in buttons] == [True,False], 'real press/release omitted')
            for f,e in buttons:
                world = step['from'] if e['pressed'] or kind == 'click' else step['to']
                d = f['delivered_input']; c = d['camera']; r = d['canvas']
                wanted = [(r[0]+r[2])/2+(world[0]-c[0])*c[2], (r[1]+r[3])/2-(world[1]-c[1])*c[2]]
                need(near(e['position'], wanted, 1e-3), 'actual pointer/world mapping')
            if step.get('switch_off'):
                mouse = step['switch_off']; cancel = step['cancel']; conflicts.append((mouse,cancel))
                key = 'drag_move' if mouse == 'drag' else 'grip_edit'
                need(state['interaction'][key] is False and content(snapshot) == content(old_snapshot)
                     and state['info'] == old_state['info'] and selected(state) == selected(old_state)
                     and state['scene_serial'] == old_state['scene_serial'], 'switch-off release partially committed')
                need(not complete['gesture_after'] and not complete['grip_after'], 'cancel retained gesture')
                ids={f['frame_id'] for f in step_frames}
                need(not any(a['frame_id'] in ids and a['detail']['kind'] in ('point_apply','grip_edit','legacy_transform')
                             for a in ledger['actions']), 'cancel dispatched manufacturing action')
                if mouse == 'drag':
                    need(any(f.get('gesture_after') and f['gesture_after'].get('object_drag')
                             and f['gesture_after']['object_drag']['dragging'] for f in step_frames),
                         'switch-off drag lacks nonempty preview witness')
                else:
                    need(any(f['state'].get('mouse_grip') and f['state']['mouse_grip']['moved']
                             and f['state']['mouse_grip']['preview']['geometry'] != f['state']['mouse_grip']['original']['geometry']
                             for f in step_frames), 'switch-off Grip lacks changed geometry witness')
                release_frame = buttons[-1][0]
                delivered = release_frame['delivered_input']; es = delivered['events']
                if cancel in ('escape','repeat'):
                    need(all(any(e.get('key') == k and e.get('pressed') for e in es) for k in ('Escape','Enter')),
                         'same-frame cancel/confirm omitted')
                    if cancel == 'repeat': need(any(e.get('repeat') is True for e in es), 'repeat omitted')
                if cancel == 'blur': need(delivered['focused'] is False, 'actual blur omitted')
                if cancel == 'gone': need(any(e['kind'] == 'gone' for e in es), 'PointerGone omitted')
                if cancel == 'ime': need(any(e['kind'] == 'ime' and 'Preedit' in e['debug'] for e in es), 'IME omitted')
            elif kind == 'drag' and step['from'] == [20.,4.]:
                combinations.add((old_state['interaction']['drag_move'],old_state['interaction']['grip_edit']))
                if not old_state['interaction']['drag_move']:
                    need(content(snapshot) == content(old_snapshot) and state['info'] == old_state['info'],
                         'disabled mouse movement mutated content/history')
            elif kind == 'drag' and step['from'] == [-2.,-2.]:
                need(len(state['selected']) == 5 and content(snapshot) == content(old_snapshot)
                     and state['info'] == old_state['info'], 'disabled/enabled box selection changed manufacture')
            elif kind == 'drag' and step['from'] == [20.5,4.]:
                pref = old_state['interaction']; apertures = {a['id']:a['shape'] for a in snapshot['apertures']}
                obj = state['selected'][0]['object']; flash = obj['geometry']['Flash']
                if pref['grip_edit']:
                    need(flash['center'] == {'x_mm':20.75,'y_mm':4.}
                         and apertures[flash['aperture_id']]['Rectangle']['width_mm'] == 2.5,
                         'enabled Grip preview/commit geometry')
                elif pref['drag_move']:
                    need(flash['center'] == {'x_mm':21.5,'y_mm':4.}
                         and apertures[flash['aperture_id']]['Rectangle']['width_mm'] == 1.,
                         'disabled Grip ordinary movement fallback')
                else:
                    need(content(snapshot) == content(old_snapshot) and state['info'] == old_state['info'],
                         'both-disabled Grip attempt mutated manufacturing')
        if previous and snapshot is not None and old_snapshot is not None and kind not in ('c_new','b_import','c_complex_import','import','reimport_exports','reopen'):
            changed = content(snapshot) != content(old_snapshot)
            if kind == 'undo':
                need(undo and content(snapshot) == undo.pop(), 'Undo not exact previous snapshot')
                redo.append(content(old_snapshot))
                need(int(state['info']['revision']) == int(old_state['info']['revision'])+1, 'Undo revision')
            elif kind == 'redo':
                need(redo and content(snapshot) == redo.pop(), 'Redo not exact snapshot')
                undo.append(content(old_snapshot))
            elif changed:
                need(kind in ('drag','move','rotate','mirror','duplicate','delete')
                     or kind == 'widget' and (step['name'] in ('transform-apply','block-create')
                     or step['name'] == 'adapter-apply' and old_state['point_adapter']['target'] == 'BlockTarget'),
                     f'nonmanufacturing step {n} changed geometry')
                need(int(state['info']['revision']) == int(old_state['info']['revision'])+1
                     and state['info']['undo_entries'] == old_state['info']['undo_entries']+1,
                     'manufacturing edit not one atomic Undo/revision')
                # Independent fixture math, including ownership and unchanged objects.
                if kind in ('rotate','mirror') or kind == 'drag' and step['from'] in ([20.,4.],[0,0]):
                    before=objects(old_snapshot);after=objects(snapshot);chosen=set(selected(old_state))
                    need(set(before)==set(after) and selected(state)==selected(old_state), 'transform identity/selection')
                    pivot=(min(center(before[i])['x_mm'] for i in chosen)+max(center(before[i])['x_mm'] for i in chosen))/2
                    for ident,obj in before.items():
                        if ident not in chosen: need(after[ident] == obj, 'unselected geometry changed');continue
                        p=center(obj);x,y=p['x_mm'],p['y_mm']
                        if kind=='rotate': wanted=[-y,x]
                        elif kind=='mirror': wanted=[2*pivot-x,y]
                        else: wanted=[x+step['to'][0]-step['from'][0], y+step['to'][1]-step['from'][1]]
                        actual=center(after[ident]);need(near([actual['x_mm'],actual['y_mm']],wanted,1e-8),'independent native transform geometry')
                    need(snapshot['apertures']==old_snapshot['apertures'] and snapshot['block_definitions']==old_snapshot['block_definitions'],
                         'transform changed shared definitions')
                if kind=='widget' and step['name']=='transform-apply':
                    p=old_state['point_transform']; need(p['operation']==operation_from_points(p), 'resolved point operation')
                    provenance=re.fullmatch(r'AreaCentroid \{ error_mm: ([^ }]+) \}',p['base']['source'])
                    need(near(p['base']['world'],[20.,4.]) and provenance is not None
                         and math.isfinite(float(provenance[1])) and 0 <= float(provenance[1]) <= 1e-9
                         and p['target']['world']==[0.,0.], 'independent single-rectangle area base/default target')
                    before=objects(old_snapshot);after=objects(snapshot);chosen=set(selected(old_state))
                    need(set(before)==set(after) and len(chosen)==1, 'explicit move ownership')
                    for ident,obj in before.items():
                        if ident not in chosen: need(after[ident]==obj,'explicit move changed unselected object')
                        else: need(center(after[ident])=={'x_mm':0.,'y_mm':0.}, 'explicit move manufacturing target')
                undo.append(content(old_snapshot)); redo = []
            elif kind not in ('recovery','save_project'):
                need(state['info']['revision'] == old_state['info']['revision'], 'display-only step changed revision')
    need(set(conflicts) == {(mouse,cancel) for mouse in ('drag','grip') for cancel in ('none','escape','repeat','blur','gone','ime')}
         and len(conflicts) == 12, 'complete cancel matrix omitted')
    need(combinations == {(a,b) for a in (False,True) for b in (False,True)}, 'four independent combinations omitted')
    need(cursor_styles == {'normal','small_cross','large_cross'}, 'three cursor styles omitted')
    roi = analyze(root/'ui-roi', report['binary_sha256'], request['source_manifest_sha256'])
    need(not roi['outliers'], 'static menu ROI outliers require inspection')
    return {'schema_version':2,'result':'C_NATIVE_FUNCTIONAL_AND_SURFACE_SCOPE_VERIFIED',
            'steps':len(records),'frames':len(frames),'cancel_conflicts':len(conflicts),
            'binary_sha256':report['binary_sha256'], 'source_manifest_sha256':request['source_manifest_sha256'],
            'roi':{k:v for k,v in roi.items() if k not in ('identity','entries','outliers')},
            'user_flicker_report':'OPEN','whole_I2_acceptance':'NOT_ACCEPTED'}


if __name__ == '__main__':
    p = argparse.ArgumentParser(description=__doc__); p.add_argument('directory',type=Path)
    p.add_argument('--source',type=Path); p.add_argument('--binary',type=Path)
    a = p.parse_args()
    print(json.dumps(verify(a.directory,a.source,a.binary),ensure_ascii=False,indent=2))
