"""S4-C4 checks with immutable before/after source hashes and raw command logs."""
import argparse
import hashlib
import json
import os
from pathlib import Path
import platform
import subprocess
import time
import source_manifest

ROOT = Path(__file__).resolve().parents[1]

def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--out', type=Path, required=True)
    parser.add_argument('--allow-dirty', action='store_true', help='Development only; cannot establish clean package acceptance')
    args = parser.parse_args()
    out = args.out.resolve()
    out.mkdir(parents=True, exist_ok=False)
    def read(command):
        return subprocess.run(command, cwd=ROOT, text=True, stdout=subprocess.PIPE, stderr=subprocess.STDOUT).stdout.strip()
    status = read(['git', 'status', '--porcelain=v1', '--untracked-files=all'])
    (out/'clean-before.txt').write_text(status+'\n')
    if status and not args.allow_dirty:
        raise SystemExit('A clean source commit is required; use --allow-dirty only for development evidence')
    (out/'HEAD').write_text(read(['git','rev-parse','HEAD'])+'\n')
    identity = source_manifest.contents()
    (out/'tested-source-before.sha256').write_text(identity)
    (out/'environment.json').write_text(json.dumps(dict(schema_version=2,stage='S4-C4',platform=platform.platform(),machine=platform.machine(),windows='deferred / not executed',clean=not bool(status)),indent=2)+'\n')
    env = dict(os.environ)
    env.update(PATH=str((ROOT/'.tools/cargo/bin').resolve())+os.pathsep+env['PATH'],CARGO_HOME=str((ROOT/'.tools/cargo').resolve()),RUSTUP_HOME=str((ROOT/'.tools/rustup').resolve()),CARGO_TARGET_DIR=str((ROOT/'.tools/target').resolve()),PYTHONPYCACHEPREFIX='/tmp/rcam-c4-pycache')
    commands = [
        ['cargo','fmt','--all','--','--check'],
        ['cargo','check','--workspace','--all-targets','--locked'],
        ['cargo','clippy','--workspace','--all-targets','--locked','--','-D','warnings'],
        ['cargo','test','--locked','-p','editor-service','--test','alignment_workflow','--test','alignment_diagnostics','--','--nocapture'],
        ['cargo','test','--workspace','--locked','--no-fail-fast'],
        ['cargo','test','--locked','-p','editor-service','--test','automation_contract'],
        ['cargo','test','--locked','-p','editor-service','--test','headless_workflow'],
        ['cargo','tree','--locked','-p','editor-service','-e','normal'],
        ['cargo','build','--release','--locked','-p','editor-app'],
        ['cargo','test','--release','--locked','-p','editor-service','--test','alignment_workflow','performance','--','--ignored','--nocapture'],
        ['cargo','test','--release','--locked','-p','editor-core','--lib','alignment::performance_tests::perf_1k_alignment_reports_bounds_deltas_transaction_and_total','--','--ignored','--exact','--nocapture'],
        ['cargo','test','--release','--locked','-p','editor-app','native_metal_c3_create_invariance','--','--ignored','--nocapture','--test-threads=1'],
        ['cargo','test','--release','--locked','-p','editor-app','native_metal_block_instance_parity','--','--ignored','--nocapture','--test-threads=1'],
        ['python3','scripts/source_manifest.py','--check'],
        ['python3','scripts/test_package_source.py'],
    ]
    gates=[]
    for i, command in enumerate(commands):
        started=time.monotonic()
        with (out/f'{i:02d}.log').open('w') as stream:
            result=subprocess.run(command,cwd=ROOT,env=env,stdout=stream,stderr=subprocess.STDOUT)
        gates.append(dict(command=command,exit_code=result.returncode,duration_seconds=time.monotonic()-started,log=f'{i:02d}.log'))
        (out/'gates.json').write_text(json.dumps(gates,indent=2)+'\n')
        print(i,result.returncode,' '.join(command),flush=True)
        if result.returncode:
            break
    (out/'tested-source-after.sha256').write_text(source_manifest.contents())
    (out/'clean-after.txt').write_text(read(['git','status','--porcelain=v1','--untracked-files=all'])+'\n')
    unchanged=identity==source_manifest.contents()
    binary=ROOT/'.tools/target/release/editor-app'
    (out/'summary.json').write_text(json.dumps(dict(gates_passed=sum(g['exit_code']==0 for g in gates),gates_expected=len(commands),source_unchanged=unchanged,release_sha256=hashlib.sha256(binary.read_bytes()).hexdigest() if binary.exists() else None),indent=2)+'\n')
    raise SystemExit(0 if unchanged and len(gates)==len(commands) and all(g['exit_code']==0 for g in gates) else 1)

if __name__ == '__main__':
    main()
