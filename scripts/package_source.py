"""Deterministic UTF-8 source ZIP, restricted to the reviewed source manifest."""
import argparse
import hashlib
from pathlib import Path
import zipfile
import source_manifest


def package(destination):
    root = source_manifest.ROOT
    manifest = source_manifest.contents()
    if (root / 'MANIFEST.sha256').read_text(encoding='utf-8') != manifest:
        raise ValueError('source manifest mismatch; review and regenerate first')
    entries = [(p.relative_to(root).as_posix(), p.read_bytes())
               for p in source_manifest.source_files()]
    entries.append(('MANIFEST.sha256', manifest.encode('utf-8')))
    package_manifest = ''.join(f'{hashlib.sha256(data).hexdigest()}  {name}\n'
                               for name, data in sorted(entries))
    entries.append(('PACKAGE_MANIFEST.sha256', package_manifest.encode('utf-8')))
    # Exclusive creation protects an existing delivery. Fixed metadata/order;
    # ZipInfo encodes Unicode filenames and sets bit 11 for non-ASCII names.
    with zipfile.ZipFile(destination, 'x', compression=zipfile.ZIP_DEFLATED) as archive:
        for name, data in sorted(entries):
            info = zipfile.ZipInfo(name, date_time=(1980, 1, 1, 0, 0, 0))
            info.create_system = 3
            info.external_attr = 0o100644 << 16
            info.compress_type = zipfile.ZIP_DEFLATED
            archive.writestr(info, data, compresslevel=9)


if __name__ == '__main__':
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('destination', type=Path)
    package(parser.parse_args().destination)
