import unittest
from release_common import compare_versions, msi_version, build_manifest
class Releases(unittest.TestCase):
    def test_semver_order(self):
        self.assertLess(compare_versions('0.2.0-beta.2', '0.2.0-beta.10'), 0)
        self.assertLess(compare_versions('0.2.0-beta.10', '0.2.0-rc.1'), 0)
        self.assertLess(compare_versions('0.2.0-rc.1', '0.2.0'), 0)
        self.assertGreater(compare_versions('0.3.0-beta.1', '0.2.9'), 0)
    def test_msi_beta_rc_stable_and_next_patch_are_monotonic(self):
        versions=['0.2.0-beta.1','0.2.0-beta.2','0.2.0-rc.1','0.2.0','0.2.1-beta.1']
        values=[tuple(map(int,msi_version(v).split('.'))) for v in versions]
        self.assertEqual(values, sorted(set(values)))
        self.assertEqual(msi_version(versions[0]),'0.2.1')
    def test_msi_rejects_unrepresentable_versions(self):
        for version in ['0.2.0-beta.128','256.0.0','0.2.256','0.2.0-dev.1']:
            with self.assertRaises(ValueError):msi_version(version)
    def test_manifest_requires_all_platforms_and_signatures(self):
        with self.assertRaises(ValueError):build_manifest('0.2.0-beta.1','miofelix/claude-office-gateway',[],'','2026-10-27T00:00:00Z')
    def test_manifest_preserves_installer_variants(self):
        records=[{'version':'0.2.0-beta.1','platform':platform,'updates':updates} for platform,updates in [
            ('macos-aarch64',{'darwin-aarch64':{'file':'arm.app.tar.gz','signature':'signed-arm'}}),
            ('macos-x86_64',{'darwin-x86_64':{'file':'intel.app.tar.gz','signature':'signed-intel'}}),
            ('windows-x86_64',{key:{'file':file,'signature':'signed-win'} for key,file in [('windows-x86_64','setup.exe'),('windows-x86_64-nsis','setup.exe'),('windows-x86_64-msi','setup.msi')]})]]
        result=build_manifest('0.2.0-beta.1','miofelix/claude-office-gateway',records,'notes','2026-10-27T00:00:00Z')
        self.assertTrue(result['platforms']['windows-x86_64-msi']['url'].endswith('/setup.msi'))
        records[0]['updates']['darwin-aarch64']['signature']=''
        with self.assertRaises(ValueError):build_manifest('0.2.0-beta.1','miofelix/claude-office-gateway',records,'','2026-10-27T00:00:00Z')
if __name__=='__main__':unittest.main()
