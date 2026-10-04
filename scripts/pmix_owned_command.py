"""Bound local PMIX subcommands and only their newly created private process group."""
import json
import os
from pathlib import Path
import signal
import subprocess
import threading
import time

from verify_s5m2_evidence import require, load, safe

LIMITS={'display-before':10,'display-active':10,'display-restored':10,'window-query':10,'image':5,
        'environment-os':3,'environment-machine':3,'environment-memory':3,'environment-power':3}
GRACE=2


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


def verify_display_environment(directory,request,display_source):
    require(safe(directory,'display.swift').read_text()==display_source,'reviewed display helper source')
    query=load(safe(directory,'window-query.json'));helper=Path(query['command'][1]).parent/'display.swift'
    before=load(safe(directory,'display-before.json'));active=load(safe(directory,'display-active.json'));restored=load(safe(directory,'display-restored.json'))
    changed=request['display_policy']=='frozen-60hz'
    commands=[('display-before',['/usr/bin/swift',str(helper),'probe'],before),
              ('display-active',['/usr/bin/swift',str(helper),'set60' if changed else 'probe'],active),
              ('display-restored',['/usr/bin/swift',str(helper),'restore' if changed else 'probe']+
               ([str(before['after']['mode_id'])] if changed else []),restored)]
    for label,command,expected in commands:
        verify_command(directory,label,command,0,stderr='')
        require(json.loads(safe(directory,label+'.subcommand.stdout').read_text())==expected,'bounded actual display query output')
    environment=load(safe(directory,'environment.json'))
    for key,label,command in [('os','environment-os',['/usr/bin/sw_vers']),('machine','environment-machine',['/usr/bin/uname','-m']),
                              ('memory_bytes','environment-memory',['/usr/sbin/sysctl','-n','hw.memsize']),('power','environment-power',['/usr/bin/pmset','-g','custom'])]:
        verify_command(directory,label,command,0,stderr='')
        raw=safe(directory,label+'.subcommand.stdout').read_text()
        actual=int(raw) if key=='memory_bytes' else (raw.strip() if key=='machine' else raw)
        require(actual==environment[key],'bounded actual environment output: '+key)
