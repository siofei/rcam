"""Bind a clean candidate commit to exact tracked bytes before/after final gates."""
import argparse
import hashlib
import json
from pathlib import Path
import subprocess

ROOT = Path(__file__).resolve().parents[1]

def git(*args):
    return subprocess.check_output(['git', *args], cwd=ROOT)

def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--out', type=Path, required=True)
    args = parser.parse_args()
    if git('status', '--porcelain').strip():
        raise SystemExit('Refusing final identity: working tree is not clean')
    commit = git('rev-parse', 'HEAD').decode().strip()
    hashes = {}
    for name in git('ls-files', '-z').decode().split('\0'):
        if not name:
            continue
        path = ROOT / name
        if path.is_symlink():
            raise SystemExit(f'Unexpected source symlink: {name}')
        data = path.read_bytes()
        if data != git('show', f'{commit}:{name}'):
            raise SystemExit(f'Commit/source mismatch: {name}')
        hashes[name] = hashlib.sha256(data).hexdigest()
    if commit != git('rev-parse', 'HEAD').decode().strip() or git('status', '--porcelain').strip():
        raise SystemExit('Source changed during identity capture')
    args.out.parent.mkdir(parents=True, exist_ok=True)
    with args.out.open('x') as stream:
        json.dump({'tested_code_commit': commit, 'git_status_porcelain': '', 'sha256': hashes}, stream, indent=2)
        stream.write('\n')
    print(f'PASS {commit}: {len(hashes)} tracked files match commit')

if __name__ == '__main__':
    main()
