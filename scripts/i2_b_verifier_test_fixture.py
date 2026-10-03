"""Fabricated protocol-v3 unit payloads only, never execution evidence.
Simulates the frozen input contract so resealed corruptions reach semantic checks.
All directories and template payloads are preserved. No program is launched.
"""
import copy,hashlib,json,math
from run_i2_b_native import steps,feedback_steps
from verify_i2_b_native_semantics import operation_from_points

def blob(v):return (json.dumps(v,separators=(',',':'))+'\n').encode()
def digest(v):return hashlib.sha256(v).hexdigest()
def build(commit,source_hash,binary,fixtures,performance):
    files={};indices={'native':[],'feedback':[]}
    for role in ('functional','feedback'):
      for run in range(3):
        copy_mode=run==2 and role=='functional';plan=steps(copy_mode) if role=='functional' else feedback_steps();prefix=f'{"native" if role=="functional" else "feedback"}/run-{run}/raw';run_id=f'SYNTHETIC-VERIFIER-ONLY-{role}-{run}'
        request={'schema_version':2,'stage':'S5-I2-B','role':role,'evidence_version':3,'copy':copy_mode,'run_id':run_id,'source_manifest_sha256':source_hash,'binary_sha256':binary,'fixtures':fixtures,'whole_I2_acceptance':'NOT_ACCEPTED','C_native_rounds':'PENDING','steps':plan,'display':{'modeRefreshHz':144},'performance_fixture_sha256':performance if role=='feedback' else None,'feedback_protocol':{'camera':[10.,1.,40.],'dpi':2.,'warmups':10,'measured':30,'p95_limit_ms':100,'scope':'SYNTHETIC VERIFIER TEST ONLY'} if role=='feedback' else None}
        points=[(1.,1.),(5.,1.),(10.,0.),(12.,2.),(20.,4.)]
        original=[{'layer_id':str(i//2),'object':{'object_id':str(i),'exposure':'dark','geometry':{'Flash':{'center':dict(zip(('x_mm','y_mm'),p)),'aperture_id':'c' if i==4 else 'a'}}}} for i,p in enumerate(points)]
        doc=copy.deepcopy(original);history=[];redo=None;focus=None;measure=[]
        state={'info':{'revision':'0','undo_entries':0},'layers':[],'selected':[],'error':None,'point_transform':None,'point_adapter':None,'point_pick':None,'modal':None,'tool':'Select','camera':[10.,1.,30.],'canvas':[0.,0.,1000.,700.],'ppp':2.,'array_base':None,'array_pitch':['10','10'],'block_reference':{'x_mm':0.,'y_mm':0.},'block_origin':['',''],'board_world':[[0.,0.],[10.,0.]],'measure':{'a':None,'b':None},'text_reference':{'enabled':False,'x':'0','y':'0'},'snap_marker':None}
        frames=[];records=[];deliveries=[];paints=[];actions=[];feedback=[];surface=[];shape={'Rectangle':{'width_mm':1.,'height_mm':1.}}
        def save():history.append((copy.deepcopy(doc),copy.deepcopy(state['selected']),copy.deepcopy(shape)))
        def revision(delta=1):state['info']['revision']=str(int(state['info']['revision'])+1);state['info']['undo_entries']+=delta
        def snapshot():
            return {'layers':[{'objects':[o['object'] for o in doc if o['layer_id']==layer]} for layer in ('0','1','2')],'apertures':[{'id':'c','shape':shape}]}
        def point(mode='Move'):
            center=[10.25,1.75] if len(state['selected'])==5 else [14.5,5.]
            p={'mode':mode,'angle':'90','base':{'world':center,'source':'BoundingCenter'},'target':{'world':[0.,0.],'source':'Numeric'},'preview':None}
            return p
        def adapter(name,world=(0.,0.)):
            target={'measure':'Measure','text':'TextReference','board':'BoardWorld(0)','array_base':'ArrayBase','array_target':'ArrayTarget','grip':'Grip(Right)','local':'BlockLocal("definition")','target':'BlockTarget','create':'BlockCreate'}[name]
            return {'target':target,'world':list(world),'grip_preview':None,'block_translation':None}
        def resolve():
            p=state['point_transform']
            if p:
                p['operation']=operation_from_points(p) if p['base'] and p['target'] else None;p['preview']={'operation':copy.deepcopy(p['operation'])}
            a=state['point_adapter']
            if a:
                if a['target'].startswith('Grip'):
                    a['grip_preview']={'geometry':{'Flash':{'center':{'x_mm':20.75,'y_mm':4.},'aperture_id':'c'}},'aperture_shape':{'Rectangle':{'width_mm':2.5,'height_mm':1.}}}
                if a['target']=='BlockTarget':a['block_translation']={'x_mm':a['world'][0]-state['block_reference']['x_mm'],'y_mm':a['world'][1]-state['block_reference']['y_mm']}
        def apply_step(step):
            nonlocal doc,redo,focus,shape
            k=step['kind'];name=step.get('name');a=state['point_adapter'];p=state['point_transform']
            if k=='new':pass
            elif k=='b_import':state['layers']=[{},{},{}];revision();state['selected']=copy.deepcopy(original)
            elif k=='b_feedback_import':state['layers']=[{'object_count':100000,'provenance':{'imported_sha256':performance}}];revision()
            elif k=='b_feedback_camera':state['camera']=[10.,1.,40.]
            elif k=='b_feedback_select':state['selected']=copy.deepcopy(original[:1]);state['selected'][0]['object']['geometry']['Flash']['center']={'x_mm':10.,'y_mm':1.}
            elif k=='b_select_all':state['selected']=copy.deepcopy(original)
            elif k=='b_select_one':state['selected']=copy.deepcopy(original[-1:])
            elif k=='b_modal':
                mode={'move':'Move','copy':'Copy','rotate':'Rotate','mirror':'HorizontalMirror','vertical':'VerticalMirror'}[step['tool']];state['point_transform']=point(mode);state['modal']='Rotate' if mode=='Rotate' else 'Mirror' if 'Mirror' in mode else 'Move'
                if role=='feedback':state['point_transform']['base']['world']=[10.,1.]
            elif k=='b_adapter':state['point_adapter']=adapter(step['tool'],(20.5,4.) if step['tool']=='grip' else (0.,0.));state['modal']='PointInput'
            elif k=='b_block_create':state['modal']='BlockCreate'
            elif k=='b_block_place':state['tool']='Block';state['modal']=None
            elif k=='b_tool_select':state['tool']='Select';state['modal']=None;state['point_adapter']=None;state['point_transform']=None
            elif k=='widget':
                focus=name
                if name=='B-area':
                    material=math.pi*(1-.25**2);p['base']={'world':[(44+22*material)/(9+2*material),(12+2*material)/(9+2*material)],'source':'AreaCentroid { error_mm: 0.0 }'}
                elif name=='B-bounds':p['base']={'world':[10.25,1.75],'source':'BoundingCenter'}
                elif name=='transform-vertical':p['mode']='VerticalMirror'
                elif name=='transform-copy':p['mode']='Copy'
                elif name in ('B-pick','T-pick','adapter-pick'):state['point_pick']={'B-pick':'Base','T-pick':'Target','adapter-pick':'Adapter'}[name];state['modal']=None
                elif name in ('block-origin','block-local','block-target'):
                    state['point_adapter']=adapter({'block-origin':'create','block-local':'local','block-target':'target'}[name]);state['modal']='PointInput'
                elif name=='adapter-bounds':a['world']=[20.,4.] if a['target']=='BlockCreate' else [0.,0.]
                elif name=='block-create':save();revision();state['modal']=None
                elif name=='transform-apply':
                    save();op=p['operation'];changed=copy.deepcopy(state['selected'])
                    for o in changed:
                        c=o['object']['geometry']['Flash']['center'];x,y=c['x_mm'],c['y_mm']
                        if op['kind'] in ('move','duplicate'):c.update(x_mm=x+op['dx_mm'],y_mm=y+op['dy_mm'])
                        elif op['kind']=='rotate':b=op['pivot_mm'];c.update(x_mm=b['x_mm']-(y-b['y_mm']),y_mm=b['y_mm']+x-b['x_mm'])
                        elif op['axis']['kind']=='horizontal':c['y_mm']=2*op['axis']['coordinate_mm']-y
                        else:c['x_mm']=2*op['axis']['coordinate_mm']-x
                        if op['kind']=='duplicate':o['object']['object_id']='copy-'+o['object']['object_id']
                    doc=doc+copy.deepcopy(changed) if op['kind']=='duplicate' else copy.deepcopy(changed);state['selected']=changed;revision();state['point_transform']=None;state['modal']=None
                elif name=='adapter-apply':
                    a=state['point_adapter'];w=a['world'];world=dict(zip(('x_mm','y_mm'),w));target=a['target'];state['point_adapter']=None;state['modal']=None
                    if target=='Measure':measure.append(world);state['measure']={'a':measure[0],'b':measure[-1] if len(measure)>1 else None}
                    elif target=='TextReference':state['text_reference']={'enabled':True,'x':str(w[0]),'y':str(w[1])}
                    elif target=='ArrayBase':state['array_base']=world
                    elif target=='ArrayTarget':b=state['array_base'];state['array_pitch']=[str(w[0]-b['x_mm']),str(w[1]-b['y_mm'])]
                    elif target.startswith('BoardWorld'):state['board_world'][0]=w
                    elif target=='BlockCreate':state['block_origin']=[str(w[0]),str(w[1])];state['modal']='BlockCreate'
                    elif target.startswith('BlockLocal'):state['block_reference']=world;state['tool']='Block'
                    elif target.startswith('Grip'):
                        save();state['selected'][0]['object']['geometry']=copy.deepcopy(a['grip_preview']['geometry']);shape=a['grip_preview']['aperture_shape'];revision()
                    elif target=='BlockTarget':
                        save();state['selected']=[{'layer_id':'2','object':{'object_id':'instance','exposure':'dark','geometry':{'BlockInstance':{'transform':{'translation':a['block_translation']}}}}}];doc+=copy.deepcopy(state['selected']);revision()
            elif k=='text':
                if focus and focus.startswith(('B-','T-')):
                    p=state['point_transform'];which='base' if focus[0]=='B' else 'target'
                    if step['value']=='NaN':p[which]=None
                    else:p[which]['world'][0 if focus.endswith('X') else 1]=float(step['value']);p[which]['source']='Numeric'
                else:state['point_adapter']['world'][0 if focus.endswith('X') else 1]=float(step['value'])
            elif k=='point_click' and not step.get('outside'):
                pick=state['point_pick'];state['point_pick']=None;state['modal']='PointInput' if pick=='Adapter' else 'Move'
                if pick=='Adapter':state['point_adapter']['world']=copy.deepcopy(step['from'])
                else:state['point_transform']['base' if pick=='Base' else 'target']={'world':copy.deepcopy(step['from']),'source':'Feature(Midpoint)' if pick=='Base' else 'Raw'}
            elif k=='escape':
                if state['point_pick']:state['point_pick']=None;state['modal']='PointInput' if state['point_adapter'] else 'Move'
                else:state['modal']=None;state['point_transform']=None;state['point_adapter']=None
            elif k=='cancel_conflict' and step['cancel']!='ime':state['point_transform']=None;state['point_adapter']=None;state['modal']=None
            elif k=='undo':
                redo=(copy.deepcopy(doc),copy.deepcopy(state['selected']),copy.deepcopy(shape));doc,selection,shape=history.pop();state['selected']=selection;revision(-1)
            elif k=='redo':save();doc,selection,shape=redo;state['selected']=selection;revision()
            elif k=='feedback_move':
                n=step['sample'];state['snap_marker']={'kind':'Some(Nearest)','world':[7+n%8+.25*math.cos(.53),1+.25*math.sin(.53)]}
            resolve()
        for n,step in enumerate(plan):
            k=step['kind'];name=step.get('name');before=copy.deepcopy(state)
            pointer=k in ('widget','point_click') or k=='cancel_conflict' and step['via']=='button'
            def key(key,pressed=True,repeat=False):return {'kind':'key','key':key,'pressed':pressed,'repeat':repeat,'modifiers':{'command':False}}
            def event_rows(release=False):
                es=[]
                if pointer:
                    pos=[50.,50.]
                    if k=='point_click':cam=state['camera'];rect=state['canvas'];w=step['from'];pos=[(rect[0]+rect[2])/2+(w[0]-cam[0])*cam[2],(rect[1]+rect[3])/2-(w[1]-cam[1])*cam[2]]
                    es=[{'kind':'button','button':'Primary','pressed':not release,'position':pos,'modifiers':{}}]
                if k=='text':es=[dict(key('A',b),modifiers={'command':True}) for b in (True,False)]+[{'kind':'text','value':step['value']}]
                if k=='escape':es=[key('Escape')]
                if k=='cancel_conflict' and (release or not pointer):
                    if step['cancel']=='escape':es += [key('Escape',repeat=step['repeat']),key('Enter',repeat=step['repeat'])]
                    elif step['cancel']=='blur':es += [{'kind':'focus','focused':False}]
                    elif step['cancel']=='gone':es += [{'kind':'gone'}]
                    else:es += [{'kind':'ime','debug':'Preedit("SYNTHETIC")'},key('Enter')]
                if k=='feedback_move':
                    cam=state['camera'];rect=state['canvas'];w=step['from'];es=[{'kind':'move','position':[(rect[0]+rect[2])/2+(w[0]-cam[0])*cam[2],(rect[1]+rect[3])/2-(w[1]-cam[1])*cam[2]]}]
                return es
            if pointer:
                fid=len(frames)+1;d={'frame_id':fid,'step':n,'t0_ns':fid*1000,'focused':True,'events':event_rows(),'camera':state['camera'],'canvas':state['canvas'],'widget':{'enabled':True,'frame':fid-1,'rect':[0.,0.,100.,100.]}}
                deliveries.append(d);paints.append({'frame_id':fid,'paint_ns':fid*1000+20});frames.append({'frame_id':fid,'step':n,'state':copy.deepcopy(state),'painted':True,'gpu_completed':True,'delivered_input':copy.deepcopy(d),'input':{'events':event_rows()},'input_ns':fid*1000+10,'completed_ns':fid*1000+30})
            apply_step(step)
            fid=len(frames)+1;events=event_rows(True);d={'frame_id':fid,'step':n,'t0_ns':fid*1000,'focused':not(k=='cancel_conflict' and step['cancel']=='blur'),'events':events,'camera':state['camera'],'canvas':state['canvas'],'widget':{'enabled':True,'frame':fid-1,'rect':[0.,0.,100.,100.]}}
            deliveries.append(d);paints.append({'frame_id':fid,'paint_ns':fid*1000+20});f={'frame_id':fid,'step':n,'state':copy.deepcopy(state),'painted':True,'gpu_completed':True,'delivered_input':copy.deepcopy(d),'input':{'events':events},'input_ns':fid*1000+10,'completed_ns':fid*1000+30};frames.append(f)
            if k=='widget' and name=='transform-apply':actions.append({'frame_id':fid,'detail':{'kind':'point_apply','operation':before['point_transform']['operation']}})
            import gzip
            files[prefix+f'/step-{n:02}.json']=blob({'input':step,'state':state,'snapshot':snapshot(),'completed_frame':f});files[prefix+f'/step-{n:02}.ppm.gz']=gzip.compress(b'P6\n1 1\n255\n\0\0\0',mtime=0)
            records.append({'step':n,'path':f'step-{n:02}.json'})
            if k=='feedback_move':
                target=state['snap_marker']['world'];rect=state['canvas'];pixel=[round(((rect[0]+rect[2])/2+(target[0]-10)*40)*2),round(((rect[1]+rect[3])/2-(target[1]-1)*40)*2)];label=f'feedback-{step["sample"]:02}-{fid}';rgb=bytearray(25*25*3)
                for yy in range(25):
                    for xx in range(25):
                        if (11<=yy<=13 and 5<=xx<=19) or (11<=xx<=13 and 5<=yy<=13):rgb[(yy*25+xx)*3:(yy*25+xx)*3+3]=bytes((94,235,205))
                ppm=b'P6\n25 25\n255\n'+rgb;files[prefix+'/'+label+'.ppm']=ppm
                row={'sample':step['sample'],'step':n,'frame_id':fid,'callback_frame_id':fid+1,'label':label,'state':copy.deepcopy(state),'pixel':pixel,'crop_origin_px':[pixel[0]-12,pixel[1]-12],'t0_ns':d['t0_ns'],'t1_ns':fid*1000+50,'latency_ms':.00005,'crop':label+'.ppm','crop_sha256':digest(ppm)};feedback.append(row);surface.append({key:row[key] for key in ('label','callback_frame_id','t1_ns','crop_sha256')})
        # Terminal input is independently present but not part of the frame capture.
        deliveries.append({'frame_id':len(frames)+1,'step':len(plan),'t0_ns':(len(frames)+1)*1000,'events':[]})
        shard=blob(frames);files[prefix+'/frames-000.json']=shard
        obs={'stage':'S5-I2-B','profile':'release','evidence_version':3,'commit':commit,'binary_sha256':binary,'error':None,'adapter':'SYNTHETIC Metal','measurement_scope':'SYNTHETIC TEST FIXTURE ONLY synthetic egui','request':request,'frame_files':[{'path':'frames-000.json','count':len(frames),'sha256':digest(shard)}],'records':records,'feedback':feedback}
        ledger={'run_id':run_id,'capture_start':1,'capture_end':len(frames),'terminal_pending_frame':len(frames)+1,'input_ids':list(range(1,len(frames)+2)),'update_ids':list(range(1,len(frames)+2)),'paint_ids':list(range(1,len(frames)+1)),'paint_count':len(frames),'delivered_inputs':deliveries,'paint_records':paints,'surface_callbacks':surface,'actions':actions}
        for name,v in [('request.json',request),('observations.json',obs),('capture-ledger.json',ledger),('runner.json',{'exit_code':0,'error':None,'binary_sha256':binary})]:files[prefix+'/'+name]=blob(v)
        indices['native' if role=='functional' else 'feedback'].append({'run_id':run_id,'path':prefix})
    for name,rows in indices.items():files[name+'/index.json']=blob(rows)
    return files
