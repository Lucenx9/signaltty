#!/usr/bin/env python3
"""Probe state-integrity failures against an isolated real server.

Run cargo build --workspace first. Prints JSON evidence, then exits 1 if a
probe fails. Does not connect to the user's server or change user state.
"""

import json
import os
from pathlib import Path
import signal
import socket
import subprocess
import tempfile
import time


def main():
    binary = Path(__file__).resolve().parents[1] / "target/debug/signaltty-server"
    failures = []
    with tempfile.TemporaryDirectory(prefix="signaltty-qa-") as tmp:
        root = Path(tmp)
        sock = root / "sock"

        def call(method, **params):
            request = dict(protocol="signaltty/1", id="qa", method=method, params=params)
            with socket.socket(socket.AF_UNIX) as control:
                control.settimeout(3)
                control.connect(str(sock))
                control.sendall((json.dumps(request) + "\n").encode())
                return json.loads(control.makefile().readline())

        def ok(method, **params):
            response = call(method, **params)
            assert response["ok"], response
            return response["result"]

        def workspace(name):
            return ok("workspace.create", name=name, cwd=tmp)["workspace"]["id"]

        def spawn(ws):
            return ok("pane.spawn", workspace_id=ws, argv=["sleep", "600"])["pane"]

        def leaf(pane):
            return dict(type="pane", pane_id=pane["id"])

        def record(name, passed, evidence):
            print(json.dumps(dict(case=name, status="PASS" if passed else "FAIL",
                                  evidence=evidence)), flush=True)
            if not passed:
                failures.append(name)

        with (root / "server.log").open("w") as log:
            server = subprocess.Popen(
                [str(binary), "--socket", str(sock), "--state-dir", str(root / "state"),
                 "--plugin-dir", str(root / "plugins"), "--agents-dir", str(root / "agents")],
                stdout=log, stderr=log, start_new_session=True,
            )
            try:
                deadline = time.monotonic() + 10
                while True:
                    assert server.poll() is None, (root / "server.log").read_text()
                    try:
                        ok("server.status")
                        break
                    except OSError:
                        assert time.monotonic() < deadline, "server startup timed out"
                        time.sleep(0.025)

                a, b = workspace("foreign-a"), workspace("foreign-b")
                tab = ok("tab.create", workspace_id=b)["tab"]["id"]
                response = call("pane.spawn", workspace_id=a, tab_id=tab,
                                argv=["sleep", "600"])
                record("reject_foreign_tab", not response["ok"], response)
                if response["ok"]:
                    pane = response["result"]["pane"]
                    ok("workspace.close", workspace_id=a)
                    after = call("pane.get", pane_id=pane["id"])
                    record("closing_workspace_stops_its_panes", not after["ok"], after)

                ws = workspace("omitted-layout")
                pane = spawn(ws)
                sibling = ok("pane.split", pane_id=pane["id"], argv=["sleep", "600"])["pane"]
                response = call("tab.set_layout", tab_id=pane["tab_id"], layout=leaf(pane))
                ok("workspace.close", workspace_id=ws)
                after = call("pane.get", pane_id=sibling["id"])
                record("layout_cannot_orphan_live_panes", not response["ok"] and not after["ok"],
                       dict(set_layout=response, after_close=after))

                pane = spawn(workspace("duplicate-layout"))
                layout = dict(type="split", dir="right", ratio=0.5,
                              first=leaf(pane), second=leaf(pane))
                response = call("tab.set_layout", tab_id=pane["tab_id"], layout=layout)
                record("reject_duplicate_layout_panes", not response["ok"], response)

                pane = spawn(workspace("ratio-layout"))
                sibling = ok("pane.split", pane_id=pane["id"], argv=["sleep", "600"])["pane"]
                layout = dict(type="split", dir="right", ratio=3,
                              first=leaf(pane), second=leaf(sibling))
                response = call("tab.set_layout", tab_id=pane["tab_id"], layout=layout)
                ratio = response.get("result", {}).get("tab", {}).get("layout", {}).get("ratio")
                record("layout_ratio_cannot_collapse_panes",
                       not response["ok"] or 0.05 <= ratio <= 0.95, response)

                ws = workspace("spawn-failure")
                before = ok("workspace.get", workspace_id=ws)
                response = call("pane.spawn", workspace_id=ws,
                                argv=[str(root / "missing-command")])
                after = ok("workspace.get", workspace_id=ws)
                record("failed_spawn_is_atomic", not response["ok"] and before["tabs"] == after["tabs"],
                       dict(error=response, tabs_before=before["tabs"], tabs_after=after["tabs"]))
            finally:
                try:
                    call("server.shutdown", force=True)
                except (OSError, ValueError):
                    pass
                try:
                    server.wait(timeout=5)
                except subprocess.TimeoutExpired:
                    os.killpg(server.pid, signal.SIGKILL)
                    server.wait()
    print(json.dumps(dict(failed=len(failures), cases=failures)))
    return 1 if failures else 0


if __name__ == "__main__":
    raise SystemExit(main())
