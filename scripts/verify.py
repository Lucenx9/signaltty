#!/usr/bin/env python3
"""Verify signaltty; full includes isolated GTK tests, desktop adds native AT-SPI QA."""
import argparse
import json
import os
from pathlib import Path
import shutil
import signal
import subprocess
import sys
import tempfile
import time

ROOT = Path(__file__).resolve().parents[1]
LIMITS = [
    'Screenshots require visual review; automated success is not visual approval.',
    'IME, folder chooser, notification clicks and paid provider sessions require manual checks.',
    'Native accessibility QA runs only in desktop mode; Xvfb does not certify a compositor.',
]


def interrupted(_signal, _frame):
    raise KeyboardInterrupt('termination requested')


def probe(argv):
    return subprocess.check_output(argv, cwd=ROOT, text=True, stderr=subprocess.STDOUT,
                                   timeout=30).strip()


def doctor(mode):
    errors, versions = [], {}
    if mode != 'doctor' and os.geteuid() == 0:
        errors.append('run verification as an unprivileged user; filesystem-permission tests cannot run as root')
    required = ['cargo', 'rustc', 'rustfmt', 'git', 'pkg-config', 'cc', 'node']
    if mode in {'doctor', 'full', 'desktop'}:
        required += ['xvfb-run', 'Xvfb', 'xauth', 'dbus-run-session']
    for executable in required:
        if not shutil.which(executable):
            errors.append(f'missing {executable}; run scripts/setup-dev.sh --system')
    for package, minimum in [('gtk4', '4.18'), ('libadwaita-1', '1.7'),
                             ('vte-2.91-gtk4', '0.80')]:
        if shutil.which('pkg-config'):
            try:
                probe(['pkg-config', f'--atleast-version={minimum}', package])
                versions[package] = probe(['pkg-config', '--modversion', package])
            except subprocess.SubprocessError:
                errors.append(f'require {package} >= {minimum}; run scripts/setup-dev.sh --system')
    if shutil.which('rustc'):
        try:
            versions['rustc'] = probe(['rustc', '--version'])
            version = versions['rustc'].split()[1].split('-')[0]
            if tuple(map(int, version.split('.'))) < (1, 92, 0):
                errors.append('locked GTK dependencies require Rust >= 1.92')
        except (subprocess.SubprocessError, ValueError, IndexError) as error:
            errors.append(f'cannot inspect Rust: {error}')
    if shutil.which('cargo'):
        try:
            versions['clippy'] = probe(['cargo', 'clippy', '--version'])
        except subprocess.SubprocessError:
            errors.append('missing Clippy; run rustup component add clippy')
    if mode == 'desktop':
        if not (os.environ.get('DISPLAY') or os.environ.get('WAYLAND_DISPLAY')):
            errors.append('desktop mode needs a real graphical session')
        try:
            probe([sys.executable, '-c',
                   'import gi; gi.require_version("Atspi", "2.0"); from gi.repository import Gio, GLib, Atspi'])
        except subprocess.SubprocessError:
            errors.append('desktop mode needs Python gi and Atspi bindings')
    return errors, versions


class Verification:
    def __init__(self, mode, output):
        self.output = output
        self.env = dict(os.environ, CARGO_TARGET_DIR=str(ROOT / 'target'),
                        SIGNALTTY_QA_LOG_DIR=str(output / 'qa-logs'))
        # Existing native helpers resolve binaries under the checkout's target/debug.
        # A separate checkout therefore owns its binaries and app sessions.
        self.report = dict(mode=mode, status='running', steps=[], errors=[], limits=LIMITS)
        if shutil.which('git'):
            try:
                self.report['revision'] = probe(['git', 'rev-parse', 'HEAD'])
                self.report['dirty'] = bool(probe(['git', 'status', '--porcelain']))
            except subprocess.SubprocessError:
                self.report['revision'] = None

    def save(self):
        (self.output / 'summary.json').write_text(json.dumps(self.report, indent=2) + '\n')

    def run(self, name, argv, timeout=900, env=None):
        log = self.output / f'{name}.log'
        step = dict(name=name, command=list(map(str, argv)), log=log.name, status='running')
        self.report['steps'].append(step)
        self.save()
        print(f'[{name}] {" ".join(map(str, argv))}', flush=True)
        started = time.monotonic()
        process = None
        try:
            with log.open('w') as stream:
                process = subprocess.Popen(argv, cwd=ROOT, env=self.env | (env or {}),
                                           stdout=stream, stderr=subprocess.STDOUT,
                                           start_new_session=True)
                step['exit_code'] = process.wait(timeout=timeout)
            if step['exit_code']:
                raise RuntimeError(f'{name} failed ({step["exit_code"]}); see {log}')
            step['status'] = 'passed'
        except BaseException:
            step['status'] = 'failed'
            if process is not None:
                try:
                    os.killpg(process.pid, signal.SIGTERM)
                    # QA helpers first stop their separately-sessioned servers/GUI.
                    # Their bounded socket waits plus child waits can exceed 5 seconds.
                    process.wait(timeout=30)
                except subprocess.TimeoutExpired:
                    os.killpg(process.pid, signal.SIGKILL)
                    process.wait()
                except ProcessLookupError:
                    pass
            if log.exists():
                print(log.read_text(errors='replace')[-6000:], file=sys.stderr)
            raise
        finally:
            step['seconds'] = round(time.monotonic() - started, 3)
            self.save()
        return log

    def checks(self, mode):
        self.run('tooling-tests', [sys.executable, '-m', 'unittest', 'discover', '-s', 'scripts/tests', '-v'])
        self.run('fmt', ['cargo', 'fmt', '--check'])
        metadata = self.run('metadata', ['cargo', 'metadata', '--locked', '--format-version', '1'])
        metadata_json = self.output / 'metadata.json'
        metadata_json.write_text('\n'.join(line for line in metadata.read_text().splitlines()
                                           if line.startswith('{')))
        self.run('architecture', [sys.executable, 'scripts/check-quality.py', 'architecture', str(metadata_json)])
        diagnostics = self.run('clippy', ['cargo', 'clippy', '--workspace', '--all-targets', '--locked',
                                          '--message-format=json'])
        # Cargo progress goes to stderr; retain it in the log, extract only JSON records.
        records = self.output / 'clippy.jsonl'
        records.write_text('\n'.join(line for line in diagnostics.read_text().splitlines()
                                     if line.startswith('{')) + '\n')
        self.run('warning-policy', [sys.executable, 'scripts/check-quality.py', 'clippy', str(records)])
        # Testkit locates real target/debug executables, not the Cargo test binaries.
        self.run('build', ['cargo', 'build', '--workspace', '--locked'])
        self.run('workspace-tests', ['cargo', 'test', '--workspace', '--locked'])
        if mode == 'fast':
            return
        self.run('server-qa', [sys.executable, 'scripts/qa-server-edge-cases.py'])
        artifacts = self.run('gui-build', ['cargo', 'test', '-p', 'signaltty-gui', '--locked',
                                            '--no-run', '--message-format=json'])
        binaries = set()
        for line in artifacts.read_text().splitlines():
            if line.startswith('{'):
                value = json.loads(line)
                if value.get('reason') == 'compiler-artifact' and value['profile']['test'] and value.get('executable'):
                    binaries.add(value['executable'])
        count = 0
        for index, binary in enumerate(sorted(binaries)):
            listing = self.run(f'gui-list-{index}', [binary, '--list', '--ignored', '--format', 'terse'])
            names = [line.removesuffix(': test') for line in listing.read_text().splitlines()
                     if line.endswith(': test')]
            for name in names:
                count += 1
                self.run(f'gtk-{count:02d}', ['xvfb-run', '-a', '-s', '-screen 0 1600x1200x24', 'dbus-run-session', '--',
                                            binary, '--exact', name, '--ignored', '--test-threads=1'],
                         timeout=120, env={'GTK_A11Y': 'none', 'GDK_BACKEND': 'x11',
                                          'GSK_RENDERER': 'cairo',
                                          'SIGNALTTY_UI_EVIDENCE': str(self.output / 'screenshots')})
        if not count:
            raise RuntimeError('no ignored GTK tests discovered')
        self.report['display_tests'] = count
        self.run('refresh-benchmark', ['xvfb-run', '-a', '-s', '-screen 0 1600x1200x24', sys.executable,
                                      'scripts/bench-gui-refresh.py', '--check'],
                 env={'GTK_A11Y': 'none', 'GDK_BACKEND': 'x11', 'GSK_RENDERER': 'cairo'})
        if mode == 'desktop':
            self.run('native-accessibility', [sys.executable, 'scripts/qa-gui-reliability.py'])


def main():
    signal.signal(signal.SIGTERM, interrupted)
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('mode', choices=['doctor', 'fast', 'full', 'desktop'])
    parser.add_argument('--output', type=Path, help='new artifact directory (must not exist)')
    args = parser.parse_args()
    if args.output:
        output = args.output.resolve()
        try:
            output.mkdir(parents=True, exist_ok=False)
        except FileExistsError:
            parser.error(f'evidence directory already exists: {output}')
    else:
        base = ROOT / 'target' / 'verification'
        base.mkdir(parents=True, exist_ok=True)
        output = Path(tempfile.mkdtemp(prefix=f'{args.mode}-', dir=base))
    print(f'Evidence: {output}', flush=True)
    run = Verification(args.mode, output)
    try:
        errors, versions = doctor(args.mode)
        run.report.update(errors=errors, versions=versions)
        if errors:
            raise RuntimeError('environment diagnosis failed: ' + '; '.join(errors))
        if args.mode != 'doctor':
            run.checks(args.mode)
        run.report['status'] = 'passed'
    except (Exception, KeyboardInterrupt) as error:
        run.report['status'] = 'failed'
        run.report['errors'].append(str(error) or 'interrupted')
        print(f'FAIL: {error}', file=sys.stderr)
    finally:
        run.save()
    return run.report['status'] != 'passed'


if __name__ == '__main__':
    raise SystemExit(main())
