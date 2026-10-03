#!/usr/bin/env python3
"""Validate Cargo metadata or Clippy JSON output against repository policy."""
import argparse
import json
from pathlib import Path
import sys

ROOT = Path(__file__).resolve().parents[1]
PURE = {
    'signaltty-core': {'serde', 'serde_json', 'uuid', 'chrono', 'thiserror'},
    'signaltty-proto': {'serde', 'serde_json', 'signaltty-core', 'thiserror'},
}
TOOLKIT = {'gtk4', 'gtk4-sys', 'gdk4', 'gdk4-sys', 'glib', 'glib-sys',
           'gio', 'gio-sys', 'libadwaita', 'libadwaita-sys', 'vte4', 'vte4-sys'}


def architecture(value):
    packages = {p['id']: p for p in value['packages']}
    edges = {n['id']: [d['pkg'] for d in n['deps']
                      if any(k['kind'] != 'dev' for k in d['dep_kinds'])]
             for n in value['resolve']['nodes']}
    errors = []
    if not value['workspace_members']:
        raise ValueError('metadata contains no workspace members')
    for member in value['workspace_members']:
        name = packages[member]['name']
        direct = {packages[d]['name'] for d in edges[member]}
        if name in PURE:
            for dependency in sorted(direct - PURE[name]):
                errors.append(f'{name}: forbidden direct dependency {dependency}')
        todo, visited = list(edges[member]), set()
        while todo:
            dependency = todo.pop()
            if dependency in visited:
                continue
            visited.add(dependency)
            dep_name = packages[dependency]['name']
            if name != 'signaltty-gui' and dep_name in TOOLKIT:
                errors.append(f'{name}: reaches GUI toolkit {dep_name}')
            if name in {'signaltty-gui', 'signaltty-cli'} and dep_name in {
                    'signaltty-server', 'portable-pty'}:
                errors.append(f'{name}: reaches server-owned dependency {dep_name}')
            todo.extend(edges[dependency])
    return errors


def clippy(path):
    baseline = json.loads((ROOT / 'scripts/clippy-baseline.json').read_text())
    allowed = {tuple(item): False for item in baseline}
    warnings, errors, finished = set(), [], False
    for line in path.read_text().splitlines():
        value = json.loads(line)
        if value.get('reason') == 'build-finished':
            finished = value['success']
        if value.get('reason') != 'compiler-message':
            continue
        message = value['message']
        if message['level'] == 'error':
            errors.append(message['message'])
        if message['level'] != 'warning':
            continue
        if not any(span['is_primary'] for span in message['spans']):
            errors.append(f'unapproved warning without location: {message["message"]}')
        for span in message['spans']:
            if span['is_primary']:
                key = ((message.get('code') or {}).get('code'), span['file_name'],
                       span.get('line_start'), message['message'])
                warnings.add(key)
    for warning in sorted(warnings, key=str):
        if warning in allowed:
            allowed[warning] = True
        else:
            errors.append(f'unapproved warning: {warning}')
    if not finished:
        errors.append('missing successful Cargo build-finished record')
    for warning, seen in allowed.items():
        if not seen:
            print(f'baseline no longer observed; remove or review entry: {warning}')
    return errors


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('check', choices=['architecture', 'clippy'])
    parser.add_argument('input', type=Path)
    args = parser.parse_args()
    try:
        errors = (architecture(json.loads(args.input.read_text()))
                  if args.check == 'architecture' else clippy(args.input))
    except (OSError, ValueError, KeyError, TypeError) as error:
        print(f'invalid {args.check} input: {error}', file=sys.stderr)
        return 1
    for error in errors:
        print(error, file=sys.stderr)
    if not errors:
        print(f'{args.check}: PASS')
    return bool(errors)


if __name__ == '__main__':
    raise SystemExit(main())
