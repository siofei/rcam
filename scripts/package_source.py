"""Deterministic UTF-8 source ZIP, restricted to the reviewed source manifest."""
import argparse
import hashlib
import json
import platform
import subprocess
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


def package(destination, *, stage='unspecified', commit='unavailable'):
    root = source_manifest.ROOT
    manifest = source_manifest.contents()
    if (root / 'MANIFEST.sha256').read_text(encoding='utf-8') != manifest:
        raise ValueError('source manifest mismatch; review and regenerate first')
    entries = [(p.relative_to(root).as_posix(), p.read_bytes())
               for p in source_manifest.source_files()]
    entries.append(('MANIFEST.sha256', manifest.encode('utf-8')))
    status = subprocess.run(["git", "status", "--porcelain=v1", "--untracked-files=all"], cwd=root, capture_output=True, text=True)
    # Extracted source packages have no Git directory; inherit attested metadata.
    inherited = json.loads((root/"PACKAGE_INFO.json").read_text()) if (root/"PACKAGE_INFO.json").exists() else {}
    clean = not status.stdout.strip() if status.returncode == 0 else inherited.get("clean_worktree")
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
