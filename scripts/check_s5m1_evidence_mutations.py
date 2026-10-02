"""Reject mutations of actual native evidence; never change the source records."""
import argparse,copy,hashlib,json,pathlib
import verify_s5m1_native as v

def check(selection,navigation):
    s=json.loads((selection/'native-observations.json').read_text());ids=json.loads((selection/'marquee-selected-ids.json').read_text());n=json.loads((navigation/'native-observations.json').read_text())
    v.verify_selection(s,ids);v.verify_navigation(n)
    results=[]
    def trial(name,base,change,verify):
        row=copy.deepcopy(base);change(row)
        try:verify(row)
        except (ValueError,KeyError,IndexError) as e:results.append(dict(case=name,status='REJECTED',reason=str(e)))
        else:results.append(dict(case=name,status='ACCEPTED'))
    for label in ['point-highlight','marquee-highlight']:
        for key,value in [('document_id','wrong-document'),('revision','stale'),('scene_serial',0),('scene_count',0),('manufacturing_count',0),('selected_count',0),('selected_primary',None),('focused',False),('painted',False),('gpu_completed',False),('selected_primary_in_scene',False),('busy',True),('display_pending',True),('blocked','RESOURCE_LIMIT'),('display_error','DISPLAY_PRECISION'),('error','ERROR'),('ui_error','ERROR')]:
            trial(f'{label}:{key}',s,lambda r,k=key,x=value:v.labels(r,label)[0]['frame'].__setitem__(k,x),lambda r:v.verify_selection(r,ids))
        for key,value in [('scene_count',0),('manufacturing_count',0),('selected_count',0),('display_pending',True),('blocked','ERROR')]:
            def both(r,k=key,x=value):
                row=v.labels(r,label)[0];row['state'][k]=row['frame'][k]=x
            trial(f'{label}:state-and-frame:{key}',s,both,lambda r:v.verify_selection(r,ids))
    trial('marquee:matched-partial-scene',s,lambda r:[v.labels(r,'marquee-highlight')[0][k].update(scene_count=99999) for k in ['state','frame']],lambda r:v.verify_selection(r,ids))
    trial('point:before-wrong-document',s,lambda r:v.labels(r,'point-highlight')[0]['before']['state'].update(document_id='wrong'),lambda r:v.verify_selection(r,ids))
    changes=[('only-last-two',lambda r:r.update(frames=r['frames'][-2:])),('delete-prefix',lambda r:r.update(frames=r['frames'][1:])),('delete-tail',lambda r:r['frames'].pop()),('delete-middle',lambda r:r['frames'].pop(len(r['frames'])//2)),('reverse',lambda r:r['frames'].reverse()),('fake-elapsed',lambda r:r['frames'][0].update(elapsed_seconds=59.999)),('fake-interval',lambda r:r['frames'][10].update(frame_interval_ms=.001)),('missing-interval',lambda r:r['frames'][-1].update(frame_interval_ms=None)),('nan-interval',lambda r:r['frames'][10].update(frame_interval_ms=float('nan'))),('wrong-input',lambda r:r['frames'][10].update(input_monotonic_ns=0)),('wrong-observed',lambda r:r['frames'][10].update(observed_monotonic_ns=0)),('wrong-ack',lambda r:r['frames'][10].update(acknowledged_monotonic_ns=0)),('missing-start-id',lambda r:v.labels(r,'navigation-start')[0].pop('frame_id')),('missing-end-id',lambda r:v.labels(r,'navigation-end')[0].pop('frame_id')),('fake-boundary-elapsed',lambda r:v.labels(r,'navigation-end')[0].update(elapsed_seconds=0)),('fake-end-time',lambda r:v.labels(r,'navigation-end')[0].update(input_monotonic_ns=0)),('fake-origin',lambda r:v.labels(r,'navigation-start')[0].update(phase_origin_ns=0)),('duplicate-start',lambda r:r['records'].append(copy.deepcopy(v.labels(r,'navigation-start')[0])))]
    for name,change in changes:trial('navigation:'+name,n,change,v.verify_navigation)
    # Even forged boundary IDs cannot hide truncation while real clocks remain.
    def prefix_ids(r):
        r['frames']=r['frames'][-2:];v.labels(r,'navigation-start')[0]['frame_id']=r['frames'][0]['frame_id']-1
    def tail_ids(r):
        r['frames']=r['frames'][:-1];v.labels(r,'navigation-end')[0]['frame_id']=r['frames'][-1]['frame_id'];v.labels(r,'navigation-end')[0]['input_monotonic_ns']=r['frames'][-1]['input_monotonic_ns']
    trial('navigation:prefix-with-forged-boundary-id',n,prefix_ids,v.verify_navigation);trial('navigation:tail-with-forged-boundary-id',n,tail_ids,v.verify_navigation)
    return {'schema_version':2,'stage':'S5-M1','unmutated':'PASS','input_sha256':{str(p):hashlib.sha256(p.read_bytes()).hexdigest() for p in [selection/'native-observations.json',navigation/'native-observations.json']},'mutations':results}

def main():
    p=argparse.ArgumentParser(description=__doc__);p.add_argument('--selection',type=pathlib.Path,required=True);p.add_argument('--navigation',type=pathlib.Path,required=True);p.add_argument('--output',type=pathlib.Path,required=True);a=p.parse_args();r=check(a.selection,a.navigation);a.output.write_text(json.dumps(r,ensure_ascii=False,indent=2)+'\n');print(f"{sum(x['status']=='REJECTED' for x in r['mutations'])}/{len(r['mutations'])} mutations rejected");raise SystemExit(any(x['status']=='ACCEPTED' for x in r['mutations']))
if __name__=='__main__':main()
