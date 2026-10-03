"""Verify deterministic Shift cancellation/early-release native inputs, without historical attribution."""
import argparse,json,math
from pathlib import Path
from run_i1_native import steps
from verify_i1_native import read,require,sha,content,selected,screen,near_point,verify_capture,verify_button_context,verify_gesture_focus,verify_select_context,verify_probe_context

MODES=('foreign_move','pointer_gone','early_release')
def verify(directory,source_root=None,binary=None):
    directory=Path(directory);report=read(directory/'observations.json');request=read(directory/'request.json');runner=read(directory/'runner.json');ledger=read(directory/'capture-ledger.json')
    mode=request['diagnostic'];require(mode in MODES and request['stress'] is False and request['steps']==steps(False),'diagnostic protocol')
    require(report['request']==request and report['schema_version']==2 and report['stage']=='S5-I1' and report['profile']=='release','diagnostic identity')
    require(report['error'] is None and runner['error'] is None and runner['exit_code']==0,'diagnostic process failed')
    require(report['binary_sha256']==request['binary_sha256']==runner['binary_sha256'],'diagnostic binary binding')
    require('metal' in json.dumps(report['adapter']).lower(),'native Metal required')
    if binary:require(sha(Path(binary))==report['binary_sha256'],'external diagnostic binary hash')
    if source_root:
        source_root=Path(source_root);require(sha(source_root/'MANIFEST.sha256')==request['source_manifest_sha256'],'diagnostic source binding')
        for name,digest in request['fixtures'].items():require(sha(source_root/'fixtures/synthetic/s5i1'/name)==digest,'diagnostic fixture binding')
    require(len(report['records'])==46,'diagnostic step coverage')
    require(read(directory/'video-command.json')['exit_code']==0 and (directory/'native-interaction.mov').stat().st_size>10000,'diagnostic video')
    verify_capture(report,ledger)
    frames=[f for f in report['frames'] if f['step']==6];require(frames,'missing Shift frames')
    events=[(f,e) for f in frames for e in f['input']['events']];buttons=[(f,e) for f,e in events if e['kind']=='button']
    require(len(buttons)==2 and [e['pressed'] for f,e in buttons]==[True,False],'diagnostic press/release')
    press,release=buttons[0][0],buttons[1][0];require(press['frame_id']<release['frame_id'],'diagnostic pointer order')
    point=screen(press['state'],[0.,0.]);mods={'alt':False,'ctrl':False,'shift':True,'mac_cmd':False,'command':False}
    for f,e in buttons:
        verify_button_context(f,e,[0.,0.],mods)
    require(press['gesture_after'] is not None and press['gesture_after']['mode']=='Remove' and not press['gesture_after']['confirmed'] and not press['gesture_after']['moved'],'diagnostic armed Remove')
    require(all(not f['grip_after'] and not f['input_before']['grip'] for f in frames),'diagnostic entered Grip')
    foreign_point=[point[0]+4.,point[1]]
    previous=None
    for f in frames:
        verify_gesture_focus(f,allow_gone=mode=='pointer_gone')
        require(all(f['state'][k]==press['state'][k] for k in ('camera','canvas','ppp','navigation_epoch')),'diagnostic input context changed')
        for e in f['input']['events']:
            if e['kind']=='move':
                expected=foreign_point if mode=='foreign_move' and press['frame_id']<f['frame_id']<release['frame_id'] else point
                near_point(e['position'],expected,'diagnostic movement')
            require(e['kind']!='key','unexpected diagnostic key cancellation')
        allowed=[point,foreign_point] if mode=='foreign_move' else [point]
        require(f['input']['position'] is not None and any(math.dist(f['input']['position'],p)<=0.0002 for p in allowed),'diagnostic held pointer context')
        for g in (f['gesture_after'],f['input_before']['gesture']):
            if g:
                require(g['mode']=='Remove','diagnostic mode changed');near_point(g['start'],point,'diagnostic start')
                require(any(math.dist(g['last'],p)<=0.0002 for p in allowed) and not g['box_select'] and g['object_drag'] is None,'diagnostic gesture context changed')
        if f['gesture_after']:near_point(f['gesture_after']['last'],f['input']['position'],'diagnostic gesture/pointer')
        if previous and f['input_before']['gesture'] and previous['gesture_after']:
            near_point(f['input_before']['gesture']['last'],previous['gesture_after']['last'],'diagnostic previous gesture')
        previous=f
    before=read(directory/'step-05.json');after=read(directory/'step-06.json');require(content(before['snapshot'])==content(after['snapshot']) and before['state']['info']==after['state']['info'],'diagnostic mutated manufacturing/history/dirty')
    complete=after['completed_frame'];require(complete in frames and complete['gpu_completed'] and complete['painted'] and complete['frame_id']>release['frame_id'] and complete['state']==after['state'],'diagnostic completion')
    ids={f['frame_id'] for f in frames};actions=[a for a in ledger['actions'] if a['frame_id'] in ids]
    probes=[a for a in actions if a['detail']['kind']=='probe'];choices=[a for a in actions if a['detail']['kind']=='select']
    require(len(probes)==1 and probes[0]['frame_id']==press['frame_id'],'diagnostic probe');verify_probe_context(probes[0]['detail'],press['state'],[0.,0.])
    require([a['detail']['kind'] for a in actions]==(['probe','select'] if mode=='early_release' else ['probe']),'diagnostic illegal accepted action sequence')
    if mode=='foreign_move':
        foreign=[(f,e) for f,e in events if e['kind']=='move' and e['position']!=point]
        require(len(foreign)==1,'foreign motion count');f,e=foreign[0];near_point(e['position'],[point[0]+4.,point[1]],'foreign move injection')
        require(press['frame_id']<f['frame_id']<release['frame_id'] and f['gesture_after']['moved'] and f['gesture_after']['confirmed'],'foreign move threshold/time')
        require(4*press['state']['ppp']>=4,'foreign move physical threshold')
        require(release['input_before']['gesture']['moved'],'motion latch lost before release')
    elif mode=='pointer_gone':
        gone=[f for f,e in events if e['kind']=='gone'];require(len(gone)==1 and press['frame_id']<gone[0]['frame_id']<release['frame_id'],'PointerGone timing')
        require(gone[0]['gesture_after'] is None and release['input_before']['gesture'] is None,'PointerGone failed to cancel')
    else:
        require(release['input_before']['gesture'] is not None and not release['input_before']['gesture']['confirmed'],'release was not before UI confirmation')
        require(len(choices)==1 and release['frame_id']<=choices[0]['frame_id']<=complete['frame_id'],'early release selection action')
        verify_select_context(choices[0]['detail'],press['state'],[0.,0.],'Remove')
    if mode=='early_release':require(selected(after['state'])==[],'early release did not remove selected object')
    else:require(not choices and selected(after['state'])==selected(before['state']),'cancelled Shift changed selection/emitted select')
    require(release['gesture_after'] is None,'release retained stale gesture')
    return {'schema_version':2,'stage':'S5-I1','result':'DIAGNOSTIC_PASS','mode':mode,'run_id':request['run_id'],'frames':len(report['frames']),'binary_sha256':report['binary_sha256'],'source_manifest_sha256':request['source_manifest_sha256'],'scope':'Current deterministic input/selection/manufacturing cancellation contract; historic failure remains non-attributable'}
if __name__=='__main__':
    p=argparse.ArgumentParser(description=__doc__);p.add_argument('directory',type=Path);p.add_argument('--source-root',type=Path);p.add_argument('--binary',type=Path);a=p.parse_args();print(json.dumps(verify(a.directory,a.source_root,a.binary),indent=2))
