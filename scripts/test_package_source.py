"""Real Python ZIP extraction, Unicode identity and source manifest regression."""
import hashlib
from pathlib import Path
import subprocess
import sys
import tempfile
import unittest
import zipfile
import package_source


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
            package_source.package(first, stage='test', commit='test-commit')
            package_source.package(second, stage='test', commit='test-commit')
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


if __name__ == '__main__':
    unittest.main()
