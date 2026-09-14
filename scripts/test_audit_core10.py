"""Public constructed cases for the private-input audit; no real sample data."""
import unittest
import hashlib
import json
from pathlib import Path
from audit_core10 import audit, identity_candidate, tokenize

class AuditTests(unittest.TestCase):
    def test_every_command_and_modal_use(self):
        source = '%FSLAX26Y26*MOMM*%%AMUNUSED*1,1,1,0,0*%%AMUSED*21,1,$1,1,0,0,0*%%ADD010USED,1*ADD11C,1*%D10*X0Y0D03*D03*G36*G01X1Y1D01*G37*G74*G02X1Y1I1J0D01*M02*'
        report = audit(source)
        self.assertFalse(report['unknown_commands'])
        self.assertEqual(report['apertures']['10']['imaging_uses'], 3)
        self.assertEqual(report['region_draw_operations'], 1)
        self.assertEqual(report['unused_macros'], ['UNUSED'])
        self.assertEqual(report['used_macros'], ['USED'])
        self.assertEqual(report['unreferenced_apertures'], ['11'])
        self.assertEqual(report['arc_operations_by_quadrant'], {'G74': 1})
        self.assertFalse(report['undefined_imaging_apertures'])
        self.assertEqual(len(report['commands']), len(list(tokenize(source))))

    def test_unknown_and_malformed_are_not_ignored(self):
        self.assertEqual(len(audit('%ZZ1*%M02*')['unknown_commands']), 1)
        for source in ['%AMX*1,1,1,0,0*', '%AMX*1,1,1,0,0%', 'G01', '%MOMM%']:
            with self.assertRaises(ValueError):
                list(tokenize(source))

    def test_identity_is_numeric_and_nonidentity_remains_visible(self):
        for command in ['OFA0.00000B0.00000', 'SFA1.00000B1.00000', 'IR0.0', 'IPPOS', 'MIA0B0']:
            self.assertTrue(identity_candidate(command), command)
        for command in ['OFA1B0', 'SFA2B1', 'IPNEG', 'IOA1B2', 'ICAS']:
            self.assertFalse(identity_candidate(command), command)

    def test_frozen_public_scope_fixtures(self):
        root = Path(__file__).resolve().parents[1]
        manifest = json.loads((root/'fixtures/synthetic/s0c/manifest.json').read_text())
        for item in manifest['fixtures']:
            with self.subTest(path=item['path']):
                data = (root/item['path']).read_bytes()
                self.assertEqual(hashlib.sha256(data).hexdigest(), item['sha256'])
                report = audit(data.decode('ascii'))
                self.assertFalse(report['unknown_commands'])
                self.assertFalse(report['usage_issues'])
                for field, value in item['expected_audit'].items():
                    self.assertEqual(report[field], value)

    def test_coordinate_modes_and_legacy_are_not_identity_shortcuts(self):
        report = audit('%FSDIX34Y34*%%MOMM*%%ICAS*%%IOA2B3*%%ADD10C,1*%G91*D10*X0010000Y0010000D03*X0010000D03*G90*X0010000D03*M02*')
        self.assertEqual(report['imaging_by_coordinate_mode'], {'I': 2, 'A': 1})
        self.assertEqual(report['explicit_format_width_counts'], {'full': 4})
        self.assertEqual(report['compatibility_commands'][0]['ascii_declaration'], True)
        self.assertEqual(report['compatibility_commands'][1]['nonzero'], True)
        self.assertEqual(report['compatibility_commands'][1]['semantic_status'], 'blocked_independent_offset_validation')
        for source in ['%ICXX*%', '%IOA?B0*%', '%FSQAX26Y26*%']:
            self.assertTrue(audit(source)['unknown_commands'])

    def test_later_definitions_cannot_repair_prior_use(self):
        for source in ['D10*X0Y0D03*%ADD10C,1*%', '%ADD10LATE*%%AMLATE*1,1,1,0,0*%', '%ADD10C,1*%%ADD10C,2*%']:
            self.assertTrue(audit(source)['usage_issues'])

    def test_macro_expression_dependency_and_sr_nonidentity(self):
        report = audit('%AMX*$1=$1X2*21,1,$1,1,0,0,0*%%ADD10X,1*%%SRX2Y1I2J0*%D10*X0Y0D03*')
        self.assertEqual(report['macros']['X']['variables'], ['$1'])
        self.assertEqual(report['macros']['X']['assignments'], 1)
        self.assertEqual(report['macros']['X']['primitives'], ['21'])
        self.assertFalse(report['compatibility_commands'][0]['identity_candidate'])

if __name__ == '__main__':
    unittest.main()
