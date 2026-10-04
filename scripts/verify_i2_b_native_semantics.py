"""Independent input/clock/fixture trajectory and surface-feedback oracles.
No execution, process lookup, network, cleanup, or acceptance shortcut.
"""
import math
from run_i2_b_native import steps, feedback_steps


def need(value, message):
    if not value:
        raise ValueError(message)


def near(a, b, tolerance=1e-9):
    return len(a) == len(b) and all(type(x) in (int,float) and math.isfinite(float(x)) and abs(x-y) <= tolerance for x,y in zip(a,b))


def operation_from_points(point):
    b=point['base']['world'];t=point['target']['world'];mode=point['mode']
    if mode in ('Move','Copy'):
        return {'kind':'move' if mode=='Move' else 'duplicate','dx_mm':t[0]-b[0],'dy_mm':t[1]-b[1]}
    if mode=='Rotate':
        return {'kind':'rotate','angle_deg':float(point['angle']),'pivot_mm':dict(zip(('x_mm','y_mm'),b))}
    return {'kind':'mirror','axis':{'kind':'horizontal' if mode=='HorizontalMirror' else 'vertical','coordinate_mm':b[1] if mode=='HorizontalMirror' else b[0]}}


def events_for(frames, step):
    return [(f, e) for f in frames if f['step']==step for e in f['delivered_input']['events']]


def verify_trace(request, obs, ledger, frames):
    role=request['role']
    need(role in ('functional','feedback') and type(request['copy']) is bool and type(obs['evidence_version']) is int and type(request['evidence_version']) is int and obs['evidence_version']==request['evidence_version']==3,'native evidence protocol version/role')
    expected=feedback_steps() if role=='feedback' else steps(request['copy'])
    return verify_input_trace(expected, request, obs, ledger, frames)


def verify_input_trace(expected, request, obs, ledger, frames):
    need(request['steps']==expected,'frozen native mandatory trajectory changed/omitted')
    delivered=ledger['delivered_inputs'];by_input={r['frame_id']:r for r in delivered}
    need(len(by_input)==len(delivered) and list(by_input)==ledger['input_ids'],'independent delivered input producer sequence')
    paint=ledger['paint_records'];by_paint={r['frame_id']:r['paint_ns'] for r in paint}
    need(len(by_paint)==len(paint) and list(by_paint)==ledger['paint_ids'],'independent paint clock producer sequence')
    last=0
    for frame in frames:
        fid=frame['frame_id'];delivery=frame['delivered_input']
        need(delivery==by_input.get(fid),'actual delivered input producer mismatch')
        t0=delivery['t0_ns'];update=frame['input_ns'];complete=frame['completed_ns']
        need(all(type(n) is int for n in (t0,update,complete)) and 0<t0<=update<=complete and (last==0 or last<t0),'native impossible/reversed input/update/completion clock')
        last=t0
        if frame['painted']:
            need(fid in by_paint and update<=by_paint[fid]<=complete,'native paint causality')
        point=frame['state'].get('point_transform')
        if point and point['operation'] is not None:
            need(point['base'] is not None and point['target'] is not None,'resolved operation lacks points')
            need(operation_from_points(point)==point['operation'],'base/target/mode do not derive operation')
    for n, step in enumerate(expected):
        kind=step['kind'];rows=events_for(frames,n);es=[e for _,e in rows]
        if kind in ('widget','point_click') or kind=='cancel_conflict' and step['via']=='button':
            buttons=[(f,e) for f,e in rows if e['kind']=='button' and e['button']=='Primary']
            need([e['pressed'] for _,e in buttons]==[True,False],'native actual button press/release omitted/reordered')
            need(near(buttons[0][1]['position'],buttons[1][1]['position'],1e-3),'native release differs from pressed target')
            for f,e in buttons:
                if kind in ('widget','cancel_conflict'):
                    widget=f['delivered_input']['widget'];rect=widget['rect']
                    need(widget['enabled'] is True and widget['frame']<f['frame_id'] and f['frame_id']-widget['frame']<=3,'native widget input lacks recent enabled control')
                    need(near(e['position'],[(rect[0]+rect[2])/2,(rect[1]+rect[3])/2],1e-3),'actual widget input is outside named control')
                else:
                    d=f['delivered_input'];camera=d['camera'];rect=d['canvas'];world=step['from']
                    pixel=[(rect[0]+rect[2])/2+(world[0]-camera[0])*camera[2],(rect[1]+rect[3])/2-(world[1]-camera[1])*camera[2]]
                    need(near(e['position'],pixel,1e-3),'canvas actual input does not map requested world point')
            if kind!='cancel_conflict':
                for f,e in buttons:need(e in f['input']['events'],'actual UI button event erased after delivery')
        if kind=='text':
            need(any(e.get('kind')=='text' and e.get('value')==step['value'] for e in es),'native actual text omitted/mismatched')
            need(all(any(e.get('kind')=='key' and e.get('key')=='A' and e.get('pressed') is pressed and e.get('modifiers',{}).get('command') is True for e in es) for pressed in (True,False)),'text replacement lacks real select-all key input')
        if kind=='escape':need(any(e.get('kind')=='key' and e.get('key')=='Escape' and e.get('pressed') is True for e in es),'native Escape input omitted')
        if kind=='cancel_conflict':
            if step['via']=='enter' or step['cancel']=='escape':
                need(all(any(e.get('kind')=='key' and e.get('key')==key and e.get('pressed') is True for e in es) for key in ('Escape','Enter')),'same-frame cancel/confirm input omitted')
                conflicts=[f for f in frames if f['step']==n and all(any(e.get('kind')=='key' and e.get('key')==key and e.get('pressed') is True for e in f['delivered_input']['events']) for key in ('Escape','Enter'))]
                need(conflicts,'cancel and confirm were separated into different frames')
                if step['repeat']:need(any(e.get('repeat') is True for e in es if e.get('kind')=='key'),'repeat-key case replaced by ordinary key')
            elif step['cancel']=='blur':need(any(e.get('kind')=='focus' and e.get('focused') is False for e in es) and any(f['delivered_input']['focused'] is False for f in frames if f['step']==n),'real blur omitted/forced focused')
            elif step['cancel']=='gone':need(any(e.get('kind')=='gone' for e in es),'PointerGone omitted')
            elif step['cancel']=='ime':need(any(e.get('kind')=='ime' and 'Preedit' in e.get('debug','') for e in es),'IME preedit omitted')
    return by_paint


def verify_fixed_transform(index, point):
    a=math.pi*(1-.25**2)
    area=[(24+22*a+20)/(9+2*a),(8+2*a+4)/(9+2*a)]
    bases=[area,[10.25,1.75],[3.,-2.],[10.25,1.75],[2.,1.]]
    need(index<5 and near(point['base']['world'],bases[index]),'fixture/input independent base point mismatch')
    sources=['AreaCentroid','BoundingCenter','Numeric','BoundingCenter','Feature(']
    need(point['base']['source'].startswith(sources[index]),'required base source provenance omitted')
    if index==4:need(near(point['target']['world'],[8.,-3.]) and point['target']['source']=='Numeric','picked B/numeric T trajectory mismatch')
    elif index==2:need(near(point['target']['world'],[0.,0.]),'numeric base move changed default target')
    need(operation_from_points(point)==point['operation'],'fixture point→operation derivation mismatch')


def verify_feedback(request,obs,ledger,frames,paint,read):
    protocol=request['feedback_protocol']
    need(protocol['camera']==[10.,1.,40.] and protocol['dpi']==2. and protocol['warmups']==10 and protocol['measured']==30 and protocol['p95_limit_ms']==100,'feedback gate configuration weakened')
    need(request['display']['modeRefreshHz']>0,'display refresh configuration absent')
    rows=obs['feedback'];need([r['sample'] for r in rows]==list(range(40)),'feedback raw warmup/measured samples omitted')
    by_frame={f['frame_id']:f for f in frames};by_callback={s['label']:s for s in ledger['surface_callbacks']};values=[];baseline=None
    need(len(by_callback)==len(rows),'actual surface callback producer count')
    for n,row in enumerate(rows):
        step=request['steps'][row['step']];fid=row['frame_id'];frame=by_frame[fid];state=row['state'];delivery=frame['delivered_input']
        need(step['kind']=='feedback_move' and step['sample']==n and step['warmup'] is (n<10),'feedback source event binding')
        need(state==frame['state'] and frame['painted'] is True and frame['gpu_completed'] is True,'feedback not bound to actual completed painted state')
        need(state['camera']==protocol['camera'] and state['ppp']==protocol['dpi'] and state['point_pick']=='Base','feedback configuration/point workflow changed')
        need(sum(l['object_count'] for l in state['layers'])==100000 and state['layers'][0]['provenance']['imported_sha256']==request['performance_fixture_sha256'],'feedback fixed P100K sample identity/count')
        current=(state['info'],state['selected'])
        if baseline is None:baseline=current
        need(current==baseline,'feedback changed manufacturing/selection')
        marker=state['snap_marker'];angle=.53;x=7+n%8
        target=[x+.25*math.cos(angle),1+.25*math.sin(angle)]
        need(marker['kind']=='Some(Nearest)' and near(marker['world'],target,1e-5),'feedback contour marker not independently derived')
        rect=state['canvas'];screen=[((rect[0]+rect[2])/2+(target[0]-10)*40)*2,((rect[1]+rect[3])/2-(target[1]-1)*40)*2]
        need(near(row['pixel'],screen,1.),'feedback image pixel differs from world/camera point')
        callback=by_frame[row['callback_frame_id']]
        need(row['t0_ns']==delivery['t0_ns'] and 0<row['t0_ns']<=frame['input_ns']<=paint[fid]<=frame['completed_ns']<=row['t1_ns']<=callback['delivered_input']['t0_ns'],'feedback input/paint/readback clock causality')
        need(by_callback.get(row['label'])=={'label':row['label'],'callback_frame_id':row['callback_frame_id'],'t1_ns':row['t1_ns'],'crop_sha256':row['crop_sha256']},'independent surface callback producer mismatch')
        need(all(f['state']['snap_marker']==marker and f['state']['info']==state['info'] for f in frames if fid<=f['frame_id']<row['callback_frame_id']),'feedback scene changed between input frame and surface readback')
        expected_raw=[((rect[0]+rect[2])/2+(step['from'][0]-10)*40),((rect[1]+rect[3])/2-(step['from'][1]-1)*40)]
        need(any(e.get('kind')=='move' and near(e.get('position',[]),expected_raw,1e-3) for e in delivery['events']),'feedback actual world→pointer input absent/mismatched')
        need(row['label']==f'feedback-{n:02}-{fid}' and row['crop']==row['label']+'.ppm','surface callback label/frame binding')
        pixels=read(row['crop']);need(pixels.startswith(b'P6\n25 25\n255\n'),'feedback controlled surface crop absent')
        import hashlib
        need(hashlib.sha256(pixels).hexdigest()==row['crop_sha256'],'surface crop hash mismatch')
        rgb=pixels.split(b'\n',3)[3];need(len(rgb)==25*25*3,'surface crop pixel count')
        green=[(i%25,i//25) for i in range(625) if all(abs(rgb[3*i+j]-color)<=8 for j,color in enumerate((94,235,205)))]
        need(10<=len(green)<=100 and any(abs(x-12)<=2 and abs(y-12)<=2 for x,y in green),'actual post-paint Snap marker pixels absent')
        need(sum(abs(y-12)<=2 and 2<=abs(x-12)<=8 for x,y in green)>=6,'surface horizontal feedback marker stroke absent')
        vertical=[(x,y) for y in range(4,11) for x in range(10,15) if 60<=rgb[(y*25+x)*3]<=105 and 150<=rgb[(y*25+x)*3+1]<=243 and 130<=rgb[(y*25+x)*3+2]<=213]
        need(len(vertical)>=4,'surface upper feedback marker stroke absent')
        need(near(row['crop_origin_px'],[row['pixel'][0]-12,row['pixel'][1]-12]),'surface crop spatial binding')
        value=(row['t1_ns']-row['t0_ns'])/1e6;need(abs(value-row['latency_ms'])<1e-9,'feedback latency arithmetic mismatch')
        if n>=10:values.append(value)
    p95=sorted(values)[math.ceil(.95*len(values))-1]
    need(len(values)==30 and p95<=100,'B-07 actual surface feedback p95 exceeds100ms')
    return {'measured':30,'warmups':10,'raw_ms':values,'p95_ms':p95,'refresh_hz':request['display']['modeRefreshHz'],'scope':protocol['scope']}
