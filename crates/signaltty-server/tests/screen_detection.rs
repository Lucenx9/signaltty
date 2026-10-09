//! Screen-detection rules (spec 027) over real PTYs: manifest `[[screen]]`
//! rules classify hook-less panes; hooks always dominate.

use serde_json::{json, Value};
use signaltty_testkit::{TestClient, TestServer};

const RULES: &str = r#"
[agent]
kind = "generic"

[[screen]]
id = "ask"
state = "blocked"
regex = ['Allow\? \[y/n\]']

[[screen]]
id = "busy"
state = "working"
lines = 1
regex = ['^working\.\.\.$']

[[screen]]
id = "prompt"
state = "idle"
lines = 1
regex = ['^ready>$']
"#;

async fn server_with(manifest_kind: &str) -> TestServer {
    let dir = std::env::temp_dir().join(format!(
        "signaltty-screen-{}-{}",
        std::process::id(),
        signaltty_core::ids::new_pane_id()
    ));
    std::fs::create_dir_all(&dir).unwrap();
    let body = RULES.replace("kind = \"generic\"", &format!("kind = \"{manifest_kind}\""));
    std::fs::write(dir.join("rules.toml"), body).unwrap();
    TestServer::start_with_dirs(None, Some(&dir)).await
}

async fn spawn(c: &mut TestClient, script: &str) -> String {
    let w = c
        .call("workspace.create", json!({"cwd": "/tmp"}))
        .await
        .unwrap();
    let p = c
        .call(
            "pane.spawn",
            json!({"workspace_id": w["workspace"]["id"], "argv": ["sh", "-c", script]}),
        )
        .await
        .unwrap();
    p["pane"]["id"].as_str().unwrap().to_string()
}

async fn wait(c: &mut TestClient, pane: &str, until: &str, secs: u64) -> Result<Value, String> {
    c.call(
        "wait",
        json!({"pane_id": pane, "until": until, "timeout_s": secs}),
    )
    .await
}

async fn pane(c: &mut TestClient, pane: &str) -> Value {
    c.call("pane.get", json!({"pane_id": pane})).await.unwrap()["pane"].clone()
}

#[tokio::test]
async fn approval_prompt_on_screen_blocks_the_pane() {
    let srv = server_with("generic").await;
    let mut c = srv.client().await;
    let id = spawn(&mut c, "printf 'Allow? [y/n] '; sleep 30").await;
    wait(&mut c, &id, "blocked", 5).await.unwrap();
    assert_eq!(pane(&mut c, &id).await["attention"], "input_required");
    srv.shutdown().await;
}

#[tokio::test]
async fn working_then_prompt_finishes_the_turn() {
    let srv = server_with("generic").await;
    let mut c = srv.client().await;
    // Released by the test, so the working phase cannot be missed under load.
    let release = std::env::temp_dir().join(signaltty_core::ids::new_pane_id());
    let script = format!(
        "echo working...; while [ ! -e {} ]; do sleep 0.1; done; echo 'ready>'; sleep 30",
        release.display()
    );
    let id = spawn(&mut c, &script).await;
    wait(&mut c, &id, "working", 10).await.unwrap();
    std::fs::write(&release, b"").unwrap();
    wait(&mut c, &id, "done", 10).await.unwrap();
    let _ = std::fs::remove_file(&release);
    assert_eq!(pane(&mut c, &id).await["attention"], "unread");
    srv.shutdown().await;
}

#[tokio::test]
async fn a_hook_silences_screen_rules_for_that_process() {
    let srv = server_with("generic").await;
    let mut c = srv.client().await;
    let id = spawn(&mut c, "sleep 1; printf 'Allow? [y/n] '; sleep 30").await;
    c.call(
        "hook-event",
        json!({"agent": "claude", "event": "UserPromptSubmit", "pane_id": id}),
    )
    .await
    .unwrap();
    let lifecycle = pane(&mut c, &id).await["lifecycle"].clone();
    assert!(wait(&mut c, &id, "blocked", 3).await.is_err());
    assert_eq!(pane(&mut c, &id).await["lifecycle"], lifecycle);
    srv.shutdown().await;
}

#[tokio::test]
async fn rules_for_another_kind_leave_the_pane_alone() {
    let srv = server_with("codex").await;
    let mut c = srv.client().await;
    let id = spawn(&mut c, "printf 'Allow? [y/n] '; sleep 30").await;
    let before = pane(&mut c, &id).await;
    assert!(wait(&mut c, &id, "blocked", 2).await.is_err());
    let after = pane(&mut c, &id).await;
    assert_eq!(after["lifecycle"], before["lifecycle"]);
    assert_eq!(after["attention"], before["attention"]);
    srv.shutdown().await;
}

#[tokio::test]
async fn a_hook_takes_over_a_screen_blocked_pane() {
    let srv = server_with("generic").await;
    let mut c = srv.client().await;
    let id = spawn(&mut c, "printf 'Allow? [y/n] '; sleep 30").await;
    wait(&mut c, &id, "blocked", 5).await.unwrap();
    c.call(
        "hook-event",
        json!({"agent": "claude", "event": "Stop", "pane_id": id}),
    )
    .await
    .unwrap();
    let p = pane(&mut c, &id).await;
    assert_eq!(
        (p["lifecycle"].clone(), p["attention"].clone()),
        (json!("done"), json!("unread"))
    );
    srv.shutdown().await;
}

/// A fake `pi` executable: shows a working line, then clears to a prompt.
fn fake_pi(script: &str) -> String {
    let dir = std::env::temp_dir().join(format!(
        "signaltty-fake-pi-{}",
        signaltty_core::ids::new_pane_id()
    ));
    std::fs::create_dir_all(&dir).unwrap();
    let pi = dir.join("pi");
    std::fs::write(&pi, format!("#!/bin/sh\n{script}\n")).unwrap();
    use std::os::unix::fs::PermissionsExt;
    std::fs::set_permissions(&pi, std::fs::Permissions::from_mode(0o755)).unwrap();
    pi.to_string_lossy().to_string()
}

async fn spawn_argv(c: &mut TestClient, argv: Value) -> String {
    let w = c
        .call("workspace.create", json!({"cwd": "/tmp"}))
        .await
        .unwrap();
    let p = c
        .call(
            "pane.spawn",
            json!({"workspace_id": w["workspace"]["id"], "argv": argv}),
        )
        .await
        .unwrap();
    assert_eq!(p["pane"]["agent"]["kind"], "pi");
    p["pane"]["id"].as_str().unwrap().to_string()
}

#[tokio::test]
async fn bundled_pi_rules_finish_a_turn_without_manifests() {
    let srv = TestServer::start().await;
    let mut c = srv.client().await;
    // The turn stays visibly "working" until the test releases it, so a
    // loaded machine cannot miss the working state between two screen ticks.
    let release = std::env::temp_dir().join(signaltty_core::ids::new_pane_id());
    let turn = format!(
        "echo Working...; while [ ! -e {} ]; do sleep 0.1; done; printf '\\033[2J\\033[H> '; sleep 30",
        release.display()
    );
    let id = spawn_argv(&mut c, json!([fake_pi(&turn)])).await;
    wait(&mut c, &id, "working", 10).await.unwrap();
    std::fs::write(&release, b"").unwrap();
    wait(&mut c, &id, "done", 10).await.unwrap();
    let _ = std::fs::remove_file(&release);
    assert_eq!(pane(&mut c, &id).await["attention"], "unread");
    srv.shutdown().await;
}

#[tokio::test]
async fn user_screen_rules_replace_the_bundled_ones() {
    let srv = server_with("pi").await;
    let mut c = srv.client().await;
    let id = spawn_argv(&mut c, json!([fake_pi("echo Working...; sleep 30")])).await;
    // The bundled pi rule would call this working; the user's rules do not.
    assert!(wait(&mut c, &id, "working", 2).await.is_err());
    srv.shutdown().await;
}

async fn explain(c: &mut TestClient, pane: &str) -> Result<Value, String> {
    c.call("pane.explain", json!({"pane_id": pane})).await
}

#[tokio::test]
async fn explain_reports_the_bundled_rule_that_matches() {
    let srv = TestServer::start().await;
    let mut c = srv.client().await;
    let id = spawn_argv(&mut c, json!([fake_pi("echo Working...; sleep 30")])).await;
    wait(&mut c, &id, "working", 5).await.unwrap();
    let e = explain(&mut c, &id).await.unwrap();
    assert_eq!(e["kind"], "pi");
    assert_eq!(e["source"], "bundled");
    assert_eq!(
        (e["live"].clone(), e["hooked"].clone()),
        (json!(true), json!(false))
    );
    assert_eq!(e["classifies"], true);
    assert_eq!(e["matched"]["id"], "working_literal");
    assert_eq!(e["matched"]["state"], "working");
    let rules = e["rules"].as_array().unwrap();
    let fallback = rules.iter().find(|r| r["id"] == "idle_fallback").unwrap();
    assert_eq!(fallback["matched"], true);
    assert_eq!(fallback["region"], "bottom(12)");
    let border = rules.iter().find(|r| r["id"] == "working_border").unwrap();
    assert_eq!(border["matched"], false);
    let literal = rules.iter().find(|r| r["id"] == "working_literal").unwrap();
    assert_eq!(
        (literal["matched"].clone(), literal["region"].clone()),
        (json!(true), json!("bottom(200)"))
    );
    // Read-only: explaining twice changes nothing.
    let before = pane(&mut c, &id).await;
    explain(&mut c, &id).await.unwrap();
    let after = pane(&mut c, &id).await;
    for field in ["lifecycle", "attention", "pending_decision"] {
        assert_eq!(after[field], before[field], "{field}");
    }
    srv.shutdown().await;
}

#[tokio::test]
async fn explain_shows_user_rules_and_hooked_panes() {
    let srv = server_with("generic").await;
    let mut c = srv.client().await;
    let id = spawn(&mut c, "printf 'Allow? [y/n] '; sleep 30").await;
    wait(&mut c, &id, "blocked", 5).await.unwrap();
    let e = explain(&mut c, &id).await.unwrap();
    assert_eq!(
        (e["source"].clone(), e["matched"]["id"].clone()),
        (json!("user"), json!("ask"))
    );
    assert_eq!(e["rules"].as_array().unwrap().len(), 3);
    c.call(
        "hook-event",
        json!({"agent": "claude", "event": "Stop", "pane_id": id}),
    )
    .await
    .unwrap();
    let e = explain(&mut c, &id).await.unwrap();
    assert_eq!(
        (e["hooked"].clone(), e["classifies"].clone()),
        (json!(true), json!(false))
    );
    srv.shutdown().await;
}

#[tokio::test]
async fn explain_without_rules_or_pane() {
    let srv = TestServer::start().await;
    let mut c = srv.client().await;
    let id = spawn(&mut c, "sleep 30").await;
    let e = explain(&mut c, &id).await.unwrap();
    assert_eq!(e["source"], "none");
    assert_eq!(e["classifies"], false);
    assert!(e["matched"].is_null());
    assert!(e["rules"].as_array().unwrap().is_empty());
    let err = explain(&mut c, "pane_nope").await.unwrap_err();
    assert!(err.starts_with("NO_SUCH_PANE"), "{err}");
    let err = c.call("pane.explain", json!({})).await.unwrap_err();
    assert!(err.starts_with("BAD_PARAMS"), "{err}");
    srv.shutdown().await;
}

#[tokio::test]
async fn explain_identifies_the_winner_among_duplicate_ids() {
    let dir = std::env::temp_dir().join(format!(
        "signaltty-screen-dup-{}",
        signaltty_core::ids::new_pane_id()
    ));
    std::fs::create_dir_all(&dir).unwrap();
    let rule = |priority: i32, re: &str| {
        format!(
            "[agent]\nkind = \"generic\"\n[[screen]]\nid = \"ask\"\nstate = \"blocked\"\npriority = {priority}\nregex = ['{re}']\n"
        )
    };
    std::fs::write(dir.join("a.toml"), rule(0, "never-shown")).unwrap();
    std::fs::write(dir.join("b.toml"), rule(5, r"Allow\?")).unwrap();
    let srv = TestServer::start_with_dirs(None, Some(&dir)).await;
    let mut c = srv.client().await;
    let id = spawn(&mut c, "printf 'Allow? '; sleep 30").await;
    wait(&mut c, &id, "blocked", 5).await.unwrap();
    let e = explain(&mut c, &id).await.unwrap();
    let index = e["matched"]["index"].as_u64().unwrap() as usize;
    assert_eq!(index, 1);
    assert_eq!(e["rules"][index]["matched"], true);
    assert_eq!(e["rules"][0]["matched"], false);
    srv.shutdown().await;
}

/// `/bin/sh` reachable as `name`, so `/proc/<pid>/cmdline` names `name`.
fn program_as(name: &str) -> String {
    let dir = std::env::temp_dir().join(format!(
        "signaltty-program-{}",
        signaltty_core::ids::new_pane_id()
    ));
    std::fs::create_dir_all(&dir).unwrap();
    let link = dir.join(name);
    std::os::unix::fs::symlink("/bin/sh", &link).unwrap();
    link.to_string_lossy().to_string()
}

const GEMINI_APPLY: &str = "printf '│ Apply this change?\\n'; sleep 30";

async fn spawn_program(c: &mut TestClient, argv: Value) -> String {
    let w = c
        .call("workspace.create", json!({"cwd": "/tmp"}))
        .await
        .unwrap();
    let p = c
        .call(
            "pane.spawn",
            json!({"workspace_id": w["workspace"]["id"], "argv": argv}),
        )
        .await
        .unwrap();
    p["pane"]["id"].as_str().unwrap().to_string()
}

#[tokio::test]
async fn bundled_generic_rules_follow_the_program() {
    let srv = TestServer::start().await;
    let mut c = srv.client().await;
    let gemini = spawn_program(&mut c, json!([program_as("gemini"), "-c", GEMINI_APPLY])).await;
    let other = spawn_program(&mut c, json!([program_as("notgemini"), "-c", GEMINI_APPLY])).await;
    // An alias reaches its agent's rules too (copilot as `ghcs`).
    let ghcs = spawn_program(
        &mut c,
        json!([
            program_as("ghcs"),
            "-c",
            "printf 'Run it? Enter to confirm · Esc to cancel\\n'; sleep 30"
        ]),
    )
    .await;
    wait(&mut c, &ghcs, "blocked", 5).await.unwrap();
    wait(&mut c, &gemini, "blocked", 5).await.unwrap();
    assert_eq!(pane(&mut c, &gemini).await["agent"]["kind"], "generic");
    // The same screen in another generic program stays unclassified.
    assert!(wait(&mut c, &other, "blocked", 2).await.is_err());
    let e = explain(&mut c, &gemini).await.unwrap();
    assert_eq!(
        (e["process"].clone(), e["source"].clone()),
        (json!("gemini"), json!("bundled"))
    );
    let e = explain(&mut c, &other).await.unwrap();
    assert_eq!(
        (e["process"].clone(), e["source"].clone()),
        (json!("notgemini"), json!("none"))
    );
    srv.shutdown().await;
}

#[tokio::test]
async fn user_generic_rules_are_scoped_by_binaries() {
    let dir = std::env::temp_dir().join(format!(
        "signaltty-screen-scoped-{}",
        signaltty_core::ids::new_pane_id()
    ));
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(
        dir.join("mytool.toml"),
        "[agent]\nkind = \"generic\"\nbinaries = [\"mytool\"]\n[[screen]]\nid = \"ask\"\nstate = \"blocked\"\nregex = ['Allow\\?']\n",
    )
    .unwrap();
    let srv = TestServer::start_with_dirs(None, Some(&dir)).await;
    let mut c = srv.client().await;
    let script = "printf 'Allow? '; sleep 30";
    let tool = spawn_program(&mut c, json!([program_as("mytool"), "-c", script])).await;
    let shell = spawn_program(&mut c, json!(["sh", "-c", script])).await;
    wait(&mut c, &tool, "blocked", 5).await.unwrap();
    assert!(wait(&mut c, &shell, "blocked", 2).await.is_err());
    srv.shutdown().await;
}

#[tokio::test]
async fn the_process_refresh_finds_a_program_started_from_a_shell() {
    let srv = TestServer::start().await;
    let mut c = srv.client().await;
    let gemini = program_as("gemini");
    let id = spawn_program(
        &mut c,
        json!([
            "sh",
            "-c",
            format!("sleep 1; exec {gemini} -c \"{GEMINI_APPLY}\"")
        ]),
    )
    .await;
    // Spawned as `sh`: no rules until the process refresh sees `gemini`.
    assert_eq!(explain(&mut c, &id).await.unwrap()["process"], "sh");
    wait(&mut c, &id, "blocked", 25).await.unwrap();
    assert_eq!(explain(&mut c, &id).await.unwrap()["process"], "gemini");
    srv.shutdown().await;
}

fn rule_file(id: &str, state: &str, re: &str) -> String {
    format!("[agent]\nkind = \"generic\"\n[[screen]]\nid = \"{id}\"\nstate = \"{state}\"\nregex = ['{re}']\n")
}

#[tokio::test]
async fn agents_reload_swaps_rules_for_a_live_pane() {
    let dir = std::env::temp_dir().join(format!(
        "signaltty-reload-{}",
        signaltty_core::ids::new_pane_id()
    ));
    std::fs::create_dir_all(&dir).unwrap();
    let srv = TestServer::start_with_dirs(None, Some(&dir)).await;
    let mut c = srv.client().await;
    let list = c.call("agents.list", json!({})).await.unwrap();
    assert_eq!(list["manifests"], json!([]));
    assert_eq!(list["dir"], dir.to_string_lossy().as_ref());
    let id = spawn(&mut c, "printf 'Allow? '; sleep 30").await;
    // No manifest yet: the screen is not classified.
    assert!(wait(&mut c, &id, "blocked", 2).await.is_err());

    // Add a rule and reload: the live pane is classified without a restart.
    std::fs::write(
        dir.join("ask.toml"),
        rule_file("ask", "blocked", r"Allow\?"),
    )
    .unwrap();
    std::fs::write(dir.join("broken.toml"), "[agent]\nkind = \"hal9000\"\n").unwrap();
    let r = c.call("agents.reload", json!({})).await.unwrap();
    assert_eq!(r["manifests"][0]["file"], "ask.toml");
    assert_eq!(r["manifests"][0]["kind"], "generic");
    assert_eq!(r["manifests"][0]["screen_rules"], 1);
    assert_eq!(r["manifests"].as_array().unwrap().len(), 1);
    assert_eq!(r["failures"][0]["file"], "broken.toml");
    assert!(r["failures"][0]["error"]
        .as_str()
        .unwrap()
        .contains("hal9000"));
    wait(&mut c, &id, "blocked", 5).await.unwrap();
    assert_eq!(explain(&mut c, &id).await.unwrap()["matched"]["id"], "ask");

    // Change the rule: explain sees the new one at once.
    std::fs::write(
        dir.join("ask.toml"),
        rule_file("ask2", "blocked", r"Allow\?"),
    )
    .unwrap();
    c.call("agents.reload", json!({})).await.unwrap();
    assert_eq!(explain(&mut c, &id).await.unwrap()["matched"]["id"], "ask2");

    // Remove it: no rules apply any more; the pane keeps running.
    std::fs::remove_file(dir.join("ask.toml")).unwrap();
    let r = c.call("agents.reload", json!({})).await.unwrap();
    assert_eq!(r["manifests"], json!([]));
    let e = explain(&mut c, &id).await.unwrap();
    assert_eq!(
        (e["source"].clone(), e["live"].clone()),
        (json!("none"), json!(true))
    );
    assert_eq!(
        c.call("agents.list", json!({})).await.unwrap()["failures"][0]["file"],
        "broken.toml"
    );
    srv.shutdown().await;
}

#[tokio::test]
async fn agents_reload_updates_spawn_detection() {
    let dir = std::env::temp_dir().join(format!(
        "signaltty-reload-detect-{}",
        signaltty_core::ids::new_pane_id()
    ));
    std::fs::create_dir_all(&dir).unwrap();
    let srv = TestServer::start_with_dirs(None, Some(&dir)).await;
    let mut c = srv.client().await;
    let wrap = program_as("mywrap");
    let before = spawn_program(&mut c, json!([wrap.clone(), "-c", "sleep 30"])).await;
    assert_eq!(pane(&mut c, &before).await["agent"]["kind"], "generic");
    std::fs::write(
        dir.join("wrap.toml"),
        "[agent]\nkind = \"codex\"\nbinaries = [\"mywrap\"]\n",
    )
    .unwrap();
    c.call("agents.reload", json!({})).await.unwrap();
    let after = spawn_program(&mut c, json!([wrap, "-c", "sleep 30"])).await;
    assert_eq!(pane(&mut c, &after).await["agent"]["kind"], "codex");
    // `before` is not asserted again: the 10 s process refresh may promote
    // it like any generic pane running a known program (docs/07).
    srv.shutdown().await;
}

#[tokio::test]
async fn an_unreadable_agents_dir_keeps_the_active_manifests() {
    use std::os::unix::fs::PermissionsExt;
    let dir = std::env::temp_dir().join(format!(
        "signaltty-reload-unreadable-{}",
        signaltty_core::ids::new_pane_id()
    ));
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(
        dir.join("ask.toml"),
        rule_file("ask", "blocked", r"Allow\?"),
    )
    .unwrap();
    let srv = TestServer::start_with_dirs(None, Some(&dir)).await;
    let mut c = srv.client().await;
    std::fs::set_permissions(&dir, std::fs::Permissions::from_mode(0o000)).unwrap();
    let err = c.call("agents.reload", json!({})).await.unwrap_err();
    std::fs::set_permissions(&dir, std::fs::Permissions::from_mode(0o755)).unwrap();
    assert!(err.starts_with("IO_ERROR"), "{err}");
    // The previous set stays active.
    let list = c.call("agents.list", json!({})).await.unwrap();
    assert_eq!(list["manifests"][0]["file"], "ask.toml");
    // A missing dir is just an empty set.
    std::fs::remove_dir_all(&dir).unwrap();
    let r = c.call("agents.reload", json!({})).await.unwrap();
    assert_eq!(r["manifests"], json!([]));
    srv.shutdown().await;
}
