"""Exercise update-feed safety boundaries without contacting GitHub."""
import importlib.util
from pathlib import Path
import unittest
from unittest.mock import patch
spec = importlib.util.spec_from_file_location('publisher', Path(__file__).with_name('publish-release.py'))
publisher = importlib.util.module_from_spec(spec)
spec.loader.exec_module(publisher)

class UpdateFeeds(unittest.TestCase):
    def setUp(self):
        self.manifest = {'version': '0.2.0-beta.1', 'platforms': {}}
        self.current = {'object': {'sha': 'old'}}
    def test_identical_release_can_resume_without_rewriting_feed(self):
        with patch.object(publisher, 'api', return_value=self.current) as api, patch.object(publisher, 'existing_feed', return_value=self.manifest):
            publisher.advance_feeds('example/repo', self.manifest)
            self.assertEqual(api.call_count, 1)
    def test_older_release_cannot_downgrade_feed(self):
        with patch.object(publisher, 'api', return_value=self.current) as api, patch.object(publisher, 'existing_feed', return_value={'version': '0.2.0-beta.2'}):
            publisher.advance_feeds('example/repo', self.manifest)
            self.assertEqual(api.call_count, 1)
    def test_published_version_is_immutable(self):
        with patch.object(publisher, 'api', return_value=self.current), patch.object(publisher, 'existing_feed', return_value={**self.manifest, 'platforms': {'changed': True}}):
            with self.assertRaises(RuntimeError):
                publisher.advance_feeds('example/repo', self.manifest)
    def test_initial_beta_creates_orphan_branch_and_only_beta_feed(self):
        with patch.object(publisher, 'api', side_effect=[None, {'sha': 'tree'}, {'sha': 'commit'}, {}]) as api:
            publisher.advance_feeds('example/repo', self.manifest)
            calls = api.call_args_list
            self.assertEqual([item['path'] for item in calls[1].args[1]['tree']], ['beta.json'])
            self.assertEqual(calls[2].args[1]['parents'], [])
            self.assertEqual(calls[3].args[1]['ref'], 'refs/heads/updates')
    def test_stable_advances_both_channels_without_forcing_branch(self):
        with patch.object(publisher, 'api', side_effect=[self.current, {'tree': {'sha': 'base'}}, {'sha': 'tree'}, {'sha': 'commit'}, {}]) as api, patch.object(publisher, 'existing_feed', return_value=self.manifest):
            publisher.advance_feeds('example/repo', {'version': '0.2.0', 'platforms': {}})
            calls = api.call_args_list
            self.assertEqual(calls[-1].kwargs['method'], 'PATCH')
            self.assertFalse(calls[-1].args[1]['force'])
            self.assertEqual(calls[2].args[1]['base_tree'], 'base')
            self.assertEqual({item['path'] for item in calls[2].args[1]['tree']}, {'beta.json', 'stable.json'})

if __name__ == '__main__':
    unittest.main()
