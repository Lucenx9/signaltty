from pathlib import Path
import json
import subprocess
import sys
import tempfile
import unittest

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))
from verification_evidence import assess_evidence, capture_checkout


class CheckoutEvidence(unittest.TestCase):
    def setUp(self):
        self.directory = tempfile.TemporaryDirectory()
        self.addCleanup(self.directory.cleanup)
        self.root = Path(self.directory.name)
        self.git('init', '-q')
        self.git('config', 'user.name', 'Fixture')
        self.git('config', 'user.email', 'fixture@example.invalid')
        (self.root / '.gitignore').write_text('/target/\n')
        (self.root / 'source.txt').write_text('original\n')
        self.git('add', '.')
        self.git('commit', '-qm', 'fixture')

    def git(self, *args):
        return subprocess.check_output(['git', *args], cwd=self.root, stderr=subprocess.STDOUT)

    def summary(self, **updates):
        snapshot = capture_checkout(self.root)
        names = ['tooling-tests', 'repository', 'fmt', 'metadata', 'architecture', 'clippy',
                 'warning-policy', 'build', 'workspace-tests', 'server-qa', 'gui-build',
                 'gui-list-0', 'gtk-01', 'refresh-benchmark']
        report = dict(schema_version=1, mode='full', status='passed', errors=[], display_tests=1,
                      provenance=dict(before=snapshot, after=snapshot, unchanged=True),
                      steps=[dict(name=name, status='passed', exit_code=0) for name in names])
        report.update(updates)
        directory = self.root / 'target/verification/full-fixture'
        directory.mkdir(parents=True, exist_ok=True)
        path = directory / 'summary.json'
        path.write_text(json.dumps(report))
        return path

    def test_full_evidence_is_fresh_only_for_the_same_inputs(self):
        path = self.summary()
        self.assertEqual(assess_evidence(self.root, path.parent)['status'], 'fresh')
        (self.root / 'source.txt').write_text('different source\n')
        result = assess_evidence(self.root, path)
        self.assertEqual(result['status'], 'stale')
        self.assertIn('verify.sh full', result['recovery'])

    def test_successful_summary_cannot_replace_completed_full_checks(self):
        cases = [dict(mode='doctor'), dict(mode='fast'), dict(steps=[]),
                 dict(steps=[dict(name='fmt', status='running')]), dict(provenance={}),
                 dict(schema_version=0), dict(errors=['unresolved failure'])]
        for updates in cases:
            with self.subTest(updates=updates):
                self.assertEqual(assess_evidence(self.root, self.summary(**updates))['status'], 'unverified')

    def test_failed_and_running_evidence_explain_what_to_do(self):
        path = self.summary(status='failed', steps=[dict(name='fmt', status='failed', exit_code=42, log='fmt.log')])
        result = assess_evidence(self.root, path)
        self.assertEqual(result['status'], 'failed')
        self.assertEqual(result['failed_step'], 'fmt')
        self.assertEqual(result['log'], str(path.parent / 'fmt.log'))
        self.assertIn('Read', result['recovery'])
        path = self.summary(status='running')
        self.assertEqual(assess_evidence(self.root, path)['status'], 'running')

    def test_changed_during_run_other_checkout_and_malformed_provenance_are_not_fresh(self):
        snapshot = capture_checkout(self.root)
        path = self.summary(provenance=dict(before=snapshot, after=snapshot, unchanged=False))
        self.assertEqual(assess_evidence(self.root, path)['status'], 'stale')
        wrong_root = dict(snapshot, root='/another/checkout')
        path = self.summary(provenance=dict(before=wrong_root, after=wrong_root, unchanged=True))
        self.assertEqual(assess_evidence(self.root, path)['status'], 'stale')
        malformed = dict(snapshot, content_digest='')
        path = self.summary(provenance=dict(before=malformed, after=malformed, unchanged=True))
        self.assertEqual(assess_evidence(self.root, path)['status'], 'unverified')
        path.write_text('{broken JSON')
        self.assertEqual(assess_evidence(self.root, path)['status'], 'unverified')
        path = self.summary(mode={'malformed': True})
        self.assertEqual(assess_evidence(self.root, path)['status'], 'unverified')
        path.unlink()
        self.assertEqual(assess_evidence(self.root, path)['status'], 'unavailable')

    def test_snapshot_detects_working_and_staged_content_and_preserves_commit_proof(self):
        before = capture_checkout(self.root)
        self.assertEqual(before['schema_version'], 1)
        self.assertEqual(before['files'], 2)
        (self.root / 'source.txt').write_text('new content\n')
        changed = capture_checkout(self.root)
        self.assertNotEqual(changed['content_digest'], before['content_digest'])
        self.assertEqual(changed['index_digest'], before['index_digest'])
        self.git('add', 'source.txt')
        staged = capture_checkout(self.root)
        self.assertNotEqual(staged['index_digest'], changed['index_digest'])
        self.git('commit', '-qm', 'unchanged files')
        committed = capture_checkout(self.root)
        self.assertNotEqual(staged['revision'], committed['revision'])
        self.assertEqual(staged['content_digest'], committed['content_digest'])
        self.assertEqual(staged['index_digest'], committed['index_digest'])

    def test_new_files_modes_deleted_files_and_symlink_targets_are_inputs(self):
        before = capture_checkout(self.root)
        (self.root / 'target').mkdir()
        (self.root / 'target/result.json').write_text('ignored evidence')
        self.assertEqual(capture_checkout(self.root), before)
        new = self.root / 'new file\nwith newline'
        new.write_bytes(b'new input\0')
        added = capture_checkout(self.root)
        self.assertNotEqual(added['content_digest'], before['content_digest'])
        self.assertEqual(added['files'], 3)
        new.chmod(0o755)
        executable = capture_checkout(self.root)
        self.assertNotEqual(executable['content_digest'], added['content_digest'])
        new.unlink()
        (self.root / 'source.txt').unlink()
        self.assertNotEqual(capture_checkout(self.root)['content_digest'], before['content_digest'])
        (self.root / 'source.txt').symlink_to('target/missing-one')
        linked = capture_checkout(self.root)
        (self.root / 'source.txt').unlink()
        (self.root / 'source.txt').symlink_to('target/missing-two')
        self.assertNotEqual(capture_checkout(self.root)['content_digest'], linked['content_digest'])

    def test_index_only_change_is_detected(self):
        before = capture_checkout(self.root)
        self.git('update-index', '--chmod=+x', 'source.txt')
        after = capture_checkout(self.root)
        self.assertEqual(after['content_digest'], before['content_digest'])
        self.assertNotEqual(after['index_digest'], before['index_digest'])

    def test_index_referencing_missing_blob_cannot_produce_proof(self):
        self.git('update-index', '--cacheinfo', '100644,' + '1' * 40 + ',source.txt')
        with self.assertRaisesRegex(ValueError, 'staged'):
            capture_checkout(self.root)

    def test_evidence_output_cannot_hide_repository_inputs(self):
        capture_checkout(self.root, self.root / 'target/verification/new')
        capture_checkout(self.root, self.root.parent / 'outside-evidence')
        with self.assertRaisesRegex(ValueError, 'ignored'):
            capture_checkout(self.root, self.root / 'source-evidence')

    def test_checkout_identification_errors_are_explicit(self):
        with tempfile.TemporaryDirectory() as directory:
            with self.assertRaisesRegex(ValueError, 'checkout'):
                capture_checkout(Path(directory))

    def test_incomplete_or_duplicate_gtk_steps_cannot_establish_delivery_proof(self):
        path = self.summary(display_tests=2)
        self.assertEqual(assess_evidence(self.root, path)['status'], 'unverified')
        path = self.summary(display_tests=0)
        self.assertEqual(assess_evidence(self.root, path)['status'], 'unverified')
        path = self.summary()
        report = json.loads(path.read_text())
        report['steps'].append(dict(name='gtk-01', status='passed', exit_code=0))
        path.write_text(json.dumps(report))
        self.assertEqual(assess_evidence(self.root, path)['status'], 'unverified')


if __name__ == '__main__':
    unittest.main()
