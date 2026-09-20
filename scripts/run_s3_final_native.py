"""Run the release macOS app's S3/P1K native harness and retain raw output."""
import argparse
import hashlib
import json
import os
from pathlib import Path
import subprocess


ROOT = Path(__file__).resolve().parents[1]


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--binary', type=Path, required=True)
    parser.add_argument('--out', type=Path, required=True)
    args = parser.parse_args()
    binary = args.binary.resolve()
    out = args.out.resolve()
    if out.exists():
        raise SystemExit('Refusing to reuse native evidence directory')
    out.parent.mkdir(parents=True, exist_ok=True)
    env = os.environ.copy()
    env.update(RCAM_NATIVE_BENCH='s2b32', RCAM_S2C1_GATE='1',
               RCAM_BENCH_OUT=str(out))
    command = [str(binary)]
    log = out.parent/'native.log'
    with log.open('w', encoding='utf-8') as stream:
        result = subprocess.run(command, cwd=ROOT, env=env,
                                stdout=stream, stderr=subprocess.STDOUT)
    record = dict(
        schema_version=2,
        stage='S3-FINAL',
        command=command,
        exit_code=result.returncode,
        binary_sha256=hashlib.sha256(binary.read_bytes()).hexdigest(),
        git_commit=subprocess.check_output(
            ['git', 'rev-parse', 'HEAD'], cwd=ROOT, text=True).strip(),
        environment={
            'RCAM_NATIVE_BENCH': 's2b32',
            'RCAM_S2C1_GATE': '1',
            'RCAM_BENCH_OUT': str(out),
        },
        windows='deferred / not executed',
    )
    if out.is_dir():
        (out/'native-command.json').write_text(
            json.dumps(record, indent=2, sort_keys=True)+'\n', encoding='utf-8')
    raise SystemExit(result.returncode)


if __name__ == '__main__':
    main()
