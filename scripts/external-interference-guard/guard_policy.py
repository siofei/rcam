"""Pure, injected policy for the external RCam microcheck guard.

No macOS calls, process control, event taps, or user content are used here.
CG any-input is age-only: the SDK does not promise an aggregate counter.
"""
from __future__ import annotations
from dataclasses import dataclass
import math

TYPES = ('mouseMoved', 'leftMouseDown', 'leftMouseUp', 'rightMouseDown',
         'rightMouseUp', 'keyDown', 'keyUp', 'flagsChanged', 'scrollWheel',
         'leftMouseDragged', 'rightMouseDragged', 'otherMouseDown',
         'otherMouseUp', 'otherMouseDragged', 'tabletPointer', 'tabletProximity')
STATES = ('hid', 'combined')
AGE_TOLERANCE_NS = 20_000_000
MAX_SAMPLE_GAP_NS = 250_000_000
FOREGROUND_LIMIT_NS = 5_000_000_000
BIND_LIMIT_NS = 20_000_000_000
READY_LIMIT_NS = 30_000_000_000


def require(condition, message):
    if not condition:
        raise ValueError(message)


@dataclass(frozen=True)
class Action:
    kind: str
    reason: str | None = None
    details: dict | None = None


def validate_sample(sample):
    require(type(sample) is dict and sample.get('event') == 'sample', 'sample event')
    require(type(sample.get('seq')) is int and sample['seq'] > 0, 'sample sequence')
    begin, end = sample.get('begin_ns'), sample.get('end_ns')
    require(type(begin) is int and type(end) is int and 0 < begin <= end,
            'sample clock')
    require(end - begin <= MAX_SAMPLE_GAP_NS, 'sample acquisition duration')
    require(sample.get('thread_main') is True, 'main-thread observation')
    for key in ('runner_pid', 'owned_pid'):
        require(type(sample.get(key)) is int and sample[key] >= 0, key)
    require(all(type(sample.get(key)) is bool for key in
                ('owned_ready', 'front_owned', 'owned_alive', 'capture_complete', 'identity_verified')),
            'owned phase fields')
    if sample['owned_pid'] == 0:
        require(not any(sample[key] for key in
                        ('owned_ready', 'front_owned', 'owned_alive', 'capture_complete', 'identity_verified')),
                'unbound owned state')
    else:
        require(sample['runner_pid'] > 0, 'owned process without runner')
        require(sample.get('identity_verified') is True, 'owned identity not verified')
    require(type(sample.get('sources')) is dict and set(sample['sources']) == set(STATES),
            'source tables')
    for state in STATES:
        rows = sample['sources'][state]
        require(type(rows) is dict and set(rows) == set(TYPES) | {'anyInput'},
                'event-type inventory')
        for name, row in rows.items():
            require(type(row) is dict, 'event row')
            clocks = [row.get(key) for key in
                      ('begin_ns', 'age_begin_ns', 'age_end_ns', 'end_ns')]
            require(all(type(clock) is int for clock in clocks) and
                    begin <= clocks[0] <= clocks[1] <= clocks[2] <= clocks[3] <= end,
                    'bracketed acquisition clocks')
            if name == 'anyInput':
                require('count_before' not in row and 'count_after' not in row,
                        'any-input must be age-only')
            else:
                for key in ('count_before', 'count_after'):
                    require(type(row.get(key)) is int and 0 <= row[key] <= 0xffffffff,
                            'event counter')
            age = row.get('age_seconds')
            if age is None:
                require(name != 'anyInput' and row['count_before'] == row['count_after'] == 0,
                        'age unavailable for observed input')
            else:
                require(type(age) in (int, float) and math.isfinite(age) and
                        0 <= age <= end / 1e9 + 1, 'invalid event age')
    return sample


def event_interval(row):
    """The last-event interval includes the actual API-call bracket."""
    if row['age_seconds'] is None:
        return None
    age_ns = row['age_seconds'] * 1_000_000_000
    return row['age_begin_ns'] - age_ns, row['age_end_ns'] - age_ns


def source_changes(previous, current, state):
    counters, events, backwards = [], [], []
    for name in (*TYPES, 'anyInput'):
        old, new = previous['sources'][state][name], current['sources'][state][name]
        if name != 'anyInput' and (new['count_before'] != old['count_after'] or
                                   new['count_before'] != new['count_after']):
            counters.append(name)
        old_interval, new_interval = event_interval(old), event_interval(new)
        if old_interval is not None and new_interval is not None:
            if new_interval[0] > old_interval[1] + AGE_TOLERANCE_NS:
                events.append(name)
            if new_interval[1] < old_interval[0] - AGE_TOLERANCE_NS:
                backwards.append(name)
        elif old_interval is not None and new_interval is None:
            backwards.append(name)
    return {'counter_types': counters, 'new_event_time_types': events,
            'backwards_time_types': backwards}


class GuardPolicy:
    def __init__(self):
        self.previous = None
        self.phase = 'ARMING'
        self.runner_pid = self.owned_pid = 0
        self.runner_started_ns = None
        self.foreground_wait_started_ns = None
        self.first_foreground_ns = None
        self.capture_completed_ns = None
        self.terminal = None

    def stop(self, reason, details=None):
        if self.terminal is None:
            self.phase = 'STOPPING'
            self.terminal = Action('halt', reason, dict(details or {}, human_attribution=False))
        return self.terminal

    def observe(self, sample):
        if self.terminal:
            return self.terminal
        try:
            validate_sample(sample)
        except (ValueError, KeyError, TypeError) as error:
            return self.stop('INVALID_OBSERVATION', {'error': str(error)})
        if self.previous is not None:
            if not (sample['seq'] == self.previous['seq'] + 1 and
                    self.previous['end_ns'] <= sample['begin_ns'] and
                    sample['begin_ns'] - self.previous['end_ns'] <= MAX_SAMPLE_GAP_NS):
                return self.stop('OBSERVATION_GAP_OR_CLOCK_ORDER')
            changes = {state: source_changes(self.previous, sample, state) for state in STATES}
            if changes['hid']['counter_types']:
                return self.stop('HID_COUNTER_CHANGE', changes)
            if changes['combined']['counter_types']:
                return self.stop('UNKNOWN_SESSION_INPUT', changes)
            if any(changes[state]['backwards_time_types'] for state in STATES):
                return self.stop('SENSOR_CLOCK_DISCONTINUITY', changes)
            if changes['hid']['new_event_time_types']:
                return self.stop('HID_NEW_EVENT_TIME', changes)
            if changes['combined']['new_event_time_types']:
                return self.stop('UNKNOWN_SESSION_INPUT', changes)
        else:
            for state in STATES:
                for name, row in sample['sources'][state].items():
                    if name != 'anyInput' and row['count_before'] != row['count_after']:
                        return self.stop('INPUT_DURING_ARMING', {'source': state})
                    interval = event_interval(row)
                    if interval is not None and interval[0] > sample['begin_ns'] + AGE_TOLERANCE_NS:
                        return self.stop('INPUT_DURING_ARMING', {'source': state})
            if sample['runner_pid'] or sample['owned_pid']:
                return self.stop('RUNNER_STARTED_BEFORE_ARMING')
        prior = self.previous
        self.previous = sample
        if sample['runner_pid']:
            if self.phase == 'ARMING':
                return self.stop('RUNNER_STARTED_BEFORE_ARMING')
            if self.runner_pid and sample['runner_pid'] != self.runner_pid:
                return self.stop('RUNNER_IDENTITY_CHANGED')
            self.runner_pid = sample['runner_pid']
            self.runner_started_ns = self.runner_started_ns or sample['begin_ns']
            if self.phase == 'ARMED':
                self.phase = 'RUNNER_STARTED'
        elif self.runner_pid:
            return self.stop('RUNNER_BINDING_REMOVED')
        if sample['owned_pid']:
            if self.owned_pid and sample['owned_pid'] != self.owned_pid:
                return self.stop('OWNED_IDENTITY_CHANGED')
            self.owned_pid = sample['owned_pid']
            if self.phase == 'RUNNER_STARTED':
                self.phase = 'OWNED_STARTED'
        elif self.owned_pid:
            return self.stop('OWNED_BINDING_REMOVED')
        # Latch the first verified foreground observation even before window-ready.
        if sample['front_owned'] and self.first_foreground_ns is None:
            self.first_foreground_ns = sample['end_ns']
        if sample['capture_complete']:
            if self.first_foreground_ns is None:
                return self.stop('CAPTURE_COMPLETE_BEFORE_FOREGROUND')
            self.capture_completed_ns = self.capture_completed_ns or sample['end_ns']
            self.phase = 'CAPTURE_FINALIZED_CLEANUP'
        # The original capture-complete boundary permits expected app release;
        # input protection remains active throughout the runner's owned cleanup.
        if self.first_foreground_ns is not None and self.capture_completed_ns is None and not sample['front_owned']:
            return self.stop('FOCUS_LOST_AFTER_FIRST_FOREGROUND')
        if self.owned_pid and not sample['owned_alive'] and self.capture_completed_ns is None:
            return self.stop('OWNED_APP_EXITED_BEFORE_CAPTURE_COMPLETE')
        if self.runner_started_ns is not None and self.capture_completed_ns is None:
            elapsed = sample['end_ns'] - self.runner_started_ns
            if not self.owned_pid and elapsed > BIND_LIMIT_NS:
                return self.stop('OWNED_BIND_DEADLINE')
            if not sample['owned_ready'] and elapsed > READY_LIMIT_NS:
                return self.stop('OWNED_READY_DEADLINE')
        if sample['owned_ready'] and self.capture_completed_ns is None:
            if sample['front_owned']:
                self.phase = 'ACTIVE'
            elif self.foreground_wait_started_ns is None:
                self.phase = 'OWNED_READY_AWAIT_FOREGROUND'
                self.foreground_wait_started_ns = sample['end_ns']
        if self.foreground_wait_started_ns is not None and self.first_foreground_ns is None:
            if sample['end_ns'] - self.foreground_wait_started_ns > FOREGROUND_LIMIT_NS:
                return self.stop('OWNED_FOREGROUND_DEADLINE')
        if self.phase == 'ARMING' and prior is not None:
            self.phase = 'ARMED'
            return Action('armed', details={'baseline_seq': sample['seq']})
        return Action('continue', details={'phase': self.phase})

