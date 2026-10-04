//! Task orchestration: task lifecycle, worktree checkout, agent worker pane spawn,
//! and background prompt delivery.

use std::collections::HashMap;
use std::path::PathBuf;
use std::time::Duration;

use chrono::Utc;
use serde_json::{json, Value};
use signaltty_core::model::{
    AgentKind, Disposition, DispositionOutcome, LiveState, Relationship, Task, TaskResult,
};
use signaltty_core::state::{Attention, Lifecycle, TaskState};
use signaltty_core::{new_context_id, new_task_id};
use signaltty_proto::{code, Request, Response};
use tokio::sync::broadcast;

use crate::params::{self, decode};
use crate::router::{ConnEffect, Ctx};
use crate::store::Store;
use crate::submit::SubmitCtx;

/// Persist a `failed` task for a `task.start` failure after the worktree was
/// created, unlock the checkout so it stays removable, and return the error
/// with `details: {task_id, stage}`. Pre-worktree validation never reaches here.
fn fail_task_start(
    ctx: &Ctx,
    reservation: crate::store::TaskReservation,
    task: Task,
    stage: &str,
    error_code: &str,
    message: String,
    req_id: &str,
) -> (Response, ConnEffect) {
    let task_id = task.id.clone();
    let wt_str = task.worktree_path.to_string_lossy().to_string();
    let repo_str = task.source_repo.to_string_lossy().to_string();
    reservation.commit(task);
    {
        let mut s = ctx.store.write().unwrap();
        s.task_fail(
            &task_id,
            Some(json!({"stage": stage, "error": message.clone()})),
        );
    }
    // Leave the checkout unlocked so `task.finish --discard` (which unlocks
    // again, harmlessly) or a plain `git worktree remove` can delete it.
    let _ = crate::git::git_output(&repo_str, &["worktree", "unlock", &wt_str]);
    ctx.mark_persist();
    (
        Response::err_with_details(
            req_id,
            error_code,
            message,
            json!({"task_id": task_id, "stage": stage}),
        ),
        ConnEffect::default(),
    )
}

/// Remove a task worktree with the herdr leftover guard: unlock, then
/// `git worktree remove`. A leftover directory is force-deleted only after
/// git says it is not a registered worktree of this repo *and* it is the
/// recorded task path — never while git still claims it (a live checkout
/// or a failed remove must not be `remove_dir_all`'d). Returns a cleanup
/// error string when the checkout survives.
fn remove_task_worktree(src_repo: &str, worktree_path: &std::path::Path) -> Option<String> {
    let wt_str = worktree_path.to_string_lossy().to_string();
    let _ = crate::git::git_output(src_repo, &["worktree", "unlock", &wt_str]);
    let rm_err = match crate::git::git_output(
        src_repo,
        &["worktree", "remove", "--force", "--force", &wt_str],
    ) {
        Ok(out) if out.status.success() => None,
        Ok(out) => Some(String::from_utf8_lossy(&out.stderr).trim().to_string()),
        Err(e) => Some(format!("git worktree remove failed: {e}")),
    };
    let _ = crate::git::git_output(src_repo, &["worktree", "prune"]);
    if !worktree_path.exists() {
        return None;
    }
    if crate::git::is_worktree_registered(src_repo, &wt_str) {
        return Some(format!(
            "worktree checkout left in place (still registered): {}",
            rm_err.unwrap_or_else(|| "remove reported success but the directory remains".into())
        ));
    }
    std::fs::remove_dir_all(worktree_path)
        .err()
        .map(|e| format!("failed to remove worktree directory: {e}"))
}

pub async fn h_task_start(ctx: &Ctx, req: &Request, params: &Value) -> (Response, ConnEffect) {
    let p: params::TaskStart = match decode(params) {
        Ok(p) => p,
        Err((ref code, ref msg)) => {
            return (Response::err(&req.id, code, msg), ConnEffect::default())
        }
    };

    if p.contract.objective.trim().is_empty() {
        return (
            Response::err(
                &req.id,
                code::BAD_PARAMS,
                "contract.objective must not be empty",
            ),
            ConnEffect::default(),
        );
    }
    if let Err(e) = p.contract.validate() {
        return (
            Response::err(&req.id, code::BAD_PARAMS, e.to_string()),
            ConnEffect::default(),
        );
    }

    // Idempotent retry: the same client_request_id returns the existing task.
    // ponytail: the lookup key doubles as a path-lock key to serialize
    // concurrent duplicates; a dedicated map if this ever needs pruning.
    let _request_guard = match &p.client_request_id {
        Some(id) => Some(
            ctx.worktrees
                .path_lock(std::path::Path::new(&format!("client-request:{id}")))
                .lock_owned()
                .await,
        ),
        None => None,
    };
    if let Some(id) = &p.client_request_id {
        let s = ctx.store.read().unwrap();
        if let Some(task) = s
            .tasks
            .values()
            .find(|t| t.client_request_id.as_deref() == Some(id))
        {
            let pane = task.pane_id.as_ref().and_then(|pid| s.panes.get(pid));
            return (
                Response::ok(&req.id, json!({ "task": task, "pane": pane })),
                ConnEffect::default(),
            );
        }
    }

    // Resolve agent and argv
    let (argv, _kind, agent_name) = match resolve_agent_and_argv(ctx, &p) {
        Ok(res) => res,
        Err((code, msg)) => return (Response::err(&req.id, &code, msg), ConnEffect::default()),
    };

    // Concurrency cap check and reservation
    let reservation = match Store::reserve_task_slot(&ctx.store, ctx.config.max_parallel_tasks) {
        Ok(guard) => guard,
        Err((running, max)) => {
            return (
                Response::err_with_details(
                    &req.id,
                    code::RATE_LIMITED,
                    format!("task concurrency limit reached ({running}/{max})"),
                    json!({
                        "active_tasks": running,
                        "max_parallel_tasks": max,
                    }),
                ),
                ConnEffect::default(),
            );
        }
    };

    let context_id = p.context_id.unwrap_or_else(new_context_id);

    // 2. Base ref resolution
    let base_ref = p.base_ref.unwrap_or_else(|| "HEAD".to_string());
    if let Err(e) = crate::git::validate_base_ref_format(&base_ref) {
        return (
            Response::err(&req.id, code::BAD_PARAMS, e),
            ConnEffect::default(),
        );
    }
    let fetch_first = p.fetch_first.unwrap_or(false);
    let base_sha = match crate::git::resolve_base_ref(&p.repo, &base_ref, fetch_first) {
        Ok(sha) => sha,
        Err(err) => {
            return (
                Response::err(&req.id, code::BAD_PARAMS, err),
                ConnEffect::default(),
            )
        }
    };
    let target_branch = crate::git::detect_target_branch(&p.repo);

    // 3. Branch and worktree path
    let task_id = new_task_id();
    let short_id = &task_id[task_id.len().saturating_sub(8)..];
    let branch = match p.branch {
        Some(b) => b,
        None => {
            let label_sanitized =
                crate::git::sanitize_branch_for_path(p.label.as_deref().unwrap_or("task"));
            format!("signaltty/{label_sanitized}-{short_id}")
        }
    };
    if let Err(e) = crate::git::validate_branch_name(&branch) {
        return (
            Response::err(&req.id, code::BAD_PARAMS, e),
            ConnEffect::default(),
        );
    }
    let preexisting_branch = crate::git::branch_exists(&p.repo, &branch);
    // The composed body must be sendable before any checkout exists.
    // pane.submit stays at 32 KiB; the worker cap grows by this task's
    // fixed preamble so a maximum objective with no extra sections fits.
    let prompt =
        signaltty_core::model::compose_worker_prompt(&task_id, &branch, &base_sha, &p.contract);
    let submit_max = crate::submit::worker_submit_max_bytes(&task_id, &branch, &base_sha);
    if prompt.len() > submit_max {
        return (
            Response::err(
                &req.id,
                code::BAD_PARAMS,
                format!(
                    "composed worker prompt is {} bytes, over the {submit_max} byte submit limit",
                    prompt.len()
                ),
            ),
            ConnEffect::default(),
        );
    }
    let worktree_path = match p.path {
        Some(path) => PathBuf::from(path),
        None => crate::git::default_worktree_path(&p.repo, &branch),
    };

    // Under per-path lock
    let path_lock = ctx.worktrees.path_lock(&worktree_path);
    let _guard = path_lock.lock().await;

    if let Err((c, m)) = crate::worktrees::create_task_worktree(
        &p.repo,
        &worktree_path,
        &branch,
        &base_sha,
        preexisting_branch,
    )
    .await
    {
        return (Response::err(&req.id, &c, m), ConnEffect::default());
    }

    // The checkout now exists: every later failure must persist a failed
    // task with evidence (never a bare error with a leaked locked worktree).
    // Pre-worktree validation above stays synchronous BAD_PARAMS.
    let now = Utc::now();
    let label = p
        .label
        .clone()
        .unwrap_or_else(|| format!("worker-{short_id}"));
    let relationship = p.relationship.unwrap_or(Relationship::Subagent);
    let mut task = Task {
        id: task_id.clone(),
        context_id: context_id.clone(),
        parent_task_id: None,
        pane_id: None,
        parent_pane_id: p.parent_pane_id.clone(),
        root_pane_id: None,
        relationship,
        label,
        contract: p.contract.clone(),
        agent: agent_name.clone(),
        source_repo: PathBuf::from(&p.repo),
        target_branch: target_branch.clone(),
        worktree_path: worktree_path.clone(),
        branch: branch.clone(),
        preexisting_branch,
        base_ref: base_ref.clone(),
        base_sha: base_sha.clone(),
        state: TaskState::Pending,
        result: None,
        disposition: Disposition::default(),
        status_reason: None,
        finish_error: None,
        worker_pid: None,
        worker_cmd: Some(argv.clone()),
        client_request_id: p.client_request_id.clone(),
        created_at: now,
        updated_at: now,
    };

    // Unknown parent pane fails here (stage parent) instead of storing a
    // dangling lineage: pane.spawn refuses it the same way (NO_SUCH_PANE).
    if let Some(parent_id) = task.parent_pane_id.clone() {
        let (root, known) = {
            let s = ctx.store.read().unwrap();
            match s.panes.get(&parent_id) {
                Some(parent) => (
                    parent
                        .root_pane_id
                        .clone()
                        .or_else(|| Some(parent.id.clone())),
                    true,
                ),
                None => (None, false),
            }
        };
        if !known {
            return fail_task_start(
                ctx,
                reservation,
                task,
                "parent",
                code::NO_SUCH_PANE,
                format!("no such parent pane '{parent_id}'"),
                &req.id,
            );
        }
        task.root_pane_id = root;
    }

    // 4. Pane setup & spawn using existing helpers (no ws.tabs[0] panic; path-reference guard alive)
    let wt_path_str = worktree_path.to_string_lossy().to_string();

    let ws_id = {
        let s = ctx.store.read().unwrap();
        s.workspaces
            .values()
            .find(|w| w.cwd == wt_path_str)
            .map(|w| w.id.clone())
    };
    let ws_id = match ws_id {
        Some(id) => id,
        None => {
            let ws_name = p
                .label
                .clone()
                .unwrap_or_else(|| format!("task-{}", short_id));
            let (val, _) = match crate::router::h_workspace_create(
                ctx,
                &json!({
                    "name": ws_name,
                    "cwd": wt_path_str.clone(),
                }),
            ) {
                Ok(res) => res,
                Err((ref c, ref m)) => {
                    return fail_task_start(
                        ctx,
                        reservation,
                        task,
                        "workspace",
                        c,
                        m.clone(),
                        &req.id,
                    );
                }
            };
            val["workspace"]["id"].as_str().unwrap().to_string()
        }
    };

    // Tab lookup / create: if active tab is empty (no layout), reuse it; otherwise create a tab
    let tab_id = {
        let s = ctx.store.read().unwrap();
        let ws = match s.workspaces.get(&ws_id) {
            Some(w) => w,
            None => {
                return fail_task_start(
                    ctx,
                    reservation,
                    task,
                    "workspace",
                    code::NO_SUCH_WORKSPACE,
                    ws_id.clone(),
                    &req.id,
                );
            }
        };
        ws.active_tab_id.as_ref().and_then(|tid| {
            s.tabs
                .get(tid)
                .filter(|t| t.layout.is_none())
                .map(|t| t.id.clone())
        })
    };
    let tab_id = match tab_id {
        Some(id) => id,
        None => {
            let (val, _) = match crate::router::h_tab_create(
                ctx,
                &json!({
                    "workspace_id": ws_id,
                    "title": "task",
                }),
            ) {
                Ok(res) => res,
                Err((ref c, ref m)) => {
                    return fail_task_start(ctx, reservation, task, "tab", c, m.clone(), &req.id);
                }
            };
            val["tab"]["id"].as_str().unwrap().to_string()
        }
    };

    let mut env = HashMap::new();
    if let Some(parent) = &task.parent_pane_id {
        env.insert("SIGNALTTY_PARENT_PANE".to_string(), parent.clone());
    }
    env.insert("SIGNALTTY_TASK".to_string(), task_id.clone());

    // Reuse h_pane_spawn: manages worktree path-reference guard, PTY spawn, tab layout, and store publishing
    let (pane_val, _) = match crate::router::h_pane_spawn(
        ctx,
        &json!({
            "workspace_id": ws_id,
            "tab_id": tab_id,
            "cwd": wt_path_str,
            "argv": argv,
            "env": env,
            "agent_hint": agent_name,
            "parent_pane_id": task.parent_pane_id,
            "label": task.label,
            "relationship": relationship,
            "task_id": task_id,
        }),
    ) {
        Ok(res) => res,
        Err((ref c, ref m)) => {
            return fail_task_start(ctx, reservation, task, "spawn", c, m.clone(), &req.id);
        }
    };

    let pane_id = pane_val["pane"]["id"].as_str().unwrap().to_string();
    task.pane_id = Some(pane_id.clone());
    task.updated_at = Utc::now();

    reservation.commit(task.clone());
    ctx.mark_persist();

    // 5. Background ready check and prompt submit
    spawn_background_submit(
        ctx,
        task_id.clone(),
        pane_id.clone(),
        prompt,
        p.ready_timeout_s.unwrap_or(30),
        p.stall_timeout_s.unwrap_or(5),
        Duration::from_millis(p.submit_delay_ms.unwrap_or(300)),
        submit_max,
    );

    (
        Response::ok(&req.id, json!({ "task": task, "pane": pane_val["pane"] })),
        ConnEffect::default(),
    )
}

type ResolvedAgent = (Vec<String>, AgentKind, Option<String>);

fn resolve_agent_and_argv(
    ctx: &Ctx,
    p: &params::TaskStart,
) -> Result<ResolvedAgent, (String, String)> {
    match (p.agent.as_deref(), p.argv.as_ref()) {
        (None, None) => Err((
            code::BAD_PARAMS.to_string(),
            "task.start requires either 'agent' or 'argv'".to_string(),
        )),
        (Some(agent_name), None) => {
            let Some(adapter) = signaltty_agent::adapter_for_name(agent_name) else {
                return Err((
                    code::BAD_PARAMS.to_string(),
                    format!("unknown agent '{agent_name}'"),
                ));
            };
            let kind = adapter.metadata().kind;
            if matches!(kind, AgentKind::Generic | AgentKind::None)
                || adapter.metadata().binaries.is_empty()
            {
                return Err((
                    code::BAD_PARAMS.to_string(),
                    format!(
                        "agent '{agent_name}' has no interactive default binary; specify 'argv'"
                    ),
                ));
            }
            let argv = vec![adapter.metadata().binaries[0].to_string()];
            Ok((argv, kind, Some(agent_name.to_string())))
        }
        (Some(agent_name), Some(argv)) => {
            if argv.is_empty() {
                return Err((
                    code::BAD_PARAMS.to_string(),
                    "'argv' must not be empty".to_string(),
                ));
            }
            let kind = AgentKind::parse(agent_name).unwrap_or_else(|| ctx.detect_kind(argv));
            Ok((argv.clone(), kind, Some(agent_name.to_string())))
        }
        (None, Some(argv)) => {
            if argv.is_empty() {
                return Err((
                    code::BAD_PARAMS.to_string(),
                    "'argv' must not be empty".to_string(),
                ));
            }
            let kind = ctx.detect_kind(argv);
            let agent_name = if matches!(kind, AgentKind::Generic | AgentKind::None) {
                None
            } else {
                Some(kind.as_str().to_string())
            };
            Ok((argv.clone(), kind, agent_name))
        }
    }
}

#[allow(clippy::too_many_arguments)]
pub(crate) fn spawn_background_submit(
    ctx: &Ctx,
    bg_task_id: String,
    bg_pane_id: String,
    prompt: String,
    ready_timeout_s: u64,
    stall_timeout_s: u64,
    submit_delay: Duration,
    submit_max_bytes: usize,
) {
    let bg_submit_ctx = SubmitCtx {
        store: ctx.store.clone(),
        ptys: ctx.ptys.clone(),
        bcast: ctx.bcast.clone(),
    };
    let ready_timeout = Duration::from_secs(ready_timeout_s);
    let stall_timeout = Duration::from_secs(stall_timeout_s);
    let mut rx = ctx.bcast.subscribe();

    tokio::spawn(async move {
        let ready_deadline = tokio::time::Instant::now() + ready_timeout;
        let mut tick = tokio::time::interval(Duration::from_millis(50));

        loop {
            enum Check {
                Done,
                PaneMissing,
                PaneExited,
                Ready,
                Wait,
            }
            let check = {
                let s = bg_submit_ctx.store.read().unwrap();
                let Some(task) = s.tasks.get(&bg_task_id) else {
                    return;
                };
                if task.state != TaskState::Pending {
                    Check::Done
                } else {
                    match s.panes.get(&bg_pane_id) {
                        None => Check::PaneMissing,
                        Some(pane) if !matches!(pane.live, LiveState::Live) => Check::PaneExited,
                        Some(pane)
                            if matches!(pane.lifecycle, Lifecycle::Idle | Lifecycle::Done) =>
                        {
                            Check::Ready
                        }
                        Some(_) => Check::Wait,
                    }
                }
            };

            match check {
                Check::Done => return,
                Check::PaneMissing => {
                    let mut s = bg_submit_ctx.store.write().unwrap();
                    s.task_fail(
                        &bg_task_id,
                        Some(json!({"stage": "ready_timeout", "error": "pane missing"})),
                    );
                    return;
                }
                Check::PaneExited => {
                    let mut s = bg_submit_ctx.store.write().unwrap();
                    s.task_fail(
                        &bg_task_id,
                        Some(json!({"stage": "ready_timeout", "error": "pane exited"})),
                    );
                    return;
                }
                Check::Ready => break,
                Check::Wait => {}
            }

            tokio::select! {
                biased;
                _ = tokio::time::sleep_until(ready_deadline) => {
                    let mut s = bg_submit_ctx.store.write().unwrap();
                    s.task_fail(&bg_task_id, Some(json!({"stage": "ready_timeout"})));
                    return;
                }
                _ = tick.tick() => {}
                event = rx.recv() => {
                    if matches!(event, Err(broadcast::error::RecvError::Closed)) {
                        return;
                    }
                }
            }
        }

        let submit_res = crate::submit::submit_prompt(
            &bg_submit_ctx,
            &bg_pane_id,
            &prompt,
            submit_delay,
            stall_timeout,
            true,
            true,
            submit_max_bytes,
        )
        .await;

        match submit_res {
            Ok(_) => {
                let mut s = bg_submit_ctx.store.write().unwrap();
                s.task_background_ready(&bg_task_id);
            }
            Err(e) => {
                let mut s = bg_submit_ctx.store.write().unwrap();
                let stage = if e.code == code::TIMEOUT {
                    "activity_gate"
                } else {
                    "submit_refused"
                };
                s.task_fail(
                    &bg_task_id,
                    Some(json!({"stage": stage, "error": e.message})),
                );
            }
        }
    });
}

pub fn h_task_get(ctx: &Ctx, req: &Request, params: &Value) -> (Response, ConnEffect) {
    let p: params::TaskGet = match decode(params) {
        Ok(p) => p,
        Err((ref c, ref m)) => return (Response::err(&req.id, c, m), ConnEffect::default()),
    };
    let s = ctx.store.read().unwrap();
    match s.tasks.get(&p.task_id) {
        Some(task) => (
            Response::ok(&req.id, json!({ "task": task })),
            ConnEffect::default(),
        ),
        None => (
            Response::err(
                &req.id,
                code::NO_SUCH_TASK,
                format!("no such task '{}'", p.task_id),
            ),
            ConnEffect::default(),
        ),
    }
}

pub fn h_task_list(ctx: &Ctx, req: &Request, params: &Value) -> (Response, ConnEffect) {
    let p: params::TaskList = match decode(params) {
        Ok(p) => p,
        Err((ref c, ref m)) => return (Response::err(&req.id, c, m), ConnEffect::default()),
    };
    let s = ctx.store.read().unwrap();
    let mut tasks: Vec<_> = s
        .tasks
        .values()
        .filter(|t| {
            if let Some(ctx_id) = &p.context_id {
                if &t.context_id != ctx_id {
                    return false;
                }
            }
            if let Some(st) = &p.state {
                if &t.state != st {
                    return false;
                }
            }
            true
        })
        .cloned()
        .collect();

    tasks.sort_by_key(|t| t.created_at);
    let limit = p.limit.unwrap_or(100).min(1000);
    tasks.truncate(limit);

    (
        Response::ok(&req.id, json!({ "tasks": tasks })),
        ConnEffect::default(),
    )
}

pub fn h_task_cancel(ctx: &Ctx, req: &Request, params: &Value) -> (Response, ConnEffect) {
    let p: params::TaskCancel = match decode(params) {
        Ok(p) => p,
        Err((ref c, ref m)) => return (Response::err(&req.id, c, m), ConnEffect::default()),
    };
    let mut s = ctx.store.write().unwrap();
    match s.task_cancel(&p.task_id) {
        Ok(_) => {
            let task = s.tasks.get(&p.task_id).unwrap();
            let pane_id = task.pane_id.clone();
            if let Some(ref pid) = pane_id {
                ctx.ptys.destroy(pid, Some("TERM"));
                s.set_exited(pid, Some(0));
            }
            ctx.mark_persist();
            let task = s.tasks.get(&p.task_id).unwrap();
            (
                Response::ok(&req.id, json!({ "task": task })),
                ConnEffect::default(),
            )
        }
        Err((c, m)) => (Response::err(&req.id, &c, m), ConnEffect::default()),
    }
}

fn task_matches_until(task: &Task, until_list: &[String]) -> bool {
    for u in until_list {
        match u.as_str() {
            "terminal" => {
                if task.state.is_terminal() {
                    return true;
                }
            }
            "settled" => {
                if task.state.is_terminal() || task.state == TaskState::InputRequired {
                    return true;
                }
            }
            s => {
                if task.state.as_str() == s {
                    return true;
                }
            }
        }
    }
    false
}

pub async fn h_task_wait(ctx: &Ctx, req: &Request, params: &Value) -> (Response, ConnEffect) {
    let p: params::TaskWait = match decode(params) {
        Ok(p) => p,
        Err((ref c, ref m)) => return (Response::err(&req.id, c, m), ConnEffect::default()),
    };

    if (p.task_id.is_none() && p.context_id.is_none())
        || (p.task_id.is_some() && p.context_id.is_some())
    {
        return (
            Response::err(
                &req.id,
                code::BAD_PARAMS,
                "exactly one of task_id or context_id must be provided",
            ),
            ConnEffect::default(),
        );
    }

    let until_list: Vec<String> = match p.until {
        None => vec!["settled".to_string()],
        Some(Value::String(s)) => vec![s],
        Some(Value::Array(arr)) => {
            let mut list = Vec::with_capacity(arr.len());
            for v in arr {
                match v {
                    Value::String(s) => list.push(s),
                    _ => {
                        return (
                            Response::err(&req.id, code::BAD_PARAMS, "until must be strings"),
                            ConnEffect::default(),
                        );
                    }
                }
            }
            list
        }
        Some(_) => {
            return (
                Response::err(&req.id, code::BAD_PARAMS, "until must be a string or array"),
                ConnEffect::default(),
            );
        }
    };

    if let Some(bad) = until_list
        .iter()
        .find(|u| !matches!(u.as_str(), "terminal" | "settled") && TaskState::parse(u).is_none())
    {
        return (
            Response::err(&req.id, code::BAD_PARAMS, format!("bad 'until': {bad}")),
            ConnEffect::default(),
        );
    }

    if until_list.is_empty() {
        return (
            Response::err(&req.id, code::BAD_PARAMS, "until must not be empty"),
            ConnEffect::default(),
        );
    }

    // Verify task_id exists if specified
    if let Some(task_id) = &p.task_id {
        let s = ctx.store.read().unwrap();
        if !s.tasks.contains_key(task_id) {
            return (
                Response::err(
                    &req.id,
                    code::NO_SUCH_TASK,
                    format!("no such task '{}'", task_id),
                ),
                ConnEffect::default(),
            );
        }
    }

    let deadline = tokio::time::Instant::now() + Duration::from_secs(p.timeout_s.unwrap_or(3600));
    let mut rx = ctx.bcast.subscribe();
    let mut tick = tokio::time::interval(Duration::from_millis(50));

    loop {
        {
            let s = ctx.store.read().unwrap();
            if let Some(task_id) = &p.task_id {
                let Some(task) = s.tasks.get(task_id) else {
                    return (
                        Response::err(
                            &req.id,
                            code::NO_SUCH_TASK,
                            format!("no such task '{}'", task_id),
                        ),
                        ConnEffect::default(),
                    );
                };
                if task_matches_until(task, &until_list) {
                    return (
                        Response::ok(&req.id, json!({ "satisfied": true, "tasks": [task] })),
                        ConnEffect::default(),
                    );
                }
            } else if let Some(context_id) = &p.context_id {
                let tasks: Vec<_> = s
                    .tasks
                    .values()
                    .filter(|t| &t.context_id == context_id)
                    .cloned()
                    .collect();
                if tasks.is_empty() || tasks.iter().all(|t| task_matches_until(t, &until_list)) {
                    return (
                        Response::ok(&req.id, json!({ "satisfied": true, "tasks": tasks })),
                        ConnEffect::default(),
                    );
                }
            }
        }

        tokio::select! {
            biased;
            _ = tokio::time::sleep_until(deadline) => {
                return (
                    Response::err(&req.id, code::TIMEOUT, "wait timed out"),
                    ConnEffect::default(),
                );
            }
            _ = tick.tick() => {}
            event = rx.recv() => {
                if matches!(event, Err(broadcast::error::RecvError::Closed)) {
                    return (
                        Response::err(&req.id, code::INTERNAL, "event bus closed"),
                        ConnEffect::default(),
                    );
                }
            }
        }
    }
}

pub fn h_task_report(ctx: &Ctx, req: &Request, params: &Value) -> (Response, ConnEffect) {
    let p: params::TaskReport = match decode(params) {
        Ok(p) => p,
        Err((ref c, ref m)) => return (Response::err(&req.id, c, m), ConnEffect::default()),
    };

    let target_task_id = if let Some(ref tid) = p.task_id {
        tid.clone()
    } else if let Some(ref pid) = p.pane_id {
        let s = ctx.store.read().unwrap();
        match s.task_for_pane(pid) {
            Some(t) => t.id.clone(),
            None => {
                return (
                    Response::err(
                        &req.id,
                        code::NO_SUCH_TASK,
                        format!("no such task for pane '{pid}'"),
                    ),
                    ConnEffect::default(),
                );
            }
        }
    } else {
        return (
            Response::err(&req.id, code::BAD_PARAMS, "must provide task_id or pane_id"),
            ConnEffect::default(),
        );
    };

    let result = TaskResult {
        status: p.status,
        summary: p.summary,
        artifacts: p.artifacts.unwrap_or_default(),
        evidence: p.evidence,
        reported_at: Utc::now(),
    };

    let mut s = ctx.store.write().unwrap();
    match s.task_report(&target_task_id, result) {
        Ok(_) => {
            ctx.mark_persist();
            let task = s.tasks.get(&target_task_id).unwrap();
            (
                Response::ok(&req.id, json!({ "task": task })),
                ConnEffect::default(),
            )
        }
        Err((c, m)) => (Response::err(&req.id, &c, m), ConnEffect::default()),
    }
}

pub fn h_attention_pending(ctx: &Ctx, req: &Request, params: &Value) -> (Response, ConnEffect) {
    let p: params::AttentionPending = match decode(params) {
        Ok(p) => p,
        Err((ref c, ref m)) => return (Response::err(&req.id, c, m), ConnEffect::default()),
    };

    let limit = p.limit.unwrap_or(50).clamp(1, 500);
    let s = ctx.store.read().unwrap();
    let mut panes: Vec<&signaltty_core::model::Pane> = s
        .panes
        .values()
        .filter(|pane| pane.attention != Attention::None)
        .collect();

    panes.sort_by(|a, b| {
        b.attention
            .severity()
            .cmp(&a.attention.severity())
            .then(b.last_activity_at.cmp(&a.last_activity_at))
    });

    let items: Vec<Value> = panes
        .into_iter()
        .take(limit)
        .map(|p| {
            let mut obj = json!({
                "pane_id": p.id,
                "workspace_id": p.workspace_id,
                "tab_id": p.tab_id,
                "lifecycle": p.lifecycle,
                "attention": p.attention,
            });
            if let Some(ref l) = p.label {
                obj["label"] = json!(l);
            }
            if let Some(ref tid) = p.task_id {
                obj["task_id"] = json!(tid);
            }
            if let Some(ref msg) = p.last_message {
                obj["last_message"] = json!(msg);
            }
            if let Some(since) = p.attention_since {
                obj["attention_since"] = json!(since);
            }
            obj
        })
        .collect();

    (
        Response::ok(&req.id, json!({ "panes": items })),
        ConnEffect::default(),
    )
}

pub async fn h_task_diff(ctx: &Ctx, req: &Request, params: &Value) -> (Response, ConnEffect) {
    let p: params::TaskDiff = match decode(params) {
        Ok(p) => p,
        Err((ref c, ref m)) => return (Response::err(&req.id, c, m), ConnEffect::default()),
    };
    let (worktree_path, base_sha) = {
        let s = ctx.store.read().unwrap();
        let task = match s.tasks.get(&p.task_id) {
            Some(t) => t,
            None => {
                return (
                    Response::err(
                        &req.id,
                        code::NO_SUCH_TASK,
                        format!("no such task '{}'", p.task_id),
                    ),
                    ConnEffect::default(),
                );
            }
        };
        let wt = task.worktree_path.to_string_lossy().to_string();
        (wt, task.base_sha.clone())
    };
    let base_for_diff = base_sha.clone();
    let diff_result = tokio::task::spawn_blocking(move || {
        crate::git::task_git_diff(&worktree_path, &base_for_diff)
    })
    .await;
    let diff_result = match diff_result {
        Ok(result) => result,
        Err(e) => {
            return (
                Response::err(
                    &req.id,
                    code::IO_ERROR,
                    format!("task diff task failed: {e}"),
                ),
                ConnEffect::default(),
            );
        }
    };
    match diff_result {
        Ok(diff) => (
            Response::ok(
                &req.id,
                json!({
                    "task_id": p.task_id,
                    "base_sha": base_sha,
                    "branch": diff.branch,
                    "files": diff.files,
                    "dirs": diff.dirs,
                    "added": diff.added,
                    "removed": diff.removed,
                }),
            ),
            ConnEffect::default(),
        ),
        Err((c, m)) => (Response::err(&req.id, &c, m), ConnEffect::default()),
    }
}

pub async fn h_task_file_diff(ctx: &Ctx, req: &Request, params: &Value) -> (Response, ConnEffect) {
    let p: params::TaskFileDiff = match decode(params) {
        Ok(p) => p,
        Err((ref c, ref m)) => return (Response::err(&req.id, c, m), ConnEffect::default()),
    };
    let (worktree_path, base_sha) = {
        let s = ctx.store.read().unwrap();
        let task = match s.tasks.get(&p.task_id) {
            Some(t) => t,
            None => {
                return (
                    Response::err(
                        &req.id,
                        code::NO_SUCH_TASK,
                        format!("no such task '{}'", p.task_id),
                    ),
                    ConnEffect::default(),
                );
            }
        };
        let wt = task.worktree_path.to_string_lossy().to_string();
        (wt, task.base_sha.clone())
    };
    match crate::file_diff::read_with_base(&worktree_path, &p.path, Some(&base_sha)).await {
        Ok(diff) => (
            Response::ok(
                &req.id,
                json!({
                    "task_id": p.task_id,
                    "path": diff.path,
                    "untracked": diff.untracked,
                    "content": diff.content,
                }),
            ),
            ConnEffect::default(),
        ),
        Err((c, m)) => (Response::err(&req.id, &c, m), ConnEffect::default()),
    }
}

pub async fn h_task_finish(ctx: &Ctx, req: &Request, params: &Value) -> (Response, ConnEffect) {
    let p: params::TaskFinish = match decode(params) {
        Ok(p) => p,
        Err((ref c, ref m)) => return (Response::err(&req.id, c, m), ConnEffect::default()),
    };

    let mode = p.mode.as_str();
    if mode != "merge" && mode != "discard" {
        return (
            Response::err(
                &req.id,
                code::BAD_PARAMS,
                format!("unsupported finish mode '{mode}', must be 'merge' or 'discard'"),
            ),
            ConnEffect::default(),
        );
    }

    // Per-path lock across the git section. The disposition re-check below
    // runs while it is held, so two finishes cannot both merge or delete.
    let worktree_for_lock = {
        let s = ctx.store.read().unwrap();
        match s.tasks.get(&p.task_id) {
            Some(t) => t.worktree_path.clone(),
            None => {
                return (
                    Response::err(
                        &req.id,
                        code::NO_SUCH_TASK,
                        format!("no such task '{}'", p.task_id),
                    ),
                    ConnEffect::default(),
                );
            }
        }
    };
    let path_lock = ctx.worktrees.path_lock(&worktree_for_lock);
    let _finish_lock = path_lock.lock().await;

    // Step 1: Re-read under the path lock and validate preliminary rules.
    let (task_snapshot, pane_id, worktree_path, source_repo, branch, preexisting_branch) = {
        let s = ctx.store.read().unwrap();
        let task = match s.tasks.get(&p.task_id) {
            Some(t) => t,
            None => {
                return (
                    Response::err(
                        &req.id,
                        code::NO_SUCH_TASK,
                        format!("no such task '{}'", p.task_id),
                    ),
                    ConnEffect::default(),
                );
            }
        };

        if task.disposition.outcome != DispositionOutcome::None {
            if !task.worktree_path.exists() {
                return (
                    Response::err(
                        &req.id,
                        code::BAD_PARAMS,
                        format!(
                            "task '{}' already has a disposition ({:?})",
                            p.task_id, task.disposition.outcome
                        ),
                    ),
                    ConnEffect::default(),
                );
            }
            let source_repo = task.source_repo.clone();
            let worktree_path = task.worktree_path.clone();
            let outcome = task.disposition.outcome;
            let target_ref = task.disposition.target_ref.clone();
            let merged_sha = task.disposition.merged_sha.clone();
            let pane_id = task.pane_id.clone();
            drop(s);
            return retry_recorded_cleanup(
                ctx,
                &req.id,
                &p.task_id,
                &source_repo,
                &worktree_path,
                pane_id.as_deref(),
                outcome,
                target_ref,
                merged_sha,
            );
        }

        if mode == "merge" && task.state != TaskState::Completed {
            return (
                Response::err_with_details(
                    &req.id,
                    code::BAD_PARAMS,
                    format!(
                        "task must be completed to merge, currently {}",
                        task.state.as_str()
                    ),
                    json!({ "state": task.state.as_str() }),
                ),
                ConnEffect::default(),
            );
        }

        // Check foreign live panes inside the worktree
        let wt_str = task.worktree_path.to_string_lossy().to_string();
        for (pid, pane) in &s.panes {
            if Some(pid) != task.pane_id.as_ref()
                && matches!(pane.live, LiveState::Live)
                && cwd_inside_worktree(&pane.cwd, &task.worktree_path)
            {
                return (
                    Response::err(
                        &req.id,
                        code::PANES_ALIVE,
                        format!("foreign live pane '{pid}' is inside task worktree '{wt_str}'"),
                    ),
                    ConnEffect::default(),
                );
            }
        }

        (
            task.clone(),
            task.pane_id.clone(),
            task.worktree_path.clone(),
            task.source_repo.clone(),
            task.branch.clone(),
            task.preexisting_branch,
        )
    };

    let wt_str = worktree_path.to_string_lossy().to_string();
    let src_str = source_repo.to_string_lossy().to_string();

    if mode == "discard" {
        let should_delete_branch = p.delete_branch.unwrap_or(false) && !preexisting_branch;

        // Cancel before closing the pane so pane-exit does not also fail the task.
        {
            let mut s = ctx.store.write().unwrap();
            let non_terminal = s
                .tasks
                .get(&p.task_id)
                .map(|t| !t.state.is_terminal())
                .unwrap_or(false);
            if non_terminal {
                if let Err((c, m)) = s.task_cancel(&p.task_id) {
                    return (Response::err(&req.id, &c, m), ConnEffect::default());
                }
            }
        }

        // Close worker pane if live
        if let Some(ref pid) = pane_id {
            ctx.ptys.destroy(pid, Some("TERM"));
            let mut s = ctx.store.write().unwrap();
            s.set_exited(pid, Some(0));
        }

        // Remove worktree (herdr-guarded: see remove_task_worktree).
        let mut cleanup_error = remove_task_worktree(&src_str, &worktree_path);

        // Delete branch if requested and not pre-existing
        if should_delete_branch {
            let b_out = crate::git::git_output(&src_str, &["branch", "-D", "--", &branch]);
            if let Ok(ref out) = b_out {
                if !out.status.success() {
                    cleanup_error = Some(format!(
                        "branch deletion failed: {}",
                        String::from_utf8_lossy(&out.stderr).trim()
                    ));
                }
            } else if let Err(ref e) = b_out {
                cleanup_error = Some(format!("branch deletion error: {e}"));
            }
        }

        let finish_error = cleanup_error
            .as_ref()
            .map(|e| json!({ "cleanup_error": e }));
        let task_clone = {
            let mut s = ctx.store.write().unwrap();
            let disposition = Disposition {
                outcome: DispositionOutcome::Discarded,
                target_ref: None,
                merged_sha: None,
                branch_deleted: Some(should_delete_branch),
                at: Some(Utc::now()),
            };
            if let Err((c, m)) = s.task_finish_record(&p.task_id, disposition, finish_error) {
                return (Response::err(&req.id, &c, m), ConnEffect::default());
            }
            s.tasks.get(&p.task_id).unwrap().clone()
        };

        ctx.mark_persist();
        let mut res = json!({ "task": task_clone });
        if let Some(err) = cleanup_error {
            res["cleanup_error"] = json!(err);
        }
        return (Response::ok(&req.id, res), ConnEffect::default());
    }

    // mode == "merge"
    // Target resolution
    let target = match p
        .target_ref
        .as_deref()
        .or(task_snapshot.target_branch.as_deref())
    {
        Some(t) if !t.trim().is_empty() => t.trim().to_string(),
        _ => {
            return (
                Response::err(
                    &req.id,
                    code::BAD_PARAMS,
                    "task started with detached HEAD; explicit target_ref required",
                ),
                ConnEffect::default(),
            );
        }
    };

    // Verify checked-out branch in source repo
    let cur_branch = match crate::git::git_output(&src_str, &["branch", "--show-current"]) {
        Ok(out) => String::from_utf8_lossy(&out.stdout).trim().to_string(),
        Err(e) => {
            return (
                Response::err(&req.id, code::IO_ERROR, format!("git branch failed: {e}")),
                ConnEffect::default(),
            );
        }
    };

    if cur_branch != target {
        return (
            Response::err_with_details(
                &req.id,
                code::BAD_PARAMS,
                format!(
                    "source repo checked out branch '{cur_branch}' does not match target branch '{target}'"
                ),
                json!({ "expected": target, "actual": cur_branch }),
            ),
            ConnEffect::default(),
        );
    }

    // Check source worktree dirt. A failed check is IO_ERROR, never
    // "clean": merging or deleting on an unverified tree could destroy
    // uncommitted work.
    if !p.ignore_dirty.unwrap_or(false) {
        match crate::git::porcelain_clean(&wt_str, &["status", "--porcelain"]) {
            Ok(true) => {}
            Ok(false) => {
                return (
                    Response::err(
                        &req.id,
                        code::BAD_PARAMS,
                        "source worktree is dirty (uncommitted/staged/untracked changes would be lost)",
                    ),
                    ConnEffect::default(),
                );
            }
            Err(e) => {
                return (
                    Response::err(
                        &req.id,
                        code::IO_ERROR,
                        format!("source dirt check failed: {e}"),
                    ),
                    ConnEffect::default(),
                );
            }
        }
    }

    // Check target checkout dirt (tracked modifications)
    match crate::git::porcelain_clean(
        &src_str,
        &["status", "--porcelain=v1", "--untracked-files=no"],
    ) {
        Ok(true) => {}
        Ok(false) => {
            return (
                Response::err(
                    &req.id,
                    code::BAD_PARAMS,
                    "target checkout is dirty (tracked modifications present)",
                ),
                ConnEffect::default(),
            );
        }
        Err(e) => {
            return (
                Response::err(
                    &req.id,
                    code::IO_ERROR,
                    format!("target dirt check failed: {e}"),
                ),
                ConnEffect::default(),
            );
        }
    }

    // Resolve the task branch tip before merging so we can prove the
    // new HEAD descends from it afterwards (a merge that resolves to the
    // checked-out branch reports success without landing task commits).
    let branch_tip = match crate::git::git_output(
        &src_str,
        &["rev-parse", "--verify", &format!("refs/heads/{branch}")],
    ) {
        Ok(out) if out.status.success() => String::from_utf8_lossy(&out.stdout).trim().to_string(),
        _ => {
            return (
                Response::err(
                    &req.id,
                    code::IO_ERROR,
                    format!("task branch '{branch}' is missing in the source repo"),
                ),
                ConnEffect::default(),
            );
        }
    };

    // Execute merge under a deadline in its own process group: a stalled
    // hook or signing helper must not hold this handler (and its path lock)
    // forever. On expiry the group is killed, then the merge is aborted.
    let merge_out = match crate::worktrees::git_status_with_timeout(
        &src_str,
        &["merge", "--no-ff", "--no-edit", "--", &branch],
        Duration::from_millis(ctx.config.merge_timeout_ms),
    )
    .await
    {
        Ok(out) => out,
        Err((c, _)) if c == code::TIMEOUT => {
            // Killed before MERGE_HEAD existed, `merge --abort` has nothing to
            // abort but the index may already hold the merge; the target was
            // verified clean above, so `reset --merge` restores it exactly.
            let ok = |args: &[&str]| {
                crate::git::git_output(&src_str, args)
                    .map(|out| out.status.success())
                    .unwrap_or(false)
            };
            let abort_ok = ok(&["merge", "--abort"]) || ok(&["reset", "--merge"]);
            let target_clean = crate::git::porcelain_clean(
                &src_str,
                &["status", "--porcelain=v1", "--untracked-files=no"],
            );
            return (
                Response::err_with_details(
                    &req.id,
                    code::TIMEOUT,
                    format!(
                        "git merge timed out after {} ms",
                        ctx.config.merge_timeout_ms
                    ),
                    json!({
                        "abort_ok": abort_ok,
                        "target_dirty": !matches!(target_clean, Ok(true)),
                    }),
                ),
                ConnEffect::default(),
            );
        }
        Err((c, m)) => {
            return (
                Response::err(&req.id, &c, format!("git merge failed: {m}")),
                ConnEffect::default(),
            );
        }
    };

    if !merge_out.status.success() {
        // Collect conflicted files
        let conflicted_files: Vec<String> = if let Ok(diff_out) =
            crate::git::git_output(&src_str, &["diff", "--name-only", "--diff-filter=U"])
        {
            String::from_utf8_lossy(&diff_out.stdout)
                .lines()
                .map(|l| l.trim().to_string())
                .filter(|l| !l.is_empty())
                .collect()
        } else {
            Vec::new()
        };

        // Abort merge. A failed abort (or a target that is still dirty
        // afterwards) must not be reported as a clean conflict: the target
        // may be mid-merge, so this is IO_ERROR with the dirt flagged.
        let abort_ok = crate::git::git_output(&src_str, &["merge", "--abort"])
            .map(|out| out.status.success())
            .unwrap_or(false);
        let target_clean = crate::git::porcelain_clean(
            &src_str,
            &["status", "--porcelain=v1", "--untracked-files=no"],
        );
        if !abort_ok || !matches!(target_clean, Ok(true)) {
            let target_dirty = !matches!(target_clean, Ok(true));
            let details = json!({
                "conflicted": conflicted_files,
                "target_dirty": target_dirty,
                "abort_ok": abort_ok,
            });
            {
                let mut s = ctx.store.write().unwrap();
                let _ = s.task_set_finish_error(&p.task_id, Some(details.clone()));
            }
            ctx.mark_persist();
            return (
                Response::err_with_details(
                    &req.id,
                    code::IO_ERROR,
                    "merge conflict abort failed or left the target dirty; target may be mid-merge",
                    details,
                ),
                ConnEffect::default(),
            );
        }

        let details = json!({ "conflicted": conflicted_files });
        {
            let mut s = ctx.store.write().unwrap();
            let _ = s.task_set_finish_error(&p.task_id, Some(details.clone()));
        }
        ctx.mark_persist();
        return (
            Response::err_with_details(
                &req.id,
                code::MERGE_CONFLICT,
                "merge conflict encountered during task.finish",
                details,
            ),
            ConnEffect::default(),
        );
    }

    // Get merge commit sha
    let head_sha = match crate::git::git_output(&src_str, &["rev-parse", "HEAD"]) {
        Ok(out) => String::from_utf8_lossy(&out.stdout).trim().to_string(),
        Err(_) => String::new(),
    };

    // The merge must have landed the task commits: the new HEAD has to
    // descend from the task branch tip. Otherwise (e.g. the branch resolved
    // to the already-checked-out target) report failure and record nothing.
    if !crate::git::head_contains_branch_tip(&src_str, &branch_tip) {
        return (
            Response::err_with_details(
                &req.id,
                code::IO_ERROR,
                format!(
                    "merge of '{branch}' did not advance '{target}' to include the task commits"
                ),
                json!({ "branch": branch, "tip": branch_tip, "head": head_sha }),
            ),
            ConnEffect::default(),
        );
    }

    // Close worker pane
    if let Some(ref pid) = pane_id {
        ctx.ptys.destroy(pid, Some("TERM"));
        let mut s = ctx.store.write().unwrap();
        s.set_exited(pid, Some(0));
    }

    // Clean up worktree (herdr-guarded: see remove_task_worktree).
    let mut cleanup_error = remove_task_worktree(&src_str, &worktree_path);

    // Delete branch if requested and not pre-existing
    let should_delete_branch = p.delete_branch.unwrap_or(false) && !preexisting_branch;
    if should_delete_branch {
        let b_out = crate::git::git_output(&src_str, &["branch", "-D", "--", &branch]);
        if let Ok(ref out) = b_out {
            if !out.status.success() {
                cleanup_error = Some(format!(
                    "branch deletion failed: {}",
                    String::from_utf8_lossy(&out.stderr).trim()
                ));
            }
        } else if let Err(ref e) = b_out {
            cleanup_error = Some(format!("branch deletion error: {e}"));
        }
    }

    let finish_error = cleanup_error
        .as_ref()
        .map(|e| json!({ "cleanup_error": e }));
    let task_clone = {
        let mut s = ctx.store.write().unwrap();
        let disposition = Disposition {
            outcome: DispositionOutcome::Merged,
            target_ref: Some(target.clone()),
            merged_sha: Some(head_sha.clone()),
            branch_deleted: Some(should_delete_branch),
            at: Some(Utc::now()),
        };
        if let Err((c, m)) = s.task_finish_record(&p.task_id, disposition, finish_error) {
            return (Response::err(&req.id, &c, m), ConnEffect::default());
        }
        match s.tasks.get(&p.task_id) {
            Some(task) => task.clone(),
            None => {
                return (
                    Response::err(
                        &req.id,
                        code::NO_SUCH_TASK,
                        format!("no such task '{}'", p.task_id),
                    ),
                    ConnEffect::default(),
                );
            }
        }
    };
    ctx.mark_persist();

    let mut res = json!({
        "task": task_clone,
        "merge": {
            "target": target,
            "sha": head_sha,
        }
    });
    if let Some(err) = cleanup_error {
        res["cleanup_error"] = json!(err);
    }

    (Response::ok(&req.id, res), ConnEffect::default())
}

/// True when `cwd` is the worktree or a directory inside it.
///
/// Both sides are canonicalized, then compared on whole path components.
/// A string prefix would treat `/data/proj-other` as inside `/data/proj`
/// and would miss the same directory reached through a symlink.
fn cwd_inside_worktree(cwd: &str, worktree: &std::path::Path) -> bool {
    let Ok(worktree) = worktree.canonicalize() else {
        return false;
    };
    std::path::Path::new(cwd)
        .canonicalize()
        .is_ok_and(|cwd| cwd.starts_with(worktree))
}

/// Second finish when a disposition is already stored but the recorded
/// checkout is still on disk: retry removal only, do not merge again.
#[allow(clippy::too_many_arguments)]
fn retry_recorded_cleanup(
    ctx: &Ctx,
    req_id: &str,
    task_id: &str,
    source_repo: &std::path::Path,
    worktree_path: &std::path::Path,
    worker_pane_id: Option<&str>,
    outcome: DispositionOutcome,
    target_ref: Option<String>,
    merged_sha: Option<String>,
) -> (Response, ConnEffect) {
    let wt_str = worktree_path.to_string_lossy().to_string();
    {
        let s = ctx.store.read().unwrap();
        for (pid, pane) in &s.panes {
            if Some(pid.as_str()) != worker_pane_id
                && matches!(pane.live, LiveState::Live)
                && cwd_inside_worktree(&pane.cwd, worktree_path)
            {
                return (
                    Response::err(
                        req_id,
                        code::PANES_ALIVE,
                        format!("foreign live pane '{pid}' is inside task worktree '{wt_str}'"),
                    ),
                    ConnEffect::default(),
                );
            }
        }
    }

    let src_str = source_repo.to_string_lossy().to_string();
    let cleanup_error = remove_task_worktree(&src_str, worktree_path);
    let finish_error = cleanup_error
        .as_ref()
        .map(|e| json!({ "cleanup_error": e }));
    let task_clone = {
        let mut s = ctx.store.write().unwrap();
        if let Err((c, m)) = s.task_set_finish_error(task_id, finish_error) {
            return (Response::err(req_id, &c, m), ConnEffect::default());
        }
        match s.tasks.get(task_id) {
            Some(task) => task.clone(),
            None => {
                return (
                    Response::err(
                        req_id,
                        code::NO_SUCH_TASK,
                        format!("no such task '{task_id}'"),
                    ),
                    ConnEffect::default(),
                );
            }
        }
    };
    ctx.mark_persist();

    let mut res = json!({ "task": task_clone });
    if outcome == DispositionOutcome::Merged {
        res["merge"] = json!({ "target": target_ref, "sha": merged_sha });
    }
    if let Some(err) = cleanup_error {
        res["cleanup_error"] = json!(err);
    }
    (Response::ok(req_id, res), ConnEffect::default())
}
