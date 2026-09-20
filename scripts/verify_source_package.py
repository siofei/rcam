"""Fresh-extract a source ZIP and verify its manifest, tests and payload identity."""
import argparse
import hashlib
import json
from pathlib import Path
import subprocess
import sys
import tempfile
import zipfile


def digest(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('archive', type=Path)
    parser.add_argument('--reference-manifest', type=Path, required=True)
    parser.add_argument('--report', type=Path)
    args = parser.parse_args()
    archive = args.archive.resolve()
    reference = args.reference_manifest.resolve().read_bytes()
    with tempfile.TemporaryDirectory(prefix='rcam-s3-source-') as tmp:
        extracted = Path(tmp)/'source'
        with zipfile.ZipFile(archive) as package:
            package.extractall(extracted)
        unpacked_manifest = (extracted/'MANIFEST.sha256').read_bytes()
        if unpacked_manifest != reference:
            raise SystemExit('FAIL extracted tracked payload manifest differs from tested source')
        commands = [
            [sys.executable, 'scripts/source_manifest.py', '--check'],
            [sys.executable, 'scripts/test_package_source.py'],
        ]
        results = []
        for command in commands:
            result = subprocess.run(command, cwd=extracted, text=True,
                                    stdout=subprocess.PIPE, stderr=subprocess.STDOUT)
            results.append(dict(command=command, exit_code=result.returncode,
                                output=result.stdout))
            if result.returncode:
                print(result.stdout, end='')
                raise SystemExit(result.returncode)
        report = dict(
            schema_version=1,
            status='PASS',
            archive=str(archive),
            archive_sha256=digest(archive),
            manifest_sha256=hashlib.sha256(reference).hexdigest(),
            payload_manifest_equal=True,
            commands=results,
        )
    output = json.dumps(report, indent=2, sort_keys=True)+'\n'
    if args.report:
        args.report.write_text(output, encoding='utf-8')
    print(output, end='')


if __name__ == '__main__':
    main()
