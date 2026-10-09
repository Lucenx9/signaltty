#!/usr/bin/env python3
"""Measure the real GTK client's IPC traffic against an isolated server.

Requires a graphical session (or xvfb-run), dbus-run-session, and cargo build
--workspace. --check rejects per-event full refreshes in the message burst.
"""
import argparse
from collections import Counter
import json
import os
from pathlib import Path
import re
import signal
import socket
import subprocess
import tempfile
import time
from qa_evidence import preserve_logs


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--gui", default="target/debug/signaltty-gui")
    parser.add_argument("--workspaces", type=int, default=8)
    parser.add_argument("--check", action="store_true")
    args = parser.parse_args()
    if args.workspaces < 1:
        parser.error("--workspaces must be positive")
    binary = Path("target/debug").resolve()
    with tempfile.TemporaryDirectory(prefix="signaltty-refresh-") as tmp:
        root = Path(tmp)
        sock = str(root / "sock")
        log = root / "gui.log"
        processes = []

        def cli(*argv):
            return json.loads(subprocess.check_output(
                [str(binary / "signaltty"), "--socket", sock, "--json", *argv],
                text=True, timeout=10))

        def settled():
            deadline = time.monotonic() + 15
            previous, since = None, time.monotonic()
            while time.monotonic() < deadline:
                size = log.stat().st_size
                if size != previous:
                    previous, since = size, time.monotonic()
                if time.monotonic() - since > 0.5:
                    assert gui.poll() is None, log.read_text()
                    return log.read_text()
                time.sleep(0.025)
            raise AssertionError("GUI did not settle")

        def measure(label, pane, message=False):
            before = len(settled())
            for i in range(200):
                extra = ["--message", f"tool {i}"] if message else []
                cli("hook-event", "--agent", "claude", "--event", "PreToolUse",
                    "--pane", pane, *extra)
            trace = settled()[before:]
            ipc = Counter(re.findall(r"\[refresh-metrics\] ipc=\d+ name=(\S+)", trace))
            events = Counter(re.findall(r"\[refresh-metrics\] event=\d+ name=(\S+)", trace))
            refresh = ipc["workspace.list"] + ipc["workspace.get"]
            count = sum(events.values())
            result = dict(case=label, hooks=200, workspaces=args.workspaces,
                          events=dict(events), ipc=dict(ipc), refresh_ipc=refresh,
                          refresh_ipc_per_event=round(refresh / count, 4) if count else None)
            print(json.dumps(result), flush=True)
            assert count, "No server events reached the GUI"
            if args.check:
                assert ipc["workspace.list"] == 0, "Event triggered a full refresh"
                if message:
                    assert events["notification.created"] == 200, "Lost events"
                    assert refresh < 200, "Events were not coalesced"
            return result

        with (root / "server.log").open("w") as server_log, log.open("w") as gui_log:
            try:
                server = subprocess.Popen([str(binary / "signaltty-server"),
                    "--socket", sock, "--state-dir", str(root / "state"),
                    "--plugin-dir", str(root / "plugins")], stdout=server_log, stderr=server_log, start_new_session=True,
                    env={**os.environ, "SIGNALTTY_LOGIND": "0"})
                processes.append(server)
                deadline = time.monotonic() + 10
                while not Path(sock).exists():
                    assert server.poll() is None, (root / "server.log").read_text()
                    assert time.monotonic() < deadline, "Server startup timed out"
                    time.sleep(0.025)
                for i in range(args.workspaces):
                    created = cli("new", "--name", f"bench-{i}", "--cwd", tmp,
                                  "--", "sleep", "600")
                pane = created["pane_id"]
                env = dict(os.environ, SIGNALTTY_REFRESH_METRICS="1", SIGNALTTY_NOTIFY="0")
                gui = subprocess.Popen(["dbus-run-session", "--", str(Path(args.gui).resolve()),
                    "--socket", sock], env=env, stdout=gui_log, stderr=gui_log, start_new_session=True)
                processes.append(gui)
                deadline = time.monotonic() + 15
                while "name=workspace.get" not in log.read_text():
                    assert gui.poll() is None, log.read_text()
                    assert time.monotonic() < deadline, log.read_text()
                    time.sleep(0.025)
                # Let mapping, focus and the initial terminal resize settle.
                time.sleep(2)
                measure("PreToolUse", pane)
                measure("PreToolUse with message", pane, message=True)
            finally:
                if Path(sock).exists():
                    # Only the isolated server and its sleep children are stopped.
                    try:
                        request = {"protocol": "signaltty/1", "id": "shutdown",
                                   "method": "server.shutdown", "params": {"force": True}}
                        with socket.socket(socket.AF_UNIX) as control:
                            control.settimeout(5)
                            control.connect(sock)
                            control.sendall((json.dumps(request) + "\n").encode())
                            control.recv(65536)
                    except (OSError, subprocess.CalledProcessError):
                        pass
                for process in reversed(processes):
                    try:
                        os.killpg(process.pid, signal.SIGTERM)
                    except ProcessLookupError:
                        pass
                    try:
                        process.wait(timeout=5)
                    except subprocess.TimeoutExpired:
                        os.killpg(process.pid, signal.SIGKILL)
                        process.wait()
                server_log.flush()
                gui_log.flush()
                preserve_logs(root, 'refresh')


if __name__ == "__main__":
    main()
