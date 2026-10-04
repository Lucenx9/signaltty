import json
from pathlib import Path
import subprocess
import sys
import tempfile
import unittest

ROOT = Path(__file__).resolve().parents[2]


class QualityCommands(unittest.TestCase):
    def run_check(self, mode, value):
        with tempfile.TemporaryDirectory() as directory:
            path = Path(directory) / 'input.json'
            path.write_text(json.dumps(value) + '\n')
            return subprocess.run([sys.executable, str(ROOT / 'scripts/check-quality.py'),
                                   mode, str(path)], text=True, capture_output=True)

    def metadata(self, dependency):
        return {
            'workspace_members': ['signaltty-core'],
            'packages': [{'id': 'signaltty-core', 'name': 'signaltty-core',
                          'dependencies': [{'name': dependency}]},
                         {'id': dependency, 'name': dependency, 'dependencies': []}],
            'resolve': {'nodes': [{'id': 'signaltty-core', 'deps': [
                {'name': 'innocent_alias', 'pkg': dependency,
                 'dep_kinds': [{'kind': None}]}]}, {'id': dependency, 'deps': []}]},
        }

    def test_core_accepts_serde_and_rejects_aliased_async_dependency(self):
        self.assertEqual(self.run_check('architecture', self.metadata('serde')).returncode, 0)
        bad = self.run_check('architecture', self.metadata('tokio'))
        self.assertNotEqual(bad.returncode, 0)
        self.assertIn('tokio', bad.stdout + bad.stderr)

    def test_toolkit_cannot_hide_behind_a_dependency(self):
        value = self.metadata('serde')
        value['packages'].append({'id': 'gtk', 'name': 'gtk4', 'dependencies': []})
        value['resolve']['nodes'][1]['deps'] = [
            {'name': 'hidden', 'pkg': 'gtk', 'dep_kinds': [{'kind': None}]}]
        value['resolve']['nodes'].append({'id': 'gtk', 'deps': []})
        result = self.run_check('architecture', value)
        self.assertNotEqual(result.returncode, 0)
        self.assertIn('gtk4', result.stdout + result.stderr)

    def test_new_warning_is_not_hidden_by_existing_lint_exception(self):
        value = {'reason': 'compiler-message', 'message': {
            'level': 'warning', 'code': {'code': 'clippy::suspicious_open_options'},
            'message': 'file opened with `create`, but `truncate` behavior not defined',
            'spans': [{'file_name': 'crates/signaltty-server/src/other.rs',
                       'is_primary': True, 'text': [{'text': '.create(true)'}]}]}}
        result = self.run_check('clippy', value)
        self.assertNotEqual(result.returncode, 0)
        self.assertIn('other.rs', result.stdout + result.stderr)

    def test_non_diagnostic_output_cannot_falsely_pass(self):
        self.assertNotEqual(self.run_check('clippy', {}).returncode, 0)

    def test_warning_without_a_source_location_is_rejected(self):
        with tempfile.TemporaryDirectory() as directory:
            path = Path(directory) / 'clippy.jsonl'
            path.write_text(json.dumps({'reason': 'compiler-message', 'message': {
                'level': 'warning', 'code': None, 'message': 'unlocated warning', 'spans': []}})
                + '\n' + json.dumps({'reason': 'build-finished', 'success': True}) + '\n')
            result = subprocess.run([sys.executable, str(ROOT / 'scripts/check-quality.py'),
                                     'clippy', str(path)], capture_output=True, text=True)
            self.assertNotEqual(result.returncode, 0)
            self.assertIn('unlocated warning', result.stderr)

    def test_recorded_warning_passes_but_same_warning_on_another_line_fails(self):
        warning = {'reason': 'compiler-message', 'message': {
            'level': 'warning', 'code': {'code': 'clippy::suspicious_open_options'},
            'message': 'file opened with `create`, but `truncate` behavior not defined',
            'spans': [{'file_name': 'crates/signaltty-server/src/audit.rs',
                       'line_start': 111, 'is_primary': True}]}}
        with tempfile.TemporaryDirectory() as directory:
            path = Path(directory) / 'clippy.jsonl'
            scripts = Path(directory) / 'scripts'
            scripts.mkdir()
            checker = scripts / 'check-quality.py'
            checker.write_text((ROOT / 'scripts/check-quality.py').read_text())
            (scripts / 'clippy-baseline.json').write_text(json.dumps([[
                warning['message']['code']['code'],
                warning['message']['spans'][0]['file_name'], 111,
                warning['message']['message']]]))
            for line, expected in [(111, 0), (112, 1)]:
                warning['message']['spans'][0]['line_start'] = line
                path.write_text(json.dumps(warning) + '\n' + json.dumps(
                    {'reason': 'build-finished', 'success': True}) + '\n')
                result = subprocess.run([sys.executable, str(checker),
                                         'clippy', str(path)], capture_output=True, text=True)
                self.assertEqual(result.returncode, expected)

    def test_all_harnesses_resolve_the_same_verification_guide(self):
        canonical = ROOT / '.agents/skills/verify-signaltty'
        for harness in ['.claude', '.cursor', '.github']:
            path = ROOT / harness / 'skills/verify-signaltty'
            self.assertTrue(path.is_symlink(), str(path))
            self.assertEqual(path.resolve(strict=True), canonical)
            self.assertEqual((path / 'SKILL.md').read_bytes(), (canonical / 'SKILL.md').read_bytes())


if __name__ == '__main__':
    unittest.main()
