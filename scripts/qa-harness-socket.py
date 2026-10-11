#!/usr/bin/env python3
"""Prove restricted Codex shell + host-owned MCP against an isolated server.

Requires installed codex and built signaltty CLI/server. No authenticated model
turns, user configuration changes or default sockets. This models the harness's
stdio process boundary; it does not prove an authenticated provider tool turn.
"""
import json
import os
from pathlib import Path
import select
import shutil
import socket
import subprocess
import tempfile
import time
from qa_evidence import preserve_logs


def main():
    codex = shutil.which('codex')
    assert codex, 'codex CLI required for native sandbox proof'
    binaries = Path(__file__).resolve().parents[1] / 'target/debug'
    with tempfile.TemporaryDirectory(prefix='signaltty-mcp-', dir='/tmp') as tmp:
        root = Path(tmp)
        sock = root / 'sock'
        env = {**os.environ, 'CODEX_HOME': str(root / '.codex'), 'TMPDIR': '/tmp',
               'SIGNALTTY_LOGIND': '0'}
        (root / '.codex').mkdir()
        bridge = None
        with (root / 'server.log').open('w') as log:
            server = subprocess.Popen([str(binaries / 'signaltty-server'), '--socket', str(sock),
                '--state-dir', str(root / 'state'), '--plugin-dir', str(root / 'plugins'),
                '--agents-dir', str(root / 'agents')], stdout=log, stderr=log, env=env)
            def ipc(method, **params):
                with socket.socket(socket.AF_UNIX) as client:
                    client.settimeout(5)
                    client.connect(str(sock))
                    client.sendall((json.dumps(dict(protocol='signaltty/1', id='qa',
                        method=method, params=params)) + '\n').encode())
                    response = json.loads(client.makefile().readline())
                    assert response['ok'], response
                    return response['result']
            try:
                deadline = time.monotonic() + 10
                while True:
                    assert server.poll() is None, (root / 'server.log').read_text()
                    try:
                        ipc('server.status')
                        break
                    except OSError:
                        assert time.monotonic() < deadline, 'server startup timeout'
                        time.sleep(.025)
                ws = ipc('workspace.create', cwd=tmp)['workspace']['id']
                pane = ipc('pane.spawn', workspace_id=ws, argv=['sleep', '60'])['pane']['id']
                env.update(SIGNALTTY_SOCKET=str(sock), SIGNALTTY_PANE=pane)
                def denied():
                    code = ('import socket; s=socket.socket(socket.AF_UNIX); '
                            f's.connect({str(sock)!r})')
                    probe = subprocess.run([codex, 'sandbox', '-c', 'sandbox_mode="workspace-write"',
                        '--', 'python3', '-c', code], env=env, cwd=tmp, capture_output=True,
                        text=True, timeout=20)
                    assert probe.returncode != 0 and 'Operation not permitted' in probe.stderr, probe.stderr
                    return True
                before = denied()
                guidance = subprocess.run([codex, 'sandbox', '-c', 'sandbox_mode="workspace-write"',
                    '--', str(binaries / 'signaltty'), 'status'], env=env, cwd=tmp,
                    capture_output=True, text=True, timeout=20)
                assert guidance.returncode != 0 and 'signaltty_context' in guidance.stderr, guidance.stderr
                # Host-owned process, as a stdio MCP harness launches it.
                bridge = subprocess.Popen([str(binaries / 'signaltty'), 'mcp', 'serve'], env=env,
                    stdin=subprocess.PIPE, stdout=subprocess.PIPE, stderr=log)
                def mcp(method, params):
                    bridge.stdin.write((json.dumps(dict(jsonrpc='2.0', id=1, method=method,
                        params=params)) + '\n').encode())
                    bridge.stdin.flush()
                    assert select.select([bridge.stdout], [], [], 10)[0], 'MCP response timeout'
                    response = json.loads(bridge.stdout.readline())
                    assert 'error' not in response, response
                    return response['result']
                mcp('initialize', dict(protocolVersion='2025-06-18', capabilities={},
                    clientInfo=dict(name='native-qa', version='1')))
                context = mcp('tools/call', dict(name='signaltty_context', arguments={}))
                assert context['structuredContent']['pane']['id'] == pane, context
                def tool(method, **params):
                    result = mcp('tools/call', dict(name='signaltty_call', arguments=dict(
                        method=method, params=params)))
                    assert not result['isError'], result
                    return result['structuredContent']
                worker = tool('pane.split', pane_id=pane, direction='right',
                    argv=['sh', '-c', "printf 'native MCP worker reached host'; sleep 60"])['pane']['id']
                output = tool('pane.wait_for_output', pane_id=worker,
                    match='native MCP worker reached host', timeout_s=5)['text']
                assert 'native MCP worker reached host' in output, output
                after = denied()
                # Validate registration with the native parser, without running a model.
                subprocess.run([str(binaries / 'signaltty'), 'integration', 'install', 'codex',
                    '--home', tmp], env=env, capture_output=True, check=True, timeout=10)
                registration = subprocess.run([codex, 'mcp', 'get', 'signaltty', '--json'],
                    env=env, capture_output=True, text=True, check=True, timeout=10)
                config = json.loads(registration.stdout)
                assert config['transport']['command'] == str(binaries / 'signaltty'), config
                print(json.dumps(dict(case='native_codex_sandbox_mcp', status='PASS',
                    codex_version=subprocess.check_output([codex, '--version'], text=True).strip(),
                    denied_before=before, denied_after=after, context_matches=True,
                    worker_output=output, native_config_accepted=True, permission_guidance=True)))
            finally:
                if bridge is not None:
                    if bridge.stdin:
                        bridge.stdin.close()
                    try:
                        bridge.wait(timeout=5)
                    except subprocess.TimeoutExpired:
                        bridge.kill()
                        bridge.wait(timeout=5)
                if server.poll() is None:
                    try:
                        ipc('server.shutdown', force=True)
                        server.wait(timeout=5)
                    except (OSError, AssertionError, subprocess.TimeoutExpired):
                        server.terminate()
                        try:
                            server.wait(timeout=5)
                        except subprocess.TimeoutExpired:
                            server.kill()
                            server.wait(timeout=5)
                preserve_logs(root, "harness-socket")


if __name__ == '__main__':
    main()
