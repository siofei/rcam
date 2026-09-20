"""Run S2-C2 local gates with immutable per-run logs; native GUI evidence is separate."""
import argparse
import hashlib
import json
import os
from pathlib import Path
import platform
import subprocess

ROOT = Path(__file__).resolve().parents[1]


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--out', type=Path, required=True)
    args = parser.parse_args()
    out = args.out.resolve()
    out.mkdir(parents=True, exist_ok=False)
    env = os.environ.copy()
    env.update(PATH=str(ROOT/'.tools/cargo/bin')+os.pathsep+env['PATH'],
               CARGO_HOME=str(ROOT/'.tools/cargo'), RUSTUP_HOME=str(ROOT/'.tools/rustup'),
               CARGO_TARGET_DIR=str(ROOT/'.tools/target'), RCAM_GUI_EVIDENCE=str(out/'gui'))
    commands = [
        ['cargo', 'fmt', '--all', '--', '--check'],
        ['cargo', 'check', '--workspace', '--all-targets', '--locked'],
        ['cargo', 'clippy', '--workspace', '--all-targets', '--locked', '--', '-D', 'warnings'],
        ['cargo', 'test', '--workspace', '--locked'],
        ['cargo', 'test', '--locked', '-p', 'editor-service', '--test', 's1b2b_transform_workflow'],
        ['cargo', 'test', '--locked', '-p', 'editor-service', '--test', 'automation_contract'],
        ['cargo', 'test', '--locked', '-p', 'editor-service', '--test', 'headless_workflow'],
        ['cargo', 'tree', '--locked', '-p', 'editor-service', '-e', 'normal'],
        ['cargo', 'build', '--release', '--locked', '-p', 'editor-app'],
        ['cargo', 'test', '--release', '--locked', '-p', 'editor-app',
         'native_metal_reference_production_pixel_parity', '--', '--ignored', '--nocapture'],
    ]
    results = []
    for number, command in enumerate(commands):
        log = out/f'{number:02d}.log'
        with log.open('w') as stream:
            result = subprocess.run(command, cwd=ROOT, env=env,
                                    stdout=stream, stderr=subprocess.STDOUT)
        results.append(dict(command=command, exit_code=result.returncode, log=log.name))
        print(number, result.returncode, ' '.join(command), flush=True)
    binary = ROOT/'.tools/target/release/editor-app'
    identity = dict(
        schema_version=2,
        stage='S2-C2',
        platform=platform.platform(),
        base_commit=subprocess.check_output(
            ['git', 'rev-parse', 'HEAD'], cwd=ROOT, text=True).strip(),
        worktree_status=subprocess.check_output(
            ['git', 'status', '--short'], cwd=ROOT, text=True).splitlines(),
        binary_sha256=hashlib.sha256(binary.read_bytes()).hexdigest(),
        commands=results,
        windows='deferred / not executed',
    )
    (out/'gates.json').write_text(json.dumps(identity, indent=2)+'\n')
    raise SystemExit(any(result['exit_code'] for result in results))


if __name__ == '__main__':
    main()
