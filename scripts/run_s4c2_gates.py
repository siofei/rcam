#!/usr/bin/env python3
"""Retain S4-C2 command results and source hashes without replacing earlier runs."""
import argparse
import hashlib
import json
import os
from pathlib import Path
import platform
import subprocess
import sys
import time

ROOT = Path(__file__).resolve().parents[1]


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--out', required=True, type=Path)
    args = parser.parse_args()
    out = args.out.resolve()
    out.mkdir(parents=True, exist_ok=False)
    env = os.environ.copy()
    env.update(PATH=str(ROOT / '.tools/cargo/bin') + os.pathsep + env['PATH'],
               CARGO_HOME=str(ROOT / '.tools/cargo'), RUSTUP_HOME=str(ROOT / '.tools/rustup'),
               CARGO_TARGET_DIR=str(ROOT / '.tools/target'),
               RCAM_GRIP_PERF_OUT=str(out / 'grip_performance.json'),
               PYTHONDONTWRITEBYTECODE='1')

    sys.dont_write_bytecode = True
    import source_manifest
    payload = source_manifest.source_files() + [ROOT / 'MANIFEST.sha256']
    (out / 'tested-source-hashes.txt').write_text(''.join(
        f'{hashlib.sha256(path.read_bytes()).hexdigest()}  {path.relative_to(ROOT).as_posix()}\n'
        for path in payload), encoding='utf-8')
    for name, command in [('HEAD.txt', ['git', 'rev-parse', 'HEAD']),
                          ('status-before.txt', ['git', 'status', '--porcelain=v1'])]:
        (out / name).write_bytes(subprocess.check_output(command, cwd=ROOT))
    (out / 'environment.json').write_text(json.dumps(dict(schema_version=2,
        stage='S4-C2', platform=platform.platform(), machine=platform.machine(),
        windows='deferred / not executed'), indent=2) + '\n', encoding='utf-8')

    commands = [
        ['cargo', 'fmt', '--all', '--', '--check'],
        ['cargo', 'check', '--workspace', '--all-targets', '--locked'],
        ['cargo', 'clippy', '--workspace', '--all-targets', '--locked', '--', '-D', 'warnings'],
        ['cargo', 'test', '--workspace', '--locked', '--no-fail-fast'],
        ['cargo', 'test', '--locked', '-p', 'editor-service', '--test', 'automation_contract',
         '--test', 'headless_workflow', '--test', 'runtime_diagnostics', '--test', 'grip_diagnostics'],
        ['cargo', 'test', '--locked', '-p', 'rcam-diagnostics'],
        ['cargo', 'tree', '--locked', '-p', 'editor-service', '-e', 'normal'],
        ['cargo', 'build', '--release', '--locked', '-p', 'editor-app'],
        ['python3', 'scripts/source_manifest.py', '--check'],
        ['python3', 'scripts/test_package_source.py'],
        ['cargo', 'test', '--locked', '-p', 'editor-core', '--test', 'grip_workflow'],
        ['cargo', 'test', '--locked', '-p', 'editor-service', '--test', 's4c2_grip_workflow'],
        ['cargo', 'test', '--locked', '-p', 'editor-app', 'grip::tests'],
        ['cargo', 'test', '--release', '--locked', '-p', 'editor-core', '--test',
         'grip_performance', '--', '--ignored', '--nocapture'],
    ]
    results = []
    for index, command in enumerate(commands):
        started = time.monotonic()
        with (out / f'{index:02d}.log').open('wb') as log:
            result = subprocess.run(command, env=env, cwd=ROOT, stdout=log,
                                    stderr=subprocess.STDOUT)
        results.append(dict(command=command, exit_code=result.returncode,
                            seconds=time.monotonic() - started, log=f'{index:02d}.log'))
        (out / 'gates.json').write_text(json.dumps(dict(schema_version=2,
            stage='S4-C2', commands=results, performance_output='grip_performance.json',
            windows='deferred / not executed'), indent=2) + '\n', encoding='utf-8')
        print(index, result.returncode, flush=True)
    (out / 'status-after.txt').write_bytes(subprocess.check_output(
        ['git', 'status', '--porcelain=v1'], cwd=ROOT))
    raise SystemExit(int(any(result['exit_code'] for result in results)))


if __name__ == '__main__':
    main()
