"""Content provenance shared by the verification runner and workflow CLI."""
import hashlib
import json
import os
from pathlib import Path
import stat
import subprocess

FULL_STEPS = {'tooling-tests', 'repository', 'fmt', 'metadata', 'architecture', 'clippy',
              'warning-policy', 'build', 'workspace-tests', 'server-qa', 'gui-build',
              'refresh-benchmark'}


def _git(root, *args, input=None):
    try:
        result = subprocess.run(['git', *args], cwd=root, input=input, capture_output=True, timeout=30)
    except subprocess.TimeoutExpired as error:
        raise ValueError(f'cannot identify checkout: Git timed out: {args}') from error
    if result.returncode:
        raise ValueError(f'cannot identify checkout: {result.stderr.decode(errors="replace").strip()}')
    return result.stdout


def _frame(digest, value):
    digest.update(len(value).to_bytes(8, 'big'))
    digest.update(value)


def capture_checkout(root, output=None):
    """Hash working files and staged entries; revision is descriptive, not identity."""
    root = Path(root).resolve()
    toplevel = Path(os.fsdecode(_git(root, 'rev-parse', '--show-toplevel').strip())).resolve()
    if root != toplevel:
        raise ValueError(f'expected checkout root {toplevel}, got {root}')
    paths = sorted(set(_git(root, 'ls-files', '-z', '--cached', '--others', '--exclude-standard').split(b'\0')) - {b''})
    if output is not None:
        destination = Path(output).resolve()
        if destination == root:
            raise ValueError('evidence output must be ignored or outside the checkout')
        if root in destination.parents:
            relative = destination.relative_to(root).as_posix()
            try:
                ignored = subprocess.run(['git', 'check-ignore', '--quiet', '--', relative + '/'],
                                         cwd=root, capture_output=True, timeout=30)
            except subprocess.TimeoutExpired as error:
                raise ValueError('cannot establish ignored evidence output: Git timed out') from error
            contains_input = any(os.fsdecode(name) == relative or
                                 os.fsdecode(name).startswith(relative + '/') for name in paths)
            if ignored.returncode != 0 or contains_input:
                raise ValueError('evidence output must be ignored or outside the checkout')
    content = hashlib.sha256()
    for name in paths:
        path = root / os.fsdecode(name)
        for parent in path.parents:
            if parent == root:
                break
            if parent.is_symlink():
                raise ValueError(f'checkout input has a symlink parent: {path}')
        _frame(content, name)
        try:
            metadata = path.lstat()
        except FileNotFoundError:
            _frame(content, b'missing')
            continue
        _frame(content, str(stat.S_IMODE(metadata.st_mode)).encode())
        if stat.S_ISLNK(metadata.st_mode):
            _frame(content, b'symlink')
            _frame(content, os.fsencode(os.readlink(path)))
        elif stat.S_ISREG(metadata.st_mode):
            _frame(content, b'file')
            _frame(content, path.read_bytes())
        else:
            raise ValueError(f'unsupported checkout file type: {path}')
    index = hashlib.sha256()
    entries = sorted(entry for entry in _git(root, 'ls-files', '-z', '--stage').split(b'\0') if entry)
    objects = [entry.split(b'\t', 1)[0].split()[1] for entry in entries]
    blobs = _git(root, 'cat-file', '--batch', input=b'\n'.join(objects) + b'\n') if objects else b''
    offset = 0
    for entry in entries:
        end = blobs.find(b'\n', offset)
        header = blobs[offset:end].split()
        if end == -1 or len(header) != 3 or header[1] != b'blob' or not header[2].isdigit():
            raise ValueError(f'cannot read staged blob: {os.fsdecode(entry)}')
        size = int(header[2])
        start = end + 1
        if start + size >= len(blobs) or blobs[start + size:start + size + 1] != b'\n':
            raise ValueError('incomplete staged blob data from Git')
        _frame(index, entry)
        _frame(index, blobs[start:start + size])
        offset = start + size + 1
    return dict(schema_version=1, root=str(root),
                revision=os.fsdecode(_git(root, 'rev-parse', 'HEAD').strip()),
                content_digest=content.hexdigest(), index_digest=index.hexdigest(), files=len(paths))


def _valid_snapshot(value):
    return (isinstance(value, dict) and type(value.get('schema_version')) is int and
            value['schema_version'] == 1 and isinstance(value.get('root'), str) and
            Path(value['root']).is_absolute() and isinstance(value.get('revision'), str) and
            bool(value['revision']) and type(value.get('files')) is int and value['files'] >= 0 and
            all(isinstance(value.get(key), str) and len(value[key]) == 64 and
                all(character in '0123456789abcdef' for character in value[key])
                for key in ('content_digest', 'index_digest')))


def _identity(snapshot):
    return tuple(snapshot[key] for key in ('root', 'content_digest', 'index_digest', 'files'))


def assess_evidence(root, summary_path):
    """Inspect an existing report without running checks or accepting legacy proof."""
    root = Path(root).resolve()
    path = Path(summary_path).resolve()
    if path.is_dir():
        path /= 'summary.json'
    result = dict(summary=str(path), status='unverified', reason='',
                  recovery='Run scripts/verify.sh full on the current checkout.')

    def finish(status, reason):
        result.update(status=status, reason=reason)
        return result

    try:
        report = json.loads(path.read_text())
    except OSError as error:
        result['recovery'] = 'Inspect the selected evidence path and its readability; correct it or select an existing report before rerunning verification.'
        return finish('unavailable', f'cannot read evidence: {error}')
    except (ValueError, UnicodeError) as error:
        return finish('unverified', f'malformed evidence: {error}')
    if not isinstance(report, dict):
        return finish('unverified', 'evidence summary must be an object')
    if report.get('status') == 'running':
        result['recovery'] = 'Inspect the running verification process and its current log; interrupted evidence must be rerun.'
        return finish('running', 'verification has not completed')
    if report.get('status') == 'failed':
        steps = report.get('steps', [])
        failed = next((step for step in reversed(steps) if isinstance(step, dict) and
                       step.get('status') == 'failed'), {}) if isinstance(steps, list) else {}
        result['failed_step'] = failed.get('name')
        log = failed.get('log')
        result['log'] = str(path.parent / log) if isinstance(log, str) else None
        failure = report.get('failure')
        result['recovery'] = (failure.get('recovery') if isinstance(failure, dict) else None) or (
            f'Read {result["log"] or path}; diagnose the failure before a bounded rerun of scripts/verify.sh full.')
        return finish('failed', 'verification failed')
    if (report.get('status') != 'passed' or type(report.get('schema_version')) is not int or
            report['schema_version'] != 1 or report.get('mode') not in ('full', 'desktop') or
            report.get('errors') != []):
        return finish('unverified', 'only a successful versioned full or desktop run is delivery proof')
    steps = report.get('steps')
    if (not isinstance(steps, list) or not steps or
            any(not isinstance(step, dict) or step.get('status') != 'passed' or
                type(step.get('exit_code')) is not int or step['exit_code'] != 0 or
                not isinstance(step.get('name'), str) for step in steps)):
        return finish('unverified', 'verification steps are missing, incomplete or unsuccessful')
    names = {step['name'] for step in steps}
    count = report.get('display_tests')
    if (len(names) != len(steps) or type(count) is not int or count < 1 or count > len(steps) or
            {name for name in names if name.startswith('gtk-')} !=
            {f'gtk-{index:02d}' for index in range(1, count + 1)}):
        return finish('unverified', 'discovered GTK test count or unique completed steps do not match')
    required = FULL_STEPS | ({'native-accessibility'} if report['mode'] == 'desktop' else set())
    if (not required <= names or not any(name.startswith('gui-list-') for name in names) or
            not any(name.startswith('gtk-') for name in names)):
        return finish('unverified', 'full verification steps are missing')
    provenance = report.get('provenance')
    if not isinstance(provenance, dict):
        return finish('unverified', 'verification has no content provenance')
    before, after = provenance.get('before'), provenance.get('after')
    if not _valid_snapshot(before) or not _valid_snapshot(after):
        return finish('unverified', 'verification provenance is unavailable or malformed')
    if before['root'] != str(root) or after['root'] != str(root):
        return finish('stale', 'verification belongs to a different checkout')
    if provenance.get('unchanged') is not True or _identity(before) != _identity(after):
        return finish('stale', 'checkout inputs changed during verification')
    try:
        current = capture_checkout(root)
    except (ValueError, OSError, subprocess.SubprocessError) as error:
        result['recovery'] = 'Inspect Git availability and checkout input errors; restore provenance before rerunning verification.'
        return finish('unavailable', f'cannot capture current checkout: {error}')
    if _identity(current) != _identity(after):
        return finish('stale', 'checkout inputs changed after verification')
    result['recovery'] = None
    return finish('fresh', 'successful full verification matches current checkout inputs')
