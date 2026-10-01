"""Real Python ZIP extraction, Unicode identity and source manifest regression."""
import hashlib
import json
from pathlib import Path
import subprocess
import sys
import tempfile
import unittest
import zipfile
from unittest.mock import patch
import package_source
import source_manifest


class SourcePackageTest(unittest.TestCase):
    def test_unicode_deterministic_roundtrip(self):
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            entries = [
                ('中文 # test.gbr', b'G04 unicode hash*\nM02*\n'),
                ('空格 文件.gbr', b'G04 spaces*\nM02*\n'),
                ('目录/中文/样本.gbr', b'G04 nested*\nM02*\n'),
            ]
            first, second = root/'unicode-a.zip', root/'unicode-b.zip'
            package_source.write_deterministic_zip(first, entries)
            package_source.write_deterministic_zip(second, entries)
            self.assertEqual(first.read_bytes(), second.read_bytes())
            with zipfile.ZipFile(first) as archive:
                self.assertEqual(archive.namelist(), sorted(name for name, _ in entries))
                for entry in archive.infolist():
                    self.assertTrue(entry.flag_bits & 0x800, entry.filename)
                archive.extractall(root/'unpacked')
            for name, data in entries:
                self.assertEqual((root/'unpacked'/name).read_bytes(), data)

    def test_complete_source_package_extracts_and_verifies(self):
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            first, second = root/'a.zip', root/'b.zip'
            package_source.package(first, stage='test')
            package_source.package(second, stage='test')
            self.assertEqual(first.read_bytes(), second.read_bytes())
            with zipfile.ZipFile(first) as archive:
                names = archive.namelist()
                self.assertIn('PACKAGE_INFO.json', names)
                self.assertIn('PACKAGE_MANIFEST.sha256', names)
                for entry in archive.infolist():
                    self.assertFalse(set(Path(entry.filename).parts) &
                                     {'.git', '.tools', 'target', 'private', 'evidence', 'evidence-public'})
                archive.extractall(root/'unpacked')
            subprocess.run([sys.executable, 'scripts/source_manifest.py', '--check'],
                           cwd=root/'unpacked', check=True)
            for line in (root/'unpacked/PACKAGE_MANIFEST.sha256').read_text(encoding='utf-8').splitlines():
                digest, name = line.split('  ', 1)
                self.assertEqual(hashlib.sha256((root/'unpacked'/name).read_bytes()).hexdigest(), digest)

    def test_wrong_commit_rejected_before_writing(self):
        with tempfile.TemporaryDirectory() as tmp:
            path = Path(tmp)/'wrong.zip'
            with self.assertRaises(ValueError):
                package_source.package(path, commit='a'*40)
            self.assertFalse(path.exists())

    def test_archive_repack_checks_binding_and_modified_payload(self):
        # Synthetic archive, not an attestation of these fixture bytes to Git.
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp).resolve()/'source'
            root.mkdir()
            for name in source_manifest.ROOT_FILES:
                (root/name).write_text('channel = "1.89.0"\n')
            with patch.object(source_manifest, 'ROOT', root):
                manifest = source_manifest.contents()
                (root/'MANIFEST.sha256').write_text(manifest)
                paths = sorted(p.relative_to(root).as_posix() for p in source_manifest.source_files())
                info = dict(schema_version=1, clean_worktree=True, supplemental_only=False,
                            git_commit='8'*40, commit='8'*40,
                            source_file_count=len(paths), manifest_count=len(paths)+1,
                            included_paths=sorted(paths+['MANIFEST.sha256']),
                            source_manifest_sha256=hashlib.sha256(manifest.encode()).hexdigest())
                (root/'PACKAGE_INFO.json').write_text(json.dumps(info))
                def seal():
                    (root/'PACKAGE_MANIFEST.sha256').write_text(''.join(
                        f'{hashlib.sha256((root/name).read_bytes()).hexdigest()}  {name}\n'
                        for name in sorted(info['included_paths']+['PACKAGE_INFO.json'])))
                seal()
                package_source.package(Path(tmp)/'valid.zip', commit='8'*40)
                valid_info = dict(info)
                for field in ['schema_version', 'source_file_count', 'manifest_count']:
                    for value in [True, False, float(valid_info[field]), str(valid_info[field]), None]:
                        with self.subTest(field=field, value=value):
                            info[field] = value
                            (root/'PACKAGE_INFO.json').write_text(json.dumps(info))
                            seal()
                            destination = Path(tmp)/'rejected.zip'
                            with self.assertRaises(ValueError):
                                package_source.package(destination)
                            self.assertFalse(destination.exists())
                            info[field] = valid_info[field]
                (root/'PACKAGE_INFO.json').write_text(json.dumps(valid_info))
                seal()
                package_source.package(Path(tmp)/'valid-restored.zip', commit='8'*40)
                (root/'README.md').write_text('modified')
                with self.assertRaises(ValueError):
                    package_source.package(Path(tmp)/'modified.zip')
                # Updating the source manifest alone cannot refresh old INFO.
                (root/'MANIFEST.sha256').write_text(source_manifest.contents())
                with self.assertRaises(ValueError):
                    package_source.package(Path(tmp)/'stale.zip')


if __name__ == '__main__':
    unittest.main()
