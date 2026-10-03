"""Portable S5-I2-B candidate/clean-final integrity check; never I2/native acceptance.
Usage: python3 verify_i2_b_evidence.py BUNDLE_DIRECTORY
No shell/Cargo execution, network, absolute-path lookup or directory cleanup.
"""
import argparse
import hashlib
import io
import json
from pathlib import Path, PurePosixPath
import re
import stat
import zipfile
import math
from verify_i2_b_native_semantics import verify_trace, verify_fixed_transform, verify_feedback

EXPECTED_COMMANDS_SHA256 = "c90447a419eb0021af2c099ca4aadac9511271fde8467e0e2dd6de2618c4c775"
GATES = ('fmt', 'check', 'clippy', 'clippy-internal', 'workspace-test',
         'automation-contract', 'headless-workflow', 'verifier-unit', 'service-boundary',
         'i2-core-release', 'i2-budget-unit-release', 'numeric-oracles', 'i2-service-release', 'i2-app-release',
         'i1-release-regression', 'batch-release-regression',
         'b-app-release', 'b-service-release', 'b-core-snap', 'b-verifier-unit',
         'metal-reference', 'metal-batch', 'metal-block',
         'release', 'release-internal', 'source-manifest')
REMAINING = ('independent_review', 'C_interaction_UX',
             'three_native_I2_rounds', 'menu_flicker_actual_reproduction',
             'stage_commit_and_clean_same_commit_closeout')
FINAL_REMAINING = ('independent_final_B_review', 'C_interaction_UX',
                   'three_native_I2_rounds', 'menu_flicker_actual_reproduction', 'Windows_deferred')
PACKAGING_ONLY = {'scripts/verify_i2_b_evidence.py', 'scripts/test_verify_i2_b_evidence.py',
                  'docs/S5_I2_B_CANDIDATE_REVIEW.md', 'docs/S5_I2_A_REVIEW.md'}


def require(condition, message):
    if not condition:
        raise ValueError(message)


def sha(data):
    return hashlib.sha256(data).hexdigest()


def safe(name):
    require(isinstance(name, str) and name and '\\' not in name and ':' not in name,
            'unsafe path')
    p = PurePosixPath(name)
    require(not p.is_absolute() and all(x not in ('.', '..', '') for x in name.split('/')),
            'unsafe path')
    return name


def manifest(data):
    entries = {}
    for line in data.decode('utf-8').splitlines():
        digest, sep, name = line.partition('  ')
        require(sep and re.fullmatch('[0-9a-f]{64}', digest), 'bad digest line')
        safe(name)
        require(name not in entries, 'duplicate manifest path')
        entries[name] = digest
    require(entries, 'empty manifest')
    return entries


def read(root, name):
    p = root / safe(name)
    for ancestor in (p, *p.parents):
        require(not ancestor.is_symlink(), 'symlink evidence path')
        if ancestor == root:
            break
    require(p.is_file() and p.stat().st_size <= 128 * 1024 * 1024, 'missing/oversize evidence')
    return p.read_bytes()

def verify_native(root,review,source_hash,binaries):
    runs=json.loads(read(root,'native/index.json'))
    require(len(runs)==3 and len({r['run_id'] for r in runs})==3,'three unique B native functional runs')
    require({json.loads(read(root,safe(r['path'])+'/request.json'))['copy'] for r in runs}=={False,True},'required Move and Copy native branches omitted')
    for run in runs:
        require(json.loads(read(root,safe(run['path'])+'/request.json'))['role']=='functional','native functional role replaced')
        verify_native_run(root,run,source_hash,binaries)
    feedback=json.loads(read(root,'feedback/index.json'))
    require(len(feedback)==3 and len({r['run_id'] for r in runs+feedback})==6,'three unique B-07 feedback runs required')
    displays=[];feedback_results=[]
    for run in feedback:
        req=json.loads(read(root,safe(run['path'])+'/request.json'));require(req['role']=='feedback','feedback role replaced');displays.append(req['display']);feedback_results.append(verify_native_run(root,run,source_hash,binaries))
    require(all(d==displays[0] for d in displays),'feedback display configuration changed across rounds')
    return feedback_results

def verify_native_run(root,run,source_hash,binaries):
    prefix=safe(run['path'])+'/'
    request=json.loads(read(root,prefix+'request.json'));obs=json.loads(read(root,prefix+'observations.json'))
    runner=json.loads(read(root,prefix+'runner.json'));ledger=json.loads(read(root,prefix+'capture-ledger.json'))
    require(request['stage']==obs['stage']=='S5-I2-B' and request['run_id']==run['run_id']==ledger['run_id'],'B run identity')
    require(request['source_manifest_sha256']==source_hash and obs['profile']=='release' and obs['commit']==binaries['release-internal']['commit'],'native source/commit/profile')
    require(request['binary_sha256']==obs['binary_sha256']==runner['binary_sha256']==binaries['release-internal']['sha256'],'native binary binding')
    require(type(runner['exit_code']) is int and runner['exit_code']==0 and runner['error'] is None and obs['error'] is None,'native failure/timeout')
    require('Metal' in str(obs['adapter']) and 'synthetic egui' in obs['measurement_scope'],'native renderer/input scope')
    require(request['whole_I2_acceptance']=='NOT_ACCEPTED' and request['C_native_rounds']=='PENDING','B cannot attest C/full I2')
    require(obs['request']==request,'native request replacement')
    with zipfile.ZipFile(root/'Source.zip') as source:
        require(request['fixtures']=={n:sha(source.read('fixtures/synthetic/s5i2b/'+n)) for n in ('layer_a.gbr','layer_b.gbr','layer_c.gbr')},'native fixture source binding')
    frames=[]
    for n,shard in enumerate(obs['frame_files']):
        require(shard['path']==f'frames-{n:03}.json','frame shard path sequence')
        payload=read(root,prefix+shard['path']);require(sha(payload)==shard['sha256'],'frame shard digest');rows=json.loads(payload)
        require(len(rows)==shard['count'] and 0<len(rows)<=2000,'frame shard omitted/oversize count');frames.extend(rows)
    frame_ids=[f['frame_id'] for f in frames]
    require(len(frame_ids)==len(set(frame_ids)) and frame_ids==list(range(1,ledger['capture_end']+1)),'native contiguous frame capture')
    require(ledger['capture_start']==1 and ledger['terminal_pending_frame']==ledger['capture_end']+1,'capture terminal boundary')
    require(len(ledger['input_ids'])==len(set(ledger['input_ids'])) and set(frame_ids)<=set(ledger['input_ids']),'raw input producer binding')
    require(len(ledger['update_ids'])==len(set(ledger['update_ids'])) and set(frame_ids)<=set(ledger['update_ids']),'UI update producer binding')
    paint=ledger['paint_ids'];require(len(paint)==len(set(paint)) and ledger['paint_count']==len(paint),'independent paint producer')
    records=obs['records'];require([r['step'] for r in records]==list(range(len(request['steps']))),'native omitted step')
    paint_clock=verify_trace(request,obs,ledger,frames)
    if request['role']=='feedback':
        with zipfile.ZipFile(root/'Source.zip') as source:
            require(request['performance_fixture_sha256']==sha(source.read('fixtures/synthetic/s2b3_2/P100K_CIRCLES.gbr')),'feedback performance fixture source binding')
        return verify_feedback(request,obs,ledger,frames,paint_clock,lambda name:read(root,prefix+name))
    prior=None;operations=set();adapters=set();cancel_seen=False;transform_index=0
    for n,record in enumerate(records):
        require(record['path']==f'step-{n:02}.json','step path binding')
        data=json.loads(read(root,prefix+record['path']));require(data['input']==request['steps'][n],'step request binding')
        state=data['state'];require(state['error'] is None,'native service error')
        if state['layers']:
            complete=data['completed_frame'];require(complete is not None and complete['state']==state and complete['painted'] is True and complete['gpu_completed'] is True,'step not settled/painted/GPU-complete')
            require(complete['frame_id'] in paint and any(f==complete for f in frames),'completion independent capture binding')
        step=data['input']
        import gzip,io
        try:
            with gzip.GzipFile(fileobj=io.BytesIO(read(root,prefix+f'step-{n:02}.ppm.gz'))) as image:
                surface=image.read(128*1024*1024+1)
            header=surface.split(b'\n',3);width,height=map(int,header[1].split())
            valid=(len(surface)<=128*1024*1024 and header[0]==b'P6' and header[2]==b'255'
                   and width>0 and height>0 and len(header[3])==width*height*3)
        except (OSError,EOFError,ValueError,IndexError):valid=False
        require(valid,'native complete lossless surface capture omitted/invalid')
        if step['kind']=='cancel_conflict':
            ready=prior['state'] if prior is not None else {}
            if step['name']=='transform-apply':
                point=ready.get('point_transform')
                require(point is not None and point['operation'] is not None and point['preview'] is not None,'cancel did not race a ready transform preview')
            else:
                adapter=ready.get('point_adapter')
                require(adapter is not None,'cancel did not race a ready adapter')
                if adapter['target'].startswith('Grip('):
                    require(adapter['grip_preview'] is not None and len(ready['selected'])==1 and adapter['grip_preview']['geometry']!=ready['selected'][0]['object']['geometry'],'cancel Grip preview was unsupported or unchanged')
                else:require(adapter['target']=='BlockTarget' and adapter['block_translation'] is not None,'cancel Block target preview was not ready')
            require(prior is not None and state['info']==prior['state']['info'] and state['selected']==prior['state']['selected'],'same-frame cancellation changed manufacturing/selection')
            ids={f['frame_id'] for f in frames if f['step']==n}
            require(not any(a['frame_id'] in ids and a['detail']['kind'] in ('point_apply','grip_edit','block_edit','legacy_transform') for a in ledger['actions']),'cancel frame enqueued a manufacturing commit')
            if step['cancel']=='ime':require(state['modal'] is not None,'IME lost originating point workflow')
            else:require(state['point_transform'] is None and state['point_adapter'] is None,'cancel frame kept active point commit')
        if step.get('kind') in ('zoom','pan') and prior is not None:
            require(state['info']==prior['state']['info'] and state['selected']==prior['state']['selected'],'navigation changed manufacturing/selection')
            require(state['point_transform']['base']==prior['state']['point_transform']['base'],'navigation changed confirmed world base')
        if step.get('kind')=='point_click' and prior is not None:
            if step.get('outside') is True:
                require(prior['state']['point_pick'] is not None and state['point_pick']==prior['state']['point_pick'] and state['modal'] is None,'outside click confirmed a point')
            else:
                require(prior['state']['point_pick'] is not None and state['point_pick'] is None and state['modal'] is not None,'pick did not return to originating point workflow')
            require(state['info']==prior['state']['info'] and state['selected']==prior['state']['selected'] and state['tool']==prior['state']['tool'],'child pick leaked to selection/manufacturing/parent tool')
        if step.get('kind')=='widget' and step.get('name')=='adapter-apply':
            require(prior is not None and prior['state']['point_adapter'] is not None,'adapter widget did not belong to a current point session')
            adapter=prior['state']['point_adapter'];target=adapter['target'];point=adapter['world']
            adapters.add(target.split('(')[0])
            if target.startswith('Grip('):
                preview=adapter['grip_preview'];require(preview is not None,'Grip preview missing')
                require(int(state['info']['revision'])==int(prior['state']['info']['revision'])+1 and state['info']['undo_entries']==prior['state']['info']['undo_entries']+1,'numeric Grip not one transaction')
                require(len(state['selected'])==1 and state['selected'][0]['object']['geometry']['Flash']['center']==preview['geometry']['Flash']['center'],'Grip preview/commit geometry mismatch')
                aperture=state['selected'][0]['object']['geometry']['Flash']['aperture_id'];require(any(a['id']==aperture and a['shape']==preview['aperture_shape'] for a in data['snapshot']['apertures']),'Grip preview/commit aperture mismatch')
            elif target=='BlockTarget':
                translation=adapter['block_translation'];require(translation is not None and len(state['selected'])==1,'Block target preview/selection missing')
                transform=state['selected'][0]['object']['geometry']['BlockInstance']['transform'];require(transform['translation']==translation,'Block reference target mapping differs from preview')
                require(int(state['info']['revision'])==int(prior['state']['info']['revision'])+1 and state['info']['undo_entries']==prior['state']['info']['undo_entries']+1,'Block placement not one transaction')
            else:
                require(state['info']==prior['state']['info'] and state['selected']==prior['state']['selected'],'readonly point adapter mutated manufacturing/selection')
                if target=='TextReference':require(state['text_reference']['enabled'] is True and [float(state['text_reference'][a]) for a in ('x','y')]==point,'text relative reference mapping')
                elif target=='Measure':require(any(p is not None and [p['x_mm'],p['y_mm']]==point for p in (state['measure']['a'],state['measure']['b'])),'measurement endpoint mapping')
                elif target=='ArrayBase':require(state['array_base']==dict(zip(('x_mm','y_mm'),point)),'array base mapping')
                elif target=='ArrayTarget':
                    base=prior['state']['array_base'];require([float(v) for v in state['array_pitch']]==[point[0]-base['x_mm'],point[1]-base['y_mm']],'signed Array B to T pitch mapping')
                elif target.startswith('BoardWorld('):require(state['board_world'][0]==point,'Board world mapping')
                elif target.startswith('BlockLocal('):require(state['block_reference']==dict(zip(('x_mm','y_mm'),point)) and state['tool']=='Block','local Block reference must retain parent placement')
                elif target=='BlockCreate':require([float(v) for v in state['block_origin']]==point and state['modal']=='BlockCreate','Block origin must resume create modal')
        if step.get('kind')=='widget' and step.get('name')=='transform-apply':
            require(prior is not None,'transform has no prior resolved preview')
            p=prior['state']['point_transform'];verify_fixed_transform(transform_index,p);transform_index+=1;operation=p['operation'];require(p['preview']['operation']==operation,'preview/commit parameter mismatch')
            require(int(state['info']['revision'])==int(prior['state']['info']['revision'])+1 and state['info']['undo_entries']==prior['state']['info']['undo_entries']+1,'transform not one atomic revision/Undo')
            ids={f['frame_id'] for f in frames if f['step']==n}
            require(any(a['frame_id'] in ids and a['detail']['kind']=='point_apply' and a['detail']['operation']==operation for a in ledger['actions']),'transform widget lacks accepted production action')
            before=prior['state']['selected'];after=state['selected'];require(len(before)==len(after)==5 and len({o['layer_id'] for o in after})==3,'five-object three-layer workflow')
            for a,b in zip(before,after):
                x=a['object']['geometry']['Flash']['center'];y=b['object']['geometry']['Flash']['center'];kind=operation['kind'];px,py=x['x_mm'],x['y_mm']
                if kind in ('move','duplicate'):expected=(px+operation['dx_mm'],py+operation['dy_mm'])
                elif kind=='rotate':
                    pivot=operation['pivot_mm'];angle=math.radians(operation['angle_deg']);dx=px-pivot['x_mm'];dy=py-pivot['y_mm'];expected=(pivot['x_mm']+dx*math.cos(angle)-dy*math.sin(angle),pivot['y_mm']+dx*math.sin(angle)+dy*math.cos(angle))
                elif kind=='mirror':
                    axis=operation['axis'];expected=(px,2*axis['coordinate_mm']-py) if axis['kind']=='horizontal' else (2*axis['coordinate_mm']-px,py)
                else:raise ValueError('unsupported native resolved operation')
                require(math.dist((y['x_mm'],y['y_mm']),expected)<=1e-9 and a['layer_id']==b['layer_id'] and a['object']['exposure']==b['object']['exposure'],'native independent manufacturing coordinate/exposure/layer oracle')
            if operation['kind']=='duplicate':
                old={o['object_id']:o for layer in prior['snapshot']['layers'] for o in layer['objects']};new={o['object_id']:o for layer in data['snapshot']['layers'] for o in layer['objects']}
                require(len(new)==len(old)+5 and all(new.get(k)==v for k,v in old.items()),'Copy changed originals or inserted wrong count')
            operations.add(operation['kind'])
        if step.get('kind')=='escape' and prior is not None:
            require(state['info']==prior['state']['info'] and state['selected']==prior['state']['selected'],'cancel changed manufacturing/selection')
            cancel_seen=True
        prior=data
    require(transform_index==5 and {'rotate','mirror'}<=operations and bool({'move','duplicate'}&operations) and cancel_seen,'required transform/cancel workflow omitted')
    require(adapters=={'Measure','TextReference','ArrayBase','ArrayTarget','BoardWorld','Grip','BlockCreate','BlockLocal','BlockTarget'},'required point adapter workflows omitted')

def verify(root, expected_manifest=None, expected_commit=None, expected_precommit_manifest=None):
    root = Path(root).absolute()
    entries = manifest(read(root, 'BUNDLE.sha256'))
    actual = set()
    for p in root.rglob('*'):
        require(not p.is_symlink(), 'symlink bundle')
        if p.is_file() and p != root/'BUNDLE.sha256':
            actual.add(p.relative_to(root).as_posix())
    require(actual == set(entries), 'bundle file set mismatch')
    for name, digest in entries.items():
        require(sha(read(root, name)) == digest, 'bundle digest mismatch: ' + name)
    review = json.loads(read(root, 'REVIEW.json'))
    require(type(review['schema_version']) is int and review['schema_version'] == 2,
            'schema version')
    if expected_manifest is not None:
        require(isinstance(expected_manifest,str) and re.fullmatch('[0-9a-f]{64}',expected_manifest)
                and review['source_manifest_sha256']==expected_manifest, 'external source reference mismatch')
    final=review.get('artifact_mode','candidate')=='clean_final_review'
    require(review.get('artifact_mode','candidate') in ('candidate','clean_final_review'), 'artifact mode')
    acceptance='B_FINAL_REVIEW_PENDING' if final else 'NOT_ACCEPTED'
    require(review['scope'] == 'S5-I2-B' and review['acceptance'] == acceptance,
            'B cannot attest whole I2 acceptance')
    remaining=FINAL_REMAINING if final else REMAINING
    require(tuple(review['required_remaining']) == remaining, 'remaining scope omitted')
    require(re.fullmatch('[0-9a-f]{40}', review['base_commit']), 'commit identity')
    if final:
        require(expected_manifest is not None and isinstance(expected_commit,str)
                and re.fullmatch('[0-9a-f]{40}',expected_commit) and expected_commit==review['base_commit'],
                'clean final requires external commit reference')
        require(isinstance(expected_precommit_manifest,str) and re.fullmatch('[0-9a-f]{64}',expected_precommit_manifest),
                'clean final requires external approved source reference')
        approved=review['precommit_B_review']
        require(approved['result']=='PRECOMMIT_B_PASS_MAC_FIRST_BOUNDED'
                and approved['source_manifest_sha256']==expected_precommit_manifest,
                'precommit B approval identity')
        require(sha(read(root,approved['report_path']))==approved['report_sha256'], 'precommit review digest')
        require(review['whole_I2_acceptance']=='NOT_ACCEPTED', 'B cannot attest whole I2')
    source = read(root, 'Source.zip')
    require(sha(source) == review['source_zip_sha256'], 'source ZIP identity')
    with zipfile.ZipFile(io.BytesIO(source)) as z:
        names = z.namelist()
        require(len(names) == len(set(names)), 'duplicate source member')
        require(sum(i.file_size for i in z.infolist()) <= 128 * 1024 * 1024,
                'oversize source ZIP')
        for i in z.infolist():
            safe(i.filename)
            require(not i.is_dir() and not stat.S_ISLNK(i.external_attr >> 16),
                    'source ZIP symlink/directory')
        package = manifest(z.read('PACKAGE_MANIFEST.sha256'))
        require(set(names) == set(package) | {'PACKAGE_MANIFEST.sha256'}, 'source ZIP file set')
        for name, digest in package.items():
            require(sha(z.read(name)) == digest, 'source member hash')
        source_manifest = z.read('MANIFEST.sha256')
        source_hash = sha(source_manifest)
        require(source_hash == review['source_manifest_sha256'], 'source manifest identity')
        files = manifest(source_manifest)
        require(set(package)==set(files)|{'MANIFEST.sha256','PACKAGE_INFO.json'},'unmanifested source payload')
        for name, digest in files.items():
            require(name in package and sha(z.read(name)) == digest, 'source file hash')
        info = json.loads(z.read('PACKAGE_INFO.json'))
        require(info['git_commit'] == review['base_commit'] and info['commit'] == review['base_commit'],
                'package commit')
        require(info['stage'] == ('S5-I2-B clean final' if final else 'S5-I2-B review candidate')
                and info['clean_worktree'] is final,
                'candidate source misrepresented as clean closeout')
        require(type(info['source_file_count']) is int and info['source_file_count'] == len(files),
                'source file count')
        require(info['source_manifest_sha256'] == source_hash, 'package source identity')
    if final:
        approved_raw=read(root,'independent-precommit/source-manifest.sha256')
        require(sha(approved_raw)==expected_precommit_manifest,'approved payload binding')
        approved_files=manifest(approved_raw)
        changed=sorted(n for n in set(files)|set(approved_files) if files.get(n)!=approved_files.get(n))
        require(set(changed)<=PACKAGING_ONLY,'product changed after precommit approval; requires new review')
        impact=json.loads(read(root,'REVIEW_IMPACT.json'))
        require(impact['changed_paths']==changed and impact['product_changes']==[]
                and impact['approved_source_manifest_sha256']==expected_precommit_manifest
                and impact['final_source_manifest_sha256']==source_hash,'review impact binding')
    summary = json.loads(read(root, 'gates/summary.json'))
    require(summary['stage'] == 'S5-I2-B' and summary['result'] == ('GATES_PASS' if final else 'CANDIDATE_GATES_PASS'),
            'gate summary status')
    require(summary['source_manifest_sha256'] == source_hash and summary['commit'] == review['base_commit'],
            'gate summary identity')
    require(summary['clean_worktree'] is final and summary['unchanged_source'] is True
            and summary['unchanged_status'] is True, 'source stability')
    require(type(summary['gates_expected']) is int and summary['gates_expected'] == len(GATES)
            and type(summary['gates_passed']) is int and summary['gates_passed'] == len(GATES),
            'gate count')
    require(read(root, 'gates/source-before.sha256') == source_manifest
            == read(root, 'gates/source-after.sha256'), 'tested source binding')
    if final:
        require(not read(root,'gates/status-before.txt').strip()
                and not read(root,'gates/status-after.txt').strip(),'final worktree dirty')
    gates = json.loads(read(root, 'gates/gates.json'))
    require(sha(json.dumps([(g['id'],g['command']) for g in gates],separators=(',',':')).encode()) == EXPECTED_COMMANDS_SHA256, 'gate command set weakened')
    require(tuple(g['id'] for g in gates) == GATES, 'required gate set/order')
    for g in gates:
        require(type(g['exit_code']) is int and g['exit_code'] == 0, 'failed/mistyped gate')
        require(g['commit'] == review['base_commit'] and g['source_manifest_sha256'] == source_hash,
                'gate identity')
        require(g['log'] == g['id'] + '.log', 'gate log binding')
        log = read(root, 'gates/' + g['log'])
        require((log or g['id'] == 'fmt') and sha(log) == g['sha256'], 'gate log digest')
        if g['id'] in ('i2-core-release', 'i2-budget-unit-release', 'i2-service-release', 'i2-app-release', 'b-app-release', 'b-service-release'):
            minimum = {'i2-core-release': 29, 'i2-budget-unit-release':3, 'i2-service-release': 8, 'i2-app-release': 6,'b-app-release':16,'b-service-release':1}[g['id']]
            outcomes = re.findall(rb'test result: ok\. (\d+) passed; 0 failed;', log)
            require(sum(int(n) for n in outcomes) >= minimum, 'missing executed I2 tests')
        if g['id'].startswith('metal-'):
            require(b'1 passed; 0 failed' in log and b'Metal' in log, 'missing native Metal execution')
    numeric=json.loads(read(root,'gates/numeric-oracles/summary.json'))
    require(numeric['source_manifest_sha256']==source_hash and numeric['result']=='NUMERIC_ORACLES_PASS'
            and type(numeric['failures']) is int and numeric['failures']==0, 'numeric oracle status/source')
    require(numeric['circle_cases']==1662 and numeric['original_circle_cases']==678
            and numeric['polygon_cases']==1470 and numeric['original_circle_ready']>=500
            and numeric['polygon_ready']>=1200, 'numeric oracle matrix/admission omitted')
    rows=numeric['original_14_successful_bounds_validated']
    require(len(rows)==14 and len({r['case'] for r in rows})==14
            and all(r['actual'].startswith('READY ') for r in rows), 'independent 14 regressions omitted')
    require(numeric['real_negative_thin_annulus']['actual'].startswith('ERR PrecisionUncertain('),
            'thin annulus false zero negative omitted')
    for kind in ('circle','polygon'):
        payload=read(root,'gates/numeric-oracles/'+kind+'-oracle.json')
        require(sha(payload)==numeric[kind+'_report_sha256'], 'numeric report digest')
        report=json.loads(payload)
        require(report['case_count']==numeric[kind+'_cases'] and report['failures']==[],
                'numeric report failures/count')
    require(numeric['macro_cases']==1512 and numeric['macro_baseline_cases']==900
            and numeric['macro_practical_cases']==numeric['macro_practical_ready']==360,
            'Macro transform/practical matrix omitted')
    payload=read(root,'gates/numeric-oracles/macro-oracle.json')
    require(sha(payload)==numeric['macro_report_sha256'], 'Macro report digest')
    macro=json.loads(payload)
    require(macro['cases']==1512 and macro['baseline_cases']==900 and macro['failures']==[],
            'Macro success error bound failed/matrix omitted')
    rows=numeric['macro_original_32_replayed']
    require(rows==macro['original_32_replayed'] and len(rows)==len({r['input'] for r in rows})==32
            and all(r['actual'].startswith(('READY ','ERR PrecisionUncertain(')) for r in rows),
            'Macro independent 32 regressions omitted')
    payload=read(root,'gates/numeric-oracles/chain-observation.json')
    require(sha(payload)==numeric['chain_report_sha256'], 'chain report digest')
    chain=json.loads(payload)['rows']
    require(chain==numeric['chain_observations'] and len(chain)==2
            and {r['chain10k'] for r in chain}=={'budget','external_cancel'}, 'chain scenarios omitted')
    by_kind={r['chain10k']:r for r in chain}
    budget=by_kind['budget'];cancel=by_kind['external_cancel']
    require(budget['outcome']=='ResourceLimit' and type(budget['polls']) is int
            and 2_000_000<=budget['polls']<2_100_000 and 0<=budget['max_unchecked_ms']<=500,
            'chain budget/checkpoint observation failed')
    require(cancel['outcome']=='Cancelled' and 0<=cancel['cancel_request_to_return_ms']<=2000,
            'chain external controlled return failed')
    boundary = json.loads(read(root, 'gates/service-boundary.json'))
    require(boundary['forbidden_dependencies'] == [], 'service window/GPU dependency')
    binaries = json.loads(read(root, 'gates/binaries.json'))
    require(set(binaries) == {'release', 'release-internal'}, 'release binary set')
    for kind, binary in binaries.items():
        data = read(root, 'gates/' + safe(binary['path']))
        require(sha(data) == binary['sha256'] and binary['source_manifest_sha256'] == source_hash,
                'binary identity')
        expected_binary=review['base_commit']+('' if final else '-dirty')
        require(binary['commit'] == expected_binary and binary['profile'] == 'release',
                'binary profile/commit')
        require(binary['commit'].encode() in data, 'embedded binary commit identity')
        if final:
            require((expected_binary+'-dirty').encode() not in data,'dirty final binary identity')
        require((b'RCAM_I1_NATIVE_DIR' in data) == (kind == 'release-internal'), 'public/internal boundary')
        require((b'RCAM_I2_B_NATIVE_DIR' in data) == (kind == 'release-internal'), 'B public/internal boundary')
    feedback_results=verify_native(root, review, source_hash, binaries)
    return {'B_native_functional':'THREE_SYNTHETIC_WIDGET_METAL_RUNS_BOUND', 'schema_version': 2, 'result': 'B_CLEAN_FINAL_INTEGRITY_PASS' if final else 'B_CANDIDATE_INTEGRITY_PASS',
            'scope': 'S5-I2-B', 'acceptance': acceptance,
            'source_manifest_sha256': source_hash, 'source_files': len(files),
            'gates': len(gates), 'B07_surface_feedback':feedback_results,'native_I2_acceptance': 'NOT_EXECUTED',
            'required_remaining': list(remaining)}


if __name__ == '__main__':
    p = argparse.ArgumentParser(description=__doc__)
    p.add_argument('bundle', type=Path)
    p.add_argument('--expected-source-manifest-sha256', required=True)
    p.add_argument('--expected-commit')
    p.add_argument('--expected-precommit-source-manifest-sha256')
    args = p.parse_args()
    try:
        print(json.dumps(verify(args.bundle,args.expected_source_manifest_sha256,args.expected_commit,
                                args.expected_precommit_source_manifest_sha256), ensure_ascii=False, indent=2))
    except (ValueError, KeyError, OSError, zipfile.BadZipFile, json.JSONDecodeError) as error:
        p.exit(1, 'FAIL: ' + str(error) + '\n')
