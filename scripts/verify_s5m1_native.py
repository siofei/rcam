"""Independent S5-M1 evidence verification; no inferred native PASS from logs."""
import argparse, hashlib, json, math, pathlib
P100K_SHA='8111ecada7a66defe30f614cd3851a328d861bbab4c595cb12ee3131fa465a31'
P10K_SHA='97b50c1af6fce5ab740455f0657bd011cd7871a49e3564ae6b89b97f26eb7b30'
PROTOCOL_SHA='196a7888ec13936e0f1fbd1fcc5bf266a7b22216e84147c2e580077d318e290e'
def require(condition,message):
    if not condition: raise ValueError(message)
def percentile(values,q):
    require(bool(values),'empty percentile population')
    require(all(isinstance(v,(int,float)) and math.isfinite(v) and v>=0 for v in values),'invalid/raw nonfinite duration')
    return sorted(values)[math.ceil(len(values)*q)-1]
def labels(report,label): return [r for r in report['records'] if r['label']==label]
def purity(before,after):
    for key in ['revision','workspace_revision','dirty','project_dirty','undo','redo']:
        require(before[key]==after[key],f'read-only state changed {key}')
def completed_frame(frame,state,full=False):
    keys=['document_id','revision','workspace_revision','scene_serial','scene_count','manufacturing_count','selected_count','selected_primary']
    require(all(frame[k]==state[k] for k in keys),'completed frame does not bind current document/revision/scene/selection')
    require(frame['document_id'] is not None and frame['revision'] is not None and frame['scene_serial'] is not None,'missing frame identity')
    require(frame['focused'] is True and frame['painted'] is True and frame['gpu_completed'] is True,'frame lacks focused painted GPU completion')
    require(frame['busy'] is False and frame['display_pending'] is False and all(frame[k] is None for k in ['blocked','display_error','error','ui_error']),'pending/blocked/error completion frame')
    require(frame['canvas_physical']==[1600.,900.],'physical canvas differs')
    require(0<frame['scene_count']<=100000 and frame['manufacturing_count']==100000,'empty/partial manufacturing completion frame')
    require(frame['selected_primary_in_scene'] is True,'selected primary absent from displayed scene')
    if full: require(frame['scene_count']==100000 and frame['selected_count']==100000,'partial full marquee completion frame')
def ns(row,key):
    value=row[key];require(type(value) is int and value>=0,f'invalid monotonic timestamp {key}');return value
def verify_navigation(report):
    frames=report['frames'];require(len(frames)>=2,'missing raw navigation frames')
    require(len(labels(report,'navigation-start'))==1 and len(labels(report,'navigation-end'))==1,'missing/duplicate navigation boundaries')
    start=labels(report,'navigation-start')[0];end=labels(report,'navigation-end')[0]
    require(all(type(r['frame_id']) is int and r['frame_id']>=0 for r in [start,end]+frames),'invalid navigation frame ID')
    require(all(start['state'][k] is not None for k in ['document_id','revision','scene_serial']),'missing navigation identity')
    for boundary in [start,end]:
        elapsed=boundary['elapsed_seconds'];require(isinstance(elapsed,(int,float)) and math.isfinite(elapsed) and abs(elapsed-ns(boundary,'monotonic_ns')/1_000_000_000)<=1e-9,'boundary elapsed/monotonic time mismatch')
    origin=ns(start,'phase_origin_ns');require(ns(end,'phase_origin_ns')==origin,'navigation origin mismatch')
    require(ns(start,'input_monotonic_ns')<=origin<=ns(start,'monotonic_ns')<ns(frames[0],'input_monotonic_ns'),'navigation start coverage/time mismatch')
    require(frames[0]['frame_id']==start['frame_id']+1 and frames[-1]['frame_id']==end['frame_id'],'navigation prefix/tail truncated')
    require(len(frames)==end['frame_id']-start['frame_id'],'navigation boundary population mismatch')
    require(all(b['frame_id']==a['frame_id']+1 for a,b in zip(frames,frames[1:])),'dropped/nonconsecutive navigation frames')
    require(all(f['phase']==3 for f in frames),'non-navigation frame in raw population')
    require(ns(end,'input_monotonic_ns')==ns(frames[-1],'input_monotonic_ns'),'navigation end input mismatch')
    require(ns(frames[-2],'input_monotonic_ns')-origin<60_000_000_000<=ns(frames[-1],'input_monotonic_ns')-origin,'navigation does not end at first full 60-second input')
    require(ns(frames[-1],'observed_monotonic_ns')<=ns(end,'monotonic_ns')<=ns(frames[-1],'acknowledged_monotonic_ns'),'navigation end observation mismatch')
    require(ns(frames[0],'input_monotonic_ns')-origin<=200_000_000,'navigation first segment pause exceeds200ms')
    previous=ns(start,'input_monotonic_ns');intervals=[]
    for i,f in enumerate(frames):
        at=ns(f,'input_monotonic_ns');observed=ns(f,'observed_monotonic_ns');ack=ns(f,'acknowledged_monotonic_ns')
        require(previous<at<=observed<=ack,'nonmonotonic navigation frame timestamps')
        if i: require(ns(frames[i-1],'acknowledged_monotonic_ns')==at,'missing next-input acknowledgement link')
        interval=f['frame_interval_ms'];percentile([interval],1.)
        require(abs(interval-(at-previous)/1_000_000)<=1e-6,'frame interval does not match monotonic time difference')
        elapsed=f['elapsed_seconds'];require(isinstance(elapsed,(int,float)) and math.isfinite(elapsed) and abs(elapsed-(at-origin)/1_000_000_000)<=1e-9,'elapsed differs from navigation origin/input time')
        require(all(f[k]==start['state'][k]==end['state'][k] for k in ['document_id','revision','scene_serial']),'navigation stale document/revision/scene')
        intervals.append(interval);previous=at
    require(all(f['focused'] is True for f in frames),'native window lost focus')
    require(all(f['canvas_physical']==[1600.,900.] for f in frames),'physical canvas differs')
    # RecoveryWrite may set generic busy while complete scene paint and navigation continue.
    # display_pending/errors, rather than unrelated worker activity, invalidate these frames.
    require(all(f['scene_count']==100000 and f['manufacturing_count']==100000 and f['painted'] is True and f['display_pending'] is False and all(f[k] is None for k in ['blocked','display_error','error','ui_error']) for f in frames),'missing/partial/fallback frame')
    p95=percentile(intervals,.95);p99=percentile(intervals,.99)
    require(p95<=33.3 and p99<=66.7 and max(intervals)<=200,'navigation p95/p99/pause threshold failed')
    purity(start['state'],end['state'])
    delta={k:end['counters'].get(k,0)-start['counters'].get(k,0) for k in ['file-open-parse','geometry-full-build-call','geometry-patch-attempt','scene-allocation','index-allocation','geometry-storage-init-upload','index-storage-init-upload']}
    require(all(v==0 for v in delta.values()),f'navigation rebuilt/reparsed/reallocated: {delta}')
    return {'frames':len(frames),'boundary_frame_ids':[start['frame_id'],end['frame_id']],'duration_seconds':(previous-origin)/1_000_000_000,'p95_ms':p95,'p99_ms':p99,'max_ms':max(intervals),'cache_delta':delta}
def verify_selection(report,ids):
    points=labels(report,'point-highlight');require(len(points)==200,'not all 200 fixed point results')
    baseline=labels(report,'complete-operable-load')[0]['state']
    for i,p in enumerate(points):
        ordinal=(i*37%100)*1000+(i*499%1000)+1
        require(p['point']==i and p['ordinal']==ordinal and p['actual_id']==f'object-{ordinal}',f'fixed point {i} wrong identity/order')
        purity(baseline,p['state'])
        require(p['before']['state']['scene_serial']==p['frame']['scene_serial'],'selection rebuilt scene')
        for key in ['file-open-parse','geometry-full-build-call','scene-allocation','index-allocation','geometry-storage-init-upload','index-storage-init-upload']:
            require(p['before']['counters'].get(key,0)==p['frame']['counters'].get(key,0),'selection reparsed/rebuilt/uploaded geometry')
        completed_frame(p['frame'],p['state'])
        require(p['state']['document_id']==baseline['document_id'] and all(p['before']['state'][k]==p['state'][k] for k in ['document_id','revision','scene_serial','scene_count','manufacturing_count']),'point input/completion context differs')
        require(p['frame']['selected_count']==1 and p['frame']['selected_primary']==p['actual_id'],'point does not bind native highlight/focus')
    cpu=report['exact_hit_cpu_ms'];p95cpu=percentile(cpu,.95)
    require(len(cpu)>=200 and p95cpu<=20,'CPU query count/p95 failed')
    times=[p['input_to_gpu_complete_upper_bound_ms'] for p in points];p95=percentile(times,.95);require(p95<=100,'point input-to-highlight p95 failed')
    require(ids==[f'object-{i}' for i in range(1,100001)],'full marquee IDs truncated/reordered/duplicated')
    marquee=labels(report,'marquee-highlight')[0];require(marquee['selected_count']==100000 and marquee['ordered'],'marquee report mismatch')
    percentile([marquee['release_to_gpu_complete_upper_bound_ms']],1.)
    completed_frame(marquee['frame'],marquee['state'],full=True)
    require(marquee['state']['document_id']==baseline['document_id'] and marquee['frame']['selected_primary'] in ids,'marquee current document/selection mismatch')
    require(marquee['release_to_gpu_complete_upper_bound_ms']<=300,'full marquee GPU-complete upper bound exceeds 300ms')
    purity(baseline,marquee['state'])
    require(len(labels(report,'marquee-start'))==1,'missing marquee input context')
    require(all(labels(report,'marquee-start')[0]['state'][k]==marquee['state'][k] for k in ['document_id','revision','scene_serial','scene_count','manufacturing_count']),'marquee input/completion context differs')
    for name in ['bounded-move-before','bounded-move-after','bounded-undo','bounded-redo','bounded-export','bounded-export-reopen','single-move-before','single-move-after','single-move-undo']:
        require(len(labels(report,name))==1,f'missing native bounded edit workflow {name}')
    single=verify_edit_counters(report)
    return {'single_move_counter_delta':single,'points':len(points),'exact_query_population':len(cpu),'exact_cpu_p95_ms':p95cpu,'highlight_p95_ms':p95,'highlight_max_ms':max(times),'marquee_ms':marquee['release_to_gpu_complete_upper_bound_ms'],'marquee_ids':len(ids)}
def verify_edit_counters(report):
    for label in ['bounded-move-before','bounded-move-after','bounded-undo','bounded-redo','bounded-export','bounded-export-reopen','single-move-before','single-move-after','single-move-undo']:
        row=labels(report,label)[0];frame=row.get('completed_frame')
        require(frame is not None,'edit counters lack completed GPU frame')
        require(all(frame.get(k)==row['state'].get(k) for k in ['document_id','revision','scene_serial','selected_count','selected_primary']),'edit frame does not bind current document/revision/selection')
        require(frame['counters']==row['counters'],'edit counters captured before callback completion')
    before=labels(report,'single-move-before')[0]['counters'];after=labels(report,'single-move-after')[0]['counters']
    keys=['file-open-parse','geometry-full-build-call','geometry-patch-attempt','scene-allocation','index-allocation','geometry-storage-init-upload','index-storage-init-upload','selection-upload','uniform-upload','draw']
    delta={k:after.get(k,0)-before.get(k,0) for k in keys}
    require(delta['file-open-parse']==0 and delta['geometry-full-build-call']==0 and delta['geometry-patch-attempt']==1,'single move reparsed/full-built or missed patch')
    require(delta['scene-allocation']==1 and delta['geometry-storage-init-upload']>0,'single move actual GPU allocation/upload absent')
    return delta
def verify_load(report):
    loads=labels(report,'complete-operable-load');require(len(loads)==4,'cold plus three same-process warm loads required')
    maximum=3000 if report['fixture']=='P10K' else 10000;count=10000 if report['fixture']=='P10K' else 100000
    require([l['round'] for l in loads]==[0,1,2,3],'cold/warm order differs')
    require(all(l['elapsed_ms']<=maximum and l['frame']['scene_count']==count and l['frame']['manufacturing_count']==count and not l['frame']['display_pending'] for l in loads),'load threshold or complete frame failed')
    return {'cold_ms':loads[0]['elapsed_ms'],'warm_ms':[l['elapsed_ms'] for l in loads[1:]]}
def verify_idle(report,samples):
    start=labels(report,'idle-start')[0];end=labels(report,'idle-end')[0];purity(start['state'],end['state'])
    require(start['state']['project_dirty'] is False,'idle contains pending dirty/recovery tasks')
    require(end['elapsed_seconds']-start['elapsed_seconds']>=60,'idle duration below 60s')
    draw=end['counters'].get('draw',0)-start['counters'].get('draw',0);require(draw<60*60,'idle is continuously redrawing at the 60Hz baseline')
    idle=[s for s in samples if int(start['wall_time_ns'])<=s['wall_time_ns']<=int(end['wall_time_ns'])];require(len(idle)>=50,'idle CPU samples missing')
    require(len(labels(report,'worker-real-wake-highlight'))==1,'real worker wake absent')
    wake=labels(report,'worker-real-wake-highlight')[0]
    require(wake['frame']['counters'].get('worker-request-repaint',0)-end['counters'].get('worker-request-repaint',0)==1,'worker did not issue exactly one result wake')
    return {'idle_redraws':draw,'cpu_samples':len(idle),'cpu_p95_percent':percentile([s['cpu_percent'] for s in idle],.95),'worker_wake_ms':labels(report,'worker-real-wake-highlight')[0]['enqueue_to_complete_ms']}
def verify_lifecycle(report,samples):
    rows=labels(report,'life-closed-idle5');require(len(rows)==20 and [r['round'] for r in rows]==list(range(20)),'same-process 20 close/idle cycles missing')
    for name in ['life-lod2','life-lod4','life-fit']:require(len(labels(report,name))==20,f'missing LOD cycles {name}')
    rss=[]
    for r in rows:
        prior=[s for s in samples if s['wall_time_ns']<=int(r['wall_time_ns'])];require(bool(prior),'no checkpoint RSS')
        sample=prior[-1];require(int(r['wall_time_ns'])-sample['wall_time_ns']<1_000_000_000,'checkpoint RSS older than 1s')
        rss.append(sample['rss_bytes'])
    require(rss[-1]-rss[4]<=100*1024**2,'round20-round5 RSS growth exceeds100MiB')
    require(max(s['rss_bytes'] for s in samples)<=1024**3,'process peak exceeds1GiB')
    gpu=report['counters'].get('metal-device-max-observed-allocated-bytes');require(gpu is not None and 0<gpu<=512*1024**2,'actual Metal device allocations absent/exceed512MiB')
    # Raw series retained for human trend review. A permissive threshold is not
    # substituted for the required no sustained linear-growth assessment.
    return {'close_rss_bytes':rss,'round20_minus5_bytes':rss[-1]-rss[4],'process_peak_bytes':max(s['rss_bytes'] for s in samples),'metal_max_observed_bytes':gpu,'linear_growth_review':'required; inspect all 20 raw checkpoints'}
def verify_directory(path):
    report=json.loads((path/'native-observations.json').read_text());runner=json.loads((path/'runner-result.json').read_text());samples=json.loads((path/'process-samples.json').read_text())
    require(not report['failures'] and runner['exit_code']==0,'native run failures or abnormal exit')
    require(report['protocol_sha256']==PROTOCOL_SHA,'unfrozen protocol')
    require(report['fixture_sha256']==(P10K_SHA if report['fixture']=='P10K' else P100K_SHA),'unfrozen sample')
    require('backend: Metal' in report['adapter'],'not native Metal')
    before=json.loads((path/'binary-before.json').read_text());require(before['sha256']==report['binary_sha256'],'binary changed during measurement')
    display=json.loads((path/'display-60hz.json').read_text())['after'];restore=json.loads((path/'display-restored.json').read_text())['after'];original=json.loads((path/'display-before.json').read_text())['after']
    require(display['refresh_hz']==60 and display['backing_scale']==2 and restore==original,'display condition/restore mismatch')
    mode=report['mode']
    if mode in ['nav','select','idle']:
        parity=labels(report,'read-only-writer-parity');require(len(parity)==1 and parity[0]['byte_identical'] and parity[0]['before_sha256']==parity[0]['after_sha256'],'real read-only writer bytes changed/missing')
        require(parity[0]['source_sha256']==report['fixture_sha256'],'original imported file changed')
    if mode=='nav':
        video=path/'native-navigation.mov';require(video.exists() and video.stat().st_size>0,'native video body missing');result=verify_navigation(report)
    elif mode=='select':result=verify_selection(report,json.loads((path/'marquee-selected-ids.json').read_text()))
    elif mode=='load':result=verify_load(report)
    elif mode=='idle':result=verify_idle(report,samples)
    else:
        require(runner.get('peak_rss_method')=='macOS wait4 actual app child ru_maxrss bytes','actual app lifetime peak RSS absent')
        require(runner['process_peak_rss_bytes']<=1024**3,'actual app lifetime peak RSS exceeds1GiB')
        result=verify_lifecycle(report,samples)
        result['actual_app_peak_rss_bytes']=runner['process_peak_rss_bytes']
        result['peak_rss_method']=runner['peak_rss_method']
    return {'status':'CHECKS_PASS','mode':mode,'fixture':report['fixture'],'binary_sha256':report['binary_sha256'],'build_source':report.get('build_source'),'result':result}
def main():
    p=argparse.ArgumentParser(description=__doc__);p.add_argument('directories',nargs='+',type=pathlib.Path);p.add_argument('--output',type=pathlib.Path);a=p.parse_args();results=[]
    for path in a.directories:
        try:result=verify_directory(path)
        except (ValueError,KeyError,OSError,IndexError) as error:result={'status':'FAIL','directory':str(path),'error':str(error)}
        results.append(result)
    text=json.dumps({'schema_version':2,'stage':'S5-M1','run_checks':results,'stage_pass':'not asserted; full gates, independent source/package reviews and native trend/coverage review still required'},ensure_ascii=False,indent=2)+'\n'
    if a.output:a.output.write_text(text)
    print(text,end='')
    raise SystemExit(0 if all(r['status']=='CHECKS_PASS' for r in results) else 1)
if __name__=='__main__':main()
