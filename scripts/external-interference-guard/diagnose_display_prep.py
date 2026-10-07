"""Opt-in bounded display preparation diagnosis. Never native/stage acceptance.

Darwin-only runtime; imports and synthetic tests have no process side effects.
The original guard policy, parser, product helpers and thresholds are reused.
"""
import argparse
from collections import deque
import json
import os
from pathlib import Path
import signal
import socket
import subprocess
import sys
import threading
import time
import uuid

import matrix
import supervise as guard
from guard_policy import GuardPolicy, CLOCK_DOMAIN, MAX_SAMPLE_GAP_NS, TYPES

SCOPE = 'DISPLAY_PREPARATION_DIAGNOSTIC'
QUEUE_LIMIT = 256
LINE_LIMIT = 8192
PARENT_LIMIT = 8 * 1024 * 1024
OBSERVER_LIMIT = 16 * 1024 * 1024
RAW_LIMIT = 64 * 1024 * 1024
QUEUE_BYTES = 768 * 1024
RESERVE_NS = int((guard.MONITOR_STOP_SECONDS + guard.POST_JOIN_BARRIER_SECONDS) * 1e9)
# owned_command's two group-drain grace periods remain in the same deadline.
HELPER_WORST_NS = 14_000_000_000
FAILURE_LIMIT = 4096
FAILURE_REASONS = frozenset(('QUEUE_COUNT', 'QUEUE_MEMORY', 'CLOSED', 'WRITER_FAILED',
    'TERMINAL_LOCK_BUSY', 'FLUSH_DEADLINE', 'ENCODE', 'LINE_CAPACITY', 'SIDECAR_CAPACITY', 'WRITE',
    'SIDECAR_OPEN', 'RAW_ENCODE', 'RAW_WRITE', 'RAW_CAPACITY'))
PAYLOAD_KINDS = frozenset(('header', 'cycle_enter', 'cycle', 'emit_start', 'emit_progress', 'emit_return', 'terminal'))


class FailureChannel:
    """One bounded atomic receipt independent of a possibly failed observer FIFO."""
    def __init__(self, receiver, nonce, role, pid, source_sha, executable_sha):
        self.receiver = receiver
        self.identity = dict(nonce=nonce, role=role, pid=pid, source_sha256=source_sha,
                             executable_sha256=executable_sha)
        self.raw = self.receipt = self.error = None

    def poll(self):
        if self.error:
            return
        # At most one receipt and one duplicate probe, including across polls.
        for _ in range(2):
            try:
                raw, ancillary, flags, _ = self.receiver.recvmsg(FAILURE_LIMIT, 1, socket.MSG_DONTWAIT)
            except BlockingIOError:
                return
            try:
                guard.require(self.raw is None and raw and len(raw) <= FAILURE_LIMIT and not ancillary and
                              not flags & (socket.MSG_TRUNC | socket.MSG_CTRUNC) and raw.endswith(b'\n'),
                              'failure receipt duplicate/partial/truncated/control')
                self.raw = raw
                value = guard.strict_json(raw)
                required = {'schema_version', 'event', 'clock_domain', *self.identity,
                            'reason', 'payload_kind', 'observed_ns'}
                optional = {'submit_enter_ns', 'lock_acquired_ns', 'queue_count', 'retained_bytes', 'errno_code', 'raw_emit'}
                guard.require(type(value) is dict and required <= value.keys() <= required | optional and
                              all(value.get(key) == expected for key, expected in self.identity.items()) and
                              type(value['pid']) is int and type(value['schema_version']) is int and
                              value['schema_version'] == 1 and value['event'] == 'diagnostic_failure' and
                              value['clock_domain'] == CLOCK_DOMAIN and value['reason'] in FAILURE_REASONS and
                              value['payload_kind'] in PAYLOAD_KINDS,
                              'failure receipt binding/schema')
                def timestamp(key):
                    now = value.get(key)
                    guard.require(now is None or type(now) is int and now > 0, 'failure receipt actual timestamp')
                    return now
                observed, entered, acquired = (timestamp(key) for key in ('observed_ns', 'submit_enter_ns', 'lock_acquired_ns'))
                guard.require(observed is not None and (entered is None or entered <= observed) and
                              (acquired is None or entered is not None and entered <= acquired <= observed),
                              'failure receipt time order')
                for key, maximum in (('queue_count', QUEUE_LIMIT), ('retained_bytes', QUEUE_BYTES), ('errno_code', 2**31-1)):
                    number = value.get(key)
                    guard.require(number is None or type(number) is int and 0 <= number <= maximum,
                                  'failure receipt bounded facts')
                emit = value.get('raw_emit')
                if value['reason'].startswith('RAW_'):
                    guard.require(value['payload_kind'] == 'emit_return' and type(emit) is dict and
                                  type(emit.get('raw_record_ordinal')) is int and emit['raw_record_ordinal'] > 0 and
                                  emit.get('event') in ('ready', 'runner_bound', 'owned_bound', 'sample', 'fatal') and
                                  (emit.get('sample_seq') is None or type(emit.get('sample_seq')) is int and emit['sample_seq'] > 0) and
                                  (emit['event'] == 'sample') == (emit.get('sample_seq') is not None),
                                  'raw failure observed ordinal/event')
                    keys = ('encode_enter_ns', 'encode_return_ns', 'write_enter_ns', 'write_return_ns')
                    times = [emit.get(key) for key in keys]
                    known = [t for t in times if t is not None]
                    guard.require(all(type(t) is int and 0 < t <= observed for t in known) and known == sorted(known) and
                                  times[0] is not None and times[3] is None and emit.get('offset_after') is None and
                                  emit.get('state') == ('IN_PROGRESS' if value['reason'] == 'RAW_CAPACITY' else 'THREW'),
                                  'raw failure never invents write return')
                    guard.require((value['reason'] == 'RAW_ENCODE' and times[1:3] == [None, None]) or
                                  (value['reason'] == 'RAW_WRITE' and times[1] is not None and times[2] is not None) or
                                  (value['reason'] == 'RAW_CAPACITY' and times[1] is not None and times[2] is None),
                                  'raw failure phase')
                    guard.require(emit.keys() <= {'raw_record_ordinal', 'event', 'sample_seq', *keys, 'expected_bytes',
                        'offset_before', 'offset_after', 'state', 'error'} and emit.get('error') ==
                        (None if value['reason'] == 'RAW_CAPACITY' else 'raw encode/write failed'), 'raw failure fields')
                    expected, offset = emit.get('expected_bytes'), emit.get('offset_before')
                    if value['reason'] == 'RAW_ENCODE':
                        guard.require(expected is None and offset is None, 'raw encode unknown bytes')
                    else:
                        guard.require(type(expected) is int and 0 < expected <= RAW_LIMIT and type(offset) is int and
                                      0 <= offset <= RAW_LIMIT and (offset + expected > RAW_LIMIT) ==
                                      (value['reason'] == 'RAW_CAPACITY'), 'raw failure actual bytes/capacity')
                else:
                    guard.require(emit is None, 'non-raw failure raw fact')
                self.receipt = value
            except BaseException as error:
                self.error = type(error).__name__ + ': ' + str(error)
                return

    def summary(self):
        return dict(status='INVALID' if self.error else 'OBSERVED' if self.receipt else 'UNKNOWN',
                    receipt=self.receipt, error=self.error)

    def close(self):
        self.receiver.close()


def immutable(path, value):
    raw = (json.dumps(value, sort_keys=True, allow_nan=False) + '\n').encode()
    guard.require(len(raw) <= LINE_LIMIT, 'immutable diagnostic line limit')
    temporary = path.with_name(path.name + '.writing')
    with temporary.open('xb') as handle:
        handle.write(raw)
        handle.flush()
        os.fsync(handle.fileno())
    try:
        os.link(temporary, path)  # complete, no-replace publication
    finally:
        temporary.unlink()


class TraceWriter:
    """FIFO encoded rows; no lock is held while encoding or writing.

    Queue admission accounts bytes and count, rejects instead of dropping rows.
    The parent's additional queue budget is 768KiB, below the 2MiB ceiling.
    """
    def __init__(self, path, nonce, *, clock=guard.system_uptime_ns,
                 limit=PARENT_LIMIT, capacity=QUEUE_LIMIT, start=True):
        self.nonce, self.clock, self.limit, self.capacity = nonce, clock, limit, capacity
        self.handle = path.open('xb')
        self.pending = deque()
        self.pending_bytes = 0
        self.lock = threading.Lock()
        self.wake = threading.Event()
        self.done = threading.Event()
        self.error = None
        self.closing = False
        self.sequence = self.bytes = 0
        self.worker = threading.Thread(target=self._work, daemon=True)
        self.submit({'event': 'header', 'queue_limit': capacity, 'line_limit_bytes': LINE_LIMIT,
                     'sidecar_limit_bytes': limit, 'memory_limit_bytes': 2 * 1024 * 1024,
                     'script_sha256': guard.sha(Path(__file__)), 'pid': os.getpid()})
        if start:
            self.worker.start()

    def submit(self, row):
        raw = (json.dumps(dict(row, schema_version=1, nonce=self.nonce,
                               clock_domain=CLOCK_DOMAIN), allow_nan=False, sort_keys=True) + '\n').encode()
        guard.require(len(raw) <= LINE_LIMIT, 'diagnostic line limit')
        with self.lock:
            guard.require(not self.error and not self.closing, 'diagnostic writer failed/closed')
            if len(self.pending) >= self.capacity or self.pending_bytes + len(raw) > QUEUE_BYTES:
                self.error = 'diagnostic queue capacity'
                self.wake.set()
                raise RuntimeError(self.error)
            self.pending.append(raw)
            self.pending_bytes += len(raw)
        self.wake.set()

    def _work(self):
        try:
            while True:
                self.wake.wait()
                with self.lock:
                    if self.error:
                        raise RuntimeError(self.error)
                    raw = self.pending.popleft() if self.pending else None
                    if raw is not None:
                        self.pending_bytes -= len(raw)
                    empty, closing = not self.pending, self.closing
                    if empty:
                        self.wake.clear()
                if raw is not None:
                    self.sequence += 1
                    value = guard.strict_json(raw)
                    value['sequence'] = self.sequence
                    encoded = (json.dumps(value, allow_nan=False, sort_keys=True) + '\n').encode()
                    guard.require(len(encoded) <= LINE_LIMIT and self.bytes + len(encoded) <= self.limit,
                                  'diagnostic sidecar capacity')
                    self.handle.write(encoded)
                    self.bytes += len(encoded)
                if empty and closing:
                    seal = dict(schema_version=1, clock_domain=CLOCK_DOMAIN, nonce=self.nonce,
                                sequence=self.sequence + 1,
                                seal={'records_before_seal': self.sequence, 'bytes_before_seal': self.bytes})
                    encoded = (json.dumps(seal, sort_keys=True) + '\n').encode()
                    guard.require(self.bytes + len(encoded) <= self.limit, 'diagnostic seal capacity')
                    self.handle.write(encoded)
                    self.handle.flush()
                    return
        except BaseException as error:
            self.error = type(error).__name__ + ': ' + str(error)
        finally:
            self.handle.close()
            self.done.set()

    def finish(self, timeout=.2):
        self.submit({'event': 'terminal'})
        with self.lock:
            self.closing = True
        self.wake.set()
        guard.require(self.done.wait(timeout) and not self.error, 'diagnostic flush incomplete: ' + str(self.error))


class DiagnosticTail(guard.FileTail):
    """Same batch/read/parse order as FileTail; records only observed boundaries."""
    def __init__(self, path, trace, stream, pid, nonce, clock):
        super().__init__(path)
        self.trace, self.stream, self.pid, self.nonce, self.clock = trace, stream, pid, nonce, clock
        self.offset = 0
        self.span = 0
        self.active = None
        self.last_yielded = None
        self.audit = []

    def read(self):
        self.span += 1
        enter = self.clock()
        # Keep original MAX_BATCH/MAX_LINE and whole-batch parsing before delivery.
        data = self.handle.read(guard.MAX_BATCH_BYTES)
        returned = self.clock()
        self.partial += data
        chunks = self.partial.split(b'\n')
        self.partial = chunks.pop()
        guard.require(len(self.partial) <= guard.MAX_LINE_BYTES, 'unterminated monitor line too large')
        guard.require(all(0 < len(row) <= guard.MAX_LINE_BYTES for row in chunks), 'invalid monitor line size')
        rows = [guard.strict_json(row) for row in chunks]
        parsed = self.clock()
        # Avoid extra unbounded per-line metadata for a malicious tiny-line batch.
        guard.require(len(rows) <= QUEUE_LIMIT, 'diagnostic batch row capacity')
        self.audit = []
        for raw, row in zip(chunks, rows):
            self.audit.append({'span_id': self.span, 'stream': self.stream, 'pid': self.pid,
                'observer_nonce': self.nonce, 'raw_offset_before': self.offset,
                'raw_offset_after': self.offset + len(raw) + 1,
                'sample_seq': row.get('seq') if type(row) is dict else None,
                'event': row.get('event') if type(row) is dict else None,
                'validation_clock_ns': None, 'processed': False, 'error': None})
            self.offset += len(raw) + 1
        self.trace.submit({'event': 'tail_batch', 'span_id': self.span, 'stream': self.stream,
                           'pid': self.pid, 'observer_nonce': self.nonce, 'tail_read_enter_ns': enter,
                           'tail_read_return_ns': returned, 'batch_parse_return_ns': parsed, 'rows': len(rows)})
        tail = self
        class OrderedRows(list):
            def __iter__(self):
                try:
                    for index, row in enumerate(list.__iter__(self)):
                        tail.active = tail.audit[index]
                        tail.active['processed'] = True
                        tail.last_yielded = tail.active
                        yield row
                finally:
                    tail.active = None
        return OrderedRows(rows)


class DiagnosticStream(guard.ObservationStream):
    def __init__(self, tail, monitor, nonce, *, trace, clock=guard.system_uptime_ns,
                 policy=None, continuous_guard=None):
        self.trace, self.real_clock = trace, clock
        super().__init__(tail, monitor, nonce, policy=policy, clock=self.validation_clock,
                         continuous_guard=continuous_guard)

    def validation_clock(self):
        now = self.real_clock()
        active = self.tail.active
        if active is not None and active['event'] == 'sample' and active['validation_clock_ns'] is None:
            active['validation_clock_ns'] = now
        return now

    def pump(self, allow_owned_stop=False):
        enter = self.real_clock()
        original_first = self.first_stream_error
        self.tail.last_yielded = None
        try:
            return super().pump(allow_owned_stop=allow_owned_stop)
        finally:
            exited = self.real_clock()
            for item in self.tail.audit:
                first = self.first_stream_error
                if first is not original_first and first and item is self.tail.last_yielded:
                    item['error'] = first['error']
                self.trace.submit(dict(item, event='raw_validation', raw_event=item['event']))
            self.tail.audit = []
            self.trace.submit({'event': 'pump', 'span_id': self.tail.span, 'stream': self.tail.stream,
                'pid': self.monitor.pid, 'observer_nonce': self.nonce,
                'pump_enter_ns': enter, 'pump_exit_ns': exited})


def timed_callback(trace, callback, clock=guard.system_uptime_ns, *, stream='continuous', pid=None, nonce=None):
    span = 0
    def run():
        nonlocal span
        span += 1
        enter = clock()
        try:
            return callback()
        finally:
            trace.submit({'event': 'continuous_callback', 'span_id': span, 'stream': stream, 'pid': pid,
                          'observer_nonce': nonce, 'continuous_callback_enter_ns': enter,
                          'continuous_callback_return_ns': clock()})
    return run


def sealed_rows(path, nonce, limit, *, role=None, pid=None):
    guard.require(path.is_file() and not path.is_symlink() and path.stat().st_size <= limit,
                  'trace regular file/capacity')
    offset = count = 0
    terminal = False
    with path.open('rb') as handle:
        while True:
            raw = handle.readline(LINE_LIMIT + 1)
            guard.require(raw and len(raw) <= LINE_LIMIT and raw.endswith(b'\n'), 'trace_incomplete: missing seal/partial')
            row = guard.strict_json(raw)
            count += 1
            guard.require(type(row) is dict and type(row.get('sequence')) is int and row['sequence'] == count and
                          type(row.get('schema_version')) is int and row['schema_version'] == 1 and
                          row.get('clock_domain') == CLOCK_DOMAIN and row.get('nonce') == nonce and
                          (role is None or row.get('role') == role) and (pid is None or row.get('pid') == pid),
                          'trace sequence/identity')
            if 'seal' in row:
                guard.require(terminal and row['seal'] == {'records_before_seal': count - 1, 'bytes_before_seal': offset}
                              and handle.read(1) == b'', 'trace seal terminal/bytes/count/EOF')
                return
            guard.require(not terminal, 'trace record after terminal')
            terminal = row.get('event') == 'terminal' or 'terminal' in row.get('payload', {})
            yield row
            offset += len(raw)


def verify_observer(sidecar, raw_path, nonce, role, pid, source_sha, executable_sha):
    """Seal alone cannot certify raw writes. Match every returned emit to raw EOF."""
    guard.require(raw_path.is_file() and not raw_path.is_symlink() and raw_path.stat().st_size <= RAW_LIMIT,
                  'observer raw capacity/file')
    ordinal = offset = cycles = 0
    pending = None
    cycle_enter = None
    last_emit = None
    last_sample = None
    with raw_path.open('rb') as raw:
        for index, row in enumerate(sealed_rows(sidecar, nonce, OBSERVER_LIMIT, role=role, pid=pid)):
            payload = row.get('payload')
            admission = [row.get('submit_enter_ns'), row.get('lock_acquired_ns')]
            guard.require(all(type(t) is int and t > 0 for t in admission) and admission == sorted(admission),
                          'observer admission actual time order')
            guard.require(type(payload) is dict and len(payload) == 1, 'typed observer payload')
            kind, wrapped = next(iter(payload.items()))
            fact = wrapped.get('_0') if type(wrapped) is dict else None
            if index == 0:
                guard.require(kind == 'header' and fact == dict(source_sha256=source_sha,
                    executable_sha256=executable_sha, queue_limit=256, memory_limit_bytes=1048576,
                    sidecar_limit_bytes=OBSERVER_LIMIT, line_limit_bytes=LINE_LIMIT, raw_limit_bytes=RAW_LIMIT),
                    'observer header pins/limits')
                continue
            if kind == 'emit_start':
                guard.require(pending is None and fact['raw_record_ordinal'] == ordinal + 1 and
                              fact['state'] == 'IN_PROGRESS', 'emit start order')
                pending = fact
            elif kind == 'emit_progress':
                guard.require(pending is not None and pending.get('write_enter_ns') is None and
                              fact['raw_record_ordinal'] == pending['raw_record_ordinal'] and
                              fact['event'] == pending['event'] and fact.get('sample_seq') == pending.get('sample_seq') and
                              fact['state'] == 'IN_PROGRESS' and fact.get('write_return_ns') is None and
                              fact.get('offset_after') is None, 'emit progress fact')
                pending = fact
            elif kind == 'emit_return':
                guard.require(pending is not None and fact['raw_record_ordinal'] == pending['raw_record_ordinal'] and
                              fact['event'] == pending['event'] and fact.get('sample_seq') == pending.get('sample_seq') and
                              pending.get('write_enter_ns') is not None and
                              all(fact.get(key) == pending.get(key) for key in ('encode_enter_ns', 'encode_return_ns',
                                  'write_enter_ns', 'expected_bytes', 'offset_before')) and
                              fact['state'] == 'RETURNED' and fact.get('error') is None, 'emit return/progress fact')
                times = [fact.get(key) for key in ('encode_enter_ns', 'encode_return_ns', 'write_enter_ns', 'write_return_ns')]
                guard.require(all(type(t) is int and t > 0 for t in times) and times == sorted(times), 'emit observed time order')
                line = raw.readline(guard.MAX_LINE_BYTES + 2)
                guard.require(line and line.endswith(b'\n') and len(line) <= guard.MAX_LINE_BYTES + 1 and
                              fact['expected_bytes'] == len(line) and fact['offset_before'] == offset and
                              fact['offset_after'] == offset + len(line), 'emit raw expected bytes/offset')
                original = guard.strict_json(line)
                guard.require(original.get('nonce') == nonce and original.get('event') == fact['event'] and
                              original.get('seq') == fact.get('sample_seq'), 'emit raw binding')
                if fact['event'] == 'sample':
                    last_sample = original
                    guard.require(original['end_ns'] <= times[0], 'sample end remains before encode')
                ordinal += 1
                offset += len(line)
                last_emit = fact
                pending = None
            elif kind == 'cycle_enter':
                guard.require(cycle_enter is None and fact['cycle_id'] == cycles + 1 and
                              fact['sample_seq'] == cycles + 1 and fact.get('previous_emit') == last_emit,
                              'cycle carry/order')
                cycle_enter = fact
            elif kind == 'cycle':
                guard.require(cycle_enter is not None and fact['cycle_id'] == cycles + 1 and
                              fact['sample_seq'] == cycles + 1 and fact['timer_enter_ns'] == cycle_enter['timer_enter_ns'] and
                              fact.get('previous_emit') == cycle_enter.get('previous_emit'), 'cycle completion/order')
                times = [fact[key] for key in ('timer_enter_ns', 'control_enter_ns', 'control_return_ns',
                    'sample_begin_ns', 'sample_end_ns', 'timer_exit_ns')]
                guard.require(all(type(t) is int and t > 0 for t in times) and times == sorted(times) and
                              last_emit and last_emit['event'] == 'sample' and last_emit['sample_seq'] == cycles + 1,
                              'cycle sample/time binding')
                guard.require(last_sample is not None and fact['sample_begin_ns'] == last_sample['begin_ns'] and
                              fact['sample_end_ns'] == last_sample['end_ns'] and
                              last_emit['write_return_ns'] <= fact['timer_exit_ns'], 'cycle original raw sample endpoints/write return')
                fields = fact['field_build']
                expected = [(state, name) for state in ('hid', 'combined') for name in (*TYPES, 'anyInput')]
                guard.require([(f['source'], f['event_type']) for f in fields] == expected, 'all34 field builds')
                previous = fact['sample_begin_ns']
                for field in fields:
                    guard.require(previous <= field['build_enter_ns'] <= field['build_return_ns'] <= fact['sample_end_ns'], 'field build order')
                    previous = field['build_return_ns']
                cycles += 1
                cycle_enter = None
            elif kind == 'terminal':
                guard.require(pending is None and cycle_enter is None, 'trace_incomplete: emit/cycle in progress')
            else:
                raise RuntimeError('unknown observer diagnostic record')
        guard.require(ordinal > 0 and cycles >= 2 and raw.read(1) == b'', 'trace_incomplete: raw EOF/last ordinal')
    return {'raw_records': ordinal, 'cycles': cycles, 'raw_bytes': offset}


def command_for(directory, label):
    guard.require(label in matrix.DISPLAY_LABELS, 'only fixed display stages')
    operation = 'set60' if label == 'display-active' else ('restore' if label == 'display-restored' else 'probe')
    helper = directory / ('display-probe.swift' if operation == 'probe' else 'display.swift')
    command = [*guard.DISPLAY_SWIFT_PREFIX, str(helper), operation, '2']
    if operation == 'restore':
        command.append('113')
    return command


def prove_intents(directory, intents, nonce, pid):
    """Every prelaunch intent needs a full paired actual leader/group release."""
    labels = []
    for ordinal, intent in enumerate(intents, 1):
        label = intent['label']
        command = command_for(directory, label)
        _, persisted, _ = guard.raw_json(directory / (label + '.attempt-intent.json'))
        guard.require(persisted == intent and intent['nonce'] == nonce and intent['pid'] == pid and
                      intent['ordinal'] == ordinal and intent['argv'] == command and
                      intent['helper_sha256'] == guard.sha(Path(command[4])) and
                      intent['clock_domain'] == CLOCK_DOMAIN and type(intent['at_ns']) is int and intent['at_ns'] > 0,
                      'exact immutable prelaunch intent')
        _, launch, _ = guard.raw_json(directory / (label + '.subcommand-launch.json'))
        _, process, _ = guard.raw_json(directory / (label + '.subcommand-process.json'))
        guard.require(launch['command'] == command and type(launch['pid']) is int and launch['pid'] > 0 and
                      launch['pgid'] == launch['pid'] and launch['private_session'] is True and
                      launch == {key: process.get(key) for key in ('pid', 'pgid', 'private_session', 'command',
                          'started_monotonic_ns', 'timeout_seconds')} and
                      process.get('schema_version') == 2 and process.get('joined') is True and
                      process.get('owned_group_released') is True and type(process.get('exit_code')) is int,
                      'attempt has no paired actual join/group release')
        labels.append(label)
    guard.require(len(labels) == len(set(labels)) and
                  set(labels) == {p.name.removesuffix('.attempt-intent.json') for p in directory.glob('*.attempt-intent.json')} and
                  set(labels) == {p.name.removesuffix('.subcommand-launch.json') for p in directory.glob('*.subcommand-launch.json')} ==
                  {p.name.removesuffix('.subcommand-process.json') for p in directory.glob('*.subcommand-process.json')},
                  'intent/launch/process exact inventory')
    return True


def validate_five(directory, validators):
    states = []
    previous_finish = previous_phase = 0
    for label in matrix.DISPLAY_LABELS:
        command = command_for(directory, label)
        process = validators.verify_command(directory, label, command, 0)
        guard.require(previous_finish < process['started_monotonic_ns'], 'display actual joined stage order')
        previous_finish = process['finished_monotonic_ns']
        _, receipt, _ = guard.raw_json(directory / (label + '.json'))
        _, stdout, _ = guard.raw_json(directory / (label + '.subcommand.stdout'))
        guard.require(receipt == stdout, 'display raw receipt bytes semantics')
        validators.validate_display_receipt(receipt, 2, probe=command[5] == 'probe')
        phases = validators.validate_display_phases((directory / (label + '.subcommand.stderr')).read_text(),
                                                    command[5], 2, process['pid'], receipt)
        guard.require(previous_phase < phases[0]['at_ns'], 'display shared uptime stage order')
        previous_phase = phases[-1]['at_ns']
        for snapshot in receipt.values():
            guard.display_snapshot(snapshot, with_scale=command[5] == 'probe')
        states.append(receipt)
    before, active, probe, restored, final = states
    matrix.original_snapshot(before['after'])
    core = guard.display_snapshot
    guard.require(before['before'] == before['after'] and core(active['before'], False) == core(before['after']) and
                  active['after']['refresh_hz'] == 60 and probe['before'] == probe['after'] and
                  core(probe['after']) == core(active['after'], False) and
                  core(restored['after'], False) == core(before['after']) and
                  final['before'] == final['after'] == before['after'], 'five exact independent display stages')
    return before['after']


def receive(channel):
    try:
        raw = channel.recv(LINE_LIMIT + 1)
    except BlockingIOError:
        return None
    guard.require(0 < len(raw) <= LINE_LIMIT, 'control datagram size')
    return guard.strict_json(raw)


def send(channel, value):
    raw = json.dumps(value, allow_nan=False).encode()
    guard.require(len(raw) <= LINE_LIMIT and channel.send(raw) == len(raw), 'control delivery failed')


def cleanup_allowance(deadline, now, *, helpers=1):
    guard.require(type(deadline) is int and deadline > 0, 'cleanup deadline UNKNOWN')
    remaining = deadline - now - RESERVE_NS
    guard.require(remaining >= helpers * HELPER_WORST_NS, 'insufficient original cleanup budget')
    return 10


def child_cycle(root, directory, nonce, channel, execution_deadline, *, clock=guard.system_uptime_ns):
    """Five stages; errors still attempt bounded finally only with reliable parent ACK."""
    validators = guard.product_display_validators(root)
    intents = []
    failure = None
    cancelled = False
    cleaning = False
    in_helper = False
    cleanup = None
    may_have_mutated = False
    original = None
    def interrupted(_signal, _frame):
        nonlocal cancelled
        cancelled = True
        if not cleaning or in_helper:
            raise KeyboardInterrupt('owned display cycle cancelled')
    old = signal.signal(signal.SIGINT, interrupted)
    def obtain_cleanup():
        nonlocal cleanup, cancelled
        # Parent already froze its origin. This short receipt wait creates no budget.
        end = clock() + MAX_SAMPLE_GAP_NS
        while True:
            message = receive(channel)
            if message is not None:
                guard.require(message.get('event') == 'cleanup' and message.get('nonce') == nonce and
                              message.get('clock_domain') == CLOCK_DOMAIN and
                              (message.get('deadline_ns') is None or type(message['deadline_ns']) is int), 'cleanup channel binding')
                cleanup = message
                cancelled = True
                send(channel, dict(message, event='cleanup_ack', pid=os.getpid()))
                immutable(directory / 'cleanup-ack.json', dict(message, event='cleanup_ack', pid=os.getpid()))
                return message['deadline_ns']
            if clock() >= end:
                break
            time.sleep(.001)
        raise RuntimeError('cleanup deadline/ACK unavailable')
    def operation(label, deadline):
        nonlocal may_have_mutated, in_helper
        if cancelled and cleanup is None:
            deadline = obtain_cleanup()
        elif cleanup is not None:
            deadline = cleanup['deadline_ns']
        cleanup_allowance(deadline, clock(), helpers=2 if label == 'display-restored' else 1)
        prove_intents(directory, intents, nonce, os.getpid())
        command = command_for(directory, label)
        intent = {'nonce': nonce, 'pid': os.getpid(), 'label': label, 'argv': command,
                  'ordinal': len(intents) + 1, 'helper_sha256': guard.sha(Path(command[4])),
                  'clock_domain': CLOCK_DOMAIN, 'at_ns': clock()}
        immutable(directory / (label + '.attempt-intent.json'), intent)
        intents.append(intent)
        # SIGINT between intent and spawn leaves an unpaired intent, blocking restore.
        if label == 'display-active':
            may_have_mutated = True
        # Close Python's signal-checkpoint race between intent IO and Popen.
        # A pending SIGINT is delivered inside the protected try before spawn;
        # once owned_command actually launches it retains original bounded drain.
        previous_mask = signal.pthread_sigmask(signal.SIG_BLOCK, {signal.SIGINT})
        in_helper = True
        try:
            if cancelled and cleanup is None:
                deadline = obtain_cleanup()
            elif cleanup is not None:
                deadline = cleanup['deadline_ns']
            cleanup_allowance(deadline, clock(), helpers=2 if label == 'display-restored' else 1)
            signal.pthread_sigmask(signal.SIG_SETMASK, previous_mask)
            returned = validators.owned_command(directory, label, command, timeout=10, grace=2, check=True)
        finally:
            signal.pthread_sigmask(signal.SIG_SETMASK, previous_mask)
            in_helper = False
        process = validators.verify_command(directory, label, command, 0)
        receipt = guard.strict_json(returned.stdout)
        validators.validate_display_receipt(receipt, 2, probe=command[5] == 'probe')
        validators.validate_display_phases(returned.stderr, command[5], 2, process['pid'], receipt)
        for value in receipt.values():
            guard.display_snapshot(value, with_scale=command[5] == 'probe')
        immutable(directory / (label + '.json'), receipt)
        return receipt
    try:
        # Parent sends GO only after the exact child's kernel credential is observed.
        while True:
            guard.require(clock() < execution_deadline, 'child execution deadline before GO')
            go = receive(channel)
            if go:
                guard.require(go == {'event': 'go', 'nonce': nonce, 'pid': os.getpid(),
                                     'script_sha256': guard.sha(Path(__file__)), 'clock_domain': CLOCK_DOMAIN}, 'authenticated child GO')
                break
            time.sleep(.001)
        before = operation('display-before', execution_deadline)
        guard.require(before['before'] == before['after'], 'original probe stable')
        original = matrix.original_snapshot(before['after'])
        active = operation('display-active', execution_deadline)
        guard.require(guard.display_snapshot(active['before'], False) == guard.display_snapshot(before['after']) and
                      active['after']['refresh_hz'] == 60, 'setter original/60 snapshot')
        probe = operation('display-active-probe', execution_deadline)
        guard.require(probe['before'] == probe['after'] and
                      guard.display_snapshot(probe['after']) == guard.display_snapshot(active['after'], False), 'independent60 probe')
    except BaseException as error:
        failure = type(error).__name__ + ': ' + str(error)
    finally:
        cleaning = True
        try:
            deadline = execution_deadline
            if failure or cancelled:
                send(channel, {'event': 'child_failure', 'nonce': nonce, 'pid': os.getpid(), 'failure': failure})
                deadline = obtain_cleanup()
            guard.require(original is not None, 'no verified original snapshot; recovery blocked')
            # Any issued intent must be proven joined, even if the operation failed.
            prove_intents(directory, intents, nonce, os.getpid())
            cleanup_allowance(deadline, clock(), helpers=2)
            restored = operation('display-restored', deadline)
            final = operation('display-restored-probe', deadline)
            guard.require(final['before'] == final['after'] == before['after'] and
                          guard.display_snapshot(restored['after'], False) == guard.display_snapshot(before['after']), 'final complete restoration')
        except BaseException as error:
            failure = failure or type(error).__name__ + ': ' + str(error)
        signal.signal(signal.SIGINT, old)
    success = failure is None and not cancelled
    if success:
        try:
            prove_intents(directory, intents, nonce, os.getpid())
            validate_five(directory, validators)
        except BaseException as error:
            success = False
            failure = type(error).__name__ + ': ' + str(error)
    immutable(directory / 'CHILD_RESULT.json', {'scope': SCOPE, 'nonce': nonce, 'pid': os.getpid(),
              'script_sha256': guard.sha(Path(__file__)), 'success': success, 'failure': failure,
              'cancelled': cancelled, 'may_have_mutated': may_have_mutated,
              'cleanup_deadline_ns': cleanup['deadline_ns'] if cleanup else None,
              'cleanup_ack': cleanup is not None, 'stage_PASS_claim': False,
              'native': 'NOT_RUN', 'full12': 'NOT_RUN', 'full48': 'NOT_RUN'})
    return 0 if success else 2


def fresh_destination(path, root):
    guard.require(path.is_absolute() and not any(p.is_symlink() for p in (path, *path.parents)),
                  'fresh nonsymlink absolute evidence')
    guard.require(not path.exists() and path != root and root not in path.parents, 'new evidence outside product')
    guard.require(path.parent.is_dir(), 'evidence parent exists')
    return path


class DiagnosticRun:
    def __init__(self, root, binary, producer, evidence, *, clock=guard.system_uptime_ns):
        self.root, self.binary, self.producer, self.evidence = root, binary, producer, evidence
        self.clock = clock
        self.nonce = str(uuid.uuid4())
        self.script_sha = guard.sha(Path(__file__))
        self.trace = None
        self.observers = []
        self.child = self.channel = None
        self.child_handles = []
        self.failure = None
        self.errors = []
        self.deadline = None
        self.cleanup_unknown = self.interrupt_sent = self.ack = False
        self.joined_ns = None
        self.stopping = False
        self.phase_span = 0
        self.barriers = {}
        self.child_result = None
        self.helpers_join_proven = False

    def fail(self, reason):
        self.failure = self.failure or reason

    def start_observer(self, role, executable, digest, source_sha):
        nonce = str(uuid.uuid4())
        base = self.evidence
        control = {'protocolVersion': 3, 'nonce': nonce, 'commandID': 0,
                   'runnerPID': 0, 'appPID': 0, 'binaryPath': None, 'native': None, 'runID': None}
        path = base / (role + '-control.json')
        guard.write(path, control)
        out = (base / (role + '.stdout')).open('xb')
        err = (base / (role + '.stderr')).open('xb')
        env = dict(os.environ, RCAM_DIAG_ROLE=role, RCAM_DIAG_SOURCE_SHA=source_sha,
                   RCAM_DIAG_EXECUTABLE_SHA=digest)
        monitor = None
        receiver = peer = None
        try:
            receiver, peer = socket.socketpair(socket.AF_UNIX, socket.SOCK_DGRAM)
            receiver.setblocking(False)
            peer.setblocking(False)
            env['RCAM_DIAG_FAILURE_FD'] = str(peer.fileno())
            monitor = subprocess.Popen([str(executable), str(path), nonce, str(base / (role + '.diag.jsonl'))],
                                       stdin=subprocess.DEVNULL, stdout=out, stderr=err, env=env, pass_fds=(peer.fileno(),))
            # Retain exact owned handle before any tail/adapter construction fails.
            item = dict(role=role, nonce=nonce, monitor=monitor, control=control, path=path,
                        handles=(out, err), stream=None, tail=None,
                        failure_channel=FailureChannel(receiver, nonce, role, monitor.pid, source_sha, digest))
            self.observers.append(item)
            tail = DiagnosticTail(base / (role + '.stdout'), self.trace, role, monitor.pid, nonce, self.clock)
            item['tail'] = tail
            callback = None
            if role == 'owned':
                continuous = self.observers[0]['stream']
                callback = timed_callback(self.trace, lambda: continuous.pump(allow_owned_stop=self.stopping), self.clock,
                                          pid=continuous.monitor.pid, nonce=continuous.nonce)
            item['stream'] = DiagnosticStream(tail, monitor, nonce, trace=self.trace, clock=self.clock,
                policy=GuardPolicy(require_app_launch=role == 'owned'), continuous_guard=callback)
        except BaseException:
            if monitor is None:
                out.close()
                err.close()
                if receiver:
                    receiver.close()
            raise
        finally:
            if peer:
                peer.close()

    def poll_failure_channels(self):
        for item in self.observers:
            channel = item.get('failure_channel')
            if channel:
                channel.poll()
                if channel.error:
                    self.fail('OBSERVER_FAILURE_RECEIPT_INVALID: ' + channel.error)
                elif channel.receipt:
                    self.fail('OBSERVER_DIAGNOSTIC_FAILURE: ' + item['role'] + ': ' + channel.receipt['reason'])

    def pump(self, allow_stop=False):
        if self.trace.error:
            self.fail('PARENT_TRACE_FAILURE: ' + self.trace.error)
        # Owned's unchanged stream pump runs continuous first, preserving priority.
        item = self.observers[-1] if self.observers else None
        if item and item['stream']:
            try:
                halt = item['stream'].pump(allow_owned_stop=allow_stop)
            except BaseException:
                original = self.observers[0]['stream'].policy.terminal or item['stream'].policy.terminal
                if original:
                    self.fail(original.reason)
                raise
            if halt:
                continuous = self.observers[0]['stream']
                first = continuous.policy.terminal if continuous else None
                self.fail((first or halt).reason)
        for observer in self.observers:
            guard.require((self.evidence / (observer['role'] + '.stdout')).stat().st_size <= RAW_LIMIT,
                          'diagnostic raw capacity')
        # Raw stream/input retains first-error priority over supplemental receipts.
        self.poll_failure_channels()
        if self.channel:
            self.phase_span += 1
            enter = self.clock()
            try:
                while True:
                    message = receive(self.channel)
                    if message is None:
                        break
                    guard.require(message.get('nonce') == self.nonce and message.get('pid') == self.child.pid,
                                  'owned control child binding')
                    if message.get('event') == 'child_failure':
                        self.fail(message.get('failure') or 'CHILD_CANCELLED')
                    elif message.get('event') == 'cleanup_ack':
                        expected = dict(event='cleanup_ack', nonce=self.nonce, pid=self.child.pid,
                                        deadline_ns=self.deadline, clock_domain=CLOCK_DOMAIN)
                        guard.require(self.interrupt_sent and message == expected, 'cleanup ACK original deadline')
                        self.ack = True
                    else:
                        raise RuntimeError('unknown child control event')
            finally:
                self.trace.submit({'event': 'phase_poll', 'span_id': self.phase_span, 'stream': 'owned-child-control',
                                   'pid': self.child.pid, 'child_nonce': self.nonce,
                                   'phase_poll_enter_ns': enter, 'phase_poll_return_ns': self.clock()})
        return self.failure

    def interrupt(self):
        if self.interrupt_sent or self.child is None or self.child.poll() is not None:
            return
        self.child.send_signal(signal.SIGINT)
        # No journal or channel IO before this original anchor.
        self.interrupt_sent = True
        try:
            self.deadline = self.clock() + guard.CLEANUP_SECONDS * 1_000_000_000
        except guard.ClockUnavailableError as error:
            self.cleanup_unknown = True
            self.errors.append(str(error))
        try:
            send(self.channel, dict(event='cleanup', nonce=self.nonce,
                                    deadline_ns=self.deadline, clock_domain=CLOCK_DOMAIN))
        except BaseException as error:
            self.errors.append('deadline delivery: ' + repr(error))
        if self.cleanup_unknown:
            # Original clock-fault path: immediate bounded relative owned join,
            # before journal IO; it never supplies a child restoration deadline.
            try:
                self.child.wait(timeout=guard.CLEANUP_SECONDS - RESERVE_NS / 1e9)
            except subprocess.TimeoutExpired:
                self.fail('CHILD_NOT_JOINED_WITH_UNKNOWN_DEADLINE')
        try:
            immutable(self.evidence / 'controlled-interrupt.json', dict(pid=self.child.pid,
                      signal=int(signal.SIGINT), reason=self.failure, nonce=self.nonce,
                      deadline_ns=self.deadline, deadline_unknown=self.cleanup_unknown, clock_domain=CLOCK_DOMAIN))
        except BaseException as error:
            # Logging failure cannot bypass actual owned wait/pump below.
            self.errors.append('interrupt journal: ' + repr(error))
            self.fail('INTERRUPT_JOURNAL_FAILURE')

    def wait_child(self, execution_deadline):
        while True:
            try:
                self.pump()
            except BaseException as error:
                self.fail(type(error).__name__ + ': ' + str(error))
            code = self.child.poll()
            if code is not None:
                self.child.wait(timeout=0)
                self.joined_ns = self.clock()  # first observed join, never poll time guessed
                if not self.interrupt_sent and self.joined_ns > execution_deadline:
                    self.fail('EXECUTION_DEADLINE')
                if self.deadline is not None and self.joined_ns > self.deadline:
                    self.fail('CLEANUP_DEADLINE')
                return
            try:
                now = self.clock()
            except guard.ClockUnavailableError as error:
                self.fail('CLOCK_UNAVAILABLE')
                self.errors.append(str(error))
                self.interrupt()
                # Actual join check survives the clock error; no new wait reserve.
                try:
                    self.child.wait(timeout=0)
                except subprocess.TimeoutExpired:
                    self.fail('CHILD_JOIN_UNKNOWN')
                return
            if now >= execution_deadline:
                self.fail('EXECUTION_DEADLINE')
            if self.failure:
                self.interrupt()
            if self.interrupt_sent and (self.deadline is None or now >= self.deadline - RESERVE_NS):
                self.fail('CHILD_NOT_JOINED_WITHIN_ORIGINAL_CLEANUP')
                return
            time.sleep(.005)

    def post_join(self):
        if self.joined_ns is None:
            self.fail('CHILD_JOIN_UNKNOWN')
            return
        end = self.joined_ns + MAX_SAMPLE_GAP_NS
        # Both streams checked together; no second barrier window.
        while True:
            self.pump()
            now = self.clock()
            for item in self.observers:
                stream = item['stream']
                last = stream.last_valid_sample
                self.barriers[item['role']] = bool(stream.integrity and stream.policy.terminal is None and
                    now <= end and last and self.joined_ns <= last['begin_ns'] <= end)
            if all(self.barriers.values()) and len(self.barriers) == 2:
                return
            if self.failure or now >= end:
                self.fail('POST_JOIN_OBSERVATION_DEADLINE')
                return
            time.sleep(.005)

    def stop_observers(self):
        # All TERM requests before any wait. One shared 4s window includes flush.
        self.stopping = True
        for item in self.observers:
            if item['monitor'].poll() is None:
                item['monitor'].terminate()
        try:
            started = self.clock()
        except guard.ClockUnavailableError:
            self.fail('MONITOR_STOP_CLOCK_UNAVAILABLE')
            # Unknown shared remaining budget allows only immediate release/check.
            for item in self.observers:
                if item['monitor'].poll() is None:
                    item['monitor'].kill()
                try:
                    item['monitor'].wait(timeout=0)
                except subprocess.TimeoutExpired:
                    pass
            return
        end = started + guard.MONITOR_STOP_SECONDS * 1_000_000_000
        if self.deadline is not None:
            end = min(end, self.deadline)
        killed = False
        while any(item['monitor'].poll() is None for item in self.observers):
            try:
                self.pump(allow_stop=True)
            except BaseException as error:
                self.fail('MONITOR_STOP_PUMP: ' + str(error))
            try:
                now = self.clock()
            except guard.ClockUnavailableError:
                self.fail('MONITOR_STOP_CLOCK_UNAVAILABLE')
                now = end  # only used to cease wait/kill, never a recorded timestamp
            if not killed and (now >= started + 2_000_000_000 or now >= end):
                for item in self.observers:
                    if item['monitor'].poll() is None:
                        item['monitor'].kill()
                killed = True
            if now >= end:
                break
            time.sleep(.005)
        for item in self.observers:
            monitor = item['monitor']
            if monitor.poll() is not None:
                monitor.wait(timeout=0)
            if monitor.returncode != -int(signal.SIGTERM):
                self.fail('MONITOR_NOT_NORMAL_JOINED_SIGTERM')
        # Final actual raw drain includes records written during TERM handling.
        self.pump(allow_stop=True)
        for item in self.observers:
            if item['tail'].partial:
                self.fail('PARTIAL_FINAL_RAW')

    def run(self):
        pins = matrix.require_pins()
        guard.require(sys.platform == 'darwin', 'diagnostic execution requires macOS')
        matrix.verify_source(self.root, pins)
        guard.require(guard.sha(self.binary) == pins.binary_sha and guard.sha(self.producer) == pins.producer_sha,
                      'all original runtime pins')
        fresh_destination(self.evidence, self.root)
        self.clock()  # before filesystem mutation/compile/launch
        self.evidence.mkdir(mode=0o700)
        source = Path(__file__).with_name('interference.swift')
        source_sha = guard.sha(source)
        executable = digest = None
        try:
            self.trace = TraceWriter(self.evidence / 'parent.diag.jsonl', self.nonce, clock=self.clock)
            executable, digest = matrix.compile_monitor(self.evidence, source)
            for name, literal in (('display.swift', 'MUTATOR_SOURCE'), ('display-probe.swift', 'PROBE_SOURCE')):
                (self.evidence / name).write_text(matrix.helper_source(self.root, literal))
            immutable(self.evidence / 'DIAGNOSTIC_IDENTITY.json', dict(scope=SCOPE, nonce=self.nonce,
                guard_base='01c5ed6fabe104833e9242c008e14e087b490d39', product_commit=pins.commit,
                source_manifest_sha256=pins.manifest_sha, binary_sha256=pins.binary_sha,
                capture_producer_sha256=pins.producer_sha, runner_sha256=pins.runner_sha,
                script_sha256=self.script_sha, observer_source_sha256=source_sha,
                observer_executable_sha256=digest, stage_PASS_claim=False))
            self.start_observer('continuous', executable, digest, source_sha)
            self.start_observer('owned', executable, digest, source_sha)
            while not all(item['stream'].policy.phase == 'ARMED' for item in self.observers):
                self.pump()
                guard.require(self.failure is None, 'observer failure before owned child: ' + str(self.failure))
                time.sleep(.005)
            parent_channel, child_channel = socket.socketpair(socket.AF_UNIX, socket.SOCK_DGRAM)
            parent_channel.setblocking(False)
            child_channel.setblocking(False)
            self.channel = parent_channel
            started = self.clock()
            execution_deadline = started + guard.EXECUTION_SECONDS * 1_000_000_000
            guard.require(guard.sha(Path(__file__)) == self.script_sha and guard.sha(source) == source_sha and
                          guard.sha(executable) == digest, 'frozen diagnostic launch source/executable')
            command = [sys.executable, '-B', str(Path(__file__).resolve()), 'owned-display-cycle',
                       '--root', str(self.root), '--evidence', str(self.evidence), '--nonce', self.nonce,
                       '--control-fd', str(child_channel.fileno()), '--execution-deadline-ns', str(execution_deadline)]
            try:
                self.child_handles = [(self.evidence / 'child.stdout').open('xb'), (self.evidence / 'child.stderr').open('xb')]
                self.child = subprocess.Popen(command, stdin=subprocess.DEVNULL, stdout=self.child_handles[0],
                    stderr=self.child_handles[1], pass_fds=(child_channel.fileno(),))
            finally:
                child_channel.close()
            owned = self.observers[1]
            owned['stream'].expected_runner_pid = self.child.pid
            owned['control'].update(commandID=1, runnerPID=self.child.pid)
            guard.write(owned['path'], owned['control'])
            while not owned['stream'].runner_bound:
                self.pump()
                guard.require(not self.failure and self.child.poll() is None and self.clock() < execution_deadline,
                              'exact child kernel binding before helpers')
                time.sleep(.005)
            immutable(self.evidence / 'owned-child.json', dict(pid=self.child.pid, nonce=self.nonce,
                argv=command, script_sha256=self.script_sha, credential=owned['stream'].runner_credential,
                started_ns=started, execution_deadline_ns=execution_deadline, clock_domain=CLOCK_DOMAIN))
            send(self.channel, {'event': 'go', 'nonce': self.nonce, 'pid': self.child.pid,
                               'script_sha256': self.script_sha, 'clock_domain': CLOCK_DOMAIN})
            self.wait_child(execution_deadline)
        except BaseException as error:
            self.fail(type(error).__name__ + ': ' + str(error))
            if self.child is not None:
                try:
                    self.interrupt()
                    self.wait_child(execution_deadline)
                except BaseException as cleanup_error:
                    self.errors.append('owned join error: ' + repr(cleanup_error))
        finally:
            if self.child is not None and self.child.returncode is not None:
                try:
                    if self.joined_ns is None:
                        self.child.wait(timeout=0)
                        self.joined_ns = self.clock()
                    self.post_join()
                    _, child_result, _ = guard.raw_json(self.evidence / 'CHILD_RESULT.json')
                    guard.require(child_result.get('nonce') == self.nonce and child_result.get('pid') == self.child.pid and
                                  child_result.get('script_sha256') == self.script_sha and
                                  child_result.get('scope') == SCOPE and child_result.get('stage_PASS_claim') is False,
                                  'exact owned diagnostic child result')
                    self.child_result = child_result
                    if self.child.returncode != 0 or child_result.get('success') is not True:
                        self.fail(child_result.get('failure') or 'CHILD_DIAGNOSTIC_FAILURE')
                    attempts = sorted(self.evidence.glob('*.attempt-intent.json'))
                    intents = [guard.raw_json(path)[1] for path in attempts]
                    intents.sort(key=lambda item: item['ordinal'])
                    prove_intents(self.evidence, intents, self.nonce, self.child.pid)
                    self.helpers_join_proven = True
                    # Cancel/gap/partial/failed restoration cannot ever become complete.
                    guard.require(self.child.returncode == 0 and child_result.get('success') is True and
                                  child_result.get('cancelled') is False and not self.interrupt_sent,
                                  'five natural stages required')
                    intents = [guard.raw_json(self.evidence / (label + '.attempt-intent.json'))[1]
                               for label in matrix.DISPLAY_LABELS]
                    prove_intents(self.evidence, intents, self.nonce, self.child.pid)
                    validate_five(self.evidence, guard.product_display_validators(self.root))
                except BaseException as error:
                    self.fail(type(error).__name__ + ': ' + str(error))
            elif self.child is not None:
                self.fail('CHILD_UNJOINED_POSSIBLE_UNRESTORED_DISPLAY')
            try:
                if self.observers:
                    self.stop_observers()
                for item in self.observers:
                    guard.require(item['stream'].integrity and item['stream'].policy.terminal is None,
                                  'original stream integrity failed')
                    verify_observer(self.evidence / (item['role'] + '.diag.jsonl'),
                        self.evidence / (item['role'] + '.stdout'), item['nonce'], item['role'], item['monitor'].pid,
                        source_sha, digest)
            except BaseException as error:
                self.fail(type(error).__name__ + ': ' + str(error))
            try:
                self.poll_failure_channels()
            except BaseException as error:
                self.fail('OBSERVER_FAILURE_CHANNEL: ' + str(error))
            try:
                if self.trace:
                    timeout = .2 if self.deadline is None else max(0, min(.2, (self.deadline - self.clock()) / 1e9))
                    self.trace.finish(timeout)
                    for _ in sealed_rows(self.evidence / 'parent.diag.jsonl', self.nonce, PARENT_LIMIT):
                        pass
            except BaseException as error:
                self.fail('PARENT_TRACE_INCOMPLETE: ' + str(error))
            for item in self.observers:
                if item['tail']:
                    item['tail'].close()
                for handle in item['handles']:
                    handle.close()
                if item.get('failure_channel'):
                    item['failure_channel'].close()
            for handle in self.child_handles:
                handle.close()
            if self.channel:
                self.channel.close()
        complete = bool(self.child and self.child.returncode == 0 and self.failure is None and not self.errors and
                        self.helpers_join_proven and len(self.barriers) == 2 and all(self.barriers.values()) and not self.interrupt_sent)
        result = dict(scope=SCOPE, status='DIAGNOSTIC_COMPLETE' if complete else 'BLOCKED',
            stage_PASS_claim=False, native='NOT_RUN', full12='NOT_RUN', full48='NOT_RUN', nonce=self.nonce,
            first_failure=self.failure, errors=self.errors, child_pid=self.child.pid if self.child else None,
            child_exit=self.child.returncode if self.child else None, child_joined_at_ns=self.joined_ns,
            child_result=self.child_result, helpers_join_proven=self.helpers_join_proven,
            possible_unrestored_display=bool(self.child and (not self.helpers_join_proven or
                not self.child_result or self.child_result.get('success') is not True)),
            cleanup_deadline_ns=self.deadline, cleanup_ack=self.ack,
            cleanup_deadline_unknown=self.cleanup_unknown, interrupt_sent=self.interrupt_sent,
            post_join_barriers=self.barriers,
            observers=[dict(role=item['role'], pid=item['monitor'].pid, exit_code=item['monitor'].returncode,
                diagnostic_failure=item['failure_channel'].summary() if item.get('failure_channel') else
                    dict(status='UNKNOWN', receipt=None, error=None),
                first_stream_error=item['stream'].first_stream_error if item['stream'] else None,
                policy_phase=item['stream'].policy.phase if item['stream'] else None) for item in self.observers],
            clock_domain=CLOCK_DOMAIN, retry=False, external_activation=False, baseline_resets=0)
        immutable(self.evidence / 'DIAGNOSTIC_RESULT.json', result)
        print(json.dumps(result, allow_nan=False))
        return 0 if complete else 2


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    sub = parser.add_subparsers(dest='operation', required=True)
    run = sub.add_parser('run')
    for name in ('root', 'binary', 'producer', 'evidence'):
        run.add_argument('--' + name, type=Path, required=True)
    run.add_argument('--display-id', type=int, required=True, choices=(2,))
    child = sub.add_parser('owned-display-cycle')
    child.add_argument('--root', type=Path, required=True)
    child.add_argument('--evidence', type=Path, required=True)
    child.add_argument('--nonce', required=True)
    child.add_argument('--control-fd', type=int, required=True)
    child.add_argument('--execution-deadline-ns', type=int, required=True)
    args = parser.parse_args()
    if args.operation == 'run':
        return DiagnosticRun(args.root.resolve(strict=True), args.binary.resolve(strict=True),
                             args.producer.resolve(strict=True), args.evidence).run()
    guard.require(sys.platform == 'darwin' and str(uuid.UUID(args.nonce)) == args.nonce, 'internal Darwin child nonce')
    matrix.verify_source(args.root, matrix.require_pins())
    guard.require(args.evidence.is_dir() and not args.evidence.is_symlink() and
                  args.root not in args.evidence.parents, 'isolated diagnostic child evidence')
    for name, literal in (('display.swift', 'MUTATOR_SOURCE'), ('display-probe.swift', 'PROBE_SOURCE')):
        guard.require((args.evidence / name).read_text() == matrix.helper_source(args.root, literal), 'reviewed helper literal')
    channel = socket.socket(fileno=args.control_fd)
    channel.setblocking(False)
    try:
        return child_cycle(args.root, args.evidence, args.nonce, channel, args.execution_deadline_ns)
    finally:
        channel.close()


if __name__ == '__main__':
    raise SystemExit(main())
