use serde_json::{json, Value};
use signaltty_integration::Hooks;
use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::PathBuf;

struct Fixture {
    home: PathBuf,
    hooks: Hooks,
}
impl Fixture {
    fn new() -> Self {
        static NEXT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
        let home = std::env::temp_dir().join(format!(
            "signaltty-hooks-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
        ));
        fs::create_dir_all(&home).unwrap();
        let reporter = home.join("a space and ' quote/signaltty");
        fs::create_dir_all(reporter.parent().unwrap()).unwrap();
        fs::write(
            &reporter,
            "#!/bin/sh\nprintf '%s\\n' \"$@\" > \"$HOOK_CAPTURE\"\n",
        )
        .unwrap();
        fs::set_permissions(&reporter, fs::Permissions::from_mode(0o700)).unwrap();
        let hooks = Hooks::new(home.clone(), home.join("config"), reporter);
        Self { home, hooks }
    }
    fn write(&self, agent: &str, text: &str) -> PathBuf {
        let file = self.hooks.file_for(agent).unwrap();
        fs::create_dir_all(file.parent().unwrap()).unwrap();
        fs::write(&file, text).unwrap();
        file
    }
}
impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.home);
    }
}

#[test]
fn malformed_and_wrong_shape_are_never_rewritten() {
    let f = Fixture::new();
    for text in [
        "{invalid",
        "null",
        "[]",
        "{\"hooks\": false}",
        "{\"hooks\": {\"Stop\": false}}",
    ] {
        let file = f.write("claude", text);
        assert!(f.hooks.install("claude").is_err());
        assert_eq!(fs::read_to_string(file).unwrap(), text);
    }
}

#[test]
fn refresh_and_uninstall_preserve_mixed_foreign_commands_and_metadata() {
    let f = Fixture::new();
    let foreign =
        json!({"type":"command", "command":"echo signaltty hook-event belongs in my notes"});
    let original = json!({"model":"opus", "disableAllHooks":true, "hooks":{"Stop":[{"matcher":"", "custom":"keep", "hooks":[foreign, {"type":"command", "command":"/old/signaltty hook-event --agent claude --event Stop --payload-stdin"}]}]}});
    let file = f.write("claude", &original.to_string());
    assert!(f.hooks.install("claude").unwrap().changed);
    let root: Value = serde_json::from_str(&fs::read_to_string(&file).unwrap()).unwrap();
    assert_eq!(root["hooks"]["Stop"][0]["hooks"][0], foreign);
    assert_ne!(
        root["hooks"]["Stop"][0]["hooks"][1]["command"],
        original["hooks"]["Stop"][0]["hooks"][1]["command"]
    );
    f.hooks.uninstall("claude").unwrap();
    let root: Value = serde_json::from_str(&fs::read_to_string(&file).unwrap()).unwrap();
    assert_eq!(root["hooks"]["Stop"][0]["hooks"], json!([foreign]));
    assert_eq!(root["hooks"]["Stop"][0]["custom"], "keep");
    assert_eq!(root["model"], "opus");
    assert_eq!(root["disableAllHooks"], true);
}

#[test]
fn repeated_setup_is_byte_preserving_and_preserves_permissions() {
    let f = Fixture::new();
    for agent in signaltty_integration::AGENTS {
        let report = f.hooks.install(agent).unwrap();
        assert!(report.changed);
        fs::set_permissions(&report.file, fs::Permissions::from_mode(0o640)).unwrap();
        let before = fs::read(&report.file).unwrap();
        let modified = fs::metadata(&report.file).unwrap().modified().unwrap();
        assert!(!f.hooks.install(agent).unwrap().changed);
        assert_eq!(fs::read(&report.file).unwrap(), before);
        assert_eq!(
            fs::metadata(&report.file).unwrap().modified().unwrap(),
            modified
        );
        assert_eq!(
            fs::metadata(&report.file).unwrap().permissions().mode() & 0o777,
            0o640
        );
        assert!(f.hooks.installed(agent).unwrap());
        assert!(f.hooks.uninstall(agent).unwrap().changed);
        assert!(!f.hooks.installed(agent).unwrap());
    }
}

#[test]
fn cursor_and_opencode_refresh_stale_reporter_paths() {
    let f = Fixture::new();
    for agent in ["cursor", "opencode", "claude"] {
        let report = f.hooks.install(agent).unwrap();
        fs::set_permissions(&report.file, fs::Permissions::from_mode(0o640)).unwrap();
        let new = Hooks::new(
            f.home.clone(),
            f.home.join("config"),
            PathBuf::from("/bin/echo"),
        );
        assert!(new.install(agent).unwrap().changed);
        assert_eq!(
            fs::metadata(&report.file).unwrap().permissions().mode() & 0o777,
            0o640
        );
        assert!(!new.install(agent).unwrap().changed);
    }
}

#[test]
fn foreign_opencode_plugin_is_never_overwritten_or_removed() {
    let f = Fixture::new();
    let text = "export default () => ({event: async () => {}});";
    let file = f.write("opencode", text);
    assert!(f.hooks.install("opencode").is_err());
    assert!(!f.hooks.uninstall("opencode").unwrap().changed);
    assert_eq!(fs::read_to_string(file).unwrap(), text);
}

#[test]
fn opencode_plugin_default_export_satisfies_v1_and_v2_loaders() {
    let f = Fixture::new();
    let report = f.hooks.install("opencode").unwrap();
    assert!(report.changed);
    let source = fs::read_to_string(&report.file).unwrap();
    // V2 loader: "Plugin must export a default definition with an id and an
    // effect or setup function." V1 (>= 1.18.29) calls `server()`. This is the
    // documented dual entrypoint: `export default { ...Plugin.define({ id,
    // setup }), server }`, with `define` inlined (identity on
    // `@opencode/plugin` 2.0.22) so the single global file has no dependency.
    assert!(source.contains("export default {"));
    assert!(source.contains("id: \"signaltty\""));
    assert!(source.contains("setup(ctx)"));
    assert!(source.contains("async server()"));
    assert!(source.contains("ctx.event.subscribe"));
    // No bare import: the single global file must load with zero dependencies.
    assert!(!source.contains("from \"@opencode/plugin\""));
    assert!(!source.contains("from '@opencode/plugin'"));
}

fn run_opencode_plugin(loader: &str, events: Value, managed: bool) -> Vec<Value> {
    let f = Fixture::new();
    let reporter = f.home.join("a space and ' quote/signaltty");
    fs::write(
        &reporter,
        r#"#!/usr/bin/env node
const fs = require('node:fs');
fs.appendFileSync(process.env.HOOK_CAPTURE, JSON.stringify({
  argv: process.argv.slice(2), payload: JSON.parse(fs.readFileSync(0, 'utf8'))
}) + '\n');
"#,
    )
    .unwrap();
    let installed = f.hooks.install("opencode").unwrap();
    let module = f.home.join("installed-plugin.mjs");
    fs::copy(installed.file, &module).unwrap();
    let capture = f.home.join("events.jsonl");
    fs::write(&capture, "").unwrap();
    let driver = r#"
import assert from 'node:assert/strict';
import { pathToFileURL } from 'node:url';
const [path, loader, input] = process.argv.slice(1);
const plugin = (await import(pathToFileURL(path))).default;
const events = JSON.parse(input);
if (loader === 'v1') {
  const hooks = await plugin.server();
  for (const event of events) await hooks.event({ event });
} else {
  let signal;
  let complete;
  const consumed = new Promise(resolve => { complete = resolve; });
  const cleanup = plugin.setup({event: {subscribe(options) {
    signal = options.signal;
    return (async function* () {
      for (const event of events) yield event;
      complete();
    })();
  }}});
  await consumed;
  await cleanup();
  assert.equal(signal.aborted, true, 'unload must abort subscription');
}
"#;
    let mut command = std::process::Command::new("node");
    command
        .args(["--input-type=module", "-e", driver])
        .arg(module)
        .arg(loader)
        .arg(events.to_string())
        .env("HOOK_CAPTURE", &capture)
        .env_remove("SIGNALTTY_PANE");
    if managed {
        command.env("SIGNALTTY_PANE", "isolated-test-pane");
    }
    let output = command
        .output()
        .expect("Node is required; run scripts/setup-dev.sh --system");
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    fs::read_to_string(capture)
        .unwrap()
        .lines()
        .map(|line| serde_json::from_str(line).unwrap())
        .collect()
}

#[test]
fn opencode_plugin_v2_failure_reports_error_instead_of_done() {
    // OpenCode 2.0.25 SessionEvent.Execution.Failed schema.
    let reports = run_opencode_plugin(
        "v2",
        json!([
            {"type":"session.execution.failed", "data":{
                "sessionID":"ses_failed", "error":{"type":"unknown", "message":"provider unavailable"}
            }}
        ]),
        true,
    );
    assert_eq!(
        reports,
        vec![json!({
            "argv":["hook-event", "--agent", "opencode", "--event", "session.error", "--payload-stdin"],
            "payload":{"session_id":"ses_failed", "message":"provider unavailable"}
        })]
    );
}

#[test]
fn opencode_plugin_v1_creation_reports_session_info_identity() {
    let reports = run_opencode_plugin(
        "v1",
        json!([
            {"type":"session.created", "properties":{"info":{"id":"ses_created"}}}
        ]),
        true,
    );
    assert_eq!(
        reports,
        vec![json!({
            "argv":["hook-event", "--agent", "opencode", "--event", "session.created", "--payload-stdin"],
            "payload":{"session_id":"ses_created"}
        })]
    );
}

#[test]
fn opencode_plugin_v1_preserves_nested_and_legacy_errors() {
    let reports = run_opencode_plugin(
        "v1",
        json!([
            {"type":"session.error", "properties":{"sessionID":"s", "error":{"data":{"message":"nested failure"}}}},
            {"type":"session.error", "properties":{"sessionID":"s", "message":"legacy failure"}},
            {"type":"session.error", "properties":{"sessionID":"s", "error":{"message":"direct failure"}}},
            {"type":"session.error", "properties":{"sessionID":"s"}}
        ]),
        true,
    );
    assert_eq!(
        reports
            .iter()
            .map(|r| r["payload"].clone())
            .collect::<Vec<_>>(),
        vec![
            json!({"session_id":"s", "message":"nested failure"}),
            json!({"session_id":"s", "message":"legacy failure"}),
            json!({"session_id":"s", "message":"direct failure"}),
            json!({"session_id":"s", "message":"unknown"})
        ]
    );
    assert!(reports.iter().all(|r| r["argv"][4] == "session.error"));
}

#[test]
fn opencode_plugin_v2_retains_lifecycle_and_unmanaged_noop() {
    let events = json!([
        null,
        {"type":"unrelated.event", "data":{}},
        {"type":"session.created", "data":{"sessionID":"s"}},
        {"type":"session.execution.started", "data":{"sessionID":"s"}},
        {"type":"session.execution.succeeded", "data":{"sessionID":"s"}},
        {"type":"session.execution.interrupted", "data":{"sessionID":"s", "reason":"user"}}
    ]);
    let reports = run_opencode_plugin("v2", events.clone(), true);
    assert_eq!(
        reports
            .iter()
            .map(|r| r["payload"].clone())
            .collect::<Vec<_>>(),
        vec![
            json!({"session_id":"s"}),
            json!({"session_id":"s", "status":"busy"}),
            json!({"session_id":"s", "status":"idle"}),
            json!({"session_id":"s", "status":"idle"})
        ]
    );
    assert!(run_opencode_plugin("v2", events, false).is_empty());
    assert!(run_opencode_plugin(
        "v1",
        json!([
            {"type":"session.created", "properties":{"sessionID":"s"}}
        ]),
        false
    )
    .is_empty());
}

#[test]
fn encoded_reporter_path_executes_without_shell_interpretation() {
    let f = Fixture::new();
    let report = f.hooks.install("claude").unwrap();
    let root: Value = serde_json::from_str(&fs::read_to_string(report.file).unwrap()).unwrap();
    let command = root["hooks"]["Stop"][0]["hooks"][0]["command"]
        .as_str()
        .unwrap();
    let capture = f.home.join("args");
    assert!(std::process::Command::new("sh")
        .args(["-c", command])
        .env("HOOK_CAPTURE", &capture)
        .status()
        .unwrap()
        .success());
    assert_eq!(
        fs::read_to_string(capture).unwrap(),
        "hook-event\n--agent\nclaude\n--event\nStop\n--payload-stdin\n"
    );
}

#[test]
fn simultaneous_installations_merge_once_without_corruption() {
    let f = Fixture::new();
    let start = std::sync::Arc::new(std::sync::Barrier::new(12));
    let threads: Vec<_> = (0..12)
        .map(|_| {
            let hooks = f.hooks.clone();
            let start = start.clone();
            std::thread::spawn(move || {
                start.wait();
                hooks.install("claude").unwrap().changed
            })
        })
        .collect();
    assert_eq!(
        threads
            .into_iter()
            .filter_map(|t| t.join().unwrap().then_some(1))
            .sum::<usize>(),
        1
    );
    let root: Value =
        serde_json::from_str(&fs::read_to_string(f.hooks.file_for("claude").unwrap()).unwrap())
            .unwrap();
    assert_eq!(root["hooks"]["Stop"].as_array().unwrap().len(), 1);
}

#[test]
fn symlink_target_is_updated_without_replacing_link() {
    let f = Fixture::new();
    let target = f.home.join("settings-target.json");
    fs::write(&target, "{\"model\":\"opus\"}").unwrap();
    let file = f.hooks.file_for("claude").unwrap();
    fs::create_dir_all(file.parent().unwrap()).unwrap();
    std::os::unix::fs::symlink(&target, &file).unwrap();
    f.hooks.install("claude").unwrap();
    assert!(fs::symlink_metadata(&file)
        .unwrap()
        .file_type()
        .is_symlink());
    let root: Value = serde_json::from_str(&fs::read_to_string(target).unwrap()).unwrap();
    assert_eq!(root["model"], "opus");
}

#[test]
fn uninstall_preserves_foreign_empty_events_and_group_metadata() {
    let f = Fixture::new();
    let root = json!({"hooks":{"Custom":[], "Stop":[{"custom":"keep", "hooks":[{"command":"/old/signaltty hook-event --agent claude --event Stop --payload-stdin"}]}]}});
    let file = f.write("claude", &root.to_string());
    f.hooks.uninstall("claude").unwrap();
    let root: Value = serde_json::from_str(&fs::read_to_string(file).unwrap()).unwrap();
    assert_eq!(root["hooks"]["Custom"], json!([]));
    assert_eq!(root["hooks"]["Stop"][0]["custom"], "keep");
    assert_eq!(root["hooks"]["Stop"][0]["hooks"], json!([]));
}

#[test]
fn a_compound_foreign_command_is_not_a_legacy_managed_hook() {
    let f = Fixture::new();
    for foreign in [
        "echo user-work; /usr/bin/signaltty hook-event --agent claude --event Stop --payload-stdin",
        "/usr/bin/signaltty hook-event --agent claude --event Stop;true --payload-stdin",
        "/usr/bin/signaltty hook-event --agent claude --event Stop\n--payload-stdin",
    ] {
        let file = f.write(
            "claude",
            &json!({"hooks":{"Stop":[{"hooks":[{"command":foreign}]}]}}).to_string(),
        );
        f.hooks.install("claude").unwrap();
        f.hooks.uninstall("claude").unwrap();
        let root: Value = serde_json::from_str(&fs::read_to_string(file).unwrap()).unwrap();
        assert_eq!(root["hooks"]["Stop"][0]["hooks"][0]["command"], foreign);
    }
}

#[test]
fn nonregular_configuration_is_rejected_without_blocking() {
    let f = Fixture::new();
    let file = f.hooks.file_for("claude").unwrap();
    fs::create_dir_all(file.parent().unwrap()).unwrap();
    let status = std::process::Command::new("mkfifo")
        .arg(&file)
        .status()
        .unwrap();
    assert!(status.success());
    let started = std::time::Instant::now();
    assert!(f
        .hooks
        .install("claude")
        .unwrap_err()
        .to_string()
        .contains("regular file"));
    assert!(started.elapsed() < std::time::Duration::from_secs(1));
}

#[test]
fn relative_provider_roots_resolve_against_launch_directory() {
    let f = Fixture::new();
    let cwd = f.home.join("project");
    fs::create_dir_all(&cwd).unwrap();
    let env = std::collections::HashMap::from([("CODEX_HOME".into(), "relative-codex".into())]);
    let hooks = f.hooks.clone().with_overrides(&env, &cwd);
    assert_eq!(
        hooks.install("codex").unwrap().file,
        cwd.join("relative-codex/hooks.json")
    );
    assert!(!f.home.join(".codex/hooks.json").exists());
}

#[test]
fn empty_provider_overrides_keep_the_default_roots() {
    // An empty `CODEX_HOME=` must not resolve to the launch directory and
    // drop hooks.json / settings.json into the project itself.
    let f = Fixture::new();
    let cwd = f.home.join("project");
    fs::create_dir_all(&cwd).unwrap();
    let env = std::collections::HashMap::from([
        ("CLAUDE_CONFIG_DIR".to_string(), String::new()),
        ("CODEX_HOME".to_string(), String::new()),
        ("OPENCODE_CONFIG_DIR".to_string(), String::new()),
    ]);
    let hooks = f.hooks.clone().with_overrides(&env, &cwd);
    assert_eq!(
        hooks.file_for("codex").unwrap(),
        f.home.join(".codex/hooks.json")
    );
    assert_eq!(
        hooks.file_for("claude").unwrap(),
        f.home.join(".claude/settings.json")
    );
    assert_eq!(
        hooks.file_for("opencode").unwrap(),
        f.home.join("config/opencode/plugins/signaltty.js")
    );
}
