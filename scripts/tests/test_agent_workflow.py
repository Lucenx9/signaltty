import json
import os
from pathlib import Path
import shutil
import subprocess
import sys
import tempfile
import unittest

ROOT = Path(__file__).resolve().parents[2]


class AgentWorkflowCLI(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory()
        self.addCleanup(self.temp.cleanup)
        self.root = Path(self.temp.name)
        (self.root / 'scripts').mkdir()
        for name in ['agent.py', 'agent.sh', 'verification_evidence.py', 'setup-agent.sh']:
            source = ROOT / 'scripts' / name
            if source.exists():
                shutil.copy2(source, self.root / 'scripts' / name)
        shutil.copytree(ROOT / '.specify/scripts', self.root / '.specify/scripts')
        (self.root / '.agents').mkdir()
        (self.root / 'docs').mkdir()
        (self.root / 'docs/guide.md').write_text('# Guide\n')
        (self.root / 'docs/15-agent-skills.md').write_text(
            '# Routing\n<!-- BEGIN GENERATED AGENT ROUTING -->\n'
            '<!-- END GENERATED AGENT ROUTING -->\n')
        for slug in ['001-original', '001-current']:
            feature = self.root / 'specs' / slug
            feature.mkdir(parents=True)
            (feature / 'spec.md').write_text('# Spec\n- FR-001: preserve work.\n')
            if slug == '001-current':
                (feature / 'checklists').mkdir()
                (feature / 'checklists/requirements.md').write_text('- [x] Requirements reviewed.\n')
        (self.root / '.specify/feature.json').write_text(
            '{"feature_directory":"specs/001-original"}\n')
        self.config = {
            'schema_version': 1,
            'sources': {'guide': 'docs/guide.md'},
            'routes': {'ipc': {'label': 'IPC', 'first': ['tdd'], 'conditional': [],
                               'documents': ['docs/guide.md'],
                               'commands': [['cargo', 'test', '-p', 'signaltty-server']]}},
            'features': {'001-original': {'lifecycle': 'unreviewed', 'enrolled': False},
                         '001-current': {'lifecycle': 'active', 'enrolled': True}},
        }
        self.save_config()
        self.run_cli('index', '--write')
        (self.root / '.gitignore').write_text('/target/\n__pycache__/\n')
        self.git('init', '-q')
        self.git('add', '.')
        self.git('-c', 'user.email=test@example.invalid', '-c', 'user.name=Test',
                 'commit', '-qm', 'fixture')

    def git(self, *args):
        return subprocess.check_output(['git', '-C', str(self.root), *args], text=True)

    def save_config(self):
        (self.root / '.agents/workflow.json').write_text(json.dumps(self.config))

    def run_cli(self, *args, expected=0):
        result = subprocess.run([sys.executable, str(self.root / 'scripts/agent.py'),
                                 *args, '--json'], cwd='/', capture_output=True, text=True,
                                env={k: v for k, v in os.environ.items()
                                     if not k.startswith('SPECIFY_')})
        self.assertEqual(result.returncode, expected, result.stdout + result.stderr)
        return json.loads(result.stdout)

    def test_explicit_feature_is_read_only_and_duplicate_prefixes_stay_distinct(self):
        before = self.git('status', '--porcelain')
        pointer = (self.root / '.specify/feature.json').read_bytes()
        result = self.run_cli('status', '--feature', '001-current')
        self.assertEqual(result['data']['feature']['slug'], '001-current')
        self.assertEqual(result['data']['feature']['lifecycle'], 'active')
        self.assertEqual(self.git('status', '--porcelain'), before)
        self.assertEqual((self.root / '.specify/feature.json').read_bytes(), pointer)

    def test_next_and_phase_check_name_the_actual_missing_prerequisite(self):
        result = self.run_cli('next', '--feature', '001-current')
        self.assertEqual(result['data']['next']['command'], ['speckit-plan'])
        result = self.run_cli('check', '--phase', 'implementation',
                              '--feature', '001-current', expected=1)
        self.assertIn('MISSING_PLAN', [f['code'] for f in result['findings']])
        (self.root / 'specs/001-current/spec.md').write_text(
            '# Spec\n- FR-001: [NEEDS CLARIFICATION: which behavior?]\n')
        result = self.run_cli('next', '--feature', '001-current')
        self.assertEqual(result['data']['next']['command'], ['speckit-clarify'])

    def test_generated_indexes_and_catalog_integrity_do_not_gate_historical_work(self):
        (self.root / 'docs/15-agent-skills.md').write_text(
            '# Routing\n<!-- BEGIN GENERATED AGENT ROUTING -->\n'
            '<!-- END GENERATED AGENT ROUTING -->\n')
        self.run_cli('index', '--write')
        result = self.run_cli('check', '--phase', 'repository')
        self.assertTrue(result['ok'])
        index = (self.root / 'specs/README.md').read_text()
        self.assertIn('001-original', index)
        self.assertIn('001-current', index)
        del self.config['features']['001-original']
        self.save_config()
        result = self.run_cli('check', '--phase', 'repository', expected=1)
        self.assertIn('CATALOG_DRIFT', [f['code'] for f in result['findings']])

    def test_handoff_preserves_existing_notes_and_refuses_overwrite(self):
        output = self.root / 'target/handoff.md'
        result = self.run_cli('handoff', '--feature', '001-current',
                              '--output', str(output), '--notes', 'Accepted: keep stable slugs.')
        self.assertEqual(result['data']['output'], str(output))
        self.assertIn('Accepted: keep stable slugs.', output.read_text())
        original = output.read_bytes()
        self.run_cli('handoff', '--feature', '001-current', '--output', str(output), expected=2)
        self.assertEqual(output.read_bytes(), original)

    def test_requirement_coverage_and_checked_tasks_need_separate_delivery_proof(self):
        feature = self.root / 'specs/001-current'
        (feature / 'plan.md').write_text('# Plan\n')
        (feature / 'tasks.md').write_text('- [ ] T001 implement\n')
        self.run_cli('index', '--write')
        result = self.run_cli('check', '--phase', 'implementation',
                              '--feature', '001-current', expected=1)
        self.assertIn('UNCOVERED_REQUIREMENTS', [f['code'] for f in result['findings']])
        (feature / 'tasks.md').write_text('- [x] T001 [FR-001] implement\n')
        result = self.run_cli('check', '--phase', 'delivery',
                              '--feature', '001-current', expected=1)
        self.assertEqual([f['code'] for f in result['findings']], ['UNVERIFIED_DELIVERY'])
        report = self.root / 'target/doctor/summary.json'
        report.parent.mkdir(parents=True)
        report.write_text('{"status":"passed","mode":"doctor"}')
        result = self.run_cli('check', '--phase', 'delivery', '--feature', '001-current',
                              '--evidence', str(report.relative_to(self.root)), expected=1)
        self.assertEqual(result['data']['evidence']['status'], 'unverified')

    def test_context_uses_canonical_routes_and_invalid_selection_is_actionable(self):
        result = self.run_cli('context', '--task', 'ipc')
        self.assertEqual(result['data']['routes']['ipc']['documents'], ['docs/guide.md'])
        self.assertEqual(result['data']['routes']['ipc']['commands'],
                         [['cargo', 'test', '-p', 'signaltty-server']])
        result = self.run_cli('status', '--feature', '../escape', expected=2)
        self.assertEqual(result['findings'][0]['code'], 'INVALID_INPUT')
        (self.root / '.specify/feature.json').write_text('{"feature_directory":42}')
        self.run_cli('status', expected=2)
        (self.root / '.specify/feature.json').unlink()
        self.assertIsNone(self.run_cli('status')['data']['feature'])

    def test_stale_generated_routing_and_broken_source_links_fail_repository_check(self):
        (self.root / 'docs/15-agent-skills.md').write_text(
            '# Routing\n<!-- BEGIN GENERATED AGENT ROUTING -->\nwrong\n'
            '<!-- END GENERATED AGENT ROUTING -->\n')
        (self.root / 'docs/guide.md').write_text('# Guide\n[missing](missing.md)\n')
        result = self.run_cli('check', '--phase', 'repository', expected=1)
        self.assertEqual({f['code'] for f in result['findings']}, {'BROKEN_LINK', 'GENERATED_DRIFT'})

    def test_setup_checker_rejects_divergent_generated_commands(self):
        sources = self.root / '.specify/commands'
        sources.mkdir()
        (sources / 'specify.md').write_text('---\ndescription: fixture\n---\nOriginal\n')
        command = ['bash', str(self.root / 'scripts/setup-agent.sh'), 'commands']
        subprocess.run(command + ['install', '--agent', 'codex'], check=True,
                       capture_output=True, text=True)
        self.assertEqual(subprocess.run(command + ['check', '--agent', 'all', '--installed-only'],
                                       capture_output=True).returncode, 0)
        generated = self.root / '.agents/skills/speckit-specify/SKILL.md'
        generated.write_text(generated.read_text().replace('Original', 'Changed'))
        result = subprocess.run(command + ['check', '--agent', 'all', '--installed-only'], capture_output=True, text=True)
        self.assertNotEqual(result.returncode, 0)
        self.assertIn('changed', result.stdout + result.stderr)
        result = self.run_cli('check', '--phase', 'repository', expected=1)
        self.assertIn('COMMAND_SOURCE_DRIFT', [f['code'] for f in result['findings']])

    def test_explicit_empty_selection_never_falls_back_to_previous_work(self):
        result = self.run_cli('status', '--feature', '', expected=2)
        self.assertEqual(result['findings'][0]['code'], 'INVALID_INPUT')

    def test_feature_alias_cannot_mix_metadata_and_artifacts(self):
        shutil.rmtree(self.root / 'specs/001-current')
        (self.root / 'specs/001-current').symlink_to('001-original', target_is_directory=True)
        result = self.run_cli('status', '--feature', '001-current', expected=2)
        self.assertEqual(result['findings'][0]['code'], 'INVALID_INPUT')

    def test_implementation_requires_the_recorded_requirements_review(self):
        feature = self.root / 'specs/001-current'
        (feature / 'plan.md').write_text('# Plan\n')
        (feature / 'tasks.md').write_text('- [x] T001 [FR-001] implement\n')
        (feature / 'checklists/requirements.md').unlink()
        self.run_cli('index', '--write')
        result = self.run_cli('check', '--phase', 'implementation',
                              '--feature', '001-current', expected=1)
        self.assertEqual([f['code'] for f in result['findings']], ['MISSING_CHECKLIST'])

    def test_next_inspects_running_or_failed_verification_before_another_run(self):
        feature = self.root / 'specs/001-current'
        (feature / 'plan.md').write_text('# Plan\n')
        (feature / 'tasks.md').write_text('- [x] T001 [FR-001] implement\n')
        report = self.root / 'target/evidence/summary.json'
        report.parent.mkdir(parents=True)
        for state in ['running', 'failed']:
            with self.subTest(state=state):
                report.write_text(json.dumps({'status': state, 'steps': []}))
                result = self.run_cli('next', '--feature', '001-current', '--evidence', str(report))
                self.assertEqual(result['data']['next']['kind'], 'manual')
                self.assertEqual(result['data']['next']['command'], [])
                self.assertIn('Inspect' if state == 'running' else 'Read', result['data']['next']['reason'])


if __name__ == '__main__':
    unittest.main()
