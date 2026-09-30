"""S4-D1 checks with immutable before/after source hashes and raw command logs."""
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
    (out/'environment.json').write_text(json.dumps(dict(schema_version=2,stage='S4-D1',platform=platform.platform(),machine=platform.machine(),windows='deferred / not executed',clean=not bool(status)),indent=2)+'\n')
    env = dict(os.environ)
    env.update(PATH=str((ROOT/'.tools/cargo/bin').resolve())+os.pathsep+env['PATH'],CARGO_HOME=str((ROOT/'.tools/cargo').resolve()),RUSTUP_HOME=str((ROOT/'.tools/rustup').resolve()),CARGO_TARGET_DIR=str((ROOT/'.tools/target').resolve()),PYTHONPYCACHEPREFIX='/tmp/rcam-d1-pycache',RCAM_S4D1_EVIDENCE_DIR=str(out/'pnp-artifacts'))
    commands = [
        ['cargo','fmt','--all','--','--check'],
        ['cargo','check','--workspace','--all-targets','--locked'],
        ['cargo','clippy','--workspace','--all-targets','--locked','--','-D','warnings'],
        ['cargo','clippy','--locked','-p','editor-app','--all-targets','--features','internal-evidence','--','-D','warnings'],
        ['cargo','test','--locked','-p','editor-core','--test','pnp_foundation','-p','editor-service','--test','pnp_workflow','--test','pnp_diagnostics','-p','rcam-project','--test','pnp_project','--','--nocapture'],
        ['cargo','test','--locked','-p','editor-app','components_ui','--','--nocapture'],
        ['cargo','test','--workspace','--locked','--no-fail-fast'],
        ['cargo','test','--locked','-p','editor-service','--test','automation_contract','--test','headless_workflow'],
        ['cargo','tree','--locked','-p','editor-service','-e','normal'],
        ['cargo','build','--release','--locked','-p','editor-app'],
        ['python3','scripts/measure_s4d1_performance.py'],
        ['cargo','test','--release','--locked','-p','editor-app','native_metal_c3_create_invariance','--','--ignored','--nocapture','--test-threads=1'],
        ['cargo','test','--release','--locked','-p','editor-app','native_metal_block_instance_parity','--','--ignored','--nocapture','--test-threads=1'],
        ['cargo','build','--release','--locked','-p','editor-app','--features','internal-evidence'],
        ['python3','scripts/source_manifest.py','--check'],
        ['python3','scripts/test_package_source.py'],
    ]
    gates=[]
    for i, command in enumerate(commands):
        started=time.monotonic()
        with (out/f'{i:02d}.log').open('w') as stream:
            result=subprocess.run(command,cwd=ROOT,env=env,stdout=stream,stderr=subprocess.STDOUT)
        if result.returncode == 0 and command[:2] == ['cargo', 'tree']:
            tree=(out/f'{i:02d}.log').read_text()
            forbidden=['egui','eframe','wgpu','winit','raw-window-handle']
            violations=[name for name in forbidden if any(line.lstrip(' │├─└').startswith(name+' v') for line in tree.splitlines())]
            (out/'service-boundary.json').write_text(json.dumps(dict(status='FAIL' if violations else 'PASS', forbidden_dependencies=violations),indent=2)+'\n')
            if violations: result.returncode=1
        if result.returncode==0 and command[:4]==['cargo','build','--release','--locked']:
            import shutil
            (out/'bin').mkdir(exist_ok=True)
            shutil.copy2(ROOT/'.tools/target/release/editor-app',out/'bin'/('editor-app-internal-evidence' if '--features' in command else 'editor-app-public'))
            if '--features' not in command:
                binary_data=(out/'bin/editor-app-public').read_bytes()
                markers=[b'RCAM_S4D1_NATIVE_DIR',b'interaction.request']
                leaks=[m.decode() for m in markers if m in binary_data]
                (out/'public-driver-boundary.json').write_text(json.dumps(dict(status='FAIL' if leaks else 'PASS',active_driver_markers=leaks,feature='default'),indent=2)+'\n')
                if leaks: result.returncode=1
        gates.append(dict(command=command,exit_code=result.returncode,duration_seconds=time.monotonic()-started,log=f'{i:02d}.log'))
        (out/'gates.json').write_text(json.dumps(gates,indent=2)+'\n')
        print(i,result.returncode,' '.join(command),flush=True)
        if result.returncode:
            break
    (out/'tested-source-after.sha256').write_text(source_manifest.contents())
    (out/'clean-after.txt').write_text(read(['git','status','--porcelain=v1','--untracked-files=all'])+'\n')
    unchanged=identity==source_manifest.contents()
    binary=ROOT/'.tools/target/release/editor-app'
    binary_hashes={p.name:hashlib.sha256(p.read_bytes()).hexdigest() for p in (out/'bin').glob('*')} if (out/'bin').exists() else {}
    (out/'summary.json').write_text(json.dumps(dict(gates_passed=sum(g['exit_code']==0 for g in gates),gates_expected=len(commands),source_unchanged=unchanged,binary_hashes=binary_hashes,release_sha256=hashlib.sha256(binary.read_bytes()).hexdigest() if binary.exists() else None),indent=2)+'\n')
    raise SystemExit(0 if unchanged and len(gates)==len(commands) and all(g['exit_code']==0 for g in gates) else 1)

if __name__ == '__main__':
    main()
