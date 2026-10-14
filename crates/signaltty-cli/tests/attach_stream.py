"""Exercise the real attach CLI with deterministic output delivery faults."""

import base64
import json
import os
import pty
import select
import signal
import socket
import sys
import tempfile
import termios
import time

binary = sys.argv[1]

for scenario in ("gap", "contiguous", "overlap"):
    with tempfile.TemporaryDirectory(prefix="signaltty-attach-stream-") as directory:
        path = os.path.join(directory, "server.sock")
        with socket.socket(socket.AF_UNIX) as listener:
            listener.bind(path)
            listener.listen()
            listener.settimeout(5)
            pid, terminal = pty.fork()
            if pid == 0:
                os.execv(binary, [binary, "--socket", path, "pane", "attach", "pane_test"])
            output = bytearray()
            try:
                with listener.accept()[0] as connection:
                    connection.settimeout(5)
                    requests = connection.makefile("rb")
                    for method, result in (
                        ("pane.get", {"pane": {"id": "pane_test"}}),
                        ("subscribe", {"subscribed": True}),
                        ("pane.attach", {
                            "snapshot_b64": base64.b64encode(b"BEFORE").decode(),
                            "output_offset": 6,
                            "live": {"state": "live"},
                        }),
                    ):
                        request = json.loads(requests.readline())
                        assert request["method"] == method, request
                        connection.sendall((json.dumps({
                            "protocol": "signaltty/1", "type": "response",
                            "id": request["id"], "ok": True, "result": result,
                        }) + "\n").encode())
                    with listener.accept()[0]:
                        data, end = {
                            "gap": (b"AFTER", 12),
                            "contiguous": (b"AFTER", 11),
                            "overlap": (b"FOREAFTER", 11),
                        }[scenario]
                        connection.sendall((json.dumps({
                            "protocol": "signaltty/1", "type": "event", "event": "pty.data",
                            "payload": {"pane_id": "pane_test", "output_offset": end,
                                        "data_b64": base64.b64encode(data).decode()},
                        }) + "\n").encode())
                        if scenario != "gap":
                            connection.sendall((json.dumps({
                                "protocol": "signaltty/1", "type": "event", "event": "pane.exited",
                                "payload": {"pane_id": "pane_test"},
                            }) + "\n").encode())
                        deadline = time.monotonic() + 3
                        status = None
                        while time.monotonic() < deadline:
                            if select.select([terminal], [], [], 0.02)[0]:
                                try:
                                    output.extend(os.read(terminal, 65536))
                                except OSError:
                                    pass
                            child, result = os.waitpid(pid, os.WNOHANG)
                            if child:
                                pid = None
                                status = result
                                break
                        assert status is not None, f"{scenario}: attach continued after output loss: {output!r}"
                        while select.select([terminal], [], [], 0.1)[0]:
                            try:
                                chunk = os.read(terminal, 65536)
                            except OSError:
                                break
                            if not chunk:
                                break
                            output.extend(chunk)
                        assert termios.tcgetattr(terminal)[3] & termios.ICANON, output
                        code = os.waitstatus_to_exitcode(status)
                        if scenario == "gap":
                            assert code != 0, output
                            assert b"output gap" in output and b"attach again" in output, output
                            assert b"AFTER" not in output, output
                        else:
                            assert code == 0, output
                            assert output.count(b"BEFOREAFTER") == 1, output
            finally:
                if pid is not None:
                    os.kill(pid, signal.SIGKILL)
                    os.waitpid(pid, 0)
                os.close(terminal)
