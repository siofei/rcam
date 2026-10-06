"""Bound local PMIX subcommands and only their newly created private process group."""
import json
import hashlib
import math
import os
from pathlib import Path
import re
import signal
import subprocess
import sys
import threading
import time
import uuid

from verify_s5m2_evidence import require, load, safe

LIMITS={'display-before':10,'display-active':10,'display-active-probe':10,
        'display-restored':10,'display-restored-probe':10,'window-query':10,'image':5,
        'environment-os':3,'environment-machine':3,'environment-memory':3,'environment-power':3}
GRACE=2
CLOCK_DOMAIN='darwin_uptime_raw_ns'
DISPLAY_SWIFT_PREFIX=('/usr/bin/swift','-swift-version','6','-warnings-as-errors')
MARKER_NAMES={'runner-binding.json','app-launch.json'}
CORE_DISPLAY_FIELDS={'display_id','mode_id','width','height','pixel_width','pixel_height','refresh_hz','in_mirror_set'}


def runner_clock_ns():
    """Actual shared Darwin uptime; never substitute process-relative monotonic."""
    clock=getattr(time,'CLOCK_UPTIME_RAW',None);reader=getattr(time,'clock_gettime_ns',None)
    require(sys.platform=='darwin' and type(clock) is int and callable(reader),'Darwin shared uptime unavailable')
    value=reader(clock)
    require(type(value) is int and 0<value<2**64,'invalid Darwin shared uptime')
    return value


def json_bytes(value):return (json.dumps(value,ensure_ascii=False,indent=2)+'\n').encode('utf-8')


def publish_runner_marker(directory,name,raw):
    """Commit complete immutable bytes; an existing marker is always fatal."""
    require(name in MARKER_NAMES and type(raw) is bytes,'runner marker name/bytes')
    target=Path(directory)/name;temporary=target.with_name(name+'.writing')
    with temporary.open('xb') as file:
        file.write(raw);file.flush();os.fsync(file.fileno())
    try:os.link(temporary,target)  # Atomic no-replace publication within one directory.
    finally:temporary.unlink()


def display_core(snapshot):return {key:snapshot[key] for key in CORE_DISPLAY_FIELDS}


def display_phase_names(operation):
    require(operation in ('probe','set60','restore'),'display phase operation')
    snapshot=lambda prefix:[prefix+'/'+call+'-'+edge for call in ('online','mode','mirror') for edge in ('enter','return')]
    if operation=='probe':
        full=lambda prefix:snapshot(prefix)+[prefix+'/screen-enter',prefix+'/screen-return']
        return ['runtime-enter','appkit-enter','appkit-return']+full('snapshot-before')+full('snapshot-after')+['receipt-return']
    return ['runtime-enter']+snapshot('snapshot-before')+['modes-enter','modes-return','mode-selected']+[
        call+'-'+edge for call in ('begin','configure','complete') for edge in ('enter','return')]+snapshot('snapshot-after')+['receipt-return']


def validate_display_phases(raw,operation,display_id,pid,receipt):
    """Only complete successful source-owned diagnostics can accompany acceptance."""
    from verify_pmix_workflow import parse_json
    require(type(pid) is int and pid>0 and type(display_id) is int and 0<display_id<2**32,
            'display phase expected PID/target types')
    validate_display_receipt(receipt,display_id,probe=operation=='probe')
    require(type(raw) is str and raw.endswith('\n'),'display raw phase framing')
    rows=[parse_json(line.encode()) for line in raw.splitlines()]
    require([row['phase'] for row in rows]==display_phase_names(operation),'display exact successful phase sequence')
    fields={'schema_version','event','producer_pid','uid','thread_main','clock_domain','at_ns','sequence',
            'operation','display_id','phase','cg_error','value','selected_mode_id'}
    previous=0;uid=None;selected=receipt['after']['mode_id'] if operation!='probe' else None
    for number,row in enumerate(rows,1):
        require(set(row)==fields and type(row['schema_version']) is int and row['schema_version']==1
                and row['event']=='display-phase' and type(row['producer_pid']) is int and row['producer_pid']==pid>0
                and type(row['uid']) is int and row['uid']>=0 and row['thread_main'] is True
                and row['clock_domain']==CLOCK_DOMAIN and type(row['at_ns']) is int and previous<row['at_ns']<2**64
                and type(row['sequence']) is int and row['sequence']==number and row['operation']==operation
                and type(row['display_id']) is int and row['display_id']==display_id,'display phase identity/clock/types')
        previous=row['at_ns'];uid=row['uid'] if uid is None else uid
        require(row['uid']==uid,'display phase UID changed')
        phase=row['phase'];returned=phase in ('begin-return','configure-return','complete-return')
        require((type(row['cg_error']) is int and row['cg_error']==0) if returned else row['cg_error'] is None,
                'display successful CGError return')
        after_selection=operation!='probe' and number>=display_phase_names(operation).index('mode-selected')+1
        require((type(row['selected_mode_id']) is int and row['selected_mode_id']==selected) if after_selection
                else row['selected_mode_id'] is None,'display selected mode phase binding')
        expected=None
        for prefix,side in (('snapshot-before','before'),('snapshot-after','after')):
            if phase==prefix+'/online-return':expected=1
            if phase==prefix+'/mode-return':expected=receipt[side]['mode_id']
            if phase==prefix+'/mirror-return':expected=int(receipt[side]['in_mirror_set'])
        if phase=='appkit-return':expected=2
        if phase=='mode-selected':expected=selected
        if phase=='modes-return':
            require(type(row['value']) is int and 0<row['value']<2**32,'display mode count')
        else:require((type(row['value']) is int and row['value']==expected) if expected is not None
                     else row['value'] is None,'display phase returned value')
    return rows


def write(path,value):
    temporary=path.with_name(path.name+'.writing')
    temporary.write_text(json.dumps(value,indent=2)+'\n');os.replace(temporary,path)


def group_present(pgid):
    try:os.killpg(pgid,0);return True
    except ProcessLookupError:return False


def drain_owned_group(process,signals,grace):
    # start_new_session established this command's exclusive PGID=PID before exec.
    # Never resolve by name, enumerate processes, or signal the runner/user group.
    for sig in (signal.SIGTERM,signal.SIGKILL):
        try:os.killpg(process.pid,sig);signals.append(int(sig))
        except ProcessLookupError:pass
        deadline=time.monotonic()+grace
        try:process.wait(timeout=max(.001,deadline-time.monotonic()))
        except subprocess.TimeoutExpired:pass
        # Bounded release polling of this known private group, including descendants.
        while group_present(process.pid) and time.monotonic()<deadline:
            threading.Event().wait(min(.01,max(0,deadline-time.monotonic())))
        if process.poll() is not None and not group_present(process.pid):return True
    return process.poll() is not None and not group_present(process.pid)


def owned_command(directory,label,command,*,timeout=None,check=False,grace=GRACE):
    require(label in LIMITS and type(command) is list and command and all(type(x) is str for x in command),'owned subcommand label/argv')
    limit=LIMITS[label] if timeout is None else min(timeout,LIMITS[label])
    require(type(limit) in (int,float) and 0<limit<=LIMITS[label] and 0<grace<=GRACE,'owned subcommand bounds')
    directory=Path(directory);base=directory/(label+'.subcommand');process=None
    started=time.monotonic_ns();timed_out=False;error=None;signals=[];cleaned=False
    stdout_path=base.with_suffix('.subcommand.stdout');stderr_path=base.with_suffix('.subcommand.stderr')
    try:
        with stdout_path.open('xb') as stdout,stderr_path.open('xb') as stderr:
            process=subprocess.Popen(command,stdin=subprocess.DEVNULL,stdout=stdout,stderr=stderr,start_new_session=True)
            write(directory/(label+'.subcommand-launch.json'),{'pid':process.pid,'pgid':process.pid,'private_session':True,
                  'command':command,'started_monotonic_ns':started,'timeout_seconds':limit})
            try:process.wait(timeout=limit)
            except subprocess.TimeoutExpired:timed_out=True
            if timed_out or process.poll() is None or group_present(process.pid):
                cleaned=drain_owned_group(process,signals,grace)
                if not timed_out:error='owned descendants remained after leader exit'
            else:cleaned=True
    except BaseException as exception:
        error=repr(exception)
        if process is not None:
            try:cleaned=drain_owned_group(process,signals,grace)
            except BaseException as cleanup_error:error+='; cleanup: '+repr(cleanup_error)
    finally:
        code=process.returncode if process is not None else None
        write(directory/(label+'.subcommand-process.json'),{'schema_version':2,'scope':'owned isolated subcommand; no native acceptance',
              'command':command,'pid':process.pid if process is not None else None,'pgid':process.pid if process is not None else None,
              'private_session':process is not None,'timeout_seconds':limit,'cleanup_grace_seconds':grace,
              'started_monotonic_ns':started,'finished_monotonic_ns':time.monotonic_ns(),'exit_code':code,
              'signal':-code if code is not None and code<0 else None,'timed_out':timed_out,'error':error,'signals_sent':signals,
              'joined':code is not None,'owned_group_released':cleaned,
              'result':'RETURNED' if not timed_out and error is None and cleaned else 'FAIL'})
    require(not timed_out and error is None and cleaned,'owned subcommand timeout/failure/cleanup: '+label)
    result=subprocess.CompletedProcess(command,code,stdout_path.read_bytes().decode('utf-8'),stderr_path.read_bytes().decode('utf-8'))
    if check:result.check_returncode()
    return result


def verify_command(directory,label,command,exit_code,stdout=None,stderr=None):
    row=load(safe(directory,label+'.subcommand-process.json'));launch=load(safe(directory,label+'.subcommand-launch.json'))
    require(type(row['schema_version']) is int and row['schema_version']==2 and row['scope']=='owned isolated subcommand; no native acceptance'
            and row['result']=='RETURNED' and row['private_session'] is True and row['timed_out'] is False
            and row['error'] is None and row['joined'] is True and row['owned_group_released'] is True
            and row['signals_sent']==[] and row['signal'] is None,'bounded owned subcommand successful cleanup')
    require(type(row['pid']) is int and row['pid']>0 and type(row['pgid']) is int and row['pgid']==row['pid'] and row['command']==command
            and type(row['exit_code']) is int and row['exit_code']==exit_code
            and type(row['timeout_seconds']) in (int,float) and 0<row['timeout_seconds']<=LIMITS[label]
            and type(row['cleanup_grace_seconds']) in (int,float) and 0<row['cleanup_grace_seconds']<=GRACE,
            'bounded subcommand identity/limits/exit')
    require(type(row['started_monotonic_ns']) is int and type(row['finished_monotonic_ns']) is int
            and 0<row['started_monotonic_ns']<row['finished_monotonic_ns']
            and launch=={key:row[key] for key in ('pid','pgid','private_session','command','started_monotonic_ns','timeout_seconds')},
            'bounded subcommand immediate launch/clock')
    raw_stdout=safe(directory,label+'.subcommand.stdout').read_text();raw_stderr=safe(directory,label+'.subcommand.stderr').read_text()
    require((stdout is None or raw_stdout==stdout) and (stderr is None or raw_stderr==stderr),'bounded subcommand raw output binding')
    return row


def verify_runner_markers(directory,request):
    """Native/offline marker proof. The external guard also verifies live kernels."""
    from verify_pmix_workflow import parse_json
    directory=Path(directory)
    raw=safe(directory,'runner-binding.json').read_bytes();binding=parse_json(raw)
    fields={'schema_version','event','launch_nonce','runner_pid','runner_path','runner_sha256',
            'native_directory','output_directory','run_id','source_manifest_sha256','binary_path','binary_sha256',
            'capture_producer_sha256','display_id','clock_domain','bound_at_ns'}
    require(type(binding) is dict and set(binding)==fields and type(binding['schema_version']) is int
            and binding['schema_version']==1 and binding['event']=='RUNNER_BINDING','exact early runner binding')
    require(type(binding['launch_nonce']) is str and str(uuid.UUID(binding['launch_nonce']))==binding['launch_nonce']
            and type(binding['run_id']) is str and str(uuid.UUID(binding['run_id']))==binding['run_id'],
            'runner launch nonce/run UUID')
    for key in ('runner_sha256','source_manifest_sha256','binary_sha256','capture_producer_sha256'):
        require(type(binding[key]) is str and re.fullmatch('[0-9a-f]{64}',binding[key]),'runner bound SHA '+key)
    for key in ('runner_path','native_directory','output_directory','binary_path'):
        path=binding[key]
        require(type(path) is str and Path(path).is_absolute() and str(Path(path))==path
                and '..' not in Path(path).parts,'canonical runner path '+key)
    require(Path(binding['runner_path']).name=='run_pmix_native.py'
            and binding['native_directory']!=binding['output_directory'],'runner/native/output path roles')
    require(type(binding['runner_pid']) is int and binding['runner_pid']>0
            and type(request['runner_pid']) is int and request['runner_pid']==binding['runner_pid']
            and type(binding['display_id']) is int and binding['display_id']==request['display_id']
            and binding['clock_domain']==request['runner_clock_domain']==CLOCK_DOMAIN
            and type(binding['bound_at_ns']) is int and 0<binding['bound_at_ns']<2**64,'runner binding PID/clock/target')
    require(hashlib.sha256(raw).hexdigest()==request['runner_binding_sha256']
            and all(request[key]==binding[key] for key in ('launch_nonce','runner_pid','native_directory',
                                                          'output_directory','run_id','source_manifest_sha256','display_id')),
            'request/immutable runner binding')
    manifest=safe(directory,'source-manifest.sha256').read_bytes()
    entries=dict(line.split('  ',1)[::-1] for line in manifest.decode().splitlines())
    runner=Path(__file__).with_name('run_pmix_native.py')
    before=load(safe(directory,'binary-before.json'));owned=load(safe(directory,'owned-process.json'))
    require(hashlib.sha256(manifest).hexdigest()==binding['source_manifest_sha256']
            and binding['runner_sha256']==entries['scripts/run_pmix_native.py']==hashlib.sha256(runner.read_bytes()).hexdigest()
            and before['path']==binding['binary_path'] and before['sha256']==binding['binary_sha256']
            and hashlib.sha256(safe(directory,'capture-producer').read_bytes()).hexdigest()==binding['capture_producer_sha256'],
            'runner source/app/capture exact bound bytes')
    launch_raw=safe(directory,'app-launch.json').read_bytes();launch=parse_json(launch_raw)
    launch_fields={'schema_version','event','binding_sha256','launch_nonce','runner_pid','native_directory',
                   'output_directory','run_id','display_id','clock_domain','launch_at_ns'}
    require(type(launch) is dict and set(launch)==launch_fields and type(launch['schema_version']) is int
            and launch['schema_version']==1 and launch['event']=='APP_LAUNCH'
            and launch['binding_sha256']==hashlib.sha256(raw).hexdigest()
            and all(launch[key]==binding[key] for key in ('launch_nonce','runner_pid','native_directory',
                                                         'output_directory','run_id','display_id','clock_domain')),
            'once app launch marker binding')
    require(type(launch['runner_pid']) is int and type(launch['display_id']) is int
            and type(launch['launch_at_ns']) is int and binding['bound_at_ns']<launch['launch_at_ns']<2**64
            and type(owned['runner_pid']) is int and owned['runner_pid']==binding['runner_pid']
            and type(owned['pid']) is int and owned['pid']>0 and owned['pid']!=binding['runner_pid']
            and owned['launch_nonce']==binding['launch_nonce'] and owned['clock_domain']==CLOCK_DOMAIN
            and owned['command']==[binding['binary_path']]
            and type(owned['app_started_uptime_ns']) is int
            and launch['launch_at_ns']<=owned['app_started_uptime_ns']<2**64
            and owned['binary_sha256']==binding['binary_sha256'],'actual app spawn follows launch opportunity')
    return binding,launch,owned


def verify_display_environment(directory,request,probe_source,mutator_source):
    require(type(request.get('schema_version')) is int and request['schema_version']==4,
            'explicit-target/runner-bound PMIX request schema')
    display_id=request.get('display_id')
    require(type(display_id) is int and 0<display_id<2**32,'explicit PMIX display ID')
    require(safe(directory,'display-probe.swift').read_text()==probe_source
            and safe(directory,'display.swift').read_text()==mutator_source,'reviewed distinct display helper sources')
    binding,launch,owned=verify_runner_markers(directory,request)
    query=load(safe(directory,'window-query.json'));helper=Path(query['command'][1]).parent/'display.swift'
    probe=helper.with_name('display-probe.swift')
    require(str(helper.parent)==binding['native_directory'],'display/early runner native path')
    before=load(safe(directory,'display-before.json'));active=load(safe(directory,'display-active.json'));restored=load(safe(directory,'display-restored.json'))
    active_probe=load(safe(directory,'display-active-probe.json'));restored_probe=load(safe(directory,'display-restored-probe.json'))
    require(request['display_policy'] in ('preserve','frozen-60hz'),'PMIX display policy')
    changed=request['display_policy']=='frozen-60hz'
    for receipt in (before,active_probe,restored_probe):validate_display_receipt(receipt,display_id,probe=True)
    for receipt in (active,restored):validate_display_receipt(receipt,display_id,probe=not changed)
    require(before['before']==before['after'],'initial target probe changed')
    require(display_core(active['before'])==display_core(before['after']),'active original target snapshot')
    require(display_core(restored['after'])==display_core(before['after']),'complete same-target restoration')
    require(not changed or all(snapshot['in_mirror_set'] is False for receipt in (before,active,active_probe,restored,restored_probe)
                               for snapshot in receipt.values()),'refuse linked mirrored display changes')
    require((changed and request['display_mode_change_authorized'] is True
             and abs(active['after']['refresh_hz']-60)<.01)
            or (not changed and active['after']==before['after']),'active target mode/preservation')
    require(active_probe['before']==active_probe['after']
            and display_core(active_probe['after'])==display_core(active['after']),'active mode did not survive setter exit')
    require(restored_probe['before']==restored_probe['after']==before['after'],
            'original full target mode did not survive restore helper exit')
    for key in ('width','height','pixel_width','pixel_height','backing_scale'):
        require(active_probe['after'][key]==before['after'][key],'active target actual geometry/scale')
    commands=[('display-before',[*DISPLAY_SWIFT_PREFIX,str(probe),'probe',str(display_id)],before),
              ('display-active',[*DISPLAY_SWIFT_PREFIX,str(helper if changed else probe),'set60' if changed else 'probe',str(display_id)],active),
              ('display-active-probe',[*DISPLAY_SWIFT_PREFIX,str(probe),'probe',str(display_id)],active_probe),
              ('display-restored',[*DISPLAY_SWIFT_PREFIX,str(helper if changed else probe),'restore' if changed else 'probe',str(display_id)]+
               ([str(before['after']['mode_id'])] if changed else []),restored),
              ('display-restored-probe',[*DISPLAY_SWIFT_PREFIX,str(probe),'probe',str(display_id)],restored_probe)]
    processes={};phases={}
    for label,command,expected in commands:
        processes[label]=verify_command(directory,label,command,0)
        require(load(safe(directory,label+'.subcommand.stdout'))==expected,'bounded actual display query output')
        phases[label]=validate_display_phases(safe(directory,label+'.subcommand.stderr').read_text(),command[5],display_id,
                                              processes[label]['pid'],expected)
    usage=load(safe(directory,'owned-resource-usage.json'))
    require(all(type(usage.get(k)) is int and 0<usage[k]<2**64
                for k in ('started_monotonic_ns','finished_monotonic_ns'))
            and usage['started_monotonic_ns']<usage['finished_monotonic_ns'],'display/app relative causal clock types')
    require(processes['display-before']['finished_monotonic_ns']<processes['display-active']['started_monotonic_ns']
            and processes['display-active']['finished_monotonic_ns']<processes['display-active-probe']['started_monotonic_ns']
            and processes['display-active-probe']['finished_monotonic_ns']<usage['started_monotonic_ns']
            and usage['finished_monotonic_ns']<processes['display-restored']['started_monotonic_ns']
            and processes['display-restored']['finished_monotonic_ns']<processes['display-restored-probe']['started_monotonic_ns'],
            'setter join/post-exit probe/app lifetime/restoration causal binding')
    require(binding['bound_at_ns']<phases['display-before'][0]['at_ns']
            and phases['display-before'][-1]['at_ns']<phases['display-active'][0]['at_ns']
            and phases['display-active'][-1]['at_ns']<phases['display-active-probe'][0]['at_ns']
            and phases['display-active-probe'][-1]['at_ns']<launch['launch_at_ns']
            and launch['launch_at_ns']<=owned['app_started_uptime_ns']<phases['display-restored'][0]['at_ns']
            and phases['display-restored'][-1]['at_ns']<phases['display-restored-probe'][0]['at_ns'],
            'shared uptime display phases/immutable launch causality')
    environment=load(safe(directory,'environment.json'))
    for key,label,command in [('os','environment-os',['/usr/bin/sw_vers']),('machine','environment-machine',['/usr/bin/uname','-m']),
                              ('memory_bytes','environment-memory',['/usr/sbin/sysctl','-n','hw.memsize']),('power','environment-power',['/usr/bin/pmset','-g','custom'])]:
        verify_command(directory,label,command,0,stderr='')
        raw=safe(directory,label+'.subcommand.stdout').read_text()
        actual=int(raw) if key=='memory_bytes' else (raw.strip() if key=='machine' else raw)
        require(actual==environment[key],'bounded actual environment output: '+key)


def validate_display_receipt(receipt,display_id,*,probe=True):
    """The verified operation selects exact 9-field probe or 8-field mutator data."""
    require(type(probe) is bool,'display receipt operation role')
    require(type(display_id) is int and 0<display_id<2**32,'explicit PMIX display ID')
    require(type(receipt) is dict and set(receipt)=={'before','after'},'target display receipt fields')
    fields=CORE_DISPLAY_FIELDS|({'backing_scale'} if probe else set())
    for snapshot in receipt.values():
        require(type(snapshot) is dict and set(snapshot)==fields,'target display snapshot fields')
        require(type(snapshot['display_id']) is int and snapshot['display_id']==display_id,
                'target display snapshot identity')
        require(type(snapshot['mode_id']) is int and 0<=snapshot['mode_id']<2**32,'target display mode ID')
        require(type(snapshot['in_mirror_set']) is bool,'target display mirror status type')
        require(all(type(snapshot[k]) is int and snapshot[k]>0
                    for k in ('width','height','pixel_width','pixel_height')),'target display dimensions')
        require(type(snapshot['refresh_hz']) in (int,float) and math.isfinite(snapshot['refresh_hz'])
                and snapshot['refresh_hz']>0,'target display rate')
        if probe:require(type(snapshot['backing_scale']) in (int,float) and math.isfinite(snapshot['backing_scale'])
                         and snapshot['backing_scale']>0,'target display actual scale')
