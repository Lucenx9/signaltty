//! Typed method params: one decode per method at the dispatch seam.
//!
//! Handlers take structs, not raw `Value`: shape validation (missing
//! fields, mistyped values, bad enum variants) lives here, domain
//! validation (unknown ids, empty titles, foreign panes) stays in the
//! handler. Unknown fields are ignored (forward compat, docs/08);
//! present-but-mistyped fields are `BAD_PARAMS` instead of silently
//! defaulting, so client bugs surface at the seam.
//!
//! Every optional is `Option<T>`: missing and explicit null both mean
//! "absent", and handlers apply defaults in one line, as before.

use serde::{Deserialize, Deserializer};
use serde_json::Value;

use signaltty_core::model::{Contract, Layout, NotificationSeverity, Relationship, SplitDir};
use signaltty_core::state::{Attention, Lifecycle, TaskState};
use signaltty_proto::code;

pub type ParamError = (String, String);

pub fn bad_params(msg: impl Into<String>) -> ParamError {
    (code::BAD_PARAMS.to_string(), msg.into())
}

/// Decode method params into `T`. Serde errors become `BAD_PARAMS`.
pub fn decode<'de, T: Deserialize<'de>>(params: &'de Value) -> Result<T, ParamError> {
    T::deserialize(params).map_err(|e| bad_params(e.to_string()))
}

// ---- shared shapes ----

/// Methods taking exactly one workspace id.
#[derive(Debug, Deserialize)]
pub struct WorkspaceId {
    pub workspace_id: String,
}

#[derive(Debug, Deserialize)]
pub struct WorkspaceFileDiff {
    pub workspace_id: String,
    pub path: String,
}

#[derive(Debug, Deserialize)]
pub struct WorktreeCreate {
    pub workspace_id: String,
    pub path: String,
    pub branch: String,
    pub name: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct WorktreeOpen {
    pub workspace_id: String,
    pub path: String,
    pub name: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct WorktreeRemove {
    pub workspace_id: String,
    pub path: String,
}

#[derive(Debug, Deserialize)]
pub struct TabClose {
    pub tab_id: String,
    #[serde(default, deserialize_with = "de_opt_signal")]
    pub signal: Option<String>,
}

/// Methods taking exactly one pane id.
#[derive(Debug, Deserialize)]
pub struct PaneId {
    pub pane_id: String,
}

#[derive(Debug, Deserialize)]
pub struct ServerShutdown {
    pub force: Option<bool>,
}

#[derive(Debug, Deserialize)]
pub struct WorkspaceCreate {
    pub name: Option<String>,
    pub cwd: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct WorkspaceRename {
    pub workspace_id: String,
    pub name: String,
}

#[derive(Debug, Deserialize)]
pub struct WorkspaceClose {
    pub workspace_id: String,
    #[serde(default, deserialize_with = "de_opt_signal")]
    pub signal: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct TabCreate {
    pub workspace_id: String,
    pub title: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct TabSetLayout {
    pub tab_id: String,
    pub layout: Layout,
}

#[derive(Debug, Deserialize)]
pub struct TabSetRatio {
    pub tab_id: String,
    pub path: SplitPath,
    pub ratio: Ratio,
}

/// 0 (first) / 1 (second) choices from the tab root.
#[derive(Debug, Deserialize)]
#[serde(transparent)]
pub struct SplitPath(#[serde(deserialize_with = "de_split_path")] pub Vec<bool>);

fn de_split_path<'de, D: Deserializer<'de>>(d: D) -> Result<Vec<bool>, D::Error> {
    let raw = Vec::<Value>::deserialize(d)?;
    raw.iter()
        .map(|v| match v.as_u64() {
            Some(0) => Ok(false),
            Some(1) => Ok(true),
            _ => Err(serde::de::Error::custom("'path' must be an array of 0/1")),
        })
        .collect()
}

/// Divider ratio, clamped to [`Layout::MIN_RATIO`]`..=`[`Layout::MAX_RATIO`].
#[derive(Debug, Deserialize)]
#[serde(transparent)]
pub struct Ratio(#[serde(deserialize_with = "de_ratio")] pub f32);

/// Reject unknown names at decode: `close` must not drop a pane whose
/// child it could not signal.
fn de_signal<'de, D: Deserializer<'de>>(d: D) -> Result<String, D::Error> {
    let raw = String::deserialize(d)?;
    crate::pty::parse_signal(&raw).map_err(serde::de::Error::custom)?;
    Ok(raw)
}

fn de_opt_signal<'de, D: Deserializer<'de>>(d: D) -> Result<Option<String>, D::Error> {
    #[derive(Deserialize)]
    struct Signal(#[serde(deserialize_with = "de_signal")] String);
    Ok(Option::<Signal>::deserialize(d)?.map(|s| s.0))
}

fn de_ratio<'de, D: Deserializer<'de>>(d: D) -> Result<f32, D::Error> {
    let raw = f64::deserialize(d)?;
    if !raw.is_finite() {
        return Err(serde::de::Error::custom("missing or invalid 'ratio'"));
    }
    Ok(Layout::clamp_ratio(raw as f32))
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum SplitDirection {
    Right,
    Down,
}

impl SplitDirection {
    pub fn split_dir(&self) -> SplitDir {
        match self {
            SplitDirection::Right => SplitDir::Right,
            SplitDirection::Down => SplitDir::Down,
        }
    }
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ReadMode {
    Screen,
    Tail,
    Rendered,
}

#[derive(Debug, Deserialize)]
pub struct PaneSpawn {
    pub workspace_id: String,
    pub tab_id: Option<String>,
    pub cwd: Option<String>,
    pub argv: Vec<String>,
    #[serde(default)]
    pub env: std::collections::HashMap<String, String>,
    pub cols: Option<u16>,
    pub rows: Option<u16>,
    pub agent_hint: Option<String>,
    pub parent_pane_id: Option<String>,
    pub label: Option<String>,
    pub relationship: Option<Relationship>,
    pub task_id: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct PaneSplit {
    pub pane_id: String,
    pub direction: Option<SplitDirection>,
    pub cwd: Option<String>,
    pub argv: Option<Vec<String>>,
}

#[derive(Debug, Deserialize)]
pub struct PaneInput {
    pub pane_id: String,
    pub data_b64: String,
}

#[derive(Debug, Deserialize)]
pub struct PaneResize {
    pub pane_id: String,
    pub cols: u16,
    pub rows: u16,
}

#[derive(Debug, Deserialize)]
pub struct PaneSignal {
    pub pane_id: String,
    #[serde(deserialize_with = "de_signal")]
    pub signal: String,
    pub group: Option<bool>,
}

#[derive(Debug, Deserialize)]
pub struct PaneRead {
    pub pane_id: String,
    pub mode: Option<ReadMode>,
    pub strip_ansi: Option<bool>,
    pub lines: Option<u64>,
    pub after_seq: Option<u64>,
}

#[derive(Debug, Deserialize)]
pub struct PaneAttach {
    pub pane_id: String,
    pub cols: Option<u16>,
    pub rows: Option<u16>,
    pub mark_seen: Option<bool>,
}

#[derive(Debug, Deserialize)]
pub struct PaneClose {
    pub pane_id: String,
    #[serde(default, deserialize_with = "de_opt_signal")]
    pub signal: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct Notify {
    pub pane_id: String,
    pub title: String,
    pub body: Option<String>,
    pub severity: Option<String>,
}

/// One structured option inside a `hook-event` decision payload.
/// Unknown fields ignored (forward compat, docs/08).
#[derive(Debug, Deserialize)]
pub struct DecisionOptionPayload {
    pub id: String,
    pub label: String,
}

/// Structured decision request carried by `hook-event` (directive 2).
#[derive(Debug, Deserialize)]
pub struct DecisionPayload {
    pub id: String,
    pub prompt: String,
    pub options: Vec<DecisionOptionPayload>,
}

#[derive(Debug, Deserialize)]
pub struct HookEvent {
    #[serde(default)]
    pub wait_for_answer: bool,
    pub wait_timeout_s: Option<u64>,
    pub agent: String,
    #[serde(rename = "event")]
    pub hook: String,
    pub pane_id: Option<String>,
    pub client_pid: Option<u64>,
    #[serde(default)]
    pub payload: Value,
    pub message: Option<String>,
    pub body: Option<String>,
    pub severity: Option<String>,
    pub title: Option<String>,
    pub decision: Option<DecisionPayload>,
}

#[derive(Debug, Deserialize)]
pub struct DecisionAnswer {
    pub pane_id: String,
    pub decision_id: String,
    pub option_id: String,
}

#[derive(Debug, Deserialize)]
pub struct ReportSession {
    pub pane_id: String,
    pub agent_session_id: String,
    pub agent: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct Subscribe {
    pub events: Option<Vec<String>>,
    pub from_seq: Option<u64>,
    pub task_ids: Option<Vec<String>>,
}

#[derive(Debug, Deserialize)]
pub struct Wait {
    pub pane_id: String,
    pub until: WaitUntil,
    pub timeout_s: Option<u64>,
    pub after: Option<crate::store::WaitBaseline>,
}

#[derive(Debug, Deserialize)]
#[serde(untagged)]
pub enum WaitUntil {
    One(String),
    Any(Vec<String>),
}
impl WaitUntil {
    pub fn into_vec(self) -> Vec<String> {
        match self {
            Self::One(value) => vec![value],
            Self::Any(values) => values,
        }
    }
}

// ---- shared value parsing (used by more than one handler) ----

/// `notify` and `hook-event` share severity parsing; absent means Info.
pub fn parse_severity(severity: &Option<String>) -> Result<NotificationSeverity, ParamError> {
    severity
        .as_deref()
        .map(|s| NotificationSeverity::parse(s).ok_or_else(|| bad_params("bad severity")))
        .transpose()
        .map(|o| o.unwrap_or(NotificationSeverity::Info))
}

/// `wait` accepts a lifecycle, an attention, or a pseudo-state.
pub fn validate_until(until: &str) -> Result<(), ParamError> {
    if Attention::parse(until).is_none()
        && Lifecycle::parse(until).is_none()
        && until != "seen"
        && until != "attention_cleared"
        && until != "exited"
    {
        return Err(bad_params(format!("bad 'until': {until}")));
    }
    Ok(())
}

#[derive(Debug, Deserialize)]
pub struct PaneSubmit {
    pub pane_id: String,
    pub text: String,
    pub submit_delay_ms: Option<u64>,
    pub stall_timeout_s: Option<u64>,
}

#[derive(Debug, Deserialize)]
pub struct TaskStart {
    pub repo: String,
    pub contract: Contract,
    pub agent: Option<String>,
    pub argv: Option<Vec<String>>,
    pub label: Option<String>,
    pub parent_pane_id: Option<String>,
    pub context_id: Option<String>,
    pub relationship: Option<Relationship>,
    pub base_ref: Option<String>,
    pub fetch_first: Option<bool>,
    pub branch: Option<String>,
    pub path: Option<String>,
    pub ready_timeout_s: Option<u64>,
    pub stall_timeout_s: Option<u64>,
}

#[derive(Debug, Deserialize)]
pub struct TaskGet {
    pub task_id: String,
}

#[derive(Debug, Deserialize)]
pub struct TaskList {
    pub context_id: Option<String>,
    pub state: Option<TaskState>,
    pub limit: Option<usize>,
}

#[derive(Debug, Deserialize)]
pub struct TaskCancel {
    pub task_id: String,
}

#[derive(Debug, Deserialize)]
pub struct TaskWait {
    pub task_id: Option<String>,
    pub context_id: Option<String>,
    pub until: Option<Value>,
    pub timeout_s: Option<u64>,
}

#[derive(Debug, Deserialize)]
pub struct TaskReport {
    pub task_id: Option<String>,
    pub pane_id: Option<String>,
    pub status: signaltty_core::model::TaskResultStatus,
    pub summary: String,
    pub artifacts: Option<Vec<signaltty_core::model::Artifact>>,
    pub evidence: Option<Value>,
}

#[derive(Debug, Deserialize)]
pub struct AttentionPending {
    pub limit: Option<usize>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn decode_ignores_unknown_fields() {
        let p: PaneId = decode(&json!({"pane_id": "p1", "future": 1})).unwrap();
        assert_eq!(p.pane_id, "p1");
    }

    #[test]
    fn decode_missing_required_is_bad_params() {
        let err = decode::<PaneId>(&json!({})).unwrap_err();
        assert_eq!(err.0, code::BAD_PARAMS);
    }

    #[test]
    fn decode_mistyped_optional_is_bad_params() {
        // Present-but-mistyped no longer defaults silently.
        assert!(decode::<WorkspaceCreate>(&json!({"cwd": 42})).is_err());
        assert!(decode::<ServerShutdown>(&json!({"force": "yes"})).is_err());
        assert!(decode::<PaneResize>(&json!({"pane_id": "p", "cols": 80, "rows": -1})).is_err());
        // Missing and explicit null still mean absent.
        let p: WorkspaceCreate = decode(&json!({"cwd": Value::Null})).unwrap();
        assert_eq!(p.cwd, None);
    }

    #[test]
    fn split_path_accepts_only_0_and_1() {
        let p: TabSetRatio = decode(&json!({"tab_id": "t", "path": [1, 0], "ratio": 0.5})).unwrap();
        assert_eq!(p.path.0, vec![true, false]);
        assert!(decode::<TabSetRatio>(&json!({"tab_id": "t", "path": [2], "ratio": 0.5})).is_err());
        assert!(
            decode::<TabSetRatio>(&json!({"tab_id": "t", "path": ["x"], "ratio": 0.5})).is_err()
        );
    }

    #[test]
    fn ratio_clamps_to_stops() {
        let p: TabSetRatio = decode(&json!({"tab_id": "t", "path": [], "ratio": 0.99})).unwrap();
        assert_eq!(p.ratio.0, Layout::MAX_RATIO);
        let p: TabSetRatio = decode(&json!({"tab_id": "t", "path": [], "ratio": 0.0})).unwrap();
        assert_eq!(p.ratio.0, Layout::MIN_RATIO);
        assert!(decode::<TabSetRatio>(&json!({"tab_id": "t", "path": [], "ratio": "x"})).is_err());
    }

    #[test]
    fn enums_reject_bad_variants() {
        assert!(decode::<PaneSplit>(&json!({"pane_id": "p", "direction": "up"})).is_err());
        let p: PaneSplit = decode(&json!({"pane_id": "p", "direction": "down"})).unwrap();
        assert!(matches!(p.direction, Some(SplitDirection::Down)));
        assert_eq!(p.direction.unwrap().split_dir(), SplitDir::Down);
        assert!(decode::<PaneRead>(&json!({"pane_id": "p", "mode": "sideways"})).is_err());
    }

    #[test]
    fn decision_payload_decode() {
        // Full decision decodes; unknown fields ignored.
        let h: HookEvent = decode(&json!({
            "agent": "codex", "event": "PermissionRequest",
            "decision": {"id": "d1", "prompt": "Allow?",
                "options": [{"id": "once", "label": "Once", "future": 1}]},
        }))
        .unwrap();
        let d = h.decision.unwrap();
        assert_eq!((d.id.as_str(), d.options.len()), ("d1", 1));
        // Absent decision stays absent.
        let h: HookEvent = decode(&json!({"agent": "codex", "event": "Stop"})).unwrap();
        assert!(h.decision.is_none());
        // Present-but-mistyped is BAD_PARAMS, never silent.
        assert!(
            decode::<HookEvent>(&json!({"agent": "codex", "event": "PermissionRequest",
                "decision": {"id": 42, "prompt": "p", "options": []}}))
            .is_err()
        );
        // Answer ids are required.
        assert!(decode::<DecisionAnswer>(&json!({"pane_id": "p", "decision_id": "d"})).is_err());
        let a: DecisionAnswer =
            decode(&json!({"pane_id": "p", "decision_id": "d", "option_id": "o"})).unwrap();
        assert_eq!(a.option_id, "o");
    }

    #[test]
    fn severity_and_until_parse() {
        assert_eq!(parse_severity(&None).unwrap(), NotificationSeverity::Info);
        assert!(parse_severity(&Some("bogus".to_string())).is_err());
        for u in [
            "blocked",
            "done",
            "unread",
            "seen",
            "attention_cleared",
            "exited",
        ] {
            validate_until(u).unwrap();
        }
        assert!(validate_until("eventually").is_err());
    }
}
