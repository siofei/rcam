"""Mutate real I1 native evidence in memory; originals are never modified."""
import argparse,copy,json,os,tempfile
from pathlib import Path
from unittest.mock import patch
import verify_i1_native as verifier
from verify_i1_evidence import native_directory

def main():
    p=argparse.ArgumentParser(description=__doc__);p.add_argument('raw',type=Path,nargs='?',default=os.environ.get('RCAM_I1_NATIVE_REFERENCE'));a=p.parse_args()
    if a.raw is None:p.error('real native reference required')
    raw=Path(a.raw).resolve()
    # Actual regression: the shared safe() validates files, not directories.
    guard_root=Path(tempfile.mkdtemp(prefix='rcam-i1-evidence-path-'))
    report=guard_root/'native/run/raw/observations.json';report.parent.mkdir(parents=True);report.write_text('{}')
    assert native_directory(guard_root,'run/raw')==report.parent
    guards=['existing-report-directory']
    for path in ('missing/raw','../outside'):
        try:native_directory(guard_root,path)
        except ValueError:guards.append('rejected:'+path)
        else:raise AssertionError('accepted unsafe/missing native report path')
    valid=verifier.verify(raw)
    source={p.name:json.loads(p.read_text()) for p in raw.glob('*.json')}
    def mutate_step(d,n,change):
        item=d[f'step-{n:02}.json'];change(item);frame=item['completed_frame']
        if frame:
            frame['state']=copy.deepcopy(item['state'])
            for i,f in enumerate(d['observations.json']['frames']):
                if f['frame_id']==frame['frame_id']:d['observations.json']['frames'][i]=copy.deepcopy(frame)
    def omit_second_layer_move(d):
        old=d['step-15.json'];mutate_step(d,16,lambda x:x['snapshot']['layers'].__setitem__(1,copy.deepcopy(old['snapshot']['layers'][1])))
    mutations={
        'wrong-reimport-provenance':lambda d:mutate_step(d,45,lambda x:x['state']['layers'][0]['provenance'].__setitem__('imported_sha256','0'*64)),
        'wrong-reimport-aperture':lambda d:mutate_step(d,45,lambda x:x['snapshot']['apertures'][0]['shape']['Circle'].__setitem__('diameter_mm',3.0)),
        'delete-middle-frame':lambda d:d['observations.json']['frames'].pop(len(d['observations.json']['frames'])//2),
        'wrong-paint-count':lambda d:d['capture-ledger.json'].__setitem__('paint_count',0),
        'missing-step':lambda d:d['observations.json']['records'].pop(),
        'wrong-binary':lambda d:d['observations.json'].__setitem__('binary_sha256','0'*64),
        'failed-video':lambda d:d['video-command.json'].__setitem__('exit_code',-15),
        'failed-process':lambda d:d['runner.json'].__setitem__('exit_code',1),
        'partial-move':omit_second_layer_move,
        'wrong-cycle':lambda d:mutate_step(d,4,lambda x:x['state'].__setitem__('selected',copy.deepcopy(d['step-03.json']['state']['selected']))),
        'wrong-revision':lambda d:mutate_step(d,16,lambda x:x['state']['info'].__setitem__('revision','900')),
        'undo-not-exact':lambda d:mutate_step(d,17,lambda x:x['snapshot']['layers'][0]['objects'].pop()),
        'lost-selection':lambda d:mutate_step(d,16,lambda x:x['state'].__setitem__('selected',[])),
        'wrong-frame-phase':lambda d:mutate_step(d,16,lambda x:x['completed_frame'].__setitem__('phase',1)),
        'not-gpu-complete':lambda d:mutate_step(d,16,lambda x:x['completed_frame'].__setitem__('gpu_completed',False)),
        'missing-input':lambda d:[f['input'].__setitem__('events',[]) for f in d['observations.json']['frames'] if f['step']==4],
    }
    results=[]
    for name,mutation in mutations.items():
        altered=copy.deepcopy(source);mutation(altered)
        def read(path):return altered[Path(path).name]
        try:
            with patch.object(verifier,'read',read):verifier.verify(raw)
        except (ValueError,KeyError,IndexError) as e:results.append({'name':name,'rejected':True,'reason':str(e)})
        else:raise AssertionError('accepted corruption: '+name)
    print(json.dumps({'valid':valid,'package_path_guards':guards,'negative_cases':results},indent=2));return 0
if __name__=='__main__':raise SystemExit(main())
