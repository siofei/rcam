"""Unit guard tests: resealed ledgers, evidence layout and capture producers.

Small media are parser fixtures, never genuine native acquisition evidence.
"""
import copy
import hashlib
import json
from pathlib import Path
import struct
import tempfile
import unittest
import zlib
import sys
from verify_pmix_evidence import evidence_layout, gate_ledger
from verify_pmix_capture import capture_receipts, mov_info, png_info
from run_pmix_native import WINDOW_SWIFT
from pmix_capture_swift import SOURCE
from run_pmix_capture_preflight import validate_probe, initialization_readiness, helper_run, validate_process
from test_pmix_owned_command import OwnedCommands
PRODUCER_SHA=hashlib.sha256(b'unit-producer').hexdigest()


def box(kind,data):return struct.pack('>I4s',len(data)+8,kind)+data

def movie():
    # Ten one-second samples; chunk payload offsets are independently bounded.
    mdat=box(b'mdat',(struct.pack('>I',1)+b'\x65')*10)
    entry=bytearray(86);struct.pack_into('>HH',entry,32,2,2)
    entry+=box(b'avcC',b'\x01\x64\x00\x1f\xff\xe1\x00\x01\x67\x01\x00\x01\x68')
    struct.pack_into('>I4s',entry,0,len(entry),b'avc1')
    stsd=box(b'stsd',b'\0'*4+struct.pack('>I',1)+entry)
    stts=box(b'stts',b'\0'*4+struct.pack('>III',1,10,1000))
    stsz=box(b'stsz',b'\0'*4+struct.pack('>II',5,10))
    stsc=box(b'stsc',b'\0'*4+struct.pack('>IIII',1,1,10,1))
    stco=box(b'stco',b'\0'*4+struct.pack('>II',1,8))
    mdhd=box(b'mdhd',b'\0'*12+struct.pack('>II',1000,10000)+b'\0'*4)
    hdlr=box(b'hdlr',b'\0'*8+b'vide'+b'\0'*12)
    return mdat+box(b'moov',box(b'trak',box(b'mdia',mdhd+hdlr+box(b'minf',box(b'stbl',stsd+stts+stsz+stsc+stco)))))


def png():
    def chunk(kind,data):return struct.pack('>I',len(data))+kind+data+struct.pack('>I',zlib.crc32(kind+data)&0xffffffff)
    return b'\x89PNG\r\n\x1a\n'+chunk(b'IHDR',struct.pack('>IIBBBBB',2,2,8,2,0,0,0))+chunk(b'IDAT',zlib.compress(b'\0'+b'\xff'*6+b'\0'+b'\xff'*6))+chunk(b'IEND',b'')


def write(path,value):path.write_text(json.dumps(value)+'\n')


class Guards(unittest.TestCase):
    def setUp(self):
        self.temp=tempfile.TemporaryDirectory(prefix='rcam-pmix-guards-')
        self.root=Path(self.temp.name)
    def tearDown(self):self.temp.cleanup()
    def reject(self,fn,reason):
        with self.assertRaisesRegex(ValueError,reason):fn()
    def envelope(self):
        for name in ('REVIEW.json','Source.zip','Evidence_MANIFEST.sha256'):(self.root/name).write_bytes(b'')
        (self.root/'gates').mkdir()
    def test_layout_positive_and_extra_resealed(self):
        self.envelope();evidence_layout(self.root)
        (self.root/'unexpected.txt').write_bytes(b'extra');(self.root/'Evidence_MANIFEST.sha256').write_text('resealed')
        self.reject(lambda:evidence_layout(self.root),'canonical evidence root layout')
    def test_layout_symlink_and_wrong_type(self):
        self.envelope();(self.root/'native').symlink_to(self.root/'gates',target_is_directory=True)
        self.reject(lambda:evidence_layout(self.root),'entry type')
    def test_ledger_positive_and_fmt_resealed(self):
        path=self.root/'gates.json';log=self.root/'fmt.log';log.write_bytes(b'')
        rows=[{'id':'fmt','exit_code':0,'sha256':hashlib.sha256(log.read_bytes()).hexdigest()}];write(path,rows)
        trusted=hashlib.sha256(path.read_bytes()).hexdigest();self.assertEqual(gate_ledger(path,trusted),rows)
        log.write_bytes(b'fabricated passing log\n');rows[0]['sha256']=hashlib.sha256(log.read_bytes()).hexdigest();write(path,rows)
        self.reject(lambda:gate_ledger(path,trusted),'external frozen gate ledger')
    def test_ledger_strict_digest(self):
        self.reject(lambda:gate_ledger(self.root/'missing',False),'digest format')
    def test_media_positive_and_truncation(self):
        video=self.root/'native-window.mov';video.write_bytes(movie());self.assertEqual(mov_info(video)['samples'],10)
        image=self.root/'native-window.png';image.write_bytes(png());self.assertEqual(png_info(image),{'width':2,'height':2})
        video.write_bytes(movie()[:-1]);self.reject(lambda:mov_info(video),'truncation')
        image.write_bytes(png()[:-1]);self.reject(lambda:png_info(image),'bounds|truncated')
    def test_media_fake_larger_than_old_threshold(self):
        video=self.root/'native-window.mov';video.write_bytes(b'not-video'*32)
        self.reject(lambda:mov_info(video),'bounds|truncation')
        image=self.root/'native-window.png';image.write_bytes(b'not-png'*32)
        self.reject(lambda:png_info(image),'signature')
    def test_movie_outside_sample_data_and_duration(self):
        path=self.root/'native-window.mov';raw=bytearray(movie());at=raw.index(b'stco');struct.pack_into('>I',raw,at+12,123456);path.write_bytes(raw)
        self.reject(lambda:mov_info(path),'outside mdat')
        raw=bytearray(movie());at=raw.index(b'mdhd');struct.pack_into('>I',raw,at+20,90000);path.write_bytes(raw)
        self.reject(lambda:mov_info(path),'duration coverage')
    def receipts(self):
        pid=123;wid=456;base=1000000000;runid='unit-parser-fixture'
        window={'window_id':wid,'owner_pid':pid,'layer':0,'bounds':{'X':0,'Y':0,'Width':2,'Height':2}}
        query={'pid':pid,'command':['/usr/bin/swift',str(self.root/'window.swift'),str(pid)],'exit_code':0,'started_monotonic_ns':base+1,'finished_monotonic_ns':base+2,'windows':[window],'stdout':json.dumps([window]),'stderr':''}
        image={'app_pid':pid,'window_id':wid,'exit_code':0,'stdout':'','stderr':'','command':['/usr/sbin/screencapture','-x','-o','-l',str(wid),str(self.root/'native-window.png')],'started_monotonic_ns':base+3,'finished_monotonic_ns':base+4}
        first={'schema_version':2,'event':'ready','producer_pid':789,'app_pid':pid,'window_id':wid,'run_id':runid,'source_kind':'owned-window-sck','clock':'DispatchTime.uptimeNanoseconds','producer_started_ns':100,'at_ns':200,'accepted_samples':1,'width':2,'height':2,'frame_status':'complete','writer_status':'writing','sample_append_succeeded':True,'sample_pts_value':1,'sample_pts_timescale':1}
        final=dict(first,event='finished',at_ns=1200,accepted_samples=10,stop_requested_ns=1100,stream_stopped=True,sample_queue_drained=True,input_marked_finished=True,writer_status='completed',finish_writing_callback_received=True,first_pts_value=1,first_pts_timescale=1,last_pts_value=10,last_pts_timescale=1,output_bytes=len(movie()))
        video={'app_pid':pid,'window_id':wid,'exit_code':0,'stdout':json.dumps(first)+'\n'+json.dumps(final)+'\n','stderr':'','pid':789,'command':[str(self.root/'capture-producer'),'--owned-window',str(pid),str(wid),str(self.root/'native-window.mov'),runid,'H264-MOV','2','2'],'producer_sha256':PRODUCER_SHA,'started_monotonic_ns':base+5,'ready_monotonic_ns':base+6,'stop_requested_monotonic_ns':base+10000000000,'finished_monotonic_ns':base+11000000000,'joined_before_app_release':True,'finalization':'SCStream.stopCapture + drain + AVAssetWriter.finishWriting + process.join','first_frame':first,'finished_event':final}
        ready={'app_pid':pid,'run_id':runid,'video_pid':789,'video_started_monotonic_ns':base+5,'ready_monotonic_ns':base+6,'first_frame':first}
        done={'app_pid':pid,'run_id':runid,'frame_id':3,'at_ns':9000000100}
        complete={'app_pid':pid,'run_id':runid,'video_pid':789,'success':True,'producer_finished_monotonic_ns':video['finished_monotonic_ns'],'release_monotonic_ns':base+12000000000,'movie_sha256':hashlib.sha256(movie()).hexdigest()}
        marker={'app_pid':pid,'run_id':runid,'frame_id':1}
        data={'owned-process.json':{'pid':pid,'command':['/tmp/unit-binary']},'owned-resource-usage.json':{'pid':pid,'exit_code':0,'started_monotonic_ns':base,'finished_monotonic_ns':base+13000000000},'window-query.json':query,'window.json':[window],'image-command.json':image,'video-command.json':video,'capture-ready.json':ready,'display-active.json':{'after':{'backing_scale':1}},'protocol-done.json':done,'capture-complete.json':complete,'window-ready.json':marker,'capture-events.json':[first,final]}
        data['owned-producer.json']={key:video[key] for key in ('pid','app_pid','window_id','command','producer_sha256','started_monotonic_ns')}
        data['owned-producer.json'].update(pgid=789,private_session=True)
        data['producer-cleanup.json']={'pid':789,'exit_code':0,'signal':None,'joined':True,'reader_joined':True,'control_state':'COMPLETE','finished_monotonic_ns':base+14000000000,'scope':'owned producer actual cleanup; never native success'}
        data['producer-cleanup.json'].update(pgid=789,private_session=True,owned_group_released=True,signals_sent=[])
        data['owned-cleanup.json']={'app_pid':pid,'app_exit_code':0,'producer_pid':789,'producer_exit_code':0,'producer_reader_joined':True,'finished_monotonic_ns':base+15000000000,'scope':'cleanup status; resource accounting only from owned wait4'}
        # Invented bounded-command receipts, solely for portable parser tests.
        for label,record,child in [('window-query',query,901),('image',image,902)]:
            bounded={'schema_version':2,'scope':'owned isolated subcommand; no native acceptance','command':record['command'],
                     'pid':child,'pgid':child,'private_session':True,'timeout_seconds':5,'cleanup_grace_seconds':2,
                     'started_monotonic_ns':record['started_monotonic_ns'],'finished_monotonic_ns':record['finished_monotonic_ns'],
                     'exit_code':0,'signal':None,'timed_out':False,'error':None,'signals_sent':[],'joined':True,'owned_group_released':True,'result':'RETURNED'}
            write(self.root/(label+'.subcommand-process.json'),bounded)
            write(self.root/(label+'.subcommand-launch.json'),{key:bounded[key] for key in ('pid','pgid','private_session','command','started_monotonic_ns','timeout_seconds')})
            (self.root/(label+'.subcommand.stdout')).write_text(record['stdout']);(self.root/(label+'.subcommand.stderr')).write_text(record['stderr'])
        # Parser-only synthetic receipts exercise the portable guards; never native evidence.
        stages=['bootstrap-attempt','bootstrap','permission-check','filter-before','filter-after','stream-created','stream-started','stream-stop-completed','writer-terminal']
        clocks=[50,60,110,120,130,140,210,1150,1201]
        extras=[{'after_policy_raw':2,'active':False,'own_windows':0},{'activation_policy':'prohibited','active':False,'own_windows':0},
                {'existing_access':True,'request_called':False},{'query_kind':'excludingDesktopWindows-owned','eligible_real_window':True},
                {'query_kind':'excludingDesktopWindows-owned','constructor':'desktopIndependentWindow'},{'app_pid':pid,'window_id':wid},{},{},{'terminal':'completed'}]
        platform=[dict(stage=stage,producer_pid=789,thread_main=i<7,at_ns=clock,**extra) for i,(stage,clock,extra) in enumerate(zip(stages,clocks,extras))]
        (self.root/'capture-platform.jsonl').write_text(''.join(json.dumps(row)+'\n' for row in platform))
        for name,value in data.items():write(self.root/name,value)
        (self.root/'window.swift').write_text(WINDOW_SWIFT);(self.root/'capture.swift').write_text(SOURCE);(self.root/'capture-producer').write_bytes(b'unit-producer');(self.root/'capture-stdout.log').write_text(video['stdout']);(self.root/'capture-stderr.log').write_text('')
        (self.root/'native-window.png').write_bytes(png());(self.root/'native-window.mov').write_bytes(movie())
        report={'pid':pid,'request':{'run_id':runid},'events':[{'label':'window-ready','at_ns':50,'data':marker},{'label':'capture-ready','at_ns':100,'data':ready},{'label':'workflow-opened','at_ns':200},{'label':'protocol-end','at_ns':done['at_ns'],'frame_id':3,'data':done},{'label':'capture-complete','at_ns':10000000100,'data':complete}],'frames':[{'input_ns':11000000100}]}
        return data,report
    def test_capture_receipts_positive(self):
        _,r=self.receipts();self.assertFalse(capture_receipts(self.root,r,PRODUCER_SHA)['codec_decode_verified'])
    def test_capture_producer_resealed_attacks(self):
        data,r=self.receipts()
        attacks=[('video-command.json','exit_code',17,'capture exit'),('video-command.json','app_pid',999,'producer binding'),('video-command.json','pid',123,'capture exit'),('video-command.json','finalization','unowned','finalization'),('image-command.json','exit_code',17,'image capture'),('window-query.json','pid',999,'PID/exit'),('capture-ready.json','video_pid',999,'ready/completion owned producers'),('owned-resource-usage.json','finished_monotonic_ns',999,'clock ordering'),('video-command.json','joined_before_app_release',False,'finalization/join')]
        for name,key,value,reason in attacks:
            with self.subTest(name=name,key=key):
                changed=copy.deepcopy(data[name]);changed[key]=value;write(self.root/name,changed)
                self.reject(lambda:capture_receipts(self.root,r,PRODUCER_SHA),reason);write(self.root/name,data[name]);capture_receipts(self.root,r,PRODUCER_SHA)
        changed=copy.deepcopy(data['video-command.json']);changed['command'][3]='999';write(self.root/'video-command.json',changed)
        self.reject(lambda:capture_receipts(self.root,r,PRODUCER_SHA),'command/window binding')
    def test_window_owner_resealed(self):
        data,r=self.receipts();data['window.json'][0]['owner_pid']=999
        data['window-query.json']['windows']=data['window.json'];data['window-query.json']['stdout']=json.dumps(data['window.json'])
        for name in ('window.json','window-query.json'):write(self.root/name,data[name])
        self.reject(lambda:capture_receipts(self.root,r,PRODUCER_SHA),'actual owner')
    def test_video_truncation_resealed(self):
        _,r=self.receipts();(self.root/'native-window.mov').write_bytes(b'not-video'*32)
        r['events'][-1]['data']['movie_sha256']=hashlib.sha256(b'not-video'*32).hexdigest();write(self.root/'capture-complete.json',r['events'][-1]['data'])
        self.reject(lambda:capture_receipts(self.root,r,PRODUCER_SHA),'bounds|truncation')
    def test_window_query_producer_resealed(self):
        _,r=self.receipts();(self.root/'window.swift').write_text('fabricated window output')
        self.reject(lambda:capture_receipts(self.root,r,PRODUCER_SHA),'reviewed window/capture producer')
    def test_movie_sample_nal_resealed(self):
        path=self.root/'native-window.mov';raw=bytearray(movie());struct.pack_into('>I',raw,8,999);path.write_bytes(raw)
        self.reject(lambda:mov_info(path),'NAL bounds')

    def probe_fixture(self, blocked=False):
        # Entirely invented parser input; does not exercise AppKit or earn initialization proof.
        stages=['bootstrap-attempt','bootstrap','permission-check','metadata-returned']
        extras=[{'after_policy_raw':2,'active':False,'own_windows':0},
                {'activation_policy':'prohibited','active':False,'own_windows':0},
                {'existing_access':True,'request_called':False},
                {'query_kind':'currentProcess-no-consent','window_count':1,'nonzero_id_count':1,
                 'onscreen_count':1,'layer0_count':1,'eligible_count':0 if blocked else 1}]
        result='BLOCKED' if blocked else 'INITIALIZATION_ONLY_PASS';ctor='NOT_EXECUTED' if blocked else 'EXECUTED'
        if not blocked:
            stages+=['filter-before','filter-after'];extras+=[{'query_kind':'currentProcess-no-consent','eligible_real_window':True},
                {'query_kind':'currentProcess-no-consent','constructor':'desktopIndependentWindow'}]
        stages+=['probe-terminal'];extras+=[{'active':False,'own_windows':0,'filter_constructor':ctor,'result':result}]
        value={'schema_version':2,'event':'initialization-probe','scope':'initialization-only','query_kind':'currentProcess-no-consent',
               'native_proof':False,'selection':'real-on-screen','writer_created':False,'active':False,'own_windows':0,
               'streams_created':0,'streams_started':0,'frames_received':0,'producer_pid':789,'existing_screen_access':True,
               'result':result,'filter_constructor':ctor,
               'trace':[dict(stage=stage,producer_pid=789,thread_main=True,at_ns=i+1,**extra) for i,(stage,extra) in enumerate(zip(stages,extras))]}
        if blocked:value.update(blocked_reason='no-eligible-window',error='ProbeBlocked(reason: no-eligible-window)')
        return value

    def test_probe_initialization_and_blocked_distinction(self):
        self.assertEqual(validate_probe(self.probe_fixture(),'real-on-screen'),'INITIALIZATION_ONLY_PASS')
        self.assertEqual(validate_probe(self.probe_fixture(True),'real-on-screen'),'BLOCKED')
        failed=self.probe_fixture();failed['result']='FAIL';failed['trace'][-1]['result']='FAIL'
        self.reject(lambda:validate_probe(failed,'real-on-screen'),'implementation FAIL')

    def test_probe_scope_thread_and_constructor_attacks(self):
        changes=[('scope','native'),('native_proof',True),('writer_created',True),('streams_started',1),
                 ('frames_received',1),('own_windows',1),('active',True),('filter_constructor','NOT_EXECUTED')]
        for key,value in changes:
            with self.subTest(key=key):
                row=self.probe_fixture();row[key]=value
                self.reject(lambda:validate_probe(row,'real-on-screen'),'probe|boundary|scope')
        for index,key,value in [(0,'thread_main',False),(2,'request_called',True),(4,'eligible_real_window',False),
                                (5,'constructor','other'),(6,'own_windows',1)]:
            with self.subTest(index=index,key=key):
                row=self.probe_fixture();row['trace'][index][key]=value
                self.reject(lambda:validate_probe(row,'real-on-screen'),'probe|bootstrap|proof|boundary')
        row=self.probe_fixture(True);row['result']='INITIALIZATION_ONLY_PASS';row['trace'][-1]['result']=row['result']
        self.reject(lambda:validate_probe(row,'real-on-screen'),'exact ctor')

    def test_constructor_without_screen_access_still_blocks_readiness(self):
        value=self.probe_fixture();value['existing_screen_access']=False;value['trace'][2]['existing_access']=False
        self.assertEqual(validate_probe(value,'real-on-screen'),'INITIALIZATION_ONLY_PASS')
        self.assertEqual(initialization_readiness(value),'BLOCKED')
        self.assertEqual(initialization_readiness(self.probe_fixture()),'PASS')
        self.assertEqual(initialization_readiness(self.probe_fixture(True)),'BLOCKED')

    def test_native_platform_thread_probe_and_late_error_attacks(self):
        _,report=self.receipts();path=self.root/'capture-platform.jsonl'
        original=[json.loads(line) for line in path.read_text().splitlines()]
        for index,key,value in [(3,'thread_main',False),(2,'existing_access',False),(3,'query_kind','currentProcess-no-consent'),
                                (4,'constructor','other'),(5,'app_pid',999),(8,'terminal','failed')]:
            with self.subTest(index=index,key=key):
                rows=copy.deepcopy(original);rows[index][key]=value
                path.write_text(''.join(json.dumps(row)+'\n' for row in rows))
                self.reject(lambda:capture_receipts(self.root,report,PRODUCER_SHA),'native')
        for stage in ('capture-error','late-notification','probe-terminal'):
            rows=original+[dict(original[-1],stage=stage,at_ns=1300)]
            path.write_text(''.join(json.dumps(row)+'\n' for row in rows))
            self.reject(lambda:capture_receipts(self.root,report,PRODUCER_SHA),'exact startup')
        path.write_text(''.join(json.dumps(row)+'\n' for row in original));capture_receipts(self.root,report,PRODUCER_SHA)

    def test_launch_and_cleanup_resealed_attacks(self):
        data,report=self.receipts()
        for name,key,value in [('owned-producer.json','pid',999),('producer-cleanup.json','joined',False),
                               ('producer-cleanup.json','signal',6),('producer-cleanup.json','control_state','STARTING'),
                               ('owned-cleanup.json','producer_reader_joined',False),('owned-cleanup.json','app_exit_code',-15)]:
            with self.subTest(name=name,key=key):
                row=copy.deepcopy(data[name]);row[key]=value;write(self.root/name,row)
                self.reject(lambda:capture_receipts(self.root,report,PRODUCER_SHA),'launch|cleanup')
                write(self.root/name,data[name])
        capture_receipts(self.root,report,PRODUCER_SHA)

    def test_background_child_actual_success_and_signal_output_retention(self):
        command=[sys.executable,'-c','import sys; print("owned-test"); print("stderr-test",file=sys.stderr)']
        result=helper_run(command,self.root,'normal-child',5)
        row=json.loads((self.root/'normal-child.process.json').read_text())
        launch=json.loads((self.root/'normal-child.launch.json').read_text())
        validate_process(row,launch,command,0,row['pid'])
        self.assertEqual(result.stdout,'owned-test\n');self.assertEqual(result.stderr,'stderr-test\n')
        command=[sys.executable,'-c','import os,signal,sys; print("before-signal",flush=True); print("raw-error",file=sys.stderr,flush=True); os.kill(os.getpid(),signal.SIGTERM)']
        result=helper_run(command,self.root,'signal-child',5)
        row=json.loads((self.root/'signal-child.process.json').read_text())
        self.assertEqual(result.returncode,-15);self.assertEqual(row['signal'],15)
        self.assertTrue(row['joined']);self.assertEqual(row['result'],'FAIL')
        self.assertEqual((self.root/'signal-child.stdout').read_text(),'before-signal\n')
        self.assertEqual((self.root/'signal-child.stderr').read_text(),'raw-error\n')

    def test_background_child_actual_timeout_retains_output_and_join(self):
        command=[sys.executable,'-c','import signal,sys; print("before-timeout",flush=True); print("timeout-error",file=sys.stderr,flush=True); signal.pause()']
        self.reject(lambda:helper_run(command,self.root,'timeout-child',.5),'timeout/launch/communication')
        row=json.loads((self.root/'timeout-child.process.json').read_text())
        self.assertTrue(row['timed_out']);self.assertTrue(row['joined']);self.assertEqual(row['result'],'FAIL')
        self.assertEqual(row['exit_code'],-15);self.assertEqual(row['signal'],15)
        self.assertEqual((self.root/'timeout-child.stdout').read_text(),'before-timeout\n')
        self.assertEqual((self.root/'timeout-child.stderr').read_text(),'timeout-error\n')

    def test_background_process_receipt_cannot_hide_timeout_or_signal(self):
        command=[sys.executable,'-c','print("unit")'];helper_run(command,self.root,'receipt-child',5)
        original=json.loads((self.root/'receipt-child.process.json').read_text())
        launch=json.loads((self.root/'receipt-child.launch.json').read_text())
        for key,value in [('timed_out',True),('signal',6),('joined',False),('result','FAIL'),('pid',999)]:
            with self.subTest(key=key):
                row=dict(original);row[key]=value
                self.reject(lambda:validate_process(row,launch,command,0,original['pid']),'actual')

if __name__=='__main__':unittest.main()
