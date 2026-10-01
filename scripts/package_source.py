"""Deterministic UTF-8 source ZIP, restricted to the reviewed source manifest."""
import argparse
import hashlib
import json
import math
import os
import platform
import re
import subprocess
import stat
from pathlib import Path
import zipfile
import source_manifest


def write_deterministic_zip(destination, entries):
    # Exclusive creation protects an existing delivery. Fixed metadata/order;
    # ZipInfo encodes Unicode filenames and sets bit 11 for non-ASCII names.
    with zipfile.ZipFile(destination, 'x', compression=zipfile.ZIP_DEFLATED) as archive:
        for name, data in sorted(entries):
            info = zipfile.ZipInfo(name, date_time=(1980, 1, 1, 0, 0, 0))
            info.create_system = 3
            info.external_attr = 0o100644 << 16
            info.compress_type = zipfile.ZIP_DEFLATED
            archive.writestr(info, data, compresslevel=9)


def checked_manifest(root, name):
    """Match Rust manifest framing and bind every entry to raw file bytes."""
    entries = {}
    raw_manifest = (root/name).read_bytes().decode('utf-8')
    lines = raw_manifest.split('\n')
    if lines[-1] == '':
        lines.pop()
    terminated_lines = raw_manifest.count('\n')
    for index, line in enumerate(lines):
        # Rust str::lines uses LF/CRLF only, not Python's Unicode separators.
        if index < terminated_lines and line.endswith('\r'):
            line = line[:-1]
        digest, name = line.split('  ', 1)
        path = root/name
        if (not re.fullmatch('[0-9a-f]{64}', digest) or name in entries
                or not name or any(c in name for c in '\\\r\n')
                or any(part in ('', '.', '..') for part in name.split('/'))
                or Path(name).is_absolute() or path.is_symlink()
                or any((root/Path(*Path(name).parts[:i])).is_symlink()
                       for i in range(1, len(Path(name).parts)+1))
                or not path.resolve().is_relative_to(root.resolve())
                or hashlib.sha256(path.read_bytes()).hexdigest() != digest):
            raise ValueError('archive package manifest mismatch')
        entries[name] = digest
    if not entries:
        raise ValueError('empty source/package manifest')
    return entries


def archive_identity(root):
    """Repacking may inherit identity only after validating the original binding."""
    info = json.loads((root/'PACKAGE_INFO.json').read_text(encoding='utf-8'))
    # bool is an int subclass and floats compare equal to ints in Python.
    if (type(info) is not dict
            or type(info.get('schema_version')) is not int
            or info['schema_version'] != 1
            or info.get('clean_worktree') is not True
            or info.get('supplemental_only') is not False
            or type(info.get('git_commit')) is not str
            or not re.fullmatch('[0-9a-f]{40}', info['git_commit'])
            or type(info.get('commit')) is not str
            or info['commit'] != info['git_commit']):
        raise ValueError('invalid/non-pristine archive identity')
    # serde_json 1.0.151 has a 128 recursion budget (127 containers) and uses
    # finite f64 for integers beyond i64/u64. Match those JSON input boundaries.
    pending = [(info, 1)]
    while pending:
        value, depth = pending.pop()
        if type(value) in (dict, list):
            if depth >= 128:
                raise ValueError('package JSON nesting limit exceeded')
            children = value.values() if type(value) is dict else value
            pending.extend((child, depth+1) for child in children)
        elif type(value) is int and not -(1 << 63) <= value < (1 << 64):
            try:
                finite = math.isfinite(float(value))
            except OverflowError:
                finite = False
            if not finite:
                raise ValueError('package JSON number out of range')
    # Reject non-JSON NaN/Infinity and lone surrogates before repacking.
    json.dumps(info, ensure_ascii=False, allow_nan=False).encode('utf-8')
    entries = checked_manifest(root, 'PACKAGE_MANIFEST.sha256')
    source = checked_manifest(root, 'MANIFEST.sha256')
    raw_source_manifest = (root/'MANIFEST.sha256').read_bytes()
    paths = sorted(p.relative_to(root).as_posix() for p in source_manifest.source_files())
    included = sorted(paths+['MANIFEST.sha256'])
    if (sorted(source) != paths
            or any(entries.get(name) != digest for name, digest in source.items())
            or info.get('source_manifest_sha256') != hashlib.sha256(raw_source_manifest).hexdigest()
            or info.get('included_paths') != included
            or type(info.get('source_file_count')) is not int
            or info.get('source_file_count') != len(paths)
            or type(info.get('manifest_count')) is not int
            or info.get('manifest_count') != len(included)
            or sorted(entries) != sorted(included+['PACKAGE_INFO.json'])):
        raise ValueError('archive source coverage/binding mismatch')
    return info


def package(destination, *, stage='unspecified', commit='unavailable'):
    root = source_manifest.ROOT
    manifest = source_manifest.contents()
    expected = {p.relative_to(root).as_posix(): hashlib.sha256(p.read_bytes()).hexdigest()
                for p in source_manifest.source_files()}
    if checked_manifest(root, 'MANIFEST.sha256') != expected:
        raise ValueError('source manifest mismatch; review and regenerate first')
    entries = [(p.relative_to(root).as_posix(), p.read_bytes())
               for p in source_manifest.source_files()]
    entries.append(('MANIFEST.sha256', manifest.encode('utf-8')))
    try:
        marker = (root/'.git').lstat()
    except FileNotFoundError:
        marker = None
    if marker is not None and stat.S_ISLNK(marker.st_mode):
        raise ValueError('Git marker symlink rejected')
    if marker is not None:
        git_env = {key: value for key, value in os.environ.items()
                   if not key.upper().startswith('GIT_')}
        def git(*args):
            return subprocess.check_output(['git', *args], cwd=root, env=git_env, text=True)
        top = git('rev-parse', '--show-toplevel').strip()
        if Path(top).resolve() != root.resolve():
            raise ValueError('Git root differs from source workspace')
        actual = git('rev-parse', 'HEAD').strip()
        status = git('status', '--porcelain=v1', '--untracked-files=all')
        clean = not status.strip()
    else:
        actual = archive_identity(root)['git_commit']
        clean = True
    if not re.fullmatch('[0-9a-f]{40}', actual) or commit not in ('unavailable', actual):
        raise ValueError('requested package commit does not match verified source identity')
    commit = actual
    rust = next(line.split("=", 1)[1].strip().strip('"') for line in (root/"rust-toolchain.toml").read_text().splitlines() if line.startswith("channel ="))
    package_info = dict(
        schema_version=1,
        stage=stage,
        git_commit=commit,
        commit=commit,
        clean_worktree=clean,
        platform=platform.system()+"-"+platform.machine(),
        rust_version=rust,
        source_zip_sha256=None,
        source_zip_sha256_note="External sidecar and public evidence; embedding a ZIP own digest is circular",
        included_paths=[name for name,_ in sorted(entries)],
        excluded_paths=[".git", "target", ".tools", "fixtures/private", "fonts", "evidence", "evidence-public", "exports"],
        manifest_count=len(entries),
        source_manifest_sha256=hashlib.sha256(manifest.encode('utf-8')).hexdigest(),
        source_file_count=len(source_manifest.source_files()),
        supplemental_only=False,
    )
    entries.append(('PACKAGE_INFO.json',
                    (json.dumps(package_info, indent=2, sort_keys=True)+'\n').encode('utf-8')))
    package_manifest = ''.join(f'{hashlib.sha256(data).hexdigest()}  {name}\n'
                               for name, data in sorted(entries))
    entries.append(('PACKAGE_MANIFEST.sha256', package_manifest.encode('utf-8')))
    write_deterministic_zip(destination, entries)


if __name__ == '__main__':
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('destination', type=Path)
    parser.add_argument('--stage', default='unspecified')
    parser.add_argument('--commit', default='unavailable')
    args = parser.parse_args()
    package(args.destination, stage=args.stage, commit=args.commit)
