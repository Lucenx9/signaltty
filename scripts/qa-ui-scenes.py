#!/usr/bin/env python3
"""Isolated populated signaltty window for visual comparisons."""

import argparse
import json
import os
import socket
import subprocess
import tempfile
import time
from pathlib import Path

import gi

gi.require_version("Atspi", "2.0")
from gi.repository import Atspi, Gio, GLib

parser = argparse.ArgumentParser()
parser.add_argument("--output", type=Path, required=True)
parser.add_argument("--hold-seconds", type=float, default=0)
parser.add_argument("--width", type=int, default=1280)
parser.add_argument("--long-choice", action="store_true")
parser.add_argument("--font", help="isolated GTK UI font, e.g. 'Sans 18'")
args = parser.parse_args()
if args.width < 360:
    parser.error("the application window minimum width is 360 pixels")
bins = Path(__file__).resolve().parents[1] / "target" / "debug"
bus = Gio.bus_get_sync(Gio.BusType.SESSION)
owned = bus.call_sync(
    "org.freedesktop.DBus",
    "/org/freedesktop/DBus",
    "org.freedesktop.DBus",
    "NameHasOwner",
    GLib.Variant("(s)", ("dev.signaltty.gui",)),
    None,
    Gio.DBusCallFlags.NONE,
    1000,
    None,
).unpack()[0]
assert not owned, "signaltty is already open; do not drive another session"


def until(probe, timeout=10):
    end = time.monotonic() + timeout
    while time.monotonic() < end:
        while GLib.MainContext.default().pending():
            GLib.MainContext.default().iteration(False)
        try:
            value = probe()
            if value:
                return value
        except (OSError, ValueError):
            pass
        time.sleep(0.05)
    raise AssertionError("UI probe timed out")


def walk(widget):
    widget.clear_cache()
    yield widget
    for child in widget:
        yield from walk(child)


with tempfile.TemporaryDirectory(prefix="signaltty-ui-") as tmp:
    root = Path(tmp)
    config = root / "config" / "gtk-4.0"
    config.mkdir(parents=True)
    if args.font:
        (config / "settings.ini").write_text(f"[Settings]\ngtk-font-name={args.font}\n")
    sock = root / "server.sock"
    server = gui = None
    with (
        (root / "server.log").open("w") as server_log,
        (root / "gui.log").open("w") as gui_log,
    ):

        def call(method, **params):
            with socket.socket(socket.AF_UNIX) as conn:
                conn.settimeout(3)
                conn.connect(str(sock))
                conn.sendall(
                    (
                        json.dumps(
                            {
                                "protocol": "signaltty/1",
                                "id": "ui",
                                "method": method,
                                "params": params,
                            }
                        )
                        + "\n"
                    ).encode()
                )
                response = json.loads(conn.makefile().readline())
                assert response["ok"], response
                return response["result"]

        def app():
            return next(
                (w for w in Atspi.get_desktop(0) if w.get_process_id() == gui.pid), None
            )

        def text():
            a = app()
            return (
                [
                    Atspi.Text.get_text(w, 0, -1)
                    for w in walk(a)
                    if "Text" in w.get_interfaces()
                ]
                if a
                else []
            )

        def workspace(name, branch, title, content, agent="codex"):
            cwd = root / name
            cwd.mkdir()
            subprocess.run(
                ["git", "init", "--quiet", "-b", branch, str(cwd)], check=True
            )
            ws = call("workspace.create", name=name, cwd=str(cwd))["workspace"]["id"]
            script = "printf '%s' \"$1\"; exec cat"
            output = f"\x1b]0;{title}\x07\x1b[1m{title}\x1b[0m\r\n\r\n{content}\r\n"
            pane = call(
                "pane.spawn",
                workspace_id=ws,
                argv=["sh", "-c", script, "fixture", output],
                agent_hint=agent,
            )["pane"]["id"]
            return ws, pane

        try:
            server = subprocess.Popen(
                [
                    str(bins / "signaltty-server"),
                    "--socket",
                    str(sock),
                    "--state-dir",
                    str(root / "state"),
                    "--plugin-dir",
                    str(root / "plugins"),
                    "--agents-dir",
                    str(root / "agents"),
                ],
                stdout=server_log,
                stderr=server_log,
            )
            until(lambda: call("server.status"))
            api, api_pane = workspace(
                "api-service",
                "fix/session-recovery",
                "Claude",
                "Inspecting session recovery and reconnect handling.\r\n\r\nTerminal output remains owned by the server.",
                "claude",
            )
            call(
                "hook-event",
                agent="claude",
                event="PreToolUse",
                pane_id=api_pane,
                message="Reviewing session recovery",
            )
            docs, docs_pane = workspace(
                "documentation",
                "docs/agent-workflows",
                "Codex",
                "Updated the workspace and agent guides.\r\n\r\nReady for review.",
            )
            call(
                "hook-event",
                agent="codex",
                event="Stop",
                pane_id=docs_pane,
                message="Updated workspace and agent guides",
            )
            ws, pane = workspace(
                "signaltty",
                "ui/workspace-refinement",
                "Codex",
                "Reviewing the native workspace interface.\r\n\r\n  Read the full approval question\r\n  Keep attention visible while navigating\r\n  Reach controls with the keyboard\r\n\r\nWaiting for permission to run the verification suite.",
            )
            second = call(
                "pane.split",
                pane_id=pane,
                direction="right",
                argv=[
                    "sh",
                    "-c",
                    "printf '\033]0;Verification\007\033[1mVerification\033[0m\n\nIsolated terminal fixture\n\nWorkspace tests\nDisplay checks\nNative light and dark inspection\n\n'; exec cat",
                ],
            )["pane"]["id"]
            if args.width < 760:
                call("pane.close", pane_id=second)
            call(
                "hook-event",
                agent="codex",
                event="PermissionRequest",
                pane_id=pane,
                message="Run the workspace verification suite?",
                decision={
                    "id": "ui-approval",
                    "prompt": "Run the workspace verification suite and inspect the native interface in light and dark themes?",
                    "options": [
                        {"id": "once", "label": "Allow once"},
                        {
                            "id": "always",
                            "label": "Allow this verification command for the current workspace session"
                            if args.long_choice
                            else "Allow for this session",
                        },
                        {"id": "deny", "label": "Deny"},
                    ],
                },
            )
            gui_env = dict(os.environ, SIGNALTTY_NOTIFY="0")
            if args.font:
                gui_env.update(
                    XDG_CONFIG_HOME=str(config.parent), GDK_DEBUG="no-portals"
                )
            gui = subprocess.Popen(
                [str(bins / "signaltty-gui"), "--socket", str(sock)],
                stdout=gui_log,
                stderr=gui_log,
                env=gui_env,
            )
            until(lambda: app() and "signaltty" in text())
            bus.call_sync(
                "dev.signaltty.gui",
                "/dev/signaltty/gui/window/1",
                "org.gtk.Actions",
                "Activate",
                GLib.Variant("(sava{sv})", ("next-attention", [], {})),
                None,
                Gio.DBusCallFlags.NONE,
                1000,
                None,
            )
            until(
                lambda: any(
                    "Run the workspace verification suite and inspect" in t
                    for t in text()
                )
            )
            script = root / "focus.js"
            script.write_text(
                "const windows = workspace.windowList().filter(w => w.pid === "
                + str(gui.pid)
                + "); if (windows.length === 1) { const w = windows[0]; const g = w.frameGeometry; w.frameGeometry = {x: g.x, y: g.y, width: "
                + str(args.width)
                + ", height: 800}; workspace.activeWindow = w; }"
            )
            name = f"signaltty-ui-{gui.pid}"
            script_id = subprocess.check_output(
                [
                    "qdbus6",
                    "org.kde.KWin",
                    "/Scripting",
                    "org.kde.kwin.Scripting.loadScript",
                    str(script),
                    name,
                ],
                text=True,
            ).strip()
            try:
                subprocess.run(
                    [
                        "qdbus6",
                        "org.kde.KWin",
                        f"/Scripting/Script{script_id}",
                        "org.kde.kwin.Script.run",
                    ],
                    check=True,
                    stdout=subprocess.DEVNULL,
                )
                time.sleep(0.3)
                assert gui.poll() is None
                frame = until(
                    lambda: next(
                        (w for w in walk(app()) if w.get_role() == Atspi.Role.FRAME),
                        None,
                    )
                )
                until(
                    lambda: (
                        frame.get_state_set().contains(Atspi.StateType.ACTIVE)
                        and Atspi.Component.get_extents(
                            frame, Atspi.CoordType.WINDOW
                        ).width
                        == args.width
                    )
                )
                size = Atspi.Component.get_extents(frame, Atspi.CoordType.WINDOW)
                args.output.parent.mkdir(parents=True, exist_ok=True)
                subprocess.run(
                    [
                        "spectacle",
                        "--background",
                        "--activewindow",
                        "--nonotify",
                        "--output",
                        str(args.output.resolve()),
                    ],
                    check=True,
                )
                print(
                    json.dumps(
                        {
                            "pid": gui.pid,
                            "socket": str(sock),
                            "pane": pane,
                            "screenshot": str(args.output.resolve()),
                            "window_size": [size.width, size.height],
                            "font": args.font,
                            "texts": text(),
                        }
                    ),
                    flush=True,
                )
                if args.hold_seconds:
                    time.sleep(args.hold_seconds)
            finally:
                subprocess.run(
                    [
                        "qdbus6",
                        "org.kde.KWin",
                        "/Scripting",
                        "org.kde.kwin.Scripting.unloadScript",
                        name,
                    ],
                    check=True,
                    stdout=subprocess.DEVNULL,
                )
        finally:
            if server and server.poll() is None:
                try:
                    call("server.shutdown", force=True)
                    server.wait(timeout=5)
                except (OSError, subprocess.TimeoutExpired):
                    server.kill()
                    server.wait()
            if gui and gui.poll() is None:
                gui.terminate()
                gui.wait(timeout=5)
