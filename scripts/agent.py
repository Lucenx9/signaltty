#!/usr/bin/env python3
"""Inspect signaltty's agent workflow; writes require index --write or handoff."""
import argparse
from datetime import datetime, timezone
import json
import os
from pathlib import Path
import re
import subprocess
import sys
import tempfile
from urllib.parse import unquote, urlsplit

ROOT = Path(__file__).resolve().parents[1]
CONFIG = '.agents/workflow.json'
LIFECYCLES = {'unreviewed', 'active', 'completed', 'superseded'}
ROUTING_START = '<!-- BEGIN GENERATED AGENT ROUTING -->'
ROUTING_END = '<!-- END GENERATED AGENT ROUTING -->'


def read_json(path):
    try:
        return json.loads(path.read_text())
    except (OSError, ValueError) as error:
        raise ValueError(f'cannot read {path}: {error}') from error


def repo_path(value):
    if not isinstance(value, str) or not value:
        raise ValueError('repository paths must be nonempty strings')
    path = ROOT / value
    if not path.resolve().is_relative_to(ROOT):
        raise ValueError(f'path escapes repository: {value}')
    return path


def configuration():
    value = read_json(ROOT / CONFIG)
    if not isinstance(value, dict) or type(value.get('schema_version')) is not int or value['schema_version'] != 1:
        raise ValueError(f'{CONFIG}: expected schema_version 1')
    for key in ['sources', 'routes', 'features']:
        if not isinstance(value.get(key), dict) or not value[key]:
            raise ValueError(f'{CONFIG}: {key} must be a nonempty object')
    for path in value['sources'].values():
        repo_path(path)
    for key, route in value['routes'].items():
        if not isinstance(route, dict) or not isinstance(route.get('label'), str):
            raise ValueError(f'invalid route: {key}')
        for field in ['first', 'conditional', 'documents']:
            if not isinstance(route.get(field), list) or not all(isinstance(x, str) for x in route[field]):
                raise ValueError(f'route {key}: {field} must be a string array')
        for path in route['documents']:
            repo_path(path)
        if not isinstance(route.get('commands'), list) or any(
                not isinstance(command, list) or not command or
                not all(isinstance(x, str) and x for x in command) for command in route['commands']):
            raise ValueError(f'route {key}: commands must be nonempty argument arrays')
    for slug, feature in value['features'].items():
        if not re.fullmatch(r'\d{3,}-[a-z0-9][a-z0-9-]*', slug) or not isinstance(feature, dict):
            raise ValueError(f'invalid feature slug or entry: {slug}')
        if feature.get('lifecycle') not in LIFECYCLES or type(feature.get('enrolled')) is not bool:
            raise ValueError(f'{slug}: invalid lifecycle or enrolled boolean')
        successor = feature.get('superseded_by')
        if feature['lifecycle'] == 'superseded' and not successor:
            raise ValueError(f'{slug}: superseded lifecycle requires superseded_by')
        if successor and (successor not in value['features'] or successor == slug):
            raise ValueError(f'{slug}: invalid superseded_by reference')
    return value


def git(*args):
    return subprocess.check_output(['git', '-C', str(ROOT), *args], text=True,
                                   stderr=subprocess.PIPE, timeout=15).strip()


def selected_feature(explicit, config):
    selection = explicit if explicit is not None else os.environ.get('SPECIFY_FEATURE_DIRECTORY')
    if selection is None:
        pointer = ROOT / '.specify/feature.json'
        if not pointer.exists():
            return None
        value = read_json(pointer)
        if not isinstance(value, dict) or not isinstance(value.get('feature_directory'), str):
            raise ValueError('feature.json must contain a feature_directory string')
        selection = value['feature_directory']
    if not isinstance(selection, str) or not selection:
        raise ValueError('feature must be a full slug or specs/<full-slug>')
    if '/' not in selection and not Path(selection).is_absolute():
        selection = 'specs/' + selection
    path = repo_path(selection)
    if path.is_symlink() or path.resolve().parent != (ROOT / 'specs').resolve() or not path.is_dir():
        raise ValueError(f'feature must be an existing direct specs child: {selection}')
    slug = path.name
    if slug not in config['features']:
        raise ValueError(f'{slug}: missing catalog entry in {CONFIG}')
    env = dict(os.environ, SPECIFY_INIT_DIR=str(ROOT),
               SPECIFY_FEATURE_DIRECTORY=str(path), SPECIFY_FEATURE_NO_PERSIST='1')
    result = subprocess.run(['bash', str(ROOT / '.specify/scripts/bash/check-prerequisites.sh'),
                             '--paths-only', '--json'], cwd=ROOT, env=env,
                            capture_output=True, text=True, timeout=15)
    if result.returncode:
        raise ValueError(f'Spec Kit resolution failed: {result.stderr.strip()}')
    resolved = json.loads(result.stdout)
    if Path(resolved['FEATURE_DIR']).resolve() != path.resolve():
        raise ValueError('Spec Kit resolved a different feature')
    return dict(config['features'][slug], slug=slug, directory=str(path))


def feature_observation(feature):
    if feature is None:
        return None
    path = Path(feature['directory'])
    for name in ['spec.md', 'plan.md', 'tasks.md']:
        if (path / name).resolve().parent != path.resolve():
            raise ValueError(f'{name} must belong to the selected feature directory')
    if (path / 'checklists').is_symlink():
        raise ValueError('checklists must belong to the selected feature directory')
    artifacts = {name: (path / name).is_file() for name in ['spec.md', 'plan.md', 'tasks.md']}
    tasks = []
    if artifacts['tasks.md']:
        for line in (path / 'tasks.md').read_text().splitlines():
            match = re.match(r'^\s*-\s+\[([ xX])\]\s+(.+)$', line)
            if match:
                identifier = re.match(r'(T\d+)\b', match[2])
                tasks.append({'id': identifier[1] if identifier else None,
                              'checked': match[1].lower() == 'x', 'text': match[2]})
    return dict(feature, artifacts=artifacts, tasks=tasks,
                task_progress={'total': len(tasks), 'checked': sum(t['checked'] for t in tasks),
                               'open': sum(not t['checked'] for t in tasks)})


def finding(code, message, action, path=None):
    return {'code': code, 'message': message, 'action': action, 'path': path}


def generated_documents(config):
    lines = ['# Feature catalog', '',
             'Generated by `scripts/agent.sh index --write` from `.agents/workflow.json`.',
             'Use the complete slug: numeric prefixes are historical and may repeat.',
             'Lifecycle is recorded intent; checked tasks and verified behavior are separate facts.',
             'Use `status --feature <slug> --evidence <run>` to inspect current proof.', '']
    for lifecycle in ['active', 'completed', 'superseded', 'unreviewed']:
        entries = [(slug, feature) for slug, feature in sorted(config['features'].items())
                   if feature['lifecycle'] == lifecycle]
        if not entries:
            continue
        lines += ['## ' + lifecycle.capitalize(), '', '| Feature | Phase checks | Artifacts | Successor |',
                  '|---|---|---|---|']
        for slug, feature in entries:
            path = ROOT / 'specs' / slug
            artifacts = ', '.join(f'[{name}]({slug}/{name})' for name in ['spec.md', 'plan.md', 'tasks.md']
                                  if (path / name).is_file())
            successor = feature.get('superseded_by', '')
            lines.append(f'| [{slug}]({slug}/spec.md) | {"enrolled" if feature["enrolled"] else "historical"} | {artifacts} | {successor} |')
        lines.append('')
    routing = ['| Key | Task | Load first | Add when needed |', '|---|---|---|---|']
    for key, route in config['routes'].items():
        first = ', '.join(f'`{skill}`' for skill in route['first'])
        conditional = '; '.join(route['conditional'])
        routing.append(f'| `{key}` | {route["label"]} | {first} | {conditional} |')
    skills = ROOT / 'docs/15-agent-skills.md'
    content = skills.read_text()
    if content.count(ROUTING_START) != 1 or content.count(ROUTING_END) != 1:
        raise ValueError('docs/15-agent-skills.md must contain one generated routing marker pair')
    start, end = content.index(ROUTING_START), content.index(ROUTING_END)
    if start >= end:
        raise ValueError('generated routing markers are reversed')
    content = content[:start + len(ROUTING_START)] + '\n\n' + '\n'.join(routing) + '\n\n' + content[end:]
    return {ROOT / 'specs/README.md': '\n'.join(lines), skills: content}


def generated_findings(config):
    errors = []
    for path, expected in generated_documents(config).items():
        if not path.is_file() or path.read_text() != expected:
            errors.append(finding('GENERATED_DRIFT', f'{path.relative_to(ROOT)} is stale.',
                                  'Run scripts/agent.sh index --write.', str(path)))
    return errors


def repository_findings(config):
    errors = []
    directories = {p.name for p in (ROOT / 'specs').iterdir() if p.is_dir()}
    catalog = set(config['features'])
    if directories != catalog:
        errors.append(finding('CATALOG_DRIFT', f'Uncataloged: {sorted(directories - catalog)}; absent: {sorted(catalog - directories)}',
                              f'Update feature entries in {CONFIG}; keep full slugs stable.'))
    documents = set(config['sources'].values())
    for route in config['routes'].values():
        documents.update(route['documents'])
    documents.update(path for path in ['AGENTS.md', 'docs/README.md', 'docs/15-agent-skills.md',
                                     'docs/17-agent-development.md', 'docs/18-agent-workflow.md']
                     if (ROOT / path).exists())
    for relative in sorted(documents):
        path = repo_path(relative)
        if not path.exists():
            errors.append(finding('MISSING_SOURCE', f'{relative} does not exist.',
                                  'Correct the authoritative source or route path.', relative))
        elif path.is_file() and path.suffix == '.md':
            content = re.sub(r'```.*?```', '', path.read_text(), flags=re.DOTALL)
            for match in re.finditer(r'\]\((<[^>]+>|[^)\s]+)(?:\s+"[^"]*")?\)', content):
                target = match[1].strip('<>')
                url = urlsplit(target)
                if url.scheme or url.netloc or not url.path:
                    continue
                linked = path.parent / unquote(url.path)
                if not linked.resolve().is_relative_to(ROOT) or not linked.exists():
                    errors.append(finding('BROKEN_LINK', f'{relative}: {target}',
                                          'Repair the local link in the authoritative document.', relative))
    for slug in sorted(directories & catalog):
        path = ROOT / 'specs' / slug
        if path.is_symlink() or path.resolve().parent != (ROOT / 'specs').resolve() or not (path / 'spec.md').is_file():
            errors.append(finding('INVALID_SPEC', f'{slug} has no spec or escapes specs/.',
                                  'Restore its spec at the stable catalog path.', str(path)))
    canonical = ROOT / '.agents/skills/verify-signaltty/SKILL.md'
    if canonical.exists():
        for harness in ['.claude', '.cursor', '.github']:
            link = ROOT / harness / 'skills/verify-signaltty'
            if not link.is_symlink() or link.resolve() != canonical.parent:
                errors.append(finding('SKILL_SOURCE_DRIFT', str(link.relative_to(ROOT)),
                                      'Restore the relative symlink to .agents/skills/verify-signaltty.'))
    command_sources = ROOT / '.specify/commands'
    sources = sorted(command_sources.glob('*.md'))
    if sources:
        result = subprocess.run(['bash', str(ROOT / 'scripts/setup-agent.sh'), 'commands', 'check',
                                 '--agent', 'all', '--installed-only'],
                                capture_output=True, text=True, timeout=30)
        if result.returncode:
            errors.append(finding('COMMAND_SOURCE_DRIFT', result.stdout.strip() + '\n' + result.stderr.strip(),
                                  'Run scripts/setup-agent.sh commands install --agent all.'))
    errors += generated_findings(config)
    return errors


def write_indexes(config):
    for path, content in generated_documents(config).items():
        repo_path(str(path.relative_to(ROOT)))
        if path.exists() and path.read_text() == content:
            continue
        with tempfile.NamedTemporaryFile(mode='w', dir=path.parent, delete=False) as stream:
            temporary = Path(stream.name)
            try:
                stream.write(content)
                stream.flush()
                os.chmod(temporary, 0o644)
                os.replace(temporary, path)
            finally:
                temporary.unlink(missing_ok=True)


def phase_findings(feature, phase):
    if feature is None:
        return [finding('NO_FEATURE', 'No feature is selected.',
                        'Pass --feature specs/<full-slug> or run speckit-specify.')]
    if not feature['enrolled']:
        return [finding('UNREVIEWED_FEATURE', 'Historical feature is not enrolled in phase checks.',
                        f'Review {feature["slug"]} and set enrolled in {CONFIG} for new implementation.')]
    required = ['spec.md']
    if phase in {'implementation', 'delivery'}:
        required += ['plan.md', 'tasks.md']
    errors = []
    path = Path(feature['directory'])
    for name in required:
        if not feature['artifacts'][name]:
            command = {'spec.md': 'specify', 'plan.md': 'plan', 'tasks.md': 'tasks'}[name]
            errors.append(finding('MISSING_' + name.split('.')[0].upper(), f'{name} is missing.',
                                  f'Run speckit-{command} for {feature["slug"]}.', str(path / name)))
        elif re.search(r'\[NEEDS CLARIFICATION(?:\]|:)', (path / name).read_text()):
            errors.append(finding('UNRESOLVED_CLARIFICATION', f'{name} has unresolved clarifications.',
                                  'Resolve the recorded question with speckit-clarify.', str(path / name)))
    if phase in {'implementation', 'delivery'} and feature['artifacts']['tasks.md']:
        tasks = feature['tasks']
        identifiers = [task['id'] for task in tasks]
        if not tasks or None in identifiers or len(identifiers) != len(set(identifiers)):
            errors.append(finding('INVALID_TASK_IDS', 'Tasks must have unique T identifiers.',
                                  'Add unique T### identifiers to the task checkboxes.', str(path / 'tasks.md')))
        spec = (path / 'spec.md').read_text() if feature['artifacts']['spec.md'] else ''
        requirements = set(re.findall(r'^\s*[-*]\s*(?:\*\*)?(FR-\d+)\b', spec, re.MULTILINE))
        references = set(re.findall(r'\[(FR-\d+)\]', '\n'.join(t['text'] for t in tasks)))
        if not requirements:
            errors.append(finding('NO_REQUIREMENTS', 'Enrolled spec has no FR identifiers.',
                                  'Give functional requirements stable FR-### identifiers.', str(path / 'spec.md')))
        if requirements - references:
            errors.append(finding('UNCOVERED_REQUIREMENTS', ', '.join(sorted(requirements - references)),
                                  'Reference each requirement as [FR-###] in at least one task.', str(path / 'tasks.md')))
        if references - requirements:
            errors.append(finding('UNKNOWN_REQUIREMENTS', ', '.join(sorted(references - requirements)),
                                  'Correct task references to the current spec requirements.', str(path / 'tasks.md')))
        if not (path / 'checklists/requirements.md').is_file():
            errors.append(finding('MISSING_CHECKLIST', 'The requirements review checklist is missing.',
                                  'Record the requirements review with speckit-specify/clarify.',
                                  str(path / 'checklists/requirements.md')))
        for checklist in sorted((path / 'checklists').glob('*.md')):
            if checklist.resolve().parent != (path / 'checklists').resolve():
                raise ValueError('checklist must belong to the selected feature')
            if re.search(r'^\s*-\s+\[ \]', checklist.read_text(), re.MULTILINE):
                errors.append(finding('OPEN_CHECKLIST', f'{checklist.name} has unresolved review items.',
                                      'Resolve and record the requirements review.', str(checklist)))
        if phase == 'delivery' and any(not t['checked'] for t in tasks):
            errors.append(finding('OPEN_TASKS', 'Implementation tasks remain open.',
                                  'Complete and verify the remaining task IDs.', str(path / 'tasks.md')))
    return errors


def evidence_status(path):
    if path is None:
        return {'status': 'unverified', 'reason': 'No evidence selected; pass --evidence <run-directory>.'}
    from verification_evidence import assess_evidence
    return assess_evidence(ROOT, ROOT / path)


def next_action(feature, evidence):
    if feature is None:
        return {'phase': 'specify', 'kind': 'agent_command', 'command': ['speckit-specify'],
                'reason': 'Create/select a feature with resolved requirements.'}
    if not feature['enrolled']:
        return {'phase': 'review', 'kind': 'manual', 'command': [],
                'reason': f'Review the recorded lifecycle and enroll {feature["slug"]} in {CONFIG} before new implementation.'}
    if not feature['artifacts']['spec.md']:
        command = 'speckit-specify'
    elif any(f['code'] == 'UNRESOLVED_CLARIFICATION' for f in phase_findings(feature, 'implementation')):
        command = 'speckit-clarify'
    elif not feature['artifacts']['plan.md']:
        command = 'speckit-plan'
    elif not feature['artifacts']['tasks.md']:
        command = 'speckit-tasks'
    else:
        errors = phase_findings(feature, 'implementation')
        if errors:
            return {'phase': 'tasks', 'kind': 'manual', 'command': [], 'reason': errors[0]['action'],
                    'findings': errors}
        if feature['task_progress']['open']:
            return {'phase': 'implement', 'kind': 'agent_command', 'command': ['speckit-implement'],
                    'task_ids': [t['id'] for t in feature['tasks'] if not t['checked']],
                    'reason': 'Implement and verify the remaining tasks.'}
        if evidence['status'] in {'running', 'failed', 'unavailable'}:
            return {'phase': 'verify' if evidence['status'] == 'running' else 'diagnose',
                    'kind': 'manual', 'command': [],
                    'reason': evidence.get('recovery') or evidence['reason'],
                    'evidence': evidence.get('summary'), 'log': evidence.get('log')}
        if evidence['status'] != 'fresh':
            return {'phase': 'verify', 'kind': 'shell', 'command': ['scripts/verify.sh', 'full'],
                    'reason': evidence['reason']}
        return {'phase': 'review', 'kind': 'agent_command', 'command': ['code-review'],
                'reason': 'Workflow checks have proof; review intent and behavior before publication.'}
    return {'phase': command.removeprefix('speckit-'), 'kind': 'agent_command',
            'command': [command], 'reason': 'Produce the missing or clarified feature artifact.'}


def create_handoff(feature, evidence, output, notes):
    if feature is None:
        raise ValueError('handoff requires a selected feature')
    if output is None:
        timestamp = datetime.now(timezone.utc).strftime('%Y%m%dT%H%M%S%fZ')
        output = ROOT / 'target/handoffs' / f'{feature["slug"]}-{timestamp}.md'
    else:
        output = (ROOT / output).absolute()
    output.parent.mkdir(parents=True, exist_ok=True)
    branch, revision = git('branch', '--show-current'), git('rev-parse', 'HEAD')
    try:
        base = git('merge-base', 'HEAD', 'origin/main')
    except subprocess.SubprocessError:
        base = 'Not recorded; add the intended base in decisions below.'
    content = '\n'.join([
        f'# Handoff: {feature["slug"]}', '',
        'Generated facts are a point-in-time observation. Task status remains in tasks.md.', '',
        f'Worktree: {ROOT}', f'Branch: {branch}', f'Base: {base}', f'Current commit: {revision}',
        f'Recorded lifecycle: {feature["lifecycle"]}',
        'Open task references: ' + ', '.join(t['id'] or '(unnumbered)' for t in feature['tasks'] if not t['checked']),
        f'Evidence before this handoff: {json.dumps(evidence, sort_keys=True)}',
        'Saving a nonignored handoff in source changes content and invalidates prior evidence.', '',
        'Working tree changes:', '```text', git('status', '--short'), '```', '',
        'Next concrete step:', '```json', json.dumps(next_action(feature, evidence), indent=2), '```', '',
        '## Decisions, file ownership and unverified behavior', '',
        notes or 'Record accepted decisions, file ownership and remaining unverified behavior here.', ''])
    with output.open('x') as stream:
        stream.write(content)
    return str(output)


class WorkflowParser(argparse.ArgumentParser):
    def error(self, message):
        if '--json' in sys.argv:
            print(json.dumps({'schema_version': 1, 'command': None, 'ok': False,
                              'findings': [finding('INVALID_ARGUMENTS', message, 'Read scripts/agent.sh --help.')]}))
            self.exit(2)
        super().error(message)


def render(result):
    data = result.get('data', {})
    if result['command'] in {'status', 'next'} and data:
        feature = data['feature']
        print(f'Checkout: {data["root"]}\nBranch: {data["branch"]}\nRevision: {data["revision"]}')
        if feature:
            progress = feature['task_progress']
            print(f'Feature: {feature["slug"]} ({feature["lifecycle"]})\nTasks: {progress["checked"]}/{progress["total"]} checked')
        else:
            print('Feature: none selected')
        print(f'Evidence: {data["evidence"]["status"]}\nNext: {data["next"]["reason"]}')
        if data['next']['command']:
            print('Command: ' + ' '.join(data['next']['command']))
    elif result['command'] == 'context' and data:
        for key, route in data['routes'].items():
            print(f'{key}: {route["label"]}\nLoad: {", ".join(route["first"])}')
            for document in route['documents']:
                print('Read: ' + document)
            for command in route['commands']:
                print('Run: ' + ' '.join(command))
            for condition in route['conditional']:
                print('When needed: ' + condition)
    elif result['ok']:
        print(f'{result["command"]}: passed')
        if 'output' in data:
            print('Handoff: ' + data['output'])
        if data.get('exemption'):
            print('Declared feature-process exemption: ' + data['exemption'])
    for issue in result.get('findings', []):
        print(f'{issue["code"]}: {issue["message"]}\nNext: {issue["action"]}', file=sys.stderr)


def main():
    parser = WorkflowParser(description=__doc__)
    parser.add_argument('command', choices=['status', 'context', 'next', 'check', 'index', 'handoff'])
    parser.add_argument('--feature', help='full slug or direct specs child; never persists selection')
    parser.add_argument('--task', help='routing key, such as ipc or gui')
    parser.add_argument('--phase', choices=['repository', 'planning', 'implementation', 'delivery'], default='repository')
    parser.add_argument('--scope', choices=['feature', 'docs-only', 'one-line'], default='feature')
    parser.add_argument('--evidence', type=Path, help='verification directory or summary.json; no test execution')
    parser.add_argument('--write', action='store_true', help='explicitly regenerate indexes')
    parser.add_argument('--output', type=Path, help='new handoff path; default under ignored target/handoffs')
    parser.add_argument('--notes', help='accepted decisions and file ownership to include in handoff')
    parser.add_argument('--json', action='store_true', help='versioned JSON output')
    args = parser.parse_args()
    if args.write and args.command != 'index':
        parser.error('--write is only valid for index')
    if (args.output is not None or args.notes is not None) and args.command != 'handoff':
        parser.error('--output and --notes are only valid for handoff')
    try:
        config = configuration()
        independent = args.command == 'index' or (args.command == 'check' and
                                                 (args.phase == 'repository' or args.scope != 'feature'))
        feature = None if independent else feature_observation(selected_feature(args.feature, config))
        evidence = evidence_status(args.evidence)
        errors = []
        if args.command == 'context':
            if args.task and args.task not in config['routes']:
                raise ValueError(f'unknown task {args.task}; choose {", ".join(config["routes"])}')
            data = {'sources': config['sources'], 'feature': feature,
                    'routes': {args.task: config['routes'][args.task]} if args.task else config['routes']}
        elif args.command in {'status', 'next'}:
            data = {'root': str(ROOT), 'branch': git('branch', '--show-current'),
                    'revision': git('rev-parse', 'HEAD'), 'dirty': bool(git('status', '--porcelain')),
                    'feature': feature, 'evidence': evidence, 'next': next_action(feature, evidence)}
        elif args.command == 'index':
            if args.write:
                write_indexes(config)
            errors = generated_findings(config)
            data = {'documents': [str(p.relative_to(ROOT)) for p in generated_documents(config)], 'written': args.write}
        elif args.command == 'handoff':
            output = create_handoff(feature, evidence, args.output, args.notes)
            data = {'output': output, 'evidence_after': evidence_status(args.evidence)}
        else:
            errors += repository_findings(config)
            if args.phase != 'repository' and args.scope == 'feature':
                errors += phase_findings(feature, args.phase)
            if args.phase == 'delivery' and evidence['status'] != 'fresh':
                errors.append(finding('UNVERIFIED_DELIVERY', evidence['reason'], evidence.get('recovery') or
                                      'Run scripts/verify.sh full and pass its evidence directory.'))
            data = {'phase': args.phase, 'scope': args.scope, 'feature': feature,
                    'evidence': evidence, 'exemption': args.scope if args.scope != 'feature' else None}
        result = {'schema_version': 1, 'command': args.command, 'ok': not errors,
                  'data': data, 'findings': errors}
        code = int(bool(errors))
    except (ValueError, OSError, KeyError, TypeError, subprocess.SubprocessError) as error:
        result, code = {'schema_version': 1, 'command': args.command, 'ok': False,
                        'findings': [{'code': 'INVALID_INPUT', 'message': str(error),
                                      'action': 'Check the named source or pass --feature specs/<full-slug>.'}]}, 2
    if args.json:
        print(json.dumps(result, indent=2))
    else:
        render(result)
    return code


if __name__ == '__main__':
    raise SystemExit(main())
