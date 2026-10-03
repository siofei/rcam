"""Real-package semantic probes. Copies evidence; rehashes mutations; restores each case.
Run after fresh extraction. Neither input Source nor input Evidence is modified.
"""
import argparse,copy,hashlib,json,shutil,tempfile
from pathlib import Path
import verify_batch_drag_native as native
import verify_batch_drag_evidence as package

def write(p,v):p.write_text(json.dumps(v,ensure_ascii=False,indent=2)+'\n')
def preview(r):return [f for f in r['frames'] if f['phase']==6]
def event(r,label):return next(e for e in r['events'] if e['label']==label)
def omit(r,which):
    fs=preview(r)
    removed=fs[101:] if which=='tail' else (fs[len(fs)//2:len(fs)//2+3] if which=='chunk' else [fs[{'first':0,'middle':len(fs)//2,'last':-1}[which]]])
    ids={f['id'] for f in removed}
    r['frames']=[f for f in r['frames'] if f['id'] not in ids]
def renumber(r):
    ids={f['id']:i for i,f in enumerate(r['frames'],1)}
    for f in r['frames']:f['id']=ids[f['id']]
    for a,b in zip(r['frames'],r['frames'][1:]):b['frame_interval_ms']=(b['input_ns']-a['input_ns'])/1e6
    r['last_observed_frame_id']=len(r['frames'])
    for e in r['events']:
        e['frame_id']=ids.get(e['frame_id'],e['frame_id'])
        if isinstance(e['data'],dict) and 'visible_frame_id' in e['data']:e['data']['visible_frame_id']=ids[e['data']['visible_frame_id']]
        if e['label']=='screenshot-request':e['data']['frame_id']=ids[e['data']['frame_id']]
        if e['label']=='screenshot':e['data']['request']['frame_id']=ids[e['data']['request']['frame_id']]
def corrupt(r,case):
    fs=preview(r)
    if case.startswith('drop-'):
        omit(r,case.split('-')[1]);
        if case.endswith('renumber'):renumber(r)
    elif case=='rephase-middle':fs[len(fs)//2]['phase']=7
    elif case=='zero-preview':
        for f in fs:f['state']['delta']={'x_mm':0.,'y_mm':0.};f['paint_delta']=[0.,0.,0.,0.]
    elif case=='missing-delta':fs[len(fs)//2]['state']['delta']=None
    elif case=='stationary-input-and-preview':
        press=event(r,'press')['data']['position']
        for f in fs:f['injected']['pointer']=press;f['gesture']['last']=press;f['state']['delta']={'x_mm':0.,'y_mm':0.};f['paint_delta']=[0.,0.,0.,0.]
    elif case=='wrong-camera':
        for f in r['frames']:f['injected']['view']['scale']=1.;f['view']['scale']=1.
    elif case=='unpainted-completion':
        for f in r['frames']:
            if f['state']['revision'] in ('1','2','3'):f['painted']=False;f['gpu_completed']=False
    elif case in ('completion-gpu-failure','cancel-unpainted'):
        e=event(r,'commit-complete' if case=='completion-gpu-failure' else 'cancel-complete')
        f=next(f for f in r['frames'] if f['id']==e['data']['visible_frame_id']);f['gpu_completed']=False
    elif case.startswith('worker-'):
        w=next(w for w in r['worker'] if w['action']=='drag-move')
        if case=='worker-zero-elapsed':w['elapsed_ms']=0.
        elif case=='worker-elapsed-disagrees':w['elapsed_ms']=w['elapsed_ms']/2
        elif case=='worker-error':w['error']={'code':'RESOURCE_LIMIT'}
        elif case=='worker-count':w['selected_count']=1
        elif case=='worker-state':w['batch']['state']['revision']='99'
        elif case=='worker-finished-too-late':w['batch']['finished_ns']=10**18
        elif case=='worker-sequence':event(r,'commit-complete')['data']['worker_sequence']=99999
    elif case=='completion-too-early':event(r,'commit-complete')['data']['visible_completed_ns']=0
    elif case=='new-project-model-only':event(r,'cancel-complete')['data']['visible_frame_id']=event(r,'release')['frame_id']
    elif case=='cancel-worker-error':next(w for w in r['worker'] if w['action']=='new-project')['error']='RESOURCE_LIMIT'
    elif case=='snapshot-history':r['snapshots'][1]['state']['undo']=2
    else:raise ValueError('unknown probe '+case)

CASES=[('drop-'+p,'move') for p in ('first','middle','last','tail','tail-renumber','first-renumber','middle-renumber','last-renumber','chunk-renumber')]+[(c,'move') for c in ('rephase-middle','zero-preview','missing-delta','stationary-input-and-preview','wrong-camera','unpainted-completion','completion-gpu-failure','worker-error','worker-zero-elapsed','worker-elapsed-disagrees','worker-count','worker-state','worker-finished-too-late','worker-sequence','completion-too-early','snapshot-history')]+[('cancel-unpainted','escape'),('new-project-model-only','new-project'),('cancel-worker-error','new-project')]

def main():
    p=argparse.ArgumentParser(description=__doc__);p.add_argument('--source',type=Path,required=True);p.add_argument('--evidence',type=Path,required=True);p.add_argument('--out',type=Path,required=True);a=p.parse_args()
    a.out.mkdir(parents=True,exist_ok=False);native.PROTOCOL=a.source/'fixtures/synthetic/s5m2b/protocol.json'
    original=package.verify(a.source,a.evidence,True);results=[]
    with tempfile.TemporaryDirectory(prefix='probes-',dir=a.out) as temp:
        evidence=Path(temp)/'Evidence';shutil.copytree(a.evidence,evidence)
        manifest=evidence/'EVIDENCE_MANIFEST.sha256';original_manifest=manifest.read_bytes()
        index_path=evidence/'native/native-index.json';index_bytes=index_path.read_bytes();rows=json.loads(index_bytes)
        for case,mode in CASES:
            selected=5000 if mode=='move' else 1000
            row_index=next(i for i,row in enumerate(rows) if row['round']==1 and row['selected']==selected and row['mode']==mode)
            directory=evidence/'native'/rows[row_index]['directory'];report=directory/'observations.json';saved=report.read_bytes()
            try:
                r=json.loads(saved);corrupt(r,case);write(report,r)
                native_reason=None
                try:summary=native.verify(directory)
                except (ValueError,KeyError,TypeError,OSError) as exc:native_reason=str(exc)
                else:
                    updated=copy.deepcopy(rows);updated[row_index]['summary']=summary;write(index_path,updated)
                manifest.write_text(''.join(hashlib.sha256(f.read_bytes()).hexdigest()+'  '+f.relative_to(evidence).as_posix()+'\n' for f in sorted(evidence.rglob('*')) if f.is_file() and f!=manifest))
                package.coverage(evidence,'EVIDENCE_MANIFEST.sha256')
                try:package.verify(a.source,evidence,True)
                except (ValueError,KeyError,TypeError,OSError) as exc:
                    require=native_reason is not None
                    if not require:raise AssertionError('native accepted although package rejected: '+case)
                    result={'case':case,'mode':mode,'manifest_integrity':'PASS','native':'REJECTED','package':'REJECTED','native_reason':native_reason,'package_reason':str(exc)}
                else:raise AssertionError('remanifested package accepted '+case)
                results.append(result);write(a.out/'results.json',{'baseline':original['result'],'checks':results});print(json.dumps(result),flush=True)
            finally:
                report.write_bytes(saved);index_path.write_bytes(index_bytes);manifest.write_bytes(original_manifest)
        restored=package.verify(a.source,evidence,True)
        write(a.out/'results.json',{'baseline':original['result'],'checks':results,'restored':restored['result']})
    return 0
if __name__=='__main__':raise SystemExit(main())
