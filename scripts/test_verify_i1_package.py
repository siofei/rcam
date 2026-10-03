"""Re-manifest real on-disk I1 evidence corruptions; never modify the supplied package."""
import argparse,copy,hashlib,json,shutil,tempfile
from pathlib import Path
import verify_i1_evidence as package

def dump(p,v):p.write_text(json.dumps(v,ensure_ascii=False,indent=2)+'\n')
def main():
    p=argparse.ArgumentParser(description=__doc__);p.add_argument('--source',required=True,type=Path);p.add_argument('--evidence',required=True,type=Path);p.add_argument('--out',required=True,type=Path);a=p.parse_args()
    valid=package.verify(a.source,a.evidence,True)
    scratch=Path(tempfile.mkdtemp(prefix='rcam-i1-corruption-',dir=a.out.resolve().parent));e=scratch/'Evidence';shutil.copytree(a.evidence,e)
    close=json.loads((e/'CLOSEOUT.json').read_text());index=e/close['native_index'];rows=json.loads(index.read_text());raw=e/'native'/rows[0]['directory'];obs=raw/'observations.json';ledger=raw/'capture-ledger.json';manifest=e/'EVIDENCE_MANIFEST.sha256'
    originals={p:p.read_bytes() for p in (obs,ledger,index,manifest)}
    report=json.loads(originals[obs]);log=json.loads(originals[ledger]);results=[]
    def frames(d):return [f for f in d['frames'] if f['step']==6]
    def off_canvas(d,l):
        for f in frames(d):
            for ev in f['input']['events']:
                if 'position' in ev:ev['position']=[9999.,9999.]
            f['input']['position']=[9999.,9999.]
    def mode(d,l):
        for f in frames(d):
            for g in (f['gesture_after'],f['input_before']['gesture']):
                if g:g['mode']='Replace'
    def release_mod(d,l):
        for f in frames(d):
            for ev in f['input']['events']:
                if ev['kind']=='button' and not ev['pressed']:ev['modifiers']['shift']=False
    def action_world(d,l):
        for v in l['actions']:
            if v['detail']['kind']=='select' and v['detail']['mode']=='Remove':v['detail']['world']=[99.,99.];break
    def thin(d,l):d['frames']=[f for f in d['frames'] if f['input']['events']]
    def lost_gesture(d,l):
        for f in frames(d):
            if any(e['kind']=='button' and e['pressed'] for e in f['input']['events']):f['gesture_after']=None;break
    def wrong_sequence(d,l):
        for f in frames(d):
            if f['gesture_after'] is not None:f['sequence']+=1;break
    def delete_wait(d,l):
        target=next(f for f in frames(d) if not f['input']['events'])
        d['frames'].remove(target)
    def missing_recovery_witness(d,l):
        for f in d['frames']:
            f['input_before'].pop('recovery_collision_due',None)
    mutations={
        'raw_pointer_off_canvas':off_canvas,
        'shift_gesture_replace':mode,
        'modifier_press_lost_gesture':lost_gesture,
        'UI_action_sequence_contradiction':wrong_sequence,
        'delete_Shift_wait_frame':delete_wait,
        'release_modifiers_wrong':release_mod,
        'selection_action_world_wrong':action_world,
        'delete_first_frame':lambda d,l:d['frames'].pop(0),
        'delete_middle_frame':lambda d,l:d['frames'].pop(len(d['frames'])//2),
        'delete_last_frame':lambda d,l:d['frames'].pop(),
        'retain_events_only':thin,
        'duplicate_frame':lambda d,l:d['frames'].append(copy.deepcopy(d['frames'][0])),
        'missing_input_hook':lambda d,l:l['input_ids'].pop(5),
        'missing_update':lambda d,l:l['update_ids'].pop(5),
        'missing_paint':lambda d,l:l['paint_ids'].pop(5),
        'wrong_paint_count':lambda d,l:l.__setitem__('paint_count',l['paint_count']-1),
        'wrong_terminal':lambda d,l:l.__setitem__('terminal_pending_frame',l['terminal_pending_frame']-1),
        'missing_forced_recovery_collision':missing_recovery_witness,
    }
    def remanifest(changed):
        updates={p.relative_to(e).as_posix():hashlib.sha256(p.read_bytes()).hexdigest() for p in changed}
        manifest.write_text(''.join(updates.get(n,h)+'  '+n+'\n' for h,n in (line.split('  ',1) for line in originals[manifest].decode().splitlines())))
    def reject(name):
        try:package.verify(a.source,e,True)
        except (ValueError,KeyError,IndexError,TypeError) as error:results.append({'name':name,'rejected':True,'reason':str(error),'remanifested':True})
        else:raise AssertionError('accepted re-manifested corruption: '+name)
        print(json.dumps(results[-1]),flush=True)
    for name,mutate in mutations.items():
        for p,data in originals.items():p.write_bytes(data)
        d,l=copy.deepcopy(report),copy.deepcopy(log);mutate(d,l);dump(obs,d);dump(ledger,l)
        rr=copy.deepcopy(rows);rr[0]['summary']['frames']=len(d['frames']);dump(index,rr)
        remanifest([obs,ledger,index]);reject(name)
    diagnostic_rows=json.loads((e/close['diagnostic_index']).read_text())
    diagnostic_cases=[('early_release','early_release_unfocused_press','focus'),('early_release','early_release_select_offscreen','point'),('foreign_move','foreign_move_unfocused_press','focus'),('pointer_gone','pointer_gone_unfocused_press','focus'),('early_release','early_release_select_camera_wrong','camera'),('early_release','early_release_select_canvas_wrong','canvas'),('early_release','early_release_select_ppp_wrong','ppp'),('early_release','early_release_navigation_wrong','epoch'),('foreign_move','foreign_move_probe_world_wrong','probe_world'),('pointer_gone','pointer_gone_button_modifiers_wrong','modifiers')]
    for mode,name,mutation in diagnostic_cases:
        for p,data in originals.items():p.write_bytes(data)
        raw=e/'native'/next(r['directory'] for r in diagnostic_rows if r['mode']==mode);o=raw/'observations.json';lg=raw/'capture-ledger.json'
        if o not in originals:originals[o]=o.read_bytes();originals[lg]=lg.read_bytes()
        d=json.loads(originals[o]);l=json.loads(originals[lg]);ff=[f for f in d['frames'] if f['step']==6]
        press=next(f for f in ff if any(ev['kind']=='button' and ev['pressed'] for ev in f['input']['events']))
        if mutation=='focus':press['input']['focused']=False
        elif mutation=='modifiers':next(ev for ev in press['input']['events'] if ev['kind']=='button')['modifiers']['shift']=False
        elif mutation=='probe_world':next(a for a in l['actions'] if a['frame_id']==press['frame_id'] and a['detail']['kind']=='probe')['detail']['world']=[99.,99.]
        else:
            ids={f['frame_id'] for f in ff};action=next(a['detail'] for a in l['actions'] if a['frame_id'] in ids and a['detail']['kind']=='select')
            if mutation=='point':action['point']=[9999.,9999.]
            elif mutation=='camera':action['camera']=[9.,9.,90.]
            elif mutation=='canvas':action['canvas']=[0.,0.,100.,100.]
            elif mutation=='ppp':action['ppp']+=1.
            elif mutation=='epoch':action['navigation_epoch']+=1
        dump(o,d);dump(lg,l);remanifest([o,lg]);reject(name)
    for p,data in originals.items():p.write_bytes(data)
    restored=package.verify(a.source,e,True)
    dump(a.out,{'valid':valid,'negative_cases':results,'restored':restored,'scratch':str(scratch),'original_evidence_untouched':True})
    print(json.dumps({'cases':len(results),'all_rejected':True,'report':str(a.out)}));return 0
if __name__=='__main__':raise SystemExit(main())
