mod attach;
mod client;
mod daemon;
mod integration;
mod permission_hook;
mod skill;
mod worktree;

use std::path::PathBuf;

use base64::Engine;
use clap::{Parser, Subcommand};
use serde_json::{json, Value};

use client::{CliError, Client};

#[derive(Debug, Parser)]
#[command(
    name = "signaltty",
    about = "signaltty client: workspaces, panes, agents"
)]
struct Args {
    /// Unix socket path (default: $XDG_RUNTIME_DIR/signaltty/signaltty.sock).
    #[arg(long, global = true)]
    socket: Option<PathBuf>,
    /// Machine-readable JSON output (raw `result` object).
    #[arg(long, global = true)]
    json: bool,
    #[command(subcommand)]
    cmd: Command,
}

#[derive(Debug, Subcommand)]
enum Command {
    /// Ensure the session server is running (start detached if needed).
    Daemon,
    /// Server status.
    Status,
    /// Print the server's API contract (methods, events, error codes).
    Schema,
    /// Create a workspace and spawn a pane in it.
    New {
        #[arg(long)]
        cwd: Option<String>,
        #[arg(long)]
        name: Option<String>,
        #[arg(long)]
        tab: Option<String>,
        /// Command to run (default: $SHELL).
        #[arg(last = true)]
        cmd: Vec<String>,
    },
    /// Workspace operations.
    Workspace {
        #[command(subcommand)]
        op: WorkspaceOp,
    },
    /// Git worktree lifecycle.
    Worktree {
        #[command(subcommand)]
        op: worktree::WorktreeOp,
    },
    /// Tab operations.
    Tab {
        #[command(subcommand)]
        op: TabOp,
    },
    /// Pane operations.
    Pane {
        #[command(subcommand)]
        op: PaneOp,
    },
    /// Emit an explicit notification for a pane.
    Notify {
        #[arg(long)]
        title: String,
        #[arg(long)]
        body: Option<String>,
        #[arg(long)]
        pane: Option<String>,
        #[arg(long, value_parser = ["info", "warning", "error"])]
        severity: Option<String>,
    },
    /// Report a native agent session id for a pane.
    ReportSession {
        #[arg(long)]
        pane: String,
        #[arg(long)]
        session: String,
        #[arg(long)]
        agent: Option<String>,
    },
    /// Deliver an agent hook event (used by installed shims).
    HookEvent {
        #[arg(long)]
        agent: String,
        #[arg(long)]
        event: String,
        #[arg(long)]
        pane: Option<String>,
        /// Read the hook payload JSON from stdin.
        #[arg(long)]
        payload_stdin: bool,
        /// Wait for a native PermissionRequest answer; output provider JSON only.
        #[arg(long)]
        wait_for_answer: bool,
        #[arg(long)]
        message: Option<String>,
        #[arg(long)]
        title: Option<String>,
        #[arg(long, value_parser = ["info", "warning", "error"])]
        severity: Option<String>,
        /// Structured decision request as JSON
        /// (`{"id","prompt","options":[{"id","label"}]}`).
        #[arg(long)]
        decision: Option<String>,
    },
    /// Answer a pending structured decision (directive 2).
    Decision {
        #[command(subcommand)]
        op: DecisionOp,
    },
    /// Manage agent hook integrations.
    Integration {
        #[command(subcommand)]
        op: IntegrationOp,
    },
    /// The gated agent skill (directive 5): read, check, install.
    Skill {
        #[command(subcommand)]
        op: SkillOp,
    },
    /// Wait until a pane reaches a state (lifecycle, attention, exited, seen).
    Wait {
        #[arg(long)]
        pane: String,
        #[arg(long, required = true, value_delimiter = ',')]
        until: Vec<String>,
        /// JSON wait_baseline captured from pane get before submitting work.
        #[arg(long)]
        after_baseline: Option<String>,
        #[arg(long)]
        timeout: Option<u64>,
    },
    /// Focus helpers.
    Focus {
        #[command(subcommand)]
        op: FocusOp,
    },
    /// Executable plugins (manifests + event hooks + commands).
    Plugin {
        #[command(subcommand)]
        op: PluginOp,
    },
}

#[derive(Debug, Subcommand)]
enum WorkspaceOp {
    List,
    Get {
        id: String,
    },
    Rename {
        id: String,
        name: String,
    },
    Close {
        id: String,
    },
    RefreshGit {
        id: String,
    },
    /// Worktree-vs-HEAD diff as data (files, per-dir groups, totals).
    Diff {
        id: String,
    },
}

#[derive(Debug, Subcommand)]
enum TabOp {
    Create {
        #[arg(long)]
        workspace: String,
        #[arg(long)]
        title: Option<String>,
    },
    Close {
        id: String,
    },
}

#[derive(Debug, Subcommand)]
enum PaneOp {
    Spawn {
        #[arg(long)]
        workspace: String,
        #[arg(long)]
        tab: Option<String>,
        #[arg(long)]
        cwd: Option<String>,
        #[arg(long)]
        agent: Option<String>,
        #[arg(last = true)]
        cmd: Vec<String>,
    },
    Split {
        id: String,
        #[arg(long, value_parser = ["right", "down"], default_value = "right")]
        direction: String,
        #[arg(long)]
        cwd: Option<String>,
        #[arg(last = true)]
        cmd: Vec<String>,
    },
    Get {
        id: String,
    },
    Input {
        id: String,
        /// Literal text to send (use --stdin for piped input).
        #[arg(long, conflicts_with = "stdin")]
        data: Option<String>,
        #[arg(long)]
        stdin: bool,
    },
    Resize {
        id: String,
        #[arg(long)]
        cols: u16,
        #[arg(long)]
        rows: u16,
    },
    Signal {
        id: String,
        #[arg(long, default_value = "INT")]
        signal: String,
        #[arg(long)]
        group: bool,
    },
    Read {
        id: String,
        #[arg(long, value_parser = ["screen", "tail"], default_value = "tail")]
        mode: String,
        #[arg(long, default_value_t = 200)]
        lines: u64,
        #[arg(long)]
        raw: bool,
    },
    Attach {
        id: String,
    },
    Close {
        id: String,
    },
    MarkSeen {
        id: String,
    },
    /// Resume a restored pane via its adapter's official resume command.
    Resume {
        id: String,
    },
}

#[derive(Debug, Subcommand)]
enum DecisionOp {
    /// Answer with one of the decision's options.
    Answer {
        #[arg(long)]
        pane: String,
        #[arg(long)]
        decision: String,
        #[arg(long)]
        option: String,
    },
}

#[derive(Debug, Subcommand)]
enum FocusOp {
    /// Id of the next pane needing a human (severity → recency).
    NextUnread,
}

#[derive(Debug, Subcommand)]
enum PluginOp {
    /// List loaded plugins, hooks, and commands.
    List,
    /// Reload plugin manifests from the plugin dir.
    Reload,
    /// Run a plugin command locally (extra args appended).
    Run {
        plugin: String,
        command: String,
        /// Extra args passed to the command.
        #[arg(last = true)]
        args: Vec<String>,
    },
}

#[derive(Debug, Subcommand)]
enum SkillOp {
    /// Print the skill document (agents read this inside a pane).
    Cat,
    /// Gate: exit 0 inside a managed pane, 1 outside.
    Check,
    /// Install the skill file into the harness skills dirs.
    Install {
        #[arg(long)]
        home: Option<String>,
    },
    /// Remove skill files installed by `install`.
    Uninstall {
        #[arg(long)]
        home: Option<String>,
    },
    /// Show skill install status.
    Status {
        #[arg(long)]
        home: Option<String>,
    },
}

#[derive(Debug, Subcommand)]
enum IntegrationOp {
    /// Install hook shims for an agent (or `all`).
    Install {
        agent: String,
        /// Home directory override (default: $HOME). For testing.
        #[arg(long)]
        home: Option<String>,
    },
    /// Remove hook shims installed by `install`.
    Uninstall {
        agent: String,
        #[arg(long)]
        home: Option<String>,
    },
    /// Show integration status.
    Status {
        #[arg(long)]
        home: Option<String>,
    },
}

fn socket_path(args: &Args) -> PathBuf {
    args.socket
        .clone()
        .unwrap_or_else(signaltty_core::paths::socket_path)
}

fn emit(json_mode: bool, result: &Value, human: String) {
    if json_mode {
        println!("{}", serde_json::to_string(result).unwrap());
    } else {
        println!("{human}");
    }
}

fn shell_cmd() -> Vec<String> {
    vec![std::env::var("SHELL").unwrap_or_else(|_| "sh".to_string())]
}

/// Parse `--after-baseline` JSON. An explicit `null` (e.g. from `jq .wait_baseline`
/// on a failed `pane get`) must not silently become a current-state wait.
fn parse_wait_baseline(raw: &str) -> Result<Value, CliError> {
    let baseline: Value = serde_json::from_str(raw)
        .map_err(|e| CliError::Usage(format!("invalid wait baseline: {e}")))?;
    if baseline.is_null() {
        return Err(CliError::Usage("wait baseline must not be null".into()));
    }
    Ok(baseline)
}

#[tokio::main]
async fn main() {
    let args = Args::parse();
    if let Err(e) = run(args).await {
        eprintln!("signaltty: {e}");
        std::process::exit(1);
    }
}

async fn run(args: Args) -> Result<(), CliError> {
    let socket = socket_path(&args);
    let json = args.json;
    match args.cmd {
        Command::Daemon => daemon::ensure_running(&socket, json).await,
        Command::Status => {
            let mut c = Client::connect(&socket).await?;
            let r = c.call("server.status", json!({})).await?;
            let human = format!(
                "signaltty {} ({}) up {}s — {} workspaces, {} tabs, {} panes ({} live)",
                r["version"].as_str().unwrap_or("?"),
                r["protocol"].as_str().unwrap_or("?"),
                r["uptime_s"].as_u64().unwrap_or(0),
                r["workspaces"].as_u64().unwrap_or(0),
                r["tabs"].as_u64().unwrap_or(0),
                r["panes"].as_u64().unwrap_or(0),
                r["live_panes"].as_u64().unwrap_or(0),
            );
            emit(json, &r, human);
            Ok(())
        }
        Command::Schema => {
            let mut c = Client::connect(&socket).await?;
            let r = c.call("server.schema", json!({})).await?;
            let names = |key: &str| {
                r[key]
                    .as_array()
                    .map(|a| {
                        a.iter()
                            .filter_map(|v| v.as_str())
                            .collect::<Vec<_>>()
                            .join(" ")
                    })
                    .unwrap_or_default()
            };
            let human = format!(
                "signaltty {} ({})\nmethods: {}\nevents: {}\ncodes: {}",
                r["version"].as_str().unwrap_or("?"),
                r["protocol"].as_str().unwrap_or("?"),
                names("methods"),
                names("events"),
                names("codes"),
            );
            emit(json, &r, human);
            Ok(())
        }
        Command::New {
            cwd,
            name,
            tab,
            cmd,
        } => {
            let mut c = Client::connect(&socket).await?;
            let mut wparams = json!({});
            if let Some(cwd) = cwd {
                wparams["cwd"] = json!(cwd);
            }
            if let Some(name) = name {
                wparams["name"] = json!(name);
            }
            let w = c.call("workspace.create", wparams).await?;
            let ws_id = w["workspace"]["id"].as_str().unwrap_or("?").to_string();
            let tab_id = if let Some(title) = tab {
                let t = c
                    .call("tab.create", json!({"workspace_id": ws_id, "title": title}))
                    .await?;
                t["tab"]["id"].as_str().unwrap_or("?").to_string()
            } else {
                String::new()
            };
            let argv = if cmd.is_empty() { shell_cmd() } else { cmd };
            let mut sparams = json!({"workspace_id": ws_id, "argv": argv});
            if !tab_id.is_empty() {
                sparams["tab_id"] = json!(tab_id);
            }
            let p = c.call("pane.spawn", sparams).await?;
            integration_notice(&p, json);
            let pane_id = p["pane"]["id"].as_str().unwrap_or("?").to_string();
            let handle = w["workspace"]["handle"].as_str().unwrap_or("?").to_string();
            let mut result = json!({"workspace_id": ws_id, "handle": handle, "pane_id": pane_id});
            if let Some(setup) = p.get("integration") {
                result["integration"] = setup.clone();
            }
            emit(
                json,
                &result,
                format!("workspace {ws_id} ({handle}) pane {pane_id}"),
            );
            Ok(())
        }
        Command::Worktree { op } => {
            let mut c = Client::connect(&socket).await?;
            worktree::run(&mut c, op, json).await
        }
        Command::Workspace { op } => {
            let mut c = Client::connect(&socket).await?;
            match op {
                WorkspaceOp::List => {
                    let r = c.call("workspace.list", json!({})).await?;
                    let mut human = String::new();
                    for w in r["workspaces"].as_array().cloned().unwrap_or_default() {
                        human.push_str(&format!(
                            "{}  {}  {}  {}  branch={}\n",
                            w["id"].as_str().unwrap_or("?"),
                            w["handle"].as_str().unwrap_or("-"),
                            w["name"].as_str().unwrap_or("?"),
                            w["cwd"].as_str().unwrap_or("?"),
                            w["git"]["branch"].as_str().unwrap_or("-"),
                        ));
                    }
                    emit(json, &r, human.trim_end().to_string());
                    Ok(())
                }
                WorkspaceOp::Get { id } => {
                    let r = c.call("workspace.get", json!({"workspace_id": id})).await?;
                    let w = &r["workspace"];
                    let human = format!(
                        "{} {} ({}) — {} tabs, {} panes",
                        w["id"].as_str().unwrap_or("?"),
                        w["name"].as_str().unwrap_or("?"),
                        w["cwd"].as_str().unwrap_or("?"),
                        r["tabs"].as_array().map(|a| a.len()).unwrap_or(0),
                        r["panes"].as_array().map(|a| a.len()).unwrap_or(0),
                    );
                    emit(json, &r, human);
                    Ok(())
                }
                WorkspaceOp::Rename { id, name } => {
                    let r = c
                        .call(
                            "workspace.rename",
                            json!({"workspace_id": id, "name": name}),
                        )
                        .await?;
                    emit(json, &r, format!("renamed {id}"));
                    Ok(())
                }
                WorkspaceOp::Close { id } => {
                    let r = c
                        .call("workspace.close", json!({"workspace_id": id}))
                        .await?;
                    emit(json, &r, format!("closed {id}"));
                    Ok(())
                }
                WorkspaceOp::RefreshGit { id } => {
                    let r = c
                        .call("workspace.refresh_git", json!({"workspace_id": id}))
                        .await?;
                    let branch = r["workspace"]["git"]["branch"].as_str().unwrap_or("-");
                    emit(json, &r, format!("branch={branch}"));
                    Ok(())
                }
                WorkspaceOp::Diff { id } => {
                    let r = c
                        .call("workspace.diff", json!({"workspace_id": id}))
                        .await?;
                    let mut human = String::new();
                    for f in r["files"].as_array().cloned().unwrap_or_default() {
                        let path = f["path"].as_str().unwrap_or("?");
                        if f["untracked"].as_bool().unwrap_or(false) {
                            human.push_str(&format!("?? {path}\n"));
                        } else if f["binary"].as_bool().unwrap_or(false) {
                            human.push_str(&format!("bin {path}\n"));
                        } else {
                            human.push_str(&format!(
                                "+{} -{} {path}\n",
                                f["added"].as_u64().unwrap_or(0),
                                f["removed"].as_u64().unwrap_or(0),
                            ));
                        }
                    }
                    for g in r["dirs"].as_array().cloned().unwrap_or_default() {
                        human.push_str(&format!(
                            "{}/ +{} -{}\n",
                            g["dir"].as_str().unwrap_or("?"),
                            g["added"].as_u64().unwrap_or(0),
                            g["removed"].as_u64().unwrap_or(0),
                        ));
                    }
                    human.push_str(&format!(
                        "total +{} -{}\n",
                        r["added"].as_u64().unwrap_or(0),
                        r["removed"].as_u64().unwrap_or(0),
                    ));
                    emit(json, &r, human.trim_end().to_string());
                    Ok(())
                }
            }
        }
        Command::Tab { op } => {
            let mut c = Client::connect(&socket).await?;
            match op {
                TabOp::Create { workspace, title } => {
                    let mut p = json!({"workspace_id": workspace});
                    if let Some(t) = title {
                        p["title"] = json!(t);
                    }
                    let r = c.call("tab.create", p).await?;
                    let id = r["tab"]["id"].as_str().unwrap_or("?").to_string();
                    emit(json, &r, format!("tab {id}"));
                    Ok(())
                }
                TabOp::Close { id } => {
                    let r = c.call("tab.close", json!({"tab_id": id})).await?;
                    emit(json, &r, format!("closed {id}"));
                    Ok(())
                }
            }
        }
        Command::Pane { op } => pane_cmd(socket, json, op).await,
        Command::Notify {
            title,
            body,
            pane,
            severity,
        } => {
            let pane = pane
                .or_else(|| std::env::var("SIGNALTTY_PANE").ok())
                .ok_or_else(|| {
                    CliError::Usage("--pane required (or run inside a pane)".to_string())
                })?;
            let mut c = Client::connect(&socket).await?;
            let mut p = json!({"pane_id": pane, "title": title});
            if let Some(b) = body {
                p["body"] = json!(b);
            }
            if let Some(s) = severity {
                p["severity"] = json!(s);
            }
            let r = c.call("notify", p).await?;
            let id = r["notification"]["id"].as_str().unwrap_or("?").to_string();
            emit(json, &r, format!("notification {id}"));
            Ok(())
        }
        Command::ReportSession {
            pane,
            session,
            agent,
        } => {
            let mut c = Client::connect(&socket).await?;
            let mut p = json!({"pane_id": pane, "agent_session_id": session});
            if let Some(a) = agent {
                p["agent"] = json!(a);
            }
            let r = c.call("report-session", p).await?;
            emit(json, &r, format!("session recorded for {pane}"));
            Ok(())
        }
        Command::HookEvent {
            agent,
            event,
            pane,
            payload_stdin,
            wait_for_answer,
            message,
            title,
            severity,
            decision,
        } => {
            let pane = pane.or_else(|| std::env::var("SIGNALTTY_PANE").ok());
            let mut c = Client::connect(&socket).await?;
            let mut p = json!({"agent": agent, "event": event, "client_pid": std::process::id()});
            if let Some(pid) = pane {
                p["pane_id"] = json!(pid);
            }
            if payload_stdin {
                use tokio::io::AsyncReadExt;
                let mut buf = String::new();
                tokio::io::stdin()
                    .read_to_string(&mut buf)
                    .await
                    .map_err(|e| CliError::Io(e.to_string()))?;
                let payload: Value = if buf.trim().is_empty() {
                    Value::Null
                } else {
                    serde_json::from_str(&buf)
                        .map_err(|e| CliError::Usage(format!("invalid payload JSON: {e}")))?
                };
                p["payload"] = payload;
            }
            if let Some(m) = message {
                p["message"] = json!(m);
            }
            if let Some(t) = title {
                p["title"] = json!(t);
            }
            if let Some(s) = severity {
                p["severity"] = json!(s);
            }
            if let Some(d) = decision {
                let v: Value = serde_json::from_str(&d)
                    .map_err(|e| CliError::Usage(format!("invalid decision JSON: {e}")))?;
                p["decision"] = v;
            }
            if wait_for_answer {
                p["wait_for_answer"] = json!(true);
            }
            let r = c.call("hook-event", p).await?;
            if wait_for_answer {
                return permission_hook::print_verdict(&r);
            }
            if json {
                println!("{}", serde_json::to_string(&r).unwrap());
            } else {
                // Human feedback goes to stderr: hook hosts parse stdout
                // (codex Stop REQUIRES stdout to be empty or JSON).
                eprintln!(
                    "accepted {}:{} lifecycle={} attention={}",
                    r["agent"].as_str().unwrap_or("?"),
                    r["event"].as_str().unwrap_or("?"),
                    r["lifecycle"].as_str().unwrap_or("-"),
                    r["attention"].as_str().unwrap_or("-"),
                );
            }
            Ok(())
        }
        Command::Integration { op } => match op {
            IntegrationOp::Install { agent, home } => {
                let home = home.map(std::path::PathBuf::from);
                if agent == "all" {
                    for a in integration::valid_agents() {
                        integration::install(home.as_deref(), a, json)?;
                    }
                    Ok(())
                } else {
                    integration::install(home.as_deref(), &agent, json)
                }
            }
            IntegrationOp::Uninstall { agent, home } => {
                let home = home.map(std::path::PathBuf::from);
                if agent == "all" {
                    for a in integration::valid_agents() {
                        integration::uninstall(home.as_deref(), a, json)?;
                    }
                    Ok(())
                } else {
                    integration::uninstall(home.as_deref(), &agent, json)
                }
            }
            IntegrationOp::Status { home } => {
                let home = home.map(std::path::PathBuf::from);
                integration::status(home.as_deref(), &[], json)
            }
        },
        Command::Decision { op } => {
            let mut c = Client::connect(&socket).await?;
            match op {
                DecisionOp::Answer {
                    pane,
                    decision,
                    option,
                } => {
                    let r = c
                        .call(
                            "decision.answer",
                            json!({"pane_id": pane, "decision_id": decision, "option_id": option}),
                        )
                        .await?;
                    let human = if r["answered"].as_bool().unwrap_or(false) {
                        format!("answered {decision} with {option}")
                    } else {
                        format!("decision {decision} already gone")
                    };
                    emit(json, &r, human);
                    Ok(())
                }
            }
        }
        Command::Skill { op } => match op {
            SkillOp::Cat => {
                if json {
                    println!(
                        "{}",
                        serde_json::json!({"version": env!("CARGO_PKG_VERSION"), "text": skill::TEXT})
                    );
                } else {
                    print!("{}", skill::TEXT);
                }
                Ok(())
            }
            SkillOp::Check => match skill::check() {
                Ok(pane) => {
                    emit(json, &json!({"pane": pane}), format!("managed pane {pane}"));
                    Ok(())
                }
                Err(e) => Err(e),
            },
            SkillOp::Install { home } => {
                let home = integration::home_dir(home.as_deref())?;
                skill::install(&home, json)
            }
            SkillOp::Uninstall { home } => {
                let home = integration::home_dir(home.as_deref())?;
                skill::uninstall(&home, json)
            }
            SkillOp::Status { home } => {
                let home = integration::home_dir(home.as_deref())?;
                skill::status(&home, json)
            }
        },
        Command::Wait {
            pane,
            until,
            after_baseline,
            timeout,
        } => {
            let mut c = Client::connect(&socket).await?;
            let mut p = json!({"pane_id": pane, "until": until});
            if let Some(baseline) = after_baseline {
                p["after"] = parse_wait_baseline(&baseline)?;
            }
            if let Some(t) = timeout {
                p["timeout_s"] = json!(t);
            }
            let r = c.call("wait", p).await?;
            let human = format!(
                "satisfied: lifecycle={} attention={}",
                r["lifecycle"].as_str().unwrap_or("?"),
                r["attention"].as_str().unwrap_or("?"),
            );
            emit(json, &r, human);
            Ok(())
        }
        Command::Focus { op } => {
            let mut c = Client::connect(&socket).await?;
            match op {
                FocusOp::NextUnread => {
                    let r = c.call("focus.next_unread", json!({})).await?;
                    let id = r["pane_id"].as_str().unwrap_or_default().to_string();
                    emit(
                        json,
                        &r,
                        if id.is_empty() {
                            "(none)".to_string()
                        } else {
                            id
                        },
                    );
                    Ok(())
                }
            }
        }
        Command::Plugin { op } => plugin_cmd(socket, json, op).await,
    }
}

async fn plugin_cmd(socket: PathBuf, json: bool, op: PluginOp) -> Result<(), CliError> {
    match op {
        PluginOp::List | PluginOp::Reload => {
            let method = match op {
                PluginOp::List => "plugin.list",
                _ => "plugin.reload",
            };
            let mut c = Client::connect(&socket).await?;
            let r = c.call(method, json!({})).await?;
            let mut human = String::new();
            let plugins = r["plugins"].as_array().cloned().unwrap_or_default();
            if plugins.is_empty() {
                human.push_str("(no plugins)\n");
            }
            for p in &plugins {
                human.push_str(&format!(
                    "{} {} — {}\n",
                    p["name"].as_str().unwrap_or("?"),
                    p["version"].as_str().unwrap_or(""),
                    p["description"].as_str().unwrap_or("")
                ));
                for h in p["hooks"].as_array().cloned().unwrap_or_default() {
                    human.push_str(&format!(
                        "  hook {:?} → {} (runs {}, errors {})\n",
                        h["events"],
                        h["command"][0].as_str().unwrap_or("?"),
                        h["runs"].as_u64().unwrap_or(0),
                        h["errors"].as_u64().unwrap_or(0),
                    ));
                }
                for cmd in p["commands"].as_array().cloned().unwrap_or_default() {
                    human.push_str(&format!(
                        "  command {} — {}\n",
                        cmd["name"].as_str().unwrap_or("?"),
                        cmd["description"].as_str().unwrap_or("")
                    ));
                }
            }
            for f in r["failures"].as_array().cloned().unwrap_or_default() {
                human.push_str(&format!(
                    "FAILED {:?}: {}\n",
                    f["dir"].as_str().unwrap_or("?"),
                    f["error"].as_str().unwrap_or("?")
                ));
            }
            emit(json, &r, human.trim_end().to_string());
            Ok(())
        }
        PluginOp::Run {
            plugin,
            command,
            args,
        } => {
            // Commands run locally (cwd = plugin dir) with the socket
            // exported so scripts can call back into the server.
            let reg = signaltty_plugin::PluginRegistry::load(signaltty_core::paths::plugin_dir());
            let (dir, cmds) = reg
                .commands(&plugin)
                .ok_or_else(|| CliError::Usage(format!("no such plugin '{plugin}'")))?;
            let cmd = cmds.iter().find(|c| c.name == command).ok_or_else(|| {
                CliError::Usage(format!("plugin '{plugin}' has no command '{command}'"))
            })?;
            let argv = signaltty_plugin::resolve_argv(&dir, &cmd.run);
            let status = std::process::Command::new(&argv[0])
                .args(&argv[1..])
                .args(&args)
                .current_dir(&dir)
                .env("SIGNALTTY_SOCKET", &socket)
                .env("SIGNALTTY_PLUGIN_DIR", &dir)
                .env("SIGNALTTY_PLUGIN_NAME", &plugin)
                .status()
                .map_err(|e| CliError::Usage(format!("cannot run plugin command: {e}")))?;
            if status.success() {
                Ok(())
            } else {
                Err(CliError::Usage(format!(
                    "plugin command exited with {status}"
                )))
            }
        }
    }
}

async fn pane_cmd(socket: PathBuf, json: bool, op: PaneOp) -> Result<(), CliError> {
    match op {
        PaneOp::Attach { id } => attach::run(&socket, &id).await,
        _ => {
            let mut c = Client::connect(&socket).await?;
            match op {
                PaneOp::Attach { .. } => unreachable!(),
                PaneOp::Spawn {
                    workspace,
                    tab,
                    cwd,
                    agent,
                    cmd,
                } => {
                    let argv = if cmd.is_empty() { shell_cmd() } else { cmd };
                    let mut p = json!({"workspace_id": workspace, "argv": argv});
                    if let Some(t) = tab {
                        p["tab_id"] = json!(t);
                    }
                    if let Some(cwd) = cwd {
                        p["cwd"] = json!(cwd);
                    }
                    if let Some(a) = agent {
                        p["agent_hint"] = json!(a);
                    }
                    let r = c.call("pane.spawn", p).await?;
                    integration_notice(&r, json);
                    let id = r["pane"]["id"].as_str().unwrap_or("?").to_string();
                    emit(json, &r, format!("pane {id}"));
                    Ok(())
                }
                PaneOp::Split {
                    id,
                    direction,
                    cwd,
                    cmd,
                } => {
                    let mut p = json!({"pane_id": id, "direction": direction});
                    if let Some(cwd) = cwd {
                        p["cwd"] = json!(cwd);
                    }
                    if !cmd.is_empty() {
                        p["argv"] = json!(cmd);
                    }
                    let r = c.call("pane.split", p).await?;
                    integration_notice(&r, json);
                    let nid = r["pane"]["id"].as_str().unwrap_or("?").to_string();
                    emit(json, &r, format!("pane {nid}"));
                    Ok(())
                }
                PaneOp::Get { id } => {
                    let r = c.call("pane.get", json!({"pane_id": id})).await?;
                    let p = &r["pane"];
                    let mut human = format!(
                        "{} {} live={:?} lifecycle={} attention={}",
                        p["id"].as_str().unwrap_or("?"),
                        p["title"].as_str().unwrap_or("?"),
                        p["live"].to_string(),
                        p["lifecycle"].as_str().unwrap_or("?"),
                        p["attention"].as_str().unwrap_or("?"),
                    );
                    // Structured decision requests print as data (directive 2):
                    // prompt plus one line per option, never prose buttons.
                    if let Some(d) = p.get("pending_decision") {
                        human.push_str(&format!(
                            "\ndecision {}: {}",
                            d["id"].as_str().unwrap_or("?"),
                            d["prompt"].as_str().unwrap_or("?"),
                        ));
                        for o in d["options"].as_array().cloned().unwrap_or_default() {
                            human.push_str(&format!(
                                "\n  {} — {}",
                                o["id"].as_str().unwrap_or("?"),
                                o["label"].as_str().unwrap_or("?"),
                            ));
                        }
                        if !d["answerable"].as_bool().unwrap_or(true) {
                            human.push_str("\n  (read-only: answer in the terminal)");
                        }
                    }
                    emit(json, &r, human);
                    Ok(())
                }
                PaneOp::Input { id, data, stdin } => {
                    let bytes = if stdin {
                        use tokio::io::AsyncReadExt;
                        let mut buf = Vec::new();
                        tokio::io::stdin()
                            .read_to_end(&mut buf)
                            .await
                            .map_err(|e| CliError::Io(e.to_string()))?;
                        buf
                    } else {
                        data.unwrap_or_default().into_bytes()
                    };
                    let r = c
                        .call(
                            "pane.input",
                            json!({
                                "pane_id": id,
                                "data_b64": base64::engine::general_purpose::STANDARD.encode(&bytes),
                            }),
                        )
                        .await?;
                    let n = r["written"].as_u64().unwrap_or(0);
                    emit(json, &r, format!("wrote {n} bytes"));
                    Ok(())
                }
                PaneOp::Resize { id, cols, rows } => {
                    let r = c
                        .call(
                            "pane.resize",
                            json!({"pane_id": id, "cols": cols, "rows": rows}),
                        )
                        .await?;
                    emit(json, &r, format!("resized {id} to {cols}x{rows}"));
                    Ok(())
                }
                PaneOp::Signal { id, signal, group } => {
                    let r = c
                        .call(
                            "pane.signal",
                            json!({"pane_id": id, "signal": signal, "group": group}),
                        )
                        .await?;
                    emit(json, &r, format!("sent {signal} to {id}"));
                    Ok(())
                }
                PaneOp::Read {
                    id,
                    mode,
                    lines,
                    raw,
                } => {
                    let r = c
                        .call("pane.read", json!({"pane_id": id, "mode": mode, "lines": lines, "strip_ansi": !raw}))
                        .await?;
                    if json {
                        println!("{}", serde_json::to_string(&r).unwrap());
                    } else {
                        println!("{}", r["text"].as_str().unwrap_or(""));
                    }
                    Ok(())
                }
                PaneOp::Close { id } => {
                    let r = c.call("pane.close", json!({"pane_id": id})).await?;
                    emit(json, &r, format!("closed {id}"));
                    Ok(())
                }
                PaneOp::MarkSeen { id } => {
                    let r = c.call("pane.mark_seen", json!({"pane_id": id})).await?;
                    emit(json, &r, format!("marked seen {id}"));
                    Ok(())
                }
                PaneOp::Resume { id } => {
                    let r = c.call("pane.resume", json!({"pane_id": id})).await?;
                    integration_notice(&r, json);
                    let pid = r["pane"]["id"].as_str().unwrap_or("?").to_string();
                    emit(json, &r, format!("resumed {pid}"));
                    Ok(())
                }
            }
        }
    }
}

fn integration_notice(result: &Value, json: bool) {
    if !json {
        if let Some(notice) = result["integration"]["notice"].as_str() {
            eprintln!("{notice}");
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn null_baseline_is_a_usage_error_not_a_current_state_wait() {
        let err = parse_wait_baseline("null").unwrap_err();
        assert!(matches!(err, CliError::Usage(_)));
    }

    #[test]
    fn malformed_baseline_is_a_usage_error() {
        let err = parse_wait_baseline("{not json").unwrap_err();
        assert!(matches!(err, CliError::Usage(_)));
    }

    #[test]
    fn object_baseline_passes_through() {
        let baseline = parse_wait_baseline("{\"pane_id\":\"p\"}").unwrap();
        assert_eq!(baseline["pane_id"], "p");
    }
}
