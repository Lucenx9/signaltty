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
#[allow(clippy::large_enum_variant)]
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
        #[arg(last = true)]
        resume_argv: Vec<String>,
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
    /// Orchestrated background tasks.
    Task {
        #[command(subcommand)]
        op: TaskOp,
    },
    /// Report a task outcome with structured result.
    Report {
        #[arg(long, value_parser = ["completed", "failed", "rejected"])]
        status: String,
        #[arg(long)]
        summary: String,
        #[arg(long)]
        task: Option<String>,
        #[arg(long)]
        pane: Option<String>,
        #[arg(long)]
        artifacts: Option<String>,
        #[arg(long)]
        evidence: Option<String>,
    },
    /// List panes requiring user attention, ranked by severity then recency.
    Attention {
        #[arg(long)]
        limit: Option<usize>,
    },
}

#[derive(Debug, Subcommand)]
#[allow(clippy::large_enum_variant)]
enum TaskOp {
    /// Start a new orchestrated background task.
    Start {
        #[arg(long)]
        repo: String,
        #[arg(long, conflicts_with = "objective_file")]
        objective: Option<String>,
        #[arg(long)]
        objective_file: Option<PathBuf>,
        #[arg(long)]
        constraints: Option<String>,
        #[arg(long)]
        output_format: Option<String>,
        #[arg(long, value_delimiter = ',')]
        acceptance_criteria: Option<Vec<String>>,
        #[arg(long)]
        agent: Option<String>,
        #[arg(long)]
        label: Option<String>,
        #[arg(long)]
        parent_pane: Option<String>,
        #[arg(long)]
        context: Option<String>,
        /// Idempotency key: a retry with the same key returns the existing task.
        #[arg(long)]
        client_request_id: Option<String>,
        #[arg(long)]
        base_ref: Option<String>,
        #[arg(long)]
        fetch_first: bool,
        #[arg(long)]
        branch: Option<String>,
        #[arg(long)]
        path: Option<String>,
        #[arg(long)]
        ready_timeout_s: Option<u64>,
        #[arg(long)]
        stall_timeout_s: Option<u64>,
        #[arg(last = true)]
        cmd: Vec<String>,
    },
    /// Get details of an orchestrated task.
    Get { id: String },
    /// List orchestrated tasks.
    List {
        #[arg(long)]
        context: Option<String>,
        #[arg(long)]
        state: Option<String>,
        #[arg(long)]
        limit: Option<usize>,
    },
    /// Wait for a task or context to settle.
    Wait {
        #[arg(value_name = "TASK_ID")]
        id: Option<String>,
        #[arg(long = "id", hide = true)]
        id_flag: Option<String>,
        #[arg(long)]
        context: Option<String>,
        #[arg(long, default_value = "settled", value_delimiter = ',')]
        until: Vec<String>,
        #[arg(long)]
        timeout: Option<u64>,
    },
    /// View diff of task worktree vs base commit.
    Diff { id: String },
    /// View diff of a specific file in task worktree vs base commit.
    FileDiff { id: String, path: String },
    /// Explicitly merge or discard a completed/working task.
    Finish {
        id: String,
        #[arg(long, conflicts_with = "discard")]
        merge: bool,
        #[arg(long)]
        discard: bool,
        #[arg(long)]
        target: Option<String>,
        /// Delete the task branch (already the default for --merge; opt-in for --discard).
        #[arg(long)]
        delete_branch: bool,
        /// Keep the task branch after --merge (--discard always keeps it unless --delete-branch).
        #[arg(long, conflicts_with_all = ["delete_branch", "discard"])]
        keep_branch: bool,
        #[arg(long)]
        ignore_dirty: bool,
    },
    /// Open a GitHub pull request for a completed task.
    Pr {
        id: String,
        #[arg(long)]
        title: Option<String>,
        #[arg(long)]
        body: Option<String>,
        #[arg(long)]
        draft: bool,
    },
    /// Refresh pull request status (state, checks, review) from GitHub.
    #[command(name = "pr-refresh")]
    PrRefresh { id: Option<String> },
    /// Cancel an orchestrated task.
    Cancel { id: String },
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
        #[arg(long)]
        parent_pane: Option<String>,
        #[arg(long)]
        label: Option<String>,
        #[arg(long, value_parser = ["fork", "subagent"])]
        relationship: Option<String>,
        #[arg(last = true)]
        cmd: Vec<String>,
    },
    /// Submit prompt to an agent pane with bracketed paste and delayed Enter.
    Submit {
        id: String,
        /// Literal text to send (use --stdin for piped input).
        #[arg(long, conflicts_with = "stdin")]
        text: Option<String>,
        #[arg(long)]
        stdin: bool,
        #[arg(long)]
        submit_delay_ms: Option<u64>,
        #[arg(long, alias = "stall-timeout")]
        stall_timeout_s: Option<u64>,
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
        #[arg(long, value_parser = ["screen", "tail", "rendered"], default_value = "tail")]
        mode: String,
        #[arg(long, default_value_t = 200)]
        lines: u64,
        #[arg(long)]
        after_seq: Option<u64>,
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
    /// Show which screen rules match a pane and whether they apply.
    Explain {
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
    // Exit without dropping the runtime: attach's pending stdin read sits on
    // a blocking thread that would hold shutdown until the next keypress.
    let _ = std::io::Write::flush(&mut std::io::stdout());
    std::process::exit(0);
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
            resume_argv,
        } => {
            let mut c = Client::connect(&socket).await?;
            let mut p = json!({"pane_id": pane, "agent_session_id": session});
            if let Some(a) = agent {
                p["agent"] = json!(a);
            }
            if !resume_argv.is_empty() {
                p["resume_argv"] = json!(resume_argv);
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
        Command::Task { op } => task_cmd(socket, json, op).await,
        Command::Report {
            status,
            summary,
            task,
            pane,
            artifacts,
            evidence,
        } => {
            let task_id = task.or_else(|| {
                std::env::var("SIGNALTTY_TASK")
                    .ok()
                    .filter(|s| !s.is_empty())
            });
            let pane_id = pane.or_else(|| {
                std::env::var("SIGNALTTY_PANE")
                    .ok()
                    .filter(|s| !s.is_empty())
            });
            if task_id.is_none() && pane_id.is_none() {
                return Err(CliError::Usage(
                    "must provide --task, --pane, or set $SIGNALTTY_TASK or $SIGNALTTY_PANE"
                        .to_string(),
                ));
            }
            let parsed_artifacts: Option<Vec<Value>> = if let Some(ref a) = artifacts {
                if let Ok(v) = serde_json::from_str::<Vec<Value>>(a) {
                    Some(v)
                } else if let Ok(content) = std::fs::read_to_string(a) {
                    Some(serde_json::from_str::<Vec<Value>>(&content).map_err(|e| {
                        CliError::Usage(format!("failed to parse artifacts JSON file: {e}"))
                    })?)
                } else {
                    return Err(CliError::Usage(format!(
                        "artifacts must be a valid JSON array or path to a JSON file: {a}"
                    )));
                }
            } else {
                None
            };
            let parsed_evidence: Option<Value> = if let Some(ref ev) = evidence {
                if let Ok(v) = serde_json::from_str::<Value>(ev) {
                    Some(v)
                } else if let Ok(content) = std::fs::read_to_string(ev) {
                    Some(serde_json::from_str::<Value>(&content).map_err(|e| {
                        CliError::Usage(format!("failed to parse evidence JSON file: {e}"))
                    })?)
                } else {
                    return Err(CliError::Usage(format!(
                        "evidence must be valid JSON or path to a JSON file: {ev}"
                    )));
                }
            } else {
                None
            };
            let mut payload = json!({
                "status": status,
                "summary": summary,
            });
            if let Some(tid) = task_id {
                payload["task_id"] = json!(tid);
            }
            if let Some(pid) = pane_id {
                payload["pane_id"] = json!(pid);
            }
            if let Some(art) = parsed_artifacts {
                payload["artifacts"] = json!(art);
            }
            if let Some(ev) = parsed_evidence {
                payload["evidence"] = ev;
            }
            let mut c = Client::connect(&socket).await?;
            let r = c.call("task.report", payload).await?;
            let task = &r["task"];
            emit(
                json,
                &r,
                format!(
                    "reported {}: task {} is {}",
                    status,
                    task["id"].as_str().unwrap_or("?"),
                    task["state"].as_str().unwrap_or("?")
                ),
            );
            Ok(())
        }
        Command::Attention { limit } => {
            let mut p = json!({});
            if let Some(lim) = limit {
                p["limit"] = json!(lim);
            }
            let mut c = Client::connect(&socket).await?;
            let r = c.call("attention.pending", p).await?;
            let mut human = String::new();
            if let Some(panes) = r["panes"].as_array() {
                if panes.is_empty() {
                    human.push_str("no panes require attention\n");
                } else {
                    for p in panes {
                        human.push_str(&format!(
                            "{:<8} {:<20} {:<12} {}\n",
                            p["pane_id"].as_str().unwrap_or("?"),
                            p["attention"].as_str().unwrap_or("?"),
                            p["task_id"].as_str().unwrap_or("-"),
                            p["last_message"].as_str().unwrap_or("")
                        ));
                    }
                }
            }
            emit(json, &r, human.trim_end().to_string());
            Ok(())
        }
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

/// One header line, then one line per rule; `*` marks the winning rule.
fn explain_text(r: &Value) -> String {
    let mut out = format!(
        "{} kind={} source={} live={} hooked={} classifies={}",
        r["pane_id"].as_str().unwrap_or("?"),
        r["kind"].as_str().unwrap_or("?"),
        r["source"].as_str().unwrap_or("?"),
        r["live"],
        r["hooked"],
        r["classifies"],
    );
    let winner = r["matched"]["index"].as_u64();
    for (i, rule) in r["rules"].as_array().into_iter().flatten().enumerate() {
        out.push_str(&format!(
            "\n{} {} {} p={} {} {}",
            if winner == Some(i as u64) { "*" } else { " " },
            rule["id"].as_str().unwrap_or("?"),
            rule["state"].as_str().unwrap_or("?"),
            rule["priority"],
            rule["region"].as_str().unwrap_or("?"),
            if rule["matched"] == true {
                "match"
            } else {
                "-"
            },
        ));
    }
    out
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
                    parent_pane,
                    label,
                    relationship,
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
                    if let Some(pp) = parent_pane {
                        p["parent_pane_id"] = json!(pp);
                    }
                    if let Some(l) = label {
                        p["label"] = json!(l);
                    }
                    if let Some(rel) = relationship {
                        p["relationship"] = json!(rel);
                    }
                    let r = c.call("pane.spawn", p).await?;
                    integration_notice(&r, json);
                    let id = r["pane"]["id"].as_str().unwrap_or("?").to_string();
                    emit(json, &r, format!("pane {id}"));
                    Ok(())
                }
                PaneOp::Submit {
                    id,
                    text,
                    stdin,
                    submit_delay_ms,
                    stall_timeout_s,
                } => {
                    let text = if stdin {
                        use tokio::io::AsyncReadExt;
                        let mut buf = String::new();
                        tokio::io::stdin()
                            .read_to_string(&mut buf)
                            .await
                            .map_err(|e| CliError::Io(e.to_string()))?;
                        buf
                    } else {
                        text.ok_or_else(|| {
                            CliError::Usage("--text or --stdin required".to_string())
                        })?
                    };
                    let mut p = json!({
                        "pane_id": id,
                        "text": text,
                    });
                    if let Some(d) = submit_delay_ms {
                        p["submit_delay_ms"] = json!(d);
                    }
                    if let Some(t) = stall_timeout_s {
                        p["stall_timeout_s"] = json!(t);
                    }
                    let r = c.call("pane.submit", p).await?;
                    let outcome = r["outcome"].as_str().unwrap_or("?");
                    let seq = r["transition_seq"].as_u64().unwrap_or(0);
                    let lc = r["lifecycle"].as_str().unwrap_or("?");
                    emit(
                        json,
                        &r,
                        format!("submitted: outcome={outcome} seq={seq} lifecycle={lc}"),
                    );
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
                    after_seq,
                    raw,
                } => {
                    let r = c
                        .call("pane.read", json!({"pane_id": id, "mode": mode, "lines": lines, "after_seq": after_seq, "strip_ansi": !raw}))
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
                PaneOp::Explain { id } => {
                    let r = c.call("pane.explain", json!({"pane_id": id})).await?;
                    emit(json, &r, explain_text(&r));
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

async fn task_cmd(socket: PathBuf, json: bool, op: TaskOp) -> Result<(), CliError> {
    let mut c = Client::connect(&socket).await?;
    match op {
        TaskOp::Start {
            repo,
            objective,
            objective_file,
            constraints,
            output_format,
            acceptance_criteria,
            agent,
            label,
            parent_pane,
            context,
            client_request_id,
            base_ref,
            fetch_first,
            branch,
            path,
            ready_timeout_s,
            stall_timeout_s,
            cmd,
        } => {
            let obj_text = if let Some(path) = objective_file {
                std::fs::read_to_string(&path)
                    .map_err(|e| CliError::Io(format!("failed to read objective file: {e}")))?
            } else if let Some(text) = objective {
                text
            } else {
                return Err(CliError::Usage(
                    "--objective or --objective-file required".to_string(),
                ));
            };

            let mut contract = json!({
                "objective": obj_text,
            });
            if let Some(con) = constraints {
                contract["constraints"] = json!(con);
            }
            if let Some(fmt) = output_format {
                contract["output_format"] = json!(fmt);
            }
            if let Some(crit) = acceptance_criteria {
                contract["acceptance_criteria"] = json!(crit);
            }

            // The server resolves paths against its own cwd, not ours.
            let abs = |path: &str| {
                std::path::absolute(path)
                    .map_err(|e| CliError::Usage(format!("cannot resolve path '{path}': {e}")))
            };
            let mut p = json!({
                "repo": abs(&repo)?,
                "contract": contract,
            });
            if let Some(a) = agent {
                p["agent"] = json!(a);
            }
            if let Some(l) = label {
                p["label"] = json!(l);
            }
            if let Some(pp) = parent_pane {
                p["parent_pane_id"] = json!(pp);
            }
            if let Some(ctx) = context {
                p["context_id"] = json!(ctx);
            }
            if let Some(id) = client_request_id {
                p["client_request_id"] = json!(id);
            }
            if let Some(br) = base_ref {
                p["base_ref"] = json!(br);
            }
            if fetch_first {
                p["fetch_first"] = json!(true);
            }
            if let Some(b) = branch {
                p["branch"] = json!(b);
            }
            if let Some(pa) = path {
                p["path"] = json!(abs(&pa)?);
            }
            if let Some(rt) = ready_timeout_s {
                p["ready_timeout_s"] = json!(rt);
            }
            if let Some(st) = stall_timeout_s {
                p["stall_timeout_s"] = json!(st);
            }
            if !cmd.is_empty() {
                p["argv"] = json!(cmd);
            }

            let r = c.call("task.start", p).await?;
            let task_id = r["task"]["id"].as_str().unwrap_or("?").to_string();
            let pane_id = r["pane"]["id"].as_str().unwrap_or("?").to_string();
            let state = r["task"]["state"].as_str().unwrap_or("?").to_string();
            emit(
                json,
                &r,
                format!("task {task_id} pane {pane_id} (state={state})"),
            );
            Ok(())
        }
        TaskOp::Get { id } => {
            let r = c.call("task.get", json!({"task_id": id})).await?;
            let t = &r["task"];
            let human = format!(
                "{} state={} repo={} branch={}",
                t["id"].as_str().unwrap_or("?"),
                t["state"].as_str().unwrap_or("?"),
                t["source_repo"].as_str().unwrap_or("?"),
                t["branch"].as_str().unwrap_or("?"),
            );
            emit(json, &r, human);
            Ok(())
        }
        TaskOp::List {
            context,
            state,
            limit,
        } => {
            let mut p = json!({});
            if let Some(ctx) = context {
                p["context_id"] = json!(ctx);
            }
            if let Some(st) = state {
                p["state"] = json!(st);
            }
            if let Some(l) = limit {
                p["limit"] = json!(l);
            }
            let r = c.call("task.list", p).await?;
            let mut human = String::new();
            let tasks = r["tasks"].as_array().cloned().unwrap_or_default();
            if tasks.is_empty() {
                human.push_str("(no tasks)\n");
            }
            for t in &tasks {
                human.push_str(&format!(
                    "{} state={} label={} context={}\n",
                    t["id"].as_str().unwrap_or("?"),
                    t["state"].as_str().unwrap_or("?"),
                    t["label"].as_str().unwrap_or("?"),
                    t["context_id"].as_str().unwrap_or("?"),
                ));
            }
            emit(json, &r, human.trim_end().to_string());
            Ok(())
        }
        TaskOp::Wait {
            id,
            id_flag,
            context,
            until,
            timeout,
        } => {
            let target_id = id.or(id_flag);
            let mut p = json!({
                "until": until,
            });
            if let Some(tid) = target_id {
                p["task_id"] = json!(tid);
            }
            if let Some(ctx) = context {
                p["context_id"] = json!(ctx);
            }
            if let Some(to) = timeout {
                p["timeout_s"] = json!(to);
            }
            let r = c.call("task.wait", p).await?;
            let count = r["tasks"].as_array().map(|a| a.len()).unwrap_or(0);
            emit(json, &r, format!("satisfied: {count} tasks"));
            Ok(())
        }
        TaskOp::Diff { id } => {
            let r = c.call("task.diff", json!({ "task_id": id })).await?;
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
        TaskOp::FileDiff { id, path } => {
            let r = c
                .call("task.file_diff", json!({ "task_id": id, "path": path }))
                .await?;
            let mut human = String::new();
            if r["untracked"].as_bool().unwrap_or(false) {
                human.push_str(&format!(
                    "untracked {}\n",
                    r["path"].as_str().unwrap_or("?")
                ));
            } else {
                human.push_str(&format!("diff {}\n", r["path"].as_str().unwrap_or("?")));
            }
            if let Some(content) = r.get("content") {
                if let Some(kind) = content.get("kind").and_then(|k| k.as_str()) {
                    match kind {
                        "text" => {
                            if let Some(hunks) = content.get("hunks").and_then(|h| h.as_array()) {
                                for hunk in hunks {
                                    if let Some(heading) =
                                        hunk.get("heading").and_then(|h| h.as_str())
                                    {
                                        human.push_str(&format!("{heading}\n"));
                                    }
                                    if let Some(lines) =
                                        hunk.get("lines").and_then(|l| l.as_array())
                                    {
                                        for l in lines {
                                            let text = l
                                                .get("text")
                                                .and_then(|t| t.as_str())
                                                .unwrap_or("");
                                            let prefix =
                                                match l.get("kind").and_then(|k| k.as_str()) {
                                                    Some("added") => "+",
                                                    Some("removed") => "-",
                                                    Some("no_newline") => "\\ ",
                                                    _ => " ",
                                                };
                                            human.push_str(&format!("{prefix}{text}\n"));
                                        }
                                    }
                                }
                            }
                        }
                        "binary" => human.push_str("(binary file)\n"),
                        "unchanged" => human.push_str("(unchanged)\n"),
                        _ => {}
                    }
                }
            }
            emit(json, &r, human.trim_end().to_string());
            Ok(())
        }
        TaskOp::Finish {
            id,
            merge,
            discard,
            target,
            delete_branch,
            keep_branch,
            ignore_dirty,
        } => {
            let mode = if merge {
                "merge"
            } else if discard {
                "discard"
            } else {
                return Err(CliError::Usage("must specify --merge or --discard".into()));
            };
            let mut p = json!({
                "task_id": id,
                "mode": mode,
            });
            if let Some(t) = target {
                p["target_ref"] = json!(t);
            }
            if delete_branch || keep_branch {
                p["delete_branch"] = json!(delete_branch);
            }
            if ignore_dirty {
                p["ignore_dirty"] = json!(true);
            }
            let r = c.call("task.finish", p).await?;
            let tid = r["task"]["id"].as_str().unwrap_or("?").to_string();
            let outcome = r["task"]["disposition"]["outcome"].as_str().unwrap_or("?");
            let mut summary = format!("finished task {tid}: {outcome}");
            if let Some(merge) = r.get("merge") {
                if let (Some(target), Some(sha)) = (
                    merge.get("target").and_then(|t| t.as_str()),
                    merge.get("sha").and_then(|s| s.as_str()),
                ) {
                    summary.push_str(&format!(" into {target} ({sha})"));
                }
            }
            if let Some(err) = r.get("cleanup_error").and_then(|e| e.as_str()) {
                summary.push_str(&format!(" (cleanup warning: {err})"));
            }
            emit(json, &r, summary);
            Ok(())
        }
        TaskOp::Pr {
            id,
            title,
            body,
            draft,
        } => {
            let mut p = json!({
                "task_id": id,
            });
            if let Some(t) = title {
                p["title"] = json!(t);
            }
            if let Some(b) = body {
                p["body"] = json!(b);
            }
            if draft {
                p["draft"] = json!(true);
            }
            let r = c.call("task.pr_open", p).await?;
            let tid = r["task"]["id"].as_str().unwrap_or("?").to_string();
            let pr = &r["task"]["pr"];
            let url = pr["url"].as_str().unwrap_or("?");
            let num = pr["number"].as_u64().unwrap_or(0);
            emit(
                json,
                &r,
                format!("opened pull request #{num} for task {tid}: {url}"),
            );
            Ok(())
        }
        TaskOp::PrRefresh { id } => {
            let mut p = json!({});
            if let Some(tid) = id {
                p["task_id"] = json!(tid);
            }
            let r = c.call("task.pr_refresh", p).await?;
            let tasks = r["tasks"].as_array();
            let count = tasks.map_or(0, |t| t.len());
            let mut summary = format!("refreshed {count} pull request(s)");
            if let Some(tasks) = tasks {
                for t in tasks {
                    let tid = t["id"].as_str().unwrap_or("?");
                    if let Some(pr) = t.get("pr") {
                        let num = pr["number"].as_u64().unwrap_or(0);
                        let state = pr["state"].as_str().unwrap_or("?");
                        let checks = pr["checks"].as_str().unwrap_or("none");
                        let review = pr["review"].as_str().unwrap_or("none");
                        summary.push_str(&format!(
                            "\n  task {tid}: #{num} [{state}] (checks: {checks}, review: {review})"
                        ));
                    }
                }
            }
            emit(json, &r, summary);
            Ok(())
        }
        TaskOp::Cancel { id } => {
            let r = c.call("task.cancel", json!({"task_id": id})).await?;
            let tid = r["task"]["id"].as_str().unwrap_or("?").to_string();
            emit(json, &r, format!("canceled task {tid}"));
            Ok(())
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn explain_marks_only_the_winning_position() {
        let r = json!({
            "pane_id": "p", "kind": "generic", "source": "user",
            "live": true, "hooked": false, "classifies": true,
            "matched": {"id": "ask", "state": "blocked", "priority": 5, "index": 1},
            "rules": [
                {"id": "ask", "state": "blocked", "priority": 0, "region": "bottom(12)", "matched": false},
                {"id": "ask", "state": "blocked", "priority": 5, "region": "bottom(12)", "matched": true},
            ],
        });
        let text = explain_text(&r);
        let starred: Vec<_> = text.lines().filter(|l| l.starts_with('*')).collect();
        assert_eq!(starred, ["* ask blocked p=5 bottom(12) match"]);
    }

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

    #[test]
    fn test_task_pr_cli_parsing() {
        let args =
            Args::try_parse_from(["signaltty", "task", "pr", "task-123", "--draft"]).unwrap();
        match args.cmd {
            Command::Task {
                op:
                    TaskOp::Pr {
                        id,
                        draft,
                        title,
                        body,
                    },
            } => {
                assert_eq!(id, "task-123");
                assert!(draft);
                assert!(title.is_none());
                assert!(body.is_none());
            }
            _ => panic!("wrong command parsed"),
        }

        let args2 = Args::try_parse_from(["signaltty", "task", "pr-refresh", "task-123"]).unwrap();
        match args2.cmd {
            Command::Task {
                op: TaskOp::PrRefresh { id },
            } => {
                assert_eq!(id, Some("task-123".to_string()));
            }
            _ => panic!("wrong command parsed"),
        }

        let args3 = Args::try_parse_from(["signaltty", "task", "pr-refresh"]).unwrap();
        match args3.cmd {
            Command::Task {
                op: TaskOp::PrRefresh { id },
            } => {
                assert_eq!(id, None);
            }
            _ => panic!("wrong command parsed"),
        }
    }
}
