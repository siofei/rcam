"""Build the fixed two-ZIP delivery and its SHA256SUMS.txt sidecar.

The sidecar is created only after BOTH final ZIP files exist, is regenerated on
every run (a stale sidecar from an earlier stage is never reused), and lists
exactly the two ZIPs of this delivery:

    <sha256>  RCam_<stage>_<shortsha>_source.zip
    <sha256>  RCam_<stage>_<shortsha>_public_evidence.zip
"""
import argparse
import hashlib
import os
import re
from pathlib import Path

SUMS_NAME = 'SHA256SUMS.txt'
_SOURCE = re.compile(r'^RCam_(?P<stage>[A-Za-z0-9]+)_(?P<sha>[0-9a-f]{7,40})_source\.zip$')
_EVIDENCE = re.compile(r'^RCam_(?P<stage>[A-Za-z0-9]+)_(?P<sha>[0-9a-f]{7,40})_public_evidence\.zip$')


def release_names(stage, short_sha):
    """Return the two required ZIP basenames for one delivery."""
    names = (f'RCam_{stage}_{short_sha}_source.zip',
             f'RCam_{stage}_{short_sha}_public_evidence.zip')
    if not _SOURCE.match(names[0]) or not _EVIDENCE.match(names[1]):
        raise ValueError(f'invalid stage/short sha for release names: {stage!r} {short_sha!r}')
    return names


def _digest(path):
    hasher = hashlib.sha256()
    with open(path, 'rb') as handle:
        for block in iter(lambda: handle.read(1 << 20), b''):
            hasher.update(block)
    return hasher.hexdigest()


def write_sums(out_dir, source_zip, evidence_zip):
    """Create SHA256SUMS.txt for exactly the two final ZIPs.

    Both ZIPs must already exist in ``out_dir``. Any previous sidecar is deleted
    first so a failed run can never leave an older stage's checksums behind.
    """
    out_dir = Path(out_dir)
    source_zip, evidence_zip = Path(source_zip), Path(evidence_zip)
    for archive in (source_zip, evidence_zip):
        if archive.parent.resolve() != out_dir.resolve() or not archive.is_file():
            raise ValueError(f'final ZIP must exist inside {out_dir}: {archive}')
    source_match = _SOURCE.match(source_zip.name)
    evidence_match = _EVIDENCE.match(evidence_zip.name)
    if not source_match or not evidence_match:
        raise ValueError('ZIP names do not follow RCam_<stage>_<shortsha>_{source,public_evidence}.zip')
    if source_match.groupdict() != evidence_match.groupdict():
        raise ValueError('source and evidence ZIP belong to different stage/commit identities')
    sums = out_dir / SUMS_NAME
    if sums.exists():
        sums.unlink()
    text = ''.join(f'{_digest(archive)}  {archive.name}\n'
                   for archive in sorted((source_zip, evidence_zip), key=lambda p: p.name))
    temporary = out_dir / (SUMS_NAME + '.tmp')
    with open(temporary, 'w', encoding='utf-8', newline='\n') as handle:
        handle.write(text)
    os.replace(temporary, sums)
    return sums


def verify_sums(out_dir):
    """Check names and hashes of SHA256SUMS.txt against the actual ZIP bytes."""
    out_dir = Path(out_dir)
    lines = (out_dir / SUMS_NAME).read_text(encoding='utf-8').splitlines()
    entries = {}
    for line in lines:
        digest, separator, name = line.partition('  ')
        if not separator or not re.fullmatch(r'[0-9a-f]{64}', digest) or '/' in name:
            raise ValueError(f'malformed checksum line: {line!r}')
        if name in entries:
            raise ValueError(f'duplicate checksum entry: {name}')
        entries[name] = digest
    zips = sorted(path.name for path in out_dir.glob('*.zip'))
    if sorted(entries) != zips:
        raise ValueError(f'sidecar names {sorted(entries)} != actual ZIPs {zips}')
    sources = [name for name in zips if _SOURCE.match(name)]
    evidence = [name for name in zips if _EVIDENCE.match(name)]
    if len(sources) != 1 or len(evidence) != 1:
        raise ValueError('delivery must contain exactly one source and one public evidence ZIP')
    if _SOURCE.match(sources[0]).groupdict() != _EVIDENCE.match(evidence[0]).groupdict():
        raise ValueError('sidecar references two different stages or commits')
    for name, digest in entries.items():
        if _digest(out_dir / name) != digest:
            raise ValueError(f'hash mismatch for {name}')
    return entries


def build(out_dir, stage, short_sha, evidence_dir, includes=(), commit='unavailable'):
    """Package source + evidence ZIPs, then create the sidecar last."""
    import package_public_evidence
    import package_source
    out_dir = Path(out_dir)
    out_dir.mkdir(parents=True, exist_ok=True)
    source_name, evidence_name = release_names(stage, short_sha)
    source_zip, evidence_zip = out_dir / source_name, out_dir / evidence_name
    stale = out_dir / SUMS_NAME
    if stale.exists():
        stale.unlink()
    package_source.package(source_zip, stage=stage, commit=commit)
    package_public_evidence.package(Path(evidence_dir), evidence_zip, list(includes), source_zip)
    sums = write_sums(out_dir, source_zip, evidence_zip)
    verify_sums(out_dir)
    return source_zip, evidence_zip, sums


if __name__ == '__main__':
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('out_dir', type=Path)
    parser.add_argument('--stage', default='S4B1')
    parser.add_argument('--short-sha', required=True)
    parser.add_argument('--commit', default='unavailable')
    parser.add_argument('--evidence-dir', type=Path, required=True)
    parser.add_argument('--include', action='append', type=Path, default=[])
    args = parser.parse_args()
    for produced in build(args.out_dir, args.stage, args.short_sha, args.evidence_dir,
                          args.include, args.commit):
        print(produced)
