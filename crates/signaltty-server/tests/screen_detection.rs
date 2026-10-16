//! Screen-detection rules (spec 027) over real PTYs: manifest `[[screen]]`
//! rules classify hook-less panes; hooks always dominate.

use serde_json::{json, Value};
use signaltty_testkit::{TempGitRepo, TestClient, TestServer};

#[tokio::test]
async fn antigravity_permission_parks_the_task_until_work_resumes() {
    antigravity_task_permission(false, false).await;
}

#[tokio::test]
async fn antigravity_permission_during_initial_submit_parks_the_task() {
    antigravity_task_permission(true, false).await;
}

#[tokio::test]
async fn antigravity_followup_permission_keeps_the_task_interrupted() {
    antigravity_task_permission(false, true).await;
}

#[tokio::test]
async fn screen_blocker_refuses_followup_without_writing_to_the_pty() {
    screen_blocker_refuses_submit(true).await;
}

#[tokio::test]
async fn screen_blocker_after_paste_refuses_delayed_enter() {
    screen_blocker_refuses_submit(false).await;
}

async fn screen_blocker_refuses_submit(block_before_submit: bool) {
    use std::time::Duration;

    let srv = server_with("generic").await;
    let mut c = srv.client().await;
    let blocked = srv.state_dir.join("block-now");
    let input = srv.state_dir.join("received-input");
    let ready = srv.state_dir.join("ready-now");
    let recorder = r#"import os, select, sys, time, tty
from pathlib import Path
tty.setraw(0)
blocked, received, ready = map(Path, sys.argv[1:])
with received.open('wb', buffering=0) as out:
 print('working...\r', flush=True)
 while not ready.exists():
  time.sleep(0.01)
 print('\033[2J\033[Hready>\r', flush=True)
 shown = False
 while True:
  if blocked.exists() and not shown:
   print('\033[2J\033[HAllow? [y/n]\r', flush=True)
   shown = True
  if select.select([0], [], [], 0.01)[0]:
   out.write(os.read(0, 4096))
"#;
    let pane_id = spawn_program(
        &mut c,
        json!(["python3", "-c", recorder, blocked, input, ready]),
    )
    .await;
    let working = wait(&mut c, &pane_id, "working", 5).await.unwrap();
    assert_eq!(working["satisfied"], true);
    std::fs::write(&ready, b"").unwrap();
    let ready = wait(&mut c, &pane_id, "done", 5).await.unwrap();
    assert_eq!(ready["satisfied"], true);
    if block_before_submit {
        std::fs::write(&blocked, b"").unwrap();
        let blocked = wait(&mut c, &pane_id, "blocked", 5).await.unwrap();
        assert_eq!(blocked["satisfied"], true);
    }
    let mut submit_client = srv.client().await;
    let submit_pane = pane_id.clone();
    let submit = tokio::spawn(async move {
        submit_client
            .call_raw_resp(
                "pane.submit",
                json!({
                    "pane_id": submit_pane, "text": "Do not approve this command",
                    "submit_delay_ms": 1500, "stall_timeout_s": 1,
                }),
            )
            .await
    });
    let expected = if block_before_submit {
        Vec::new()
    } else {
        let pasted = b"\x1b[200~Do not approve this command\x1b[201~".to_vec();
        tokio::time::timeout(Duration::from_secs(2), async {
            while std::fs::read(&input).unwrap() != pasted {
                tokio::time::sleep(Duration::from_millis(5)).await;
            }
        })
        .await
        .expect("prompt pasted before blocker");
        std::fs::write(&blocked, b"").unwrap();
        let blocked = wait(&mut c, &pane_id, "blocked", 5).await.unwrap();
        assert_eq!(blocked["satisfied"], true);
        pasted
    };
    let response = submit.await.unwrap().unwrap();
    // Drain the recorder after any incorrectly accepted prompt or Enter.
    c.call(
        "pane.input",
        json!({
            "pane_id": pane_id, "data_b64": "UkVDT1JERVJfQkFSUklFUg==",
        }),
    )
    .await
    .unwrap();
    tokio::time::timeout(Duration::from_secs(2), async {
        while !std::fs::read(&input)
            .unwrap()
            .ends_with(b"RECORDER_BARRIER")
        {
            tokio::time::sleep(Duration::from_millis(5)).await;
        }
    })
    .await
    .expect("recorder drained");
    assert_eq!(
        std::fs::read(&input).unwrap(),
        [expected, b"RECORDER_BARRIER".to_vec()].concat(),
        "ordinary follow-up must not write prompt bytes or Enter into a screen blocker"
    );
    let error = response
        .error
        .expect("screen blocker refuses ordinary follow-up");
    assert_eq!(error.code, signaltty_proto::code::AGENT_BUSY);
    if !block_before_submit {
        assert_eq!(
            error.details,
            json!({"stage":"delayed_enter", "paste_delivered":true})
        );
    }
    let current = pane(&mut c, &pane_id).await;
    assert_eq!(current["lifecycle"], "blocked");
    assert_eq!(current["attention"], "input_required");
    assert!(current["pending_decision"].is_null());
    srv.shutdown().await;
}

async fn antigravity_task_permission(block_on_submit: bool, followup_blocker: bool) {
    use std::os::unix::fs::PermissionsExt;

    let repo = TempGitRepo::new();
    let srv = TestServer::start().await;
    let directory = srv.socket.parent().unwrap();
    let ready = directory.join("ready");
    let working = directory.join("working");
    let blocked = directory.join("blocked");
    let resumed = directory.join("resumed");
    let followup = directory.join("followup");
    let agy = directory.join("agy");
    let busy = r"printf '\033[2J\033[H⠙ Reading files\n'";
    let idle = r"printf '\033[2J\033[H> \n? for shortcuts\n'";
    let permission = r"printf '\033[2J\033[HRequesting permission for:\n  command -v codexbar\nRun this command?\n> 1. Yes, run command\n  2. No, cancel\n↑/↓ Navigate · tab Amend · ctrl+g edit/expand command\nesc to cancel\n'";
    let first_activity = if block_on_submit { "" } else { busy };
    let resume_output = if followup_blocker { idle } else { busy };
    let after_resume = if followup_blocker {
        format!(
            "while [ ! -e '{}' ]; do sleep 0.05; done\n{permission}",
            followup.display()
        )
    } else {
        String::new()
    };
    std::fs::write(
        &agy,
        format!(
            r"#!/bin/sh
printf '⠙ Starting\n'
while [ ! -e '{}' ]; do sleep 0.05; done
{idle}
while [ ! -e '{}' ]; do sleep 0.05; done
{first_activity}
while [ ! -e '{}' ]; do sleep 0.05; done
{permission}
while [ ! -e '{}' ]; do sleep 0.05; done
{resume_output}
{after_resume}
while read line; do :; done
",
            ready.display(),
            working.display(),
            blocked.display(),
            resumed.display()
        ),
    )
    .unwrap();
    std::fs::set_permissions(&agy, std::fs::Permissions::from_mode(0o755)).unwrap();
    let mut c = srv.client().await;
    let start = c
        .call(
            "task.start",
            json!({
                "repo": repo.path(), "argv": [agy],
                "contract": {"objective": "Review diagnostics"},
                "submit_delay_ms": 10, "stall_timeout_s": 10,
            }),
        )
        .await
        .unwrap();
    let pane_id = start["pane"]["id"].as_str().unwrap();
    let task_id = start["task"]["id"].as_str().unwrap();
    wait(&mut c, pane_id, "working", 5).await.unwrap();
    std::fs::write(&ready, b"").unwrap();
    wait(&mut c, pane_id, "done", 5).await.unwrap();
    // Let the server paste the prompt before the fixture starts its turn.
    wait_for_screen_text(&mut c, pane_id, "Review diagnostics").await;
    if block_on_submit {
        std::fs::write(&blocked, b"").unwrap();
    }
    std::fs::write(&working, b"").unwrap();
    if !block_on_submit {
        c.call(
            "task.wait",
            json!({"task_id": task_id, "until": "working", "timeout_s": 5}),
        )
        .await
        .unwrap();
        std::fs::write(&blocked, b"").unwrap();
    }
    wait(&mut c, pane_id, "blocked", 5).await.unwrap();
    c.call(
        "task.wait",
        json!({"task_id": task_id, "until": "settled", "timeout_s": 5}),
    )
    .await
    .unwrap();
    let task = c
        .call("task.get", json!({"task_id": task_id}))
        .await
        .unwrap();
    assert_eq!(task["task"]["state"], "input_required");
    assert_eq!(task["task"]["status_reason"]["reason"], "screen_blocked");
    c.call(
        "task.wait",
        json!({"task_id": task_id, "until": "settled", "timeout_s": 1}),
    )
    .await
    .unwrap();
    std::fs::write(&resumed, b"").unwrap();
    if followup_blocker {
        wait(&mut c, pane_id, "idle", 5).await.unwrap();
        let mut submit_client = srv.client().await;
        let submit_pane = pane_id.to_string();
        let submit = tokio::spawn(async move {
            submit_client
                .call_raw_resp(
                    "pane.submit",
                    json!({
                        "pane_id": submit_pane, "text": "Follow-up review",
                        "submit_delay_ms": 1500, "stall_timeout_s": 5,
                    }),
                )
                .await
        });
        wait_for_screen_text(&mut c, pane_id, "Follow-up review").await;
        std::fs::write(&followup, b"").unwrap();
        wait(&mut c, pane_id, "blocked", 5).await.unwrap();
        let response = submit.await.unwrap().unwrap();
        let error = response
            .error
            .expect("blocker arrived before delayed Enter");
        assert_eq!(error.code, signaltty_proto::code::AGENT_BUSY);
        assert_eq!(
            error.details,
            json!({"stage":"delayed_enter", "paste_delivered":true})
        );
        let task = c
            .call("task.get", json!({"task_id": task_id}))
            .await
            .unwrap();
        assert_eq!(task["task"]["state"], "input_required");
        assert_eq!(task["task"]["status_reason"]["reason"], "screen_blocked");
    } else {
        wait(&mut c, pane_id, "working", 5).await.unwrap();
        let task = c
            .call("task.get", json!({"task_id": task_id}))
            .await
            .unwrap();
        assert_eq!(task["task"]["state"], "working");
        assert!(task["task"]["status_reason"].is_null());
    }
    srv.shutdown().await;
}

async fn wait_for_screen_text(c: &mut TestClient, pane_id: &str, text: &str) {
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
    loop {
        let read = c
            .call("pane.read", json!({"pane_id": pane_id, "mode": "screen"}))
            .await
            .unwrap();
        if read["text"].as_str().unwrap().contains(text) {
            break;
        }
        assert!(
            std::time::Instant::now() < deadline,
            "screen did not contain {text}"
        );
        tokio::time::sleep(std::time::Duration::from_millis(25)).await;
    }
}

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

const GEMINI_APPLY: &str = "printf '│ Apply this change?\\n'; sleep 30; :";

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

#[tokio::test]
async fn a_versioned_muse_binary_reaches_the_muse_rules() {
    let srv = TestServer::start().await;
    let mut c = srv.client().await;
    let muse = program_as("muse-bin-0.2.1-R708.1");
    let id = spawn_program(
        &mut c,
        json!([
            muse,
            "-c",
            "printf '◆ Working (3s · esc to interrupt)\\n'; sleep 30"
        ]),
    )
    .await;
    wait(&mut c, &id, "working", 5).await.unwrap();
    let e = explain(&mut c, &id).await.unwrap();
    assert_eq!(e["matched"]["id"], "working_esc_interrupt");
    srv.shutdown().await;
}

/// Owns only this test's fixture directory, including manifests and release files.
struct OscFixture(std::path::PathBuf);

impl OscFixture {
    fn new() -> Self {
        let dir = std::env::temp_dir().join(signaltty_core::ids::new_pane_id());
        std::fs::create_dir_all(&dir).unwrap();
        Self(dir)
    }
}

impl Drop for OscFixture {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

#[tokio::test]
async fn progress_rules_finish_a_turn_without_creating_notifications() {
    let fixture = OscFixture::new();
    std::fs::write(
        fixture.0.join("progress.toml"),
        r#"
[agent]
kind = "generic"
[[screen]]
id = "progress_busy"
state = "working"
region = "osc_progress"
regex = ['^4;1;-1$']
[[screen]]
id = "progress_idle"
state = "idle"
region = "osc_progress"
regex = ['^4;0;0$']
"#,
    )
    .unwrap();
    let srv = TestServer::start_with_dirs(None, Some(&fixture.0)).await;
    let mut c = srv.client().await;
    let release = fixture.0.join("release");
    let script = format!("printf '\\033]9;4;1;-1\\007'; while [ ! -e {} ]; do sleep 0.05; done; printf '\\033]9;4;0;0\\033\\\\'; sleep 30", release.display());
    let id = spawn(&mut c, &script).await;
    wait(&mut c, &id, "working", 5).await.unwrap();
    assert_eq!(pane(&mut c, &id).await["attention"], "none");
    let e = explain(&mut c, &id).await.unwrap();
    assert_eq!(e["osc_progress"], "4;1;-1");
    assert_eq!(e["matched"]["id"], "progress_busy");
    std::fs::write(&release, b"").unwrap();
    wait(&mut c, &id, "done", 5).await.unwrap();
    let e = explain(&mut c, &id).await.unwrap();
    assert_eq!(e["osc_progress"], "4;0;0");
    assert_eq!(e["matched"]["id"], "progress_idle");
    assert_eq!(pane(&mut c, &id).await["attention"], "unread");
    srv.shutdown().await;
}

fn fake_grok(fixture: &OscFixture, body: &str) -> String {
    use std::os::unix::fs::PermissionsExt;
    let path = fixture.0.join("grok");
    std::fs::write(&path, format!("#!/usr/bin/python3\nimport sys\n{body}\n")).unwrap();
    std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o755)).unwrap();
    path.to_string_lossy().into_owned()
}

async fn spawn_grok(c: &mut TestClient, executable: &str) -> String {
    let w = c
        .call("workspace.create", json!({"cwd": "/tmp"}))
        .await
        .unwrap();
    let p = c
        .call(
            "pane.spawn",
            json!({"workspace_id": w["workspace"]["id"],
        "argv": [executable]}),
        )
        .await
        .unwrap();
    assert_eq!(p["pane"]["agent"]["kind"], "generic");
    p["pane"]["id"].as_str().unwrap().to_string()
}

async fn advance(c: &mut TestClient, id: &str) {
    use base64::Engine;
    c.call(
        "pane.input",
        json!({"pane_id": id,
        "data_b64": base64::engine::general_purpose::STANDARD.encode(b"\n")}),
    )
    .await
    .unwrap();
}

#[tokio::test]
async fn bundled_grok_progress_blockers_and_completion_use_the_same_explain_evidence() {
    let fixture = OscFixture::new();
    let grok = fake_grok(
        &fixture,
        r#"
print('\x1b]0;custom session\x07\x1b]9;4;1;-1\x07', end='', flush=True)
sys.stdin.readline()
print('\x1b[2J\x1b[H┃  2 (○) Yes, proceed', flush=True)
sys.stdin.readline()
print('\x1b[2J\x1b[H\x1b]9;4;1;-1\x07', end='', flush=True)
sys.stdin.readline()
print('\x1b]9;4;0;0\x07', end='', flush=True)
sys.stdin.readline()
"#,
    );
    let srv = TestServer::start().await;
    let mut c = srv.client().await;
    let id = spawn_grok(&mut c, &grok).await;
    wait(&mut c, &id, "working", 5).await.unwrap();
    assert_eq!(
        explain(&mut c, &id).await.unwrap()["matched"]["id"],
        "osc_progress_working"
    );
    advance(&mut c, &id).await;
    wait(&mut c, &id, "blocked", 5).await.unwrap();
    let e = explain(&mut c, &id).await.unwrap();
    assert_eq!(e["osc_progress"], "4;1;-1");
    assert_eq!(e["matched"]["id"], "option_dialog_blocked");
    assert_eq!(pane(&mut c, &id).await["attention"], "input_required");
    advance(&mut c, &id).await;
    wait(&mut c, &id, "working", 5).await.unwrap();
    assert_eq!(pane(&mut c, &id).await["attention"], "none");
    advance(&mut c, &id).await;
    wait(&mut c, &id, "done", 5).await.unwrap();
    assert_eq!(pane(&mut c, &id).await["attention"], "unread");
    assert_eq!(
        explain(&mut c, &id).await.unwrap()["matched"]["id"],
        "osc_progress_idle"
    );
    advance(&mut c, &id).await;
    wait(&mut c, &id, "exited", 5).await.unwrap();
    let e = explain(&mut c, &id).await.unwrap();
    assert_eq!(e["osc_progress"], "");
    assert_eq!(e["classifies"], false);
    srv.shutdown().await;
}

#[tokio::test]
async fn bundled_grok_without_osc_progress_stays_working_until_the_prompt_returns() {
    let fixture = OscFixture::new();
    let grok = fake_grok(
        &fixture,
        r#"
import os
assert os.environ['TERM_PROGRAM'] == 'signaltty'
assert os.environ['TERM'] == 'xterm-256color'
print('\x1b]0;grok\x07  Waiting on subagent… 2.8s   13s ⇣29.7k [stop]\nShift+Tab:mode │ Ctrl+.:shortcuts', flush=True)
sys.stdin.readline()
print('\x1b[2J\x1b[HWaiting on subagent… [stop]\n┃  2 (○) Yes, proceed', flush=True)
sys.stdin.readline()
print('\x1b[2J\x1b[HWaiting on subagent… [stop]\nShift+Tab:mode │ Ctrl+.:shortcuts', flush=True)
sys.stdin.readline()
print('\x1b[2J\x1b[HShift+Tab:mode │ Ctrl+.:shortcuts', flush=True)
sys.stdin.readline()
"#,
    );
    let srv = TestServer::start_with_env(&[("TERM", "xterm-256color")]).await;
    let mut c = srv.client().await;
    let id = spawn_grok(&mut c, &grok).await;
    wait(&mut c, &id, "working", 5).await.unwrap();
    let e = explain(&mut c, &id).await.unwrap();
    assert_eq!(e["osc_progress"], "");
    assert_eq!(e["matched"]["id"], "spinner_status_working");
    assert!(wait(&mut c, &id, "done", 1).await.is_err());
    assert_eq!(pane(&mut c, &id).await["attention"], "none");
    advance(&mut c, &id).await;
    wait(&mut c, &id, "blocked", 5).await.unwrap();
    assert_eq!(pane(&mut c, &id).await["attention"], "input_required");
    advance(&mut c, &id).await;
    wait(&mut c, &id, "working", 5).await.unwrap();
    advance(&mut c, &id).await;
    wait(&mut c, &id, "done", 5).await.unwrap();
    assert_eq!(pane(&mut c, &id).await["attention"], "unread");
    assert_eq!(explain(&mut c, &id).await.unwrap()["osc_progress"], "");
    srv.shutdown().await;
}

#[tokio::test]
async fn ordinary_terminal_progress_never_raises_attention() {
    let srv = TestServer::start().await;
    let mut c = srv.client().await;
    let id = spawn(
        &mut c,
        "printf '\\033]9;4;1;-1\\007\\033]9;4;0;0\\007ready\\n'; read line",
    )
    .await;
    c.call(
        "pane.wait_for_output",
        json!({"pane_id": id, "match": "ready", "timeout_s": 5}),
    )
    .await
    .unwrap();
    let p = pane(&mut c, &id).await;
    assert_eq!(p["attention"], "none");
    let e = explain(&mut c, &id).await.unwrap();
    assert_eq!(e["osc_progress"], "4;0;0");
    assert_eq!(e["source"], "none");
    assert_eq!(e["classifies"], false);
    srv.shutdown().await;
}

#[tokio::test]
async fn semantic_hooks_override_grok_progress_and_screen_evidence() {
    let fixture = OscFixture::new();
    let grok = fake_grok(
        &fixture,
        r#"
print('\x1b]9;4;1;-1\x07', end='', flush=True)
sys.stdin.readline()
print('\x1b]9;4;1;-1\x07┃  2 (○) Yes, proceed\nready', flush=True)
sys.stdin.readline()
"#,
    );
    let srv = TestServer::start().await;
    let mut c = srv.client().await;
    let id = spawn_grok(&mut c, &grok).await;
    wait(&mut c, &id, "working", 5).await.unwrap();
    c.call(
        "hook-event",
        json!({"pane_id": id, "agent": "claude", "event": "Stop"}),
    )
    .await
    .unwrap();
    advance(&mut c, &id).await;
    c.call(
        "pane.wait_for_output",
        json!({"pane_id": id, "match": "ready", "timeout_s": 5}),
    )
    .await
    .unwrap();
    // Give the normal classification tick a chance to contradict the hook.
    assert!(wait(&mut c, &id, "blocked", 1).await.is_err());
    assert_eq!(pane(&mut c, &id).await["lifecycle"], "done");
    let e = explain(&mut c, &id).await.unwrap();
    assert_eq!(e["osc_progress"], "4;1;-1");
    assert_eq!(e["hooked"], true);
    assert_eq!(e["classifies"], false);
    srv.shutdown().await;
}

#[tokio::test]
async fn progress_is_cleared_on_exit_respawn_and_cold_restart() {
    let mut srv = TestServer::start().await;
    let mut c = srv.client().await;
    let id = spawn(
        &mut c,
        "printf '\\033]9;4;1;-1\\007initial-ready\\n'; read line",
    )
    .await;
    c.call(
        "pane.wait_for_output",
        json!({"pane_id": id, "match": "initial-ready", "timeout_s": 5}),
    )
    .await
    .unwrap();
    assert_eq!(
        explain(&mut c, &id).await.unwrap()["osc_progress"],
        "4;1;-1"
    );
    c.call(
        "report-session",
        json!({"pane_id": id, "agent": "generic", "agent_session_id": "test-progress",
        "resume_argv": ["python3", "-c", "print('replacement-ready', flush=True); input()"]}),
    )
    .await
    .unwrap();
    advance(&mut c, &id).await;
    wait(&mut c, &id, "exited", 5).await.unwrap();
    assert_eq!(explain(&mut c, &id).await.unwrap()["osc_progress"], "");
    c.call("pane.resume", json!({"pane_id": id})).await.unwrap();
    c.call(
        "pane.wait_for_output",
        json!({"pane_id": id, "match": "replacement-ready", "timeout_s": 5}),
    )
    .await
    .unwrap();
    assert_eq!(explain(&mut c, &id).await.unwrap()["osc_progress"], "");
    srv.restart().await;
    c = srv.client().await;
    assert_eq!(explain(&mut c, &id).await.unwrap()["osc_progress"], "");
    srv.shutdown().await;
}
