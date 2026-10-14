"""Drive the real attach CLI in a controlling terminal against the test server."""

import json
import os
import pty
import select
import signal
import socket
import sys
import termios
import time

binary, socket_path, address, pane_id, action = sys.argv[1:]


def call(method, params):
    with socket.socket(socket.AF_UNIX) as connection:
        connection.settimeout(5)
        connection.connect(socket_path)
        connection.sendall(
            (json.dumps({"protocol": "signaltty/1", "id": "1", "method": method, "params": params}) + "\n").encode()
        )
        response = json.loads(connection.makefile().readline())
        assert response["ok"], response
        return response["result"]


pid, terminal = pty.fork()
if pid == 0:
    os.execv(binary, [binary, "--socket", socket_path, "pane", "attach", address])

output = bytearray()


def read_output():
    if select.select([terminal], [], [], 0.02)[0]:
        try:
            output.extend(os.read(terminal, 65536))
        except OSError:
            pass


def wait_for(predicate, message, timeout=3):
    deadline = time.monotonic() + timeout
    while time.monotonic() < deadline:
        read_output()
        if predicate():
            return
    raise AssertionError(f"{message}: {output!r}")


try:
    wait_for(
        lambda: not termios.tcgetattr(terminal)[3] & termios.ICANON,
        "attach did not enter raw mode",
        timeout=10,
    )
    if action == "close":
        call("pane.close", {"pane_id": pane_id})
    elif action == "exit":
        os.write(terminal, b"\x04")
    elif action == "rename":
        pane = call("pane.get", {"pane_id": pane_id})["pane"]
        call("pane.rename", {"pane_id": pane_id, "name": "renamed"})
        tab = call("tab.create", {"workspace_id": pane["workspace_id"]})["tab"]["id"]
        replacement = call("pane.spawn", {"workspace_id": pane["workspace_id"], "tab_id": tab, "argv": ["cat"]})["pane"]["id"]
        call("pane.rename", {"pane_id": replacement, "name": address})
        os.write(terminal, b"pinned-input-marker\n")
        wait_for(lambda: b"pinned-input-marker" in output, "input no longer reached the attached pane")
        call("pane.close", {"pane_id": pane_id})
    else:
        raise AssertionError(action)

    status = []

    def exited():
        child, result = os.waitpid(pid, os.WNOHANG)
        if child:
            status.append(result)
        return bool(child)

    wait_for(exited, "attach stayed open after its pane ended")
    pid = None
    assert os.waitstatus_to_exitcode(status[0]) == 0, output
finally:
    if pid is not None:
        os.kill(pid, signal.SIGKILL)
        os.waitpid(pid, 0)
    os.close(terminal)
