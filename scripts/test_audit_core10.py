"""Public constructed cases for the private-input audit; no real sample data."""
import unittest
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
        for source in ['%AMX*1,1,1,0,0*', 'G01', '%MOMM%']:
            with self.assertRaises(ValueError):
                list(tokenize(source))

    def test_identity_is_numeric_and_nonidentity_remains_visible(self):
        for command in ['OFA0.00000B0.00000', 'SFA1.00000B1.00000', 'IR0.0', 'IPPOS', 'MIA0B0']:
            self.assertTrue(identity_candidate(command), command)
        for command in ['OFA1B0', 'SFA2B1', 'IPNEG', 'IOA1B2', 'ICAS']:
            self.assertFalse(identity_candidate(command), command)

if __name__ == '__main__':
    unittest.main()
