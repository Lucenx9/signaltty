//! Task orchestration: task lifecycle, worktree checkout, agent worker pane spawn,
//! and background prompt delivery.

use std::collections::HashMap;
use std::path::PathBuf;
use std::time::Duration;

use chrono::Utc;
use serde_json::{json, Value};
use signaltty_core::model::{AgentKind, Disposition, LiveState, Relationship, Task, TaskResult};
use signaltty_core::state::{Attention, Lifecycle, TaskState};
use signaltty_core::{new_context_id, new_task_id};
use signaltty_proto::{code, Request, Response};
use tokio::sync::broadcast;

use crate::params::{self, decode};
use crate::router::{ConnEffect, Ctx};
use crate::store::Store;
use crate::submit::SubmitCtx;

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
    let preexisting_branch = crate::git::branch_exists(&p.repo, &branch);
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
                    return (Response::err(&req.id, c, m), ConnEffect::default())
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
                return (
                    Response::err(&req.id, code::NO_SUCH_WORKSPACE, ws_id),
                    ConnEffect::default(),
                )
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
                    return (Response::err(&req.id, c, m), ConnEffect::default())
                }
            };
            val["tab"]["id"].as_str().unwrap().to_string()
        }
    };

    let (parent_pane_id, root_pane_id) = {
        let s = ctx.store.read().unwrap();
        match &p.parent_pane_id {
            Some(parent_id) => {
                let root = s.panes.get(parent_id).and_then(|parent| {
                    parent
                        .root_pane_id
                        .clone()
                        .or_else(|| Some(parent.id.clone()))
                });
                (Some(parent_id.clone()), root)
            }
            None => (None, None),
        }
    };

    let mut env = HashMap::new();
    if let Some(parent) = &parent_pane_id {
        env.insert("SIGNALTTY_PARENT_PANE".to_string(), parent.clone());
    }
    env.insert("SIGNALTTY_TASK".to_string(), task_id.clone());

    let relationship = p.relationship.unwrap_or(Relationship::Subagent);

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
            "parent_pane_id": parent_pane_id,
            "label": p.label,
            "relationship": relationship,
            "task_id": task_id,
        }),
    ) {
        Ok(res) => res,
        Err((ref c, ref m)) => return (Response::err(&req.id, c, m), ConnEffect::default()),
    };

    let pane_id = pane_val["pane"]["id"].as_str().unwrap().to_string();
    let now = Utc::now();
    let repo_buf = PathBuf::from(&p.repo);
    let label = p
        .label
        .unwrap_or_else(|| format!("worker-{}", &task_id[task_id.len().saturating_sub(8)..]));

    let task = Task {
        id: task_id.clone(),
        context_id,
        parent_task_id: None,
        pane_id: Some(pane_id.clone()),
        parent_pane_id,
        root_pane_id,
        relationship,
        label,
        contract: p.contract,
        agent: agent_name,
        source_repo: repo_buf,
        target_branch,
        worktree_path,
        branch,
        preexisting_branch,
        base_ref,
        base_sha,
        state: TaskState::Pending,
        result: None,
        disposition: Disposition::default(),
        status_reason: None,
        finish_error: None,
        worker_pid: None,
        worker_cmd: Some(argv),
        created_at: now,
        updated_at: now,
    };

    reservation.commit(task.clone());
    ctx.mark_persist();

    // 5. Background ready check and prompt submit
    let prompt = task.compose_worker_prompt();
    spawn_background_submit(
        ctx,
        task_id.clone(),
        pane_id.clone(),
        prompt,
        p.ready_timeout_s.unwrap_or(30),
        p.stall_timeout_s.unwrap_or(5),
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

pub(crate) fn spawn_background_submit(
    ctx: &Ctx,
    bg_task_id: String,
    bg_pane_id: String,
    prompt: String,
    ready_timeout_s: u64,
    stall_timeout_s: u64,
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
            Duration::from_millis(100),
            stall_timeout,
            true,
            false,
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
        Some(Value::String(s)) => vec![s],
        Some(Value::Array(arr)) => arr
            .into_iter()
            .filter_map(|v| v.as_str().map(|s| s.to_string()))
            .collect(),
        _ => vec!["settled".to_string()],
    };

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
