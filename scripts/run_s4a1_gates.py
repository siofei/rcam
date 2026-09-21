"""Run immutable text-stage macOS gates from a clean commit."""
import argparse
import hashlib
import json
import os
from pathlib import Path
import platform
import subprocess


ROOT = Path(__file__).resolve().parents[1]


def output(command):
    return subprocess.run(command, cwd=ROOT, text=True, stdout=subprocess.PIPE,
                          stderr=subprocess.STDOUT, check=False).stdout.strip()


def sha256(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--out', type=Path, required=True)
    parser.add_argument('--stage', default='S4-A1')
    parser.add_argument('--extra-service-test', action='append', default=[])
    args = parser.parse_args()
    out = args.out.resolve()
    out.mkdir(parents=True, exist_ok=False)
    status_before = output(['git', 'status', '--porcelain=v1', '--untracked-files=all'])
    (out/'clean-status-before.txt').write_text(status_before+'\n', encoding='utf-8')
    if status_before:
        raise SystemExit('text gates require a clean commit')
    commit = output(['git', 'rev-parse', 'HEAD'])
    (out/'git-head.txt').write_text(commit+'\n', encoding='utf-8')
    manifest = ROOT/'MANIFEST.sha256'
    (out/'tested-source-hashes.txt').write_bytes(manifest.read_bytes())
    environment = dict(
        schema_version=2,
        stage=args.stage,
        platform=platform.platform(),
        machine=platform.machine(),
        macos=platform.mac_ver()[0],
        python=platform.python_version(),
        hardware=output(['/usr/sbin/sysctl', '-n', 'machdep.cpu.brand_string']),
        rust=output(['env', 'CARGO_HOME='+str(ROOT/'.tools/cargo'), 'RUSTUP_HOME='+str(ROOT/'.tools/rustup'), str(ROOT/'.tools/cargo/bin/rustc'), '--version']),
        windows='deferred / not executed',
    )
    (out/'environment.json').write_text(
        json.dumps(environment, indent=2, sort_keys=True)+'\n', encoding='utf-8')
    env = os.environ.copy()
    env.update(
        PATH=str(ROOT/'.tools/cargo/bin')+os.pathsep+env['PATH'],
        CARGO_HOME=str(ROOT/'.tools/cargo'),
        RUSTUP_HOME=str(ROOT/'.tools/rustup'),
        CARGO_TARGET_DIR=str(ROOT/'.tools/target'),
        PYTHONPYCACHEPREFIX='/tmp/rcam-s4a1-pycache',
        RCAM_GUI_EVIDENCE=str(out/'gui'),
        RCAM_TEXT_EVIDENCE=str(out/'text'),
    )
    commands = [
        ['cargo', 'fmt', '--all', '--', '--check'],
        ['cargo', 'check', '--workspace', '--all-targets', '--locked'],
        ['cargo', 'clippy', '--workspace', '--all-targets', '--locked', '--', '-D', 'warnings'],
        ['cargo', 'test', '--workspace', '--locked'],
        ['cargo', 'test', '--locked', '-p', 'editor-service', '--test', 'automation_contract'],
        ['cargo', 'test', '--locked', '-p', 'editor-service', '--test', 'headless_workflow'],
        ['cargo', 'test', '--locked', '-p', 'editor-service', '--test', 's1b_edit_workflow'],
        ['cargo', 'test', '--locked', '-p', 'editor-service', '--test', 's1b2_edit_workflow'],
        ['cargo', 'test', '--locked', '-p', 'editor-service', '--test', 's1b2b_transform_workflow'],
        ['cargo', 'test', '--locked', '-p', 'editor-service', '--test', 's1b2c_workspace_workflow'],
        ['cargo', 'test', '--locked', '-p', 'editor-service', '--test', 's2b_select_rect_workflow'],
        ['cargo', 'test', '--locked', '-p', 'editor-service', '--test', 'metrics_workflow'],
        ['cargo', 'test', '--locked', '-p', 'editor-service', '--test', 's3_edit_closeout'],
        ['cargo', 'test', '--locked', '-p', 'editor-service', '--test', 's4a1_text_workflow'],
        ['cargo', 'tree', '--locked', '-p', 'editor-service', '-e', 'normal'],
        ['cargo', 'build', '--release', '--locked', '-p', 'editor-app'],
        ['python3', 'scripts/source_manifest.py', '--check'],
        ['python3', 'scripts/test_audit_core10.py'],
        ['python3', 'scripts/test_package_source.py'],
        ['python3', 'scripts/test_package_release.py'],
        ['cargo', 'test', '--release', '--locked', '-p', 'editor-app',
         'native_metal_reference_production_pixel_parity', '--', '--ignored', '--nocapture'],
    ]
    commands[14:14] = [['cargo', 'test', '--locked', '-p', 'editor-service', '--test', target, '--', '--nocapture'] for target in args.extra_service_test]
    results = []
    for number, command in enumerate(commands):
        log = out/f'{number:02d}.log'
        with log.open('w', encoding='utf-8') as stream:
            result = subprocess.run(command, cwd=ROOT, env=env,
                                    stdout=stream, stderr=subprocess.STDOUT)
        results.append(dict(command=command, exit_code=result.returncode, log=log.name))
        # Retain diagnostic text, redact only the private checkout path.
        log.write_text(log.read_text(encoding='utf-8').replace(str(ROOT), '<workspace>'), encoding='utf-8')
        print(number, result.returncode, ' '.join(command), flush=True)
    binary = ROOT/'.tools/target/release/editor-app'
    if binary.is_file():
        (out/'binary-sha256.txt').write_text(
            f'{sha256(binary)}  {binary.name}\n', encoding='utf-8')
    status_after = output(['git', 'status', '--porcelain=v1', '--untracked-files=all'])
    (out/'clean-status-after.txt').write_text(status_after+'\n', encoding='utf-8')
    summary = dict(
        schema_version=2,
        stage=args.stage,
        base_commit=commit,
        source_manifest_sha256=sha256(manifest),
        clean_before=not bool(status_before),
        clean_after=not bool(status_after),
        commands=results,
        status='PASS' if not status_after and all(r['exit_code'] == 0 for r in results)
        else 'FAIL',
        windows='deferred / not executed',
    )
    (out/'gates.json').write_text(
        json.dumps(summary, indent=2, sort_keys=True)+'\n', encoding='utf-8')
    raise SystemExit(summary['status'] != 'PASS')


if __name__ == '__main__':
    main()
