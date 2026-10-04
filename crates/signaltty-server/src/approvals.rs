//! Live native hook routes. Store is always locked before this registry.
use std::collections::HashMap;
use std::sync::Mutex;
use std::time::Duration;

use serde_json::{json, Value};
use signaltty_core::model::{AgentKind, LiveState};
use signaltty_proto::{code, Response};
use tokio::sync::{broadcast, oneshot};

use crate::params::{self, bad_params, decode};
use crate::router::{ConnEffect, Ctx};

const PREFIX: &str = "permission_";
#[derive(Default)]
pub struct Approvals {
    routes: Mutex<HashMap<String, Route>>,
}
struct Route {
    pane: String,
    session: String,
    agent: AgentKind,
    deadline: tokio::time::Instant,
    sender: oneshot::Sender<Value>,
}
impl Approvals {
    pub fn is_native(id: &str) -> bool {
        id.starts_with(PREFIX)
    }
    pub fn contains(&self, id: &str) -> bool {
        self.routes
            .lock()
            .unwrap()
            .get(id)
            .is_some_and(|route| !route.sender.is_closed())
    }
}

struct Waiting<'a> {
    ctx: &'a Ctx,
    pane: String,
    id: String,
}
impl Drop for Waiting<'_> {
    fn drop(&mut self) {
        let mut store = self.ctx.store.write().unwrap();
        self.ctx.approvals.routes.lock().unwrap().remove(&self.id);
        let current = store
            .panes
            .get(&self.pane)
            .and_then(|p| p.pending_decision.as_ref())
            .is_some_and(|d| d.id == self.id);
        let event = if current {
            store.clear_decision(&self.pane, "native_cancelled")
        } else {
            None
        };
        drop(store);
        if event.is_some() {
            self.ctx.mark_persist();
        }
    }
}

pub fn answer(ctx: &Ctx, pane_id: &str, id: &str, option: &str) -> Result<Value, (String, String)> {
    let verdict = signaltty_agent::permission::permission_output(option)
        .ok_or_else(|| bad_params(format!("unknown native permission option '{option}'")))?;
    let mut store = ctx.store.write().unwrap();
    let pane = store
        .panes
        .get(pane_id)
        .ok_or_else(|| (code::NO_SUCH_PANE.to_owned(), pane_id.to_owned()))?;
    if !matches!(pane.live, LiveState::Live) {
        return Err((code::PANE_EXITED.to_owned(), pane_id.to_owned()));
    }
    if !pane
        .pending_decision
        .as_ref()
        .is_some_and(|d| d.id == id && d.answerable)
    {
        return Err((code::NO_SUCH_DECISION.to_owned(), id.to_owned()));
    }
    let route = ctx
        .approvals
        .routes
        .lock()
        .unwrap()
        .remove(id)
        .filter(|route| {
            !route.sender.is_closed()
                && route.pane == pane_id
                && pane.agent.kind == route.agent
                && pane.agent.agent_session_id.as_deref() == Some(route.session.as_str())
                && tokio::time::Instant::now() < route.deadline
        })
        .ok_or_else(|| (code::NO_SUCH_DECISION.to_owned(), id.to_owned()))?;
    store.answer_decision(pane_id, id, option).unwrap();
    store.mark_seen(pane_id, "decision_answer");
    let pane = &store.panes[pane_id];
    let result = json!({"answered":true,"lifecycle":pane.lifecycle.as_str(),"attention":pane.attention.as_str()});
    let delivered = route.sender.send(verdict).is_ok();
    drop(store);

    ctx.mark_persist();
    if delivered {
        Ok(result)
    } else {
        Err((code::NO_SUCH_DECISION.to_owned(), id.to_owned()))
    }
}

pub async fn wait(ctx: &Ctx, req: &signaltty_proto::Request) -> (Response, ConnEffect) {
    let result = wait_inner(ctx, &req.params).await;
    let response = match result {
        Ok(result) => Response::ok(&req.id, result),
        Err((code, message)) => Response::err(&req.id, &code, message),
    };
    (response, ConnEffect::default())
}

async fn wait_inner(ctx: &Ctx, value: &Value) -> Result<Value, (String, String)> {
    let p: params::HookEvent = decode(value)?;
    if p.hook != "PermissionRequest" || p.decision.is_some() {
        return Err(bad_params(
            "native waiting requires PermissionRequest without an explicit decision",
        ));
    }
    let timeout = p.wait_timeout_s.unwrap_or(120);
    if !(1..=120).contains(&timeout) {
        return Err(bad_params("wait_timeout_s must be between 1 and 120"));
    }
    params::parse_severity(&p.severity)?;
    let permission =
        signaltty_agent::permission::native_permission(&p.agent, &p.payload).map_err(bad_params)?;
    let agent = AgentKind::parse(&p.agent).unwrap();
    let pane = p
        .pane_id
        .or_else(|| {
            p.client_pid
                .and_then(|pid| u32::try_from(pid).ok())
                .and_then(|pid| {
                    crate::attrib::resolve_pane_by_ancestry(pid, &ctx.ptys.child_pids())
                })
        })
        .ok_or_else(|| bad_params("native waiting requires a managed pane"))?;
    let id = signaltty_core::new_notif_id().replacen("notif_", PREFIX, 1);
    let mut events = ctx.bcast.subscribe();
    let (sender, mut receiver) = oneshot::channel();
    let expires = tokio::time::Instant::now() + Duration::from_secs(timeout);
    {
        let store = ctx.store.write().unwrap();
        let target = store
            .panes
            .get(&pane)
            .ok_or_else(|| (code::NO_SUCH_PANE.to_owned(), pane.clone()))?;
        if !matches!(target.live, LiveState::Live) {
            return Err((code::PANE_EXITED.to_owned(), pane));
        }
        ctx.approvals.routes.lock().unwrap().insert(
            id.clone(),
            Route {
                pane: pane.clone(),
                session: permission.session_id.clone(),
                agent,
                deadline: expires,
                sender,
            },
        );
    }
    let _waiting = Waiting {
        ctx,
        pane: pane.clone(),
        id: id.clone(),
    };
    let mut ingest = value.clone();
    ingest["wait_for_answer"] = json!(false);
    ingest["pane_id"] = json!(pane);
    ingest["decision"] = json!({"id":id,"prompt":permission.prompt,"options":signaltty_agent::permission::permission_options()});
    crate::router::h_native_hook_event(ctx, &ingest)?;
    let deadline = tokio::time::sleep_until(expires);
    tokio::pin!(deadline);
    loop {
        tokio::select! {
            // A consumed grant wins over the accompanying state-change event.
            biased;
            verdict = &mut receiver => return Ok(json!({"accepted":true,"native_verdict":verdict.ok()})),
            _ = &mut deadline => return Ok(json!({"accepted":true,"native_verdict":Value::Null,"cancelled":"timeout"})),
            _ = ctx.shutdown.notified() => return Ok(json!({"accepted":true,"native_verdict":Value::Null,"cancelled":"shutdown"})),
            event = events.recv() => {
                if matches!(event, Err(broadcast::error::RecvError::Closed)) {
                    return Ok(json!({"accepted":true,"native_verdict":Value::Null,"cancelled":"shutdown"}));
                }
                let store = ctx.store.read().unwrap();
                let still_current = store.panes.get(&pane).is_some_and(|p| {
                    matches!(p.live, LiveState::Live)
                        && p.agent.kind == agent
                        && p.agent.agent_session_id.as_deref() == Some(&permission.session_id)
                        && p.pending_decision.as_ref().is_some_and(|d| d.id == id && d.answerable)
                });
                if !still_current {
                    // The event arm may have won before the grant was queued,
                    // then waited for the answer's Store lock. Recheck the grant
                    // after that lock so consumption cannot masquerade as cancellation.
                    if let Ok(verdict) = receiver.try_recv() {
                        return Ok(json!({"accepted":true,"native_verdict":verdict}));
                    }
                    return Ok(json!({"accepted":true,"native_verdict":Value::Null,"cancelled":"moved_on"}));
                }
            }
        }
    }
}
