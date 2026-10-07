"""Explicit report-only comparisons; no performance or flicker PASS verdict."""
import math

SCOPE = 'current-refresh-functional-performance'


def numeric(value, *, positive=False):
    if type(value) not in (int, float) or not math.isfinite(value) or (value <= 0 if positive else value < 0):
        raise ValueError('invalid finite performance duration')
    return value


def distribution(values, *, positive=True):
    values = [numeric(value, positive=positive) for value in values]
    if not values:
        raise ValueError('missing performance samples')
    ordered = sorted(values)
    result = {'count': len(values), 'p50_ms': ordered[math.ceil(len(values)*.50)-1],
              'p95_ms': ordered[math.ceil(len(values)*.95)-1],
              'p99_ms': ordered[math.ceil(len(values)*.99)-1], 'max_ms': max(values)}
    result['over_50ms_count'] = sum(value > 50 for value in values)
    result['over_200ms_count'] = sum(value > 200 for value in values)
    result['long_stalls_over_200ms'] = [{'sample_index': index, 'duration_ms': value}
                                      for index, value in enumerate(values) if value > 200]
    return result


class PerformanceReport:
    def __init__(self):
        self.comparisons = []
        self.series = {}

    def compare(self, name, actual, historical_limit):
        numeric(actual, positive=True)
        numeric(historical_limit, positive=True)
        self.comparisons.append({'metric': name, 'actual_ms': actual,
                                 'historical_limit_ms': historical_limit,
                                 'result': 'EXCEEDED' if actual > historical_limit else 'WITHIN_HISTORICAL_LIMIT',
                                 'report_only': True})

    def samples(self, name, values, *, positive=True):
        self.series[name] = distribution(values, positive=positive)

    def result(self):
        return {'status': 'MEASURED_REPORT_ONLY', 'distributions': self.series,
                'historical_budget_comparisons': self.comparisons,
                'smoothness': 'UNVERIFIED', 'flicker': 'UNVERIFIED',
                'clock_scope': 'CPU/update intervals and input to GPU completion upper bounds; not scanout FPS',
                'video_scope': '60fps capture cannot establish 144fps presentation or absence of flicker'}
