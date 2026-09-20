"""Run S2-C1 local gates with immutable per-run logs; native surface run is separate."""
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
        ['cargo', 'test', '--locked', '-p', 'editor-service', '--test', 'automation_contract'],
        ['cargo', 'test', '--locked', '-p', 'editor-service', '--test', 'headless_workflow'],
        ['cargo', 'tree', '--locked', '-p', 'editor-service', '-e', 'normal'],
        ['cargo', 'build', '--release', '--locked', '-p', 'editor-app'],
        ['cargo', 'test', '--release', '--locked', '-p', 'editor-app', 'native_metal_reference_production_pixel_parity', '--', '--ignored', '--nocapture'],
    ]
    results = []
    for n, cmd in enumerate(commands):
        log = out/f'{n:02d}.log'
        with log.open('w') as f:
            result = subprocess.run(cmd, cwd=ROOT, env=env, stdout=f, stderr=subprocess.STDOUT)
        results.append(dict(command=cmd, exit_code=result.returncode, log=log.name))
        print(n, result.returncode, ' '.join(cmd), flush=True)
    binary = ROOT/'.tools/target/release/editor-app'
    identity = dict(schema_version=2, platform=platform.platform(),
                    base_commit=subprocess.check_output(['git','rev-parse','HEAD'],cwd=ROOT,text=True).strip(),
                    binary_sha256=hashlib.sha256(binary.read_bytes()).hexdigest(),commands=results,
                    windows='deferred / not executed')
    (out/'gates.json').write_text(json.dumps(identity, indent=2)+'\n')
    raise SystemExit(any(r['exit_code'] for r in results))


if __name__ == '__main__':
    main()
