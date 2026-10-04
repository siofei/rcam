"""Compile the reviewed Mac producer and test actual synthetic MOV finalization.

No owned-window capture, GUI, or display setting is used by this preflight.
"""
import argparse
import hashlib
import json
import platform
import subprocess
import time
from pathlib import Path

from pmix_capture_swift import SOURCE
from verify_pmix_capture import mov_info
from verify_s5m2_evidence import require

CASES = [('short', 0, 1), ('normal', 0, 8), ('no-frames', 2, 0),
         ('stop-before-ready', 2, 0), ('fail-after-ready', 2, 0), ('invalid-pts', 2, 0)]
ROOT = Path(__file__).resolve().parents[1]

def sha(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()

def write(path, value):
    path.write_text(json.dumps(value, indent=2)+'\n')

def validate_probe(value, selection):
    require(type(value) is dict and type(value['schema_version']) is int and value['schema_version']==2
            and value['event']=='initialization-probe' and value['scope']=='initialization-only'
            and value['query_kind']=='currentProcess-no-consent' and value['native_proof'] is False,
            'initialization-only probe scope')
    require(value['selection']==selection and value['writer_created'] is False and value['active'] is False
            and all(type(value[key]) is int and value[key]==0 for key in ('streams_created','streams_started','frames_received','own_windows')),
            'probe must not create GUI/writer/stream/frame')
    require(type(value['producer_pid']) is int and value['producer_pid']>0
            and type(value['existing_screen_access']) is bool, 'same helper probe PID/access')
    trace=value['trace'];stages=[row['stage'] for row in trace]
    prefix=['bootstrap-attempt','bootstrap','permission-check']
    require(stages[:3]==prefix and stages[-1]=='probe-terminal', 'actual bootstrap/terminal trace')
    require(all(row['thread_main'] is True and type(row['at_ns']) is int and row['at_ns']>0
                and type(row['producer_pid']) is int and row['producer_pid']==value['producer_pid'] for row in trace)
            and all(a['at_ns']<b['at_ns'] for a,b in zip(trace,trace[1:])), 'actual mainthread/PID/clock probe')
    require(type(trace[0]['after_policy_raw']) is int and trace[0]['after_policy_raw']==2
            and trace[0]['active'] is False and trace[0]['own_windows']==0
            and trace[1]['activation_policy']=='prohibited' and trace[1]['active'] is False and trace[1]['own_windows']==0
            and trace[2]['request_called'] is False and trace[2]['existing_access']==value['existing_screen_access'],
            'non-GUI non-prompting bootstrap/query')
    terminal=trace[-1]
    require(terminal['active'] is False and type(terminal['own_windows']) is int and terminal['own_windows']==0
            and terminal['filter_constructor']==value['filter_constructor'] and terminal['result']==value['result'],
            'actual terminal UI/constructor/result boundary')
    if value['result']=='BLOCKED' and value.get('blocked_reason')=='api-unavailable':
        require(stages==prefix+['probe-terminal'] and value['filter_constructor']=='NOT_EXECUTED', 'unavailable API stays blocked')
        return value['result']
    require(stages[3]=='metadata-returned' and trace[3]['query_kind']=='currentProcess-no-consent', 'actual metadata trace')
    for key in ('window_count','nonzero_id_count','onscreen_count','layer0_count','eligible_count'):
        require(type(trace[3][key]) is int and 0<=trace[3][key]<=trace[3]['window_count'], 'actual metadata eligibility counts')
    if value['result']=='INITIALIZATION_ONLY_PASS':
        require(selection=='real-on-screen' and value['filter_constructor']=='EXECUTED'
                and stages==prefix+['metadata-returned','filter-before','filter-after','probe-terminal']
                and trace[3]['eligible_count']>0 and trace[4]['eligible_real_window'] is True
                and trace[4]['query_kind']==trace[5]['query_kind']=='currentProcess-no-consent'
                and trace[5]['constructor']=='desktopIndependentWindow', 'genuine exact ctor initialization-only proof')
    else:
        require(value['result']=='BLOCKED' and value['blocked_reason']=='no-eligible-window'
                and value['filter_constructor']=='NOT_EXECUTED' and stages==prefix+['metadata-returned','probe-terminal']
                and (selection=='own-window-none' or trace[3]['eligible_count']==0),
                'missing real window stays BLOCKED; implementation FAIL cannot substitute')
    return value['result']


def initialization_readiness(probe):
    state=validate_probe(probe,'real-on-screen')
    return 'PASS' if state=='INITIALIZATION_ONLY_PASS' and probe['existing_screen_access'] is True else 'BLOCKED'


def helper_run(command, out, name, timeout):
    """Only the owned child; preserve failure output and actual join on all paths."""
    process=None;stdout=stderr=b'';timed_out=False;error=None;started=time.monotonic_ns()
    try:
        process=subprocess.Popen(command,stdout=subprocess.PIPE,stderr=subprocess.PIPE)
        write(out/(name+'.launch.json'),{'pid':process.pid,'command':command,'started_monotonic_ns':started})
        try:stdout,stderr=process.communicate(timeout=timeout)
        except subprocess.TimeoutExpired:
            timed_out=True;process.terminate()
            try:stdout,stderr=process.communicate(timeout=2)
            except subprocess.TimeoutExpired:process.kill();stdout,stderr=process.communicate()
    except BaseException as exception:
        error=repr(exception)
        if process is not None and process.poll() is None:
            process.kill();stdout,stderr=process.communicate()
    finally:
        (out/(name+'.stdout')).write_bytes(stdout);(out/(name+'.stderr')).write_bytes(stderr)
        code=process.returncode if process is not None else None
        write(out/(name+'.process.json'),{'schema_version':2,'scope':'owned background helper process; no native acceptance',
              'pid':process.pid if process is not None else None,'command':command,'exit_code':code,
              'signal':-code if code is not None and code<0 else None,'timed_out':timed_out,
              'joined':code is not None,'error':error,'started_monotonic_ns':started,'finished_monotonic_ns':time.monotonic_ns(),
              'result':'FAIL' if timed_out or error or code is None or code<0 else 'RETURNED'})
    require(not timed_out and error is None, 'background helper timeout/launch/communication failure: '+name)
    return subprocess.CompletedProcess(command,code,stdout.decode('utf-8'),stderr.decode('utf-8'))


def validate_process(row, launch, command, expected_exit, pid):
    require(type(row['schema_version']) is int and row['schema_version']==2
            and row['scope']=='owned background helper process; no native acceptance'
            and row['result']=='RETURNED' and row['timed_out'] is False and row['joined'] is True
            and row['error'] is None and row['signal'] is None, 'actual background helper non-timeout join')
    require(type(row['pid']) is int and row['pid']==pid>0 and row['command']==command
            and type(row['exit_code']) is int and row['exit_code']==expected_exit
            and type(row['started_monotonic_ns']) is int and row['started_monotonic_ns']>0
            and type(row['finished_monotonic_ns']) is int and row['finished_monotonic_ns']>row['started_monotonic_ns']
            and launch=={key:row[key] for key in ('pid','command','started_monotonic_ns')}, 'actual helper launch/PID/exit/clock')


def run(out):
    require(platform.system() == 'Darwin', 'Mac capture producer preflight required')
    out.mkdir(parents=True, exist_ok=False)
    source = out/'capture.swift'; source.write_text(SOURCE)
    binary = out/'capture-producer'
    command = ['/usr/bin/swiftc', '-swift-version', '5', '-parse-as-library', str(source), '-o', str(binary)]
    result = subprocess.run(command, text=True, capture_output=True, timeout=120)
    (out/'compile.stdout').write_text(result.stdout); (out/'compile.stderr').write_text(result.stderr)
    write(out/'compile.json', {'command': command, 'exit_code': result.returncode,
          'swift_version': subprocess.check_output(['/usr/bin/swiftc', '--version'], text=True),
          'sdk': subprocess.check_output(['/usr/bin/xcrun', '--show-sdk-path'], text=True)})
    require(result.returncode == 0, 'capture producer compilation failed')
    probes=[]
    for mode,selection in [('--probe-initialization','real-on-screen'),('--probe-no-window','own-window-none')]:
        command=[str(binary),mode];name=mode[2:];result=helper_run(command,out,name,15)
        events=[json.loads(line) for line in result.stdout.splitlines()]
        require(not result.stderr and len(events)==1, 'actual initialization probe raw output')
        state=validate_probe(events[0],selection)
        validate_process(json.loads((out/(name+'.process.json')).read_text()),json.loads((out/(name+'.launch.json')).read_text()),
                         command,result.returncode,events[0]['producer_pid'])
        require(type(result.returncode) is int and result.returncode==(0 if state=='INITIALIZATION_ONLY_PASS' else 2), 'actual initialization probe exit')
        if selection=='own-window-none':require(state=='BLOCKED', 'windowless helper negative must remain blocked')
        probes.append({'mode':mode,'command':command,'exit_code':result.returncode,'receipt':events[0]})
    rows = []
    for mode, expected_exit, samples in CASES:
        movie = out/(mode+'.mov')
        command = [str(binary), '--self-test', mode, str(movie)]
        result = helper_run(command,out,mode,30)
        events = [json.loads(line) for line in result.stdout.splitlines()]
        require(result.returncode == expected_exit and not result.stderr, 'synthetic writer exit/stderr: '+mode)
        require(events and all((row.get('source_kind') == 'synthetic-self-test' and row['app_pid'] == row['window_id'] == 0)
                              or (row.get('event') == 'failure' and set(row) == {'event','error','schema_version'} and row['schema_version'] == 2)
                              for row in events), 'synthetic scope identity: '+mode)
        info = None
        if samples:
            require([row['event'] for row in events] == ['ready', 'finished'], 'synthetic lifecycle sequence: '+mode)
            first, last = events
            require(first['frame_status'] == 'complete' and first['sample_append_succeeded'] is True
                    and last['writer_status'] == 'completed' and last['accepted_samples'] == samples
                    and all(last[key] is True for key in ('stream_stopped', 'sample_queue_drained', 'input_marked_finished',
                                                        'finish_writing_callback_received')), 'synthetic finalization: '+mode)
            info = mov_info(movie)
            require(info['samples'] == samples and (info['width'], info['height']) == (64, 48)
                    and info['duration_seconds'] > 0 and movie.stat().st_size == last['output_bytes'], 'synthetic MOV: '+mode)
        else:
            require(any(row['event'] == 'failure' for row in events)
                    and not any(row['event'] == 'finished' for row in events)
                    and not movie.exists(), 'failed writer must not produce successful MOV: '+mode)
        process_receipt=json.loads((out/(mode+'.process.json')).read_text())
        validate_process(process_receipt,json.loads((out/(mode+'.launch.json')).read_text()),command,result.returncode,process_receipt['pid'])
        require(all(row.get('producer_pid',process_receipt['pid'])==process_receipt['pid'] for row in events), 'writer actual child PID')
        rows.append({'mode': mode, 'command': command, 'exit_code': result.returncode, 'producer_pid':process_receipt['pid'], 'samples': samples,
                     'stdout_sha256': sha(out/(mode+'.stdout')), 'stderr_sha256': sha(out/(mode+'.stderr')),
                     'events': events, 'movie': info, 'movie_sha256': sha(movie) if movie.exists() else None})
    receipt = {'result': initialization_readiness(probes[0]['receipt']),
               'background_writer_and_refusal_tests':'PASS', 'probes':probes,
               'scope': 'background initialization metadata and synthetic writer only; no stream capture or GUI',
               'source_manifest_sha256': sha(ROOT/'MANIFEST.sha256'), 'producer_sha256': sha(binary),
               'producer_source_sha256': sha(source), 'cases': rows,
               'files': {p.name: sha(p) for p in sorted(out.iterdir()) if p.is_file()}}
    write(out/'RESULTS.json', receipt)
    return receipt

if __name__ == '__main__':
    parser = argparse.ArgumentParser(description=__doc__); parser.add_argument('--out', required=True, type=Path)
    args = parser.parse_args(); result=run(args.out.resolve()); print(json.dumps(result)); raise SystemExit(0 if result['result']=='PASS' else 2)
