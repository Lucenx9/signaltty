//! IPC envelopes for `signaltty/1`: JSONL over a Unix socket.
//! See docs/08.

use serde::{Deserialize, Serialize};
use serde_json::Value;

pub const PROTOCOL: &str = "signaltty/1";
pub const PROTOCOL_MAJOR: u32 = 1;
/// Max JSONL line accepted from a client (PTY bursts are chunked).
pub const MAX_LINE_BYTES: usize = 16 * 1024 * 1024;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Request {
    pub protocol: String,
    pub id: String,
    pub method: String,
    #[serde(default)]
    pub params: Value,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ErrorBody {
    pub code: String,
    pub message: String,
    #[serde(default, skip_serializing_if = "Value::is_null")]
    pub details: Value,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Response {
    pub protocol: String,
    pub id: String,
    pub ok: bool,
    #[serde(default, skip_serializing_if = "Value::is_null")]
    pub result: Value,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub error: Option<ErrorBody>,
}

impl Response {
    pub fn ok(id: &str, result: Value) -> Response {
        Response {
            protocol: PROTOCOL.to_string(),
            id: id.to_string(),
            ok: true,
            result,
            error: None,
        }
    }

    pub fn err(id: &str, code: &str, message: impl Into<String>) -> Response {
        Response {
            protocol: PROTOCOL.to_string(),
            id: id.to_string(),
            ok: false,
            result: Value::Null,
            error: Some(ErrorBody {
                code: code.to_string(),
                message: message.into(),
                details: Value::Null,
            }),
        }
    }

    pub fn to_line(&self) -> String {
        let mut s = serde_json::to_string(self).unwrap_or_else(|_| {
            r#"{"protocol":"signaltty/1","id":"?","ok":false,"error":{"code":"INTERNAL","message":"encode failed"}}"#.to_string()
        });
        s.push('\n');
        s
    }
}

/// Server → client event. `seq` is a monotonic server counter.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EventMsg {
    pub protocol: String,
    #[serde(rename = "type")]
    pub kind: String,
    pub event: String,
    pub seq: u64,
    pub payload: Value,
}

impl EventMsg {
    pub fn new(event: &str, seq: u64, payload: Value) -> EventMsg {
        EventMsg {
            protocol: PROTOCOL.to_string(),
            kind: "event".to_string(),
            event: event.to_string(),
            seq,
            payload,
        }
    }

    pub fn to_line(&self) -> String {
        let mut s = serde_json::to_string(self).unwrap_or_default();
        s.push('\n');
        s
    }
}

// Method names (docs/08).
pub mod method {
    pub const SERVER_STATUS: &str = "server.status";
    pub const SERVER_SHUTDOWN: &str = "server.shutdown";
    pub const WORKSPACE_CREATE: &str = "workspace.create";
    pub const WORKSPACE_LIST: &str = "workspace.list";
    pub const WORKSPACE_GET: &str = "workspace.get";
    pub const WORKSPACE_RENAME: &str = "workspace.rename";
    pub const WORKSPACE_CLOSE: &str = "workspace.close";
    pub const WORKSPACE_REFRESH_GIT: &str = "workspace.refresh_git";
    pub const TAB_CREATE: &str = "tab.create";
    pub const TAB_CLOSE: &str = "tab.close";
    pub const TAB_SET_LAYOUT: &str = "tab.set_layout";
    pub const PANE_SPAWN: &str = "pane.spawn";
    pub const PANE_SPLIT: &str = "pane.split";
    pub const PANE_GET: &str = "pane.get";
    pub const PANE_INPUT: &str = "pane.input";
    pub const PANE_RESIZE: &str = "pane.resize";
    pub const PANE_SIGNAL: &str = "pane.signal";
    pub const PANE_READ: &str = "pane.read";
    pub const PANE_ATTACH: &str = "pane.attach";
    pub const PANE_DETACH: &str = "pane.detach";
    pub const PANE_CLOSE: &str = "pane.close";
    pub const PANE_RESUME: &str = "pane.resume";
    pub const PANE_MARK_SEEN: &str = "pane.mark_seen";
    pub const NOTIFY: &str = "notify";
    pub const HOOK_EVENT: &str = "hook-event";
    pub const REPORT_SESSION: &str = "report-session";
    pub const SUBSCRIBE: &str = "subscribe";
    pub const WAIT: &str = "wait";
    pub const FOCUS_NEXT_UNREAD: &str = "focus.next_unread";
    pub const PLUGIN_LIST: &str = "plugin.list";
    pub const PLUGIN_RELOAD: &str = "plugin.reload";
}

// Event names (docs/08).
pub mod event {
    pub const WORKSPACE_CREATED: &str = "workspace.created";
    pub const WORKSPACE_UPDATED: &str = "workspace.updated";
    pub const WORKSPACE_CLOSED: &str = "workspace.closed";
    pub const TAB_CREATED: &str = "tab.created";
    pub const TAB_UPDATED: &str = "tab.updated";
    pub const TAB_CLOSED: &str = "tab.closed";
    pub const PANE_CREATED: &str = "pane.created";
    pub const PANE_UPDATED: &str = "pane.updated";
    pub const PANE_EXITED: &str = "pane.exited";
    pub const PANE_CLOSED: &str = "pane.closed";
    pub const PANE_RESIZED: &str = "pane.resized";
    pub const PTY_DATA: &str = "pty.data";
    pub const AGENT_WORKING: &str = "agent.working";
    pub const AGENT_BLOCKED: &str = "agent.blocked";
    pub const AGENT_DONE: &str = "agent.done";
    pub const AGENT_FAILED: &str = "agent.failed";
    pub const AGENT_IDLE: &str = "agent.idle";
    pub const AGENT_UNKNOWN: &str = "agent.unknown";
    pub const AGENT_EXITED: &str = "agent.exited";
    pub const ATTENTION_CREATED: &str = "attention.created";
    pub const ATTENTION_UPDATED: &str = "attention.updated";
    pub const ATTENTION_CLEARED: &str = "attention.cleared";
    pub const NOTIFICATION_CREATED: &str = "notification.created";
    pub const GIT_BRANCH_CHANGED: &str = "git.branch_changed";
    pub const SERVER_WILL_SHUTDOWN: &str = "server.will_shutdown";
}

// Error codes (docs/08).
pub mod code {
    pub const BAD_PROTOCOL: &str = "BAD_PROTOCOL";
    pub const UNKNOWN_METHOD: &str = "UNKNOWN_METHOD";
    pub const BAD_PARAMS: &str = "BAD_PARAMS";
    pub const NO_SUCH_WORKSPACE: &str = "NO_SUCH_WORKSPACE";
    pub const NO_SUCH_TAB: &str = "NO_SUCH_TAB";
    pub const NO_SUCH_PANE: &str = "NO_SUCH_PANE";
    pub const PANE_EXITED: &str = "PANE_EXITED";
    pub const PANES_ALIVE: &str = "PANES_ALIVE";
    pub const SPAWN_FAILED: &str = "SPAWN_FAILED";
    pub const IO_ERROR: &str = "IO_ERROR";
    pub const TIMEOUT: &str = "TIMEOUT";
    pub const RATE_LIMITED: &str = "RATE_LIMITED";
    pub const FORBIDDEN: &str = "FORBIDDEN";
    pub const INTERNAL: &str = "INTERNAL";
}

/// Subscription glob: `*`, `prefix.*`, or exact name.
pub fn glob_matches(glob: &str, event: &str) -> bool {
    if glob == "*" {
        return true;
    }
    if let Some(prefix) = glob.strip_suffix(".*") {
        return event == prefix || event.starts_with(&format!("{prefix}."));
    }
    glob == event
}

pub fn check_protocol(req: &Request) -> bool {
    req.protocol == PROTOCOL
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn glob_matching() {
        assert!(glob_matches("*", "pane.created"));
        assert!(glob_matches("agent.*", "agent.done"));
        assert!(!glob_matches("agent.*", "pane.created"));
        assert!(glob_matches("pane.created", "pane.created"));
        assert!(!glob_matches("pane.created", "pane.exited"));
    }

    #[test]
    fn response_roundtrip() {
        let r = Response::ok("1", serde_json::json!({"a": 1}));
        let line = r.to_line();
        assert!(line.ends_with('\n'));
        let back: Response = serde_json::from_str(line.trim()).unwrap();
        assert!(back.ok);
        let e = Response::err("2", code::BAD_PARAMS, "nope");
        assert!(!e.ok);
        assert_eq!(e.error.unwrap().code, "BAD_PARAMS");
    }

    #[test]
    fn protocol_check() {
        let r = Request {
            protocol: "signaltty/1".into(),
            id: "1".into(),
            method: "server.status".into(),
            params: Value::Null,
        };
        assert!(check_protocol(&r));
    }
}
