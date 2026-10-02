import base64
import json
import os
import pathlib
import re
import shlex
import socket
import subprocess
import tempfile
import time

ROOT = pathlib.Path(tempfile.mkdtemp(prefix="signaltty-native-approval-"))
BIN = pathlib.Path(__file__).resolve().parents[2] / "target/debug/signaltty-server"
if not BIN.is_file():
    raise SystemExit("Run cargo build -p signaltty-server first")
SOCK = ROOT / "probe.sock"
print(json.dumps({"root": str(ROOT)}), flush=True)
log = (ROOT / "server.log").open("w")
probe_env = {**os.environ, "DISABLE_UPDATES": "1"}
version = subprocess.check_output(
    ["claude", "--version"], env=probe_env, text=True
).strip()
print(json.dumps({"cli_version": version}), flush=True)
server = subprocess.Popen(
    [
        str(BIN),
        "--socket",
        str(SOCK),
        "--state-dir",
        str(ROOT / "state"),
        "--plugin-dir",
        str(ROOT / "plugins"),
    ],
    stdout=log,
    stderr=log,
    env=probe_env,
)


def call(method, params):
    with socket.socket(socket.AF_UNIX, socket.SOCK_STREAM) as s:
        s.settimeout(5)
        s.connect(str(SOCK))
        s.sendall(
            (
                json.dumps(
                    {
                        "protocol": "signaltty/1",
                        "id": "probe",
                        "method": method,
                        "params": params,
                    }
                )
                + "\n"
            ).encode()
        )
        with s.makefile("rb") as f:
            while True:
                reply = json.loads(f.readline())
                if reply.get("type") == "event":
                    continue
                if not reply["ok"]:
                    raise RuntimeError(reply)
                return reply["result"]


def send(pane, data):
    return call(
        "pane.input", {"pane_id": pane, "data_b64": base64.b64encode(data).decode()}
    )


panes = []
try:
    for i in range(50):
        if SOCK.exists():
            break
        if server.poll() is not None:
            raise RuntimeError("server exited")
        time.sleep(0.1)
    ws = call(
        "workspace.create", {"name": "isolated approval probe", "cwd": str(ROOT)}
    )["workspace"]["id"]
    hook = ROOT / "hook.py"
    hook.write_text("""import json,os,pathlib,signal,sys,time
base=pathlib.Path(sys.argv[1])
def stop(sig,frame):
 (base/'cancelled.json').write_text(json.dumps({'signal':sig}))
 raise SystemExit(0)
signal.signal(signal.SIGTERM,stop)
p=json.load(sys.stdin)
r={k:p.get(k) for k in ('hook_event_name','tool_name','tool_input')}
r.update({'pane_id':os.environ.get('SIGNALTTY_PANE'),'socket':os.environ.get('SIGNALTTY_SOCKET')})
(base/'request.json').write_text(json.dumps(r))
expiry=time.monotonic()+45
while not (base/'answer.json').exists() and time.monotonic()<expiry:time.sleep(.05)
a=json.loads((base/'answer.json').read_text()) if (base/'answer.json').exists() else {'behavior':'deny','message':'probe timeout'}
if p.get('tool_name')!='Bash' or p.get('tool_input',{}).get('command')!='printf approved > approval-proof.txt':
 a={'behavior':'deny','message':'Unexpected command in isolated probe'}
 (base/'unexpected-request.json').write_text(json.dumps(r))
print(json.dumps({'hookSpecificOutput':{'hookEventName':'PermissionRequest','decision':a}}),flush=True)
(base/'returned.json').write_text(json.dumps(a))
""")
    cases = []
    ask = json.dumps(
        {
            "hookSpecificOutput": {
                "hookEventName": "PreToolUse",
                "permissionDecision": "ask",
                "permissionDecisionReason": "isolated signaltty approval probe",
            }
        }
    )
    for choice in ("allow", "deny"):
        base = ROOT / choice
        base.mkdir()
        settings = base / "settings.json"
        settings.write_text(
            json.dumps(
                {
                    "hooks": {
                        "PreToolUse": [
                            {
                                "matcher": "Bash",
                                "hooks": [
                                    {
                                        "type": "command",
                                        "command": "printf '%s\\n' '" + ask + "'",
                                        "timeout": 5,
                                    }
                                ],
                            }
                        ],
                        "PermissionRequest": [
                            {
                                "matcher": "Bash",
                                "hooks": [
                                    {
                                        "type": "command",
                                        "command": f"python3 {shlex.quote(str(hook))} {shlex.quote(str(base))}",
                                        "timeout": 60,
                                    }
                                ],
                            }
                        ],
                    }
                }
            )
        )
        argv = [
            "claude",
            "--model",
            "haiku",
            "--effort",
            "low",
            "--setting-sources",
            "",
            "--settings",
            str(settings),
            "--strict-mcp-config",
            "--mcp-config",
            '{"mcpServers":{}}',
            "--tools",
            "Bash",
            "--permission-mode",
            "manual",
            "--disable-slash-commands",
            "Use Bash exactly once to run this exact command: printf approved > approval-proof.txt . If it is denied do not retry or use other tools. Then answer briefly.",
        ]
        tab = call("tab.create", {"workspace_id": ws, "title": choice})["tab"]["id"]
        pane = call(
            "pane.spawn",
            {
                "workspace_id": ws,
                "tab_id": tab,
                "cwd": str(base),
                "argv": argv,
                "cols": 140,
                "rows": 40,
            },
        )["pane"]["id"]
        panes.append(pane)
        cases.append(
            {
                "choice": choice,
                "base": base,
                "pane": pane,
                "trust_seen": None,
                "trust_arrow": None,
                "trusted": False,
                "request_seen": None,
                "answered": None,
                "done": False,
            }
        )
    started = time.monotonic()
    while time.monotonic() - started < 100 and not all(c["done"] for c in cases):
        for c in cases:
            if c["done"]:
                continue
            now = time.monotonic()
            base = c["base"]
            view = call(
                "pane.read",
                {"pane_id": c["pane"], "mode": "screen", "strip_ansi": True},
            )["text"]
            (base / "screen.txt").write_text(view)
            compact = re.sub(r"\s+", "", view)
            if not c["trusted"] and "Yes,Itrustthisfolder" in compact:
                if c["trust_seen"] is None:
                    c["trust_seen"] = now
                if c["trust_arrow"] is None and now - c["trust_seen"] > 1:
                    send(c["pane"], b"\x1b[B")
                    c["trust_arrow"] = now
                elif c["trust_arrow"] is not None and now - c["trust_arrow"] > 0.5:
                    send(c["pane"], b"\r")
                    c["trusted"] = True
            if c["request_seen"] is None and (base / "request.json").exists():
                c["request_seen"] = now
                print(
                    json.dumps(
                        {
                            "choice": c["choice"],
                            "request": json.loads((base / "request.json").read_text()),
                        }
                    ),
                    flush=True,
                )
            if (
                c["request_seen"] is not None
                and c["answered"] is None
                and now - c["request_seen"] > 2
            ):
                (base / "answer.json").write_text(
                    json.dumps(
                        {
                            "behavior": c["choice"],
                            "message": "User selected "
                            + c["choice"]
                            + " in signaltty probe",
                        }
                    )
                )
                c["answered"] = now
            if c["answered"] is not None and (
                (base / "approval-proof.txt").exists() or now - c["answered"] > 12
            ):
                c["done"] = True
        time.sleep(0.1)
    results = []
    for c in cases:
        base = c["base"]
        request = (
            json.loads((base / "request.json").read_text())
            if (base / "request.json").exists()
            else None
        )
        proof = (base / "approval-proof.txt").exists()
        contents = (base / "approval-proof.txt").read_text() if proof else None
        result = {
            "choice": c["choice"],
            "request_seen": request is not None,
            "expected_pane": c["pane"],
            "request_pane": request.get("pane_id") if request else None,
            "socket_matches": request.get("socket") == str(SOCK) if request else False,
            "proof_exists": proof,
            "hook_returned": (base / "returned.json").exists(),
            "denial_visible": "Denied by PermissionRequest hook"
            in (base / "screen.txt").read_text(),
            "cli_version": version,
            "proof_contents": contents,
            "unexpected_request": (base / "unexpected-request.json").exists(),
        }
        result["passed"] = (
            not result["unexpected_request"]
            and result["request_seen"]
            and result["expected_pane"] == result["request_pane"]
            and result["socket_matches"]
            and result["hook_returned"]
            and proof == (c["choice"] == "allow")
            and (not proof or contents == "approved")
            and (c["choice"] == "allow" or result["denial_visible"])
        )
        results.append(result)
        print(json.dumps(result), flush=True)
    (ROOT / "results.json").write_text(json.dumps(results, indent=2) + "\n")
    if not all(r["passed"] for r in results):
        raise RuntimeError("probe failed; inspect evidence")
finally:
    for pane in panes:
        try:
            call("pane.close", {"pane_id": pane, "signal": "TERM"})
        except Exception as e:
            print("cleanup:", str(e), flush=True)
    try:
        call("server.shutdown", {"force": True})
    except Exception:
        server.terminate()
    try:
        server.wait(timeout=5)
    except subprocess.TimeoutExpired:
        server.kill()
        server.wait()
    log.close()
print(json.dumps({"complete": True, "root": str(ROOT)}), flush=True)
