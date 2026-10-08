from pathlib import Path
import json
import os
import subprocess
import sys
import time
import tempfile
import unittest

ROOT = Path(__file__).resolve().parents[2]


class VerificationCLI(unittest.TestCase):
    def test_failed_check_is_recorded_and_stops_the_run(self):
        if os.environ.get("SIGNALTTY_VERIFY_TEST_NESTED"):
            self.skipTest("outer CLI test already exercises this case")
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            for name, body in {
                'cargo': 'case "$1" in clippy) echo clippy;; *) echo broken-check; exit 42;; esac',
                'rustc': 'echo "rustc 1.94.0 (fixture)"',
                'rustfmt': 'exit 0', 'git': 'exit 0',
                'pkg-config': 'echo 99.0', 'cc': 'exit 0', 'node': 'exit 0',
            }.items():
                tool = root / name
                tool.write_text('#!/bin/sh\n' + body + '\n')
                tool.chmod(0o755)
            output = root / 'evidence'
            result = subprocess.run(
                [sys.executable, str(ROOT / 'scripts/verify.py'), 'fast', '--output', str(output)],
                env=dict(os.environ, PATH=directory, SIGNALTTY_VERIFY_TEST_NESTED='1'),
                capture_output=True, text=True)
            self.assertNotEqual(result.returncode, 0)
            report = json.loads((output / 'summary.json').read_text())
            self.assertEqual(report['status'], 'failed')
            self.assertEqual(report['steps'][-1]['name'], 'fmt')
            self.assertEqual(report['steps'][-1]['exit_code'], 42)
            self.assertIn('broken-check', (output / 'fmt.log').read_text())

    def test_termination_stops_the_active_child_and_records_failure(self):
        if os.environ.get('SIGNALTTY_VERIFY_TEST_NESTED'):
            self.skipTest('outer CLI test already exercises cancellation')
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            for name, body in {
                'cargo': 'case "$1" in clippy) echo clippy;; *) echo $$ > "$SIGNALTTY_TEST_PID"; exec /bin/sleep 60;; esac',
                'rustc': 'echo "rustc 1.94.0 (fixture)"',
                'rustfmt': 'exit 0', 'git': 'exit 0', 'pkg-config': 'echo 99.0', 'cc': 'exit 0', 'node': 'exit 0',
            }.items():
                tool = root / name
                tool.write_text('#!/bin/sh\n' + body + '\n')
                tool.chmod(0o755)
            output, pidfile = root / 'evidence', root / 'child.pid'
            with (root / 'runner.log').open('w') as log:
                process = subprocess.Popen(
                    [sys.executable, str(ROOT / 'scripts/verify.py'), 'fast', '--output', str(output)],
                    env=dict(os.environ, PATH=directory, SIGNALTTY_VERIFY_TEST_NESTED='1',
                             SIGNALTTY_TEST_PID=str(pidfile)), stdout=log, stderr=log)
                try:
                    deadline = time.monotonic() + 15
                    while not pidfile.exists() and process.poll() is None and time.monotonic() < deadline:
                        time.sleep(0.05)
                    self.assertTrue(pidfile.exists(), (root / 'runner.log').read_text())
                    child_pid = int(pidfile.read_text())
                    process.terminate()
                    self.assertNotEqual(process.wait(timeout=10), 0)
                    with self.assertRaises(ProcessLookupError):
                        os.kill(child_pid, 0)
                    report = json.loads((output / 'summary.json').read_text())
                    self.assertEqual(report['status'], 'failed')
                    self.assertEqual(report['steps'][-1]['status'], 'failed')
                finally:
                    if process.poll() is None:
                        process.kill()
                        process.wait()

    def test_missing_tools_fail_and_leave_a_machine_readable_report(self):
        with tempfile.TemporaryDirectory() as directory:
            output = Path(directory) / 'evidence'
            result = subprocess.run(
                [sys.executable, str(ROOT / 'scripts/verify.py'), 'doctor',
                 '--output', str(output)], env=dict(os.environ, PATH=directory),
                capture_output=True, text=True)
            self.assertNotEqual(result.returncode, 0)
            report = json.loads((output / 'summary.json').read_text())
            self.assertEqual(report['status'], 'failed')
            self.assertIn('cargo', '\n'.join(report['errors']))
            self.assertEqual(report['steps'], [])

    def test_existing_evidence_is_never_overwritten(self):
        with tempfile.TemporaryDirectory() as directory:
            marker = Path(directory) / 'summary.json'
            marker.write_text('original evidence')
            result = subprocess.run(
                [sys.executable, str(ROOT / 'scripts/verify.py'), 'doctor',
                 '--output', directory], capture_output=True, text=True)
            self.assertNotEqual(result.returncode, 0)
            self.assertEqual(marker.read_text(), 'original evidence')


if __name__ == '__main__':
    unittest.main()
