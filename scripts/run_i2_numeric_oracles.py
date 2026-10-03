"""Compile current semantic core/probe and check independent analytic truth.
Test-only executables, no product CLI, new Cargo target, network or cleanup.
"""
import hashlib
import json
import os
import re
from pathlib import Path
import subprocess
import sys

ROOT = Path(__file__).resolve().parents[1]
REGRESSIONS = (
    'outer-0.25-1e-08-0-False', 'inner-0.25-1e-08-0-True',
    'outer-0.25-1e-08-1.1-False', 'inner-0.25-1e-08-1.1-True',
    'outer-0.25-1e-10-0-False', 'outer-0.25-1e-10-0.3-False',
    'outer-0.25-1e-10-1.1-False', 'outer-0.5-1e-10-0.3-False',
    'outer-0.5-1e-10-1.1-False', 'outer-1-1e-10-0.3-False',
    'outer-1-1e-10-1.1-False', 'outer-2-1e-10-0.3-False',
    'outer-2-1e-10-1.1-False', 'outer-4-1e-10-1.1-False',
)


def sha(p):
    return hashlib.sha256(p.read_bytes()).hexdigest()


def main():
    out = Path(os.environ['RCAM_I2_A_GATES_OUT'])/'numeric-oracles'
    out.mkdir(parents=True, exist_ok=False)
    deps = Path(os.environ['CARGO_TARGET_DIR'])/'release/deps'
    serde = max(deps.glob('libserde-*.rlib'), key=lambda p: p.stat().st_mtime_ns)
    records = []
    commands = [
        ['rustc', '--edition=2024', '--crate-name', 'editor_core', '--crate-type', 'rlib',
         '-O', str(ROOT/'crates/editor-core/src/lib.rs'), '-L', 'dependency='+str(deps),
         '--extern', 'serde='+str(serde), '-o', str(out/'libeditor_core_oracle.rlib')],
        ['rustc', '--edition=2024', '-O', str(ROOT/'scripts/i2_geometry_probe.rs'),
         '-L', 'dependency='+str(deps), '--extern',
         'editor_core='+str(out/'libeditor_core_oracle.rlib'), '-o', str(out/'probe')],
        [sys.executable, '-B', str(ROOT/'scripts/i2_circle_oracle.py'), str(out)],
        [sys.executable, '-B', str(ROOT/'scripts/i2_polygon_oracle.py'), str(out)],
        ['rustc', '--edition=2024', '-O', str(ROOT/'scripts/i2_macro_probe.rs'),
         '-L', 'dependency='+str(deps), '--extern',
         'editor_core='+str(out/'libeditor_core_oracle.rlib'), '-o', str(out/'macro_probe')],
        [sys.executable, '-B', str(ROOT/'scripts/i2_macro_oracle.py'), str(out)],
    ]
    for i, command in enumerate(commands):
        log = out/f'command-{i}.log'
        with log.open('w') as stream:
            code = subprocess.run(command, cwd=ROOT, stdout=stream,
                                  stderr=subprocess.STDOUT).returncode
        records.append(dict(command=command, exit_code=code, log=log.name, sha256=sha(log)))
        (out/'commands.json').write_text(json.dumps(records, indent=2)+'\n')
        print('numeric command', i, 'exit', code, flush=True)
        if code:
            print(log.read_text()[-6000:])
            return code
    circles = json.loads((out/'circle-oracle.json').read_text())
    polygons = json.loads((out/'polygon-oracle.json').read_text())
    macro=json.loads((out/'macro-oracle.json').read_text())
    rows = {r['case']: r for r in circles['rows']}
    regression_rows = [rows[n] for n in REGRESSIONS]
    assert all(r['actual'].startswith('READY ') for r in regression_rows)
    thin = rows['annulus-1000000.0-1e-11']
    assert thin['actual'].startswith('ERR PrecisionUncertain('), thin
    core_log=Path(os.environ.get('RCAM_I2_CORE_LOG',str(out.parent/'i2-core-release.log')))
    chain=[json.loads(s) for s in re.findall(r'\{[^\n]*"chain10k"[^\n]*\}',core_log.read_text())]
    assert len(chain)==2 and {r['chain10k'] for r in chain}=={'budget','external_cancel'}
    by_kind={r['chain10k']:r for r in chain}
    assert by_kind['budget']['outcome']=='ResourceLimit' and 2_000_000<=by_kind['budget']['polls']<2_100_000
    assert by_kind['budget']['max_unchecked_ms']<=500
    assert by_kind['external_cancel']['outcome']=='Cancelled' and by_kind['external_cancel']['cancel_request_to_return_ms']<=2000
    (out/'chain-observation.json').write_text(json.dumps(dict(rows=chain,claim='CPU callback/external request only; not native visible feedback'),indent=2)+'\n')
    summary = dict(schema_version=2, issues=['I2A-NUM-01','I2A-NUM-02','I2A-BUDGET-01'], result='NUMERIC_ORACLES_PASS',
        source_manifest_sha256=sha(ROOT/'MANIFEST.sha256'),
        circle_cases=circles['case_count'], circle_ready=circles['ready'],
        circle_zero=circles['zero'], circle_rejected=sum(circles['rejected'].values()),
        original_circle_cases=circles['baseline_case_count'],
        original_circle_ready=circles['baseline_ready'],
        original_14_successful_bounds_validated=regression_rows,
        polygon_cases=polygons['case_count'], polygon_ready=polygons['ready'],
        polygon_rejected=sum(polygons['rejected'].values()), failures=0,
        real_negative_thin_annulus=thin,
        circle_report_sha256=sha(out/'circle-oracle.json'),
        polygon_report_sha256=sha(out/'polygon-oracle.json'),
        probe_sha256=sha(out/'probe'),
        macro_cases=macro['cases'],macro_baseline_cases=macro['baseline_cases'],macro_ready=macro['ready'],
        macro_rejected=sum(macro['rejected'].values()),macro_practical_cases=macro['practical_cases'],macro_practical_ready=macro['practical_ready'],
        macro_original_32_replayed=macro['original_32_replayed'],macro_report_sha256=sha(out/'macro-oracle.json'),
        chain_report_sha256=sha(out/'chain-observation.json'),chain_observations=chain,
        claim='Exact f64-input analytic truth. Rejections are not accuracy successes; no GUI/native acceptance.')
    (out/'summary.json').write_text(json.dumps(summary, indent=2)+'\n')
    print(json.dumps({k:v for k,v in summary.items() if k not in
        ('original_14_successful_bounds_validated','macro_original_32_replayed')}, indent=2))
    return 0


if __name__ == '__main__':
    raise SystemExit(main())
