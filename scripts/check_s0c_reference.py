"""Local, synthetic differential checks; gerbv is never linked into the product.
No private inputs, no production Gerber export, no GUI. An installed gerbv is required.
"""
import argparse
import hashlib
import json
from pathlib import Path
import shutil
import subprocess


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--out', type=Path, required=True)
    args = parser.parse_args()
    args.out.mkdir(parents=True, exist_ok=False)
    binary = shutil.which('gerbv')
    if binary is None:
        raise SystemExit('BLOCKED: gerbv is not installed')
    cases = {}
    for units in ['MM', 'IN']:
        header = f'%FSLAX26Y26*%\n%MO{units}*%\n'
        aperture = '%ADD10C,0.1*%\nD10*\n'
        cases['io_' + units] = (
            header + '%IOA0.2B0.3*%\n' + aperture + 'X100000Y200000D03*\nM02*\n',
            header + aperture + 'X300000Y500000D03*\nM02*\n')
    header = '%FSLAX34Y34*%\n%MOMM*%\n'
    aperture = '%ADD10C,0.1*%\nD10*\n'
    cases['ic_ascii'] = (header + '%ICAS*%\n' + aperture + 'X10000Y10000D03*\nM02*\n',
                         header + aperture + 'X10000Y10000D03*\nM02*\n')
    cases['fsd_incremental'] = (
        '%FSDIX34Y34*%\n%MOMM*%\nG91*\n' + aperture + 'X0010000Y0010000D03*\nX0010000D03*\nM02*\n',
        header + aperture + 'X10000Y10000D03*\nX20000Y10000D03*\nM02*\n')
    results = []
    version = subprocess.run([binary, '--version'], capture_output=True, text=True, check=True)
    for name, pair in cases.items():
        outputs = []
        for index, source in enumerate(pair):
            path = args.out/f'{name}-{index}.gbr'
            path.write_text(source)
            output = path.with_suffix('.svg')
            command = [binary, '-x', 'svg', '-O', '0x0', '-W', '1x1', '-B', '0', '-o', str(output), str(path)]
            run = subprocess.run(command, capture_output=True, text=True, timeout=30)
            (path.with_suffix('.log')).write_text(json.dumps(command) + '\n' + run.stdout + run.stderr)
            if run.returncode or run.stderr.strip():
                raise RuntimeError(f'{name}: independent renderer returned diagnostics; inspect log')
            data = output.read_bytes()
            if b'<path' not in data:
                raise RuntimeError(f'{name}: empty rendering cannot be a comparison baseline')
            outputs.append(data)
        results.append(dict(check=name, status='passed' if outputs[0] == outputs[1] else 'failed',
                            hashes=[hashlib.sha256(data).hexdigest() for data in outputs]))
    report = dict(kind='synthetic_independent_differential_not_manufacturing_validation',
                  binary=str(Path(binary).resolve()), binary_sha256=hashlib.sha256(Path(binary).read_bytes()).hexdigest(),
                  version=version.stdout + version.stderr, results=results)
    (args.out/'results.json').write_text(json.dumps(report, indent=2))
    for item in results:
        print(item['check'], item['status'])
    return int(any(item['status'] != 'passed' for item in results))


if __name__ == '__main__':
    raise SystemExit(main())
