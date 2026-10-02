"""Hash the explicit distributable source set, never private samples/build caches."""
import argparse
import hashlib
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
ROOT_FILES = ['.gitignore', 'AGENTS.md', 'Cargo.toml', 'Cargo.lock', 'rust-toolchain.toml',
              'README.md', 'THIRD_PARTY_NOTICES.md', 'RCam_S5M1_P100K_NATIVE_CLOSEOUT_NEXT_TASK.md']
DIRECTORIES = ['crates', 'docs', '.github', 'scripts', 'fixtures/synthetic']
SUFFIXES = {'.csv', '.tsv', '.rs', '.wgsl', '.toml', '.md', '.json', '.py', '.yml', '.yaml', '.gbr', '.sha256', '.txt', '.log', '.png', '.jpg', '.gbx', '.rcam'}


def source_files():
    files = [ROOT / name for name in ROOT_FILES]
    files += [path for folder in DIRECTORIES for path in (ROOT/folder).rglob('*')
              if path.is_file() and path.suffix in SUFFIXES and '__pycache__' not in path.parts]
    if any(path.is_symlink() or not path.resolve().is_relative_to(ROOT) for path in files):
        raise ValueError('source manifest refuses symlinks or paths outside workspace')
    return sorted(files, key=lambda path: path.relative_to(ROOT).as_posix())


def contents():
    return ''.join(f'{hashlib.sha256(path.read_bytes()).hexdigest()}  {path.relative_to(ROOT).as_posix()}\n'
                   for path in source_files())


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--check', action='store_true')
    args = parser.parse_args()
    manifest = ROOT/'MANIFEST.sha256'
    expected = contents()
    if args.check:
        if manifest.read_text() != expected:
            raise SystemExit('source manifest mismatch: regenerate after reviewing source changes')
        print(f'PASS current source manifest: {len(source_files())} files')
    else:
        manifest.write_text(expected)
        print(f'Wrote current source manifest: {len(source_files())} files')

if __name__ == '__main__':
    main()
