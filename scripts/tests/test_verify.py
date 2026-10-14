from pathlib import Path
import json
import os
import shutil
import subprocess
import sys
import time
import tempfile
import unittest

ROOT = Path(__file__).resolve().parents[2]
GIT = shutil.which('git')


class VerificationCLI(unittest.TestCase):
    def fixture(self, fmt='echo broken-check; exit 42'):
        directory = tempfile.TemporaryDirectory()
        self.addCleanup(directory.cleanup)
        base = Path(directory.name)
        checkout, tools = base / 'checkout', base / 'tools'
        (checkout / 'scripts/tests').mkdir(parents=True)
        tools.mkdir()
        for name in ['verify.py', 'verification_evidence.py', 'check-quality.py', 'clippy-baseline.json']:
            source = ROOT / 'scripts' / name
            if source.exists():
                shutil.copy(source, checkout / 'scripts' / name)
        (checkout / '.gitignore').write_text('/target/\n__pycache__/\n')
        (checkout / 'source.txt').write_text('fixture source\n')
        (checkout / 'scripts/tests/test_fixture.py').write_text(
            'from pathlib import Path\nimport unittest\nclass Fixture(unittest.TestCase):\n'
            '    def test_fixture(self):\n        self.assertEqual(Path("source.txt").read_text(), "fixture source\\n")\n')
        agent = checkout / 'scripts/agent.sh'
        agent.write_text('#!/bin/sh\nexit 0\n')
        agent.chmod(0o755)
        metadata = json.dumps(dict(packages=[dict(id='core', name='signaltty-core')],
                                   workspace_members=['core'], resolve=dict(nodes=[dict(id='core', deps=[])])))
        cargo = ('case "$1" in\n'
                 'clippy) if [ "$2" = --version ]; then echo clippy; else '
                 'echo \'{"reason":"build-finished","success":true}\'; fi;;\n'
                 f'metadata) echo \'{metadata}\';;\n'
                 f'fmt) {fmt};;\n'
                 '*) exit 0;;\nesac')
        for name, body in {
            'cargo': cargo, 'rustc': 'echo "rustc 1.94.0 (fixture)"',
            'rustfmt': 'exit 0', 'git': f'exec "{GIT}" "$@"',
            'pkg-config': 'echo 99.0', 'cc': 'exit 0', 'node': 'exit 0',
            'xvfb-run': 'exit 0', 'Xvfb': 'exit 0', 'xauth': 'exit 0',
            'dbus-run-session': 'exit 0',
        }.items():
            tool = tools / name
            tool.write_text('#!/bin/sh\n' + body + '\n')
            tool.chmod(0o755)
        for args in [('init', '-q'), ('config', 'user.name', 'Fixture'),
                     ('config', 'user.email', 'fixture@example.invalid'), ('add', '.'),
                     ('commit', '-qm', 'fixture')]:
            subprocess.run([GIT, *args], cwd=checkout, check=True, capture_output=True)
        env = dict(os.environ, PATH=str(tools), SIGNALTTY_VERIFY_TEST_NESTED='1')
        return checkout, env

    def test_failed_check_is_recorded_and_stops_the_run(self):
        if os.environ.get('SIGNALTTY_VERIFY_TEST_NESTED'):
            self.skipTest('outer CLI test already exercises this case')
        checkout, env = self.fixture()
        output = checkout / 'target/evidence'
        result = subprocess.run([sys.executable, str(checkout / 'scripts/verify.py'), 'fast', '--output', str(output)],
                                env=env, capture_output=True, text=True)
        self.assertNotEqual(result.returncode, 0)
        report = json.loads((output / 'summary.json').read_text())
        self.assertEqual(report['status'], 'failed')
        self.assertEqual(report['steps'][-1]['name'], 'fmt')
        self.assertEqual(report['steps'][-1]['exit_code'], 42)
        self.assertIn('broken-check', (output / 'fmt.log').read_text())
        self.assertEqual(report['provenance']['unchanged'], True)
        self.assertEqual(report['failure']['category'], 'command')
        self.assertIn('fmt.log', report['failure']['recovery'])

    def test_success_records_source_provenance_and_repository_check(self):
        checkout, env = self.fixture(fmt='exit 0')
        output = checkout / 'target/evidence'
        result = subprocess.run([sys.executable, str(checkout / 'scripts/verify.py'), 'fast', '--output', str(output)],
                                env=env, capture_output=True, text=True)
        self.assertEqual(result.returncode, 0, result.stderr)
        report = json.loads((output / 'summary.json').read_text())
        self.assertEqual(report['status'], 'passed')
        self.assertEqual(report['provenance']['before']['root'], str(checkout))
        self.assertEqual(report['provenance']['unchanged'], True)
        self.assertTrue({'tooling-tests', 'repository', 'fmt', 'metadata'} <=
                        {step['name'] for step in report['steps']})

    def test_cold_cache_is_ready_before_offline_tooling_checks(self):
        checkout, env = self.fixture(fmt='exit 0')
        cache = checkout.parent / 'cargo-cache-ready'
        env['SIGNALTTY_TEST_CACHE'] = str(cache)
        cargo = Path(env['PATH']) / 'cargo'
        cargo.write_text(cargo.read_text().replace('metadata)',
                         'metadata) : > "$SIGNALTTY_TEST_CACHE";'))
        (checkout / 'scripts/tests/test_fixture.py').write_text(
            'import os\nfrom pathlib import Path\nimport unittest\n'
            'class OfflinePackaging(unittest.TestCase):\n'
            '    def test_dependencies_are_available(self):\n'
            '        self.assertTrue(Path(os.environ["SIGNALTTY_TEST_CACHE"]).exists())\n')
        output = checkout / 'target/evidence'
        result = subprocess.run([sys.executable, str(checkout / 'scripts/verify.py'),
                                 'fast', '--output', str(output)], env=env,
                                capture_output=True, text=True)
        self.assertEqual(result.returncode, 0, result.stderr)

    def test_source_mutation_during_successful_checks_fails_proof(self):
        checkout, env = self.fixture(fmt='echo mutation >> source.txt; exit 0')
        output = checkout / 'target/evidence'
        result = subprocess.run([sys.executable, str(checkout / 'scripts/verify.py'), 'fast', '--output', str(output)],
                                env=env, capture_output=True, text=True)
        self.assertNotEqual(result.returncode, 0)
        report = json.loads((output / 'summary.json').read_text())
        self.assertEqual(report['status'], 'failed')
        self.assertEqual(report['provenance']['unchanged'], False)
        self.assertEqual(report['failure']['category'], 'stale')
        self.assertIn('changed', '\n'.join(report['errors']))

    def test_termination_stops_the_active_child_and_records_failure(self):
        if os.environ.get('SIGNALTTY_VERIFY_TEST_NESTED'):
            self.skipTest('outer CLI test already exercises cancellation')
        checkout, env = self.fixture(fmt='echo $$ > "$SIGNALTTY_TEST_PID"; exec /bin/sleep 60')
        output, pidfile = checkout / 'target/evidence', checkout.parent / 'child.pid'
        env['SIGNALTTY_TEST_PID'] = str(pidfile)
        logfile = checkout.parent / 'runner.log'
        with logfile.open('w') as log:
            process = subprocess.Popen([sys.executable, str(checkout / 'scripts/verify.py'), 'fast', '--output', str(output)],
                                       env=env, stdout=log, stderr=log)
            try:
                deadline = time.monotonic() + 15
                while not pidfile.exists() and process.poll() is None and time.monotonic() < deadline:
                    time.sleep(0.05)
                self.assertTrue(pidfile.exists(), logfile.read_text())
                child_pid = int(pidfile.read_text())
                process.terminate()
                self.assertNotEqual(process.wait(timeout=10), 0)
                with self.assertRaises(ProcessLookupError):
                    os.kill(child_pid, 0)
                report = json.loads((output / 'summary.json').read_text())
                self.assertEqual(report['status'], 'failed')
                self.assertEqual(report['steps'][-1]['status'], 'failed')
                self.assertEqual(report['failure']['category'], 'interruption')
            finally:
                if process.poll() is None:
                    process.kill()
                    process.wait()

    def test_missing_tools_fail_and_leave_a_machine_readable_report(self):
        with tempfile.TemporaryDirectory() as directory:
            output = Path(directory) / 'evidence'
            result = subprocess.run([sys.executable, str(ROOT / 'scripts/verify.py'), 'doctor', '--output', str(output)],
                                    env=dict(os.environ, PATH=directory), capture_output=True, text=True)
            self.assertNotEqual(result.returncode, 0)
            report = json.loads((output / 'summary.json').read_text())
            self.assertEqual(report['status'], 'failed')
            self.assertIn('cargo', '\n'.join(report['errors']))
            self.assertEqual(report['steps'], [])
            self.assertEqual(report['failure']['category'], 'environment')

    def test_existing_evidence_is_never_overwritten(self):
        with tempfile.TemporaryDirectory() as directory:
            marker = Path(directory) / 'summary.json'
            marker.write_text('original evidence')
            result = subprocess.run([sys.executable, str(ROOT / 'scripts/verify.py'), 'doctor', '--output', directory],
                                    capture_output=True, text=True)
            self.assertNotEqual(result.returncode, 0)
            self.assertEqual(marker.read_text(), 'original evidence')

    def test_nonignored_output_is_rejected_without_creating_directory(self):
        checkout, env = self.fixture(fmt='exit 0')
        output = checkout / 'source-evidence'
        result = subprocess.run([sys.executable, str(checkout / 'scripts/verify.py'), 'fast', '--output', str(output)],
                                env=env, capture_output=True, text=True)
        self.assertNotEqual(result.returncode, 0)
        self.assertIn('ignored', result.stderr)
        self.assertFalse(output.exists())

    def test_doctor_can_diagnose_without_git_provenance_but_checks_cannot_pass(self):
        checkout, env = self.fixture(fmt='exit 0')
        (Path(env['PATH']) / 'git').write_text('#!/bin/sh\necho unavailable-git >&2\nexit 1\n')
        for mode, expected in [('doctor', 'passed'), ('fast', 'failed')]:
            with self.subTest(mode=mode):
                output = checkout.parent / f'{mode}-evidence'
                result = subprocess.run([sys.executable, str(checkout / 'scripts/verify.py'), mode, '--output', str(output)],
                                        env=env, capture_output=True, text=True)
                report = json.loads((output / 'summary.json').read_text())
                self.assertEqual(report['status'], expected, result.stderr)
                self.assertIsNone(report['provenance']['before'])
                self.assertFalse(report['provenance']['unchanged'])
                self.assertIn('unavailable-git', report['provenance']['error'])
                if mode == 'fast':
                    self.assertNotEqual(result.returncode, 0)
                    self.assertEqual(report['failure']['category'], 'provenance')
                    self.assertEqual(report['steps'], [])
                else:
                    self.assertEqual(result.returncode, 0, result.stderr)


if __name__ == '__main__':
    unittest.main()
