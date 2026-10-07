"""Synthetic fault/lifecycle tests. Never real display, APP or native evidence."""
from copy import deepcopy
import errno
import json
import os
from pathlib import Path
import signal
import socket
import subprocess
import sys
import tempfile
import threading
import unittest
from unittest.mock import Mock, patch
import diagnose_display_prep as diag
import supervise as guard
from guard_policy import CLOCK_DOMAIN, TYPES, GuardPolicy
from test_guard import sample, BASE_NS

NONCE = '00000000-0000-4000-8000-000000000001'
SHA = 'a' * 64

def fill_datagram_channel(peer):
    """Reach actual send pressure, not proof that ENOBUFS means queue full."""
    peer.setsockopt(socket.SOL_SOCKET, socket.SO_SNDBUF, 4096)
    for _ in range(10000):
        try: peer.send(b'x' * 256)
        except OSError as error:
            if error.errno not in (errno.EAGAIN, errno.EWOULDBLOCK, errno.ENOBUFS): raise
            return error.errno
    raise AssertionError('failed to reach actual nonblocking channel capacity')

class Sink:
    error = None
    def __init__(self): self.rows = []
    def submit(self, row): self.rows.append(row)
class Monitor:
    pid = 17
    returncode = None
    def poll(self): return self.returncode

def ready(): return {'event':'ready', 'monitor_pid':17, 'thread_main':True}
def lines(rows):
    return b''.join((json.dumps(dict(row, protocol_version=3, nonce=NONCE, clock_domain=CLOCK_DOMAIN))+'\n').encode() for row in rows)
def seal(path, rows):
    raw = b''
    for sequence, row in enumerate(rows, 1):
        raw += (json.dumps(dict(row, schema_version=1, nonce=NONCE, clock_domain=CLOCK_DOMAIN, sequence=sequence))+'\n').encode()
    end = dict(schema_version=1, nonce=NONCE, clock_domain=CLOCK_DOMAIN, sequence=len(rows)+1,
               seal={'records_before_seal':len(rows), 'bytes_before_seal':len(raw)})
    if rows and 'role' in rows[0]:end.update(role=rows[0]['role'],pid=rows[0]['pid'])
    path.write_bytes(raw+(json.dumps(end)+'\n').encode())

class TemporaryTest(unittest.TestCase):
    def setUp(self):
        self.tmp = tempfile.TemporaryDirectory()
        self.base = Path(self.tmp.name).resolve(strict=True)
    def tearDown(self): self.tmp.cleanup()

class StreamTests(TemporaryTest):
    def make(self, rows):
        self.now = BASE_NS+200_000
        self.sink = Sink()
        self.path = self.base/'raw'
        self.path.write_bytes(lines(rows))
        self.tail = diag.DiagnosticTail(self.path, self.sink, 'owned', 17, NONCE, lambda:self.now)
        self.addCleanup(self.tail.close)
        return diag.DiagnosticStream(self.tail, Monitor(), NONCE, trace=self.sink, clock=lambda:self.now,
                                     policy=GuardPolicy(require_app_launch=True))
    def append(self, rows):
        with self.path.open('ab') as f: f.write(lines(rows))
    def test_clock_and_bytes_offsets(self):
        stream = self.make([ready(),sample()])
        self.assertIsNone(stream.pump())
        records = [r for r in self.sink.rows if r['event']=='raw_validation']
        self.assertIsNone(records[0]['validation_clock_ns'])
        self.assertEqual(records[1]['validation_clock_ns'],self.now)
        self.assertEqual(records[-1]['raw_offset_after'],self.path.stat().st_size)
        self.assertEqual(records[1]['raw_offset_before'],records[0]['raw_offset_after'])
    def test_gap_has_null_clock_and_latched_failure(self):
        stream = self.make([ready(),sample()]); stream.pump()
        self.append([sample(2,begin=BASE_NS+300_000_000)])
        self.now=BASE_NS+300_200_000
        self.assertEqual(stream.pump().reason,'INVALID_MONITOR_STREAM')
        first=deepcopy(stream.first_stream_error)
        self.assertIsNone(first['observed_at_ns'])
        record=[r for r in self.sink.rows if r.get('sample_seq')==2][-1]
        self.assertIsNone(record['validation_clock_ns'])
        self.append([sample(3,begin=BASE_NS+350_000_000)]); self.now+=50_000_000
        stream.pump()
        self.assertFalse(stream.integrity); self.assertEqual(stream.first_stream_error,first)
    def test_whole_batch_parse_before_any_validation(self):
        stream=self.make([ready(),sample()])
        with self.path.open('ab') as f:f.write(b'{broken}\n')
        stream.pump(); self.assertFalse(stream.ready); self.assertIsNone(stream.last_valid_sample)
    def test_partial_raw_is_not_fabricated(self):
        stream=self.make([ready()])
        with self.path.open('ab') as f:f.write(b'{partial')
        stream.pump();self.assertEqual(self.tail.partial,b'{partial')
    def test_original_and_diagnostic_agree(self):
        stream=self.make([ready(),sample(),sample(2)])
        self.now=BASE_NS+50_200_000
        original_tail=guard.FileTail(self.path);self.addCleanup(original_tail.close)
        original=guard.ObservationStream(original_tail,Monitor(),NONCE,clock=lambda:self.now,
                                         policy=GuardPolicy(require_app_launch=True))
        self.assertEqual(stream.pump(),original.pump())
        self.assertEqual(stream.last_valid_sample,original.last_valid_sample)
        self.assertEqual(stream.policy.phase,'ARMED')
    def test_no_app_stays_preparing(self):
        stream=self.make([ready(),sample(),sample(2)]);self.now=BASE_NS+50_200_000;stream.pump()
        stream.expected_runner_pid=42
        self.append([{'event':'runner_bound','credential':{'pid':42,'parent_pid':2,'start_seconds':1,'start_micros':0}},sample(3,runner_pid=42)])
        self.now+=50_000_000;stream.pump()
        self.assertEqual(stream.policy.phase,'RUNNER_PREPARING_APP')
        self.assertEqual(stream.policy.owned_pid,0);self.assertIsNone(stream.policy.app_launch_ns)
        self.assertIsNone(stream.policy.capture_completed_ns)
    def test_continuous_first_input_priority(self):
        stream=self.make([ready(),sample()])
        policy=GuardPolicy();halt=policy.stop('HID_COUNTER_CHANGE')
        stream.continuous_guard=lambda:halt;stream.pump()
        self.assertEqual(stream.policy.terminal.details['reason'],'HID_COUNTER_CHANGE')

    def test_duplicate_seq_error_belongs_only_to_actual_failing_raw_row(self):
        stream=self.make([ready(),sample()]);stream.pump()
        self.append([sample(2),sample(2)]);self.now=BASE_NS+50_200_000
        stream.pump()
        rows=[r for r in self.sink.rows if r.get('sample_seq')==2]
        self.assertEqual(len(rows),2);self.assertIsNone(rows[0]['error'])
        self.assertIsNotNone(rows[1]['error']);self.assertIsNone(rows[1]['validation_clock_ns'])
    def test_duplicate_ready_error_belongs_only_to_second_row(self):
        stream=self.make([ready(),ready()]);stream.pump()
        rows=[r for r in self.sink.rows if r.get('raw_event')=='ready']
        self.assertIsNone(rows[0]['error']);self.assertIsNotNone(rows[1]['error'])

class TraceTests(TemporaryTest):
    def test_concurrent_real_parent_fifo_preserves_every_producer_order(self):
        writer=diag.TraceWriter(self.base/'trace',NONCE)
        start=threading.Barrier(5);errors=[]
        def produce(producer):
            try:
                start.wait(timeout=1)
                for ordinal in range(30):writer.submit(dict(event='item',producer=producer,ordinal=ordinal))
            except BaseException as error:errors.append(error)
        threads=[threading.Thread(target=produce,args=(p,)) for p in range(4)]
        for thread in threads:thread.start()
        start.wait(timeout=1)
        for thread in threads:thread.join(timeout=2);self.assertFalse(thread.is_alive())
        self.assertEqual(errors,[]);writer.finish()
        rows=list(diag.sealed_rows(self.base/'trace',NONCE,diag.PARENT_LIMIT))
        self.assertEqual(len(rows),122)
        for producer in range(4):
            self.assertEqual([r['ordinal'] for r in rows if r.get('producer')==producer],list(range(30)))
        with self.assertRaisesRegex(RuntimeError,'failed/closed'):writer.submit(dict(event='late'))
    def test_fifo_capacity_rejects_without_overwrite(self):
        writer=diag.TraceWriter(self.base/'trace',NONCE,capacity=2,start=False)
        writer.submit({'event':'first'});before=list(writer.pending)
        with self.assertRaisesRegex(RuntimeError,'queue capacity'):writer.submit({'event':'overflow'})
        self.assertEqual(list(writer.pending),before);writer.handle.close()
    def test_line_capacity(self):
        writer=diag.TraceWriter(self.base/'trace',NONCE,start=False)
        with self.assertRaisesRegex(RuntimeError,'line limit'):writer.submit({'value':'a'*diag.LINE_LIMIT})
        writer.handle.close()
    def test_sidecar_capacity_never_seals(self):
        writer=diag.TraceWriter(self.base/'trace',NONCE,limit=2);writer.done.wait(.5)
        self.assertIn('capacity',writer.error)
        with self.assertRaises(RuntimeError):list(diag.sealed_rows(self.base/'trace',NONCE,diag.PARENT_LIMIT))
    def test_actual_terminal_seal(self):
        writer=diag.TraceWriter(self.base/'trace',NONCE);writer.submit({'event':'sample'});writer.finish()
        rows=list(diag.sealed_rows(self.base/'trace',NONCE,diag.PARENT_LIMIT))
        self.assertEqual([r['event'] for r in rows],['header','sample','terminal'])
    def test_partial_and_bad_seal_nonce_seq_eof_fail(self):
        path=self.base/'trace'
        for mutation in ('partial','bytes','nonce','seq','eof','terminal'):
            with self.subTest(mutation=mutation):
                seal(path,[{'event':'terminal'}]);raw=path.read_bytes()
                if mutation=='partial':raw=raw[:-1]
                elif mutation=='bytes':raw=raw.replace(b'"bytes_before_seal": ',b'"bytes_before_seal": 9')
                elif mutation=='nonce':raw=raw.replace(NONCE.encode(),b'x'*len(NONCE),1)
                elif mutation=='seq':raw=raw.replace(b'"sequence": 1',b'"sequence": 2',1)
                elif mutation=='eof':raw+=b'{}\n'
                else:seal(path,[{'event':'other'}]);raw=path.read_bytes()
                path.write_bytes(raw)
                with self.assertRaises(RuntimeError):list(diag.sealed_rows(path,NONCE,diag.PARENT_LIMIT))

class FailureChannelTests(TemporaryTest):
    def test_harness_extracts_candidate_types_and_fifo_verbatim(self):
        source=Path(diag.__file__).with_name('interference.swift').read_text()
        start=source.index('// Diagnostic-only values cross the FIFO.')
        end=source.index('\n@MainActor\nprivate final class InterferenceObserver')
        harness=swift_fifo_harness(source)
        self.assertTrue(harness.startswith('import Darwin\nimport Foundation\nimport os\n'+source[start:end]))
        self.assertTrue(harness.endswith(SWIFT_FIFO_HARNESS))
        emit=source[source.index('    private func emit(_ value: [String: Any])'):source.index('    private func fatal(_ reason: String)')]
        self.assertEqual(harness.count(emit),1)
        self.assertTrue(harness.splitlines()[3].startswith('// Diagnostic-only'))
        self.assertEqual(harness.count('private final class DiagnosticFIFO:'),1)
        self.assertNotIn('@unchecked',harness)
    def setUp(self):
        super().setUp()
        self.receiver,self.peer=socket.socketpair(socket.AF_UNIX,socket.SOCK_DGRAM)
        self.receiver.setblocking(False);self.peer.setblocking(False)
        self.addCleanup(self.peer.close)
        self.channel=diag.FailureChannel(self.receiver,NONCE,'owned',17,SHA,SHA)
        self.addCleanup(self.channel.close)
        self.fact=dict(schema_version=1,event='diagnostic_failure',clock_domain=CLOCK_DOMAIN,
            nonce=NONCE,role='owned',pid=17,source_sha256=SHA,executable_sha256=SHA,reason='QUEUE_COUNT',
            payload_kind='cycle',observed_ns=BASE_NS+2,submit_enter_ns=BASE_NS,lock_acquired_ns=BASE_NS+1,
            queue_count=256,retained_bytes=12345)
    def send(self,value):self.peer.send((json.dumps(value)+'\n').encode())
    def test_actual_datagram_and_unknown_not_clean(self):
        self.channel.poll();self.assertEqual(self.channel.summary()['status'],'UNKNOWN')
        self.send(self.fact);self.channel.poll()
        self.assertEqual(self.channel.summary()['receipt'],self.fact)
        self.assertEqual(self.channel.summary()['status'],'OBSERVED')
    def test_bad_identity_time_schema_and_unknown_reason_rejected(self):
        for key,value in (('nonce','wrong'),('pid',18),('role','continuous'),('source_sha256','b'*64),
                          ('executable_sha256','b'*64),('reason','LOCK_CONTENTION_PROVEN'),('schema_version',True),
                          ('lock_acquired_ns',BASE_NS-1),('queue_count',257),('observed_ns',None)):
            with self.subTest(key=key):
                self.channel.error=None;self.channel.raw=None;self.channel.receipt=None
                self.send(dict(self.fact,**{key:value}));self.channel.poll()
                self.assertEqual(self.channel.summary()['status'],'INVALID')
    def test_raw_failure_preserves_known_phase_bytes_and_unknown_write_return(self):
        common=dict(raw_record_ordinal=1,event='ready',encode_enter_ns=BASE_NS)
        for reason,emit in (
            ('RAW_ENCODE',dict(common,state='THREW',error='raw encode/write failed')),
            ('RAW_WRITE',dict(common,state='THREW',error='raw encode/write failed',encode_return_ns=BASE_NS+1,
                write_enter_ns=BASE_NS+2,expected_bytes=100,offset_before=0)),
            ('RAW_CAPACITY',dict(common,state='IN_PROGRESS',encode_return_ns=BASE_NS+1,
                expected_bytes=100,offset_before=diag.RAW_LIMIT))):
            with self.subTest(reason=reason):
                fact=dict(self.fact,reason=reason,payload_kind='emit_return',observed_ns=BASE_NS+3,raw_emit=emit)
                self.channel.raw=None;self.channel.receipt=None;self.send(fact);self.channel.poll()
                self.assertIsNone(self.channel.error);self.assertEqual(self.channel.receipt['raw_emit'],emit)
                for key,value in (('write_return_ns',BASE_NS+3),('offset_after',100),('raw_record_ordinal',True)):
                    self.channel.raw=None;self.channel.receipt=None;self.channel.error=None
                    self.send(dict(fact,raw_emit=dict(emit,**{key:value})));self.channel.poll()
                    self.assertEqual(self.channel.summary()['status'],'INVALID')
                self.channel.error=None
    def test_duplicate_even_later_and_truncated_partial_control_rejected(self):
        self.send(self.fact);self.channel.poll();self.send(self.fact);self.channel.poll()
        self.assertIn('duplicate',self.channel.error)
        for raw in (b'{partial',b'{}\ntrailing\n'):
            self.channel.error=None;self.channel.raw=None;self.channel.receipt=None
            self.peer.send(raw);self.channel.poll();self.assertIsNotNone(self.channel.error)
        self.channel.error=None;self.channel.raw=None;self.channel.receipt=None
        import array
        self.peer.sendmsg([(json.dumps(self.fact)+'\n').encode()],[(socket.SOL_SOCKET,socket.SCM_RIGHTS,array.array('i',[self.peer.fileno()]))])
        self.channel.poll();self.assertIn('control',self.channel.error)
    def test_actual_oversize_send_rejection_is_not_received_truncation(self):
        raw=b'a'*(diag.FAILURE_LIMIT+1)
        try:self.peer.send(raw)
        except OSError as error:
            self.assertEqual(error.errno,errno.EMSGSIZE)
            self.channel.poll();self.assertEqual(self.channel.summary()['status'],'UNKNOWN')
            self.assertIsNone(self.channel.raw)
        else:
            self.channel.poll();self.assertEqual(self.channel.summary()['status'],'INVALID')
            self.assertIn('truncated',self.channel.error)
    def test_receive_truncation_flags_reject_even_well_formed_receipt(self):
        raw=(json.dumps(self.fact)+'\n').encode()
        for flag in (socket.MSG_TRUNC,socket.MSG_CTRUNC):
            with self.subTest(flag=flag):
                receiver=Mock();receiver.recvmsg.return_value=(raw,[],flag,None)
                channel=diag.FailureChannel(receiver,NONCE,'owned',17,SHA,SHA)
                channel.poll();self.assertEqual(channel.summary()['status'],'INVALID')
                self.assertIn('truncated',channel.error)
    def test_actual_small_receive_buffer_reports_truncation(self):
        self.send(self.fact)
        receiver=Mock()
        receiver.recvmsg.side_effect=lambda size,ancillary,flags:self.receiver.recvmsg(128,ancillary,flags)
        channel=diag.FailureChannel(receiver,NONCE,'owned',17,SHA,SHA)
        channel.poll();self.assertEqual(channel.summary()['status'],'INVALID')
        self.assertIn('truncated',channel.error);self.assertIsNone(channel.receipt)
    def test_receive_errno_preserved_and_unexpected_reset_never_recovers(self):
        for code in (errno.ECONNRESET,errno.ENOBUFS,errno.EBADF):
            with self.subTest(errno=code):
                receiver=Mock();receiver.recvmsg.side_effect=OSError(code,os.strerror(code))
                channel=diag.FailureChannel(receiver,NONCE,'owned',17,SHA,SHA)
                channel.poll();self.assertEqual(channel.summary()['status'],'INVALID')
                self.assertEqual(channel.summary()['transport']['errno_code'],code)
                self.assertEqual(channel.summary()['transport']['operation'],'recvmsg')
                receiver.recvmsg.side_effect=None
                receiver.recvmsg.return_value=((json.dumps(self.fact)+'\n').encode(),[],0,None)
                channel.poll();self.assertIsNone(channel.receipt)
                self.assertEqual(receiver.recvmsg.call_count,1)
    def test_unexpected_reset_preserves_already_observed_failure_receipt(self):
        receiver=Mock();receiver.recvmsg.side_effect=[((json.dumps(self.fact)+'\n').encode(),[],0,None),
            OSError(errno.ECONNRESET,'unexpected reset')]
        channel=diag.FailureChannel(receiver,NONCE,'owned',17,SHA,SHA)
        channel.poll();self.assertEqual(channel.summary()['status'],'INVALID')
        self.assertEqual(channel.receipt,self.fact)
        self.assertEqual(channel.summary()['transport']['classification'],'RECEIVE_FAILURE')
    def test_wouldblock_is_empty_not_a_transport_error(self):
        receiver=Mock();receiver.recvmsg.side_effect=OSError(errno.EAGAIN,'empty')
        channel=diag.FailureChannel(receiver,NONCE,'owned',17,SHA,SHA)
        channel.poll();self.assertEqual(channel.summary()['status'],'UNKNOWN')
        self.assertIsNone(channel.transport);self.assertIsNone(channel.error)
    def test_expected_close_requires_exact_actual_join_and_receipt_absence_stays_unknown(self):
        with subprocess.Popen([sys.executable,'-c','pass'],stdout=subprocess.DEVNULL,stderr=subprocess.DEVNULL) as child:
            child.wait(timeout=2)
            fact=dict(self.fact,pid=child.pid)
            for received in (False,True):
                with self.subTest(received=received):
                    receiver=Mock()
                    responses=[OSError(errno.ECONNRESET,os.strerror(errno.ECONNRESET))]
                    if received:responses.insert(0,((json.dumps(fact)+'\n').encode(),[],0,None))
                    receiver.recvmsg.side_effect=responses
                    channel=diag.FailureChannel(receiver,NONCE,'owned',child.pid,SHA,SHA)
                    channel.poll(joined_peer=child)
                    self.assertEqual(channel.summary()['status'],'OBSERVED' if received else 'UNKNOWN')
                    self.assertEqual(channel.summary()['transport']['classification'],'EXPECTED_PEER_CLOSE')
                    self.assertEqual(channel.summary()['transport']['joined_exit_code'],0)
                    self.assertEqual(channel.receipt,fact if received else None)
            receiver=Mock();receiver.recvmsg.side_effect=OSError(errno.ECONNRESET,'reset')
            channel=diag.FailureChannel(receiver,NONCE,'owned',child.pid+1,SHA,SHA)
            channel.poll(joined_peer=child);self.assertEqual(channel.summary()['status'],'INVALID')
    def test_nonblocking_full_or_closed_channel_cannot_manufacture_receipt(self):
        full_errno=fill_datagram_channel(self.peer)
        self.assertIn(full_errno,(errno.EAGAIN,errno.EWOULDBLOCK,errno.ENOBUFS))
        with self.assertRaises(OSError) as caught:self.send(self.fact)
        self.assertIn(caught.exception.errno,(errno.EAGAIN,errno.EWOULDBLOCK,errno.ENOBUFS))
        self.channel.poll();self.assertEqual(self.channel.summary()['status'],'INVALID')
        self.assertIsNone(self.channel.receipt)
        self.receiver.close()
        with self.assertRaises(OSError):self.send(self.fact)
    def test_receipt_latches_failure_preserves_firstcause_and_starts_no_child(self):
        run=diag.DiagnosticRun(self.base,self.base/'APP',self.base/'CAP',self.base)
        run.observers=[dict(role='owned',failure_channel=self.channel,monitor=Monitor())]
        self.send(self.fact)
        with patch.object(diag.subprocess,'Popen',side_effect=AssertionError('no APP or child permitted')):
            run.poll_failure_channels()
        self.assertIn('QUEUE_COUNT',run.failure);self.assertIsNone(run.child)
        run.failure='original gap';run.poll_failure_channels();self.assertEqual(run.failure,'original gap')
    def test_known_failure_reason_precedes_supplemental_unexpected_reset(self):
        receiver=Mock();receiver.recvmsg.side_effect=[((json.dumps(self.fact)+'\n').encode(),[],0,None),
            OSError(errno.ECONNRESET,'unexpected reset')]
        channel=diag.FailureChannel(receiver,NONCE,'owned',17,SHA,SHA)
        run=diag.DiagnosticRun(self.base,self.base/'APP',self.base/'CAP',self.base)
        run.observers=[dict(role='owned',failure_channel=channel,monitor=Monitor())]
        run.poll_failure_channels()
        self.assertIn('QUEUE_COUNT',run.failure);self.assertEqual(channel.summary()['status'],'INVALID')
        self.assertEqual(channel.summary()['transport']['errno_code'],errno.ECONNRESET)
    def test_run_closure_requires_own_stop_intent_as_well_as_joined_sigterm(self):
        with subprocess.Popen([sys.executable,'-c','import time; time.sleep(20)'],
            stdout=subprocess.DEVNULL,stderr=subprocess.DEVNULL) as child:
            child.terminate();self.assertEqual(child.wait(timeout=2),-int(signal.SIGTERM))
            for requested in (False,True):
                with self.subTest(requested=requested):
                    receiver=Mock();receiver.recvmsg.side_effect=OSError(errno.ECONNRESET,'reset')
                    channel=diag.FailureChannel(receiver,NONCE,'owned',child.pid,SHA,SHA)
                    run=diag.DiagnosticRun(self.base,self.base/'APP',self.base/'CAP',self.base)
                    run.stopping=True
                    run.observers=[dict(role='owned',failure_channel=channel,monitor=child,stop_requested=requested)]
                    run.poll_failure_channels()
                    self.assertEqual(channel.summary()['status'],'UNKNOWN' if requested else 'INVALID')
                    self.assertEqual(run.failure is None,requested)
                    self.assertIsNone(channel.receipt);self.assertIsNone(run.child)
    def test_spawn_failure_closes_only_channel_and_handles(self):
        run=diag.DiagnosticRun(self.base,self.base/'APP',self.base/'CAP',self.base)
        descriptors=[]
        def rejected(*args,**kwargs):
            self.assertEqual(len(kwargs['pass_fds']),1)
            descriptors.extend([*kwargs['pass_fds'],kwargs['stdout'].fileno(),kwargs['stderr'].fileno()])
            self.assertEqual(str(kwargs['pass_fds'][0]),kwargs['env']['RCAM_DIAG_FAILURE_FD'])
            raise OSError('synthetic spawn failure')
        with patch.object(diag.subprocess,'Popen',side_effect=rejected):
            with self.assertRaisesRegex(OSError,'spawn failure'):run.start_observer('owned',self.base/'observer',SHA,SHA)
        self.assertEqual(run.observers,[])
        for descriptor in descriptors:
            with self.assertRaises(OSError):os.fstat(descriptor)
    def test_socketpair_failure_closes_open_logs_starts_no_process(self):
        run=diag.DiagnosticRun(self.base,self.base/'APP',self.base/'CAP',self.base)
        opened=[];original=Path.open
        def tracked(path,*args,**kwargs):
            handle=original(path,*args,**kwargs)
            if path.name.endswith(('.stdout','.stderr')):opened.append(handle)
            return handle
        with patch.object(diag.socket,'socketpair',side_effect=OSError('socket allocation failure')), \
             patch.object(diag.subprocess,'Popen',side_effect=AssertionError('must not start observer')), \
             patch.object(Path,'open',tracked):
            with self.assertRaisesRegex(OSError,'allocation'):run.start_observer('owned',self.base/'observer',SHA,SHA)
        self.assertEqual(run.observers,[]);self.assertEqual(len(opened),2)
        self.assertTrue(all(handle.closed for handle in opened))
    def test_adapter_failure_retains_owned_monitor_and_receive_endpoint(self):
        run=diag.DiagnosticRun(self.base,self.base/'APP',self.base/'CAP',self.base)
        descriptors=[]
        def spawned(*args,**kwargs):
            descriptors.extend(kwargs['pass_fds']);return Monitor()
        with patch.object(diag.subprocess,'Popen',side_effect=spawned), \
             patch.object(diag,'DiagnosticTail',side_effect=RuntimeError('adapter setup failure')):
            with self.assertRaisesRegex(RuntimeError,'adapter setup'):run.start_observer('owned',self.base/'observer',SHA,SHA)
        self.assertEqual(len(run.observers),1)
        item=run.observers[0];self.assertEqual(item['monitor'].pid,17);self.assertIsNone(item['tail'])
        os.fstat(item['failure_channel'].receiver.fileno())
        for descriptor in descriptors:
            with self.assertRaises(OSError):os.fstat(descriptor)
        for handle in item['handles']:handle.close()
        item['failure_channel'].close()


def swift_fifo_harness(source):
    """Extract the candidate FIFO verbatim; hooks exist only in this test file."""
    checked=source[source.index('// Diagnostic-only values cross the FIFO.'):source.index(
        '\n@MainActor\nprivate final class InterferenceObserver')]
    emit=source[source.index('    private func emit(_ value: [String: Any])'):source.index('    private func fatal(_ reason: String)')]
    raw_actor='''
@MainActor
private final class RawIOTest {
    private let nonce: String, diagnostic: (any DiagnosticSink)?
    private var rawOrdinal = 0, rawBytes: UInt64 = 0
    private var previousEmit: EmitFact?
    init(_ nonce: String, _ sink: any DiagnosticSink) { self.nonce = nonce; diagnostic = sink }
    private func monotonicNowNS() -> UInt64 { DispatchTime.now().uptimeNanoseconds }
    private func trace(_ value: TracePayload) { if let diagnostic, !diagnostic.submit(value) { exit(74) } }
'''+emit+'''
    func test(_ mode: String) {
        if mode == "raw-capacity" { rawBytes = 67108864 }
        if mode == "raw-write" { try? FileHandle.standardOutput.close() }
        let bad: Double? = mode == "raw-encode" ? Double.nan :
            mode == "raw-infinity" ? Double.infinity : mode == "raw-negative-infinity" ? -Double.infinity : nil
        emit(["event": "ready", "synthetic": bad.map { $0 as Any } ?? (true as Any)])
        exit(1)
    }
}
'''
    return 'import Darwin\nimport Foundation\nimport os\n'+checked+raw_actor+SWIFT_FIFO_HARNESS


SWIFT_FIFO_HARNESS = r'''
private func check(_ value: Bool, _ message: String) {
    if !value { FileHandle.standardError.write(Data((message + "\n").utf8)); exit(1) }
}
private func payload(_ ordinal: Int) -> TracePayload {
    .emit_start(EmitFact(raw_record_ordinal: ordinal, event: "test", sample_seq: nil))
}
@available(macOS 13.0, *)
private extension DiagnosticFIFO {
    func hold(_ entered: DispatchSemaphore, _ released: DispatchSemaphore) {
        state.withLock { _ in entered.signal(); released.wait() }
    }
    func testCapacity(memory: Bool) {
        let field = FieldBuild(source: "hid", event_type: "anyInput", build_enter_ns: 1, build_return_ns: 2)
        var cycle = CycleFact(cycle_id: 1, sample_seq: 1, timer_enter_ns: 1, previous_emit: nil)
        cycle.field_build = Array(repeating: field, count: 64)
        let value: TracePayload = memory ? .cycle(cycle) : payload(1)
        let entered = DispatchTime.now().uptimeNanoseconds
        let rejected = state.withLock { state in
            while true {
                let result = admit(value, entered: entered, state: &state)
                if result.failure != nil {
                    let before = state.queue.count
                    let retained = state.retainedFieldBytes
                    let again = admit(value, entered: entered, state: &state)
                    check(result.failure == again.failure && before == state.queue.count &&
                        retained == state.retainedFieldBytes, "rejection changed the actual queue")
                    return result
                }
            }
        }
        check(rejected.failure == (memory ? .queueMemory : .queueCount), "wrong actual capacity cause")
        check(!admitted(rejected, value: value, entered: entered), "capacity accepted")
    }
    func waitForWorker() { check(finished.wait(timeout: .now() + .seconds(2)) == .success, "worker did not return") }
    func makeOutputReadOnly(_ path: String) {
        let replacement = open(path, O_RDONLY)
        check(replacement >= 0 && dup2(replacement, fd) == fd, "readonly injection failed")
        close(replacement)
    }
    func blockOutput() -> Int32 {
        var ends: [Int32] = [0, 0]
        check(pipe(&ends) == 0, "test pipe failed")
        check(fcntl(ends[1], F_SETFL, O_NONBLOCK) == 0, "nonblocking test pipe failed")
        let data = Data(repeating: 0, count: 4096)
        var full = false
        for _ in 0..<4096 {
            let n = data.withUnsafeBytes { Darwin.write(ends[1], $0.baseAddress!, $0.count) }
            if n < 0 { check(errno == EAGAIN, "pipe fill failed"); full = true; break }
        }
        check(full && fcntl(ends[1], F_SETFL, 0) == 0 && dup2(ends[1], fd) == fd, "blocking output injection failed")
        close(ends[1]); return ends[0]
    }
    func waitForDequeue() {
        let end = DispatchTime.now() + .seconds(1)
        while !state.withLock({ $0.queue.isEmpty }) {
            check(DispatchTime.now() < end, "worker did not dequeue")
            Thread.sleep(forTimeInterval: 0.001)
        }
    }
}
@main
private struct FIFOTestMain {
    @MainActor
    static func main() {
        if #available(macOS 13.0, *) { run() } else { exit(64) }
    }
    @available(macOS 13.0, *)
    @MainActor
    static func run() {
        // Harness only: observe writer failure without killing the test process.
        signal(SIGTERM, SIG_IGN)
        signal(SIGPIPE, SIG_IGN) // harness-only EPIPE injection, never production
        let mode = CommandLine.arguments[1], path = CommandLine.arguments[2]
        let fifo = DiagnosticFIFO(path: path, nonce: CommandLine.arguments[3])
        if mode.hasPrefix("raw-") {
            RawIOTest(CommandLine.arguments[3], fifo).test(mode)
        } else if mode == "count" || mode == "memory" || mode == "channel-full" || mode == "channel-closed" {
            fifo.testCapacity(memory: mode == "memory")
        } else if mode == "wait" || mode == "terminal-busy" || mode == "shared-deadline" {
            let entered = DispatchSemaphore(value: 0), release = DispatchSemaphore(value: 0)
            let holderDone = DispatchSemaphore(value: 0)
            Thread { fifo.hold(entered, release); holderDone.signal() }.start()
            check(entered.wait(timeout: .now() + .seconds(1)) == .success, "holder absent")
            if mode == "wait" {
                let called = DispatchSemaphore(value: 0), done = DispatchSemaphore(value: 0)
                Thread { called.signal(); check(fifo.submit(payload(1)), "ordinary contention rejected"); done.signal() }.start()
                check(called.wait(timeout: .now() + .seconds(1)) == .success, "producer absent")
                check(done.wait(timeout: .now() + .milliseconds(60)) == .timedOut, "producer did not wait")
                release.signal()
                check(done.wait(timeout: .now() + .seconds(1)) == .success, "producer stuck after release")
                check(holderDone.wait(timeout: .now() + .seconds(1)) == .success, "holder stuck")
                // Exercise actual repeated admission/drain, with independent EOF validation in Python.
                for ordinal in 2...100 { check(fifo.submit(payload(ordinal)), "ordinary admission failed") }
                fifo.terminate(.terminal(nil), deadline: .now() + .milliseconds(200))
            } else {
                let started = DispatchTime.now(), deadline = started + .milliseconds(200)
                if mode == "shared-deadline" { Thread.sleep(forTimeInterval: 0.15) }
                fifo.terminate(.terminal(nil), deadline: deadline)
                let returned = DispatchTime.now().uptimeNanoseconds
                // Release only after terminate returned: a blocking terminal path deadlocks this test.
                release.signal()
                check(holderDone.wait(timeout: .now() + .seconds(1)) == .success, "holder stuck")
                print("{\"started_ns\":\(started.uptimeNanoseconds),\"deadline_ns\":\(deadline.uptimeNanoseconds),\"returned_ns\":\(returned)}")
                return
            }
        } else if mode == "closed" {
            fifo.terminate(.terminal(nil), deadline: .now() + .milliseconds(200))
            check(!fifo.submit(payload(1)), "record admitted after terminal")
        } else if mode == "write" || mode == "line" || mode == "worker-busy" {
            let end = DispatchTime.now() + .seconds(1)
            while true {
                let data = try? Data(contentsOf: URL(fileURLWithPath: path))
                if data?.last == 10 { break }
                check(DispatchTime.now() < end, "header was not written")
                Thread.sleep(forTimeInterval: 0.001)
            }
            if mode == "worker-busy" {
                let reader = fifo.blockOutput()
                check(fifo.submit(payload(1)), "blocked write admission failed")
                fifo.waitForDequeue()
                let entered = DispatchSemaphore(value: 0), release = DispatchSemaphore(value: 0)
                let done = DispatchSemaphore(value: 0)
                Thread { fifo.hold(entered, release); done.signal() }.start()
                check(entered.wait(timeout: .now() + .seconds(1)) == .success, "holder absent")
                close(reader) // actual worker EPIPE while the queue owner remains held
                fifo.waitForWorker()
                release.signal()
                check(done.wait(timeout: .now() + .seconds(1)) == .success, "holder stuck")
                // Missed failed latch can accept until SIGTERM; acceptance cannot certify completeness.
                check(fifo.submit(payload(2)), "test did not exercise missed failed latch")
                print("{}"); return
            } else if mode == "write" {
                fifo.makeOutputReadOnly(path)
                check(fifo.submit(payload(1)), "write injection admission failed")
            } else {
                let field = FieldBuild(source: "hid", event_type: "anyInput", build_enter_ns: 1, build_return_ns: 2)
                var cycle = CycleFact(cycle_id: 1, sample_seq: 1, timer_enter_ns: 1, previous_emit: nil)
                cycle.field_build = Array(repeating: field, count: 128)
                check(fifo.submit(.cycle(cycle)), "line injection admission failed")
            }
            fifo.waitForWorker()
            check(!fifo.submit(payload(2)), "failed writer accepted another record")
        } else { exit(64) }
        print("{}")
    }
}
'''


@unittest.skipUnless(sys.platform=='darwin', 'NOT_RUN: same-source Swift FIFO requires macOS Swift6')
class SwiftFIFOTests(TemporaryTest):
    def test_checked_swift_fifo_concurrency_capacity_terminal_and_failure(self):
        source=Path(diag.__file__).with_name('interference.swift')
        harness=self.base/'FIFO.swift';harness.write_text(swift_fifo_harness(source.read_text()))
        executable=self.base/'FIFO'
        compile_command=['/usr/bin/swiftc','-parse-as-library','-swift-version','6',
            '-strict-concurrency=complete','-warnings-as-errors',str(harness),'-o',str(executable)]
        compiled=subprocess.run(compile_command,capture_output=True,timeout=30)
        self.assertEqual(compiled.returncode,0,compiled.stderr.decode())
        source_sha,harness_sha,exe_sha=guard.sha(source),guard.sha(harness),guard.sha(executable)
        for mode,reason in (('wait',None),('terminal-busy','TERMINAL_LOCK_BUSY'),
            ('shared-deadline','TERMINAL_LOCK_BUSY'),('count','QUEUE_COUNT'),('memory','QUEUE_MEMORY'),
            ('closed','CLOSED'),('write','WRITE'),('worker-busy','WRITE'),('line','LINE_CAPACITY'),
            ('raw-encode','RAW_ENCODE'),('raw-infinity','RAW_ENCODE'),('raw-negative-infinity','RAW_ENCODE'),
            ('raw-write','RAW_WRITE'),('raw-capacity','RAW_CAPACITY'),
            ('open','SIDECAR_OPEN'),('channel-full',None),('channel-closed',None)):
            with self.subTest(mode=mode):
                receiver,peer=socket.socketpair(socket.AF_UNIX,socket.SOCK_DGRAM)
                receiver.setblocking(False);peer.setblocking(False)
                sidecar=self.base/(mode+'.jsonl')
                if mode=='open':sidecar.write_bytes(b'preexisting')
                channel=None
                try:
                    if mode=='channel-full':
                        self.assertIn(fill_datagram_channel(peer),(errno.EAGAIN,errno.EWOULDBLOCK,errno.ENOBUFS))
                    if mode=='channel-closed':receiver.close()
                    env=dict(os.environ,RCAM_DIAG_FAILURE_FD=str(peer.fileno()),RCAM_DIAG_ROLE='owned',
                             RCAM_DIAG_SOURCE_SHA=source_sha,RCAM_DIAG_EXECUTABLE_SHA=exe_sha)
                    with subprocess.Popen([str(executable),mode,str(sidecar),NONCE],env=env,
                        pass_fds=(peer.fileno(),),stdout=subprocess.PIPE,stderr=subprocess.PIPE) as child:
                        peer.close()
                        try:stdout,stderr=child.communicate(timeout=4)
                        except subprocess.TimeoutExpired:
                            child.kill();stdout,stderr=child.communicate();self.fail('FIFO harness exceeded bounded lifetime')
                        self.assertEqual(child.returncode,74 if mode.startswith('raw-') or mode=='open' else 0,stderr.decode())
                        if mode!='channel-closed':
                            channel=diag.FailureChannel(receiver,NONCE,'owned',child.pid,source_sha,exe_sha)
                            channel.poll(joined_peer=child)
                    if mode=='wait':
                        self.assertEqual(channel.summary()['status'],'UNKNOWN')
                        rows=list(diag.sealed_rows(sidecar,NONCE,diag.OBSERVER_LIMIT,role='owned',pid=child.pid))
                        emits=[r for r in rows if 'emit_start' in r.get('payload',{})]
                        self.assertEqual([r['payload']['emit_start']['_0']['raw_record_ordinal'] for r in emits],list(range(1,101)))
                        self.assertGreaterEqual(emits[0]['lock_acquired_ns']-emits[0]['submit_enter_ns'],20_000_000)
                        self.assertTrue(all(0<r['submit_enter_ns']<=r['lock_acquired_ns'] for r in rows))
                    elif reason:
                        self.assertEqual(channel.summary()['status'],'OBSERVED',channel.summary())
                        self.assertEqual(channel.receipt['reason'],reason)
                        if mode=='closed':
                            self.assertEqual(len(list(diag.sealed_rows(sidecar,NONCE,diag.OBSERVER_LIMIT))),2)
                        else:
                            with self.assertRaises(RuntimeError):list(diag.sealed_rows(sidecar,NONCE,diag.OBSERVER_LIMIT))
                        if mode in ('terminal-busy','shared-deadline'):
                            timings=json.loads(stdout)
                            self.assertEqual(timings['deadline_ns']-timings['started_ns'],200_000_000)
                            self.assertLessEqual(timings['returned_ns']-timings['started_ns'],250_000_000)
                            self.assertNotIn('lock_acquired_ns',channel.receipt)
                        if mode=='write':self.assertEqual(channel.receipt['errno_code'],9)
                        if mode=='worker-busy':self.assertEqual(channel.receipt['errno_code'],32)
                    elif mode=='channel-full':self.assertEqual(channel.summary()['status'],'INVALID')
                finally:
                    peer.close();receiver.close()
        print(json.dumps(dict(swift_fifo_harness='SOURCE_ONLY_CONCURRENCY_TEST',source_sha256=source_sha,
            harness_sha256=harness_sha,executable_sha256=exe_sha,stage_PASS_claim=False)))

class ObserverProofTests(TemporaryTest):
    def setUp(self):
        super().setUp();self.raw=self.base/'raw';self.side=self.base/'side';self.rows=[]
        originals=[ready(),sample(),sample(2)];raws=[lines([row]) for row in originals];self.raw.write_bytes(b''.join(raws))
        def add(kind,fact):self.rows.append({'role':'owned','pid':17,'submit_enter_ns':BASE_NS-10000,
            'lock_acquired_ns':BASE_NS-9999,'payload':{kind:{'_0':fact}}})
        add('header',dict(source_sha256=SHA,executable_sha256=SHA,queue_limit=256,memory_limit_bytes=1048576,
                         sidecar_limit_bytes=diag.OBSERVER_LIMIT,line_limit_bytes=8192,raw_limit_bytes=diag.RAW_LIMIT))
        offset=0;last=None;cycle=None
        for ordinal,(original,line) in enumerate(zip(originals,raws),1):
            if original['event']=='sample':
                begin,end=original['begin_ns'],original['end_ns']
                cycle=dict(cycle_id=original['seq'],sample_seq=original['seq'],timer_enter_ns=begin-3,
                    control_enter_ns=begin-2,control_return_ns=begin-1,sample_begin_ns=begin,sample_end_ns=end,
                    timer_exit_ns=end+6,previous_emit=last,field_build=[dict(source=state,event_type=name,
                    build_enter_ns=begin+index*100,build_return_ns=begin+index*100+1) for index,(state,name) in
                    enumerate((s,n) for s in ('hid','combined') for n in (*TYPES,'anyInput'))])
                add('cycle_enter',dict(cycle_id=original['seq'],sample_seq=original['seq'],timer_enter_ns=begin-3,previous_emit=last))
            end=original.get('end_ns',BASE_NS-100)
            fact=dict(raw_record_ordinal=ordinal,event=original['event'],sample_seq=original.get('seq'),state='IN_PROGRESS')
            add('emit_start',fact)
            fact=dict(fact,state='RETURNED',encode_enter_ns=end+1,encode_return_ns=end+2,write_enter_ns=end+3,
                      write_return_ns=end+4,expected_bytes=len(line),offset_before=offset,offset_after=offset+len(line))
            progress=dict(fact,state='IN_PROGRESS');progress.pop('write_return_ns');progress.pop('offset_after')
            add('emit_progress',progress);add('emit_return',fact);last=fact;offset+=len(line)
            if original['event']=='sample':add('cycle',cycle)
        add('terminal',cycle);seal(self.side,self.rows)
    def verify(self):return diag.verify_observer(self.side,self.raw,NONCE,'owned',17,SHA,SHA)
    def test_full_synthetic_trace(self):self.assertEqual(self.verify()['cycles'],2)
    def test_admission_wait_is_actual_and_missing_reversed_bool_fail(self):
        for mutation in ('missing', 'reversed', 'bool'):
            with self.subTest(mutation=mutation):
                rows=deepcopy(self.rows)
                if mutation=='missing':rows[0].pop('lock_acquired_ns')
                elif mutation=='reversed':rows[0]['lock_acquired_ns']=rows[0]['submit_enter_ns']-1
                else:rows[0]['lock_acquired_ns']=True
                seal(self.side,rows)
                with self.assertRaisesRegex(RuntimeError,'admission actual time'):self.verify()
    def test_raw_extra_partial_rejects_even_if_sealed(self):
        with self.raw.open('ab') as f:f.write(b'{partial')
        with self.assertRaisesRegex(RuntimeError,'raw EOF'):self.verify()
    def test_no_inferred_write_return(self):
        self.rows[-3]['payload']['emit_return']['_0'].pop('write_return_ns');seal(self.side,self.rows)
        with self.assertRaisesRegex(RuntimeError,'time order'):self.verify()
    def test_bytes_offset_ordinal_carry_and_sample_bindings(self):
        for key in ('expected_bytes','offset_after','raw_record_ordinal'):
            with self.subTest(key=key):
                rows=deepcopy(self.rows);next(r for r in rows if 'emit_return' in r['payload'])['payload']['emit_return']['_0'][key]+=1;seal(self.side,rows)
                with self.assertRaises(RuntimeError):self.verify()
        for key in ('previous_emit','sample_begin_ns'):
            rows=deepcopy(self.rows)
            if key=='previous_emit':next(r for r in rows if 'cycle_enter' in r['payload'])['payload']['cycle_enter']['_0'][key]=None
            else:next(r for r in rows if 'cycle' in r['payload'])['payload']['cycle']['_0'][key]+=1
            seal(self.side,rows)
            with self.assertRaises(RuntimeError):self.verify()
    def test_missing_emit_or_unfinished_cycle(self):
        for rows in (self.rows[:1]+self.rows[3:],self.rows[:-2]+self.rows[-1:]):
            seal(self.side,rows)
            with self.assertRaises(RuntimeError):self.verify()

    def test_progress_missing_duplicate_or_contradictory_fails(self):
        progress=next(r for r in self.rows if 'emit_progress' in r['payload'])
        index=self.rows.index(progress)
        for mutation in ('missing','duplicate','contradictory'):
            with self.subTest(mutation=mutation):
                rows=deepcopy(self.rows)
                if mutation=='missing':rows.pop(index)
                elif mutation=='duplicate':rows.insert(index,deepcopy(rows[index]))
                else:rows[index]['payload']['emit_progress']['_0']['write_enter_ns']+=1
                seal(self.side,rows)
                with self.assertRaises(RuntimeError):self.verify()

class ChildFixture(TemporaryTest):
    def setUp(self):
        super().setUp();(self.base/'display.swift').write_text('synthetic');(self.base/'display-probe.swift').write_text('synthetic')
        self.parent,self.child=socket.socketpair(socket.AF_UNIX,socket.SOCK_DGRAM)
        self.parent.setblocking(False);self.child.setblocking(False)
        self.now=BASE_NS;self.deadline=BASE_NS+40_000_000_000;self.labels=[];self.fail_label=None;self.omit_pair=False;self.cancel_label=None
        self.before=dict(display_id=2,mode_id=113,refresh_hz=144,width=1920,height=1080,pixel_width=3840,pixel_height=2160,backing_scale=2,in_mirror_set=False)
        owner=self
        class Validators:
            def owned_command(self,directory,label,command,**kwargs):
                owner.labels.append(label);before=deepcopy(owner.before);after=deepcopy(before)
                if label in ('display-active','display-active-probe'):after.update(mode_id=114,refresh_hz=60)
                if label=='display-active-probe':before=deepcopy(after)
                if command[5]!='probe':before.pop('backing_scale');after.pop('backing_scale')
                receipt={'before':before,'after':after}
                launch=dict(pid=100+len(owner.labels),pgid=100+len(owner.labels),private_session=True,command=command,started_monotonic_ns=owner.now,timeout_seconds=10)
                process=dict(launch,schema_version=2,joined=True,owned_group_released=True,exit_code=0,finished_monotonic_ns=owner.now+1)
                if not(owner.omit_pair and owner.fail_label==label):
                    diag.immutable(directory/(label+'.subcommand-launch.json'),launch);diag.immutable(directory/(label+'.subcommand-process.json'),process)
                (directory/(label+'.subcommand.stdout')).write_text(json.dumps(receipt));(directory/(label+'.subcommand.stderr')).write_text('synthetic')
                owner.now+=1_000_000
                if owner.cancel_label==label:
                    diag.send(owner.parent,dict(event='cleanup',nonce=NONCE,deadline_ns=owner.deadline,clock_domain=CLOCK_DOMAIN))
                    signal.getsignal(signal.SIGINT)(signal.SIGINT,None)
                if owner.fail_label==label:
                    diag.send(owner.parent,dict(event='cleanup',nonce=NONCE,deadline_ns=owner.deadline,clock_domain=CLOCK_DOMAIN))
                    raise RuntimeError('synthetic failed helper')
                return subprocess.CompletedProcess(command,0,json.dumps(receipt),'synthetic')
            def verify_command(self,directory,label,*args):return guard.raw_json(directory/(label+'.subcommand-process.json'))[1]
            def validate_display_receipt(self,*args,**kwargs):pass
            def validate_display_phases(self,*args,**kwargs):return [{'at_ns':owner.now}]
        self.validators=Validators()
        diag.send(self.parent,dict(event='go',nonce=NONCE,pid=os.getpid(),script_sha256=guard.sha(Path(diag.__file__)),clock_domain=CLOCK_DOMAIN))
    def tearDown(self):self.parent.close();self.child.close();super().tearDown()
    def run_child(self):
        with patch.object(guard,'product_display_validators',return_value=self.validators), \
             patch.object(diag,'validate_five',return_value=self.before), \
             patch.object(diag.subprocess,'Popen',side_effect=AssertionError('unexpected APP/capture/extra spawn')):
            code=diag.child_cycle(self.base/'product',self.base,NONCE,self.child,BASE_NS+190_000_000_000,clock=lambda:self.now)
        return code,guard.raw_json(self.base/'CHILD_RESULT.json')[1]

class ChildTests(ChildFixture):
    def test_five_natural_stages_never_native(self):
        code,result=self.run_child();self.assertEqual(code,0);self.assertEqual(self.labels,list(diag.matrix.DISPLAY_LABELS))
        self.assertFalse(result['stage_PASS_claim']);self.assertFalse(result['cancelled']);self.assertEqual(result['full12'],'NOT_RUN')
    def test_failed_joined_helper_restores_but_remains_fail(self):
        self.fail_label='display-active';code,result=self.run_child();self.assertEqual(code,2)
        self.assertEqual(self.labels,['display-before','display-active','display-restored','display-restored-probe'])
        self.assertTrue(result['cleanup_ack']);self.assertFalse(result['success']);self.assertIn('synthetic failed helper',result['failure'])
    def test_missing_entire_pair_blocks_restore(self):
        self.fail_label='display-active';self.omit_pair=True;code,result=self.run_child()
        self.assertEqual(code,2);self.assertEqual(self.labels,['display-before','display-active']);self.assertTrue(result['may_have_mutated'])
    def test_unknown_deadline_blocks_restore(self):
        self.fail_label='display-active';self.deadline=None;code,result=self.run_child()
        self.assertEqual(code,2);self.assertEqual(self.labels,['display-before','display-active']);self.assertIsNone(result['cleanup_deadline_ns'])
    def test_cancel_during_finally_cannot_start_late_probe(self):
        self.cancel_label='display-restored';self.deadline=BASE_NS+5_000_000_000;code,result=self.run_child()
        self.assertEqual(code,2);self.assertEqual(self.labels[-1],'display-restored');self.assertTrue(result['cancelled']);self.assertFalse(result['success'])
    def test_cancel_with_restoration_never_success(self):
        self.cancel_label='display-active';code,result=self.run_child()
        self.assertEqual(code,2);self.assertEqual(self.labels[-2:],['display-restored','display-restored-probe']);self.assertFalse(result['success'])
    def test_intent_write_failure_starts_no_helper(self):
        original=diag.immutable
        def fail(path,row):
            if path.name.endswith('.attempt-intent.json'):raise OSError('intent write failed')
            return original(path,row)
        diag.send(self.parent,dict(event='cleanup',nonce=NONCE,deadline_ns=self.deadline,clock_domain=CLOCK_DOMAIN))
        with patch.object(diag,'immutable',side_effect=fail):code,result=self.run_child()
        self.assertEqual(code,2);self.assertEqual(self.labels,[]);self.assertIn('intent write failed',result['failure'])

class BudgetTests(TemporaryTest):
    def test_both_graces_shared_monitor_and_barrier(self):
        exact=BASE_NS+28_000_000_000+diag.RESERVE_NS
        self.assertEqual(diag.cleanup_allowance(exact,BASE_NS,helpers=2),10)
        with self.assertRaises(RuntimeError):diag.cleanup_allowance(exact-1,BASE_NS,helpers=2)
        with self.assertRaisesRegex(RuntimeError,'UNKNOWN'):diag.cleanup_allowance(None,BASE_NS)
    def test_only_display_helpers_no_app(self):
        commands=[diag.command_for(Path('/synthetic'),label) for label in diag.matrix.DISPLAY_LABELS]
        self.assertEqual([c[5:] for c in commands],[['probe','2'],['set60','2'],['probe','2'],['restore','2','113'],['probe','2']])
        self.assertTrue(all(c[:4]==list(guard.DISPLAY_SWIFT_PREFIX) for c in commands))
        with self.assertRaises(RuntimeError):diag.command_for(Path('/synthetic'),'app-launch')
    def test_nonsymlink_fresh_destination(self):
        root=self.base/'product';root.mkdir();link=self.base/'link';link.symlink_to(self.base,target_is_directory=True)
        for destination in (root/'evidence',link/'evidence',self.base):
            with self.assertRaises(RuntimeError):diag.fresh_destination(destination,root)
        self.assertEqual(diag.fresh_destination(self.base/'new',root),self.base/'new')
    def test_macos_var_alias_fixture_resolves_but_runtime_still_rejects_alias(self):
        private_var=self.base/'private'/'var';folders=private_var/'folders'
        folders.mkdir(parents=True)
        alias=self.base/'var';alias.symlink_to(private_var,target_is_directory=True)
        manager=tempfile.TemporaryDirectory(dir=str(alias/'folders'))
        fixture=TemporaryTest()
        with patch.object(tempfile,'TemporaryDirectory',return_value=manager):
            fixture.setUp()
        self.addCleanup(fixture.tearDown)
        raw_base=Path(manager.name)
        self.assertIn(alias,raw_base.parents)
        self.assertNotEqual(raw_base,fixture.base)
        self.assertEqual(fixture.base,raw_base.resolve(strict=True))
        self.assertTrue(os.path.samefile(raw_base,fixture.base))
        root=self.base/'product';root.mkdir()
        with self.assertRaisesRegex(RuntimeError,'fresh nonsymlink absolute evidence'):
            diag.fresh_destination(raw_base/'new',root)
        self.assertEqual(diag.fresh_destination(fixture.base/'new',root),fixture.base/'new')


class ParentLifecycleTests(TemporaryTest):
    def make_run(self, clock=lambda: BASE_NS):
        run=diag.DiagnosticRun(self.base/'product',self.base/'app',self.base/'capture',self.base,clock=clock)
        run.trace=Sink()
        parent,child=socket.socketpair(socket.AF_UNIX,socket.SOCK_DGRAM)
        parent.setblocking(False);child.setblocking(False)
        self.addCleanup(parent.close);self.addCleanup(child.close)
        run.channel=parent
        class Child:
            pid=42
            returncode=None
            signals=[]
            waits=[]
            def poll(self):return self.returncode
            def send_signal(self,sig):self.signals.append(sig)
            def wait(self,timeout):
                self.waits.append(timeout)
                if self.returncode is None:raise subprocess.TimeoutExpired('synthetic',timeout)
                return self.returncode
        run.child=Child()
        return run,child
    def test_deadline_frozen_before_journal_only_one_sigint_ack_exact(self):
        current=[BASE_NS]
        run,peer=self.make_run(lambda:current[0]);run.fail('HID_COUNTER_CHANGE')
        original=diag.immutable
        def journal(path,row):
            self.assertEqual(run.deadline,BASE_NS+40_000_000_000)
            current[0]+=20_000_000_000
            return original(path,row)
        with patch.object(diag,'immutable',side_effect=journal):run.interrupt()
        message=diag.receive(peer)
        self.assertEqual(message['deadline_ns'],BASE_NS+40_000_000_000)
        diag.send(peer,dict(message,event='cleanup_ack',pid=42))
        run.pump();self.assertTrue(run.ack);run.interrupt()
        self.assertEqual(run.child.signals,[signal.SIGINT]);self.assertEqual(run.deadline,BASE_NS+40_000_000_000)
    def test_deadline_missing_ack_is_not_delivery_success(self):
        run,peer=self.make_run();run.interrupt();run.pump();self.assertFalse(run.ack)
        msg=diag.receive(peer);diag.send(peer,dict(msg,event='cleanup_ack',pid=42,deadline_ns=msg['deadline_ns']+1))
        with self.assertRaisesRegex(RuntimeError,'ACK'):run.pump()
        self.assertFalse(run.ack)
    def test_unknown_clock_preserves_actual_bounded_join_before_journal(self):
        def bad():raise guard.ClockUnavailableError('synthetic clock unavailable')
        run,peer=self.make_run(bad);events=[]
        def wait(timeout):events.append(('join',timeout));run.child.returncode=2;return 2
        run.child.wait=wait
        with patch.object(diag,'immutable',side_effect=lambda *a:events.append(('journal',None))):run.interrupt()
        self.assertEqual(events,[('join',35.75),('journal',None)])
        self.assertTrue(run.cleanup_unknown);self.assertIsNone(diag.receive(peer)['deadline_ns'])
    def test_journal_failure_does_not_skip_join(self):
        run,peer=self.make_run();run.fail('original gap')
        with patch.object(diag,'immutable',side_effect=OSError('disk failure')):run.interrupt()
        run.child.returncode=2;run.wait_child(BASE_NS+190_000_000_000)
        self.assertEqual(run.child.waits,[0]);self.assertEqual(run.joined_ns,BASE_NS)
        self.assertEqual(run.failure,'original gap');self.assertTrue(run.errors)
    def test_diagnostic_submit_error_cannot_replace_latched_raw_gap(self):
        run,peer=self.make_run()
        class Stream:
            policy=GuardPolicy()
            def pump(self,**kwargs):
                self.policy.stop('HID_COUNTER_CHANGE')
                raise RuntimeError('diagnostic queue capacity')
        stream=Stream();run.observers=[dict(stream=stream)]
        with self.assertRaises(RuntimeError):run.pump()
        self.assertEqual(run.failure,'HID_COUNTER_CHANGE')
    def test_two_monitors_term_before_wait_one_shared_window(self):
        now=[BASE_NS];calls=[]
        run,peer=self.make_run(lambda:now[0]);run.channel=None
        class Actor:
            def __init__(self,pid):self.pid=pid;self.returncode=None
            def poll(self):return self.returncode
            def terminate(self):calls.append(('term',self.pid));self.returncode=-15
            def kill(self):calls.append(('kill',self.pid));self.returncode=-9
            def wait(self,timeout):calls.append(('wait',self.pid));return self.returncode
        class Tail:partial=b''
        run.observers=[dict(monitor=Actor(pid),tail=Tail()) for pid in (17,18)]
        run.pump=lambda **kwargs:calls.append(('pump',kwargs.get('allow_stop')))
        run.stop_observers()
        self.assertEqual(calls[:2],[('term',17),('term',18)])
        self.assertEqual(calls[-1],('pump',True));self.assertTrue(run.stopping)
    def test_unjoined_timeout_explicit_fail_no_new_recovery(self):
        run,peer=self.make_run();run.interrupt_sent=True;run.deadline=BASE_NS+diag.RESERVE_NS
        run.wait_child(BASE_NS+190_000_000_000)
        self.assertIn('NOT_JOINED',run.failure);self.assertIsNone(run.joined_ns)
        self.assertEqual(run.child.signals,[])

class FinalBoundaryCancellationTests(ChildFixture):
    def test_interrupt_after_restore_intent_blocks_new_helper(self):
        original=diag.immutable
        self.deadline=BASE_NS+5_000_000_000
        def inject(path,row):
            original(path,row)
            if path.name=='display-restored.attempt-intent.json':
                diag.send(self.parent,dict(event='cleanup',nonce=NONCE,deadline_ns=self.deadline,clock_domain=CLOCK_DOMAIN))
                signal.getsignal(signal.SIGINT)(signal.SIGINT,None)
        with patch.object(diag,'immutable',side_effect=inject):code,result=self.run_child()
        self.assertEqual(code,2);self.assertEqual(self.labels,['display-before','display-active','display-active-probe'])
        self.assertTrue(result['cleanup_ack']);self.assertFalse(result['success'])

if __name__=='__main__':unittest.main()
