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
            first, second = root/'a.zip', root/'b.zip'
            package_source.package(first)
            package_source.package(second)
            self.assertEqual(first.read_bytes(), second.read_bytes())
            with zipfile.ZipFile(first) as archive:
                names = archive.namelist()
                self.assertTrue(any('中文 #' in name for name in names))
                for entry in archive.infolist():
                    if not entry.filename.isascii():
                        self.assertTrue(entry.flag_bits & 0x800, entry.filename)
                    self.assertFalse(set(Path(entry.filename).parts) & {'.git', '.tools', 'target', 'private'})
                archive.extractall(root/'unpacked')
            subprocess.run([sys.executable, 'scripts/source_manifest.py', '--check'],
                           cwd=root/'unpacked', check=True)
            for line in (root/'unpacked/PACKAGE_MANIFEST.sha256').read_text(encoding='utf-8').splitlines():
                digest, name = line.split('  ', 1)
                self.assertEqual(hashlib.sha256((root/'unpacked'/name).read_bytes()).hexdigest(), digest)


if __name__ == '__main__':
    unittest.main()
