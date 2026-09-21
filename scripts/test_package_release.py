"""Gate 0.2: SHA256SUMS sidecar must describe exactly the current two ZIPs."""
import hashlib
from pathlib import Path
import tempfile
import unittest
import zipfile

import package_release


def make_zip(path, payload):
    with zipfile.ZipFile(path, 'x') as archive:
        archive.writestr('payload.txt', payload)


class ReleaseSidecarTest(unittest.TestCase):
    def setUp(self):
        self.tmp = tempfile.TemporaryDirectory()
        self.root = Path(self.tmp.name)
        source, evidence = package_release.release_names('S4B1', 'abc1234')
        self.source, self.evidence = self.root / source, self.root / evidence
        make_zip(self.source, 'source bytes')
        make_zip(self.evidence, 'evidence bytes')

    def tearDown(self):
        self.tmp.cleanup()

    def test_filenames_match_actual_zip_basenames(self):
        sums = package_release.write_sums(self.root, self.source, self.evidence)
        names = [line.split('  ', 1)[1] for line in sums.read_text().splitlines()]
        self.assertEqual(names, sorted([self.source.name, self.evidence.name]))
        self.assertEqual(names, sorted(path.name for path in self.root.glob('*.zip')))
        self.assertTrue(all('S4A2' not in name for name in names))

    def test_hashes_match_actual_bytes(self):
        sums = package_release.write_sums(self.root, self.source, self.evidence)
        for line in sums.read_text().splitlines():
            digest, name = line.split('  ', 1)
            self.assertEqual(hashlib.sha256((self.root / name).read_bytes()).hexdigest(), digest)
        self.assertEqual(len(package_release.verify_sums(self.root)), 2)

    def test_previous_stage_sidecar_is_never_reused(self):
        stale = self.root / package_release.SUMS_NAME
        stale.write_text('0' * 64 + '  RCam_S4A22_deadbee_source.zip\n', encoding='utf-8')
        with self.assertRaises(ValueError):
            package_release.verify_sums(self.root)
        package_release.write_sums(self.root, self.source, self.evidence)
        self.assertNotIn('S4A22', stale.read_text())
        package_release.verify_sums(self.root)

    def test_rerun_regenerates_from_current_bytes(self):
        package_release.write_sums(self.root, self.source, self.evidence)
        first = (self.root / package_release.SUMS_NAME).read_text()
        self.evidence.unlink()
        make_zip(self.evidence, 'different evidence')
        with self.assertRaises(ValueError):
            package_release.verify_sums(self.root)
        package_release.write_sums(self.root, self.source, self.evidence)
        self.assertNotEqual(first, (self.root / package_release.SUMS_NAME).read_text())
        package_release.verify_sums(self.root)

    def test_mixed_stage_or_missing_zip_is_rejected(self):
        other = self.root / 'RCam_S4A22_abc1234_public_evidence.zip'
        make_zip(other, 'old evidence')
        with self.assertRaises(ValueError):
            package_release.write_sums(self.root, self.source, other)
        other.unlink()
        with self.assertRaises(ValueError):
            package_release.write_sums(self.root, self.source, self.root / 'missing.zip')


if __name__ == '__main__':
    unittest.main()
