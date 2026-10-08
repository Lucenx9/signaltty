//! Method dispatch for `signaltty/1`. Every method validates params,
//! mutates the store, emits events, and requests persistence.

use std::collections::HashMap;
use std::sync::Arc;

use base64::Engine;
use chrono::Utc;
use serde_json::{json, Value};
use tokio::sync::broadcast;

use std::time::Duration;

use crate::submit::SubmitCtx;
use signaltty_core::model::{
    AgentKind, LiveState, Notification, NotificationSeverity, Pane, PtySize, RestoreState,
    SplitDir, Tab, Workspace,
};
use signaltty_core::state::{Attention, Lifecycle, TaskState};
use signaltty_core::{new_notif_id, new_tab_id, new_ws_id};
use signaltty_proto::{code, event, method, Request, Response};
use signaltty_term::TerminalBackend;

use crate::config::Config;
use crate::params::{self, bad_params, decode, parse_severity, validate_until};
use crate::pty::{PtyManager, SpawnRequest};
use crate::store::{SharedStore, StoredEvent};

pub struct Ctx {
    pub store: SharedStore,
    pub approvals: crate::approvals::Approvals,
    pub worktrees: crate::worktrees::Worktrees,
    pub bcast: broadcast::Sender<StoredEvent>,
    pub ptys: PtyManager,
    pub config: Config,
    pub shutdown: Arc<tokio::sync::Notify>,
    pub plugins: signaltty_plugin::PluginRegistry,
    /// Manifest detection overlays (data, not code — see ADR-0009).
    /// Consulted before builtins by [`Ctx::adapter`] and spawn detection.
    pub overlays: Vec<signaltty_agent::OverlayAdapter>,
}

/// Load detection overlays from `<dir>/*.toml`. Per-file failure
/// isolation: malformed files are logged and skipped, never fatal.
pub fn load_overlays(dir: &std::path::Path) -> Vec<signaltty_agent::OverlayAdapter> {
    let mut out = Vec::new();
    let Ok(entries) = std::fs::read_dir(dir) else {
        return out;
    };
    let mut files: Vec<_> = entries
        .flatten()
        .map(|e| e.path())
        .filter(|p| p.extension().is_some_and(|e| e == "toml"))
        .collect();
    files.sort();
    for path in files {
        let name = path
            .file_stem()
            .map(|s| s.to_string_lossy().to_string())
            .unwrap_or_default();
        match std::fs::read_to_string(&path) {
            Ok(text) => match signaltty_agent::parse_manifest(&text) {
                Ok(manifest) => match signaltty_agent::OverlayAdapter::new(manifest) {
                    Ok(overlay) => {
                        tracing::info!(
                            "agent overlay '{name}' ({}): {}",
                            overlay.kind().as_str(),
                            path.display()
                        );
                        out.push(overlay);
                    }
                    Err(e) => tracing::warn!("agent manifest {}: {e} (skipped)", path.display()),
                },
                Err(e) => tracing::warn!("agent manifest {}: {e} (skipped)", path.display()),
            },
            Err(e) => tracing::warn!("agent manifest {}: {e} (skipped)", path.display()),
        }
    }
    out
}

impl Ctx {
    /// Overlay-first adapter routing (manifests are data, not code): the
    /// first overlay for the named kind wins, else the builtin. Unknown
    /// names stay `None` so callers keep reporting `BAD_PARAMS`.
    pub fn adapter(&self, name: &str) -> Option<&dyn signaltty_agent::AgentAdapter> {
        let kind = AgentKind::parse(name)?;
        Some(self.adapter_for_kind(kind))
    }

    pub fn adapter_for_kind(&self, kind: AgentKind) -> &dyn signaltty_agent::AgentAdapter {
        self.overlays
            .iter()
            .find(|o| o.kind() == kind)
            .map(|o| o as &dyn signaltty_agent::AgentAdapter)
            .unwrap_or_else(|| signaltty_agent::adapter_for_kind(kind))
    }

    pub fn detect_kind(&self, argv: &[String]) -> AgentKind {
        signaltty_agent::detect_kind_with_overlays(argv, &self.overlays)
    }

    pub fn mark_persist(&self) {
        self.ptys.mark_persist();
    }

    pub fn emit(&self, name: &str, payload: Value) {
        // The audit lives in Store::emit: every broadcast event is logged
        // at the single seam, whatever the caller.
        self.store.write().unwrap().emit(name, payload);
    }
}

#[derive(Default)]
pub struct ConnEffect {
    pub attach: Vec<String>,
    pub detach: Vec<String>,
    pub subscribe: Option<Vec<String>>,
    pub task_ids: Option<Vec<String>>,
    pub replay: Option<Vec<StoredEvent>>,
    pub fence: Option<u64>,
    pub close: bool,
}

type Handler = Result<(Value, ConnEffect), (String, String)>;

fn pane_result(pane: &Pane) -> Value {
    json!({"pane": pane})
}

pub async fn dispatch(ctx: &Ctx, req: &Request) -> (Response, ConnEffect) {
    let out: Handler = match req.method.as_str() {
        method::SERVER_STATUS => h_server_status(ctx, &req.params),
        method::SERVER_SHUTDOWN => h_server_shutdown(ctx, &req.params),
        method::SERVER_SCHEMA => h_server_schema(),
        method::WORKSPACE_CREATE => h_workspace_create(ctx, &req.params),
        method::WORKSPACE_LIST => h_workspace_list(ctx),
        method::WORKSPACE_GET => h_workspace_get(ctx, &req.params),
        method::WORKSPACE_RENAME => h_workspace_rename(ctx, &req.params),
        method::WORKSPACE_CLOSE => h_workspace_close(ctx, &req.params),
        method::WORKSPACE_REFRESH_GIT => h_workspace_refresh_git(ctx, &req.params),
        method::WORKSPACE_DIFF => h_workspace_diff(ctx, &req.params),
        method::WORKSPACE_FILE_DIFF => h_workspace_file_diff(ctx, &req.params).await,
        method::WORKTREE_LIST => match decode::<params::WorkspaceId>(&req.params) {
            Ok(p) => crate::worktrees::list(ctx, p)
                .await
                .map(|v| (v, ConnEffect::default())),
            Err(e) => Err(e),
        },
        method::WORKTREE_CREATE => match decode::<params::WorktreeCreate>(&req.params) {
            Ok(p) => crate::worktrees::create(ctx, p)
                .await
                .map(|v| (v, ConnEffect::default())),
            Err(e) => Err(e),
        },
        method::WORKTREE_OPEN => match decode::<params::WorktreeOpen>(&req.params) {
            Ok(p) => crate::worktrees::open(ctx, p)
                .await
                .map(|v| (v, ConnEffect::default())),
            Err(e) => Err(e),
        },
        method::WORKTREE_REMOVE => match decode::<params::WorktreeRemove>(&req.params) {
            Ok(p) => crate::worktrees::remove(ctx, p)
                .await
                .map(|v| (v, ConnEffect::default())),
            Err(e) => Err(e),
        },
        method::TAB_CREATE => h_tab_create(ctx, &req.params),
        method::TAB_CLOSE => h_tab_close(ctx, &req.params),
        method::TAB_SET_LAYOUT => h_tab_set_layout(ctx, &req.params),
        method::TAB_SET_RATIO => h_tab_set_ratio(ctx, &req.params),
        method::PANE_SPAWN => h_pane_spawn(ctx, &req.params),
        method::PANE_SPLIT => h_pane_split(ctx, &req.params),
        method::PANE_GET => h_pane_get(ctx, &req.params),
        method::PANE_INPUT => h_pane_input(ctx, &req.params),
        method::PANE_RESIZE => h_pane_resize(ctx, &req.params),
        method::PANE_SIGNAL => h_pane_signal(ctx, &req.params),
        method::PANE_READ => h_pane_read(ctx, &req.params),
        method::PANE_ATTACH => h_pane_attach(ctx, &req.params),
        method::PANE_DETACH => h_pane_detach(ctx, &req.params),
        method::PANE_CLOSE => h_pane_close(ctx, &req.params),
        method::PANE_RESUME => h_pane_resume(ctx, &req.params),
        method::PANE_MARK_SEEN => h_pane_mark_seen(ctx, &req.params),
        method::DECISION_ANSWER => h_decision_answer(ctx, &req.params),
        method::NOTIFY => h_notify(ctx, &req.params),
        method::HOOK_EVENT => match decode::<params::HookEvent>(&req.params) {
            Ok(p) if p.wait_for_answer => return crate::approvals::wait(ctx, req).await,
            Ok(_) => h_hook_event(ctx, &req.params),
            Err(error) => Err(error),
        },
        method::REPORT_SESSION => h_report_session(ctx, &req.params),
        method::SUBSCRIBE => h_subscribe(ctx, &req.params),
        method::WAIT => return h_wait(ctx, req, &req.params).await,
        method::FOCUS_NEXT_UNREAD => h_next_unread(ctx),
        method::PLUGIN_LIST => h_plugin_list(ctx),
        method::PLUGIN_RELOAD => h_plugin_reload(ctx),
        method::TASK_START => return crate::tasks::h_task_start(ctx, req, &req.params).await,
        method::TASK_GET => return crate::tasks::h_task_get(ctx, req, &req.params),
        method::TASK_LIST => return crate::tasks::h_task_list(ctx, req, &req.params),
        method::TASK_WAIT => return crate::tasks::h_task_wait(ctx, req, &req.params).await,
        method::TASK_CANCEL => return crate::tasks::h_task_cancel(ctx, req, &req.params),
        method::PANE_SUBMIT => return h_pane_submit(ctx, req, &req.params).await,
        method::TASK_REPORT => return crate::tasks::h_task_report(ctx, req, &req.params),
        method::ATTENTION_PENDING => {
            return crate::tasks::h_attention_pending(ctx, req, &req.params)
        }
        method::TASK_DIFF => return crate::tasks::h_task_diff(ctx, req, &req.params).await,
        method::TASK_FILE_DIFF => {
            return crate::tasks::h_task_file_diff(ctx, req, &req.params).await
        }
        method::TASK_FINISH => return crate::tasks::h_task_finish(ctx, req, &req.params).await,
        method::TASK_PR_OPEN => return crate::tasks::h_task_pr_open(ctx, req, &req.params).await,
        method::TASK_PR_REFRESH => {
            return crate::tasks::h_task_pr_refresh(ctx, req, &req.params).await
        }
        _ => Err((
            code::UNKNOWN_METHOD.to_string(),
            format!("unknown method '{}'", req.method),
        )),
    };
    match out {
        Ok((result, effect)) => (Response::ok(&req.id, result), effect),
        Err((c, m)) => (Response::err(&req.id, &c, m), ConnEffect::default()),
    }
}

// ---- server ----

fn h_server_status(ctx: &Ctx, _params: &Value) -> Handler {
    let s = ctx.store.read().unwrap();
    let uptime = (Utc::now() - s.started_at).num_seconds().max(0);
    Ok((
        json!({
            "version": env!("CARGO_PKG_VERSION"),
            "protocol": signaltty_proto::PROTOCOL,
            "uptime_s": uptime,
            "workspaces": s.workspaces.len(),
            "tabs": s.tabs.len(),
            "panes": s.panes.len(),
            "live_panes": s.live_panes(),
            "seq": s.seq,
        }),
        ConnEffect::default(),
    ))
}

/// Self-printing contract: the same constants the router matches
/// on, so clients (human or agent) can read the API at runtime.
/// Unknown params are ignored, like `server.status`.
fn h_server_schema() -> Handler {
    Ok((
        json!({
            "protocol": signaltty_proto::PROTOCOL,
            "version": env!("CARGO_PKG_VERSION"),
            "methods": method::ALL,
            "events": event::ALL,
            "codes": code::ALL,
            "capabilities": {"subscribe_replay_coverage":true,"wait_baseline":true,"wait_multiple_outcomes":true},
        }),
        ConnEffect::default(),
    ))
}

fn h_server_shutdown(ctx: &Ctx, params: &Value) -> Handler {
    let force = decode::<params::ServerShutdown>(params)?
        .force
        .unwrap_or(false);
    {
        let s = ctx.store.read().unwrap();
        if !force && s.live_panes() > 0 {
            return Err((
                code::PANES_ALIVE.to_string(),
                format!("{} live panes; use force=true", s.live_panes()),
            ));
        }
    }
    if let Err(e) = crate::persist::save(&ctx.store, &ctx.ptys.terms(), &ctx.config) {
        tracing::warn!("shutdown snapshot failed: {e}");
    }
    ctx.emit(event::SERVER_WILL_SHUTDOWN, json!({}));
    ctx.shutdown.notify_waiters();
    Ok((json!({"stopped": true}), ConnEffect::default()))
}

// ---- workspaces ----

fn default_cwd() -> String {
    std::env::var("HOME").unwrap_or_else(|_| "/tmp".to_string())
}

/// Unique handle for `base`: `base`, `base-2`, … Handles are immutable
/// after create; the name stays the editable label.
pub fn unique_handle(store: &crate::store::Store, base: &str) -> String {
    if !store.workspaces.values().any(|w| w.handle == base) {
        return base.to_string();
    }
    for n in 2.. {
        let candidate = format!("{base}-{n}");
        if !store.workspaces.values().any(|w| w.handle == candidate) {
            return candidate;
        }
    }
    unreachable!()
}

/// Resolve a `workspace_id` param: exact id first, then handle.
/// Namespaces are disjoint by construction (handles never contain `_`).
pub fn resolve_workspace(store: &crate::store::Store, handle_or_id: &str) -> Option<String> {
    if store.workspaces.contains_key(handle_or_id) {
        return Some(handle_or_id.to_string());
    }
    store
        .workspaces
        .values()
        .find(|w| w.handle == handle_or_id)
        .map(|w| w.id.clone())
}

pub(crate) fn h_workspace_create(ctx: &Ctx, params: &Value) -> Handler {
    let p: params::WorkspaceCreate = decode(params)?;
    let cwd = p.cwd.unwrap_or_else(default_cwd);
    let _path_reference = ctx.worktrees.references.enter(&cwd)?;
    if !std::path::Path::new(&cwd).is_dir() {
        return Err(bad_params(format!("cwd is not a directory: {cwd}")));
    }
    let now = Utc::now();
    let name = p.name.unwrap_or_else(|| "workspace".to_string());
    let git = crate::git::git_info(&cwd);
    let ws = {
        // Pick the handle under the insert's lock so concurrent creates
        // cannot claim the same one.
        let mut s = ctx.store.write().unwrap();
        let ws = Workspace {
            id: new_ws_id(),
            handle: unique_handle(&s, &signaltty_core::model::slugify(&name)),
            name,
            git,
            cwd,
            tabs: Vec::new(),
            active_tab_id: None,
            auto_resume: false,
            created_at: now,
            updated_at: now,
        };
        s.workspaces.insert(ws.id.clone(), ws.clone());
        ws
    };
    ctx.emit(event::WORKSPACE_CREATED, json!({"workspace": ws}));
    ctx.mark_persist();
    Ok((json!({"workspace": ws}), ConnEffect::default()))
}

fn h_workspace_list(ctx: &Ctx) -> Handler {
    let s = ctx.store.read().unwrap();
    let mut v: Vec<&Workspace> = s.workspaces.values().collect();
    v.sort_by(|a, b| a.created_at.cmp(&b.created_at));
    Ok((json!({"workspaces": v}), ConnEffect::default()))
}

fn h_workspace_get(ctx: &Ctx, params: &Value) -> Handler {
    let raw = decode::<params::WorkspaceId>(params)?.workspace_id;
    let s = ctx.store.read().unwrap();
    let id = resolve_workspace(&s, &raw)
        .ok_or_else(|| (code::NO_SUCH_WORKSPACE.to_string(), raw.clone()))?;
    let ws = s.workspaces.get(&id).unwrap();
    let tabs: Vec<&Tab> = ws.tabs.iter().filter_map(|t| s.tabs.get(t)).collect();
    let mut panes = Vec::new();
    for t in &tabs {
        if let Some(layout) = &t.layout {
            for p in layout.panes() {
                if let Some(pane) = s.panes.get(&p) {
                    panes.push(pane);
                }
            }
        }
    }
    Ok((
        json!({"workspace": ws, "tabs": tabs, "panes": panes}),
        ConnEffect::default(),
    ))
}

fn h_workspace_rename(ctx: &Ctx, params: &Value) -> Handler {
    let p: params::WorkspaceRename = decode(params)?;
    let raw = p.workspace_id;
    let name = p.name;
    // Handles are immutable: the name is the editable label, the handle
    // stays the stable address.
    let id = {
        let s = ctx.store.read().unwrap();
        resolve_workspace(&s, &raw)
            .ok_or_else(|| (code::NO_SUCH_WORKSPACE.to_string(), raw.clone()))?
    };
    let mut s = ctx.store.write().unwrap();
    let ws = s.workspaces.get_mut(&id).unwrap();
    ws.name = name;
    ws.updated_at = Utc::now();
    let ws = ws.clone();
    s.emit(event::WORKSPACE_UPDATED, json!({"workspace": ws}));
    drop(s);

    ctx.mark_persist();
    Ok((json!({"workspace": ws}), ConnEffect::default()))
}

fn close_pane_locked(ctx: &Ctx, s: &mut crate::store::Store, pane_id: &str, signal: Option<&str>) {
    ctx.ptys.destroy(pane_id, signal);
    if let Some(pane) = s.remove_pane(pane_id) {
        if let Some(tab) = s.tabs.get_mut(&pane.tab_id) {
            if let Some(layout) = tab.layout.as_mut() {
                let ids = layout.panes();
                if ids.len() <= 1 {
                    tab.layout = None;
                } else {
                    layout.remove(pane_id);
                }
            }
            if tab.active_pane_id.as_deref() == Some(pane_id) {
                tab.active_pane_id = tab
                    .layout
                    .as_ref()
                    .and_then(|l| l.panes().into_iter().next());
            }
        }
    }
}

pub(crate) fn h_workspace_close(ctx: &Ctx, params: &Value) -> Handler {
    let p: params::WorkspaceClose = decode(params)?;
    let raw = p.workspace_id;
    let signal = p.signal;
    let id = {
        let s = ctx.store.read().unwrap();
        resolve_workspace(&s, &raw)
            .ok_or_else(|| (code::NO_SUCH_WORKSPACE.to_string(), raw.clone()))?
    };
    let mut s = ctx.store.write().unwrap();
    let ws = s
        .workspaces
        .remove(&id)
        .ok_or_else(|| (code::NO_SUCH_WORKSPACE.to_string(), id.clone()))?;
    let panes: Vec<_> = s
        .panes
        .values()
        .filter(|pane| pane.workspace_id == id)
        .map(|pane| pane.id.clone())
        .collect();
    for pane_id in panes {
        close_pane_locked(ctx, &mut s, &pane_id, signal.as_deref());
    }
    let mut tabs: std::collections::HashSet<_> = ws.tabs.into_iter().collect();
    tabs.extend(
        s.tabs
            .values()
            .filter(|tab| tab.workspace_id == id)
            .map(|tab| tab.id.clone()),
    );
    for tab_id in tabs {
        if s.tabs.remove(&tab_id).is_some() {
            s.emit(event::TAB_CLOSED, json!({"tab_id": tab_id}));
        }
    }
    s.emit(event::WORKSPACE_CLOSED, json!({"workspace_id": id}));
    drop(s);

    ctx.mark_persist();
    Ok((json!({"closed": true}), ConnEffect::default()))
}

fn h_workspace_refresh_git(ctx: &Ctx, params: &Value) -> Handler {
    let raw = decode::<params::WorkspaceId>(params)?.workspace_id;
    let id = {
        let s = ctx.store.read().unwrap();
        resolve_workspace(&s, &raw)
            .ok_or_else(|| (code::NO_SUCH_WORKSPACE.to_string(), raw.clone()))?
    };
    let (ws, branch_changed) = {
        let mut s = ctx.store.write().unwrap();
        let ws = s.workspaces.get_mut(&id).unwrap();
        let old_branch = ws.git.branch.clone();
        ws.git = crate::git::git_info(&ws.cwd.clone());
        ws.updated_at = Utc::now();
        (ws.clone(), old_branch != ws.git.branch)
    };
    ctx.emit(event::WORKSPACE_UPDATED, json!({"workspace": ws}));
    if branch_changed {
        ctx.emit(
            event::GIT_BRANCH_CHANGED,
            json!({"workspace_id": id, "branch": ws.git.branch}),
        );
    }
    ctx.mark_persist();
    Ok((json!({"workspace": ws}), ConnEffect::default()))
}

/// Worktree-vs-HEAD diff as data (t3code `+N −N` language). On demand
/// only; a non-repo is `BAD_PARAMS`, never an empty lie.
fn h_workspace_diff(ctx: &Ctx, params: &Value) -> Handler {
    let raw = decode::<params::WorkspaceId>(params)?.workspace_id;
    let (id, cwd) = {
        let s = ctx.store.read().unwrap();
        let id = resolve_workspace(&s, &raw)
            .ok_or_else(|| (code::NO_SUCH_WORKSPACE.to_string(), raw.clone()))?;
        let cwd = s.workspaces.get(&id).unwrap().cwd.clone();
        (id, cwd)
    };
    match crate::git::git_diff(&cwd) {
        Some(diff) => Ok((
            json!({
                "workspace_id": id, "branch": diff.branch,
                "files": diff.files, "dirs": diff.dirs,
                "added": diff.added, "removed": diff.removed,
            }),
            ConnEffect::default(),
        )),
        None => Err(bad_params(format!("not a git repo: {cwd}"))),
    }
}

async fn h_workspace_file_diff(ctx: &Ctx, value: &Value) -> Handler {
    let p = decode::<params::WorkspaceFileDiff>(value)?;
    let (id, cwd) = {
        let s = ctx.store.read().unwrap();
        let id = resolve_workspace(&s, &p.workspace_id)
            .ok_or_else(|| (code::NO_SUCH_WORKSPACE.into(), p.workspace_id.clone()))?;
        let cwd = s.workspaces[&id].cwd.clone();
        (id, cwd)
    };
    let diff = crate::file_diff::read(&cwd, &p.path).await?;
    Ok((
        json!({"workspace_id":id,"path":diff.path,"untracked":diff.untracked,"content":diff.content}),
        ConnEffect::default(),
    ))
}

// ---- tabs ----

pub(crate) fn h_tab_create(ctx: &Ctx, params: &Value) -> Handler {
    let p: params::TabCreate = decode(params)?;
    let raw = p.workspace_id;
    let ws_id = {
        let s = ctx.store.read().unwrap();
        resolve_workspace(&s, &raw)
            .ok_or_else(|| (code::NO_SUCH_WORKSPACE.to_string(), raw.clone()))?
    };
    let now = Utc::now();
    let tab = Tab {
        id: new_tab_id(),
        workspace_id: ws_id.clone(),
        title: p.title.unwrap_or_else(|| "tab".to_string()),
        layout: None,
        active_pane_id: None,
        created_at: now,
    };
    {
        let mut s = ctx.store.write().unwrap();
        let ws = s
            .workspaces
            .get_mut(&ws_id)
            .ok_or_else(|| (code::NO_SUCH_WORKSPACE.to_string(), ws_id.clone()))?;
        ws.tabs.push(tab.id.clone());
        ws.active_tab_id = Some(tab.id.clone());
        ws.updated_at = now;
        s.tabs.insert(tab.id.clone(), tab.clone());
    }
    ctx.emit(event::TAB_CREATED, json!({"tab": tab}));
    ctx.mark_persist();
    Ok((json!({"tab": tab}), ConnEffect::default()))
}

fn h_tab_close(ctx: &Ctx, params: &Value) -> Handler {
    let p: params::TabClose = decode(params)?;
    let id = p.tab_id;
    let signal = p.signal;
    let mut s = ctx.store.write().unwrap();
    let tab = s
        .tabs
        .remove(&id)
        .ok_or_else(|| (code::NO_SUCH_TAB.to_string(), id.clone()))?;
    let panes: Vec<_> = s
        .panes
        .values()
        .filter(|pane| pane.tab_id == id)
        .map(|pane| pane.id.clone())
        .collect();
    for p in panes {
        close_pane_locked(ctx, &mut s, &p, signal.as_deref());
    }
    if let Some(ws) = s.workspaces.get_mut(&tab.workspace_id) {
        ws.tabs.retain(|t| t != &id);
        if ws.active_tab_id.as_deref() == Some(&id) {
            ws.active_tab_id = ws.tabs.last().cloned();
        }
        ws.updated_at = Utc::now();
    }
    s.emit(event::TAB_CLOSED, json!({"tab_id": id}));
    drop(s);

    ctx.mark_persist();
    Ok((json!({"closed": true}), ConnEffect::default()))
}

fn h_tab_set_layout(ctx: &Ctx, params: &Value) -> Handler {
    let p: params::TabSetLayout = decode(params)?;
    let id = p.tab_id;
    let mut layout = p.layout;
    layout
        .validate_and_normalize()
        .map_err(|e| bad_params(e.to_string()))?;
    // All panes in the layout must exist and belong to this tab.
    let mut s = ctx.store.write().unwrap();
    for p in layout.panes() {
        match s.panes.get(&p) {
            Some(pane) if pane.tab_id == id => {}
            _ => return Err(bad_params(format!("layout references foreign pane {p}"))),
        }
    }
    let owned: std::collections::HashSet<_> = s
        .panes
        .values()
        .filter(|pane| pane.tab_id == id)
        .map(|pane| pane.id.clone())
        .collect();
    let referenced: std::collections::HashSet<_> = layout.panes().into_iter().collect();
    if referenced != owned {
        return Err(bad_params(
            "layout must contain every pane owned by the tab",
        ));
    }
    let tab = s
        .tabs
        .get_mut(&id)
        .ok_or_else(|| (code::NO_SUCH_TAB.to_string(), id.clone()))?;
    tab.layout = Some(layout);
    let tab = tab.clone();
    s.emit(event::TAB_UPDATED, json!({"tab": tab}));
    drop(s);

    ctx.mark_persist();
    Ok((json!({"tab": tab}), ConnEffect::default()))
}

/// Move one divider: `path` holds 0 (first) / 1 (second) choices from
/// the tab root (`[]` = root) and must resolve to a `Split`. A
/// targeted update, so a dragged divider never overwrites a layout
/// another client changed concurrently (whole-tree `tab.set_layout`
/// would). Out-of-range ratios clamp; the echo is `tab.updated`.
fn h_tab_set_ratio(ctx: &Ctx, params: &Value) -> Handler {
    let p: params::TabSetRatio = decode(params)?;
    let id = p.tab_id;
    let path = p.path.0;
    let ratio = p.ratio.0;
    let mut s = ctx.store.write().unwrap();
    let tab = s
        .tabs
        .get_mut(&id)
        .ok_or_else(|| (code::NO_SUCH_TAB.to_string(), id.clone()))?;
    let applied = tab
        .layout
        .as_mut()
        .is_some_and(|layout| layout.set_ratio_at_path(&path, ratio));
    if !applied {
        return Err(bad_params("path does not resolve to a split"));
    }
    let tab = tab.clone();
    s.emit(event::TAB_UPDATED, json!({"tab": tab}));
    drop(s);

    ctx.mark_persist();
    Ok((json!({"tab": tab}), ConnEffect::default()))
}

// ---- panes ----

fn resolve_size(cols: Option<u16>, rows: Option<u16>) -> PtySize {
    PtySize::clamp(cols.unwrap_or(80), rows.unwrap_or(24))
}

fn launch_result(pane: &Pane, integration: Value) -> Value {
    let mut result = pane_result(pane);
    if !integration.is_null() {
        result["integration"] = integration;
    }
    result
}

fn anchor_launch_executable(
    mut argv: Vec<String>,
    cwd: &str,
) -> Result<Vec<String>, (String, String)> {
    let program = std::path::Path::new(&argv[0]);
    if argv[0].contains('/') && !program.is_absolute() {
        argv[0] = std::path::absolute(std::path::Path::new(cwd).join(program))
            .map_err(|e| (code::IO_ERROR.to_string(), e.to_string()))?
            .to_string_lossy()
            .into_owned();
    }
    Ok(argv)
}

pub(crate) fn h_pane_spawn(ctx: &Ctx, params: &Value) -> Handler {
    let p: params::PaneSpawn = decode(params)?;
    let raw = p.workspace_id;
    let ws_id = {
        let s = ctx.store.read().unwrap();
        resolve_workspace(&s, &raw)
            .ok_or_else(|| (code::NO_SUCH_WORKSPACE.to_string(), raw.clone()))?
    };
    let argv = p.argv;
    if argv.is_empty() {
        return Err(bad_params("'argv' must not be empty"));
    }
    let mut env = p.env;
    let size = resolve_size(p.cols, p.rows);
    // Explicit hint wins; otherwise detect from argv (process info layer).
    let kind = match p.agent_hint {
        Some(hint) => AgentKind::parse(&hint).unwrap_or(AgentKind::None),
        None => ctx.detect_kind(&argv),
    };

    let (parent_pane_id, root_pane_id) = {
        let s = ctx.store.read().unwrap();
        match &p.parent_pane_id {
            Some(parent_id) => {
                let parent = s
                    .panes
                    .get(parent_id)
                    .ok_or_else(|| (code::NO_SUCH_PANE.to_string(), parent_id.clone()))?;
                let root = parent
                    .root_pane_id
                    .clone()
                    .unwrap_or_else(|| parent.id.clone());
                (Some(parent_id.clone()), Some(root))
            }
            None => (None, None),
        }
    };
    if let Some(parent) = &parent_pane_id {
        env.insert("SIGNALTTY_PARENT_PANE".to_string(), parent.clone());
    }

    // Hold ownership stable until the process and its pane are published.
    // Pump/reaper threads release their PTY locks before acquiring Store.
    let now = Utc::now();
    let mut s = ctx.store.write().unwrap();
    let ws = s
        .workspaces
        .get(&ws_id)
        .ok_or_else(|| (code::NO_SUCH_WORKSPACE.to_string(), ws_id.clone()))?;
    let cwd = p.cwd.unwrap_or_else(|| ws.cwd.clone());
    if !std::path::Path::new(&cwd).is_dir() {
        return Err(bad_params(format!("cwd is not a directory: {cwd}")));
    }
    let argv = anchor_launch_executable(argv, &cwd)?;
    let existing_tab = p.tab_id.or_else(|| ws.active_tab_id.clone());
    let staged_tab = if existing_tab.is_none() {
        Some(Tab {
            id: new_tab_id(),
            workspace_id: ws_id.clone(),
            title: "agents".to_string(),
            layout: None,
            active_pane_id: None,
            created_at: now,
        })
    } else {
        None
    };
    let tab_id = existing_tab.unwrap_or_else(|| staged_tab.as_ref().unwrap().id.clone());
    if let Some(tab) = s.tabs.get(&tab_id) {
        if tab.workspace_id != ws_id {
            return Err(bad_params("tab belongs to another workspace"));
        }
        if tab.layout.is_some() {
            return Err(bad_params("tab already has panes; use pane.split"));
        }
    } else if staged_tab.is_none() {
        return Err((code::NO_SUCH_TAB.to_string(), tab_id));
    }
    let mut pane = Pane::new(
        ws_id.clone(),
        tab_id.clone(),
        cwd.clone(),
        argv.clone(),
        size,
        now,
    );
    pane.agent.kind = kind;
    pane.parent_pane_id = parent_pane_id;
    pane.root_pane_id = root_pane_id;
    pane.label = p.label;
    pane.relationship = p.relationship;
    pane.task_id = p.task_id;
    pane.agent.config_env = env
        .iter()
        .filter(|(key, _)| {
            matches!(
                key.as_str(),
                "CLAUDE_CONFIG_DIR" | "CODEX_HOME" | "OPENCODE_CONFIG_DIR"
            )
        })
        .map(|(key, value)| {
            let path = std::path::Path::new(value);
            let absolute = if path.is_absolute() {
                path.to_path_buf()
            } else {
                std::path::Path::new(&cwd).join(path)
            };
            (key.clone(), absolute.to_string_lossy().into_owned())
        })
        .collect();
    let _path_reference = ctx.worktrees.references.enter(&cwd)?;
    let integration = ctx
        .ptys
        .spawn(SpawnRequest {
            pane_id: pane.id.clone(),
            cwd,
            argv,
            env,
            size,
            socket_path: ctx.config.socket_path.to_string_lossy().to_string(),
        })
        .map_err(|e| (code::SPAWN_FAILED.to_string(), e))?;
    if let Some(tab) = staged_tab {
        let ws = s.workspaces.get_mut(&ws_id).unwrap();
        ws.tabs.push(tab_id.clone());
        ws.active_tab_id = Some(tab_id.clone());
        s.tabs.insert(tab_id.clone(), tab.clone());
        s.emit(event::TAB_CREATED, json!({"tab": tab}));
    }
    s.panes.insert(pane.id.clone(), pane.clone());
    let tab = s.tabs.get_mut(&tab_id).unwrap();
    tab.layout = Some(signaltty_core::model::Layout::Pane {
        pane_id: pane.id.clone(),
    });
    tab.active_pane_id = Some(pane.id.clone());
    s.publish_pane(&pane, false);

    drop(s);
    ctx.mark_persist();
    Ok((launch_result(&pane, integration), ConnEffect::default()))
}

fn user_shell() -> String {
    std::env::var("SHELL").unwrap_or_else(|_| "sh".to_string())
}

fn h_pane_split(ctx: &Ctx, params: &Value) -> Handler {
    let ps: params::PaneSplit = decode(params)?;
    let pane_id = ps.pane_id;
    let dir = ps
        .direction
        .as_ref()
        .map(|d| d.split_dir())
        .unwrap_or(SplitDir::Right);
    let mut s = ctx.store.write().unwrap();
    let p = s
        .panes
        .get(&pane_id)
        .ok_or_else(|| (code::NO_SUCH_PANE.to_string(), pane_id.clone()))?;
    let (ws_id, tab_id, cwd, size) = (
        p.workspace_id.clone(),
        p.tab_id.clone(),
        ps.cwd.clone().unwrap_or_else(|| p.cwd.clone()),
        p.pty_size,
    );
    if !s
        .tabs
        .get(&tab_id)
        .and_then(|tab| tab.layout.as_ref())
        .is_some_and(|layout| layout.panes().contains(&pane_id))
    {
        return Err(bad_params("pane is not in its tab layout"));
    }
    if !std::path::Path::new(&cwd).is_dir() {
        return Err(bad_params(format!("cwd is not a directory: {cwd}")));
    }
    let argv = ps.argv.unwrap_or_else(|| vec![user_shell()]);
    if argv.is_empty() {
        return Err(bad_params("'argv' must not be empty"));
    }
    let argv = anchor_launch_executable(argv, &cwd)?;
    let now = Utc::now();
    let mut pane = Pane::new(ws_id, tab_id.clone(), cwd.clone(), argv.clone(), size, now);
    pane.agent.kind = ctx.detect_kind(&argv);
    let _path_reference = ctx.worktrees.references.enter(&cwd)?;
    let integration = ctx
        .ptys
        .spawn(SpawnRequest {
            pane_id: pane.id.clone(),
            cwd,
            argv,
            env: HashMap::new(),
            size,
            socket_path: ctx.config.socket_path.to_string_lossy().to_string(),
        })
        .map_err(|e| (code::SPAWN_FAILED.to_string(), e))?;
    {
        s.panes.insert(pane.id.clone(), pane.clone());
        let tab_snapshot = {
            let tab = s.tabs.get_mut(&tab_id).unwrap();
            match tab.layout.as_mut() {
                Some(layout) => {
                    layout.split(&pane_id, dir, pane.id.clone());
                }
                None => {
                    tab.layout = Some(signaltty_core::model::Layout::Pane {
                        pane_id: pane.id.clone(),
                    });
                }
            }
            tab.active_pane_id = Some(pane.id.clone());
            tab.clone()
        };
        s.emit(event::TAB_UPDATED, json!({"tab": tab_snapshot}));
    }
    s.publish_pane(&pane, false);

    drop(s);
    ctx.mark_persist();
    Ok((launch_result(&pane, integration), ConnEffect::default()))
}

fn h_pane_get(ctx: &Ctx, params: &Value) -> Handler {
    let id = decode::<params::PaneId>(params)?.pane_id;
    let s = ctx.store.read().unwrap();
    let pane = s
        .panes
        .get(&id)
        .ok_or_else(|| (code::NO_SUCH_PANE.to_string(), id))?;
    Ok((
        json!({"pane":pane,"wait_baseline":s.wait_baseline(&pane.id)}),
        ConnEffect::default(),
    ))
}

fn h_pane_input(ctx: &Ctx, params: &Value) -> Handler {
    let p: params::PaneInput = decode(params)?;
    let id = p.pane_id;
    let data_b64 = p.data_b64;
    if data_b64.len() > 1024 * 1024 {
        return Err((
            code::RATE_LIMITED.to_string(),
            "input too large".to_string(),
        ));
    }
    let data = base64::engine::general_purpose::STANDARD
        .decode(data_b64.as_bytes())
        .map_err(|_| bad_params("invalid base64 in 'data_b64'"))?;
    {
        let s = ctx.store.read().unwrap();
        let pane = s
            .panes
            .get(&id)
            .ok_or_else(|| (code::NO_SUCH_PANE.to_string(), id.clone()))?;
        if !matches!(pane.live, LiveState::Live) {
            return Err((code::PANE_EXITED.to_string(), id));
        }
    }
    let written = ctx.ptys.input(&id, &data).map_err(|e| {
        if e == "pane has no live PTY" {
            (code::PANE_EXITED.to_string(), id.clone())
        } else {
            (code::IO_ERROR.to_string(), e)
        }
    })?;
    Ok((json!({"written": written}), ConnEffect::default()))
}

fn h_pane_resize(ctx: &Ctx, params: &Value) -> Handler {
    let p: params::PaneResize = decode(params)?;
    let id = p.pane_id;
    let cols = p.cols;
    let rows = p.rows;
    let size = PtySize::clamp(cols, rows);
    let pane = {
        let mut s = ctx.store.write().unwrap();
        let pane = s
            .panes
            .get_mut(&id)
            .ok_or_else(|| (code::NO_SUCH_PANE.to_string(), id.clone()))?;
        pane.pty_size = size;
        pane.clone()
    };
    // Last-writer-wins arbitration: apply even with multiple viewers.
    if let Err(e) = ctx.ptys.resize(&id, size) {
        tracing::warn!("resize {id}: {e}");
    }
    ctx.emit(event::PANE_RESIZED, json!({"pane": pane}));
    ctx.mark_persist();
    Ok((pane_result(&pane), ConnEffect::default()))
}

fn h_pane_signal(ctx: &Ctx, params: &Value) -> Handler {
    let p: params::PaneSignal = decode(params)?;
    let id = p.pane_id;
    let signal = p.signal;
    let group = p.group.unwrap_or(false);
    {
        let s = ctx.store.read().unwrap();
        if !s.panes.contains_key(&id) {
            return Err((code::NO_SUCH_PANE.to_string(), id));
        }
    }
    ctx.ptys.signal(&id, &signal, group).map_err(|e| {
        if e == "pane has no live process" {
            (code::PANE_EXITED.to_string(), id.clone())
        } else {
            (code::IO_ERROR.to_string(), e)
        }
    })?;
    Ok((json!({"sent": true}), ConnEffect::default()))
}

fn h_pane_read(ctx: &Ctx, params: &Value) -> Handler {
    let p: params::PaneRead = decode(params)?;
    let id = p.pane_id;
    {
        let s = ctx.store.read().unwrap();
        if !s.panes.contains_key(&id) {
            return Err((code::NO_SUCH_PANE.to_string(), id));
        }
    }
    let mode = p.mode.unwrap_or(params::ReadMode::Tail);
    let strip = p.strip_ansi.unwrap_or(true);
    let terms = ctx.ptys.terms();
    let terms = terms.lock().unwrap();
    match mode {
        params::ReadMode::Screen => {
            let text = terms.snapshot(&id);
            let text = if strip {
                signaltty_term::strip_ansi(&text)
            } else {
                text
            };
            Ok((
                json!({"text": text, "truncated": false}),
                ConnEffect::default(),
            ))
        }
        params::ReadMode::Tail => {
            let n = p.lines.unwrap_or(200).min(5000) as usize;
            let lines = terms.tail(&id, n, strip).unwrap_or_default();
            let total = terms.ring_len(&id);
            Ok((
                json!({"text": lines.join("\n"), "truncated": total > lines.len()}),
                ConnEffect::default(),
            ))
        }
        params::ReadMode::Rendered => {
            let n = p.lines.unwrap_or(200).min(5000) as usize;
            let after = p.after_seq.unwrap_or(0);
            match terms.rendered(&id, after, n) {
                None => Err((code::NO_SUCH_PANE.to_string(), id)),
                Some(r) => Ok((
                    json!({
                        "text": r.text,
                        "seq": r.seq,
                        "next_seq": r.next_seq,
                        "dropped": r.dropped,
                        "truncated": r.truncated,
                    }),
                    ConnEffect::default(),
                )),
            }
        }
    }
}

fn h_pane_attach(ctx: &Ctx, params: &Value) -> Handler {
    let p: params::PaneAttach = decode(params)?;
    let id = p.pane_id;
    // Optional resize on attach = last-writer-wins arbitration.
    if let (Some(cols), Some(rows)) = (p.cols, p.rows) {
        let size = PtySize::clamp(cols, rows);
        let pane = {
            let mut s = ctx.store.write().unwrap();
            let pane = s
                .panes
                .get_mut(&id)
                .ok_or_else(|| (code::NO_SUCH_PANE.to_string(), id.clone()))?;
            pane.pty_size = size;
            pane.clone()
        };
        let _ = ctx.ptys.resize(&id, size);
        ctx.emit(event::PANE_RESIZED, json!({"pane": pane}));
        ctx.mark_persist();
    }
    let pane = {
        let s = ctx.store.read().unwrap();
        s.panes
            .get(&id)
            .cloned()
            .ok_or_else(|| (code::NO_SUCH_PANE.to_string(), id.clone()))?
    };
    let (snapshot, output_offset) = ctx.ptys.snapshot(&id);
    // Attach marks seen only when the client passes mark_seen (CLI
    // interactive attach = explicit focus). Multi-pane GUIs pass false
    // and clear per-pane on focus instead — visibility alone must not
    // clear attention.
    let want_seen = p.mark_seen.unwrap_or(true);
    let pane = {
        let mut s = ctx.store.write().unwrap();
        if want_seen {
            s.mark_seen(&id, "attach");
        }
        s.panes.get(&id).cloned().unwrap_or(pane)
    };
    if want_seen {
        ctx.mark_persist();
    }
    let mut effect = ConnEffect::default();
    effect.attach.push(id.clone());
    Ok((
        json!({
            "snapshot_b64": base64::engine::general_purpose::STANDARD.encode(&snapshot),
            "output_offset": output_offset,
            "size": pane.pty_size,
            "live": pane.live,
            "restore_state": pane.restore_state,
            "lifecycle": pane.lifecycle.as_str(),
            "attention": pane.attention.as_str(),
        }),
        effect,
    ))
}

fn h_pane_detach(_ctx: &Ctx, params: &Value) -> Handler {
    let id = decode::<params::PaneId>(params)?.pane_id;
    let mut effect = ConnEffect::default();
    effect.detach.push(id);
    Ok((json!({"detached": true}), effect))
}

fn h_pane_close(ctx: &Ctx, params: &Value) -> Handler {
    let p: params::PaneClose = decode(params)?;
    let id = p.pane_id;
    let signal = p.signal;
    {
        let s = ctx.store.read().unwrap();
        if !s.panes.contains_key(&id) {
            return Err((code::NO_SUCH_PANE.to_string(), id));
        }
    }
    {
        let mut s = ctx.store.write().unwrap();
        close_pane_locked(ctx, &mut s, &id, signal.as_deref());
    }
    ctx.mark_persist();
    Ok((json!({"closed": true}), ConnEffect::default()))
}

/// One-key resume: spawn the adapter's official resume command in a
/// restored tombstone. Never automatic — the user (or automation)
/// explicitly invokes it per pane.
fn h_pane_resume(ctx: &Ctx, params: &Value) -> Handler {
    let id = decode::<params::PaneId>(params)?.pane_id;
    // Retain ownership and prevent concurrent resume until publication.
    let mut s = ctx.store.write().unwrap();
    let (cwd, size, argv, env) = {
        let pane = s
            .panes
            .get(&id)
            .ok_or_else(|| (code::NO_SUCH_PANE.to_string(), id.clone()))?;
        if matches!(pane.live, LiveState::Live) {
            return Err(bad_params("pane is already live"));
        }
        let argv = pane
            .agent
            .resume_argv
            .clone()
            .ok_or_else(|| bad_params("pane has no adapter resume command (not resumable)"))?;
        (
            pane.cwd.clone(),
            pane.pty_size,
            signaltty_agent::resolve_resume_argv(&pane.argv, &argv),
            pane.agent.config_env.clone(),
        )
    };
    let _path_reference = ctx.worktrees.references.enter(&cwd)?;
    let integration = ctx
        .ptys
        .spawn(SpawnRequest {
            pane_id: id.clone(),
            cwd,
            argv: argv.clone(),
            env,
            size,
            socket_path: ctx.config.socket_path.to_string_lossy().to_string(),
        })
        .map_err(|e| (code::SPAWN_FAILED.to_string(), e))?;
    let pane = {
        {
            let pane = s.panes.get_mut(&id).unwrap();
            pane.live = LiveState::Live;
            pane.agent.resume_argv = Some(argv);
            pane.restore_state = RestoreState::Live;
            pane.last_activity_at = Utc::now();
        }
        s.set_lifecycle(&id, Lifecycle::Unknown);
        let pane = s.panes.get(&id).cloned().unwrap();
        pane
    };
    s.publish_pane(&pane, true);

    drop(s);
    ctx.mark_persist();
    Ok((launch_result(&pane, integration), ConnEffect::default()))
}

fn h_pane_mark_seen(ctx: &Ctx, params: &Value) -> Handler {
    let id = decode::<params::PaneId>(params)?.pane_id;
    let mut s = ctx.store.write().unwrap();
    s.panes
        .get(&id)
        .ok_or_else(|| (code::NO_SUCH_PANE.to_string(), id.clone()))?;
    s.mark_seen(&id, "mark_seen");
    let pane_snapshot = s.panes.get(&id).cloned().unwrap();
    drop(s);
    ctx.mark_persist();
    Ok((pane_result(&pane_snapshot), ConnEffect::default()))
}

/// Answer a pending decision through the pane adapter's channel.
/// Consumes the id first so a concurrent clear can never double-deliver;
/// stale/double answers are typed `NO_SUCH_DECISION` (the client refreshes
/// and drops its bar), never a redelivery.
fn h_decision_answer(ctx: &Ctx, params: &Value) -> Handler {
    let p: params::DecisionAnswer = decode(params)?;
    let (kind, live) = {
        let s = ctx.store.read().unwrap();
        let pane = s
            .panes
            .get(&p.pane_id)
            .ok_or_else(|| (code::NO_SUCH_PANE.to_string(), p.pane_id.clone()))?;
        (pane.agent.kind, pane.live)
    };
    if !matches!(live, LiveState::Live) {
        return Err((code::PANE_EXITED.to_string(), p.pane_id));
    }
    let pending = {
        let s = ctx.store.read().unwrap();
        s.panes
            .get(&p.pane_id)
            .and_then(|pane| pane.pending_decision.clone())
    };
    let Some(pending) = pending else {
        return Err((
            code::NO_SUCH_DECISION.to_string(),
            format!("pane {} has no pending decision", p.pane_id),
        ));
    };
    if pending.id != p.decision_id {
        return Err((
            code::NO_SUCH_DECISION.to_string(),
            format!(
                "decision {} is stale (pending {})",
                p.decision_id, pending.id
            ),
        ));
    }
    if crate::approvals::Approvals::is_native(&pending.id) {
        return crate::approvals::answer(ctx, &p.pane_id, &p.decision_id, &p.option_id)
            .map(|result| (result, ConnEffect::default()));
    }
    if !pending.options.iter().any(|o| o.id == p.option_id) {
        return Err(bad_params(format!("unknown option '{}'", p.option_id)));
    }
    let adapter = ctx.adapter_for_kind(kind);
    let Some(channel) = adapter.answer_channel() else {
        return Err(bad_params(format!(
            "adapter '{}' has no answer channel (answer in the terminal)",
            adapter.metadata().display_name,
        )));
    };
    let bytes = signaltty_agent::answer_bytes(channel, &pending.options, &p.option_id)
        .ok_or_else(|| bad_params(format!("unknown option '{}'", p.option_id)))?;
    match consume_deliver_resume(&ctx.store, &p, || {
        ctx.ptys.input(&p.pane_id, &bytes).map(|_| ())
    }) {
        Ok(()) => {}
        Err(TypedAnswerError::Stale) => {
            return Err((
                code::NO_SUCH_DECISION.to_string(),
                format!("decision {} is stale", p.decision_id),
            ));
        }
        Err(TypedAnswerError::Delivery(e)) => {
            return Err(if e == "pane has no live PTY" {
                (code::PANE_EXITED.to_string(), p.pane_id)
            } else {
                (code::IO_ERROR.to_string(), e)
            });
        }
    }
    let (lifecycle, attention) = {
        let s = ctx.store.read().unwrap();
        let pane = s.panes.get(&p.pane_id).unwrap();
        (
            pane.lifecycle.as_str().to_string(),
            pane.attention.as_str().to_string(),
        )
    };
    ctx.mark_persist();
    Ok((
        json!({"answered": true, "lifecycle": lifecycle, "attention": attention}),
        ConnEffect::default(),
    ))
}

enum TypedAnswerError {
    Stale,
    Delivery(String),
}

/// Consume before delivering: a concurrent clear turns a retry into a typed
/// stale-id, never a second delivery. The pane and its task resume only once
/// `deliver` landed; on failure the gate stays consumed but the pane keeps
/// its blocked state so the user answers in-terminal.
fn consume_deliver_resume(
    store: &SharedStore,
    p: &params::DecisionAnswer,
    deliver: impl FnOnce() -> Result<(), String>,
) -> Result<(), TypedAnswerError> {
    let consumed = {
        let mut s = store.write().unwrap();
        s.answer_decision(&p.pane_id, &p.decision_id, &p.option_id)
            .map(|ev| {
                // Resolve this gate under the consume lock. A newer decision
                // arriving during delivery must retain its own attention.
                let cleared = s.mark_seen(&p.pane_id, "decision_answer");
                (ev, cleared)
            })
    };
    if consumed.is_none() {
        return Err(TypedAnswerError::Stale);
    }

    if let Err(e) = deliver() {
        // The gate is consumed but the bytes never landed (the child
        // exited between the checks): loud error, user answers in-terminal.
        store.write().unwrap().emit(
            event::DECISION_CLEARED,
            json!({"pane_id": p.pane_id, "decision_id": p.decision_id, "reason": "pane_exited"}),
        );
        return Err(TypedAnswerError::Delivery(e));
    }
    store.write().unwrap().resume_after_answer(&p.pane_id);
    Ok(())
}

// ---- notifications ----

/// Shared by `notify`, OSC pump, and `hook-event`: sanitize, store,
/// raise attention, set last_message, emit. Returns the notification.
pub fn push_notification_full(
    store: &SharedStore,
    pane_id: &str,
    given_title: Option<&str>,
    fallback_title: &str,
    body: &str,
    severity: NotificationSeverity,
    source: &str,
) -> Notification {
    let given = given_title
        .map(|t| signaltty_term::sanitize_notification_text(t, 200))
        .filter(|t| !t.is_empty());
    // Notifications always carry a title; the pane's last message only
    // repeats one the caller actually gave.
    let titled = given.is_some();
    let title = given.unwrap_or_else(|| fallback_title.to_string());
    let body = signaltty_term::sanitize_notification_text(body, 2000);
    let workspace_id = store
        .read()
        .unwrap()
        .panes
        .get(pane_id)
        .map(|p| p.workspace_id.clone());
    let now = Utc::now();
    let notif = Notification {
        id: new_notif_id(),
        pane_id: Some(pane_id.to_string()),
        workspace_id,
        title: title.clone(),
        body: if body.is_empty() {
            None
        } else {
            Some(body.clone())
        },
        severity,
        source: source.to_string(),
        created_at: now,
        read_at: None,
    };
    {
        let mut s = store.write().unwrap();
        s.push_notification(notif.clone());
        if let Some(p) = s.panes.get_mut(pane_id) {
            let msg = if body.is_empty() {
                p.last_message.clone().unwrap_or(title.clone())
            } else if titled {
                format!("{title}: {}", truncate(&body, 300))
            } else {
                truncate(&body, 300)
            };
            p.last_message = Some(msg);
            p.last_activity_at = now;
        }
        s.emit(event::NOTIFICATION_CREATED, json!({"notification": notif}));
    }
    store
        .write()
        .unwrap()
        .raise_attention(pane_id, severity.attention());
    notif
}

pub fn push_notification(
    store: &SharedStore,
    pane_id: &str,
    title: Option<&str>,
    body: &str,
    severity: NotificationSeverity,
    source: &str,
) -> Notification {
    push_notification_full(store, pane_id, title, "signaltty", body, severity, source)
}

fn truncate(s: &str, max: usize) -> String {
    if s.chars().count() <= max {
        s.to_string()
    } else {
        s.chars().take(max).collect::<String>() + "…"
    }
}

fn h_notify(ctx: &Ctx, params: &Value) -> Handler {
    let p: params::Notify = decode(params)?;
    let pane_id = p.pane_id;
    let title = p.title;
    if title.is_empty() {
        return Err(bad_params("'title' must not be empty"));
    }
    let body = p.body.unwrap_or_default();
    let severity = parse_severity(&p.severity)?;
    {
        let s = ctx.store.read().unwrap();
        if !s.panes.contains_key(&pane_id) {
            return Err((code::NO_SUCH_PANE.to_string(), pane_id));
        }
    }
    let notif = push_notification(&ctx.store, &pane_id, Some(&title), &body, severity, "cli");
    ctx.mark_persist();
    Ok((json!({"notification": notif}), ConnEffect::default()))
}

/// Adapter-classified hook event: session identity, lifecycle,
/// attention, and notification — all owned by the agent's adapter.
/// Explicit `message`/`severity` params still win over the adapter's
/// notification draft when present.
fn h_hook_event(ctx: &Ctx, params: &Value) -> Handler {
    h_hook_event_inner(ctx, params, false)
}

pub(crate) fn h_native_hook_event(ctx: &Ctx, params: &Value) -> Handler {
    h_hook_event_inner(ctx, params, true)
}

fn h_hook_event_inner(ctx: &Ctx, params: &Value, native_route: bool) -> Handler {
    let mut p: params::HookEvent = decode(params)?;
    if p.decision.is_none() && p.hook == "PermissionRequest" {
        if let Ok(native) = signaltty_agent::permission::native_permission(&p.agent, &p.payload) {
            p.decision = Some(params::DecisionPayload {
                id: signaltty_core::new_notif_id().replacen("notif_", "permission_", 1),
                prompt: native.prompt,
                options: signaltty_agent::permission::permission_options()
                    .into_iter()
                    .map(|o| params::DecisionOptionPayload {
                        id: o.id,
                        label: o.label,
                    })
                    .collect(),
            });
        }
    }
    parse_severity(&p.severity)?;
    if let Some(d) = &p.decision {
        if d.prompt.trim().is_empty()
            || d.options.is_empty()
            || d.options
                .iter()
                .any(|o| o.id.is_empty() || o.label.is_empty())
        {
            return Err(bad_params("decision needs a prompt and named options"));
        }
        let ids: std::collections::HashSet<_> = d.options.iter().map(|o| &o.id).collect();
        if ids.len() != d.options.len() {
            return Err(bad_params("decision option IDs must be unique"));
        }
    }
    let agent = p.agent;
    let hook = p.hook;
    let adapter = ctx
        .adapter(&agent)
        .ok_or_else(|| bad_params(format!("unknown agent '{agent}'")))?;
    let mut pane_id = p.pane_id;
    // Fallback: attribute by process ancestry (survives env-stripping
    // sandboxes; ignores foreign hooks like Cursor Desktop).
    if pane_id.is_none() {
        if let Some(client_pid) = p.client_pid {
            if let Ok(pid) = u32::try_from(client_pid) {
                pane_id = crate::attrib::resolve_pane_by_ancestry(pid, &ctx.ptys.child_pids());
            }
        }
    }
    let payload = p.payload;
    let event = signaltty_agent::AdapterEvent {
        agent: agent.clone(),
        hook: hook.clone(),
        payload,
    };

    let Some(pid) = pane_id.clone() else {
        // No pane routing: accept (so shims never break the agent),
        // but classify nothing.
        return Ok((
            json!({"accepted": true, "agent": agent, "event": hook, "pane_id": Value::Null}),
            ConnEffect::default(),
        ));
    };
    {
        let s = ctx.store.read().unwrap();
        let Some(_pane) = s.panes.get(&pid) else {
            return Err((code::NO_SUCH_PANE.to_string(), pid.clone()));
        };
        // Harness mismatch check:
        // 1. Agent header vs payload agent mismatch
        let incoming_kind = adapter.metadata().kind;
        let payload_agent = event
            .payload
            .get("agent")
            .and_then(|v| v.as_str())
            .or_else(|| event.payload.get("harness").and_then(|v| v.as_str()));
        let payload_mismatch = if let Some(pa) = payload_agent {
            pa != agent && pa != incoming_kind.as_str()
        } else {
            false
        };

        // 2. Task wrapper-identity vs hook-harness mismatch
        let task_mismatch = s.tasks.values().any(|t| {
            t.pane_id.as_deref() == Some(&pid)
                && t.agent
                    .as_deref()
                    .is_some_and(|a| a != incoming_kind.as_str())
        });

        if payload_mismatch || task_mismatch {
            return Ok((
                json!({
                    "accepted": false,
                    "dropped": true,
                    "reason": "harness_mismatch",
                    "agent": agent,
                    "event": hook,
                    "pane_id": pid
                }),
                ConnEffect::default(),
            ));
        }
    }

    // 1. Session identity (+ resume argv) from the adapter.
    if let Some(sid) = adapter.session_identity(&event) {
        let kind = adapter.metadata().kind;
        let resume = adapter.resume_capability(&sid).map(|r| r.argv);
        let mut s = ctx.store.write().unwrap();
        if let Some(p) = s.panes.get_mut(&pid) {
            p.agent.kind = kind;
            if resume.is_some() {
                p.agent.resume_argv =
                    resume.map(|argv| signaltty_agent::resolve_resume_argv(&p.argv, &argv));
            }
            p.last_activity_at = Utc::now();
        }
        s.set_agent_session(&pid, sid);
    }

    let notif_draft = adapter.notification_event(&event);
    let session_id_opt = adapter.session_identity(&event);
    let mut decision = adapter.lifecycle_state(&event);

    let is_unrecognized = decision.lifecycle.is_none()
        && decision.attention.is_none()
        && session_id_opt.is_none()
        && notif_draft.is_none()
        && p.decision.is_none()
        && p.message.is_none()
        && p.body.is_none()
        && !matches!(
            hook.as_str(),
            "PreToolUse" | "PostToolUse" | "PreCompact" | "PostCompact" | "SubagentStop"
        );
    if is_unrecognized {
        return Ok((
            json!({
                "accepted": false,
                "dropped": true,
                "reason": "unrecognized_hook",
                "agent": agent,
                "event": hook,
                "pane_id": pid
            }),
            ConnEffect::default(),
        ));
    }
    {
        let mut s = ctx.store.write().unwrap();
        // Tool traffic (e.g. a subagent's tools during a pending permission)
        // must not clear `blocked`; it still starts work from any other state.
        if matches!(
            hook.as_str(),
            "PreToolUse" | "PostToolUse" | "PreCompact" | "PostCompact"
        ) && s
            .panes
            .get(&pid)
            .is_some_and(|p| p.lifecycle == Lifecycle::Blocked)
        {
            decision.lifecycle = None;
            decision.attention = None;
        }
        // Preserve turn-end intent even when its pane outcome is already settled.
        let ends_turn = decision.lifecycle == Some(Lifecycle::Done);
        // A session end trailing a finished turn restates the outcome: it must
        // not reopen a read `done`, rewrite `failed` as `done`, or replace the
        // useful last message with "session ended".
        if hook.eq_ignore_ascii_case("sessionend")
            && decision.lifecycle == Some(Lifecycle::Done)
            && s.panes
                .get(&pid)
                .is_some_and(|p| matches!(p.lifecycle, Lifecycle::Done | Lifecycle::Failed))
        {
            decision = signaltty_agent::LifecycleDecision::default();
        }
        if let Some(msg) = &decision.message {
            if let Some(p) = s.panes.get_mut(&pid) {
                p.last_message = Some(msg.clone());
                p.last_activity_at = Utc::now();
            }
        }
        if let Some(lifecycle) = decision.lifecycle {
            s.set_lifecycle(&pid, lifecycle);
        }
        // A suppressed SessionEnd still settles a worker that did not report.
        // Idle hooks never end a turn.
        if ends_turn {
            let target_task_id = s
                .tasks
                .values()
                .find(|t| t.pane_id.as_deref() == Some(&pid) && t.state == TaskState::Working)
                .map(|t| t.id.clone());
            if let Some(tid) = target_task_id {
                let last_message = s.panes.get(&pid).and_then(|p| p.last_message.clone());
                let evidence = json!({
                    "reason": "turn_ended_without_report",
                    "last_message": last_message,
                });
                s.task_input_required_on_turn_end(&tid, Some(evidence));
            }
        }
        match decision.attention {
            Some(Attention::None) => {
                s.clear_attention(&pid, "hook");
                s.clear_decision(&pid, "attention_cleared");
            }
            Some(att) => {
                s.raise_attention(&pid, att);
            }
            None => {}
        }
    }
    let decision_prompt = p.decision.as_ref().map(|d| d.prompt.clone());
    // 2b. Structured decision ingest (directive 2): explicit data wins.
    // A decision payload sets/supersedes; without one, leaving `blocked`
    // means the gate is gone and the bar must not stick.
    if let Some(payload) = p.decision {
        if payload.prompt.is_empty() || payload.options.is_empty() {
            return Err(bad_params(
                "decision needs a prompt and at least one option",
            ));
        }
        let answerable = if crate::approvals::Approvals::is_native(&payload.id) {
            native_route && ctx.approvals.contains(&payload.id)
        } else {
            adapter.answer_channel().is_some()
        };
        let record = signaltty_core::model::Decision {
            id: payload.id,
            prompt: payload.prompt,
            options: payload
                .options
                .into_iter()
                .map(|o| signaltty_core::model::DecisionOption {
                    id: o.id,
                    label: o.label,
                })
                .collect(),
            answerable,
            received_at: Utc::now(),
        };
        ctx.store.write().unwrap().set_decision(&pid, record);
    } else {
        let moved_on = {
            let s = ctx.store.read().unwrap();
            s.panes
                .get(&pid)
                .is_some_and(|pane| pane.lifecycle != Lifecycle::Blocked)
        };
        if moved_on {
            // A still-unanswered decision at turn end (Stop/SessionEnd) is
            // kept: dropping it would park the task in working with nobody
            // left to answer. Only a stale record on a live turn is cleared.
            let mut s = ctx.store.write().unwrap();
            let unanswered_at_turn_end = s.panes.get(&pid).is_some_and(|pane| {
                pane.pending_decision.is_some() && !matches!(pane.lifecycle, Lifecycle::Working)
            });
            if !unanswered_at_turn_end {
                s.clear_decision(&pid, "moved_on");
            }
        }
    }
    if let Some(message) = decision.message.or(decision_prompt) {
        let mut s = ctx.store.write().unwrap();
        if let Some(p) = s.panes.get_mut(&pid) {
            p.last_message = Some(message);
            p.last_activity_at = Utc::now();
        }
    }

    // 3. Notification: explicit params win, else the adapter's draft.
    let explicit = p.message.or_else(|| p.body.clone());
    if let Some(body) = explicit {
        if !body.is_empty() {
            let severity = parse_severity(&p.severity)?;
            push_notification(
                &ctx.store,
                &pid,
                p.title.as_deref(),
                &body,
                severity,
                &format!("hook:{agent}:{hook}"),
            );
        }
    } else if let Some(draft) = adapter.notification_event(&event) {
        push_notification_full(
            &ctx.store,
            &pid,
            p.title.as_deref(),
            &draft.title,
            draft.body.as_deref().unwrap_or(""),
            draft.severity,
            &format!("hook:{agent}:{hook}"),
        );
    }

    // Touch activity even for no-op events (the agent is alive).
    {
        let mut s = ctx.store.write().unwrap();
        if let Some(p) = s.panes.get_mut(&pid) {
            p.last_activity_at = Utc::now();
        }
    }
    let (lifecycle, attention) = {
        let s = ctx.store.read().unwrap();
        let p = s.panes.get(&pid).unwrap();
        (
            p.lifecycle.as_str().to_string(),
            p.attention.as_str().to_string(),
        )
    };
    ctx.mark_persist();
    Ok((
        json!({"accepted": true, "agent": agent, "event": hook, "pane_id": pid,
               "lifecycle": lifecycle, "attention": attention}),
        ConnEffect::default(),
    ))
}

fn h_report_session(ctx: &Ctx, params: &Value) -> Handler {
    let p: params::ReportSession = decode(params)?;
    let pane_id = p.pane_id;
    let session_id = p.agent_session_id;
    let agent = p.agent.unwrap_or_else(|| "generic".to_string());
    let kind = AgentKind::parse(&agent).unwrap_or(AgentKind::Generic);
    let mut s = ctx.store.write().unwrap();
    let pane = s
        .panes
        .get_mut(&pane_id)
        .ok_or_else(|| (code::NO_SUCH_PANE.to_string(), pane_id.clone()))?;
    pane.agent.kind = kind;
    // The adapter owns the official resume command for this id.
    pane.agent.resume_argv = ctx
        .adapter_for_kind(kind)
        .resume_capability(&session_id)
        .map(|r| signaltty_agent::resolve_resume_argv(&pane.argv, &r.argv));
    let changed = s.set_agent_session(&pane_id, session_id).is_some();
    let pane = s.panes[&pane_id].clone();
    if !changed {
        s.emit(event::PANE_UPDATED, json!({"pane":pane}));
    }
    drop(s);

    ctx.mark_persist();
    Ok((pane_result(&pane), ConnEffect::default()))
}

// ---- subscribe / wait / focus ----

fn h_subscribe(ctx: &Ctx, params: &Value) -> Handler {
    let p: params::Subscribe = decode(params)?;
    let events = p.events.unwrap_or_else(|| vec!["*".to_string()]);
    let s = ctx.store.read().unwrap();
    let mut reply = json!({"subscribed":true,"seq":s.seq});
    let mut effect = ConnEffect {
        subscribe: Some(events.clone()),
        task_ids: p.task_ids.clone(),
        fence: Some(s.seq),
        ..ConnEffect::default()
    };
    if let Some(from) = p.from_seq {
        let (coverage, replay) = s.replay(from, &events, p.task_ids.as_deref());
        effect.close = coverage["status"] != "complete";
        reply["subscribed"] = json!(!effect.close);
        reply["replay"] = coverage;
        effect.replay = Some(replay);
    }
    Ok((reply, effect))
}

fn wait_satisfied(pane: &Pane, until: &str) -> bool {
    if until == "seen" || until == "attention_cleared" {
        return pane.attention == Attention::None;
    }
    if let Some(a) = Attention::parse(until) {
        return pane.attention == a;
    }
    if until == "exited" {
        return matches!(pane.live, LiveState::Exited { .. })
            || pane.lifecycle == Lifecycle::Exited;
    }
    match Lifecycle::parse(until) {
        Some(l) => pane.lifecycle == l,
        None => false,
    }
}

async fn h_wait(ctx: &Ctx, req: &Request, params: &Value) -> (Response, ConnEffect) {
    let respond = |result: Result<Value, (String, String)>| {
        (
            match result {
                Ok(value) => Response::ok(&req.id, value),
                Err((code, message)) => Response::err(&req.id, &code, message),
            },
            ConnEffect::default(),
        )
    };
    let p: params::Wait = match decode(params) {
        Ok(p) => p,
        Err(e) => return respond(Err(e)),
    };
    let outcomes = p.until.into_vec();
    if outcomes.is_empty() {
        return respond(Err(bad_params("until must contain at least one outcome")));
    }
    for outcome in &outcomes {
        if let Err(e) = validate_until(outcome) {
            return respond(Err(e));
        }
    }
    if p.after
        .as_ref()
        .map(|a| a.pane_id != p.pane_id || a.process_instance.is_empty())
        .unwrap_or(false)
    {
        return respond(Err(bad_params(
            "baseline must identify this pane and its process",
        )));
    }
    // Subscribe first; all evaluations use authoritative progress, including lag.
    let mut rx = ctx.bcast.subscribe();
    let Some(deadline) = tokio::time::Instant::now()
        .checked_add(std::time::Duration::from_secs(p.timeout_s.unwrap_or(3600)))
    else {
        return respond(Err(bad_params("wait timeout is too large")));
    };
    let mut tick = tokio::time::interval(std::time::Duration::from_millis(500));
    loop {
        {
            let s = ctx.store.read().unwrap();
            let Some(pane) = s.panes.get(&p.pane_id) else {
                return respond(Err((code::NO_SUCH_PANE.into(), p.pane_id.clone())));
            };
            let current = s.wait_baseline(&p.pane_id);
            if let Some(after) = &p.after {
                let Some(current) = &current else {
                    return respond(Err((
                        code::IDENTITY_CHANGED.into(),
                        "pane process is unavailable".into(),
                    )));
                };
                if after.process_instance != current.process_instance
                    || after.session_generation != current.session_generation
                    || after
                        .agent_session_id
                        .as_ref()
                        .map(|id| Some(id) != current.agent_session_id.as_ref())
                        .unwrap_or(false)
                {
                    return respond(Err((
                        code::IDENTITY_CHANGED.into(),
                        "pane process or agent session changed".into(),
                    )));
                }
                if after.lifecycle_seq > current.lifecycle_seq
                    || after.attention_seq > current.attention_seq
                {
                    return respond(Err(bad_params("baseline transition is ahead of this pane")));
                }
            }
            for outcome in &outcomes {
                let transition = s.matching_transition(&p.pane_id, outcome).unwrap_or(0);
                let satisfied = match &p.after {
                    Some(after) => transition > after.threshold(outcome),
                    None => wait_satisfied(pane, outcome),
                };
                if satisfied {
                    return respond(Ok(
                        json!({"satisfied":true,"outcome":outcome,"transition_seq":transition,"lifecycle":pane.lifecycle.as_str(),"attention":pane.attention.as_str()}),
                    ));
                }
            }
        }
        tokio::select! {
            biased;
            _ = tokio::time::sleep_until(deadline) => return respond(Err((code::TIMEOUT.into(),format!("timed out waiting for {}",outcomes.join(","))))),
            _ = tick.tick() => {},
            event = rx.recv() => {
                if matches!(event,Err(broadcast::error::RecvError::Closed)) { return respond(Err((code::INTERNAL.into(),"event bus closed".into()))); }
            }
        }
    }
}

fn h_next_unread(ctx: &Ctx) -> Handler {
    let s = ctx.store.read().unwrap();
    Ok((json!({"pane_id": s.next_unread()}), ConnEffect::default()))
}

fn h_plugin_list(ctx: &Ctx) -> Handler {
    Ok((ctx.plugins.status(), ConnEffect::default()))
}

fn h_plugin_reload(ctx: &Ctx) -> Handler {
    ctx.plugins.reload();
    Ok((ctx.plugins.status(), ConnEffect::default()))
}

// ---- pane.submit & task orchestration ----

async fn h_pane_submit(ctx: &Ctx, req: &Request, params: &Value) -> (Response, ConnEffect) {
    let p: params::PaneSubmit = match decode(params) {
        Ok(p) => p,
        Err((code, msg)) => return (Response::err(&req.id, &code, msg), ConnEffect::default()),
    };
    let submit_delay = Duration::from_millis(p.submit_delay_ms.unwrap_or(300));
    let stall_timeout = Duration::from_secs(p.stall_timeout_s.unwrap_or(5));
    match crate::submit::submit_prompt(
        &SubmitCtx::from(ctx),
        &p.pane_id,
        &p.text,
        submit_delay,
        stall_timeout,
        false,
        true,
        crate::submit::PANE_SUBMIT_MAX_BYTES,
    )
    .await
    {
        Ok(outcome) => (
            Response::ok(&req.id, serde_json::to_value(outcome).unwrap()),
            ConnEffect::default(),
        ),
        Err(e) => {
            let resp = match e.details {
                Some(details) => Response::err_with_details(&req.id, &e.code, e.message, details),
                None => Response::err(&req.id, &e.code, e.message),
            };
            (resp, ConnEffect::default())
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::store::Store;
    use signaltty_core::model::{Decision, DecisionOption, Pane, PtySize};
    use signaltty_core::state::Attention;
    use std::sync::RwLock;

    fn blocked_store() -> (SharedStore, params::DecisionAnswer) {
        let now = chrono::Utc::now();
        let mut pane = Pane::new(
            "ws_1".into(),
            "tab_1".into(),
            "/tmp".into(),
            vec!["sh".into()],
            PtySize::default(),
            now,
        );
        pane.lifecycle = Lifecycle::Blocked;
        let pane_id = pane.id.clone();
        let mut store = Store::new();
        store.panes.insert(pane_id.clone(), pane);
        store.set_decision(
            &pane_id,
            Decision {
                id: "d1".into(),
                prompt: "Allow?".into(),
                options: vec![DecisionOption {
                    id: "once".into(),
                    label: "Once".into(),
                }],
                answerable: true,
                received_at: now,
            },
        );
        store.raise_attention(&pane_id, Attention::PermissionRequired);
        let answer = params::DecisionAnswer {
            pane_id,
            decision_id: "d1".into(),
            option_id: "once".into(),
        };
        (Arc::new(RwLock::new(store)), answer)
    }

    fn names(store: &SharedStore) -> Vec<String> {
        let s = store.read().unwrap();
        s.events.iter().map(|e| e.name.clone()).collect()
    }

    #[test]
    fn failed_typed_delivery_keeps_the_pane_blocked() {
        let (store, p) = blocked_store();
        let err = consume_deliver_resume(&store, &p, || Err("pane has no live PTY".into()));
        assert!(matches!(err, Err(TypedAnswerError::Delivery(_))));
        let s = store.read().unwrap();
        let pane = &s.panes[&p.pane_id];
        // The gate is consumed (no second delivery) but nothing resumed.
        assert!(pane.pending_decision.is_none());
        assert_eq!(pane.lifecycle, Lifecycle::Blocked);
        let events = names(&store);
        assert!(events.contains(&signaltty_proto::event::DECISION_ANSWERED.to_string()));
        let last = s.events.back().unwrap();
        assert_eq!(last.name, event::DECISION_CLEARED);
        assert_eq!(last.payload["reason"], "pane_exited");
        assert!(!events.contains(&"agent.working".to_string()));
    }

    #[test]
    fn delivered_typed_answer_resumes_the_blocked_pane() {
        let (store, p) = blocked_store();
        assert!(consume_deliver_resume(&store, &p, || Ok(())).is_ok());
        let s = store.read().unwrap();
        let pane = &s.panes[&p.pane_id];
        assert!(pane.pending_decision.is_none());
        assert_eq!(pane.lifecycle, Lifecycle::Working);
        assert_eq!(pane.attention, Attention::None);
    }

    #[test]
    fn stale_typed_answer_never_delivers() {
        let (store, mut p) = blocked_store();
        p.decision_id = "old".into();
        let err = consume_deliver_resume(&store, &p, || panic!("must not deliver"));
        assert!(matches!(err, Err(TypedAnswerError::Stale)));
        let s = store.read().unwrap();
        assert_eq!(s.panes[&p.pane_id].lifecycle, Lifecycle::Blocked);
        assert!(s.panes[&p.pane_id].pending_decision.is_some());
    }
}
