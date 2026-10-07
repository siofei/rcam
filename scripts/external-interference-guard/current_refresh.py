"""External nav1 then move1 at unchanged144Hz; no full12/aggregate or display recovery."""
import argparse
import importlib
import importlib.util
from pathlib import Path
import sys

import matrix
import supervise as guard
from performance_v1 import PerformanceReport, SCOPE

CASES = (('nav', 1), ('move', 1))


def bound_import(name, expected):
    """Reject cached or shadow modules before any unreviewed module can execute."""
    cached = sys.modules.get(name)
    if cached is not None:
        guard.require(Path(cached.__file__).resolve(strict=True) == expected, 'different cached module: ' + name)
    spec = importlib.util.find_spec(name)
    guard.require(spec is not None and spec.origin is not None and
                  Path(spec.origin).resolve(strict=True) == expected, 'shadow module: ' + name)
    module = importlib.import_module(name)
    guard.require(Path(module.__file__).resolve(strict=True) == expected, 'exact module import: ' + name)
    return module


def report_verifier(root):
    scripts = root / 'scripts'
    external = Path(__file__).resolve(strict=True).parent
    # External copies take precedence; original helpers remain bound to source pins.
    sys.path.insert(0, str(scripts))
    sys.path.insert(0, str(external))
    for name in ('verify_s5m2_evidence', 'verify_pmix_workflow', 'verify_i2_c_ui_roi',
                 'verify_pmix_capture', 'pmix_owned_command', 'pmix_display_swift'):
        bound_import(name, scripts / (name + '.py'))
    for name in ('performance_v1', 'report_batch_v1', 'report_pmix_v1'):
        module = bound_import(name, external / (name + '.py'))
    return module.verify


def report(root, directory, local, receipt, seen_ids):
    """Authenticate original bytes and run every copied product functional predicate."""
    pins = matrix.require_pins()
    matrix.verify_source(root, pins)
    request = guard.strict_json((directory / 'request.json').read_bytes())
    case = (request.get('mode'), request.get('round'))
    guard.require(case in CASES, 'current-refresh cases only')
    launch = guard.strict_json((local / 'runner-launch.json').read_bytes())
    guard.require(launch.get('root') == str(root) and launch.get('product_pins') == {
        'source_manifest_sha256': pins.manifest_sha, 'binary_sha256': pins.binary_sha,
        'capture_producer_sha256': pins.producer_sha, 'runner_sha256': pins.runner_sha},
        'external exact product launch pins')
    guard.require(guard.sha(Path(launch['command'][launch['command'].index('--binary')+1])) == pins.binary_sha and
                  guard.sha(Path(launch['command'][launch['command'].index('--capture-producer')+1])) == pins.producer_sha,
                  'unchanged actual runtime binaries')
    matrix.validate_round(directory, case, receipt, set(seen_ids), None, local, display_policy='preserve')
    # Full source authentication precedes importing any original product helper.
    verify = report_verifier(root)
    performance = PerformanceReport()
    try:
        return verify(directory, source_manifest=pins.manifest_sha, commit=pins.commit,
                      binary_sha256=pins.binary_sha, capture_producer_sha256=pins.producer_sha,
                      product_root=root, performance=performance)
    except (OSError, ValueError, KeyError, TypeError, StopIteration, OverflowError) as error:
        # Only an explicit FAIL receipt; never convert a failed functional assertion to PASS.
        return {'scope': SCOPE, 'functional_result': 'FAIL',
                'functional_error': type(error).__name__ + ': ' + str(error),
                'performance': dict(performance.result(), validation='PARTIAL_BEFORE_FUNCTIONAL_FAILURE'),
                'stage_PASS_claim': False, 'overall_PASS_claim': False, 'user_flicker_report': 'OPEN'}


def check(input_path, output):
    _, inputs, input_sha = guard.raw_json(input_path)
    guard.require(type(inputs) is dict and set(inputs) == {'scope', 'root', 'directory', 'local', 'receipt_sha256', 'seen_ids'}
                  and inputs['scope'] == SCOPE and type(inputs['seen_ids']) is list
                  and len(inputs['seen_ids']) == len(set(inputs['seen_ids'])), 'report worker input')
    root, directory, local = [Path(inputs[key]).resolve(strict=True) for key in ('root', 'directory', 'local')]
    guard.require(str(directory) == inputs['directory'] and str(root) == inputs['root'] and str(local) == inputs['local'],
                  'canonical worker paths')
    _, receipt, receipt_sha = guard.raw_json(local / 'SUPERVISOR_RESULT.json')
    guard.require(receipt_sha == inputs['receipt_sha256'], 'worker original supervisor bytes')
    result = report(root, directory, local, receipt, inputs['seen_ids'])
    result.update(input_sha256=input_sha, supervisor_receipt_sha256=receipt_sha,
                  directory=str(directory), run_id=receipt['run_id'])
    guard.require(output.parent.resolve(strict=True) == local and not output.exists(), 'fresh external report destination')
    guard.write(output, result)
    return 0 if result['functional_result'] == 'PASS' else 2


def run(*, root, binary, producer, evidence):
    pins = matrix.require_pins()
    guard.require(sys.platform == 'darwin', 'current-refresh GUI runs only on macOS')
    guard.require(not root.is_symlink() and not evidence.is_symlink(), 'regular source/evidence roots')
    root, binary, producer = [path.resolve(strict=True) for path in (root, binary, producer)]
    evidence = evidence.resolve(strict=False)
    guard.require(root != evidence and root not in evidence.parents and evidence not in root.parents,
                  'fresh runtime evidence outside frozen product checkout')
    matrix.verify_source(root, pins)
    guard.require(guard.sha(binary) == pins.binary_sha and guard.sha(producer) == pins.producer_sha, 'exact app/producer pins')
    evidence.mkdir(parents=True, exist_ok=False)
    guard.system_uptime_ns()
    monitor = matrix.compile_monitor(evidence, Path(__file__).with_name('interference.swift'))
    sentinel = matrix.ContinuousSentinel(evidence, *monitor)
    results, seen_ids = [], set()
    launch_times, joined_times = [], []
    failure = None
    try:
        sentinel.start()
        for mode, round_number in CASES:
            guard.require(sentinel.pump() is None, 'continuous input halt between current-refresh rounds')
            local = evidence / (mode + str(round_number))
            local.mkdir(exist_ok=False)
            output = evidence / ('native-' + mode + str(round_number))
            full = guard.FullRun(root, binary, producer, output, mode, round_number, 2,
                                 pins.manifest_sha, pins.binary_sha, pins.producer_sha, pins.runner_sha,
                                 display_policy='preserve')
            code = guard.run(local, full=full, continuous_guard=sentinel.pump, compiled_monitor=monitor)
            _, receipt, receipt_sha = guard.raw_json(local / 'SUPERVISOR_RESULT.json')
            row = {'mode': mode, 'round': round_number, 'guard_exit_code': code, 'guard_success': receipt.get('success')}
            results.append(row)
            guard.require(code == 0 and receipt.get('success') is True, 'first failed owned run stops current-refresh driver')
            launch = guard.strict_json((local / 'runner-launch.json').read_bytes())
            launch_times.append(launch['started_uptime_ns'])
            joined_times.append(receipt['runner_joined_at_ns'])
            worker_input = local / 'report-input.json'
            guard.write(worker_input, dict(scope=SCOPE, root=str(root), directory=str(output), local=str(local),
                                           receipt_sha256=receipt_sha, seen_ids=sorted(seen_ids)))
            _, _, input_sha = guard.raw_json(worker_input)
            command = ['python3', '-B', str(Path(__file__).resolve()), 'report', '--input', str(worker_input),
                       '--output', str(local / 'REPORT.json')]
            matrix.watched_command(command, root, local, 'functional-report', sentinel)
            # The same sentinel watches all expensive raw replay and product verification.
            result = guard.strict_json((local / 'REPORT.json').read_bytes())
            _, _, current_receipt_sha = guard.raw_json(local / 'SUPERVISOR_RESULT.json')
            guard.require(guard.sha(worker_input) == input_sha == result.get('input_sha256') and
                          receipt_sha == current_receipt_sha == result.get('supervisor_receipt_sha256') and
                          result.get('directory') == str(output) and result.get('run_id') == receipt['run_id'] and
                          result.get('scope') == SCOPE and result.get('functional_result') == 'PASS',
                          'original report input/receipt/case/function binding')
            seen_ids.add(receipt['run_id'])
            row.update(functional_result='PASS', report_sha256=guard.sha(local / 'REPORT.json'))
    except (Exception, KeyboardInterrupt) as error:
        failure = type(error).__name__ + ': ' + str(error)
    finally:
        continuous_pass = sentinel.finish()
        continuous_replay = False
        if failure is None and len(seen_ids) == 2 and continuous_pass:
            try:
                continuous_replay = matrix.verify_continuous_evidence(evidence, launch_times, joined_times)
            except (OSError, ValueError, RuntimeError, KeyError, TypeError) as error:
                failure = 'continuous raw replay: ' + type(error).__name__ + ': ' + str(error)
        completed = failure is None and len(results) == 2 and len(seen_ids) == 2 and continuous_pass and continuous_replay
        result = {'scope': SCOPE, 'execution_result': 'COMPLETE' if completed else 'FAIL',
                  'functional_result': 'PASS' if completed else 'FAIL', 'failure': failure,
                  'runs': results, 'continuous_input_guard': continuous_pass, 'retry': False,
                  'continuous_raw_replay': continuous_replay,
                  'display_policy': 'preserve', 'display_mode_change_authorized': False,
                  'display_recovery': 'NONE_PRESERVE_PROBES_ONLY', 'performance': 'REPORT_ONLY',
                  'flicker': 'UNVERIFIED', 'user_flicker_report': 'OPEN',
                  'stage_PASS_claim': False, 'overall_PASS_claim': False,
                  'build_commit': pins.commit, 'source_manifest_sha256': pins.manifest_sha}
        guard.write(evidence / 'CURRENT_REFRESH_RESULT.json', result)
    return 0 if completed else 2


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('operation', choices=('run', 'report'))
    for name in ('root', 'binary', 'producer', 'evidence', 'input', 'output'):
        parser.add_argument('--' + name, type=Path)
    args = parser.parse_args()
    if args.operation == 'report':
        if args.input is None or args.output is None:
            parser.error('report requires --input and --output')
        return check(args.input, args.output)
    if any(getattr(args, name) is None for name in ('root', 'binary', 'producer', 'evidence')):
        parser.error('run requires --root --binary --producer --evidence')
    return run(root=args.root, binary=args.binary, producer=args.producer, evidence=args.evidence)


if __name__ == '__main__':
    raise SystemExit(main())
