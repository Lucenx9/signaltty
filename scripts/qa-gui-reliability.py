#!/usr/bin/env python3
"""Real GTK reliability probes using an isolated server and a Unix relay.

Requires a display and Python GObject/AT-SPI. Run cargo build --workspace first.
Does not connect to the user's signaltty server. Prints JSON evidence.
"""
import argparse
import base64
import json
import os
from pathlib import Path
import select
import signal
import socket
import subprocess
import tempfile
import threading
import time

import gi

gi.require_version("Atspi", "2.0")
from gi.repository import Atspi, Gio, GLib


def until(probe, seconds=10):
    deadline = time.monotonic() + seconds
    while time.monotonic() < deadline:
        while GLib.MainContext.default().pending():
            GLib.MainContext.default().iteration(False)
        result = probe()
        if result:
            return result
        time.sleep(0.05)
    raise AssertionError("probe timed out")


class Relay:
    def __init__(self, proxy, server):
        self.server = str(server)
        self.offline = threading.Event()
        self.stopped = threading.Event()
        self.listener = socket.socket(socket.AF_UNIX)
        self.listener.bind(str(proxy))
        self.listener.listen()
        self.listener.settimeout(0.1)
        self.thread = threading.Thread(target=self.accept, daemon=True)
        self.thread.start()

    def accept(self):
        while not self.stopped.is_set():
            try:
                client, _ = self.listener.accept()
            except socket.timeout:
                continue
            except OSError:
                break
            threading.Thread(target=self.forward, args=(client,), daemon=True).start()

    def forward(self, client):
        with client, socket.socket(socket.AF_UNIX) as backend:
            try:
                if self.offline.is_set():
                    return
                backend.connect(self.server)
                while not self.offline.is_set() and not self.stopped.is_set():
                    readable, _, _ = select.select([client, backend], [], [], 0.05)
                    for source in readable:
                        data = source.recv(65536)
                        if not data:
                            return
                        (backend if source is client else client).sendall(data)
            except OSError:
                pass

    def close(self):
        self.stopped.set()
        self.listener.close()
        self.thread.join(timeout=1)


def walk(widget):
    widget.clear_cache()
    yield widget
    for child in widget:
        yield from walk(child)


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("--hold-seconds", type=float, default=0,
                        help="keep the final test window open for visual inspection")
    parser.add_argument("--screenshot", type=Path, help="capture the owned test window using KDE KWin/Spectacle")
    args = parser.parse_args()
    binaries = Path(__file__).resolve().parents[1] / "target/debug"
    bus = Gio.bus_get_sync(Gio.BusType.SESSION)
    results = []
    with tempfile.TemporaryDirectory(prefix="signaltty-gui-qa-") as tmp:
        root = Path(tmp)
        sock, proxy = root / "server.sock", root / "proxy.sock"
        log = (root / "server.log").open("w")
        gui_log = (root / "gui.log").open("w")
        server = gui = relay = None

        def start_server():
            return subprocess.Popen([str(binaries / "signaltty-server"), "--socket", str(sock),
                "--state-dir", str(root / "state"), "--plugin-dir", str(root / "plugins"),
                "--agents-dir", str(root / "agents")], stdout=log, stderr=log)

        def call(method, **params):
            with socket.socket(socket.AF_UNIX) as connection:
                connection.settimeout(3)
                connection.connect(str(sock))
                connection.sendall((json.dumps(dict(protocol="signaltty/1", id="qa", method=method, params=params)) + "\n").encode())
                response = json.loads(connection.makefile().readline())
                assert response["ok"], response
                return response["result"]

        def ready():
            try:
                return call("server.status")
            except (OSError, ValueError):
                return False

        def app():
            for item in Atspi.get_desktop(0):
                if item.get_process_id() == gui.pid:
                    return item
            return None

        def texts():
            target = app()
            if not target:
                return []
            return [Atspi.Text.get_text(w, 0, -1) for w in walk(target)
                    if "Text" in w.get_interfaces()]

        def terminal_text():
            target = app()
            if not target:
                return ""
            return "\n".join(Atspi.Text.get_text(w, 0, -1) for w in walk(target)
                if w.get_role_name() == "terminal")

        def gtk(method, parameters=None, timeout=1000):
            return bus.call_sync("dev.signaltty.gui", "/dev/signaltty/gui/window/1", "org.gtk.Actions",
                                 method, parameters, None, Gio.DBusCallFlags.NONE, timeout, None)

        def action(name):
            gtk("Activate", GLib.Variant("(sava{sv})", (name, [], {})))

        def input_to(pane, text):
            call("pane.input", pane_id=pane, data_b64=base64.b64encode(text.encode()).decode())

        def pane_get(pane):
            return call("pane.get", pane_id=pane)["pane"]

        def record(case, **evidence):
            result = dict(case=case, status="PASS", evidence=evidence)
            results.append(result)
            print(json.dumps(result), flush=True)

        def capture(path):
            assert gui.poll() is None, "test GUI already closed"
            focus_script = root / "focus.js"
            focus_script.write_text("const windows = workspace.windowList().filter(w => w.resourceClass === 'dev.signaltty.gui' || w.resourceClass === 'signaltty-gui'); if (windows.length) workspace.activeWindow = windows[windows.length - 1];")
            name = f"signaltty-qa-{gui.pid}"
            script_id = subprocess.check_output(["qdbus6", "org.kde.KWin", "/Scripting", "org.kde.kwin.Scripting.loadScript", str(focus_script), name], text=True).strip()
            try:
                subprocess.run(["qdbus6", "org.kde.KWin", f"/Scripting/Script{script_id}", "org.kde.kwin.Script.run"], check=True, stdout=subprocess.DEVNULL)
                assert gui.poll() is None
                subprocess.run(["spectacle", "--background", "--activewindow", "--nonotify", "--output", str(path.resolve())], check=True)
            finally:
                subprocess.run(["qdbus6", "org.kde.KWin", "/Scripting", "org.kde.kwin.Scripting.unloadScript", name], check=True, stdout=subprocess.DEVNULL)

        try:
            server = start_server()
            until(ready)
            ws = call("workspace.create", name="Reliability", cwd=tmp)["workspace"]["id"]
            pane = call("pane.spawn", workspace_id=ws, argv=["sh"], agent_hint="codex")["pane"]["id"]
            input_to(pane, "echo QASTART\n")
            relay = Relay(proxy, sock)
            gui = subprocess.Popen([str(binaries / "signaltty-gui"), "--socket", str(proxy)],
                stdout=gui_log, stderr=gui_log, env=dict(os.environ, SIGNALTTY_NOTIFY="0", SIGNALTTY_REFRESH_METRICS="1"))
            until(lambda: app() and "QASTART" in terminal_text())
            before = len(app())
            subprocess.run([str(binaries / "signaltty-gui"), "--socket", str(proxy)],
                           stdout=gui_log, stderr=gui_log, timeout=5, check=True)
            time.sleep(0.2)
            after = len(app())
            assert before == after == 1, (before, after)
            record("single_window_activation", before=before, after=after)

            other_ws = call("workspace.create", name="Approval", cwd=tmp)["workspace"]["id"]
            other = call("pane.spawn", workspace_id=other_ws, argv=["cat"], agent_hint="codex")["pane"]["id"]
            call("hook-event", agent="codex", event="PermissionRequest", pane_id=other,
                 decision=dict(id="qa-approval", prompt="QA approval needed", options=[dict(id="allow", label="Allow"), dict(id="deny", label="Deny")]))
            until(lambda: "Approval" in texts())
            action("next-attention")
            until(lambda: "QA approval needed" in texts())
            current = pane_get(other)
            assert current["pending_decision"]["id"] == "qa-approval" and current["attention"] == "permission_required", current
            allow = until(lambda: next((w for w in walk(app()) if "Action" in w.get_interfaces() and w.get_name() == "Allow"), None))
            Atspi.Action.do_action(allow, 0)
            until(lambda: pane_get(other).get("pending_decision") is None and pane_get(other)["attention"] == "none")
            record("approval_survives_focus_and_can_be_answered", decision_before_answer=current["pending_decision"], attention=current["attention"])

            # Keep the active approval terminal for reconnect, so the same VTE
            # must recover outage data. cat provides deterministic byte echo.
            input_to(other, "QARECONNECTA\n")
            until(lambda: "QARECONNECTA" in terminal_text())
            relay.offline.set()
            time.sleep(0.3)
            input_to(other, "QARECONNECTB\n")
            until(lambda: "QARECONNECTB" in call("pane.read", pane_id=other)["text"])
            relay.offline.clear()
            until(lambda: "QARECONNECTB" in terminal_text())
            input_to(other, "QAONLINE\n")
            content = until(lambda: terminal_text() if "QAONLINE" in terminal_text() else False)
            assert all(content.count(marker) == 2 for marker in ["QARECONNECTA", "QARECONNECTB", "QAONLINE"]), content
            record("reconnect_restores_terminal", terminal_text=content)

            os.kill(server.pid, signal.SIGSTOP)
            try:
                action("new-tab")
                started = time.monotonic()
                gtk("List")
                elapsed = time.monotonic() - started
                assert elapsed < 1, elapsed
            finally:
                os.kill(server.pid, signal.SIGCONT)
            record("stalled_server_keeps_gtk_responsive", seconds=elapsed)
            # Let the in-flight new tab finish before checking layout actions.
            until(lambda: len(call("workspace.get", workspace_id=other_ws)["panes"]) == 2)
            action("split-right")
            until(lambda: len(call("workspace.get", workspace_id=other_ws)["panes"]) == 3)
            action("close-pane")
            until(lambda: len(call("workspace.get", workspace_id=other_ws)["panes"]) == 2)
            record("split_and_close_after_recovery", owned_panes=2)

            restored_ws = call("workspace.create", name="Restored", cwd=tmp)["workspace"]["id"]
            restored = call("pane.spawn", workspace_id=restored_ws, argv=["sleep", "600"], agent_hint="codex")["pane"]["id"]
            call("hook-event", agent="codex", event="PreToolUse", pane_id=restored)
            until(lambda: "Working" in texts())
            call("server.shutdown", force=True)
            server.wait(timeout=5)
            server = start_server()
            until(ready)
            until(lambda: "Exited" in texts() and "Working" not in texts() and "Running" not in texts())
            current = pane_get(restored)
            assert current["live"]["state"] == "exited", current
            record("restored_status_is_exited", live=current["live"], saved_lifecycle=current["lifecycle"], ui_text=texts())
            print(json.dumps(dict(inspect_pid=gui.pid, temp_dir=tmp)), flush=True)
            if args.screenshot:
                capture(args.screenshot)
            if args.hold_seconds:
                time.sleep(args.hold_seconds)
        except Exception:
            if args.screenshot and gui and gui.poll() is None:
                capture(args.screenshot.with_name(args.screenshot.stem + "-failure.png"))
            print(json.dumps({"debug_ui": [(w.get_role_name(), w.get_name()) for w in walk(app())] if app() else [], "gui_log": (root / "gui.log").read_text(), "terminal_text": terminal_text(), "server_terminal": call("pane.read", pane_id=other) if "other" in locals() else None}), flush=True)
            raise
        finally:
            if server and server.poll() is None:
                os.kill(server.pid, signal.SIGCONT)
                try:
                    call("server.shutdown", force=True)
                    server.wait(timeout=5)
                except (OSError, ValueError, subprocess.TimeoutExpired):
                    server.kill()
                    server.wait()
            if gui and gui.poll() is None:
                gui.terminate()
                gui.wait(timeout=5)
            if relay:
                relay.close()
            log.close()
            gui_log.close()


if __name__ == "__main__":
    main()
