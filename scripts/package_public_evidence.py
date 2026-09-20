"""Create a deterministic public S3 evidence ZIP with a complete hash list."""
import argparse
import hashlib
from pathlib import Path
import zipfile


FIXED_TIME = (1980, 1, 1, 0, 0, 0)
BANNED_PARTS = {'.git', '.tools', 'target', 'private'}


def sha256(data):
    return hashlib.sha256(data).hexdigest()


def package(source, destination, includes, source_zip):
    source = source.resolve()
    destination = destination.resolve()
    if destination.is_relative_to(source):
        raise ValueError('evidence archive must be outside its input directory')
    files = sorted((path for path in source.rglob('*') if path.is_file()
                    and path.name != 'EVIDENCE.sha256'),
                   key=lambda path: path.relative_to(source).as_posix())
    if any(path.is_symlink() or set(path.relative_to(source).parts) & BANNED_PARTS
           for path in files):
        raise ValueError('public evidence refuses symlinks or private/build paths')
    entries = [(path.relative_to(source).as_posix(), path.read_bytes()) for path in files]
    for path in includes:
        resolved = path.resolve()
        if resolved.is_symlink() or not resolved.is_file():
            raise ValueError(f'invalid include: {path}')
        entries.append((resolved.name, resolved.read_bytes()))
    source_zip_bytes = source_zip.resolve().read_bytes()
    source_hash = f'{sha256(source_zip_bytes)}  {source_zip.name}\n'.encode('utf-8')
    entries.append(('source-zip-sha256.txt', source_hash))
    if len({name for name, _ in entries}) != len(entries):
        raise ValueError('duplicate evidence entry name')
    evidence_manifest = ''.join(
        f'{sha256(data)}  {name}\n' for name, data in sorted(entries)
    ).encode('utf-8')
    entries.append(('EVIDENCE.sha256', evidence_manifest))
    (source/'EVIDENCE.sha256').write_bytes(evidence_manifest)
    with zipfile.ZipFile(destination, 'x', compression=zipfile.ZIP_DEFLATED) as archive:
        for name, data in sorted(entries):
            info = zipfile.ZipInfo(name, date_time=FIXED_TIME)
            info.create_system = 3
            info.external_attr = 0o100644 << 16
            info.compress_type = zipfile.ZIP_DEFLATED
            archive.writestr(info, data, compresslevel=9)


if __name__ == '__main__':
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('source', type=Path)
    parser.add_argument('destination', type=Path)
    parser.add_argument('--include', action='append', type=Path, default=[])
    parser.add_argument('--source-zip', type=Path, required=True)
    args = parser.parse_args()
    package(args.source, args.destination, args.include, args.source_zip)
