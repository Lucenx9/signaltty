//! CLI end-to-end tests: real `signaltty` binary against a hermetic
//! server. Verifies JSON output and opaque-id returns (§15, §23).

use std::process::Command;

use serde_json::Value;
use signaltty_testkit::{bin_path, TestServer};

fn cli(socket: &std::path::Path, args: &[&str]) -> (bool, String) {
    let out = Command::new(bin_path("signaltty"))
        .arg("--socket")
        .arg(socket)
        .args(args)
        .output()
        .expect("run signaltty");
    let text = String::from_utf8(out.stdout).unwrap();
    (out.status.success(), text)
}

fn cli_json(socket: &std::path::Path, args: &[&str]) -> Value {
    let mut full = vec!["--json"];
    full.extend(args.iter());
    let (ok, text) = cli(socket, &full);
    assert!(ok, "cli failed: {text}");
    serde_json::from_str(text.trim()).expect("valid JSON")
}

#[tokio::test]
async fn cli_status_and_new_json() {
    let srv = TestServer::start().await;
    let st = cli_json(&srv.socket, &["status"]);
    assert_eq!(st["protocol"], "signaltty/1");
    assert!(st["version"].is_string());

    let new = cli_json(&srv.socket, &["new", "--cwd", "/tmp", "--", "echo", "hi"]);
    let ws = new["workspace_id"].as_str().unwrap().to_string();
    let pane = new["pane_id"].as_str().unwrap().to_string();
    assert!(ws.starts_with("ws_"), "{ws}");
    assert!(pane.starts_with("pane_"), "{pane}");

    // Read eventually shows output.
    let start = std::time::Instant::now();
    loop {
        let r = cli_json(&srv.socket, &["pane", "read", &pane]);
        if r["text"].as_str().unwrap_or("").contains("hi") {
            break;
        }
        assert!(start.elapsed() < std::time::Duration::from_secs(5));
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;
    }

    // Split, notify, wait, focus.
    let sp = cli_json(&srv.socket, &["pane", "split", &pane, "--", "sleep", "30"]);
    assert!(sp["pane"]["id"].as_str().unwrap().starts_with("pane_"));
    let n = cli_json(
        &srv.socket,
        &["notify", "--pane", &pane, "--title", "t", "--body", "b"],
    );
    assert!(n["notification"]["id"]
        .as_str()
        .unwrap()
        .starts_with("notif_"));
    let f = cli_json(&srv.socket, &["focus", "next-unread"]);
    assert_eq!(f["pane_id"], pane);
    let (ok, _) = cli(&srv.socket, &["pane", "mark-seen", &pane]);
    assert!(ok);
    let w = cli_json(
        &srv.socket,
        &["wait", "--pane", &pane, "--until", "seen", "--timeout", "5"],
    );
    assert_eq!(w["satisfied"], true);
    srv.shutdown().await;
}

#[tokio::test]
async fn cli_hook_event_and_resume() {
    use std::io::Write;
    use std::process::Stdio;
    let srv = TestServer::start().await;
    let new = cli_json(&srv.socket, &["new", "--cwd", "/tmp", "--", "sleep", "30"]);
    let pane = new["pane_id"].as_str().unwrap().to_string();

    // hook-event with piped payload JSON, like a real shim.
    let mut child = Command::new(bin_path("signaltty"))
        .arg("--socket")
        .arg(&srv.socket)
        .args([
            "--json",
            "hook-event",
            "--agent",
            "codex",
            "--event",
            "Stop",
            "--pane",
            &pane,
            "--payload-stdin",
        ])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .spawn()
        .unwrap();
    child
        .stdin
        .as_mut()
        .unwrap()
        .write_all(br#"{"session_id": "cli-sess-1"}"#)
        .unwrap();
    let out = child.wait_with_output().unwrap();
    assert!(out.status.success());
    let v: Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(v["lifecycle"], "done");
    assert_eq!(v["attention"], "unread");
    let g = cli_json(&srv.socket, &["pane", "get", &pane]);
    assert_eq!(g["pane"]["agent"]["agent_session_id"], "cli-sess-1");
    assert_eq!(
        g["pane"]["agent"]["resume_argv"],
        serde_json::json!(["codex", "resume", "cli-sess-1"])
    );
    // Non-JSON hook-event keeps stdout silent (codex Stop parses stdout).
    let out = Command::new(bin_path("signaltty"))
        .arg("--socket")
        .arg(&srv.socket)
        .args([
            "hook-event",
            "--agent",
            "codex",
            "--event",
            "Stop",
            "--pane",
            &pane,
        ])
        .output()
        .unwrap();
    assert!(out.status.success());
    assert!(out.stdout.is_empty());
    assert!(String::from_utf8(out.stderr).unwrap().contains("accepted"));
    srv.shutdown().await;
}

#[tokio::test]
async fn cli_decision_answer_flow() {
    let srv = TestServer::start().await;
    let new = cli_json(&srv.socket, &["new", "--cwd", "/tmp", "--", "cat"]);
    let pane = new["pane_id"].as_str().unwrap().to_string();
    // The fixture child is `cat`, so hint the pane as codex (the channel owner).
    // `new` spawns a plain shell pane; re-hinting is not supported, so drive
    // the flow through hook-event + decision.answer against the recorded kind:
    // plain panes are read-only, which the CLI must surface, not fake.
    let (ok, text) = cli(
        &srv.socket,
        &[
            "hook-event",
            "--agent",
            "codex",
            "--event",
            "PermissionRequest",
            "--pane",
            &pane,
            "--decision",
            r#"{"id":"d1","prompt":"Allow?","options":[{"id":"once","label":"Once"}]}"#,
        ],
    );
    assert!(ok, "{text}");
    let g = cli_json(&srv.socket, &["pane", "get", &pane]);
    assert_eq!(g["pane"]["pending_decision"]["id"], "d1");
    // Human `pane get` prints the prompt plus one line per option.
    let (ok, text) = cli(&srv.socket, &["pane", "get", &pane]);
    assert!(ok, "{text}");
    assert!(text.contains("Allow?"), "{text}");
    assert!(text.contains("once — Once"), "{text}");
    // Invalid decision JSON surfaces at the CLI seam, before any socket call.
    let out = Command::new(bin_path("signaltty"))
        .arg("--socket")
        .arg(&srv.socket)
        .args([
            "hook-event",
            "--agent",
            "codex",
            "--event",
            "Stop",
            "--decision",
            "{",
        ])
        .output()
        .unwrap();
    assert!(!out.status.success());
    let err = String::from_utf8(out.stderr).unwrap();
    assert!(err.contains("invalid decision JSON"), "{err}");
    // This pane runs `cat` (generic kind): answering refuses loudly
    // instead of guessing a channel, and the schema lists the method.
    let out = Command::new(bin_path("signaltty"))
        .arg("--socket")
        .arg(&srv.socket)
        .args([
            "decision",
            "answer",
            "--pane",
            &pane,
            "--decision",
            "d1",
            "--option",
            "once",
        ])
        .output()
        .unwrap();
    assert!(!out.status.success());
    let err = String::from_utf8(out.stderr).unwrap();
    assert!(err.contains("no answer channel"), "{err}");
    let (_, schema) = cli(&srv.socket, &["schema"]);
    assert!(schema.contains("decision.answer"), "{schema}");
    assert!(schema.contains("NO_SUCH_DECISION"), "{schema}");
    srv.shutdown().await;
}

#[tokio::test]
async fn cli_skill_cat_check_install() {
    // `skill` prints the embedded doc; `check` gates on SIGNALTTY_PANE.
    let out = Command::new(bin_path("signaltty"))
        .args(["skill", "cat"])
        .output()
        .unwrap();
    assert!(out.status.success());
    let body = String::from_utf8(out.stdout).unwrap();
    assert!(
        body.lines().next().unwrap().trim() == "<!-- signaltty-skill -->",
        "{body}"
    );
    assert!(body.contains("signaltty schema"));
    let out = Command::new(bin_path("signaltty"))
        .args(["skill", "check"])
        .env_remove("SIGNALTTY_PANE")
        .output()
        .unwrap();
    assert!(!out.status.success());
    let out = Command::new(bin_path("signaltty"))
        .args(["skill", "check"])
        .env("SIGNALTTY_PANE", "pane_x")
        .output()
        .unwrap();
    assert!(out.status.success());
    // Install is idempotent and removes only its own files.
    let home = std::env::temp_dir().join(format!(
        "signaltty-skill-home-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    let home_s = home.display().to_string();
    // A foreign file where our skill would go must survive uninstall.
    let foreign = home.join(".agents/skills/signaltty/SKILL.md");
    std::fs::create_dir_all(foreign.parent().unwrap()).unwrap();
    std::fs::write(&foreign, "mine\n").unwrap();
    let run = |args: &[&str]| {
        Command::new(bin_path("signaltty"))
            .args(args)
            .output()
            .unwrap()
    };
    let out = run(&["skill", "install", "--home", &home_s]);
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8(out.stderr).unwrap()
    );
    // Foreign content wins: install must not overwrite it.
    assert_eq!(std::fs::read_to_string(&foreign).unwrap(), "mine\n");
    std::fs::remove_file(&foreign).unwrap();
    let out = run(&["skill", "install", "--home", &home_s]);
    assert!(out.status.success());
    let out = run(&["skill", "install", "--home", &home_s]);
    assert!(out.status.success());
    assert!(String::from_utf8(out.stdout)
        .unwrap()
        .contains("already installed"));
    for d in [".agents/skills", ".claude/skills", ".codex/skills"] {
        let f = home.join(d).join("signaltty/SKILL.md");
        assert!(f.is_file(), "{d}");
    }
    let out = run(&["skill", "uninstall", "--home", &home_s]);
    assert!(out.status.success());
    for d in [".agents/skills", ".claude/skills", ".codex/skills"] {
        assert!(!home.join(d).join("signaltty/SKILL.md").exists(), "{d}");
    }
    std::fs::remove_dir_all(&home).ok();
}

#[tokio::test]
async fn cli_workspace_diff_reports_counts() {
    let dir = std::env::temp_dir().join(format!(
        "signaltty-clidiff-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    std::fs::create_dir_all(&dir).unwrap();
    let run = |args: &[&str]| {
        let out = Command::new("git")
            .arg("-C")
            .arg(&dir)
            .args(args)
            .output()
            .unwrap();
        assert!(out.status.success(), "{args:?}");
    };
    run(&["init", "-q"]);
    run(&["config", "user.email", "t@t"]);
    run(&["config", "user.name", "t"]);
    run(&["config", "commit.gpgsign", "false"]);
    std::fs::write(dir.join("a.txt"), "1\n").unwrap();
    run(&["add", "."]);
    run(&["commit", "-qm", "base"]);
    std::fs::write(dir.join("a.txt"), "1\n2\n").unwrap();
    let srv = TestServer::start().await;
    let new = cli_json(
        &srv.socket,
        &["new", "--cwd", dir.to_str().unwrap(), "--", "sleep", "30"],
    );
    let ws = new["workspace_id"].as_str().unwrap().to_string();
    let d = cli_json(&srv.socket, &["workspace", "diff", &ws]);
    assert_eq!(d["added"], 1);
    let (ok, text) = cli(&srv.socket, &["workspace", "diff", &ws]);
    assert!(ok, "{text}");
    assert!(text.contains("+1 -0 a.txt"), "{text}");
    assert!(text.contains("total +1 -0"), "{text}");
    srv.shutdown().await;
    std::fs::remove_dir_all(&dir).ok();
}

#[tokio::test]
async fn cli_integration_status_lists_manifests() {
    let dir = std::env::temp_dir().join(format!(
        "signaltty-cli-agents-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(
        dir.join("wrap.toml"),
        "[agent]\nkind = \"codex\"\nbinaries = [\"codex-wrap\"]\n",
    )
    .unwrap();
    std::fs::write(dir.join("broken.toml"), "[agent\nkind = ").unwrap();
    // Scoped env override: only this test reads manifests from here, and
    // no other CLI test asserts on the manifests key.
    std::env::set_var("SIGNALTTY_AGENTS_DIR", &dir);
    let srv = TestServer::start().await;
    let st = cli_json(&srv.socket, &["integration", "status"]);
    let manifests = st["manifests"].as_array().cloned().unwrap_or_default();
    assert_eq!(manifests.len(), 2, "{manifests:?}");
    let wrap = manifests.iter().find(|m| m["name"] == "wrap").unwrap();
    assert_eq!(wrap["manifest"]["kind"], "codex");
    assert_eq!(wrap["manifest"]["ok"], true);
    let broken = manifests.iter().find(|m| m["name"] == "broken").unwrap();
    assert_eq!(broken["manifest"]["ok"], false);
    let (ok, text) = cli(&srv.socket, &["integration", "status"]);
    assert!(ok, "{text}");
    assert!(text.contains("manifest wrap: kind=codex"), "{text}");
    assert!(text.contains("BROKEN"), "{text}");
    std::env::remove_var("SIGNALTTY_AGENTS_DIR");
    srv.shutdown().await;
    std::fs::remove_dir_all(&dir).ok();
}

#[tokio::test]
async fn cli_integration_install_uninstall() {
    let home = std::env::temp_dir().join(format!(
        "signaltty-home-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .subsec_nanos()
    ));
    std::fs::create_dir_all(&home).unwrap();
    let home_s = home.to_string_lossy().to_string();

    // Pre-existing user content must survive install + uninstall.
    let claude_settings = home.join(".claude/settings.json");
    std::fs::create_dir_all(claude_settings.parent().unwrap()).unwrap();
    std::fs::write(
        &claude_settings,
        r#"{"model": "opus", "hooks": {"Stop": [{"matcher": "", "hooks": [{"type": "command", "command": "my-own-hook"}]}]}}"#,
    )
    .unwrap();

    let bin = bin_path("signaltty");
    let run = |args: &[&str]| {
        Command::new(&bin)
            .args(args)
            .env_remove("XDG_CONFIG_HOME")
            .output()
            .expect("run signaltty")
    };
    // Install claude + codex + cursor + opencode.
    for agent in ["claude", "codex", "cursor", "opencode"] {
        let out = run(&["integration", "install", agent, "--home", &home_s]);
        assert!(
            out.status.success(),
            "{agent}: {}",
            String::from_utf8(out.stderr).unwrap()
        );
    }
    // Idempotent reinstall.
    let out = run(&["integration", "install", "claude", "--home", &home_s]);
    assert!(out.status.success());

    let status = run(&["--json", "integration", "status", "--home", &home_s]);
    let v: Value = serde_json::from_slice(&status.stdout).unwrap();
    for agent in ["claude", "codex", "cursor", "opencode"] {
        assert_eq!(v[agent]["installed"], true, "{agent}");
    }
    // User content preserved alongside our entries.
    let settings: Value =
        serde_json::from_str(&std::fs::read_to_string(&claude_settings).unwrap()).unwrap();
    assert_eq!(settings["model"], "opus");
    let stop = settings["hooks"]["Stop"].as_array().unwrap();
    assert_eq!(stop.len(), 2);
    assert!(serde_json::to_string(stop).unwrap().contains("my-own-hook"));
    assert!(serde_json::to_string(stop)
        .unwrap()
        .contains("signaltty hook-event"));
    // Codex + cursor files created with valid JSON.
    for rel in [".codex/hooks.json", ".cursor/hooks.json"] {
        let text = std::fs::read_to_string(home.join(rel)).unwrap();
        let v: Value = serde_json::from_str(&text).unwrap();
        assert!(v.get("hooks").is_some(), "{rel}");
    }
    assert!(home.join(".config/opencode/plugins/signaltty.js").is_file());

    // Uninstall removes only our entries.
    for agent in ["claude", "codex", "cursor", "opencode"] {
        let out = run(&["integration", "uninstall", agent, "--home", &home_s]);
        assert!(out.status.success(), "{agent}");
    }
    let settings: Value =
        serde_json::from_str(&std::fs::read_to_string(&claude_settings).unwrap()).unwrap();
    let stop = settings["hooks"]["Stop"].as_array().unwrap();
    assert_eq!(stop.len(), 1);
    assert!(serde_json::to_string(stop).unwrap().contains("my-own-hook"));
    assert!(!home.join(".config/opencode/plugins/signaltty.js").exists());
    let status = run(&["--json", "integration", "status", "--home", &home_s]);
    let v: Value = serde_json::from_slice(&status.stdout).unwrap();
    for agent in ["claude", "codex", "cursor", "opencode"] {
        assert_eq!(v[agent]["installed"], false, "{agent}");
    }
    std::fs::remove_dir_all(&home).ok();
}

#[tokio::test]
async fn cli_human_output_and_errors() {
    let srv = TestServer::start().await;
    let (ok, text) = cli(&srv.socket, &["status"]);
    assert!(ok);
    assert!(text.contains("workspaces"), "{text}");
    // Unknown pane → nonzero exit + message on stderr.
    let out = Command::new(bin_path("signaltty"))
        .arg("--socket")
        .arg(&srv.socket)
        .args(["pane", "get", "pane_nope"])
        .output()
        .unwrap();
    assert!(!out.status.success());
    let err = String::from_utf8(out.stderr).unwrap();
    assert!(err.contains("NO_SUCH_PANE"), "{err}");
    srv.shutdown().await;
}

#[tokio::test]
async fn cli_plugin_list_run_json() {
    // Plugin dir served to the server AND the local `plugin run`.
    let base = std::env::temp_dir().join(format!(
        "signaltty-plugcli-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .subsec_nanos()
    ));
    let plugdir = base.join("plugins");
    let sub = plugdir.join("greeter");
    std::fs::create_dir_all(&sub).unwrap();
    std::fs::write(
        sub.join("plugin.toml"),
        "[plugin]\nname = \"greeter\"\nversion = \"1.0\"\n\n\
         [[command]]\nname = \"say-hi\"\nrun = [\"./hi.sh\"]\n",
    )
    .unwrap();
    std::fs::write(sub.join("hi.sh"), "#!/bin/sh\necho hi-from-plugin \"$@\"\n").unwrap();
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(sub.join("hi.sh"), std::fs::Permissions::from_mode(0o755))
            .unwrap();
    }
    let srv = TestServer::start_with_plugin_dir(Some(&plugdir)).await;

    let list = cli_json(&srv.socket, &["plugin", "list"]);
    assert_eq!(list["plugins"][0]["name"], "greeter");
    assert_eq!(list["plugins"][0]["commands"][0]["name"], "say-hi");

    // `plugin run` resolves the same dir via env and inherits socket.
    let out = Command::new(bin_path("signaltty"))
        .arg("--socket")
        .arg(&srv.socket)
        .args(["plugin", "run", "greeter", "say-hi", "--", "bob"])
        .env("SIGNALTTY_PLUGIN_DIR", &plugdir)
        .output()
        .unwrap();
    assert!(out.status.success());
    let text = String::from_utf8(out.stdout).unwrap();
    assert!(text.contains("hi-from-plugin bob"), "{text}");

    // Unknown plugin/command → nonzero exit.
    let out = Command::new(bin_path("signaltty"))
        .arg("--socket")
        .arg(&srv.socket)
        .args(["plugin", "run", "greeter", "nope"])
        .env("SIGNALTTY_PLUGIN_DIR", &plugdir)
        .output()
        .unwrap();
    assert!(!out.status.success());
    srv.shutdown().await;
    std::fs::remove_dir_all(&base).ok();
}

#[test]
fn integration_rejects_invalid_configuration_without_rewriting() {
    let home = std::env::temp_dir().join(format!("signaltty-invalid-hooks-{}", std::process::id()));
    let file = home.join(".claude/settings.json");
    std::fs::create_dir_all(file.parent().unwrap()).unwrap();
    for original in ["{broken", "null", "[]", "{\"hooks\": false}"] {
        std::fs::write(&file, original).unwrap();
        let out = Command::new(bin_path("signaltty"))
            .args(["integration", "install", "claude", "--home"])
            .arg(&home)
            .output()
            .unwrap();
        assert!(
            !out.status.success(),
            "invalid configuration accepted: {original}"
        );
        assert_eq!(std::fs::read_to_string(&file).unwrap(), original);
    }
    std::fs::remove_dir_all(home).unwrap();
}

#[tokio::test]
async fn cli_new_preserves_automatic_setup_json_and_human_notice() {
    use std::os::unix::fs::PermissionsExt;
    let srv = TestServer::start().await;
    let codex = srv.socket.parent().unwrap().join("codex");
    std::fs::write(&codex, "#!/bin/sh\necho fixture-codex\n").unwrap();
    std::fs::set_permissions(&codex, std::fs::Permissions::from_mode(0o700)).unwrap();
    let result = cli_json(
        &srv.socket,
        &["new", "--cwd", "/tmp", "--", codex.to_str().unwrap()],
    );
    assert_eq!(result["integration"]["status"], "configured");
    assert_eq!(result["integration"]["changed"], true);
    assert!(result["integration"]["notice"]
        .as_str()
        .unwrap()
        .contains("/hooks"));
    std::fs::remove_file(srv.integration_home.join(".codex/hooks.json")).unwrap();
    let output = Command::new(bin_path("signaltty"))
        .arg("--socket")
        .arg(&srv.socket)
        .args(["new", "--cwd", "/tmp", "--"])
        .arg(&codex)
        .output()
        .unwrap();
    assert!(output.status.success());
    assert!(String::from_utf8(output.stderr).unwrap().contains("/hooks"));
    srv.shutdown().await;
}

#[tokio::test]
async fn cli_pane_submit_and_spawn_lineage() {
    let srv = TestServer::start().await;
    let new = cli_json(&srv.socket, &["new", "--cwd", "/tmp", "--", "sleep", "30"]);
    let ws = new["workspace_id"].as_str().unwrap().to_string();
    let root_pane = new["pane_id"].as_str().unwrap().to_string();

    // Spawn child with lineage
    let tab = cli_json(&srv.socket, &["tab", "create", "--workspace", &ws]);
    let tab_id = tab["tab"]["id"].as_str().unwrap().to_string();

    let child = cli_json(
        &srv.socket,
        &[
            "pane",
            "spawn",
            "--workspace",
            &ws,
            "--tab",
            &tab_id,
            "--parent-pane",
            &root_pane,
            "--label",
            "worker-child",
            "--relationship",
            "subagent",
            "--",
            "sleep",
            "30",
        ],
    );
    let child_pane = child["pane"]["id"].as_str().unwrap().to_string();

    let get = cli_json(&srv.socket, &["pane", "get", &child_pane]);
    assert_eq!(get["pane"]["parent_pane_id"], root_pane);
    assert_eq!(get["pane"]["label"], "worker-child");
    assert_eq!(get["pane"]["relationship"], "subagent");

    // pane submit without activity gate (pane is idle/done or working -> unknown/working)
    // Here sleep 30 has no adapter, so lifecycle is unknown -> AGENT_NOT_READY
    let out = Command::new(bin_path("signaltty"))
        .arg("--socket")
        .arg(&srv.socket)
        .args(["pane", "submit", &child_pane, "--text", "echo hi"])
        .output()
        .unwrap();
    assert!(!out.status.success());
    let err = String::from_utf8(out.stderr).unwrap();
    assert!(
        err.contains("AGENT_NOT_READY") || err.contains("agent is unknown"),
        "{err}"
    );

    srv.shutdown().await;
}

#[tokio::test]
async fn cli_task_orchestration() {
    let repo_dir = std::env::temp_dir().join(format!(
        "signaltty-clitest-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .subsec_nanos()
    ));
    std::fs::create_dir_all(&repo_dir).unwrap();
    let r = std::process::Command::new("git")
        .args(["init", "-b", "main"])
        .current_dir(&repo_dir)
        .output()
        .unwrap();
    assert!(r.status.success());
    let _ = std::process::Command::new("git")
        .args(["config", "user.name", "Orchestrator Test"])
        .current_dir(&repo_dir)
        .output();
    let _ = std::process::Command::new("git")
        .args(["config", "user.email", "orch@test.local"])
        .current_dir(&repo_dir)
        .output();
    std::fs::write(
        repo_dir.join("README.md"),
        "# Test Repo
",
    )
    .unwrap();
    let _ = std::process::Command::new("git")
        .args(["add", "README.md"])
        .current_dir(&repo_dir)
        .output();
    let _ = std::process::Command::new("git")
        .args(["commit", "-m", "Initial commit"])
        .current_dir(&repo_dir)
        .output();

    let srv = TestServer::start().await;

    // 1. task start
    let start = cli_json(
        &srv.socket,
        &[
            "task",
            "start",
            "--repo",
            repo_dir.to_str().unwrap(),
            "--objective",
            "Implement CLI task feature",
            "--label",
            "worker-cli-test",
            "--context",
            "ctx_cli_1",
            "--",
            "sleep",
            "30",
        ],
    );
    let task_id = start["task"]["id"].as_str().unwrap().to_string();
    assert_eq!(start["task"]["state"], "pending");
    assert_eq!(start["task"]["context_id"], "ctx_cli_1");

    // 2. task get
    let get = cli_json(&srv.socket, &["task", "get", &task_id]);
    assert_eq!(get["task"]["id"], task_id);

    // 3. task list
    let list = cli_json(&srv.socket, &["task", "list", "--context", "ctx_cli_1"]);
    assert_eq!(list["tasks"].as_array().unwrap().len(), 1);
    assert_eq!(list["tasks"][0]["id"], task_id);

    // 4. task cancel
    let cancel = cli_json(&srv.socket, &["task", "cancel", &task_id]);
    assert_eq!(cancel["task"]["state"], "canceled");

    srv.shutdown().await;
}

#[tokio::test]
async fn test_cli_phase5_report_attention_wait() {
    let repo_dir = std::env::temp_dir().join(format!(
        "signaltty-clitest-phase5-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .subsec_nanos()
    ));
    std::fs::create_dir_all(&repo_dir).unwrap();
    let _ = std::process::Command::new("git")
        .args(["init", "-b", "main"])
        .current_dir(&repo_dir)
        .output();
    let _ = std::process::Command::new("git")
        .args(["config", "user.name", "Orchestrator Test"])
        .current_dir(&repo_dir)
        .output();
    let _ = std::process::Command::new("git")
        .args(["config", "user.email", "orch@test.local"])
        .current_dir(&repo_dir)
        .output();
    std::fs::write(repo_dir.join("README.md"), "# Test\n").unwrap();
    let _ = std::process::Command::new("git")
        .args(["add", "README.md"])
        .current_dir(&repo_dir)
        .output();
    let _ = std::process::Command::new("git")
        .args(["commit", "-m", "Initial commit"])
        .current_dir(&repo_dir)
        .output();

    let srv = TestServer::start().await;

    // Start a task
    let start = cli_json(
        &srv.socket,
        &[
            "task",
            "start",
            "--repo",
            repo_dir.to_str().unwrap(),
            "--objective",
            "Phase 5 CLI objective",
            "--context",
            "ctx_cli_p5",
            "--",
            "sleep",
            "30",
        ],
    );
    let task_id = start["task"]["id"].as_str().unwrap().to_string();
    let pane_id = start["pane"]["id"].as_str().unwrap().to_string();

    // 1. attention command
    let att = cli_json(&srv.socket, &["attention"]);
    assert!(att["panes"].is_array());
    let (att_ok, _att_text) = cli(&srv.socket, &["attention"]);
    assert!(att_ok);

    // 2. task wait with positional syntax
    // Waiting on empty context matches immediately
    let wait_ctx = cli_json(
        &srv.socket,
        &["task", "wait", "--context", "ctx_nonexistent"],
    );
    assert_eq!(wait_ctx["tasks"].as_array().unwrap().len(), 0);

    // 3. report command with --task
    let rep = cli_json(
        &srv.socket,
        &[
            "report",
            "--task",
            &task_id,
            "--status",
            "completed",
            "--summary",
            "Phase 5 task reported via CLI",
        ],
    );
    assert_eq!(rep["task"]["state"], "completed");
    assert_eq!(
        rep["task"]["result"]["summary"],
        "Phase 5 task reported via CLI"
    );

    // Human output check
    let (ok, _text) = cli(
        &srv.socket,
        &[
            "report",
            "--pane",
            &pane_id,
            "--status",
            "completed",
            "--summary",
            "Second report",
        ],
    );
    assert!(!ok, "second report should fail");

    srv.shutdown().await;
}
